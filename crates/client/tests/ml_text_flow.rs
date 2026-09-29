//! Host and guest over loopback: player chat stays literal in v20's chat
//! format on both HUDs, and both render a server center print through the
//! shared ML renderer. No window or OS input.
//! Run: cargo test -p bri-client --test ml_text_flow -- --ignored --nocapture
use anyhow::{Result, bail, ensure};
use bri_client::{app::App, platform::PlatformApp};
use bri_ui::{
    api::*,
    gpu::{Headless, UiRenderer},
};
use std::{
    path::Path,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// A free port for this binary's hosts, so a game already hosting on 28000
/// (the player's own, say) does not break the test. Shared by every test
/// here, as the fixed port was.
fn test_port() -> u16 {
    static PORT: std::sync::OnceLock<u16> = std::sync::OnceLock::new();
    *PORT.get_or_init(|| {
        let port = std::net::UdpSocket::bind("127.0.0.1:0")
            .and_then(|s| s.local_addr())
            .map(|a| a.port())
            .expect("a free UDP port");
        // SAFETY: set once, before any host or join in this binary starts.
        unsafe {
            std::env::set_var("BRI_TEST_HOST_PORT", port.to_string());
            std::env::set_var("BRI_TEST_DISCOVERY_PORT", "0");
        }
        port
    })
}

const SIZE: (u32, u32) = (960, 720);
const BEDROOM: &str = "v20/add-ons/map_bedroom/bedroom.mis";

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

#[test]
#[ignore = "requires converted native content, loopback QUIC and an offscreen GPU; no window"]
fn host_and_guest_chat_markup_and_player_text() -> Result<()> {
    let port = test_port();
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = workspace.join("artifacts/ml-text/flow");
    let run_id = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let host_state = out.join(format!("host-{run_id}"));
    let guest_state = out.join(format!("guest-{run_id}"));
    std::fs::create_dir_all(&host_state)?;
    std::fs::create_dir_all(&guest_state)?;
    let mut host = App::load(&workspace.join("content"), &host_state, SIZE)?;
    let mut guest = App::load(&workspace.join("content"), &guest_state, SIZE)?;

    host.ui.core.request(UiAction::HostGame {
        map: BEDROOM.into(),
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
    let gpu = Headless::new()?;
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
