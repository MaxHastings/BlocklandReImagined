//! v0.2.9 integration: item lifecycle across lanes. #40 (a pickup rule may
//! remove the spawner the pickup loop holds) meets #42 (restore modes, the
//! respawn timer reads `available_again`). Originally: an Add-On hook runs in the middle of the session's own loops and its
//! operations apply at once, so a hook may remove what the loop holds the
//! id of: the brick it was asked about, a player later in the list. The
//! loop must find each again after the hook, never trust the id it held
//! (the class of the v0.2.8 soccer crash, where a queued push outlived
//! its body).
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::{Definition, Definitions},
    session::Session,
    simulation::Simulation,
};
use bri_world::{Brick, ContentRef, World};
use glam::Vec3;
use rapier3d::prelude::*;
use serde_json::json;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::Arc,
    sync::atomic::{AtomicUsize, Ordering},
};

/// Picking up a kit from its spawner uses the spawner up: the rule removes
/// the brick, then answers "take".
const REMOVE: &str = r#"
fn on_pickup(p, item, info) {
    set("touches", get("touches") + 1);
    if info.spawner != () { remove_brick(info.spawner); }
    "take"
}
"#;

fn definitions() -> Definitions {
    let mesh = Mesh {
        schema_version: 1,
        id: "plate".into(),
        footprint_studs: [2, 2],
        height_plates: 1,
        attachment_rows: vec!["bb".into(), "bb".into()],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![],
    };
    let collision = CollisionBody {
        id: "plate".into(),
        parts: vec![Part::Box {
            center: [0.; 3],
            size: [1., 0.2, 1.],
        }],
    };
    let shape = bri_physics::content::collider(&collision)
        .unwrap()
        .build()
        .shared_shape()
        .clone();
    Definitions {
        entries: BTreeMap::from([(
            "plate".into(),
            Definition {
                mesh,
                collision,
                shape,
                indestructible: false,
                special: Default::default(),
                reflection: None,
                link: None,
                glass: [0.0; 4],
                bot: None,
            },
        )]),
    }
}

fn weapons() -> bri_weapons::Pack {
    let pack = json!({
        "schema_version": 5,
        "id": "kit",
        "items": { "kit:weapon/kit": { "ui_name": "Kit" } }
    });
    bri_weapons::Pack::from_json(&serde_json::to_vec(&pack).unwrap()).unwrap()
}

