//! A batch in a map zone the viewer cannot see into is not drawn.
#[path = "unified_lighting/common.rs"]
mod common;
use anyhow::Result;
use bri_content::zones::{Zone, ZoneGraph};
use bri_render::scene::*;

/// Two unlit quads side by side: red in zone 0 (x < 0), green in zone 1.
fn two_rooms() -> SceneData {
    let mut data = SceneData::default();
    let mut material = Material::surface("flat", 0, 0);
    material.kind = MaterialKind::Unlit;
    data.materials = vec![material];
    data.images = vec![SceneImage {
        label: "white".into(),
        width: 1,
        height: 1,
        rgba: vec![255; 4],
        srgb: true,
    }];
    for (zone, x, color) in [(0, -4., [1., 0., 0., 1.]), (1, 0., [0., 1., 0., 1.])] {
        let base = data.vertices.len() as u32;
        for (dx, dz) in [(0., -4.), (0., 4.), (4., 4.), (4., -4.)] {
            data.vertices.push(SceneVertex {
                position: [x + dx, 0., dz],
                normal: [0., 1., 0.],
                uv: [0.5; 2],
                lightmap_uv: [0.; 2],
                color,
                fx: [0.; 4],
            });
        }
        let start = data.indices.len() as u32;
        data.indices.extend([0, 1, 2, 0, 2, 3].map(|i| base + i));
        data.batches.push(MeshBatch {
            indices: start..data.indices.len() as u32,
            material: 0,
            center: [x + 2., 0., 0.],
            zone: Some(zone),
        });
    }
    data
}

#[test]
fn a_zone_out_of_sight_is_not_drawn() -> Result<()> {
    let (device, queue) = common::gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut renderer = SceneRenderer::new(&device, format);
    let target = common::color_target(&device, format, 64, 64);
    let scene = renderer.upload(&device, &queue, &two_rooms())?;
    let camera = Camera::perspective([0., 12., 0.01], [0.; 3], 1., 1., 0.05, 100.);
    renderer.update_camera(&queue, &camera);
    let seen = |renderer: &mut SceneRenderer| -> Result<([u8; 4], [u8; 4])> {
        let pixels = common::render(&device, &queue, renderer, &target, &[&scene], &[], &[])?;
        let at = |x: usize| <[u8; 4]>::try_from(&pixels[(32 * 64 + x) * 4..][..4]).unwrap();
        // Looking straight down, +x runs left on screen.
        Ok((at(44), at(20)))
    };
    let (left, right) = seen(&mut renderer)?;
    assert!(left[0] > 200 && right[1] > 200, "{left:?} {right:?}");

    // Two zones with no opening between them: from zone 0 only zone 0 shows.
    let graph = ZoneGraph {
        zones: vec![Zone::default(); 2],
        portals: Vec::new(),
    };
    let frustum = frustum_planes(glam::Mat4::from_cols_array(&camera.view_projection));
    renderer.set_view_zones(
        0,
        Some(graph.visible(0, glam::Vec3::new(-2., 12., 0.), &frustum)),
    );
    let (left, right) = seen(&mut renderer)?;
    assert!(left[0] > 200, "the eye's own zone stays: {left:?}");
    assert!(right[1] < 100, "the closed-off zone is gone: {right:?}");

    renderer.set_view_zones(0, None);
    let (_, right) = seen(&mut renderer)?;
    assert!(right[1] > 200, "no zone set draws everything: {right:?}");
    Ok(())
}
