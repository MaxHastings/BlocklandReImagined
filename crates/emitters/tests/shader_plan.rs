use bri_emitters::draw::BlendFactor::*;
use bri_emitters::shader_plan::*;
use glam::{Mat3, Vec3};

fn shader() -> ShaderProps {
    ShaderProps {
        specular: false,
        specularity_mask: MaskSource::None,
        modulate_specular_2x: false,
        opacity: Opacity::None,
        self_illumination: false,
        self_illumination_mask: false,
        detail: None,
        output_blending: OutputBlend::Normal,
        two_sided: false,
    }
}

#[test]
fn env_map_types_choose_the_lookup_axes() {
    assert_eq!(env_source(0), EnvSource::World);
    assert_eq!(env_source(1), EnvSource::Camera);
    let world = Mat3::from_diagonal(Vec3::new(1.0, -1.0, 1.0));
    let view = Mat3::from_cols(Vec3::Y, Vec3::Z, Vec3::X);
    assert_eq!(env_direction(EnvSource::World, world, view), world);
    assert_eq!(env_direction(EnvSource::Camera, world, view), view);
}

#[test]
fn specular_adds_the_masked_environment_or_modulates_it() {
    let mut s = shader();
    s.specular = true;
    s.specularity_mask = MaskSource::BitmapAlpha;
    let p = plan(&s, true, true);
    assert_eq!(
        p.specular,
        Some(SpecularPass {
            blend: SpecularBlend::Add,
            mask: MaskSource::BitmapAlpha
        })
    );
    // ModulateSpecular2X ignores the mask.
    s.modulate_specular_2x = true;
    let p = plan(&s, true, true);
    assert_eq!(
        p.specular,
        Some(SpecularPass {
            blend: SpecularBlend::Modulate2X,
            mask: MaskSource::None
        })
    );
    assert_eq!(plan(&shader(), true, true).specular, None);
}

#[test]
fn detail_needs_no_opacity_and_the_device_settings() {
    let mut s = shader();
    s.detail = Some(8.0);
    assert_eq!(plan(&s, true, true).detail_scale, Some(8.0));
    assert_eq!(plan(&s, false, true).detail_scale, None);
    assert_eq!(plan(&s, true, false).detail_scale, None);
    s.opacity = Opacity::BitmapAlpha;
    assert_eq!(plan(&s, true, true).detail_scale, None);
}

#[test]
fn self_illumination_replaces_or_blends_by_its_mask() {
    let mut s = shader();
    assert_eq!(
        plan(&s, true, true).self_illumination,
        SelfIllumination::None
    );
    s.self_illumination = true;
    assert_eq!(
        plan(&s, true, true).self_illumination,
        SelfIllumination::Replace
    );
    s.self_illumination_mask = true;
    assert_eq!(
        plan(&s, true, true).self_illumination,
        SelfIllumination::Masked
    );
}

#[test]
fn output_blending_modes_pick_their_frame_buffer_factors() {
    let blend = |b| output_blend_state(OutputBlend::from_byte(b)).blend;
    assert_eq!(blend(0), None);
    assert_eq!(blend(2), Some((DstColor, SrcColor)));
    assert_eq!(blend(3), Some((One, One)));
    assert_eq!(blend(4), Some((Zero, One)));
    assert_eq!(blend(5), Some((One, InvSrcColor)));
    assert_eq!(blend(6), Some((Zero, InvSrcColor)));
    let mut s = shader();
    s.output_blending = OutputBlend::Masked;
    assert!((plan(&s, true, true).alpha_ref.unwrap() - 127.0 / 255.0).abs() < 1e-6);
}
