//! The thin adapter from the UE2 emitter crate's output to the renderer's general
//! dynamic-sprite batches. The renderer knows nothing of UE2; this crate carries the one
//! piece of UE2 policy the draw needs: how a particle draw style, the frame-blend alpha
//! trick and the vertex colour dword map onto a generic batch state.
pub mod projector_bake;
pub mod projector_runtime;
use bri_render::dynamic_sprites::{
    Combine, Factor, Fog, Frames, ImageId, ProjectionMode, SpriteProjection, SpriteVertex, State,
    Topology,
};
use bri_render::projected_textures::{Projection, ProjectionBlend, ProjectorImage, ViewFog};
use bri_render::surface_projections::{BaseBlend, SurfaceImage, SurfaceProjection, SurfaceVertex};
use bri_ue2_emitters::draw::{BlendFactor, StageOp};
use bri_ue2_emitters::particle_projector::{ParticleProjectorInfo, ParticleProjectorMode};
use bri_ue2_emitters::projector::{FrameBufferBlend, ProjectorVolume};
use bri_ue2_emitters::projector_geometry::ReceiverVertex;
use bri_ue2_emitters::sprite::{SpriteDraw, SpriteVertex as EmitterVertex};
use glam::{Mat4, Vec3};

/// The scene's linear distance fog, when it has one.
#[derive(Clone, Copy, Debug)]
pub struct SceneFog {
    pub start: f32,
    pub end: f32,
    pub color: [f32; 3],
}

fn factor(f: BlendFactor) -> Factor {
    match f {
        BlendFactor::Zero => Factor::Zero,
        BlendFactor::One => Factor::One,
        BlendFactor::SrcColor => Factor::SrcColor,
        BlendFactor::InvSrcColor => Factor::InvSrcColor,
        BlendFactor::SrcAlpha => Factor::SrcAlpha,
        BlendFactor::InvSrcAlpha => Factor::InvSrcAlpha,
        BlendFactor::DstColor => Factor::DstColor,
    }
}

/// The batch state a draw describes. `image` is the emitter's texture as the host loaded
/// it, `topology` how its indices read. A draw style that fades to a fixed fog colour
/// replaces the scene fog's colour; the scene's range is kept.
pub fn state(
    draw: &SpriteDraw,
    image: ImageId,
    topology: Topology,
    scene_fog: Option<SceneFog>,
) -> State {
    let mut s = State::new(image);
    s.topology = topology;
    s.blend = draw.state.blend.map(|(a, b)| (factor(a), factor(b)));
    if draw.state.stage == StageOp::BlendDiffuseAlpha {
        s.color = Combine::BlendByVertexAlpha;
        s.alpha = Combine::BlendByVertexAlpha;
    }
    if draw.blend_subdivisions {
        s.frames = Frames::ByVertexAlpha;
    }
    s.depth_test = draw.z_test;
    s.depth_write = draw.z_write;
    s.two_sided = draw.two_sided;
    if draw.alpha_test {
        s.alpha_test = Some(draw.alpha_ref as f32 / 255.0);
    }
    s.fog = scene_fog.map(|f| Fog {
        start: f.start,
        end: f.end,
        color: draw.state.fog_color.unwrap_or(f.color),
    });
    s
}

fn unpack(color: u32) -> [f32; 4] {
    let b = |shift: u32| ((color >> shift) & 0xff) as f32 / 255.0;
    [b(16), b(8), b(0), b(24)]
}

