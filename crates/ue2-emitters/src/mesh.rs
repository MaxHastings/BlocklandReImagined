//! `UMeshEmitter` (Engine.dll 0x174890..0x176000, see `specs/spark-mesh-rope-emitters.md`):
//! one static mesh drawn per live particle under that particle's transform.
use crate::emitter::{Ctx, Emitter, OwnerState};
use crate::particle::flags;
use crate::props::{CoordinateSystem, ParticleEmitterProps};
use crate::random::Rand;
use crate::ribbon::{draw_state, pack_color};
use crate::rotation::{Rotator, rotation_matrix};
use crate::sprite::SpriteDraw;
use glam::{Mat4, Vec3};

#[derive(Clone, Debug, Default)]
pub struct MeshProps {
    pub use_mesh_blend_mode: bool,
    pub render_two_sided: bool,
    pub use_particle_color: bool,
    pub mesh_extent: Vec3,
    /// Read from the binary, but its matrix is discarded there (see the spec): it only
    /// suppresses the spin rotation here.
    pub use_absolute_rotation: bool,
}

/// One mesh draw: the transform to submit it under and the global colour dword.
#[derive(Clone, Copy, Debug)]
pub struct MeshInstance {
    pub local_to_world: Mat4,
    pub color: u32,
}

#[derive(Clone, Debug)]
pub struct MeshInstances {
    pub instances: Vec<MeshInstance>,
    pub use_mesh_blend_mode: bool,
    pub use_particle_color: bool,
    /// Particle material state for the mesh-blend-off path (`two_sided` is the emitter's
    /// `RenderTwoSided`; the colour comes from the texture factor).
    pub draw: SpriteDraw,
}

#[derive(Clone, Debug)]
pub struct Mesh {
    pub emitter: Emitter,
    pub props: MeshProps,
}

impl Mesh {
    /// `bounds` is the static mesh's bounding box (min, max), when it has a mesh.
    pub fn new(
        base: ParticleEmitterProps,
        mut props: MeshProps,
        bounds: Option<(Vec3, Vec3)>,
        count: usize,
        rand: &mut Rand,
    ) -> Self {
        let mut emitter = Emitter::new(base, count, rand);
        if let Some((min, max)) = bounds {
            props.mesh_extent = min.max(max) - (min + max) * 0.5;
        }
        emitter.real_extent_multiplier = props.mesh_extent * emitter.props.extent_multiplier;
        Self { emitter, props }
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
    }

    pub fn update_particles(&mut self, ctx: &mut Ctx<'_>, dt: f32) -> usize {
        let ret = self.emitter.update_particles(ctx, dt);
        let r = &self.emitter.props.start_size_range;
        let m = if self.emitter.props.uniform_size {
            r.x.min.max(r.x.max)
        } else {
            r.x.min
                .max(r.x.max)
                .max(r.y.min.max(r.y.max))
                .max(r.z.min.max(r.z.max))
        };
        let q = m * self.emitter.max_size_scale;
        if self.emitter.bounding_box.valid {
            let ext = self.props.mesh_extent;
            self.emitter
                .bounding_box
                .expand_by(2.0 * ext.x.max(ext.y).max(ext.z) * q);
            self.emitter
                .bounding_box
                .expand_by(ctx.owner.pre_pivot.length() * q);
        }
        ret
    }

    /// Every live particle's transform, as `RenderParticles` builds it (the original draws
    /// the mesh's sections once per entry).
    pub fn render(&mut self, owner: &OwnerState) -> Option<MeshInstances> {
        let e = &mut self.emitter;
        let active = e.active_particles.min(e.particles.len());
        let relative = e.props.coordinate_system == CoordinateSystem::Relative;
        let mut instances = Vec::new();
        for idx in 0..active {
            let p = &mut e.particles[idx];
            if p.flags & flags::ALIVE == 0 {
                continue;
            }
            p.num_frames_rendered = p.num_frames_rendered.saturating_add(1);
            // The original multiplies row-vector matrices `D * S * [Rot] * T(loc) [* Rot(owner)
            // * T(owner)]`, applied left to right; the same order is applied here.
            let mut chain = vec![
                Mat4::from_translation(-owner.pre_pivot),
                Mat4::from_scale(p.size),
            ];
            if e.props.spin_particles && !self.props.use_absolute_rotation {
                let rot = Rotator::new(
                    (p.spins_per_second.y * p.time + p.start_spin.y) as i32,
                    (p.spins_per_second.x * p.time + p.start_spin.x) as i32,
                    (p.spins_per_second.z * p.time + p.start_spin.z) as i32,
                );
                chain.push(Mat4::from_mat3(rotation_matrix(rot)));
            }
            chain.push(Mat4::from_translation(p.location));
            if relative {
                chain.push(Mat4::from_mat3(rotation_matrix(owner.rotation)));
                chain.push(Mat4::from_translation(owner.location));
            }
            let m = chain
                .into_iter()
                .fold(Mat4::IDENTITY, |acc, step| step * acc);
            instances.push(MeshInstance {
                local_to_world: m,
                color: pack_color(p.color),
            });
        }
        e.inactive = false;
        if instances.is_empty() {
            return None;
        }
        let mut draw = draw_state(&e.props, false, None);
        draw.two_sided = self.props.render_two_sided;
        Some(MeshInstances {
            instances,
            use_mesh_blend_mode: self.props.use_mesh_blend_mode,
            use_particle_color: self.props.use_particle_color,
            draw,
        })
    }
}
