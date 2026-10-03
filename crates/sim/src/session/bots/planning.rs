//! Bounded deterministic forward planning over grounded actions.
//!
//! The caller owns semantics, identity, authority and live execution. Groups
//! model known immediate transitions only, never asynchronous rule scheduling.
//! Revalidate every selected action through its authoritative provider.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum FactValue {
    Bool(bool),
    Number(i64),
}

pub(super) type Facts = BTreeMap<String, FactValue>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Compare {
    Equal,
    NotEqual,
    Less,
    AtMost,
    Greater,
    AtLeast,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Predicate {
    pub key: String,
    pub compare: Compare,
    pub value: FactValue,
}

impl Predicate {
    pub(super) fn matches(&self, facts: &Facts) -> bool {
        let Some(actual) = facts.get(&self.key) else {
            return false;
        };
        use Compare::*;
        match (actual, &self.value) {
            (FactValue::Number(a), FactValue::Number(b)) => match self.compare {
                Equal => a == b,
                NotEqual => a != b,
                Less => a < b,
                AtMost => a <= b,
                Greater => a > b,
                AtLeast => a >= b,
            },
            (FactValue::Bool(a), FactValue::Bool(b)) => match self.compare {
                Equal => a == b,
                NotEqual => a != b,
                _ => false,
            },
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Effect {
    Set { key: String, value: FactValue },
    Add { key: String, amount: i64 },
}

/// Guards see effects of earlier groups in authored order. Unknown facts
/// fail a guard, including NotEqual. A provider must guarantee successful
/// effects when guarded; arithmetic failure rejects the entire projection.
/// This is deliberately not an emulator of independently failing event rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EffectGroup {
    pub guards: Vec<Predicate>,
    pub effects: Vec<Effect>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Action {
    /// Caller maps this identity to an authoritative, ordinary-control executor.
    pub id: String,
    pub cost: u32,
    pub preconditions: Vec<Predicate>,
    pub effect_groups: Vec<EffectGroup>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Goal(pub Vec<Predicate>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Limits {
    /// Total accepted search entries, including initial. Also bounds retained
    /// labels and frontier allocations; repeated improvements consume budget.
    pub max_nodes: usize,
    pub max_depth: usize,
    pub max_actions: usize,
    /// Action evaluations, including inapplicable actions. Bounds fanout work.
    pub max_candidates: usize,
    pub max_facts: usize,
    /// Initial facts, goal predicates, actions, preconditions, groups, guards
    /// and effects together. Bounds individual model collections before clone.
    pub max_model_terms: usize,
    /// Aggregate UTF-8 bytes of initial keys, goal keys, action IDs and all
    /// predicate/effect keys. Bounds strings copied into search state/path.
    pub max_model_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Failure {
    Unsupported,
    NoPlan,
    NodeBudgetExceeded,
    ActionBudgetExceeded,
    CandidateBudgetExceeded,
    FactBudgetExceeded,
    ModelBudgetExceeded,
    DepthLimitExceeded,
    InvalidActionModel,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct QueueEntry {
    priority: (u64, Reverse<usize>),
    cost: u64,
    depth: usize,
    path: Vec<String>,
    facts: Facts,
}

/// Mandatory equalities shared by every possible achiever form landmarks.
/// Earlier authored effects may supply a guard in the same action, so those
/// guards are never inferred as external prerequisites. Add effects make an
/// equality's prerequisites ambiguous and disable that inference entirely.
/// Action costs are partitioned across all landmarks they can establish: the
/// sum of landmark weights covered by any action cannot exceed its cost.
/// Consequently their unsatisfied sum is a lower bound, even with shared
/// achievers, deletes, conditional groups and repeated actions. This guides
/// ordinary search without changing its optimal-cost or depth contract.
#[derive(Default)]
struct Guidance {
    predicates: Vec<Predicate>,
    parents: Vec<Vec<usize>>,
    roots: Vec<usize>,
    weights: Vec<u64>,
    step_weights: Vec<u64>,
}

impl Guidance {
    const NODES: usize = 128;

    fn build(actions: &[&Action], goal: &Goal, mut work: usize) -> Option<Self> {
        let mut out = Self::default();
        for predicate in &goal.0 {
            let index = out.insert(predicate)?;
            if !out.roots.contains(&index) {
                out.roots.push(index);
            }
        }
        let mut at = 0;
        while at < out.predicates.len() {
            let requirements = Self::requirements(&out.predicates[at], actions, &mut work)?;
            for predicate in requirements {
                let parent = out.insert(&predicate)?;
                if !out.parents[at].contains(&parent) {
                    out.parents[at].push(parent);
                }
            }
            at += 1;
        }
        out.weights = vec![u64::MAX; out.predicates.len()];
        out.step_weights = vec![u64::MAX; out.predicates.len()];
        for action in actions {
            let mut covers = Vec::new();
            for (index, predicate) in out.predicates.iter().enumerate() {
                let mut covered = false;
                for group in &action.effect_groups {
                    for effect in &group.effects {
                        work = work.checked_sub(1)?;
                        covered |= match effect {
                            Effect::Set { key, value } => {
                                key == &predicate.key
                                    && (predicate.compare != Compare::Equal
                                        || value == &predicate.value)
                            }
                            Effect::Add { key, .. } => key == &predicate.key,
                        };
                    }
                }
                if covered {
                    covers.push(index);
                }
            }
            if !covers.is_empty() {
                let weight = u64::from(action.cost) / covers.len() as u64;
                let step_weight = 1 / covers.len() as u64;
                for index in covers {
                    out.weights[index] = out.weights[index].min(weight);
                    out.step_weights[index] = out.step_weights[index].min(step_weight);
                }
            }
        }
        for weight in out.weights.iter_mut().chain(&mut out.step_weights) {
            if *weight == u64::MAX {
                *weight = 0; // Lack of a relaxed achiever is not a new NoPlan proof.
            }
        }
        Some(out)
    }

    fn insert(&mut self, predicate: &Predicate) -> Option<usize> {
        if let Some(index) = self.predicates.iter().position(|p| p == predicate) {
            return Some(index);
        }
        if self.predicates.len() == Self::NODES {
            return None;
        }
        self.predicates.push(predicate.clone());
        self.parents.push(Vec::new());
        Some(self.predicates.len() - 1)
    }

    fn requirements(
        predicate: &Predicate,
        actions: &[&Action],
        work: &mut usize,
    ) -> Option<Vec<Predicate>> {
        if predicate.compare != Compare::Equal {
            return Some(Vec::new());
        }
        let mut common: Option<Vec<Predicate>> = None;
        for action in actions {
            let mut written = Vec::<&str>::new();
            for group in &action.effect_groups {
                for effect in &group.effects {
                    *work = work.checked_sub(1)?;
                    match effect {
                        Effect::Add { key, .. } if key == &predicate.key => {
                            return Some(Vec::new());
                        }
                        Effect::Set { key, value }
                            if key == &predicate.key && value == &predicate.value =>
                        {
                            let mut required = Vec::new();
                            for p in action.preconditions.iter().chain(
                                group
                                    .guards
                                    .iter()
                                    .filter(|p| !written.contains(&p.key.as_str())),
                            ) {
                                *work = work.checked_sub(1)?;
                                if p.compare == Compare::Equal && !required.contains(p) {
                                    if required.len() == Self::NODES {
                                        return None;
                                    }
                                    required.push(p.clone());
                                }
                            }
                            if let Some(common) = &mut common {
                                common.retain(|p| required.contains(p));
                            } else {
                                common = Some(required);
                            }
                        }
                        _ => {}
                    }
                }
                for effect in &group.effects {
                    let (Effect::Set { key, .. } | Effect::Add { key, .. }) = effect;
                    if !written.contains(&key.as_str()) {
                        if written.len() == Self::NODES {
                            return None;
                        }
                        written.push(key);
                    }
                }
            }
        }
        Some(common.unwrap_or_default())
    }

    fn estimate(&self, facts: &Facts) -> u64 {
        self.estimate_with(facts, &self.weights)
    }

    fn min_steps(&self, facts: &Facts) -> u64 {
        self.estimate_with(facts, &self.step_weights)
    }

    fn estimate_with(&self, facts: &Facts, weights: &[u64]) -> u64 {
        let mut visited = [false; Self::NODES];
        let mut stack = [0; Self::NODES];
        let mut length = 0;
        for root in &self.roots {
            visited[*root] = true;
            stack[length] = *root;
            length += 1;
        }
        let mut estimate = 0_u64;
        while length > 0 {
            length -= 1;
            let index = stack[length];
            if self.predicates[index].matches(facts) {
                continue; // Already established: its old prerequisites need not be restored.
            }
            estimate = estimate.saturating_add(weights[index]);
            for parent in &self.parents[index] {
                if !visited[*parent] {
                    visited[*parent] = true;
                    stack[length] = *parent;
                    length += 1;
                }
            }
        }
        estimate
    }
}

/// Least-cost plan within all supplied bounds. An admissible, bounded landmark
/// estimate guides the queue; equal estimates prefer deeper progress, then cost,
/// path IDs and ordered facts. Depth belongs to visited identity: a cheap deep route
/// must not evict a costlier shallow route with more remaining actions.
/// Budget exhaustion returns no partial plan, even if a goal was queued.
/// Callers use small deterministic budgets and schedule/retry between ticks.
pub(super) fn plan(
    initial: &Facts,
    actions: &[Action],
    goal: &Goal,
    limits: Limits,
) -> Result<Vec<String>, Failure> {
    validate(initial, actions, goal, limits)?;
    let mut ordered: Vec<&Action> = actions.iter().collect();
    ordered.sort_by(|a, b| a.id.cmp(&b.id));
    if ordered.windows(2).any(|p| p[0].id == p[1].id) {
        return Err(Failure::InvalidActionModel);
    }
    if reached(initial, goal) {
        return Ok(Vec::new());
    }
    if limits.max_nodes == 0 {
        return Err(Failure::NodeBudgetExceeded);
    }
    let guidance = Guidance::build(
        &ordered,
        goal,
        limits.max_candidates.min(limits.max_model_terms),
    )
    .unwrap_or_default();
    search(initial, &ordered, goal, limits, &guidance)
}

fn search(
    initial: &Facts,
    ordered: &[&Action],
    goal: &Goal,
    limits: Limits,
    guidance: &Guidance,
) -> Result<Vec<String>, Failure> {
    // The same partition, using unit action cost, only rejects when it proves
    // the goal cannot fit the existing depth envelope. Shared achievers may
    // yield zero weights; this is deliberately a weak conservative proof.
    if guidance.min_steps(initial) > limits.max_depth as u64 {
        return Err(Failure::DepthLimitExceeded);
    }
    let first = QueueEntry {
        priority: (guidance.estimate(initial), Reverse(0)),
        cost: 0,
        depth: 0,
        path: Vec::new(),
        facts: initial.clone(),
    };
    let mut queue = BinaryHeap::from([Reverse(first.clone())]);
    let mut best = BTreeMap::from([((first.facts.clone(), 0), 0_u64)]);
    let mut accepted = 1_usize;
    let mut candidates = 0_usize;
    let mut depth_limited = false;
    while let Some(Reverse(current)) = queue.pop() {
        if best.get(&(current.facts.clone(), current.depth)) != Some(&current.cost) {
            continue;
        }
        if reached(&current.facts, goal) {
            return Ok(current.path);
        }
        for action in ordered {
            if candidates >= limits.max_candidates {
                return Err(Failure::CandidateBudgetExceeded);
            }
            candidates += 1;
            if !action
                .preconditions
                .iter()
                .all(|p| p.matches(&current.facts))
            {
                continue;
            }
            let Some(next_facts) = apply(&current.facts, &action.effect_groups, limits.max_facts)?
            else {
                continue;
            };
            if next_facts == current.facts {
                continue;
            }
            if current.depth >= limits.max_depth {
                depth_limited = true;
                continue;
            }
            let Some(next_cost) = current.cost.checked_add(u64::from(action.cost)) else {
                continue;
            };
            let depth = current.depth + 1;
            // Same-depth dominance is sufficient for correctness. Avoid a
            // cross-depth cost-only shortcut: remaining depth is a resource.
            if best
                .get(&(next_facts.clone(), depth))
                .is_some_and(|old| *old <= next_cost)
            {
                continue;
            }
            if accepted >= limits.max_nodes {
                return Err(Failure::NodeBudgetExceeded);
            }
            accepted += 1;
            let mut path = current.path.clone();
            path.push(action.id.clone());
            best.insert((next_facts.clone(), depth), next_cost);
            queue.push(Reverse(QueueEntry {
                priority: (
                    next_cost.saturating_add(guidance.estimate(&next_facts)),
                    Reverse(depth),
                ),
                cost: next_cost,
                depth,
                path,
                facts: next_facts,
            }));
        }
    }
    Err(if depth_limited {
        Failure::DepthLimitExceeded
    } else {
        Failure::NoPlan
    })
}

fn validate(
    initial: &Facts,
    actions: &[Action],
    goal: &Goal,
    limits: Limits,
) -> Result<(), Failure> {
    if actions.len() > limits.max_actions {
        return Err(Failure::ActionBudgetExceeded);
    }
    if initial.len() > limits.max_facts {
        return Err(Failure::FactBudgetExceeded);
    }
    let mut terms = 0_usize;
    let mut bytes = 0_usize;
    let mut charge = |count: usize, text: usize| {
        terms = terms.saturating_add(count);
        bytes = bytes.saturating_add(text);
        if terms > limits.max_model_terms || bytes > limits.max_model_bytes {
            Err(Failure::ModelBudgetExceeded)
        } else {
            Ok(())
        }
    };
    charge(initial.len(), 0)?;
    for key in initial.keys() {
        if key.is_empty() {
            return Err(Failure::InvalidActionModel);
        }
        charge(0, key.len())?;
    }
    charge(goal.0.len(), 0)?;
    for p in &goal.0 {
        if p.key.is_empty() {
            return Err(Failure::InvalidActionModel);
        }
        charge(0, p.key.len())?;
    }
    charge(actions.len(), 0)?;
    for action in actions {
        if action.id.is_empty() || action.cost == 0 {
            return Err(Failure::InvalidActionModel);
        }
        charge(
            action
                .preconditions
                .len()
                .saturating_add(action.effect_groups.len()),
            action.id.len(),
        )?;
        for p in &action.preconditions {
            if p.key.is_empty() {
                return Err(Failure::InvalidActionModel);
            }
            charge(0, p.key.len())?;
        }
        for group in &action.effect_groups {
            charge(group.guards.len().saturating_add(group.effects.len()), 0)?;
            for p in &group.guards {
                if p.key.is_empty() {
                    return Err(Failure::InvalidActionModel);
                }
                charge(0, p.key.len())?;
            }
            for effect in &group.effects {
                let key = match effect {
                    Effect::Set { key, .. } | Effect::Add { key, .. } => key,
                };
                if key.is_empty() {
                    return Err(Failure::InvalidActionModel);
                }
                charge(0, key.len())?;
            }
        }
    }
    Ok(())
}

fn reached(facts: &Facts, goal: &Goal) -> bool {
    goal.0.iter().all(|p| p.matches(facts))
}

fn apply(
    facts: &Facts,
    groups: &[EffectGroup],
    max_facts: usize,
) -> Result<Option<Facts>, Failure> {
    let mut next = facts.clone();
    for group in groups {
        if !group.guards.iter().all(|p| p.matches(&next)) {
            continue;
        }
        for effect in &group.effects {
            match effect {
                Effect::Set { key, value } => {
                    if !next.contains_key(key) && next.len() >= max_facts {
                        return Err(Failure::FactBudgetExceeded);
                    }
                    next.insert(key.clone(), value.clone());
                }
                Effect::Add { key, amount } => {
                    let Some(FactValue::Number(before)) = next.get(key) else {
                        return Ok(None);
                    };
                    let Some(after) = before.checked_add(*amount) else {
                        return Ok(None);
                    };
                    next.insert(key.clone(), FactValue::Number(after));
                }
            }
        }
    }
    Ok(Some(next))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn number(key: &str, compare: Compare, value: i64) -> Predicate {
        Predicate {
            key: key.into(),
            compare,
            value: FactValue::Number(value),
        }
    }

    fn set_flag(id: &str, flag: &str) -> Action {
        Action {
            id: id.into(),
            cost: 1,
            preconditions: vec![number(flag, Compare::Equal, 0)],
            effect_groups: vec![EffectGroup {
                guards: vec![],
                effects: vec![Effect::Set {
                    key: flag.into(),
                    value: FactValue::Number(1),
                }],
            }],
        }
    }

    fn limits() -> Limits {
        Limits {
            max_nodes: 256,
            max_depth: 16,
            max_actions: 128,
            max_candidates: 32768,
            max_facts: 64,
            max_model_terms: 4096,
            max_model_bytes: 65536,
        }
    }

    #[test]
    fn four_distinct_bricks_require_four_distinct_scoped_flags() {
        let mut initial = Facts::new();
        for flag in ["brick-a", "brick-b", "brick-c", "brick-d"] {
            initial.insert(flag.into(), FactValue::Number(0));
        }
        let actions = [
            set_flag("activate-a", "brick-a"),
            set_flag("activate-b", "brick-b"),
            set_flag("activate-c", "brick-c"),
            set_flag("activate-d", "brick-d"),
        ];
        let goal = Goal(
            ["brick-a", "brick-b", "brick-c", "brick-d"]
                .into_iter()
                .map(|flag| number(flag, Compare::Equal, 1))
                .collect(),
        );
        let expected = vec![
            "activate-a".to_string(),
            "activate-b".to_string(),
            "activate-c".to_string(),
            "activate-d".to_string(),
        ];
        assert_eq!(
            plan(&initial, &actions, &goal, limits()),
            Ok(expected.clone())
        );

        let mut reversed = actions.to_vec();
        reversed.reverse();
        assert_eq!(plan(&initial, &reversed, &goal, limits()), Ok(expected));
    }

    #[test]
    fn eight_independent_guards_compose_under_the_existing_search_caps() {
        let initial = (0..8)
            .map(|n| (format!("unfamiliar-{n}"), FactValue::Number(0)))
            .chain([("victory".into(), FactValue::Bool(false))])
            .collect();
        let mut actions = (0..8)
            .map(|n| set_flag(&format!("switch-{n}"), &format!("unfamiliar-{n}")))
            .collect::<Vec<_>>();
        actions[7].effect_groups.push(EffectGroup {
            guards: (0..8)
                .map(|n| number(&format!("unfamiliar-{n}"), Compare::Equal, 1))
                .collect(),
            effects: vec![Effect::Set {
                key: "victory".into(),
                value: FactValue::Bool(true),
            }],
        });
        let goal = Goal(vec![Predicate {
            key: "victory".into(),
            compare: Compare::Equal,
            value: FactValue::Bool(true),
        }]);
        let bound = Limits {
            max_candidates: 4096,
            max_depth: 12,
            ..limits()
        };
        let expected = (0..8).map(|n| format!("switch-{n}")).collect::<Vec<_>>();
        let ordered = actions.iter().collect::<Vec<_>>();
        assert!(matches!(
            search(&initial, &ordered, &goal, bound, &Guidance::default()),
            Err(Failure::NodeBudgetExceeded | Failure::CandidateBudgetExceeded)
        ));
        assert_eq!(plan(&initial, &actions, &goal, bound), Ok(expected.clone()));
        actions.reverse();
        assert_eq!(plan(&initial, &actions, &goal, bound), Ok(expected));
    }

    #[test]
    #[ignore = "optimized planner diagnostic; no timing thresholds"]
    fn profile_guidance_against_the_same_unguided_search() {
        for count in [4, 8, 16] {
            let initial: Facts = (0..count)
                .map(|n| (format!("flag-{n}"), FactValue::Number(0)))
                .chain([("victory".into(), FactValue::Bool(false))])
                .collect();
            let mut actions = (0..count)
                .map(|n| set_flag(&format!("switch-{n:02}"), &format!("flag-{n}")))
                .collect::<Vec<_>>();
            actions[count - 1].effect_groups.push(EffectGroup {
                guards: (0..count)
                    .map(|n| number(&format!("flag-{n}"), Compare::Equal, 1))
                    .collect(),
                effects: vec![Effect::Set {
                    key: "victory".into(),
                    value: FactValue::Bool(true),
                }],
            });
            let goal = Goal(vec![Predicate {
                key: "victory".into(),
                compare: Compare::Equal,
                value: FactValue::Bool(true),
            }]);
            let bound = Limits {
                max_candidates: 4096,
                max_depth: 12,
                ..limits()
            };
            let ordered = actions.iter().collect::<Vec<_>>();
            for guided in [false, true] {
                let mut samples = Vec::new();
                let mut outcome = None;
                for _ in 0..200 {
                    let start = std::time::Instant::now();
                    let result = if guided {
                        plan(&initial, &actions, &goal, bound)
                    } else {
                        search(&initial, &ordered, &goal, bound, &Guidance::default())
                    };
                    samples.push(start.elapsed().as_nanos());
                    if let Some(old) = &outcome {
                        assert_eq!(old, &result);
                    }
                    outcome = Some(result);
                }
                samples.sort_unstable();
                let p =
                    |f: f64| samples[((samples.len() - 1) as f64 * f).ceil() as usize] as f64 / 1e6;
                eprintln!(
                    "planner flags={count} guided={guided} samples={} ms[p50={:.4},p95={:.4},p99={:.4},max={:.4}] outcome={outcome:?}",
                    samples.len(),
                    p(0.5),
                    p(0.95),
                    p(0.99),
                    p(1.0)
                );
            }
        }
    }

    #[test]
    fn guidance_does_not_count_old_prerequisites_or_shared_effects_twice() {
        let actions = [
            Action {
                id: "shared".into(),
                cost: 5,
                preconditions: vec![number("stage", Compare::Equal, 0)],
                effect_groups: vec![EffectGroup {
                    guards: vec![],
                    effects: vec![
                        Effect::Set {
                            key: "stage".into(),
                            value: FactValue::Number(1),
                        },
                        Effect::Set {
                            key: "other".into(),
                            value: FactValue::Number(1),
                        },
                    ],
                }],
            },
            Action {
                id: "finish".into(),
                cost: 1,
                preconditions: vec![
                    number("stage", Compare::Equal, 1),
                    number("other", Compare::Equal, 1),
                ],
                effect_groups: vec![EffectGroup {
                    guards: vec![],
                    effects: vec![Effect::Set {
                        key: "done".into(),
                        value: FactValue::Bool(true),
                    }],
                }],
            },
        ];
        let goal = Goal(vec![Predicate {
            key: "done".into(),
            compare: Compare::Equal,
            value: FactValue::Bool(true),
        }]);
        let references = actions.iter().collect::<Vec<_>>();
        let guidance = Guidance::build(&references, &goal, 4096).unwrap();
        let initial = Facts::from([
            ("stage".into(), FactValue::Number(0)),
            ("other".into(), FactValue::Number(0)),
        ]);
        assert!(guidance.estimate(&initial) <= 6);
        assert!(guidance.min_steps(&initial) <= 2);
        let progressed = apply(&initial, &actions[0].effect_groups, 64)
            .unwrap()
            .unwrap();
        assert_eq!(
            guidance.estimate(&progressed),
            1,
            "the stage0 prerequisite was consumed, not a goal to restore"
        );
        assert_eq!(guidance.min_steps(&progressed), 1);
        assert_eq!(
            plan(&initial, &actions, &goal, limits()),
            Ok(vec!["shared".into(), "finish".into()])
        );
    }

    #[test]
    fn proved_depth_excess_does_not_reject_a_shared_achiever() {
        let initial: Facts = (0..16)
            .map(|n| (format!("item-{n}"), FactValue::Number(0)))
            .collect();
        let mut actions = (0..16)
            .map(|n| set_flag(&format!("touch-{n:02}"), &format!("item-{n}")))
            .collect::<Vec<_>>();
        let goal = Goal(
            (0..16)
                .map(|n| number(&format!("item-{n}"), Compare::Equal, 1))
                .collect(),
        );
        let bound = Limits {
            max_depth: 12,
            max_candidates: 4096,
            ..limits()
        };
        assert_eq!(
            plan(&initial, &actions, &goal, bound),
            Err(Failure::DepthLimitExceeded)
        );
        actions.push(Action {
            id: "one_shared_mechanism".into(),
            cost: 1,
            preconditions: vec![],
            effect_groups: vec![EffectGroup {
                guards: vec![],
                effects: (0..16)
                    .map(|n| Effect::Set {
                        key: format!("item-{n}"),
                        value: FactValue::Number(1),
                    })
                    .collect(),
            }],
        });
        assert_eq!(
            plan(&initial, &actions, &goal, bound),
            Ok(vec!["one_shared_mechanism".into()])
        );
    }

    #[test]
    fn guided_costs_match_zero_guidance_on_varied_small_models() {
        // Exhaustive state search is a test oracle, not a runtime fallback
        // with inflated budgets. Vary costs, shared setters, deletes and Add.
        for seed in 0..80 {
            let initial = Facts::from([
                ("x".into(), FactValue::Number(0)),
                ("y".into(), FactValue::Number(0)),
            ]);
            let mut actions = vec![
                transition("advance", 0, 1, 1 + seed % 7),
                transition("direct", 0, 2, 1 + seed % 11),
                transition("finish", 1, 2, 1 + seed % 5),
            ];
            actions[0].effect_groups[0].effects.push(Effect::Set {
                key: "y".into(),
                value: FactValue::Number(1),
            });
            actions.push(Action {
                id: "increment".into(),
                cost: 1 + seed % 3,
                preconditions: vec![number("y", Compare::Less, 2)],
                effect_groups: vec![EffectGroup {
                    guards: vec![number("x", Compare::Greater, 0)],
                    effects: vec![Effect::Add {
                        key: "y".into(),
                        amount: 1,
                    }],
                }],
            });
            actions.push(Action {
                id: "delete".into(),
                cost: 1,
                preconditions: vec![],
                effect_groups: vec![EffectGroup {
                    guards: vec![],
                    effects: vec![Effect::Set {
                        key: "y".into(),
                        value: FactValue::Number(0),
                    }],
                }],
            });
            let goal = Goal(vec![
                number("x", Compare::Equal, 2),
                number("y", Compare::AtLeast, 1),
            ]);
            let bound = Limits {
                max_depth: 6,
                ..limits()
            };
            let references = actions.iter().collect::<Vec<_>>();
            let plain = search(&initial, &references, &goal, bound, &Guidance::default()).unwrap();
            let guided = plan(&initial, &actions, &goal, bound).unwrap();
            let cost = |path: &[String]| {
                path.iter()
                    .map(|id| u64::from(actions.iter().find(|a| a.id == *id).unwrap().cost))
                    .sum::<u64>()
            };
            assert_eq!(cost(&guided), cost(&plain), "seed {seed}");
            for action in &mut actions {
                action.cost = 1;
            }
            let references = actions.iter().collect::<Vec<_>>();
            let minimum =
                search(&initial, &references, &goal, bound, &Guidance::default()).unwrap();
            let guidance = Guidance::build(&references, &goal, 4096).unwrap();
            assert!(
                guidance.min_steps(&initial) <= minimum.len() as u64,
                "minimum steps seed {seed}"
            );
        }
    }

    #[test]
    fn ordered_checkpoint_plan_uses_preconditions_and_partial_progress() {
        let initial = Facts::from([
            ("checkpoint".into(), FactValue::Number(0)),
            ("score".into(), FactValue::Number(0)),
        ]);
        let actions = [
            Action {
                id: "enter-checkpoint-2".into(),
                cost: 1,
                preconditions: vec![number("checkpoint", Compare::Equal, 1)],
                effect_groups: vec![EffectGroup {
                    guards: vec![],
                    effects: vec![Effect::Set {
                        key: "checkpoint".into(),
                        value: FactValue::Number(2),
                    }],
                }],
            },
            Action {
                id: "enter-checkpoint-1".into(),
                cost: 1,
                preconditions: vec![number("checkpoint", Compare::Equal, 0)],
                effect_groups: vec![EffectGroup {
                    guards: vec![],
                    effects: vec![Effect::Set {
                        key: "checkpoint".into(),
                        value: FactValue::Number(1),
                    }],
                }],
            },
            Action {
                id: "finish".into(),
                cost: 2,
                preconditions: vec![number("checkpoint", Compare::Equal, 2)],
                effect_groups: vec![EffectGroup {
                    guards: vec![],
                    effects: vec![Effect::Add {
                        key: "score".into(),
                        amount: 1,
                    }],
                }],
            },
        ];
        let goal = Goal(vec![number("score", Compare::AtLeast, 1)]);
        assert_eq!(
            plan(&initial, &actions, &goal, limits()),
            Ok(vec![
                "enter-checkpoint-1".into(),
                "enter-checkpoint-2".into(),
                "finish".into()
            ])
        );
        let partial = Facts::from([
            ("checkpoint".into(), FactValue::Number(1)),
            ("score".into(), FactValue::Number(0)),
        ]);
        assert_eq!(
            plan(&partial, &actions, &goal, limits()),
            Ok(vec!["enter-checkpoint-2".into(), "finish".into()])
        );
    }

    #[test]
    fn cycles_and_noops_do_not_prevent_a_goal_or_loop_forever() {
        let initial = Facts::from([("stage".into(), FactValue::Number(0))]);
        let actions = [
            Action {
                id: "advance".into(),
                cost: 1,
                preconditions: vec![number("stage", Compare::Less, 2)],
                effect_groups: vec![EffectGroup {
                    guards: vec![],
                    effects: vec![Effect::Add {
                        key: "stage".into(),
                        amount: 1,
                    }],
                }],
            },
            Action {
                id: "cycle-back".into(),
                cost: 1,
                preconditions: vec![number("stage", Compare::Greater, 0)],
                effect_groups: vec![EffectGroup {
                    guards: vec![],
                    effects: vec![Effect::Add {
                        key: "stage".into(),
                        amount: -1,
                    }],
                }],
            },
            Action {
                id: "noop".into(),
                cost: 1,
                preconditions: vec![],
                effect_groups: vec![EffectGroup {
                    guards: vec![],
                    effects: vec![Effect::Set {
                        key: "stage".into(),
                        value: FactValue::Number(0),
                    }],
                }],
            },
        ];
        assert_eq!(
            plan(
                &initial,
                &actions,
                &Goal(vec![number("stage", Compare::Equal, 2)]),
                limits()
            ),
            Ok(vec!["advance".into(), "advance".into()])
        );
    }

    #[test]
    fn negative_additions_and_checked_overflow_are_handled() {
        let initial = Facts::from([("counter".into(), FactValue::Number(2))]);
        let decrement = Action {
            id: "decrement".into(),
            cost: 1,
            preconditions: vec![number("counter", Compare::Greater, 0)],
            effect_groups: vec![EffectGroup {
                guards: vec![],
                effects: vec![Effect::Add {
                    key: "counter".into(),
                    amount: -1,
                }],
            }],
        };
        assert_eq!(
            plan(
                &initial,
                &[decrement],
                &Goal(vec![number("counter", Compare::Equal, 0)]),
                limits()
            ),
            Ok(vec!["decrement".into(), "decrement".into()])
        );
        let overflow = Action {
            id: "overflow".into(),
            cost: 1,
            preconditions: vec![],
            effect_groups: vec![EffectGroup {
                guards: vec![],
                effects: vec![Effect::Add {
                    key: "counter".into(),
                    amount: 1,
                }],
            }],
        };
        let top = Facts::from([("counter".into(), FactValue::Number(i64::MAX))]);
        assert_eq!(
            plan(
                &top,
                &[overflow],
                &Goal(vec![number("counter", Compare::Less, 0)]),
                limits()
            ),
            Err(Failure::NoPlan)
        );
    }

    #[test]
    fn unreachable_goals_and_each_search_bound_have_diagnostics() {
        let initial = Facts::from([("x".into(), FactValue::Number(0))]);
        let impossible = [Action {
            id: "wrong-precondition".into(),
            cost: 1,
            preconditions: vec![number("x", Compare::Equal, 1)],
            effect_groups: vec![EffectGroup {
                guards: vec![],
                effects: vec![Effect::Set {
                    key: "x".into(),
                    value: FactValue::Number(2),
                }],
            }],
        }];
        assert_eq!(
            plan(
                &initial,
                &impossible,
                &Goal(vec![number("x", Compare::Equal, 2)]),
                limits()
            ),
            Err(Failure::NoPlan)
        );

        let chain = [
            Action {
                id: "step-1".into(),
                cost: 1,
                preconditions: vec![number("x", Compare::Equal, 0)],
                effect_groups: vec![EffectGroup {
                    guards: vec![],
                    effects: vec![Effect::Set {
                        key: "x".into(),
                        value: FactValue::Number(1),
                    }],
                }],
            },
            Action {
                id: "step-2".into(),
                cost: 1,
                preconditions: vec![number("x", Compare::Equal, 1)],
                effect_groups: vec![EffectGroup {
                    guards: vec![],
                    effects: vec![Effect::Set {
                        key: "x".into(),
                        value: FactValue::Number(2),
                    }],
                }],
            },
        ];
        let goal = Goal(vec![number("x", Compare::Equal, 2)]);
        assert_eq!(
            plan(
                &initial,
                &chain,
                &goal,
                Limits {
                    max_nodes: 8,
                    max_depth: 1,
                    max_actions: 8,
                    ..limits()
                }
            ),
            Err(Failure::DepthLimitExceeded)
        );
        assert_eq!(
            plan(
                &initial,
                &chain,
                &goal,
                Limits {
                    max_nodes: 1,
                    max_depth: 8,
                    max_actions: 8,
                    ..limits()
                }
            ),
            Err(Failure::NodeBudgetExceeded)
        );
    }

    #[test]
    fn selects_least_cost_route_not_first_action_name() {
        let initial = Facts::from([("done".into(), FactValue::Number(0))]);
        let actions = [
            Action {
                id: "a-expensive".into(),
                cost: 50,
                preconditions: vec![],
                effect_groups: vec![EffectGroup {
                    guards: vec![],
                    effects: vec![Effect::Set {
                        key: "done".into(),
                        value: FactValue::Number(1),
                    }],
                }],
            },
            Action {
                id: "z-cheap-first".into(),
                cost: 2,
                preconditions: vec![],
                effect_groups: vec![EffectGroup {
                    guards: vec![],
                    effects: vec![Effect::Set {
                        key: "done".into(),
                        value: FactValue::Number(1),
                    }],
                }],
            },
        ];
        assert_eq!(
            plan(
                &initial,
                &actions,
                &Goal(vec![number("done", Compare::Equal, 1)]),
                limits()
            ),
            Ok(vec!["z-cheap-first".into()])
        );
    }

    #[test]
    fn bool_facts_and_all_comparisons_are_typed() {
        let facts = Facts::from([
            ("ready".into(), FactValue::Bool(true)),
            ("count".into(), FactValue::Number(3)),
        ]);
        let goal = Goal(vec![
            Predicate {
                key: "ready".into(),
                compare: Compare::NotEqual,
                value: FactValue::Bool(false),
            },
            number("count", Compare::AtMost, 3),
            number("count", Compare::AtLeast, 3),
        ]);
        assert!(reached(&facts, &goal));
        assert!(
            !Predicate {
                key: "ready".into(),
                compare: Compare::Less,
                value: FactValue::Bool(false),
            }
            .matches(&facts)
        );
    }

    #[test]
    fn invalid_action_models_are_rejected() {
        let initial = Facts::new();
        let action = Action {
            id: "same".into(),
            cost: 1,
            preconditions: vec![],
            effect_groups: vec![EffectGroup {
                guards: vec![],
                effects: vec![],
            }],
        };
        assert_eq!(
            plan(&initial, &[action.clone(), action], &Goal(vec![]), limits()),
            Err(Failure::InvalidActionModel)
        );
        assert_eq!(
            plan(
                &initial,
                &[Action {
                    id: "free".into(),
                    cost: 0,
                    preconditions: vec![],
                    effect_groups: vec![EffectGroup {
                        guards: vec![],
                        effects: vec![]
                    }],
                }],
                &Goal(vec![]),
                limits()
            ),
            Err(Failure::InvalidActionModel)
        );
    }

    fn transition(id: &str, from: i64, to: i64, cost: u32) -> Action {
        Action {
            id: id.into(),
            cost,
            preconditions: vec![number("x", Compare::Equal, from)],
            effect_groups: vec![EffectGroup {
                guards: vec![],
                effects: vec![Effect::Set {
                    key: "x".into(),
                    value: FactValue::Number(to),
                }],
            }],
        }
    }

    #[test]
    fn expensive_shallow_route_survives_a_cheaper_deep_route() {
        let initial = Facts::from([("x".into(), FactValue::Number(0))]);
        let actions = [
            transition("a-cheap-detour", 0, 1, 1),
            transition("b-detour-end", 1, 2, 1),
            transition("c-direct", 0, 2, 5),
            transition("d-finish", 2, 3, 1),
        ];
        assert_eq!(
            plan(
                &initial,
                &actions,
                &Goal(vec![number("x", Compare::Equal, 3)]),
                Limits {
                    max_depth: 2,
                    ..limits()
                }
            ),
            Ok(vec!["c-direct".into(), "d-finish".into()])
        );
    }

    #[test]
    fn later_guards_see_earlier_effects_and_authored_order_matters() {
        let initial = Facts::from([("x".into(), FactValue::Number(0))]);
        let mut action = transition("activate", 0, 1, 1);
        action.effect_groups.push(EffectGroup {
            guards: vec![number("x", Compare::Equal, 1)],
            effects: vec![Effect::Set {
                key: "done".into(),
                value: FactValue::Bool(true),
            }],
        });
        let goal = Goal(vec![Predicate {
            key: "done".into(),
            compare: Compare::Equal,
            value: FactValue::Bool(true),
        }]);
        assert_eq!(
            plan(&initial, &[action.clone()], &goal, limits()),
            Ok(vec!["activate".into()])
        );
        action.effect_groups.reverse();
        assert_eq!(
            plan(&initial, &[action], &goal, limits()),
            Err(Failure::NoPlan)
        );
    }

    #[test]
    fn missing_or_wrong_type_is_unknown_even_for_inequality() {
        let facts = Facts::from([("x".into(), FactValue::Bool(true))]);
        assert!(!number("missing", Compare::NotEqual, 0).matches(&facts));
        assert!(!number("x", Compare::NotEqual, 0).matches(&facts));
        let add = EffectGroup {
            guards: vec![],
            effects: vec![Effect::Add {
                key: "x".into(),
                amount: 1,
            }],
        };
        assert_eq!(apply(&facts, &[add], 8), Ok(None));
    }

    #[test]
    fn failed_projection_does_not_mutate_snapshot_or_keep_partial_effects() {
        let facts = Facts::from([("x".into(), FactValue::Number(i64::MAX))]);
        let groups = [EffectGroup {
            guards: vec![],
            effects: vec![
                Effect::Set {
                    key: "changed".into(),
                    value: FactValue::Bool(true),
                },
                Effect::Add {
                    key: "x".into(),
                    amount: 1,
                },
            ],
        }];
        assert_eq!(apply(&facts, &groups, 8), Ok(None));
        assert_eq!(facts.len(), 1);
        assert_eq!(facts["x"], FactValue::Number(i64::MAX));
    }

    #[test]
    fn retained_successors_are_bounded_before_the_next_expansion() {
        let initial = Facts::from([("x".into(), FactValue::Number(0))]);
        let actions = [
            transition("a", 0, 1, 1),
            transition("b", 0, 2, 1),
            transition("c", 0, 3, 1),
        ];
        assert_eq!(
            plan(
                &initial,
                &actions,
                &Goal(vec![number("x", Compare::Equal, 4)]),
                Limits {
                    max_nodes: 2,
                    ..limits()
                }
            ),
            Err(Failure::NodeBudgetExceeded)
        );
    }

    #[test]
    fn inapplicable_candidates_are_also_charged() {
        let initial = Facts::from([("x".into(), FactValue::Number(0))]);
        let actions = [transition("a", 9, 1, 1), transition("b", 9, 2, 1)];
        assert_eq!(
            plan(
                &initial,
                &actions,
                &Goal(vec![number("x", Compare::Equal, 3)]),
                Limits {
                    max_candidates: 1,
                    ..limits()
                }
            ),
            Err(Failure::CandidateBudgetExceeded)
        );
    }

    #[test]
    fn model_strings_collections_and_state_growth_have_separate_bounds() {
        let initial = Facts::from([("x".into(), FactValue::Number(0))]);
        let mut action = transition("advance", 0, 1, 1);
        let goal = Goal(vec![number("x", Compare::Equal, 2)]);
        assert_eq!(
            plan(
                &initial,
                &[action.clone()],
                &goal,
                Limits {
                    max_model_terms: 2,
                    ..limits()
                }
            ),
            Err(Failure::ModelBudgetExceeded)
        );
        assert_eq!(
            plan(
                &initial,
                &[action.clone()],
                &goal,
                Limits {
                    max_model_bytes: 2,
                    ..limits()
                }
            ),
            Err(Failure::ModelBudgetExceeded)
        );
        assert_eq!(
            plan(
                &initial,
                &[action.clone()],
                &goal,
                Limits {
                    max_actions: 0,
                    ..limits()
                }
            ),
            Err(Failure::ActionBudgetExceeded)
        );
        assert_eq!(
            plan(
                &initial,
                &[action.clone()],
                &goal,
                Limits {
                    max_facts: 0,
                    ..limits()
                }
            ),
            Err(Failure::FactBudgetExceeded)
        );
        action.effect_groups[0].effects.push(Effect::Set {
            key: "new".into(),
            value: FactValue::Bool(true),
        });
        assert_eq!(
            plan(
                &initial,
                &[action],
                &goal,
                Limits {
                    max_facts: 1,
                    ..limits()
                }
            ),
            Err(Failure::FactBudgetExceeded)
        );
    }

    #[test]
    fn zero_search_budget_allows_already_satisfied_goal_but_validates_model() {
        let initial = Facts::new();
        assert_eq!(
            plan(
                &initial,
                &[],
                &Goal(vec![]),
                Limits {
                    max_nodes: 0,
                    max_candidates: 0,
                    ..limits()
                }
            ),
            Ok(vec![])
        );
        assert_eq!(
            plan(
                &initial,
                &[],
                &Goal(vec![number("unknown", Compare::Equal, 1)]),
                Limits {
                    max_nodes: 0,
                    ..limits()
                }
            ),
            Err(Failure::NodeBudgetExceeded)
        );
        let mut invalid = transition("bad", 0, 1, 1);
        invalid.effect_groups[0].effects = vec![Effect::Set {
            key: String::new(),
            value: FactValue::Bool(true),
        }];
        assert_eq!(
            plan(&initial, &[invalid], &Goal(vec![]), limits()),
            Err(Failure::InvalidActionModel)
        );
    }
}
