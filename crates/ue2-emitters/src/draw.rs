//! How a particle draw style sets up drawing, read from the original renderer's particle
//! material handler (D3DDrv.dll 0x4020, jump table 0x4a9c). Factors are Direct3D's:
//! the first is applied to the source, the second to the destination.
use crate::props::DrawStyle;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendFactor {
    Zero,
    One,
    SrcColor,
    InvSrcColor,
    SrcAlpha,
    InvSrcAlpha,
    DstColor,
}

/// How texel and vertex colour combine in the particle's one texture stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageOp {
    /// texel x diffuse (or the texture factor), colour and alpha.
    Modulate,
    /// texel x a + other x (1 - a), a = the diffuse alpha, colour and alpha. The
    /// "modulated" style uses this so a fading particle goes to the neutral colour.
    BlendDiffuseAlpha,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawState {
    /// None draws opaque (One, Zero with blending off).
    pub blend: Option<(BlendFactor, BlendFactor)>,
    pub stage: StageOp,
    /// The fog colour for the draw: None keeps the scene's, Some replaces it
    /// (additive and similar draws fade to black, modulated to mid grey).
    pub fog_color: Option<[f32; 3]>,
}

impl DrawStyle {
    pub fn draw_state(self) -> DrawState {
        use BlendFactor::*;
        let black = Some([0.0; 3]);
        match self {
            DrawStyle::Regular => DrawState {
                blend: None,
                stage: StageOp::Modulate,
                fog_color: None,
            },
            DrawStyle::AlphaBlend => DrawState {
                blend: Some((SrcAlpha, InvSrcAlpha)),
                stage: StageOp::Modulate,
                fog_color: None,
            },
            DrawStyle::Modulated => DrawState {
                blend: Some((DstColor, SrcColor)),
                stage: StageOp::BlendDiffuseAlpha,
                fog_color: Some([127.0 / 255.0; 3]),
            },
            DrawStyle::Translucent => DrawState {
                blend: Some((One, One)),
                stage: StageOp::Modulate,
                fog_color: black,
            },
            DrawStyle::AlphaModulateMightNotFogCorrectly => DrawState {
                blend: Some((One, InvSrcAlpha)),
                stage: StageOp::Modulate,
                fog_color: black,
            },
            DrawStyle::Darken => DrawState {
                blend: Some((Zero, InvSrcColor)),
                stage: StageOp::Modulate,
                fog_color: black,
            },
            DrawStyle::Brighten => DrawState {
                blend: Some((One, InvSrcColor)),
                stage: StageOp::Modulate,
                fog_color: black,
            },
        }
    }
}

/// The frame-buffer blend of a `FinalBlend` material (D3DDrv.dll 0x9e20, the switch at
/// 0x9e6f): source and destination factors by `FrameBufferBlending`, and the fog colour the
/// draw fades toward (grey 127 for the two modulating blends, black for the rest).
pub fn final_blend_state(frame_buffer_blending: u8) -> DrawState {
    use BlendFactor::*;
    let grey = Some([127.0 / 255.0; 3]);
    let black = Some([0.0; 3]);
    let (blend, fog_color) = match frame_buffer_blending {
        0 => (None, None),
        1 => (Some((DstColor, SrcColor)), grey),
        2 => (Some((SrcAlpha, InvSrcAlpha)), None),
        3 => (Some((One, InvSrcAlpha)), black),
        4 | 9 => (Some((One, InvSrcColor)), black),
        5 => (Some((Zero, InvSrcColor)), black),
        6 => (Some((SrcAlpha, One)), black),
        7 => (Some((Zero, One)), black),
        8 => (Some((Zero, SrcColor)), grey),
        10 => (Some((One, One)), black),
        _ => (None, None),
    };
    DrawState {
        blend,
        stage: StageOp::Modulate,
        fog_color,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_seven_styles_match_the_original_table() {
        use BlendFactor::*;
        let table = [
            (DrawStyle::Regular, None),
            (DrawStyle::AlphaBlend, Some((SrcAlpha, InvSrcAlpha))),
            (DrawStyle::Modulated, Some((DstColor, SrcColor))),
            (DrawStyle::Translucent, Some((One, One))),
            (
                DrawStyle::AlphaModulateMightNotFogCorrectly,
                Some((One, InvSrcAlpha)),
            ),
            (DrawStyle::Darken, Some((Zero, InvSrcColor))),
            (DrawStyle::Brighten, Some((One, InvSrcColor))),
        ];
        for (style, blend) in table {
            assert_eq!(style.draw_state().blend, blend, "{style:?}");
        }
    }
}