struct Root(PathBuf);
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn catalog(script: &str) -> Arc<Catalog> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = Root(std::env::temp_dir().join(format!(
        "bri-hook-lifecycle-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    let dir = root.0.join("kit");
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = json!({
        "schema_version": 1, "id": "kit", "version": "1.0.0", "api": 1,
        "name": "kit", "license": "CC0-1.0",
        "capabilities": ["world.edit"],
        "provides": [
            { "kind": "behaviour", "id": "kit:behaviour/main", "file": "behaviour.json" },
            { "kind": "script", "id": "kit:script/main", "file": "main.rhai" }
        ]
    });
    let behaviour = json!({
        "schema_version": 1,
        "script": "main.rhai",
        "on_pickup": true,
        "state": { "global": {
            "touches": { "default": 0, "visible": "everyone" }
        } }
    });
    std::fs::write(dir.join("package.json"), manifest.to_string()).unwrap();
    std::fs::write(dir.join("behaviour.json"), behaviour.to_string()).unwrap();
    std::fs::write(dir.join("main.rhai"), script).unwrap();
    let set = PackageSet {
        schema_version: 1,
        packages: vec![PackageEntry {
            id: "kit".into(),
            version: "1.0.0".into(),
            side: Side::Server,
            dir: "kit".into(),
            role: None,
        }],
    };
    Arc::new(Catalog::load(&root.0, &set, true).unwrap_or_else(|e| panic!("{e:#?}")))
}

const LEAVE: &str = r#"
fn on_pickup(p, item, info) { set("touches", get("touches") + 1); false }
"#;
const TAKE: &str = r#"
fn on_pickup(p, item, info) { set("touches", get("touches") + 1); "take" }
"#;

fn session(script: &str, restore: bri_world::ItemRestore, extra_far: bool) -> Session {
    let mut world = World::new("Kit".into(), "kit".into(), vec![[1.; 4]]);
    let mut spawner = Brick::new(ContentRef::Resolved("plate".into()), [0., 0.1, 0.], 0);
    spawner.item_spawn.item = Some(ContentRef::Resolved("kit:weapon/kit".into()));
    spawner.item_spawn.respawn_ms = 1000;
    spawner.item_spawn.restore = restore;
    world.bricks.insert(1, spawner);
    if extra_far {
        let mut far = Brick::new(ContentRef::Resolved("plate".into()), [20., 0.1, 0.], 0);
        far.item_spawn.item = Some(ContentRef::Resolved("kit:weapon/kit".into()));
        far.item_spawn.restore = restore;
        world.bricks.insert(2, far);
    }
    world.next_brick_id = 3;
    let ground = ColliderBuilder::cuboid(100., 0.5, 100.).translation(Vector::new(0., -0.5, 0.));
    let mut s = Session::new(Simulation::new(world, definitions(), vec![ground]).unwrap());
    s.set_weapon_pack(weapons()).unwrap();
    s.set_item_bounds(BTreeMap::new()).unwrap();
    s.install_packages(catalog(script), None).unwrap();
    s
}
fn spawner_available_at(s: &Session, brick: u64) -> Option<u64> {
    s.weapon_view()
        .static_items
        .iter()
        .find(|i| i.brick == brick)
        .map(|i| i.available_at)
}

/// A rule removes its spawner on pickup, in every restore mode: the
/// respawn bookkeeping must not look the gone brick up, and the other
/// spawner keeps working (Reset mode stays gone, Timer mode returns).
#[test]
fn rule_removal_of_the_spawner_is_safe_in_every_restore_mode() {
    for restore in [bri_world::ItemRestore::Timer, bri_world::ItemRestore::Reset] {
        let mut s = session(REMOVE, restore, true);
        s.join("A".into(), Vec3::new(0., 0.35, 0.), true).unwrap();
        for _ in 0..60 {
            s.step().unwrap();
        }
        assert!(
            !s.simulation().state().bricks.contains_key(&1),
            "{restore:?}"
        );
        assert_eq!(
            spawner_available_at(&s, 1),
            None,
            "{restore:?}: offers nothing"
        );
        assert!(
            spawner_available_at(&s, 2).is_some(),
            "{restore:?}: other stays"
        );
        assert_eq!(
            s.package_state().packages["kit"].global["touches"],
            json!(1),
            "{restore:?}: asked once"
        );
    }
}

/// A rule answering `false` takes nothing, so neither mode starts a
/// respawn; "take" in Reset mode keeps the item gone (NEVER) while Timer
/// mode schedules its return.
#[test]
fn restore_mode_follows_what_the_rule_decided() {
    let mut s = session(LEAVE, bri_world::ItemRestore::Reset, false);
    s.join("A".into(), Vec3::new(0., 0.35, 0.), true).unwrap();
    for _ in 0..10 {
        s.step().unwrap();
    }
    let at = spawner_available_at(&s, 1).unwrap();
    assert!(at < bri_sim::item_spawners::NEVER, "false: still on offer");
    assert!(
        s.package_state().packages["kit"].global["touches"]
            .as_i64()
            .unwrap()
            >= 1
    );

    let mut s = session(TAKE, bri_world::ItemRestore::Reset, false);
    s.join("A".into(), Vec3::new(0., 0.35, 0.), true).unwrap();
    for _ in 0..10 {
        s.step().unwrap();
    }
    assert_eq!(
        spawner_available_at(&s, 1),
        Some(bri_sim::item_spawners::NEVER)
    );

    let mut s = session(TAKE, bri_world::ItemRestore::Timer, false);
    s.join("A".into(), Vec3::new(0., 0.35, 0.), true).unwrap();
    for _ in 0..10 {
        s.step().unwrap();
    }
    assert!(spawner_available_at(&s, 1).unwrap() < bri_sim::item_spawners::NEVER);
}

const MATH_AND_REQUIRE: &str = r#"
fn on_pickup(p, item, info) {
    // #41's script math next to #42's require(handle).
    let n = clamp(floor(2.7), 0, 5);
    set("touches", get("touches") + n);
    let h = remove_brick(info.spawner);
    require(h);
    "take"
}
"#;

/// Script math (#41) and `require` on an operation handle (#42) in the one
/// pickup rule that also removes its spawner (#40): the required removal
/// applies, the math result is stored, and the session plays on.
#[test]
fn math_and_required_operations_work_in_a_pickup_rule() {
    let mut s = session(MATH_AND_REQUIRE, bri_world::ItemRestore::Reset, true);
    s.join("A".into(), Vec3::new(0., 0.35, 0.), true).unwrap();
    for _ in 0..30 {
        s.step().unwrap();
    }
    assert_eq!(
        s.package_state().packages["kit"].global["touches"].as_f64(),
        Some(2.0)
    );
    assert!(!s.simulation().state().bricks.contains_key(&1));
    assert!(s.simulation().state().bricks.contains_key(&2));
}
