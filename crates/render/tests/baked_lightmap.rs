//! A baked lightmap on a physical surface (`Material::baked_lightmap`) stands
//! for the sun and ambient light: Dynamic shows the map's light and shadow as
//! baked, whatever the live sun, and without the flag the live light applies.

#[path = "unified_lighting/common.rs"]
mod common;

use anyhow::Result;
use bri_content::shape::Surface;
use bri_render::scene::*;
use common::*;

/// A grey floor at y = 0 whose lightmap is dark on its left half and bright
/// on its right half (a 2x1 image read with the second UV set).
fn floor(baked: Option<f32>) -> SceneData {
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
    data.images.push(SceneImage {
        label: "lightmap".into(),
        width: 2,
        height: 1,
        rgba: vec![8, 8, 8, 255, 255, 255, 255, 255],
        srgb: false,
    });
    let surface = Surface {
        normal: None,
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
    };
    let mut maps = [0; 7];
    for (k, map) in SurfaceMap::ALL.into_iter().enumerate() {
        data.images.push(map.stand_in());
        maps[k] = data.images.len() - 1;
    }
    let mut material = Material::physical("floor", 0, maps, &surface);
    if let Some(strength) = baked {
        material.images[8] = 1;
        material.baked_lightmap = Some(strength);
    }
    data.materials.push(material);
    for (x, z) in [(-10.0, -10.0), (-10.0, 10.0), (10.0, 10.0), (10.0, -10.0)] {
        data.vertices.push(SceneVertex {
            position: [x, 0.0, z],
            normal: [0.0, 1.0, 0.0],
            uv: [x / 20.0 + 0.5, z / 20.0 + 0.5],
            lightmap_uv: [x / 20.0 + 0.5, 0.5],
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

/// The darker and brighter side pixels' green, in Dynamic lighting.
fn sides(data: &SceneData, ambient: f32, soft: bool) -> Result<(i32, i32)> {
    sides_lit(data, ambient, soft, &[])
}

/// [`sides`] with `lights` in the point light pool.
fn sides_lit(
    data: &SceneData,
    ambient: f32,
    soft: bool,
    lights: &[PointLight],
) -> Result<(i32, i32)> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = color_target(&device, format, 64, 64);
    // The modern shader (shadows on), where Dynamic lighting lives.
    let settings = bri_render::shadow::ShadowSettings {
        light_cubes: true,
        ..bri_render::shadow::ShadowSettings::BEST
    };
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(settings));
    // Dynamic lighting (with no map lights) draws through the modern shader.
    renderer.set_dynamic_lights(&device, &queue, &[])?;
    let scene = renderer.upload(&device, &queue, data)?;
    let mut camera = Camera::perspective([0., 22., 0.1], [0.; 3], 1.0, 1.0, 0.05, 400.0);
    camera.sun_direction = [0.0, -1.0, 0.0, 0.0];
    camera.sun_color = [0.8, 0.8, 0.8, 1.0];
    camera.ambient = [ambient, ambient, ambient, 3.0];
    // Soft Shading on draws through the modern shader, off through the original.
    camera.shading = if soft { [1.0, 1.0, 1.0, 1.0] } else { [0.0; 4] };
    renderer.update_camera(&queue, &camera);
    renderer.update_lights(&queue, lights)?;
    let pixels = render(&device, &queue, &mut renderer, &target, &[&scene], &[], &[])?;
    let at = |x: usize| i32::from(pixels[(32 * 64 + x) * 4 + 1]);
    // (the darker side, the brighter side): which is left depends on the view
    let (a, b) = (at(20), at(44));
    Ok((a.min(b), a.max(b)))
}

#[test]
fn the_baked_light_replaces_the_live_sun_and_ambient() -> Result<()> {
    for soft in [false, true] {
        check(soft)?;
    }
    Ok(())
}

fn check(soft: bool) -> Result<()> {
    let (left, right) = sides(&floor(Some(1.0)), 0.4, soft)?;
    assert!(left < 60 && right > left + 100, "baked {left} {right}");
    // The map's light is the whole light: a brighter live ambient and sun
    // change nothing on it.
    let (left2, right2) = sides(&floor(Some(1.0)), 0.9, soft)?;
    assert_eq!((left, right), (left2, right2));
    // The strength scales it.
    let (_, half) = sides(&floor(Some(0.5)), 0.4, soft)?;
    assert!(half + 40 < right, "half {half} of {right}");
    // Without the flag the live light is even across the floor.
    let (a, b) = sides(&floor(None), 0.4, soft)?;
    assert!((a - b).abs() < 6 && a > 100, "live {a} {b}");
    Ok(())
}

/// A point light over the floor, `w` the kind of light it is.
fn lamp(w: f32) -> PointLight {
    PointLight {
        position_radius: [0.0, 3.0, 0.0, 30.0],
        color: [1.0, 1.0, 1.0, w],
        cone: [0.0; 4],
    }
}

#[test]
fn the_maps_own_lights_add_nothing_to_a_baked_surface() -> Result<()> {
    for soft in [false, true] {
        let baked = sides(&floor(Some(1.0)), 0.4, soft)?;
        // An overflow map light (colour w 1) is in the lightmap already.
        let overflow = sides_lit(&floor(Some(1.0)), 0.4, soft, &[lamp(1.0)])?;
        // (its highlight on the glossy floor aside, a level or two)
        assert!(
            (baked.0 - overflow.0).abs() < 4 && (baked.1 - overflow.1).abs() < 4,
            "overflow light {overflow:?}, baked {baked:?}, soft {soft}"
        );
        // An effect light (w 0) is not, and lights it.
        let effect = sides_lit(&floor(Some(1.0)), 0.4, soft, &[lamp(0.0)])?;
        assert!(
            effect.0 > baked.0 + 20,
            "effect {effect:?}, baked {baked:?}"
        );
    }
    Ok(())
}
