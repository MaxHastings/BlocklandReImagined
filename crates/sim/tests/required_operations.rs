//! `require` before the call takes effect, for the operations that grant,
//! hurt, place or award something: a shop that charges for what it cannot
//! hand over charges nobody (docs/modding/rules.md, "Reliable operations").
//!
//! Each case runs the same script twice. As `pay`, it takes 5 bits, then
//! `require`s the operation, then counts a sale: when the engine cannot do
//! the operation, the call is refused with `op.required` and neither the
//! bits nor the count move. As `free`, the operation is not required and
//! the same impossible request is reported as `op.failed` or skipped, as
//! before: the pre-check only ever applies to what a script asked it to.
//! The agreeing half matters as much: every operation here that the check
//! lets through must also really happen.
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::{Definition, Definitions},
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
fn think(me) { }
// One operation by name, aimed at player `t` (another player, or nobody).
fn what(p, name, t) {
    if name == "teleport" { return teleport(t, 0.0, 5.0, 0.0); }
    if name == "heal" { return heal(t, 5.0); }
    if name == "damage_player" { return damage(`player:${t}`, 1.0); }
    if name == "damage_entity" { return damage("entity:777", 1.0); }
    if name == "give_ammo" { return give_ammo(t, "rocket", 3); }
    if name == "set_rounds_missing" { return set_rounds(p, "kit:weapon/mine", 3); }
    if name == "set_reserve" { return set_reserve(t, "rocket", 3); }
    if name == "set_score" { return set_score(t, 5); }
    if name == "drop_known" { return drop_item("kit:weapon/mine", 4.0, 0.2, 4.0); }
    if name == "drop_unknown" { return drop_item("kit:weapon/nope", 4.0, 0.2, 4.0); }
    if name == "remove_drop_unknown" { return remove_drop(424242); }
    if name == "spawn_cow" { return spawn_entity("kit:entity/cow", 10.0, 0.0, 10.0); }
    if name == "spawn_unknown" { return spawn_entity("kit:entity/none", 10.0, 0.0, 10.0); }
    if name == "plant" { return plant_brick("v20/brick/plate", [20.0, 0.1, 20.0], 0, 0, 0); }
    if name == "plant_unknown" { return plant_brick("v20/brick/nothing", [20.0, 0.1, 20.0], 0, 0, 0); }
    if name == "plant_colour" { return plant_brick("v20/brick/plate", [24.0, 0.1, 24.0], 0, 9, 0); }
    if name == "plant_foreign_build" { return plant_brick("v20/brick/plate", [28.0, 0.1, 28.0], 0, 0, 7); }
    if name == "place" { return place_brick("v20/brick/plate", 30.0, 0.1, 30.25, 1.0, 1.0, 1.0); }
    if name == "place_unknown" { return place_brick("v20/brick/nothing", 30.0, 0.1, 30.0, 1.0, 1.0, 1.0); }
    if name == "remove_missing" { return remove_brick(987654); }
    ()
}
fn cmd_pay(p, name, t) {
    add_player(p, "bits", -5);
    require(what(p, name, t));
    set("sales", get("sales") + 1);
}
fn cmd_free(p, name, t) {
    add_player(p, "bits", -5);
    what(p, name, t);
    set("sales", get("sales") + 1);
}
fn cmd_slay(p, t) { damage(`player:${t}`, 1000.0); }
fn cmd_bits(p) { set("bits", get_player(p, "bits")); }
"#;

fn behaviour() -> Value {
    let counter = json!({ "default": 0, "visible": "everyone" });
    json!({
        "schema_version": 1,
        "script": "main.rhai",
        "commands": [
            { "name": "pay", "args": ["string", "int"] },
            { "name": "free", "args": ["string", "int"] },
            { "name": "slay", "args": ["int"] },
            { "name": "bits", "args": [] }
        ],
        "state": {
            "player": { "bits": { "default": 10, "visible": "everyone" } },
            "global": { "sales": counter, "bits": counter }
        }
    })
}

fn weapons() -> bri_weapons::Pack {
    let pack = json!({
        "schema_version": 5,
        "id": "kit",
        "items": { "kit:weapon/mine": { "ui_name": "Kit Mine" } },
        "images": {},
        "projectiles": {}
    });
    bri_weapons::Pack::from_json(&serde_json::to_vec(&pack).unwrap()).unwrap()
}

