//! Trench Warfare (`packages/trench-warfare`), played headless through the
//! authoritative session, and the engine seams under it: a game mode's own
//! mini-game (`mode.json` `minigame`), voxels placed back into a generated
//! world (`place_voxel`, `voxel`, `can_place_voxel`), and team uniforms
//! (`set_avatar_colors`).
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::{Definition, Definitions},
    player::MoveInput,
    session::{Command, MiniGameRequest, PackageArg, PackageCommand, PackageSave, Session},
    simulation::Simulation,
};
use bri_world::{OwnerId, World};
use glam::Vec3;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};

const CUBE: &str = "v20/brick/brick4xcubedata";
const RULES: &str = "trench";
const MODE: &str = "trench-mode:mode/trench-warfare";

fn definitions() -> Definitions {
    let mesh = Mesh {
        schema_version: 1,
        id: CUBE.into(),
        footprint_studs: [4, 4],
        height_plates: 10,
        attachment_rows: vec!["bbbb".into(); 40],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![],
    };
    let collision = CollisionBody {
        id: CUBE.into(),
        parts: vec![Part::Box {
            center: [0.0; 3],
            size: [2.0, 2.0, 2.0],
        }],
    };
    let shape = bri_physics::content::collider(&collision)
        .unwrap()
        .build()
        .shared_shape()
        .clone();
    Definitions {
        entries: [(
            CUBE.into(),
            Definition {
                mesh,
                collision,
                shape,
                indestructible: false,
                special: Default::default(),
                reflection: None,
                link: None,
                glass: [0.0; 4],
            },
        )]
        .into(),
    }
}

/// The stand-in digging tool (CC0): an image that runs the rules' `dig`
/// on a click and `place` on jet, with no model of its own.
const KIT_PACKAGE: &str = r#"{ "schema_version": 1, "id": "trench-kit", "version": "1.0.0", "api": 1,
  "name": "Test pick", "license": "CC0-1.0", "provenance": { "source": "original" },
  "provides": [{ "kind": "weapons", "id": "trench-kit:weapons/main", "file": "assets/weapons.json" }] }"#;
const KIT_WEAPONS: &str = r#"{ "schema_version": 3, "id": "trench-kit",
  "items": { "trench-kit:weapon/pick": { "ui_name": "Test Pick", "image": "trench-kit:image/pick",
    "model": "", "icon": "", "can_drop": false } },
  "images": { "trench-kit:image/pick": { "name": "TestPickImage", "model": "", "melee": true,
    "arm_ready": true, "command": "trench:dig", "commands": { "jet": "trench:place" },
    "states": [
      { "name": "Activate", "ticks": 18, "timeout": 1 },
      { "name": "Ready", "down": 2 },
      { "name": "PreFire", "ticks": 6, "timeout": 3, "arm": "armattack" },
      { "name": "Fire", "ticks": 30, "timeout": 4, "script": "onFire", "allow_change": false },
      { "name": "CheckFire", "down": 3, "up": 5 },
      { "name": "StopFire", "ticks": 6, "timeout": 1, "arm": "root" } ] } },
  "sounds": { "trench-kit:dig": { "file": "sounds/dig.wav" }, "trench-kit:place": { "file": "sounds/place.wav" },
    "trench-kit:whistle": { "file": "sounds/whistle.wav" } } }"#;

fn trench() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/trench-warfare")
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// The four Trench Warfare Add-Ons, plus a test hand that teleports its
/// caller and hurts players in their name, as the running mode.
fn catalog() -> Arc<Catalog> {
    static CATALOG: std::sync::OnceLock<Arc<Catalog>> = std::sync::OnceLock::new();
    CATALOG.get_or_init(load_catalog).clone()
}

