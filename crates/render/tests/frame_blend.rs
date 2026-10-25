//! Unreal Engine 2's frame-buffer blends over a known surface: each mode's factors, and its fade toward the mode's neutral output.
//! The expected values restate the shader's arithmetic and D3DDrv's (source, destination) factors from `docs/modding/materials.md`.
#[path = "unified_lighting/common.rs"]
mod common;
use anyhow::Result;
use bri_render::scene::*;

fn plane(y: f32, rgba: [u8; 4], tint: [f32; 4], alpha: AlphaMode) -> SceneData {
    let mut material = Material::surface("frame-blend", 0, 0);
    material.kind = MaterialKind::Unlit;
    material.alpha = alpha;
    SceneData {
        images: vec![SceneImage {
            label: "pixels".into(),
            width: 1,
            height: 1,
            rgba: rgba.to_vec(),
            srgb: true,
        }],
        materials: vec![material],
        vertices: [
            [-10., y, -10.],
            [-10., y, 10.],
            [10., y, 10.],
            [10., y, -10.],
        ]
        .map(|position| SceneVertex {
            position,
            normal: [0., 1., 0.],
            uv: [0.5; 2],
            lightmap_uv: [0.; 2],
            color: tint,
            fx: [0.; 4],
        })
        .to_vec(),
        indices: vec![0, 1, 2, 0, 2, 3],
        batches: vec![MeshBatch {
            zone: None,
            indices: 0..6,
            material: 0,
            center: [0., y, 0.],
        }],
        ..Default::default()
    }
}

fn linear(display: f32) -> f32 {
    if display <= 0.04045 {
        display / 12.92
    } else {
        ((display + 0.055) / 1.055).powf(2.4)
    }
}
fn display(linear: f32) -> f32 {
    if linear <= 0.0031308 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

/// The result a mode gives one channel: the mark's texel `t` (display, 0..1) times its tint, faded by `a`, over `dest` (linear).
fn expect(mode: FrameBlend, t: f32, tint: f32, a: f32, dest: f32) -> f32 {
    let m = t * tint;
    match mode {
        FrameBlend::Modulate => {
            let src = linear(display(0.5) * (1.0 - a) + m * a);
            (2.0 * src * dest).min(1.0)
        }
        FrameBlend::Darken => dest * (1.0 - linear(m * a)),
        FrameBlend::Screen => {
            let src = linear(m * a);
            src + dest * (1.0 - src)
        }
        FrameBlend::Premultiplied => linear(m * a) + dest * (1.0 - a),
        FrameBlend::Invisible => dest,
        FrameBlend::TrueAdditive => (linear(m * a) + dest).min(1.0),
    }
}

#[test]
fn each_frame_blend_applies_its_factors_and_fades_to_its_neutral_output() -> Result<()> {
    let (device, queue) = common::gpu()?;
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let mut renderer = SceneRenderer::new(&device, format);
    let target = common::color_target(&device, format, 64, 64);
    let ground = [180u8, 120, 60, 255];
    let base = renderer.upload(
        &device,
        &queue,
        &plane(0., ground, [1.; 4], AlphaMode::Opaque),
    )?;
    let camera = Camera::perspective([0., 3., 0.01], [0.; 3], 1., 1., 0.05, 100.);
    renderer.update_camera(&queue, &camera);
    let (texel, tint) = (96u8, 1.5f32);
    for mode in [
        FrameBlend::Modulate,
        FrameBlend::Darken,
        FrameBlend::Screen,
        FrameBlend::Premultiplied,
        FrameBlend::Invisible,
        FrameBlend::TrueAdditive,
    ] {
        for alpha in [1.0f32, 0.5, 0.0] {
            let mark = renderer.upload(
                &device,
                &queue,
                &plane(
                    0.01,
                    [texel, texel, texel, 255],
                    [tint, tint, tint, alpha],
                    AlphaMode::Frame(mode),
                ),
            )?;
            let pixels = common::render(
                &device,
                &queue,
                &mut renderer,
                &target,
                &[&base, &mark],
                &[],
                &[],
            )?;
            let pixel = &pixels[(32 * 64 + 32) * 4..][..4];
            for (actual, ground) in pixel[..3].iter().zip(ground) {
                let want = display(expect(
                    mode,
                    f32::from(texel) / 255.0,
                    tint,
                    alpha,
                    linear(f32::from(ground) / 255.0),
                )) * 255.0;
                assert!(
                    (f32::from(*actual) - want).abs() <= 3.0,
                    "{mode:?} alpha {alpha}: got {pixel:?}, wanted {want:.1} over {ground}"
                );
            }
        }
    }
    Ok(())
}
