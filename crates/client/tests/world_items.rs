//! CPU projection and bounded offscreen GPU lifecycle for the native item adapter.
use anyhow::Result;
use bri_client::{items::ItemAssets, world_items::*};
use bri_render::scene::SceneRenderer;
use bri_sim::{item_spawners::StaticItem, session::WeaponView};
use bri_ui::gpu::Headless;
use glam::{Mat4, Vec3};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn packs() -> Result<(Arc<ItemAssets>, Arc<bri_weapons::Pack>)> {
    let root = root();
    let assets = ItemAssets::load(
        &root.join("content/item-presentation-pack-005"),
        &root.join("content/weapons-pack-004"),
    )?;
    let weapons = bri_weapons::Pack::from_json(&std::fs::read(
        root.join("content/weapons-pack-004/weapons.json"),
    )?)?;
    Ok((Arc::new(assets), Arc::new(weapons)))
}
fn frame() -> WorldItemFrame {
    WorldItemFrame {
        tick: 120,
        seconds: 1.,
        eye: Vec3::ZERO,
        local_owner: None,
        first_person: false,
    }
}
fn static_item(brick: u64, position: [f32; 3]) -> StaticItem {
    StaticItem {
        brick,
        item: "v20.weapon.gunitem".into(),
        position,
        direction: 2,
        available_at: 0,
    }
}

#[test]
#[ignore = "requires converted item and weapon packs"]
fn model_instances_share_cpu_model_and_missing_mounts_never_guess() -> Result<()> {
    let (assets, weapons) = packs()?;
    let mut adapter = WorldItems::new(assets, weapons, WorldItemLimits::default())?;
    let view = WeaponView {
        static_items: vec![static_item(1, [0., 0., 0.]), static_item(2, [2., 0., 0.])],
        ..Default::default()
    };
    adapter.sync(&view, &BTreeMap::new(), frame(), |_| None)?;
    assert_eq!(adapter.instances().count(), 2);
    assert_eq!(adapter.diagnostics.cached_models, 1);
    assert_eq!(adapter.diagnostics.geometry_slots, 1);
    assert_eq!(adapter.diagnostics.model_builds, 1);

    let mounted_image = bri_sim::session::MountedImage {
        image: "v20.image.gunimage".into(),
        state: "Fire".into(),
        hand: 0,
    };
    let mounted = WeaponView {
        images: BTreeMap::from([(7, vec![mounted_image.clone()]), (8, vec![mounted_image])]),
        ..Default::default()
    };
    let poses = std::cell::Cell::new(0);
    adapter.sync(&mounted, &BTreeMap::new(), frame(), |_| {
        poses.set(poses.get() + 1);
        Some(MountPose {
            eye: Mat4::IDENTITY,
            mounts: BTreeMap::from([(0, Mat4::IDENTITY)]),
            velocity: Vec3::ZERO,
        })
    })?;
    assert_eq!(
        poses.get(),
        2,
        "avatar pose should be sampled once per owner"
    );
    assert_eq!(adapter.instances().count(), 2);
    assert_eq!(adapter.diagnostics.cached_models, 1);
    assert_eq!(
        adapter.diagnostics.geometry_slots, 1,
        "matching sampled poses should reuse geometry"
    );
    assert!(
        adapter
            .mounted_node(7, 0, "v20.image.gunimage", "mountPoint")
            .is_ok()
    );
    assert!(
        adapter
            .mounted_node(7, 0, "v20.image.gunimage", "absent-node")
            .is_err()
    );
    assert!(
        adapter
            .mounted_node(7, 0, "other-image", "mountPoint")
            .is_err()
    );

    let missing_mount = MountPose {
        eye: Mat4::IDENTITY,
        mounts: BTreeMap::new(),
        velocity: Vec3::ZERO,
    };
    adapter.sync(
        &mounted,
        &BTreeMap::new(),
        WorldItemFrame {
            tick: 121,
            seconds: 1.01,
            ..frame()
        },
        |_| Some(missing_mount.clone()),
    )?;
    assert_eq!(
        adapter.instances().count(),
        0,
        "missing authored mount must not create a guessed transform"
    );
    assert_eq!(adapter.diagnostics.missing_poses, 2);
    assert!(
        adapter
            .diagnostics
            .messages
            .iter()
            .any(|m| m.contains("Missing/invalid authored mount"))
    );
    Ok(())
}

