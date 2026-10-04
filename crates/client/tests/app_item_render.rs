//! Bounded normal-App render probe for original held tools.
//! Runs on the made-up content root; the ignored variants run on the
//! generated v20 content (`-- --ignored`). Loopback QUIC and an offscreen
//! GPU; never opens a window.
use anyhow::{Context, Result, ensure};
use bri_client::{
    app::App,
    platform::{PlatformApp, RenderContext},
};
use bri_ui::{
    api::*,
    gpu::{Headless, UiRenderer},
};
use std::{path::Path, time::Duration};

#[macro_use]
mod support;
use support::{content_root::ContentRoot, wait};

synthetic_and_content!(
    ContentRoot: native_core_tools_render_from_eye_and_original_mounts,
    bricks_in_hand_render_the_grey_brick_in_first_and_third_person,
    detection_regions_render_with_building_tools_and_hide_when_put_away,
);

const SIZE: (u32, u32) = (640, 480);
const BEDROOM: &str = "v20/add-ons/map_bedroom/bedroom.mis";

fn detection_regions_render_with_building_tools_and_hide_when_put_away(
    f: &ContentRoot,
) -> Result<()> {
    let artifact = f.out("detection-regions")?;
    let state = f.state()?;
    let mut app = App::load(&f.root, state.path(), SIZE)?;
    app.ui.core.pop(bri_ui::screens::ScreenId::DefaultControls);
    app.ui.core.request(UiAction::HostGame {
        map: BEDROOM.into(),
        mode: ServerMode::SinglePlayer,
        game_mode: None,
        max_players: 1,
        server_name: "Region outlines".into(),
        password: String::new(),
        admin_password: String::new(),
        super_admin_password: String::new(),
    });
    pump(&mut app)?;
    until(&mut app, "region fixture host", |a| {
        matches!(a.ui.core.conn, ConnectionState::InGame { .. })
            && a.local_motion().is_some_and(|(p, _)| p.grounded)
    })
    .with_context(|| {
        format!(
            "chat {:?}; message {:?}",
            app.ui.core.chat.lines,
            app.ui
                .screen(bri_ui::screens::ScreenId::MessageBox)
                .map(|s| s
                    .view()
                    .walk()
                    .map(|n| s.view().text_of(n))
                    .collect::<Vec<_>>())
        )
    })?;
    app.ui.core.request(UiAction::ChatCommand {
        name: "rulelab".into(),
        args: vec!["hill".into()],
    });
    pump(&mut app)?;
    wait::until_one(
        &mut app,
        "hill region replica",
        Duration::from_secs(45),
        step,
        |a| {
            a.ui.core.bottom_print.is_some()
                || a.network_view().is_some_and(|v| {
                    v.world
                        .bricks
                        .values()
                        .any(bri_world::regions::has_region_input)
                })
        },
    )
    .with_context(|| {
        format!(
            "chat {:?}; world brick count {}; message {:?}",
            app.ui.core.chat.lines,
            app.network_view().map_or(0, |v| v.world.bricks.len()),
            app.ui
                .screen(bri_ui::screens::ScreenId::MessageBox)
                .map(|s| s
                    .view()
                    .walk()
                    .map(|n| s.view().text_of(n))
                    .collect::<Vec<_>>())
        )
    })?;
    ensure!(
        app.network_view().is_some_and(|v| v
            .world
            .bricks
            .values()
            .any(bri_world::regions::has_region_input)),
        "Workshop setup rejected: {:?}",
        app.ui.core.bottom_print
    );
    let (yaw, pitch) = app.controls.view_angles();
    app.ui.core.request(UiAction::Game(GameAction::Look {
        yaw: std::f32::consts::FRAC_PI_2 - yaw,
        pitch,
    }));
    pump(&mut app)?;
    step(&mut app, Duration::from_millis(16))?;
    let gpu = support::gpu::turn()?;
    let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    app.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
    let hidden = capture(&mut app, &gpu, &mut renderer)?;
    ensure!(
        !app.region_outlines_visible(),
        "regions visible with empty hands"
    );
    let mut tool_pixels = Vec::new();
    for slot in [0, 1, 2] {
        app.ui.core.request(UiAction::UseTool { slot });
        pump(&mut app)?;
        until(&mut app, "building tool", |a| {
            a.network_view()
                .is_some_and(|v| v.tools[&v.owner].selected == Some(slot))
        })?;
        step(&mut app, Duration::from_millis(16))?;
        let pixels = capture(&mut app, &gpu, &mut renderer)?;
        ensure!(
            app.region_outlines_visible(),
            "building tool did not upload region geometry"
        );
        let cyan = pixels
            .chunks_exact(4)
            .zip(hidden.chunks_exact(4))
            .filter(|(p, old)| {
                p[0] < 130
                    && p[1] > 180
                    && p[2] > 220
                    && p.iter().zip(old.iter()).any(|(a, b)| a.abs_diff(*b) > 20)
            })
            .count();
        ensure!(
            cyan > 12,
            "region produced only {cyan} visible cyan pixels for slot {slot}"
        );
        save(&artifact.join(format!("tool-{slot}.png")), &pixels)?;
        tool_pixels = pixels;
    }
    // Exercise the same live editor draft the renderer reads, with no window.
    // Editing dimensions must move actual GPU edges without changing authority.
    let (region, authored_size) = app
        .network_view()
        .unwrap()
        .world
        .bricks
        .iter()
        .find(|(_, b)| bri_world::regions::has_region_input(b))
        .map(|(&id, b)| (id, b.rule_region))
        .unwrap();
    app.ui.core.wrench.open(
        region,
        bri_ui::api::WrenchVariant::Normal,
        "Builder".into(),
        bri_ui::api::WrenchData {
            rule_region: Some([8.0, 5.0, 8.0]),
            region_inputs: true,
            ..Default::default()
        },
        false,
        true,
    );
    let preview = capture(&mut app, &gpu, &mut renderer)?;
    let changed_edges = preview
        .chunks_exact(4)
        .zip(tool_pixels.chunks_exact(4))
        // Exclude the held tool and HUD; require visible pale preview edges.
        .take((SIZE.0 * SIZE.1 * 2 / 3) as usize)
        .filter(|(p, old)| {
            p[0] > 130
                && p[1] > 220
                && p[2] > 220
                && p.iter().zip(old.iter()).any(|(a, b)| a.abs_diff(*b) > 20)
        })
        .count();
    ensure!(
        changed_edges > 12,
        "8 x 5 x 8 draft did not move visible edges"
    );
    ensure!(
        app.network_view().unwrap().world.bricks[&region].rule_region == authored_size,
        "unsent preview changed authoritative dimensions"
    );
    save(&artifact.join("preview-8x5x8.png"), &preview)?;
    app.ui.core.wrench.close();
    let cancelled = capture(&mut app, &gpu, &mut renderer)?;
    ensure!(
        cancelled == tool_pixels,
        "Cancel did not restore authored edges"
    );
    app.ui.core.request(UiAction::UnUseTool);
    pump(&mut app)?;
    until(&mut app, "put tool away", |a| {
        a.network_view()
            .is_some_and(|v| v.tools[&v.owner].selected.is_none())
    })?;
    step(&mut app, Duration::from_millis(16))?;
    capture(&mut app, &gpu, &mut renderer)?;
    ensure!(
        !app.region_outlines_visible(),
        "put-away tool left region outlines"
    );
    app.ui.core.request(UiAction::Disconnect);
    pump(&mut app)?;
    ensure!(
        !app.region_outlines_visible(),
        "disconnect left old region outlines"
    );
    app.gpu_stopped();
    Ok(())
}

