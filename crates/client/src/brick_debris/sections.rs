//! Visible members, shared falling motion, and bounded local chunk physics.
//! A chunk is a transform plus a cheap collision proxy, never replacement art.
use super::*;
use crate::building::SectionProbe;
use crate::local_physics::{CollisionMotion, CollisionWindow, GRAVITY};
use bri_sim::grid::Index;
use std::collections::VecDeque;
#[cfg(test)]
mod interaction_tests;
mod motion;
mod partition;
mod proxy;
mod release;
#[cfg(test)]
mod scheduling_tests;
pub(super) mod shots;
use partition::Partition;

pub(super) const MAX_VISIBLE: usize = bri_sim::presentation::MAX_SECTION_MEMBERS;
const WITNESSES: usize = 72;
const MAX_PROXY_PARTS: usize = 2048;
const MAX_CHUNK_MEMBERS: usize = 16;
const MAX_CHUNK_SPAN: f32 = 6.;
// Collision work follows fixed simulation steps, keeping the same per-second
// allowance when a frame must advance several local physics steps.
const COLLISION_QUERIES_PER_STEP: u32 = 256;
const CONTACT_SKIN: f32 = 0.02;
const RESTITUTION: f32 = 0.25;

/// Read-only motion evidence for offscreen repros and diagnostics. Collecting
/// this does not advance a clock, prepare collision or alter admission.
#[derive(serde::Serialize)]
pub struct SectionSnapshot {
    pub impulse: Option<bri_sim::presentation::RadialImpulse>,
    pub release_center: [f32; 3],
    pub release_velocity: [f32; 3],
    pub release_spin: [f32; 3],
    pub impact_velocity: Option<[f32; 3]>,
    pub impact_spin: Option<[f32; 3]>,
    pub key: [u64; 2],
    pub members: usize,
    pub expected: usize,
    pub sealed: bool,
    pub gpu_ready: bool,
    pub staging_age: f32,
    pub age: f32,
    pub shown: f32,
    pub confirmed: f32,
    pub impact_at: Option<f32>,
    pub mode: &'static str,
    pub pending_contacts: usize,
    pub prepared_members: usize,
    pub planning: bool,
    pub pending_groups: usize,
    pub center: [f32; 3],
    pub rotation: [f32; 4],
    pub chunks: Vec<ChunkSnapshot>,
}

#[derive(serde::Serialize)]
pub struct ChunkSnapshot {
    pub center: [f32; 3],
    pub rotation: [f32; 4],
    pub velocity: [f32; 3],
    pub resident: bool,
    pub released: bool,
    pub debt: f32,
}