fn load_catalog() -> Arc<Catalog> {
    let root = std::env::temp_dir().join(format!("bri-trench-{}", std::process::id()));
    for id in ["trench", "trench-hud", "trench-mode"] {
        copy_dir(&trench().join(id), &root.join(id));
    }
    // A stand-in for the digging tool, written here: the real one is the
    // player's own copy of the original Add-On, which is never committed.
    let kit = root.join("trench-kit");
    std::fs::create_dir_all(kit.join("assets")).unwrap();
    std::fs::write(kit.join("package.json"), KIT_PACKAGE).unwrap();
    std::fs::write(kit.join("assets/weapons.json"), KIT_WEAPONS).unwrap();
    let hand = root.join("test-hand");
    std::fs::create_dir_all(&hand).unwrap();
    std::fs::write(
        hand.join("package.json"),
        r#"{ "schema_version": 1, "id": "test-hand", "version": "1.0.0", "api": 1,
             "name": "Test hand", "license": "CC0-1.0",
             "provenance": { "source": "original" },
             "capabilities": ["player", "damage"],
             "provides": [
               { "kind": "behaviour", "id": "test-hand:behaviour/main", "file": "behaviour.json" },
               { "kind": "script", "id": "test-hand:script/main", "file": "hand.rhai" }
             ] }"#,
    )
    .unwrap();
    std::fs::write(
        hand.join("behaviour.json"),
        r#"{ "schema_version": 1, "script": "hand.rhai", "commands": [
             { "name": "goto", "args": ["float", "float", "float"] },
             { "name": "hurt", "args": ["int", "float"] } ] }"#,
    )
    .unwrap();
    std::fs::write(
        hand.join("hand.rhai"),
        "fn cmd_goto(p, x, y, z) { teleport(p, x, y, z); }\n\
         fn cmd_hurt(p, target, amount) { damage(target, amount, p); }\n",
    )
    .unwrap();
    let mut packages = vec![PackageEntry {
        id: "v20-bricks".into(),
        version: "4.0.0".into(),
        side: Side::Shared,
        dir: "unused".into(),
        role: Some("brick_catalog".into()),
    }];
    for (id, side) in [
        ("trench-kit", Side::Shared),
        ("trench", Side::Server),
        ("trench-hud", Side::Client),
        ("trench-mode", Side::Server),
        ("test-hand", Side::Server),
    ] {
        packages.push(PackageEntry {
            id: id.into(),
            version: "1.0.0".into(),
            side,
            dir: id.into(),
            role: None,
        });
    }
    let set = PackageSet {
        schema_version: 1,
        packages,
    };
    let mut catalog = Catalog::load(&root, &set, true).unwrap_or_else(|e| panic!("{e:#?}"));
    // What `for_mode` gives a host, with the test hand added.
    let mode = catalog.for_mode(MODE).unwrap_or_else(|e| panic!("{e:#?}"));
    assert!(mode.packages.contains_key("trench-hud"));
    catalog.mode = mode.mode;
    Arc::new(catalog)
}

/// The Trench Pick, and stand-ins for v20's Gun, Spear and Sword (the
/// mode's loadout): the Commando rifle's data under their ids.
fn weapons() -> bri_weapons::Pack {
    let rifle = std::fs::read_to_string(
        trench().join("../samples/sample-commando-rifle/assets/weapons.json"),
    )
    .unwrap()
    .replace("sample-commando-rifle:weapon/rifle", "v20.weapon.gunitem")
    .replace("sample-commando-rifle:image/rifle", "v20.image.gunimage");
    let mut base: serde_json::Value = serde_json::from_str(&rifle).unwrap();
    let gun = base["items"]["v20.weapon.gunitem"].clone();
    for id in ["v20.weapon.spearitem", "v20.weapon.sworditem"] {
        let mut item = gun.clone();
        item["id"] = id.into();
        base["items"][id] = item;
    }
    let base = bri_weapons::Pack::from_json(&serde_json::to_vec(&base).unwrap()).unwrap();
    let kit = bri_weapons::Pack::from_json(
        KIT_WEAPONS.as_bytes(),
    )
    .unwrap();
    let (pack, notes) = base.merge(vec![("trench-kit".into(), kit)]);
    assert!(notes.is_empty(), "{notes:?}");
    pack.validate().unwrap();
    pack
}