fn pump(app: &mut App) -> Result<()> {
    ensure!(
        app.pump()?.is_empty(),
        "Unexpected native window command in offscreen test"
    );
    Ok(())
}
fn step(app: &mut App, dt: Duration) -> Result<()> {
    app.tick(dt)?;
    app.ui.update(dt.as_millis() as u64);
    pump(app)
}
fn until(app: &mut App, what: &str, ready: impl Fn(&App) -> bool) -> Result<()> {
    wait::until_one(app, what, Duration::from_secs(45), step, ready).with_context(|| {
        format!(
            "chat {:?}; message {:?}",
            app.ui.core.chat.lines,
            app.ui
                .screen(bri_ui::screens::ScreenId::MessageBox)
                .map(|s| s
                    .view()
                    .walk()
                    .map(|n| s.view().text_of(n))
                    .collect::<Vec<_>>())
        )
    })
}
fn capture(app: &mut App, gpu: &Headless, renderer: &mut UiRenderer) -> Result<Vec<u8>> {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let extent = wgpu::Extent3d {
        width: SIZE.0,
        height: SIZE.1,
        depth_or_array_layers: 1,
    };
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("native held-tool offscreen target"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    ensure!(
        app.render_scene(&mut RenderContext {
            device: &gpu.device,
            queue: &gpu.queue,
            encoder: &mut encoder,
            target: &view,
            format,
            size: SIZE,
            ui_renderer: renderer
        })?,
        "App did not render a session camera"
    );
    let row = (SIZE.0 * 4).div_ceil(256) * 256;
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("held-tool readback"),
        size: u64::from(row) * u64::from(SIZE.1),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(SIZE.1),
            },
        },
        extent,
    );
    gpu.queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    gpu.device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(Duration::from_secs(30)),
    })?;
    rx.recv_timeout(Duration::from_secs(5))??;
    let mapped = buffer
        .slice(..)
        .get_mapped_range()
        .map_err(|e| anyhow::anyhow!("readback: {e:?}"))?;
    let mut pixels = Vec::with_capacity((SIZE.0 * SIZE.1 * 4) as usize);
    for line in mapped.chunks_exact(row as usize) {
        pixels.extend_from_slice(&line[..SIZE.0 as usize * 4]);
    }
    drop(mapped);
    buffer.unmap();
    Ok(pixels)
}
fn save(path: &Path, data: &[u8]) -> Result<()> {
    image::save_buffer(path, data, SIZE.0, SIZE.1, image::ColorType::Rgba8)?;
    Ok(())
}
/// Pixels that differ perceptibly. The live predicted camera can move by
/// float noise between frames, which shifts a few texels by one or two levels.
fn changed_pixels(a: &[u8], b: &[u8]) -> usize {
    a.chunks_exact(4)
        .zip(b.chunks_exact(4))
        .filter(|(x, y)| x.iter().zip(y.iter()).any(|(p, q)| p.abs_diff(*q) > 8))
        .count()
}
fn write_diff(path: &Path, a: &[u8], b: &[u8]) -> Result<()> {
    let diff = a
        .chunks_exact(4)
        .zip(b.chunks_exact(4))
        .flat_map(|(x, y)| {
            let d = (0..3)
                .map(|i| x[i].abs_diff(y[i]).saturating_mul(3))
                .collect::<Vec<_>>();
            [d[0], d[1], d[2], 255]
        })
        .collect::<Vec<_>>();
    save(path, &diff)
}

