use bri_weather::*;
use glam::Vec3;
use std::sync::Arc;
fn fixture(change: impl FnOnce(&mut WeatherManifest)) -> Arc<WeatherPack> {
    bri_weather::testing::pack(change)
}
fn world(pack: Arc<WeatherPack>) -> WeatherWorld {
    let mut w = WeatherWorld::new(pack, WeatherLimits::default(), 91).unwrap();
    w.set_map("map").unwrap();
    w
}
fn plane(ray: CollisionRay, y: f32, surface: WeatherSurface) -> Option<WeatherHit> {
    let delta = ray.end - ray.start;
    if delta.y.abs() < 1e-8 {
        return None;
    }
    let t = (y - ray.start.y) / delta.y;
    if !(0.0..=1.).contains(&t) {
        return None;
    }
    Some(WeatherHit {
        position: ray.start + delta * t,
        normal: Vec3::Y,
        surface,
    })
}

#[test]
fn roofs_occlude_initial_drops_and_changed_world_invalidates_cache() {
    let mut w = world(fixture(|_| {}));
    let mut roof = |r| plane(r, 2., WeatherSurface::Solid);
    w.advance(0., CameraState::default(), 1, &mut roof).unwrap();
    let frame = w.snapshot();
    assert!(frame.drops > 0 && frame.drops < 64);
    assert!(frame.instances.iter().all(|p| p.position.y >= 2.));
    let before = w.diagnostics().collision_queries;
    w.advance(0., CameraState::default(), 1, &mut roof).unwrap();
    assert_eq!(w.diagnostics().collision_queries, before);
    w.advance(0., CameraState::default(), 2, &mut |_| None)
        .unwrap();
    assert_eq!(w.snapshot().drops, 64);
    assert_eq!(w.diagnostics().collision_queries, before + 64);
}
#[test]
fn rain_hits_water_once_and_splashes_expire_at_authored_lifetime() {
    let mut w = world(fixture(|_| {}));
    let mut water = |r| plane(r, 0., WeatherSurface::Water);
    w.advance(0., CameraState::default(), 1, &mut water)
        .unwrap();
    for _ in 0..12 {
        w.advance(0.032, CameraState::default(), 1, &mut water)
            .unwrap();
    }
    assert!(w.splash_count() > 0);
    assert!(w.diagnostics().water_impacts > 0);
    assert!(
        w.snapshot()
            .instances
            .iter()
            .filter(|p| p.splash)
            .all(|p| p.position.y.abs() < 0.001 && p.uv[0] >= 0. && p.uv[3] <= 1.)
    );
    w.set_density(0.).unwrap();
    assert_eq!(w.splash_count(), 0);
    w.advance(1., CameraState::default(), 1, &mut water)
        .unwrap();
    assert!(w.snapshot().instances.is_empty());
    // Retain a fixed finite cohort above the surface; every impact eventually leaves the splash list.
    let mut w = world(fixture(|m| {
        m.placements[0].height = 1000.;
        m.placements[0].speed_per_tick = [1., 1.];
    }));
    w.advance(0., CameraState::default(), 1, &mut water)
        .unwrap();
    for _ in 0..20 {
        w.advance(0.032, CameraState::default(), 1, &mut water)
            .unwrap();
    }
    let created = w.diagnostics().splashes_created;
    assert!(created > 0);
    for _ in 0..10 {
        w.advance(0.032, CameraState::default(), 1, &mut water)
            .unwrap();
    }
    assert!(w.splash_count() < w.diagnostics().splashes_created as usize);
}
#[test]
fn frame_partition_does_not_change_seeded_motion_or_collision_results() {
    let pack = fixture(|m| m.placements[0].use_turbulence = true);
    let mut a = world(pack.clone());
    let mut b = world(pack);
    let camera = CameraState::default();
    a.set_environment(WeatherEnvironment {
        wind_velocity: Vec3::new(0.4, 0., -0.2),
    })
    .unwrap();
    b.set_environment(WeatherEnvironment {
        wind_velocity: Vec3::new(0.4, 0., -0.2),
    })
    .unwrap();
    for _ in 0..124 {
        a.advance(0.016, camera, 1, &mut |r| {
            plane(r, -2., WeatherSurface::Solid)
        })
        .unwrap();
    }
    for _ in 0..31 {
        b.advance(0.064, camera, 1, &mut |r| {
            plane(r, -2., WeatherSurface::Solid)
        })
        .unwrap();
    }
    let a = a.snapshot();
    let b = b.snapshot();
    assert_eq!(a.instances.len(), b.instances.len());
    for (a, b) in a.instances.iter().zip(&b.instances) {
        assert!(a.position.distance(b.position) < 0.0001);
        assert_eq!(a.uv, b.uv);
    }
}
#[test]
fn query_budget_never_exposes_unvalidated_drops_and_teleports_reseed() {
    let pack = fixture(|m| m.placements[0].drops = 8);
    let mut w = WeatherWorld::new(
        pack,
        WeatherLimits {
            queries_per_advance: 2,
            ..Default::default()
        },
        5,
    )
    .unwrap();
    w.set_map("map").unwrap();
    w.advance(0., CameraState::default(), 1, &mut |_| None)
        .unwrap();
    assert_eq!(w.snapshot().drops, 2);
    assert_eq!(w.diagnostics().pending_queries, 6);
    w.advance(0., CameraState::default(), 1, &mut |_| None)
        .unwrap();
    assert_eq!(w.snapshot().drops, 4);
    let camera = CameraState {
        position: Vec3::new(1000., 1000., 1000.),
        ..Default::default()
    };
    w.advance(0., camera, 1, &mut |_| None).unwrap();
    assert_eq!(w.snapshot().drops, 2);
    assert_eq!(w.diagnostics().teleports, 1);
    assert!(
        w.snapshot()
            .instances
            .iter()
            .all(|p| p.position.distance(camera.position) < 12.)
    );
}
#[test]
fn invalid_collision_data_is_hidden_and_retried() {
    let mut w = world(fixture(|m| m.placements[0].drops = 2));
    w.advance(0., CameraState::default(), 1, &mut |_| {
        Some(WeatherHit {
            position: Vec3::splat(f32::NAN),
            normal: Vec3::Y,
            surface: WeatherSurface::Solid,
        })
    })
    .unwrap();
    assert!(w.snapshot().instances.is_empty());
    assert_eq!(w.diagnostics().invalid_hits, 2);
    w.advance(0., CameraState::default(), 1, &mut |_| None)
        .unwrap();
    assert_eq!(w.snapshot().drops, 2);
}
#[test]
fn snow_uses_whole_atlas_cells_and_true_billboards_without_fake_splashes() {
    let mut w = world(fixture(|m| {
        m.definitions[0].true_billboards = true;
        m.definitions[0].splash_texture = None;
        m.placements[0].use_turbulence = true;
    }));
    let camera = CameraState {
        velocity: Vec3::X * 20.,
        ..Default::default()
    };
    w.advance(0., camera, 1, &mut |_| None).unwrap();
    let frame = w.snapshot();
    assert!(
        frame
            .instances
            .iter()
            .all(|p| (p.right - Vec3::X * 0.75).length() < 0.001
                && (p.up - Vec3::Y * 0.75).length() < 0.001
                && (p.uv[2] - p.uv[0] - 0.5).abs() < 0.001)
    );
    for _ in 0..20 {
        w.advance(0.032, camera, 2, &mut |r| {
            plane(r, 0., WeatherSurface::Solid)
        })
        .unwrap();
    }
    assert_eq!(w.splash_count(), 0);
    assert!(w.diagnostics().impacts > 0);
}
#[test]
fn legacy_speed_conversion_wind_and_velocity_billboards_are_explicit() {
    let mut w = world(fixture(|m| {
        let p = &mut m.placements[0];
        p.speed_per_tick = [0.032, 0.032];
        p.mass = [1., 1.];
        p.collision = false;
        p.height = 100.;
    }));
    let camera = CameraState::default();
    w.advance(0., camera, 1, &mut |_| None).unwrap();
    let before = w.snapshot().instances;
    w.advance(0.032, camera, 1, &mut |_| None).unwrap();
    let after = w.snapshot().instances;
    let by_x = |mut p: Vec<WeatherInstance>| {
        p.sort_by(|a, b| a.position.x.total_cmp(&b.position.x));
        p
    };
    let a = by_x(before);
    let b = by_x(after);
    for (a, b) in a.iter().zip(&b) {
        assert!((a.position.y - b.position.y - 0.032).abs() < 0.00001);
    }
    w.set_environment(WeatherEnvironment {
        wind_velocity: Vec3::X * 2.,
    })
    .unwrap();
    w.advance(
        0.032,
        CameraState {
            velocity: Vec3::X * 30.,
            ..camera
        },
        1,
        &mut |_| None,
    )
    .unwrap();
    assert!(w.snapshot().instances.iter().any(|p| p.up.x.abs() > 0.01));
}
#[test]
fn density_caps_stalls_and_map_teardown_are_bounded() {
    let pack = fixture(|_| {});
    let mut w = WeatherWorld::new(
        pack,
        WeatherLimits {
            drops: 16,
            steps_per_advance: 4,
            ..Default::default()
        },
        8,
    )
    .unwrap();
    w.set_map("map").unwrap();
    assert_eq!(w.drop_count(), 16);
    assert_eq!(w.diagnostics().drop_capacity_clipped, 48);
    w.set_density(0.125).unwrap();
    assert_eq!(w.drop_count(), 8);
    w.advance(60., CameraState::default(), 1, &mut |_| None)
        .unwrap();
    assert_eq!(w.diagnostics().steps, 4);
    assert!(w.diagnostics().skipped_cosmetic_seconds > 59.);
    assert!(
        w.snapshot()
            .instances
            .iter()
            .all(|p| p.position.is_finite())
    );
    assert!(
        w.advance(f64::NAN, CameraState::default(), 1, &mut |_| None)
            .is_err()
    );
    assert!(w.set_density(2.).is_err());
    w.set_map("dry-map").unwrap();
    assert_eq!(w.drop_count(), 0);
    assert!(w.snapshot().instances.is_empty());
    w.clear();
    assert!(w.map_id().is_none());
}
/// Every placement in `pack` keeps its authored drop count and draws and
/// lands drops over a solid floor in its reference wind.
fn every_placement_rains_its_authored_drops(pack: Arc<WeatherPack>) {
    assert!(!pack.manifest.placements.is_empty());
    for p in &pack.manifest.placements {
        let mut w = WeatherWorld::new(pack.clone(), WeatherLimits::default(), 54).unwrap();
        w.set_map(&p.map_id).unwrap();
        w.set_environment(WeatherEnvironment {
            wind_velocity: Vec3::from_array(p.reference_wind_velocity),
        })
        .unwrap();
        w.advance(
            0.,
            CameraState {
                position: Vec3::Y * 10.,
                ..Default::default()
            },
            1,
            &mut |r| plane(r, 0., WeatherSurface::Solid),
        )
        .unwrap();
        for _ in 0..60 {
            w.advance(
                1. / 60.,
                CameraState {
                    position: Vec3::Y * 10.,
                    ..Default::default()
                },
                1,
                &mut |r| plane(r, 0., WeatherSurface::Solid),
            )
            .unwrap();
        }
        assert_eq!(w.drop_count(), p.drops as usize);
        assert!(w.snapshot().drops > 0);
        assert!(w.diagnostics().impacts > 0);
    }
}
#[test]
fn every_placement_rains_its_authored_drops_synthetic() {
    every_placement_rains_its_authored_drops(bri_weather::testing::showcase_pack());
}
fn content_pack() -> Arc<WeatherPack> {
    WeatherPack::load(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/weather-pack-002"),
    )
    .unwrap()
}
#[test]
#[ignore = "requires generated v20 content"]
fn actual_original_weather_rains_its_authored_drops() {
    every_placement_rains_its_authored_drops(content_pack());
}
#[test]
#[ignore = "requires generated v20 content"]
fn actual_original_weather_preserves_counts_atlases_and_alpha() {
    let pack = content_pack();
    assert_eq!(pack.manifest.placements.len(), 2);
    assert_eq!(pack.textures.len(), 3);
    let snow = pack
        .manifest
        .definitions
        .iter()
        .find(|d| d.id.ends_with("snowa"))
        .unwrap();
    assert_eq!(snow.drops_per_side, 2);
    assert!(snow.splash_texture.is_none());
    let rain = pack
        .textures
        .iter()
        .find(|t| t.id.ends_with("/rain"))
        .unwrap();
    assert_eq!(rain.rgba.chunks_exact(4).map(|p| p[3]).max(), Some(51));
    let splash = pack
        .textures
        .iter()
        .find(|t| t.id.ends_with("water_splash"))
        .unwrap();
    assert_eq!(splash.rgba.chunks_exact(4).map(|p| p[3]).max(), Some(147));
}

