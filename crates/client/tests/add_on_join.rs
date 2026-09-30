//! Hosting and joining with Add-Ons: a host requires and offers only the
//! Add-Ons it has on, toggles apply to the next game without a restart, and
//! a joiner who lacks an Add-On the host runs downloads it and joins.
//!
//! Every test runs on one generated content root (BRI_CONTENT, else the
//! checkout's `content/`), hosts on a free loopback port, and stages the
//! repository's Add-Ons it needs in a hidden folder of that root for its
//! length. Host and guest load their Add-On lists without writing the root's.
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

/// The host's list decides what a game runs. Add-Ons the host turned on and
/// off again are not required; ones it runs download, a HUD only in the game
/// mode that shows it; bricks reach the guest's menu; and client code (the
/// Ragdoll) runs for a guest who never turned it on, without asking, and for
/// no one when the host has it off. Uses the repository's Add-Ons, staged
/// in the content root for the test's length, on a free port.
#[test]
#[ignore = "generated content (BRI_CONTENT or content/) and loopback UDP; no window"]
fn add_ons_the_host_turns_off_are_not_required_and_ones_it_runs_download() -> Result<()> {
    const BRICKS: &str = "brick_portal";
    const RAGDOLL: &str = "ragdoll";
    let content = std::env::var_os("BRI_CONTENT").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"),
        PathBuf::from,
    );
    let repo = RepoAddOns::install(&content)?;
    // Everyone starts from the base game with no Add-On on.
    let mut base = bri_package::packages::PackageSet::load_root(&content)?;
    base.packages.retain(|p| p.role.is_some());
    let with = |ids: &[&str]| -> Result<bri_package::packages::PackageSet> {
        let mut set = base.clone();
        set.packages.extend(repo.with(ids)?);
        Ok(set)
    };
    // The Ragdoll the content has installed (off, as a release ships it):
    // the guest has exactly the host's code.
    let installed = bri_package::library::Library::scan(&content)?
        .get(RAGDOLL)
        .map(|e| e.package.clone())
        .filter(|p| !p.dir.starts_with('.'))
        .with_context(|| {
            format!(
                "{} has no Ragdoll installed; rerun tools/bootstrap.py",
                content.display()
            )
        })?;
    let port = std::net::UdpSocket::bind("127.0.0.1:0")?
        .local_addr()?
        .port();
    let mut host_app = app(&content, "Hoster")?;
    let mut guest = app(&content, "Joiner")?;
    let round = |host_app: &mut App,
                 guest: &mut App,
                 host_set: &bri_package::packages::PackageSet,
                 guest_set: &bri_package::packages::PackageSet,
                 mode: bool,
                 what: &str|
     -> Result<()> {
        host_app
            .apply_packages(host_set)
            .context("the host's Add-Ons")?;
        guest
            .apply_packages(guest_set)
            .context("the guest's Add-Ons")?;
        if mode {
            let mode = host_app
                .ui
                .core
                .game_modes
                .first()
                .cloned()
                .context("no game mode")?;
            host_mode(host_app, port, &mode)?;
        } else {
            host(host_app, port)?;
        }
        until(&mut [&mut *host_app], "host in game", 180, |a| {
            in_game(a[0])
        })?;
        join(guest, port)?;
        until(&mut [&mut *host_app, &mut *guest], what, 300, |a| {
            in_game(a[1])
        })?;
        Ok(())
    };

    // 1. Turned on, then off again in the same session (no restart): the
    //    host neither requires the Add-Ons nor shows the HUD.
    host_app.apply_packages(&with(&["stresslab-hud", BRICKS])?)?;
    round(
        &mut host_app,
        &mut guest,
        &base,
        &base,
        false,
        "guest in game (Add-Ons off)",
    )?;
    ensure!(
        host_panels(&host_app).is_empty(),
        "HUD of a disabled Add-On: {:?}",
        host_panels(&host_app)
    );
    println!("off: guest joined, no HUD");
    leave(&mut [&mut guest, &mut host_app])?;

    // 2. A HUD Add-On on: on Slate nobody sees it; in its game mode the
    //    guest downloads it and sees it.
    let hud = with(&["stresslab-hud", "stresslab-mode"])?;
    round(
        &mut host_app,
        &mut guest,
        &hud,
        &base,
        false,
        "guest in game (HUD on)",
    )?;
    ensure!(
        host_panels(&host_app).is_empty(),
        "HUD shown on Slate: {:?}",
        host_panels(&host_app)
    );
    leave(&mut [&mut guest, &mut host_app])?;
    round(
        &mut host_app,
        &mut guest,
        &hud,
        &base,
        true,
        "guest in the game mode",
    )?;
    until(
        &mut [&mut host_app, &mut guest],
        "guest sees the mode's panel",
        120,
        |a| !host_panels(a[1]).is_empty(),
    )?;
    let cache = guest_cache_ids(&guest);
    ensure!(
        cache.iter().any(|c| c == "stresslab-hud"),
        "HUD not downloaded: {cache:?}"
    );
    println!(
        "HUD: guest downloaded it and sees {:?}",
        host_panels(&guest)
    );
    leave(&mut [&mut guest, &mut host_app])?;

    // 3. A brick Add-On on: the guest downloads it, joins, and can use it.
    round(
        &mut host_app,
        &mut guest,
        &with(&[BRICKS])?,
        &base,
        false,
        "guest in game (bricks on)",
    )?;
    let has_bricks = |app: &App| {
        app.ui
            .core
            .bricks
            .iter()
            .any(|b| b.id.starts_with(&format!("{BRICKS}:")))
    };
    ensure!(
        has_bricks(&host_app),
        "the host's brick menu lacks {BRICKS}"
    );
    let cache = guest_cache_ids(&guest);
    ensure!(
        cache.iter().any(|c| c == BRICKS),
        "{BRICKS} not downloaded: {cache:?}"
    );
    ensure!(
        has_bricks(&guest),
        "the guest's brick menu lacks the downloaded {BRICKS}"
    );
    println!("bricks: guest joined with {BRICKS}");
    leave(&mut [&mut guest, &mut host_app])?;

    // 4. Client code follows the host. The host runs the Ragdoll and the
    //    guest, who has it off, runs it too without being asked (asked
    //    code waits for an answer before it runs); with the host's off and
    //    the guest's on, nobody runs it.
    let mut ragdoll = base.clone();
    ragdoll.packages.push(installed);
    round(
        &mut host_app,
        &mut guest,
        &ragdoll,
        &base,
        false,
        "guest in game (Ragdoll on)",
    )?;
    ensure!(
        guest.add_on_code_running() == ["Ragdoll"],
        "the host's Ragdoll does not run for the guest: {:?}",
        guest.add_on_code_running()
    );
    leave(&mut [&mut guest, &mut host_app])?;
    round(
        &mut host_app,
        &mut guest,
        &base,
        &ragdoll,
        false,
        "guest in game (Ragdoll off)",
    )?;
    ensure!(
        guest.add_on_code_running().is_empty(),
        "the guest runs the Ragdoll the host turned off: {:?}",
        guest.add_on_code_running()
    );
    println!("code: the guest runs the host's Ragdoll, and only the host's");
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

