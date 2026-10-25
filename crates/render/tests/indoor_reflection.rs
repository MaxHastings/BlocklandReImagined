//! A model-lit floor reflects the sky only where the sky is above it: under
//! a roof the unprobed reflection is the room's, not the open sky's.

#[path = "unified_lighting/common.rs"]
mod common;

use anyhow::Result;
use bri_content::shape::Surface;
use bri_render::scene::*;
use common::*;
use glam::Vec3;

/// A grey 20x20 floor at y = 0 facing up, its texture across x (u) and
/// z (v), with `normal` as its one normal-map texel.
fn floor(normal: [u8; 4], surface: &Surface) -> SceneData {
    let mut data = SceneData {
        images: vec![SceneImage {
            label: "grey".into(),
            width: 1,
            height: 1,
            rgba: vec![180, 180, 180, 255],
            srgb: true,
        }],
        ..Default::default()
    };
    data.images.push(SceneImage {
        label: "normal".into(),
        width: 1,
        height: 1,
        rgba: normal.to_vec(),
        srgb: false,
    });
    let mut maps = [1; 7];
    for (k, map) in SurfaceMap::ALL.into_iter().enumerate().skip(1) {
        data.images.push(map.stand_in());
        maps[k] = data.images.len() - 1;
    }
    data.materials
        .push(Material::physical("floor", 0, maps, surface));
    for (x, z) in [(-10.0, -10.0), (-10.0, 10.0), (10.0, 10.0), (10.0, -10.0)] {
        data.vertices.push(SceneVertex {
            position: [x, 0.0, z],
            normal: [0.0, 1.0, 0.0],
            uv: [x / 20.0 + 0.5, z / 20.0 + 0.5],
            lightmap_uv: [0.0; 2],
            color: [1.0; 4],
            fx: [0.0; 4],
        });
    }
    data.indices.extend([0, 1, 2, 0, 2, 3]);
    data.batches.push(MeshBatch {
        indices: 0..6,
        material: 0,
        center: [0.0; 3],
        zone: None,
    });
    data
}

fn surface() -> Surface {
    Surface {
        normal: Some(1),
        normal_scale: 1.0,
        metallic_roughness: None,
        metallic: 0.0,
        roughness: 1.0,
        occlusion: None,
        occlusion_strength: 1.0,
        emissive_map: None,
        detail: None,
        detail_scale: 1.0,
        emissive: [0.0; 3],
        reflection: None,
        lightmap: None,
        lightmap_strength: 1.0,
        lightmap_exact: false,
    }
}

/// The floor's red at the picture's middle, seen at a low angle, with a
/// roof over it or not.
fn middle(covered: bool) -> Result<i32> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = color_target(&device, format, 64, 64);
    let mut renderer = SceneRenderer::new(&device, format);
    renderer.sky_exposure_budget(9);
    let floor = renderer.upload(&device, &queue, &floor([128, 128, 255, 255], &surface()))?;
    let roof = renderer.upload(
        &device,
        &queue,
        &cuboid(Vec3::new(-30., 6., -30.), Vec3::new(30., 7., 30.)),
    )?;
    let mut camera = Camera::perspective([0., 1.5, 14.], [0., 0., 0.], 1., 1.2, 0.05, 400.);
    // Dynamic lighting with the sky's colour a bright lilac.
    camera.ambient = [0.2, 0.2, 0.2, 3.0];
    camera.sun_color = [0.; 4];
    camera.fog_color = [0.8, 0.6, 1.0, 0.0];
    camera.set_sky_ambient(true);
    renderer.update_camera(&queue, &camera);
    let scenes = if covered {
        vec![&floor, &roof]
    } else {
        vec![&floor]
    };
    let mut encoder = device.create_command_encoder(&Default::default());
    renderer.render_view_sky_exposure_with_moving(
        &mut encoder,
        0,
        ShadowCasters {
            scenes: &scenes,
            instances: &[],
        },
        ShadowCasters {
            scenes: &[],
            instances: &[],
        },
    );
    let view = target.create_view(&Default::default());
    let depth = create_depth(&device, 64, 64).create_view(&Default::default());
    renderer.render(
        &mut encoder,
        &view,
        &depth,
        &[&floor],
        Some(wgpu::Color::BLACK),
    );
    let pixels = read_back(&device, &queue, encoder, &target)?;
    Ok(i32::from(pixels[(32 * 64 + 32) * 4]))
}

#[test]
fn a_roof_keeps_the_open_skys_reflection_off_the_floor() -> Result<()> {
    let open = middle(false)?;
    let roofed = middle(true)?;
    assert!(roofed < open, "open {open}, under a roof {roofed}");
    Ok(())
}
