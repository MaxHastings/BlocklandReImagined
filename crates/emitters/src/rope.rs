//! `URopeBeamEmitter` (AGP.dll 0x1c010..0x1f0a0, see `specs/spark-mesh-rope-emitters.md`):
//! a beam whose points are a Verlet chain with line-checked moves, segment constraints,
//! optional flex and anchors. One fixed 35 ms step per update, as the original runs it.
use crate::beam::{Beam, BeamProps};
use crate::emitter::{Ctx, Emitter};
use crate::particle::flags;
use crate::props::ParticleEmitterProps;
use crate::random::Rand;
use crate::rotation::{mirror_by_vector, safe_normal};
use glam::Vec3;

/// The fixed step of the original's `UpdateParticles` (0x3d0f5c29).
pub const ROPE_STEP: f32 = 0.035;

#[derive(Clone, Copy, Debug)]
pub struct RopeAnchor {
    /// The tag the host resolves with [`crate::emitter::World::actor_location`].
    pub tag: u32,
    /// Position along the rope, 0..1.
    pub position: f32,
    /// Particle index, filled by [`Rope::link_anchors`] (-1: not found).
    pub index: i32,
}

#[derive(Clone, Debug)]
pub struct RopeProps {
    /// 0 is `BRT_NOTROPE`; any other value records segment tension.
    pub rope_type: u8,
    pub initial_direction: Vec3,
    pub min_vel_threshold: f32,
    pub segment_length: f32,
    pub bounciness: f32,
    pub use_flex: bool,
    pub flex_threshold: f32,
    pub relax_factor: i32,
    pub twist_factor: f32,
    pub use_anchors: bool,
    pub anchors: Vec<RopeAnchor>,
    pub verlet_integration_count: i32,
}

