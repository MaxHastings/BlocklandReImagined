//! Recorded-input playback against the real App: from the main menu to
//! walking in single player using only clicks and key presses, the input a
//! player's hands make. No window, GPU or OS input is created.
//! Run: cargo test -p bri-client --test input_playback --release -- --ignored --nocapture
use anyhow::{Context, Result, bail, ensure};
use bri_client::{
    app::App,
    playback::{self, Frame, Script},
};
use bri_ui::{api::ConnectionState, input::Key, screens::ScreenId};
use std::{
    path::Path,
    time::{Duration, Instant},
};

const SIZE: (u32, u32) = (960, 720);

/// Replay `frames` in real time (the host and loader run on other threads).
fn play(app: &mut App, frames: &[Frame]) -> Result<()> {
    playback::replay(app, frames, true, |app, _| {
        if let ConnectionState::Failed { reason } = &app.ui.core.conn {
            bail!("Connection failed during playback: {reason}");
        }
        Ok(true)
    })
}

/// Replay idle frames until `ready` holds.
fn until(app: &mut App, what: &str, timeout: Duration, ready: impl Fn(&App) -> bool) -> Result<()> {
    let start = Instant::now();
    let idle = Script::default().wait(Duration::from_millis(50)).frames.clone();
    while !ready(app) {
        ensure!(
            start.elapsed() < timeout,
            "Timed out waiting for {what}; screens {:?}; state {:?}",
            app.ui.stack(),
            app.ui.core.conn
        );
        play(app, &idle)?;
    }
    Ok(())
}

fn control(app: &App, screen: ScreenId, name: &str) -> Result<(f32, f32)> {
    app.ui
        .control_center(screen, name)
        .with_context(|| format!("{screen:?} has no visible control {name}"))
}

#[test]
#[ignore = "requires converted native content; no window, GPU or OS input"]
fn a_player_starts_single_player_and_walks_using_only_clicks_and_keys() -> Result<()> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let state = tempfile::tempdir()?;
    let mut app = App::load(&workspace.join("content"), state.path(), SIZE)?;
    let mut recorded = Vec::new();

    // First launch asks for a control scheme over the main menu: accept it.
    until(&mut app, "the main menu", Duration::from_secs(10), |a| {
        a.ui.is_open(ScreenId::MainMenu)
    })?;
    if app.ui.top_id() == ScreenId::DefaultControls {
        let mut script = Script::default();
        script
            .wait(Duration::from_millis(200))
            .click(control(&app, ScreenId::DefaultControls, "DefaultControlsGui.apply();")?)
            .wait(Duration::from_millis(200));
        play(&mut app, &script.frames)?;
        recorded.extend(script.frames);
    }
    ensure!(
        app.ui.top_id() == ScreenId::MainMenu,
        "Something covers the main menu: {:?}",
        app.ui.stack()
    );
    // Main menu: Start Game.
    let mut script = Script::default();
    script
        .wait(Duration::from_millis(200))
        .click(control(&app, ScreenId::MainMenu, "MM_StartButton")?)
        .wait(Duration::from_millis(200));
    play(&mut app, &script.frames)?;
    recorded.extend(script.frames);
    ensure!(
        app.ui.is_open(ScreenId::StartMission),
        "Start Game did not open the mission list: {:?}",
        app.ui.stack()
    );

    // Mission list: the default map, Start.
    let mut script = Script::default();
    script.click(control(&app, ScreenId::StartMission, "SM_StartMission();")?);
    play(&mut app, &script.frames)?;
    recorded.extend(script.frames);
    // The loading screen shows what is actually happening, not a fixed phase.
    let mut statuses: Vec<String> = Vec::new();
    let start = Instant::now();
    let idle = Script::default().wait(Duration::from_millis(20)).frames.clone();
    while !(matches!(app.ui.core.conn, ConnectionState::InGame { .. })
        && app.presented_local().is_some())
    {
        ensure!(
            start.elapsed() < Duration::from_secs(120),
            "Timed out spawning; saw {statuses:?}; state {:?}",
            app.ui.core.conn
        );
        if let ConnectionState::Loading { status, .. } = &app.ui.core.conn
            && statuses.last() != Some(status)
        {
            statuses.push(status.clone());
        }
        play(&mut app, &idle)?;
    }
    eprintln!("loading screen showed {statuses:?}");
    // Frames sample the screen, so only the long map load is certain to be
    // seen; a small local world downloads between two frames. The download's
    // brick counts are pinned by the net loopback test
    // (join_reports_the_world_download_in_bricks).
    ensure!(
        statuses.iter().any(|s| s.starts_with("LOADING MAP")),
        "Loading screen stages: {statuses:?}"
    );
    // Let the spawn settle before measuring movement.
    until(&mut app, "the player to land", Duration::from_secs(10), |a| {
        a.presented_local().is_some_and(|p| p.grounded)
    })?;
    let before = app.presented_local().context("no local player")?.feet;

    // Walk forward for a second with the default W bind.
    let mut script = Script::default();
    script
        .hold(Key::Letter('w'), Duration::from_secs(1))
        .wait(Duration::from_millis(300));
    play(&mut app, &script.frames)?;
    recorded.extend(script.frames);
    let after = app.presented_local().context("no local player")?.feet;
    let walked = ((after[0] - before[0]).powi(2) + (after[2] - before[2]).powi(2)).sqrt();
    eprintln!("walked {walked:.2} units from {before:?} to {after:?}");
    ensure!(walked > 2.0, "Holding W moved the player only {walked:.2} units");

    // Escape opens the in-game menu.
    let mut script = Script::default();
    script.press(Key::Escape).wait(Duration::from_millis(200));
    play(&mut app, &script.frames)?;
    recorded.extend(script.frames);
    ensure!(
        app.ui.is_open(ScreenId::EscapeMenu),
        "Escape did not open the menu: {:?}",
        app.ui.stack()
    );

    // What was played is a recording like any other: it saves and loads
    // unchanged, so a failing flow can be kept and replayed.
    let path = state.path().join("walk.jsonl");
    playback::save(&path, &recorded)?;
    ensure!(playback::load(&path)? == recorded, "Recording round trip changed it");
    eprintln!("{} frames recorded", recorded.len());
    Ok(())
}
