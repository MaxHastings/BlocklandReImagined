//! Player graphics options. Each reads v20's own option pref where v20 had
//! one, so the authored Graphics options drive the modern renderer.
use bri_render::{reflection::ReflectionSettings, scene::TextureFiltering, shadow::ShadowSettings};
use bri_ui::{api::Settings, prefs::Prefs};
use std::collections::BTreeMap;

/// Native anti-aliasing pref (v20 had none): world-pass MSAA on or off.
pub const ANTI_ALIASING: &str = "$pref::Video::AntiAliasing";
/// Native pref: bricks cast sun shadows too (default off). v20's projected
/// shape shadows came from players, vehicles and items, never bricks.
pub const BRICK_SHADOWS: &str = "$pref::Video::BrickShadows";
pub use bri_ui::screens::options::{LIGHTING, REFLECTIONS, RENDER_SCALE, SOFT_SHADING};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Graphics {
    pub filtering: TextureFiltering,
    /// World pass samples per pixel: 4 (MSAA) or 1. WebGPU guarantees 4x
    /// for the swapchain formats and depth.
    pub samples: u32,
    /// Sun shadows from v20's Shadow Quality radios (`$pref::ShadowQuality`,
    /// 0 Best .. 4 Minimum, v20 default 0). Minimum turns them off.
    pub shadows: Option<ShadowSettings>,
    pub brick_shadows: bool,
    /// Mirrors an Add-On's bricks carry.
    pub reflections: ReflectionSettings,
    /// Native `$pref::Video::Lighting`: 0 Classic (v20's look: baked maps,
    /// sun-lit bricks), 2 Unified (default: bricks and maps share the map's
    /// recovered lights, sun and shadows, with highlights; see
    /// `bri_render::map_lighting`), 3 Dynamic (live illumination of current
    /// geometry, with no legacy lightmap/visibility/residual shading).
    /// A saved 1 (Unified without highlights) reads as 2.
    pub lighting: u8,
    /// Native `$pref::Video::RenderScale`: the world draws at this percent
    /// of the window's width and height (100: every pixel), stretched over
    /// it; the interface always draws at full size.
    pub render_scale: u32,
    /// Native `$pref::Video::SoftShading` (on unless turned off): sky-tinted
    /// ambient and ambient occlusion, in Unified and Dynamic lighting only.
    pub soft_shading: bool,
}
pub fn reflection_settings(level: i64) -> ReflectionSettings {
    match level {
        ..=0 => ReflectionSettings::OFF,
        1 => ReflectionSettings::LOW,
        2 => ReflectionSettings::MEDIUM,
        _ => ReflectionSettings::HIGH,
    }
}
pub fn shadow_settings(level: i64) -> Option<ShadowSettings> {
    match level {
        ..=0 => Some(ShadowSettings::BEST),
        1 => Some(ShadowSettings::HIGH),
        2 => Some(ShadowSettings::MEDIUM),
        3 => Some(ShadowSettings::LOW),
        _ => None,
    }
}
impl Graphics {
    /// Shadow resources follow the mode that can shade the accepted source,
    /// while the stored preference remains the user's requested selection.
    pub fn with_lighting(self, lighting: u8) -> Self {
        Self {
            lighting,
            shadows: self.shadows.map(|s| ShadowSettings {
                light_cubes: lighting == 3,
                ..s
            }),
            ..self
        }
    }
    pub fn from_settings(settings: &Settings) -> Self {
        let prefs = Prefs::new(&BTreeMap::new(), &settings.prefs);
        let default = TextureFiltering::default();
        let anisotropy = prefs
            .str_or("$pref::OpenGL::anisotropy", "")
            .trim()
            .parse::<f32>()
            .unwrap_or((f32::from(default.anisotropy) - 1.0) / 15.0);
        let shadows = shadow_settings(prefs.i64_or("$pref::ShadowQuality", 0));
        // Shadow quality never changes the lighting model. Dynamic with
        // shadows disabled is the same live model, explicitly unshadowed.
        let lighting = bri_ui::screens::options::lighting(&prefs) as u8;
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
            shadows: shadows.map(|s| ShadowSettings {
                light_cubes: lighting == 3,
                ..s
            }),
            brick_shadows: prefs.bool_or(BRICK_SHADOWS, false),
            reflections: reflection_settings(bri_ui::screens::options::reflections(&prefs)),
            lighting,
            render_scale: bri_ui::screens::options::render_scale(&prefs),
            soft_shading: prefs.bool_or(SOFT_SHADING, true),
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
        assert_eq!(graphics(&[]).shadows, Some(ShadowSettings::BEST));
        assert_eq!(
            graphics(&[("$pref::ShadowQuality", "3")]).shadows,
            Some(ShadowSettings::LOW)
        );
        assert_eq!(graphics(&[("$pref::ShadowQuality", "4")]).shadows, None);
        // Dynamic lighting keeps a light cube per map light.
        let dynamic = graphics(&[(LIGHTING, "3")]);
        assert_eq!(
            (dynamic.lighting, dynamic.shadows.map(|s| s.light_cubes)),
            (3, Some(true))
        );
        assert_eq!(graphics(&[]).shadows.map(|s| s.light_cubes), Some(false));
        assert_eq!(
            graphics(&[(LIGHTING, "3"), ("$pref::ShadowQuality", "4")]).lighting,
            3
        );
        // The old Unified without highlights is Unified.
        assert_eq!(graphics(&[(LIGHTING, "1")]).lighting, 2);
        // Render Scale: every pixel unless chosen, never below a quarter.
        assert_eq!(graphics(&[]).render_scale, 100);
        assert_eq!(graphics(&[(RENDER_SCALE, "70")]).render_scale, 70);
        assert_eq!(graphics(&[(RENDER_SCALE, "5")]).render_scale, 25);
        assert_eq!(graphics(&[(RENDER_SCALE, "400")]).render_scale, 100);
        assert!(!graphics(&[]).brick_shadows);
        assert!(graphics(&[(BRICK_SHADOWS, "1")]).brick_shadows);
        assert_eq!(graphics(&[]).reflections, ReflectionSettings::MEDIUM);
        assert_eq!(
            graphics(&[(REFLECTIONS, "0")]).reflections,
            ReflectionSettings::OFF
        );
        assert_eq!(
            graphics(&[(REFLECTIONS, "7")]).reflections,
            ReflectionSettings::HIGH
        );
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