#[test]
#[ignore = "requires converted packs and an offscreen GPU adapter"]
fn shared_gpu_geometry_clears_and_recreates_without_cpu_loss() -> Result<()> {
    let (assets, weapons) = packs()?;
    let gpu = Headless::new()?;
    let renderer = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let mut adapter = WorldItems::new(assets, weapons, WorldItemLimits::default())?;
    let view = WeaponView {
        static_items: vec![static_item(1, [0., 0., 0.]), static_item(2, [2., 0., 0.])],
        ..Default::default()
    };
    adapter.sync(&view, &BTreeMap::new(), frame(), |_| None)?;
    adapter.upload(&renderer, &gpu.device, &gpu.queue)?;
    assert_eq!(adapter.model_scenes().count(), 1);
    assert_eq!(
        adapter.draws().len(),
        1,
        "same model should draw as one instanced group"
    );
    assert_eq!(adapter.diagnostics.model_uploads, 1);
    assert_eq!(adapter.diagnostics.shared_geometry_uploads, 1);

    adapter.clear_gpu();
    assert!(adapter.draws().is_empty());
    assert_eq!(
        adapter.model_scenes().count(),
        1,
        "device reset should preserve CPU-authored data"
    );
    adapter.upload(&renderer, &gpu.device, &gpu.queue)?;
    assert_eq!(adapter.draws().len(), 1);
    assert_eq!(adapter.diagnostics.model_uploads, 2);
    assert_eq!(adapter.diagnostics.shared_geometry_uploads, 2);

    adapter.reset();
    assert_eq!(adapter.model_scenes().count(), 0);
    assert!(adapter.draws().is_empty());
    assert_eq!(adapter.diagnostics.cached_models, 0);
    Ok(())
}

#[test]
fn drop_and_projectile_fades_are_bounded_and_finite() {
    assert_eq!(drop_opacity(0, 1000), 1.);
    assert!(drop_opacity(880, 1000) > drop_opacity(904, 1000));
    assert_eq!(drop_opacity(1000, 1000), 0.);
    assert_eq!(projectile_opacity(0, 20, 120), 1.);
    assert_eq!(projectile_opacity(120, 20, 120), 0.);
    assert!(projectile_opacity(119, 20, 120).is_finite());
}

#[test]
#[ignore = "requires converted item and weapon packs"]
fn bounded_cache_reclaims_absent_models_and_prioritizes_held_items() -> Result<()> {
    let (assets, weapons) = packs()?;
    let mut adapter = WorldItems::new(
        assets,
        weapons,
        WorldItemLimits {
            models: 1,
            geometry_slots: 1,
            ..Default::default()
        },
    )?;
    let mut view = WeaponView {
        static_items: vec![static_item(1, [0.; 3])],
        ..Default::default()
    };
    adapter.sync(&view, &BTreeMap::new(), frame(), |_| None)?;
    assert_eq!(adapter.instances().count(), 1);
    view.static_items[0].item = "v20.weapon.bowitem".into();
    adapter.sync(&view, &BTreeMap::new(), frame(), |_| None)?;
    assert_eq!(
        adapter.instances().count(),
        1,
        "old cached gun must not starve new bow"
    );
    assert_eq!(adapter.diagnostics.deferred, 0);
    assert_eq!(adapter.diagnostics.cached_models, 1);
    assert_eq!(adapter.diagnostics.geometry_slots, 1);
    view.images.insert(
        7,
        vec![bri_sim::session::MountedImage {
            image: "v20.image.gunimage".into(),
            state: "Ready".into(),
            hand: 0,
        }],
    );
    let local_frame = WorldItemFrame {
        local_owner: Some(7),
        ..frame()
    };
    adapter.sync(&view, &BTreeMap::new(), local_frame, |_| {
        Some(MountPose {
            eye: Mat4::IDENTITY,
            mounts: BTreeMap::from([(0, Mat4::IDENTITY)]),
            velocity: Vec3::ZERO,
        })
    })?;
    assert_eq!(
        adapter.instances().map(|(id, _)| id).collect::<Vec<_>>(),
        vec![ItemIdentity::Mounted(7, 0)],
        "local hand must beat previously cached static model even when that model sorts first"
    );
    assert_eq!(adapter.diagnostics.deferred, 1);
    view.images.clear();
    adapter.sync(&view, &BTreeMap::new(), local_frame, |_| None)?;
    assert_eq!(
        adapter.instances().map(|(id, _)| id).collect::<Vec<_>>(),
        vec![ItemIdentity::Static(1)]
    );
    adapter.sync(
        &WeaponView::default(),
        &BTreeMap::new(),
        local_frame,
        |_| None,
    )?;
    assert_eq!(adapter.diagnostics.cached_models, 0);
    assert_eq!(adapter.diagnostics.geometry_slots, 0);
    assert_eq!(adapter.diagnostics.geometry_vertices, 0);
    Ok(())
}

