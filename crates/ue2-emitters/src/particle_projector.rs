//! Projectors on particles (`AEmitter::CheckForProjectors`, Engine.dll 0x195100, and the
//! particle material's `ParticleProjectorInfo`, 0x4c bytes; see `specs/projectors-2.md`):
//! a projector with `bProjectParticles` near an emitter hands the emitter's material its
//! picture and matrix; the particle draw then joins that picture to its colour. No gradient,
//! mask or fade is involved.
use glam::Mat4;

/// The most projectors one particle material applies (the stage budget).
pub const MAX_PARTICLE_PROJECTORS: usize = 4;

/// A projector the area query found.
#[derive(Clone, Debug)]
pub struct ProjectorCandidate {
    pub project_particles: bool,
    /// The projector's `ProjectTag` (`None` is "no tag").
    pub project_tag: Option<u32>,
    /// The projected material when it is a bitmap material.
    pub bitmap: Option<u32>,
    /// `Projector.Matrix` (world to picture, the row-vector form as a column matrix).
    pub matrix: Mat4,
    /// `FOV != 0`.
    pub perspective: bool,
    /// `FrameBufferBlendingOp`.
    pub blend_op: u8,
}

/// One slot of the particle material's projector list.
#[derive(Clone, Debug)]
pub struct ParticleProjectorInfo {
    pub bitmap: Option<u32>,
    pub matrix: Mat4,
    pub perspective: bool,
    pub blend_op: u8,
}

/// How the picture joins the particle colour, by `FrameBufferBlendingOp`: 2 by the picture's
/// alpha, 3 modulated doubled, anything else modulated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleProjectorMode {
    Modulate,
    Modulate2X,
    ByPictureAlpha,
}

impl ParticleProjectorInfo {
    pub fn mode(&self) -> ParticleProjectorMode {
        match self.blend_op {
            2 => ParticleProjectorMode::ByPictureAlpha,
            3 => ParticleProjectorMode::Modulate2X,
            _ => ParticleProjectorMode::Modulate,
        }
    }
}

/// `CheckForProjectors`: the projectors (in the order the query returns them) that apply to
/// an emitter tagged `emitter_tag`, capped at four. The emitter must have
/// `UseParticleProjectors` set; `candidates` are those whose volume overlaps the emitter's box.
pub fn select(
    use_particle_projectors: bool,
    emitter_tag: Option<u32>,
    candidates: &[ProjectorCandidate],
) -> Vec<ParticleProjectorInfo> {
    if !use_particle_projectors {
        return Vec::new();
    }
    candidates
        .iter()
        .filter(|c| {
            c.project_particles && (c.project_tag.is_none() || c.project_tag == emitter_tag)
        })
        .take(MAX_PARTICLE_PROJECTORS)
        .map(|c| ParticleProjectorInfo {
            bitmap: c.bitmap,
            matrix: c.matrix,
            perspective: c.perspective,
            blend_op: c.blend_op,
        })
        .collect()
}
