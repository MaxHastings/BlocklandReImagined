//! A plasma shell glows in its facing colour where the surface faces the eye
//! and in its edge colour where it turns away, at the strength the vertex
//! colour's alpha gives; no strength adds nothing.
#[path = "unified_lighting/common.rs"]
mod common;
use anyhow::Result;
use bri_render::scene::*;
use glam::Vec3;

fn sphere(strength: f32) -> SceneData {
    let mut material = Material::surface("plasma", 0, 0);
    material.kind = MaterialKind::Plasma;
    material.alpha = AlphaMode::Additive;
    material.images = [0; 13];
    material.images[1] = 0;
    material.parameters = Some([
        // Facing: orange. Edge: blue (brightness multiplied in); the w of each is the depth scroll.
        [1.0, 0.545, 0.243, 0.0],
        [0.067, 0.067, 1.0, 0.0],
        [1.0, 0.0, 0.0, 1.0],
        [1.0, 0.0, 0.0, 1.0],
    ]);
    let (rings, segments) = (24usize, 32usize);
    let mut vertices = vec![];
    for r in 0..=rings {
        let phi = std::f32::consts::PI * r as f32 / rings as f32;
        for s in 0..=segments {
            let theta = std::f32::consts::TAU * s as f32 / segments as f32;
            let n = Vec3::new(phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin());
            vertices.push(SceneVertex {
                position: (n * 2.0).to_array(),
                normal: n.to_array(),
                uv: [s as f32 / segments as f32, r as f32 / rings as f32],
                lightmap_uv: [0.; 2],
                color: [1., 1., 1., strength],
                fx: [0.; 4],
            });
        }
    }
    let mut indices = vec![];
    for r in 0..rings {
        for s in 0..segments {
            let a = (r * (segments + 1) + s) as u32;
            let b = a + segments as u32 + 1;
            indices.extend([a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    let count = indices.len() as u32;
    SceneData {
        images: vec![SceneImage {
            label: "noise".into(),
            width: 1,
            height: 1,
            rgba: vec![255; 4],
            srgb: false,
        }],
        materials: vec![material],
        vertices,
        indices,
        batches: vec![MeshBatch {
            indices: 0..count,
            material: 0,
            center: [0.; 3],
        }],
        ..Default::default()
    }
}

#[test]
fn the_shell_runs_from_its_facing_colour_to_its_edge_colour() -> Result<()> {
    let (device, queue) = common::gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut renderer = SceneRenderer::new(&device, format);
    let target = common::color_target(&device, format, 64, 64);
    for modern in [false, true] {
        let mut camera = Camera::perspective([0., 0., 8.], [0.; 3], 1., 0.7, 0.05, 100.);
        if modern {
            camera.shading[0] = 1.;
            camera.ambient[3] = 1.;
        }
        renderer.update_camera(&queue, &camera);
        let mut image = |strength: f32| -> Result<Vec<u8>> {
            let scene = renderer.upload(&device, &queue, &sphere(strength))?;
            common::render(&device, &queue, &mut renderer, &target, &[&scene], &[], &[])
        };
        let full = image(1.0)?;
        let none = image(0.0)?;
        let pixel = |p: &[u8], x: usize, y: usize| p[(y * 64 + x) * 4..][..3].to_vec();
        let middle = pixel(&full, 32, 32);
        // Find the sphere's edge along the middle row: the last lit pixel.
        let edge_x = (0..64)
            .rev()
            .find(|&x| pixel(&full, x, 32).iter().any(|c| *c > 8))
            .expect("the sphere shows");
        let rim = pixel(&full, edge_x, 32);
        let label = format!("modern={modern}");
        assert!(
            middle[0] > middle[2] + 40,
            "{label}: the middle is orange: {middle:?}"
        );
        assert!(rim[2] > rim[0] + 40, "{label}: the rim is blue: {rim:?}");
        assert!(
            pixel(&none, 32, 32).iter().all(|c| *c < 6),
            "{label}: no strength adds nothing: {:?}",
            pixel(&none, 32, 32)
        );
    }
    Ok(())
}
