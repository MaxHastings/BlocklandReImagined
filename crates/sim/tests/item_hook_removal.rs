//! Hooks that run inside the pickup loop edit the world as they run. The
//! loop re-finds every spawner after each hook (`on_pickup` before the take,
//! `onItemPickup` and `on_collected` after it), so a spawner a hook removed
//! hands its item to nobody later in the same tick, and the respawn
//! bookkeeping never looks a gone brick up.
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
        "bri-item-hook-removal-{}-{}",
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
        "on_collected": true,
        "state": { "global": {
            "touches": { "default": 0, "visible": "everyone" },
            "collected": { "default": 0, "visible": "everyone" }
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

/// Taking the kit from the near spawner uses the far one up as well: the
/// result hook removes it while another player stands on it.
const COLLECTED_REMOVES_FAR: &str = r#"
fn on_pickup(p, item, info) { set("touches", get("touches") + 1); "take" }
fn on_collected(p, item, info) {
    set("collected", get("collected") + 1);
    if info.spawner != () { remove_brick(2); }
}
"#;

/// The near spawner's rule removes the brick, then answers "take".
const PICKUP_REMOVES_SELF: &str = r#"
fn on_pickup(p, item, info) {
    set("touches", get("touches") + 1);
    if info.spawner != () { remove_brick(info.spawner); }
    "take"
}
fn on_collected(p, item, info) { set("collected", get("collected") + 1); }
"#;

fn session(script: &str, restore: bri_world::ItemRestore) -> Session {
    let mut world = World::new("Kit".into(), "kit".into(), vec![[1.; 4]]);
    let mut near = Brick::new(ContentRef::Resolved("plate".into()), [0., 0.1, 0.], 0);
    near.item_spawn.item = Some(ContentRef::Resolved("kit:weapon/kit".into()));
    near.item_spawn.respawn_ms = 1000;
    near.item_spawn.restore = restore;
    world.bricks.insert(1, near);
    let mut far = Brick::new(ContentRef::Resolved("plate".into()), [20., 0.1, 0.], 0);
    far.item_spawn.item = Some(ContentRef::Resolved("kit:weapon/kit".into()));
    far.item_spawn.restore = restore;
    world.bricks.insert(2, far);
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

/// `on_collected` on the near brick's kit removes the far spawner while a
/// second player stands on it, in every restore mode: the far spawner's
/// rule never runs for that player (one touch, one collection), it offers
/// nothing, and nothing panics.
#[test]
fn a_spawner_removed_by_on_collected_gives_nothing_to_a_later_contact() {
    for restore in [bri_world::ItemRestore::Timer, bri_world::ItemRestore::Reset] {
        let mut s = session(COLLECTED_REMOVES_FAR, restore);
        s.join("A".into(), Vec3::new(0., 0.35, 0.), true).unwrap();
        s.join("B".into(), Vec3::new(20., 0.35, 0.), true).unwrap();
        for _ in 0..60 {
            s.step().unwrap();
        }
        let globals = &s.package_state().packages["kit"].global;
        assert_eq!(globals["touches"], json!(1), "{restore:?}: asked once");
        assert_eq!(globals["collected"], json!(1), "{restore:?}");
        assert!(
            !s.simulation().state().bricks.contains_key(&2),
            "{restore:?}"
        );
        assert_eq!(
            spawner_available_at(&s, 2),
            None,
            "{restore:?}: offers nothing"
        );
        assert!(
            spawner_available_at(&s, 1).is_some(),
            "{restore:?}: near stays"
        );
        assert!(
            s.package_diagnostics().is_empty(),
            "{restore:?}: {:?}",
            s.package_diagnostics()
        );
    }
}

/// A pickup rule removes its own spawner, in every restore mode: no respawn
/// is booked for the gone brick, the other spawner keeps working.
#[test]
fn rule_removal_of_the_spawner_is_safe_in_every_restore_mode() {
    for restore in [bri_world::ItemRestore::Timer, bri_world::ItemRestore::Reset] {
        let mut s = session(PICKUP_REMOVES_SELF, restore);
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
