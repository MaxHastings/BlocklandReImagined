//! Engine seams a game-mode Add-On builds on, played headless through the
//! authoritative session: a game mode's own mini-game (`mode.json`
//! `minigame`), voxels dug out of and placed back into a generated world
//! (`voxel`, `place_voxel`, `can_place_voxel`, saved with the world), and
//! uniforms over a player's own colours (`set_avatar_colors`). Every Add-On
//! here is written by the test; all are our own (CC0).
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
use std::{collections::BTreeMap, sync::Arc};

const CUBE: &str = "v20/brick/brick4xcubedata";
const RULES: &str = "dig";
const MODE: &str = "dig-mode:mode/dig";

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
                bot: None,
            },
        )]
        .into(),
    }
}

/// A tool whose image runs the rules' `dig` on a click and `place` on jet.
const KIT_WEAPONS: &str = r#"{ "schema_version": 3, "id": "dig-kit",
  "items": { "dig-kit:weapon/spade": { "ui_name": "Spade", "image": "dig-kit:image/spade",
    "model": "", "icon": "", "can_drop": false } },
  "images": { "dig-kit:image/spade": { "name": "SpadeImage", "model": "", "melee": true,
    "command": "dig:dig", "commands": { "jet": "dig:place" },
    "states": [
      { "name": "Activate", "ticks": 18, "timeout": 1 },
      { "name": "Ready", "down": 2 },
      { "name": "Fire", "ticks": 30, "timeout": 1, "script": "onFire" } ] } } }"#;

/// A 16 x 16 patch of ground: stone at layer 0 under three layers of dirt.
const WORLD: &str = r#"{ "schema_version": 1, "generate": "generate_chunk",
  "chunk_voxels": 8, "voxel_size": 2.0, "voxel_brick": "v20/brick/brick4xcubedata",
  "view_chunks": 2, "radius_chunks": 2, "seed": 7,
  "materials": [
    { "id": "dig:material/stone", "name": "Stone", "color": [0.5, 0.5, 0.5, 1.0], "indestructible": true },
    { "id": "dig:material/dirt", "name": "Dirt", "color": [0.45, 0.32, 0.19, 1.0] } ] }"#;

const BEHAVIOUR: &str = r#"{ "schema_version": 1, "script": "dig.rhai",
  "commands": [ { "name": "dig", "cooldown_ticks": 24 }, { "name": "place", "cooldown_ticks": 24 } ],
  "state": { "player": { "dirt": { "default": 0, "visible": "owner", "persist": false } } },
  "on_join": true }"#;

const SCRIPT: &str = r#"
fn generate_chunk(cx, cz) {
    let out = [];
    if cx < 0 || cx > 1 || cz < 0 || cz > 1 { return out; }
    for lx in 0..8 {
        for lz in 0..8 {
            for y in 0..4 {
                out.push([cx * 8 + lx, y, cz * 8 + lz, if y == 0 { 0 } else { 1 }]);
            }
        }
    }
    out
}

fn on_join(p) {
    let coat = [0.72, 0.12, 0.1];
    set_avatar_colors(p, #{ torso: coat, larm: coat, rarm: coat });
}

fn aimed(me) {
    let hit = raycast([me.ex, me.ey, me.ez], [me.lx, me.ly, me.lz], 6.0, me.id);
    if hit == () || hit.kind != "brick" { return (); }
    let v = voxel(hit.id);
    if v == () { return (); }
    #{ v: v, hit: hit }
}

fn cmd_dig(p) {
    let a = aimed(player(p));
    if a == () || a.v.material != "dig:material/dirt" { return; }
    remove_brick(a.hit.id);
    set_player(p, "dirt", get_player(p, "dirt") + 1);
}

// On top of the voxel aimed at.
fn cmd_place(p) {
    let a = aimed(player(p));
    let carried = get_player(p, "dirt");
    if a == () || carried <= 0 { return; }
    let v = a.v;
    if !can_place_voxel(v.x, v.y + 1, v.z) { return; }
    place_voxel(v.x, v.y + 1, v.z, "dig:material/dirt");
    set_player(p, "dirt", carried - 1);
}
"#;

fn write(root: &std::path::Path, id: &str, files: &[(&str, &str)]) {
    let dir = root.join(id);
    for (name, body) in files {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }
}

fn manifest(id: &str, capabilities: &str, deps: &str, provides: &str) -> String {
    format!(
        r#"{{ "schema_version": 1, "id": "{id}", "version": "1.0.0", "api": 1,
             "name": "{id}", "license": "CC0-1.0", "provenance": {{ "source": "original" }},
             "dependencies": {{ {deps} }}, "capabilities": [{capabilities}], "provides": [{provides}] }}"#
    )
}

