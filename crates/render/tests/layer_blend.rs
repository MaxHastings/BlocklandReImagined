//! LayerBlend: four layers weighted by one RGBA map, each moved by its own affine, times twice the vertex light.
#[path = "unified_lighting/common.rs"]
mod common;
use anyhow::Result;
use bri_render::scene::*;

fn pixel(weights: [u8; 4], vertex: f32, transforms: Option<[[f32; 6]; 4]>) -> Result<[u8; 3]> {
    passes(&[(0., AlphaMode::Opaque, weights, vertex)], transforms)
}

/// Planes at the given heights, drawn in order, each a LayerBlend pass: (height, alpha mode, weights, vertex light).
fn passes(
    list: &[(f32, AlphaMode, [u8; 4], f32)],
    transforms: Option<[[f32; 6]; 4]>,
) -> Result<[u8; 3]> {
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
    ];
    images.push(SceneImage::white());
    let mut materials = vec![];
    let mut vertices = vec![];
    let mut indices = vec![];
    let mut batches = vec![];
    for (k, &(y, alpha, weights, vertex)) in list.iter().enumerate() {
        images.push(SceneImage {
            srgb: false,
            ..solid("weights", weights)
        });
        let mut material = Material::surface("layers", 0, 4);
        material.kind = MaterialKind::LayerBlend;
        material.alpha = alpha;
        material.clamp_nearest = true;
        material.images = [0, 1, 2, 3, 4, 4, 4, 4, 4, 5 + k, 4, 4, 4];
        material.layer_transforms = transforms;
        materials.push(material);
        let base = vertices.len() as u32;
        vertices.extend(
            [
                [-10., y, -10.],
                [-10., y, 10.],
                [10., y, 10.],
                [10., y, -10.],
            ]
            .map(|position| SceneVertex {
                position,
                normal: [0., 1., 0.],
                uv: [0.25; 2],
                lightmap_uv: [0.5; 2],
                color: [vertex, vertex, vertex, 1.],
                fx: [0.; 4],
            }),
        );
        let start = indices.len() as u32;
        indices.extend([0, 1, 2, 0, 2, 3].map(|i| base + i));
        batches.push(MeshBatch {
            zone: None,
            indices: start..start + 6,
            material: k,
            center: [0., y, 0.],
        });
    }
    let data = SceneData {
        images,
        materials,
        vertices,
        indices,
        batches,
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

#[test]
fn extra_passes_add_and_single_layer_passes_blend_over() -> Result<()> {
    let identity = [[1., 0., 0., 1., 0., 0.]; 4];
    let add = AlphaMode::Frame(FrameBlend::TrueAdditive);
    // First pass red at light 0.25 (128); an additive pass above at light 0.125 adds 64.
    let [r, g, b] = passes(
        &[
            (0., AlphaMode::Opaque, [255, 0, 0, 0], 0.25),
            (0.5, add, [255, 0, 0, 0], 0.125),
        ],
        Some(identity),
    )?;
    assert!(
        (r as i32 - 192).abs() <= 3 && g <= 2 && b <= 2,
        "{r} {g} {b}"
    );
    // A one-layer pass whose weight is 0.5 blends over the pass behind it: 0.5 red 128 + 0.5 green 128.
    let [r, g, _] = passes(
        &[
            (0., AlphaMode::Opaque, [255, 0, 0, 0], 0.25),
            (0.5, AlphaMode::Blend, [0, 128, 0, 0], 0.25),
        ],
        Some(identity),
    )?;
    assert!(
        (r as i32 - 64).abs() <= 4 && (g as i32 - 32).abs() <= 4,
        "{r} {g}"
    );
    Ok(())
}
