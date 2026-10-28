//! Visible members, shared falling motion, and bounded local chunk physics.
//! A chunk is a transform plus a cheap collision proxy, never replacement art.
use super::*;
use crate::building::SectionProbe;
use crate::local_physics::{CollisionMotion, CollisionWindow, GRAVITY};
use bri_sim::grid::Index;
use std::collections::VecDeque;
mod partition;
mod proxy;
pub(super) mod shots;
use partition::Partition;

pub(super) const MAX_VISIBLE: usize = bri_sim::presentation::MAX_SECTION_MEMBERS;
const MAX_CHUNKS: usize = 64;
const WITNESSES: usize = 72;
const MAX_PROXY_PARTS: usize = 2048;

#[derive(Clone)]
pub(super) struct SectionPiece {
    pub brick: BrickId,
    pub look: Look,
    pub offset: Vec3,
    pub turn: Quat,
    pub half: Vec3,
    pub origin: Vec3,
    pub tick: u64,
    pub active: bool,
    pub members: usize,
    pub section_half: Vec3,
    pub pivot: Vec3,
}
#[derive(Clone, Copy, PartialEq, Debug)]
enum Motion {
    Topple,
    Span,
    Drop,
}
struct Chunk {
    local: Vec3,
    half: Vec3,
    proxy: Option<SharedShape>,
    parts: usize,
    center: Vec3,
    turn: Quat,
    velocity: Vec3,
    spin: Vec3,
    handle: Option<RigidBodyHandle>,
    collision: CollisionWindow,
    recoil: Vec3,
    released: bool,
}
impl Chunk {
    fn pose(&self, world: &PhysicsWorld) -> (Vec3, Quat) {
        let (center, turn) = self.handle.map_or((self.center, self.turn), |h| {
            let rb = &world.bodies[h];
            (
                Vec3::from_array(rb.translation().to_array()),
                Quat::from_array(rb.rotation().to_array()),
            )
        });
        (center + self.recoil, turn)
    }
    fn freeze(&mut self, world: &mut PhysicsWorld) {
        (self.center, self.turn) = self.pose(world);
        if let Some(h) = self.handle.take() {
            world.remove_body_with_colliders(h, true);
        }
        self.released = true;
        self.recoil = Vec3::ZERO;
        self.collision.reset();
    }
}
pub(super) struct Section {
    pub pieces: Vec<SectionPiece>,
    pub age: f32,
    pub(super) sealed: bool,
    pub(super) barrier_tick: Option<u64>,
    recovery_after: Option<BrickId>,
    pub(super) gpu_ready: bool,
    staging_age: f32,
    prepared: usize,
    source_generation: u64,
    recoil: Vec3,
    pub(super) expected: usize,
    origin: Vec3,
    half: Vec3,
    pivot: Vec3,
    motion: Motion,
    axis: Vec3,
    shown: f32,
    confirmed: f32,
    impact_at: Option<f32>,
    probe_generation: u64,
    pub landed: bool,
    shared_only: bool,
    pending: VecDeque<(SectionProbe, f32, Vec3, Vec3)>,
    contact_point: Option<Vec3>,
    snap_told: bool,
    landing_told: bool,
    target: f32,
    contact: Option<f32>,
    witnesses: [Option<Vec3>; WITNESSES],
    scores: [f32; WITNESSES],
    ids: BTreeSet<BrickId>,
    index: std::sync::Arc<Index>,
    remaining: BTreeSet<usize>,
    plan: Option<Partition>,
    finishing: Option<VecDeque<Vec<usize>>>,
    chunks: Vec<Chunk>,
    assignment: Vec<Option<usize>>,
}
impl Section {
    fn new(p: &SectionPiece, group: u64) -> Self {
        let width = p.section_half.x.max(p.section_half.z);
        let motion = if p.section_half.y > width * 1.6 {
            Motion::Topple
        } else if width > p.section_half.x.min(p.section_half.z).max(p.section_half.y) * 2. {
            Motion::Span
        } else {
            Motion::Drop
        };
        let mut rng = Seeded::new(group);
        let lean = (p.origin - p.pivot) * Vec3::new(1., 0., 1.);
        let lean = if motion == Motion::Span && lean.length_squared() < 0.01 {
            if p.section_half.x > p.section_half.z {
                Vec3::X
            } else {
                Vec3::Z
            }
        } else {
            lean.normalize_or(Vec3::new(rng.signed(), 0., rng.signed()).normalize_or(Vec3::X))
        };
        Self {
            pieces: Vec::new(),
            age: 0.,
            sealed: false,
            barrier_tick: None,
            recovery_after: None,
            gpu_ready: true,
            staging_age: 0.,
            prepared: 0,
            source_generation: u64::MAX,
            recoil: Vec3::ZERO,
            expected: p.members,
            origin: p.origin,
            half: p.section_half,
            pivot: p.pivot - p.origin,
            motion,
            axis: Vec3::Y.cross(lean).normalize_or(Vec3::X),
            shown: 0.,
            confirmed: 0.,
            impact_at: None,
            probe_generation: 0,
            landed: false,
            shared_only: false,
            pending: VecDeque::new(),
            contact_point: None,
            snap_told: false,
            landing_told: false,
            target: 0.,
            contact: None,
            witnesses: [None; WITNESSES],
            scores: [f32::NEG_INFINITY; WITNESSES],
            ids: Default::default(),
            index: Default::default(),
            remaining: Default::default(),
            plan: None,
            finishing: None,
            chunks: Vec::new(),
            assignment: Vec::new(),
        }
    }
    fn pose(&self, age: f32) -> (Vec3, Quat) {
        if self.motion == Motion::Drop {
            return (
                self.origin - Vec3::Y * (0.5 * GRAVITY * age * age),
                Quat::from_axis_angle(self.axis, 0.22 * age),
            );
        }
        let length = if self.motion == Motion::Topple {
            self.half.y * 2.
        } else {
            self.half.max_element() * 2.
        };
        let angle = (0.12 * age + GRAVITY / (2. * length.max(0.5)) * age * age).min(1.65);
        let turn = Quat::from_axis_angle(self.axis, angle);
        // The hinge only sells the beginning: a severed assembly then falls free.
        let release = (2. * length / GRAVITY).sqrt();
        let drop = 0.5 * GRAVITY * (age - release).max(0.).powi(2);
        (
            self.origin + self.pivot - turn * self.pivot - Vec3::Y * drop,
            turn,
        )
    }
    fn admit(&mut self, mut p: SectionPiece) {
        if self.sealed {
            return;
        }
        if !self.ids.insert(p.brick) {
            return;
        }
        p.offset -= self.origin;
        self.record_member(&p);
        self.pieces.push(p);
        self.assignment.push(None);
    }
    fn prepare_members(&mut self, work: &mut usize) {
        while self.prepared < self.pieces.len() && *work > 0 {
            *work -= 1;
            let i = self.prepared;
            let p = self.pieces[i].clone();
            std::sync::Arc::get_mut(&mut self.index)
                .expect("unsealed section index")
                .insert(i as u64, partition::bounds(&p));
            self.remaining.insert(i);
            self.prepared += 1;
        }
    }
    // Admission is already spread over delivered members. Prepare the small
    // contact witness set there; the landing index can catch up during flight.
    fn record_member(&mut self, p: &SectionPiece) {
        // Original corners plus distributed real material samples. Outer
        // sail corners can miss a narrow deck while its middle clips through.
        for x in [-1., 1.] {
            for y in [-1., 1.] {
                for z in [-1., 1.] {
                    self.record_point(p.offset + p.turn * (p.half * Vec3::new(x, y, z)));
                }
            }
        }
        self.record_point(p.offset + p.turn * (Vec3::NEG_Y * p.half.y));
        let axes = self.sample_axes();
        let h = (p.turn * Vec3::X).abs() * p.half.x
            + (p.turn * Vec3::Y).abs() * p.half.y
            + (p.turn * Vec3::Z).abs() * p.half.z;
        if h[axes[0]] > self.half[axes[0]] / 8. || h[axes[1]] > self.half[axes[1]] / 4. {
            for x in 0..8 {
                for y in 0..4 {
                    for z in 0..2 {
                        let target = self.grid_point([x, y, z]) * self.half;
                        let mut point = target.clamp(p.offset - h, p.offset + h);
                        point.y = p.offset.y - h.y;
                        self.record_point(point);
                    }
                }
            }
        }
    }
    fn record_point(&mut self, point: Vec3) {
        if self.motion == Motion::Topple && point.y - self.pivot.y < self.half.y * 0.25 {
            return;
        }
        let q = point / self.half.max(Vec3::splat(0.1));
        for w in 0..8 {
            let direction = Vec3::new(
                if w & 1 == 0 { -1. } else { 1. },
                if w & 2 == 0 { -1. } else { 1. },
                if w & 4 == 0 { -1. } else { 1. },
            );
            let score = q.dot(direction);
            if score > self.scores[w] {
                self.scores[w] = score;
                self.witnesses[w] = Some(point);
            }
        }
        let axes = self.sample_axes();
        let dims = [8, 4, 2];
        let cells: [usize; 3] = std::array::from_fn(|i| {
            (((q[axes[i]] + 1.) * 0.5 * dims[i] as f32).floor() as usize).min(dims[i] - 1)
        });
        let slot = 8 + cells[0] + 8 * cells[1] + 32 * cells[2];
        let target = self.grid_point(cells);
        let score = -64. * q.y - q.distance_squared(target);
        if score > self.scores[slot] {
            self.scores[slot] = score;
            self.witnesses[slot] = Some(point);
        }
    }
    fn recover(&mut self, key: (u64, u64), building: &Building, work: &mut usize) {
        if self.sealed {
            return;
        }
        let Some(tick) = self.barrier_tick.filter(|tick| building.world_at(*tick)) else {
            return;
        };
        for (brick, b) in building.section_bricks([key.0, key.1], self.recovery_after, *work) {
            *work -= 1;
            self.recovery_after = Some(brick);
            if self.ids.contains(&brick) {
                continue;
            }
            let ContentRef::Resolved(definition) = &b.definition else {
                continue;
            };
            let Some(half) = building.definition_half_extents(definition) else {
                continue;
            };
            self.admit(SectionPiece {
                brick,
                look: Look {
                    definition: definition.clone(),
                    color: b.color,
                    color_effect: b.color_effect,
                    shape_effect: b.shape_effect,
                    print: b.print.clone(),
                    shard: None,
                },
                offset: Vec3::from(b.position),
                turn: Quat::from_rotation_y(-f32::from(b.quarter_turns) * FRAC_PI_2),
                half,
                origin: self.origin,
                tick,
                active: true,
                members: self.expected,
                section_half: self.half,
                pivot: self.origin + self.pivot,
            });
        }
        if building
            .section_bricks([key.0, key.1], self.recovery_after, 1)
            .is_empty()
        {
            // Newer world state can already have restored some missing sources.
            self.expected = self.pieces.len();
            self.sealed = true;
        }
    }
    fn sample_axes(&self) -> [usize; 3] {
        if self.motion == Motion::Drop {
            [0, 2, 1]
        } else if self.half.x >= self.half.z {
            [0, 1, 2]
        } else {
            [2, 1, 0]
        }
    }
    fn grid_point(&self, cells: [usize; 3]) -> Vec3 {
        let axes = self.sample_axes();
        let dims = [8, 4, 2];
        let mut target = Vec3::ZERO;
        for i in 0..3 {
            target[axes[i]] = (cells[i] as f32 + 0.5) / dims[i] as f32 * 2. - 1.;
        }
        target
    }
    fn advance_motion(
        &mut self,
        building: &Building,
        work: &mut u32,
        dead: &BTreeSet<BrickId>,
    ) -> Result<()> {
        if !self.sealed || !self.gpu_ready || self.landed || self.pieces.is_empty() {
            return Ok(());
        }
        let (at, _) = self.pose(self.shown);
        let extent = Vec3::splat(self.half.length());
        if building.query_changed_since(self.probe_generation, at - extent, at + extent) {
            self.pending.clear();
            self.confirmed = self.shown;
            self.age = self.shown;
            self.impact_at = None;
        }
        self.probe_generation = building.query_generation();
        if self.pending.is_empty() && self.impact_at.is_none() && self.confirmed < self.age + 0.1 {
            if *work < WITNESSES as u32 {
                return Ok(());
            }
            *work -= WITNESSES as u32;
            self.target = self.age + 0.2;
            self.contact = None;
            self.contact_point = None;
            let (old, ot) = self.pose(self.confirmed);
            let (next, nt) = self.pose(self.target);
            for point in self.witnesses.iter().flatten() {
                let from = old + ot * *point + Vec3::Y * 0.02;
                let to = next + nt * *point;
                if from.distance_squared(to) < 1e-6 {
                    continue;
                }
                let reach = from.distance(to);
                self.pending.push_back((
                    building.section_probe(from, to),
                    reach,
                    from,
                    (to - from) / reach,
                ));
            }
        }
        while let Some((p, reach, from, direction)) = self.pending.front_mut() {
            let Some(hit) = building.step_section_probe(p, work, |id| dead.contains(&id))? else {
                break;
            };
            if let Some(distance) = hit {
                let f = ((distance - 0.02) / reach.max(0.001)).clamped(0., 1.);
                if self.contact.is_none_or(|old| f < old) {
                    self.contact = Some(f);
                    self.contact_point = Some(*from + *direction * distance);
                }
            }
            self.pending.pop_front();
        }
        if self.pending.is_empty() && self.target > self.confirmed {
            if let Some(f) = self.contact {
                self.impact_at = Some(self.confirmed + (self.target - self.confirmed) * f);
            }
            self.confirmed = self.target;
        }
        self.shown = self.age.min(self.impact_at.unwrap_or(self.confirmed));
        self.landed = self.impact_at.is_some_and(|at| self.age >= at);
        Ok(())
    }
    fn keep_shared(&mut self) {
        debug_assert!(self.chunks.iter().all(|c| c.handle.is_none()));
        self.shared_only = true;
        self.plan = None;
        self.finishing = None;
        self.chunks.clear();
        self.assignment.fill(None);
    }
    fn partition(&mut self, quality: usize, slots: usize, work: &mut u32) {
        if !self.sealed
            || self.shared_only
            || self.prepared != self.pieces.len()
            || self.pieces.len() < self.expected
            || (self.finishing.is_none() && !self.chunks.is_empty())
        {
            return;
        }
        if self.plan.is_none() && self.finishing.is_none() {
            if quality == 0 || slots == 0 {
                self.keep_shared();
                return;
            }
            let desired = ((self.pieces.len() as f32).sqrt() * (quality as f32 / 2048.).sqrt())
                .ceil() as usize;
            let target = self
                .pieces
                .len()
                .div_ceil(desired.clamp(1, MAX_CHUNKS).min(slots))
                .min(2048);
            self.plan = Some(Partition::new(
                self.index.clone(),
                std::mem::take(&mut self.remaining),
                target,
            ));
        }
        if let Some(plan) = &mut self.plan {
            if !plan.step(work) {
                return;
            }
            if plan.over_limit {
                self.keep_shared();
                return;
            }
            if !self.landed {
                return;
            }
            self.finishing = Some(self.plan.take().unwrap().groups.into());
        }
        let (center, turn) = self.pose(self.shown);
        let (before, bt) = self.pose((self.shown - 0.01).max(0.));
        let velocity = ((center - before) / 0.01).clamp_length_max(12.);
        let spin = (turn * bt.inverse()).to_scaled_axis() / 0.01;
        while self
            .finishing
            .as_ref()
            .is_some_and(|groups| !groups.is_empty())
        {
            let groups = self.finishing.as_mut().unwrap();
            if groups.front().unwrap().len() > *work as usize {
                break;
            }
            let members = groups.pop_front().unwrap();
            *work -= members.len() as u32;
            let mut low = Vec3::splat(f32::INFINITY);
            let mut high = Vec3::splat(f32::NEG_INFINITY);
            for &i in &members {
                let p = &self.pieces[i];
                let h = (p.turn * Vec3::X).abs() * p.half.x
                    + (p.turn * Vec3::Y).abs() * p.half.y
                    + (p.turn * Vec3::Z).abs() * p.half.z;
                low = low.min(p.offset - h);
                high = high.max(p.offset + h);
                self.assignment[i] = Some(self.chunks.len());
            }
            let local = (low + high) * 0.5;
            let (proxy, parts) = proxy::shape(&self.pieces, &members, self.origin + local);
            if proxy.is_none() {
                self.keep_shared();
                return;
            }
            self.chunks.push(Chunk {
                local,
                half: (high - low) * 0.5,
                proxy,
                parts,
                center: center + turn * local,
                turn,
                velocity: (velocity + spin.cross(turn * local)).clamp_length_max(12.),
                spin: spin.clamp_length_max(3.),
                handle: None,
                collision: Default::default(),
                recoil: Vec3::ZERO,
                released: false,
            });
        }
        if self
            .finishing
            .as_ref()
            .is_some_and(|groups| groups.is_empty())
        {
            self.finishing = None;
        }
    }
}
impl BrickDebris {
    pub(super) fn retire_section(&mut self, key: (u64, u64)) {
        if let Some(mut section) = self.sections.remove(&key) {
            for chunk in &mut section.chunks {
                chunk.freeze(&mut self.world);
            }
        }
    }
    pub(super) fn reset_section_collision(&mut self) {
        for section in self.sections.values_mut() {
            for chunk in &mut section.chunks {
                chunk.collision.reset();
            }
        }
    }
    pub(super) fn section_piece_count(&self) -> usize {
        self.sections.values().map(|s| s.pieces.len()).sum()
    }
    pub(super) fn chunk_handles(&self) -> impl Iterator<Item = RigidBodyHandle> + '_ {
        self.sections
            .values()
            .flat_map(|s| s.chunks.iter().filter_map(|c| c.handle))
    }
    pub(super) fn freeze_chunks(&mut self, keep: usize) -> u64 {
        let mut left = keep;
        let mut removed = 0;
        for s in self.sections.values_mut() {
            for c in &mut s.chunks {
                if c.handle.is_some() {
                    if left > 0 {
                        left -= 1;
                    } else {
                        c.freeze(&mut self.world);
                        removed += 1;
                    }
                }
            }
        }
        removed
    }
    pub(super) fn add_section(
        &mut self,
        key: (u64, u64),
        pieces: Vec<SectionPiece>,
        _building: &Building,
    ) -> Result<()> {
        if pieces.is_empty() {
            return Ok(());
        }
        if self.retired_sections.contains(&key) {
            return Ok(());
        }
        // Independent render safety ceiling. Never charge member bricks to
        // the rigid-body limit. Normal quality changes preserve every member.
        if !self.sections.contains_key(&key)
            && (self.sections.len() >= bri_sim::presentation::MAX_SECTION_GROUPS
                || self
                    .sections
                    .values()
                    .filter(|s| s.age < SOLID_SECONDS + FADE_SECONDS)
                    .map(|s| s.expected)
                    .sum::<usize>()
                    + pieces[0].members
                    > MAX_VISIBLE)
        {
            // Server admission normally prevents this. A delayed client retires
            // old presentation before discarding newly removed material.
            while self.sections.values().map(|s| s.expected).sum::<usize>() + pieces[0].members
                > MAX_VISIBLE
                || self.sections.len() >= bri_sim::presentation::MAX_SECTION_GROUPS
            {
                let Some(oldest) = self
                    .sections
                    .iter()
                    .max_by(|a, b| a.1.staging_age.total_cmp(&b.1.staging_age))
                    .map(|(k, _)| *k)
                else {
                    break;
                };
                if let Some(mut old) = self.sections.remove(&oldest) {
                    for c in &mut old.chunks {
                        c.freeze(&mut self.world);
                    }
                }
            }
        }
        let s = self
            .sections
            .entry(key)
            .or_insert_with(|| Section::new(&pieces[0], key.1));
        if s.age >= SOLID_SECONDS + FADE_SECONDS {
            return Ok(());
        }
        for p in pieces {
            s.admit(p);
        }
        if let Some(&(members, tick)) = self.section_barriers.get(&key) {
            s.expected = members;
            s.barrier_tick = Some(tick);
            s.sealed = s.pieces.len() == members;
        }
        Ok(())
    }
    pub(super) fn advance_sections(&mut self, dt: f32, building: &Building) -> Result<()> {
        let mut contacts = 4096;
        let mut planning = 4096;
        let mut collision = 512 * crate::local_physics::COLLISION_QUERY_COST;
        let mut material = 256;
        let mut recovery = 256;
        let mut available = self
            .limit
            .saturating_sub(self.bodies.len() + self.chunk_handles().count());
        available = available.min(MAX_CHUNKS.saturating_sub(self.chunk_handles().count()));
        let used_parts = self.bodies.len()
            + self
                .sections
                .values()
                .flat_map(|s| &s.chunks)
                .filter(|c| c.handle.is_some())
                .map(|c| c.parts)
                .sum::<usize>();
        let mut part_slots = MAX_PROXY_PARTS.saturating_sub(used_parts);
        let mut keys: Vec<_> = self.sections.keys().copied().collect();
        if !keys.is_empty() {
            let offset = self.section_turn % keys.len();
            keys.rotate_left(offset);
            self.section_turn = self.section_turn.wrapping_add(1);
        }
        for key in keys {
            let s = self.sections.get_mut(&key).unwrap();
            s.recover(key, building, &mut recovery);
            s.prepare_members(&mut material);
            s.recoil *= (-6. * dt).exp();
            for c in &mut s.chunks {
                c.recoil *= (-6. * dt).exp();
            }
            if s.source_generation != building.damage_generation() {
                for (i, piece) in s.pieces.iter_mut().enumerate() {
                    if piece.active
                        && building.section_source_retired_after(
                            piece.brick,
                            piece.tick,
                            [key.0, key.1],
                        )
                    {
                        piece.active = false;
                        if let Some(chunk) = s.assignment[i] {
                            s.chunks[chunk].freeze(&mut self.world);
                        }
                    }
                }
                s.source_generation = building.damage_generation();
            }
            s.staging_age += dt;
            if s.sealed && s.gpu_ready && (s.landed || s.confirmed >= s.age + dt) {
                s.age += dt;
            }
            if s.age >= SOLID_SECONDS + FADE_SECONDS {
                for c in &mut s.chunks {
                    if c.handle.is_some() {
                        c.freeze(&mut self.world);
                    }
                }
                s.pieces.clear();
                s.prepared = 0;
                s.chunks.clear();
                s.assignment.clear();
                s.ids.clear();
                s.index = Default::default();
                s.remaining.clear();
                s.plan = None;
                s.finishing = None;
                s.landed = true;
                continue;
            }
            s.advance_motion(building, &mut contacts, &self.dead)?;
            if s.shown > 0. && !s.snap_told && s.pieces.iter().any(|p| p.active) {
                s.snap_told = true;
                self.foley.report(super::foley::Hit {
                    source: super::foley::Source::Section(key.0, key.1),
                    kind: super::foley::Kind::Snap,
                    point: s.origin + s.pivot,
                    speed: self
                        .foley
                        .policy
                        .as_ref()
                        .map_or(0., |p| p.full_speed * 0.7),
                });
            }
            if s.landed && !s.landing_told && s.pieces.iter().any(|p| p.active) {
                s.landing_told = true;
                if let Some(point) = s.contact_point {
                    let (center, turn) = s.pose(s.shown);
                    let (before, bt) = s.pose((s.shown - 0.02).max(0.));
                    let local = turn.inverse() * (point - center);
                    self.foley.report(super::foley::Hit {
                        source: super::foley::Source::Section(key.0, key.1),
                        kind: if self.foley.policy.as_ref().is_some_and(|p| {
                            s.pieces.iter().filter(|m| m.active).count() >= p.landing_min_members
                        }) {
                            super::foley::Kind::Landing
                        } else {
                            super::foley::Kind::Impact
                        },
                        point,
                        speed: (point - (before + bt * local)).length() / 0.02,
                    });
                }
            }
            s.partition(self.limit, available, &mut planning);
            if s.finishing.is_some() {
                continue;
            }
            if !s.chunks.is_empty()
                && s.chunks.iter().all(|c| c.handle.is_none())
                && (s.chunks.len() > available
                    || s.chunks.iter().map(|c| c.parts).sum::<usize>() > part_slots)
            {
                s.keep_shared();
            }
            for c in &mut s.chunks {
                if c.released {
                    continue;
                }
                if c.handle.is_none()
                    && (available == 0 || c.parts > part_slots || c.proxy.is_none())
                {
                    continue;
                }
                // Rapier already makes sleeping bodies cheap. Keep their hit
                // targets so projectiles can wake the visible wreckage.
                if c.handle.is_some_and(|h| self.world.bodies[h].is_sleeping())
                    && c.collision.unchanged(building)
                {
                    continue;
                }
                let (center, turn) = c.pose(&self.world);
                let velocity = c.handle.map_or(c.velocity, |h| {
                    Vec3::from_array(self.world.bodies[h].linvel().to_array())
                });
                let angular = c
                    .handle
                    .map_or(c.spin.length(), |h| self.world.bodies[h].angvel().length());
                let h = (turn * Vec3::X).abs() * c.half.x
                    + (turn * Vec3::Y).abs() * c.half.y
                    + (turn * Vec3::Z).abs() * c.half.z;
                let ready = c.collision.prepare(
                    &mut self.surroundings,
                    &mut self.world,
                    building,
                    CollisionMotion {
                        center,
                        half: h,
                        velocity,
                        angular,
                    },
                    &mut collision,
                    |id| self.dead.contains(&id),
                )?;
                if let Some(handle) = c.handle {
                    self.world.bodies[handle].set_enabled(ready);
                    continue;
                }
                if !ready {
                    continue;
                }
                let body = RigidBodyBuilder::dynamic()
                    .pose(Pose::from_parts(
                        Vector::from_array(center.to_array()),
                        Rotation::from_array(turn.to_array()),
                    ))
                    .linvel(Vector::from_array(c.velocity.to_array()))
                    .angvel(Vector::from_array(c.spin.to_array()))
                    .linear_damping(0.15)
                    .angular_damping(0.5)
                    .ccd_enabled(true);
                // Exact unions of original brick boxes preserve empty sail
                // and beam space; a whole-group box invents solid air and can
                // launch the group out of a live deck on first contact.
                let collider = ColliderBuilder::new(c.proxy.as_ref().unwrap().clone())
                    .density(DENSITY)
                    .friction(PLASTIC_FRICTION)
                    .restitution(0.25)
                    .collision_groups(InteractionGroups::new(
                        Group::GROUP_2,
                        Group::GROUP_1 | Group::GROUP_2,
                        InteractionTestMode::And,
                    ));
                let (handle, _) = self.world.insert(body, collider);
                c.handle = Some(handle);
                super::foley::configure(&mut self.world, handle, self.foley.policy.as_deref());
                self.foley.born(handle, 0.12);
                available -= 1;
                part_slots -= c.parts;
            }
        }
        self.sections
            .retain(|_, s| s.age < 30. && (s.sealed || s.staging_age < 30.));
        self.section_barriers
            .retain(|key, _| self.sections.contains_key(key));
        Ok(())
    }
    pub(super) fn section_instances(&self) -> impl Iterator<Item = (&Look, SceneTransform)> {
        self.sections
            .values()
            .filter(|s| s.age < SOLID_SECONDS + FADE_SECONDS)
            .flat_map(move |s| {
                let (center, turn) = s.pose(s.shown);
                let fade = 1. - ((s.age - SOLID_SECONDS) / FADE_SECONDS).clamped(0., 1.);
                s.pieces
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| p.active)
                    .map(move |(i, p)| {
                        let (at, rotation) = s.assignment[i].map_or(
                            (center + turn * p.offset + s.recoil, turn * p.turn),
                            |a| {
                                let c = &s.chunks[a];
                                let (cc, ct) = c.pose(&self.world);
                                (cc + ct * (p.offset - c.local), ct * p.turn)
                            },
                        );
                        (
                            &p.look,
                            SceneTransform {
                                transform: Mat4::from_rotation_translation(rotation, at),
                                tint: [1., 1., 1., fade],
                            },
                        )
                    })
            })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn cue(
        id: u64,
        at: [f32; 3],
        count: u32,
        half: [f32; 3],
        origin: [f32; 3],
        pivot: [f32; 3],
    ) -> Cue {
        let mut c = super::super::tests::kill(id, id, at, origin, 0., 0.);
        if let CueKind::BrickKill { death, .. } = &mut c.kind {
            *death = BrickDeath::Section {
                group: 7,
                started: 0,
                members: count,
                half,
                pivot,
            };
        }
        c
    }
    fn run(d: &mut BrickDebris, b: &Building, frames: usize) {
        for section in d.sections.values_mut() {
            if section.pieces.len() == section.expected {
                section.sealed = true;
            }
        }
        for _ in 0..frames {
            d.advance(1. / 60., b).unwrap();
        }
    }
    #[test]
    fn incomplete_delivery_stays_staged_until_the_complete_assembly_can_fall() -> Result<()> {
        let (b, _) = super::super::tests::building(&[]);
        let mut d = BrickDebris::new();
        d.set_limit(2);
        d.cues(
            &[cue(
                10,
                [0., 1.3, 0.],
                3,
                [1.5, 0.3, 0.5],
                [1., 1.3, 0.],
                [1., 1., 0.],
            )],
            &b,
        )?;
        run(&mut d, &b, 60);
        let before = d.instances().next().unwrap().1.transform;
        assert!(!d.sections[&(0, 7)].landed);
        assert!(
            d.chunk_handles().next().is_none(),
            "wait for the complete assembly before breakup"
        );
        d.cues(
            &[
                cue(
                    11,
                    [1., 1.3, 0.],
                    3,
                    [1.5, 0.3, 0.5],
                    [1., 1.3, 0.],
                    [1., 1., 0.],
                ),
                cue(
                    12,
                    [2., 1.3, 0.],
                    3,
                    [1.5, 0.3, 0.5],
                    [1., 1.3, 0.],
                    [1., 1., 0.],
                ),
            ],
            &b,
        )?;
        assert_eq!(d.instances().count(), 3);
        assert_eq!(before, d.instances().next().unwrap().1.transform);
        run(&mut d, &b, 30);
        assert_eq!(d.instances().count(), 3);
        assert!(d.len() <= 2);
        d.set_limit(0);
        assert_eq!(
            d.instances().count(),
            3,
            "quality never discards visible section material"
        );
        run(&mut d, &b, 800);
        assert_eq!(d.instances().count(), 0);
        assert_eq!(d.world.bodies.len(), 0);
        Ok(())
    }
    #[test]
    fn a_large_connected_mast_keeps_all_members_with_only_a_few_bodies() -> Result<()> {
        let (b, _) = super::super::tests::building(&[]);
        let mut d = BrickDebris::new();
        d.set_limit(8);
        let cues: Vec<_> = (0..1500)
            .map(|i| {
                cue(
                    i + 1,
                    [(i % 20) as f32, 0.3 + (i / 20) as f32 * 0.6, 0.],
                    1500,
                    [10., 22.5, 0.5],
                    [9.5, 22.5, 0.],
                    [9.5, 0., 0.],
                )
            })
            .collect();
        d.cues(&cues, &b)?;
        let mut maximum = 0;
        d.sections.get_mut(&(0, 7)).unwrap().sealed = true;
        for _ in 0..300 {
            d.advance(1. / 60., &b)?;
            assert_eq!(d.instances().count(), 1500);
            maximum = maximum.max(d.len());
            assert!(d.len() <= 8);
        }
        let s = &d.sections[&(0, 7)];
        assert!(s.landed && !s.chunks.is_empty());
        assert!(maximum > 0 && maximum <= 8);
        assert_eq!(s.assignment.iter().filter(|a| a.is_some()).count(), 1500);
        assert!(
            s.chunks.len() < 20,
            "large connected groups, not one body per member"
        );
        // Every group can be reached over member faces; no disconnected art is glued together.
        for group in 0..s.chunks.len() {
            let ids: BTreeSet<_> = s
                .assignment
                .iter()
                .enumerate()
                .filter_map(|(i, a)| (*a == Some(group)).then_some(i))
                .collect();
            let mut reached = BTreeSet::from([*ids.first().unwrap()]);
            loop {
                let old = reached.len();
                for &i in &ids {
                    if reached.iter().any(|&j| {
                        bri_sim::grid::share_face(
                            partition::bounds(&s.pieces[i]),
                            partition::bounds(&s.pieces[j]),
                        )
                    }) {
                        reached.insert(i);
                    }
                }
                if old == reached.len() {
                    break;
                }
            }
            assert_eq!(reached.len(), ids.len());
        }
        Ok(())
    }

    #[test]
    fn large_collapses_begin_moving_before_landing_partition_work_finishes() -> Result<()> {
        let (building, _) = super::super::tests::building(&[]);
        let mut late = Vec::new();
        for (count, width) in [(40, 1), (1500, 20), (12000, 40)] {
            let height = count / width;
            let origin = [(width - 1) as f32 * 0.5, height as f32 * 0.3, 0.];
            let cues: Vec<_> = (0..count)
                .map(|i| {
                    cue(
                        i + 1,
                        [(i % width) as f32, 0.3 + (i / width) as f32 * 0.6, 0.],
                        count as u32,
                        [width as f32 * 0.5, height as f32 * 0.3, 0.5],
                        origin,
                        [origin[0], 0., 0.],
                    )
                })
                .collect();
            let initial_tip = Vec3::from(cues.last().unwrap().position);
            let mut d = BrickDebris::new();
            d.set_limit(8);
            d.cues(&cues, &building)?;
            d.cues(
                &[Cue {
                    id: count + 1,
                    tick: 1,
                    position: origin,
                    kind: CueKind::BrickSectionReady {
                        group: 7,
                        started: 0,
                        members: count as u32,
                    },
                }],
                &building,
            )?;
            let mut first = None;
            let mut tip_at_100ms = 0.;
            for frame in 1..=60 {
                d.advance(1. / 60., &building)?;
                assert_eq!(d.instances().count(), count as usize);
                let tip = d.instances().last().unwrap().1.transform.w_axis.truncate();
                let moved = tip.distance(initial_tip);
                if first.is_none() && moved > 0.01 {
                    first = Some(frame * 1000 / 60);
                }
                if frame == 6 {
                    tip_at_100ms = moved;
                }
            }
            eprintln!(
                "collapse {count} members: first tip motion {first:?}ms; tip at100ms {tip_at_100ms:.3} units"
            );
            if first.is_none_or(|ms| ms > 100) {
                late.push((count, first));
            }
        }
        assert!(
            late.is_empty(),
            "sealed visible assemblies waited for partition preparation: {late:?}"
        );
        Ok(())
    }
    #[test]
    fn tall_mast_leans_before_landing_and_floor_drops() -> Result<()> {
        let (b, _) = super::super::tests::building(&[]);
        let mut d = BrickDebris::new();
        let cues: Vec<_> = (0..40)
            .map(|i| {
                cue(
                    i + 1,
                    [0., 0.3 + i as f32 * 0.6, 0.],
                    40,
                    [0.5, 12., 0.5],
                    [0., 12., 0.],
                    [0., 0., 0.],
                )
            })
            .collect();
        d.cues(&cues, &b)?;
        run(&mut d, &b, 40);
        let s = &d.sections[&(0, 7)];
        assert_eq!(s.motion, Motion::Topple);
        assert!(
            !s.landed,
            "a broken base touching the deck must not end the fall immediately"
        );
        let (_, turn) = s.pose(s.shown);
        assert!(turn.to_scaled_axis().length() > 0.2);
        let mut p = SectionPiece {
            brick: 1,
            look: d.sections[&(0, 7)].pieces[0].look.clone(),
            offset: Vec3::ZERO,
            turn: Quat::IDENTITY,
            half: Vec3::ONE,
            origin: Vec3::new(0., 10., 0.),
            tick: 0,
            active: true,
            members: 1,
            section_half: Vec3::new(10., 0.3, 10.),
            pivot: Vec3::new(0., 9.7, 0.),
        };
        let floor = Section::new(&p, 1);
        assert_eq!(floor.motion, Motion::Drop);
        assert!(floor.pose(0.5).0.y < 8.);
        p.section_half = Vec3::new(12., 0.3, 0.5);
        assert_eq!(Section::new(&p, 1).motion, Motion::Span);
        Ok(())
    }
}

