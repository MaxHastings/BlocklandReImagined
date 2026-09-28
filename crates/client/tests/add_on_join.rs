//! Hosting and joining with Add-Ons: a host requires and offers only the
//! Add-Ons it has on, toggles apply to the next game without a restart, and
//! a joiner who lacks an Add-On the host runs downloads it and joins.
//!
//! Needs two content roots: BRI_ADD_ON_HOST_ROOT (base game plus Add-Ons
//! under it, not yet listed in packages.json) and BRI_ADD_ON_JOIN_ROOT (base
//! game only). The host listens on UDP BRI_ADD_ON_PORT (default 28117) so a
//! game on 28000 is left alone. Skips when the roots are unset.
use anyhow::{Result, bail, ensure};
use bri_client::{app::App, platform::PlatformApp};
use bri_ui::{api::*, screens::ScreenId};
use std::{
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

const SIZE: (u32, u32) = (960, 720);
const SLATE: &str = "v20/add-ons/map_slate/slate.mis";

fn roots() -> Option<(PathBuf, PathBuf, u16)> {
    let host = PathBuf::from(std::env::var_os("BRI_ADD_ON_HOST_ROOT")?);
    let join = PathBuf::from(std::env::var_os("BRI_ADD_ON_JOIN_ROOT")?);
    let port = std::env::var("BRI_ADD_ON_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(28117);
    Some((host, join, port))
}

fn step(app: &mut App) -> Result<()> {
    app.tick(Duration::from_millis(16))?;
    app.ui.update(16);
    let commands = app.pump()?;
    ensure!(
        commands.is_empty(),
        "Unexpected window command {commands:?}"
    );
    Ok(())
}

fn request(app: &mut App, action: UiAction) -> Result<()> {
    app.ui.core.request(action);
    let commands = app.pump()?;
    ensure!(
        commands.is_empty(),
        "Unexpected window command {commands:?}"
    );
    app.ui.update(0);
    Ok(())
}

fn in_game(app: &App) -> bool {
    matches!(app.ui.core.conn, ConnectionState::InGame { .. })
        && app
            .network_view()
            .is_some_and(|v| v.poses.contains_key(&v.owner))
}

fn until(
    apps: &mut [&mut App],
    what: &str,
    secs: u64,
    ready: impl Fn(&[&mut App]) -> bool,
) -> Result<()> {
    let start = Instant::now();
    loop {
        for app in apps.iter_mut() {
            step(app)?;
        }
        if ready(apps) {
            return Ok(());
        }
        for app in apps.iter() {
            if let ConnectionState::Failed { reason } = &app.ui.core.conn {
                bail!("{what}: connection failed: {reason}");
            }
        }
        ensure!(
            start.elapsed() < Duration::from_secs(secs),
            "Timed out waiting for {what}"
        );
        thread::sleep(Duration::from_millis(8));
    }
}

fn app(root: &Path, name: &str) -> Result<App> {
    let state = std::env::temp_dir().join(format!("bri-add-on-join-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    let mut app = App::load(root, &state, SIZE)?;
    app.ui.core.pop(ScreenId::DefaultControls);
    app.ui.core.settings.avatar.lan_name = name.into();
    Ok(app)
}

fn set_add_on(app: &mut App, id: &str, enabled: bool) -> Result<()> {
    request(
        app,
        UiAction::SetAddOnEnabled {
            id: id.into(),
            enabled,
        },
    )?;
    ensure!(
        app.ui
            .core
            .add_ons
            .rows
            .iter()
            .any(|r| r.id == id && r.enabled == enabled),
        "{id} not {}: {}",
        if enabled { "on" } else { "off" },
        app.ui.core.add_ons.notice
    );
    Ok(())
}

fn host(app: &mut App, port: u16) -> Result<()> {
    app.ui
        .core
        .prefs
        .set("$Pref::Server::Port", port.to_string());
    request(
        app,
        UiAction::HostGame {
            map: SLATE.into(),
            mode: ServerMode::Lan,
            game_mode: None,
            max_players: 4,
            server_name: "Add-On join".into(),
            password: String::new(),
            admin_password: String::new(),
            super_admin_password: String::new(),
        },
    )
}

fn join(app: &mut App, port: u16) -> Result<()> {
    request(
        app,
        UiAction::JoinServer {
            address: format!("127.0.0.1:{port}"),
            password: String::new(),
        },
    )
}

fn leave(apps: &mut [&mut App]) -> Result<()> {
    for app in apps.iter_mut() {
        request(app, UiAction::Disconnect)?;
        app.ui.core.pop(ScreenId::MessageBox);
    }
    for _ in 0..60 {
        for app in apps.iter_mut() {
            step(app)?;
        }
        thread::sleep(Duration::from_millis(8));
    }
    Ok(())
}

fn host_panels(app: &App) -> Vec<String> {
    app.ui
        .core
        .package_panels
        .iter()
        .map(|p| p.title.clone())
        .collect()
}

#[test]
#[ignore = "two content roots and loopback UDP BRI_ADD_ON_PORT; no window"]
fn add_ons_the_host_turns_off_are_not_required_and_ones_it_runs_download() -> Result<()> {
    let Some((host_root, join_root, port)) = roots() else {
        eprintln!("BRI_ADD_ON_HOST_ROOT / BRI_ADD_ON_JOIN_ROOT unset: skipped");
        return Ok(());
    };
    let mut host_app = app(&host_root, "Hoster")?;
    let mut guest = app(&join_root, "Joiner")?;
    // Reset the host's list: everything beyond the base game off.
    request(&mut host_app, UiAction::DefaultAddOns)?;

    // 1. Turned on, then off again in the same session (no restart): the
    //    host neither requires the Add-On nor shows its HUD.
    for id in ["stresslab-hud", "brick_fence"] {
        set_add_on(&mut host_app, id, true)?;
        set_add_on(&mut host_app, id, false)?;
    }
    host(&mut host_app, port)?;
    until(&mut [&mut host_app], "host in game", 180, |a| in_game(a[0]))?;
    join(&mut guest, port)?;
    until(
        &mut [&mut host_app, &mut guest],
        "guest in game (Add-Ons off)",
        180,
        |a| in_game(a[1]),
    )?;
    ensure!(
        host_panels(&host_app).is_empty(),
        "HUD of a disabled Add-On: {:?}",
        host_panels(&host_app)
    );
    println!("off: guest joined, no HUD");
    leave(&mut [&mut guest, &mut host_app])?;

    // 2. A client-only Add-On on (the Stress Lab HUD): joiners need not
    //    have it, and on Slate nobody sees it.
    set_add_on(&mut host_app, "stresslab-hud", true)?;
    host(&mut host_app, port)?;
    until(&mut [&mut host_app], "host in game", 180, |a| in_game(a[0]))?;
    join(&mut guest, port)?;
    until(
        &mut [&mut host_app, &mut guest],
        "guest in game (HUD Add-On on)",
        240,
        |a| in_game(a[1]),
    )?;
    ensure!(
        host_panels(&host_app).is_empty(),
        "HUD shown on Slate: {:?}",
        host_panels(&host_app)
    );
    println!("client-only on: guest joined, no HUD on Slate");
    leave(&mut [&mut guest, &mut host_app])?;

    // 3. A brick Add-On on: the guest downloads it, joins, and can use it.
    set_add_on(&mut host_app, "stresslab-hud", false)?;
    set_add_on(&mut host_app, "brick_fence", true)?;
    host(&mut host_app, port)?;
    until(&mut [&mut host_app], "host in game", 180, |a| in_game(a[0]))?;
    ensure!(
        host_app
            .ui
            .core
            .bricks
            .iter()
            .any(|b| b.id.starts_with("brick_fence:")),
        "the host's brick menu lacks the fence after turning it on"
    );
    join(&mut guest, port)?;
    until(
        &mut [&mut host_app, &mut guest],
        "guest in game (brick Add-On on)",
        240,
        |a| in_game(a[1]),
    )?;
    let cache = guest_cache_ids(&guest);
    ensure!(
        cache.iter().any(|c| c == "brick_fence"),
        "fence not downloaded: {cache:?}"
    );
    ensure!(
        guest
            .ui
            .core
            .bricks
            .iter()
            .any(|b| b.id.starts_with("brick_fence:")),
        "the guest's brick menu lacks the downloaded fence"
    );
    println!("bricks: guest joined with the fence");
    leave(&mut [&mut guest, &mut host_app])?;
    Ok(())
}

fn guest_cache_ids(app: &App) -> Vec<String> {
    walk(&app.content.paths.root.join(".downloads"))
}

fn walk(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.join("package.json").is_file() {
            if let Ok(text) = std::fs::read_to_string(p.join("package.json"))
                && let Some(id) = text
                    .split("\"id\"")
                    .nth(1)
                    .and_then(|r| r.split('"').nth(1))
            {
                out.push(id.to_string());
            }
        } else if p.is_dir() {
            out.extend(walk(&p));
        }
    }
    out
}
