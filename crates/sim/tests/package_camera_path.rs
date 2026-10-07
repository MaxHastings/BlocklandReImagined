//! Rules' camera paths (`follow_path`, Torque's `PathCamera`): the camera
//! flies the knots while the body stands still, the rules hear each knot
//! (`on_path_node`), the path replicates in vitals, and control comes back
//! when the rules let go. Also the small world seams rules use beside it:
//! `set_brick_color`, `set_brick_item`, `palette()` and `set_zone_period`.
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::Definitions,
    player::MoveInput,
    session::{Command, ControlObject, ObserverButton, PackageArg, PackageCommand, Session},
    simulation::Simulation,
};
use bri_world::{OwnerId, World};
use glam::Vec3;
use rapier3d::prelude::*;
use serde_json::json;
use std::{path::PathBuf, sync::Arc};

const SCRIPT: &str = r#"
fn cmd_fly(p) {
    let me = player(p);
    let from = me.camera;
    follow_path(p, [
        #{ at: from.at, yaw: from.yaw, pitch: from.pitch, speed: 10, path: "linear" },
        #{ at: [from.at[0] + 10.0, from.at[1], from.at[2]], yaw: 1.0, speed: 10, path: "linear" },
        #{ at: [from.at[0] + 10.0, from.at[1] + 10.0, from.at[2]], speed: 10, jump: true }
    ]);
}
fn cmd_land(p) { follow_path(p, ()); }
fn cmd_bad(p) { follow_path(p, [#{ at: [0, 0, 0], speed: 0 }]); }
fn on_path_node(p, knot) {
    let heard = get("heard");
    heard.push(knot);
    set("heard", heard);
    if knot == 2 {
        let me = player(p);
        set("camera", me.camera.at);
    }
}
fn cmd_slow(p) { set_zone_period(0, 500); }
fn cmd_free(p) { free_camera(p); }
fn cmd_orbit(p) { orbit_point(p, [3.0, 1.0, 4.0], 6.0); }
fn cmd_far(p) { orbit_point(p, [3.0, 1.0, 4.0], 500.0); }
fn on_observer(p, button) {
    let keys = get("keys");
    keys.push(button);
    set("keys", keys);
    true
}
fn cmd_colours(p) { set("palette", palette().len()); }
fn cmd_stock(p) { set_brick_item(1, "v20.weapon.gunitem"); }
fn cmd_foreign(p) { set_brick_item(1, "stranger:weapon/flag"); }
"#;

struct Root(PathBuf);
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn add_on(test: &str) -> (Root, Arc<Catalog>) {
    let root =
        Root(std::env::temp_dir().join(format!("bri-camera-path-{}-{test}", std::process::id())));
    let dir = root.0.join("probe");
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = json!({
        "schema_version": 1, "id": "probe", "version": "1.0.0", "api": 1,
        "name": "probe", "license": "CC0-1.0",
        "capabilities": ["player", "minigame", "world.edit"],
        "provides": [
            { "kind": "behaviour", "id": "probe:behaviour/main", "file": "behaviour.json" },
            { "kind": "script", "id": "probe:script/main", "file": "main.rhai" }
        ]
    });
    let behaviour = json!({
        "schema_version": 1,
        "script": "main.rhai",
        "commands": [
            { "name": "fly" }, { "name": "land" }, { "name": "bad" },
            { "name": "slow" }, { "name": "colours" },
            { "name": "free" }, { "name": "orbit" }, { "name": "far" },
            { "name": "stock" }, { "name": "foreign" }
        ],
        "state": { "global": {
            "heard": { "default": [], "visible": "everyone", "persist": false },
            "camera": { "default": null, "visible": "everyone", "persist": false },
            "palette": { "default": 0, "visible": "everyone", "persist": false },
            "keys": { "default": [], "visible": "everyone", "persist": false }
        } },
        "zones": [ { "bricks": ["v20/brick/brick2x2data"] } ],
        "on_path_node": true,
        "on_observer": true
    });
    std::fs::write(dir.join("package.json"), manifest.to_string()).unwrap();
    std::fs::write(dir.join("behaviour.json"), behaviour.to_string()).unwrap();
    std::fs::write(
        dir.join("main.rhai"),
        SCRIPT.to_string() + "fn on_zone(p, b, e) {}\n",
    )
    .unwrap();
    let set = PackageSet {
        schema_version: 1,
        packages: vec![PackageEntry {
            id: "probe".into(),
            version: "1.0.0".into(),
            side: Side::Server,
            dir: "probe".into(),
            role: None,
        }],
    };
    let catalog = Catalog::load(&root.0, &set, true).unwrap_or_else(|e| panic!("{e:#?}"));
    (root, Arc::new(catalog))
}

fn session() -> Session {
    let mut s = Session::new(
        Simulation::new(
            World::new(
                "Paths".into(),
                "paths".into(),
                vec![[1.0; 4], [0.0; 4], [0.5; 4]],
            ),
            Definitions::default(),
            vec![
                ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0)),
            ],
        )
        .unwrap(),
    );
    s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 0.0)]).unwrap();
    s
}

fn steps(s: &mut Session, players: &[OwnerId], n: u64) {
    for _ in 0..n {
        for &p in players {
            let seq = 1_000_000 + s.simulation().state().tick;
            s.movement(p, seq, MoveInput::default()).unwrap();
        }
        s.step().unwrap();
    }
}

fn run(s: &mut Session, owner: OwnerId, seq: u64, command: &str) -> anyhow::Result<()> {
    s.command(
        owner,
        seq,
        Command::Package(PackageCommand {
            package: "probe".into(),
            command: command.into(),
            args: Vec::<PackageArg>::new(),
        }),
    )
    .map(drop)
}