#[cfg(test)]
mod landing_regressions {
    use super::*;
    pub(super) fn piece(id: u64, at: Vec3) -> SectionPiece {
        SectionPiece {
            brick: id,
            look: Look {
                definition: "brick".into(),
                color: 1,
                color_effect: 0,
                shape_effect: 0,
                print: None,
                shard: None,
            },
            offset: at,
            turn: Quat::IDENTITY,
            half: Vec3::new(0.5, 0.3, 0.5),
            origin: Vec3::ZERO,
            tick: 0,
            active: true,
            members: 1,
            section_half: Vec3::ONE,
            pivot: Vec3::ZERO,
        }
    }
    #[test]
    fn collision_union_keeps_the_opening_between_mast_members() {
        let positions = [
            (-1., 0.3),
            (-1., 0.9),
            (-1., 1.5),
            (0., 1.5),
            (1., 1.5),
            (1., 0.9),
            (1., 0.3),
        ];
        let pieces: Vec<_> = positions
            .into_iter()
            .enumerate()
            .map(|(i, (x, y))| piece(i as u64, Vec3::new(x, y, 0.)))
            .collect();
        let ids: Vec<_> = (0..pieces.len()).collect();
        let (shape, parts) = proxy::shape(&pieces, &ids, Vec3::ZERO);
        let shape = shape.unwrap();
        assert!(
            parts <= 3,
            "straight runs merge without filling the opening"
        );
        assert!(
            !shape.contains_point(&Pose::IDENTITY, Vector::new(0., 0.3, 0.)),
            "empty space inside a group is not solid debris"
        );
        assert!(shape.contains_point(&Pose::IDENTITY, Vector::new(-1., 0.3, 0.)));
    }
    #[test]
    fn sleeping_visible_chunk_keeps_a_projectile_target_and_wakes_on_hit() -> Result<()> {
        let (b, _) = super::super::tests::building(&[]);
        let mut d = BrickDebris::new();
        let p = piece(1, Vec3::new(0., 2., 0.));
        d.add_section((0, 1), vec![p], &b)?;
        d.sections.get_mut(&(0, 1)).unwrap().sealed = true;
        for _ in 0..180 {
            d.advance(1. / 60., &b)?;
        }
        let handle = d.sections[&(0, 1)].chunks[0]
            .handle
            .expect("settled art retains a hit target");
        d.world.bodies[handle].sleep();
        d.advance(1. / 60., &b)?;
        assert_eq!(d.sections[&(0, 1)].chunks[0].handle, Some(handle));
        let center = Vec3::from_array(d.world.bodies[handle].translation().to_array());
        for sign in [-1., 1.] {
            d.shots(&[Shot {
                id: 99,
                position: center + Vec3::Z * 2. * sign,
                velocity: Vec3::Z * 40.,
            }]);
        }
        assert_eq!(d.diagnostics.projectile_hits, 1);
        assert!(!d.world.bodies[handle].is_sleeping());
        assert!(
            d.world.bodies[handle].linvel().length() >= 1.49,
            "a group gets a visible hit reaction"
        );
        assert_eq!(d.instances().count(), 1);
        Ok(())
    }
    #[test]
    fn broad_sail_hits_a_narrow_deck_before_its_middle_passes_through() -> Result<()> {
        let deck: Vec<_> = (5..=14)
            .flat_map(|x| {
                (-2..=2).map(move |z| (1000 + (x * 5 + z + 2) as u64, [x as f32, 6.7, z as f32]))
            })
            .collect();
        let (b, _) = super::super::tests::building(&deck);
        let mut pieces: Vec<_> = (0..34)
            .map(|i| piece(i + 1, Vec3::new(0., 4.3 + i as f32 * 0.6, 0.)))
            .collect();
        for x in -4..=4 {
            for y in 7..=22 {
                if x != 0 {
                    pieces.push(piece(
                        pieces.len() as u64 + 1,
                        Vec3::new(0., 4.3 + y as f32 * 0.6, x as f32),
                    ));
                }
            }
        }
        for p in &mut pieces {
            p.origin = Vec3::new(0., 14.2, 0.);
            p.pivot = Vec3::new(0., 4., 0.);
            p.section_half = Vec3::new(0.5, 10.2, 4.5);
        }
        let mut s = Section::new(&pieces[0], 1);
        s.expected = pieces.len();
        s.axis = Vec3::NEG_Z;
        let mut old = Section::new(&pieces[0], 1);
        old.expected = pieces.len();
        old.axis = Vec3::NEG_Z;
        for p in pieces.iter().cloned() {
            old.admit(p);
        }
        let mut prepare_work = usize::MAX;
        old.prepare_members(&mut prepare_work);
        old.sealed = true;
        old.witnesses[8..].fill(None);
        for p in pieces {
            s.admit(p);
        }
        s.prepare_members(&mut prepare_work);
        s.sealed = true;
        for i in 0..180 {
            old.age = (i + 1) as f32 / 60.;
            old.advance_motion(&b, &mut 4096, &BTreeSet::new())?;
            if old.landed {
                break;
            }
        }
        for i in 0..180 {
            s.age = (i + 1) as f32 / 60.;
            s.advance_motion(&b, &mut 4096, &BTreeSet::new())?;
            if s.landed {
                break;
            }
        }
        assert!(s.landed);
        fn lowest(s: &Section) -> f32 {
            let (center, turn) = s.pose(s.shown);
            let mut lowest = 7.0_f32;
            for p in &s.pieces {
                for x in [-1., 1.] {
                    for y in [-1., 1.] {
                        for z in [-1., 0., 1.] {
                            let local =
                                p.offset + Vec3::new(x * p.half.x, y * p.half.y, z * p.half.z);
                            if local.y - s.pivot.y < s.half.y * 0.25 {
                                continue;
                            }
                            let at = center + turn * local;
                            if (4.5..=14.5).contains(&at.x) && at.z.abs() <= 2.5 {
                                lowest = lowest.min(at.y);
                            }
                        }
                    }
                }
            }
            lowest
        }
        assert!(
            lowest(&old) < 6.75,
            "negative control: outer-corner probes must miss the raised narrow deck"
        );
        assert!(
            lowest(&s) >= 6.75,
            "sail penetrated deck to y={}; shared fall stopped too late at {}",
            lowest(&s),
            s.shown
        );
        Ok(())
    }
}