struct Game {
    s: Session,
    seq: BTreeMap<OwnerId, u64>,
    moves: BTreeMap<OwnerId, u64>,
    looks: BTreeMap<OwnerId, MoveInput>,
}
impl Game {
    fn new(save: Option<PackageSave>) -> Self {
        let world = World::new("Trench".into(), "trench".into(), vec![[1.0; 4]]);
        let mut s = Session::new(Simulation::new(world, definitions(), vec![]).unwrap());
        s.set_weapon_pack(weapons()).unwrap();
        let spawns = s.install_packages(catalog(), save).unwrap();
        s.set_spawn_points(spawns).unwrap();
        Self {
            s,
            seq: BTreeMap::new(),
            moves: BTreeMap::new(),
            looks: BTreeMap::new(),
        }
    }
    fn join(&mut self, name: &str) -> OwnerId {
        let owner = self
            .s
            .join(name.into(), Vec3::new(1.0, 16.0, 1.0), true)
            .unwrap();
        self.looks.insert(owner, MoveInput::default());
        owner
    }
    fn cmd(&mut self, owner: OwnerId, command: Command) -> anyhow::Result<()> {
        let n = self.seq.entry(owner).or_default();
        *n += 1;
        self.s.command(owner, *n, command).map(drop)
    }
    fn run(&mut self, owner: OwnerId, package: &str, command: &str, args: Vec<PackageArg>) {
        self.cmd(
            owner,
            Command::Package(PackageCommand {
                package: package.into(),
                command: command.into(),
                args,
            }),
        )
        .unwrap();
    }
    fn steps(&mut self, n: usize) {
        for _ in 0..n {
            for (owner, input) in &self.looks {
                let m = self.moves.entry(*owner).or_default();
                *m += 1;
                let _ = self.s.movement(*owner, *m, *input);
            }
            self.s.step().unwrap();
        }
    }
    fn look(&mut self, owner: OwnerId, yaw: f32, pitch: f32) {
        let look = self.looks.get_mut(&owner).unwrap();
        look.yaw = yaw;
        look.pitch = pitch;
        self.steps(3);
    }
    /// Stand at the top of voxel column (x, z) of the field.
    fn stand(&mut self, owner: OwnerId, x: i64, z: i64) {
        let at = |v: i64| v as f64 * 2.0 + 1.0;
        self.run(
            owner,
            "test-hand",
            "goto",
            vec![
                PackageArg::Float(at(x)),
                PackageArg::Float(26.0),
                PackageArg::Float(at(z)),
            ],
        );
        self.steps(150);
    }
    fn player(&self, owner: OwnerId, key: &str) -> serde_json::Value {
        self.s
            .package_value(RULES, owner, key)
            .unwrap_or(serde_json::Value::Null)
    }
    fn global(&self, key: &str) -> serde_json::Value {
        self.s.package_state().packages[RULES].global[key].clone()
    }
    fn health(&self, owner: OwnerId) -> f32 {
        self.s.vitals()[&owner].health
    }
    fn feet(&self, owner: OwnerId) -> Vec3 {
        Vec3::from(
            self.s
                .snapshot()
                .players
                .iter()
                .find(|p| p.owner == owner)
                .unwrap()
                .feet,
        )
    }
    /// Every voxel of the field, by position.
    fn voxels(&self) -> BTreeMap<[i64; 3], String> {
        self.s
            .simulation()
            .state()
            .bricks
            .keys()
            .filter_map(|id| self.s.package_voxel(*id))
            .collect()
    }
    fn quiet(&self) {
        assert!(
            self.s.package_diagnostics().is_empty(),
            "{:#?}",
            self.s.package_diagnostics()
        );
    }
}