impl Default for RopeProps {
    fn default() -> Self {
        Self {
            rope_type: 1,
            initial_direction: Vec3::new(0.0, 0.0, -1.0),
            min_vel_threshold: 0.0,
            segment_length: 10.0,
            bounciness: 0.0,
            use_flex: false,
            flex_threshold: 1.0,
            relax_factor: 1,
            twist_factor: 0.0,
            use_anchors: false,
            anchors: Vec::new(),
            verlet_integration_count: 4,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Rope {
    pub beam: Beam,
    pub props: RopeProps,
    pub particle_accel: Vec<Vec3>,
    pub segment_tension: Vec<f32>,
    pub avg_delta_time: f32,
}

impl Rope {
    pub fn new(
        base: ParticleEmitterProps,
        beam_props: BeamProps,
        mut props: RopeProps,
        count: usize,
        rand: &mut Rand,
    ) -> Self {
        if props.initial_direction.length_squared() >= 1e-8 {
            props.initial_direction = props.initial_direction.normalize();
        }
        Self {
            beam: Beam::new(base, beam_props, count, rand),
            props,
            particle_accel: vec![Vec3::ZERO; count],
            segment_tension: vec![0.0; count.saturating_sub(1)],
            avg_delta_time: 0.0,
        }
    }

    pub fn emitter(&self) -> &Emitter {
        &self.beam.emitter
    }

    /// `LinkAnchors`: the particle index of each anchor, or -1 when the tagged actor is gone.
    pub fn link_anchors(&mut self, ctx: &mut Ctx<'_>) {
        let max = self.beam.emitter.max_active_particles as f32;
        for a in &mut self.props.anchors {
            a.index = if ctx.world.actor_location(a.tag).is_some() {
                (max * a.position - 1.0 + 0.5).floor() as i32
            } else {
                -1
            };
        }
    }

    /// `IsAnchorIndex`: particle 0 hangs from the owner; others from linked anchors.
    fn anchor(&self, ctx: &mut Ctx<'_>, i: usize) -> Option<Vec3> {
        if i == 0 {
            return Some(ctx.owner.location);
        }
        if !self.props.use_anchors {
            return None;
        }
        let a = self.props.anchors.iter().find(|a| a.index == i as i32)?;
        ctx.world.actor_location(a.tag)
    }

    fn loc(&self, i: usize) -> Vec3 {
        self.beam.emitter.particles[i].location
    }

    /// `MoveParticle` (0x1d830): a line-checked move with the bounce response.
    fn move_particle(
        &mut self,
        ctx: &mut Ctx<'_>,
        mut a: Vec3,
        mut b: Vec3,
        index: usize,
        set_old: bool,
    ) {
        let mut delta = b - a;
        self.move_worker(ctx, &mut a, &mut b, &mut delta);
        let p = &mut self.beam.emitter.particles[index];
        if set_old {
            p.old_location = b - delta;
        }
        p.location = b;
    }

    /// `MoveParticleWorker` (0x1db50).
    fn move_worker(&mut self, ctx: &mut Ctx<'_>, a: &mut Vec3, b: &mut Vec3, delta: &mut Vec3) {
        if !self.beam.emitter.props.use_collision {
            return;
        }
        let Some(hit) = ctx.world.line_check(*a, *b, false) else {
            return;
        };
        let rp = &self.props;
        let mut n = hit.normal;
        let span = (*b - *a).length();
        let t = if span > 0.0 {
            (hit.location - *a).length() / span
        } else {
            0.0
        };
        *delta *= rp.bounciness;
        let d = *b - *a;
        let p = *a + d * t;
        *delta = mirror_by_vector(*delta, n);
        *b = p + *delta;
        *a = p + n * 0.0001;
        let e = *b - *a;
        if e.length() < rp.min_vel_threshold {
            if n.dot(e) < 0.0 {
                n = -n;
            }
            let s = n.dot(e);
            if s < 0.0001 {
                *b = p + n;
            } else {
                *b -= n * s;
            }
            let u = n.dot(*a - p).abs();
            if u < 0.0001 {
                *a = p + n;
            } else if n.dot(*a - *b) >= 0.0 {
                *a -= n * u;
            } else {
                *a += n * u;
            }
        }
    }

    /// `SpawnParticle` (0x1f0a0). The final point fill repeats the beam's with the rope's
    /// initial direction (the spec read it as the same algorithm and did not transcribe it);
    /// `LockSystem` overwrites the positions every update.
    pub fn spawn_particle(
        &mut self,
        ctx: &mut Ctx<'_>,
        index: usize,
        age: f32,
        flags_arg: i32,
        offset: Vec3,
    ) {
        self.beam
            .spawn_particle(ctx, index, age, flags_arg, 0, offset);
        self.finish_spawns(ctx);
    }

    /// The rope's own steps of `SpawnParticle`, for every particle the beam finished spawning.
    fn finish_spawns(&mut self, ctx: &mut Ctx<'_>) {
        for (index, age) in std::mem::take(&mut self.beam.spawned) {
            if index >= self.beam.emitter.particles.len() {
                continue;
            }
            let size = ctx.rand.vec_yxz(self.beam.emitter.props.start_size_range);
            {
                let p = &mut self.beam.emitter.particles[index];
                p.start_size = size;
                p.size = size;
            }
            self.particle_accel[index] = Vec3::ZERO;
            if index == 0 {
                let p = &mut self.beam.emitter.particles[0];
                p.location = ctx.owner.location;
                p.old_location = ctx.owner.location;
            } else {
                let prev = self.loc(index - 1);
                let target = prev + self.props.initial_direction * self.props.segment_length;
                self.beam.emitter.particles[index].location = target;
                self.move_particle(ctx, prev, target, index, true);
            }
            let loc = self.loc(index);
            let a = self.props.initial_direction * self.props.segment_length;
            self.beam
                .generate_points(ctx, index, loc, a, Vec3::ZERO, false, age, false);
        }
    }

    /// `UpdateParticles`: the passed dt is ignored.
    pub fn update_particles(&mut self, ctx: &mut Ctx<'_>, _dt: f32) -> usize {
        let ret = self.beam.update_particles(ctx, ROPE_STEP);
        self.finish_spawns(ctx);
        self.integrate(ctx, ROPE_STEP);
        let avg = self.avg_delta_time;
        self.constrain(ctx, avg);
        self.lock_system();
        self.calculate_forces(ctx);
        ret
    }

    fn active(&self) -> usize {
        let e = &self.beam.emitter;
        e.active_particles.min(e.particles.len())
    }

    /// `CalculateForces`: gravity of the owner's physics volume.
    fn calculate_forces(&mut self, ctx: &mut Ctx<'_>) {
        let g = ctx.owner.gravity.unwrap_or(Vec3::ZERO);
        for i in 0..self.active() {
            self.particle_accel[i] = g;
        }
    }

    /// `IntegrateSystem`: time-corrected Verlet.
    fn integrate(&mut self, ctx: &mut Ctx<'_>, dt: f32) {
        let mut prev = self.avg_delta_time;
        if prev == 0.0 {
            self.avg_delta_time = dt;
            prev = dt;
        }
        self.avg_delta_time = dt * 0.1 + self.avg_delta_time * 0.9;
        let t = self.avg_delta_time;
        let damping = self.beam.emitter.props.damping_factor_range;
        for i in 0..self.active() {
            let (cur, old) = {
                let p = &self.beam.emitter.particles[i];
                (p.location, p.old_location)
            };
            let v = (cur - old) * (t / prev);
            let a = self.particle_accel[i];
            let d = ctx.rand.vec_yxz(damping);
            let new = cur + v * d + a * t * t;
            let p = &mut self.beam.emitter.particles[i];
            p.location = new;
            p.old_location = cur;
            self.move_particle(ctx, cur, new, i, true);
        }
    }

    /// `ConstrainSystem` (0x1c5a0).
    fn constrain(&mut self, ctx: &mut Ctx<'_>, _dt: f32) {
        let n = self.active();
        let seg = self.props.segment_length;
        for _ in 0..self.props.verlet_integration_count.max(0) {
            for i in 1..n {
                let a = self.anchor(ctx, i);
                let b = self.anchor(ctx, i - 1);
                let pi = self.loc(i);
                let pj = self.loc(i - 1);
                let d = pi - pj;
                let l = d.length();
                let corr = (l - seg) / l;
                let h = d * corr * 0.5;
                let (mut ci, mut cj) = (h, h);
                let (mut pi_m, mut pj_m) = (pi, pj);
                if let Some(al) = a {
                    ci = Vec3::ZERO;
                    cj *= 2.0;
                    pi_m = al;
                }
                if let Some(bl) = b {
                    cj = Vec3::ZERO;
                    ci *= 2.0;
                    pj_m = bl;
                }
                let new_pi = pi_m - ci;
                let mut new_pj = pj_m + cj;
                if self.props.use_flex && i + 1 < n {
                    let pp = self.loc(i - 1);
                    let pn = self.loc(i + 1);
                    let w = pn - pp;
                    let lenw = w.length();
                    let threshold = 2.0 * self.props.flex_threshold * seg;
                    if lenw < threshold {
                        let mut push = 0.5 * (threshold - lenw) / self.props.relax_factor as f32
                            * safe_normal(w);
                        push += safe_normal((pn - pi).cross(pp - pi)) * self.props.twist_factor;
                        let next_anchor = self.anchor(ctx, i + 1);
                        let (mut push_k, mut push_j) = (push, push);
                        let mut pn_m = pn;
                        if let Some(nl) = next_anchor {
                            push_k = Vec3::ZERO;
                            push_j *= 2.0;
                            pn_m = nl;
                        }
                        if b.is_some() {
                            push_j = Vec3::ZERO;
                            push_k *= 2.0;
                        }
                        new_pj -= push_j;
                        let new_pn = pn_m + push_k;
                        let from = self.loc(i + 1);
                        self.move_particle(ctx, from, new_pn, i + 1, false);
                    }
                }
                let from_j = self.loc(i - 1);
                self.move_particle(ctx, from_j, new_pj, i - 1, false);
                let from_i = self.loc(i);
                self.move_particle(ctx, from_i, new_pi, i, false);
            }
        }
        if self.props.rope_type != 0 {
            for i in 0..n.saturating_sub(1) {
                if i < self.segment_tension.len() {
                    self.segment_tension[i] = (self.loc(i + 1) - self.loc(i)).length() / seg - seg;
                }
            }
        }
    }

    /// `LockSystem`: re-linearise the drawn points along the simulated chain.
    fn lock_system(&mut self) {
        let n = self.active();
        if n == 0 {
            return;
        }
        let h = self.beam.props.high_frequency_points;
        for i in 0..n - 1 {
            let first = self.loc(i);
            let last = self.loc(i + 1);
            let hf = &mut self.beam.hf_points;
            hf[i * h].location = first;
            hf[(i + 1) * h - 1].location = last;
            for k in 1..h - 1 {
                let t = hf[i * h + k].t;
                hf[i * h + k].location = first + (last - first) * t;
            }
        }
        let tail = self.loc(n - 1);
        for k in 0..h {
            self.beam.hf_points[(n - 1) * h + k].location = tail;
        }
    }

    /// Particles that are alive (for hosts deciding whether to draw).
    pub fn live(&self) -> usize {
        self.beam.emitter.particles[..self.active()]
            .iter()
            .filter(|p| p.flags & flags::ALIVE != 0)
            .count()
    }
}
