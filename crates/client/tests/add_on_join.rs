//! Hosting and joining with Add-Ons: a host requires and offers only the
//! Add-Ons it has on, toggles apply to the next game without a restart, and
//! a joiner who lacks an Add-On the host runs downloads it and joins.
//!
//! Needs two content roots: BRI_ADD_ON_HOST_ROOT (base game plus Add-Ons
//! under it, not yet listed in packages.json) and BRI_ADD_ON_JOIN_ROOT (base
//! game only). The host listens on UDP BRI_ADD_ON_PORT (default 28117) so a
//! game on 28000 is left alone. Skips when the roots are unset.
use anyhow::{Context, Result, bail, ensure};
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

fn host_mode(app: &mut App, port: u16, mode: &GameModeInfo) -> Result<()> {
    app.ui.core.prefs.set("$Pref::Server::Port", port.to_string());
    request(
        app,
        UiAction::HostGame {
            map: mode.map.clone().unwrap_or_else(|| SLATE.into()),
            mode: ServerMode::Lan,
            game_mode: Some(mode.id.clone()),
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

    // 2. A client-only Add-On on (the Stress Lab HUD): on Slate nobody sees
    //    it; in the Stress Lab game mode the guest downloads it and sees it.
    set_add_on(&mut host_app, "stresslab-hud", true)?;
    set_add_on(&mut host_app, "stresslab-mode", true)?;
    host(&mut host_app, port)?;
    until(&mut [&mut host_app], "host in game", 180, |a| in_game(a[0]))?;
    join(&mut guest, port)?;
    until(&mut [&mut host_app, &mut guest], "guest in game (HUD Add-On on)", 240, |a| in_game(a[1]))?;
    ensure!(host_panels(&host_app).is_empty(), "HUD shown on Slate: {:?}", host_panels(&host_app));
    leave(&mut [&mut guest, &mut host_app])?;
    let mode = host_app.ui.core.game_modes.first().cloned().context("no Stress Lab mode")?;
    host_mode(&mut host_app, port, &mode)?;
    until(&mut [&mut host_app], "host in the Stress Lab", 180, |a| in_game(a[0]))?;
    join(&mut guest, port)?;
    until(&mut [&mut host_app, &mut guest], "guest sees the miner panel", 240, |a| {
        in_game(a[1]) && !host_panels(a[1]).is_empty()
    })?;
    let cache = guest_cache_ids(&guest);
    ensure!(cache.iter().any(|c| c == "stresslab-hud"), "HUD not downloaded: {cache:?}");
    println!("client-only: guest downloaded the HUD and sees {:?}", host_panels(&guest));
    leave(&mut [&mut guest, &mut host_app])?;
    set_add_on(&mut host_app, "stresslab-mode", false)?;

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

/// Every Add-On in the repository (`packages/`: the Duplicator, the samples
/// and the Stress Lab), copied into a hidden folder of the content root
/// for the test's length, in dependency order.
struct RepoAddOns {
    dir: PathBuf,
    entries: Vec<bri_package::packages::PackageEntry>,
}
impl RepoAddOns {
    fn install(content: &Path) -> Result<Self> {
        let folder = format!(".repo-add-ons-{}", std::process::id());
        let dir = content.join(&folder);
        let _ = std::fs::remove_dir_all(&dir);
        let mut found = Vec::new();
        for source in manifests(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages")) {
            let info: bri_package::library::PackageInfo =
                serde_json::from_slice(&std::fs::read(source.join("package.json"))?)?;
            copy_dir(&source, &dir.join(&info.id))?;
            found.push(info);
        }
        ensure!(found.len() >= 10, "the repository's Add-Ons: {}", found.len());
        // Dependencies load first, as the Add-Ons screen orders them.
        let mut entries: Vec<bri_package::packages::PackageEntry> = Vec::new();
        while entries.len() < found.len() {
            let before = entries.len();
            for info in &found {
                let listed = |id: &String| entries.iter().any(|e| &e.id == id);
                if listed(&info.id)
                    || !info.dependencies.keys().all(|d| listed(d) || !found.iter().any(|f| &f.id == d))
                {
                    continue;
                }
                entries.push(bri_package::packages::PackageEntry {
                    id: info.id.clone(),
                    version: info.version.clone(),
                    side: bri_package::library::side_for_kinds(
                        info.provides.iter().map(|p| p.kind.as_str()),
                    )
                    .unwrap_or(bri_package::packages::Side::Shared),
                    dir: format!("{folder}/{}", info.id),
                    role: None,
                });
            }
            ensure!(entries.len() > before, "a dependency cycle among the repository's Add-Ons");
        }
        Ok(Self { dir, entries })
    }
}
impl Drop for RepoAddOns {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
fn manifests(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries.into_iter().filter(|p| p.is_dir()) {
        if path.join("package.json").is_file() {
            out.push(path);
        } else {
            out.extend(manifests(&path));
        }
    }
    out
}
fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// The a17 join failure: a host with the Duplicator on, a guest who has it
/// too, and the guest's item HUD refusing the Duplicator's wand ("Item HUD
/// catalog coverage mismatch"). The gate never enabled an Add-On, so it
/// never saw that. Here a host turns on every Add-On in the repository and
/// a guest with only the base game downloads them, loads them and joins.
#[test]
#[ignore = "generated content (BRI_CONTENT or content/) and loopback UDP; no window"]
fn a_guest_joins_a_host_running_every_repository_add_on() -> Result<()> {
    let content = std::env::var_os("BRI_CONTENT").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"),
        PathBuf::from,
    );
    let add_ons = RepoAddOns::install(&content)?;
    let mut set = bri_package::packages::PackageSet::load_root(&content)?;
    set.packages.extend(add_ons.entries.iter().cloned());
    let port = std::net::UdpSocket::bind("127.0.0.1:0")?.local_addr()?.port();
    let mut host_app = app(&content, "RepoHost")?;
    // What turning them on in the Add-Ons screen loads, without writing the
    // content root's lists.
    host_app
        .apply_packages(&set)
        .context("the host loads every repository Add-On")?;
    let mut guest = app(&content, "RepoGuest")?;
    host(&mut host_app, port)?;
    until(&mut [&mut host_app], "host in game", 180, |a| in_game(a[0]))?;
    join(&mut guest, port)?;
    // The guest agrees to the download and to the samples' client code, as
    // a player would.
    let start = Instant::now();
    while !in_game(&guest) {
        step(&mut host_app)?;
        step(&mut guest)?;
        if let ConnectionState::Failed { reason } = &guest.ui.core.conn {
            bail!("the guest could not join: {reason}");
        }
        request(&mut guest, UiAction::ApproveDownload)?;
        request(&mut guest, UiAction::TrustAddOnCode)?;
        ensure!(start.elapsed() < Duration::from_secs(300), "the guest never joined");
        thread::sleep(Duration::from_millis(8));
    }
    let cache = guest_cache_ids(&guest);
    ensure!(
        cache.iter().any(|id| id == "duplicator-tool"),
        "the Duplicator was not downloaded: {cache:?}"
    );
    leave(&mut [&mut guest, &mut host_app])?;
    Ok(())
}
