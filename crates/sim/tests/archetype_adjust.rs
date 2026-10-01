//! An Add-On adjusting one of v20's player types while it is on
//! (`ArchetypeDef::adjusts`), as a v20 Add-On's
//! `PlayerNoJet.maxStepHeight = 1.2;` did: players of that type move by the
//! new constants, everyone else as v20, and with no Add-On adjusting it the
//! type is v20's. The Add-On is written by the test (CC0).
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::Definitions,
    player::MoveInput,
    session::{Command, MiniGameRequest, Session},
    simulation::Simulation,
};
use bri_world::{OwnerId, World};
use glam::Vec3;
use rapier3d::prelude::*;
use std::{collections::BTreeMap, sync::Arc};

const NO_JET: &str = "v20.player.playernojet";

/// An Add-On whose only content raises No Jet's step to 1.2.
fn catalog() -> Arc<Catalog> {
    let root = std::env::temp_dir().join(format!("bri-archetype-adjust-{}", std::process::id()));
    let dir = root.join("high-steps");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("package.json"),
        r#"{ "schema_version": 1, "id": "high-steps", "version": "1.0.0", "api": 1,
             "name": "High steps", "license": "CC0-1.0", "provenance": { "source": "original" },
             "dependencies": {}, "capabilities": [],
             "provides": [ { "kind": "archetype", "id": "high-steps:archetype/nojet", "file": "nojet.json" } ] }"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("nojet.json"),
        r#"{ "schema_version": 1, "adjusts": "v20.player.playernojet", "movement": { "step_height": 1.2 } }"#,
    )
    .unwrap();
    let set = PackageSet {
        schema_version: 1,
        packages: vec![PackageEntry {
            id: "high-steps".into(),
            version: "1.0.0".into(),
            side: Side::Server,
            dir: "high-steps".into(),
            role: None,
        }],
    };
    Arc::new(Catalog::load(&root, &set, true).unwrap_or_else(|e| panic!("{e:#?}")))
}

/// Flat ground with a 1.1 tall ledge across the way ahead (-Z), and two
/// players walking at it: one in a No Jet mini-game, one not.
fn walk(with_addon: bool) -> (f32, f32) {
    let world = World::new("Steps".into(), "steps".into(), vec![[1.0; 4]]);
    let mut s = Session::new(
        Simulation::new(
            world,
            Definitions::default(),
            vec![
                ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0)),
                ColliderBuilder::cuboid(100.0, 0.55, 5.0).translation(Vector::new(0.0, 0.55, -8.0)),
            ],
        )
        .unwrap(),
    );
    if with_addon {
        s.install_packages(catalog(), None).unwrap();
    }
    s.set_spawn_points(vec![Vec3::new(-3.0, 0.05, 0.0)]).unwrap();
    let host = s.join("Host".into(), Vec3::new(-3.0, 0.05, 0.0), true).unwrap();
    let walker = s.join("Walker".into(), Vec3::new(3.0, 0.05, 0.0), false).unwrap();
    let mut seq = BTreeMap::<OwnerId, u64>::new();
    let mut cmd = |s: &mut Session, owner: OwnerId, command: Command| {
        let n = seq.entry(owner).or_default();
        *n += 1;
        s.command(owner, *n, command).unwrap();
    };
    let settings = bri_minigames::Settings {
        player_type: NO_JET.into(),
        ..Default::default()
    };
    cmd(&mut s, host, Command::MiniGame(MiniGameRequest::Create { color: 1, settings }));
    for _ in 0..60 {
        s.step().unwrap();
    }
    assert_eq!(s.vitals()[&host].minigame, Some(s.minigame_views()[0].id));
    assert_eq!(s.vitals()[&walker].minigame, None);
    let mut moves = 0;
    for _ in 0..150 {
        moves += 1;
        for owner in [host, walker] {
            let _ = s.movement(
                owner,
                moves,
                MoveInput {
                    forward: 1.0,
                    ..Default::default()
                },
            );
        }
        s.step().unwrap();
    }
    let feet = |owner| {
        s.snapshot()
            .players
            .iter()
            .find(|p| p.owner == owner)
            .unwrap()
            .feet[1]
    };
    (feet(host), feet(walker))
}

#[test]
fn an_addon_raises_no_jets_step_and_no_one_elses() {
    // v20: a 1.1 ledge (above its 1.0 step) stops everyone, No Jet or not.
    let (no_jet, standard) = walk(false);
    assert!(no_jet < 0.1, "{no_jet}");
    assert!(standard < 0.1, "{standard}");
    // With the Add-On, No Jet players step up onto it; others still stop.
    let (no_jet, standard) = walk(true);
    assert!((no_jet - 1.1).abs() < 0.05, "{no_jet}");
    assert!(standard < 0.1, "{standard}");
}

#[test]
fn an_adjustment_names_a_v20_player_type_and_nothing_else() {
    use bri_package_runtime::content::ArchetypeDef;
    let def = |json: &str| serde_json::from_str::<ArchetypeDef>(json).unwrap().validate();
    assert!(def(r#"{ "schema_version": 1, "adjusts": "v20.player.playernojet" }"#).is_ok());
    assert!(def(r#"{ "schema_version": 1, "adjusts": "mod:archetype/x" }"#).is_err());
    assert!(
        def(r#"{ "schema_version": 1, "adjusts": "v20.player.playernojet", "base": "v20.player.playerstandardarmor" }"#)
            .is_err()
    );
    assert!(def(r#"{ "schema_version": 1, "adjusts": "v20.player.playernojet", "name": "Mine" }"#).is_err());
}
