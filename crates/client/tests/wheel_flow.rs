//! The mouse wheel reaches the brick bar through the real App (v20
//! `scrollInventory`). Never creates a window or OS input.
//! Runs on the made-up content root, and (ignored) on the generated v20
//! content: `--ignored`, defaulting to the workspace `content/`, override
//! with BRI_CONTENT.
use anyhow::{Result, bail, ensure};
use bri_client::{app::App, platform::PlatformApp};
use bri_ui::{
    api::*,
    input::{InputEvent, Key, Modifiers},
    screens::ScreenId,
};
use std::time::Duration;

#[macro_use]
mod support;
use support::{content_root::ContentRoot, wait};

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
    wait::until_one(app, what, Duration::from_secs(45), step, ready).map_err(|error| {
        anyhow::anyhow!(
            "{error:#}; screens {:?}; hud bricks {}; hud mode {:?} cur {:?} active {}",
            app.ui.stack(),
            app.ui.core.hud.bricks.iter().flatten().count(),
            app.ui.core.hud.mode,
            app.ui.core.hud.cur_brick,
            app.ui.core.hud.brick_active,
        )
    })
}

synthetic_and_content!(ContentRoot: wheel_scrolls_the_brick_bar_in_the_real_app);

fn wheel_scrolls_the_brick_bar_in_the_real_app(f: &ContentRoot) -> Result<()> {
    let state = f.state()?;
    let mut app = App::load(&f.root, state.path(), (960, 720))?;
    app.ui.core.pop(ScreenId::DefaultControls);
    app.ui.update(0);
    app.ui.core.request(UiAction::HostGame {
        map: BEDROOM.into(),
        mode: ServerMode::SinglePlayer,
        game_mode: None,
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
