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
        &root.join("content/item-presentation-pack-010"),
        &root.join("content/weapons-pack-009"),
    )?;
    let weapons = bri_weapons::Pack::from_json(&std::fs::read(
        root.join("content/weapons-pack-009/weapons.json"),
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
        reflected_self: false,
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
    adapter.sync(&view, frame(), |_| None)?;
    assert_eq!(adapter.instances().count(), 2);
    assert_eq!(adapter.diagnostics.cached_models, 1);
    assert_eq!(adapter.diagnostics.geometry_slots, 1);
    assert_eq!(adapter.diagnostics.model_builds, 1);

    let mounted_image = bri_sim::session::MountedImage {
        paint: None,
image: "v20.image.gunimage".into(),
        state: "Fire".into(),
        hand: 0,
    };
    let mounted = WeaponView {
        images: BTreeMap::from([(7, vec![mounted_image.clone()]), (8, vec![mounted_image])]),
        ..Default::default()
    };
    let poses = std::cell::Cell::new(0);
    adapter.sync(&mounted, frame(), |_| {
        poses.set(poses.get() + 1);
        Some(MountPose {
            eye: Mat4::IDENTITY,
            mounts: BTreeMap::from([(0, Mat4::IDENTITY)]),
            actions: BTreeMap::new(),
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
        actions: BTreeMap::new(),
        velocity: Vec3::ZERO,
    };
    adapter.sync(
        &mounted,
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
    adapter.sync(&view, frame(), |_| None)?;
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
fn a_picked_up_brick_item_stays_as_a_ghost_until_it_respawns() -> Result<()> {
    let (assets, weapons) = packs()?;
    let mut adapter = WorldItems::new(assets, weapons, WorldItemLimits::default())?;
    let mut item = static_item(1, [0.; 3]);
    item.item = "v20.weapon.bowitem".into();
    item.available_at = 121;
    let view = WeaponView {
        static_items: vec![item],
        ..Default::default()
    };
    let tint = |adapter: &WorldItems| {
        let found: Vec<_> = adapter.instances().collect();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, ItemIdentity::Static(1));
        found[0].1.tint
    };
    adapter.sync(&view, frame(), |_| None)?;
    assert_eq!(tint(&adapter), [1., 1., 1., RESPAWN_GHOST_ALPHA]);
    assert_eq!(adapter.diagnostics.cooling_down, 1);
    // `fadeIn` at the respawn tick restores the solid image-coloured item.
    adapter.sync(
        &view,
        WorldItemFrame {
            tick: 121,
            ..frame()
        },
        |_| None,
    )?;
    assert_eq!(tint(&adapter), [1.; 4]);
    assert_eq!(adapter.diagnostics.cooling_down, 0);
    Ok(())
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
    adapter.sync(&view, frame(), |_| None)?;
    assert_eq!(adapter.instances().count(), 1);
    view.static_items[0].item = "v20.weapon.bowitem".into();
    adapter.sync(&view, frame(), |_| None)?;
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
            paint: None,
image: "v20.image.gunimage".into(),
            state: "Ready".into(),
            hand: 0,
        }],
    );
    let local_frame = WorldItemFrame {
        local_owner: Some(7),
        ..frame()
    };
    adapter.sync(&view, local_frame, |_| {
        Some(MountPose {
            eye: Mat4::IDENTITY,
            mounts: BTreeMap::from([(0, Mat4::IDENTITY)]),
            actions: BTreeMap::new(),
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
    adapter.sync(&view, local_frame, |_| None)?;
    assert_eq!(
        adapter.instances().map(|(id, _)| id).collect::<Vec<_>>(),
        vec![ItemIdentity::Static(1)]
    );
    adapter.sync(
        &WeaponView::default(),
        local_frame,
        |_| None,
    )?;
    assert_eq!(adapter.diagnostics.cached_models, 0);
    assert_eq!(adapter.diagnostics.geometry_slots, 0);
    assert_eq!(adapter.diagnostics.geometry_vertices, 0);
    Ok(())
}

#[test]
#[ignore = "requires converted item and weapon packs"]
fn mirrors_show_the_local_players_held_item_as_everyone_else_sees_it() -> Result<()> {
    let (assets, weapons) = packs()?;
    let mut adapter = WorldItems::new(assets, weapons, WorldItemLimits::default())?;
    let view = WeaponView {
        images: BTreeMap::from([(
            7,
            vec![bri_sim::session::MountedImage {
                paint: None,
                image: "v20.image.gunimage".into(),
                state: "Ready".into(),
                hand: 0,
            }],
        )]),
        ..Default::default()
    };
    let pose = |_| {
        Some(MountPose {
            eye: Mat4::from_translation(Vec3::new(0.0, 2.3, 0.0)),
            mounts: BTreeMap::from([(0, Mat4::from_translation(Vec3::new(0.4, 1.2, 0.2)))]),
            actions: BTreeMap::new(),
            velocity: Vec3::ZERO,
        })
    };
    let placed = |adapter: &WorldItems| -> BTreeMap<ItemIdentity, Mat4> {
        adapter
            .instances()
            .map(|(id, t)| (id, t.transform))
            .collect()
    };
    let local = WorldItemFrame {
        local_owner: Some(7),
        ..frame()
    };
    adapter.sync(&view, local, pose)?;
    let others_see = placed(&adapter)[&ItemIdentity::Mounted(7, 0)];
    // First person without mirrors: only the held view.
    let first = WorldItemFrame {
        first_person: true,
        ..local
    };
    adapter.sync(&view, first, pose)?;
    let held = placed(&adapter);
    assert_eq!(held.keys().copied().collect::<Vec<_>>(), vec![ItemIdentity::Mounted(7, 0)]);
    // With mirrors, a copy where everyone else sees it, for them only.
    adapter.sync(
        &view,
        WorldItemFrame {
            reflected_self: true,
            ..first
        },
        pose,
    )?;
    let both = placed(&adapter);
    assert_eq!(both[&ItemIdentity::Mounted(7, 0)], held[&ItemIdentity::Mounted(7, 0)]);
    assert!(both[&ItemIdentity::Reflected(7, 0)].abs_diff_eq(others_see, 1e-5));
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

#[test]
#[ignore = "requires converted item and weapon packs"]
fn a_held_hammer_swings_again_on_every_fire_entry() -> Result<()> {
    // hammerImage loops Fire, CheckFire (0 ticks), Fire while the trigger is
    // held, so the replicated state reads "Fire" throughout. Each entry's
    // Fire sequence cue must start the view model's swing over.
    let (assets, weapons) = packs()?;
    let mut adapter = WorldItems::new(assets, weapons, WorldItemLimits::default())?;
    let view = WeaponView {
        images: BTreeMap::from([(
            7,
            vec![bri_sim::session::MountedImage {
                paint: None,
                image: "v20.image.hammerimage".into(),
                state: "Fire".into(),
                hand: 0,
            }],
        )]),
        ..Default::default()
    };
    let pose = |_| {
        Some(MountPose {
            eye: Mat4::IDENTITY,
            mounts: BTreeMap::from([(0, Mat4::IDENTITY)]),
            actions: BTreeMap::new(),
            velocity: Vec3::ZERO,
        })
    };
    let head = |adapter: &WorldItems| {
        adapter
            .mounted_node(7, 0, "v20.image.hammerimage", "FPhammer9999")
            .unwrap()
    };
    let same = |a: Mat4, b: Mat4| a.abs_diff_eq(b, 1e-4);
    let at = |seconds: f64| WorldItemFrame {
        seconds,
        first_person: true,
        reflected_self: false,
        local_owner: Some(7),
        ..frame()
    };
    adapter.sync(&view, at(1.0), pose)?;
    let start = head(&adapter);
    adapter.sync(&view, at(1.5), pose)?;
    let end = head(&adapter);
    assert!(!same(start, end), "the fire clip moves the head");
    adapter.sync(&view, at(1.6), pose)?;
    assert!(same(head(&adapter), end), "one entry swings once");
    adapter.restart_image_sequence(7, 0, "Fire");
    adapter.sync(&view, at(1.6), pose)?;
    assert!(same(head(&adapter), start), "a new entry swings again");
    Ok(())
}

#[test]
#[ignore = "requires converted packs and an offscreen GPU adapter"]
fn a_thrown_spear_hides_its_image_without_crashing_the_renderer() -> Result<()> {
    // a16: "buffer slice can not be empty". spear.dts's `fire` sequence keys
    // both spear objects invisible while it is thrown, so the posed held
    // image has no geometry; its shadow draw bound the empty buffers.
    let (assets, weapons) = packs()?;
    let gpu = Headless::new()?;
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let mut renderer = SceneRenderer::with_settings(
        &gpu.device,
        format,
        1,
        Some(bri_render::shadow::ShadowSettings::LOW),
    );
    let mut camera =
        bri_render::scene::Camera::perspective([4., 3., 4.], [0.; 3], 1.0, 1.0, 0.05, 100.0);
    camera.sun_direction = [0.3, -1.0, 0.0, 0.0];
    renderer.update_camera(&gpu.queue, &camera);
    let mut adapter = WorldItems::new(assets, weapons, WorldItemLimits::default())?;
    let view = WeaponView {
        images: BTreeMap::from([(
            7,
            vec![bri_sim::session::MountedImage {
                paint: None,
                image: "v20.image.spearimage".into(),
                state: "Fire".into(),
                hand: 0,
            }],
        )]),
        ..Default::default()
    };
    let pose = |_| {
        Some(MountPose {
            eye: Mat4::IDENTITY,
            mounts: BTreeMap::from([(0, Mat4::IDENTITY)]),
            actions: BTreeMap::new(),
            velocity: Vec3::ZERO,
        })
    };
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("thrown spear target"),
        size: wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let color = target.create_view(&Default::default());
    let depth = bri_render::scene::create_depth(&gpu.device, 64, 64).create_view(&Default::default());
    // Mid-throw (hidden) and after it (shown again).
    for seconds in [1.0, 1.05, 1.5] {
        adapter.sync(
            &view,
            WorldItemFrame {
                seconds,
                ..frame()
            },
            pose,
        )?;
        adapter.upload(&renderer, &gpu.device, &gpu.queue)?;
        let draws = adapter.draws();
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        renderer.render_shadows(
            &mut encoder,
            bri_render::scene::ShadowCasters {
                scenes: &[],
                instances: &draws,
            },
            bri_render::scene::ShadowCasters {
                scenes: &[],
                instances: &draws,
            },
        );
        renderer.render_with_instances(&mut encoder, &color, &depth, &[], &draws, None);
        gpu.queue.submit([encoder.finish()]);
    }
    Ok(())
}

#[test]
#[ignore = "requires converted item and weapon packs"]
fn a_stuck_arrow_keeps_pointing_the_way_it_flew() -> Result<()> {
    // Sticking zeroes the arrow's velocity; it must not flip to point up.
    let (assets, weapons) = packs()?;
    let mut adapter = WorldItems::new(assets, weapons, WorldItemLimits::default())?;
    let arrow = |velocity: Vec3, age: u32| bri_weapons::Projectile {
        id: 5,
        definition: "v20.projectile.arrowprojectile".into(),
        source: bri_weapons::ActorId(1),
        position: Vec3::new(0., 2., 0.),
        velocity,
        scale: 1.,
        age,
        bounced: false,
        stuck: velocity == Vec3::ZERO,
        origin: Vec3::ZERO,
        was_thrown: false,
        paint: None,
        heading: None,
    };
    let nose = |adapter: &WorldItems| {
        let (_, t) = adapter
            .instances()
            .find(|(id, _)| matches!(id, ItemIdentity::Projectile(5)))
            .expect("arrow drawn");
        t.transform.transform_vector3(Vec3::NEG_Z).normalize()
    };
    let flying = Vec3::new(30., -5., 0.);
    let view = |p| WeaponView {
        projectiles: vec![p],
        ..Default::default()
    };
    adapter.sync(&view(arrow(flying, 10)), frame(), |_| None)?;
    assert!(nose(&adapter).abs_diff_eq(flying.normalize(), 1e-4));
    let later = WorldItemFrame {
        seconds: 1.5,
        ..frame()
    };
    adapter.sync(&view(arrow(Vec3::ZERO, 20)), later, |_| None)?;
    assert!(
        nose(&adapter).abs_diff_eq(flying.normalize(), 1e-4),
        "stuck arrow points along its flight, not up: {}",
        nose(&adapter)
    );
    // A player who joins after it stuck gets the replicated heading.
    let mut joiner = WorldItems::new(packs()?.0, packs()?.1, WorldItemLimits::default())?;
    let stuck = bri_weapons::Projectile {
        heading: Some(flying.normalize()),
        ..arrow(Vec3::ZERO, 20)
    };
    joiner.sync(&view(stuck), later, |_| None)?;
    assert!(nose(&joiner).abs_diff_eq(flying.normalize(), 1e-4));
    Ok(())
}

/// brickWeapon.dts has no muzzlePoint: `brickTrailEmitter` streams from the
/// held brick's own transform (`ShapeBase::getMuzzleTransform`), first
/// person included, instead of being dropped for want of a pose.
#[test]
#[ignore = "requires converted item and weapon packs"]
fn the_brick_trail_streams_from_the_held_brick_without_a_muzzle_point() -> Result<()> {
    use anyhow::Context;
    use bri_sim::presentation::{Cue, CueKind};
    let (assets, weapons) = packs()?;
    let mut adapter = WorldItems::new(assets, weapons, WorldItemLimits::default())?;
    let view = WeaponView {
        images: BTreeMap::from([(
            7,
            vec![bri_sim::session::MountedImage {
                paint: None,
                image: "v20.image.brickimage".into(),
                state: "Fire".into(),
                hand: 0,
            }],
        )]),
        ..Default::default()
    };
    let hand = Mat4::from_translation(Vec3::new(0.4, 1.2, -0.3));
    let eye = Mat4::from_translation(Vec3::new(0.0, 2.0, 0.0));
    let cue = Cue {
        id: 1,
        tick: 120,
        kind: CueKind::WeaponEffect {
            source: bri_weapons::TargetId::Actor(bri_weapons::ActorId(7)),
            definition: "brickTrailEmitter".into(),
            node: String::new(),
            seconds: 0.1,
            image: Some("v20.image.brickimage".into()),
            hand: Some(0),
            direction: Some([0.0, 0.0, -1.0]),
            scale: 1.0,
        },
        position: [0.0; 3],
    };
    for first_person in [false, true] {
        let frame = WorldItemFrame {
            local_owner: Some(7),
            first_person,
            reflected_self: false,
            ..frame()
        };
        adapter.sync(&view, frame, |_| {
            Some(MountPose {
                eye,
                mounts: BTreeMap::from([(0, hand)]),
                actions: BTreeMap::new(),
                velocity: Vec3::ZERO,
            })
        })?;
        let held = adapter
            .mounted_transform(7, 0)
            .context("the brick is mounted")?;
        let muzzle = adapter
            .mounted_node(7, 0, "v20.image.brickimage", "muzzlePoint")
            .unwrap_or(held);
        let pose = adapter
            .effect_pose(&cue)
            .with_context(|| format!("no trail pose, first person {first_person}"))?;
        assert!(
            pose.position.distance(muzzle.w_axis.truncate()) < 1e-4,
            "first person {first_person}"
        );
    }
    Ok(())
}
