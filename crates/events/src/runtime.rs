use crate::catalog::compile;
use crate::*;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Limits {
    pub rows_per_brick: usize,
    pub bricks: usize,
    pub pending: usize,
    pub named_targets: usize,
    pub origins: usize,
    pub steps_per_phase: usize,
    pub steps_per_origin: usize,
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
            loop_warning_depth: 256,
            state_bytes: 128 << 20,
            expansions_per_phase: 8192,
            expansions_per_origin: 4096,
        }
    }
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
    pub diagnostics: Vec<String>,
    pub changed_programs: BTreeSet<Id>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Job {
    context: Trigger,
    target: Entity,
    row: u16,
    output: String,
    action: Action,
    row_snapshot: Row,
    due: u64,
    sequence: u64,
    cancelable: bool,
    depth: u32,
    #[serde(skip)]
    encoded_bytes: usize,
}
impl Job {
    fn measure(&mut self) {
        self.encoded_bytes = serde_json::to_vec(&self.row_snapshot)
            .expect("validated row")
            .len()
            + serde_json::to_vec(&self.action)
                .expect("validated action")
                .len()
            + self.output.len()
            + 512
            + serde_json::to_vec(&self.context)
                .expect("validated context")
                .len();
    }
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
    bricks: Vec<BrickProgram>,
    jobs: Vec<Job>,
    reappear: Vec<(Id, u64)>,
}
struct Plan {
    jobs: Vec<Job>,
    cancel: BTreeSet<Id>,
}
struct CompiledRow {
    input: String,
    class: Class,
    output: String,
    action: Action,
    cost: usize,
}
pub struct EventWorld {
    catalog: Catalog,
    bindings: Bindings,
    limits: Limits,
    now: u64,
    next_sequence: u64,
    last_origin: u64,
    bricks: BTreeMap<Id, BrickProgram>,
    compiled: BTreeMap<Id, Vec<Option<CompiledRow>>>,
    names: BTreeMap<(u64, String), BTreeSet<Id>>,
    queues: BTreeMap<u64, BTreeMap<(u64, u64), Job>>,
    pending: usize,
    held: BTreeMap<u64, Job>,
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
            bricks: BTreeMap::new(),
            compiled: BTreeMap::new(),
            names: BTreeMap::new(),
            queues: BTreeMap::new(),
            pending: 0,
            held: BTreeMap::new(),
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
    pub fn pending(&self) -> usize {
        self.pending
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
                    output: output.name.clone(),
                    action,
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
        if mode == CancelMode::AuthoredDelayed && !self.delayed.contains_key(&id) {
            return 0;
        }
        self.retain_jobs(|j| !(j.context.source == id && (mode == CancelMode::All || j.cancelable)))
    }
    pub fn cancel_origin(&mut self, origin: u64) -> usize {
        self.retain_jobs(|j| j.context.origin != origin)
    }
    fn retain_jobs(&mut self, keep: impl Fn(&Job) -> bool) -> usize {
        let before = self.pending;
        for q in self.queues.values_mut() {
            q.retain(|_, j| keep(j));
        }
        self.queues.retain(|_, q| !q.is_empty());
        self.held.retain(|_, j| keep(j));
        self.pending = self.queues.values().map(BTreeMap::len).sum::<usize>() + self.held.len();
        self.job_bytes = self
            .queues
            .values()
            .flat_map(|q| q.values())
            .chain(self.held.values())
            .map(|j| j.encoded_bytes)
            .sum();
        self.delayed.clear();
        for j in self
            .queues
            .values()
            .flat_map(|q| q.values())
            .chain(self.held.values())
        {
            if j.cancelable {
                let entry = self.delayed.entry(j.context.source).or_default();
                entry.0 += 1;
                entry.1 += j.encoded_bytes;
            }
        }
        before - self.pending
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
    fn plan(&self, t: &Trigger, fired_at: u64, depth: u32) -> Result<Plan> {
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
        };
        if input.name.eq_ignore_ascii_case("onRelay") && brick.implicit_cancel_relays {
            plan.cancel.insert(brick.id);
        }
        let context_cost = serde_json::to_vec(t)?.len();
        for (idx, (row, compiled)) in brick.rows.iter().zip(&self.compiled[&brick.id]).enumerate() {
            let Some(compiled) = compiled else {
                continue;
            };
            if !row.enabled || compiled.input != input.id {
                continue;
            }
            let action = &compiled.action;
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
                    action: action.clone(),
                    row_snapshot: row.clone(),
                    due: fired_at
                        .checked_add(u64::from(row.delay_ms) * 1000)
                        .context("Event deadline overflow")?,
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
        let (cancelled, cancelled_bytes) = p
            .cancel
            .iter()
            .filter_map(|id| self.delayed.get(id))
            .fold((0usize, 0usize), |(n, b), (dn, db)| (n + dn, b + db));
        let added_bytes: usize = p.jobs.iter().map(|j| j.encoded_bytes).sum();
        let mut origins: BTreeSet<u64> = if cancelled == 0 {
            self.queues
                .keys()
                .chain(self.held.values().map(|j| &j.context.origin))
                .copied()
                .collect()
        } else {
            self.queues
                .values()
                .flat_map(|q| q.values())
                .chain(self.held.values())
                .filter(|j| !j.cancelable || !p.cancel.contains(&j.context.source))
                .map(|j| j.context.origin)
                .collect()
        };
        origins.extend(p.jobs.iter().map(|j| j.context.origin));
        self.pending - cancelled + p.jobs.len() <= self.limits.pending
            && self.program_bytes + self.job_bytes - cancelled_bytes + added_bytes
                <= self.limits.state_bytes
            && origins.len() <= self.limits.origins
            && self
                .next_sequence
                .checked_add(p.jobs.len() as u64)
                .is_some()
    }
    fn commit(&mut self, p: Plan) -> usize {
        if p.jobs.is_empty() && p.cancel.is_empty() {
            return 0;
        }
        let n = if p.cancel.iter().all(|id| !self.delayed.contains_key(id)) {
            0
        } else {
            self.retain_jobs(|j| !j.cancelable || !p.cancel.contains(&j.context.source))
        };
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
            .insert((j.due, j.sequence), j);
        self.pending += 1;
    }
    /// Atomic admission. On error caller retains the input and explicitly retries/reports it.
    pub fn trigger(&mut self, t: Trigger) -> Result<usize> {
        let p = self.plan(&t, self.now, 0)?;
        ensure!(
            self.can_commit(&p),
            "Event admission backpressure: no mutation committed"
        );
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
                    && !(row.delay_ms == 0 && matches!(c.action, Action::Cancel))
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
            let child = self.plan(&t, self.now, parent.depth.saturating_add(1))?;
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
        self.now = now_us;
        Ok(())
    }
    pub fn advance(&mut self, now_us: u64, host: &mut impl Host) -> Result<RunReport> {
        ensure!(now_us >= self.now, "Event clock cannot run backwards");
        self.now = now_us;
        let mut r = RunReport::default();
        let mut blocked = BTreeSet::new();

        while r.steps < self.limits.steps_per_phase {
            let mut candidates: Vec<_> = self
                .queues
                .iter()
                .filter(|(origin, q)| {
                    !blocked.contains(*origin)
                        && r.origins.get(origin).map_or(0, |v| v.steps)
                            < self.limits.steps_per_origin
                        && q.first_key_value()
                            .is_some_and(|((due, _), _)| *due <= self.now)
                })
                .map(|(o, _)| *o)
                .collect();
            if candidates.is_empty() {
                break;
            }
            candidates.sort_by_key(|o| (*o <= self.last_origin, *o));
            let origin = candidates[0];
            self.last_origin = origin;
            let q = self.queues.get_mut(&origin).unwrap();
            let (_, job) = q.pop_first().unwrap();
            self.pending -= 1;
            if q.is_empty() {
                self.queues.remove(&origin);
            }
            self.remove_job_index(&job);
            r.steps += 1;
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
                || (!matches!(job.action, Action::Reappear(_))
                    && job.context.client.is_some_and(|c| !host.alive(c)))
            {
                r.stale += 1;
                continue;
            }
            if !matches!(job.action, Action::Reappear(_))
                && !host.permitted(&job.context, job.target, &job.output)
            {
                r.rejected += 1;
                Self::note(
                    &mut r,
                    format!("origin {origin}: permission rejected {}", job.output),
                );
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
                    self.held.insert(job.sequence, job);
                    self.pending += 1;
                    r.admission_backpressure += 1;
                    r.origins.get_mut(&origin).unwrap().deferred += 1;
                }
            }
        }
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
        r.global_budget_limited = r.steps >= self.limits.steps_per_phase && r.due_pending > 0;
        Ok(r)
    }
    fn execute(&mut self, j: &Job, host: &mut impl Host, r: &mut RunReport) -> Result<bool> {
        let mut child = Plan {
            jobs: Vec::new(),
            cancel: BTreeSet::new(),
        };
        let mut digit = None;
        let mut timer = None;
        let mut expanded = 0;
        let intent = match &j.action {
            Action::Cancel => {
                r.cancelled += self.cancel_source(j.target.id, CancelMode::AuthoredDelayed);
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
                    job.action = Action::Reappear(token);
                    job.context.source = j.target.id;
                    job.output = "reappear".into();
                    job.cancelable = false;
                    job.due = self
                        .now
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
            output: j.output.clone(),
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
        if let Some(b) = self.bricks.get_mut(&id) {
            match selection {
                RowSelection::All => {
                    for row in &mut b.rows {
                        row.enabled = value.unwrap_or(!row.enabled)
                    }
                }
                RowSelection::Indices(ids) => {
                    for index in ids {
                        if let Some(row) = b.rows.get_mut(*index as usize) {
                            row.enabled = value.unwrap_or(!row.enabled);
                        }
                    }
                }
            }
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
                matches!(j.action, Action::Reappear(_)) || j.cancelable == expected_cancel,
                "Checkpoint cancellation flag mismatch"
            );
            let horizon = if matches!(j.action, Action::Reappear(_)) {
                300_000_000
            } else {
                u64::from(j.row_snapshot.delay_ms) * 1000
            };
            ensure!(
                j.due <= s.now.saturating_add(horizon),
                "Checkpoint deadline outside authored horizon"
            );
            match j.action {
                Action::Reappear(token) => ensure!(
                    matches!(expected,Action::Intent(Intent::Brick(BrickOp::Disappear{seconds})) if seconds>0)
                        && j.output == "reappear"
                        && !j.cancelable
                        && token > 0
                        && token < s.next_sequence,
                    "Invalid reappear schedule"
                ),
                _ => ensure!(
                    expected == j.action
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