fn material(name: &str) -> String {
    format!("trench:material/{name}")
}

#[test]
fn the_field_is_mirrored_walled_floored_and_guarded_by_sandbags() {
    let g = Game::new(None);
    let voxels = g.voxels();
    assert!(voxels.len() > 20_000, "{}", voxels.len());
    let top = |x: i64, z: i64| {
        (0..20)
            .rev()
            .find(|y| voxels.contains_key(&[x, *y, z]))
            .unwrap()
    };
    for z in -20..20 {
        for x in 0..40 {
            // Neither side has better ground.
            assert_eq!(top(x, z), top(-x - 1, z), "column {x}, {z}");
            for y in 0..=top(x, z) {
                assert_eq!(voxels[&[x, y, z]], voxels[&[-x - 1, y, z]]);
            }
            // Bedrock under everything.
            assert_eq!(voxels[&[x, 0, z]], material("bedrock"));
        }
        // Flat bases behind a line of sandbags with gaps in it.
        assert_eq!(top(37, z), 6);
        assert_eq!(voxels[&[37, 6, z]], material("grass"));
    }
    let bags = (-20..20)
        .filter(|z| voxels.get(&[34, 7, *z]) == Some(&material("sandbag")))
        .count();
    assert!((20..40).contains(&bags), "{bags} sandbags");
    // A rock rim all round, as high as dirt may pile.
    for z in -21..=20 {
        assert_eq!(top(40, z), 9);
        assert_eq!(voxels[&[40, 4, z]], material("rock"));
        assert_eq!(voxels[&[-41, 4, z]], material("rock"));
    }
    for x in -41..=40 {
        assert_eq!(voxels[&[x, 9, 20]], material("rock"));
        assert_eq!(voxels[&[x, 9, -21]], material("rock"));
    }
    // Shell holes and rolls in no-man's-land.
    let heights: std::collections::BTreeSet<i64> = (0..30)
        .flat_map(|x| (-20..20).map(move |z| (x, z)))
        .map(|(x, z)| top(x, z))
        .collect();
    assert!(heights.len() >= 3, "{heights:?}");
    assert!(
        !voxels
            .keys()
            .any(|[x, _, z]| *x > 40 || *x < -41 || *z > 20 || *z < -21)
    );
}

