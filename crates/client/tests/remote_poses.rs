//! Two headless clients on one host over loopback: what one player sees of
//! the other's tool swings and look must match what the swinger draws of
//! itself. Never creates a window or OS input. Runs on the made-up content
//! root; the ignored variant runs on the generated v20 content
//! (`--release -- --ignored`, BRI_CONTENT or content/).
use anyhow::{Context, Result, bail, ensure};
use bri_client::{app::App, platform::PlatformApp};
use bri_ui::{api::*, screens::ScreenId};
use glam::{Mat4, Quat};
use std::{
    path::Path,
    thread,
    time::{Duration, Instant},
};

#[macro_use]
mod support;
use support::content_root::ContentRoot;

synthetic_and_content!(ContentRoot: players_see_each_others_swings_and_look_as_the_swinger_draws_them);

const SIZE: (u32, u32) = (960, 720);
/// Longest game time one frame may advance. A loaded machine runs slow
/// frames; stepping them by wall time would sample the swing so sparsely
/// that each side catches a different point near its peak. Capped, the game
/// runs behind the wall clock instead, and every recording samples the
/// animation at least this densely.
const MAX_FRAME: Duration = Duration::from_millis(4);

fn step(app: &mut App, elapsed: Duration) -> Result<()> {
    app.tick(elapsed)?;
    app.ui.update(elapsed.as_millis() as u64);
    ensure!(app.pump()?.is_empty(), "Unexpected window command");
    if let ConnectionState::Failed { reason } = &app.ui.core.conn {
        bail!("Connection failed: {reason}");
    }
    Ok(())
}

/// Step both apps until `ready` holds, calling `each` after every frame.
/// Frames advance by wall time up to [`MAX_FRAME`]; `ready` gets the game
/// time stepped so far, which never runs ahead of the wall clock.
fn until(
    apps: &mut [&mut App],
    what: &str,
    timeout: Duration,
    mut each: impl FnMut(&[&mut App]),
    ready: impl Fn(&[&mut App], Duration) -> bool,
) -> Result<()> {
    let start = Instant::now();
    let mut previous = start;
    let mut game = Duration::ZERO;
    loop {
        thread::sleep(Duration::from_millis(1));
        let now = Instant::now();
        let elapsed = now.duration_since(previous).min(MAX_FRAME);
        for app in apps.iter_mut() {
            step(app, elapsed)?;
        }
        previous = now;
        game += elapsed;
        each(apps);
        if ready(apps, game) {
            return Ok(());
        }
        ensure!(start.elapsed() < timeout, "Timed out waiting for {what}");
    }
}

fn request(app: &mut App, action: UiAction) -> Result<()> {
    app.ui.core.request(action);
    ensure!(app.pump()?.is_empty(), "Unexpected window command");
    app.ui.update(0);
    Ok(())
}

fn in_game(app: &App) -> bool {
    matches!(app.ui.core.conn, ConnectionState::InGame { .. })
        && app
            .network_view()
            .is_some_and(|v| v.poses.contains_key(&v.owner))
}

fn use_test_ports() -> u16 {
    let port = std::net::UdpSocket::bind("127.0.0.1:0")
        .and_then(|s| s.local_addr())
        .map(|a| a.port())
        .expect("a free UDP port");
    // SAFETY: set before any host or join starts, under the GPU turn, which
    // every test here holding a port takes first.
    unsafe {
        std::env::set_var("BRI_TEST_HOST_PORT", port.to_string());
        std::env::set_var("BRI_TEST_DISCOVERY_PORT", "0");
    }
    port
}

fn rotation(node: Mat4) -> Quat {
    node.to_scale_rotation_translation().1
}

/// The largest turn of `node` away from `rest` over a recording.
fn reach(frames: &[Mat4], rest: Mat4) -> f32 {
    frames
        .iter()
        .map(|m| rotation(*m).angle_between(rotation(rest)))
        .fold(0.0, f32::max)
}