/// The rules, the tool, the mode, and a test hand that teleports its caller.
fn catalog() -> Arc<Catalog> {
    static CATALOG: std::sync::OnceLock<Arc<Catalog>> = std::sync::OnceLock::new();
    CATALOG.get_or_init(load_catalog).clone()
}

fn load_catalog() -> Arc<Catalog> {
    let root = std::env::temp_dir().join(format!("bri-mode-voxels-{}", std::process::id()));
    write(
        &root,
        "dig-kit",
        &[
            (
                "package.json",
                &manifest(
                    "dig-kit",
                    "",
                    "",
                    r#"{ "kind": "weapons", "id": "dig-kit:weapons/main", "file": "assets/weapons.json" }"#,
                ),
            ),
            ("assets/weapons.json", KIT_WEAPONS),
        ],
    );
    write(
        &root,
        "dig",
        &[
            (
                "package.json",
                &manifest(
                    "dig",
                    r#""world.edit", "player""#,
                    r#""v20-bricks": ">=4.0.0", "dig-kit": "^1.0.0""#,
                    r#"{ "kind": "behaviour", "id": "dig:behaviour/rules", "file": "behaviour.json" },
                       { "kind": "script", "id": "dig:script/rules", "file": "dig.rhai" },
                       { "kind": "world", "id": "dig:world/patch", "file": "world.json" }"#,
                ),
            ),
            ("behaviour.json", BEHAVIOUR),
            ("dig.rhai", SCRIPT),
            ("world.json", WORLD),
        ],
    );
    write(
        &root,
        "dig-mode",
        &[
            (
                "package.json",
                &manifest(
                    "dig-mode",
                    "",
                    r#""dig": "^1.0.0", "dig-kit": "^1.0.0""#,
                    r#"{ "kind": "mode", "id": "dig-mode:mode/dig", "file": "mode.json" }"#,
                ),
            ),
            (
                "mode.json",
                r#"{ "schema_version": 1, "name": "Dig", "description": "Dig.",
                     "map": "dig:world/patch", "add_ons": ["dig", "dig-kit"],
                     "minigame": { "title": "Dig Off", "loadout": ["dig-kit:weapon/spade"],
                       "player_type": "v20.player.playernojet", "respawn_seconds": 5,
                       "self_damage": false, "building": false, "painting": false } }"#,
            ),
        ],
    );
    write(
        &root,
        "test-hand",
        &[
            (
                "package.json",
                &manifest(
                    "test-hand",
                    r#""player""#,
                    "",
                    r#"{ "kind": "behaviour", "id": "test-hand:behaviour/main", "file": "behaviour.json" },
                       { "kind": "script", "id": "test-hand:script/main", "file": "hand.rhai" }"#,
                ),
            ),
            (
                "behaviour.json",
                r#"{ "schema_version": 1, "script": "hand.rhai", "commands": [
                     { "name": "goto", "args": ["float", "float", "float"] } ] }"#,
            ),
            (
                "hand.rhai",
                "fn cmd_goto(p, x, y, z) { teleport(p, x, y, z); }\n",
            ),
        ],
    );
    let mut packages = vec![PackageEntry {
        id: "v20-bricks".into(),
        version: "4.0.0".into(),
        side: Side::Shared,
        dir: "unused".into(),
        role: Some("brick_catalog".into()),
    }];
    for (id, side) in [
        ("dig-kit", Side::Shared),
        ("dig", Side::Server),
        ("dig-mode", Side::Server),
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
    // What `for_mode` gives a host, with the test hand left in.
    let mode = catalog.for_mode(MODE).unwrap_or_else(|e| panic!("{e:#?}"));
    assert!(mode.packages.contains_key("dig"));
    catalog.mode = mode.mode;
    Arc::new(catalog)
}