#[test]
fn players_split_into_uniformed_teams_inside_the_modes_own_minigame() {
    let mut g = Game::new(None);
    let colors: BTreeMap<String, [f32; 4]> = [
        "head",
        "torso",
        "hat",
        "accent",
        "pack",
        "secondpack",
        "hip",
        "rarm",
        "larm",
        "rhand",
        "lhand",
        "rleg",
        "lleg",
    ]
    .iter()
    .map(|k| (k.to_string(), [1.0, 0.9, 0.2, 1.0]))
    .collect();
    let none = |f: &str| serde_json::json!({"file": f, "sha256": "", "source": "", "width": 1, "height": 1});
    g.s.set_avatar_catalog(
        serde_json::from_value(serde_json::json!({
            "schema_version": 1, "id": "test", "rig": "rig.json", "rig_sha256": "",
            "parts": {"hat": ["none"], "accent": ["none"], "pack": ["none"],
                "secondpack": ["none"], "chest": ["chest"], "hip": ["pants"],
                "rarm": ["rarm"], "larm": ["larm"], "rhand": ["rhand"],
                "lhand": ["lhand"], "rleg": ["rshoe"], "lleg": ["lshoe"]},
            "accents_allowed": {}, "faces": ["smiley"], "decals": ["AAA-None"],
            "surfaces": {}, "textures": {"smiley": none("s.png"), "AAA-None": none("n.png")},
            "defaults": {"parts": {}, "colors": colors, "face": "smiley", "decal": "AAA-None"}
        }))
        .unwrap(),
    )
    .unwrap();
    let a = g.join("Alice");
    let b = g.join("Bob");
    let c = g.join("Carol");
    g.steps(10);
    assert_eq!(g.player(a, "team"), "Red");
    assert_eq!(g.player(b, "team"), "Blue");
    assert_eq!(g.player(c, "team"), "Red");
    // Uniforms over their own colours; hands and heads stay their own.
    let avatars = g.s.avatars();
    assert_eq!(avatars[&a].colors["torso"], [0.72, 0.12, 0.1, 1.0]);
    assert_eq!(avatars[&b].colors["torso"], [0.12, 0.27, 0.72, 1.0]);
    assert_eq!(avatars[&b].colors["rleg"], [0.08, 0.13, 0.33, 1.0]);
    assert_eq!(avatars[&a].colors["head"], [1.0, 0.9, 0.2, 1.0]);
    // Everyone is in the mode's mini-game, which the server owns.
    let games = g.s.minigame_views();
    assert_eq!(games.len(), 1);
    assert_eq!(games[0].owner, 0);
    assert_eq!(games[0].settings.title, "Trench Warfare");
    assert_eq!(games[0].settings.player_type, "v20.player.playernojet");
    let vitals = g.s.vitals();
    for p in [a, b, c] {
        assert_eq!(vitals[&p].minigame, Some(games[0].id));
    }
    // It is the only one: nobody leaves it or starts another.
    let leave = g
        .cmd(a, Command::MiniGame(MiniGameRequest::Leave))
        .unwrap_err();
    assert!(format!("{leave:#}").contains("game mode"), "{leave:#}");
    assert!(
        g.cmd(
            b,
            Command::MiniGame(MiniGameRequest::Create {
                color: 1,
                settings: Default::default()
            })
        )
        .is_err()
    );
    // Each team spawns behind its own sandbags.
    assert!(g.feet(a).x < -68.0 && g.feet(c).x < -68.0, "{}", g.feet(a));
    assert!(g.feet(b).x > 68.0, "{}", g.feet(b));
    g.quiet();
}

#[test]
fn digging_fills_the_bag_and_dirt_piles_back_up_and_is_saved() {
    let mut g = Game::new(None);
    let a = g.join("Digger");
    g.steps(10);
    let dirt = |g: &Game| g.player(a, "dirt").as_i64().unwrap();
    g.stand(a, 10, 0);
    let before = g.voxels();
    let feet = g.feet(a);
    // Straight down: the cube under their feet comes out, into the bag.
    g.look(a, 0.0, -1.5);
    g.run(a, RULES, "dig", vec![]);
    g.steps(60);
    assert_eq!(dirt(&g), 1);
    let after = g.voxels();
    assert_eq!(after.len(), before.len() - 1);
    assert!(g.feet(a).y < feet.y - 1.5, "they drop into the hole");
    // Piling it back where they stand is refused: they are in the way.
    g.run(a, RULES, "place", vec![]);
    g.steps(30);
    assert_eq!(dirt(&g), 1);
    assert_eq!(g.voxels().len(), after.len());
    // Aimed at the ground ahead, it goes on top of what they aim at.
    g.stand(a, 12, 0);
    g.look(a, 0.0, -0.6);
    g.run(a, RULES, "place", vec![]);
    g.steps(30);
    assert_eq!(dirt(&g), 0);
    let placed: Vec<([i64; 3], String)> = g
        .voxels()
        .into_iter()
        .filter(|(p, _)| !after.contains_key(p))
        .collect();
    assert_eq!(placed.len(), 1, "{placed:?}");
    let (at, what) = placed[0].clone();
    assert_eq!(what, material("dirt"));
    // An empty bag places nothing.
    g.run(a, RULES, "place", vec![]);
    g.steps(30);
    assert_eq!(g.voxels().len(), after.len() + 1);
    // Rock and sandbags do not dig; nor does anything at the bases.
    g.stand(a, 37, 0);
    g.look(a, 0.0, -1.5);
    g.run(a, RULES, "dig", vec![]);
    g.steps(30);
    assert_eq!(dirt(&g), 0);
    g.quiet();
    // The world's edits both ways come back with a restart.
    let save = g.s.package_save().unwrap();
    let world = save.world.as_ref().unwrap();
    assert_eq!(world.added[&material("dirt")], vec![at]);
    let save = PackageSave::decode(&save.encode().unwrap()).unwrap();
    let again = Game::new(Some(save));
    let voxels = again.voxels();
    assert_eq!(voxels.get(&at), Some(&material("dirt")));
    assert_eq!(voxels.len(), after.len() + 1);
}

