//! Slayer's teams and its Capture the Flag mode, played through the
//! authoritative session with what the importer makes of the stand-in
//! copies in `tests/fixtures/ports` (CC0, the originals' folder names and
//! function shapes with their own numbers): `/teams` sets up and sorts the
//! teams, players spawn on their team's spawn, a flag stands on each Flag
//! Spawn in its brick's colour, an enemy flag rides on its carrier's back
//! and scores at their own flag, and a dropped flag is recovered by its
//! own team.
use bri_addon_import::{Options, import_with, ports::Ports};
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::{Definition, Definitions, Special},
    session::{Command, MiniGameRequest, Notice, PackageArg, PackageCommand, Reply, Session},
    simulation::Simulation,
};
use bri_world::{BrickId, OwnerId, World};
use glam::Vec3;
use rapier3d::prelude::*;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};

const FLAG: &str = "gamemode_slayer_ctf:brick/brickslyrctfflagdata";
const TEAM_SPAWN: &str = "gamemode_slayer:brick/brickslyrspawnpointdata";
const FLAG_ITEM: &str = "gamemode_slayer_ctf:weapon/slyrctf_flagitem";
const FLAG_IMAGE: &str = "gamemode_slayer_ctf:image/slyrctf_flagimage";
const SLAYER: &str = "gamemode_slayer-rules";
const RED: u8 = 0;
const BLUE: u8 = 1;
/// The stand-in's own numbers (`tests/fixtures/ports/Gamemode_Slayer_CTF`).
const FLAG_SLOT: u8 = 2;
const CAPTURE_POINTS: i64 = 25;
const RECOVERY_POINTS: i64 = 5;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/ports")
        .join(name)
}

/// The imports, their host rules, and a probe that moves players and reads
/// their teams.
fn content(root: &Path) -> (Arc<Catalog>, bri_weapons::Pack) {
    let ports = Ports::builtin();
    let mut ids = vec![];
    for (addon, ns) in [
        ("Gamemode_Slayer", "gamemode_slayer"),
        ("Gamemode_Slayer_CTF", "gamemode_slayer_ctf"),
    ] {
        let report = import_with(
            &Options {
                input: fixture(addon),
                out: root.join("addons").join(ns),
                reference: None,
                core: vec![],
                version: "1.0.0".into(),
            },
            &ports,
        )
        .unwrap();
        let rules = report.ports[0].rules.as_ref().expect("rules");
        ids.push((ns.to_owned(), Side::Shared));
        ids.push((rules.id.clone(), Side::Server));
    }
    let probe = root.join("probe");
    std::fs::create_dir_all(&probe).unwrap();
    std::fs::write(
        probe.join("package.json"),
        r#"{ "schema_version": 1, "id": "probe", "version": "1.0.0", "api": 1,
             "name": "Probe", "license": "CC0-1.0",
             "capabilities": ["player"],
             "provides": [
               { "kind": "behaviour", "id": "probe:behaviour/main", "file": "behaviour.json" },
               { "kind": "script", "id": "probe:script/main", "file": "probe.rhai" } ] }"#,
    )
    .unwrap();
    std::fs::write(
        probe.join("behaviour.json"),
        r#"{ "schema_version": 1, "script": "probe.rhai",
             "commands": [ { "name": "goto", "args": ["float", "float", "float"] },
                           { "name": "colour", "while_dead": true } ],
             "state": { "global": { "colours": { "default": {}, "visible": "everyone" } } } }"#,
    )
    .unwrap();
    std::fs::write(
        probe.join("probe.rhai"),
        "fn cmd_goto(p, x, y, z) { teleport(p, x, y, z); }\n\
         fn cmd_colour(p) {\n\
             let me = player(p);\n\
             let colours = get(\"colours\");\n\
             colours[`${p}`] = ();\n\
             if me.minigame != () { for t in minigame(me.minigame).teams { if t.id == me.team { colours[`${p}`] = t.color; } } }\n\
             set(\"colours\", colours);\n\
         }\n",
    )
    .unwrap();
    ids.push(("probe".into(), Side::Server));
    let set = PackageSet {
        schema_version: 1,
        packages: ids
            .into_iter()
            .map(|(id, side)| PackageEntry {
                dir: if id == "probe" {
                    "probe".into()
                } else {
                    format!("addons/{id}")
                },
                id,
                version: "1.0.0".into(),
                side,
                role: None,
            })
            .collect(),
    };
    let catalog = Catalog::load(root, &set, true).unwrap_or_else(|e| panic!("{e:#?}"));
    let pack = bri_weapons::Pack::from_json(
        &std::fs::read(root.join("addons/gamemode_slayer_ctf/assets/weapons.json")).unwrap(),
    )
    .unwrap();
    (Arc::new(catalog), pack)
}

