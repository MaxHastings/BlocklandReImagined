//! An Add-On hook runs in the middle of the session's own loops and its
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
const SCRIPT: &str = r#"
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
fn catalog() -> Arc<Catalog> {
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
    std::fs::write(dir.join("main.rhai"), SCRIPT).unwrap();
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

#[test]
fn a_pickup_rule_that_removes_its_own_spawner_takes_the_item_and_the_brick_goes() {
    // Before: the pickup loop looked the spawner brick up again by the id
    // it held, after the rule had removed it, and the host panicked.
    let mut world = World::new("Kit".into(), "kit".into(), vec![[1.; 4]]);
    // World-owned, so a rule may remove it.
    let mut spawner = Brick::new(ContentRef::Resolved("plate".into()), [0., 0.1, 0.], 0);
    spawner.item_spawn.item = Some(ContentRef::Resolved("kit:weapon/kit".into()));
    spawner.item_spawn.respawn_ms = 1000;
    world.bricks.insert(1, spawner);
    // A second spawner far off, which nobody touches.
    let mut far = Brick::new(ContentRef::Resolved("plate".into()), [20., 0.1, 0.], 0);
    far.item_spawn.item = Some(ContentRef::Resolved("kit:weapon/kit".into()));
    world.bricks.insert(2, far);
    world.next_brick_id = 3;
    let ground = ColliderBuilder::cuboid(100., 0.5, 100.).translation(Vector::new(0., -0.5, 0.));
    let mut s = Session::new(Simulation::new(world, definitions(), vec![ground]).unwrap());
    s.set_weapon_pack(weapons()).unwrap();
    s.set_item_bounds(BTreeMap::new()).unwrap();
    s.install_packages(catalog(), None).unwrap();
    let player = s.join("A".into(), Vec3::new(0., 0.35, 0.), true).unwrap();
    for _ in 0..3 {
        s.step().unwrap();
    }
    let touches = s.package_state().packages["kit"].global["touches"].clone();
    assert_eq!(touches, json!(1), "asked once: then it was gone");
    let bricks = &s.simulation().state().bricks;
    assert!(!bricks.contains_key(&1), "the rule removed the spawner");
    assert!(bricks.contains_key(&2), "the other spawner stays");
    let statics: Vec<_> = s
        .weapon_view()
        .static_items
        .iter()
        .map(|i| i.brick)
        .collect();
    assert_eq!(statics, vec![2], "only the spawner left offers an item");
    assert!(
        !s.tool_inventories()[&player]
            .slots
            .iter()
            .any(|t| t.as_deref() == Some("kit:weapon/kit")),
        "\"take\" uses the kit up rather than giving it"
    );
    // The session plays on.
    for _ in 0..30 {
        s.step().unwrap();
    }
}