#[test]
fn a_round_holds_fire_to_dig_in_then_scores_kills_across_the_lines() {
    let mut g = Game::new(None);
    let red = g.join("Red One");
    let blue = g.join("Blue One");
    let red2 = g.join("Red Two");
    g.steps(10);
    assert_eq!(g.player(red2, "team"), "Red");
    assert_eq!(g.global("phase"), "dig");
    let hurt = |g: &mut Game, by: OwnerId, target: OwnerId, amount: f64| {
        g.run(
            by,
            "test-hand",
            "hurt",
            vec![PackageArg::Int(target as i64), PackageArg::Float(amount)],
        );
        g.steps(2);
    };
    // Past spawn protection, but still in the ceasefire: nobody is hurt.
    g.steps(400);
    hurt(&mut g, red, blue, 30.0);
    assert_eq!(g.health(blue), 100.0);
    assert!(
        g.global("status")
            .as_str()
            .unwrap()
            .starts_with("Ceasefire")
    );
    // The whistle: 45 seconds after the round began.
    g.steps(45 * 120);
    assert_eq!(g.global("phase"), "fight");
    assert!(g.global("status").as_str().unwrap().starts_with("Fight!"));
    // No friendly fire; the other side is fair game.
    hurt(&mut g, red, red2, 30.0);
    assert_eq!(g.health(red2), 100.0);
    hurt(&mut g, red, blue, 30.0);
    assert_eq!(g.health(blue), 70.0);
    hurt(&mut g, red, blue, 100.0);
    assert!(!g.s.vitals()[&blue].alive);
    g.steps(2);
    assert_eq!(g.global("red"), 1);
    assert_eq!(g.global("blue"), 0);
    // An admin can start the next round: scores clear, the ceasefire is back.
    g.run(red, RULES, "newround", vec![]);
    g.steps(2);
    assert_eq!(g.global("round"), 2);
    assert_eq!(g.global("red"), 0);
    assert_eq!(g.global("phase"), "dig");
    g.quiet();
}

#[test]
fn dirt_is_handed_to_teammates_only() {
    let mut g = Game::new(None);
    let a = g.join("Alice");
    let b = g.join("Bob");
    let c = g.join("Carol");
    g.steps(10);
    let dirt = |g: &Game, p| g.player(p, "dirt").as_i64().unwrap();
    g.stand(a, 10, 0);
    g.look(a, 0.0, -1.5);
    g.run(a, RULES, "dig", vec![]);
    g.steps(60);
    g.run(a, RULES, "dig", vec![]);
    g.steps(60);
    assert_eq!(dirt(&g, a), 2);
    let give = |g: &mut Game, amount: i64, name: &str| {
        g.run(
            a,
            RULES,
            "givedirt",
            vec![PackageArg::Int(amount), PackageArg::String(name.into())],
        );
        g.steps(80);
    };
    // Bob plays for Blue: nothing moves.
    give(&mut g, 1, "bo");
    assert_eq!((dirt(&g, a), dirt(&g, b)), (2, 0));
    // Carol is Red: she gets what was asked, never more than Alice has.
    give(&mut g, 5, "CAR");
    assert_eq!((dirt(&g, a), dirt(&g, c)), (0, 2));
    g.quiet();
}