fn native_core_tools_render_from_eye_and_original_mounts(f: &ContentRoot) -> Result<()> {
    let artifact = f.out("native-world-items")?;
    let state = f.state()?;
    let mut app = App::load(&f.root, state.path(), SIZE)?;
    app.ui.core.pop(bri_ui::screens::ScreenId::DefaultControls);
    app.ui.core.request(UiAction::HostGame {
        map: BEDROOM.into(),
        mode: ServerMode::SinglePlayer,
        game_mode: None,
        max_players: 1,
        server_name: "Native held item render".into(),
        password: String::new(),
        admin_password: String::new(),
        super_admin_password: String::new(),
    });
    pump(&mut app)?;
    until(&mut app, "Bedroom host/player", |a| {
        matches!(a.ui.core.conn, ConnectionState::InGame { .. })
            && a.network_view()
                .is_some_and(|v| v.poses.contains_key(&v.owner))
    })?;
    // Spawn points sit above the floor; let the predicted player land before
    // comparing static frames.
    until(&mut app, "player landing", |a| {
        a.local_motion()
            .is_some_and(|(p, _)| p.grounded && glam::Vec3::from(p.velocity).length() < 0.001)
    })?;
    for _ in 0..30 {
        step(&mut app, Duration::from_millis(16))?;
    }
    let gpu = support::gpu::turn().context("offscreen native held-item renderer")?;
    let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    app.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;

    let angles = [
        ("front", std::f32::consts::PI, 0.08),
        ("quarter", std::f32::consts::PI * 0.5, 0.28),
        ("down", std::f32::consts::PI, -0.35),
    ];
    let mut report = serde_json::json!({"adapter":gpu.adapter_info.name,"size":SIZE,"map":BEDROOM,"visible_window":false,"audio_device":false,"cases":[]});
    for (angle, yaw, pitch) in angles {
        // GameAction::Look carries mouse deltas, with positive screen Y looking down.
        let (current_yaw, current_pitch) = app.controls.view_angles();
        let yaw_delta = (yaw - current_yaw + std::f32::consts::PI)
            .rem_euclid(2.0 * std::f32::consts::PI)
            - std::f32::consts::PI;
        app.ui.core.request(UiAction::Game(GameAction::Look {
            yaw: yaw_delta,
            pitch: current_pitch - pitch,
        }));
        pump(&mut app)?;
        ensure!(
            (app.controls.view_angles().0 - yaw).abs() < 0.02
                || (app.controls.view_angles().0 - yaw).abs() > 6.26,
            "local view yaw did not reach static test angle"
        );
        step(&mut app, Duration::from_millis(16))?;
        let baseline = capture(&mut app, &gpu, &mut renderer)?;
        save(&artifact.join(format!("none-{angle}.png")), &baseline)?;
        for (slot, label) in [(0, "hammer"), (1, "wrench"), (2, "printer")] {
            app.ui.core.request(UiAction::UseTool { slot });
            pump(&mut app)?;
            until(&mut app, "selected core tool", |a| {
                a.network_view().is_some_and(|v| {
                    let t = &v.tools[&v.owner];
                    t.selected == Some(slot) && t.slots[slot].is_some()
                })
            })?;
            step(&mut app, Duration::from_millis(16))?;
            let frame = capture(&mut app, &gpu, &mut renderer)?;
            let diff = changed_pixels(&baseline, &frame);
            ensure!(
                diff > 20,
                "{label} at {angle} changed too few visible pixels ({diff}); item may be offscreen"
            );
            ensure!(
                app.world_item_stats().visible_instances >= 1,
                "{label} at {angle} produced no mounted world-item instance"
            );
            let name = format!("{label}-{angle}");
            save(&artifact.join(format!("{name}.png")), &frame)?;
            write_diff(
                &artifact.join(format!("{name}-diff.png")),
                &baseline,
                &frame,
            )?;
            report["cases"].as_array_mut().unwrap().push(serde_json::json!({"tool":label,"view":angle,"changed_pixels":diff,
                "visible_instances":app.world_item_stats().visible_instances,"models":app.world_item_stats().cached_models,
                "geometry_slots":app.world_item_stats().geometry_slots,"pose_samples":app.world_item_stats().pose_samples}));
        }
        app.ui.core.request(UiAction::UnUseTool);
        pump(&mut app)?;
        until(&mut app, "tool deselection", |a| {
            a.network_view()
                .is_some_and(|v| v.tools[&v.owner].selected.is_none())
        })?;
        step(&mut app, Duration::from_millis(16))?;
        ensure!(
            app.world_item_stats().visible_instances == 0,
            "deselected core tool left a mounted instance"
        );
        let restored = capture(&mut app, &gpu, &mut renderer)?;
        let residual = changed_pixels(&baseline, &restored);
        save(&artifact.join(format!("none-{angle}-after.png")), &restored)?;
        ensure!(
            residual < 20,
            "deselection did not restore baseline first-person render ({residual} pixels differ at {angle})"
        );
    }

    // Third-person view puts the local player back into the real App scene and
    // uses the authored Mount0 hand transform for the same selected tool.
    app.ui.core.request(UiAction::Game(GameAction::Held {
        control: HeldControl::Zoom,
        down: true,
    }));
    app.ui
        .core
        .request(UiAction::Game(GameAction::SetZoomFov { fov: 12.0 }));
    app.ui
        .core
        .request(UiAction::Game(GameAction::ToggleFirstPerson {
            fast: false,
        }));
    pump(&mut app)?;
    app.ui.core.request(UiAction::UnUseTool);
    pump(&mut app)?;
    until(&mut app, "third-person empty hands", |a| {
        a.network_view()
            .is_some_and(|v| v.tools[&v.owner].selected.is_none())
    })?;
    step(&mut app, Duration::from_millis(16))?;
    let third_baseline = capture(&mut app, &gpu, &mut renderer)?;
    app.ui.core.request(UiAction::UseTool { slot: 1 });
    pump(&mut app)?;
    until(&mut app, "third-person wrench", |a| {
        a.network_view()
            .is_some_and(|v| v.tools[&v.owner].selected == Some(1))
    })?;
    step(&mut app, Duration::from_millis(16))?;
    let third = capture(&mut app, &gpu, &mut renderer)?;
    let third_diff = changed_pixels(&third_baseline, &third);
    ensure!(
        third_diff > 20,
        "third-person wrench changed too few visible pixels ({third_diff})"
    );
    ensure!(
        app.world_item_stats().visible_instances >= 1,
        "third-person wrench missing from world projection"
    );
    save(&artifact.join("wrench-third-person.png"), &third)?;
    save(
        &artifact.join("wrench-third-person-none.png"),
        &third_baseline,
    )?;
    write_diff(
        &artifact.join("wrench-third-person-diff.png"),
        &third_baseline,
        &third,
    )?;
    app.gpu_stopped();
    app.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
    let reset = capture(&mut app, &gpu, &mut renderer)?;
    ensure!(
        changed_pixels(&third, &reset) == 0,
        "GPU recreation changed static third-person item render"
    );
    save(&artifact.join("wrench-third-person-reset.png"), &reset)?;
    report["third_person_wrench_changed_pixels"] = third_diff.into();
    report["third_person_visible_instances"] = app.world_item_stats().visible_instances.into();
    report["third_person_models"] = app.world_item_stats().cached_models.into();
    report["third_person_geometry_slots"] = app.world_item_stats().geometry_slots.into();
    report["gpu_reset_equal_pixels"] = (SIZE.0 * SIZE.1).into();
    std::fs::write(
        artifact.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    app.gpu_stopped();
    Ok(())
}

fn holds_brick(app: &App) -> Option<bool> {
    let view = app.network_view()?;
    Some(view.weapons.images.get(&view.owner).is_some_and(|images| {
        images
            .iter()
            .any(|i| i.hand == 0 && i.image == "v20.image.brickimage")
    }))
}

fn bricks_in_hand_render_the_grey_brick_in_first_and_third_person(f: &ContentRoot) -> Result<()> {
    let artifact = f.out("native-held-brick")?;
    let state = f.state()?;
    let mut app = App::load(&f.root, state.path(), SIZE)?;
    app.ui.core.pop(bri_ui::screens::ScreenId::DefaultControls);
    app.ui.core.request(UiAction::HostGame {
        map: BEDROOM.into(),
        mode: ServerMode::SinglePlayer,
        game_mode: None,
        max_players: 1,
        server_name: "Native held brick render".into(),
        password: String::new(),
        admin_password: String::new(),
        super_admin_password: String::new(),
    });
    pump(&mut app)?;
    until(&mut app, "Bedroom host/player", |a| {
        matches!(a.ui.core.conn, ConnectionState::InGame { .. })
            && a.network_view()
                .is_some_and(|v| v.poses.contains_key(&v.owner))
    })?;
    until(&mut app, "player landing", |a| {
        a.local_motion()
            .is_some_and(|(p, _)| p.grounded && glam::Vec3::from(p.velocity).length() < 0.001)
    })?;
    for _ in 0..30 {
        step(&mut app, Duration::from_millis(16))?;
    }
    let gpu = support::gpu::turn().context("offscreen native held-brick renderer")?;
    let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    app.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
    let (current_yaw, current_pitch) = app.controls.view_angles();
    let yaw = std::f32::consts::PI;
    let yaw_delta = (yaw - current_yaw + std::f32::consts::PI)
        .rem_euclid(2.0 * std::f32::consts::PI)
        - std::f32::consts::PI;
    app.ui.core.request(UiAction::Game(GameAction::Look {
        yaw: yaw_delta,
        pitch: current_pitch - 0.08,
    }));
    pump(&mut app)?;
    let mut report = serde_json::json!({"adapter":gpu.adapter_info.name,"size":SIZE,"map":BEDROOM,"visible_window":false,"audio_device":false});
    let mut shoot = |app: &mut App, name: &str| -> Result<()> {
        app.ui.core.request(UiAction::UnUseTool);
        pump(app)?;
        until(app, "empty hands", |a| holds_brick(a) == Some(false))?;
        for _ in 0..40 {
            step(app, Duration::from_millis(16))?;
        }
        let baseline = capture(app, &gpu, &mut renderer)?;
        app.ui.core.request(UiAction::InstantUseBrick {
            brick: f.brick.clone(),
        });
        pump(app)?;
        until(app, "brick in hand", |a| holds_brick(a) == Some(true))?;
        // Let the armReady blend settle.
        for _ in 0..40 {
            step(app, Duration::from_millis(16))?;
        }
        let frame = capture(app, &gpu, &mut renderer)?;
        let diff = changed_pixels(&baseline, &frame);
        ensure!(
            diff > 20,
            "{name}: brick in hand changed too few pixels ({diff})"
        );
        ensure!(
            app.world_item_stats().visible_instances >= 1,
            "{name}: no mounted brick instance"
        );
        save(&artifact.join(format!("{name}-none.png")), &baseline)?;
        save(&artifact.join(format!("{name}.png")), &frame)?;
        write_diff(
            &artifact.join(format!("{name}-diff.png")),
            &baseline,
            &frame,
        )?;
        report[name] = serde_json::json!({"changed_pixels":diff,"visible_instances":app.world_item_stats().visible_instances});
        Ok(())
    };
    shoot(&mut app, "brick-first-person")?;
    app.ui.core.request(UiAction::Game(GameAction::Held {
        control: HeldControl::Zoom,
        down: true,
    }));
    app.ui
        .core
        .request(UiAction::Game(GameAction::SetZoomFov { fov: 12.0 }));
    app.ui
        .core
        .request(UiAction::Game(GameAction::ToggleFirstPerson {
            fast: false,
        }));
    pump(&mut app)?;
    shoot(&mut app, "brick-third-person")?;
    std::fs::write(
        artifact.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    app.gpu_stopped();
    Ok(())
}
