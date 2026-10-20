//! `UBeamEmitter` (Engine.dll 0xf90f0..0xfb870, see `specs/beam-trail-emitters.md`): a
//! polyline of low-frequency points, resampled into high-frequency points with noise,
//! drawn as a ribbon (optionally several rotated sheets).
use crate::emitter::{Ctx, Emitter, Event};
use crate::particle::flags;
use crate::props::{ParticleEmitterProps, Range, RangeVector};
use crate::random::Rand;
use crate::ribbon::{IndexedBatch, draw_state, pack_color};
use crate::rotation::{rotate_angle_axis, safe_normal};
use crate::sprite::SpriteVertex;
use glam::{Mat4, Vec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndPointBy {
    Velocity,
    Distance,
    Offset,
    Actor,
    TraceOffset,
    OffsetAsAbsolute,
    DynamicDistance,
    RopeSegment,
}

#[derive(Clone, Debug)]
pub struct BeamEndPoint {
    /// The tag the host resolves with [`crate::emitter::World::actor_location`].
    pub actor_tag: u32,
    pub offset: RangeVector,
    pub weight: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct BeamScale {
    pub frequency_scale: Vec3,
    pub relative_length: f32,
}

#[derive(Clone, Debug)]
pub struct BeamProps {
    pub beam_distance_range: Range,
    pub end_points: Vec<BeamEndPoint>,
    pub determine_end_point_by: EndPointBy,
    pub texture_u_scale: f32,
    pub texture_v_scale: f32,
    pub rotating_sheets: i32,
    pub trigger_endpoint: bool,
    pub low_frequency_noise_range: RangeVector,
    pub low_frequency_points: usize,
    pub high_frequency_noise_range: RangeVector,
    pub high_frequency_points: usize,
    pub lf_scale_factors: Vec<BeamScale>,
    pub hf_scale_factors: Vec<BeamScale>,
    pub lf_scale_repeats: f32,
    pub hf_scale_repeats: f32,
    pub use_high_frequency_scale: bool,
    pub use_low_frequency_scale: bool,
    pub noise_determines_end_point: bool,
    pub dynamic_hf_noise_range: RangeVector,
    pub dynamic_hf_noise_points_range: Range,
    pub dynamic_time_between_noise_range: Range,
    pub use_branching: bool,
    pub branch_probability: Range,
    pub branch_hf_points_range: Range,
    pub branch_emitter: i32,
    pub branch_spawn_amount_range: Range,
    pub linkup_lifetime: bool,
}

impl BeamProps {
    /// `PostEditChange`'s clamps.
    pub fn clamp(&mut self) {
        self.high_frequency_points = self.high_frequency_points.max(2);
        self.low_frequency_points = self.low_frequency_points.max(2);
    }
}

impl Default for BeamProps {
    fn default() -> Self {
        Self {
            beam_distance_range: Range::fixed(0.0),
            end_points: Vec::new(),
            determine_end_point_by: EndPointBy::Velocity,
            texture_u_scale: 1.0,
            texture_v_scale: 1.0,
            rotating_sheets: 1,
            trigger_endpoint: false,
            low_frequency_noise_range: RangeVector::fixed(Vec3::ZERO),
            low_frequency_points: 2,
            high_frequency_noise_range: RangeVector::fixed(Vec3::ZERO),
            high_frequency_points: 2,
            lf_scale_factors: Vec::new(),
            hf_scale_factors: Vec::new(),
            lf_scale_repeats: 0.0,
            hf_scale_repeats: 0.0,
            use_high_frequency_scale: false,
            use_low_frequency_scale: false,
            noise_determines_end_point: false,
            dynamic_hf_noise_range: RangeVector::fixed(Vec3::ZERO),
            dynamic_hf_noise_points_range: Range::fixed(0.0),
            dynamic_time_between_noise_range: Range::fixed(0.0),
            use_branching: false,
            branch_probability: Range::fixed(0.0),
            branch_hf_points_range: Range::fixed(0.0),
            branch_emitter: -1,
            branch_spawn_amount_range: Range::fixed(0.0),
            linkup_lifetime: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BeamPoint {
    pub location: Vec3,
    pub t: f32,
}

#[derive(Clone, Debug)]
pub struct Beam {
    pub emitter: Emitter,
    pub props: BeamProps,
    pub sheets_used: usize,
    pub beam_value_sum: f32,
    pub hf_points: Vec<BeamPoint>,
    pub lf_points: Vec<Vec3>,
    pub time_since_last_dynamic_noise: f32,
    /// The particles whose spawn the beam finished in the last call (slot and age), for a subclass
    /// that adds its own steps (the rope).
    pub spawned: Vec<(usize, f32)>,
}

fn frac(v: f32) -> f32 {
    v - v.floor()
}

/// The noise for point `i` through a frequency-scale table; keeps `prev` when the
/// position is past every entry (no draws then), as read.
#[allow(clippy::too_many_arguments)]
fn scaled_noise(
    factors: &[BeamScale],
    repeats: f32,
    inv_n: f32,
    i: usize,
    noise: RangeVector,
    rand: &mut Rand,
    prev: Vec3,
) -> Vec3 {
    let u = frac((repeats + 1.0) * inv_n * i as f32);
    let Some(j) = factors.iter().position(|f| f.relative_length >= u) else {
        return prev;
    };
    let cur = factors[j];
    let (ps, pl) = if j > 0 {
        (
            factors[j - 1].frequency_scale,
            factors[j - 1].relative_length,
        )
    } else {
        (Vec3::ONE, 0.0)
    };
    let f = if cur.relative_length == 0.0 {
        1.0
    } else {
        (u - pl) / (cur.relative_length - pl)
    };
    let s = ps + (cur.frequency_scale - ps) * f;
    rand.vec_yxz(noise) * s
}

impl Beam {
    pub fn new(
        base: ParticleEmitterProps,
        mut props: BeamProps,
        count: usize,
        rand: &mut Rand,
    ) -> Self {
        props.clamp();
        let sheets = if props.rotating_sheets != 0 {
            props.rotating_sheets as usize
        } else {
            1
        };
        Self {
            emitter: {
                let mut e = Emitter::new(base, count, rand);
                e.log_spawns = true;
                e
            },
            sheets_used: sheets,
            beam_value_sum: 0.0,
            hf_points: vec![BeamPoint::default(); props.high_frequency_points * count],
            lf_points: vec![Vec3::ZERO; props.low_frequency_points * count],
            time_since_last_dynamic_noise: 0.0,
            spawned: Vec::new(),
            props,
        }
    }

    /// Low- and high-frequency points for slot `idx` from `loc`, displacement `a` and the
    /// absolute end `b` (when `use_b`). Branching only on a fresh spawn.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn generate_points(
        &mut self,
        ctx: &mut Ctx<'_>,
        idx: usize,
        loc: Vec3,
        a: Vec3,
        b: Vec3,
        use_b: bool,
        age: f32,
        branching: bool,
    ) {
        let bp = self.props.clone();
        let (hf, lf) = (bp.high_frequency_points, bp.low_frequency_points);
        let dh = 1.0 / (hf - 1) as f32;
        let dl = 1.0 / (lf - 1) as f32;
        let inv_lf = 1.0 / lf as f32;
        let inv_hf = 1.0 / hf as f32;
        let lfp = idx * lf;
        let hfp = idx * hf;
        self.lf_points[lfp] = loc;
        let mut n = Vec3::ZERO;
        for i in 1..lf {
            n = if bp.use_low_frequency_scale {
                scaled_noise(
                    &bp.lf_scale_factors,
                    bp.lf_scale_repeats,
                    inv_lf,
                    i,
                    bp.high_frequency_noise_range,
                    ctx.rand,
                    n,
                )
            } else {
                ctx.rand.vec_yxz(bp.low_frequency_noise_range)
            };
            self.lf_points[lfp + i] = loc + a * (i as f32 * dl) + n;
        }
        if use_b {
            self.lf_points[lfp + lf - 1] = b;
        }
        self.hf_points[hfp] = BeamPoint {
            location: self.lf_points[lfp],
            t: 0.0,
        };
        let mut j = 1usize;
        let mut s = dh;
        n = Vec3::ZERO;
        for i in 1..hf {
            n = if bp.use_high_frequency_scale {
                scaled_noise(
                    &bp.hf_scale_factors,
                    bp.hf_scale_repeats,
                    inv_hf,
                    i,
                    bp.high_frequency_noise_range,
                    ctx.rand,
                    n,
                )
            } else {
                ctx.rand.vec_yxz(bp.high_frequency_noise_range)
            };
            let cursor = (j - 1).min(lf - 1);
            let base = if j < lf {
                let w = (lf - 1) as f32 * s;
                (1.0 - w) * self.lf_points[lfp + cursor] + w * self.lf_points[lfp + j]
            } else {
                self.lf_points[lfp + cursor]
            };
            let point = base + n;
            self.hf_points[hfp + i] = BeamPoint {
                location: point,
                t: i as f32 * dh,
            };
            if branching
                && bp.use_branching
                && bp.branch_hf_points_range.min <= i as f32
                && i as f32 <= bp.branch_hf_points_range.max
            {
                let p = ctx.rand.range(bp.branch_probability);
                let r2 = ctx.rand.unit();
                if r2 <= p && bp.branch_emitter >= 0 {
                    self.spawn_branch(ctx, idx, point, age);
                }
            }
            s += dh;
            if s > dl {
                j += 1;
                s -= dl;
            }
        }
        if use_b {
            self.hf_points[hfp + hf - 1].location = b;
        }
    }

    fn spawn_branch(&mut self, ctx: &mut Ctx<'_>, idx: usize, at: Vec3, age: f32) {
        let target = self.props.branch_emitter as usize;
        let Some(Some(mut other)) = ctx.others.get_mut(target).map(Option::take) else {
            return;
        };
        if other.initialized {
            let count = ctx.rand.range(self.props.branch_spawn_amount_range) as i32;
            for _ in 0..count {
                let slot = other.particle_index;
                let saved = ctx.me;
                ctx.me = target;
                other.spawn_particle(ctx, slot, age, 0, 3, at);
                ctx.me = saved;
                if self.props.linkup_lifetime {
                    let (t, m) = (
                        self.emitter.particles[idx].time,
                        self.emitter.particles[idx].max_lifetime,
                    );
                    if let Some(q) = other.particles.get_mut(slot) {
                        q.time = t;
                        q.max_lifetime = m;
                    }
                }
                other.particle_index = (other.particle_index + 1) % other.max_active_particles;
                other.active_particles = other.active_particles.max(other.particle_index + 1);
            }
        }
        ctx.others[target] = Some(other);
    }

    pub fn spawn_particle(
        &mut self,
        ctx: &mut Ctx<'_>,
        index: usize,
        age: f32,
        flags_arg: i32,
        mode: i32,
        offset: Vec3,
    ) {
        self.emitter
            .spawn_particle(ctx, index, age, flags_arg, mode, offset);
        self.drain_spawns(ctx);
    }

    /// Finish `UBeamEmitter::SpawnParticle` for every particle the base emitter spawned since
    /// the last call (its spawn log).
    fn drain_spawns(&mut self, ctx: &mut Ctx<'_>) {
        for (index, age, snapshot) in std::mem::take(&mut self.emitter.spawn_log) {
            if index < self.emitter.particles.len() {
                self.finish_spawn(ctx, index, age, &snapshot);
                self.spawned.push((index, age));
            }
        }
    }

    fn finish_spawn(
        &mut self,
        ctx: &mut Ctx<'_>,
        index: usize,
        age: f32,
        snapshot: &crate::particle::Particle,
    ) {
        let bp = self.props.clone();
        let loc = snapshot.location;
        // Step 1.
        let mut pick: Option<usize> = None;
        if matches!(
            bp.determine_end_point_by,
            EndPointBy::Offset
                | EndPointBy::Actor
                | EndPointBy::TraceOffset
                | EndPointBy::OffsetAsAbsolute
        ) {
            if self.beam_value_sum != 0.0 {
                let r = ctx.rand.unit() * self.beam_value_sum;
                let mut acc = 0.0;
                for (i, ep) in bp.end_points.iter().enumerate() {
                    if acc < r && r <= acc + ep.weight {
                        pick = Some(i);
                        break;
                    }
                    acc += ep.weight;
                }
            }
            if pick.is_none() && !bp.end_points.is_empty() {
                pick = Some(0);
            }
        }
        // Step 3.
        let (mut a, mut b, mut use_b) = (Vec3::ZERO, Vec3::ZERO, false);
        let mut local = snapshot.clone();
        let p = &mut local;
        match bp.determine_end_point_by {
            EndPointBy::Velocity => a = p.velocity * p.max_lifetime,
            EndPointBy::Distance => {
                p.velocity = safe_normal(p.velocity);
                let len = ctx.rand.range(bp.beam_distance_range);
                a = p.velocity * len;
            }
            EndPointBy::DynamicDistance => {
                p.velocity = safe_normal(p.velocity);
                let len = ctx.rand.range(bp.beam_distance_range);
                a = p.velocity * len * p.size.y;
            }
            EndPointBy::Offset | EndPointBy::OffsetAsAbsolute | EndPointBy::TraceOffset => {
                use_b = true;
                if let Some(e) = pick {
                    let o = ctx.rand.vec_yxz(bp.end_points[e].offset);
                    match bp.determine_end_point_by {
                        EndPointBy::Offset => {
                            a = o;
                            b = loc + o;
                        }
                        EndPointBy::OffsetAsAbsolute => {
                            b = o;
                            a = o - loc;
                        }
                        _ => {
                            a = o;
                            b = loc + o;
                            if let Some(hit) = ctx.world.line_check(loc, b, false) {
                                b = hit.location;
                                a = b - loc;
                            }
                        }
                    }
                }
            }
            EndPointBy::Actor => {
                use_b = true;
                if let Some(e) = pick {
                    let tag = bp.end_points[e].actor_tag;
                    if let Some(target) = ctx.world.actor_location(tag) {
                        a = target - loc;
                        b = target;
                        if bp.trigger_endpoint {
                            ctx.events.push(Event::TriggerActor { tag });
                        }
                    }
                }
            }
            EndPointBy::RopeSegment => {}
        }
        self.emitter.particles[index].velocity = local.velocity;
        if bp.noise_determines_end_point {
            use_b = false;
        }
        self.generate_points(ctx, index, loc, a, b, use_b, age, true);
    }

    /// `UpdateBeamLength`: rebuild the points of every live particle (DynamicDistance).
    fn update_beam_length(&mut self, ctx: &mut Ctx<'_>) {
        let active = self
            .emitter
            .active_particles
            .min(self.emitter.particles.len());
        for idx in 0..active {
            if self.emitter.particles[idx].flags & flags::ALIVE == 0 {
                continue;
            }
            let len = ctx.rand.range(self.props.beam_distance_range);
            let p = &mut self.emitter.particles[idx];
            p.velocity = safe_normal(p.velocity);
            let a = p.velocity * len * p.size.y;
            let loc = p.location;
            self.generate_points(ctx, idx, loc, a, Vec3::ZERO, false, 0.0, false);
        }
    }

    pub fn update_particles(&mut self, ctx: &mut Ctx<'_>, dt: f32) -> usize {
        self.beam_value_sum = self.props.end_points.iter().map(|e| e.weight).sum();
        if self.props.branch_emitter >= 0 {
            self.props.branch_emitter = self.props.branch_emitter.min(ctx.others.len() as i32 - 1);
        }
        let ret = self.emitter.update_particles(ctx, dt);
        self.drain_spawns(ctx);
        self.time_since_last_dynamic_noise += dt;
        if self.props.determine_end_point_by == EndPointBy::DynamicDistance {
            self.update_beam_length(ctx);
        }
        // Bounding box.
        let bp = &self.props;
        let hf = bp.high_frequency_points;
        self.emitter.bounding_box = Default::default();
        let active = self
            .emitter
            .active_particles
            .min(self.emitter.particles.len());
        for idx in 0..active {
            if self.emitter.particles[idx].flags & flags::ALIVE != 0 {
                self.emitter
                    .bounding_box
                    .add(self.hf_points[idx * hf].location);
                self.emitter
                    .bounding_box
                    .add(self.hf_points[idx * hf + hf - 1].location);
            }
        }
        let width = |r: RangeVector| {
            (r.x.max - r.x.min)
                .abs()
                .max((r.y.max - r.y.min).abs())
                .max((r.z.max - r.z.min).abs())
        };
        let size = self
            .emitter
            .props
            .start_size_range
            .x
            .min
            .max(self.emitter.props.start_size_range.x.max);
        let m = size
            * self.emitter.max_size_scale
            * (width(bp.high_frequency_noise_range) + width(bp.low_frequency_noise_range));
        self.emitter.bounding_box.expand_by(m);
        ret
    }

    /// `RenderParticles`. `camera` is in the emitter's own space (local for a relative
    /// emitter); `local_to_world` is the transform the vertices are submitted under.
    pub fn render(
        &mut self,
        ctx: &mut Ctx<'_>,
        camera: Vec3,
        local_to_world: Option<Mat4>,
    ) -> Option<IndexedBatch> {
        let bp = &self.props;
        let e = &mut self.emitter;
        let active = e.active_particles.min(e.particles.len());
        for p in &mut e.particles[..active] {
            if p.flags & flags::ALIVE != 0 {
                p.num_frames_rendered = p.num_frames_rendered.saturating_add(1);
            }
        }
        e.inactive = false;
        let hf = bp.high_frequency_points;
        let sheets = self.sheets_used;
        let per_particle = (hf * sheets * 2) as u32;
        let mut vertices: Vec<SpriteVertex> = Vec::new();
        let mut indices = Vec::new();
        let mut drawn = 0u32;
        for idx in 0..active {
            if e.particles[idx].flags & flags::ALIVE == 0 {
                continue;
            }
            let color = pack_color(e.particles[idx].color);
            let size_x = e.particles[idx].size.x;
            let mut ticks = 1i32;
            let interval = ctx.rand.range(bp.dynamic_time_between_noise_range);
            if interval != 0.0 {
                ticks = (self.time_since_last_dynamic_noise / interval) as i32;
                self.time_since_last_dynamic_noise %= interval;
            }
            vertices.resize(
                (drawn * per_particle) as usize + (hf * sheets * 2),
                SpriteVertex {
                    position: Vec3::ZERO,
                    color: 0,
                    uv: [0.0; 2],
                    uv2: [0.0; 2],
                },
            );
            for sheet in 0..sheets {
                let hp = idx * hf;
                let mut prev = 2.0 * self.hf_points[hp].location - self.hf_points[hp + 1].location;
                for j in 0..hf {
                    if ticks != 0
                        && bp.dynamic_hf_noise_points_range.min < j as f32
                        && (j as f32) < bp.dynamic_hf_noise_points_range.max
                    {
                        let d = ctx.rand.vec_yxz(bp.dynamic_hf_noise_range);
                        self.hf_points[hp + j].location += d * ticks as f32;
                    }
                    let pt = self.hf_points[hp + j];
                    let mut tangent = pt.location - prev;
                    let mut w = safe_normal(tangent.cross(prev - camera));
                    if sheet != 0 {
                        tangent = safe_normal(tangent);
                        w = rotate_angle_axis(w, (0x8000 / sheets as i32) * sheet as i32, tangent);
                    }
                    w *= size_x;
                    let u = bp.texture_u_scale * pt.t;
                    let v0 = (drawn as usize) * per_particle as usize + (hf * sheet + j) * 2;
                    vertices[v0] = SpriteVertex {
                        position: pt.location + w,
                        color,
                        uv: [u, 0.0],
                        uv2: [u, 0.0],
                    };
                    vertices[v0 + 1] = SpriteVertex {
                        position: pt.location - w,
                        color,
                        uv: [u, bp.texture_v_scale],
                        uv2: [u, bp.texture_v_scale],
                    };
                    prev = pt.location;
                }
                for k in 0..hf - 1 {
                    let v = per_particle * drawn + ((hf * sheet + k) * 2) as u32;
                    indices.extend_from_slice(&[v, v + 1, v + 2, v + 1, v + 3, v + 2]);
                }
            }
            drawn += 1;
        }
        if drawn == 0 {
            return None;
        }
        vertices.truncate((drawn * per_particle) as usize);
        Some(IndexedBatch {
            vertices,
            indices,
            draw: draw_state(
                &e.props,
                e.props.blend_between_subdivisions && !e.props.use_random_subdivision,
                local_to_world,
            ),
        })
    }
}