fn plate() -> (String, Definition) {
    let mesh = Mesh {
        schema_version: 1,
        id: "v20/brick/plate".into(),
        footprint_studs: [2, 1],
        height_plates: 1,
        attachment_rows: vec!["bb".into()],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![],
    };
    let collision = CollisionBody {
        id: "v20/brick/plate".into(),
        parts: vec![Part::Box {
            center: [0.0; 3],
            size: [1.0, 0.2, 0.5],
        }],
    };
    let shape = bri_physics::content::collider(&collision)
        .unwrap()
        .build()
        .shared_shape()
        .clone();
    (
        "v20/brick/plate".into(),
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
    )
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
        "bri-required-ops-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    let write = |dir: &str, file: &str, bytes: &[u8]| {
        std::fs::create_dir_all(root.0.join(dir)).unwrap();
        std::fs::write(root.0.join(dir).join(file), bytes).unwrap();
    };
    write(
        "kit",
        "package.json",
        json!({
            "schema_version": 1, "id": "kit", "version": "1.0.0", "api": 1,
            "name": "kit", "license": "CC0-1.0",
            "capabilities": ["damage", "player", "world.edit", "entity", "minigame"],
            "dependencies": { "kitmodel": "^1.0.0" },
            "provides": [
                { "kind": "behaviour", "id": "kit:behaviour/main", "file": "behaviour.json" },
                { "kind": "script", "id": "kit:script/main", "file": "main.rhai" },
                { "kind": "entity", "id": "kit:entity/cow", "file": "cow.json" }
            ]
        })
        .to_string()
        .as_bytes(),
    );
    write("kit", "behaviour.json", behaviour().to_string().as_bytes());
    write("kit", "main.rhai", SCRIPT.as_bytes());
    write(
        "kit",
        "cow.json",
        json!({
            "schema_version": 1, "name": "cow", "model": "kitmodel:model/body",
            "think": "think", "think_interval": 30,
            "scale": 0.5, "health": 10.0, "max_alive": 1
        })
        .to_string()
        .as_bytes(),
    );
    write(
        "kitmodel",
        "package.json",
        json!({
            "schema_version": 1, "id": "kitmodel", "version": "1.0.0", "api": 1,
            "name": "kitmodel", "license": "CC0-1.0", "capabilities": [],
            "provides": [{ "kind": "model", "id": "kitmodel:model/body", "file": "body.json" }]
        })
        .to_string()
        .as_bytes(),
    );
    write(
        "kitmodel",
        "body.json",
        json!({ "schema_version": 1, "boxes": [
            { "center": [0.0, 0.5, 0.0], "size": [0.5, 1.0, 0.5], "color": [0.2, 0.8, 0.2, 1.0] }
        ] })
        .to_string()
        .as_bytes(),
    );
    let entry = |id: &str, side| PackageEntry {
        id: id.into(),
        version: "1.0.0".into(),
        side,
        dir: id.into(),
        role: None,
    };
    let set = PackageSet {
        schema_version: 1,
        packages: vec![entry("kit", Side::Server), entry("kitmodel", Side::Client)],
    };
    Arc::new(Catalog::load(&root.0, &set, true).unwrap_or_else(|e| panic!("{e:#?}")))
}

