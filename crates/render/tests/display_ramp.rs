//! The display ramp (Brightness, Contrast, Gamma of Unreal Engine 2 games)
//! changes the frame as D3DDrv's gamma table does, and leaves it alone at
//! the neutral values. One test: the ramp is process-wide state.
#[path = "unified_lighting/common.rs"]
mod common;
use anyhow::Result;
use bri_console::Clamp;
use bri_render::color::{RAMP_NEUTRAL, set_display_ramp};
use bri_render::scene::*;

const PICTURE: [u8; 3] = [180, 120, 60];
/// How far a rendered channel may sit from the formula, in 8-bit steps.
const TOLERANCE: f32 = 3.0;

fn plane() -> SceneData {
    let mut material = Material::surface("ramp-probe", 0, 0);
    material.kind = MaterialKind::Unlit;
    SceneData {
        images: vec![SceneImage {
            label: "ramp-probe".into(),
            width: 1,
            height: 1,
            rgba: vec![PICTURE[0], PICTURE[1], PICTURE[2], 255],
            srgb: true,
        }],
        materials: vec![material],
        vertices: [
            [-10., 0., -10.],
            [-10., 0., 10.],
            [10., 0., 10.],
            [10., 0., -10.],
        ]
        .map(|position| SceneVertex {
            position,
            normal: [0., 1., 0.],
            uv: [0.5; 2],
            lightmap_uv: [0.; 2],
            color: [1.; 4],
            fx: [0.; 4],
        })
        .to_vec(),
        indices: vec![0, 1, 2, 0, 2, 3],
        batches: vec![MeshBatch {
            indices: 0..6,
            material: 0,
            center: [0., 0., 0.],
        }],
        ..Default::default()
    }
}

/// The table `UD3DRenderDevice::UpdateGamma` builds, as a fraction of full
/// scale, for an input channel `c` in 0..=1.
fn reference(c: f32, [b, k, g]: [f32; 3]) -> f32 {
    let x = c.powf(1.0 / (g - 0.2));
    (x * (k + 0.5) + (b - k) * 0.5).clamped(0.0, 1.0)
}

fn center_pixel(ramp: [f32; 3]) -> Result<[u8; 3]> {
    set_display_ramp(ramp);
    let (device, queue) = common::gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut renderer = SceneRenderer::new(&device, format);
    let target = common::color_target(&device, format, 64, 64);
    let base = renderer.upload(&device, &queue, &plane())?;
    let camera = Camera::perspective([0., 3., 0.01], [0.; 3], 1., 1., 0.05, 100.);
    renderer.update_camera(&queue, &camera);
    let pixels = common::render(&device, &queue, &mut renderer, &target, &[&base], &[], &[])?;
    let at = (32 * 64 + 32) * 4;
    Ok([pixels[at], pixels[at + 1], pixels[at + 2]])
}

#[test]
fn display_ramp_follows_the_gamma_table_and_is_neutral_by_default() -> Result<()> {
    let neutral = center_pixel(RAMP_NEUTRAL)?;
    for (got, want) in neutral.iter().zip(PICTURE) {
        assert!(
            (f32::from(*got) - f32::from(want)).abs() <= TOLERANCE,
            "neutral ramp changed the picture: {neutral:?}"
        );
    }
    // Army Operations' System/Default.ini: Brightness 0.8, Contrast 0.7,
    // Gamma 1.0.
    let ramp = [0.8, 0.7, 1.0];
    let got = center_pixel(ramp)?;
    for ((got, source), channel) in got.iter().zip(neutral).zip(0..) {
        let want = reference(f32::from(source) / 255.0, ramp) * 255.0;
        assert!(
            (f32::from(*got) - want).abs() <= TOLERANCE,
            "channel {channel}: got {got}, the table gives {want}"
        );
    }
    set_display_ramp(RAMP_NEUTRAL);
    Ok(())
}