fn players_see_each_others_swings_and_look_as_the_swinger_draws_them(
    f: &ContentRoot,
) -> Result<()> {
    let gpu = support::gpu::turn().context("offscreen adapter")?;
    let port = use_test_ports();
    let load = |name: &str, state: &Path| -> Result<Box<App>> {
        let mut app = App::load(&f.root, state, SIZE)?;
        app.ui.core.pop(ScreenId::DefaultControls);
        app.ui.core.settings.avatar.lan_name = name.into();
        Ok(app)
    };
    let (host_state, guest_state) = (f.state()?, f.state()?);
    let mut host = load("Hosty", host_state.path())?;
    let mut guest = load("Guesty", guest_state.path())?;
    host.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
    guest.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
    host.ui.core.request(UiAction::HostGame {
        map: f.map.0.clone(),
        mode: ServerMode::Internet,
        game_mode: None,
        max_players: 8,
        server_name: "Remote poses".into(),
        password: String::new(),
        admin_password: String::new(),
        super_admin_password: String::new(),
    });
    until(
        &mut [&mut host],
        "host in game",
        Duration::from_secs(90),
        |_| {},
        |a, _| in_game(a[0]),
    )?;
    request(
        &mut guest,
        UiAction::JoinServer {
            address: format!("127.0.0.1:{port}"),
            password: String::new(),
        },
    )?;
    until(
        &mut [&mut host, &mut guest],
        "guest in game",
        Duration::from_secs(90),
        |_| {},
        |a, _| {
            in_game(a[1])
                && a.iter()
                    .all(|app| app.network_view().unwrap().poses.len() == 2)
        },
    )?;
    let mut report = String::new();
    // Each side swings in turn while the other watches.
    for swinger_is_host in [true, false] {
        let (swinger, watcher) = if swinger_is_host {
            (&mut host, &mut guest)
        } else {
            (&mut guest, &mut host)
        };
        let id = swinger.network_view().unwrap().owner;
        // Look down at the floor, as when wrenching a brick.
        request(
            swinger,
            UiAction::Game(GameAction::Look {
                yaw: 0.3,
                pitch: -1.1,
            }),
        )?;
        let settle = |apps: &mut [&mut App]| {
            until(
                apps,
                "settle",
                Duration::from_secs(120),
                |_| {},
                |_, game| game > Duration::from_millis(1500),
            )
        };
        // Every tool with a third-person swing.
        for (slot, what) in [(0, "hammer"), (1, "wrench")] {
            request(swinger, UiAction::UseTool { slot })?;
            settle(&mut [&mut *swinger, &mut *watcher])?;
            let rest = |app: &App| app.avatar_node(id, "RightHand").context("posed right hand");
            let (own_rest, seen_rest) = (rest(swinger)?, rest(watcher)?);
            let ready = rotation(own_rest).angle_between(rotation(seen_rest));
            ensure!(
                ready < 0.02,
                "{what}: watcher's ready pose is {ready} rad from the swinger's"
            );
            let (mut own, mut seen) = (Vec::new(), Vec::new());
            for (down, hold) in [(true, 700), (false, 1200)] {
                request(
                    swinger,
                    UiAction::Game(GameAction::Held {
                        control: HeldControl::Fire,
                        down,
                    }),
                )?;
                until(
                    &mut [&mut *swinger, &mut *watcher],
                    "swing",
                    Duration::from_secs(120),
                    |a| {
                        own.extend(a[0].avatar_node(id, "RightHand"));
                        seen.extend(a[1].avatar_node(id, "RightHand"));
                    },
                    |_, game| game > Duration::from_millis(hold),
                )?;
            }
            let (own_reach, seen_reach) = (reach(&own, own_rest), reach(&seen, seen_rest));
            let who = if swinger_is_host { "host" } else { "guest" };
            report.push_str(&format!(
                "{who} {what}: swinger draws {own_reach:.3} rad, watcher sees {seen_reach:.3} rad\n"
            ));
            ensure!(own_reach > 0.2, "{who} {what}: no swing ({own_reach})");
            ensure!(
                (own_reach - seen_reach).abs() < 0.05,
                "{who} {what}: watcher sees {seen_reach} rad, swinger draws {own_reach}"
            );
        }
    }
    println!("{report}");
    Ok(())
}
