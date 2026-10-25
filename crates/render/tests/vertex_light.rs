//! A mesh with its light baked into its vertices (`MaterialKind::VertexLight`) shows the picture times twice its vertex colour in
//! every lighting mode and with Soft Shading on or off: neither black nor lit by the live sun and ambient on top.
#[path = "unified_lighting/common.rs"]
mod common;

use anyhow::Result;
use bri_render::scene::*;
use common::*;

/// A grey floor at y = 0 whose vertices all carry `light`.
fn floor(light: f32) -> SceneData {
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
    let mut material = Material::vertex_lit("prop", 0);
    material.kind = MaterialKind::VertexLight;
    data.materials.push(material);
    for (x, z) in [(-10.0, -10.0), (-10.0, 10.0), (10.0, 10.0), (10.0, -10.0)] {
        data.vertices.push(SceneVertex {
            position: [x, 0.0, z],
            normal: [0.0, 1.0, 0.0],
            uv: [0.5, 0.5],
            lightmap_uv: [0.0; 2],
            color: [light, light, light, 1.0],
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

/// The centre pixel's green under `mode` (`camera.ambient.w`), Soft Shading `soft` and a live ambient of `ambient`.
fn shown(light: f32, mode: f32, soft: bool, ambient: f32) -> Result<u8> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = color_target(&device, format, 64, 64);
    let settings = bri_render::shadow::ShadowSettings {
        light_cubes: true,
        ..bri_render::shadow::ShadowSettings::BEST
    };
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(settings));
    if mode == 3.0 {
        renderer.set_dynamic_lights(&device, &queue, &[])?;
    }
    let scene = renderer.upload(&device, &queue, &floor(light))?;
    let mut camera = Camera::perspective([0., 22., 0.1], [0.; 3], 1.0, 1.0, 0.05, 400.0);
    camera.sun_direction = [0.0, -1.0, 0.0, 0.0];
    camera.sun_color = [0.8, 0.8, 0.8, 1.0];
    camera.ambient = [ambient, ambient, ambient, mode];
    camera.shading = if soft { [1.0, 0.6, 0.7, 1.0] } else { [0.0; 4] };
    renderer.update_camera(&queue, &camera);
    let pixels = render(&device, &queue, &mut renderer, &target, &[&scene], &[], &[])?;
    Ok(pixels[(32 * 64 + 32) * 4 + 1])
}

#[test]
fn baked_vertex_light_is_the_same_in_every_mode() -> Result<()> {
    let reference = shown(0.5, 0.0, false, 0.4)?;
    // Neutral (0.5) light leaves the picture as it is: not black, not brighter.
    assert!(
        (i32::from(reference) - 200).abs() < 8,
        "neutral {reference}"
    );
    let dark = shown(0.25, 0.0, false, 0.4)?;
    assert!(dark + 40 < reference, "quarter light {dark} of {reference}");
    for mode in [2.0, 3.0] {
        for soft in [false, true] {
            // A much brighter live ambient and the sun change nothing.
            assert_eq!(
                shown(0.25, mode, soft, 0.9)?,
                dark,
                "mode {mode} soft {soft}"
            );
        }
    }
    Ok(())
}
