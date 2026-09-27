//! Original native models on a host-style offscreen device; no window/input.
use anyhow::{Result, ensure};
use bri_client::items::ItemAssets;
use bri_render::scene::*;
use bri_ui::gpu::Headless;
use glam::{Mat4, Vec3};
use std::path::{Path, PathBuf};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn assets() -> Result<ItemAssets> {
    ItemAssets::load(
        &root().join("content/item-presentation-pack-008"),
        &root().join("content/weapons-pack-007"),
    )
}

#[test]
fn native_models_tints_mounts_icons_and_persistent_pose() -> Result<()> {
    let assets = assets()?;
    assert_eq!(assets.presentation.items.len(), 21);
    for id in [
        "hammeritem",
        "wrenchitem",
        "printgun",
        "wanditem",
        "gunitem",
        "akimbogunitem",
    ] {
        let id = format!("v20.weapon.{id}");
        let scene = assets.item_scene(&id, Mat4::IDENTITY)?;
        assert!(!scene.vertices.is_empty(), "Empty item {id}");
        assert!(assets.icon(&id)?.is_some());
        assert!(scene.vertices.iter().all(|v| v.lightmap_uv == [0.; 2]));
    }
    assert_eq!(
        assets.presentation.items["v20.weapon.bluekeyitem"].tint,
        [0., 0., 1., 1.]
    );
    assert_eq!(
        assets.presentation.items["v20.weapon.pushbroomitem"].tint,
        [102. / 255., 50. / 255., 0., 1.]
    );
    let first = assets.mount_transform("v20.image.hammerimage", true, Mat4::IDENTITY, |_| None)?;
    assert!(
        first
            .w_axis
            .truncate()
            .abs_diff_eq(Vec3::new(0.7, -0.15, -1.2), 0.00001)
    );
    let third = assets.mount_transform("v20.image.hammerimage", false, Mat4::IDENTITY, |n| {
        (n == 0).then_some(Mat4::from_translation(Vec3::X * 3.))
    })?;
    assert_ne!(first, third);
    // Original Ski eyeRotation=eulerToMatrix("90 -90 0"). Engine-family
    // row matrix sends source +Y to -X, hence native -Z to -X.
    let ski = assets.mount_transform("v20.image.skiweaponimage", true, Mat4::IDENTITY, |_| None)?;
    assert!(
        ski.transform_vector3(-Vec3::Z)
            .abs_diff_eq(-Vec3::X, 0.00001)
    );
    let left =
        assets.mount_transform("v20.image.lefthandedgunimage", true, Mat4::IDENTITY, |n| {
            (n == 1).then_some(Mat4::from_translation(-Vec3::X))
        })?;
    let right = assets.mount_transform("v20.image.gunimage", true, Mat4::IDENTITY, |n| {
        (n == 0).then_some(Mat4::from_translation(Vec3::X))
    })?;
    assert!(left.determinant() > 0. && right.determinant() > 0.);
    assert!((right.w_axis.x - left.w_axis.x - 2.).abs() < 0.0001);
    assert!(
        assets
            .mount_transform(
                "v20.image.lefthandedgunimage",
                false,
                Mat4::IDENTITY,
                |_| None
            )
            .is_err()
    );
    let model = &assets.presentation.items["v20.weapon.gunitem"].model;
    let tint = assets.presentation.items["v20.weapon.gunitem"].tint;
    let mut mesh = assets.mesh(model, tint)?;
    let before = mesh.data.vertices[0].position;
    let resources = (mesh.data.images.len(), mesh.data.materials.len());
    assert!(!mesh.pose(
        &assets,
        Mat4::from_translation(Vec3::new(2., 3., 4.)),
        None,
        0.
    )?);
    assert!(
        Vec3::from(mesh.data.vertices[0].position)
            .abs_diff_eq(Vec3::from(before) + Vec3::new(2., 3., 4.), 0.00001)
    );
    assert_eq!(
        resources,
        (mesh.data.images.len(), mesh.data.materials.len())
    );
    let valid = mesh.data.vertices[0].position;
    assert!(
        mesh.pose(&assets, Mat4::IDENTITY, Some("definitely_missing"), 0.)
            .is_err()
    );
    assert_eq!(mesh.data.vertices[0].position, valid);
    mesh.pose(&assets, Mat4::IDENTITY, Some("fire"), 0.)?;
    let fire_start: Vec<_> = mesh.data.vertices.iter().map(|v| v.position).collect();
    mesh.pose(&assets, Mat4::IDENTITY, Some("fire"), 0.05)?;
    assert_ne!(
        fire_start,
        mesh.data
            .vertices
            .iter()
            .map(|v| v.position)
            .collect::<Vec<_>>(),
        "Authored gun fire clip did not animate"
    );
    let pose = assets.pose(model, None, 0.)?;
    let muzzle = assets.node_transform(model, &pose, Mat4::IDENTITY, "muzzlePoint")?;
    assert!(muzzle.is_finite());
    assert!(
        assets
            .node_transform(model, &pose, Mat4::IDENTITY, "absent")
            .is_err()
    );
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}
#[test]
fn corrupt_or_oversized_native_resources_reject() -> Result<()> {
    let output = root().join("artifacts/native-items");
    std::fs::create_dir_all(&output)?;
    let fixture = output.join(format!(
        "loader-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    copy_dir(&root().join("content/item-presentation-pack-008"), &fixture)?;
    let manifest = fixture.join("presentation.json");
    let original = std::fs::read(&manifest)?;
    let value: serde_json::Value = serde_json::from_slice(&original)?;
    let weapons = root().join("content/weapons-pack-007");
    for mode in 0..6 {
        let mut bad = value.clone();
        match mode {
            0 => {
                bad["models"]
                    .as_object_mut()
                    .unwrap()
                    .values_mut()
                    .next()
                    .unwrap()["file"] = "../outside.json".into();
            }
            1 => {
                bad["textures"]
                    .as_object_mut()
                    .unwrap()
                    .values_mut()
                    .next()
                    .unwrap()["width"] = 4096.into();
            }
            2 => {
                for t in bad["textures"].as_object_mut().unwrap().values_mut() {
                    t["width"] = 4096.into();
                    t["height"] = 4096.into();
                }
            }
            3 => {
                bad["weapons_sha256"] = "0".repeat(64).into();
            }
            4 => {
                bad["models"]
                    .as_object_mut()
                    .unwrap()
                    .values_mut()
                    .next()
                    .unwrap()["sha256"] = "0".repeat(64).into();
            }
            _ => {
                bad["items"]["v20.weapon.gunitem"]["model"] = "base/data/shapes/wand.dts".into();
            }
        }
        std::fs::write(&manifest, serde_json::to_vec(&bad)?)?;
        assert!(
            ItemAssets::load(&fixture, &weapons).is_err(),
            "Accepted corruption mode{mode}"
        );
    }
    std::fs::write(&manifest, original)?;
    let texture = value["textures"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()["file"]
        .as_str()
        .unwrap();
    std::fs::write(fixture.join(texture), [0u8; 16])?;
    assert!(ItemAssets::load(&fixture, &weapons).is_err());
    std::fs::remove_dir_all(&fixture)?;
    Ok(())
}

fn frame(
    gpu: &Headless,
    renderer: &mut SceneRenderer,
    scenes: &[&GpuScene],
    camera: &Camera,
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
    let depth = create_depth(&gpu.device, 256, 256);
    renderer.update_camera(&gpu.queue, camera);
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    renderer.render(
        &mut encoder,
        &target.create_view(&Default::default()),
        &depth.create_view(&Default::default()),
        scenes,
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
fn triangle(color: [f32; 4], z: f32, alpha: AlphaMode, kind: MaterialKind) -> SceneData {
    let vertices = [[-0.9, -0.8, z], [0.9, -0.8, z], [0., 0.9, z]]
        .map(|position| SceneVertex {
            position,
            normal: [0., 0., 1.],
            uv: [0.; 2],
            lightmap_uv: [0.; 2],
            color,
        })
        .to_vec();
    let mut scene = SceneData {
        vertices,
        indices: vec![0, 1, 2],
        ..Default::default()
    };
    let mut material = Material::vertex_lit("fixture", 0);
    material.alpha = alpha;
    material.kind = kind;
    scene.materials.push(material);
    scene.batches.push(MeshBatch {
        indices: 0..3,
        material: 0,
        center: [0., 0., z],
    });
    scene
}
#[test]
fn additive_unlit_and_ordinary_alpha_pixels() -> Result<()> {
    for kind in [MaterialKind::Unlit, MaterialKind::UnlitOverlay] {
        let mut invalid = triangle([1.; 4], 0.5, AlphaMode::Opaque, kind);
        for v in &mut invalid.vertices {
            v.lightmap_uv = BrickFx::new(1, 0)?.encode()?;
        }
        assert!(
            invalid.validate().is_err(),
            "Unlit item material accepted brick-only FX marker"
        );
    }
    let gpu = Headless::new()?;
    let mut renderer = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let mut camera = Camera {
        ambient: [0.; 4],
        sun_color: [0.; 4],
        ..Default::default()
    };
    let background = renderer.upload(
        &gpu.device,
        &gpu.queue,
        &triangle(
            [0., 0., 0.4, 1.],
            0.8,
            AlphaMode::Opaque,
            MaterialKind::Unlit,
        ),
    )?;
    let additive = renderer.upload(
        &gpu.device,
        &gpu.queue,
        &triangle(
            [0.4, 0., 0., 0.5],
            0.5,
            AlphaMode::Additive,
            MaterialKind::Unlit,
        ),
    )?;
    let blended = renderer.upload(
        &gpu.device,
        &gpu.queue,
        &triangle(
            [0.4, 0., 0., 0.5],
            0.5,
            AlphaMode::Blend,
            MaterialKind::Unlit,
        ),
    )?;
    let center = (128 * 256 + 128) * 4;
    let add = frame(&gpu, &mut renderer, &[&background, &additive], &camera)?;
    let blend = frame(&gpu, &mut renderer, &[&background, &blended], &camera)?;
    // Independent sRGB calculation: linear(0.4)*0.5 -> display 0.2859.
    let linear = ((0.4_f32 + 0.055) / 1.055).powf(2.4) * 0.5;
    let half = ((1.055 * linear.powf(1. / 2.4) - 0.055) * 255.).round() as u8;
    assert!(add[center].abs_diff(half) <= 1 && add[center + 2].abs_diff(102) <= 1);
    assert!(blend[center].abs_diff(half) <= 1 && blend[center + 2].abs_diff(half) <= 1);
    assert_eq!(add[center + 3], 255);
    camera.sun_color = [1.; 4];
    camera.ambient = [1.; 4];
    renderer.update_lights(
        &gpu.queue,
        &[PointLight {
            position_radius: [0., 0., 1., 5.],
            color: [4., 4., 4., 0.],
        }],
    )?;
    assert_eq!(
        add,
        frame(&gpu, &mut renderer, &[&background, &additive], &camera)?
    );
    let occluder = renderer.upload(
        &gpu.device,
        &gpu.queue,
        &triangle(
            [0., 1., 0., 1.],
            0.2,
            AlphaMode::Opaque,
            MaterialKind::Unlit,
        ),
    )?;
    let covered = frame(&gpu, &mut renderer, &[&occluder, &additive], &camera)?;
    assert_eq!(&covered[center..center + 3], &[0, 255, 0]);
    let mut overlay = triangle(
        [0.8, 0.4, 0.2, 1.],
        0.5,
        AlphaMode::Opaque,
        MaterialKind::UnlitOverlay,
    );
    overlay.images.push(SceneImage {
        label: "half black tint mask".into(),
        width: 1,
        height: 1,
        rgba: vec![0, 0, 0, 128],
        srgb: false,
    });
    overlay.materials[0].images[0] = 1;
    let overlay = renderer.upload(&gpu.device, &gpu.queue, &overlay)?;
    let pixels = frame(&gpu, &mut renderer, &[&overlay], &camera)?;
    for (channel, color) in pixels[center..center + 3].iter().zip([0.8, 0.4, 0.2]) {
        let expected = (color * (1. - 128. / 255.) * 255.) as u8;
        assert!(channel.abs_diff(expected) <= 1);
    }
    Ok(())
}

fn camera(scene: &SceneData) -> Camera {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for vertex in &scene.vertices {
        let p = Vec3::from(vertex.position);
        min = min.min(p);
        max = max.max(p);
    }
    if scene.vertices.is_empty() {
        return Camera::default();
    }
    let center = (min + max) * 0.5;
    let span = (max - min).max_element().max(0.1);
    Camera::perspective(
        (center + Vec3::new(1., 0.7, 1.2) * span).to_array(),
        center.to_array(),
        1.,
        50f32.to_radians(),
        0.01,
        1000.,
    )
}
#[test]
fn all_stock_items_projectiles_and_pose_offscreen() -> Result<()> {
    let assets = assets()?;
    let gpu = Headless::new()?;
    let mut renderer = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let out = root().join("artifacts/native-items");
    std::fs::create_dir_all(&out)?;
    let mut records = vec![];
    for (group, ids) in [
        (
            "items",
            assets.presentation.items.keys().collect::<Vec<_>>(),
        ),
        (
            "projectiles",
            assets.presentation.projectiles.keys().collect::<Vec<_>>(),
        ),
    ] {
        let mut gallery = image::RgbaImage::new(256 * 7, 256 * ids.len().div_ceil(7) as u32);
        for (i, id) in ids.into_iter().enumerate() {
            let scene = if group == "items" {
                assets.item_scene(id, Mat4::IDENTITY)?
            } else {
                assets.projectile_scene(id, Mat4::IDENTITY)?
            };
            let uploaded = renderer.upload(&gpu.device, &gpu.queue, &scene)?;
            let pixels = frame(&gpu, &mut renderer, &[&uploaded], &camera(&scene))?;
            let visible = pixels
                .chunks_exact(4)
                .filter(|p| p[..3] != [0, 0, 0])
                .count();
            if group == "items" {
                ensure!(visible > 50, "Empty original item render: {id}");
            }
            let tile = image::RgbaImage::from_raw(256, 256, pixels).unwrap();
            image::imageops::replace(
                &mut gallery,
                &tile,
                (i % 7 * 256) as i64,
                (i / 7 * 256) as i64,
            );
            records.push(serde_json::json!({"group":group,"index":i,"id":id,"triangles":scene.indices.len()/3,"visible_pixels":visible,"materials":scene.materials.iter().map(|m|format!("{:?}/{:?}",m.kind,m.alpha)).collect::<Vec<_>>(),"omissions":scene.omissions}));
        }
        gallery.save(out.join(format!("{group}.png")))?;
    }
    let model = &assets.presentation.items["v20.weapon.gunitem"].model;
    let mut instance = assets.mesh(model, assets.presentation.items["v20.weapon.gunitem"].tint)?;
    let mut uploaded = renderer.upload(&gpu.device, &gpu.queue, &instance.data)?;
    let view = camera(&instance.data);
    let before = frame(&gpu, &mut renderer, &[&uploaded], &view)?;
    ensure!(
        !instance.pose(&assets, Mat4::from_translation(Vec3::X * 0.2), None, 0.)?,
        "Rigid pose changed topology"
    );
    uploaded.update_vertices(
        &gpu.queue,
        &instance.data.vertices,
        &instance
            .data
            .batches
            .iter()
            .map(|b| b.center)
            .collect::<Vec<_>>(),
    )?;
    assert_ne!(before, frame(&gpu, &mut renderer, &[&uploaded], &view)?);
    std::fs::write(
        out.join("report.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"schema_version":1,"adapter":gpu.adapter_info.name,"pack":"item-presentation-pack-008","single_upload_pose_update":true,"records":records,"not_original_parity_acceptance":true}),
        )?,
    )?;
    Ok(())
}
