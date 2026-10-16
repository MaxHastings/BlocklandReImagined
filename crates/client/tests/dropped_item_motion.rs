//! Ordinary Q/DropTool through the headless App, local authority and presentation.
//! Invented native content; no OS input, visible window or direct world mutation.
use anyhow::{Context, Result, ensure};
use bri_client::{
    app::App, platform::PlatformApp, testing::content_root::ContentRoot, world_items::ItemIdentity,
};
use bri_ui::{
    api::{ConnectionState, GameAction, ServerMode, UiAction},
    input::{InputEvent, Key, Modifiers},
    screens::ScreenId,
};
use std::time::{Duration, Instant};

fn frame(app: &mut App, previous: &mut Instant) -> Result<()> {
    std::thread::sleep(Duration::from_millis(16));
    let now = Instant::now();
    let elapsed = now.duration_since(*previous);
    *previous = now;
    app.tick(elapsed)?;
    app.ui.update(elapsed.as_millis() as u64);
    ensure!(app.pump()?.is_empty(), "Unexpected native window command");
    if let ConnectionState::Failed { reason } = &app.ui.core.conn {
        anyhow::bail!("App connection failed: {reason}");
    }
    Ok(())
}
fn until(app: &mut App, previous: &mut Instant, ready: impl Fn(&App) -> bool) -> Result<()> {
    let start = Instant::now();
    while !ready(app) {
        frame(app, previous)?;
        ensure!(
            start.elapsed() < Duration::from_secs(60),
            "App wait timed out"
        );
    }
    Ok(())
}
fn q(app: &mut App) {
    app.ui.handle_input(InputEvent::KeyDown {
        key: Key::Letter('q'),
        mods: Modifiers::NONE,
        repeat: false,
    });
    app.ui.handle_input(InputEvent::KeyUp {
        key: Key::Letter('q'),
        mods: Modifiers::NONE,
    });
    app.ui.update(0);
}

fn upward_throw(single: bool) -> Result<()> {
    let fixture = ContentRoot::synthetic()?;
    let packages = bri_package::packages::PackageSet::load_root(&fixture.root)?;
    let directory = packages.role_dir(&fixture.root, "map_bundle")?;
    let bundle: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("bundle.json"))?)?;
    let ids: Vec<_> = bundle["maps"]
        .as_array()
        .context("Fixture maps")?
        .iter()
        .filter_map(|map| map["id"].as_str())
        .collect();
    let mut rooms = bri_content::testing::map_bundle::rooms_for(&ids);
    let room = rooms.iter_mut().find(|r| r.id == fixture.map.0).unwrap();
    room.half = 200.;
    room.height = 40.;
    room.props.clear();
    bri_content::testing::map_bundle::write_bundle(&directory, &rooms)?;
    let state = fixture.state()?;
    let mut app = App::load(&fixture.root, state.path(), (800, 600))?;
    app.host_on_any_port();
    app.ui.core.pop(ScreenId::DefaultControls);
    app.ui.update(0);
    app.ui.core.request(UiAction::HostGame {
        map: fixture.map.0.clone(),
        mode: if single {
            ServerMode::SinglePlayer
        } else {
            ServerMode::Lan
        },
        game_mode: None,
        max_players: if single { 1 } else { 8 },
        server_name: "Drop motion regression".into(),
        password: String::new(),
        admin_password: String::new(),
        super_admin_password: String::new(),
    });
    let mut previous = Instant::now();
    until(&mut app, &mut previous, |a| {
        matches!(a.ui.core.conn, ConnectionState::InGame { .. })
            && a.network_view()
                .is_some_and(|v| v.poses.get(&v.owner).is_some_and(|p| p.player.grounded))
    })?;
    q(&mut app);
    until(&mut app, &mut previous, |a| {
        a.ui.core.hud.tool_active
            && a.network_view()
                .is_some_and(|v| v.tools[&v.owner].selected == Some(0))
            && a.pending_requests() == 0
    })?;
    app.ui.core.request(UiAction::Game(GameAction::Look {
        yaw: 0.,
        pitch: -10000.,
    }));
    until(&mut app, &mut previous, |a| {
        a.network_view()
            .is_some_and(|v| v.poses[&v.owner].player.pitch > 1.5)
    })?;
    app.ui.core.request(UiAction::Game(GameAction::DropTool));
    until(&mut app, &mut previous, |a| {
        a.network_view()
            .is_some_and(|v| !v.weapons.drops.is_empty())
    })?;
    let view = app.network_view().unwrap();
    let owner = view.owner;
    let drop = &view.weapons.drops[0];
    ensure!(
        drop.item == bri_weapons::HAMMER,
        "Expected ordinary stock Hammer"
    );
    ensure!(drop.velocity.y > 15., "Expected upward launch: {drop:?}");
    let id = drop.id;
    let start_tick = view.tick;
    let (mut peak, mut shown_peak) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
    // Dropping the selected item puts its inventory away. Normalize only through Q.
    if app.ui.core.hud.tool_active {
        q(&mut app);
    }
    let (mut opened, mut closed) = (false, false);
    let start = Instant::now();
    let (height, velocity, shown_last) = loop {
        frame(&mut app, &mut previous)?;
        let view = app.network_view().context("Lost world")?;
        let elapsed = view.tick.saturating_sub(start_tick);
        let drop = view
            .weapons
            .drops
            .iter()
            .find(|d| d.id == id)
            .context("Drop disappeared")?;
        peak = peak.max(drop.position.y);
        let (height, velocity) = (drop.position.y, drop.velocity.y);
        let shown = app
            .world_item_instances()
            .find(|(identity, _)| *identity == ItemIdentity::Drop(id))
            .context("Missing presented drop")?
            .1
            .transform
            .w_axis
            .y;
        shown_peak = shown_peak.max(shown);
        ensure!(view.owner == owner && view.time_scale == 1.);
        // Finish once both views descend, before the falling Hammer can be
        // picked up by its stationary thrower. A frozen arc runs to the deadline.
        if elapsed >= 360
            || (closed && height < peak - 2. && velocity < -5. && shown < shown_peak - 2.)
        {
            break (height, velocity, shown);
        }
        if !opened && elapsed >= 48 {
            q(&mut app);
            ensure!(app.ui.core.hud.tool_active, "Q must open inventory");
            opened = true;
        } else if opened && !closed && elapsed >= 132 {
            q(&mut app);
            ensure!(!app.ui.core.hud.tool_active, "Q must close inventory");
            closed = true;
        }
        ensure!(
            start.elapsed() < Duration::from_secs(30),
            "World ticks stalled"
        );
    };
    ensure!(closed && opened);
    ensure!(
        height < peak - 2. && velocity < -5.,
        "Authority froze at apex: peak={peak}, y={height}, vy={velocity}"
    );
    ensure!(shown_last < shown_peak - 2., "Presented drop froze at apex");
    Ok(())
}

#[test]
fn single_player_q_throw_descends_in_authority_and_presentation() -> Result<()> {
    upward_throw(true)
}
#[test]
fn listen_host_q_throw_descends_in_authority_and_presentation() -> Result<()> {
    upward_throw(false)
}
