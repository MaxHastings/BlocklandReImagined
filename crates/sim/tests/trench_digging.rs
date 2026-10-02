//! The Trench Digging port's host rules
//! (`crates/addon-import/ports/gamemode_trenchdigging/rules`), played headless
//! through the authoritative session with the brick seams under them
//! (`brick`, `bricks_in`, `can_plant`, `can_edit`, `plant_brick`). The
//! dirt bricks and tools are stand-ins with the import's ids and the port's
//! patched image states; nothing here is the original Add-On's.
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::{Definition, Definitions},
    player::MoveInput,
    session::{Command, MiniGameRequest, Notice, PackageArg, PackageCommand, Reply, Session},
    simulation::Simulation,
};
use bri_world::{Brick, BrickId, OwnerId, World};
use glam::Vec3;
use rapier3d::prelude::*;
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

const NS: &str = "gamemode_trenchdigging";
const RULES: &str = "gamemode_trenchdigging-rules";
const SHOVEL: &str = "gamemode_trenchdigging:weapon/trenchshovelitem";
const DIRT: &str = "gamemode_trenchdigging:weapon/trenchdirtitem";
const BROWN: u8 = 1;

fn kind(name: &str) -> String {
    format!("{NS}:brick/{name}")
}

fn port_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../addon-import/ports/gamemode_trenchdigging")
}

/// The import's dirt bricks: cubes 2x to 64x, flats 1x1 to 8x16.
fn definitions() -> Definitions {
    let mut entries = BTreeMap::new();
    let mut add = |name: &str, w: u32, d: u32, h: u32| {
        let id = kind(name);
        let mesh = Mesh {
            schema_version: 1,
            id: id.clone(),
            footprint_studs: [w, d],
            height_plates: h,
            attachment_rows: vec!["b".repeat(w as usize); (d * h) as usize],
            collision_boxes: vec![],
            needs_external_collision: false,
            coverage: None,
            quads: vec![],
        };
        let collision = CollisionBody {
            id: id.clone(),
            parts: vec![Part::Box {
                center: [0.0; 3],
                size: [w as f32 * 0.5, h as f32 * 0.2, d as f32 * 0.5],
            }],
        };
        let shape = bri_physics::content::collider(&collision)
            .unwrap()
            .build()
            .shared_shape()
            .clone();
        entries.insert(
            id,
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
        );
    };
    for (i, name) in ["2x", "4x", "8x", "16x", "32x"].iter().enumerate() {
        let s = 2u32 << i;
        add(&format!("brick{name}cubedirtdata"), s, s, s * 5 / 2);
    }
    for (name, w, d) in [
        ("1x1", 1, 1),
        ("2x2", 2, 2),
        ("4x4", 4, 4),
        ("8x8", 8, 8),
        ("8x16", 8, 16),
    ] {
        add(&format!("brick{name}dirtdata"), w, d, 3);
    }
    Definitions { entries }
}

/// The import's tools with the port's patch applied: the images run the
/// rules' commands with the port's own states.
fn weapons() -> Value {
    let port: Value =
        serde_json::from_slice(&std::fs::read(port_dir().join("port.json")).unwrap()).unwrap();
    let patched = &port["patch"]["assets/weapons.json"]["images"];
    let admin_states = json!([
        { "name": "Activate", "ticks": 6, "timeout": 1 },
        { "name": "Ready", "down": 2 },
        { "name": "Fire", "ticks": 5, "allow_change": false, "timeout": 1, "script": "onFire" }
    ]);
    let mut images = serde_json::Map::new();
    for (name, melee) in [
        ("trenchshovelimage", true),
        ("trenchdirtimage", false),
        ("adminshovelimage", true),
        ("admindirtimage", true),
    ] {
        let patch = &patched[format!("{{namespace}}:image/{name}")];
        let command = patch["command"].as_str().unwrap().replace("{rules}", RULES);
        let id = format!("{NS}:image/{name}");
        images.insert(
            id.clone(),
            json!({ "id": id, "name": name, "model": "", "melee": melee, "arm_ready": true,
                    "command": command,
                    "states": patch.get("states").cloned().unwrap_or(admin_states.clone()) }),
        );
    }
    json!({
        "schema_version": 3, "id": NS,
        "items": {
            SHOVEL: { "ui_name": "Trench Shovel", "image": format!("{NS}:image/trenchshovelimage"),
                "model": "", "icon": "", "can_drop": true },
            DIRT: { "ui_name": "Trench Dirt", "image": format!("{NS}:image/trenchdirtimage"),
                "model": "", "icon": "", "can_drop": true }
        },
        "images": images,
        "projectiles": {
            format!("{NS}:projectile/trenchdirtprojectile"): {
                "id": format!("{NS}:projectile/trenchdirtprojectile"),
                "name": "TrenchDirtProjectile", "speed": 40.0, "inherit": 1.0,
                "lifetime_ticks": 42, "ballistic": true }
        }
    })
}