#[test]
fn native_loader_rejects_corruption_dimensions_and_unsafe_paths() {
    use std::{fs, io::Cursor};
    let root = std::env::temp_dir().join(format!(
        "bri-weather-native-fixture-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let mut manifest = fixture(|_| {}).manifest.clone();
    let mut png = Cursor::new(Vec::new());
    image::RgbaImage::from_raw(4, 4, vec![255; 64])
        .unwrap()
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let png = png.into_inner();
    manifest.textures.get_mut("atlas").unwrap().sha256 = sha256(&png);
    fs::write(root.join("atlas.png"), &png).unwrap();
    let write = |m: &WeatherManifest| {
        fs::write(root.join("weather.json"), serde_json::to_vec(m).unwrap()).unwrap()
    };
    write(&manifest);
    assert!(WeatherPack::load(&root).is_ok());
    let mut broken = manifest.clone();
    broken.textures.get_mut("atlas").unwrap().file = "../atlas.png".into();
    write(&broken);
    assert!(WeatherPack::load(&root).is_err());
    let mut broken = manifest.clone();
    broken.textures.get_mut("atlas").unwrap().width = 8;
    write(&broken);
    assert!(WeatherPack::load(&root).is_err());
    let mut broken = manifest.clone();
    broken.textures.get_mut("atlas").unwrap().rgba_sha256 = "00".into();
    write(&broken);
    assert!(WeatherPack::load(&root).is_err());
    write(&manifest);
    fs::write(root.join("atlas.png"), [0u8; 8]).unwrap();
    assert!(WeatherPack::load(&root).is_err());
    for name in ["weather.json", "atlas.png"] {
        fs::remove_file(root.join(name)).unwrap();
    }
    fs::remove_dir(root).unwrap();
}

#[cfg(feature = "gpu")]
#[test]
fn gpu_preserves_atlas_alpha_and_reads_without_writing_host_depth() {
    pollster::block_on(async {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = instance.request_adapter(&Default::default()).await.unwrap();
        let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
        let manifest = fixture(|_| {}).manifest.clone();
        let mut rgba = Vec::new();
        for _y in 0..4 {
            for x in 0..4 {
                rgba.extend(if x < 2 {
                    [255, 0, 0, 128]
                } else {
                    [0, 255, 0, 128]
                });
            }
        }
        let pack = WeatherPack::from_parts(
            manifest,
            vec![WeatherTexture {
                id: "atlas".into(),
                width: 4,
                height: 4,
                rgba,
            }],
        )
        .unwrap();
        let mut renderer = gpu::WeatherRenderer::new(
            &device,
            &queue,
            &pack,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Depth32Float,
            1,
            8,
        )
        .unwrap();
        let particle = |z, uv| WeatherInstance {
            position: Vec3::new(0., 0., z),
            right: Vec3::X,
            up: Vec3::Y,
            uv,
            texture: 0,
            color: [1.; 4],
            splash: false,
        };
        let frame = WeatherFrame {
            instances: vec![
                particle(0.2, [0., 0., 0.5, 0.5]),
                particle(0.8, [0.5, 0., 1., 0.5]),
            ],
            drops: 2,
            splashes: 0,
        };
        renderer
            .prepare(&queue, glam::Mat4::IDENTITY, &frame)
            .unwrap();
        let size = wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        };
        // Reversed depth: 0.9 is nearer than both particles, 0 the far plane.
        for (depth_value, expected) in [(0.9, [0, 0, 0]), (0., [64, 128, 0])] {
            let texture = |format, usage| {
                device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("weather GPU contract"),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
            };
            let color = texture(
                wgpu::TextureFormat::Rgba8Unorm,
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            );
            let depth = texture(
                wgpu::TextureFormat::Depth32Float,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
            );
            let cv = color.create_view(&Default::default());
            let dv = depth.create_view(&Default::default());
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &cv,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &dv,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(depth_value),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                renderer.render(&mut pass);
            }
            let read = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 64 * 64 * 4,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            encoder.copy_texture_to_buffer(
                color.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &read,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(256),
                        rows_per_image: Some(64),
                    },
                },
                size,
            );
            queue.submit([encoder.finish()]);
            let (tx, rx) = std::sync::mpsc::channel();
            read.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                tx.send(r).unwrap();
            });
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: Some(std::time::Duration::from_secs(30)),
                })
                .unwrap();
            rx.recv_timeout(std::time::Duration::from_secs(30))
                .unwrap()
                .unwrap();
            let bytes = read.slice(..).get_mapped_range().unwrap();
            let offset = (32 * 64 + 32) * 4;
            for i in 0..3 {
                assert!(
                    (i32::from(bytes[offset + i]) - expected[i]).abs() <= 2,
                    "pixel {:?}, expected {expected:?}",
                    &bytes[offset..offset + 4]
                );
            }
        }
    });
}
