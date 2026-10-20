//! The indexed output of the beam and trail emitters: a vertex list, 16-bit-style triangle
//! indices and the same draw state the sprite emitters report.
use crate::props::{Color, ParticleEmitterProps};
use crate::sprite::{SpriteDraw, SpriteVertex};
use glam::Mat4;

#[derive(Clone, Debug)]
pub struct IndexedBatch {
    pub vertices: Vec<SpriteVertex>,
    /// Triangle list.
    pub indices: Vec<u32>,
    pub draw: SpriteDraw,
}

/// D3D colour dword for a particle colour (`A << 24 | R << 16 | G << 8 | B`).
pub fn pack_color(c: Color) -> u32 {
    u32::from(c.a) << 24 | u32::from(c.r) << 16 | u32::from(c.g) << 8 | u32::from(c.b)
}

/// The draw state the beam and trail renderers write (material-state block in the spec).
pub fn draw_state(
    e: &ParticleEmitterProps,
    blend_subdivisions: bool,
    local_to_world: Option<Mat4>,
) -> SpriteDraw {
    SpriteDraw {
        state: e.draw_style.draw_state(),
        blend_subdivisions,
        alpha_test: e.alpha_test,
        alpha_ref: e.alpha_ref,
        z_test: e.z_test,
        z_write: e.z_write,
        two_sided: true,
        accepts_projectors: e.accepts_projectors,
        local_to_world,
    }
}

/// Independent line segments (two vertices each), as the spark emitter draws them.
#[derive(Clone, Debug)]
pub struct LineBatch {
    pub vertices: Vec<SpriteVertex>,
    pub draw: SpriteDraw,
}
