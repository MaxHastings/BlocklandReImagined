//! Invented native water; no original content or interactive game inputs.
use anyhow::Result;
use bri_content::{
    scene::{Kind, Node, Scene},
    water::Water,
};
use bri_render::{
    scene::*,
    scene_loader::{self, MapScene},
    water_scene::*,
};
use glam::{Mat4, Vec3};
use sha2::{Digest, Sha256};
use std::sync::Arc;
#[path = "unified_lighting/common.rs"]
mod common;

fn fixture(period: Option<f32>) -> Result<(tempfile::TempDir, Water, String)> {
    let dir = tempfile::tempdir()?;
    let maps = &bri_render::testing::rooms()[..1];
    bri_render::testing::write_bundle(dir.path(), maps)?;
    let bundle_path = dir.path().join("bundle.json");
    let mut bundle: serde_json::Value = serde_json::from_slice(&std::fs::read(&bundle_path)?)?;
    let scene_path = dir.path().join(bundle["maps"][0]["file"].as_str().unwrap());
    let mut scene: Scene = serde_json::from_slice(&std::fs::read(&scene_path)?)?;
    let mut water = Water::volume([0., -4., -64.], [64., 0., 0.]);
    water.node = scene.nodes.len();
    water.repeat_period = period;
    scene.nodes.push(Node {
        name: "invented-water".into(),
        parent: None,
        kind: Kind::Water,
        transform: Mat4::IDENTITY.to_cols_array(),
        asset: None,
        properties: Default::default(),
    });
    let bytes = bri_content::testing::png(1, 1, |_, _| [17, 103, 199, 255])?;
    water.surface.sha256 = format!("{:x}", Sha256::digest(&bytes));
    water.shore = water.surface.clone();
    std::fs::write(dir.path().join(&water.surface.file), bytes)?;
    bundle["waters"] = serde_json::json!({ scene.id.clone(): [water.clone()] });
    std::fs::write(&scene_path, serde_json::to_vec(&scene)?)?;
    std::fs::write(bundle_path, serde_json::to_vec(&bundle)?)?;
    Ok((dir, water, scene.id))
}
// Same geometric coverage predicate as the baseline, applied to all selected draws.
fn covers(data: &SceneData, point: [f32; 2]) -> bool {
    data.batches
        .iter()
        .filter(|b| data.materials[b.material].kind == MaterialKind::Water)
        .any(|b| {
            data.indices[b.indices.start as usize..b.indices.end as usize]
                .chunks_exact(3)
                .any(|t| {
                    let p = t
                        .iter()
                        .map(|&i| data.vertices[i as usize].position)
                        .collect::<Vec<_>>();
                    let sign = |a: [f32; 3], b: [f32; 3]| {
                        (point[0] - b[0]) * (a[2] - b[2]) - (a[0] - b[0]) * (point[1] - b[2])
                    };
                    let d = [sign(p[0], p[1]), sign(p[1], p[2]), sign(p[2], p[0])];
                    !(d.iter().any(|&v| v < 0.) && d.iter().any(|&v| v > 0.))
                })
        })
}
fn loaded_covers(loaded: &MapScene, point: [f32; 2]) -> Result<bool> {
    if covers(&loaded.scene, point) {
        return Ok(true);
    }
    for water in &loaded.repeated_water {
        for copy in water.visible_from(&[Vec3::new(point[0], 10., point[1])], 1.)? {
            if covers(&water.copy_scene(copy)?, point) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
#[test]
fn native_loader_water_covers_an_authored_far_repeat() -> Result<()> {
    let (dir, water, id) = fixture(Some(64.))?;
    for loaded in [
        scene_loader::load_map_bundle(dir.path(), &id)?,
        scene_loader::load_map_bundle_dynamic(dir.path(), &id)?,
    ] {
        for [kx, kz] in [[0., 0.], [5., -4.], [-17., 21.], [700., -800.]] {
            let point = [
                water.min[0] + (kx + 0.25) * 64.,
                water.max[2] + (kz - 0.25) * 64.,
            ];
            assert!(water.footprint(point[0], point[1]).is_some());
            assert!(
                loaded_covers(&loaded, point)?,
                "authoritative water footprint at {point:?} has no rendered triangle beyond the fixed 3x3 patch"
            );
        }
        assert!(
            loaded
                .scene
                .materials
                .iter()
                .all(|m| m.kind != MaterialKind::Water)
        );
    }
    Ok(())
}
#[test]
fn finite_native_water_stays_within_its_original_footprint() -> Result<()> {
    let (dir, water, id) = fixture(None)?;
    for loaded in [
        scene_loader::load_map_bundle(dir.path(), &id)?,
        scene_loader::load_map_bundle_dynamic(dir.path(), &id)?,
    ] {
        assert!(loaded.repeated_water.is_empty());
        assert!(loaded_covers(&loaded, [16., -16.])?);
        for point in [[336., -272.], [-336., 272.], [80., -16.]] {
            assert!(water.footprint(point[0], point[1]).is_none());
            assert!(!loaded_covers(&loaded, point)?);
        }
    }
    Ok(())
}
fn plain_water(period: f32) -> Result<WaterScene> {
    let mut water = Water::volume([0., -4., -64.], [64., 0., 0.]);
    water.repeat_period = Some(period);
    WaterScene::new(
        water,
        SceneData::default(),
        [0; 3],
        ([1.; 4], 6.),
        Vec::new(),
    )
}
#[test]
fn gapped_footprints_view_union_and_global_uvs_are_preserved() -> Result<()> {
    let water = plain_water(128.)?;
    let eyes = [
        Vec3::new(16., 10., -16.),
        Vec3::new(16. + 128. * 15., 10., -16. - 128. * 10.),
    ];
    let a = water.visible_from(&eyes[..1], 10.)?;
    let b = water.visible_from(&eyes[1..], 10.)?;
    let both = water.visible_from(&[eyes[0], eyes[1], eyes[0]], 10.)?;
    assert_eq!(both, a.union(&b).copied().collect());
    assert_eq!(both.len(), 2);
    assert!(
        water
            .visible_from(&[Vec3::new(96., 2., -32.)], 10.)?
            .is_empty(),
        "intentional repeat gap"
    );
    for copy in [[0, 0], [15, -10], [-30, 40]] {
        let data = water.copy_scene(copy)?;
        for v in &data.vertices {
            assert!((v.uv[0] - v.lightmap_uv[0] - copy[0] as f32).abs() < 1e-5);
            assert!((v.uv[1] - v.lightmap_uv[1] + copy[1] as f32).abs() < 1e-5);
            assert!((v.position[0] - v.lightmap_uv[0] * 64. - copy[0] as f32 * 128.).abs() < 1e-4);
            assert!((v.position[2] + v.lightmap_uv[1] * 64. - copy[1] as f32 * 128.).abs() < 1e-4);
        }
        assert!(
            data.images
                .last()
                .unwrap()
                .rgba
                .chunks_exact(4)
                .all(|p| p[0] == 64 && p[1] == 0)
        );
    }
    Ok(())
}
fn field(repeat: bool) -> Arc<bri_content::terrain_field::TerrainField> {
    use bri_content::{Terrain, TerrainLayer, terrain_field::*};
    let side = 128;
    let terrain = Terrain {
        schema_version: 1,
        id: "invented-terrain".into(),
        side,
        elevations: vec![-2.; side as usize * side as usize],
        primary_layers: vec![0; side as usize * side as usize],
        layers: vec![TerrainLayer {
            slot: 0,
            material: "m".into(),
            weights: vec![255; side as usize * side as usize],
        }],
    };
    let instance = TerrainInstance {
        schema_version: TERRAIN_INSTANCE_SCHEMA,
        node: 0,
        terrain: terrain.id.clone(),
        square_size: 0.5,
        origin: [0., 0., 0.],
        repeat,
        repeat_source: RepeatSource::Authored,
        empty_runs: vec![[0, 32]],
        detail: None,
        bump: TerrainBump {
            texture: None,
            scale: 1.,
            offset: 0.01,
            zero_scale: 8,
        },
        diagnostics: vec![],
    };
    Arc::new(TerrainField::new(terrain, &instance).unwrap())
}
#[test]
fn depth_masks_sample_each_copy_and_primary_holes_do_not_repeat() -> Result<()> {
    for repeat in [true, false] {
        let mut scene = plain_water(64.)?;
        scene.water.depth_mask = true;
        scene.water.depth_alpha = [1., 1., 0., 0.];
        // Recreate through public inputs, retaining actual terrain queries.
        let water = WaterScene::new(
            scene.water,
            SceneData::default(),
            [0; 3],
            ([1.; 4], 6.),
            vec![field(repeat)],
        )?;
        let primary = water.copy_scene([0, 0])?;
        let distant = water.copy_scene([5, -4])?;
        let a = &primary.images.last().unwrap().rgba;
        let b = &distant.images.last().unwrap().rgba;
        assert!(a.chunks_exact(4).any(|p| p[0] == 0), "primary terrain hole");
        assert!(
            a.chunks_exact(4).any(|p| p[0] == 255),
            "covered terrain squares"
        );
        assert!(
            b.chunks_exact(4)
                .all(|p| p[0] == if repeat { 255 } else { 0 }),
            "actual distant terrain coverage"
        );
    }
    Ok(())
}
#[test]
fn invalid_or_excessive_coverage_is_explicitly_rejected() -> Result<()> {
    let water = plain_water(64.)?;
    for (eye, radius) in [
        (Vec3::ZERO, f32::NAN),
        (Vec3::ZERO, 0.),
        (Vec3::splat(f32::NAN), 1.),
        (Vec3::splat(20_000_000.), 1.),
        (Vec3::ZERO, 4000.),
    ] {
        assert!(
            water.visible_from(&[eye], radius).is_err(),
            "{eye:?}/{radius}"
        );
    }
    assert!(water.copy_scene([i32::MAX, 0]).is_err());
    assert!(water.copy_scene([1_000_000, 0]).is_err());
    Ok(())
}
#[test]
fn gpu_water_keeps_stationary_resources_and_evicts_travelled_copies() -> Result<()> {
    let (device, queue) = pollster::block_on(async {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = instance.request_adapter(&Default::default()).await?;
        anyhow::Ok(adapter.request_device(&Default::default()).await?)
    })?;
    let mut renderer = SceneRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let mut waters = vec![GpuWater::new(Arc::new(plain_water(128.)?), 256.)?];
    GpuWater::update_all(
        &mut waters,
        &renderer,
        &device,
        &queue,
        &[Vec3::new(16., 10., -16.)],
        10.,
    )?;
    let counts = renderer.upload_counts();
    assert_eq!(waters[0].copy_count(), 1);
    let masks = waters[0].mask_builds();
    let textures = waters[0].texture_count();
    for x in [16., 17., 18., 16.] {
        GpuWater::update_all(
            &mut waters,
            &renderer,
            &device,
            &queue,
            &[Vec3::new(x, 10., -16.)],
            10.,
        )?;
        assert_eq!(renderer.upload_counts(), counts);
        assert_eq!(waters[0].mask_builds(), masks);
    }
    for step in 1..=20 {
        GpuWater::update_all(
            &mut waters,
            &renderer,
            &device,
            &queue,
            &[Vec3::new(16. + step as f32 * 128., 10., -16.)],
            10.,
        )?;
        assert_eq!(waters[0].copy_count(), 1);
        assert_eq!(waters[0].texture_count(), textures, "no mask texture trail");
        assert_eq!(waters[0].draws().count(), 1);
    }
    GpuWater::update_all(
        &mut waters,
        &renderer,
        &device,
        &queue,
        &[Vec3::new(16., 10., -16.), Vec3::new(-1008., 10., -16.)],
        10.,
    )?;
    assert_eq!(waters[0].copy_count(), 2, "secondary eye retained");
    assert_eq!(
        waters[0].texture_count(),
        textures,
        "equal masks share texture"
    );
    let eye = Vec3::new(-1008., 20., 0.);
    let camera = Camera::perspective(
        eye.to_array(),
        [-1008., 0., -16.],
        1.,
        60f32.to_radians(),
        0.05,
        200.,
    );
    renderer.update_camera(&queue, &camera);
    let target = common::color_target(&device, wgpu::TextureFormat::Rgba8Unorm, 64, 64);
    let empty = common::render(&device, &queue, &mut renderer, &target, &[], &[], &[])?;
    let scenes: Vec<_> = waters.iter().flat_map(|w| w.draws()).collect();
    let drawn = common::render(&device, &queue, &mut renderer, &target, &scenes, &[], &[])?;
    assert!(
        drawn
            .chunks_exact(4)
            .zip(empty.chunks_exact(4))
            .filter(|(a, b)| a != b)
            .count()
            > 100,
        "far secondary-eye water must reach actual GPU output"
    );
    assert!(
        GpuWater::update_all(&mut waters, &renderer, &device, &queue, &[Vec3::ZERO], 257.).is_err()
    );
    let before = renderer.upload_counts();
    let shared = Arc::new(plain_water(64.)?);
    let mut excessive = (0..5)
        .map(|_| GpuWater::new(shared.clone(), 400.))
        .collect::<Result<Vec<_>>>()?;
    assert!(
        GpuWater::update_all(
            &mut excessive,
            &renderer,
            &device,
            &queue,
            &[Vec3::ZERO],
            400.
        )
        .is_err(),
        "whole-map budget"
    );
    assert_eq!(
        renderer.upload_counts(),
        before,
        "budget refusal precedes GPU allocation"
    );
    GpuWater::update_all(&mut waters, &renderer, &device, &queue, &[], 10.)?;
    assert_eq!(waters[0].copy_count(), 0);
    assert_eq!(
        waters[0].texture_count(),
        1,
        "only shared source white remains"
    );
    Ok(())
}

#[test]
fn multi_water_transfer_evicts_old_ownership_before_admitting_the_new_view_union() -> Result<()> {
    let (device, queue) = common::gpu()?;
    let renderer = SceneRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let a = Arc::new(plain_water(1024.)?);
    let mut spec = Water::volume([512., -4., -64.], [576., 0., 0.]);
    spec.repeat_period = Some(1024.);
    let b = Arc::new(WaterScene::new(
        spec,
        SceneData::default(),
        [0; 3],
        ([1.; 4], 6.),
        Vec::new(),
    )?);
    let mut waters = vec![GpuWater::new(a, 10.)?, GpuWater::new(b, 10.)?];
    let old: Vec<_> = std::iter::once(Vec3::new(16., 10., -16.))
        .chain((0..400).map(|k| Vec3::new(528. + k as f32 * 1024., 10., -16.)))
        .collect();
    let new: Vec<_> = std::iter::once(Vec3::new(528., 10., -16.))
        .chain((0..400).map(|k| Vec3::new(16. + k as f32 * 1024., 10., -16.)))
        .collect();
    GpuWater::update_all(&mut waters, &renderer, &device, &queue, &old, 10.)?;
    assert_eq!(
        waters.iter().map(|w| w.copy_count()).collect::<Vec<_>>(),
        [1, 400]
    );
    let old_large = waters[1].resource_bound_bytes()?;
    GpuWater::update_all(&mut waters, &renderer, &device, &queue, &new, 10.)?;
    assert_eq!(
        waters.iter().map(|w| w.copy_count()).collect::<Vec<_>>(),
        [400, 1]
    );
    assert!(
        waters[0].resource_bound_bytes()? + old_large > WATER_BUDGET_BYTES,
        "fixture must exceed the bound if outgoing group B remains while A grows"
    );
    let counts = renderer.upload_counts();
    let before: Vec<_> = waters
        .iter()
        .map(|w| (w.copy_count(), w.mask_builds(), w.texture_count()))
        .collect();
    let excessive: Vec<_> = old.iter().chain(&new).copied().collect();
    assert!(
        GpuWater::update_all(&mut waters, &renderer, &device, &queue, &excessive, 10.).is_err()
    );
    assert_eq!(renderer.upload_counts(), counts, "refusal before uploads");
    assert_eq!(
        waters
            .iter()
            .map(|w| (w.copy_count(), w.mask_builds(), w.texture_count()))
            .collect::<Vec<_>>(),
        before,
        "refusal must preserve the current visible resources"
    );
    GpuWater::update_all(&mut waters, &renderer, &device, &queue, &old, 10.)?;
    assert_eq!(
        waters.iter().map(|w| w.copy_count()).collect::<Vec<_>>(),
        [1, 400]
    );
    assert!(
        waters.iter().all(|w| w.texture_count() == 2),
        "no retired depth-mask texture trail"
    );
    Ok(())
}
