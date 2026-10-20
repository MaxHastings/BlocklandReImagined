//! `UTrailEmitter` (Engine.dll 0x25b6c0..0x25d1e0, see `specs/beam-trail-emitters.md`):
//! a ring of points per particle, a ribbon drawn through them.
use crate::emitter::{Ctx, Emitter};
use crate::particle::flags;
use crate::props::{Color, DrawStyle, ParticleEmitterProps};
use crate::random::Rand;
use crate::ribbon::{IndexedBatch, draw_state, pack_color};
use crate::rotation::{rotate_angle_axis, safe_normal};
use crate::sprite::SpriteVertex;
use glam::Vec3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShadeType {
    None,
    RandomStatic,
    RandomDynamic,
    Linear,
    PointLife,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrailLocation {
    AttachedToParticle,
    FollowEmitter,
}

#[derive(Clone, Debug)]
pub struct TrailProps {
    pub shade_type: ShadeType,
    pub location: TrailLocation,
    pub max_points_per_trail: usize,
    pub distance_threshold: f32,
    pub use_crossed_sheets: bool,
    pub max_trail_twist_angle: i32,
    pub point_life_time: f32,
}

impl TrailProps {
    /// `PostEditChange`'s clamps.
    pub fn clamp(&mut self) {
        self.max_points_per_trail = self.max_points_per_trail.max(3);
        self.point_life_time = self.point_life_time.max(0.0);
        self.distance_threshold = self.distance_threshold.max(2.0);
    }
}

impl Default for TrailProps {
    fn default() -> Self {
        Self {
            shade_type: ShadeType::None,
            location: TrailLocation::AttachedToParticle,
            max_points_per_trail: 20,
            distance_threshold: 20.0,
            use_crossed_sheets: false,
            max_trail_twist_angle: 0,
            point_life_time: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TrailPoint {
    pub location: Vec3,
    pub color: Color,
    pub size: f32,
    pub expiry: f64,
}
impl Default for TrailPoint {
    fn default() -> Self {
        Self {
            location: Vec3::ZERO,
            color: Color::rgba(0, 0, 0, 0),
            size: 0.0,
            expiry: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TrailInfo {
    pub trail_index: usize,
    pub num_points: usize,
    pub last_location: Vec3,
    pub last_emitter_location: Vec3,
}

#[derive(Clone, Debug)]
pub struct Trail {
    pub emitter: Emitter,
    pub props: TrailProps,
    pub data: Vec<TrailPoint>,
    pub info: Vec<TrailInfo>,
}

/// `SetInitialPointColor`: random shading of a new point's colour for the static type.
fn initial_color(tp: &TrailProps, style: DrawStyle, c: Color, rand: &mut Rand) -> Color {
    if tp.shade_type != ShadeType::RandomStatic {
        return c;
    }
    let r = rand.unit();
    let mut out = c;
    if matches!(
        style,
        DrawStyle::AlphaBlend | DrawStyle::Modulated | DrawStyle::AlphaModulateMightNotFogCorrectly
    ) {
        out.a = (f32::from(c.a) * r) as u8;
    } else {
        out.r = (f32::from(c.r) * r) as u8;
        out.g = (f32::from(c.g) * r) as u8;
        out.b = (f32::from(c.b) * r) as u8;
    }
    out
}

impl Trail {
    pub fn new(
        base: ParticleEmitterProps,
        mut props: TrailProps,
        count: usize,
        rand: &mut Rand,
    ) -> Self {
        props.clamp();
        let mp = props.max_points_per_trail;
        Self {
            emitter: {
                let mut e = Emitter::new(base, count, rand);
                e.log_spawns = true;
                e
            },
            data: vec![TrailPoint::default(); mp * count],
            info: vec![TrailInfo::default(); count],
            props,
        }
    }

    /// `ResetTrail`: kill every particle, leave the point data.
    pub fn reset_trail(&mut self) {
        let n = self
            .emitter
            .active_particles
            .min(self.emitter.particles.len());
        for p in &mut self.emitter.particles[..n] {
            p.flags &= !flags::ALIVE;
        }
        self.emitter.inactive = false;
        self.emitter.all_particles_dead = false;
        self.emitter.active_particles = 0;
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
        let Trail {
            emitter,
            props,
            data,
            info,
        } = self;
        drain_spawns(emitter, props, data, info, ctx);
    }

    pub fn update_particles(&mut self, ctx: &mut Ctx<'_>, dt: f32) -> usize {
        let Trail {
            emitter,
            props,
            data,
            info,
        } = self;
        let ret = emitter.update_particles_with(ctx, dt, &mut |e, c, idx, _dt| {
            // A particle's trail starts with its spawn, before the update that follows it.
            drain_spawns(e, props, data, info, c);
            update_particle(e, props, data, info, c, idx)
        });
        drain_spawns(emitter, props, data, info, ctx);
        if emitter.bounding_box.valid {
            let m = emitter.max_size_scale;
            emitter.bounding_box.expand_by(m);
        }
        ret
    }

    /// The ribbon through every live trail, as `RenderParticles` and its vertex/index
    /// callbacks build it (vertices in world space).
    pub fn render(&mut self, camera: Vec3) -> Option<IndexedBatch> {
        let tp = &self.props;
        let e = &mut self.emitter;
        let mp = tp.max_points_per_trail;
        let active = e.active_particles.min(e.particles.len());
        for p in &mut e.particles[..active] {
            if p.flags & flags::ALIVE != 0 {
                p.num_frames_rendered = p.num_frames_rendered.saturating_add(1);
            }
        }
        e.inactive = false;
        let sheets = if tp.use_crossed_sheets { 2 } else { 1 };
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let mut pair = 0u32;
        for idx in 0..active {
            let n = self.info[idx].num_points;
            if e.particles[idx].flags & flags::ALIVE == 0 || n <= 1 {
                continue;
            }
            let head = self.info[idx].trail_index;
            let slot = |k: usize| &self.data[idx * mp + (head + mp - k % mp) % mp];
            for sheet in 0..sheets {
                for k in 0..n {
                    let pk = slot(k).location;
                    let t = if k == 0 {
                        safe_normal(slot(1).location - pk)
                    } else if k == n - 1 {
                        safe_normal(pk - slot(n - 2).location)
                    } else {
                        safe_normal(
                            0.5 * ((pk - slot(k - 1).location) + (slot(k + 1).location - pk)),
                        )
                    };
                    let view = safe_normal(pk - camera);
                    let side = safe_normal(Vec3::new(-t.y, t.x, 0.0));
                    let w = if tp.use_crossed_sheets {
                        if sheet == 0 {
                            side
                        } else {
                            safe_normal(rotate_angle_axis(side, 16384, t))
                        }
                    } else {
                        let angle = (side.dot(view) * tp.max_trail_twist_angle as f32) as i32;
                        safe_normal(rotate_angle_axis(side, angle, t))
                    };
                    let pt = slot(k);
                    let color = pack_color(pt.color);
                    let u = k as f32 / n as f32;
                    vertices.push(SpriteVertex {
                        position: pk + w * pt.size,
                        color,
                        uv: [u, 1.0],
                        uv2: [u, 1.0],
                    });
                    vertices.push(SpriteVertex {
                        position: pk - w * pt.size,
                        color,
                        uv: [u, 0.0],
                        uv2: [u, 0.0],
                    });
                }
                for _ in 0..n - 1 {
                    let v = 2 * pair;
                    indices.extend_from_slice(&[v, v + 1, v + 2, v + 2, v + 1, v + 3]);
                    pair += 1;
                }
                pair += 1;
            }
        }
        if vertices.is_empty() {
            return None;
        }
        Some(IndexedBatch {
            vertices,
            indices,
            draw: draw_state(&e.props, false, None),
        })
    }
}

/// What `UTrailEmitter::SpawnParticle` (0x25b990) adds after the base spawn, for every particle
/// the base emitter spawned since the last call (its spawn log).
fn drain_spawns(
    e: &mut Emitter,
    tp: &TrailProps,
    data: &mut [TrailPoint],
    info: &mut [TrailInfo],
    ctx: &mut Ctx<'_>,
) {
    let mp = tp.max_points_per_trail;
    let style = e.props.draw_style;
    for (index, _, p) in std::mem::take(&mut e.spawn_log) {
        if index >= info.len() {
            continue;
        }
        let inf = &mut info[index];
        if tp.location == TrailLocation::FollowEmitter {
            inf.last_emitter_location = ctx.owner.location;
        }
        inf.trail_index = 1;
        inf.num_points = 2;
        inf.last_location = p.location;
        for k in 0..2 {
            data[index * mp + k] = TrailPoint {
                location: p.location,
                color: initial_color(tp, style, p.color, ctx.rand),
                size: p.size.x,
                expiry: f64::from(ctx.owner.time_seconds) + f64::from(tp.point_life_time),
            };
        }
    }
}

/// `UpdateParticle(dt, index)` (0x25bf80).
fn update_particle(
    e: &mut Emitter,
    tp: &TrailProps,
    data: &mut [TrailPoint],
    info: &mut [TrailInfo],
    ctx: &mut Ctx<'_>,
    idx: usize,
) {
    let mp = tp.max_points_per_trail;
    let base = idx * mp;
    let t = f64::from(ctx.owner.time_seconds);
    let life = f64::from(tp.point_life_time);
    let style = e.props.draw_style;
    let inf = &mut info[idx];
    // 1.
    if tp.location == TrailLocation::FollowEmitter {
        let d = ctx.owner.location - inf.last_emitter_location;
        e.particles[idx].location += d;
        inf.last_emitter_location = ctx.owner.location;
    }
    let l = e.particles[idx].location;
    let (p_color, p_size) = (e.particles[idx].color, e.particles[idx].size.x);
    let mut head = inf.trail_index;
    let mut n = inf.num_points;
    // 2.
    if e.kill_pending && n == 2 {
        let p = &mut e.particles[idx];
        p.time = p.max_lifetime + 1.0;
    }
    // 3.
    if tp.point_life_time > 0.0 {
        while n > 2 {
            let tail = (head + mp + 1 - n) % mp;
            if t > data[base + tail].expiry {
                n -= 1;
            } else {
                break;
            }
        }
    }
    // 4 and 5.
    let last0 = inf.last_location;
    let dist = (last0 - l).length();
    let thr = tp.distance_threshold;
    let fresh = |c: Color, rand: &mut Rand| initial_color(tp, style, c, rand);
    if !e.kill_pending && dist >= thr {
        let inv = 1.0 / dist;
        let delta = l - last0;
        let mut s = thr;
        data[base + head].location = last0 + delta * (inv * s);
        inf.last_location = data[base + head].location;
        s += thr;
        head = (head + 1) % mp;
        n = (n + 1).min(mp);
        data[base + head] = TrailPoint {
            location: last0 + delta * (inv * s),
            color: fresh(p_color, ctx.rand),
            size: p_size,
            expiry: t + life,
        };
        if dist >= s {
            loop {
                head = (head + 1) % mp;
                n = (n + 1).min(mp);
                data[base + head] = TrailPoint {
                    location: last0 + delta * (inv * s),
                    color: fresh(p_color, ctx.rand),
                    size: p_size,
                    expiry: t + life,
                };
                inf.last_location = data[base + head].location;
                s += thr;
                if dist < s {
                    break;
                }
            }
        }
    }
    // 6.
    if !e.kill_pending {
        data[base + head] = TrailPoint {
            location: l,
            color: fresh(p_color, ctx.rand),
            size: p_size,
            expiry: t + life,
        };
    }
    // 7.
    inf.num_points = n;
    inf.trail_index = head;
    // 8.
    for k in 0..n {
        e.bounding_box
            .add(data[base + (head + mp - k) % mp].location);
    }
    // 9.
    if matches!(
        tp.shade_type,
        ShadeType::RandomDynamic | ShadeType::Linear | ShadeType::PointLife
    ) && style != DrawStyle::Regular
    {
        let mut f = 1.0f32;
        for k in 0..n {
            let slot = &mut data[base + (head + mp - k) % mp];
            match tp.shade_type {
                ShadeType::RandomDynamic => f = ctx.rand.unit(),
                ShadeType::Linear => f = 1.0 - (k as f32) / (n as f32),
                _ => {
                    if tp.point_life_time > 0.0 {
                        f = ((slot.expiry - t) / life) as f32;
                    }
                }
            }
            if matches!(
                style,
                DrawStyle::AlphaBlend
                    | DrawStyle::Modulated
                    | DrawStyle::AlphaModulateMightNotFogCorrectly
            ) {
                slot.color.a = (f32::from(p_color.a) * f) as u8;
            } else {
                slot.color.r = (f32::from(p_color.r) * f) as u8;
                slot.color.g = (f32::from(p_color.g) * f) as u8;
                slot.color.b = (f32::from(p_color.b) * f) as u8;
            }
        }
    }
}