struct Game {
    s: Session,
    seq: BTreeMap<OwnerId, u64>,
    moves: BTreeMap<OwnerId, u64>,
    looks: BTreeMap<OwnerId, MoveInput>,
}
impl Game {
    fn new(save: Option<PackageSave>) -> Self {
        let world = World::new("Dig".into(), "dig".into(), vec![[1.0; 4]]);
        let mut s = Session::new(Simulation::new(world, definitions(), vec![]).unwrap());
        s.set_weapon_pack(bri_weapons::Pack::from_json(KIT_WEAPONS.as_bytes()).unwrap())
            .unwrap();
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
            .join(name.into(), Vec3::new(9.0, 12.0, 9.0), true)
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
    /// Stand on top of voxel column (x, z).
    fn stand(&mut self, owner: OwnerId, x: i64, z: i64) {
        let at = |v: i64| v as f64 * 2.0 + 1.0;
        self.run(
            owner,
            "test-hand",
            "goto",
            vec![
                PackageArg::Float(at(x)),
                PackageArg::Float(12.0),
                PackageArg::Float(at(z)),
            ],
        );
        self.steps(150);
    }
    fn dirt(&self, owner: OwnerId) -> i64 {
        self.s
            .package_value(RULES, owner, "dirt")
            .and_then(|v| v.as_i64())
            .unwrap()
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

#[test]
fn a_modes_minigame_holds_everyone_and_rules_dress_them() {
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
    g.steps(10);
    // The rules' colours go over the player's own; the rest stay theirs.
    let avatars = g.s.avatars();
    assert_eq!(avatars[&a].colors["torso"], [0.72, 0.12, 0.1, 1.0]);
    assert_eq!(avatars[&b].colors["rarm"], [0.72, 0.12, 0.1, 1.0]);
    assert_eq!(avatars[&a].colors["head"], [1.0, 0.9, 0.2, 1.0]);
    assert_eq!(avatars[&a].colors["rleg"], [1.0, 0.9, 0.2, 1.0]);
    // Everyone is in the mode's mini-game, which the server owns.
    let games = g.s.minigame_views();
    assert_eq!(games.len(), 1);
    assert_eq!(games[0].owner, 0);
    assert_eq!(games[0].settings.title, "Dig Off");
    assert_eq!(games[0].settings.player_type, "v20.player.playernojet");
    let vitals = g.s.vitals();
    for p in [a, b] {
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
    g.quiet();
}

#[test]
fn voxels_dig_out_pile_back_up_and_are_saved() {
    let mut g = Game::new(None);
    let a = g.join("Digger");
    g.steps(10);
    let generated = g.voxels();
    assert_eq!(generated.len(), 16 * 16 * 4);
    g.stand(a, 4, 4);
    let feet = g.feet(a);
    // Straight down: the cube under their feet comes out.
    g.look(a, 0.0, -1.5);
    g.run(a, RULES, "dig", vec![]);
    g.steps(60);
    assert_eq!(g.dirt(a), 1);
    let dug = g.voxels();
    assert_eq!(dug.len(), generated.len() - 1);
    assert!(!dug.contains_key(&[4, 3, 4]));
    assert!(g.feet(a).y < feet.y - 1.5, "they drop into the hole");
    // Piling it back on what they stand on is refused: they are in the way.
    g.run(a, RULES, "place", vec![]);
    g.steps(30);
    assert_eq!(g.dirt(a), 1);
    assert_eq!(g.voxels().len(), dug.len());
    // From the ground beside it, aimed ahead, it goes on top.
    g.stand(a, 10, 4);
    g.look(a, 0.0, -0.6);
    g.run(a, RULES, "place", vec![]);
    g.steps(30);
    assert_eq!(g.dirt(a), 0);
    let placed: Vec<([i64; 3], String)> = g
        .voxels()
        .into_iter()
        .filter(|(p, _)| !dug.contains_key(p))
        .collect();
    assert_eq!(placed.len(), 1, "{placed:?}");
    let (at, what) = placed[0].clone();
    assert_eq!(at[1], 4);
    assert_eq!(what, "dig:material/dirt");
    // Only dirt digs: the stone under it stays.
    g.stand(a, 4, 4);
    g.look(a, 0.0, -1.5);
    for _ in 0..3 {
        g.run(a, RULES, "dig", vec![]);
        g.steps(60);
    }
    assert!(g.voxels().contains_key(&[4, 0, 4]));
    g.quiet();
    // The world's edits both ways come back with a restart.
    let save = g.s.package_save().unwrap();
    let world = save.world.as_ref().unwrap();
    assert_eq!(world.added["dig:material/dirt"], vec![at]);
    let save = PackageSave::decode(&save.encode().unwrap()).unwrap();
    let again = Game::new(Some(save));
    let voxels = again.voxels();
    assert_eq!(voxels.get(&at), Some(&"dig:material/dirt".to_string()));
    assert!(!voxels.contains_key(&[4, 3, 4]));
}
