//! Admitted damage jobs, bounded support proofs and cosmetic section commits.
//! The simulation owns original families; this scheduler only owns work.
use super::fracture::{self, Pieces, Reach};
use super::*;
use crate::{
    grid::{Bounds, QueryCursor},
    presentation::{BrickDeath, CueKind},
    simulation::SupportProbe,
};
use bri_minigames as mg;
use std::{cmp::Reverse, collections::BinaryHeap, sync::Arc};
const MAX_PIECES: usize = 16_384;
const PIECES_PER_BLAST: usize = 2048;
const WORK_PER_TICK: u32 = 8192;
// Payload/cue limits still bound the commit. Two hundred fifty-six small
// records avoid an extra second of delivery after a large proof completes.
const COMMIT_PER_TICK: usize = 256;
const MAX_JOBS: usize = 256;
pub(super) const MUTATION_BYTES_PER_TICK: u64 = 4 * 1024 * 1024;
/// Reserve presentation capacity before hiding any section member. An
/// over-envelope proof leaves its material standing, never invisible.
const MAX_SECTION_MEMBERS: usize = crate::presentation::MAX_SECTION_MEMBERS;
const PRESENTATION_LEASE: u64 = 3600;

#[derive(Default)]
pub(super) struct Destruction {
    pending: VecDeque<BlastJob>,
    searches: VecDeque<Search>,
    detach: VecDeque<Detached>,
    roots: BTreeMap<BrickId, (Bounds, bool, bool)>,
    palettes: BTreeMap<String, Pieces>,
    palette_revision: usize,
    tick_pieces_left: usize,
    mutation_bytes_left: u64,
    next_group: u64,
    work: DestructionWork,
    reservations: BTreeMap<(u64, u64), Reservation>,
}
struct Reservation {
    members: usize,
    looks: usize,
    quads: usize,
    draw_quads: usize,
    buckets: usize,
    until: u64,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct DestructionWork {
    pub split_bricks: u64,
    pub created_pieces: u64,
    pub capacity_holds: u64,
    pub support_visits: u64,
    pub query_work: u64,
    pub sections: u64,
    pub detached_bricks: u64,
    pub rejected_jobs: u64,
}
pub(super) struct Scope {
    source: OwnerId,
    impact: Vec3,
    game: Option<(mg::GameId, u64)>,
    rules: mg::MinigamesWorld,
    damage_source: mg::DamageSource,
    aliases: BTreeMap<OwnerId, OwnerId>,
    offline_host: Option<(OwnerId, Actor)>,
    own: BTreeSet<OwnerId>,
    lan: bool,
    delay: u64,
}
impl Scope {
    fn valid(&self, s: &Session) -> bool {
        self.game
            .is_none_or(|(id, round)| s.minigames.game(id).is_ok_and(|g| g.round == round))
    }
    pub(super) fn allows(&self, b: &bri_world::Brick) -> bool {
        match self.game {
            None if self.lan => true,
            None => self.own.contains(&b.owner),
            Some((id, _)) if self.lan => self.rules.game(id).is_ok_and(|g| g.settings.brick_damage),
            Some(_) => {
                self.rules.can_radius_damage(
                    self.damage_source,
                    mg::Target::Object {
                        kind: mg::ObjectKind::Brick,
                        owner: Some(mg::AccountId(
                            self.aliases.get(&b.owner).copied().unwrap_or_else(|| {
                                self.offline_host
                                    .as_ref()
                                    .filter(|(_, actor)| {
                                        actor.trusted(b.owner, bri_world::authority::trust::FULL)
                                    })
                                    .map_or(b.owner, |(id, _)| *id)
                            }),
                        )),
                        membership: mg::Membership::OwnerAt(b.position),
                        spawn_brick: false,
                    },
                ) == mg::Decision::Allow
            }
        }
    }
}
struct BlastJob {
    scope: Arc<Scope>,
    queue: VecDeque<(BrickId, super::debris::BrickBlast)>,
    query: Option<QueryCursor>,
    reach: Option<Reach>,
    blast: super::debris::BrickBlast,
    floating_limit: f32,
    current: Option<(BrickId, SupportProbe, super::debris::BrickBlast)>,
    made: usize,
}
struct Search {
    scope: Arc<Scope>,
    seeds: Arc<Vec<BrickId>>,
    tail: Option<SectionSeed>,
    tail_next: Option<BrickId>,
    tail_done: bool,
    parents: BTreeSet<BrickId>,
    next: usize,
    frontier: BinaryHeap<Reverse<(i32, BrickId)>>,
    component: BTreeSet<BrickId>,
    supported: BTreeSet<BrickId>,
    probe: Option<(BrickId, SupportProbe)>,
    area: Option<Bounds>,
    shape: SectionShape,
    looks: std::collections::HashSet<(ContentRef, u8, u8, u8, Option<ContentRef>)>,
    quads: usize,
    draw_quads: usize,
    expensive_look: bool,
    buckets: usize,
}
impl Search {
    fn new(scope: Arc<Scope>, seeds: Vec<BrickId>) -> Self {
        Self {
            scope,
            seeds: Arc::new(seeds),
            tail: None,
            tail_next: None,
            tail_done: false,
            parents: Default::default(),
            next: 0,
            frontier: Default::default(),
            component: Default::default(),
            supported: Default::default(),
            probe: None,
            area: None,
            shape: SectionShape::default(),
            looks: Default::default(),
            quads: 0,
            draw_quads: 0,
            expensive_look: false,
            buckets: 0,
        }
    }
    fn from_section(
        scope: Arc<Scope>,
        ids: Arc<BTreeSet<BrickId>>,
        after: Option<BrickId>,
    ) -> Self {
        let mut search = Self::new(scope, Vec::new());
        search.tail = Some(SectionSeed { ids, after });
        search.tail_next = after;
        search
    }
    fn restart(&mut self) {
        let mut fresh = Self::new(self.scope.clone(), Vec::new());
        fresh.seeds = self.seeds.clone();
        fresh.tail = self.tail.clone();
        fresh.tail_next = fresh.tail.as_ref().and_then(|tail| tail.after);
        *self = fresh;
    }
    fn next_seed(&mut self) -> Option<BrickId> {
        if let Some(&id) = self.seeds.get(self.next) {
            self.next += 1;
            return Some(id);
        }
        if self.tail_done {
            return None;
        }
        let tail = self.tail.as_ref()?;
        use std::ops::Bound::{Excluded, Unbounded};
        let id = tail
            .ids
            .range((self.tail_next.map_or(Unbounded, Excluded), Unbounded))
            .next()
            .copied();
        if id.is_none() {
            self.tail_done = true;
        }
        self.tail_next = id;
        id
    }
    fn push(&mut self, id: BrickId, sim: &Simulation) {
        if self.component.insert(id) {
            if let Some(root) = sim.damage_parent(id) {
                self.parents.insert(root);
            }
            let buckets = crate::grid::bucket_count(sim.index_bounds(id));
            self.buckets = self.buckets.saturating_add(buckets);
            self.expensive_look |= buckets > crate::presentation::MAX_SECTION_MEMBER_BUCKETS;
            let brick = &sim.state().bricks[&id];
            self.expensive_look |= brick.stored_bound() > MUTATION_BYTES_PER_TICK;
            self.draw_quads = self
                .draw_quads
                .saturating_add(sim.definitions.get(brick).map_or(0, |d| d.mesh.quads.len()));
            if self.looks.insert((
                brick.definition.clone(),
                brick.color,
                brick.color_effect,
                brick.shape_effect,
                brick.print.clone(),
            )) && let Ok(definition) = sim.definitions.get(brick)
            {
                let quads = definition.mesh.quads.len();
                self.quads = self.quads.saturating_add(quads);
                self.expensive_look |= quads > crate::presentation::MAX_SECTION_LOOK_QUADS;
            }
            if let Some((low, high)) = sim.brick_box(id) {
                self.shape.include(low, high, self.scope.impact);
            }
            self.frontier
                .push(Reverse((sim.index_bounds(id).min[1], id)));
        }
    }
    fn watches(&self, changes: &[Bounds]) -> bool {
        changes
            .iter()
            .any(|b| self.area.is_some_and(|a| a.intersection(*b).is_some()))
    }
    fn watch(&mut self, area: Bounds) {
        self.area = Some(self.area.map_or(area, |a| {
            let min = std::array::from_fn(|i| a.min[i].min(area.min[i]));
            let max: [i32; 3] = std::array::from_fn(|i| a.max()[i].max(area.max()[i]));
            Bounds {
                min,
                size: std::array::from_fn(|i| max[i] - min[i]),
            }
        }));
    }
}
#[derive(Clone)]
struct SectionShape {
    low: Vec3,
    high: Vec3,
    base: Vec3,
}
impl Default for SectionShape {
    fn default() -> Self {
        Self {
            low: Vec3::splat(f32::INFINITY),
            high: Vec3::splat(f32::NEG_INFINITY),
            base: Vec3::ZERO,
        }
    }
}
impl SectionShape {
    fn include(&mut self, low: Vec3, high: Vec3, impact: Vec3) {
        let base = Vec3::new(
            impact.x.max(low.x).min(high.x),
            low.y,
            impact.z.max(low.z).min(high.z),
        );
        if low.y < self.low.y - 0.001
            || ((low.y - self.low.y).abs() < 0.001
                && base.distance_squared(impact) < self.base.distance_squared(impact))
        {
            self.base = base;
        }
        self.low = self.low.min(low);
        self.high = self.high.max(high);
    }
}
#[derive(Clone)]
struct SectionSeed {
    ids: Arc<BTreeSet<BrickId>>,
    after: Option<BrickId>,
}
struct Detached {
    scope: Arc<Scope>,
    ids: Arc<BTreeSet<BrickId>>,
    next: Option<BrickId>,
    group: u64,
    started: u64,
    origin: Vec3,
    half: Vec3,
    pivot: Vec3,
    members: u32,
    area: Option<Bounds>,
}

impl Detached {
    fn remaining(&self) -> impl Iterator<Item = &BrickId> {
        use std::ops::Bound::{Excluded, Unbounded};
        self.ids
            .range((self.next.map_or(Unbounded, Excluded), Unbounded))
    }
    fn candidates(&mut self, count: usize, sim: &Simulation, bytes_left: &mut u64) -> Vec<BrickId> {
        let mut ids = Vec::new();
        for &id in self.remaining().take(count) {
            let cost = sim.state().bricks.get(&id).map_or(0, |b| b.stored_bound());
            if cost > *bytes_left {
                break;
            }
            *bytes_left -= cost;
            ids.push(id);
        }
        if let Some(&last) = ids.last() {
            self.next = Some(last);
        }
        ids
    }
}
impl Session {
    pub fn destruction_work(&self) -> DestructionWork {
        self.destruction.work
    }
    pub(super) fn fake_dead(&self, id: BrickId) -> bool {
        self.simulation
            .state()
            .bricks
            .get(&id)
            .is_some_and(|b| b.damage_hidden)
    }
    fn live_damage_brick(&self, id: BrickId) -> bool {
        self.simulation.state().bricks.contains_key(&id) && !self.fake_dead(id)
    }
    pub(super) fn sync_damage_lifecycle(&mut self) {
        let (changed, ended) = self.simulation.take_damage_changes();
        self.dirty.extend(changed.iter().copied());
        for id in ended {
            self.events.respawns.remove(&id);
            self.events.restore_pending.remove(&id);
        }
        for id in changed {
            if !self.simulation.state().bricks.contains_key(&id) {
                self.events.respawns.remove(&id);
                self.events.restore_pending.remove(&id);
                self.destruction.roots.remove(&id);
            }
        }
        self.flush_section_journal();
    }
    fn flush_section_journal(&mut self) {
        for ((started, group), members, retired) in
            self.simulation.take_section_barriers(self.cues.remaining())
        {
            let kind = if retired {
                self.destruction.reservations.remove(&(started, group));
                CueKind::BrickSectionRetired { started, group }
            } else {
                CueKind::BrickSectionReady {
                    started,
                    group,
                    members,
                }
            };
            self.cues.emit(self.simulation.state().tick, kind, [0.; 3]);
        }
    }
    pub(super) fn protected_foundation(&self, id: BrickId) -> bool {
        self.destruction
            .roots
            .get(&id)
            .is_some_and(|(bounds, _, protected)| {
                self.simulation.state().bricks.contains_key(&id)
                    && self.simulation.index_bounds(id) == *bounds
                    && *protected
            })
    }
    fn remember_probe(&mut self, id: BrickId, p: &SupportProbe) -> (bool, bool) {
        let b = &self.simulation.state().bricks[&id];
        let bounds = self.simulation.index_bounds(id);
        let old = self
            .destruction
            .roots
            .get(&id)
            .filter(|(at, _, _)| *at == bounds)
            .copied();
        let floating =
            b.base_plate && !p.grounded && !p.below && old.is_none_or(|(_, rooted, _)| rooted);
        let plate = match &b.definition {
            ContentRef::Resolved(d) => {
                self.tool_catalog.foundation_bricks.contains(d)
                    && self.simulation.definitions.entries[d].mesh.height_plates == 1
            }
            _ => false,
        };
        let rooted = p.grounded || floating;
        let protected = (plate && p.grounded) || floating;
        self.destruction
            .roots
            .insert(id, (bounds, floating, protected));
        // Any upper marked plate had a support before this operation. Never
        // reinterpret the resulting absence as a deliberate floating root.
        for &other in &p.neighbors {
            let at = self.simulation.index_bounds(other);
            if at.min[1] >= bounds.max()[1] {
                self.destruction
                    .roots
                    .entry(other)
                    .or_insert((at, false, false));
            }
        }
        (rooted, protected)
    }
    pub(super) fn damage_scope(&self, source: OwnerId, impact: Vec3) -> Option<Arc<Scope>> {
        let player = self.peers.get(&source)?.combat.player;
        if self.teleport_lockout(source, super::admin_players::TELEPORT_WEAPON_LOCK_MS, false) {
            return None;
        }
        let game = self.game_of(source);
        if game.is_some_and(|id| {
            self.minigames
                .game(id)
                .is_ok_and(|g| !g.settings.brick_damage)
        }) {
            return None;
        }
        let mut own = BTreeSet::from([source]);
        let mut aliases = BTreeMap::new();
        for &id in self
            .simulation
            .state()
            .owners
            .keys()
            .chain(self.peers.keys())
        {
            if self.brick_group_player(id) == Some(source) {
                own.insert(id);
            }
            aliases.insert(id, self.brick_group_owner_for(id, game));
        }
        Some(Arc::new(Scope {
            source,
            impact,
            game: game.and_then(|id| self.minigames.game(id).ok().map(|g| (id, g.round))),
            rules: self.minigames.clone(),
            damage_source: self.minigames.projectile_source(player).ok()?,
            aliases,
            offline_host: game
                .and_then(|id| self.minigames.game(id).ok())
                .and_then(|g| {
                    self.peers
                        .get(&g.owner.account.0)
                        .map(|p| (g.owner.account.0, p.actor.clone()))
                }),
            own,
            lan: self.lan_host,
            delay: self
                .minigames
                .respawn_delay(game, mg::RespawnObject::Brick)
                .unwrap_or(3600)
                .max(1),
        }))
    }
    pub(super) fn queue_blast(
        &mut self,
        source: OwnerId,
        target: Option<BrickId>,
        position: Vec3,
        impact: &bri_weapons::BrickImpact,
    ) {
        let Some(scope) = self.damage_scope(source, position) else {
            return;
        };
        if !impact.direct && target.is_some() || impact.max_volume <= 0.0 {
            return;
        }
        let radius = if target.is_some() {
            0.02
        } else {
            impact.radius
        };
        if target.is_none() && radius <= 0.0 {
            return;
        }
        let blast = super::debris::BrickBlast {
            origin: position,
            force: impact.force,
            radius,
            struck: target.is_some(),
        };
        let queue = target.into_iter().map(|id| (id, blast)).collect();
        let query = target.is_none().then(|| {
            let cell = crate::grid::CELL;
            let min = std::array::from_fn(|a| ((position[a] - radius) / cell[a]).floor() as i32);
            let max: [i32; 3] =
                std::array::from_fn(|a| ((position[a] + radius) / cell[a]).ceil() as i32 + 1);
            QueryCursor::new(Bounds {
                min,
                size: std::array::from_fn(|a| (max[a] - min[a]).max(1)),
            })
        });
        self.admit_damage_job(BlastJob {
            scope,
            queue,
            query,
            reach: Some(Reach {
                origin: position,
                radius,
                max_volume: impact.max_volume,
            }),
            blast,
            floating_limit: impact.max_floating_volume,
            current: None,
            made: 0,
        });
    }
    pub(super) fn queue_knockouts(
        &mut self,
        source: OwnerId,
        kills: &[(BrickId, super::debris::BrickBlast)],
    ) {
        let Some(scope) =
            self.damage_scope(source, kills.first().map_or(Vec3::ZERO, |k| k.1.origin))
        else {
            return;
        };
        let Some(&(_, blast)) = kills.first() else {
            return;
        };
        self.admit_damage_job(BlastJob {
            scope,
            queue: kills.iter().copied().collect(),
            query: None,
            reach: None,
            blast,
            floating_limit: f32::MAX,
            current: None,
            made: 0,
        });
    }
    fn admit_damage_job(&mut self, job: BlastJob) {
        if self.destruction.pending.len() + self.destruction.searches.len() >= MAX_JOBS {
            self.destruction.work.rejected_jobs += 1;
            super::events::note(
                &mut self.events.diagnostics,
                "Destruction work queue full; damage request rejected".into(),
            );
        } else {
            self.destruction.pending.push_back(job);
        }
    }
    fn hide_damage_bricks(&mut self, scope: &Scope, ids: &[BrickId]) -> Result<()> {
        let due = self.simulation.state().tick.saturating_add(scope.delay);
        let section = self.simulation.section_writer();
        for &id in ids {
            if self.simulation.damage_parent(id).is_none() {
                self.simulation.begin_damage_family(id);
                self.events.respawns.insert(id, due);
            }
        }
        self.simulation.damage_transaction(|sim| {
            sim.mutate_many(ids, |b| {
                b.visible = false;
                b.raycast = false;
                b.colliding = false;
                b.damage_hidden = true;
                b.damage_section = section.map(|(started, group)| Box::new([started, group]));
            })
        })?;
        self.dirty.extend(ids.iter().copied());
        Ok(())
    }
    pub(super) fn remove_damage_pieces(&mut self, ids: &[BrickId]) -> Result<()> {
        let engine = Actor {
            administrator: true,
            ..Default::default()
        };
        let ids: Vec<_> = ids
            .iter()
            .copied()
            .filter(|id| self.simulation.state().bricks.contains_key(id))
            .collect();
        self.simulation
            .damage_transaction(|sim| sim.remove_many(&engine, &ids))?;
        self.dirty.extend(ids.iter().copied());
        Ok(())
    }
    /// None defers an untouched hit until next tick's presentation/mutation
    /// allowance. Some(empty) is a permanent refusal or a protected brick.
    fn apply_damage(
        &mut self,
        job: &mut BlastJob,
        id: BrickId,
        probe: &SupportProbe,
        blast: super::debris::BrickBlast,
    ) -> Result<Option<Vec<BrickId>>> {
        if probe.exhausted {
            self.destruction.work.capacity_holds += 1;
            return Ok(Some(Vec::new()));
        }
        let mut seeds = probe.neighbors.clone();
        let (_, protected) = self.remember_probe(id, probe);
        let original = self.simulation.state().bricks[&id].clone();
        let d = self.simulation.definitions.get(&original)?;
        if protected || d.indestructible || !original.colliding || !job.scope.allows(&original) {
            return Ok(Some(Vec::new()));
        }
        let volume = d.mesh.footprint_studs[0] as f32
            * d.mesh.footprint_studs[1] as f32
            * d.mesh.height_plates as f32;
        let alpha = self.simulation.state().palette[usize::from(original.color)][3];
        let effective = if alpha < 0.95 {
            (volume * 0.25).floor()
        } else {
            volume
        };
        let above = probe.neighbors.iter().any(|&other| {
            self.simulation.index_bounds(other).min[1] >= self.simulation.index_bounds(id).max()[1]
        });
        let whole = job.reach.is_none_or(|r| {
            effective <= r.max_volume
                || (effective <= r.max_volume.max(job.floating_limit) && !(above && probe.below))
        });
        if whole {
            let cue = self.brick_kill_cue(id, Some(blast))?;
            if self.simulation.damage_parent(id).is_some() {
                self.remove_damage_pieces(&[id])?;
            } else {
                self.hide_damage_bricks(&job.scope, &[id])?;
                self.fire_input(id, "onBlownUp", Some(job.scope.source));
            }
            self.emit_brick_kill(cue);
        } else {
            if !d.is_plain_box() || original.print.is_some() {
                return Ok(Some(Vec::new()));
            }
            let namespace = content_namespace(&original.definition);
            if self.destruction.palette_revision != self.simulation.definitions.entries.len() {
                self.destruction.palettes.clear();
                self.destruction.palette_revision = self.simulation.definitions.entries.len();
            }
            let pieces = self
                .destruction
                .palettes
                .entry(namespace.clone())
                .or_insert_with(|| {
                    Pieces::in_namespace(&self.simulation.definitions, Some(&namespace))
                });
            let edges = fracture::edges(
                probe
                    .neighbors
                    .iter()
                    .map(|&id| self.simulation.index_bounds(id)),
            );
            let alpha = self.simulation.state().palette[usize::from(original.color)][3];
            let root = self.simulation.damage_parent(id).unwrap_or(id);
            let limit = PIECES_PER_BLAST
                .saturating_sub(job.made)
                .min(MAX_PIECES.saturating_sub(self.simulation.damage_part_count()))
                .min(
                    PIECES_PER_BLAST.saturating_sub(self.simulation.damage_family_part_count(root)),
                )
                .min(bri_world::MAX_BRICKS.saturating_sub(self.simulation.state().bricks.len()));
            let reach = job.reach.expect("split reach");
            let Some(plan) = fracture::split(
                self.simulation.index_bounds(id),
                &reach,
                alpha < 0.95,
                pieces,
                [&edges[0], &edges[1]],
                limit,
            ) else {
                self.destruction.work.capacity_holds += 1;
                return Ok(Some(Vec::new()));
            };
            if plan.standing.len() + plan.taken.len() > self.destruction.tick_pieces_left
                || plan.taken.len() + 1 > self.cues.remaining().saturating_sub(16)
            {
                return Ok(None);
            }
            let bricks: Vec<_> = plan
                .standing
                .iter()
                .chain(&plan.taken)
                .map(|cell| {
                    cell.brick(pieces, &original)
                        .context("Missing planned size")
                })
                .collect::<Result<_>>()?;
            let fresh_bytes: u64 = bricks.iter().map(|b| b.stored_bound()).sum();
            if fresh_bytes > MUTATION_BYTES_PER_TICK.saturating_sub(original.stored_bound()) {
                self.destruction.work.capacity_holds += 1;
                return Ok(Some(Vec::new()));
            }
            if fresh_bytes > self.destruction.mutation_bytes_left {
                return Ok(None);
            }
            self.destruction.mutation_bytes_left -= fresh_bytes;
            let fresh = match self
                .simulation
                .damage_transaction(|sim| sim.insert_pieces(bricks))
            {
                Ok(ids) => ids,
                Err(error) => {
                    self.destruction.work.capacity_holds += 1;
                    super::events::note(
                        &mut self.events.diagnostics,
                        format!("Split held without changing material: {error}"),
                    );
                    return Ok(Some(Vec::new()));
                }
            };
            job.made += fresh.len();
            self.destruction.tick_pieces_left -= fresh.len();
            self.destruction.work.split_bricks += 1;
            self.destruction.work.created_pieces += fresh.len() as u64;
            let taken = &fresh[plan.standing.len()..];
            let cues = taken
                .iter()
                .map(|&part| self.brick_kill_cue(part, Some(blast)))
                .collect::<Result<Vec<_>>>()?;
            if id == root {
                let (mut kind, position) = self.brick_kill_cue(id, None)?;
                if let CueKind::BrickKill { death, .. } = &mut kind {
                    *death = BrickDeath::Partition;
                }
                self.hide_damage_bricks(&job.scope, &[id])?;
                self.emit_brick_kill((kind, position));
            } else {
                self.remove_damage_pieces(&[id])?;
            }
            self.remove_damage_pieces(taken)?;
            let standing = &fresh[..plan.standing.len()];
            self.simulation.register_damage_parts(root, standing);
            self.dirty.extend(standing.iter().copied());
            seeds.extend_from_slice(standing);
            for cue in cues {
                self.emit_brick_kill(cue);
            }
            if id == root {
                self.fire_input(root, "onBlownUp", Some(job.scope.source));
            }
        }
        Ok(Some(seeds))
    }
    pub(super) fn step_damage_support(&mut self) -> Result<()> {
        if self.destruction.pending.is_empty()
            && self.destruction.searches.is_empty()
            && self.destruction.detach.is_empty()
        {
            self.sync_damage_lifecycle();
            let _ = self.simulation.take_support_edits();
            return Ok(());
        }
        self.simulation.hold_settle(true);
        let result = self.run_damage_support();
        self.simulation.hold_settle(false);
        result
    }
    fn run_damage_support(&mut self) -> Result<()> {
        let now = self.simulation.state().tick;
        self.destruction
            .reservations
            .retain(|_, reservation| reservation.until >= now);
        self.simulation.retire_section_journal();
        self.destruction.tick_pieces_left = PIECES_PER_BLAST;
        self.destruction.mutation_bytes_left = MUTATION_BYTES_PER_TICK;
        self.sync_damage_lifecycle();
        let (changes, map_change) = self.simulation.take_support_edits();
        for detached in std::mem::take(&mut self.destruction.detach) {
            if map_change
                || changes
                    .iter()
                    .any(|b| detached.area.is_some_and(|a| a.intersection(*b).is_some()))
            {
                self.finish_section(&detached);
                self.destruction.searches.push_back(Search::from_section(
                    detached.scope,
                    detached.ids,
                    detached.next,
                ));
            } else {
                self.destruction.detach.push_back(detached);
            }
        }
        for search in &mut self.destruction.searches {
            if map_change || search.watches(&changes) {
                search.restart();
            }
        }
        for job in &mut self.destruction.pending {
            if job.current.as_ref().is_some_and(|(_, p, _)| {
                map_change || changes.iter().any(|b| p.area.intersection(*b).is_some())
            }) {
                let (id, _, blast) = job.current.take().unwrap();
                if self.simulation.state().bricks.contains_key(&id) {
                    job.queue.push_front((id, blast));
                }
            }
        }
        let mut work = WORK_PER_TICK / 2;
        let mut commits = COMMIT_PER_TICK.min(self.cues.remaining().saturating_sub(16));
        while work > 1 && commits > COMMIT_PER_TICK / 2 {
            if let Some(mut job) = self.destruction.pending.pop_front() {
                if !job.scope.valid(self) {
                    continue;
                }
                if job.current.is_none() {
                    if let Some((id, blast)) = job.queue.pop_front() {
                        if self.live_damage_brick(id)
                            && self.simulation.state().bricks[&id].colliding
                            && job.scope.allows(&self.simulation.state().bricks[&id])
                        {
                            job.current =
                                Some((id, SupportProbe::new(&self.simulation, id), blast));
                        }
                    } else if let Some(query) = &mut job.query {
                        work -= 1;
                        match query.step(self.simulation.damage_index()) {
                            Some(Some(id)) if self.live_damage_brick(id) => {
                                let r = job.reach.unwrap();
                                if self.simulation.brick_box(id).is_some_and(|(lo, hi)| {
                                    r.origin.clamp(lo, hi).distance(r.origin) <= r.radius
                                }) {
                                    job.queue.push_back((id, job.blast));
                                }
                            }
                            None => job.query = None,
                            _ => {}
                        }
                    } else {
                        continue;
                    }
                }
                if let Some((id, mut probe, blast)) = job.current.take() {
                    if probe.step(&self.simulation, &mut work, |other| {
                        self.live_damage_brick(other)
                    })? {
                        let bytes = self
                            .simulation
                            .state()
                            .bricks
                            .get(&id)
                            .map_or(0, |b| b.stored_bound());
                        if bytes > MUTATION_BYTES_PER_TICK {
                            self.destruction.work.capacity_holds += 1;
                        } else if bytes > self.destruction.mutation_bytes_left {
                            job.current = Some((id, probe, blast));
                            self.destruction.pending.push_back(job);
                            break;
                        } else {
                            self.destruction.mutation_bytes_left -= bytes;
                            let Some(seeds) = self.apply_damage(&mut job, id, &probe, blast)?
                            else {
                                self.destruction.mutation_bytes_left += bytes;
                                job.current = Some((id, probe, blast));
                                self.destruction.pending.push_back(job);
                                break;
                            };
                            commits -= 1;
                            let retained_seeds: usize = self
                                .destruction
                                .searches
                                .iter()
                                .map(|s| {
                                    s.seeds.len() + s.tail.as_ref().map_or(0, |tail| tail.ids.len())
                                })
                                .sum();
                            if retained_seeds + seeds.len() > MAX_SECTION_MEMBERS * 2 {
                                self.destruction.work.capacity_holds += 1;
                            } else if !seeds.is_empty() {
                                if let Some(search) = self
                                    .destruction
                                    .searches
                                    .iter_mut()
                                    .find(|s| Arc::ptr_eq(&s.scope, &job.scope))
                                {
                                    Arc::make_mut(&mut search.seeds).extend(seeds);
                                } else {
                                    self.destruction
                                        .searches
                                        .push_back(Search::new(job.scope.clone(), seeds));
                                }
                            }
                        }
                    } else {
                        job.current = Some((id, probe, blast));
                    }
                }
                self.destruction.pending.push_back(job);
            } else {
                break;
            }
        }
        // Damage may invalidate a proof that started on an earlier tick.
        let (edits, map_edit) = self.simulation.take_support_edits();
        for pending in &mut self.destruction.pending {
            if pending
                .current
                .as_ref()
                .is_some_and(|(_, p, _)| edits.iter().any(|b| p.area.intersection(*b).is_some()))
            {
                let (id, _, blast) = pending.current.take().unwrap();
                pending.queue.push_front((id, blast));
            }
        }
        for search in &mut self.destruction.searches {
            if map_edit || search.watches(&edits) {
                search.restart();
            }
        }
        for detached in std::mem::take(&mut self.destruction.detach) {
            if map_edit
                || edits
                    .iter()
                    .any(|b| detached.area.is_some_and(|a| a.intersection(*b).is_some()))
            {
                self.finish_section(&detached);
                self.destruction.searches.push_back(Search::from_section(
                    detached.scope,
                    detached.ids,
                    detached.next,
                ));
            } else {
                self.destruction.detach.push_back(detached);
            }
        }
        work += WORK_PER_TICK / 2;
        let mut proof_members: usize = self
            .destruction
            .searches
            .iter()
            .map(|s| s.component.len() + s.supported.len())
            .sum();
        let mut waiting = 0;
        while work > 1 && commits > 0 {
            let Some(mut job) = self.destruction.searches.pop_front() else {
                break;
            };
            proof_members = proof_members.saturating_sub(job.component.len() + job.supported.len());
            work -= 1;
            if !job.scope.valid(self) {
                continue;
            }
            if job.frontier.is_empty() && job.probe.is_none() {
                if !job.component.is_empty() {
                    if job.expensive_look
                        || job.looks.len() > crate::presentation::MAX_SECTION_LOOKS
                        || job.quads > crate::presentation::MAX_SECTION_QUADS
                        || job.draw_quads > crate::presentation::MAX_SECTION_DRAW_QUADS
                        || job.buckets > crate::presentation::MAX_SECTION_BUCKETS
                    {
                        self.destruction.work.capacity_holds += 1;
                        continue;
                    }
                    if self.destruction.detach.len() >= 8 {
                        proof_members += job.component.len() + job.supported.len();
                        self.destruction.searches.push_back(job);
                        waiting += 1;
                        if waiting >= self.destruction.searches.len() {
                            break;
                        }
                        continue;
                    }
                    let ids = std::mem::take(&mut job.component);
                    let reserved: usize = self
                        .destruction
                        .reservations
                        .values()
                        .map(|r| r.members)
                        .sum();
                    let looks: usize = self
                        .destruction
                        .reservations
                        .values()
                        .map(|r| r.looks)
                        .sum();
                    let quads: usize = self
                        .destruction
                        .reservations
                        .values()
                        .map(|r| r.quads)
                        .sum();
                    let buckets: usize = self
                        .destruction
                        .reservations
                        .values()
                        .map(|r| r.buckets)
                        .sum();
                    let draw: usize = self
                        .destruction
                        .reservations
                        .values()
                        .map(|r| r.draw_quads)
                        .sum();
                    if ids.len() > MAX_SECTION_MEMBERS.saturating_sub(reserved)
                        || job.draw_quads
                            > crate::presentation::MAX_SECTION_DRAW_QUADS.saturating_sub(draw)
                        || job.looks.len()
                            > crate::presentation::MAX_SECTION_LOOKS.saturating_sub(looks)
                        || job.quads > crate::presentation::MAX_SECTION_QUADS.saturating_sub(quads)
                        || job.buckets
                            > crate::presentation::MAX_SECTION_BUCKETS.saturating_sub(buckets)
                        || self.destruction.reservations.len()
                            >= crate::presentation::MAX_SECTION_GROUPS
                    {
                        // No destructive commitment without presentation capacity.
                        self.destruction.work.capacity_holds += 1;
                        job.component = ids;
                        proof_members += job.component.len() + job.supported.len();
                        self.destruction.searches.push_back(job);
                        work -= 1;
                        waiting += 1;
                        if waiting >= self.destruction.searches.len() {
                            break;
                        }
                        continue;
                    }
                    let ids = Arc::new(ids);
                    let shape = std::mem::take(&mut job.shape);
                    self.destruction.next_group += 1;
                    self.destruction.reservations.insert(
                        (self.simulation.state().tick, self.destruction.next_group),
                        Reservation {
                            members: ids.len(),
                            looks: job.looks.len(),
                            quads: job.quads,
                            draw_quads: job.draw_quads,
                            buckets: job.buckets,
                            until: u64::MAX,
                        },
                    );
                    self.simulation.begin_section(
                        (self.simulation.state().tick, self.destruction.next_group),
                        ids.clone(),
                        Arc::new(std::mem::take(&mut job.parents)),
                    );
                    self.destruction.detach.push_back(Detached {
                        scope: job.scope.clone(),
                        members: ids.len() as u32,
                        ids,
                        next: None,
                        group: self.destruction.next_group,
                        started: self.simulation.state().tick,
                        origin: (shape.low + shape.high) * 0.5,
                        half: (shape.high - shape.low) * 0.5,
                        pivot: shape.base,
                        area: job.area,
                    });
                    job.supported.clear();
                    job.looks.clear();
                    job.quads = 0;
                    job.draw_quads = 0;
                    job.buckets = 0;
                    self.destruction.work.sections += 1;
                }
                while work > 0 {
                    work -= 1;
                    let Some(id) = job.next_seed() else { break };
                    if self.live_damage_brick(id) && !job.supported.contains(&id) {
                        job.push(id, &self.simulation);
                        break;
                    }
                }
                if job.frontier.is_empty() {
                    continue;
                }
            }
            waiting = 0;
            let (id, mut probe) = match job.probe.take() {
                Some(probe) => probe,
                None => {
                    let Reverse((_, id)) = job.frontier.pop().unwrap();
                    if !self.live_damage_brick(id) {
                        job.component.remove(&id);
                        proof_members += job.component.len() + job.supported.len();
                        self.destruction.searches.push_back(job);
                        continue;
                    }
                    self.destruction.work.support_visits += 1;
                    let p = SupportProbe::new(&self.simulation, id);
                    job.watch(p.area);
                    (id, p)
                }
            };
            if probe.step(&self.simulation, &mut work, |other| {
                self.live_damage_brick(other)
            })? {
                if probe.exhausted {
                    self.destruction.work.capacity_holds += 1;
                    continue;
                }
                let (rooted, _) = self.remember_probe(id, &probe);
                let b = &self.simulation.state().bricks[&id];
                if rooted
                    || self.destruction.detach.iter().any(|d| {
                        d.ids.contains(&id)
                            && !self.simulation.section_interrupted((d.started, d.group))
                    })
                    || job.supported.contains(&id)
                    || self.simulation.definitions.get(b)?.indestructible
                    || !job.scope.allows(b)
                {
                    job.supported.extend(std::mem::take(&mut job.component));
                    job.frontier.clear();
                    job.shape = SectionShape::default();
                    job.looks.clear();
                    job.quads = 0;
                    job.draw_quads = 0;
                    job.expensive_look = false;
                    job.parents.clear();
                    job.buckets = 0;
                } else {
                    for other in probe.neighbors {
                        job.push(other, &self.simulation);
                        if job.component.len() > MAX_SECTION_MEMBERS {
                            break;
                        }
                    }
                }
            } else {
                job.probe = Some((id, probe));
            }
            let retained = job.component.len() + job.supported.len();
            if job.looks.len() > crate::presentation::MAX_SECTION_LOOKS
                || job.quads > crate::presentation::MAX_SECTION_QUADS
                || job.draw_quads > crate::presentation::MAX_SECTION_DRAW_QUADS
                || job.expensive_look
                || job.buckets > crate::presentation::MAX_SECTION_BUCKETS
                || retained > MAX_SECTION_MEMBERS
                || proof_members + retained > MAX_SECTION_MEMBERS * 2
            {
                self.destruction.work.capacity_holds += 1;
            } else {
                proof_members += retained;
                self.destruction.searches.push_back(job);
            }
        }
        let rounds = self.destruction.detach.len();
        if rounds > 1 {
            self.destruction.detach.rotate_left(1);
        }
        let quota = (commits / rounds.max(1)).max(1);
        for _ in 0..rounds {
            let Some(mut detached) = self.destruction.detach.pop_front() else {
                break;
            };
            if !detached.scope.valid(self)
                || self
                    .simulation
                    .section_interrupted((detached.started, detached.group))
            {
                self.finish_section(&detached);
                if detached.scope.valid(self) && detached.remaining().next().is_some() {
                    self.destruction.searches.push_back(Search::from_section(
                        detached.scope,
                        detached.ids,
                        detached.next,
                    ));
                }
                continue;
            }
            let candidates = detached.candidates(
                quota.min(commits),
                &self.simulation,
                &mut self.destruction.mutation_bytes_left,
            );
            commits -= candidates.len();
            let ids: Vec<_> = candidates
                .into_iter()
                .filter(|&id| self.live_damage_brick(id))
                .collect();
            let mut cues = Vec::new();
            for &id in &ids {
                let (mut kind, position) = self.brick_kill_cue(id, None)?;
                if let CueKind::BrickKill { death, origin, .. } = &mut kind {
                    *death = BrickDeath::Section {
                        group: detached.group,
                        started: detached.started,
                        members: detached.members,
                        half: detached.half.to_array(),
                        pivot: detached.pivot.to_array(),
                    };
                    *origin = detached.origin.to_array();
                }
                cues.push((kind, position));
            }
            let previous = self
                .simulation
                .set_section_writer(Some((detached.started, detached.group)));
            let hidden = self.hide_damage_bricks(&detached.scope, &ids);
            self.simulation.set_section_writer(previous);
            hidden?;
            for &id in &ids {
                self.simulation
                    .section_member((detached.started, detached.group), id);
            }
            self.destruction.work.detached_bricks += ids.len() as u64;
            for &id in &ids {
                if self.simulation.damage_parent(id).is_none() {
                    self.fire_input(id, "onBlownUp", Some(detached.scope.source));
                }
            }
            for cue in cues {
                self.emit_brick_kill(cue);
            }
            if detached.remaining().next().is_some() {
                self.destruction.detach.push_back(detached);
            } else {
                self.finish_section(&detached);
            }
        }
        // Discard only the known section removals. Event outputs run later and
        // their changes are observed next tick before more members detach.
        let _ = self.simulation.take_support_edits();
        self.flush_section_journal();
        self.destruction.work.query_work += u64::from(WORK_PER_TICK - work);
        Ok(())
    }
    fn finish_section(&mut self, section: &Detached) {
        self.simulation
            .seal_section((section.started, section.group), section.scope.delay);
        if let Some(reservation) = self
            .destruction
            .reservations
            .get_mut(&(section.started, section.group))
        {
            reservation.until = self
                .simulation
                .state()
                .tick
                .saturating_add(PRESENTATION_LEASE);
        }
    }
}
fn content_namespace(reference: &ContentRef) -> String {
    match reference {
        ContentRef::Resolved(id) => id.split_once([':', '/']).map_or("", |p| p.0).into(),
        _ => String::new(),
    }
}
#[cfg(test)]
mod tests;