#[derive(Clone)]
pub(super) struct SectionPiece {
    pub impulse: Option<bri_sim::presentation::RadialImpulse>,
    pub brick: BrickId,
    pub cue: u64,
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
struct Chunk {
    local: Vec3,
    mass_center: Vec3,
    half: Vec3,
    proxy: Option<SharedShape>,
    parts: usize,
    center: Vec3,
    turn: Quat,
    velocity: Vec3,
    spin: Vec3,
    handle: Option<RigidBodyHandle>,
    collision: CollisionWindow,
    released: bool,
    debt: f32,
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
        (center, turn)
    }
    fn freeze(&mut self, world: &mut PhysicsWorld) {
        (self.center, self.turn) = self.pose(world);
        if let Some(h) = self.handle.take() {
            let rb = &world.bodies[h];
            self.velocity = Vec3::from_array(rb.linvel().to_array());
            self.spin = Vec3::from_array(rb.angvel().to_array());
            world.remove_body_with_colliders(h, true);
        }
        self.collision.reset();
    }
}
pub(super) struct Section {
    impulse: Option<bri_sim::presentation::RadialImpulse>,
    mass: release::Mass,
    release: Option<release::Motion>,
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
    shown: f32,
    confirmed: f32,
    impact_at: Option<f32>,
    probe_generation: u64,
    pub landed: bool,
    chunk_turn: usize,
    preparation_turn: usize,
    pending: VecDeque<(SectionProbe, f32, Vec3, Vec3)>,
    contact_point: Option<Vec3>,
    contact_normal: Vec3,
    impact_motion: Option<release::Motion>,
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
    fn member_pose(
        &self,
        i: usize,
        center: Vec3,
        turn: Quat,
        world: &PhysicsWorld,
    ) -> (Vec3, Quat) {
        let p = &self.pieces[i];
        self.assignment[i]
            .filter(|_| self.finishing.is_none())
            .map_or(
                (center + turn * p.offset + self.recoil, turn * p.turn),
                |a| {
                    let c = &self.chunks[a];
                    let (cc, ct) = c.pose(world);
                    (cc + ct * (p.offset - c.local), ct * p.turn)
                },
            )
    }
    pub(super) fn append_attachment_frames(
        &self,
        world: &PhysicsWorld,
        wanted: &impl Fn(BrickId) -> bool,
        keep: &mut impl FnMut(BrickId, AttachmentFrame),
    ) {
        let (center, turn) = self.pose(self.shown);
        for (i, p) in self
            .pieces
            .iter()
            .enumerate()
            .filter(|(_, p)| p.active && wanted(p.brick))
        {
            let (at, rotation) = self.member_pose(i, center, turn, world);
            keep(
                p.brick,
                AttachmentFrame {
                    cue: p.cue,
                    delta: Mat4::from_rotation_translation(rotation, at)
                        * Mat4::from_rotation_translation(p.turn, self.origin + p.offset).inverse(),
                },
            );
        }
    }
    fn new(p: &SectionPiece) -> Self {
        Self {
            impulse: p.impulse,
            mass: release::Mass::default(),
            release: None,
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
            shown: 0.,
            confirmed: 0.,
            impact_at: None,
            probe_generation: 0,
            landed: false,
            chunk_turn: 0,
            preparation_turn: 0,
            pending: VecDeque::new(),
            contact_point: None,
            contact_normal: Vec3::ZERO,
            impact_motion: None,
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
        let motion = self.release.unwrap_or_else(|| self.mass.motion());
        motion.pose(self.origin, age)
    }
    fn admit(&mut self, mut p: SectionPiece) {
        if self.ids.contains(&p.brick) {
            if let Some(old) = self.pieces.iter_mut().find(|old| old.brick == p.brick) {
                old.cue = old.cue.max(p.cue);
            }
            return;
        }
        if self.sealed {
            return;
        }
        self.ids.insert(p.brick);
        p.offset -= self.origin;
        if p.active {
            self.mass.add(&p, self.impulse, 1.);
        }
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
                impulse: self.impulse,
                brick,
                // Recovered from replicated section membership, not a death
                // cue. A later matching cue supplies its real sequence.
                cue: 0,
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
        // Spend the witness grid on the broadest dimensions, independent of
        // any assumed motion or content category.
        let mut axes = [0, 1, 2];
        axes.sort_by(|&a, &b| self.half[b].total_cmp(&self.half[a]));
        axes
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
        let motion = *self.release.get_or_insert_with(|| self.mass.motion());
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
            // Limit each rotating witness chord by the existing contact skin.
            // Fast small fragments must not sweep across a wide arc as a line.
            let radius = self.half.length() + motion.center.length();
            let angle = (2.
                * (1. - f64::from(CONTACT_SKIN) / f64::from(radius.max(CONTACT_SKIN))).acos())
                as f32;
            let window = if motion.spin.length() > 0. {
                (angle / motion.spin.length()).min(0.2)
            } else {
                0.2
            };
            self.target = (self.age + 0.2).min(self.confirmed + window);
            self.contact = None;
            self.contact_point = None;
            self.contact_normal = Vec3::ZERO;
            let (old, ot) = self.pose(self.confirmed);
            let (next, nt) = self.pose(self.target);
            for point in self.witnesses.iter().flatten() {
                let from = old + ot * *point + Vec3::Y * CONTACT_SKIN;
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
            if let Some((distance, normal)) = hit {
                let f = ((distance - CONTACT_SKIN) / reach.max(0.001)).clamped(0., 1.);
                let point = *from + *direction * distance;
                let age = self.confirmed + (self.target - self.confirmed) * f;
                let (center, turn) = motion.pose(self.origin, age);
                let com = center + turn * motion.center;
                let closer_on_same_face = self.contact.is_some_and(|old| {
                    (f - old).abs() * *reach <= CONTACT_SKIN
                        && normal == self.contact_normal
                        && self.contact_point.is_some_and(|old| {
                            point.distance_squared(com) < old.distance_squared(com)
                        })
                });
                if self.contact.is_none_or(|old| {
                    f < old && ((old - f) * *reach > CONTACT_SKIN || normal != self.contact_normal)
                }) || closer_on_same_face
                {
                    self.contact_point = Some(point);
                    self.contact_normal = normal;
                }
                self.contact = Some(self.contact.map_or(f, |old| old.min(f)));
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
        if self.landed
            && self.impact_motion.is_none()
            && let Some(point) = self.contact_point
        {
            let (center, turn) = self.pose(self.shown);
            let mut incoming = motion;
            incoming.velocity -= Vec3::Y * GRAVITY * self.shown;
            self.impact_motion =
                Some(
                    self.mass
                        .impact(incoming, turn, point - center, self.contact_normal),
                );
        }
        Ok(())
    }
    fn partition(&mut self, work: &mut u32) {
        if !self.sealed
            || self.prepared != self.pieces.len()
            || self.pieces.len() < self.expected
            || (self.finishing.is_none() && !self.chunks.is_empty())
        {
            return;
        }
        if self.plan.is_none() && self.finishing.is_none() {
            self.plan = Some(Partition::new(
                self.index.clone(),
                std::mem::take(&mut self.remaining),
                MAX_CHUNK_MEMBERS,
            ));
        }
        if let Some(plan) = &mut self.plan {
            if !plan.step(work) {
                return;
            }
            // Collision-aware clumps participate in flight, not only after a
            // static witness has stopped the complete assembly. Otherwise a
            // player can pass through falling material before first landing.
            self.finishing = Some(self.plan.take().unwrap().groups.into());
        }
        let (center, turn) = self.pose(self.shown);
        let motion = self.impact_motion.unwrap_or_else(|| {
            let mut motion = self.release.unwrap_or_else(|| self.mass.motion());
            motion.velocity -= Vec3::Y * GRAVITY * self.shown;
            motion
        });
        let velocity = motion.velocity;
        while self
            .finishing
            .as_ref()
            .is_some_and(|groups| !groups.is_empty())
        {
            let groups = self.finishing.as_mut().unwrap();
            if groups.front().unwrap().len() > *work as usize {
                break;
            }
            let mut members = groups.pop_front().unwrap();
            *work -= members.len() as u32;
            // Sources may retire while delivery/planning is pending. Charge
            // the planned work, but never recreate their invisible collision.
            members.retain(|&i| self.pieces[i].active);
            if members.is_empty() {
                continue;
            }
            let mut low = Vec3::splat(f32::INFINITY);
            let mut high = Vec3::splat(f32::NEG_INFINITY);
            let mut weight = 0.;
            let mut first = glam::DVec3::ZERO;
            for &i in &members {
                let p = &self.pieces[i];
                let mass = f64::from(p.half.x) * f64::from(p.half.y) * f64::from(p.half.z);
                weight += mass;
                first += p.offset.as_dvec3() * mass;
                let h = (p.turn * Vec3::X).abs() * p.half.x
                    + (p.turn * Vec3::Y).abs() * p.half.y
                    + (p.turn * Vec3::Z).abs() * p.half.z;
                low = low.min(p.offset - h);
                high = high.max(p.offset + h);
                self.assignment[i] = Some(self.chunks.len());
            }
            let local = (low + high) * 0.5;
            let mass_center = (first / weight).as_vec3();
            let (proxy, parts) = proxy::shape(&self.pieces, &members, self.origin + local);
            debug_assert!(proxy.is_some(), "bounded groups fit exact proxies");
            self.chunks.push(Chunk {
                local,
                mass_center,
                half: (high - low) * 0.5,
                proxy,
                parts,
                center: center + turn * local + self.recoil,
                turn,
                velocity: (velocity + motion.spin.cross(turn * (mass_center - motion.center)))
                    .clamp_length_max(MAX_SPEED),
                spin: motion.spin,
                handle: None,
                collision: Default::default(),
                released: false,
                debt: 0.,
            });
        }
        if self
            .finishing
            .as_ref()
            .is_some_and(|groups| groups.is_empty())
        {
            // Publish all prepared groups together at the current shared pose.
            // Incremental proxy construction must not leave earlier groups at
            // an older pose or start their clocks while later groups are built.
            for c in &mut self.chunks {
                c.center = center + turn * c.local + self.recoil;
                c.turn = turn;
                // Each exact proxy determines its own contact and angular
                // response from here; no whole-section impact is required.
                c.velocity = (velocity + motion.spin.cross(turn * (c.mass_center - motion.center)))
                    .clamp_length_max(MAX_SPEED);
                c.spin = motion.spin;
            }
            self.finishing = None;
        }
    }
}
impl BrickDebris {
    pub fn section_snapshots(&self) -> impl Iterator<Item = SectionSnapshot> + '_ {
        self.sections.iter().map(|(key, s)| {
            let (center, turn) = s.pose(s.shown);
            SectionSnapshot {
                impulse: s.impulse,
                release_center: s
                    .release
                    .unwrap_or_else(|| s.mass.motion())
                    .center
                    .to_array(),
                release_velocity: s
                    .release
                    .unwrap_or_else(|| s.mass.motion())
                    .velocity
                    .to_array(),
                release_spin: s.release.unwrap_or_else(|| s.mass.motion()).spin.to_array(),
                impact_velocity: s.impact_motion.map(|m| m.velocity.to_array()),
                impact_spin: s.impact_motion.map(|m| m.spin.to_array()),
                key: [key.0, key.1],
                members: s.pieces.len(),
                expected: s.expected,
                sealed: s.sealed,
                gpu_ready: s.gpu_ready,
                staging_age: s.staging_age,
                age: s.age,
                shown: s.shown,
                confirmed: s.confirmed,
                impact_at: s.impact_at,
                mode: "freefall",
                pending_contacts: s.pending.len(),
                prepared_members: s.prepared,
                planning: s.plan.is_some(),
                pending_groups: s.finishing.as_ref().map_or(0, VecDeque::len),
                center: center.to_array(),
                rotation: turn.to_array(),
                chunks: s
                    .chunks
                    .iter()
                    .map(|c| {
                        let (center, turn) = c.pose(&self.world);
                        ChunkSnapshot {
                            center: center.to_array(),
                            rotation: turn.to_array(),
                            velocity: c.handle.map_or(c.velocity.to_array(), |h| {
                                self.world.bodies[h].linvel().to_array()
                            }),
                            resident: c.handle.is_some(),
                            released: c.released,
                            debt: c.debt,
                        }
                    })
                    .collect(),
            }
        })
    }
    pub(super) fn blast_fallback(
        &mut self,
        origin: Vec3,
        force: f32,
        radius: f32,
        normal: Option<Vec3>,
    ) {
        for c in self
            .sections
            .values_mut()
            .flat_map(|s| &mut s.chunks)
            .filter(|c| c.handle.is_none() && !c.released)
        {
            let Some(shape) = &c.proxy else {
                continue;
            };
            let pose = Pose::from_parts(
                Vector::from_array(c.center.to_array()),
                Rotation::from_array(c.turn.to_array()),
            );
            let nearest = shape
                .project_point(&pose, Vector::from_array(origin.to_array()), true)
                .point;
            let offset = Vec3::from_array(nearest.to_array()) - origin;
            if offset.length() > radius {
                continue;
            }
            let direction = (offset.normalize_or(Vec3::Y) + normal.unwrap_or(Vec3::ZERO) * 2.)
                .normalize_or(Vec3::Y);
            let speed =
                (force * FORCE_TO_SPEED * (1. - offset.length() / radius).clamped(0.25, 1.))
                    .min(MAX_SPEED);
            let mut kick = direction * speed;
            if normal.is_some() {
                kick.y = kick.y.max((speed * 0.45).min(8.));
            }
            c.velocity = (c.velocity + kick).clamp_length_max(12.);
        }
    }
    pub(super) fn fallback_bounds(
        &self,
    ) -> impl Iterator<Item = rapier3d::parry::bounding_volume::Aabb> + '_ {
        self.sections
            .values()
            .filter(|s| s.finishing.is_none())
            .flat_map(|s| &s.chunks)
            .filter(|c| c.handle.is_none() && !c.released)
            .filter_map(|c| {
                c.proxy.as_ref().map(|shape| {
                    shape.compute_aabb(&Pose::from_parts(
                        Vector::from_array(c.center.to_array()),
                        Rotation::from_array(c.turn.to_array()),
                    ))
                })
            })
    }
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
            .or_insert_with(|| Section::new(&pieces[0]));
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
        let steps = (((self.accumulator + dt) / STEP) as u32).clamp(1, MAX_STEPS);
        let mut collision =
            COLLISION_QUERIES_PER_STEP * steps * crate::local_physics::COLLISION_QUERY_COST;
        let mut material = 256;
        let mut recovery = 256;
        let mut keys: Vec<_> = self.sections.keys().copied().collect();
        if !keys.is_empty() {
            let offset = self.section_turn % keys.len();
            keys.rotate_left(offset);
            self.section_turn = self.section_turn.wrapping_add(1);
        }
        for &key in &keys {
            let s = self.sections.get_mut(&key).unwrap();
            s.recover(key, building, &mut recovery);
            s.prepare_members(&mut material);
            s.recoil *= (-6. * dt).exp();
            if s.source_generation != building.damage_generation() {
                let mut changed = BTreeMap::<usize, Vec<usize>>::new();
                for (i, piece) in s.pieces.iter_mut().enumerate() {
                    if piece.active
                        && building.section_source_retired_after(
                            piece.brick,
                            piece.tick,
                            [key.0, key.1],
                        )
                    {
                        if s.release.is_none() {
                            s.mass.add(piece, s.impulse, -1.);
                        }
                        piece.active = false;
                        if let Some(chunk) = s.assignment[i]
                            && let std::collections::btree_map::Entry::Vacant(entry) =
                                changed.entry(chunk)
                        {
                            s.chunks[chunk].freeze(&mut self.world);
                            entry.insert(Vec::new());
                        }
                    }
                }
                s.source_generation = building.damage_generation();
                // Retired members must leave both physical representations.
                for (i, _) in s.pieces.iter().enumerate().filter(|(_, p)| p.active) {
                    if let Some(members) = s.assignment[i].and_then(|chunk| changed.get_mut(&chunk))
                    {
                        members.push(i);
                    }
                }
                for (chunk, members) in changed {
                    let c = &mut s.chunks[chunk];
                    c.released = members.is_empty();
                    if !c.released {
                        (c.proxy, c.parts) = proxy::shape(&s.pieces, &members, s.origin + c.local);
                    }
                }
            }
            s.staging_age += dt;
            if s.sealed && s.gpu_ready {
                // Consume confirmed partial time too. Requiring an entire
                // dt can strand age one rounding bit below a known impact.
                s.age = if s.landed {
                    s.age + dt
                } else {
                    (s.age + dt).min(s.confirmed)
                };
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
            s.partition(&mut planning);
            if s.finishing.is_some() {
                continue;
            }
        }
        // Collision geometry is prepared once and shared by both backends.
        // No bodyless chunk repeats the same brick scan for every tiny step.
        keys.sort_unstable();
        let collision_start = self.section_collision_turn % keys.len().max(1);
        keys.rotate_left(collision_start);
        let mut last_served = None;
        let before_preparation = collision;
        let mut ready_chunks = BTreeSet::new();
        for (order, &key) in keys.iter().enumerate() {
            let s = self.sections.get_mut(&key).unwrap();
            if s.finishing.is_some() {
                continue;
            }
            let count = s.chunks.len();
            let start = s.preparation_turn;
            for offset in 0..count {
                let i = (start + offset) % count;
                let c = &mut s.chunks[i];
                if c.released {
                    continue;
                }
                if c.handle.is_some_and(|h| self.world.bodies[h].is_sleeping())
                    && c.collision.unchanged(building)
                {
                    ready_chunks.insert((key.0, key.1, i));
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
                let (h, velocity) = if c.handle.is_none() {
                    // Bound an ordinary collision response and actor separation,
                    // not just the velocity before a player touches the chunk.
                    (
                        h + Vec3::splat(motion::MAX_SEPARATION),
                        Vec3::splat(MAX_SPEED),
                    )
                } else {
                    (h, velocity)
                };
                let before = collision;
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
                if collision < before {
                    s.preparation_turn = (i + 1) % count;
                    last_served = Some(order);
                }
                if ready {
                    ready_chunks.insert((key.0, key.1, i));
                    if let Some(handle) = c.handle {
                        self.world.bodies[handle].set_enabled(true);
                    }
                } else if c.handle.is_some() {
                    c.freeze(&mut self.world);
                }
            }
        }
        self.diagnostics.section_preparation_work += u64::from(before_preparation - collision);
        let bodyless = self
            .sections
            .values()
            .any(|s| s.chunks.iter().any(|c| c.handle.is_none() && !c.released));
        if bodyless && self.collision_query_revision != self.surroundings.revision() {
            // Newly streamed colliders must be visible to queries now, not
            // only after the next solver step. Use the island-safe adapter.
            bri_physics::detect_collisions(&mut self.world);
            self.collision_query_revision = self.surroundings.revision();
        }
        // Capture all newly partitioned groups together, after preparation.
        // This snapshot is consumed within this frame and never retained.
        let scene = motion::RubbleScene::new(self);
        let frame = motion::Frame {
            dt,
            actors: &self.section_pushers,
            scene: &scene,
        };
        let mut available = self
            .limit
            .saturating_sub(self.bodies.len() + self.chunk_handles().count());
        let used_parts = self.bodies.len()
            + self
                .sections
                .values()
                .flat_map(|s| &s.chunks)
                .filter(|c| c.handle.is_some())
                .map(|c| c.parts)
                .sum::<usize>();
        let mut part_slots = MAX_PROXY_PARTS.saturating_sub(used_parts);
        let before_motion = collision;
        for (order, &key) in keys.iter().enumerate() {
            let s = self.sections.get_mut(&key).unwrap();
            if s.finishing.is_some() {
                continue;
            }
            let count = s.chunks.len();
            let start = s.chunk_turn;
            for i in 0..count {
                let index = (i + start) % count;
                let c = &mut s.chunks[index];
                if c.released || c.handle.is_some() {
                    continue;
                }
                let ready = ready_chunks.contains(&(key.0, key.1, index));
                if !ready || available == 0 || c.parts > part_slots || c.proxy.is_none() {
                    if c.advance_fallback(
                        &frame,
                        (key.0, key.1, index),
                        &self.world,
                        ready,
                        &mut collision,
                    )? {
                        s.chunk_turn = (index + 1) % count;
                        last_served = Some(order);
                        self.diagnostics.section_fallback_steps += 1;
                    }
                    continue;
                }
                let (center, turn) = c.pose(&self.world);
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
                    .restitution(RESTITUTION)
                    .collision_groups(InteractionGroups::new(
                        Group::GROUP_2,
                        Group::GROUP_1 | Group::GROUP_2,
                        InteractionTestMode::And,
                    ));
                let (handle, _) = self.world.insert(body, collider);
                c.handle = Some(handle);
                s.chunk_turn = (index + 1) % count;
                last_served = Some(order);
                c.debt = 0.;
                super::foley::configure(&mut self.world, handle, self.foley.policy.as_deref());
                self.foley.born(handle, 0.12);
                available -= 1;
                part_slots -= c.parts;
            }
        }
        self.diagnostics.section_motion_work += u64::from(before_motion - collision);
        if let Some(order) = last_served {
            self.section_collision_turn = (collision_start + order + 1) % keys.len();
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
                        let (at, rotation) = s.member_pose(i, center, turn, &self.world);
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
                impulse: None,
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
    fn attachment_frames_match_rendered_section_members_through_motion_and_fade() -> Result<()> {
        let (b, _) = super::super::tests::building(&[]);
        let mut d = BrickDebris::new();
        let origin = [0.5, 3.3, 0.0];
        d.cues(
            &[
                cue(1, [0.0, 3.3, 0.0], 2, [1.0, 0.3, 0.5], origin, origin),
                cue(2, [1.0, 3.3, 0.0], 2, [1.0, 0.3, 0.5], origin, origin),
                Cue {
                    id: 3,
                    tick: 1,
                    position: origin,
                    kind: CueKind::BrickSectionReady {
                        started: 0,
                        group: 7,
                        members: 2,
                    },
                },
            ],
            &b,
        )?;
        for target in [0.2, SOLID_SECONDS * 0.8, SOLID_SECONDS + FADE_SECONDS * 0.5] {
            while d.sections[&(0, 7)].age < target {
                d.advance(1.0 / 60.0, &b)?;
            }
            let frames = d.attachment_frames(|_| true);
            assert_eq!(frames.len(), 2);
            let section = &d.sections[&(0, 7)];
            for (p, (_, drawn)) in section.pieces.iter().zip(d.section_instances()) {
                let original = Mat4::from_rotation_translation(p.turn, section.origin + p.offset);
                let attached = frames[&p.brick].delta * original;
                assert!(
                    attached.abs_diff_eq(drawn.transform, 1e-4),
                    "brick {}: source frame differs from its actual rendered member",
                    p.brick
                );
                assert_eq!(frames[&p.brick].cue, p.cue);
            }
        }
        d.cues(
            &[Cue {
                id: 4,
                tick: 2,
                position: origin,
                kind: CueKind::BrickSectionRetired {
                    started: 0,
                    group: 7,
                },
            }],
            &b,
        )?;
        assert!(d.attachment_frames(|_| true).is_empty());
        assert_eq!(d.section_instances().count(), 0);
        Ok(())
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
    fn a_later_collapse_recovers_breakup_after_earlier_shot_debris_expires() -> Result<()> {
        let (b, _) = super::super::tests::building(&[]);
        let mut d = BrickDebris::new();
        d.set_limit(1);
        // The first hit leaves ordinary flying debris. The later hit detaches
        // a connected section while that earlier debris owns the body slot.
        d.cues(
            &[super::super::tests::kill(
                1,
                100,
                [-4., 0.3, 0.],
                [-4., 0.3, 0.],
                10.,
                1.,
            )],
            &b,
        )?;
        let frames = ((SOLID_SECONDS + FADE_SECONDS - 0.2) * 60.) as usize;
        run(&mut d, &b, frames);
        assert_eq!(d.bodies.len(), 1);
        d.cues(
            &[
                cue(
                    10,
                    [4., 3.3, 0.],
                    2,
                    [0.5, 0.6, 0.5],
                    [4., 3.6, 0.],
                    [4., 3., 0.],
                ),
                cue(
                    11,
                    [4., 3.9, 0.],
                    2,
                    [0.5, 0.6, 0.5],
                    [4., 3.6, 0.],
                    [4., 3., 0.],
                ),
                Cue {
                    id: 12,
                    tick: 1,
                    position: [0.; 3],
                    kind: CueKind::BrickSectionReady {
                        started: 0,
                        group: 7,
                        members: 2,
                    },
                },
            ],
            &b,
        )?;
        run(&mut d, &b, 120);
        let section = &d.sections[&(0, 7)];
        assert!(d.bodies.is_empty(), "the earlier hit must have expired");
        assert!(section.landed);
        assert_eq!(d.instances().count(), 2, "retain every member");
        assert!(
            !section.chunks.is_empty(),
            "temporary pressure must not permanently remove breakup"
        );
        assert_eq!(
            d.chunk_handles().count(),
            1,
            "landed members regain physical interaction"
        );
        assert!(d.len() <= d.limit());
        let handle = d.chunk_handles().next().unwrap();
        let before = Vec3::from_array(d.world.bodies[handle].translation().to_array());
        for frame in 0..60 {
            d.push(&[Pusher {
                id: 1,
                center: Vec3::new(before.x - 2. + frame as f32 / 15., 1.2, before.z),
                rotation: Quat::IDENTITY,
                half: Vec3::new(0.5, 1.2, 0.5),
                shape: None,
            }]);
            d.advance(1. / 60., &b)?;
        }
        let after = Vec3::from_array(d.world.bodies[handle].translation().to_array());
        assert!(
            after.x > before.x + 0.25,
            "walking into the recovered debris must push it: {before} -> {after}"
        );
        Ok(())
    }
    #[test]
    fn prepared_airborne_clumps_keep_motion_while_waiting_for_capacity() -> Result<()> {
        let (b, _) = super::super::tests::building(&[]);
        let mut d = BrickDebris::new();
        d.set_limit(1);
        d.cues(
            &[
                cue(
                    10,
                    [4., 3.3, 0.],
                    2,
                    [0.5, 0.6, 0.5],
                    [4., 3.6, 0.],
                    [4., 3., 0.],
                ),
                cue(
                    11,
                    [4., 3.9, 0.],
                    2,
                    [0.5, 0.6, 0.5],
                    [4., 3.6, 0.],
                    [4., 3., 0.],
                ),
            ],
            &b,
        )?;
        d.set_limit(0);
        run(&mut d, &b, 1);
        assert_eq!(d.sections[&(0, 7)].chunks.len(), 1);
        assert!(!d.sections[&(0, 7)].landed);
        // Preparation is independent of body capacity. Consume the newly
        // available slot before the next frame can admit a prepared clump.
        d.set_limit(1);
        d.cues(
            &[super::super::tests::kill(
                20,
                100,
                [-4., 0.3, 0.],
                [-4., 0.3, 0.],
                10.,
                1.,
            )],
            &b,
        )?;
        d.bodies.values_mut().next().unwrap().age = SOLID_SECONDS + FADE_SECONDS - 2.;
        run(&mut d, &b, 90);
        let section = &d.sections[&(0, 7)];
        assert!(section.landed);
        assert_eq!(section.chunks.len(), 1, "retain the exact prepared proxy");
        assert!(section.chunks[0].handle.is_none());
        assert!(!section.chunks.is_empty());
        assert_eq!(d.bodies.len(), 1, "older debris must still occupy capacity");
        assert_eq!(d.len(), 1, "do not exceed capacity while waiting");
        run(&mut d, &b, 90);
        assert!(d.bodies.is_empty());
        assert_eq!(d.chunk_handles().count(), 1);
        assert_eq!(d.instances().count(), 2);
        assert!(d.len() <= d.limit());
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
            s.chunks.len() > d.limit(),
            "breakup geometry remains independent of the resident body pool"
        );
        for chunk in 0..s.chunks.len() {
            assert!(
                s.assignment.iter().filter(|a| **a == Some(chunk)).count() <= MAX_CHUNK_MEMBERS
            );
            assert!(s.chunks[chunk].half.max_element() * 2. <= MAX_CHUNK_SPAN + 0.001);
        }
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
            let origin = [(width - 1) as f32 * 0.5, 10. + height as f32 * 0.3, 0.];
            let cues: Vec<_> = (0..count)
                .map(|i| {
                    cue(
                        i + 1,
                        [(i % width) as f32, 10.3 + (i / width) as f32 * 0.6, 0.],
                        count as u32,
                        [width as f32 * 0.5, height as f32 * 0.3, 0.5],
                        origin,
                        [origin[0], 10., 0.],
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
    fn detached_shapes_fall_without_a_size_or_seed_dependent_hinge() {
        for half in [
            Vec3::new(0.5, 12., 0.5),
            Vec3::new(10., 0.3, 10.),
            Vec3::new(12., 0.3, 0.5),
        ] {
            let mut p = landing_regressions::piece(1, Vec3::new(0., 20., 0.));
            p.section_half = half;
            p.pivot = Vec3::new(5., 0., 2.);
            let s = Section::new(&p);
            for age in [0., 0.1, 0.5] {
                let (center, turn) = s.pose(age);
                assert_eq!(turn, Quat::IDENTITY);
                assert!(
                    (center - (p.origin - Vec3::Y * (0.5 * GRAVITY * age * age))).length() < 1e-5
                );
            }
        }
    }
}

#[cfg(test)]
mod landing_regressions {
    use super::*;
    pub(super) fn piece(id: u64, at: Vec3) -> SectionPiece {
        SectionPiece {
            impulse: None,
            cue: id,
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
        let deck: Vec<_> = (2..=4)
            .flat_map(|x| {
                (-1..=1).map(move |z| (1000 + (x * 3 + z + 1) as u64, [x as f32, 6.7, z as f32]))
            })
            .collect();
        let (b, _) = super::super::tests::building(&deck);
        let mut pieces: Vec<_> = (-10..=10)
            .flat_map(|x| (-4..=4).map(move |z| (x, z)))
            .enumerate()
            .map(|(i, (x, z))| piece(i as u64 + 1, Vec3::new(x as f32, 10., z as f32)))
            .collect();
        for p in &mut pieces {
            p.origin = Vec3::new(0., 10., 0.);
            p.pivot = Vec3::new(0., 9.7, 0.);
            p.section_half = Vec3::new(10.5, 0.3, 4.5);
        }
        let mut s = Section::new(&pieces[0]);
        s.expected = pieces.len();
        let mut old = Section::new(&pieces[0]);
        old.expected = pieces.len();
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
                            let at = center + turn * local;
                            if (1.5..=4.5).contains(&at.x) && at.z.abs() <= 1.5 {
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
        for _ in 0..12 {
            d.advance(1. / 60., &b)?;
        }
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
        // Retirement before the Ready barrier must also affect the eventual
        // physics proxy, not just suppress the visible member while staged.
        d.cues(
            &[Cue {
                id: 100,
                tick: 11,
                position: [0.; 3],
                kind: CueKind::BrickSectionReady {
                    started: 0,
                    group: 7,
                    members: 2,
                },
            }],
            &b,
        )?;
        for _ in 0..120 {
            d.advance(1. / 60., &b)?;
        }
        let section = &d.sections[&(0, 7)];
        let chunk =
            &section.chunks[section.assignment[1].expect("remaining material gets contact motion")];
        let retired_point = Vector::from_array((section.pieces[0].offset - chunk.local).to_array());
        assert!(
            !chunk
                .proxy
                .as_ref()
                .unwrap()
                .contains_point(&Pose::IDENTITY, retired_point),
            "late breakup must not reintroduce retired material as invisible collision"
        );
        assert_eq!(d.instances().count(), 1);
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
    fn disconnected_groups_keep_independent_collision_motion_beyond_the_body_pool() -> Result<()> {
        let (b, _) = super::super::tests::building(&[]);
        let mut d = BrickDebris::new();
        let mut pieces: Vec<_> = (0..100)
            .map(|i| {
                super::landing_regressions::piece(
                    i + 1,
                    Vec3::new((i % 10) as f32 * 2., 3.3, (i / 10) as f32 * 2.),
                )
            })
            .collect();
        for p in &mut pieces {
            p.members = 100;
            p.origin = Vec3::new(9., 3.3, 9.);
            p.section_half = Vec3::new(9.5, 0.3, 9.5);
            p.pivot = Vec3::new(9., 3., 9.);
        }
        d.add_section((0, 1), pieces, &b)?;
        d.sections.get_mut(&(0, 1)).unwrap().sealed = true;
        for _ in 0..180 {
            d.advance(1. / 60., &b)?;
        }
        let section = &d.sections[&(0, 1)];
        assert!(
            section.landed,
            "flight must reach floor: age={} shown={} confirmed={} prepared={} pending={} impact={:?}",
            section.age,
            section.shown,
            section.confirmed,
            section.prepared,
            section.pending.len(),
            section.impact_at
        );
        assert_eq!(section.chunks.len(), 100);
        assert_eq!(d.instances().count(), 100);
        assert!(d.len() <= d.limit());
        assert!(section.chunks.iter().all(|c| c.center.is_finite()));
        assert!(section.landed);
        assert!(
            d.instances()
                .all(|(_, t)| (0.2..1.5).contains(&t.transform.w_axis.y)),
            "every disconnected group falls independently onto the floor"
        );
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
