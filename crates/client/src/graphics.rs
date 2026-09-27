//! Player graphics options. Each reads v20's own option pref where v20 had
//! one, so the authored Graphics options drive the modern renderer.
use bri_render::scene::TextureFiltering;
use bri_ui::{api::Settings, prefs::Prefs};
use std::collections::BTreeMap;

/// Native anti-aliasing pref (v20 had none): world-pass MSAA on or off.
pub const ANTI_ALIASING: &str = "$pref::Video::AntiAliasing";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Graphics {
    pub filtering: TextureFiltering,
    /// World pass samples per pixel: 4 (MSAA) or 1. WebGPU guarantees 4x
    /// for the swapchain formats and depth.
    pub samples: u32,
}
impl Graphics {
    pub fn from_settings(settings: &Settings) -> Self {
        let prefs = Prefs::new(&BTreeMap::new(), &settings.prefs);
        let default = TextureFiltering::default();
        let anisotropy = prefs
            .str_or("$pref::OpenGL::anisotropy", "")
            .trim()
            .parse::<f32>()
            .unwrap_or((f32::from(default.anisotropy) - 1.0) / 15.0);
        Self {
            filtering: TextureFiltering::from_v20(
                prefs.bool_or("$pref::OpenGL::textureTrilinear", default.trilinear),
                prefs.bool_or("$pref::OpenGL::useGLNearest", default.sharp),
                anisotropy,
            ),
            samples: if prefs.bool_or(ANTI_ALIASING, true) {
                4
            } else {
                1
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graphics(prefs: &[(&str, &str)]) -> Graphics {
        let mut settings = Settings::default();
        for (key, value) in prefs {
            settings.prefs.insert((*key).into(), (*value).into());
        }
        Graphics::from_settings(&settings)
    }

    #[test]
    fn v20_filtering_prefs_select_sampler_state() {
        assert_eq!(graphics(&[]).filtering, TextureFiltering::default());
        assert_eq!(graphics(&[]).samples, 4);
        assert_eq!(graphics(&[(ANTI_ALIASING, "0")]).samples, 1);
        let chosen = graphics(&[
            ("$pref::OpenGL::textureTrilinear", "0"),
            ("$pref::OpenGL::useGLNearest", "1"),
            ("$pref::OpenGL::anisotropy", "1"),
        ]);
        assert_eq!(
            chosen.filtering,
            TextureFiltering {
                trilinear: false,
                sharp: true,
                anisotropy: 16
            }
        );
        assert_eq!(
            graphics(&[("$pref::OpenGL::anisotropy", "0")])
                .filtering
                .anisotropy,
            1
        );
        assert_eq!(
            graphics(&[("$pref::OpenGL::anisotropy", "0.3")])
                .filtering
                .anisotropy,
            4
        );
    }
}
