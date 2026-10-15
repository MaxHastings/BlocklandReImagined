//! UV motion stays on the texture, with the actual native material shader.
#[path = "unified_lighting/common.rs"]
mod common;
use anyhow::Result;
use bri_render::scene::*;

#[test]
fn authored_texture_rotation_uses_the_render_clock_and_declared_center() -> Result<()> {
    let (device, queue) = common::gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut renderer = SceneRenderer::new(&device, format);
    let target = common::color_target(&device, format, 64, 64);
    let mut material = Material::surface("animated panel", 0, 0);
    material.kind = MaterialKind::Unlit;
    material.clamp_nearest = true;
    material.uv_motion = Some(bri_content::shape::UvMotion {
        center: [0.5; 2],
        rotation_rate_degrees: 360.,
        ..Default::default()
    });
    let data = SceneData {
        images: vec![SceneImage {
            label: "quadrants".into(),
            width: 2,
            height: 2,
            rgba: vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ],
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
            uv: [0.25; 2],
            lightmap_uv: [0.; 2],
            color: [1.; 4],
            fx: [0.; 4],
        })
        .to_vec(),
        indices: vec![0, 1, 2, 0, 2, 3],
        batches: vec![MeshBatch {
            indices: 0..6,
            material: 0,
            center: [0.; 3],
        }],
        ..Default::default()
    };
    let gpu = renderer.upload(&device, &queue, &data)?;
    for modern in [false, true] {
        let mut observed = Vec::new();
        for seconds in [0., 0.25, 1.] {
            let mut camera = Camera::perspective([0., 3., 0.01], [0.; 3], 1., 1., 0.05, 100.);
            camera.atmosphere[2] = seconds;
            if modern {
                camera.shading[0] = 1.;
                camera.ambient[3] = 2.;
            }
            renderer.update_camera(&queue, &camera);
            let pixels =
                common::render(&device, &queue, &mut renderer, &target, &[&gpu], &[], &[])?;
            observed.push(pixels[(32 * 64 + 32) * 4..][..3].to_vec());
        }
        assert!(
            observed[0][0] > 200 && observed[0][1] < 10,
            "{:?}",
            observed
        );
        assert!(
            observed[1][1] > 200 && observed[1][0] < 10,
            "{:?}",
            observed
        );
        assert_eq!(
            observed[0], observed[2],
            "a full rotation restores the panel without changing geometry"
        );
    }
    Ok(())
}
