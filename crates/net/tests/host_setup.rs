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
        game_version: None,
        bot_tuning: None,
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
        game_version: None,
        bot_tuning: None,
    };
    let (session, spawn_points) = setup.session(&setup.hosted("fixture")?, map("fixture"))?;
    assert_eq!(spawn_points, options().spawn_points);
    assert_eq!(session.spawn_points(), &spawn_points[..]);
    assert!(session.package_save().is_some(), "Add-On scripts installed");
    Ok(())
}