/// A 2x1 plate under each brick kind the rules look for.
fn definitions() -> Definitions {
    let plate = |id: &str, special: Special| {
        let mesh = Mesh {
            schema_version: 1,
            id: id.into(),
            footprint_studs: [2, 1],
            height_plates: 1,
            attachment_rows: vec!["bb".into()],
            collision_boxes: vec![],
            needs_external_collision: false,
            coverage: None,
            quads: vec![],
        };
        let collision = CollisionBody {
            id: id.into(),
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
        let definition = Definition {
            mesh,
            collision,
            shape,
            indestructible: true,
            special,
            reflection: None,
            link: None,
            glass: [0.0; 4],
        };
        (id.to_owned(), definition)
    };
    Definitions {
        entries: [
            plate(FLAG, Special::None),
            plate(TEAM_SPAWN, Special::SpawnPoint),
        ]
        .into(),
    }
}

struct Game {
    s: Session,
    seq: BTreeMap<OwnerId, u64>,
    _root: Root,
}
struct Root(PathBuf);
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Game {
    fn new(name: &str) -> Self {
        let root = Root(std::env::temp_dir().join(format!(
            "bri-slayer-{}-{name}",
            std::process::id()
        )));
        let _ = std::fs::remove_dir_all(&root.0);
        let (catalog, pack) = content(&root.0);
        let ground = ColliderBuilder::cuboid(100.0, 0.5, 100.0)
            .translation(Vector::new(0.0, -0.5, 0.0))
            .user_data(u128::MAX);
        let mut s = Session::new(
            Simulation::new(
                World::new(
                    "CTF".into(),
                    "ctf".into(),
                    vec![[0.9, 0.1, 0.1, 1.0], [0.2, 0.4, 1.0, 1.0], [0.1, 0.8, 0.1, 1.0]],
                ),
                definitions(),
                vec![ground],
            )
            .unwrap(),
        );
        s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 20.0)]).unwrap();
        s.set_weapon_pack(pack).unwrap();
        s.set_item_bounds(BTreeMap::new()).unwrap();
        s.install_packages(catalog, None).unwrap();
        Self {
            s,
            seq: BTreeMap::new(),
            _root: root,
        }
    }
    fn cmd(&mut self, owner: OwnerId, command: Command) -> anyhow::Result<Reply> {
        let n = self.seq.entry(owner).or_default();
        *n += 1;
        self.s.command(owner, *n, command)
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
        .unwrap_or_else(|e| panic!("/{command}: {e:#}"));
    }
    fn teams(&mut self, owner: OwnerId, line: &str) {
        self.run(owner, SLAYER, "teams", vec![PackageArg::String(line.into())]);
        // Past the command's cooldown.
        self.steps(13);
    }
    /// Moves `owner` to `at`. A teleported player picks nothing up for the
    /// next five seconds (`TELEPORT_PICKUP_LOCK_MS`); see [`Game::settle`].
    fn goto(&mut self, owner: OwnerId, at: Vec3) {
        let [x, y, z] = at.to_array().map(|v| PackageArg::Float(v.into()));
        self.run(owner, "probe", "goto", vec![x, y, z]);
    }
    fn plant(&mut self, owner: OwnerId, kind: &str, x: f32, z: f32, color: u8) -> BrickId {
        self.steps(121);
        match self.cmd(
            owner,
            Command::Plant {
                definition: kind.into(),
                position: [x, 0.1, z + 0.25],
                quarter_turns: 0,
                color,
            },
        ) {
            Ok(Reply::Planted(id)) => id,
            other => panic!("plant {kind}: {other:?}"),
        }
    }
    /// Past the pickup lock after a teleport.
    fn settle(&mut self) {
        self.steps(5 * 120 + 10);
    }
    fn steps(&mut self, n: usize) {
        for _ in 0..n {
            self.s.step().unwrap();
        }
    }
    /// The paint colour of `owner`'s team.
    fn colour(&mut self, owner: OwnerId) -> Option<u8> {
        self.run(owner, "probe", "colour", vec![]);
        self.s
            .package_state()
            .packages
            .get("probe")
            .and_then(|ns| ns.global.get("colours").cloned())
            .and_then(|c| c.get(owner.to_string()).cloned())
            .and_then(|c| c.as_u64())
            .map(|c| c as u8)
    }
    fn score(&self, owner: OwnerId) -> i64 {
        self.s.vitals()[&owner].score
    }
    fn feet(&self, owner: OwnerId) -> Vec3 {
        self.s
            .motion_states()
            .into_iter()
            .find(|(p, _)| p.owner == owner)
            .unwrap()
            .0
            .feet
            .into()
    }
    /// The flag standing on `brick`, in its colour, if one does.
    fn flag_on(&self, brick: BrickId) -> Option<Option<u8>> {
        self.s
            .weapon_view()
            .static_items
            .iter()
            .find(|i| i.brick == brick && i.item == FLAG_ITEM)
            .map(|i| i.paint)
    }
    /// The flag `owner` carries on their back, in its colour.
    fn carried(&self, owner: OwnerId) -> Option<Option<u8>> {
        self.s
            .weapon_view()
            .images
            .get(&owner)?
            .iter()
            .find(|i| i.image == FLAG_IMAGE && i.hand == FLAG_SLOT)
            .map(|i| i.paint)
    }
    fn heard(&mut self, text: &str) -> bool {
        self.s
            .take_private_notices()
            .iter()
            .any(|(_, n)| matches!(n, Notice::Chat(t) if t.contains(text)))
    }
    fn quiet(&self) {
        let problems: Vec<String> = self
            .s
            .package_diagnostics()
            .iter()
            .map(|d| format!("{}: {}", d.code, d.message))
            .collect();
        assert!(problems.is_empty(), "{problems:?}");
    }
}

