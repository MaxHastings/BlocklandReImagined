//! Add-On rules and their own items and shots, played through the
//! authoritative session with a made-up package: `on_pickup` (an ammo box
//! used up where it lies, a mine left alone), `on_drop` data a dropped gun
//! keeps for whoever picks it up, `on_projectile_hit`, `take_item`,
//! `drop_item`, tool-only commands, the cancel key and the player's
//! muzzle and tools.
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::Definitions,
    session::{Command, PackageArg, PackageCommand, Session},
    simulation::Simulation,
};
use bri_world::{OwnerId, World};
use glam::Vec3;
use rapier3d::prelude::*;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::Arc,
    sync::atomic::{AtomicUsize, Ordering},
};

const SCRIPT: &str = r#"
fn on_pickup(p, item, info) {
    set("touches", get("touches") + 1);
    if item == "kit:weapon/ammo" {
        // A bag left by `drop_item` carries its own count.
        set("rounds", get("rounds") + if info.data != () { info.data.rounds } else { 10 });
        return "take";
    }
    if item == "kit:weapon/mine" { return false; }
    if info.data != () { set("mag", info.data.rounds); }
    ()
}
fn on_drop(p, item, slot) { #{ rounds: 7 } }
fn on_projectile_hit(hit) {
    set("hit", `${hit.kind}|${hit.by}|${hit.projectile}|${hit.ny}`);
}
fn cmd_shoot(p) { fire("kit:projectile/round", 30.0, 5.0, 30.0, 0.0, -60.0, 0.0, p); }
fn cmd_toss(p, item, x, z) { drop_item(item, x, 0.2, z); }
fn cmd_bag(p, x, z) { drop_item("kit:weapon/ammo", x, 0.2, z, 0.0, 0.0, 0.0, #{ rounds: 25 }); }
fn cmd_take(p, item) { take_item(p, item); }
fn cmd_give(p, item) { give_item(p, item, true); }
fn cmd_facts(p) {
    let me = player(p);
    set("tools", me.tools.len());
    set("gun_slot", me.tools.index_of("kit:weapon/gun"));
    set("muzzle", me.my - me.ey);
}
fn cmd_fire(p) { set("fired", get("fired") + 1); }
fn cmd_mode(p) { set("mode", get("mode") + 1); }
"#;

fn behaviour() -> Value {
    let command = |name: &str, args: &[&str]| json!({ "name": name, "args": args });
    let counter = json!({ "default": 0, "visible": "everyone" });
    json!({
        "schema_version": 1,
        "script": "main.rhai",
        "on_pickup": true,
        "on_drop": true,
        "on_projectile_hit": true,
        "commands": [
            command("shoot", &[]),
            command("toss", &["string", "float", "float"]),
            command("bag", &["float", "float"]),
            command("take", &["string"]),
            command("give", &["string"]),
            command("facts", &[]),
            { "name": "fire", "tool_only": true },
            { "name": "mode", "tool_only": true }
        ],
        "state": { "global": {
            "touches": counter, "rounds": counter, "mag": counter,
            "tools": counter, "gun_slot": counter, "muzzle": { "default": 0.0, "visible": "everyone" },
            "fired": counter, "mode": counter,
            "hit": { "default": "", "visible": "everyone" }
        } }
    })
}

/// A gun whose trigger runs `kit:fire` and whose cancel key runs
/// `kit:mode`, an ammo box and a mine nobody holds, and a round.
fn weapons() -> bri_weapons::Pack {
    let pack = json!({
        "schema_version": 5,
        "id": "kit",
        "items": {
            "kit:weapon/gun": { "ui_name": "Kit Gun", "image": "kit:image/gun" },
            "kit:weapon/ammo": { "ui_name": "Kit Ammo" },
            "kit:weapon/mine": { "ui_name": "Kit Mine" }
        },
        "images": {
            "kit:image/gun": {
                "commands": { "states": { "onfire": "kit:fire" }, "cancel": "kit:mode" },
                "states": [
                    { "name": "Activate", "ticks": 6, "timeout": 1 },
                    { "name": "Ready", "down": 2 },
                    { "name": "Fire", "ticks": 12, "script": "onFire", "timeout": 3 },
                    { "name": "Hold", "up": 1 }
                ]
            }
        },
        "projectiles": {
            "kit:projectile/round": { "speed": 60.0, "lifetime_ticks": 240, "fade_ticks": 240 }
        }
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
        "bri-item-hooks-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    let dir = root.0.join("kit");
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = json!({
        "schema_version": 1, "id": "kit", "version": "1.0.0", "api": 1,
        "name": "kit", "license": "CC0-1.0",
        "capabilities": ["damage", "player"],
        "provides": [
            { "kind": "behaviour", "id": "kit:behaviour/main", "file": "behaviour.json" },
            { "kind": "script", "id": "kit:script/main", "file": "main.rhai" }
        ]
    });
    std::fs::write(dir.join("package.json"), manifest.to_string()).unwrap();
    std::fs::write(dir.join("behaviour.json"), behaviour().to_string()).unwrap();
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

struct Game {
    s: Session,
    seq: u64,
}
impl Game {
    fn new() -> Self {
        let ground = ColliderBuilder::cuboid(100.0, 0.5, 100.0)
            .translation(Vector::new(0.0, -0.5, 0.0))
            .user_data(u128::MAX);
        let mut s = Session::new(
            Simulation::new(
                World::new("Kit".into(), "kit".into(), vec![[1.0; 4]]),
                Definitions::default(),
                vec![ground],
            )
            .unwrap(),
        );
        s.set_weapon_pack(weapons()).unwrap();
        // Every item takes the fallback pickup box.
        s.set_item_bounds(BTreeMap::new()).unwrap();
        s.install_packages(catalog(), None).unwrap();
        Self { s, seq: 0 }
    }
    fn join(&mut self, at: Vec3) -> OwnerId {
        self.s.join("P".into(), at, true).unwrap()
    }
    fn send(&mut self, owner: OwnerId, command: Command) -> anyhow::Result<()> {
        self.seq += 1;
        self.s.command(owner, self.seq, command).map(drop)
    }
    fn run(&mut self, owner: OwnerId, command: &str, args: Vec<PackageArg>) -> anyhow::Result<()> {
        self.send(
            owner,
            Command::Package(PackageCommand {
                package: "kit".into(),
                command: command.into(),
                args,
            }),
        )
    }
    fn steps(&mut self, n: usize) {
        for _ in 0..n {
            self.s.step().unwrap();
        }
    }
    fn value(&self, key: &str) -> Value {
        self.s
            .package_state()
            .packages
            .get("kit")
            .and_then(|ns| ns.global.get(key).cloned())
            .unwrap_or(Value::Null)
    }
    fn drops(&self, item: &str) -> Vec<Vec3> {
        self.s
            .weapon_view()
            .drops
            .iter()
            .filter(|d| d.item == item)
            .map(|d| d.position)
            .collect()
    }
    fn holds(&self, owner: OwnerId, item: &str) -> bool {
        self.s.tool_inventories()[&owner]
            .slots
            .iter()
            .any(|s| s.as_deref() == Some(item))
    }
    fn diagnostics(&self) -> Vec<String> {
        self.s
            .package_diagnostics()
            .iter()
            .map(|d| format!("{}: {}", d.code, d.message))
            .collect()
    }
}
fn text(s: &str) -> PackageArg {
    PackageArg::String(s.into())
}
fn toss(item: &str, x: f32, z: f32) -> Vec<PackageArg> {
    vec![
        text(item),
        PackageArg::Float(x.into()),
        PackageArg::Float(z.into()),
    ]
}

#[test]
fn on_pickup_uses_up_an_ammo_box_and_leaves_a_mine() {
    let mut g = Game::new();
    let a = g.join(Vec3::new(0.0, 0.05, 0.0));
    g.steps(2);
    g.run(a, "toss", toss("kit:weapon/ammo", 0.0, 0.0)).unwrap();
    g.run(a, "toss", toss("kit:weapon/mine", 0.0, 0.0)).unwrap();
    assert_eq!(g.drops("kit:weapon/ammo").len(), 1);
    g.steps(4);
    // The ammo box went into the rules' count, not the player's tools.
    assert_eq!(g.value("rounds"), json!(10));
    assert!(g.drops("kit:weapon/ammo").is_empty());
    assert!(!g.holds(a, "kit:weapon/ammo"));
    // The mine stays where it lies, asked about while it is touched.
    let touches = g.value("touches").as_i64().unwrap();
    g.steps(4);
    assert_eq!(g.drops("kit:weapon/mine").len(), 1);
    assert!(!g.holds(a, "kit:weapon/mine"));
    assert!(g.value("touches").as_i64().unwrap() > touches);
    assert!(g.diagnostics().is_empty(), "{:?}", g.diagnostics());
}

#[test]
fn an_item_a_rule_drops_carries_its_data_to_whoever_picks_it_up() {
    let mut g = Game::new();
    let a = g.join(Vec3::new(0.0, 0.05, 0.0));
    g.steps(2);
    g.run(
        a,
        "bag",
        vec![PackageArg::Float(0.0), PackageArg::Float(0.0)],
    )
    .unwrap();
    g.steps(4);
    assert_eq!(g.value("rounds"), json!(25));
    assert!(g.drops("kit:weapon/ammo").is_empty());
    assert!(g.diagnostics().is_empty(), "{:?}", g.diagnostics());
}

#[test]
fn a_dropped_gun_keeps_what_on_drop_returned_for_its_next_holder() {
    let mut g = Game::new();
    let a = g.join(Vec3::new(0.0, 0.05, 0.0));
    g.steps(2);
    g.run(a, "give", vec![text("kit:weapon/gun")]).unwrap();
    g.steps(2);
    let slot = g.s.tool_inventories()[&a].selected.unwrap();
    g.send(a, Command::DropTool { slot }).unwrap();
    assert!(!g.holds(a, "kit:weapon/gun"));
    // Let it land, then someone else walks onto it.
    g.steps(360);
    let [at] = g.drops("kit:weapon/gun")[..] else {
        panic!("one dropped gun");
    };
    let b = g.join(Vec3::new(at.x, 0.05, at.z));
    g.steps(4);
    assert!(g.holds(b, "kit:weapon/gun"));
    assert_eq!(g.value("mag"), json!(7));
    assert!(g.diagnostics().is_empty(), "{:?}", g.diagnostics());
}

#[test]
fn on_projectile_hit_hears_where_its_round_struck() {
    let mut g = Game::new();
    let a = g.join(Vec3::new(0.0, 0.05, 0.0));
    g.steps(2);
    g.run(a, "shoot", vec![]).unwrap();
    g.steps(30);
    assert_eq!(
        g.value("hit"),
        json!(format!("map|{a}|kit:projectile/round|1.0"))
    );
}

#[test]
fn take_item_and_the_player_view_of_tools_and_muzzle() {
    let mut g = Game::new();
    let a = g.join(Vec3::new(0.0, 0.05, 0.0));
    g.steps(2);
    g.run(a, "give", vec![text("kit:weapon/gun")]).unwrap();
    g.steps(12);
    g.run(a, "facts", vec![]).unwrap();
    // The default hammer, wrench and printer first, then the gun.
    assert_eq!(g.value("tools"), json!(5));
    assert_eq!(g.value("gun_slot"), json!(3));
    // The host fires from the eye.
    assert_eq!(g.value("muzzle"), json!(0.0));
    g.run(a, "take", vec![text("kit:weapon/gun")]).unwrap();
    assert!(!g.holds(a, "kit:weapon/gun"));
    assert!(!g.s.weapon_view().images.contains_key(&a), "put away");
    g.run(a, "facts", vec![]).unwrap();
    assert_eq!(g.value("gun_slot"), json!(-1));
}

#[test]
fn tool_only_commands_run_from_the_gun_and_the_cancel_key_not_from_chat() {
    let mut g = Game::new();
    let a = g.join(Vec3::new(0.0, 0.05, 0.0));
    g.steps(2);
    // Typed, they are refused.
    for command in ["fire", "mode"] {
        let error = format!("{:#}", g.run(a, command, vec![]).unwrap_err());
        assert!(error.contains("run by its tool"), "{error}");
    }
    // The cancel key with nothing in hand does nothing.
    g.send(a, Command::CancelBrick).unwrap();
    assert_eq!(g.value("mode"), json!(0));
    g.run(a, "give", vec![text("kit:weapon/gun")]).unwrap();
    g.steps(12);
    g.send(a, Command::WeaponTrigger { down: true }).unwrap();
    g.steps(2);
    g.send(a, Command::WeaponTrigger { down: false }).unwrap();
    g.steps(20);
    assert_eq!(g.value("fired"), json!(1));
    g.send(a, Command::CancelBrick).unwrap();
    assert_eq!(g.value("mode"), json!(1));
    assert!(g.diagnostics().is_empty(), "{:?}", g.diagnostics());
}

/// Playtest (v0.1.12): a mini-game death dropped an item through an Add-On
/// rule, and every client left with "Invalid item drop view": an item the
/// world puts down has no thrower, which the clients' check refused.
#[test]
fn an_item_a_rule_drops_is_one_every_client_accepts() {
    let mut g = Game::new();
    let a = g.join(Vec3::new(0.0, 0.05, 0.0));
    g.steps(2);
    g.run(a, "toss", toss("kit:weapon/gun", 3.0, 3.0)).unwrap();
    let view = g.s.weapon_view();
    assert_eq!(view.drops.len(), 1);
    assert_eq!(view.drops[0].source, bri_weapons::ActorId::NOBODY);
    view.validate(&g.s.names()).unwrap();
    g.steps(4);
    g.s.weapon_view().validate(&g.s.names()).unwrap();
}