fn write(root: &std::path::Path, dir: &str, files: &[(&str, String)]) {
    for (name, body) in files {
        let path = root.join(dir).join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }
}

/// The import (shared: its tools) and its rules, filled in as the importer
/// fills them for this namespace and a copy whose reach reads 10.
fn catalog() -> Arc<Catalog> {
    static CATALOG: std::sync::OnceLock<Arc<Catalog>> = std::sync::OnceLock::new();
    CATALOG
        .get_or_init(|| {
            let root = std::env::temp_dir().join(format!("bri-trench-port-{}", std::process::id()));
            let manifest = |id: &str, caps: &str, deps: &str, provides: &str| {
                format!(
                    r#"{{ "schema_version": 1, "id": "{id}", "version": "1.0.0", "api": 1,
                         "name": "{id}", "license": "CC0-1.0", "provenance": {{ "source": "original" }},
                         "dependencies": {{ {deps} }}, "capabilities": [{caps}], "provides": [{provides}] }}"#
                )
            };
            write(
                &root,
                NS,
                &[
                    (
                        "package.json",
                        manifest(
                            NS,
                            "",
                            "",
                            &format!(
                                r#"{{ "kind": "weapons", "id": "{NS}:weapons/main", "file": "assets/weapons.json" }}"#
                            ),
                        ),
                    ),
                    ("assets/weapons.json", weapons().to_string()),
                ],
            );
            let fill = |text: String| {
                text.replace("{{namespace}}", NS)
                    .replace("{{rules}}", RULES)
                    .replace("{{reach}}", "10")
            };
            let rules = |file: &str| fill(std::fs::read_to_string(port_dir().join("rules").join(file)).unwrap());
            write(
                &root,
                RULES,
                &[
                    (
                        "package.json",
                        manifest(
                            RULES,
                            r#""world.edit", "chat", "player", "physics", "damage""#,
                            &format!(r#""{NS}": "=1.0.0""#),
                            &format!(
                                r#"{{ "kind": "behaviour", "id": "{RULES}:behaviour/behaviour", "file": "behaviour.json" }},
                                   {{ "kind": "script", "id": "{RULES}:script/trench", "file": "trench.rhai" }},
                                   {{ "kind": "archetype", "id": "{RULES}:archetype/playernojet", "file": "archetypes/playernojet.json" }}"#
                            ),
                        ),
                    ),
                    ("behaviour.json", rules("behaviour.json")),
                    ("trench.rhai", rules("trench.rhai")),
                    (
                        "archetypes/playernojet.json",
                        rules("archetypes/playernojet.json"),
                    ),
                ],
            );
            // Reads a player's eye for the test's aim.
            write(
                &root,
                "test-eye",
                &[
                    (
                        "package.json",
                        manifest(
                            "test-eye",
                            r#""player""#,
                            "",
                            r#"{ "kind": "behaviour", "id": "test-eye:behaviour/main", "file": "behaviour.json" },
                               { "kind": "script", "id": "test-eye:script/main", "file": "eye.rhai" }"#,
                        ),
                    ),
                    (
                        "behaviour.json",
                        r#"{ "schema_version": 1, "script": "eye.rhai", "commands": [ { "name": "eye" } ],
                             "state": { "player": { "eye": { "default": [], "persist": false } } } }"#
                            .into(),
                    ),
                    (
                        "eye.rhai",
                        "fn cmd_eye(p) { let m = player(p); set_player(p, \"eye\", [m.ex, m.ey, m.ez]); }\n".into(),
                    ),
                ],
            );
            let packages = [(NS, Side::Shared), (RULES, Side::Server), ("test-eye", Side::Server)]
                .into_iter()
                .map(|(id, side)| PackageEntry {
                    id: id.into(),
                    version: "1.0.0".into(),
                    side,
                    dir: id.into(),
                    role: None,
                })
                .collect();
            let set = PackageSet {
                schema_version: 1,
                packages,
            };
            Arc::new(Catalog::load(&root, &set, true).unwrap_or_else(|e| panic!("{e:#?}")))
        })
        .clone()
}