#[cfg(test)]
mod lifecycle_regressions {
    use super::*;
    fn ready(members: u32) -> Cue {
        Cue {
            id: 10001,
            tick: 10,
            position: [0.; 3],
            kind: CueKind::BrickSectionReady {
                started: 0,
                group: 7,
                members,
            },
        }
    }
    #[test]
    fn explicit_barrier_starts_motion_and_retirement_prevents_late_resurrection() -> Result<()> {
        let (b, _) = super::super::tests::building(&[]);
        let mut d = BrickDebris::new();
        let first = super::tests::cue(
            1,
            [0., 3., 0.],
            2,
            [0.5, 1., 0.5],
            [0., 3., 0.],
            [0., 2., 0.],
        );
        d.cues(&[first], &b)?;
        for _ in 0..90 {
            d.advance(1. / 60., &b)?;
        }
        assert_eq!(
            d.sections[&(0, 7)].shown,
            0.,
            "membership count alone is not completion"
        );
        d.cues(&[ready(1)], &b)?; // Producer sealed its committed prefix.
        for _ in 0..60 {
            d.advance(1. / 60., &b)?;
        }
        assert!(d.sections[&(0, 7)].shown > 0.);
        d.cues(
            &[Cue {
                id: 10002,
                tick: 10,
                position: [0.; 3],
                kind: CueKind::BrickSectionRetired {
                    started: 0,
                    group: 7,
                },
            }],
            &b,
        )?;
        assert_eq!(d.instances().count(), 0);
        let late = super::tests::cue(
            2,
            [1., 3., 0.],
            2,
            [0.5, 1., 0.5],
            [0., 3., 0.],
            [0., 2., 0.],
        );
        d.cues(&[late, ready(2)], &b)?;
        assert_eq!(d.instances().count(), 0);
        assert_eq!(d.world.bodies.len(), 0);
        Ok(())
    }
    #[test]
    fn material_preparation_is_bounded_without_culling_received_members() -> Result<()> {
        let (b, _) = super::super::tests::building(&[]);
        let mut d = BrickDebris::new();
        d.set_limit(0);
        let mut cues: Vec<_> = (0..1000)
            .map(|i| {
                super::tests::cue(
                    i + 1,
                    [0., 0.3 + i as f32 * 0.6, 0.],
                    1000,
                    [0.5, 300., 0.5],
                    [0., 300., 0.],
                    [0.; 3],
                )
            })
            .collect();
        cues.push(ready(1000));
        d.cues(&cues, &b)?;
        assert_eq!(d.instances().count(), 1000);
        d.advance(1. / 60., &b)?;
        assert_eq!(d.sections[&(0, 7)].prepared, 256);
        assert_eq!(d.sections[&(0, 7)].shown, 0.);
        assert_eq!(d.instances().count(), 1000);
        Ok(())
    }
    #[test]
    fn visible_assembly_without_physics_body_still_reacts_to_projectile() -> Result<()> {
        let (b, _) = super::super::tests::building(&[]);
        let mut d = BrickDebris::new();
        d.set_limit(0);
        d.add_section(
            (0, 1),
            vec![super::landing_regressions::piece(1, Vec3::new(0., 2., 0.))],
            &b,
        )?;
        d.sections.get_mut(&(0, 1)).unwrap().sealed = true;
        for _ in 0..100 {
            d.advance(1. / 60., &b)?;
        }
        assert_eq!(d.len(), 0);
        let before = d.instances().next().unwrap().1.transform.w_axis.truncate();
        for z in [-2., 2.] {
            d.shots(&[Shot {
                id: 99,
                position: before + Vec3::Z * z,
                velocity: Vec3::Z * 40.,
            }]);
        }
        assert_eq!(d.diagnostics.projectile_hits, 1);
        let after = d.instances().next().unwrap().1.transform.w_axis.truncate();
        assert!(after.distance(before) > 0.2);
        assert_eq!(
            d.len(),
            0,
            "a hit reaction must not buy an extra physics body"
        );
        Ok(())
    }
    #[test]
    fn distant_geometry_change_does_not_pause_an_active_chunk() -> Result<()> {
        let (mut b, _) = super::super::tests::building(&[]);
        let mut d = BrickDebris::new();
        d.add_section(
            (0, 1),
            vec![super::landing_regressions::piece(1, Vec3::new(0., 2., 0.))],
            &b,
        )?;
        d.sections.get_mut(&(0, 1)).unwrap().sealed = true;
        for _ in 0..180 {
            d.advance(1. / 60., &b)?;
        }
        let h = d.sections[&(0, 1)].chunks[0].handle.unwrap();
        d.world.bodies[h].set_linvel(Vector::new(1., 0., 0.), true);
        let before = d.world.bodies[h].translation().x;
        let generation = b.query_generation();
        let mut world = bri_net::protocol::PublicWorld {
            name: "test".into(),
            map_id: "test".into(),
            palette: vec![[1.; 4]; 2],
            bricks: Default::default(),
        };
        world.bricks.insert(
            999,
            bri_world::Brick::new(ContentRef::Resolved("brick".into()), [1000., 0.3, 1000.], 0),
        );
        b.sync_world(&world)?;
        assert!(!b.query_changed_since(generation, Vec3::splat(-5.), Vec3::splat(5.)));
        d.advance(1. / 60., &b)?;
        assert!(d.world.bodies[h].is_enabled());
        assert!(d.world.bodies[h].translation().x > before);
        Ok(())
    }
    #[test]
    fn render_model_construction_has_a_separate_per_frame_budget() -> Result<()> {
        let mut models = DebrisModels::default();
        let looks: Vec<_> = (0..10)
            .map(|i| Look {
                definition: format!("brick{i}"),
                color: 0,
                color_effect: 0,
                shape_effect: 0,
                print: None,
                shard: None,
            })
            .collect();
        let transform = SceneTransform {
            transform: Mat4::IDENTITY,
            tint: [1.; 4],
        };
        for expected in [4, 8, 10] {
            models.frame_models(looks.iter().map(|look| (look, transform)), 1, 4, |_| {
                Ok(None)
            })?;
            assert_eq!(models.models.len(), expected);
        }
        Ok(())
    }
}

