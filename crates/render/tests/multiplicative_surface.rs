//! Authored surface marks modulate the picture underneath, including fading.
#[path = "unified_lighting/common.rs"]
mod common;
use anyhow::Result;
use bri_render::scene::*;

fn plane(y: f32, rgba: [u8; 4], tint: [f32; 4], alpha: AlphaMode) -> SceneData {
    let mut material = Material::surface("authored-mark", 0, 0);
    material.kind = MaterialKind::Unlit;
    material.alpha = alpha;
    SceneData {
        images: vec![SceneImage {
            label: "original-pixels".into(),
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
            indices: 0..6,
            material: 0,
            center: [0., y, 0.],
        }],
        ..Default::default()
    }
}

#[test]
fn multiplication_preserves_texture_colour_alpha_and_fades_to_the_underlying_surface() -> Result<()>
{
    let (device, queue) = common::gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut renderer = SceneRenderer::new(&device, format);
    let target = common::color_target(&device, format, 64, 64);
    let base = renderer.upload(
        &device,
        &queue,
        &plane(0., [180, 120, 60, 255], [1.; 4], AlphaMode::Opaque),
    )?;
    for modern in [false, true] {
        let mut camera = Camera::perspective([0., 3., 0.01], [0.; 3], 1., 1., 0.05, 100.);
        if modern {
            camera.shading[0] = 1.;
            camera.ambient[3] = 1.;
        }
        renderer.update_camera(&queue, &camera);
        for (alpha, weight) in [(1., 0.5), (0.5, 0.75), (0., 1.)] {
            let mark = renderer.upload(
                &device,
                &queue,
                &plane(
                    0.01,
                    [64, 64, 64, 255],
                    [2., 2., 2., alpha],
                    AlphaMode::Multiply,
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
            for (actual, background) in pixel[..3].iter().zip([180., 120., 60.]) {
                assert!(
                    (f32::from(*actual) - background * weight).abs() < 4.,
                    "modern={modern}, alpha={alpha}, pixel={pixel:?}"
                );
            }
            assert_eq!(pixel[3], 255, "marks preserve destination coverage");
        }
    }
    Ok(())
}