/// Two players in Alpha's mini-game, sorted onto Red and Blue by Slayer's
/// rules: (red player, blue player).
fn two_teams(g: &mut Game) -> (OwnerId, OwnerId) {
    let a = g.s.join("Alpha".into(), Vec3::new(-2.0, 0.05, 20.0), false).unwrap();
    let b = g.s.join("Bravo".into(), Vec3::new(2.0, 0.05, 20.0), false).unwrap();
    g.cmd(
        a,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            // The stock tools are not in this test's items.
            settings: bri_minigames::Settings {
                loadout: Default::default(),
                ..Default::default()
            },
        }),
    )
    .unwrap();
    let game = g.s.minigame_views()[0].id;
    g.cmd(b, Command::MiniGame(MiniGameRequest::Join { game })).unwrap();
    g.steps(2);
    g.teams(a, "add 0 Red");
    g.teams(a, "add 1 Blue");
    let (ca, cb) = (g.colour(a), g.colour(b));
    // One each, whichever way the draw fell.
    assert!(ca.is_some() && cb.is_some() && ca != cb, "{ca:?} {cb:?}");
    if ca == Some(RED) { (a, b) } else { (b, a) }
}

#[test]
fn teams_sort_and_spawn_on_their_own_team_spawns() {
    let mut g = Game::new("teams");
    let (red, blue) = two_teams(&mut g);
    let owner = g.s.minigame_views()[0].owner;
    let red_spawn = Vec3::new(-15.5, 0.2, 0.25);
    let blue_spawn = Vec3::new(15.5, 0.2, 0.25);
    g.plant(owner, TEAM_SPAWN, red_spawn.x, 0.0, RED);
    g.plant(owner, TEAM_SPAWN, blue_spawn.x, 0.0, BLUE);
    for (p, spawn) in [(red, red_spawn), (blue, blue_spawn)] {
        g.cmd(p, Command::Suicide).unwrap();
        g.steps(125);
        g.cmd(p, Command::Respawn).unwrap();
        g.steps(2);
        let feet = g.feet(p);
        assert!(
            Vec3::new(feet.x - spawn.x, 0.0, feet.z - spawn.z).length() < 1.0,
            "spawned at {feet} for the spawn at {spawn}"
        );
    }
    // Slayer's own words for its commands, and its short forms.
    g.s.take_private_notices();
    g.run(red, SLAYER, "teamcount", vec![]);
    g.steps(1);
    assert!(g.heard("(1) Red"));
    g.run(blue, SLAYER, "jointeam", vec![PackageArg::String("Blue".into())]);
    g.steps(1);
    assert!(g.heard("You're already on"));
    g.quiet();
}

