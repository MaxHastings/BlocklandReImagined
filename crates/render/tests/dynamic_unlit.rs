//! Authored-unlit materials retain their textures and opaque shadow participation.
//! Split by behavior so serial software-GPU verification fits the per-binary watchdog.

#[path = "unified_lighting/common.rs"]
mod common;

use anyhow::Result;
use bri_render::{scene::*, shadow::ShadowSettings};
use common::*;
use glam::{Mat4, Vec3};

/// Authored unlit is a material property, independent of model/texture names.
/// The modern loader must retain it without changing the compatibility loader.
#[test]
fn modern_static_unlit_keeps_its_texture_without_lighting_the_housing() -> Result<()> {
    use bri_content::testing::map_bundle::PROP;
    use bri_content::testing::{ScratchDir, material, plain, png, rigid_shape};
    use bri_render::scene_loader;
    let dir = ScratchDir::new("authored-unlit")?;
    let mut maps = bri_render::testing::rooms();
    maps.truncate(1);
    maps[0].props = vec![Vec3::ZERO];
    bri_render::testing::write_bundle(dir.path(), &maps)?;
    let mut bundle: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("bundle.json"))?)?;
    let mut luminous = material("renamed-white-face", "opaque");
    luminous.unlit = true;
    let shape = rigid_shape(
        PROP,
        &[("root", None, [0.; 3])],
        &[
            (0, [-1.5, 0.6, 0.], [0.5; 3], plain(0)),
            (0, [1.5, 0.6, 0.], [0.5; 3], plain(1)),
        ],
        vec![luminous, material("renamed-housing", "opaque")],
    );
    shape.validate()?;
    std::fs::write(
        dir.path().join(bundle["assets"][PROP].as_str().unwrap()),
        serde_json::to_vec(&shape)?,
    )?;
    // Opaque source materials must still ignore texture alpha, even unlit ones.
    std::fs::write(
        dir.path().join("opaque-zero-alpha.png"),
        png(1, 1, |_, _| [255, 255, 255, 0])?,
    )?;
    let bindings = bundle["bindings"].as_array_mut().unwrap();
    let binding = bindings.iter_mut().find(|b| b["asset"] == PROP).unwrap();
    binding["texture"] = "opaque-zero-alpha.png".into();
    let mut housing = binding.clone();
    housing["shape_material"] = 1.into();
    bindings.push(housing);
    std::fs::write(dir.path().join("bundle.json"), serde_json::to_vec(&bundle)?)?;
    let modern = scene_loader::load_map_bundle_dynamic(dir.path(), &maps[0].id)?;
    let compatibility = scene_loader::load_map_bundle(dir.path(), &maps[0].id)?;
    let luminous_index = modern
        .scene
        .materials
        .iter()
        .position(|m| m.name == format!("{PROP}/renamed-white-face"))
        .unwrap();
    let housing_index = modern
        .scene
        .materials
        .iter()
        .position(|m| m.name == format!("{PROP}/renamed-housing"))
        .unwrap();
    assert_eq!(
        modern.scene.materials[luminous_index].kind,
        MaterialKind::Unlit
    );
    assert!(modern.scene.materials[luminous_index].ignore_texture_alpha);
    let legacy_index = compatibility
        .scene
        .materials
        .iter()
        .position(|m| m.name == format!("{PROP}/renamed-white-face"))
        .unwrap();
    assert_eq!(
        compatibility.scene.materials[legacy_index].kind,
        MaterialKind::Surface
    );
    assert_eq!(
        modern.scene.materials[housing_index].kind,
        MaterialKind::VertexLit
    );
    let mut legacy_housing = compatibility
        .scene
        .materials
        .iter()
        .find(|m| m.name == format!("{PROP}/renamed-housing"))
        .unwrap()
        .clone();
    // The compatibility loader also loads lightmaps, so image slots differ.
    let modern_image = &modern.scene.images[modern.scene.materials[housing_index].images[0]];
    let legacy_image = &compatibility.scene.images[legacy_housing.images[0]];
    assert_eq!(modern_image.rgba, legacy_image.rgba);
    assert_eq!(
        (modern_image.width, modern_image.height),
        (legacy_image.width, legacy_image.height)
    );
    legacy_housing.images = modern.scene.materials[housing_index].images;
    assert_eq!(modern.scene.materials[housing_index], legacy_housing);
    let range = modern.shape_indices.values().next().unwrap().clone();
    let mut data = modern.scene;
    data.batches
        .retain(|b| b.indices.start >= range.start && b.indices.end <= range.end);
    let centre = |material: usize| {
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for batch in data.batches.iter().filter(|b| b.material == material) {
            for &index in &data.indices[batch.indices.start as usize..batch.indices.end as usize] {
                let p = Vec3::from(data.vertices[index as usize].position);
                min = min.min(p);
                max = max.max(p);
            }
        }
        (min + max) * 0.5
    };
    let points = [centre(luminous_index), centre(housing_index)];
    let mid = (points[0] + points[1]) * 0.5;
    let mut camera = Camera::perspective(
        (mid + Vec3::new(0., 6., 8.)).to_array(),
        mid.to_array(),
        1.,
        1.1,
        0.05,
        400.,
    );
    camera.sun_color = [0.; 4];
    camera.ambient = [0., 0., 0., 3.];
    let (device, queue) = gpu()?;
    let target = color_target(&device, wgpu::TextureFormat::Rgba8Unorm, 128, 128);
    let mut renderer =
        SceneRenderer::with_settings(&device, wgpu::TextureFormat::Rgba8Unorm, 1, None);
    let scene = renderer.upload(&device, &queue, &data)?;
    renderer.update_camera(&queue, &camera);
    let dark = render(&device, &queue, &mut renderer, &target, &[&scene], &[], &[])?;
    let view = Mat4::from_cols_array(&camera.view_projection);
    let at = |pixels: &[u8], p: Vec3| -> u8 {
        let projected = view.project_point3(p);
        let x = ((projected.x * 0.5 + 0.5) * 128.) as usize;
        let y = ((0.5 - projected.y * 0.5) * 128.) as usize;
        pixels[(y * 128 + x) * 4]
    };
    assert!(
        at(&dark, points[0]) >= 250,
        "authored face must remain visible in the dark"
    );
    assert!(
        at(&dark, points[1]) <= 2,
        "housing must not become luminous"
    );
    // The old loader assignment, with otherwise identical geometry and uniforms,
    // recreates the reported loss of surface visibility in Dynamic.
    let mut old = data.clone();
    old.materials[luminous_index].kind = MaterialKind::Surface;
    let old = renderer.upload(&device, &queue, &old)?;
    let counterfactual = render(&device, &queue, &mut renderer, &target, &[&old], &[], &[])?;
    assert!(at(&counterfactual, points[0]) <= 2);
    camera.ambient = [0.25, 0.25, 0.25, 3.];
    renderer.update_camera(&queue, &camera);
    let lit = render(&device, &queue, &mut renderer, &target, &[&scene], &[], &[])?;
    assert_eq!(at(&lit, points[0]), at(&dark, points[0]));
    assert!(at(&lit, points[1]) > 20 && at(&lit, points[1]) < 200);
    Ok(())
}

