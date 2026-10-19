//! Real ordinary App render inputs; source-free, offscreen, no OS/gameplay UI automation.
use anyhow::{Result, ensure};
use bri_client::{
    app::App,
    platform::{PlatformApp, RenderContext},
};
use bri_ui::{
    api::*,
    gpu::{Headless, UiRenderer},
    input::{InputEvent, MouseButton},
};
use serde_json::json;
use std::time::Duration;
mod support;
use support::{content_root::ContentRoot, wait};
const SIZE: (u32, u32) = (320, 240);
fn step(app: &mut App, dt: Duration) -> Result<()> {
    app.tick(dt)?;
    app.ui.update(dt.as_millis() as u64);
    ensure!(app.pump()?.is_empty(), "unexpected native window action");
    Ok(())
}
fn until(app: &mut App, what: &str, ready: impl Fn(&App) -> bool) -> Result<()> {
    wait::until_one(app, what, Duration::from_secs(45), step, ready)
}
fn draw(
    app: &mut App,
    gpu: &Headless,
    ui: &mut UiRenderer,
    size: (u32, u32),
) -> Result<(bool, Vec<u8>)> {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("weapon observation check"),
        size: wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target = texture.create_view(&Default::default());
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    let rendered = app.render_scene(&mut RenderContext {
        device: &gpu.device,
        queue: &gpu.queue,
        encoder: &mut encoder,
        target: &target,
        format,
        size,
        ui_renderer: ui,
    })?;
    let row = (size.0 * 4).div_ceil(256) * 256;
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("local view readback"),
        size: u64::from(row) * u64::from(size.1),
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
                rows_per_image: Some(size.1),
            },
        },
        wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit([encoder.finish()]);
    support::gpu::wait(&gpu.device, "weapon observation render")?;
    let (tx, rx) = std::sync::mpsc::channel();
    buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    support::gpu::wait(&gpu.device, "local image readback")?;
    rx.recv_timeout(Duration::from_secs(5))??;
    let mapped = buffer
        .slice(..)
        .get_mapped_range()
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let mut pixels = Vec::new();
    for line in mapped.chunks_exact(row as usize) {
        pixels.extend_from_slice(&line[..size.0 as usize * 4]);
    }
    drop(mapped);
    buffer.unmap();
    Ok((rendered, pixels))
}
// Existing camera-control ABI, encoded directly to avoid a new compiler/dependency.
// Invented test selector: an ordinary world FOV below 56 requests a custom shot.
fn camera_override_module() -> Vec<u8> {
    fn uint(mut n: usize, out: &mut Vec<u8>) {
        loop {
            let b = (n & 127) as u8;
            n >>= 7;
            out.push(b | if n > 0 { 128 } else { 0 });
            if n == 0 {
                break;
            }
        }
    }
    fn text(s: &str, out: &mut Vec<u8>) {
        uint(s.len(), out);
        out.extend(s.bytes());
    }
    fn section(w: &mut Vec<u8>, id: u8, data: Vec<u8>) {
        w.push(id);
        uint(data.len(), w);
        w.extend(data);
    }
    let mut wasm = b"\0asm\x01\0\0\0".to_vec();
    section(
        &mut wasm,
        1,
        vec![
            4, 0x60, 1, 0x7f, 0, 0x60, 1, 0x7f, 1, 0x7f, 0x60, 2, 0x7d, 0x7d, 0, 0x60, 0, 0,
        ],
    );
    let mut imports = vec![3];
    for (name, ty) in [("view", 0), ("camera", 0), ("camera_set", 1)] {
        text("bri", &mut imports);
        text(name, &mut imports);
        imports.extend([0, ty]);
    }
    section(&mut wasm, 2, imports);
    section(&mut wasm, 3, vec![3, 3, 2, 2]);
    section(&mut wasm, 5, vec![1, 0, 1]);
    let mut exports = vec![4];
    for (name, kind, index) in [
        ("memory", 2, 0),
        ("init", 0, 3),
        ("frame", 0, 4),
        ("camera_frame", 0, 5),
    ] {
        text(name, &mut exports);
        exports.extend([kind, index]);
    }
    section(&mut wasm, 7, exports);
    let mut camera = vec![0, 0x41, 0, 0x10, 0, 0x41, 0, 0x2a, 2, 0, 0x43];
    camera.extend(56f32.to_le_bytes());
    camera.extend([
        0x5d, 0x04, 0x40, 0x41, 0xc0, 0, 0x10, 1, 0x41, 0xc0, 0, 0x10, 2, 0x1a, 0x0b, 0x0b,
    ]);
    let mut bodies = vec![3];
    for body in [vec![0, 0x0b], vec![0, 0x0b], camera] {
        uint(body.len(), &mut bodies);
        bodies.extend(body);
    }
    section(&mut wasm, 10, bodies);
    wasm
}
#[test]
fn ordinary_local_view_has_real_projection_pixels_effects_and_lifecycle() -> Result<()> {
    let f = ContentRoot::synthetic()?;
    let mut packages = bri_package::packages::PackageSet::load_root(&f.root)?;
    let weapon_dir = packages.role_dir(&f.root, "weapons")?;
    let item_dir = packages.role_dir(&f.root, "item_presentation")?;
    let file = weapon_dir.join("weapons.json");
    let mut weapons: serde_json::Value = serde_json::from_slice(&std::fs::read(&file)?)?;
    let gun = bri_weapons::testing::GUN_IMAGE;
    let item = bri_weapons::testing::GUN_ITEM;
    for state in weapons["images"][gun]["states"].as_array_mut().unwrap() {
        if state["name"] == "Fire" {
            state["emitter"] = json!("emitter");
            state["emitter_node"] = json!("muzzlePoint");
            state["emitter_seconds"] = json!(1.5);
        }
    }
    let policy = bri_weapons::FirstPersonView {
        horizontal_fov: 62.,
        near: 0.03,
        far: 400.,
        scale: 0.7,
        eye_offset_reference_fov: Some(100.),
        reference_aspect: None,
    };
    weapons["images"][gun]["first_person_view"] = serde_json::to_value(policy)?;
    weapons["images"][gun]["eye_offset"] = json!([0.18, -0.2, -0.7]);
    weapons["images"][gun]["first_person_origin"] = json!("eye");
    weapons["images"][gun]["follow_arm"] = json!(false);
    let bytes = serde_json::to_vec(&weapons)?;
    bri_weapons::Pack::from_json(&bytes)?;
    std::fs::write(file, &bytes)?;
    let presentation_file = item_dir.join("presentation.json");
    let mut presentation: bri_client::items::Presentation =
        serde_json::from_slice(&std::fs::read(&presentation_file)?)?;
    presentation.weapons_sha256 = bri_client::testing::sha256(&bytes);
    let image = presentation.images.get_mut(gun).unwrap();
    image.eye_offset = [0.18, -0.2, -0.7];
    image.first_person_origin = Some(bri_weapons::FirstPersonOrigin::Eye);
    image.follow_arm = false;
    std::fs::write(presentation_file, serde_json::to_vec(&presentation)?)?;
    let rules = f.root.join("frame-probe");
    std::fs::create_dir(&rules)?;
    std::fs::write(rules.join("package.json"),json!({"schema_version":1,"id":"frameprobe","version":"1.0.0","api":1,"name":"Frame probe fixture","license":"CC0-1.0","capabilities":["player"],"provides":[{"kind":"behaviour","id":"frameprobe:behaviour/main","file":"behaviour.json"},{"kind":"script","id":"frameprobe:script/main","file":"main.rhai"}]}).to_string())?;
    std::fs::write(
        rules.join("behaviour.json"),
        json!({"schema_version":1,"script":"main.rhai","on_loadout":true,"commands":[{"name":"narrow"},{"name":"normal"},{"name":"custom"}]}).to_string(),
    )?;
    std::fs::write(
        rules.join("main.rhai"),
        format!(
            "fn on_loadout(p) {{ require(give_item(p, \"{item}\", false)); }} fn cmd_narrow(p) {{ require(set_fov(p,60.0)); }} fn cmd_normal(p) {{ require(set_fov(p,90.0)); }} fn cmd_custom(p) {{ require(set_fov(p,55.0)); }}"
        ),
    )?;
    packages.packages.push(bri_package::packages::PackageEntry {
        id: "frameprobe".into(),
        version: "1.0.0".into(),
        side: bri_package::packages::Side::Server,
        dir: "frame-probe".into(),
        role: None,
    });
    let code = f.root.join("camera-probe");
    std::fs::create_dir(&code)?;
    std::fs::write(code.join("main.wasm"), camera_override_module())?;
    std::fs::write(code.join("package.json"),json!({"schema_version":1,"api":1,"id":"camera-probe","version":"1.0.0","name":"Source-free camera override","license":"CC0-1.0","provides":[],"provenance":{"source":"original"},"client":{"module":"main.wasm","capabilities":["render.layer","world.read","camera.control"]}}).to_string())?;
    packages.packages.push(bri_package::packages::PackageEntry {
        id: "camera-probe".into(),
        version: "1.0.0".into(),
        side: bri_package::packages::Side::Shared,
        dir: "camera-probe".into(),
        role: None,
    });
    std::fs::write(f.root.join("packages.json"), serde_json::to_vec(&packages)?)?;
    let state = f.state()?;
    let mut app = App::load(&f.root, state.path(), SIZE)?;
    app.host_on_any_port();
    app.ui.core.pop(bri_ui::screens::ScreenId::DefaultControls);
    app.ui.core.request(UiAction::HostGame {
        map: f.map.0.clone(),
        mode: ServerMode::SinglePlayer,
        game_mode: None,
        max_players: 1,
        server_name: "Weapon frame observation".into(),
        password: String::new(),
        admin_password: String::new(),
        super_admin_password: String::new(),
    });
    until(&mut app, "ordinary host/loadout", |a| {
        a.network_view().is_some_and(|v| {
            v.tools
                .get(&v.owner)
                .is_some_and(|t| t.slots.iter().any(|i| i.as_deref() == Some(item)))
        })
    })?;
    let (owner, slot) = {
        let v = app.network_view().unwrap();
        (
            v.owner,
            v.tools[&v.owner]
                .slots
                .iter()
                .position(|i| i.as_deref() == Some(item))
                .unwrap(),
        )
    };
    app.ui.core.request(UiAction::UseTool { slot });
    until(&mut app, "ordinary gun equip", |a| {
        a.mounted_image_observation(owner, 0, gun).is_some()
    })?;
    assert!(
        app.mounted_image_observation(owner, 0, "wrong:image")
            .is_none()
    );
    assert!(app.mounted_image_observation(owner, 1, gun).is_none());

    if !app.ui.core.first_person {
        app.ui
            .core
            .request(UiAction::Game(GameAction::ToggleFirstPerson { fast: true }));
    }
    until(&mut app, "local eye", |a| {
        a.mounted_image_observation(owner, 0, gun)
            .is_some_and(|m| m.first_person)
    })?;
    until(&mut app, "grounded", |a| {
        a.local_motion().is_some_and(|(p, _)| p.grounded)
    })?;
    let gpu = support::gpu::turn()?;
    let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    app.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
    app.set_weapon_frame_probe(true);
    let out = std::env::var_os("BRI_FIRST_PERSON_VIEW_OUT")
        .map(std::path::PathBuf::from)
        .map(Ok)
        .unwrap_or_else(|| f.out("first-person-view"))?;
    std::fs::create_dir_all(&out)?;
    for (size, aa, scale) in [((320, 240), "1", "100"), ((480, 270), "0", "75")] {
        let mut settings = app.ui.settings();
        settings
            .prefs
            .insert(bri_client::graphics::ANTI_ALIASING.into(), aa.into());
        settings
            .prefs
            .insert("$pref::Video::RenderScale".into(), scale.into());
        settings
            .prefs
            .insert(bri_ui::screens::options::LIGHTING.into(), "3".into());
        app.ui
            .core
            .request(UiAction::SaveSettings(Box::new(settings)));
        step(&mut app, Duration::ZERO)?;
        app.gpu_stopped();
        app.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
        let (drawn, pixels) = draw(&mut app, &gpu, &mut renderer, size)?;
        ensure!(drawn, "ordinary local render");
        let observation = app.weapon_frame_observation().unwrap();
        assert_eq!(observation.local_views.len(), 1);
        let local = &observation.local_views[0];
        assert_eq!(local.affinity.image, gun);
        assert_eq!(local.policy, policy);
        // With zero yaw/pitch this is the source-independent fixed horizontal law.
        let expected = 1. / (62f32.to_radians() * 0.5).tan();
        assert!((local.camera.view_projection.x_axis.x - expected).abs() < 1e-4);
        assert!(
            (local.camera.view_projection.y_axis.y
                - expected * local.viewport[0] as f32 / local.viewport[1] as f32)
                .abs()
                < 1e-4
        );
        assert_ne!(
            local.camera.view_projection,
            observation.camera.view_projection
        );
        assert_eq!((observation.cut_sprites, observation.cut_lights), (0, 0));
        image::save_buffer(
            out.join(format!("local-{}x{}.png", size.0, size.1)),
            &pixels,
            size.0,
            size.1,
            image::ColorType::Rgba8,
        )?;
        // Same camera and animation time, before firing: only local geometry
        // eligibility changes. Smoke/FOV/camera transitions cannot prove this.
        let world_matrix = observation.camera.view_projection;
        let submitted_model_draws = app.render_stats().unwrap().reflection_draws;
        assert!(observation.local_views[0].weapon_frame.particles.is_empty());
        app.ui.core.prefs.set("$pref::Player::renderMyItems", "0");
        step(&mut app, Duration::ZERO)?;
        let (drawn, plain) = draw(&mut app, &gpu, &mut renderer, size)?;
        ensure!(drawn, "same-frame suppressed model render");
        let suppressed = app.weapon_frame_observation().unwrap();
        assert_eq!(
            suppressed.camera.view_projection, world_matrix,
            "visibility comparison must keep identical world camera"
        );
        assert!(suppressed.local_views.is_empty());
        assert!(
            submitted_model_draws > app.render_stats().unwrap().reflection_draws,
            "actual local model draw submitted, independent of lighting changes"
        );
        assert_eq!(
            app.weapon_effect_counts().1,
            0,
            "FX cannot supply model visibility evidence"
        );
        let changed = pixels
            .chunks_exact(4)
            .zip(plain.chunks_exact(4))
            .filter(|(a, b)| a[..3] != b[..3])
            .count();
        assert!(
            changed > 20,
            "local model did not change submitted pixels: {changed}"
        );
        eprintln!(
            "LOCAL_VIEW_MODEL_PIXELS size={size:?} aa={aa} scale={scale} changed={changed} same_world_matrix=true no_weapon_particles=true"
        );
        app.ui.core.prefs.set("$pref::Player::renderMyItems", "1");
        step(&mut app, Duration::ZERO)?;
        draw(&mut app, &gpu, &mut renderer, size)?;
    }
    app.ui.handle_input(InputEvent::MouseDown {
        button: MouseButton::Left,
        x: 160.,
        y: 120.,
    });
    until(&mut app, "ordinary projected transient", |a| {
        a.weapon_effect_counts().1 > 0 && a.weapon_effect_diagnostics().accepted_cues > 0
    })?;
    draw(&mut app, &gpu, &mut renderer, (480, 270))?;
    let observation = app.weapon_frame_observation().unwrap();
    assert!(
        !observation.local_views[0].weapon_frame.particles.is_empty(),
        "actual projected particles, not just a matrix"
    );
    assert!(observation.local_views[0].weapon_frame.lights.is_empty());
    assert_eq!(observation.cut_sprites, 0);
    let local_before = observation.local_views[0].camera.view_projection;
    let world_before = observation.camera.view_projection.x_axis;
    app.ui.handle_input(InputEvent::MouseUp {
        button: MouseButton::Left,
        x: 160.,
        y: 120.,
    });
    app.ui.core.request(UiAction::Game(GameAction::Package {
        package: "frameprobe".into(),
        command: "narrow".into(),
        pressed: None,
    }));
    wait::until_one(
        &mut app,
        "resolved world FOV change",
        Duration::from_secs(10),
        |a, dt| {
            step(a, dt)?;
            draw(a, &gpu, &mut renderer, (480, 270)).map(drop)
        },
        |a| {
            a.weapon_frame_observation().is_some_and(|o| {
                o.camera.view_projection.x_axis != world_before
                    && o.local_views
                        .first()
                        .is_some_and(|l| (l.world_horizontal_fov - 60.).abs() < 0.01)
            })
        },
    )?;
    draw(&mut app, &gpu, &mut renderer, (480, 270))?;
    let observation = app.weapon_frame_observation().unwrap();
    assert_eq!(
        observation.local_views[0].camera.view_projection.x_axis, local_before.x_axis,
        "local projection independent of world FOV"
    );
    assert!(
        app.add_on_code_running()
            .contains(&"Source-free camera override")
    );
    app.ui.core.request(UiAction::Game(GameAction::Package {
        package: "frameprobe".into(),
        command: "custom".into(),
        pressed: None,
    }));
    wait::until_one(
        &mut app,
        "CURRENT custom shot suppresses local layers",
        Duration::from_secs(10),
        |a, dt| {
            step(a, dt)?;
            draw(a, &gpu, &mut renderer, (480, 270)).map(drop)
        },
        |a| {
            a.weapon_frame_observation()
                .is_some_and(|o| o.local_views.is_empty())
                && a.mounted_image_observation(owner, 0, gun)
                    .is_some_and(|m| m.first_person)
        },
    )?;
    app.ui.core.request(UiAction::Game(GameAction::Package {
        package: "frameprobe".into(),
        command: "normal".into(),
        pressed: None,
    }));
    wait::until_one(
        &mut app,
        "stock eye restores fresh local projection",
        Duration::from_secs(10),
        |a, dt| {
            step(a, dt)?;
            draw(a, &gpu, &mut renderer, (480, 270)).map(drop)
        },
        |a| {
            a.weapon_frame_observation()
                .is_some_and(|o| o.local_views.len() == 1)
        },
    )?;
    app.ui
        .core
        .request(UiAction::Game(GameAction::ToggleFirstPerson { fast: true }));
    until(&mut app, "third person", |a| {
        a.mounted_image_observation(owner, 0, gun)
            .is_some_and(|m| !m.first_person)
    })?;
    draw(&mut app, &gpu, &mut renderer, (480, 270))?;
    assert!(
        app.weapon_frame_observation()
            .unwrap()
            .local_views
            .is_empty()
    );
    app.ui
        .core
        .request(UiAction::Game(GameAction::ToggleFirstPerson { fast: true }));
    until(&mut app, "back to eye", |a| {
        a.mounted_image_observation(owner, 0, gun)
            .is_some_and(|m| m.first_person)
    })?;
    app.ui.core.request(UiAction::UnUseTool);
    until(&mut app, "ordinary holster", |a| {
        a.mounted_image_observation(owner, 0, gun).is_none()
    })?;
    let (drawn, _) = draw(&mut app, &gpu, &mut renderer, (480, 270))?;
    ensure!(drawn, "holstered render");
    assert!(
        app.weapon_frame_observation()
            .unwrap()
            .local_views
            .is_empty()
    );
    app.ui.core.request(UiAction::Disconnect);
    step(&mut app, Duration::ZERO)?;
    assert!(app.weapon_frame_observation().is_none());
    app.gpu_stopped();
    Ok(())
}

