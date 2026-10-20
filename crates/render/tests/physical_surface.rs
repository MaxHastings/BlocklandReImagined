//! A modern model's full look (`MaterialKind::Physical`) on an offscreen
//! adapter: its normal map turns the surface toward or away from the sun,
//! its glow shows in the dark and bare metal reflects instead of painting.

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

/// The middle pixel's red, green and blue.
fn middle(data: &SceneData, camera: &Camera) -> Result<[i32; 3]> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = color_target(&device, format, 64, 64);
    let mut renderer = SceneRenderer::new(&device, format);
    let scene = renderer.upload(&device, &queue, data)?;
    renderer.update_camera(&queue, camera);
    let pixels = render(&device, &queue, &mut renderer, &target, &[&scene], &[], &[])?;
    let at = (32 * 64 + 32) * 4;
    Ok([0, 1, 2].map(|c| i32::from(pixels[at + c])))
}

fn camera() -> Camera {
    let mut camera = Camera::perspective([0., 22., 0.1], [0.; 3], 1.0, 1.0, 0.05, 400.0);
    // A low sun from +x.
    camera.sun_direction = Vec3::new(-1.0, -0.4, 0.0)
        .normalize()
        .extend(0.0)
        .to_array();
    camera.sun_color = [0.8, 0.8, 0.8, 1.0];
    camera.ambient = [0.1, 0.1, 0.1, 1.0];
    camera
}

#[test]
fn the_normal_map_turns_the_surface_to_the_sun() -> Result<()> {
    let flat = middle(&floor([128, 128, 255, 255], &surface()), &camera())?;
    // Tilted toward +u, which runs along +x: toward the sun.
    let toward = middle(&floor([220, 128, 200, 255], &surface()), &camera())?;
    let away = middle(&floor([36, 128, 200, 255], &surface()), &camera())?;
    assert!(
        toward[1] > flat[1] + 20 && away[1] + 10 < flat[1],
        "toward {toward:?}, flat {flat:?}, away {away:?}"
    );
    // Green is up the picture, toward smaller v, which runs along -z: away
    // from a sun in +z.
    let mut from_z = camera();
    from_z.sun_direction = Vec3::new(0.0, -0.4, -1.0)
        .normalize()
        .extend(0.0)
        .to_array();
    let flat = middle(&floor([128, 128, 255, 255], &surface()), &from_z)?;
    let up = middle(&floor([128, 220, 200, 255], &surface()), &from_z)?;
    assert!(up[1] + 10 < flat[1], "up {up:?}, flat {flat:?}");
    Ok(())
}

#[test]
fn glow_shows_in_the_dark_and_metal_reflects() -> Result<()> {
    let mut dark = camera();
    dark.sun_color = [0.0, 0.0, 0.0, 1.0];
    dark.ambient = [0.0, 0.0, 0.0, 1.0];
    let unlit = middle(&floor([128, 128, 255, 255], &surface()), &dark)?;
    let mut glowing = surface();
    glowing.emissive = [1.0, 0.2, 0.0];
    let glow = middle(&floor([128, 128, 255, 255], &glowing), &dark)?;
    assert!(unlit[0] < 30, "dark without glow: {unlit:?}");
    assert!(
        glow[0] > 150 && glow[0] > glow[2] + 100,
        "orange glow: {glow:?}"
    );

    // A painted surface takes the light; polished metal shows its
    // reflection of the sky instead, tinted by its colour.
    let painted = middle(&floor([128, 128, 255, 255], &surface()), &camera())?;
    let mut steel = surface();
    steel.metallic = 1.0;
    steel.roughness = 0.2;
    let metal = middle(&floor([128, 128, 255, 255], &steel), &camera())?;
    assert_ne!(painted, metal, "metal draws differently");
    Ok(())
}

#[test]
fn an_authored_reflection_keeps_its_mask_and_view_angle_colours() -> Result<()> {
    let mut authored = surface();
    authored.reflection = Some(bri_content::shape::SurfaceReflection {
        environment: 0,
        mask: 0,
        perpendicular: [0.8, 0.0, 0.0],
        parallel: [0.0, 0.0, 0.8],
        detail_inverse_mask: false,
    });
    let mut data = floor([128, 128, 255, 255], &authored);
    let environment = data.images.len() - 2;
    let mask = data.images.len() - 1;
    data.images[environment].rgba = vec![255; 4];
    let mut dark = camera();
    dark.sun_color = [0.0; 4];
    dark.ambient = [0.0; 4];
    let face = middle(&data, &dark)?;
    assert!(face[0] > 150 && face[2] < 10, "face: {face:?}");
    dark = Camera::perspective([0., 2., 22.], [0.; 3], 1., 1., 0.05, 400.);
    dark.sun_color = [0.0; 4];
    dark.ambient = [0.0; 4];
    let grazing = middle(&data, &dark)?;
    assert!(grazing[2] > 150 && grazing[0] < 35, "grazing: {grazing:?}");
    data.images[mask].rgba = vec![0, 0, 0, 255];
    let hidden = middle(&data, &dark)?;
    assert!(hidden.iter().all(|v| *v < 10), "masked: {hidden:?}");
    Ok(())
}