/// Illumination-independent faces remain physical map blockers in Dynamic.
/// Shading kind must not change opaque/masked coverage, or make sky, water
/// and blended materials solid to the map's light cubes and sun layer.
#[test]
fn dynamic_unlit_map_blockers_preserve_surface_shadow_coverage() -> Result<()> {
    use bri_render::map_lighting::MapLight;
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = color_target(&device, format, 128, 128);
    let settings = ShadowSettings {
        lamps: 0,
        light_cubes: true,
        ..ShadowSettings::BEST
    };
    let mut native_water = SceneData::default();
    bri_render::water_scene::append(
        &mut native_water,
        &bri_content::water::Water::volume([-4., 2., -4.], [4., 3.3, 4.]),
        [0; 3],
        ([0.5; 4], 8.),
        false,
        |_, _| None,
    )?;
    let water_uniforms = native_water.materials.last().unwrap().parameters;
    let mut camera = Camera::perspective([0., 22., 0.1], [0.; 3], 1., 1.4, 0.05, 400.);
    camera.sun_direction = Vec3::NEG_Y.extend(0.).to_array();
    camera.ambient = [0., 0., 0., 3.];
    let view = Mat4::from_cols_array(&camera.view_projection);
    let at = |pixels: &[u8], p: Vec3| -> i32 {
        let ndc = view.project_point3(p);
        let x = ((ndc.x * 0.5 + 0.5) * 128.) as usize;
        let y = ((0.5 - ndc.y * 0.5) * 128.) as usize;
        i32::from(pixels[(y * 128 + x) * 4 + 1])
    };
    let points = [Vec3::ZERO, Vec3::new(-7., 0., 0.)];
    for sun in [false, true] {
        camera.sun_color = if sun { [0.6, 0.6, 0.6, 0.] } else { [0.; 4] };
        let lights = if sun {
            vec![]
        } else {
            vec![MapLight {
                position: [0., 8., 0.],
                color: [0.8; 3],
                inner: 0.,
                outer: 40.,
                channel: None,
            }]
        };
        let mut expected = None;
        for (kind, alpha, texture_alpha, blocks) in [
            (MaterialKind::Surface, AlphaMode::Opaque, 0, true),
            (MaterialKind::Unlit, AlphaMode::Opaque, 0, true),
            (MaterialKind::Surface, AlphaMode::Mask(0.5), 255, true),
            (MaterialKind::Unlit, AlphaMode::Mask(0.5), 255, true),
            (MaterialKind::Unlit, AlphaMode::Mask(0.5), 0, false),
            (MaterialKind::Unlit, AlphaMode::Blend, 255, false),
            (MaterialKind::Sky, AlphaMode::Opaque, 255, false),
            (MaterialKind::Water, AlphaMode::Opaque, 255, false),
        ] {
            let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(settings));
            renderer.set_dynamic_lights(&device, &queue, &lights)?;
            let floor = renderer.upload(
                &device,
                &queue,
                &cuboid(Vec3::new(-10., -0.3, -10.), Vec3::new(10., 0., 10.)),
            )?;
            let mut data = cuboid(Vec3::new(-4., 3., -4.), Vec3::new(4., 3.3, 4.));
            data.images = vec![SceneImage {
                width: 1,
                height: 1,
                rgba: vec![255, 255, 255, texture_alpha],
                ..SceneImage::white()
            }];
            data.materials[0] = Material::surface("generic map blocker", 0, 0);
            data.materials[0].kind = kind;
            if kind == MaterialKind::Water {
                data.materials[0].parameters = water_uniforms;
            }
            data.materials[0].alpha = alpha;
            data.materials[0].ignore_texture_alpha = alpha == AlphaMode::Opaque;
            let blocker = renderer.upload(&device, &queue, &data)?;
            let mut sample = [0; 2];
            // All six faces are ready before the receipt is sampled.
            for _ in 0..6 {
                renderer.update_camera(&queue, &camera);
                let pixels = render_with_map(
                    &device,
                    &queue,
                    &mut renderer,
                    &target,
                    &[&floor],
                    &[],
                    &[],
                    &[&floor, &blocker],
                )?;
                sample = points.map(|p| at(&pixels, p));
            }
            assert!(sample[1] > 60, "sun={sun}, kind={kind:?}, open={sample:?}");
            if blocks {
                assert!(
                    sample[0] <= 2,
                    "sun={sun}, kind={kind:?}, blocked={sample:?}"
                );
                if let Some(reference) = expected {
                    assert_eq!(sample, reference, "same coverage: sun={sun}, kind={kind:?}");
                } else {
                    expected = Some(sample);
                }
            } else {
                assert!(
                    sample[0] > 60,
                    "sun={sun}, kind={kind:?}, alpha={alpha:?}: {sample:?}"
                );
            }
        }
    }
    Ok(())
}
