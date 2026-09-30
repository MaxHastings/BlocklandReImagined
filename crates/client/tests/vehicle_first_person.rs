//! First person in every seat of a Tank, for a guest who joined a LAN game
//! and for the host: the rendered camera sits where v20 puts the rider's
//! eye and faces the way the seat does. `Player::getCameraTransform` at
//! `pos` 0 (blocklandv20.exe 0x5ab7d0) gives every rider of a vehicle
//! `getRenderEyeTransform`: the posed `eye` node through the seat, turned
//! with it. In third person a passenger keeps their own camera; only a
//! player with a control object (the driver) hands it to the Tank.
//! The expected eye is built here from the vehicle pack and the avatar rig,
//! not from the app. The Tank's seats hold `root`, so the old fixed 1.6 over
//! the seat sat the eye about half a unit too low, inside the hull.
//! Run: cargo test -p bri-client --test vehicle_first_person -- --ignored --nocapture
//! Requires converted v20 content, loopback QUIC and an offscreen GPU; never
//! opens a window or moves the mouse.
use anyhow::{Context, Result, ensure};
use bri_client::{
    app::App,
    avatar::{AvatarAnimationInput, AvatarAssets},
    platform::{PlatformApp, RenderContext},
};
use bri_ui::{
    api::*,
    gpu::{Headless, UiRenderer},
    screens::ScreenId,
};
use bri_vehicles::schema::{Definition, Pack, SeatRole};
use glam::{Mat4, Quat, Vec3};
use sha2::Digest;
use std::{
    collections::BTreeSet,
    path::Path,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const SIZE: (u32, u32) = (320, 240);
const SLATE: &str = "v20/add-ons/map_slate/slate.mis";
const TANK: &str = "v20.vehicle.tankvehicle";
const VEHICLE_SPAWN: &str = "v20/brick/brickvehiclespawndata";

fn request(app: &mut App, action: UiAction) -> Result<()> {
    app.ui.core.request(action);
    ensure!(app.pump()?.is_empty(), "Unexpected window command");
    app.ui.update(0);
    Ok(())
}
fn step(app: &mut App, dt: Duration) -> Result<()> {
    app.tick(dt)?;
    app.ui.update(dt.as_millis() as u64);
    ensure!(app.pump()?.is_empty(), "Unexpected window command");
    Ok(())
}
/// Run every app for `seconds`, or until `ready` (failing after `secs`).
fn until(
    apps: &mut [&mut App],
    what: &str,
    secs: u64,
    mut ready: impl FnMut(&mut [&mut App]) -> Result<bool>,
) -> Result<()> {
    let start = Instant::now();
    let mut previous = start;
    loop {
        let now = Instant::now();
        for app in apps.iter_mut() {
            step(app, now.duration_since(previous))?;
        }
        previous = now;
        if ready(apps)? {
            return Ok(());
        }
        ensure!(
            start.elapsed() < Duration::from_secs(secs),
            "Timed out waiting for {what}"
        );
        thread::sleep(Duration::from_millis(8));
    }
}
fn run_for(apps: &mut [&mut App], seconds: f32) -> Result<()> {
    let start = Instant::now();
    until(apps, "time", 600, |_| {
        Ok(start.elapsed().as_secs_f32() >= seconds)
    })
}
fn in_game(app: &App) -> bool {
    matches!(app.ui.core.conn, ConnectionState::InGame { .. })
        && app
            .network_view()
            .is_some_and(|v| v.poses.contains_key(&v.owner))
}
fn free_port() -> Result<u16> {
    Ok(std::net::UdpSocket::bind("127.0.0.1:0")?
        .local_addr()?
        .port())
}
fn app(content: &Path, state: &Path, name: &str) -> Result<App> {
    let state = state.join(name);
    let _ = std::fs::remove_dir_all(&state);
    let mut app = App::load(content, &state, SIZE)?;
    app.ui.core.pop(ScreenId::DefaultControls);
    app.ui.core.settings.avatar.lan_name = name.into();
    Ok(app)
}
fn seat(app: &App) -> Option<(u64, usize)> {
    let view = app.network_view()?;
    let (vehicle, seat) = view.vitals.get(&view.owner)?.mounted?;
    Some((vehicle, usize::from(seat)))
}
/// Load a Tank spawn brick eight units ahead of the host's player.
fn load_tank(app: &mut App, state: &Path) -> Result<()> {
    load_vehicle(app, state, TANK)
}
/// Load a spawn brick for `vehicle` eight units ahead of the host's player.
fn load_vehicle(app: &mut App, state: &Path, vehicle: &str) -> Result<()> {
    let view = app.network_view().context("not in a game")?;
    let player = &view.poses.get(&view.owner).context("no player")?.player;
    let feet = Vec3::from(player.feet);
    let ahead = Vec3::new(player.yaw.sin(), 0.0, -player.yaw.cos()) * 8.0;
    let map_id = view.world.map_id.clone();
    let mut world =
        bri_world::World::new("Tank".into(), map_id.clone(), view.world.palette.clone());
    let mut brick = bri_world::Brick::new(
        bri_world::ContentRef::Resolved(VEHICLE_SPAWN.into()),
        [
            ((feet.x + ahead.x) * 2.0).round() / 2.0,
            (feet.y / 0.2).round() * 0.2 + 0.1,
            ((feet.z + ahead.z) * 2.0).round() / 2.0,
        ],
        view.owner,
    );
    brick.vehicle = Some(bri_world::VehicleSpawn {
        vehicle: bri_world::ContentRef::Resolved(vehicle.into()),
        recolor: false,
    });
    world.bricks.insert(1, brick);
    world.next_brick_id = 2;
    let folder = state
        .join("saves")
        .join(format!("map-{:x}", sha2::Sha256::digest(map_id.as_bytes())));
    std::fs::create_dir_all(&folder)?;
    std::fs::write(
        folder.join("tank.world.json"),
        serde_json::to_vec(&bri_world::build::SavedBuild::new(world))?,
    )?;
    let map = app
        .content
        .maps
        .iter()
        .find(|m| m.id == map_id)
        .context("the map's name")?
        .name
        .clone();
    request(
        app,
        UiAction::LoadBricks {
            map,
            name: "tank.world.json".into(),
            ownership: true,
        },
    )
}
fn held(app: &mut App, control: HeldControl, down: bool) -> Result<()> {
    request(app, UiAction::Game(GameAction::Held { control, down }))
}
/// Run and jump at the Tank until `apps[rider]` boards it.
fn board(apps: &mut [&mut App], rider: usize) -> Result<()> {
    for attempt in 0..160 {
        if seat(apps[rider]).is_some() {
            break;
        }
        let app = &mut *apps[rider];
        let view = app.network_view().context("view")?;
        let target = view
            .vehicle_poses
            .values()
            .next()
            .map(|p| Vec3::from(p.position))
            .context("no tank")?;
        let (player, _) = app.local_motion().context("player")?;
        let to = target - Vec3::from(player.feet);
        let yaw = to.x.atan2(-to.z);
        let (current, _) = app.controls.view_angles();
        let turn = (yaw - current + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        request(
            app,
            UiAction::Game(GameAction::Look {
                yaw: turn,
                pitch: 0.0,
            }),
        )?;
        held(app, HeldControl::Forward, true)?;
        held(app, HeldControl::Jump, attempt % 2 == 0)?;
        run_for(apps, 0.25)?;
    }
    held(apps[rider], HeldControl::Forward, false)?;
    held(apps[rider], HeldControl::Jump, false)?;
    ensure!(seat(apps[rider]).is_some(), "never boarded the Tank");
    Ok(())
}
/// The rider's `eye` node in the seat's pose, in its own shape space.
fn eye_node(assets: &AvatarAssets, sitting: bool) -> Result<Vec3> {
    let mut body = assets.mesh(assets.package.defaults.clone())?;
    let rider = bri_sim::player::PlayerState {
        owner: 1,
        feet: [0.0; 3],
        velocity: [0.0; 3],
        yaw: 0.0,
        pitch: 0.0,
        head_yaw: 0.0,
        grounded: true,
        crouched: false,
        jetting: false,
        jump: Default::default(),
        archetype: Default::default(),
        scale: 1.0,
        energy: 100.0,
        tick: Default::default(),
    };
    for time in [0.0, 1.0] {
        body.pose_with_animation(
            assets,
            &rider,
            time,
            &AvatarAnimationInput {
                sitting,
                ..Default::default()
            },
        )?;
    }
    Ok(body
        .model_node(assets, "Eye")
        .context("eye node")?
        .w_axis
        .truncate())
}
/// Where v20 puts `app`'s first-person eye in its current seat.
fn expected_eye(app: &App, tank: &Definition, assets: &AvatarAssets) -> Result<Vec3> {
    let (vehicle, index) = seat(app).context("not seated")?;
    let pose = app
        .network_view()
        .and_then(|v| v.vehicle_poses.get(&vehicle))
        .context("tank pose")?;
    let s = &tank.seats[index];
    let eye = eye_node(assets, s.pose == "sit")?;
    let node = Vec3::from(s.transform.position);
    let world =
        Mat4::from_rotation_translation(Quat::from_array(pose.rotation), Vec3::from(pose.position));
    let mut seat = Mat4::from_rotation_translation(Quat::from_array(s.transform.rotation), node);
    // The gunner rides the turret, turned by its aim about its mount.
    if let (Some(mount), SeatRole::Gunner) = (&tank.attachment_mount, tank.seat_role(index)) {
        let pivot = Vec3::from(mount.position);
        seat = Mat4::from_translation(pivot)
            * Mat4::from_rotation_y(pose.turret_aim[0])
            * Mat4::from_translation(-pivot)
            * seat;
    }
    Ok((world * seat).transform_point3(eye))
}
fn render(app: &mut App, gpu: &Headless, renderer: &mut UiRenderer) -> Result<Vec3> {
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("first-person offscreen target"),
        size: wgpu::Extent3d {
            width: SIZE.0,
            height: SIZE.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let target = texture.create_view(&Default::default());
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    ensure!(
        app.render_scene(&mut RenderContext {
            device: &gpu.device,
            queue: &gpu.queue,
            encoder: &mut encoder,
            target: &target,
            format: wgpu::TextureFormat::Rgba8Unorm,
            size: SIZE,
            ui_renderer: renderer,
        })?,
        "App did not render a session camera"
    );
    gpu.queue.submit([encoder.finish()]);
    Ok(app.rendered_camera().context("rendered camera")?.0)
}
/// Check `apps[rider]`'s first-person camera in its seat against v20's.
fn check(
    apps: &mut [&mut App],
    rider: usize,
    who: &str,
    tank: &Definition,
    assets: &AvatarAssets,
    gpu: &Headless,
    renderer: &mut UiRenderer,
) -> Result<usize> {
    run_for(apps, 1.0)?;
    let app = &mut *apps[rider];
    ensure!(
        !app.controls.third_person_view(),
        "{who} is not in first person"
    );
    let (_, index) = seat(app).context("not seated")?;
    let eye = render(app, gpu, renderer)?;
    let expected = expected_eye(app, tank, assets)?;
    println!(
        "{who}: seat {index} ({:?}) eye {eye} expected {expected}",
        tank.seat_role(index)
    );
    ensure!(
        eye.distance(expected) < 0.05,
        "{who} seat {index}: first-person eye {eye}, v20 {expected}"
    );
    // A driver or passenger looks the way the seat faces, pitched and
    // rolled with the hull, until they move their head.
    if tank.seat_role(index) != SeatRole::Gunner {
        let (vehicle, _) = seat(app).context("not seated")?;
        let pose = app
            .network_view()
            .and_then(|v| v.vehicle_poses.get(&vehicle))
            .context("tank pose")?;
        let expected = Quat::from_array(pose.rotation)
            * Quat::from_array(tank.seats[index].transform.rotation);
        let (_, yaw, pitch) = app.rendered_camera().context("rendered camera")?;
        let drawn = Quat::from_rotation_y(-yaw)
            * Quat::from_rotation_x(pitch)
            * Quat::from_rotation_z(app.rendered_roll());
        ensure!(
            drawn.angle_between(expected) < 0.01,
            "{who} seat {index}: view {drawn}, seat {expected}"
        );
    }
    Ok(index)
}
/// In third person a passenger, who has no control object, keeps their own
/// player camera round the seat (`Player::getCameraTransform` 0x5ab80e only
/// hands a controlling player's camera to what they control), not the
/// Tank's chase camera.
fn check_passenger_own_camera(
    apps: &mut [&mut App],
    rider: usize,
    tank: &Definition,
    gpu: &Headless,
    renderer: &mut UiRenderer,
) -> Result<()> {
    request(
        apps[rider],
        UiAction::Game(GameAction::ToggleFirstPerson { fast: true }),
    )?;
    run_for(apps, 0.5)?;
    let app = &mut *apps[rider];
    let eye = render(app, gpu, renderer)?;
    let (vehicle, _) = seat(app).context("not seated")?;
    let pose = app
        .network_view()
        .and_then(|v| v.vehicle_poses.get(&vehicle))
        .context("tank pose")?;
    let center = (Vec3::from(tank.bounds_min) + Vec3::from(tank.bounds_max)) * 0.5;
    let (expected, ..) = bri_client::vehicle_camera::driver_view(
        Vec3::from(pose.position),
        Quat::from_array(pose.rotation),
        center,
        &tank.camera,
        None,
        1.0,
        |_, _| Ok(None),
    )?;
    let (feet, _) = app
        .network_view()
        .and_then(|v| {
            let s = &tank.seats[seat(app)?.1];
            let world = Mat4::from_rotation_translation(
                Quat::from_array(pose.rotation),
                Vec3::from(pose.position),
            );
            Some((world.transform_point3(Vec3::from(s.transform.position)), v))
        })
        .context("seat")?;
    println!("passenger third person: eye {eye}, seat {feet}, chase camera {expected}");
    ensure!(
        eye.distance(expected) > 1.0 && eye.distance(feet) < 10.0,
        "passenger's third-person camera {eye} should orbit the seat {feet}, not sit at the Tank's chase camera {expected}"
    );
    request(
        apps[rider],
        UiAction::Game(GameAction::ToggleFirstPerson { fast: true }),
    )?;
    run_for(apps, 0.5)?;
    Ok(())
}

#[test]
#[ignore = "requires converted native v20 content, loopback QUIC and offscreen GPU; no window"]
fn every_tank_seat_sees_from_the_riders_eye_for_host_and_guest() -> Result<()> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let content = workspace.join("content");
    let tank = Pack::load(content.join("vehicles-pack-012/vehicles.json"))?
        .definitions
        .into_iter()
        .find(|d| d.id == TANK)
        .context("the Tank")?;
    let assets = AvatarAssets::load(&content.join("avatar-pack-002"))?;
    let state = std::env::temp_dir().join(format!(
        "bri-vehicle-first-person-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    let port = free_port()?;
    let mut host = app(&content, &state, "Host")?;
    let mut guest = app(&content, &state, "Guest")?;
    host.ui
        .core
        .prefs
        .set("$Pref::Server::Port", port.to_string());
    request(
        &mut host,
        UiAction::HostGame {
            map: SLATE.into(),
            mode: ServerMode::Lan,
            game_mode: None,
            max_players: 4,
            server_name: "Vehicle first person".into(),
            password: String::new(),
            admin_password: String::new(),
            super_admin_password: String::new(),
        },
    )?;
    until(&mut [&mut host], "host in game", 180, |a| Ok(in_game(a[0])))?;
    request(
        &mut guest,
        UiAction::JoinServer {
            address: format!("127.0.0.1:{port}"),
            password: String::new(),
        },
    )?;
    until(&mut [&mut host, &mut guest], "guest in game", 240, |a| {
        Ok(in_game(a[1]))
    })?;
    run_for(&mut [&mut host, &mut guest], 2.0)?;
    load_tank(&mut host, &state.join("Host"))?;
    until(&mut [&mut host, &mut guest], "the Tank", 60, |a| {
        Ok(a.iter().all(|a| {
            a.network_view()
                .is_some_and(|v| !v.vehicle_poses.is_empty())
        }))
    })?;
    run_for(&mut [&mut host, &mut guest], 1.0)?;

    let gpu = Headless::new().context("offscreen renderer")?;
    let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    for app in [&mut host, &mut guest] {
        app.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
        if app.controls.third_person_view() {
            request(
                app,
                UiAction::Game(GameAction::ToggleFirstPerson { fast: true }),
            )?;
        }
    }
    // The guest boards and takes every seat in turn.
    board(&mut [&mut host, &mut guest], 1)?;
    let mut seen = BTreeSet::new();
    for _ in 0..tank.seats.len() {
        let index = check(
            &mut [&mut host, &mut guest],
            1,
            "guest",
            &tank,
            &assets,
            &gpu,
            &mut renderer,
        )?;
        seen.insert(index);
        if tank.seat_role(index) == SeatRole::Passenger {
            check_passenger_own_camera(
                &mut [&mut host, &mut guest],
                1,
                &tank,
                &gpu,
                &mut renderer,
            )?;
        }
        request(&mut guest, UiAction::Game(GameAction::NextSeat))?;
        until(&mut [&mut host, &mut guest], "the next seat", 30, |a| {
            Ok(seat(a[1]).is_some_and(|(_, s)| s != index))
        })?;
    }
    ensure!(seen.len() == tank.seats.len(), "guest sat in {seen:?}");
    // The guest moves to the turret, and the host boards below it.
    while seat(&guest).is_some_and(|(_, s)| tank.seat_role(s) != SeatRole::Gunner) {
        let index = seat(&guest).map(|(_, s)| s);
        request(&mut guest, UiAction::Game(GameAction::NextSeat))?;
        until(&mut [&mut host, &mut guest], "the turret", 30, |a| {
            Ok(seat(a[1]).map(|(_, s)| s) != index)
        })?;
    }
    board(&mut [&mut guest, &mut host], 1)?;
    check(
        &mut [&mut guest, &mut host],
        1,
        "host",
        &tank,
        &assets,
        &gpu,
        &mut renderer,
    )?;
    for app in [&mut guest, &mut host] {
        request(app, UiAction::Disconnect)?;
        app.gpu_stopped();
    }
    let _ = std::fs::remove_dir_all(&state);
    Ok(())
}

const FLYING_JEEP: &str = "v20.vehicle.flyingwheeledjeepvehicle";
/// The driven vehicle's nose pitch (radians, up positive) in the host's
/// newest pose, and its speed.
fn nose(app: &App) -> Option<(f32, f32)> {
    let view = app.network_view()?;
    let (vehicle, _) = seat(app)?;
    let pose = view.vehicle_poses.get(&vehicle)?;
    let forward = Quat::from_array(pose.rotation) * Vec3::NEG_Z;
    Some((forward.y.asin(), Vec3::from(pose.velocity).length()))
}

/// Through the whole app: take off in a Flying Wheeled Jeep (mouse-steered
/// like the Stunt Plane) in first person, push the mouse up, and return how
/// far the host's pose and this client's predicted view pitched. `invert`
/// is Options' Invert Mouse In Vehicles; `None` leaves the default.
fn mouse_up_pitch(
    invert: Option<bool>,
    gpu: &Headless,
    renderer: &mut UiRenderer,
) -> Result<(f32, f32)> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let content = workspace.join("content");
    let state = std::env::temp_dir().join(format!(
        "bri-vehicle-mouse-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    let port = free_port()?;
    let mut host = app(&content, &state, "Host")?;
    host.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
    host.ui
        .core
        .prefs
        .set("$Pref::Server::Port", port.to_string());
    if let Some(invert) = invert {
        host.ui
            .core
            .prefs
            .set("$Pref::Input::VehicleMouseInvert", if invert { "1" } else { "0" });
    }
    request(
        &mut host,
        UiAction::HostGame {
            map: SLATE.into(),
            mode: ServerMode::Lan,
            game_mode: None,
            max_players: 4,
            server_name: "Vehicle mouse".into(),
            password: String::new(),
            admin_password: String::new(),
            super_admin_password: String::new(),
        },
    )?;
    until(&mut [&mut host], "host in game", 180, |a| Ok(in_game(a[0])))?;
    run_for(&mut [&mut host], 1.0)?;
    load_vehicle(&mut host, &state.join("Host"), FLYING_JEEP)?;
    until(&mut [&mut host], "the jeep", 60, |a| {
        Ok(a[0].network_view().is_some_and(|v| !v.vehicle_poses.is_empty()))
    })?;
    run_for(&mut [&mut host], 1.0)?;
    board(&mut [&mut host], 0)?;
    ensure!(seat(&host).is_some_and(|(_, s)| s == 0), "not in the driver's seat");
    if host.controls.third_person_view() {
        request(
            &mut host,
            UiAction::Game(GameAction::ToggleFirstPerson { fast: true }),
        )?;
    }
    held(&mut host, HeldControl::Forward, true)?;
    until(&mut [&mut host], "take-off speed", 60, |a| {
        Ok(nose(a[0]).is_some_and(|(_, speed)| speed > 39.0))
    })?;
    run_for(&mut [&mut host], 3.0)?;
    let (before, _) = nose(&host).context("nose")?;
    render(&mut host, gpu, renderer)?;
    let view_before = host.rendered_camera().context("camera")?.2;
    // Mouse up: the OS reports y shrinking.
    for _ in 0..30 {
        request(
            &mut host,
            UiAction::Game(GameAction::Look {
                yaw: 0.0,
                pitch: -0.03,
            }),
        )?;
        run_for(&mut [&mut host], 1.0 / 60.0)?;
    }
    run_for(&mut [&mut host], 0.3)?;
    let (after, _) = nose(&host).context("nose")?;
    render(&mut host, gpu, renderer)?;
    let view_after = host.rendered_camera().context("camera")?.2;
    request(&mut host, UiAction::Disconnect)?;
    host.gpu_stopped();
    let _ = std::fs::remove_dir_all(&state);
    Ok((after - before, view_after - view_before))
}

/// Stock v20's Invert Mouse In Vehicles is on: mouse up dips the nose, in
/// the host's pose and in the view this client predicts. Turned off in
/// Options, mouse up raises it.
#[test]
#[ignore = "requires converted native v20 content, loopback QUIC and offscreen GPU; no window"]
fn invert_mouse_in_vehicles_turns_the_nose_both_ways_through_the_app() -> Result<()> {
    let gpu = Headless::new().context("offscreen renderer")?;
    let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    for (invert, down) in [(None, true), (Some(false), false), (Some(true), true)] {
        let (host, view) = mouse_up_pitch(invert, &gpu, &mut renderer)?;
        println!("invert {invert:?}: host nose {host:+.3}, predicted view {view:+.3}");
        let sign = if down { -1.0 } else { 1.0 };
        ensure!(host * sign > 0.01, "invert {invert:?}: host nose moved {host}");
        ensure!(view * sign > 0.01, "invert {invert:?}: predicted view moved {view}");
    }
    Ok(())
}
