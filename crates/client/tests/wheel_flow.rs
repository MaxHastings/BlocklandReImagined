//! The mouse wheel reaches the brick bar through the real App (v20
//! `scrollInventory`). Never creates a window or OS input.
//! Run: cargo test -p bri-client --test wheel_flow --release -- --ignored --nocapture
//! Content defaults to the workspace `content/`; override with BRI_CONTENT.
use anyhow::{Result, bail, ensure};
use bri_client::{app::App, platform::PlatformApp};
use bri_ui::{
    api::*,
    input::{InputEvent, Key, Modifiers},
    screens::ScreenId,
};
use std::{
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const BEDROOM: &str = "v20/add-ons/map_bedroom/bedroom.mis";

fn step(app: &mut App, elapsed: Duration) -> Result<()> {
    app.tick(elapsed)?;
    app.ui.update(elapsed.as_millis() as u64);
    ensure!(app.pump()?.is_empty(), "unexpected window command");
    if let ConnectionState::Failed { reason } = &app.ui.core.conn {
        bail!("connection failed: {reason}");
    }
    Ok(())
}

fn until(app: &mut App, what: &str, ready: impl Fn(&App) -> bool) -> Result<()> {
    let start = Instant::now();
    let mut previous = start;
    loop {
        let now = Instant::now();
        step(app, now.duration_since(previous))?;
        previous = now;
        if ready(app) {
            return Ok(());
        }
        ensure!(
            start.elapsed() < Duration::from_secs(45),
            "timed out waiting for {what}; screens {:?}; hud mode {:?} cur {:?} active {}",
            app.ui.stack(),
            app.ui.core.hud.mode,
            app.ui.core.hud.cur_brick,
            app.ui.core.hud.brick_active,
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "native Bedroom map and loopback host; no window or audio device"]
fn wheel_scrolls_the_brick_bar_in_the_real_app() -> Result<()> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let content = std::env::var_os("BRI_CONTENT")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("content"));
    let run_id = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let state = workspace.join(format!("artifacts/wheel-flow/state-{run_id}"));
    std::fs::create_dir_all(&state)?;
    let mut app = App::load(&content, &state, (960, 720))?;
    app.ui.core.pop(ScreenId::DefaultControls);
    app.ui.update(0);
    app.ui.core.request(UiAction::HostGame {
        map: BEDROOM.into(),
        mode: ServerMode::SinglePlayer,
        max_players: 1,
        server_name: "Wheel".into(),
        password: String::new(),
        admin_password: String::new(),
        super_admin_password: String::new(),
    });
    until(&mut app, "in game with a brick inventory", |a| {
        matches!(a.ui.core.conn, ConnectionState::InGame { .. })
            && a.ui.core.hud.bricks.iter().filter(|b| b.is_some()).count() >= 3
    })?;
    eprintln!("stack {:?}", app.ui.stack());
    let key = |app: &mut App, down: bool| {
        app.ui.handle_input(if down {
            InputEvent::KeyDown {
                key: Key::Digit(1),
                mods: Modifiers::NONE,
                repeat: false,
            }
        } else {
            InputEvent::KeyUp {
                key: Key::Digit(1),
                mods: Modifiers::NONE,
            }
        })
    };
    key(&mut app, true);
    key(&mut app, false);
    until(&mut app, "brick slot 1 equipped", |a| {
        a.ui.core.hud.brick_active && a.ui.core.hud.cur_brick == Some(0)
    })?;
    for expected in [1, 2, 1] {
        let delta = if expected == 1 && app.ui.core.hud.cur_brick == Some(2) {
            1.0
        } else {
            -1.0
        };
        app.ui.handle_input(InputEvent::Wheel { delta });
        until(&mut app, &format!("wheel to slot {expected}"), |a| {
            a.ui.core.hud.brick_active && a.ui.core.hud.cur_brick == Some(expected)
        })?;
        // The authoritative echo must not undo the wheel selection.
        for _ in 0..20 {
            step(&mut app, Duration::from_millis(16))?;
        }
        ensure!(
            app.ui.core.hud.cur_brick == Some(expected) && app.ui.core.hud.brick_active,
            "selection reverted to {:?}",
            app.ui.core.hud.cur_brick
        );
    }
    Ok(())
}
