//! A fresh source checkout has the default Add-Ons (packages/default-addons.json)
//! with no setup step: starting the game installs them into its generated
//! content (here the startup check, opted in; a plain check changes
//! nothing), then /dup gives the Duplicator and a Stunt Plane spawns, in
//! single player and for a guest who joins a LAN game.
//!
//! The checkout is laid out in a temporary folder: `packages/` as committed
//! and a `content/` holding only the generated base game (hard-linked from
//! BRI_CONTENT, else the workspace `content/`), as bootstrap leaves it.
//! Run: cargo test -p bri-client --test default_add_ons -- --ignored --nocapture
use anyhow::{Context, Result, bail, ensure};
use bri_client::{app::App, platform::PlatformApp};
use bri_package::{defaults, packages::PackageSet};
use bri_ui::{api::*, screens::ScreenId};
use sha2::Digest;
use std::{
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

const SIZE: (u32, u32) = (960, 720);
const SLATE: &str = "v20/add-ons/map_slate/slate.mis";
const TOOL: &str = "duplicator-tool:weapon/duplicator";
const PLANE: &str = "vehicle_stunt_plane:vehicle/stuntplanevehicle";
const VEHICLE_SPAWN: &str = "v20/brick/brickvehiclespawndata";

fn generated_content() -> PathBuf {
    std::env::var_os("BRI_CONTENT").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"),
        PathBuf::from,
    )
}

/// A source checkout in a temporary folder, removed on drop.
struct Checkout {
    root: PathBuf,
}
impl Checkout {
    fn new(generated: &Path) -> Result<Self> {
        // One folder per checkout: tests in this binary run in parallel.
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "bri-fresh-checkout-{}-{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let checkout = Self { root };
        // packages/: the list and the default Add-Ons, as committed.
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages");
        let packages = checkout.root.join("packages");
        std::fs::create_dir_all(&packages)?;
        std::fs::copy(
            repo.join(defaults::LIST_FILE),
            packages.join(defaults::LIST_FILE),
        )?;
        for addon in defaults::list() {
            copy_dir(&repo.join(&addon.path), &packages.join(&addon.path), false)?;
        }
        // content/: the generated base game and nothing else.
        for package in PackageSet::base().packages {
            let from = generated.join(&package.dir);
            ensure!(
                from.is_dir(),
                "{} lacks {}: run python tools/bootstrap.py",
                generated.display(),
                package.dir
            );
            copy_dir(&from, &checkout.content().join(&package.dir), true)?;
        }
        Ok(checkout)
    }
    fn content(&self) -> PathBuf {
        self.root.join("content")
    }
}
impl Drop for Checkout {
    fn drop(&mut self) {
        // Removing a hard link leaves the generated original alone.
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Copy a folder; `link` hard-links its files instead where it can (the
/// base game is large and only read).
fn copy_dir(from: &Path, to: &Path, link: bool) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target, link)?;
        } else if !link || std::fs::hard_link(entry.path(), &target).is_err() {
            std::fs::copy(entry.path(), &target)
                .with_context(|| format!("Copying {}", entry.path().display()))?;
        }
    }
    Ok(())
}

