//! A lit moving body whose vertex colour already holds the room's light
//! (`MaterialKind::ActorLit` on a `vertex_light` material, a map's mover):
//! the baked light stands, and live lights and the glow add on top of it. A
//! map lamp that only moving bodies take is left out, as its light is baked.

#[path = "unified_lighting/common.rs"]
mod common;

use anyhow::Result;
use bri_render::scene::*;
use common::*;

/// A grey 20x20 floor at y = 0 whose vertices hold the light `baked`, drawn
/// as an actor-lit material with `baked_flag` and `glow`.
fn floor(baked: f32, baked_flag: f32, glow: f32) -> SceneData {
    let mut data = SceneData {
        images: vec![SceneImage {
            label: "grey".into(),
            width: 1,
            height: 1,
            rgba: vec![128, 128, 128, 255],
            srgb: false,
        }],
        ..Default::default()
    };
    let mut material = Material::vertex_lit("mover", 0);
    material.kind = MaterialKind::ActorLit;
    material.parameters = Some([[1.0, 1.0, baked_flag, glow], [0.0; 4], [0.0; 4], [0.0; 4]]);
    data.materials.push(material);
    for (x, z) in [(-10.0, -10.0), (-10.0, 10.0), (10.0, 10.0), (10.0, -10.0)] {
        data.vertices.push(SceneVertex {
            position: [x, 0.0, z],
            normal: [0.0, 1.0, 0.0],
            uv: [0.5; 2],
            lightmap_uv: [0.0; 2],
            color: [baked, baked, baked, 1.0],
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

/// The middle pixel's green, with `lights` over a floor lit as given.
fn middle(data: &SceneData, lights: &[PointLight]) -> Result<i32> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = color_target(&device, format, 64, 64);
    let mut renderer = SceneRenderer::new(&device, format);
    let scene = renderer.upload(&device, &queue, data)?;
    let mut camera = Camera::perspective([0., 22., 0.1], [0.; 3], 1.0, 1.0, 0.05, 400.0);
    camera.sun_color = [0.; 4];
    camera.ambient = [0., 0., 0., 2.0];
    renderer.update_camera(&queue, &camera);
    renderer.update_lights(&queue, lights)?;
    let pixels = render(&device, &queue, &mut renderer, &target, &[&scene], &[], &[])?;
    Ok(i32::from(pixels[(32 * 64 + 32) * 4 + 1]))
}

fn lamp(w: f32) -> PointLight {
    PointLight {
        position_radius: [0.0, 3.0, 0.0, 30.0],
        color: [1.0, 1.0, 1.0, w],
        cone: [0.0; 4],
    }
}

#[test]
fn live_light_and_glow_add_to_the_baked_light() -> Result<()> {
    let baked = middle(&floor(0.5, 1.0, 0.0), &[])?;
    // Neutral baked light (0.5) shows the picture.
    assert!(baked > 60, "baked alone: {baked}");
    let lit = middle(&floor(0.5, 1.0, 0.0), &[lamp(0.0)])?;
    assert!(lit > baked + 20, "a live light adds: {lit} over {baked}");
    let glowing = middle(&floor(0.5, 1.0, 0.3), &[])?;
    assert!(glowing > baked + 20, "glow adds: {glowing} over {baked}");
    let map_lamp = middle(&floor(0.5, 1.0, 0.0), &[lamp(ACTOR_LAMP)])?;
    assert_eq!(map_lamp, baked, "a lamp the bake holds adds nothing");
    Ok(())
}

#[test]
fn without_the_baked_flag_the_body_takes_only_what_it_is_lit_by() -> Result<()> {
    let dark = middle(&floor(0.5, 0.0, 0.0), &[])?;
    assert!(dark < 20, "unlit by anything: {dark}");
    let map_lamp = middle(&floor(0.5, 0.0, 0.0), &[lamp(ACTOR_LAMP)])?;
    assert!(
        map_lamp > dark + 20,
        "the lamp lights a free body: {map_lamp}"
    );
    Ok(())
}
