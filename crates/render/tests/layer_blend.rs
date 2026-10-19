//! LayerBlend: four layers weighted by one RGBA map, each moved by its own affine, times twice the vertex light.
#[path = "unified_lighting/common.rs"]
mod common;
use anyhow::Result;
use bri_render::scene::*;

fn pixel(weights: [u8; 4], vertex: f32, transforms: Option<[[f32; 6]; 4]>) -> Result<[u8; 3]> {
    let (device, queue) = common::gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut renderer = SceneRenderer::new(&device, format);
    let target = common::color_target(&device, format, 64, 64);
    let solid = |label: &str, rgba: [u8; 4]| SceneImage {
        label: label.into(),
        width: 1,
        height: 1,
        rgba: rgba.to_vec(),
        srgb: true,
    };
    // Layers: red, green, blue, and a left-red/right-green 2x1 strip; weights map in image 4.
    let mut images = vec![
        solid("red", [255, 0, 0, 255]),
        solid("green", [0, 255, 0, 255]),
        solid("blue", [0, 0, 255, 255]),
        SceneImage {
            label: "strip".into(),
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 255, 0, 255, 0, 255],
            srgb: true,
        },
        SceneImage {
            srgb: false,
            ..solid("weights", weights)
        },
    ];
    images.push(SceneImage::white());
    let mut material = Material::surface("layers", 0, 5);
    material.kind = MaterialKind::LayerBlend;
    material.clamp_nearest = true;
    material.images = [0, 1, 2, 3, 5, 5, 5, 5, 5, 4, 5, 5, 5];
    material.layer_transforms = transforms;
    let data = SceneData {
        images,
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
            lightmap_uv: [0.5; 2],
            color: [vertex, vertex, vertex, 1.],
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
    let camera = Camera::perspective([0., 3., 0.01], [0.; 3], 1., 1., 0.05, 100.);
    renderer.update_camera(&queue, &camera);
    let pixels = common::render(&device, &queue, &mut renderer, &target, &[&gpu], &[], &[])?;
    let p = &pixels[(32 * 64 + 32) * 4..][..3];
    Ok([p[0], p[1], p[2]])
}

#[test]
fn layer_blend_weights_layers_and_doubles_the_vertex_light() -> Result<()> {
    let identity = [[1., 0., 0., 1., 0., 0.]; 4];
    // Half red, half green; vertex light 0.25 doubles to 0.5: (0.25, 0.25, 0) in display space.
    let [r, g, b] = pixel([128, 128, 0, 0], 0.25, Some(identity))?;
    let want = 128. * 0.5;
    assert!(
        (r as f32 - want).abs() <= 2. && (g as f32 - want).abs() <= 2. && b <= 2,
        "{r} {g} {b} want {want}"
    );
    // The weights pick the layer: full blue weight is absent here, full red weight gives pure red at half light.
    let [r, g, b] = pixel([255, 0, 0, 0], 0.25, Some(identity))?;
    let want = 255. * 0.5;
    assert!(
        (r as f32 - want).abs() <= 2. && g <= 2 && b <= 2,
        "{r} {g} {b} want {want}"
    );
    // Light 0.5 doubles to 1.0 and clamps; light above that stays clamped.
    let [r, ..] = pixel([255, 0, 0, 0], 0.75, Some(identity))?;
    assert_eq!(r, 255);
    // The fourth layer is a 2x1 strip: uv 0.25 reads its left (red) texel, a +0.5 shift its right (green).
    let mut shifted = identity;
    shifted[3][4] = 0.5;
    let [r, g, _] = pixel([0, 0, 0, 255], 0.5, Some(identity))?;
    assert!(r > 250 && g < 5, "{r} {g}");
    let [r, g, _] = pixel([0, 0, 0, 255], 0.5, Some(shifted))?;
    assert!(g > 250 && r < 5, "{r} {g}");
    Ok(())
}