fn global(s: &Session, key: &str) -> serde_json::Value {
    s.package_state().packages["probe"].global[key].clone()
}

#[test]
fn a_rules_camera_path_flies_its_knots_and_hands_control_back() {
    let (_root, add_on) = add_on("fly");
    let mut s = session();
    s.install_packages(add_on, None).unwrap();
    let p = s
        .join("Flyer".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    steps(&mut s, &[p], 10);
    let feet = s
        .motion_states()
        .into_iter()
        .find(|(m, _)| m.owner == p)
        .unwrap()
        .0
        .feet;

    run(&mut s, p, 1, "fly").unwrap();
    steps(&mut s, &[p], 1);
    assert_eq!(s.control(p), Some(ControlObject::Path));
    let path = s.vitals()[&p]
        .camera_path
        .clone()
        .expect("the path replicates");
    assert_eq!(path.knots.len(), 3);
    // Ten units at ten a second, then a cut to the last knot.
    assert_eq!(path.duration_ticks(), 120);
    assert_eq!(global(&s, "heard"), json!([0]));

    steps(&mut s, &[p], 130);
    assert_eq!(global(&s, "heard"), json!([0, 1, 2]));
    let camera = global(&s, "camera");
    let last = path.knots[2].view.eye;
    assert!(
        (camera[1].as_f64().unwrap() - f64::from(last[1])).abs() < 1e-3,
        "the camera reads where the path is: {camera}"
    );
    // The body stood still the whole way.
    let now = s
        .motion_states()
        .into_iter()
        .find(|(m, _)| m.owner == p)
        .unwrap()
        .0
        .feet;
    assert!(Vec3::from(now).distance(Vec3::from(feet)) < 0.01);

    run(&mut s, p, 2, "land").unwrap();
    steps(&mut s, &[p], 1);
    assert_eq!(s.control(p), Some(ControlObject::Player));
    assert!(s.vitals()[&p].camera_path.is_none());

    // A knot without a speed is refused as the rules' fault.
    let refused = run(&mut s, p, 3, "bad").unwrap_err();
    assert!(refused.to_string().contains("FollowPath"), "{refused}");
    steps(&mut s, &[p], 1);
    assert_eq!(s.control(p), Some(ControlObject::Player));
}

#[test]
fn rules_read_the_palette_and_slow_their_zones() {
    let (_root, add_on) = add_on("palette");
    let mut s = session();
    s.install_packages(add_on, None).unwrap();
    let p = s
        .join("Rules".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    steps(&mut s, &[p], 2);
    run(&mut s, p, 1, "colours").unwrap();
    run(&mut s, p, 2, "slow").unwrap();
    steps(&mut s, &[p], 2);
    assert_eq!(global(&s, "palette"), json!(3));
    assert!(
        s.package_diagnostics().is_empty(),
        "{:?}",
        s.package_diagnostics()
    );
}

/// Rules may stock a brick with a base-game item, but not with another
/// package's they do not depend on.
#[test]
fn rules_may_hand_out_base_game_items_but_not_a_strangers() {
    let (_root, add_on) = add_on("stock");
    let mut s = session();
    s.install_packages(add_on, None).unwrap();
    let p = s
        .join("Rules".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    steps(&mut s, &[p], 2);
    let refused = |s: &Session| {
        s.package_diagnostics()
            .iter()
            .any(|d| format!("{d:?}").contains("is not an item of"))
    };
    run(&mut s, p, 1, "stock").unwrap();
    steps(&mut s, &[p], 1);
    assert!(!refused(&s), "{:?}", s.package_diagnostics());
    run(&mut s, p, 2, "foreign").unwrap();
    steps(&mut s, &[p], 1);
    assert!(refused(&s), "{:?}", s.package_diagnostics());
}

#[test]
fn rules_give_spectators_a_free_camera_or_an_orbit_and_hear_their_keys() {
    let (_root, add_on) = add_on("spectate");
    let mut s = session();
    s.install_packages(add_on, None).unwrap();
    let p = s
        .join("Watcher".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    steps(&mut s, &[p], 2);
    // On their body, a player's keys are their own.
    assert!(
        s.command(p, 1, Command::ObserverButton(ObserverButton::Fire))
            .is_err()
    );

    run(&mut s, p, 2, "orbit").unwrap();
    steps(&mut s, &[p], 1);
    assert_eq!(s.control(p), Some(ControlObject::Point));
    let point = s.vitals()[&p].camera_point.expect("the point replicates");
    assert_eq!((point.at, point.distance), ([3.0, 1.0, 4.0], 6.0));
    // The rules' camera is theirs to hand back, not the player's.
    assert!(s.command(p, 3, Command::ControlPlayer).is_err());
    s.command(p, 4, Command::ObserverButton(ObserverButton::Jet))
        .unwrap();
    s.command(p, 5, Command::ObserverButton(ObserverButton::Light))
        .unwrap();
    assert_eq!(global(&s, "keys"), json!(["jet", "light"]));

    run(&mut s, p, 6, "free").unwrap();
    steps(&mut s, &[p], 1);
    assert_eq!(s.control(p), Some(ControlObject::Observer));
    assert!(s.vitals()[&p].camera_point.is_none());
    // Unlike an admin's camera, it shows no orb.
    assert!(s.camera_orbs().is_empty());
    // An orbit too far out is refused.
    assert!(run(&mut s, p, 7, "far").is_err());
    run(&mut s, p, 8, "land").unwrap();
    steps(&mut s, &[p], 1);
    assert_eq!(
        s.control(p),
        Some(ControlObject::Observer),
        "follow_path(()) leaves other cameras"
    );
}