struct Game {
    s: Session,
    seq: u64,
}
impl Game {
    fn new() -> Self {
        let mut s = Session::new(
            Simulation::new(
                World::new("Kit".into(), "kit".into(), vec![[1.0; 4], [0.0; 4]]),
                Definitions {
                    entries: [plate()].into(),
                },
                vec![
                    ColliderBuilder::cuboid(100.0, 0.5, 100.0)
                        .translation(Vector::new(0.0, -0.5, 0.0))
                        .user_data(u128::MAX),
                ],
            )
            .unwrap(),
        );
        s.set_weapon_pack(weapons()).unwrap();
        s.set_item_bounds(BTreeMap::new()).unwrap();
        s.install_packages(catalog(), None).unwrap();
        Self { s, seq: 0 }
    }
    fn join(&mut self, at: Vec3) -> OwnerId {
        self.s.join("P".into(), at, true).unwrap()
    }
    fn join_plain(&mut self, at: Vec3) -> OwnerId {
        self.s.join("Q".into(), at, false).unwrap()
    }
    fn run(&mut self, owner: OwnerId, command: &str, args: Vec<PackageArg>) -> anyhow::Result<()> {
        self.seq += 1;
        self.s
            .command(
                owner,
                self.seq,
                Command::Package(PackageCommand {
                    package: "kit".into(),
                    command: command.into(),
                    args,
                }),
            )
            .map(drop)
    }
    fn call(
        &mut self,
        owner: OwnerId,
        how: &str,
        what: &str,
        target: OwnerId,
    ) -> anyhow::Result<()> {
        self.run(
            owner,
            how,
            vec![
                PackageArg::String(what.into()),
                PackageArg::Int(target as i64),
            ],
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
    fn bits(&mut self, owner: OwnerId) -> i64 {
        self.run(owner, "bits", vec![]).unwrap();
        self.steps(1);
        self.value("bits").as_i64().unwrap()
    }
    fn diagnostics(&self) -> Vec<String> {
        self.s
            .package_diagnostics()
            .iter()
            .map(|d| format!("{}: {}", d.code, d.message))
            .collect()
    }
}

/// A game with a buyer, a bystander, and room around them.
fn game() -> (Game, OwnerId, OwnerId) {
    let mut g = Game::new();
    let buyer = g.join(Vec3::new(0.0, 0.05, 0.0));
    let other = g.join(Vec3::new(3.0, 0.05, 0.0));
    g.steps(2);
    (g, buyer, other)
}

/// An operation the engine cannot carry out is refused as `op.required`
/// with the reason, before anything of the call is kept; the same request
/// without `require` is reported as before.
#[test]
fn a_required_operation_the_engine_cannot_do_charges_nobody() {
    // (operation, what the refusal says)
    let impossible = [
        ("teleport", "No such player"),
        ("heal", "No such player"),
        ("damage_player", "No such player"),
        ("damage_entity", "No such entity"),
        ("give_ammo", "No such player"),
        ("set_rounds_missing", "carry no such item"),
        ("set_reserve", "No such player"),
        ("set_score", "No such player"),
        ("drop_unknown", "not an item of this server"),
        ("remove_drop_unknown", "not one `kit` put in the world"),
        ("spawn_unknown", "Unknown entity kind"),
        ("plant_unknown", "No brick `v20/brick/nothing` is loaded"),
        ("plant_colour", "not in the world's palette"),
        ("place_unknown", "No brick `v20/brick/nothing` is loaded"),
        ("remove_missing", "No such brick"),
    ];
    for (name, reason) in impossible {
        let (mut g, buyer, _) = game();
        // A target that is not in the game.
        let nobody = 4242;
        let refusal = g.call(buyer, "pay", name, nobody).expect_err(name);
        let text = format!("{refusal:#}");
        assert!(
            text.contains("op.required") || text.contains("cannot be done"),
            "{name}: {text}"
        );
        assert!(text.contains(reason), "{name}: wanted `{reason}` in {text}");
        assert_eq!(g.bits(buyer), 10, "{name}: charged for nothing");
        assert_eq!(g.value("sales"), json!(0), "{name}: counted a sale");
        let required = g
            .diagnostics()
            .into_iter()
            .filter(|d| d.starts_with("op.required"))
            .count();
        assert_eq!(required, 1, "{name}: {:?}", g.diagnostics());
        // Without `require` the call goes through and reports (or, for a
        // brick that does not fit, skips) as it always did.
        let (mut g, buyer, _) = game();
        g.call(buyer, "free", name, nobody).unwrap();
        g.steps(1);
        assert_eq!(g.bits(buyer), 5, "{name}: the unrequired call ran");
        assert_eq!(g.value("sales"), json!(1), "{name}");
        assert!(
            !g.diagnostics().iter().any(|d| d.starts_with("op.required")),
            "{name}: {:?}",
            g.diagnostics()
        );
    }
}

/// What the check lets through really happens: the sale is kept and no
/// operation fails afterwards.
#[test]
fn a_required_operation_the_engine_can_do_is_kept_and_happens() {
    let (mut g, buyer, other) = game();
    let possible = [
        "teleport",
        "heal",
        "damage_player",
        "give_ammo",
        "set_reserve",
        "set_score",
        "drop_known",
        "spawn_cow",
        "plant",
        "place",
    ];
    for (n, name) in possible.into_iter().enumerate() {
        g.call(buyer, "pay", name, other)
            .unwrap_or_else(|e| panic!("{name}: {e:#}"));
        g.steps(2);
        assert_eq!(
            g.value("sales"),
            json!(n + 1),
            "{name}: {:?}",
            g.diagnostics()
        );
    }
    assert_eq!(g.bits(buyer), 10 - 5 * possible.len() as i64);
    let failed: Vec<String> = g
        .diagnostics()
        .into_iter()
        .filter(|d| d.starts_with("op.failed") || d.starts_with("op.required"))
        .collect();
    assert!(
        failed.is_empty(),
        "an operation the check allowed failed: {failed:?}"
    );
    assert_eq!(g.s.package_stats().entities, 1, "the cow was spawned");
    assert_eq!(g.s.weapon_view().drops.len(), 1, "the item was dropped");
}

/// A package at its limit is refused up front: the one cow its kind allows
/// is out, so a second `spawn_entity` is not charged for.
#[test]
fn a_limit_reached_earlier_is_seen_by_the_next_required_call() {
    let (mut g, buyer, other) = game();
    g.call(buyer, "pay", "spawn_cow", other).unwrap();
    g.steps(2);
    let refusal = g.call(buyer, "pay", "spawn_cow", other).unwrap_err();
    assert!(
        format!("{refusal:#}").contains("at most 1 alive"),
        "{refusal:#}"
    );
    assert_eq!(g.bits(buyer), 5);
    assert_eq!(g.value("sales"), json!(1));
}

/// A brick that does not fit is skipped without a report when planted as
/// before, but a `require`d plant that cannot place it charges nobody.
#[test]
fn a_required_brick_that_does_not_fit_is_refused_not_skipped() {
    let (mut g, buyer, other) = game();
    for how in ["plant", "place"] {
        g.call(buyer, "pay", how, other).unwrap();
        g.steps(2);
        // The same spot again: the first brick is in the way.
        let refusal = g.call(buyer, "pay", how, other).unwrap_err();
        assert!(
            format!("{refusal:#}").contains("No room"),
            "{how}: {refusal:#}"
        );
    }
    assert_eq!(g.bits(buyer), 0, "two bricks bought, two refused");
    assert_eq!(g.value("sales"), json!(2));
    // Unrequired, the second plant is skipped as v20's `plant()` was.
    let before = g.s.package_diagnostics().len();
    g.call(buyer, "free", "plant", other).unwrap();
    g.steps(2);
    assert_eq!(g.s.package_diagnostics().len(), before, "skipped silently");
}

/// A dead player cannot be teleported or healed: the pre-check sees it.
#[test]
fn a_required_operation_on_a_dead_player_is_refused() {
    let (mut g, buyer, other) = game();
    // Past the spawn protection of a new life.
    g.steps(400);
    g.run(buyer, "slay", vec![PackageArg::Int(other as i64)])
        .unwrap();
    g.steps(2);
    for name in ["teleport", "heal"] {
        let refusal = g.call(buyer, "pay", name, other).unwrap_err();
        assert!(
            format!("{refusal:#}").contains("living") || format!("{refusal:#}").contains("alive"),
            "{name}: {refusal:#}"
        );
    }
    assert_eq!(g.bits(buyer), 10);
}

/// A plain player's rule may not plant into another player's build, and a
/// `require` hears so before charging.
#[test]
fn a_required_plant_into_a_build_the_caller_has_no_trust_on_is_refused() {
    let mut g = Game::new();
    let buyer = g.join_plain(Vec3::new(0.0, 0.05, 0.0));
    g.steps(2);
    let refusal = g
        .call(buyer, "pay", "plant_foreign_build", buyer)
        .unwrap_err();
    assert!(
        format!("{refusal:#}").contains("has trust on"),
        "{refusal:#}"
    );
    assert_eq!(g.bits(buyer), 10);
}