/// Every package manifest under `dir` (the download cache).
fn manifests(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut out = Vec::new();
    for path in entries.flatten().map(|e| e.path()) {
        if path.is_dir() {
            out.extend(manifests(&path));
        } else if path.file_name().is_some_and(|n| n == "package.json") {
            out.push(path);
        }
    }
    out
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

fn app(root: &Path, state: &Path, name: &str) -> Result<App> {
    let state = state.join(name);
    let _ = std::fs::remove_dir_all(&state);
    let mut app = App::load(root, &state, SIZE)?;
    app.ui.core.pop(ScreenId::DefaultControls);
    app.ui.core.settings.avatar.lan_name = name.into();
    Ok(app)
}

fn host(app: &mut App, mode: ServerMode, port: u16) -> Result<()> {
    app.ui
        .core
        .prefs
        .set("$Pref::Server::Port", port.to_string());
    request(
        app,
        UiAction::HostGame {
            map: SLATE.into(),
            mode,
            game_mode: None,
            max_players: 4,
            server_name: "Default Add-Ons".into(),
            password: String::new(),
            admin_password: String::new(),
            super_admin_password: String::new(),
        },
    )
}

fn free_port() -> Result<u16> {
    Ok(std::net::UdpSocket::bind("127.0.0.1:0")?
        .local_addr()?
        .port())
}

/// The Vehicle list a vehicle spawn brick's wrench offers.
fn offers_plane(app: &App) -> bool {
    app.ui
        .core
        .datablocks
        .get("Vehicle")
        .is_some_and(|list| list.iter().any(|c| c.id == PLANE))
}

fn holds_duplicator(app: &App) -> bool {
    app.network_view().is_some_and(|v| {
        v.tools
            .get(&v.owner)
            .is_some_and(|t| t.slots.iter().any(|s| s.as_deref() == Some(TOOL)))
    })
}

fn sees_plane(app: &App) -> bool {
    app.network_view()
        .is_some_and(|v| v.vehicles.values().any(|v| v.definition == PLANE))
}

/// `/dup` typed in chat, as a player does.
fn dup(app: &mut App) -> Result<()> {
    request(
        app,
        UiAction::ChatCommand {
            name: "dup".into(),
            args: vec![],
        },
    )
}

/// The host loads a saved build of one vehicle spawn brick set to the
/// Stunt Plane, beside the host's player, and the plane spawns on it.
fn load_plane_spawn(app: &mut App, state: &Path) -> Result<()> {
    let view = app.network_view().context("not in a game")?;
    let player = &view.poses.get(&view.owner).context("no player")?.player;
    let feet = glam::Vec3::from(player.feet);
    let map_id = view.world.map_id.clone();
    let mut world = bri_world::World::new(
        "Stunt Plane".into(),
        map_id.clone(),
        view.world.palette.clone(),
    );
    let mut brick = bri_world::Brick::new(
        bri_world::ContentRef::Resolved(VEHICLE_SPAWN.into()),
        [
            feet.x.round() + 12.0,
            (feet.y / 0.2).round() * 0.2 + 0.1,
            feet.z.round(),
        ],
        view.owner,
    );
    brick.vehicle = Some(bri_world::VehicleSpawn {
        vehicle: bri_world::ContentRef::Resolved(PLANE.into()),
        recolor: false,
    });
    world.bricks.insert(1, brick);
    world.next_brick_id = 2;
    let folder = state
        .join("saves")
        .join(format!("map-{:x}", sha2::Sha256::digest(map_id.as_bytes())));
    std::fs::create_dir_all(&folder)?;
    std::fs::write(
        folder.join("plane.world.json"),
        serde_json::to_vec(&bri_world::build::SavedBuild::new(world))?,
    )?;
    let map = app
        .content
        .maps
        .iter()
        .find(|m| m.id == map_id)
        .context("the map's name")?
        .name
        .clone();
    request(
        app,
        UiAction::LoadBricks {
            map,
            name: "plane.world.json".into(),
            ownership: true,
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

#[test]
#[ignore = "generated content (BRI_CONTENT or content/), the bri-client binary and loopback UDP; no window"]
fn a_fresh_checkout_gives_the_duplicator_and_spawns_the_stunt_plane() -> Result<()> {
    let checkout = Checkout::new(&generated_content())?;
    let content = checkout.content();
    ensure!(
        !content.join("addons").exists() && !content.join("packages.json").exists(),
        "the fresh checkout already has Add-Ons"
    );

    // 1. A plain startup check (what the push gate runs over the shared
    //    content) validates and changes nothing.
    let state = checkout.root.join("state");
    let check = |install: bool| {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_bri-client"));
        command.env_remove("BRI_INSTALL_DEFAULT_ADD_ONS");
        if install {
            command.env("BRI_INSTALL_DEFAULT_ADD_ONS", "1");
        }
        command
            .arg("--check")
            .arg(&content)
            .arg(state.join("check"))
            .output()
    };
    let out = check(false)?;
    ensure!(
        out.status.success(),
        "bri-client --check failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    ensure!(
        !content.join("addons").exists() && !content.join("packages.json").exists(),
        "a plain --check changed the content"
    );

    // 2. The check opted in, as a fresh checkout's first run does, sets the
    //    default Add-Ons up without writing a package list.
    let out = check(true)?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    ensure!(out.status.success(), "bri-client --check failed:\n{text}");
    ensure!(
        text.contains(
            "Installed the default Add-Ons duplicator, duplicator-tool, vehicle_stunt_plane, brick_mirror, ragdoll."
        ) && text.contains("Startup validation passed"),
        "{text}"
    );
    for addon in defaults::list() {
        ensure!(
            content.join(addon.dir()).join("package.json").is_file(),
            "{} was not installed",
            addon.id
        );
    }
    ensure!(
        !content.join("packages.json").exists(),
        "the check wrote a package list, which would stop the checkout following the base game's"
    );
    let listed: Vec<String> = PackageSet::load_root(&content)?
        .packages
        .into_iter()
        .map(|p| p.id)
        .collect();
    for addon in defaults::list() {
        ensure!(
            listed.contains(&addon.id) == addon.enabled,
            "{} should be {}: {listed:?}",
            addon.id,
            if addon.enabled { "on" } else { "off" }
        );
    }
    println!("check: installed and on: {listed:?}");

    // 3. Single player: /dup puts the Duplicator in hand, and the Stunt
    //    Plane is on offer and spawns.
    let mut solo = app(&content, &state, "Solo")?;
    host(&mut solo, ServerMode::SinglePlayer, free_port()?)?;
    until(&mut [&mut solo], "single player in game", 180, |a| {
        in_game(a[0])
    })?;
    // The wrench's vehicle list is the game's, filled once it starts.
    ensure!(
        offers_plane(&solo),
        "single player's vehicle list lacks {PLANE}"
    );
    dup(&mut solo)?;
    until(&mut [&mut solo], "/dup in single player", 30, |a| {
        holds_duplicator(a[0])
    })?;
    load_plane_spawn(&mut solo, &state.join("Solo"))?;
    until(
        &mut [&mut solo],
        "the Stunt Plane in single player",
        60,
        |a| sees_plane(a[0]),
    )?;
    println!("single player: /dup gave the Duplicator; the Stunt Plane spawned");
    leave(&mut [&mut solo])?;
    drop(solo);

    // 4. A LAN game: a guest from the same checkout joins with nothing to
    //    download, /dup gives them the Duplicator, and they see and are
    //    offered the Stunt Plane the host spawns.
    let port = free_port()?;
    let mut host_app = app(&content, &state, "Host")?;
    let mut guest = app(&content, &state, "Guest")?;
    host(&mut host_app, ServerMode::Lan, port)?;
    until(&mut [&mut host_app], "host in game", 180, |a| in_game(a[0]))?;
    request(
        &mut guest,
        UiAction::JoinServer {
            address: format!("127.0.0.1:{port}"),
            password: String::new(),
        },
    )?;
    until(
        &mut [&mut host_app, &mut guest],
        "guest in game",
        240,
        |a| in_game(a[1]),
    )?;
    ensure!(
        offers_plane(&guest),
        "the guest's vehicle list lacks {PLANE}"
    );
    dup(&mut guest)?;
    until(
        &mut [&mut host_app, &mut guest],
        "/dup for the guest",
        30,
        |a| holds_duplicator(a[1]),
    )?;
    load_plane_spawn(&mut host_app, &state.join("Host"))?;
    until(
        &mut [&mut host_app, &mut guest],
        "the Stunt Plane on the guest's screen",
        60,
        |a| sees_plane(a[0]) && sees_plane(a[1]),
    )?;
    let downloaded = manifests(&content.join(".downloads"));
    ensure!(
        downloaded.is_empty(),
        "the guest downloaded Add-Ons it already had: {downloaded:?}"
    );
    println!("guest: joined, /dup gave the Duplicator, the Stunt Plane spawned");
    leave(&mut [&mut guest, &mut host_app])?;
    Ok(())
}

/// The Mirror Add-On ships no geometry: its brick is the base game's
/// 1x4x5 window, found by shape, with the window's menu icon and placement,
/// and its mirrors are the window's two broad faces.
#[test]
#[ignore = "generated content (BRI_CONTENT or content/)"]
fn the_mirror_is_the_base_games_window_with_mirror_faces() -> Result<()> {
    const WINDOW: &str = "v20/brick/brick4x1x5windowdata";
    const MIRROR: &str = "brick_mirror:brick/brickmirror1x4x5data";
    let checkout = Checkout::new(&generated_content())?;
    let content = checkout.content();
    defaults::install(
        &content,
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages"),
    )?;
    let loaded = bri_client::content::ClientContent::load(&content)?;
    let menu = |id: &str| {
        loaded
            .bricks
            .iter()
            .find(|b| b.id == id)
            .with_context(|| format!("{id} is not in the brick menu"))
    };
    let icon = &menu(WINDOW)?.icon;
    ensure!(
        *icon != bri_ui::api::IconRef::None && menu(MIRROR)?.icon == *icon,
        "the mirror lacks the window's icon"
    );
    let fix = |id: &str| {
        loaded
            .selectable
            .iter()
            .find(|(b, _)| b == id)
            .map(|(_, f)| *f)
    };
    ensure!(
        fix(MIRROR) == fix(WINDOW),
        "the mirror turns unlike the window"
    );
    let paths = &loaded.paths;
    let definitions = bri_sim::definitions::Definitions::load_with(
        &paths.brick_catalog,
        &paths.geometry,
        &paths.brick_extras,
    )?;
    let (mirror, window) = (
        &definitions.entries[MIRROR].mesh,
        &definitions.entries[WINDOW].mesh,
    );
    ensure!(mirror.id == window.id);
    // The window's glass would film the reflection over: the mirror
    // draws the window's frame without it.
    let glass = |mesh: &bri_content::brick::Brick| {
        mesh.quads
            .iter()
            .filter(|q| q.colors.is_some_and(|c| c.iter().any(|v| v[3] < 1.0)))
            .count()
    };
    eprintln!(
        "window {} quads ({} translucent), mirror {} ({} translucent)",
        window.quads.len(),
        glass(window),
        mirror.quads.len(),
        glass(mirror)
    );
    for quad in window
        .quads
        .iter()
        .filter(|q| q.colors.is_some_and(|c| c.iter().any(|v| v[3] < 1.0)))
    {
        eprintln!("window glass at {:?}", quad.vertices.map(|v| v.position));
    }
    ensure!(
        mirror.quads.len() < window.quads.len() && glass(mirror) < glass(window).max(1),
        "the mirror still draws the window's glass"
    );
    let shapes = bri_client::mirrors::shapes(&definitions);
    let quads = shapes[MIRROR].quads();
    ensure!(quads.len() == 2, "{} mirror faces", quads.len());
    for quad in quads {
        let [width, height] = [quad[1] - quad[0], quad[3] - quad[0]].map(|edge| edge.length());
        // Four studs (2 units) by five bricks (3 units), less the frame.
        ensure!(
            width.max(height) > 2.5 && width.min(height) > 1.5,
            "a mirror face is {width} by {height}: not the window's broad side"
        );
    }
    Ok(())
}