/// The renderer's vertices for the emitter vertices, transformed by `local_to_world` when the
/// draw was submitted under a transform. With frame blending the original's second frame
/// carries the mix weight in the vertex alpha, while the batch mixes the first coordinate by
/// that alpha, so the two coordinate pairs trade places.
pub fn vertices(
    source: &[EmitterVertex],
    local_to_world: Option<Mat4>,
    blend_subdivisions: bool,
) -> Vec<SpriteVertex> {
    source
        .iter()
        .map(|v| {
            let p = match local_to_world {
                Some(m) => m.transform_point3(v.position),
                None => v.position,
            };
            let (uv, uv2) = if blend_subdivisions {
                (v.uv2, v.uv)
            } else {
                (v.uv, v.uv2)
            };
            SpriteVertex {
                position: Vec3::to_array(&p),
                color: unpack(v.color),
                uv,
                uv2,
            }
        })
        .collect()
}

/// The renderer's projection for a projector's computed volume. `gradient` is the depth-fade
/// picture when the projector has one; `project_on_backfaces` switches off the facing fade.
pub fn projection(
    volume: &ProjectorVolume,
    image: ProjectorImage,
    gradient: Option<ProjectorImage>,
    blend: FrameBufferBlend,
    project_on_backfaces: bool,
) -> Projection {
    Projection {
        image,
        world_to_texture: volume.matrix,
        divide: volume.perspective,
        planes: volume.planes.iter().map(|p| p.to_array()).collect(),
        blend: match blend {
            FrameBufferBlend::Modulate2X => ProjectionBlend::Modulate2X,
            FrameBufferBlend::Alpha => ProjectionBlend::Alpha,
            FrameBufferBlend::Add => ProjectionBlend::Add,
        },
        gradient: gradient.zip(volume.gradient),
        normal_fade: (!project_on_backfaces).then_some(volume.forward),
        bounds: Some((volume.box_min, volume.box_max)),
        wrap: [false, false],
        accept: 0,
        surface_alpha: false,
        draw_last: false,
    }
}

/// The projected pictures a particle batch carries. `image_of` finds the host's image for a
/// projector's bitmap; a projector without one is skipped.
pub fn sprite_projections(
    infos: &[ParticleProjectorInfo],
    image_of: &dyn Fn(u32) -> Option<ImageId>,
) -> [Option<SpriteProjection>; 4] {
    let mut out = [None; 4];
    let usable = infos.iter().filter_map(|i| Some((i, image_of(i.bitmap?)?)));
    for (slot, (info, image)) in out.iter_mut().zip(usable) {
        *slot = Some(SpriteProjection {
            image,
            world_to_texture: info.matrix,
            divide: info.perspective,
            mode: match info.mode() {
                ParticleProjectorMode::Modulate => ProjectionMode::Modulate,
                ParticleProjectorMode::Modulate2X => ProjectionMode::Modulate2X,
                ParticleProjectorMode::ByPictureAlpha => ProjectionMode::ByPictureAlpha,
            },
        });
    }
    out
}

/// Projection state for the depth projector from a projector's authored flags:
/// `clamp` is the leaf bitmap's `TC_Clamp` per axis (wrapping otherwise).
pub fn apply_projector_flags(
    projection: &mut Projection,
    clamp_u: bool,
    clamp_v: bool,
    draw_last: bool,
) {
    projection.wrap = [!clamp_u, !clamp_v];
    projection.draw_last = draw_last;
}

/// The flags of a projector's render info and material that choose how a pass draws.
#[derive(Clone, Copy, Debug)]
pub struct ProjectorPass {
    pub blend: FrameBufferBlend,
    /// BaseMaterialBlending (the render info's MaterialBlendingOp): 1 modulates the
    /// surface's own picture in, 2 blends by the picture's alpha, 0 and 3 leave it out.
    pub material_blending: u8,
    pub project_on_alpha: bool,
    pub project_on_backfaces: bool,
    pub gradient: bool,
    pub two_sided: bool,
    pub fog: Option<ViewFog>,
}

/// The pictures a projector pass draws with, as the host loaded them.
#[derive(Clone, Copy, Debug)]
pub struct ProjectorImages {
    pub projected: SurfaceImage,
    pub gradient: Option<SurfaceImage>,
    /// The receiving surface's alpha-masked material, for bProjectOnAlpha.
    pub mask: Option<SurfaceImage>,
    /// The receiving surface's own material, for a nonzero BaseMaterialBlending.
    pub base: Option<SurfaceImage>,
    /// The projected picture's TC_Clamp per axis (the original wraps otherwise).
    pub clamp: [bool; 2],
}

