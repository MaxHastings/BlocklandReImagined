//! The bundled originals as a player gets them, end to end: the release's
//! bundle (`tools/addon_bundle.py build`, then `install`) laid into a game
//! folder, turned on by a `packages.json` written before their host rules
//! were installed (a player who turned them on in an earlier build), hosted
//! the way the game hosts (`bri_net::dedicated`) and played by a real
//! client over loopback QUIC.
//!
//! Every original the release bundles whose CC0 stand-in is in
//! `tests/fixtures/ports` goes through it; each one's host rules must load
//! right after it, and the ones whose rules make them work at all (the
//! Hookshot's pull, the Grapple Rope's rope, the Fill Can's fill) must do
//! it through the client. Made-up base game (`bri_net::testing`), no
//! original game assets.
use anyhow::{Context, Result, bail};
use bri_net::{
    client::{Client, ClientEvent},
    dedicated, server,
};
use bri_package::{library::Library, packages::PackageSet};
use bri_sim::{
    player::{MoveInput, PlayerState, PlayerTuning},
    session::{Command, PackageCommand, Reply, ToolInventory},
};
use glam::Vec3;
use serde_json::{Value, json};
use std::f32::consts::PI;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

const MAP: &str = bri_net::testing::MAP;
const HOOKSHOT: &str = "weapon_loz_hookshot";
const ROPE: &str = "tool_grapplerope";
const FILL_CAN: &str = "tool_fill_can";

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn stand_ins() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ports")
}

fn read(path: &Path) -> Value {
    serde_json::from_slice(
        &std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display())),
    )
    .unwrap()
}

/// Every original the release bundles that has a stand-in here, as
/// (classic name, package id); and fails when an original whose port has
/// host rules has none, so no ported original goes untested.
fn bundled_originals() -> Vec<(String, String)> {
    let list = read(&repo().join("packages/default-addons.json"));
    let ports = read(&repo().join("crates/addon-import/ports/ports.json"));
    let mut out = Vec::new();
    for addon in list["addons"].as_array().unwrap() {
        let Some(original) = addon.get("original") else {
            continue;
        };
        let name = original["addon"].as_str().unwrap();
        let id = addon["id"].as_str().unwrap();
        if stand_ins().join(name).is_dir() {
            out.push((name.to_owned(), id.to_owned()));
            continue;
        }
        let port = ports["ports"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["addon"].as_str() == Some(name));
        let rules = port.is_some_and(|p| {
            let dir = repo()
                .join("crates/addon-import/ports")
                .join(p["port"].as_str().unwrap());
            read(&dir.join("port.json")).get("rules").is_some()
        });
        assert!(
            !rules,
            "{name} ships with host rules but has no stand-in in tests/fixtures/ports to test them"
        );
    }
    out
}

fn python() -> &'static str {
    ["python3", "python"]
        .into_iter()
        .find(|p| {
            std::process::Command::new(p)
                .arg("--version")
                .output()
                .is_ok_and(|o| o.status.success())
        })
        .expect("Python 3 runs tools/addon_bundle.py")
}

