//! A round-held corpse must not poll script policy; releasing the hold
//! still permits its ordinary automatic respawn.
use super::*;
use bri_package::packages::{PackageEntry, PackageSet, Side};
use serde_json::json;

#[test]
fn a_held_dead_bot_waits_without_polling_policy_then_respawns_when_released() {
    let root = tempfile::tempdir().unwrap();
    let addon = root.path().join("respawn-check");
    std::fs::create_dir(&addon).unwrap();
    for (file, value) in [
        (
            "package.json",
            json!({"schema_version":1,"id":"respawn-check","version":"1.0.0","api":1,
            "name":"Invented respawn observer","license":"CC0-1.0","provides":[
                {"kind":"behaviour","id":"respawn-check:behaviour/main","file":"behaviour.json"},
                {"kind":"script","id":"respawn-check:script/main","file":"main.rhai"}]}),
        ),
        (
            "behaviour.json",
            json!({"schema_version":1,"script":"main.rhai","policies":["respawn"],
                "state":{"global":{"requests":{"default":0,"visible":"everyone"}}}}),
        ),
    ] {
        std::fs::write(addon.join(file), value.to_string()).unwrap();
    }
    std::fs::write(
        addon.join("main.rhai"),
        "fn allow_respawn(p) { set(\"requests\", get(\"requests\") + 1); true }\n",
    )
    .unwrap();
    let catalog = bri_package_runtime::Catalog::load(
        root.path(),
        &PackageSet {
            schema_version: 1,
            packages: vec![PackageEntry {
                id: "respawn-check".into(),
                version: "1.0.0".into(),
                side: Side::Server,
                dir: "respawn-check".into(),
                role: None,
            }],
        },
        true,
    )
    .unwrap();
    let sim = crate::simulation::Simulation::new(
        bri_world::World::new("Respawn hold".into(), "fixture".into(), vec![[1.; 4]]),
        crate::testing::definitions(),
        vec![
            rapier3d::prelude::ColliderBuilder::cuboid(20., 0.5, 20.)
                .translation(Vec3::new(0., -0.5, 0.)),
        ],
    )
    .unwrap();
    let mut s = Session::new(sim);
    s.install_packages(Arc::new(catalog), None).unwrap();
    let at = Vec3::new(0., 0.05, 0.);
    s.set_spawn_points(vec![at]).unwrap();
    let bot = s.join("Waiting bot".into(), at, false).unwrap();
    s.command(
        bot,
        1,
        Command::MiniGame(super::super::MiniGameRequest::Create {
            color: 0,
            settings: bri_minigames::Settings::default(),
        }),
    )
    .unwrap();
    s.bots
        .brains
        .insert(bot, Brain::new(None, BotKind::default(), at, bot, 0));
    let player = s.peers[&bot].combat.player;
    s.minigames.hold_respawn(player, true).unwrap();
    s.suicide(bot).unwrap();
    let ready = s.peers[&bot].combat.respawn_tick;
    while s.simulation.state().tick <= ready + 300 {
        s.step().unwrap();
        assert!(!s.is_alive(bot), "the round still holds its next life");
    }
    let requests = |s: &Session| {
        s.package_state().packages["respawn-check"].global["requests"]
            .as_i64()
            .unwrap()
    };
    assert_eq!(
        requests(&s),
        0,
        "a held corpse must not poll game policy every tick"
    );
    s.minigames.hold_respawn(player, false).unwrap();
    s.step().unwrap();
    assert!(s.is_alive(bot), "lifting the hold permits normal respawn");
    assert_eq!(
        requests(&s),
        1,
        "an eligible respawn still asks game policy"
    );
    assert_eq!(s.bot_step_failures().0, 0);
}
