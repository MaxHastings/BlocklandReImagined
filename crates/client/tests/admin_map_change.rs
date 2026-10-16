//! Actual headless App/UI confirmation route, with invented native content.
//! Map selection sets the UI model and advances an ordinary frame; Change and
//! Yes are real screen-control inputs. No server command or confirmation is seeded.
use anyhow::{Context, Result, ensure};
use bri_client::{app::App, platform::PlatformApp, testing::content_root::ContentRoot};
use bri_ui::{
    api::{ConnectionState, ServerMode, UiAction},
    input::{InputEvent, Key, Modifiers, MouseButton},
    screens::ScreenId,
};
use std::{
    path::Path,
    time::{Duration, Instant},
};

const FIRST: &str = "first-map:map/main";
const SECOND: &str = "second-map:map/main";

fn step(app: &mut App) -> Result<()> {
    app.tick(Duration::from_millis(16))?;
    app.ui.update(16);
    ensure!(app.pump()?.is_empty(), "Unexpected native window command");
    if let ConnectionState::Failed { reason } = &app.ui.core.conn {
        anyhow::bail!("App connection failed: {reason}");
    }
    std::thread::sleep(Duration::from_millis(10));
    Ok(())
}

fn until(app: &mut App, label: &str, ready: impl Fn(&App) -> bool) -> Result<()> {
    let started = Instant::now();
    while !ready(app) {
        step(app)?;
        ensure!(
            !app.ui.core.admin.status.starts_with("Rejected:"),
            "{label}: {}",
            app.ui.core.admin.status
        );
        ensure!(
            started.elapsed() < Duration::from_secs(120),
            "{label} stalled: {:?}; status {}; stack {:?}; map {:?}",
            app.ui.core.conn,
            app.ui.core.admin.status,
            app.ui.stack(),
            app.network_view().map(|v| &v.world.map_id)
        );
    }
    Ok(())
}

fn click(app: &mut App, screen: ScreenId, control: &str) -> Result<()> {
    let (x, y) = app
        .ui
        .control_center(screen, control)
        .with_context(|| format!("Missing {screen:?} control {control}"))?;
    app.ui.handle_input(InputEvent::MouseMove { x, y });
    app.ui.handle_input(InputEvent::MouseDown {
        button: MouseButton::Left,
        x,
        y,
    });
    app.ui.handle_input(InputEvent::MouseUp {
        button: MouseButton::Left,
        x,
        y,
    });
    app.ui.update(0);
    Ok(())
}

fn add_native_maps(root: &Path) -> Result<()> {
    let mut set = serde_json::to_value(bri_package::packages::PackageSet::load_root(root)?)?;
    for (package, map) in [("first-map", FIRST), ("second-map", SECOND)] {
        let dir = root.join(package);
        let mut rooms = bri_content::testing::map_bundle::rooms_for(&[map]);
        rooms[0].name = package.into();
        if map == SECOND {
            rooms[0].origin.x = 80.;
        }
        bri_content::testing::map_bundle::write_bundle(&dir.join("native"), &rooms)?;
        std::fs::write(
            dir.join("package.json"),
            serde_json::to_vec(&serde_json::json!({
                "schema_version":1,"id":package,"version":"1.0.0","api":1,
                "name":package,"license":"MIT",
                "provides":[{"id":map,"kind":"map","file":"native/bundle.json"}]
            }))?,
        )?;
        set["packages"]
            .as_array_mut()
            .context("Fixture package index")?
            .push(
                serde_json::json!({"id":package,"version":"1.0.0","side":"shared","dir":package}),
            );
    }
    std::fs::write(root.join("packages.json"), serde_json::to_vec(&set)?)?;
    Ok(())
}

fn confirm_map_change(addon: bool, yes_button: bool) -> Result<()> {
    let fixture = ContentRoot::synthetic()?;
    add_native_maps(&fixture.root)?;
    let state = fixture.state()?;
    let mut app = App::load(&fixture.root, state.path(), (800, 600))?;
    app.ui.core.pop(ScreenId::DefaultControls);
    app.ui.update(0);
    let (start, target) = if addon {
        (FIRST, SECOND)
    } else {
        (fixture.map.0.as_str(), fixture.open_map.0.as_str())
    };
    app.ui.core.request(UiAction::HostGame {
        map: start.into(),
        mode: ServerMode::SinglePlayer,
        game_mode: None,
        max_players: 1,
        server_name: "Single-player map confirmation".into(),
        password: String::new(),
        admin_password: String::new(),
        super_admin_password: String::new(),
    });
    until(&mut app, "Hosting", |app| {
        matches!(app.ui.core.conn, ConnectionState::InGame { .. }) && app.network_view().is_some()
    })?;
    let owner = app.network_view().context("Missing hosted view")?.owner;
    app.ui.core.request(UiAction::OpenAdmin);
    until(&mut app, "Admin menu", |app| {
        app.ui.stack().contains(&ScreenId::Admin) && !app.ui.core.admin.busy()
    })?;
    app.ui.core.push(ScreenId::AdminMaps);
    app.ui.update(0);
    until(&mut app, "Host map list", |app| {
        !app.ui.core.admin.busy() && app.ui.core.admin.maps.iter().any(|m| m.id == target)
    })?;
    // This is a UI selection driver, not an injected admin/network operation.
    // Its normal update frame enables the Change button before the click.
    app.ui.core.admin.selected_map = Some(target.into());
    step(&mut app)?;
    click(&mut app, ScreenId::AdminMaps, "changeMapButton")?;
    until(&mut app, "Confirmation dialog", |app| {
        app.ui.stack().contains(&ScreenId::AdminConfirm)
    })?;
    if yes_button {
        click(&mut app, ScreenId::AdminConfirm, "Yes")?;
    } else {
        app.ui.handle_input(InputEvent::KeyDown {
            key: Key::Return,
            mods: Modifiers::NONE,
            repeat: false,
        });
        app.ui.update(0);
    }
    until(&mut app, "New map entered", |app| {
        app.network_view().is_some_and(|v| v.world.map_id == target)
            && matches!(app.ui.core.conn, ConnectionState::InGame { .. })
    })?;
    let view = app.network_view().context("Missing new-map view")?;
    assert_eq!(view.owner, owner);
    assert_eq!(view.world.map_id, target);
    for screen in [ScreenId::Admin, ScreenId::AdminMaps, ScreenId::AdminConfirm] {
        ensure!(
            !app.ui.stack().contains(&screen),
            "{screen:?} remained open"
        );
    }
    Ok(())
}

#[test]
fn single_player_stock_map_yes_and_enter_complete_the_actual_app_route() -> Result<()> {
    for yes in [true, false] {
        confirm_map_change(false, yes)?;
    }
    Ok(())
}

#[test]
fn single_player_addon_map_yes_and_enter_complete_the_actual_app_route() -> Result<()> {
    for yes in [true, false] {
        confirm_map_change(true, yes)?;
    }
    Ok(())
}