/// The release's path for the originals: a stand-in checkout whose default
/// list pins each stand-in, `addon_bundle.py build` against this game's
/// content, and `install` into it, where a release holds its Add-Ons.
fn install_bundle(root: &Path, originals: &[(String, String)]) {
    let checkout = root.with_extension("checkout");
    let _ = std::fs::remove_dir_all(&checkout);
    let addons: Vec<Value> = originals
        .iter()
        .map(|(name, id)| {
            let sha = bri_addon_import::import(&bri_addon_import::Options {
                input: stand_ins().join(name),
                out: checkout.join("sha").join(id),
                ..Default::default()
            })
            .unwrap()
            .source
            .sha256;
            json!({ "id": id, "enabled": false, "original": {
                "addon": name, "title": name, "authors": ["Tester"],
                "version": "1.0.0", "sha256": [sha] } })
        })
        .collect();
    let _ = std::fs::remove_dir_all(checkout.join("sha"));
    std::fs::create_dir_all(checkout.join("packages")).unwrap();
    std::fs::write(
        checkout.join("packages/default-addons.json"),
        serde_json::to_vec_pretty(&json!({ "schema_version": 2, "addons": addons })).unwrap(),
    )
    .unwrap();
    for file in [
        "crates/addon-import/ports/ports.json",
        "crates/package/base-packages.json",
    ] {
        std::fs::create_dir_all(checkout.join(file).parent().unwrap()).unwrap();
        std::fs::copy(repo().join(file), checkout.join(file)).unwrap();
    }
    std::fs::write(checkout.join("core.cs"), "").unwrap();
    std::fs::create_dir_all(checkout.join("v20/base")).unwrap();
    std::fs::create_dir_all(checkout.join("v20/Add-Ons")).unwrap();
    let path = |p: &Path| p.to_str().unwrap().to_owned();
    let run = |args: &[&str]| {
        let out = std::process::Command::new(python())
            .arg(repo().join("tools/addon_bundle.py"))
            .args(args)
            .arg("--repo")
            .arg(&checkout)
            .env_remove("BRI_ADDON_SEARCH")
            .env_remove("BRI_V20")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "addon_bundle.py {args:?}: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    };
    run(&[
        "build",
        "--search",
        &path(&stand_ins()),
        "--v20",
        &path(&checkout.join("v20")),
        "--core",
        &path(&checkout.join("core.cs")),
        "--importer",
        env!("CARGO_BIN_EXE_bri-import-addon"),
        "--out",
        &path(&checkout.join("bundle")),
        "--content-root",
        &path(root),
    ]);
    run(&[
        "install",
        "--bundle",
        &path(&checkout.join("bundle")),
        "--content-root",
        &path(root),
    ]);
    std::fs::remove_dir_all(&checkout).unwrap();
}

/// The companions (host rules) the installed original `id` names.
fn companions(root: &Path, id: &str) -> Vec<String> {
    read(&root.join("addons").join(id).join("package.json"))["companions"]
        .as_array()
        .map(|c| c.iter().map(|v| v.as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
}

/// The first item the installed original `id` adds.
fn item_of(root: &Path, id: &str) -> String {
    let weapons = read(&root.join("addons").join(id).join("assets/weapons.json"));
    weapons["items"]
        .as_object()
        .and_then(|items| items.keys().next().cloned())
        .unwrap_or_else(|| panic!("{id} adds no item"))
}

/// The game folder: the made-up base game with the bundle installed and
/// `packages.json` turning every original on, in one of two ways.
struct Game {
    _scratch: bri_content::testing::ScratchDir,
    root: PathBuf,
    originals: Vec<(String, String)>,
}

#[derive(Clone, Copy, Debug)]
enum TurnedOn {
    /// By a list written before their host rules were installed: each
    /// original, nothing after it (a player who turned them on in an
    /// earlier build).
    ByAnOldList,
    /// In the Add-Ons screen, one after another.
    InTheAddOnsScreen,
}

impl Game {
    fn new(how: TurnedOn) -> Self {
        let scratch = bri_content::testing::ScratchDir::new("bundled-in-game").unwrap();
        let root = scratch.path().to_path_buf();
        bri_net::testing::write_root(&root, &[MAP]).unwrap();
        let originals = bundled_originals();
        install_bundle(&root, &originals);
        match how {
            TurnedOn::ByAnOldList => {
                let library = Library::scan(&root).unwrap();
                let mut set = PackageSet::load(&root.join("packages.json")).unwrap();
                set.packages.extend(originals.iter().map(|(_, id)| {
                    library
                        .get(id)
                        .unwrap_or_else(|| panic!("{id} is not installed"))
                        .package
                        .clone()
                }));
                std::fs::write(
                    root.join("packages.json"),
                    serde_json::to_vec_pretty(&set).unwrap(),
                )
                .unwrap();
            }
            TurnedOn::InTheAddOnsScreen => {
                for (_, id) in &originals {
                    let mut library = Library::scan(&root).unwrap();
                    let plan = library.plan(id, true);
                    assert!(plan.allowed(), "{id}: {:?}", plan.refused);
                    library.apply(&plan).unwrap();
                }
            }
        }
        Self {
            _scratch: scratch,
            root,
            originals,
        }
    }

    /// What the game loads: each original, then its host rules right
    /// after it. Returns the rules ids.
    fn check_list(&self, how: TurnedOn) -> Vec<String> {
        let set = PackageSet::load_root(&self.root).unwrap();
        let ids: Vec<&str> = set.packages.iter().map(|p| p.id.as_str()).collect();
        let mut rules = Vec::new();
        for (_, id) in &self.originals {
            let at = ids
                .iter()
                .position(|p| p == id)
                .unwrap_or_else(|| panic!("{how:?}: {id} is not on: {ids:?}"));
            for (n, companion) in companions(&self.root, id).iter().enumerate() {
                assert_eq!(
                    ids.get(at + 1 + n),
                    Some(&companion.as_str()),
                    "{how:?}: {id}'s host rules {companion} do not load right after it: {ids:?}"
                );
                rules.push(companion.clone());
            }
        }
        rules
    }
}

#[test]
fn bundled_originals_load_their_host_rules_however_they_were_turned_on() {
    for how in [TurnedOn::ByAnOldList, TurnedOn::InTheAddOnsScreen] {
        let game = Game::new(how);
        let rules = game.check_list(how);
        for name in [HOOKSHOT, ROPE, FILL_CAN] {
            assert!(
                rules.contains(&format!("{name}-rules")),
                "{name} has host rules: {rules:?}"
            );
        }
        // In the Add-Ons screen's library each is part of its original: no
        // switch of its own, on and off with it.
        let library = Library::scan(&game.root).unwrap();
        for (_, id) in &game.originals {
            for rules in companions(&game.root, id) {
                assert_eq!(library.companion_of(&rules), Some(id.as_str()));
                for on in [false, true] {
                    let plan = library.plan(&rules, on);
                    assert!(
                        !plan.allowed(),
                        "{how:?}: {rules} can be turned {} by itself",
                        if on { "on" } else { "off" }
                    );
                }
                assert!(
                    library.plan(id, false).also.contains(&rules),
                    "{how:?}: turning {id} off leaves {rules} on"
                );
            }
        }
        // The host loads every one of them.
        let host = dedicated::load(
            &game.root,
            bri_world::World::new("Bundled".into(), MAP.into(), vec![[1.0; 4]; 4]),
        )
        .unwrap_or_else(|e| panic!("{how:?}: {e:#}"));
        let loaded: Vec<&str> = host
            .environment
            .packages
            .iter()
            .map(|p| p.id.as_str())
            .collect();
        for id in &rules {
            assert!(
                loaded.contains(&id.as_str()),
                "{how:?}: the host did not load {id}: {loaded:?}"
            );
        }
        assert!(
            host.session.package_diagnostics().is_empty(),
            "{how:?}: {:?}",
            host.session.package_diagnostics()
        );
    }
}

/// Over loopback QUIC, in a game hosted from the folder an old list turned
/// the originals on in: the Hookshot pulls its shooter, the Grapple Rope
/// holds its holder on the rope, and `/fillcan` gives the Fill Can, which
/// fills a brick with the colour picked. Each works only when its host
/// rules run.
#[test]
fn bundled_originals_work_for_a_client_in_a_hosted_game() -> Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?
        .block_on(played(Game::new(TurnedOn::ByAnOldList)))
}

async fn played(game: Game) -> Result<()> {
    let root = &game.root;
    let set = PackageSet::load_root(root)?;
    let host = dedicated::load(
        root,
        bri_world::World::new("Bundled".into(), MAP.into(), vec![[1.0; 4]; 4]),
    )?;
    let mut session = host.session;
    let mut loadout = ToolInventory::default();
    for (slot, id) in [HOOKSHOT, ROPE].into_iter().enumerate() {
        loadout.slots[slot] = Some(item_of(root, id));
    }
    session.set_spawn_loadout(loadout)?;
    let shelf = bri_net::packages::PackageShelf::new(root, &set, &host.environment)?;
    let server = server::start(
        session,
        server::ServerOptions {
            bind: "127.0.0.1:0".parse()?,
            environment: host.environment.clone(),
            spawn_points: host.spawn_points.clone(),
            certificate: None,
            map_loader: Some(host.setup.clone()),
            packages: Some(std::sync::Arc::new(shelf)),
        },
    )?;
    let mut client = Client::connect(
        server.address,
        &server.certificate,
        "Player".into(),
        host.environment.client_packages(),
        None,
    )
    .await?;
    let result = play(&mut client).await;
    client.close();
    server.stop().await?;
    result
}

struct Player<'a> {
    client: &'a mut Client,
    moves: u64,
}

impl Player<'_> {
    fn me(&self) -> Option<PlayerState> {
        self.client
            .replica
            .poses
            .get(&self.client.owner)
            .map(|p| p.player.clone())
    }
    fn feet(&self) -> Vec3 {
        Vec3::from(self.me().expect("in the game").feet)
    }
    async fn until(&mut self, what: &str, done: impl Fn(&Self) -> bool) -> Result<()> {
        let waited = tokio::time::timeout(Duration::from_secs(15), async {
            while !done(self) {
                if let ClientEvent::Notice(notice) = self.client.receive().await? {
                    println!("notice: {notice:?}");
                }
            }
            Result::<()>::Ok(())
        })
        .await;
        waited.with_context(|| format!("timed out waiting for {what}"))?
    }
    async fn command(&mut self, command: Command) -> Result<Reply> {
        self.client.command(command).await
    }
    /// Face `yaw`, `pitch` (radians) until the host has the look.
    async fn look(&mut self, yaw: f32, pitch: f32) -> Result<()> {
        self.moves += 1;
        let sequence = self.moves;
        self.client.movement(
            sequence,
            &[MoveInput {
                yaw,
                pitch,
                ..Default::default()
            }],
            None,
            None,
        )?;
        self.until("the look", |p| {
            p.client
                .replica
                .poses
                .get(&p.client.owner)
                .is_some_and(|pose| pose.acknowledged_input >= sequence)
        })
        .await
    }
    /// Hold the tool in `slot`, ready to use, standing.
    async fn equip(&mut self, slot: usize) -> Result<()> {
        self.command(Command::EquipTool { slot: Some(slot) })
            .await?;
        self.ready().await
    }
    /// The item in hand ready to use, standing.
    async fn ready(&mut self) -> Result<()> {
        let owner = self.client.owner;
        self.until("the tool ready", |p| {
            p.me().is_some_and(|m| m.grounded)
                && p.client
                    .replica
                    .weapons
                    .images
                    .get(&owner)
                    .is_some_and(|images| images.iter().any(|i| i.state == "Ready"))
        })
        .await
    }
    /// Walk with `forward` (-1 backs away) facing `yaw`, `pitch`, sending
    /// one input a tick as a client does, for `time`.
    async fn walk(&mut self, yaw: f32, pitch: f32, forward: f32, time: Duration) -> Result<()> {
        let end = tokio::time::Instant::now() + time;
        let mut tick = tokio::time::interval(Duration::from_millis(32));
        while tokio::time::Instant::now() < end {
            tick.tick().await;
            self.moves += 1;
            self.client.movement(
                self.moves,
                &[MoveInput {
                    yaw,
                    pitch,
                    forward,
                    ..Default::default()
                }],
                None,
                None,
            )?;
            while self.client.queued() > 0 {
                self.client.receive().await?;
            }
        }
        let sequence = self.moves;
        self.until("the walk", |p| {
            p.client
                .replica
                .poses
                .get(&p.client.owner)
                .is_some_and(|pose| pose.acknowledged_input >= sequence)
        })
        .await
    }
    async fn click(&mut self) -> Result<()> {
        for down in [true, false] {
            self.command(Command::WeaponTrigger { down }).await?;
        }
        Ok(())
    }
}