struct Game {
    s: Session,
    seq: BTreeMap<OwnerId, u64>,
    moves: BTreeMap<OwnerId, u64>,
    looks: BTreeMap<OwnerId, MoveInput>,
}
impl Game {
    fn new() -> Self {
        Self::with_map(vec![])
    }
    /// Over flat ground and `extra` map shapes.
    fn with_map(extra: Vec<ColliderBuilder>) -> Self {
        let world = World::new(
            "Trench".into(),
            "trench".into(),
            vec![[1.0; 4], [0.45, 0.32, 0.19, 1.0], [0.3, 0.2, 0.1, 1.0]],
        );
        let mut s = Session::new(
            Simulation::new(
                world,
                definitions(),
                [ColliderBuilder::cuboid(100.0, 0.5, 100.0)
                    .translation(Vector::new(0.0, -0.5, 0.0))]
                .into_iter()
                .chain(extra)
                .collect(),
            )
            .unwrap(),
        );
        s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 20.0)])
            .unwrap();
        s.set_weapon_pack(bri_weapons::Pack::from_json(weapons().to_string().as_bytes()).unwrap())
            .unwrap();
        s.install_packages(catalog(), None).unwrap();
        Self {
            s,
            seq: BTreeMap::new(),
            moves: BTreeMap::new(),
            looks: BTreeMap::new(),
        }
    }
    fn join(&mut self, name: &str, at: Vec3, admin: bool) -> OwnerId {
        let owner = self.s.join(name.into(), at, admin).unwrap();
        self.looks.insert(owner, MoveInput::default());
        owner
    }
    fn cmd(&mut self, owner: OwnerId, command: Command) -> anyhow::Result<Reply> {
        let n = self.seq.entry(owner).or_default();
        *n += 1;
        self.s.command(owner, *n, command)
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
    fn plant(&mut self, owner: OwnerId, name: &str, position: [f32; 3]) -> BrickId {
        match self.cmd(
            owner,
            Command::Plant {
                definition: kind(name),
                position,
                quarter_turns: 0,
                color: BROWN,
            },
        ) {
            Ok(Reply::Planted(id)) => id,
            other => panic!("plant {name} at {position:?}: {other:?}"),
        }
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
    /// Where the player's eye is now.
    fn eye(&mut self, owner: OwnerId) -> Vec3 {
        self.cmd(
            owner,
            Command::Package(PackageCommand {
                package: "test-eye".into(),
                command: "eye".into(),
                args: vec![],
            }),
        )
        .unwrap();
        let eye: Vec<f32> = serde_json::from_value(
            self.s
                .package_value("test-eye", owner, "eye")
                .unwrap()
                .clone(),
        )
        .unwrap();
        Vec3::new(eye[0], eye[1], eye[2])
    }
    /// Look from the eye at `target`. The eye moves a little as the head
    /// pitches, so aim again from where it went.
    fn look_at(&mut self, owner: OwnerId, target: Vec3) {
        for _ in 0..3 {
            let d = (target - self.eye(owner)).normalize();
            let look = self.looks.get_mut(&owner).unwrap();
            look.yaw = d.x.atan2(-d.z);
            look.pitch = d.y.asin();
            self.steps(3);
        }
    }
    /// Hold `item` and click once with it.
    fn click(&mut self, owner: OwnerId, item: &str) {
        let slot = self.s.tool_inventories()[&owner]
            .slots
            .iter()
            .position(|s| s.as_deref() == Some(item))
            .unwrap_or_else(|| panic!("no {item}"));
        if self.s.tool_inventories()[&owner].selected != Some(slot) {
            self.cmd(owner, Command::EquipTool { slot: Some(slot) })
                .unwrap();
        }
        self.until(owner, "Ready");
        self.cmd(owner, Command::WeaponTrigger { down: true })
            .unwrap();
        self.until(owner, "Fire");
        self.s.release_trigger(owner).unwrap();
        self.until(owner, "Ready");
    }
    /// Step until the held image is in `state`.
    fn until(&mut self, owner: OwnerId, state: &str) {
        for _ in 0..600 {
            let images = self.s.weapon_view().images.get(&owner).cloned();
            if images.is_some_and(|i| i.iter().any(|i| i.hand == 0 && i.state == state)) {
                return;
            }
            self.steps(1);
        }
        panic!("the image never reached {state}");
    }
    fn typed(&mut self, owner: OwnerId, command: &str) -> anyhow::Result<Reply> {
        self.cmd(
            owner,
            Command::Package(PackageCommand {
                package: String::new(),
                command: command.into(),
                args: Vec::<PackageArg>::new(),
            }),
        )
    }
    fn dirt(&self, owner: OwnerId) -> i64 {
        self.s
            .package_value(RULES, owner, "dirt")
            .and_then(|v| v.as_i64())
            .unwrap()
    }
    /// Every brick, by kind short name and centre.
    fn bricks(&self) -> Vec<(String, [f32; 3])> {
        let mut out: Vec<(String, [f32; 3])> = self
            .s
            .snapshot()
            .world
            .bricks
            .into_iter()
            .map(|(_, b): (BrickId, Brick)| {
                let bri_world::ContentRef::Resolved(k) = &b.definition else {
                    panic!("unresolved brick")
                };
                (
                    k.trim_start_matches(&format!("{NS}:brick/")).to_string(),
                    b.position,
                )
            })
            .collect();
        out.sort_by(|a, b| a.partial_cmp(b).unwrap());
        out
    }
    fn prints(&mut self, owner: OwnerId) -> Vec<String> {
        self.s
            .take_private_notices()
            .into_iter()
            .filter(|(o, _)| *o == owner)
            .filter_map(|(_, n)| match n {
                // The colour codes the rules' `\cN` became, read back as
                // the original wrote them.
                Notice::Center { text, .. } | Notice::Bottom { text, .. } => Some(
                    text.chars()
                        .map(|c| match c as u32 {
                            n @ 0xE000..=0xE009 => format!("\\c{}", n - 0xE000),
                            _ => c.to_string(),
                        })
                        .collect(),
                ),
                _ => None,
            })
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

fn has(bricks: &[(String, [f32; 3])], name: &str, at: [f32; 3]) -> bool {
    bricks
        .iter()
        .any(|(k, p)| k == name && (0..3).all(|a| (p[a] - at[a]).abs() < 0.01))
}

#[test]
fn the_shovel_splits_dirt_down_to_the_piece_it_hit_and_the_dirt_puts_it_back() {
    let mut g = Game::new();
    let host = g.join("Host", Vec3::new(1.25, 0.05, 1.5), true);
    g.steps(10);
    g.s.give_tool(host, SHOVEL, false).unwrap();
    g.s.give_tool(host, DIRT, false).unwrap();
    // An 8x cube, x 0 to 4, up 0 to 4, z -4 to 0.
    g.plant(host, "brick8xcubedirtdata", [2.0, 2.0, -2.0]);
    g.steps(5);
    g.prints(host);

    // Straight ahead at its face, at eye height left of middle: it splits
    // into 4x cubes, the upper left front one into 2x cubes, and the 2x
    // the shovel hit goes into the pocket.
    g.look_at(host, Vec3::new(1.25, 2.156, 0.0));
    g.click(host, SHOVEL);
    assert_eq!(g.dirt(host), 1);
    let bricks = g.bricks();
    assert_eq!(bricks.len(), 7 + 7, "{bricks:?}");
    assert!(!has(&bricks, "brick4xcubedirtdata", [1.0, 3.0, -1.0]));
    assert!(has(&bricks, "brick4xcubedirtdata", [3.0, 1.0, -3.0]));
    assert!(!has(&bricks, "brick2xcubedirtdata", [1.5, 2.5, -0.5]));
    assert!(has(&bricks, "brick2xcubedirtdata", [0.5, 2.5, -0.5]));
    assert_eq!(g.prints(host).last().unwrap(), "\\c31\\c6/\\c3100 dirt");

    // The dirt goes back into the hole (against the 2x behind it), the
    // eight 2x cubes become a 4x again and the eight 4x cubes the 8x.
    g.click(host, DIRT);
    assert_eq!(g.dirt(host), 0);
    let bricks = g.bricks();
    assert_eq!(
        bricks,
        [("brick8xcubedirtdata".to_string(), [2.0, 2.0, -2.0])],
        "{bricks:?}"
    );
    assert_eq!(g.prints(host).last().unwrap(), "\\c30\\c6/\\c3100 dirt");

    // With none left the dirt says so and plants nothing.
    g.click(host, DIRT);
    assert!(
        g.prints(host)
            .contains(&"\\c3You have no dirt to release!".to_string())
    );
    assert_eq!(g.bricks().len(), 1);
    g.quiet();
}

#[test]
fn dirt_is_dug_under_the_minigame_and_trust_rules() {
    let mut g = Game::new();
    let host = g.join("Host", Vec3::new(10.0, 0.05, 10.0), true);
    let digger = g.join("Digger", Vec3::new(1.25, 0.05, 1.5), false);
    g.steps(10);
    g.plant(host, "brick8xcubedirtdata", [2.0, 2.0, -2.0]);
    g.steps(5);
    // Outside a minigame, the host's dirt is not the digger's to dig.
    g.s.give_tool(digger, SHOVEL, false).unwrap();
    g.look_at(digger, Vec3::new(1.25, 2.156, 0.0));
    g.click(digger, SHOVEL);
    assert_eq!(g.dirt(digger), 0);
    assert_eq!(g.bricks().len(), 1);

    // The host's minigame hands out the shovel and the dirt, and plays
    // with the host's bricks: now it digs.
    // Joining spawns them again: in front of the dirt.
    g.s.set_spawn_points(vec![Vec3::new(1.25, 0.05, 1.5)])
        .unwrap();
    let settings = bri_minigames::Settings {
        loadout: [Some(SHOVEL.into()), Some(DIRT.into()), None, None, None],
        ..Default::default()
    };
    g.cmd(
        host,
        Command::MiniGame(MiniGameRequest::Create { color: 1, settings }),
    )
    .unwrap();
    let game = g.s.minigame_views()[0].id;
    g.cmd(digger, Command::MiniGame(MiniGameRequest::Join { game }))
        .unwrap();
    g.steps(60);
    let tools = g.s.tool_inventories()[&digger].clone();
    assert_eq!(tools.slots[0].as_deref(), Some(SHOVEL));
    assert_eq!(tools.slots[1].as_deref(), Some(DIRT));
    let feet = g.feet(digger);
    g.look_at(digger, Vec3::new(feet.x, feet.y + 2.156, 0.0));
    g.click(digger, SHOVEL);
    assert_eq!(g.dirt(digger), 1);
    assert_eq!(g.bricks().len(), 14);
    // The pieces stay the host's.
    let snapshot = g.s.snapshot();
    assert!(snapshot.world.bricks.values().all(|b| b.owner == host));
    g.quiet();
}

#[test]
fn dumped_dirt_piles_up_as_4x_cubes_and_admins_get_endless_dirt() {
    let mut g = Game::new();
    let host = g.join("Host", Vec3::new(4.25, 0.05, 1.0), true);
    g.steps(10);
    g.s.give_tool(host, SHOVEL, false).unwrap();
    // A 16x cube, x 0 to 8, up 0 to 8, z -8 to 0, and a 2x beside.
    g.plant(host, "brick16xcubedirtdata", [4.0, 4.0, -4.0]);
    g.plant(host, "brick2xcubedirtdata", [1.0, 0.5, 4.0]);
    g.steps(5);
    // Eight digs straight ahead tunnel through it.
    g.look_at(host, Vec3::new(4.25, 2.156, -1.0));
    for n in 1..=8 {
        g.click(host, SHOVEL);
        assert_eq!(g.dirt(host), n);
    }
    // /dumpdirt on top of the 2x: one 4x cube (eight dirt), on it.
    g.look_at(host, Vec3::new(1.0, 1.0, 4.0));
    g.typed(host, "dumpdirt").unwrap();
    g.steps(2);
    assert_eq!(g.dirt(host), 0);
    let bricks = g.bricks();
    assert!(
        has(&bricks, "brick4xcubedirtdata", [1.5, 2.0, 3.5]),
        "{bricks:?}"
    );

    // Endless dirt for an admin: digging adds none, placing takes none.
    g.typed(host, "infinitedigging").unwrap();
    g.steps(2);
    assert!(
        g.prints(host)
            .iter()
            .any(|t| t.ends_with("\\c3100 dirt") && t.starts_with("\\c3∞"))
    );
    g.look_at(host, Vec3::new(2.75, 2.156, 0.0));
    let before = g.bricks();
    g.click(host, SHOVEL);
    assert_eq!(g.dirt(host), 0);
    assert_ne!(g.bricks(), before);
    // And it is admins' only: a player is refused.
    let other = g.join("Other", Vec3::new(-5.0, 0.05, 5.0), false);
    g.steps(5);
    assert!(g.typed(other, "infinitedigging").is_err());
    assert!(g.typed(other, "speeddig").is_err());
    g.quiet();
}

#[test]
fn flats_split_into_quarters_and_four_1x1s_become_a_2x2() {
    let mut g = Game::new();
    let host = g.join("Host", Vec3::new(1.25, 0.05, 1.5), true);
    g.steps(10);
    g.s.give_tool(host, SHOVEL, false).unwrap();
    g.s.give_tool(host, DIRT, false).unwrap();
    // A 4x4 flat, x 0 to 2, z -2 to 0.
    g.plant(host, "brick4x4dirtdata", [1.0, 0.3, -1.0]);
    g.steps(5);
    // Its top near the front left: 2x2s, the front left one into 1x1s,
    // and the 1x1 under the aim is taken.
    g.look_at(host, Vec3::new(0.6, 0.6, -0.4));
    g.click(host, SHOVEL);
    assert_eq!(g.dirt(host), 1);
    let bricks = g.bricks();
    assert_eq!(bricks.len(), 3 + 3, "{bricks:?}");
    assert!(has(&bricks, "brick2x2dirtdata", [1.5, 0.3, -1.5]));
    assert!(has(&bricks, "brick1x1dirtdata", [0.25, 0.3, -0.25]));
    assert!(
        !has(&bricks, "brick1x1dirtdata", [0.75, 0.3, -0.25]),
        "{bricks:?}"
    );

    // Against the side of the 1x1 behind the hole: it goes back, and the
    // four 1x1s are a 2x2 again, then the four 2x2s the 4x4.
    g.look_at(host, Vec3::new(0.75, 0.3, -0.5));
    g.click(host, DIRT);
    assert_eq!(g.dirt(host), 0);
    let bricks = g.bricks();
    assert_eq!(
        bricks,
        [("brick4x4dirtdata".to_string(), [1.0, 0.3, -1.0])],
        "{bricks:?}"
    );
    g.quiet();
}

#[test]
fn no_jet_players_step_up_onto_a_2x_cube_while_it_is_on() {
    // A plate-high slab under where the cubes go.
    let mut g = Game::with_map(vec![
        ColliderBuilder::cuboid(10.0, 0.1, 0.5).translation(Vector::new(0.0, 0.1, -1.5)),
    ]);
    // server.cs: PlayerNoJet.maxStepHeight = 1.2; everyone else keeps 1.0.
    let table = g.s.archetypes();
    let step = |id: &str| table.resolve(table.find(id).unwrap()).movement.step_height;
    assert_eq!(step("v20.player.playernojet"), 1.2);
    assert_eq!(step("v20.player.playerstandardarmor"), 1.0);
    let host = g.join("Host", Vec3::new(-3.0, 0.05, 2.0), true);
    let walker = g.join("Walker", Vec3::new(3.0, 0.05, 2.0), false);
    g.steps(10);
    // A row of 2x cubes on the slab: their tops are 1.2 up.
    for x in -5..=5 {
        g.plant(host, "brick2xcubedirtdata", [x as f32 + 0.5, 0.7, -1.5]);
    }
    g.s.set_spawn_points(vec![Vec3::new(-3.0, 0.05, 2.0)])
        .unwrap();
    let settings = bri_minigames::Settings {
        player_type: "v20.player.playernojet".into(),
        loadout: [Some(SHOVEL.into()), Some(DIRT.into()), None, None, None],
        ..Default::default()
    };
    g.cmd(
        host,
        Command::MiniGame(MiniGameRequest::Create { color: 1, settings }),
    )
    .unwrap();
    g.steps(60);
    for owner in [host, walker] {
        g.looks.get_mut(&owner).unwrap().forward = 1.0;
    }
    g.steps(90);
    // The No Jet host stands on the cubes; the standard walker is stopped.
    assert!((g.feet(host).y - 1.2).abs() < 0.05, "{}", g.feet(host));
    assert!(g.feet(walker).y < 0.1, "{}", g.feet(walker));
    g.quiet();
}