/// Every Add-On in the repository (`packages/`: the default Add-Ons, the
/// samples, the showcase and the Stress Lab), copied into a hidden folder
/// of the content root for the test's length, in dependency order.
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
                    side: info.side().unwrap_or(bri_package::packages::Side::Shared),
                    dir: format!("{folder}/{}", info.id),
                    role: None,
                });
            }
            ensure!(entries.len() > before, "a dependency cycle among the repository's Add-Ons");
        }
        Ok(Self { dir, entries })
    }
}
impl RepoAddOns {
    /// The entries of `ids` and every Add-On they need, dependencies first.
    fn with(&self, ids: &[&str]) -> Result<Vec<bri_package::packages::PackageEntry>> {
        let mut wanted: Vec<String> = ids.iter().map(|id| id.to_string()).collect();
        let mut i = 0;
        while i < wanted.len() {
            let entry = self
                .entries
                .iter()
                .find(|e| e.id == wanted[i])
                .with_context(|| format!("the repository has no {}", wanted[i]))?;
            let info: bri_package::library::PackageInfo = serde_json::from_slice(&std::fs::read(
                self.dir.join(&entry.id).join("package.json"),
            )?)?;
            for dependency in info.dependencies.keys() {
                if !wanted.contains(dependency) {
                    wanted.push(dependency.clone());
                }
            }
            i += 1;
        }
        Ok(self
            .entries
            .iter()
            .filter(|e| wanted.contains(&e.id))
            .cloned()
            .collect())
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
    // The content may already list Add-Ons turned on (a release's or a
    // checkout's default Add-Ons, a Stress Lab release's); add only the
    // rest, and expect the guest to download one of those.
    let added: Vec<_> = add_ons
        .entries
        .iter()
        .filter(|e| !set.packages.iter().any(|listed| listed.id == e.id))
        .cloned()
        .collect();
    let downloaded = ["duplicator-tool", "sample-bubble-blaster"]
        .into_iter()
        .find(|id| added.iter().any(|e| e.id == *id))
        .context("every repository weapon Add-On is already listed")?;
    set.packages.extend(added);
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
    // Nothing asks about the download; the guest agrees to the samples'
    // client code, the one question a join may ask.
    let start = Instant::now();
    while !in_game(&guest) {
        step(&mut host_app)?;
        step(&mut guest)?;
        if let ConnectionState::Failed { reason } = &guest.ui.core.conn {
            bail!("the guest could not join: {reason}");
        }
        request(&mut guest, UiAction::TrustAddOnCode)?;
        ensure!(start.elapsed() < Duration::from_secs(300), "the guest never joined");
        thread::sleep(Duration::from_millis(8));
    }
    let cache = guest_cache_ids(&guest);
    ensure!(
        cache.iter().any(|id| id == downloaded),
        "{downloaded} was not downloaded: {cache:?}"
    );
    leave(&mut [&mut guest, &mut host_app])?;
    Ok(())
}

/// The Stunt Plane, a default Add-On (packages/default-addons.json): a host
/// that runs it lists it among its spawnable vehicles, and a guest who has
/// it turned off downloads it, joins and can pick it too. Runs on a
/// release's content, which ships it on, and on a checkout's, whether or
/// not the game has installed it there yet.
#[test]
#[ignore = "generated content (BRI_CONTENT or content/) and loopback UDP; no window"]
fn a_guest_without_the_stunt_plane_downloads_it_and_can_spawn_it() -> Result<()> {
    const PLANE: &str = "vehicle_stunt_plane";
    const VEHICLE: &str = "vehicle_stunt_plane:vehicle/stuntplanevehicle";
    let content = std::env::var_os("BRI_CONTENT").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"),
        PathBuf::from,
    );
    let listed = bri_package::packages::PackageSet::load_root(&content)?;
    // The host runs the content's own copy when it has one on; otherwise
    // the repository's, staged in the content root for the test's length.
    let mut set = listed.clone();
    let _staged = if set.packages.iter().any(|p| p.id == PLANE) {
        None
    } else {
        let staged = RepoAddOns::install(&content)?;
        set.packages.push(
            staged
                .entries
                .iter()
                .find(|e| e.id == PLANE)
                .context("the repository has no Stunt Plane")?
                .clone(),
        );
        Some(staged)
    };
    // The guest has it off, as after turning it off in the Add-Ons screen.
    let mut without = listed;
    without.packages.retain(|p| p.id != PLANE);
    let spawnable = |app: &App| {
        app.ui
            .core
            .datablocks
            .get("Vehicle")
            .is_some_and(|list| list.iter().any(|c| c.id == VEHICLE))
    };
    let port = std::net::UdpSocket::bind("127.0.0.1:0")?.local_addr()?.port();
    let mut host_app = app(&content, "PlaneHost")?;
    host_app
        .apply_packages(&set)
        .context("the host loads the Stunt Plane")?;
    let mut guest = app(&content, "PlaneGuest")?;
    guest
        .apply_packages(&without)
        .context("the guest turns the Stunt Plane off")?;
    ensure!(!spawnable(&guest), "the guest has the Stunt Plane before joining");
    host(&mut host_app, port)?;
    until(&mut [&mut host_app], "host in game", 180, |a| in_game(a[0]))?;
    ensure!(spawnable(&host_app), "the host's vehicle list lacks {VEHICLE}");
    join(&mut guest, port)?;
    until(&mut [&mut host_app, &mut guest], "guest in game", 300, |a| {
        in_game(a[1])
    })?;
    let cache = guest_cache_ids(&guest);
    ensure!(
        cache.iter().any(|id| id == PLANE),
        "{PLANE} was not downloaded: {cache:?}"
    );
    ensure!(spawnable(&guest), "the guest's vehicle list lacks {VEHICLE}");
    leave(&mut [&mut guest, &mut host_app])?;
    Ok(())
}