#[cfg(test)]
mod recovery_regressions {
    use super::*;
    #[test]
    fn removed_fragment_retires_before_the_rest_of_its_section() -> Result<()> {
        let (mut b, _) = super::super::tests::building(&[]);
        let mut world = bri_net::protocol::PublicWorld {
            name: "test".into(),
            map_id: "test".into(),
            palette: vec![[1.; 4]; 2],
            bricks: Default::default(),
        };
        for (id, x) in [(1, 0.), (2, 1.)] {
            let mut brick =
                bri_world::Brick::new(ContentRef::Resolved("brick".into()), [x, 3.3, 0.], 0);
            brick.visible = false;
            brick.raycast = false;
            brick.colliding = false;
            brick.damage_hidden = true;
            brick.damage_section = Some(Box::new([0, 7]));
            world.bricks.insert(id, brick);
        }
        b.sync_world(&world)?;
        b.set_world_tick(10);
        let mut d = BrickDebris::new();
        d.set_limit(0);
        let cues: Vec<_> = [(1, 0.), (2, 1.)]
            .into_iter()
            .map(|(id, x)| {
                super::tests::cue(
                    id,
                    [x, 3.3, 0.],
                    2,
                    [1., 0.3, 0.5],
                    [0.5, 3.3, 0.],
                    [0.5, 3., 0.],
                )
            })
            .collect();
        d.cues(&cues, &b)?;
        d.advance(1. / 60., &b)?;
        assert_eq!(d.instances().count(), 2);
        // Family restoration deletes this transient fragment while another
        // original still owns its member of the same streamed section.
        world.bricks.remove(&1);
        world.bricks.insert(
            3,
            bri_world::Brick::new(ContentRef::Resolved("brick".into()), [0., 3.3, 0.], 0),
        );
        b.sync_world(&world)?;
        b.set_world_tick(11);
        d.advance(1. / 60., &b)?;
        assert_eq!(d.instances().count(), 1);
        assert!(!d.sections[&(0, 7)].pieces[0].active);
        assert!(d.sections[&(0, 7)].pieces[1].active);
        Ok(())
    }

