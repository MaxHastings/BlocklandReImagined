use crate::catalog::compile;
use crate::*;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Limits {
    pub rows_per_brick: usize,
    pub bricks: usize,
    pub pending: usize,
    pub named_targets: usize,
    pub origins: usize,
    pub steps_per_phase: usize,
    pub steps_per_origin: usize,
    /// Work one owner's bricks may do in one phase, across all their
    /// activations, in cost units (`RunReport::cost`), so one owner's loops
    /// cannot take every other owner's turn.
    #[serde(default = "default_cost_per_scope")]
    pub cost_per_scope: usize,
    /// Work every owner together may do in one phase, in cost units.
    #[serde(default = "default_cost_per_phase")]
    pub cost_per_phase: usize,
    pub loop_warning_depth: u32,
    pub state_bytes: usize,
    pub expansions_per_phase: usize,
    pub expansions_per_origin: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            rows_per_brick: 4096,
            bricks: 65536,
            pending: 131072,
            named_targets: 4096,
            origins: 1024,
            steps_per_phase: 32768,
            steps_per_origin: 8192,
            cost_per_scope: default_cost_per_scope(),
            cost_per_phase: default_cost_per_phase(),
            loop_warning_depth: 256,
            state_bytes: 128 << 20,
            expansions_per_phase: 8192,
            expansions_per_origin: 4096,
        }
    }
}
fn default_cost_per_scope() -> usize {
    4096
}
fn default_cost_per_phase() -> usize {
    32768
}
/// What one row costs against the budgets: one unit for the row, plus one
/// for every job it expands into (a relay or a named target reaching many
/// bricks). Counted, never timed, so a run is the same on any machine.
fn row_cost(expanded: usize) -> usize {
    1 + expanded
}
impl Limits {
    fn validate(self) -> Result<()> {
        ensure!(
            self.rows_per_brick > 0
                && self.rows_per_brick <= 4096
                && self.bricks > 0
                && self.bricks <= 262144
                && self.pending > 0
                && self.pending <= 262144
                && self.named_targets > 0
                && self.named_targets <= 65536
                && self.origins > 0
                && self.origins <= 4096
                && self.steps_per_phase > 0
                && self.steps_per_phase <= 1048576
                && self.steps_per_origin > 0
                && self.steps_per_origin <= 1048576
                && self.cost_per_scope > 0
                && self.cost_per_scope <= 1048576
                && self.cost_per_phase > 0
                && self.cost_per_phase <= 4194304
                && self.loop_warning_depth > 0
                && self.state_bytes >= 4096
                && self.state_bytes <= 256 << 20
                && self.expansions_per_phase > 0
                && self.expansions_per_phase <= 262144
                && self.expansions_per_origin > 0
                && self.expansions_per_origin <= 262144,
            "Invalid event limits"
        );
        Ok(())
    }
}
/// A row's owner and the phase's expansion count when it started.
#[derive(Clone, Copy)]
struct Charge {
    scope: u64,
    expanded: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelMode {
    AuthoredDelayed,
    All,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct OriginReport {
    pub steps: usize,
    pub applied: usize,
    pub deferred: usize,
    pub loops: usize,
    pub pending_due: usize,
    pub oldest_due_age_us: u64,
    pub budget_limited: bool,
    pub expanded: usize,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ScopeReport {
    pub steps: usize,
    /// Cost units this owner's rows spent (`row_cost`).
    pub cost: usize,
    /// This owner's rows reached `cost_per_scope`; the rest waited for the
    /// next phase.
    pub budget_limited: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RunReport {
    pub steps: usize,
    pub applied: usize,
    pub cancelled: usize,
    pub stale: usize,
    pub rejected: usize,
    pub pending: usize,
    pub due_pending: usize,
    pub oldest_due_age_us: u64,
    pub admission_backpressure: usize,
    pub global_budget_limited: bool,
    pub expanded: usize,
    pub state_bytes: usize,
    pub origins: BTreeMap<u64, OriginReport>,
    /// Owners whose bricks ran rows this phase.
    pub scopes: BTreeMap<u64, ScopeReport>,
    /// Cost units every row spent this phase (`row_cost`).
    pub cost: usize,
    pub diagnostics: Vec<String>,
    pub changed_programs: BTreeSet<Id>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Job {
    context: Trigger,
    target: Entity,
    row: u16,
    /// The compiled row's output, action and authored row, shared with its
    /// program: a relay loop queues thousands of jobs a tick, and each one
    /// only reads them.
    #[serde(with = "shared")]
    output: Arc<str>,
    #[serde(with = "shared")]
    action: Arc<Action>,
    #[serde(with = "shared")]
    row_snapshot: Arc<Row>,
    /// Logical time the row was scheduled: its activation's time, or the
    /// due time of the job whose relay or chain fired it.
    #[serde(default)]
    scheduled: u64,
    due: u64,
    /// Which activation of its host tick set this row off. v20 stamps every
    /// input with its own millisecond; activations sharing one of our ticks
    /// keep that order, and everything they schedule inherits it.
    #[serde(default)]
    order: u32,
    sequence: u64,
    cancelable: bool,
    depth: u32,
    #[serde(skip)]
    encoded_bytes: usize,
}
impl Job {
    fn measure(&mut self) {
        self.encoded_bytes = serde_json::to_vec(&*self.row_snapshot)
            .expect("validated row")
            .len()
            + serde_json::to_vec(&*self.action)
                .expect("validated action")
                .len()
            + self.output.len()
            + 512
            + context_bytes(&self.context);
    }
}
/// Serde for the `Arc`s a job shares with its compiled row, in the same
/// encoding as the values themselves.
mod shared {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::sync::Arc;
    pub fn serialize<T: Serialize + ?Sized, S: Serializer>(
        value: &Arc<T>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        (**value).serialize(serializer)
    }
    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<Arc<T>, D::Error>
    where
        T: ?Sized,
        Box<T>: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        Box::<T>::deserialize(deserializer).map(Arc::from)
    }
}
/// What a queued job's activation context counts against `state_bytes`: an
/// upper bound on its encoded size. Every chained row carries one, so it is
/// counted, not serialised.
fn context_bytes(t: &Trigger) -> usize {
    const ENTITY: usize = 80;
    128 + t.input.len() + ENTITY * (t.targets.len() + usize::from(t.client.is_some()))
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    schema_version: u32,
    catalog: String,
    bindings: Bindings,
    limits: Limits,
    now: u64,
    next_sequence: u64,
    last_origin: u64,
    #[serde(default)]
    next_order: u32,
    bricks: Vec<BrickProgram>,
    jobs: Vec<Job>,
    reappear: Vec<(Id, u64)>,
}
struct Plan {
    jobs: Vec<Job>,
    cancel: BTreeSet<Id>,
    /// Logical instant of the activation; its cancels see only jobs pending then.
    at: Instant,
}
/// A logical event time: microseconds, then activation order within them.
type Instant = (u64, u32);
struct CompiledRow {
    input: String,
    class: Class,
    output: Arc<str>,
    action: Arc<Action>,
    /// The authored row as queued jobs record it.
    row: Arc<Row>,
    cost: usize,
}
pub struct EventWorld {
    catalog: Catalog,
    bindings: Bindings,
    limits: Limits,
    now: u64,
    next_sequence: u64,
    last_origin: u64,
    next_order: u32,
    bricks: BTreeMap<Id, BrickProgram>,
    compiled: BTreeMap<Id, Vec<Option<CompiledRow>>>,
    names: BTreeMap<(u64, String), BTreeSet<Id>>,
    queues: BTreeMap<u64, BTreeMap<(u64, u32, u64), Job>>,
    pending: usize,
    held: BTreeMap<u64, Job>,
    /// How many held jobs each origin has, so admission counts origins
    /// without walking every held job.
    held_origins: BTreeMap<u64, usize>,
    program_costs: BTreeMap<Id, usize>,
    program_bytes: usize,
    job_bytes: usize,
    delayed: BTreeMap<Id, (usize, usize)>,
    reappear: BTreeMap<Id, u64>,
}
impl EventWorld {
    pub fn new(catalog: Catalog, bindings: Bindings, limits: Limits) -> Result<Self> {
        catalog.validate()?;
        limits.validate()?;
        ensure!(
            bindings.palette_len > 0 && bindings.palette_len <= 256,
            "Invalid native paint palette"
        );
        ensure!(
            serde_json::to_vec(&bindings)?.len() <= 8 << 20,
            "Event bindings exceed 8MiB"
        );
        Ok(Self {
            catalog,
            bindings,
            limits,
            now: 0,
            next_sequence: 1,
            last_origin: 0,
            next_order: 0,
            bricks: BTreeMap::new(),
            compiled: BTreeMap::new(),
            names: BTreeMap::new(),
            queues: BTreeMap::new(),
            pending: 0,
            held: BTreeMap::new(),
            held_origins: BTreeMap::new(),
            program_costs: BTreeMap::new(),
            program_bytes: 0,
            job_bytes: 0,
            delayed: BTreeMap::new(),
            reappear: BTreeMap::new(),
        })
    }
    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }
    /// Check colour parameters against a colorset of `palette_len` colours
    /// from now on, as the host's colorset grows (a loaded save's colours)
    /// or is replaced. Installed programs keep the rows they were checked
    /// with; the host reinstalls them.
    pub fn set_palette_len(&mut self, palette_len: usize) -> Result<()> {
        ensure!(
            palette_len > 0 && palette_len <= 256,
            "Invalid event colorset size {palette_len}"
        );
        self.bindings.palette_len = palette_len;
        Ok(())
    }
    pub fn pending(&self) -> usize {
        self.pending
    }
    /// The per-phase and per-owner work limits this world runs under.
    pub fn limits(&self) -> Limits {
        self.limits
    }
    /// Rows `input` on brick `id` would schedule now, as v20's
    /// `ProcessInputEvent` counts them against the schedule quota.
    pub fn activation_count(&self, id: Id, input: &str) -> Result<usize> {
        self.expansion_count(&[id], input)
    }
    /// Scheduled rows from bricks of owner `scope` still waiting to run.
    pub fn pending_for_scope(&self, scope: u64) -> usize {
        self.queues
            .values()
            .flat_map(|q| q.values())
            .chain(self.held.values())
            .filter(|j| {
                self.bricks
                    .get(&j.context.source)
                    .is_some_and(|b| b.owner_scope == scope)
            })
            .count()
    }
    pub fn now_us(&self) -> u64 {
        self.now
    }
    pub fn program(&self, id: Id) -> Option<&BrickProgram> {
        self.bricks.get(&id)
    }
    pub fn install_brick(&mut self, mut brick: BrickProgram) -> Result<()> {
        ensure!(
            brick.id.index > 0 && brick.id.generation > 0 && brick.print_count < 10,
            "Invalid event brick identity/state"
        );
        ensure!(
            brick.rows.len() <= self.limits.rows_per_brick,
            "Brick event row limit exceeded"
        );
        ensure!(
            self.bricks.contains_key(&brick.id) || self.bricks.len() < self.limits.bricks,
            "Event brick limit exceeded"
        );
        if let Some(n) = &brick.name {
            ensure!(
                !n.is_empty() && n.len() <= 128 && !n.contains(['\0', '\n', '\r']),
                "Invalid brick event name"
            );
        }
        let compiled = brick
            .rows
            .iter()
            .map(|row| -> Result<Option<CompiledRow>> {
                let class = self.catalog.validate_row(row, &self.bindings)?;
                if row.preserved.is_some() {
                    return Ok(None);
                }
                let output = self.catalog.output(class, &row.output).unwrap();
                let action = compile(class, &output.name, &row.params)?;
                let cost = serde_json::to_vec(row)?.len()
                    + serde_json::to_vec(&action)?.len()
                    + output.name.len()
                    + 512;
                Ok(Some(CompiledRow {
                    input: self.catalog.input(&row.input).unwrap().id.clone(),
                    class,
                    output: output.name.as_str().into(),
                    action: Arc::new(action),
                    row: Arc::new(row.clone()),
                    cost,
                }))
            })
            .collect::<Result<Vec<_>>>()?;
        brick.implicit_cancel_relays |= brick.rows.iter().any(|r| {
            self.catalog
                .input(&r.input)
                .is_some_and(|i| i.name.eq_ignore_ascii_case("onRelay"))
                && matches!(r.target, Target::Slot(Slot::SelfBrick))
                && self
                    .catalog
                    .output(Class::Brick, &r.output)
                    .is_some_and(|o| o.name.eq_ignore_ascii_case("fireRelay"))
        });
        let cost = serde_json::to_vec(&brick)?.len()
            + brick.rows.iter().filter(|r| r.enabled).count()
            + 64;
        let old_cost = self.program_costs.get(&brick.id).copied().unwrap_or(0);
        ensure!(
            self.program_bytes - old_cost + cost + self.job_bytes <= self.limits.state_bytes,
            "Event program/state byte budget exceeded"
        );
        self.program_bytes = self.program_bytes - old_cost + cost;
        self.program_costs.insert(brick.id, cost);
        if let Some(old) = self.bricks.remove(&brick.id) {
            self.unindex(&old);
        }
        if let Some(n) = &brick.name {
            self.names
                .entry((brick.owner_scope, n.to_ascii_lowercase()))
                .or_default()
                .insert(brick.id);
        }
        self.compiled.insert(brick.id, compiled);
        self.bricks.insert(brick.id, brick);
        Ok(())
    }
    fn unindex(&mut self, b: &BrickProgram) {
        if let Some(n) = &b.name {
            let key = (b.owner_scope, n.to_ascii_lowercase());
            if let Some(ids) = self.names.get_mut(&key) {
                ids.remove(&b.id);
                if ids.is_empty() {
                    self.names.remove(&key);
                }
            }
        }
    }
    pub fn remove_brick(&mut self, id: Id) {
        self.compiled.remove(&id);
        if let Some(b) = self.bricks.remove(&id) {
            self.unindex(&b);
        }
        self.cancel_source(id, CancelMode::All);
        self.reappear.remove(&id);
        self.program_bytes -= self.program_costs.remove(&id).unwrap_or(0);
    }
    pub fn set_print_count(&mut self, id: Id, digit: u8) -> Result<()> {
        ensure!(digit < 10, "Print digit outside 0..9");
        self.bricks
            .get_mut(&id)
            .context("Missing print brick")?
            .print_count = digit;
        Ok(())
    }
    pub fn cancel_source(&mut self, id: Id, mode: CancelMode) -> usize {
        match mode {
            CancelMode::All => self.retain_jobs(|j| j.context.source != id),
            CancelMode::AuthoredDelayed => {
                self.cancel_authored(&BTreeSet::from([id]), (self.now, self.next_order))
            }
        }
    }
    /// v20's `cancelEvents` at logical time `at`: removes the sources'
    /// delayed rows that were already scheduled by then and have not run yet.
    /// Rows scheduled later (a click after a late cancel came due) and rows
    /// due earlier (which v20 had already run) are untouched.
    fn cancel_authored(&mut self, sources: &BTreeSet<Id>, at: Instant) -> usize {
        if sources.iter().all(|id| !self.delayed.contains_key(id)) {
            return 0;
        }
        self.retain_jobs(|j| !Self::cancelled_by(j, sources, at))
    }
    fn cancelled_by(j: &Job, sources: &BTreeSet<Id>, at: Instant) -> bool {
        j.cancelable
            && sources.contains(&j.context.source)
            && (j.scheduled, j.order) <= at
            && (j.due, j.order) >= at
    }
    pub fn cancel_origin(&mut self, origin: u64) -> usize {
        self.retain_jobs(|j| j.context.origin != origin)
    }
    /// Drop every queued or held job `keep` refuses, keeping the pending,
    /// byte and cancellation indexes in step job by job rather than
    /// rebuilding them: a relay's implicit cancel runs this on every hop.
    fn retain_jobs(&mut self, keep: impl Fn(&Job) -> bool) -> usize {
        let mut dropped = Vec::new();
        for q in self.queues.values_mut() {
            q.retain(|_, j| {
                keep(j) || {
                    dropped.push((j.encoded_bytes, j.cancelable.then_some(j.context.source)));
                    false
                }
            });
        }
        self.queues.retain(|_, q| !q.is_empty());
        let held_origins = &mut self.held_origins;
        self.held.retain(|_, j| {
            keep(j) || {
                dropped.push((j.encoded_bytes, j.cancelable.then_some(j.context.source)));
                let n = held_origins.get_mut(&j.context.origin).unwrap();
                *n -= 1;
                if *n == 0 {
                    held_origins.remove(&j.context.origin);
                }
                false
            }
        });
        for (bytes, source) in &dropped {
            self.pending -= 1;
            self.job_bytes -= bytes;
            if let Some(source) = source {
                let v = self.delayed.get_mut(source).unwrap();
                v.0 -= 1;
                v.1 -= bytes;
                if v.0 == 0 {
                    self.delayed.remove(source);
                }
            }
        }
        dropped.len()
    }
    fn targets(&self, source: &BrickProgram, t: &Trigger, target: &Target) -> Result<Vec<Entity>> {
        match target {
            Target::Named(name) => {
                let ids = self
                    .names
                    .get(&(source.owner_scope, name.to_ascii_lowercase()));
                ensure!(
                    ids.map_or(0, BTreeSet::len) <= self.limits.named_targets,
                    "Named target fanout exceeds explicit limit"
                );
                Ok(ids
                    .into_iter()
                    .flatten()
                    .copied()
                    .map(Entity::brick)
                    .collect())
            }
            Target::Slot(Slot::SelfBrick) => Ok(vec![Entity::brick(source.id)]),
            Target::Slot(slot) => Ok(t.targets.get(slot).copied().into_iter().collect()),
        }
    }
    fn validate_context(&self, t: &Trigger) -> Result<()> {
        ensure!(
            t.origin > 0 && t.targets.len() <= 7 && t.input.len() <= 256,
            "Invalid event context"
        );
        if let Some(c) = t.client {
            ensure!(
                c.class == Class::Client && c.id.index > 0 && c.id.generation > 0,
                "Invalid captured event client"
            );
        }
        let input = self
            .catalog
            .input(&t.input)
            .context("Unknown event input")?;
        for (slot, target) in &t.targets {
            let (_, class) = input
                .targets
                .iter()
                .find(|(s, _)| Slot::parse(s) == Some(*slot))
                .context("Unexpected input target slot")?;
            ensure!(
                Class::parse(class) == Some(target.class)
                    && target.id.index > 0
                    && target.id.generation > 0,
                "Input target class/generation mismatch"
            );
            ensure!(
                *slot != Slot::SelfBrick,
                "Self target derives from source, never context override"
            );
        }
        Ok(())
    }
    fn plan(&self, t: &Trigger, fired_at: u64, order: u32, depth: u32) -> Result<Plan> {
        let brick = self
            .bricks
            .get(&t.source)
            .context("Missing event source generation")?;
        ensure!(
            t.origin > 0 && t.targets.len() <= 7,
            "Invalid event origin/context"
        );
        self.validate_context(t)?;
        let input = self.catalog.input(&t.input).unwrap();
        let mut plan = Plan {
            jobs: Vec::new(),
            cancel: BTreeSet::new(),
            at: (fired_at, order),
        };
        if input.name.eq_ignore_ascii_case("onRelay") && brick.implicit_cancel_relays {
            plan.cancel.insert(brick.id);
        }
        let context_cost = context_bytes(t);
        for (idx, (row, compiled)) in brick.rows.iter().zip(&self.compiled[&brick.id]).enumerate() {
            let Some(compiled) = compiled else {
                continue;
            };
            if !row.enabled || compiled.input != input.id {
                continue;
            }
            let action = &*compiled.action;
            let targets = self.targets(brick, t, &row.target)?;
            for target in targets {
                ensure!(
                    target.class == compiled.class,
                    "Compiled target class mismatch"
                );
                if row.delay_ms == 0 && matches!(action, Action::Cancel) {
                    plan.cancel.insert(target.id);
                    continue;
                }
                ensure!(
                    plan.jobs.len() < self.limits.pending,
                    "Activation fanout exceeds pending limit; admission rejected atomically"
                );
                plan.jobs.push(Job {
                    context: t.clone(),
                    target,
                    row: idx as u16,
                    output: compiled.output.clone(),
                    action: compiled.action.clone(),
                    row_snapshot: compiled.row.clone(),
                    scheduled: fired_at,
                    due: fired_at
                        .checked_add(u64::from(row.delay_ms) * 1000)
                        .context("Event deadline overflow")?,
                    // A runaway zero-delay chain gives up its place in its
                    // instant and queues behind everything else due then, so
                    // other activations' chains keep taking turns with it.
                    order: if row.delay_ms == 0 && depth >= self.limits.loop_warning_depth {
                        u32::MAX
                    } else {
                        order
                    },
                    sequence: 0,
                    cancelable: row.delay_ms > 0
                        && (!input.name.eq_ignore_ascii_case("onToolBreak")
                            || matches!(row.target, Target::Named(_))),
                    depth: if row.delay_ms == 0 { depth } else { 0 },
                    encoded_bytes: compiled.cost + context_cost,
                });
            }
        }
        Ok(plan)
    }
    fn can_commit(&self, p: &Plan) -> bool {
        if p.jobs.is_empty() && p.cancel.is_empty() {
            return true;
        }
        // Refuse from the counts alone first: at most the cancelled sources'
        // delayed rows can make room. A full queue then turns a row away in
        // constant time instead of walking every queued job for each row
        // that retries, which made a tick's work grow with the queue.
        let added_bytes: usize = p.jobs.iter().map(|j| j.encoded_bytes).sum();
        let (most, most_bytes) = p
            .cancel
            .iter()
            .filter_map(|id| self.delayed.get(id))
            .fold((0usize, 0usize), |(n, b), (dn, db)| (n + dn, b + db));
        if self.pending - most.min(self.pending) + p.jobs.len() > self.limits.pending
            || (self.program_bytes + self.job_bytes + added_bytes).saturating_sub(most_bytes)
                > self.limits.state_bytes
        {
            return false;
        }
        let (cancelled, cancelled_bytes) =
            if p.cancel.iter().any(|id| self.delayed.contains_key(id)) {
                self.queues
                    .values()
                    .flat_map(|q| q.values())
                    .chain(self.held.values())
                    .filter(|j| Self::cancelled_by(j, &p.cancel, p.at))
                    .fold((0usize, 0usize), |(n, b), j| (n + 1, b + j.encoded_bytes))
            } else {
                (0, 0)
            };
        let origins = if cancelled == 0 {
            // Every queued origin stays: count the queues plus the origins
            // only the held jobs and this plan bring, without collecting
            // every origin on each chained row.
            let mut extra: Vec<u64> = self
                .held_origins
                .keys()
                .copied()
                .chain(p.jobs.iter().map(|j| j.context.origin))
                .filter(|origin| !self.queues.contains_key(origin))
                .collect();
            extra.sort_unstable();
            extra.dedup();
            self.queues.len() + extra.len()
        } else {
            let mut origins: BTreeSet<u64> = self
                .queues
                .values()
                .flat_map(|q| q.values())
                .chain(self.held.values())
                .filter(|j| !Self::cancelled_by(j, &p.cancel, p.at))
                .map(|j| j.context.origin)
                .collect();
            origins.extend(p.jobs.iter().map(|j| j.context.origin));
            origins.len()
        };
        self.pending - cancelled + p.jobs.len() <= self.limits.pending
            && self.program_bytes + self.job_bytes - cancelled_bytes + added_bytes
                <= self.limits.state_bytes
            && origins <= self.limits.origins
            && self
                .next_sequence
                .checked_add(p.jobs.len() as u64)
                .is_some()
    }
    fn commit(&mut self, p: Plan) -> usize {
        if p.jobs.is_empty() && p.cancel.is_empty() {
            return 0;
        }
        let n = self.cancel_authored(&p.cancel, p.at);
        for mut j in p.jobs {
            j.sequence = self.next_sequence;
            self.next_sequence += 1;
            self.push(j);
        }
        n
    }
    fn add_job_index(&mut self, j: &Job) {
        self.job_bytes += j.encoded_bytes;
        if j.cancelable {
            let v = self.delayed.entry(j.context.source).or_default();
            v.0 += 1;
            v.1 += j.encoded_bytes;
        }
    }
    fn remove_job_index(&mut self, j: &Job) {
        self.job_bytes -= j.encoded_bytes;
        if j.cancelable {
            let v = self.delayed.get_mut(&j.context.source).unwrap();
            v.0 -= 1;
            v.1 -= j.encoded_bytes;
            if v.0 == 0 {
                self.delayed.remove(&j.context.source);
            }
        }
    }
    fn push(&mut self, j: Job) {
        self.add_job_index(&j);
        self.queues
            .entry(j.context.origin)
            .or_default()
            .insert((j.due, j.order, j.sequence), j);
        self.pending += 1;
    }
    /// Atomic admission. On error caller retains the input and explicitly retries/reports it.
    pub fn trigger(&mut self, t: Trigger) -> Result<usize> {
        let p = self.plan(&t, self.now, self.next_order, 0)?;
        ensure!(
            self.can_commit(&p),
            "Event admission backpressure: no mutation committed"
        );
        self.next_order = self.next_order.saturating_add(1);
        let count = p.jobs.len();
        self.commit(p);
        Ok(count)
    }
    fn expansion_count(&self, ids: &[Id], input: &str) -> Result<usize> {
        let input = self.catalog.input(input).context("Missing chained input")?;
        let mut count = 0usize;
        for id in ids {
            let b = self.bricks.get(id).context("Missing chained brick")?;
            for (row, c) in b.rows.iter().zip(&self.compiled[id]) {
                if row.enabled
                    && let Some(c) = c
                    && c.input == input.id
                    && !(row.delay_ms == 0 && matches!(*c.action, Action::Cancel))
                {
                    count += match &row.target {
                        Target::Named(name) => {
                            let n = self
                                .names
                                .get(&(b.owner_scope, name.to_ascii_lowercase()))
                                .map_or(0, BTreeSet::len);
                            ensure!(n <= self.limits.named_targets, "Named fanout exceeds bound");
                            n
                        }
                        Target::Slot(Slot::SelfBrick) => 1,
                        Target::Slot(_) => 1,
                    };
                    ensure!(
                        count <= self.limits.pending,
                        "Chained activation exceeds pending bound"
                    );
                }
            }
        }
        Ok(count)
    }
    fn check_expansion(&self, j: &Job, ids: &[Id], input: &str, r: &RunReport) -> Result<usize> {
        let count = self.expansion_count(ids, input)?;
        ensure!(
            r.expanded + count <= self.limits.expansions_per_phase
                && r.origins.get(&j.context.origin).map_or(0, |v| v.expanded) + count
                    <= self.limits.expansions_per_origin,
            "Expansion budget reached; ordered continuation retained ({count} jobs)"
        );
        Ok(count)
    }
    fn account_expansion(r: &mut RunReport, origin: u64, count: usize) {
        r.expanded += count;
        r.origins.entry(origin).or_default().expanded += count;
    }
    fn chain(&self, parent: &Job, ids: Vec<Id>, input: &str) -> Result<Plan> {
        let mut p = Plan {
            jobs: Vec::new(),
            cancel: BTreeSet::new(),
            at: (parent.due, parent.order),
        };
        for id in ids {
            let mut t = Trigger::new(id, input, parent.context.origin);
            t.client = parent
                .context
                .client
                .or_else(|| parent.context.targets.get(&Slot::Client).copied());
            if !input.eq_ignore_ascii_case("onRelay")
                && let Some(client) = t.client
            {
                t.targets.insert(Slot::Client, client);
            }
            // Chained rows are scheduled when their parent ran, as v20's
            // `schedule` from inside the parent's call, not at this phase's
            // later clock.
            let child = self.plan(&t, parent.due, parent.order, parent.depth.saturating_add(1))?;
            ensure!(
                p.jobs.len() + child.jobs.len() <= self.limits.pending,
                "Relay fanout exceeds pending limit"
            );
            p.jobs.extend(child.jobs);
            p.cancel.extend(child.cancel);
        }
        Ok(p)
    }
    fn note(report: &mut RunReport, text: String) {
        if report.diagnostics.len() < 128 {
            report.diagnostics.push(text);
        }
    }
    /// Move the clock to the current host tick without running anything, so
    /// inputs triggered during that tick measure their delays from it.
    pub fn set_clock(&mut self, now_us: u64) -> Result<()> {
        ensure!(now_us >= self.now, "Event clock cannot run backwards");
        if now_us > self.now {
            self.next_order = 0;
        }
        self.now = now_us;
        Ok(())
    }
    /// The owner whose budget a job spends: its source brick's owner.
    fn scope_of(&self, j: &Job) -> u64 {
        self.bricks
            .get(&j.context.source)
            .map_or(0, |b| b.owner_scope)
    }
    /// Runs every due row in order, within the step and cost limits. Rows
    /// that do not fit wait, in order, for the next phase. Deterministic:
    /// the same queue runs the same rows on any machine.
    pub fn advance(&mut self, now_us: u64, host: &mut impl Host) -> Result<RunReport> {
        self.set_clock(now_us)?;
        let mut r = RunReport::default();
        let mut blocked = BTreeSet::new();
        let mut spent = BTreeSet::new();

        while r.steps < self.limits.steps_per_phase && r.cost < self.limits.cost_per_phase {
            // v20 runs every scheduled row from one queue, earliest due
            // first and then in scheduling order. Rows from different
            // activations therefore interleave exactly as they were
            // scheduled: a click's 100 ms revert runs before a later click's
            // glow, and a late cancelEvents only sees rows scheduled before
            // it. Origins are only budget and deferral boundaries.
            let Some(origin) = self
                .queues
                .iter()
                .filter(|(origin, _)| {
                    !blocked.contains(*origin)
                        && r.origins.get(origin).map_or(0, |v| v.steps)
                            < self.limits.steps_per_origin
                })
                .filter_map(|(origin, q)| {
                    q.first_key_value()
                        .filter(|((due, _, _), job)| {
                            *due <= self.now && !spent.contains(&self.scope_of(job))
                        })
                        .map(|(key, _)| (*key, *origin))
                })
                .min()
                .map(|(_, origin)| origin)
            else {
                break;
            };
            self.last_origin = origin;
            let q = self.queues.get_mut(&origin).unwrap();
            let (_, job) = q.pop_first().unwrap();
            self.pending -= 1;
            if q.is_empty() {
                self.queues.remove(&origin);
            }
            self.remove_job_index(&job);
            r.steps += 1;
            let scope = self.scope_of(&job);
            r.scopes.entry(scope).or_default().steps += 1;
            // Charged when the row ends, whichever way it ends.
            let charge = Charge {
                scope,
                expanded: r.expanded,
            };
            let stats = r.origins.entry(origin).or_default();
            stats.steps += 1;
            if job.depth >= self.limits.loop_warning_depth {
                stats.loops += 1;
                if stats.loops == 1 {
                    Self::note(
                        &mut r,
                        format!(
                            "origin {origin}: deep/repeated zero-delay chain at brick {} generation {} row {} (depth {})",
                            job.context.source.index,
                            job.context.source.generation,
                            job.row,
                            job.depth
                        ),
                    );
                }
            }
            if !self.bricks.contains_key(&job.context.source)
                || !host.alive(job.target)
                || (!matches!(*job.action, Action::Reappear(_))
                    && job.context.client.is_some_and(|c| !host.alive(c)))
            {
                r.stale += 1;
                self.charge(&mut r, &mut spent, charge);
                continue;
            }
            if !matches!(*job.action, Action::Reappear(_))
                && !host.permitted(&job.context, job.target, &job.output)
            {
                r.rejected += 1;
                Self::note(
                    &mut r,
                    format!("origin {origin}: permission rejected {}", job.output),
                );
                self.charge(&mut r, &mut spent, charge);
                continue;
            }
            let consumed_before = r.rejected + r.stale;
            match self.execute(&job, host, &mut r) {
                Ok(true) => {
                    if r.rejected + r.stale == consumed_before {
                        r.applied += 1;
                        r.origins.get_mut(&origin).unwrap().applied += 1;
                    }
                }
                Ok(false) => {
                    self.push(job);
                    blocked.insert(origin);
                    r.origins.get_mut(&origin).unwrap().deferred += 1;
                }
                Err(reason) => {
                    Self::note(
                        &mut r,
                        format!("origin {origin}: retained {}: {reason}", job.output),
                    );
                    self.add_job_index(&job);
                    *self.held_origins.entry(job.context.origin).or_default() += 1;
                    self.held.insert(job.sequence, job);
                    self.pending += 1;
                    r.admission_backpressure += 1;
                    r.origins.get_mut(&origin).unwrap().deferred += 1;
                }
            }
            self.charge(&mut r, &mut spent, charge);
        }
        self.held_origins.clear();
        for (_, job) in std::mem::take(&mut self.held) {
            self.pending -= 1;
            self.remove_job_index(&job);
            self.push(job);
        }
        r.pending = self.pending;
        r.state_bytes = self.program_bytes + self.job_bytes;
        for j in self.queues.values().flat_map(|q| q.values()) {
            if j.due <= self.now {
                r.due_pending += 1;
                let origin = r.origins.entry(j.context.origin).or_default();
                origin.pending_due += 1;
                origin.oldest_due_age_us = origin.oldest_due_age_us.max(self.now - j.due);
                origin.budget_limited = origin.steps >= self.limits.steps_per_origin;
                r.oldest_due_age_us = r.oldest_due_age_us.max(self.now - j.due);
            }
        }
        // Owners that spent their share and still have rows due.
        let waiting: BTreeMap<u64, usize> = self
            .queues
            .values()
            .flat_map(|q| q.values())
            .filter(|j| j.due <= self.now)
            .map(|j| self.scope_of(j))
            .filter(|scope| spent.contains(scope))
            .fold(BTreeMap::new(), |mut m, scope| {
                *m.entry(scope).or_default() += 1;
                m
            });
        for (scope, due) in waiting {
            let stats = r.scopes.entry(scope).or_default();
            stats.budget_limited = true;
            let text = format!(
                "owner {scope}: event budget for this tick reached ({} rows, cost {}); {due} due rows wait",
                stats.steps, stats.cost
            );
            Self::note(&mut r, text);
        }
        r.global_budget_limited = (r.steps >= self.limits.steps_per_phase
            || r.cost >= self.limits.cost_per_phase)
            && r.due_pending > 0;
        Ok(r)
    }
    /// Charge the row that ran (`row_cost`) to its owner and the phase,
    /// and stop that owner once it has had its share.
    fn charge(&self, r: &mut RunReport, spent: &mut BTreeSet<u64>, charge: Charge) {
        let cost = row_cost(r.expanded - charge.expanded);
        r.cost += cost;
        let stats = r.scopes.entry(charge.scope).or_default();
        stats.cost += cost;
        if stats.cost >= self.limits.cost_per_scope {
            spent.insert(charge.scope);
        }
    }
    fn execute(&mut self, j: &Job, host: &mut impl Host, r: &mut RunReport) -> Result<bool> {
        let mut child = Plan {
            jobs: Vec::new(),
            cancel: BTreeSet::new(),
            at: (j.due, j.order),
        };
        let mut digit = None;
        let mut timer = None;
        let mut expanded = 0;
        let intent = match &*j.action {
            Action::Cancel => {
                r.cancelled +=
                    self.cancel_authored(&BTreeSet::from([j.target.id]), (j.due, j.order));
                return Ok(true);
            }
            Action::SetEnabled(selection, value) => {
                self.change_enabled(j.target.id, selection, Some(*value));
                r.changed_programs.insert(j.target.id);
                return Ok(true);
            }
            Action::Toggle(selection) => {
                self.change_enabled(j.target.id, selection, None);
                r.changed_programs.insert(j.target.id);
                return Ok(true);
            }
            Action::Relay(direction) => {
                let ids = if let Some(direction) = direction {
                    let ids = host
                        .relay_neighbors(j.target.id, *direction, self.limits.named_targets)
                        .map_err(anyhow::Error::msg)?;
                    ensure!(
                        ids.len() <= self.limits.named_targets,
                        "Host relay query exceeds limit"
                    );
                    let scope = self
                        .bricks
                        .get(&j.target.id)
                        .context("Relay target is not registered")?
                        .owner_scope;
                    let mut ids: Vec<_> = ids
                        .into_iter()
                        .filter(|id| {
                            *id != j.target.id
                                && self.bricks.get(id).is_some_and(|b| b.owner_scope == scope)
                        })
                        .collect();
                    ids.sort();
                    ids.dedup();
                    ids
                } else {
                    vec![j.target.id]
                };
                let expanded = self.check_expansion(j, &ids, "onRelay", r)?;
                child = self.chain(j, ids, "onRelay")?;
                ensure!(
                    self.can_commit(&child),
                    "Relay pending capacity: continuation retained"
                );
                r.cancelled += self.commit(child);
                Self::account_expansion(r, j.context.origin, expanded);
                return Ok(true);
            }
            Action::Print { delta, set } => {
                let old = self
                    .bricks
                    .get(&j.target.id)
                    .context("Print target missing")?
                    .print_count;
                let raw = set.map_or(i16::from(old) + i16::from(*delta), i16::from);
                let value = raw.rem_euclid(10) as u8;
                digit = Some(value);
                if set.is_none() && !(0..10).contains(&raw) {
                    expanded = self.check_expansion(
                        j,
                        &[j.target.id],
                        if raw >= 10 {
                            "onPrintCountOverFlow"
                        } else {
                            "onPrintCountUnderFlow"
                        },
                        r,
                    )?;
                    child = self.chain(
                        j,
                        vec![j.target.id],
                        if raw >= 10 {
                            "onPrintCountOverFlow"
                        } else {
                            "onPrintCountUnderFlow"
                        },
                    )?;
                }
                Intent::Brick(BrickOp::PrintDigit(value))
            }
            Action::Reappear(token) => {
                if self.reappear.get(&j.target.id) != Some(token) {
                    r.stale += 1;
                    return Ok(true);
                }
                Intent::Brick(BrickOp::Presence {
                    rendering: true,
                    ray_casting: true,
                    colliding: true,
                    revive_fake_dead: false,
                })
            }
            Action::Intent(Intent::Brick(BrickOp::Disappear { seconds })) => {
                let token = self.next_sequence;
                timer = Some(token);
                if *seconds > 0 {
                    let mut job = j.clone();
                    job.action = Arc::new(Action::Reappear(token));
                    job.context.source = j.target.id;
                    job.output = "reappear".into();
                    job.cancelable = false;
                    job.scheduled = j.due;
                    job.due = j
                        .due
                        .checked_add(*seconds as u64 * 1_000_000)
                        .context("Reappear deadline overflow")?;
                    job.measure();
                    child.jobs.push(job);
                }
                let show = *seconds == 0;
                Intent::Brick(BrickOp::Presence {
                    rendering: show,
                    ray_casting: show,
                    colliding: show,
                    revive_fake_dead: show,
                })
            }
            Action::Intent(intent) => intent.clone(),
        };
        ensure!(
            self.can_commit(&child),
            "Event child admission backpressure; parent retained"
        );
        let client = j
            .context
            .client
            .or_else(|| j.context.targets.get(&Slot::Client).copied());
        let dispatch = Dispatch {
            source: j.context.source,
            target: j.target,
            origin: j.context.origin,
            client,
            input: self.catalog.input(&j.context.input).unwrap().name.clone(),
            row: j.row,
            output: j.output.to_string(),
            scheduled_us: j.due,
            now_us: self.now,
            intent,
        };
        match host.apply(&dispatch) {
            Apply::Applied => {
                if let Some(digit) = digit {
                    self.bricks.get_mut(&j.target.id).unwrap().print_count = digit;
                }
                if let Some(token) = timer {
                    self.reappear.insert(j.target.id, token);
                }
                r.cancelled += self.commit(child);
                Self::account_expansion(r, j.context.origin, expanded);
                Ok(true)
            }
            Apply::Deferred(reason) => {
                Self::note(r, format!("host deferred {}: {reason}", j.output));
                Ok(false)
            }
            Apply::Rejected(reason) => {
                r.rejected += 1;
                Self::note(r, format!("host rejected {}: {reason}", j.output));
                Ok(true)
            }
        }
    }
    fn change_enabled(&mut self, id: Id, selection: &RowSelection, value: Option<bool>) {
        let Some(b) = self.bricks.get_mut(&id) else {
            return;
        };
        let rows = b.rows.len();
        let compiled = self.compiled.get_mut(&id).unwrap();
        let mut change = |index: usize| {
            let Some(row) = b.rows.get_mut(index) else {
                return;
            };
            row.enabled = value.unwrap_or(!row.enabled);
            // Rows queued from now on record the row as it now stands.
            if let Some(Some(c)) = compiled.get_mut(index) {
                Arc::make_mut(&mut c.row).enabled = row.enabled;
            }
        };
        match selection {
            RowSelection::All => (0..rows).for_each(&mut change),
            RowSelection::Indices(ids) => ids.iter().for_each(|i| change(*i as usize)),
        }
    }
    pub fn save(&self) -> Result<Vec<u8>> {
        let state = Snapshot {
            schema_version: 1,
            catalog: self.catalog.fingerprint(),
            bindings: self.bindings.clone(),
            limits: self.limits,
            now: self.now,
            next_sequence: self.next_sequence,
            last_origin: self.last_origin,
            next_order: self.next_order,
            bricks: self.bricks.values().cloned().collect(),
            jobs: self
                .queues
                .values()
                .flat_map(|q| q.values().cloned())
                .collect(),
            reappear: self.reappear.iter().map(|(id, n)| (*id, *n)).collect(),
        };
        let bytes = serde_json::to_vec(&state)?;
        ensure!(bytes.len() <= 320 << 20, "Event checkpoint exceeds 320MiB");
        Ok(bytes)
    }
    pub fn restore(catalog: Catalog, bindings: Bindings, bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() <= 320 << 20, "Event checkpoint exceeds 320MiB");
        let s: Snapshot = serde_json::from_slice(bytes)?;
        ensure!(
            s.schema_version == 1 && s.catalog == catalog.fingerprint(),
            "Event checkpoint schema/catalog mismatch"
        );
        ensure!(
            s.bindings == bindings,
            "Event checkpoint native bindings mismatch"
        );
        let mut w = Self::new(catalog, bindings, s.limits)?;
        ensure!(
            s.bricks.len() <= w.limits.bricks
                && s.jobs.len() <= w.limits.pending
                && s.next_sequence > 0,
            "Event checkpoint limits"
        );
        for b in s.bricks {
            ensure!(!w.bricks.contains_key(&b.id), "Duplicate checkpoint brick");
            w.install_brick(b)?;
        }
        let mut sequence = BTreeSet::new();
        for mut j in s.jobs {
            ensure!(
                j.sequence > 0
                    && j.sequence < s.next_sequence
                    && sequence.insert(j.sequence)
                    && j.context.origin > 0
                    && j.scheduled <= j.due
                    && w.bricks.contains_key(&j.context.source)
                    && j.target.id.index > 0
                    && j.target.id.generation > 0,
                "Invalid checkpoint schedule identity"
            );
            ensure!(
                j.output.len() <= 128
                    && j.context.input.len() <= 256
                    && j.context.targets.len() <= 7,
                "Checkpoint job budget"
            );
            w.validate_context(&j.context)?;
            ensure!(
                j.row_snapshot.preserved.is_none()
                    && w.catalog.input(&j.context.input).map(|i| &i.id)
                        == w.catalog.input(&j.row_snapshot.input).map(|i| &i.id),
                "Checkpoint trigger/row mismatch"
            );
            let class = w.catalog.validate_row(&j.row_snapshot, &w.bindings)?;
            ensure!(
                class == j.target.class && j.row < 4096,
                "Checkpoint row/target mismatch"
            );
            let expected = compile(
                class,
                &w.catalog
                    .output(class, &j.row_snapshot.output)
                    .unwrap()
                    .name,
                &j.row_snapshot.params,
            )?;
            let expected_cancel = j.row_snapshot.delay_ms > 0
                && (!w
                    .catalog
                    .input(&j.context.input)
                    .unwrap()
                    .name
                    .eq_ignore_ascii_case("onToolBreak")
                    || matches!(j.row_snapshot.target, Target::Named(_)));
            ensure!(
                matches!(*j.action, Action::Reappear(_)) || j.cancelable == expected_cancel,
                "Checkpoint cancellation flag mismatch"
            );
            let horizon = if matches!(*j.action, Action::Reappear(_)) {
                300_000_000
            } else {
                u64::from(j.row_snapshot.delay_ms) * 1000
            };
            ensure!(
                j.due <= s.now.saturating_add(horizon),
                "Checkpoint deadline outside authored horizon"
            );
            match *j.action {
                Action::Reappear(token) => ensure!(
                    matches!(expected,Action::Intent(Intent::Brick(BrickOp::Disappear{seconds})) if seconds>0)
                        && &*j.output == "reappear"
                        && !j.cancelable
                        && token > 0
                        && token < s.next_sequence,
                    "Invalid reappear schedule"
                ),
                _ => ensure!(
                    expected == *j.action
                        && w.catalog
                            .output(class, &j.output)
                            .is_some_and(|o| o.name.eq_ignore_ascii_case(&j.row_snapshot.output)
                                || o.id == j.row_snapshot.output),
                    "Checkpoint action differs from typed row"
                ),
            }
            j.measure();
            ensure!(
                w.program_bytes + w.job_bytes + j.encoded_bytes <= w.limits.state_bytes,
                "Checkpoint state byte budget exceeded"
            );
            w.push(j);
        }
        ensure!(
            w.queues.len() <= w.limits.origins,
            "Checkpoint origin limit"
        );
        w.now = s.now;
        w.next_sequence = s.next_sequence;
        w.last_origin = s.last_origin;
        w.next_order = s.next_order;
        ensure!(
            s.reappear.len() <= w.limits.bricks,
            "Reappear checkpoint bound"
        );
        for (id, n) in s.reappear {
            ensure!(
                w.bricks.contains_key(&id)
                    && n > 0
                    && n <= s.next_sequence
                    && w.reappear.insert(id, n).is_none(),
                "Invalid reappear checkpoint"
            );
        }
        Ok(w)
    }
}
