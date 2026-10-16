//! A host's every map runs what its first map runs: Change Map goes through
//! the same setup as Start Game, Add-On scripts included. Synthetic content
//! and the repository's sample Add-On; no original game assets.
mod common;

use anyhow::Result;
use bri_admin::{Action, Request};
use bri_net::{
    client::Client,
    host_setup::{HostSetup, HostedAddOns, MapSession, SessionContent},
    server,
};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::session::{Command, MapListing, PackageCommand, Reply};
use bri_world::World;
use common::options;
use std::{path::Path, sync::Arc, time::Duration};

const POINTS: &str = "sample-survival-points";

fn add_ons(saves: &Path) -> HostedAddOns {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/samples");
    let set = PackageSet {
        schema_version: 1,
        packages: vec![PackageEntry {
            id: POINTS.into(),
            version: "1.0.0".into(),
            side: Side::Server,
            dir: POINTS.into(),
            role: None,
        }],
    };
    HostedAddOns {
        server: Arc::new(Catalog::load(&root, &set, true).unwrap()),
        mode: None,
        saves: Some(saves.to_path_buf()),
    }
}

fn content() -> SessionContent {
    SessionContent {
        animation_assets: Default::default(),
        tool_catalog: Default::default(),
        weapon_pack: bri_weapons::Pack {
            schema_version: bri_weapons::SCHEMA,
            id: "test.empty".into(),
            items: Default::default(),
            images: Default::default(),
            projectiles: Default::default(),
            external_projectiles: Default::default(),
            damage_types: Default::default(),
            explosions: Default::default(),
            sounds: Default::default(),
            effects: Default::default(),
            definitions: vec![],
            resources: vec![],
            diagnostics: vec![],
            bindings: vec![],
        },
        item_bounds: Default::default(),
        avatar_catalog: serde_json::from_value(serde_json::json!({
            "schema_version": 1, "id": "test", "rig": "rig.json", "rig_sha256": "",
            "parts": {"hat": ["none"], "accent": ["none"], "pack": ["none"],
                "secondpack": ["none"], "chest": ["chest"], "hip": ["pants"],
                "rarm": ["rarm"], "larm": ["larm"], "rhand": ["rhand"],
                "lhand": ["lhand"], "rleg": ["rshoe"], "lleg": ["lshoe"]},
            "accents_allowed": {}, "faces": ["smiley"],
            "decals": ["AAA-None"], "surfaces": {}, "textures": {
                "smiley": {"file": "smiley.png", "sha256": "", "source": "", "width": 1, "height": 1},
                "AAA-None": {"file": "none.png", "sha256": "", "source": "", "width": 1, "height": 1}},
            "defaults": {"parts": {}, "colors": {"head": [1.0, 0.88, 0.61, 1.0],
                "torso": [0.9, 0.9, 0.9, 1.0], "hat": [1.0, 1.0, 0.0, 1.0],
                "accent": [0.0, 0.2, 0.64, 0.7], "pack": [0.0, 0.4, 0.8, 1.0],
                "secondpack": [0.0, 1.0, 0.0, 1.0], "hip": [0.0, 0.0, 1.0, 1.0],
                "rarm": [0.9, 0.0, 0.0, 1.0], "larm": [0.9, 0.0, 0.0, 1.0],
                "rhand": [1.0, 0.88, 0.61, 1.0], "lhand": [1.0, 0.88, 0.61, 1.0],
                "rleg": [0.0, 0.0, 1.0, 1.0], "lleg": [0.0, 0.0, 1.0, 1.0]},
                "face": "smiley", "decal": "AAA-None"}
        }))
        .unwrap(),
        vehicle_pack: serde_json::from_value(serde_json::json!({
            "schema_version": bri_vehicles::schema::SCHEMA_VERSION, "definitions": [], "assets": [],
            "evidence": [], "unresolved": [], "animation_aliases": {}
        }))
        .unwrap(),
        body_mounts: Vec::new(),
        body_grips: Default::default(),
        package_body_grips: Default::default(),
        image_barrels: Default::default(),
        bot_kinds: Vec::new(),
        event_catalog: bri_events::testing::catalog(),
        event_sounds: Vec::new(),
    }
}

