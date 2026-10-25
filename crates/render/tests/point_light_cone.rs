//! A point light with a cone (`PointLight::cone`) lights only the floor inside
//! its beam; without one it lights the floor all round. Both shaders.

#[path = "unified_lighting/common.rs"]
mod common;

use anyhow::Result;
use bri_render::scene::*;
use common::*;
use glam::Vec3;

fn floor() -> SceneData {
    let mut data = SceneData {
        images: vec![SceneImage {
            label: "grey".into(),
            width: 1,
            height: 1,
            rgba: vec![200, 200, 200, 255],
            srgb: true,
        }],
        ..Default::default()
    };
    data.materials.push(Material::vertex_lit("floor", 0));
    for (x, z) in [(-12.0, -12.0), (-12.0, 12.0), (12.0, 12.0), (12.0, -12.0)] {
        data.vertices.push(SceneVertex {
            position: [x, 0.0, z],
            normal: [0.0, 1.0, 0.0],
            uv: [0.5, 0.5],
            lightmap_uv: [0.0; 2],
            color: [1.0; 4],
            fx: [0.0; 4],
        });
    }
    data.indices.extend([0, 1, 2, 0, 2, 3]);
    data.batches.push(MeshBatch {
        zone: None,
        indices: 0..6,
        material: 0,
        center: [0.0; 3],
    });
    data
}

/// The centre pixel and one near the edge of the floor, green channel.
fn lit(cone: [f32; 4], soft: bool) -> Result<(i32, i32)> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = color_target(&device, format, 64, 64);
    let settings = bri_render::shadow::ShadowSettings {
        light_cubes: true,
        ..bri_render::shadow::ShadowSettings::BEST
    };
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(settings));
    renderer.set_dynamic_lights(&device, &queue, &[])?;
    let scene = renderer.upload(&device, &queue, &floor())?;
    let mut camera = Camera::perspective([0., 22., 0.1], [0.; 3], 1.0, 1.0, 0.05, 400.0);
    camera.sun_color = [0.; 4];
    camera.ambient = [0., 0., 0., 3.0];
    camera.shading = if soft { [1.0; 4] } else { [0.0; 4] };
    renderer.update_camera(&queue, &camera);
    renderer.update_lights(
        &queue,
        &[PointLight {
            position_radius: [0., 5., 0., 40.],
            color: [1., 1., 1., 0.],
            cone,
        }],
    )?;
    let pixels = render(&device, &queue, &mut renderer, &target, &[&scene], &[], &[])?;
    let at = |x: usize, y: usize| i32::from(pixels[(y * 64 + x) * 4 + 1]);
    Ok((at(32, 32), at(32, 56)))
}

#[test]
fn the_beam_lights_only_inside_its_cone() -> Result<()> {
    // Straight down, full within 10 degrees, none at 30.
    let beam = spot_words(Vec3::NEG_Y, 10.0, 30.0);
    for soft in [false, true] {
        let (under, far) = lit(beam, soft)?;
        assert!(under > 60 && far < 8, "beam {under} {far} soft {soft}");
        let (under, far) = lit([0.0; 4], soft)?;
        assert!(under > 60 && far > 8, "all round {under} {far} soft {soft}");
    }
    Ok(())
}