#[test]
fn current_exact_two_image_policies_keep_tp_reflections_and_scope_lifecycle_separate() -> Result<()>
{
    use bri_client::{items::ItemAssets, world_items::*};
    use glam::{Mat4, Vec3};
    use std::{collections::BTreeMap, sync::Arc};
    let f = support::item_fixture::ItemFixture::synthetic()?;
    let assets = Arc::new(ItemAssets::load(&f.presentation, &f.weapons)?);
    let mut pack = bri_weapons::Pack::from_json(&std::fs::read(f.weapons.join("weapons.json"))?)?;
    let policy = |fov, scale| bri_weapons::FirstPersonView {
        horizontal_fov: fov,
        near: 0.03,
        far: 400.,
        scale,
        eye_offset_reference_fov: None,
        reference_aspect: None,
    };
    pack.images
        .get_mut(&f.right_image)
        .unwrap()
        .first_person_view = Some(policy(62., 0.5));
    pack.images
        .get_mut(&f.left_image)
        .unwrap()
        .first_person_view = Some(policy(78., 0.75));
    let mut items = WorldItems::new(assets, Arc::new(pack), WorldItemLimits::default())?;
    items.set_first_person_fov(80.);
    let mounted = |image: &str, hand| bri_sim::session::MountedImage {
        image: image.into(),
        state: "Ready".into(),
        hand,
        paint: None,
    };
    let view = bri_sim::session::WeaponView {
        images: BTreeMap::from([
            (
                1,
                vec![mounted(&f.right_image, 0), mounted(&f.left_image, 1)],
            ),
            (2, vec![mounted(&f.right_image, 0)]),
        ]),
        ..Default::default()
    };
    let frame = WorldItemFrame {
        tick: 10,
        seconds: 1.,
        eye: Vec3::ZERO,
        local_owner: Some(1),
        first_person: true,
        reflected_self: true,
        held_values: Default::default(),
    };
    let pose = |_| {
        Some(MountPose {
            eye: Mat4::IDENTITY,
            mounts: BTreeMap::from([
                (0, Mat4::from_translation(Vec3::new(0.2, -0.1, -1.))),
                (1, Mat4::from_translation(Vec3::new(-0.2, -0.1, -1.))),
            ]),
            actions: BTreeMap::new(),
            velocity: Vec3::ZERO,
            straddle: None,
        })
    };
    items.sync(&view, frame, pose)?;
    let views = items.local_views(1);
    assert_eq!(views.len(), 2);
    assert_ne!(views[0].0.policy, views[1].0.policy);
    assert!(items.local_views(2).is_empty());
    let local = items.mounted_transform(1, 0).unwrap();
    let remote = items.mounted_transform(2, 0).unwrap();
    assert!((local.x_axis.truncate().length() - 0.5).abs() < 1e-5);
    assert!((remote.x_axis.truncate().length() - 1.).abs() < 1e-5);
    assert!(
        local.w_axis.distance(remote.w_axis) < 1e-5,
        "visual scale must not scale actor translation"
    );
    let reflected = items
        .instances()
        .find(|(id, _)| *id == ItemIdentity::Reflected(1, 0))
        .unwrap()
        .1;
    assert_eq!(
        reflected.transform, remote,
        "local reflected model retains third-person placement/scale"
    );
    items.set_scoped(true);
    assert!(items.local_views(1).is_empty());
    items.set_scoped(false);
    assert_eq!(items.local_views(1).len(), 2);
    items.sync(
        &bri_sim::session::WeaponView::default(),
        WorldItemFrame {
            tick: 11,
            seconds: 1.1,
            ..frame
        },
        pose,
    )?;
    assert!(items.local_views(1).is_empty());
    items.reset();
    assert!(
        items
            .mounted_image_observation(1, 0, &f.right_image)
            .is_none()
    );
    Ok(())
}