fn pixels(
    gpu: &Headless,
    renderer: &mut SceneRenderer,
    scenes: &[&bri_render::scene::GpuScene],
    instances: &[(
        &bri_render::scene::GpuScene,
        &bri_render::scene::GpuInstances,
    )],
    camera: &bri_render::scene::Camera,
) -> Result<Vec<u8>> {
    let size = wgpu::Extent3d {
        width: 256,
        height: 256,
        depth_or_array_layers: 1,
    };
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("items offscreen"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth = bri_render::scene::create_depth(&gpu.device, 256, 256);
    renderer.update_camera(&gpu.queue, camera);
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    renderer.render_with_instances(
        &mut encoder,
        &target.create_view(&Default::default()),
        &depth.create_view(&Default::default()),
        scenes,
        instances,
        Some(wgpu::Color::BLACK),
    );
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("items readback"),
        size: 256 * 256 * 4,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(1024),
                rows_per_image: Some(256),
            },
        },
        size,
    );
    gpu.queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    gpu.device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(std::time::Duration::from_secs(30)),
    })?;
    rx.recv_timeout(std::time::Duration::from_secs(30))??;
    Ok(buffer.slice(..).get_mapped_range()?.to_vec())
}
#[test]
#[ignore = "requires converted packs and an offscreen GPU adapter"]
fn actual_item_instances_match_independently_baked_world_geometry() -> Result<()> {
    let (assets, weapons) = packs()?;
    let gpu = Headless::new()?;
    let mut renderer = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let mut adapter = WorldItems::new(assets.clone(), weapons, Default::default())?;
    let mut items = vec![static_item(1, [0., 0., 0.]), static_item(2, [2., 0., 0.])];
    items[1].direction = 4;
    adapter.sync(
        &WeaponView {
            static_items: items.clone(),
            ..Default::default()
        },
        &BTreeMap::new(),
        frame(),
        |_| None,
    )?;
    adapter.upload(&renderer, &gpu.device, &gpu.queue)?;
    let binding = &assets.presentation.items[&items[0].item];
    let mut expected = Vec::new();
    for item in &items {
        let mut mesh = assets.mesh(&binding.model, binding.tint)?;
        mesh.pose(
            &assets,
            Mat4::from_rotation_translation(item.rotation(), Vec3::from(item.position)),
            None,
            0.,
        )?;
        expected.push(renderer.upload(&gpu.device, &gpu.queue, &mesh.data)?);
    }
    let camera = bri_render::scene::Camera::perspective(
        [1., 2., 5.],
        [1., 0., 0.],
        1.,
        45f32.to_radians(),
        0.05,
        50.,
    );
    let actual = pixels(&gpu, &mut renderer, &[], &adapter.draws(), &camera)?;
    let oracle = pixels(
        &gpu,
        &mut renderer,
        &expected.iter().collect::<Vec<_>>(),
        &[],
        &camera,
    )?;
    assert!(
        actual
            .chunks_exact(4)
            .filter(|pixel| pixel[..3].iter().any(|v| *v > 10))
            .count()
            > 20,
        "native items must actually produce visible pixels"
    );
    assert!(
        actual.iter().zip(&oracle).all(|(a, b)| a.abs_diff(*b) <= 1),
        "shared item models must match independent CPU world transforms"
    );
    adapter.clear_gpu();
    adapter.upload(&renderer, &gpu.device, &gpu.queue)?;
    assert_eq!(
        actual,
        pixels(&gpu, &mut renderer, &[], &adapter.draws(), &camera)?
    );
    adapter.reset();
    let empty = pixels(&gpu, &mut renderer, &[], &adapter.draws(), &camera)?;
    assert!(empty.chunks_exact(4).all(|p| p[..3] == [0, 0, 0]));
    Ok(())
}