/// The synthetic map under another id: the session's world names its map.
fn map(id: &str) -> MapSession {
    MapSession {
        simulation: common::simulation_with(World::new(
            "Stress".into(),
            id.into(),
            vec![[1.0; 4], [0.0; 4]],
        )),
        spawn_points: options().spawn_points,
        breakables: Vec::new(),
        teleporters: Vec::new(),
        tutorial: None,
    }
}

async fn points(client: &mut Client) -> Result<Reply> {
    client
        .command(Command::Package(PackageCommand {
            package: POINTS.into(),
            command: "top".into(),
            args: vec![],
        }))
        .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn change_map_runs_the_add_ons_the_host_started_with_and_keeps_their_state() -> Result<()> {
    let saves = tempfile::tempdir()?;
    let setup = Arc::new(HostSetup {
        lan: true,
        content: content(),
        maps: ["fixture", "other"]
            .map(|id| MapListing {
                id: id.into(),
                name: id.into(),
            })
            .into(),
        settings: None,
        passwords: None,
        add_ons: Some(add_ons(saves.path())),
        load_map: Some(Arc::new(|id: &str| Ok(map(id)))),
        copies: None,
        addon_data: None,
        game_version: None,
        bot_tuning: None,
        bot_overrides: None,
    });
    // Start Game's path: the first map's session from the same setup.
    let (game, spawn_points) = setup.session(&setup.hosted("fixture")?, map("fixture"))?;
    let mut opts = options();
    opts.spawn_points = spawn_points;
    opts.map_loader = Some(setup.clone());
    let server = server::start(game, opts)?;
    let mut admin = Client::connect_with_host(
        server.address,
        &server.certificate,
        "Admin".into(),
        Vec::new(),
        None,
        Some(server.host_token.clone()),
    )
    .await?;
    assert!(
        points(&mut admin).await.is_ok(),
        "the first map runs the Add-On"
    );
    admin
        .command(Command::Admin(Request::new(Action::ChangeMap {
            map: "other".into(),
        })))
        .await?;
    tokio::time::timeout(Duration::from_secs(10), async {
        while !admin
            .replica
            .chat
            .iter()
            .any(|l| l.text.contains("changed the map to"))
        {
            admin.receive().await?;
        }
        Result::<()>::Ok(())
    })
    .await??;
    // The new map's session runs it too: its command answers.
    let reply = points(&mut admin).await;
    assert!(reply.is_ok(), "the new map runs the Add-On: {reply:?}");
    server.stop().await?;
    // Each map's Add-On state is kept under its own key: the first map's
    // when the map changed, the second's when the host stopped.
    for key in ["fixture", "other"] {
        assert!(
            saves.path().join(format!("{key}.save.json")).is_file(),
            "{key}'s Add-On state was kept"
        );
    }
    Ok(())
}

#[test]
fn a_session_from_the_setup_has_everything_the_host_installs() -> Result<()> {
    let saves = tempfile::tempdir()?;
    let setup = HostSetup {
        lan: true,
        content: content(),
        maps: vec![MapListing {
            id: "fixture".into(),
            name: "Fixture".into(),
        }],
        settings: None,
        passwords: None,
        add_ons: Some(add_ons(saves.path())),
        load_map: None,
        copies: None,
        addon_data: None,
        game_version: None,
        bot_tuning: None,
        bot_overrides: None,
    };
    let (session, spawn_points) = setup.session(&setup.hosted("fixture")?, map("fixture"))?;
    assert_eq!(spawn_points, options().spawn_points);
    assert_eq!(session.spawn_points(), &spawn_points[..]);
    assert!(session.package_save().is_some(), "Add-On scripts installed");
    Ok(())
}

/// A game runs the bot kinds whose bodies it has: on a base map with the
/// Playground mode also on, the mode claims the Stomper and the Fizzer, so
/// their bodies are not in the game and their bots are left out instead of
/// refusing to host; a bot with the Blockhead's body always runs.
#[test]
fn a_game_runs_only_the_bots_whose_bodies_it_has() -> Result<()> {
    let packages = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages");
    let kinds = bri_net::content_identity::bot_kinds_from_files(&[
        (
            "blockhead_bot/assets/bots.json".into(),
            packages.join("blockhead_bot/assets/bots.json"),
        ),
        (
            "playground-stomper/bots.json".into(),
            packages.join("playground/playground-stomper/bots.json"),
        ),
        (
            "playground-fizzer/bots.json".into(),
            packages.join("playground/playground-fizzer/bots.json"),
        ),
    ])?;
    let set = PackageSet {
        schema_version: 1,
        packages: ["playground-stomper", "playground-stomper-look"]
            .into_iter()
            .map(|id| PackageEntry {
                id: id.into(),
                version: "1.0.0".into(),
                side: Side::Shared,
                dir: id.into(),
                role: None,
            })
            .collect(),
    };
    let catalog = Catalog::load(&packages.join("playground"), &set, true)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let session = bri_sim::session::Session::new(bri_sim::simulation::Simulation::new(
        World::new("Bots".into(), "fixture".into(), vec![[1.0; 4]]),
        Default::default(),
        Vec::new(),
    )?);
    let ids = |kinds: Vec<bri_sim::bot_kind::BotKind>| -> Vec<String> {
        kinds.into_iter().map(|k| k.id).collect()
    };
    assert_eq!(
        ids(bri_net::host_setup::playable_bot_kinds(
            &kinds,
            &session,
            Some(&catalog)
        )),
        ["bot.blockhead", "playground-stomper:bot/stomper"]
    );
    assert_eq!(
        ids(bri_net::host_setup::playable_bot_kinds(
            &kinds, &session, None
        )),
        ["bot.blockhead"]
    );
    Ok(())
}

/// What an Add-On's rules keep on the host (`set_host_data`) is there in
/// the next game after a restart: a roleplay server's bank, kept in the
/// host's own `addon-data` folder.
#[test]
fn what_rules_keep_on_the_host_outlives_a_restart() -> Result<()> {
    let root = tempfile::tempdir()?;
    let dir = root.path().join("bank");
    std::fs::create_dir_all(&dir)?;
    let write = |file: &str, value: serde_json::Value| {
        std::fs::write(dir.join(file), value.to_string()).unwrap()
    };
    write(
        "package.json",
        serde_json::json!({
            "schema_version": 1, "id": "bank", "version": "1.0.0", "api": 1,
            "name": "Bank", "license": "CC0-1.0", "capabilities": ["storage"],
            "provides": [
                { "kind": "behaviour", "id": "bank:behaviour/main", "file": "behaviour.json" },
                { "kind": "script", "id": "bank:script/main", "file": "main.rhai" }
            ]
        }),
    );
    write(
        "behaviour.json",
        serde_json::json!({
            "schema_version": 1, "script": "main.rhai",
            "commands": [{ "name": "deposit", "args": ["int"] }, { "name": "look" }],
            "state": { "global": { "seen": { "default": 0, "visible": "everyone" } } }
        }),
    );
    std::fs::write(
        dir.join("main.rhai"),
        r#"
fn cmd_deposit(p, n) {
    let vault = host_data("vault");
    set_host_data("vault", if vault == () { n } else { vault + n });
}
fn cmd_look(p) { set("seen", host_data("vault")); }
"#,
    )?;
    let set = PackageSet {
        schema_version: 1,
        packages: vec![PackageEntry {
            id: "bank".into(),
            version: "1.0.0".into(),
            side: Side::Server,
            dir: "bank".into(),
            role: None,
        }],
    };
    let state = tempfile::tempdir()?;
    let setup = || HostSetup {
        lan: true,
        content: content(),
        maps: vec![],
        settings: None,
        passwords: None,
        add_ons: Some(HostedAddOns {
            server: Arc::new(Catalog::load(root.path(), &set, true).unwrap()),
            mode: None,
            saves: None,
        }),
        load_map: None,
        copies: None,
        addon_data: Some(Arc::new(bri_net::addon_data::AddOnDataFiles::new(
            state.path().join("addon-data"),
        ))),
        game_version: None,
        bot_tuning: None,
        bot_overrides: None,
    };
    let send = |session: &mut bri_sim::session::Session, owner, seq, command: &str, args| {
        session
            .command(
                owner,
                seq,
                Command::Package(PackageCommand {
                    package: "bank".into(),
                    command: command.into(),
                    args,
                }),
            )
            .map(drop)
    };
    {
        let host = setup();
        let (mut session, spawns) = host.session(&host.hosted("fixture")?, map("fixture"))?;
        let p = session.join("P".into(), spawns[0], true)?;
        send(
            &mut session,
            p,
            1,
            "deposit",
            vec![bri_sim::session::PackageArg::Int(40)],
        )?;
        send(
            &mut session,
            p,
            2,
            "deposit",
            vec![bri_sim::session::PackageArg::Int(2)],
        )?;
    }
    // The host is gone; a new one starts on the same state folder.
    let host = setup();
    let (mut session, spawns) = host.session(&host.hosted("fixture")?, map("fixture"))?;
    let p = session.join("P".into(), spawns[0], true)?;
    send(&mut session, p, 1, "look", vec![])?;
    assert_eq!(
        session.package_state().packages["bank"].global["seen"],
        serde_json::json!(42)
    );
    assert!(state.path().join("addon-data/bank.json").is_file());
    Ok(())
}

/// `/botreload` while a game runs reads every enabled Add-On's bots again,
/// and keeps only those the game can run, as hosting did: a bot whose body
/// is in an Add-On this game leaves out (a mode claims it) is left out
/// again rather than failing the reload.
#[test]
fn bot_reload_keeps_only_the_bots_the_game_can_run() -> Result<()> {
    let packages = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages");
    let files = [
        (
            "blockhead_bot/assets/bots.json".to_string(),
            packages.join("blockhead_bot/assets/bots.json"),
        ),
        (
            "playground-fizzer/bots.json".to_string(),
            packages.join("playground/playground-fizzer/bots.json"),
        ),
    ];
    let mut content = content();
    content.bot_kinds = bri_net::content_identity::bot_kinds_from_files(&files)?;
    let setup = HostSetup {
        lan: true,
        content,
        maps: Vec::new(),
        settings: None,
        passwords: None,
        add_ons: None,
        load_map: None,
        copies: None,
        addon_data: None,
        game_version: None,
        bot_tuning: Some(bri_sim::session::BotTuning {
            reload: Some(Arc::new(move || {
                bri_net::content_identity::bot_kinds_from_files(&files)
            })),
            overrides: None,
        }),
        bot_overrides: None,
    };
    let (mut session, spawn_points) = setup.session(&setup.hosted("fixture")?, map("fixture"))?;
    let admin = session.join("Admin".into(), spawn_points[0], true)?;
    session.take_private_notices();
    let reply = session.command(
        admin,
        1,
        Command::Package(PackageCommand {
            package: String::new(),
            command: "botreload".into(),
            args: vec![],
        }),
    )?;
    assert!(matches!(reply, Reply::Accepted), "{reply:?}");
    let said: Vec<String> = session
        .take_private_notices()
        .into_iter()
        .filter(|(to, _)| *to == admin)
        .map(|(_, notice)| format!("{notice:?}"))
        .collect();
    assert!(
        said.iter()
            .any(|l| l.contains("Bot settings reloaded: 1 kinds")),
        "{said:?}"
    );
    Ok(())
}