    #[test]
    fn authoritative_section_tags_recover_missing_cues_without_inventing_material() -> Result<()> {
        let (mut b, _) = super::super::tests::building(&[]);
        let mut world = bri_net::protocol::PublicWorld {
            name: "test".into(),
            map_id: "test".into(),
            palette: vec![[1.; 4]; 2],
            bricks: Default::default(),
        };
        for (id, x) in [(1, 0.), (2, 1.)] {
            let mut brick =
                bri_world::Brick::new(ContentRef::Resolved("brick".into()), [x, 3.3, 0.], 0);
            brick.visible = false;
            brick.raycast = false;
            brick.colliding = false;
            brick.damage_hidden = true;
            brick.damage_section = Some(Box::new([0, 7]));
            world.bricks.insert(id, brick);
        }
        b.sync_world(&world)?;
        b.set_world_tick(10);
        let mut d = BrickDebris::new();
        let first = super::tests::cue(
            1,
            [0., 3.3, 0.],
            2,
            [1., 0.3, 0.5],
            [0.5, 3.3, 0.],
            [0.5, 3., 0.],
        );
        let end = Cue {
            id: 3,
            tick: 10,
            position: [0.; 3],
            kind: CueKind::BrickSectionReady {
                started: 0,
                group: 7,
                members: 2,
            },
        };
        d.cues(&[first, end], &b)?;
        assert_eq!(d.instances().count(), 1);
        d.advance(1. / 60., &b)?;
        assert_eq!(d.instances().count(), 2);
        assert_eq!(d.sections[&(0, 7)].ids, BTreeSet::from([1, 2]));
        assert!(d.sections[&(0, 7)].sealed);
        assert_eq!(
            d.sections[&(0, 7)]
                .pieces
                .iter()
                .map(|p| p.half.x * p.half.y * p.half.z * 8.)
                .sum::<f32>(),
            1.2
        );
        Ok(())
    }
}