/// The legacy depth bias the original sets for its projection passes (ZBIAS 5), in the
/// renderer's depth units.
pub const PROJECTION_DEPTH_BIAS: i32 = 8;

fn base_blend(op: u8, base: Option<SurfaceImage>) -> Option<(SurfaceImage, BaseBlend)> {
    match (op, base) {
        (1, Some(b)) => Some((b, BaseBlend::Modulate2X)),
        (2, Some(b)) => Some((b, BaseBlend::ByPictureAlpha)),
        _ => None,
    }
}

fn blend(b: FrameBufferBlend) -> ProjectionBlend {
    match b {
        FrameBufferBlend::Modulate2X => ProjectionBlend::Modulate2X,
        FrameBufferBlend::Alpha => ProjectionBlend::Alpha,
        FrameBufferBlend::Add => ProjectionBlend::Add,
    }
}

/// The static path (BSP records, terrain sector lists, bStatic static meshes): the
/// picture's fade comes from the triangles, the mask is used whenever the surface has one,
/// and there is no depth gradient.
pub fn static_surface_projection(
    volume: &ProjectorVolume,
    pass: &ProjectorPass,
    images: &ProjectorImages,
) -> SurfaceProjection {
    SurfaceProjection {
        image: images.projected,
        world_to_texture: volume.matrix,
        divide: volume.perspective,
        blend: blend(pass.blend),
        wrap: [!images.clamp[0], !images.clamp[1]],
        gradient: None,
        mask: images.mask,
        base: base_blend(pass.material_blending, images.base),
        reflection: None,
        normal_fade: None,
        vertex_fade: true,
        two_sided: pass.two_sided,
        depth_bias: PROJECTION_DEPTH_BIAS,
        fog: pass.fog,
    }
}

/// The dynamic path (skeletal and static-mesh sections, the second terrain pass). How far
/// down the fallback chain the pass compiled: 0 is the full program, 1 drops the depth
/// gradient, 2 also drops the facing fade.
pub fn dynamic_surface_projection(
    volume: &ProjectorVolume,
    pass: &ProjectorPass,
    images: &ProjectorImages,
    fallback_level: u8,
) -> SurfaceProjection {
    let gradient = (fallback_level == 0 && pass.gradient)
        .then(|| images.gradient.zip(volume.gradient))
        .flatten();
    let facing = fallback_level < 2 && !pass.project_on_backfaces;
    SurfaceProjection {
        image: images.projected,
        world_to_texture: volume.matrix,
        divide: volume.perspective,
        blend: blend(pass.blend),
        wrap: [!images.clamp[0], !images.clamp[1]],
        gradient,
        mask: if pass.project_on_alpha {
            images.mask
        } else {
            None
        },
        base: base_blend(pass.material_blending, images.base),
        reflection: None,
        normal_fade: facing.then_some(volume.forward),
        vertex_fade: false,
        two_sided: pass.two_sided,
        depth_bias: PROJECTION_DEPTH_BIAS,
        fog: pass.fog,
    }
}

/// Receiver vertices as the pass wants them (the normal is the surface's, +Z when unknown).
pub fn surface_vertices(
    receivers: &[ReceiverVertex],
    normals: Option<&[Vec3]>,
) -> Vec<SurfaceVertex> {
    receivers
        .iter()
        .enumerate()
        .map(|(i, v)| SurfaceVertex {
            position: v.position.to_array(),
            normal: normals
                .and_then(|n| n.get(i))
                .map_or([0.0, 0.0, 1.0], |n| n.to_array()),
            uv: v.uv.to_array(),
            fade: v.fade,
        })
        .collect()
}
