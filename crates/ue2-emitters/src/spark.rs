//! `USparkEmitter` (Engine.dll 0x2287e0..0x229000, see `specs/spark-mesh-rope-emitters.md`):
//! each particle is drawn as a polyline traced analytically back along its path.
use crate::emitter::{Ctx, Emitter};
use crate::particle::flags;
use crate::props::{ParticleEmitterProps, Range};
use crate::random::Rand;
use crate::ribbon::{LineBatch, draw_state, pack_color};
use crate::sprite::SpriteVertex;
use glam::{Mat4, Vec3};

#[derive(Clone, Debug)]
pub struct SparkProps {
    pub line_segments_range: Range,
    pub time_before_visible_range: Range,
    pub time_between_segments_range: Range,
}

impl Default for SparkProps {
    fn default() -> Self {
        Self {
            line_segments_range: Range::fixed(2.0),
            time_before_visible_range: Range::fixed(0.0),
            time_between_segments_range: Range::fixed(0.05),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SparkData {
    pub time_before_visible: f32,
    pub time_between_segments: f32,
    pub start_location: Vec3,
    pub start_velocity: Vec3,
}

#[derive(Clone, Debug)]
pub struct Spark {
    pub emitter: Emitter,
    pub props: SparkProps,
    pub num_segments: usize,
    pub data: Vec<SparkData>,
}

impl Spark {
    pub fn new(
        base: ParticleEmitterProps,
        props: SparkProps,
        count: usize,
        rand: &mut Rand,
    ) -> Self {
        let n = props
            .line_segments_range
            .min
            .max(props.line_segments_range.max)
            .max(2.0) as usize;
        Self {
            emitter: {
                let mut e = Emitter::new(base, count, rand);
                e.log_spawns = true;
                e
            },
            num_segments: n,
            data: vec![SparkData::default(); count],
            props,
        }
    }

    /// Finish what `USparkEmitter::SpawnParticle` adds after the base spawn, for every particle the
    /// base emitter spawned since the last call (its spawn log).
    fn drain_spawns(&mut self, ctx: &mut Ctx<'_>) {
        for (index, _, p) in std::mem::take(&mut self.emitter.spawn_log) {
            if index >= self.data.len() {
                continue;
            }
            let before = ctx.rand.range(self.props.time_before_visible_range);
            let between = ctx.rand.range(self.props.time_between_segments_range);
            self.data[index] = SparkData {
                time_before_visible: before,
                time_between_segments: between,
                start_location: p.location,
                start_velocity: p.velocity,
            };
        }
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

    pub fn update_particles(&mut self, ctx: &mut Ctx<'_>, dt: f32) -> usize {
        let ret = self.emitter.update_particles(ctx, dt);
        self.drain_spawns(ctx);
        if self.emitter.bounding_box.valid {
            let m = self.emitter.max_size_scale;
            self.emitter.bounding_box.expand_by(m);
        }
        ret
    }

    /// The line list: per live particle, `num_segments` segments, texture `u` 0..1 across a
    /// segment and `v = k / n`. `local_to_world` is set for a relative emitter.
    pub fn render(&mut self, local_to_world: Option<Mat4>) -> Option<LineBatch> {
        let e = &mut self.emitter;
        let active = e.active_particles.min(e.particles.len());
        for p in &mut e.particles[..active] {
            if p.flags & flags::ALIVE != 0 {
                p.num_frames_rendered = p.num_frames_rendered.saturating_add(1);
            }
        }
        e.inactive = false;
        let n = self.num_segments;
        let step = 1.0 / n as f32;
        let accel = e.props.acceleration;
        let mut vertices = Vec::new();
        for idx in 0..active {
            let p = &e.particles[idx];
            if p.flags & flags::ALIVE == 0 {
                continue;
            }
            let s = &self.data[idx];
            let color = pack_color(p.color);
            // No 0.5 factor on the acceleration term: the code multiplies t * A by t.
            let pos = |t: f32| s.start_location + s.start_velocity * t + accel * (t * t);
            for k in 0..n {
                let ta = (p.time - k as f32 * s.time_between_segments).max(0.0);
                let tb = (p.time - (k + 1) as f32 * s.time_between_segments).max(0.0);
                let v = k as f32 * step;
                vertices.push(SpriteVertex {
                    position: pos(ta),
                    color,
                    uv: [0.0, v],
                    uv2: [0.0, v],
                });
                vertices.push(SpriteVertex {
                    position: pos(tb),
                    color,
                    uv: [1.0, v],
                    uv2: [1.0, v],
                });
            }
        }
        if vertices.is_empty() {
            return None;
        }
        Some(LineBatch {
            vertices,
            draw: draw_state(&e.props, false, local_to_world),
        })
    }
}