async fn play(client: &mut Client) -> Result<()> {
    let mut p = Player { client, moves: 0 };
    p.until("standing in the game", |p| {
        p.me().is_some_and(|m| m.grounded)
    })
    .await?;

    // The Hookshot, aimed at the floor ahead, pulls its shooter there.
    p.equip(0).await?;
    let start = p.feet();
    p.look(0.0, -0.25).await?;
    p.click().await?;
    p.until("the Hookshot to pull its shooter", |p| {
        let moved = p.feet() - start;
        Vec3::new(moved.x, 0.0, moved.z).length() > 3.0
    })
    .await?;

    // The Grapple Rope, struck into the floor ahead, holds its holder on
    // the rope: turning and walking away, they stay where they hung.
    p.equip(1).await?;
    p.look(0.0, -0.25).await?;
    p.command(Command::WeaponTrigger { down: true }).await?;
    p.walk(0.0, -0.25, 0.0, Duration::from_millis(1500)).await?;
    let roped = p.feet();
    p.walk(PI, 0.0, 1.0, Duration::from_secs(2)).await?;
    let held = p.feet().distance(roped);
    p.command(Command::WeaponTrigger { down: false }).await?;

    // Let go, the same walk carries them away.
    p.until("landing", |p| p.me().is_some_and(|m| m.grounded))
        .await?;
    let free = p.feet();
    p.walk(PI, 0.0, 1.0, Duration::from_secs(2)).await?;
    let walked = p.feet().distance(free);
    assert!(
        held < 0.75 * walked,
        "the Grapple Rope did not hold its holder: moved {held} on the rope, {walked} off it"
    );

    // The Fill Can fills a brick planted ahead of it.
    p.until("standing again", |p| p.me().is_some_and(|m| m.grounded))
        .await?;
    let feet = p.feet();
    // On the stud and plate grid, its bottom at the floor's grid line.
    let at = Vec3::new(
        (feet.x * 2.0).round() / 2.0,
        (feet.y / 0.2).floor() * 0.2 + 0.3,
        ((feet.z - 3.0) * 2.0).round() / 2.0,
    );
    const PAINTED: u8 = 3;
    let brick = match p
        .command(Command::Plant {
            definition: bri_net::testing::MENU_BRICKS[2].0.into(),
            position: at.to_array(),
            quarter_turns: 0,
            color: PAINTED,
        })
        .await?
    {
        Reply::Planted(id) => id,
        other => bail!("expected a plant, got {other:?}"),
    };
    // `/fillcan` (its host rules) puts it in hand; picking a colour keeps
    // it there with that colour.
    const COLOR: u8 = 1;
    p.command(Command::EquipTool { slot: None }).await?;
    let reply = p
        .command(Command::Package(PackageCommand {
            package: format!("{FILL_CAN}-rules"),
            command: "fillcan".into(),
            args: vec![],
        }))
        .await?;
    anyhow::ensure!(reply == Reply::Accepted, "/fillcan: {reply:?}");
    p.command(Command::UseSprayCan { color: COLOR }).await?;
    let owner = p.client.owner;
    p.until("the Fill Can in hand", |p| {
        p.client
            .replica
            .weapons
            .images
            .get(&owner)
            .is_some_and(|images| {
                images
                    .iter()
                    .any(|i| i.image.starts_with(FILL_CAN) && i.state == "Ready")
            })
    })
    .await?;
    let me = p.me().unwrap();
    let flat = at - Vec3::from(me.feet);
    let yaw = flat.x.atan2(-flat.z);
    let eye = PlayerState { yaw, ..me }.eye(&PlayerTuning::default());
    let d = at - eye;
    p.look(yaw, d.y.atan2(Vec3::new(d.x, 0.0, d.z).length()))
        .await?;
    p.click().await?;
    p.until("the Fill Can to fill the brick", |p| {
        p.client
            .replica
            .world
            .bricks
            .get(&brick)
            .is_some_and(|b| b.color == COLOR)
    })
    .await?;
    Ok(())
}