#[cfg(test)]
mod overload_regressions {
    use super::*;
    #[test]
    fn too_many_disconnected_landing_groups_keep_one_visible_shared_assembly() -> Result<()> {
        let (b, _) = super::super::tests::building(&[]);
        let mut d = BrickDebris::new();
        let mut pieces: Vec<_> = (0..100)
            .map(|i| super::landing_regressions::piece(i + 1, Vec3::new(i as f32 * 3., 3.3, 0.)))
            .collect();
        for p in &mut pieces {
            p.members = 100;
            p.origin = Vec3::new(148.5, 3.3, 0.);
            p.section_half = Vec3::new(150., 0.3, 0.5);
            p.pivot = Vec3::new(148.5, 3., 0.);
        }
        d.add_section((0, 1), pieces, &b)?;
        d.sections.get_mut(&(0, 1)).unwrap().sealed = true;
        for _ in 0..30 {
            d.advance(1. / 60., &b)?;
        }
        let section = &d.sections[&(0, 1)];
        assert!(section.shared_only);
        assert!(section.chunks.is_empty());
        assert_eq!(d.instances().count(), 100);
        assert_eq!(d.len(), 0);
        Ok(())
    }
}

#[cfg(test)]
mod audio_regressions {
    use super::*;
    #[test]
    fn staged_material_is_quiet_then_shared_motion_snaps_and_lands_once() -> Result<()> {
        let pack = bri_weapons::Pack::from_json(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/brick-destruction-audio/assets/weapons.json"
        )))?;
        let (b, _) = super::super::tests::building(&[]);
        let mut d = BrickDebris::new();
        d.set_limit(0);
        d.set_audio_policy(pack.effects.brick_debris_audio.as_ref());
        d.set_audio_listener(Vec3::new(8., 1., 2.));
        let mut cues: Vec<_> = (0..8)
            .map(|i| {
                super::tests::cue(
                    i + 1,
                    [4.5 + i as f32, 3.3, 0.],
                    8,
                    [4., 0.3, 0.5],
                    [8., 3.3, 0.],
                    [8., 3., 0.],
                )
            })
            .collect();
        d.cues(&cues, &b)?;
        for _ in 0..10 {
            d.advance(1. / 60., &b)?;
            assert!(d.take_audio().is_empty());
        }
        cues.clear();
        cues.push(Cue {
            id: 9,
            tick: 0,
            position: [0.; 3],
            kind: CueKind::BrickSectionReady {
                group: 7,
                started: 0,
                members: 8,
            },
        });
        d.cues(&cues, &b)?;
        let mut snaps = 0;
        let mut landings = 0;
        for _ in 0..180 {
            d.advance(1. / 60., &b)?;
            for sound in d.take_audio() {
                if sound.profile.ends_with(":snap") {
                    snaps += 1;
                }
                if sound.profile.ends_with(":landing") {
                    landings += 1;
                    assert!(sound.point[1].abs() < 0.1, "{:?}", sound.point);
                }
            }
        }
        assert_eq!((snaps, landings), (1, 1));
        assert_eq!(
            d.world.bodies.len(),
            0,
            "shared motion needs no audio-only physics body"
        );
        d.clear();
        assert!(d.take_audio().is_empty());
        assert!(
            d.foley.policy.is_some(),
            "session clear retains loaded package policy"
        );
        Ok(())
    }
}
