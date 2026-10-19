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
fn draw(app: &mut App, gpu: &Headless, ui: &mut UiRenderer) -> Result<bool> {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("weapon observation check"),
        size: wgpu::Extent3d {
            width: SIZE.0,
            height: SIZE.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
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
        size: SIZE,
        ui_renderer: ui,
    })?;
    gpu.queue.submit([encoder.finish()]);
    support::gpu::wait(&gpu.device, "weapon observation render")?;
    Ok(rendered)
}
#[test]
fn actual_prepared_weapon_frame_is_opt_in_and_current() -> Result<()> {
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
    let bytes = serde_json::to_vec(&weapons)?;
    bri_weapons::Pack::from_json(&bytes)?;
    std::fs::write(file, &bytes)?;
    let presentation_file = item_dir.join("presentation.json");
    let mut presentation: bri_client::items::Presentation =
        serde_json::from_slice(&std::fs::read(&presentation_file)?)?;
    presentation.weapons_sha256 = bri_client::testing::sha256(&bytes);
    std::fs::write(presentation_file, serde_json::to_vec(&presentation)?)?;
    let rules = f.root.join("frame-probe");
    std::fs::create_dir(&rules)?;
    std::fs::write(rules.join("package.json"),json!({"schema_version":1,"id":"frameprobe","version":"1.0.0","api":1,"name":"Frame probe fixture","license":"CC0-1.0","capabilities":["player"],"provides":[{"kind":"behaviour","id":"frameprobe:behaviour/main","file":"behaviour.json"},{"kind":"script","id":"frameprobe:script/main","file":"main.rhai"}]}).to_string())?;
    std::fs::write(
        rules.join("behaviour.json"),
        json!({"schema_version":1,"script":"main.rhai","on_loadout":true}).to_string(),
    )?;
    std::fs::write(
        rules.join("main.rhai"),
        format!("fn on_loadout(p) {{ require(give_item(p, \"{item}\", false)); }}"),
    )?;
    packages.packages.push(bri_package::packages::PackageEntry {
        id: "frameprobe".into(),
        version: "1.0.0".into(),
        side: bri_package::packages::Side::Server,
        dir: "frame-probe".into(),
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
    let gpu = support::gpu::turn()?;
    let mut ui = UiRenderer::new(&gpu.device, &gpu.queue);
    app.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
    ensure!(draw(&mut app, &gpu, &mut ui)?, "default-off render");
    assert!(app.weapon_frame_observation().is_none());
    app.set_weapon_frame_probe(true);
    app.ui.handle_input(InputEvent::MouseDown {
        button: MouseButton::Left,
        x: 160.,
        y: 120.,
    });
    until(&mut app, "ordinary admitted transient", |a| {
        a.weapon_effect_counts().0 > 0
            && a.weapon_effect_counts().1 > 0
            && a.weapon_effect_diagnostics().accepted_cues > 0
    })?;
    ensure!(draw(&mut app, &gpu, &mut ui)?, "opt-in render");
    let observation = app
        .weapon_frame_observation()
        .expect("actual successful render capture");
    assert_eq!(observation.owner, owner);
    assert_eq!(observation.replica_tick, app.network_view().unwrap().tick);
    assert!(observation.camera.view_projection.is_finite());
    assert_eq!(
        observation.camera.position,
        app.rendered_camera().unwrap().0
    );
    assert!(
        !observation.attachments.is_empty(),
        "accepted attachment reaches real renderer path"
    );
    eprintln!(
        "WEAPON_RENDER_PROBE particles={} camera={:?} attachments={:?}",
        app.weapon_effect_counts().1,
        observation.camera,
        observation.attachments
    );
    assert!(
        !observation.weapon_frame.particles.is_empty(),
        "actual prebudget weapon sprites"
    );
    assert_eq!((observation.cut_sprites, observation.cut_lights), (0, 0));
    assert!(observation.combined_particles >= observation.weapon_frame.particles.len());
    let mounted = observation
        .mounted_images
        .iter()
        .find(|m| m.owner == owner && m.hand == 0 && m.image == gun)
        .unwrap();
    let current = app.mounted_image_observation(owner, 0, gun).unwrap();
    assert_eq!(mounted.transform, current.transform);
    assert_eq!(mounted.sequence, current.sequence);
    assert_eq!(mounted.seconds.to_bits(), current.seconds.to_bits());
    assert_eq!(mounted.live, current.live);
    app.set_weapon_frame_probe(false);
    assert!(app.weapon_frame_observation().is_none());
    ensure!(draw(&mut app, &gpu, &mut ui)?, "off again render");
    assert!(app.weapon_frame_observation().is_none());
    app.set_weapon_frame_probe(true);
    ensure!(draw(&mut app, &gpu, &mut ui)?, "fresh reenabled render");
    assert!(app.weapon_frame_observation().is_some());
    app.ui.core.request(UiAction::Disconnect);
    step(&mut app, Duration::ZERO)?;
    assert!(
        app.weapon_frame_observation().is_none(),
        "disconnect drops previous frame"
    );
    assert!(
        !draw(&mut app, &gpu, &mut ui)?,
        "no entered session early return"
    );
    assert!(app.weapon_frame_observation().is_none());
    app.gpu_stopped();
    Ok(())
}
