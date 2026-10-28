//! Visible members, shared falling motion, and bounded local chunk physics.
//! A chunk is a transform plus a cheap collision proxy, never replacement art.
use super::*;
use crate::building::SectionProbe;
use crate::local_physics::{GRAVITY, LocalRegion};
use bri_sim::grid::Index;
use std::collections::VecDeque;
mod partition;
mod proxy;
use partition::Partition;

pub(super) const MAX_VISIBLE: usize = 65_536;
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
    pub delay: f32,
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
    region: Option<LocalRegion>,
    generation: u64,
    ready: bool,
    released: bool,
}
impl Chunk {
    fn pose(&self, world: &PhysicsWorld) -> (Vec3, Quat) {
        self.handle.map_or((self.center, self.turn), |h| {
            let rb = &world.bodies[h];
            (
                Vec3::from_array(rb.translation().to_array()),
                Quat::from_array(rb.rotation().to_array()),
            )
        })
    }
    fn freeze(&mut self, world: &mut PhysicsWorld) {
        (self.center, self.turn) = self.pose(world);
        if let Some(h) = self.handle.take() {
            world.remove_body_with_colliders(h, true);
        }
        self.released = true;
        self.region = None;
    }
}
pub(super) struct Section {
    pub pieces: Vec<SectionPiece>,
    pub age: f32,
    expected: usize,
    origin: Vec3,
    half: Vec3,
    pivot: Vec3,
    motion: Motion,
    axis: Vec3,
    shown: f32,
    pub landed: bool,
    pending: VecDeque<(SectionProbe, f32)>,
    target: f32,
    contact: Option<f32>,
    witnesses: [Option<Vec3>; WITNESSES],
    scores: [f32; WITNESSES],
    ids: BTreeSet<BrickId>,
    index: Index,
    remaining: BTreeSet<usize>,
    plan: Option<Partition>,
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
            age: p.delay,
            expected: p.members,
            origin: p.origin,
            half: p.section_half,
            pivot: p.pivot - p.origin,
            motion,
            axis: Vec3::Y.cross(lean).normalize_or(Vec3::X),
            shown: p.delay,
            landed: false,
            pending: VecDeque::new(),
            target: p.delay,
            contact: None,
            witnesses: [None; WITNESSES],
            scores: [f32::NEG_INFINITY; WITNESSES],
            ids: Default::default(),
            index: Default::default(),
            remaining: Default::default(),
            plan: None,
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
        if !self.ids.insert(p.brick) {
            return;
        }
        p.offset -= self.origin;
        let i = self.pieces.len();
        self.index.insert(i as u64, partition::bounds(&p));
        self.remaining.insert(i);
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
        self.pieces.push(p);
        self.assignment.push(None);
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
        if self.landed || self.pieces.is_empty() {
            return Ok(());
        }
        if self.pending.is_empty() {
            if *work < WITNESSES as u32 {
                return Ok(());
            }
            *work -= WITNESSES as u32;
            self.target = self.age;
            self.contact = None;
            let (old, ot) = self.pose(self.shown);
            let (next, nt) = self.pose(self.target);
            for point in self.witnesses.iter().flatten() {
                let from = old + ot * *point + Vec3::Y * 0.02;
                let to = next + nt * *point;
                if from.distance_squared(to) < 1e-6 {
                    continue;
                }
                let reach = from.distance(to);
                self.pending
                    .push_back((building.section_probe(from, to), reach));
            }
        }
        while let Some((p, reach)) = self.pending.front_mut() {
            let Some(hit) = building.step_section_probe(p, work, |id| dead.contains(&id))? else {
                break;
            };
            if let Some(distance) = hit {
                let f = ((distance - 0.02) / reach.max(0.001)).clamped(0., 1.);
                self.contact = Some(self.contact.map_or(f, |old| old.min(f)));
            }
            self.pending.pop_front();
        }
        if self.pending.is_empty() {
            self.shown = self.shown + (self.target - self.shown) * self.contact.unwrap_or(1.);
            if self.contact.is_some() {
                self.landed = true;
            }
        }
        Ok(())
    }
    fn partition(&mut self, quality: usize, work: &mut u32) {
        if self.pieces.len() < self.expected || !self.chunks.is_empty() {
            return;
        }
        if quality == 0 {
            return;
        }
        if self.plan.is_none() {
            let desired = ((self.pieces.len() as f32).sqrt() * (quality as f32 / 2048.).sqrt())
                .ceil() as usize;
            let target = self.pieces.len().div_ceil(desired.clamp(1, MAX_CHUNKS));
            self.plan = Some(Partition::new(
                std::mem::take(&mut self.index),
                std::mem::take(&mut self.remaining),
                target,
            ));
        }
        if !self.plan.as_mut().unwrap().step(work) || !self.landed {
            return;
        }
        let plan = self.plan.take().unwrap();
        let (center, turn) = self.pose(self.shown);
        let (before, bt) = self.pose((self.shown - 0.01).max(0.));
        let velocity = ((center - before) / 0.01).clamp_length_max(12.);
        let spin = (turn * bt.inverse()).to_scaled_axis() / 0.01;
        for members in plan.groups {
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
                region: None,
                generation: 0,
                ready: false,
                released: false,
            });
        }
    }
}
impl BrickDebris {
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
        // Independent render safety ceiling. Never charge member bricks to
        // the rigid-body limit. Normal quality changes preserve every member.
        if !self.sections.contains_key(&key)
            && (self.sections.len() >= 1024
                || self
                    .sections
                    .values()
                    .filter(|s| s.age < SOLID_SECONDS + FADE_SECONDS)
                    .map(|s| s.expected)
                    .sum::<usize>()
                    + pieces[0].members
                    > MAX_VISIBLE)
        {
            self.diagnostics.skipped += pieces.len() as u64;
            return Ok(());
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
        Ok(())
    }
    pub(super) fn advance_sections(&mut self, dt: f32, building: &Building) -> Result<()> {
        let mut contacts = 4096;
        let mut planning = 4096;
        let mut collision = 512;
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
        for s in self.sections.values_mut() {
            s.age += dt;
            if s.age >= SOLID_SECONDS + FADE_SECONDS {
                for c in &mut s.chunks {
                    if c.handle.is_some() {
                        c.freeze(&mut self.world);
                    }
                }
                s.pieces.clear();
                s.chunks.clear();
                s.assignment.clear();
                s.ids.clear();
                s.index = Index::default();
                s.remaining.clear();
                s.plan = None;
                s.landed = true;
                continue;
            }
            s.advance_motion(building, &mut contacts, &self.dead)?;
            s.partition(self.limit, &mut planning);
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
                    && c.generation == building.query_generation()
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
                let margin = Vec3::splat(0.1 + c.half.length() * angular * STEP * MAX_STEPS as f32)
                    + velocity.abs() * STEP * MAX_STEPS as f32;
                let low = center - h - margin;
                let high = center + h + margin;
                if c.generation != building.query_generation()
                    || c.region.as_ref().is_none_or(|r| !r.contains(low, high))
                {
                    c.region = Some(LocalRegion::new(low - Vec3::ONE, high + Vec3::ONE));
                    c.ready = false;
                    c.generation = building.query_generation();
                }
                if !c.ready {
                    c.ready = self.surroundings.load_region(
                        &mut self.world,
                        building,
                        c.region.as_mut().unwrap(),
                        &mut collision,
                        |id| self.dead.contains(&id),
                    )?;
                }
                if let Some(handle) = c.handle {
                    self.world.bodies[handle].set_enabled(c.ready);
                    continue;
                }
                if !c.ready {
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
                available -= 1;
                part_slots -= c.parts;
            }
        }
        self.sections.retain(|_, s| s.age < 30.);
        Ok(())
    }
    pub(super) fn section_instances(&self) -> impl Iterator<Item = (&Look, SceneTransform)> {
        self.sections
            .values()
            .filter(|s| s.age < SOLID_SECONDS + FADE_SECONDS)
            .flat_map(move |s| {
                let (center, turn) = s.pose(s.shown);
                let fade = 1. - ((s.age - SOLID_SECONDS) / FADE_SECONDS).clamped(0., 1.);
                s.pieces.iter().enumerate().map(move |(i, p)| {
                    let (at, rotation) =
                        s.assignment[i].map_or((center + turn * p.offset, turn * p.turn), |a| {
                            let c = &s.chunks[a];
                            let (cc, ct) = c.pose(&self.world);
                            (cc + ct * (p.offset - c.local), ct * p.turn)
                        });
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
    fn cue(
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
        for _ in 0..frames {
            d.advance(1. / 60., b).unwrap();
        }
    }
    #[test]
    fn late_members_join_landed_pose_without_restart_or_loss() -> Result<()> {
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
        assert!(d.sections[&(0, 7)].landed);
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
            delay: 0.,
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
    fn piece(id: u64, at: Vec3) -> SectionPiece {
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
            delay: 0.,
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
        old.witnesses[8..].fill(None);
        for p in pieces {
            s.admit(p);
        }
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