#[test]
fn an_enemy_flag_rides_on_the_carriers_back_and_scores_at_home() {
    let mut g = Game::new("capture");
    let (red, blue) = two_teams(&mut g);
    let owner = g.s.minigame_views()[0].owner;
    let red_flag = g.plant(owner, FLAG, -8.5, 0.0, RED);
    let blue_flag = g.plant(owner, FLAG, 8.5, 0.0, BLUE);
    g.steps(31);
    // Each Flag Spawn holds its flag, in the brick's colour.
    assert_eq!(g.flag_on(red_flag), Some(Some(RED)));
    assert_eq!(g.flag_on(blue_flag), Some(Some(BLUE)));

    // Blue takes Red's flag: it leaves the stand and rides on Blue's back.
    g.s.take_private_notices();
    g.goto(blue, Vec3::new(-8.5, 0.25, 0.25));
    g.settle();
    assert_eq!(g.carried(blue), Some(Some(RED)), "carrying the red flag");
    assert_eq!(g.flag_on(red_flag), None);
    assert!(g.heard("picked up the"));
    // A flag never goes into the tools.
    assert!(!g.s.tool_inventories()[&blue]
        .slots
        .iter()
        .any(|s| s.as_deref() == Some(FLAG_ITEM)));

    // Home to Blue's own flag: a capture.
    g.goto(blue, Vec3::new(8.5, 0.25, 0.25));
    g.steps(30);
    assert_eq!(g.score(blue), CAPTURE_POINTS);
    assert_eq!(g.carried(blue), None);
    assert_eq!(g.flag_on(red_flag), Some(Some(RED)), "the red flag went home");
    assert!(g.heard("returned the"));
    assert_eq!(g.score(red), 0);
    g.quiet();
}

#[test]
fn a_dropped_flag_falls_in_its_colour_and_its_team_recovers_it() {
    let mut g = Game::new("drop");
    let (red, blue) = two_teams(&mut g);
    let owner = g.s.minigame_views()[0].owner;
    let red_flag = g.plant(owner, FLAG, -8.5, 0.0, RED);
    g.plant(owner, FLAG, 8.5, 0.0, BLUE);
    g.steps(31);
    g.goto(blue, Vec3::new(-8.5, 0.25, 0.25));
    g.settle();
    assert!(g.carried(blue).is_some());

    // Killed while carrying it: the flag falls where Blue stood, red.
    g.goto(blue, Vec3::new(0.0, 0.25, 0.0));
    g.steps(4);
    g.cmd(blue, Command::Suicide).unwrap();
    g.steps(2);
    assert_eq!(g.carried(blue), None);
    let drops: Vec<_> = g
        .s
        .weapon_view()
        .drops
        .into_iter()
        .filter(|d| d.item == FLAG_ITEM)
        .collect();
    let [dropped] = &drops[..] else {
        panic!("one dropped flag: {drops:?}");
    };
    assert_eq!(dropped.paint, Some(RED));
    assert!(g.heard("dropped the"));
    assert_eq!(g.flag_on(red_flag), None);

    // Red walks onto it: recovered, home, and points for Red.
    g.steps(120);
    let at = g
        .s
        .weapon_view()
        .drops
        .iter()
        .find(|d| d.item == FLAG_ITEM)
        .expect("still lying there")
        .position;
    g.goto(red, at + Vec3::new(0.0, 0.1, 0.0));
    g.settle();
    assert!(g.s.weapon_view().drops.iter().all(|d| d.item != FLAG_ITEM));
    assert_eq!(g.flag_on(red_flag), Some(Some(RED)));
    assert_eq!(g.score(red), RECOVERY_POINTS);
    assert!(g.heard("recovered the"));
    g.quiet();
}
