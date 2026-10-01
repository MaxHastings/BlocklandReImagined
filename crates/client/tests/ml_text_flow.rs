//! Host and guest over loopback: player chat stays literal in v20's chat
//! format on both HUDs, and both render a server center print through the
//! shared ML renderer. Runs on the made-up content root; the ignored variant
//! runs on the generated v20 content (`-- --ignored`, BRI_CONTENT or
//! content/). No window or OS input.
use anyhow::{Result, bail, ensure};
use bri_client::{app::App, platform::PlatformApp};
use bri_ui::{api::*, gpu::UiRenderer};
use std::{
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[macro_use]
mod support;
use support::content_root::ContentRoot;

synthetic_and_content!(ContentRoot: host_and_guest_chat_markup_and_player_text);

/// A free port for this test's host, so a game already hosting on 28000
/// (the player's own, say) does not break the test. Set under the GPU
/// turn, which every test here holding a port takes first.
fn test_port() -> u16 {
    let port = std::net::UdpSocket::bind("127.0.0.1:0")
        .and_then(|s| s.local_addr())
        .map(|a| a.port())
        .expect("a free UDP port");
    // SAFETY: set before this test's host or join starts.
    unsafe {
        std::env::set_var("BRI_TEST_HOST_PORT", port.to_string());
        std::env::set_var("BRI_TEST_DISCOVERY_PORT", "0");
    }
    port
}

const SIZE: (u32, u32) = (960, 720);

fn step(app: &mut App, elapsed: Duration) -> Result<()> {
    app.tick(elapsed)?;
    app.ui.update(elapsed.as_millis() as u64);
    ensure!(app.pump()?.is_empty(), "unexpected native window command");
    if let ConnectionState::Failed { reason } = &app.ui.core.conn {
        bail!("connection failed: {reason}");
    }
    Ok(())
}

/// Step both apps until `ready` holds.
fn until(
    apps: &mut [&mut App],
    what: &str,
    timeout: Duration,
    ready: impl Fn(&[&mut App]) -> bool,
) -> Result<()> {
    let start = Instant::now();
    let mut previous = start;
    loop {
        let now = Instant::now();
        for app in apps.iter_mut() {
            step(app, now.duration_since(previous))?;
        }
        previous = now;
        if ready(apps) {
            return Ok(());
        }
        ensure!(
            start.elapsed() < timeout,
            "timed out waiting for {what}; chat {:?}",
            apps.iter().map(|a| chat_lines(a)).collect::<Vec<_>>()
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn in_game(app: &App) -> bool {
    matches!(app.ui.core.conn, ConnectionState::InGame { .. })
        && app
            .network_view()
            .is_some_and(|v| v.poses.contains_key(&v.owner))
}

fn chat_lines(app: &App) -> Vec<String> {
    app.ui
        .core
        .chat
        .lines
        .iter()
        .map(|l| l.text.clone())
        .collect()
}

fn host_and_guest_chat_markup_and_player_text(f: &ContentRoot) -> Result<()> {
    let gpu = support::gpu::turn()?;
    let port = test_port();
    let out = f.out("ml-text/flow")?;
    let run_id = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let (host_state, guest_state) = (f.state()?, f.state()?);
    let mut host = App::load(&f.root, host_state.path(), SIZE)?;
    let mut guest = App::load(&f.root, guest_state.path(), SIZE)?;

    host.ui.core.request(UiAction::HostGame {
        map: f.map.0.clone(),
        mode: ServerMode::Lan,
        game_mode: None,
        max_players: 4,
        server_name: "ML text host".into(),
        password: String::new(),
        admin_password: String::new(),
        super_admin_password: String::new(),
    });
    until(
        &mut [&mut host],
        "host in game",
        Duration::from_secs(60),
        |a| in_game(a[0]),
    )?;
    guest.ui.core.request(UiAction::JoinServer {
        address: format!("127.0.0.1:{port}"),
        password: String::new(),
    });
    until(
        &mut [&mut host, &mut guest],
        "guest in game",
        Duration::from_secs(60),
        |a| in_game(a[1]),
    )?;

    // Player chat: markup and colour codes typed by a player stay literal.
    let typed = format!("<color:00ff00>green? {run_id}");
    guest.ui.core.request(UiAction::Chat {
        channel: ChatChannel::Say,
        text: typed,
    });
    let player_line = |l: &String| {
        l.starts_with("\u{E007}\u{E003}")
            && l.ends_with(&format!("\u{E007}\u{E006}: ‹color:00ff00›green? {run_id}"))
    };
    until(
        &mut [&mut host, &mut guest],
        "player chat on both HUDs",
        Duration::from_secs(15),
        |a| {
            a.iter().all(|app| {
                let lines = chat_lines(app);
                lines.iter().any(player_line)
            })
        },
    )?;

    // Render both HUDs offscreen with a center and bottom print from the
    // same client path (`GameConnection::CenterPrint` text).
    let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    for (name, app) in [("host", &mut host), ("guest", &mut guest)] {
        app.ui.apply(UiUpdate::CenterPrint {
            text: bri_ui::ml::sanitize(
                "<color:FFFFFF>It's no longer Badspot's' Birthday.<br>Attempts to butter Badspot by making presents will go ignored from now.",
            ),
            seconds: 5.0,
        });
        app.ui.update(16);
        let dl = app.ui.draw();
        let px = gpu.render_rgba(
            &mut renderer,
            &app.ui.core.pack,
            &dl,
            SIZE,
            app.ui.scale(),
            [0.45, 0.62, 0.78, 1.0],
        )?;
        image::save_buffer(
            out.join(format!("{name}-hud.png")),
            &px,
            SIZE.0,
            SIZE.1,
            image::ColorType::Rgba8,
        )?;
        // Some pure-white text pixels exist in the center print band.
        let white = px
            .chunks_exact(4)
            .enumerate()
            .filter(|(i, p)| {
                let y = (*i as u32) / SIZE.0;
                (200..400).contains(&y) && p[..3] == [255, 255, 255]
            })
            .count();
        ensure!(
            white > 200,
            "{name}: center print should be white ({white})"
        );
        println!("{name} chat: {:?}", chat_lines(app));
    }
    Ok(())
}
