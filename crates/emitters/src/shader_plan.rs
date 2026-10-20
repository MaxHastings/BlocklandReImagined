//! What a `Shader` material asks of the renderer, as D3DDrv's shader handler (0x6510) and the
//! texture-modifier collector (0xc1d0, 0xa9e0) build it; see `specs/envmap-specular.md`.
//! Specular and environment maps become a reflection overlay over the surface's own
//! triangles, detail and self-illumination stay in the surface shading, opacity replaces the
//! surface alpha and the output blend chooses the frame-buffer blend.
use crate::draw::{BlendFactor, DrawState, StageOp};
use glam::Mat3;

/// Where a texture coordinate source takes the environment lookup direction from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvSource {
    /// `TexCoordSource` 10: the view ray reflected off the normal, in Unreal world axes.
    World,
    /// `TexCoordSource` 11: the same reflection in camera space (the lookup turns with the view).
    Camera,
}

/// `UTexEnvMap::GetMatrix`: `EnvMapType` 0 selects the world-space source, 1 (the class
/// default) the camera-space one.
pub fn env_source(env_map_type: u8) -> EnvSource {
    if env_map_type == 0 {
        EnvSource::World
    } else {
        EnvSource::Camera
    }
}

/// The rotation from engine world space into the cube's axes for a reflection lookup.
/// `world_to_unreal` maps the engine's world axes to Unreal's (identity when they are the
/// same); `view_rotation` is the camera's world-to-camera rotation in the cube's camera
/// axes (x right, y up, z forward).
pub fn env_direction(source: EnvSource, world_to_unreal: Mat3, view_rotation: Mat3) -> Mat3 {
    match source {
        EnvSource::World => world_to_unreal,
        EnvSource::Camera => view_rotation,
    }
}

/// Where the specular strength comes from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MaskSource {
    /// No mask: a strength of one.
    None,
    /// The alpha channel of the mask bitmap.
    BitmapAlpha,
    /// The alpha byte of a constant colour.
    ConstantAlpha(u8),
    /// The mask is the diffuse itself, so its alpha is the mask.
    DiffuseAlpha,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OutputBlend {
    Normal,
    Masked,
    Modulate,
    Translucent,
    Invisible,
    Brighten,
    Darken,
}

impl OutputBlend {
    /// `OutputBlending` byte of the shader: its order in `Engine.u`.
    pub fn from_byte(b: u8) -> Self {
        match b {
            1 => Self::Masked,
            2 => Self::Modulate,
            3 => Self::Translucent,
            4 => Self::Invisible,
            5 => Self::Brighten,
            6 => Self::Darken,
            _ => Self::Normal,
        }
    }
}

/// Where the surface alpha comes from when the shader has an opacity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Opacity {
    None,
    /// The opacity bitmap's alpha channel.
    BitmapAlpha,
    /// The constant colour's alpha byte.
    ConstantAlpha(u8),
    /// The vertex colour's alpha.
    VertexAlpha,
}

#[derive(Clone, Debug)]
pub struct ShaderProps {
    pub specular: bool,
    pub specularity_mask: MaskSource,
    pub modulate_specular_2x: bool,
    pub opacity: Opacity,
    pub self_illumination: bool,
    pub self_illumination_mask: bool,
    /// `Shader.Detail` or the diffuse texture's own detail, with its scale (default 8).
    pub detail: Option<f32>,
    pub output_blending: OutputBlend,
    pub two_sided: bool,
}

/// How the specular overlay joins the surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpecularBlend {
    /// `base + mask * environment` (the mask-add stage).
    Add,
    /// `2 * base * environment`, the mask ignored.
    Modulate2X,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpecularPass {
    pub blend: SpecularBlend,
    pub mask: MaskSource,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SelfIllumination {
    None,
    /// Replaces the diffuse entirely and is drawn unlit.
    Replace,
    /// `lerp(lit, unlit, mask.alpha)`.
    Masked,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShaderPlan {
    pub specular: Option<SpecularPass>,
    /// Detail repeats; `rgb *= 2 * detail.rgb` at the base UV times the scale, mip bias -1.
    pub detail_scale: Option<f32>,
    pub self_illumination: SelfIllumination,
    /// The surface alpha is replaced by this opacity (never multiplied in).
    pub opacity: Opacity,
    /// Alpha test reference for a masked output (the original's 0x7f of 255).
    pub alpha_ref: Option<f32>,
    pub blend: DrawState,
    pub two_sided: bool,
}

/// The frame-buffer blend of the output blending mode (pass 0 only; a wrapping FinalBlend
/// overrides it).
pub fn output_blend_state(mode: OutputBlend) -> DrawState {
    use BlendFactor::*;
    let blend = match mode {
        OutputBlend::Normal | OutputBlend::Masked => None,
        OutputBlend::Modulate => Some((DstColor, SrcColor)),
        OutputBlend::Translucent => Some((One, One)),
        OutputBlend::Invisible => Some((Zero, One)),
        OutputBlend::Brighten => Some((One, InvSrcColor)),
        OutputBlend::Darken => Some((Zero, InvSrcColor)),
    };
    DrawState {
        blend,
        stage: StageOp::Modulate,
        fog_color: None,
    }
}

/// Plan a shader. `detail_textures` is the device's DetailTextures setting; `detail_state`
/// the render-state flag that gates detail (its writer is not located, so hosts pass true).
pub fn plan(props: &ShaderProps, detail_textures: bool, detail_state: bool) -> ShaderPlan {
    let specular = if !props.specular {
        None
    } else if props.modulate_specular_2x {
        Some(SpecularPass {
            blend: SpecularBlend::Modulate2X,
            mask: MaskSource::None,
        })
    } else {
        Some(SpecularPass {
            blend: SpecularBlend::Add,
            mask: props.specularity_mask,
        })
    };
    let detail_scale = (props.opacity == Opacity::None && detail_textures && detail_state)
        .then_some(props.detail)
        .flatten();
    let self_illumination = match (props.self_illumination, props.self_illumination_mask) {
        (false, _) => SelfIllumination::None,
        (true, false) => SelfIllumination::Replace,
        (true, true) => SelfIllumination::Masked,
    };
    ShaderPlan {
        specular,
        detail_scale,
        self_illumination,
        opacity: props.opacity,
        alpha_ref: (props.output_blending == OutputBlend::Masked).then_some(127.0 / 255.0),
        blend: output_blend_state(props.output_blending),
        two_sided: props.two_sided,
    }
}
