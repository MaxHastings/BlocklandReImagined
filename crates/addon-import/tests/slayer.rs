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
    session::{
        Command, ControlObject, MiniGameRequest, Notice, ObserverButton, OrbitBody, PackageArg,
        PackageCommand, Reply, Session, SettingEdit, TeamEdit,
    },
    simulation::Simulation,
};
use bri_world::{BrickId, EventRow, EventTarget, EventValue, OwnerId, World, authority::Edit};
use glam::Vec3;
use rapier3d::prelude::*;
use bri_minigames::SettingValue as Value;
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
const CTF: &str = "gamemode_slayer_ctf-rules";
/// The stand-ins' game modes (`server/defaults/game-modes.cs`,
/// `game-mode.cs`).
const TEAM_MODE: &str = "Slayer_TeamDeathmatch";
const CTF_MODE: &str = "Slayer_CTF";
/// The stand-in's time between rounds, seconds.
const BETWEEN_ROUNDS: usize = 6;
const FROZEN: &str = "gamemode_slayer:archetype/playerfrozenarmor";
const RED: u8 = 0;
const BLUE: u8 = 1;
/// The stand-in's own numbers (`tests/fixtures/ports/Gamemode_Slayer_CTF`).
const FLAG_SLOT: u8 = 2;
const CAPTURE_POINTS: i64 = 25;
/// The stand-in's capture point: three ticks to fill, a tick each 100 ms,
/// seven points a capture.
const CP: &str = "gamemode_slayer:brick/brickslyrcpdata";
const CP_TICK: usize = 12;
const CP_POINTS: i64 = 7;
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
                ..Default::default()
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
             "capabilities": ["player", "brick_events", "world.edit", "damage"],
             "provides": [
               { "kind": "behaviour", "id": "probe:behaviour/main", "file": "behaviour.json" },
               { "kind": "script", "id": "probe:script/main", "file": "probe.rhai" } ] }"#,
    )
    .unwrap();
    std::fs::write(
        probe.join("behaviour.json"),
        r#"{ "schema_version": 1, "script": "probe.rhai",
             "brick_inputs": [ { "name": "onPoke", "targets": ["Player", "Client", "MiniGame"] } ],
             "commands": [ { "name": "goto", "args": ["float", "float", "float"] },
                           { "name": "colour", "while_dead": true },
                           { "name": "kit", "while_dead": true },
                           { "name": "poke", "args": ["int"] },
                           { "name": "unstock", "args": ["int"] },
                           { "name": "strip", "args": ["int"] },
                           { "name": "kill", "args": ["int"] } ],
             "state": { "global": { "colours": { "default": {}, "visible": "everyone" },
                                    "kits": { "default": {}, "visible": "everyone" } } } }"#,
    )
    .unwrap();
    std::fs::write(
        probe.join("probe.rhai"),
        "fn cmd_goto(p, x, y, z) { teleport(p, x, y, z); }\n\
         fn cmd_poke(p, brick) { fire_brick_input(brick, \"onPoke\", p); }\n\
         fn cmd_colour(p) {\n\
             let me = player(p);\n\
             let colours = get(\"colours\");\n\
             colours[`${p}`] = ();\n\
             if me.minigame != () { for t in minigame(me.minigame).teams { if t.id == me.team { colours[`${p}`] = t.color; } } }\n\
             set(\"colours\", colours);\n\
         }\n\
         fn cmd_kit(p) { let kits = get(\"kits\"); kits[`${p}`] = player(p).tools; set(\"kits\", kits); }\n\
         fn cmd_unstock(p, brick) { set_brick_item(brick, ()); }\n\
         fn cmd_strip(p, slot) { mount_image(p, (), slot); }\n\
         fn cmd_kill(p, t) { damage(t, 1000.0, p); }\n",
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
            plate(CP, Special::None),
        ]
        .into(),
    }
}

/// v20's avatar lists, in their order: Slayer's custom uniform names parts
/// by their place in them.
fn avatars() -> bri_content::avatar::Package {
    let texture = |file: &str| {
        serde_json::json!({ "file": file, "sha256": "", "source": "", "width": 1, "height": 1 })
    };
    let color = |c: f32| serde_json::json!([c, c, c, 1.0]);
    let slots = [
        "head", "torso", "hat", "accent", "pack", "secondpack", "hip", "rarm", "larm", "rhand",
        "lhand", "rleg", "lleg",
    ];
    let plumes = serde_json::json!(["none", "plume", "triplume", "septplume"]);
    serde_json::from_value(serde_json::json!({
        "schema_version": 1, "id": "test", "rig": "rig.json", "rig_sha256": "",
        "parts": {
            "hat": ["none", "helmet", "pointyhelmet", "flarehelmet", "scouthat", "bicorn",
                "cophat", "knithat"],
            "accent": ["none", "plume", "triplume", "septplume", "visor"],
            "pack": ["none", "armor", "bucket", "cape", "pack", "quiver", "tank"],
            "secondpack": ["none", "epaulets", "epauletsranka", "epauletsrankb",
                "epauletsrankc", "epauletsrankd", "shoulderpads"],
            "chest": ["chest", "femchest"], "hip": ["pants", "skirthip"],
            "rarm": ["rarm", "rarmslim"], "larm": ["larm", "larmslim"],
            "rhand": ["rhand", "rhook"], "lhand": ["lhand", "lhook"],
            "rleg": ["rshoe", "rpeg"], "lleg": ["lshoe", "lpeg"]
        },
        "accents_allowed": {
            "helmet": ["none", "visor"], "pointyhelmet": plumes, "flarehelmet": plumes,
            "scouthat": plumes, "bicorn": plumes, "cophat": plumes, "knithat": plumes
        },
        "faces": ["smiley", "smileyEvil1"], "decals": ["AAA-None", "Alyx"], "surfaces": {},
        "textures": {
            "smiley": texture("smiley.png"), "smileyEvil1": texture("evil.png"),
            "AAA-None": texture("none.png"), "Alyx": texture("alyx.png")
        },
        "defaults": {
            "parts": {},
            "colors": slots.iter().map(|s| (s.to_string(), color(0.5))).collect::<serde_json::Map<_, _>>(),
            "face": "smiley", "decal": "AAA-None"
        }
    }))
    .unwrap()
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
        let root =
            Root(std::env::temp_dir().join(format!("bri-slayer-{}-{name}", std::process::id())));
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
                    vec![
                        [0.9, 0.1, 0.1, 1.0],
                        [0.2, 0.4, 1.0, 1.0],
                        [0.1, 0.8, 0.1, 1.0],
                    ],
                ),
                definitions(),
                vec![ground],
            )
            .unwrap(),
        );
        s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 20.0)])
            .unwrap();
        s.set_avatar_catalog(avatars()).unwrap();
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
        self.run(
            owner,
            SLAYER,
            "teams",
            vec![PackageArg::String(line.into())],
        );
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
    /// The archetype id `owner`'s body is.
    fn body(&self, owner: OwnerId) -> String {
        let (state, _) = self
            .s
            .motion_states()
            .into_iter()
            .find(|(p, _)| p.owner == owner)
            .unwrap();
        self.s.archetypes().resolve(state.archetype).id.clone()
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
    /// Changes Add-On settings of the game, as its owner in the Mini-Game
    /// window: `("ns-rules:key", value)`.
    fn set(&mut self, owner: OwnerId, settings: &[(&str, Value)]) {
        let game = self.s.minigame_views()[0].id;
        self.cmd(
            owner,
            Command::MiniGame(MiniGameRequest::AddOnSettings {
                game,
                settings: settings
                    .iter()
                    .map(|(key, value)| SettingEdit {
                        key: (*key).into(),
                        value: Some(value.clone()),
                    })
                    .collect(),
                teams: None,
            }),
        )
        .unwrap();
        self.steps(1);
    }
    /// Changes Add-On settings of the team in `color`, as the game's owner
    /// in the Mini-Game window, leaving the other teams as they are.
    fn set_team(&mut self, owner: OwnerId, color: u8, settings: &[(&str, Value)]) {
        let view = self.s.minigame_views()[0].clone();
        let teams = view
            .teams
            .iter()
            .map(|t| TeamEdit {
                id: Some(t.id.0),
                name: t.name.clone(),
                color: t.color,
                settings: if t.color == color {
                    settings
                        .iter()
                        .map(|(key, value)| SettingEdit {
                            key: (*key).into(),
                            value: Some(value.clone()),
                        })
                        .collect()
                } else {
                    vec![]
                },
            })
            .collect();
        self.cmd(
            owner,
            Command::MiniGame(MiniGameRequest::AddOnSettings {
                game: view.id,
                settings: vec![],
                teams: Some(teams),
            }),
        )
        .unwrap();
        self.steps(2);
    }
    /// The item in each of `owner`'s tool slots, empty for none.
    fn tools(&mut self, owner: OwnerId) -> Vec<String> {
        self.run(owner, "probe", "kit", vec![]);
        self.steps(13);
        let kits = self.s.package_state().packages["probe"].global["kits"].clone();
        serde_json::from_value(kits[owner.to_string()].clone()).unwrap()
    }
    fn scale(&self, owner: OwnerId) -> f32 {
        let (state, _) = self
            .s
            .motion_states()
            .into_iter()
            .find(|(p, _)| p.owner == owner)
            .unwrap();
        state.scale
    }
    fn round_over(&self) -> bool {
        // Everyone's camera is off their body while a round is over.
        self.s.vitals().values().all(|v| v.respawn_held)
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
    two_teams_as(g, false)
}

/// [`two_teams`], Alpha an admin when `admin`.
fn two_teams_as(g: &mut Game, admin: bool) -> (OwnerId, OwnerId) {
    let a =
        g.s.join("Alpha".into(), Vec3::new(-2.0, 0.05, 20.0), admin)
            .unwrap();
    let b =
        g.s.join("Bravo".into(), Vec3::new(2.0, 0.05, 20.0), false)
            .unwrap();
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
    g.cmd(b, Command::MiniGame(MiniGameRequest::Join { game }))
        .unwrap();
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
    g.run(
        blue,
        SLAYER,
        "jointeam",
        vec![PackageArg::String("Blue".into())],
    );
    g.steps(1);
    assert!(g.heard("You're already on"));
    g.quiet();
}

#[test]
fn an_enemy_flag_rides_on_the_carriers_back_and_scores_at_home() {
    let mut g = Game::new("capture");
    let (red, blue) = two_teams(&mut g);
    let owner = g.s.minigame_views()[0].owner;
    g.set(owner, &[(&key(SLAYER, "mode"), Value::Text(CTF_MODE.into()))]);
    // Its brick events are in builders' wrench.
    let inputs: Vec<_> = g
        .s
        .package_brick_inputs()
        .into_iter()
        .filter(|i| i.source != "probe")
        .map(|i| i.name)
        .collect();
    assert!(inputs.ends_with(&[
        "onFlagPickedUp".to_string(),
        "onFlagDropped".into(),
        "onFlagReturned".into(),
        "onFlagRecovered".into()
    ]));
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
    assert!(
        !g.s.tool_inventories()[&blue]
            .slots
            .iter()
            .any(|s| s.as_deref() == Some(FLAG_ITEM))
    );

    // Home to Blue's own flag: a capture.
    g.goto(blue, Vec3::new(8.5, 0.25, 0.25));
    g.steps(30);
    assert_eq!(g.score(blue), CAPTURE_POINTS);
    assert_eq!(g.carried(blue), None);
    assert_eq!(
        g.flag_on(red_flag),
        Some(Some(RED)),
        "the red flag went home"
    );
    assert!(g.heard("returned the"));
    assert_eq!(g.score(red), 0);
    g.quiet();
}

/// The flags follow the game mode (`Slayer_CTF::onGameModeStart` and
/// `onGameModeEnd`), stay on their stands when something else takes the
/// stand's item (`serverCmdSetWrenchData`, packaged), and a carried flag is
/// no other Add-On's to take off (`Player::unMountImage`, packaged).
#[test]
fn flags_follow_the_mode_and_stay_on_their_stands_and_backs() {
    let mut g = Game::new("flag-guards");
    let (red, blue) = two_teams(&mut g);
    let owner = g.s.minigame_views()[0].owner;
    let mode = key(SLAYER, "mode");
    g.set(owner, &[(&mode, Value::Text(CTF_MODE.into()))]);
    let red_flag = g.plant(owner, FLAG, -8.5, 0.0, RED);
    let blue_flag = g.plant(owner, FLAG, 8.5, 0.0, BLUE);
    g.steps(31);
    assert_eq!(g.flag_on(red_flag), Some(Some(RED)));

    // The stand's item taken: the flag is back on the next check.
    g.run(owner, "probe", "unstock", vec![PackageArg::Int(red_flag as i64)]);
    g.steps(31);
    assert_eq!(g.flag_on(red_flag), Some(Some(RED)));

    // Carried, no other Add-On takes it off.
    g.goto(blue, Vec3::new(-8.5, 0.25, 0.25));
    g.settle();
    assert_eq!(g.carried(blue), Some(Some(RED)));
    g.run(blue, "probe", "strip", vec![PackageArg::Int(i64::from(FLAG_SLOT))]);
    g.steps(1);
    assert_eq!(g.carried(blue), Some(Some(RED)), "still on Blue's back");

    // Another mode: every flag gone at once, stands and backs.
    g.set(owner, &[(&mode, Value::Text(TEAM_MODE.into()))]);
    g.steps(1);
    assert_eq!(g.carried(blue), None);
    assert_eq!(g.flag_on(red_flag), None);
    assert_eq!(g.flag_on(blue_flag), None);
    // Capture the Flag again: both stand at home at once.
    g.goto(blue, Vec3::new(0.0, 0.25, 0.0));
    g.set(owner, &[(&mode, Value::Text(CTF_MODE.into()))]);
    assert_eq!(g.flag_on(red_flag), Some(Some(RED)));
    assert_eq!(g.flag_on(blue_flag), Some(Some(BLUE)));
    assert_eq!(g.score(red), 0);
}

#[test]
fn a_dropped_flag_falls_in_its_colour_and_its_team_recovers_it() {
    let mut g = Game::new("drop");
    let (red, blue) = two_teams(&mut g);
    let owner = g.s.minigame_views()[0].owner;
    g.set(owner, &[(&key(SLAYER, "mode"), Value::Text(CTF_MODE.into()))]);
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
    let drops: Vec<_> =
        g.s.weapon_view()
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
    // It counts down the seconds until it goes home, in red, over itself.
    let name = |g: &Game| {
        g.s.weapon_view()
            .drops
            .into_iter()
            .find(|d| d.item == FLAG_ITEM)
            .and_then(|d| d.name)
    };
    g.steps(30);
    let first = name(&g).expect("named");
    assert_eq!(first.color, RED);
    assert_eq!(first.text, "7", "the stand-in's Dropped Flag Respawn Time");
    g.steps(120);
    let later = name(&g).expect("named");
    assert_eq!(
        later.text.parse::<i64>().unwrap(),
        first.text.parse::<i64>().unwrap() - 1,
        "{first:?} then {later:?}"
    );

    // Red walks onto it: recovered, home, and points for Red.
    g.steps(120);
    let at =
        g.s.weapon_view()
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

fn key(rules: &str, key: &str) -> String {
    format!("{rules}:{key}")
}

/// Two teams playing Capture the Flag, with a flag each: (red, blue, red
/// flag, blue flag).
fn capture_the_flag(g: &mut Game, settings: &[(&str, Value)]) -> (OwnerId, OwnerId, BrickId, BrickId) {
    let (red, blue) = two_teams(g);
    let owner = g.s.minigame_views()[0].owner;
    let mode = key(SLAYER, "mode");
    let mut all = vec![(mode.as_str(), Value::Text(CTF_MODE.into()))];
    all.extend(settings.iter().cloned());
    g.set(owner, &all);
    let red_flag = g.plant(owner, FLAG, -8.5, 0.0, RED);
    let blue_flag = g.plant(owner, FLAG, 8.5, 0.0, BLUE);
    g.steps(31);
    (red, blue, red_flag, blue_flag)
}
fn capture(g: &mut Game, blue: OwnerId) {
    g.goto(blue, Vec3::new(-8.5, 0.25, 0.25));
    g.settle();
    assert!(g.carried(blue).is_some(), "carrying the red flag");
    g.goto(blue, Vec3::new(8.5, 0.25, 0.25));
    g.steps(30);
}

#[test]
fn enough_captures_win_the_round_and_slayer_resets_it() {
    let mut g = Game::new("ctf-win");
    let to_win = key(CTF, "flag_returns_to_win");
    let (red, blue, red_flag, _) = capture_the_flag(&mut g, &[(&to_win, Value::Int(1))]);
    g.s.take_private_notices();
    capture(&mut g, blue);
    g.steps(13);
    // Slayer's end of round: the winner announced, everyone out and their
    // camera on their own body, the reset counted down.
    assert!(g.heard("won this round with a score of 25 points"));
    assert!(g.round_over());
    for p in [red, blue] {
        assert_eq!(g.s.control(p), Some(watching(p)));
        assert!(g.cmd(p, Command::WeaponTrigger { down: true }).is_err());
        assert!(g.cmd(p, Command::ControlPlayer).is_err(), "no clicking out of it");
    }
    // No more captures until the reset.
    assert_eq!(g.flag_on(red_flag), Some(Some(RED)));
    g.steps(BETWEEN_ROUNDS * 120);
    assert!(!g.round_over());
    for p in [red, blue] {
        assert_eq!(g.s.control(p), Some(ControlObject::Player));
        assert_eq!(g.score(p), 0);
    }
    g.quiet();
}

#[test]
fn the_points_to_win_end_a_round_too() {
    let mut g = Game::new("ctf-points");
    let to_win = key(CTF, "flag_returns_to_win");
    let points = key(SLAYER, "points");
    let (_, blue, _, _) =
        capture_the_flag(&mut g, &[(&to_win, Value::Int(0)), (&points, Value::Int(20))]);
    g.s.take_private_notices();
    capture(&mut g, blue);
    g.steps(13);
    assert!(g.heard("won this round with a score of 25 points"));
    assert!(g.round_over());
    g.quiet();
}

#[test]
fn a_team_out_of_lives_loses_and_the_next_round_counts_down() {
    let mut g = Game::new("lives");
    let (red, blue) = two_teams(&mut g);
    let owner = g.s.minigame_views()[0].owner;
    g.set(
        owner,
        &[
            (&key(SLAYER, "lives"), Value::Int(1)),
            (&key(SLAYER, "pre_round_seconds"), Value::Int(2)),
        ],
    );
    g.s.take_private_notices();
    g.cmd(red, Command::Suicide).unwrap();
    g.steps(13);
    // Red's last life: out, no respawn, and Blue is the last team standing.
    assert!(g.s.vitals()[&red].respawn_held);
    g.steps(600);
    assert!(g.cmd(red, Command::Respawn).is_err());
    assert!(g.heard("won this round"));
    assert_eq!(g.s.control(blue), Some(watching(blue)));

    // The reset brings everyone back, standing still through the countdown.
    g.steps(BETWEEN_ROUNDS * 120 - 600);
    assert!(g.s.vitals()[&red].alive);
    assert!(!g.round_over());
    // Frozen (`PlayerFrozenArmor`): no moving, no tools, the camera behind.
    let frozen = g.s.archetypes().resolve(g.s.archetypes().find(FROZEN).unwrap()).clone();
    assert_eq!(g.body(red), FROZEN);
    assert!(!frozen.uses_items && frozen.look.third_person_only);
    assert_eq!((frozen.movement.forward, frozen.movement.jump_speed, frozen.movement.can_jet), (0.0, 0.0, false));
    let refused = |g: &mut Game| match g.cmd(red, Command::EquipTool { slot: Some(0) }) {
        Err(e) => e.to_string().contains("cannot use items"),
        Ok(_) => false,
    };
    assert!(refused(&mut g));
    assert!(g.heard("1 life - The last team standing wins."));
    g.steps(2 * 120 + 13);
    for p in [red, blue] {
        assert_eq!(g.body(p), "v20.player.playerstandardarmor", "thawed at GO");
    }
    assert!(!refused(&mut g), "tools come back at GO");
    // A voice for each second, then the buzzer, for each player.
    let sounds: Vec<_> = g
        .s
        .take_private_notices()
        .into_iter()
        .filter_map(|(p, n)| match n {
            Notice::Sound(s) if p == red => Some(s),
            _ => None,
        })
        .collect();
    assert_eq!(
        sounds,
        [
            "gamemode_slayer:sound/slayer_2_seconds_sound",
            "gamemode_slayer:sound/slayer_1_seconds_sound",
            "gamemode_slayer:sound/slayer_begin_sound"
        ]
    );
    g.quiet();
}

#[test]
fn a_time_limit_counts_down_and_ends_the_round() {
    let mut g = Game::new("time");
    let a =
        g.s.join("Alpha".into(), Vec3::new(-2.0, 0.05, 20.0), false)
            .unwrap();
    g.cmd(
        a,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: bri_minigames::Settings {
                loadout: Default::default(),
                ..Default::default()
            },
        }),
    )
    .unwrap();
    g.set(a, &[(&key(SLAYER, "time"), Value::Int(1))]);
    g.s.take_private_notices();
    g.steps(30 * 120 + 13);
    assert!(g.heard("30 seconds remaining."));
    g.steps(30 * 120);
    // Nobody scored: nobody won.
    assert!(g.heard("Nobody won this round."));
    assert!(g.round_over());
    g.quiet();
}

#[test]
fn the_mini_game_window_sets_up_teams_and_their_settings() {
    let mut g = Game::new("window");
    let a =
        g.s.join("Alpha".into(), Vec3::new(-2.0, 0.05, 20.0), false)
            .unwrap();
    let b =
        g.s.join("Bravo".into(), Vec3::new(2.0, 0.05, 20.0), false)
            .unwrap();
    g.cmd(
        a,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: bri_minigames::Settings {
                loadout: Default::default(),
                ..Default::default()
            },
        }),
    )
    .unwrap();
    let game = g.s.minigame_views()[0].id;
    g.cmd(b, Command::MiniGame(MiniGameRequest::Join { game }))
        .unwrap();
    g.steps(2);
    // The menu lists Slayer's settings under the Add-On players know.
    let menu = g.s.addon_settings();
    let mode = menu.iter().find(|s| s.key() == key(SLAYER, "mode")).unwrap();
    assert_eq!(mode.package_name, "Stand-in Slayer");
    assert_eq!(mode.items.len(), 3, "Capture the Flag joins the modes");
    let lives = key(SLAYER, "team_lives");
    let team = |name: &str, color: u8, settings: Vec<SettingEdit>| TeamEdit {
        id: None,
        name: name.into(),
        color,
        settings,
    };
    // The mode and both teams in one Apply; a player who is not the owner
    // cannot.
    let request = MiniGameRequest::AddOnSettings {
        game,
        settings: vec![SettingEdit {
            key: key(SLAYER, "mode"),
            value: Some(Value::Text(TEAM_MODE.into())),
        }],
        teams: Some(vec![
            team(
                "Red",
                RED,
                vec![SettingEdit {
                    key: lives.clone(),
                    value: Some(Value::Int(3)),
                }],
            ),
            team("Blue", BLUE, vec![]),
        ]),
    };
    assert!(g.cmd(b, Command::MiniGame(request.clone())).is_err());
    g.cmd(a, Command::MiniGame(request)).unwrap();
    g.steps(13);
    let view = g.s.minigame_views()[0].clone();
    assert_eq!(view.teams.len(), 2);
    assert_eq!(view.teams[0].addon_settings.get(&lives), Some(&Value::Int(3)));
    // Slayer sorted both players, one a side.
    let (ca, cb) = (g.colour(a), g.colour(b));
    assert!(ca.is_some() && cb.is_some() && ca != cb, "{ca:?} {cb:?}");
    // A bad value is refused whole.
    let bad = MiniGameRequest::AddOnSettings {
        game,
        settings: vec![SettingEdit {
            key: key(SLAYER, "lives"),
            value: Some(Value::Int(1000)),
        }],
        teams: None,
    };
    assert!(g.cmd(a, Command::MiniGame(bad)).is_err());
    // `/teams friendlyfire on` is the same setting the window shows.
    g.teams(a, "friendlyfire on");
    let view = g.s.minigame_views()[0].clone();
    assert_eq!(
        view.addon_settings.get(&key(SLAYER, "friendly_fire")),
        Some(&Value::Bool(true))
    );
    g.quiet();
}

#[test]
fn standing_on_a_capture_point_fills_its_bar_and_captures_it() {
    let mut g = Game::new("cp");
    let (red, blue) = two_teams(&mut g);
    let owner = g.s.minigame_views()[0].owner;
    let neutral = 2;
    let cp = g.plant(owner, CP, 0.0, 8.0, neutral);
    let colour = |g: &Game| g.s.simulation().state().bricks[&cp].color;
    let bars = |g: &mut Game| -> Vec<String> {
        g.s.take_private_notices()
            .into_iter()
            .filter_map(|(_, n)| match n {
                Notice::Bottom { text, .. } if text.contains('_') => Some(text),
                _ => None,
            })
            .collect()
    };
    let on = Vec3::new(0.0, 0.25, 8.25);
    let away = Vec3::new(0.0, 0.25, 20.0);
    g.s.take_private_notices();

    // Blue stands on it: its bar fills a step each tick, then it is Blue's.
    g.goto(blue, on);
    g.steps(CP_TICK * 2 + 1);
    let shown = bars(&mut g);
    assert!(
        shown.iter().any(|b| b.matches('_').count() == 3 && b.matches("<color:").count() == 2),
        "a part-filled bar: {shown:?}"
    );
    g.steps(CP_TICK * 3);
    assert_eq!(colour(&g), BLUE, "captured in Blue's colour");
    assert_eq!(g.score(blue), CP_POINTS);

    // Red starts to take it and walks off: the bar eases back, and the
    // point shows Blue's colour again once it is empty.
    g.goto(blue, away);
    g.goto(red, on);
    g.steps(CP_TICK * 2 + 1);
    assert_eq!(g.score(red), 0);
    g.goto(red, away);
    g.steps(120 + CP_TICK * 4);
    assert_eq!(colour(&g), BLUE);

    // Red stays: from an empty bar again, Red's after a full one.
    g.goto(red, on);
    g.steps(CP_TICK * 3 + 1);
    assert_eq!(g.score(red), 0, "the bar started over");
    g.steps(CP_TICK * 2);
    assert_eq!(colour(&g), RED);
    assert_eq!(g.score(red), CP_POINTS);

    // A reset gives it back to the colour it was built in.
    g.cmd(owner, Command::MiniGame(MiniGameRequest::Reset)).unwrap();
    g.steps(2);
    assert_eq!(colour(&g), neutral);

    // Tick Time slows every point's trigger (`tickPeriodMS`).
    g.set(owner, &[(&key(SLAYER, "cp_tick_ms"), Value::Int(500))]);
    g.goto(red, away);
    g.goto(blue, on);
    g.steps(CP_TICK * 5 + 1);
    assert_eq!(g.score(blue), 0, "not yet (the reset cleared scores)");
    g.steps(60 * 5);
    assert_eq!(g.score(blue), CP_POINTS);
    g.quiet();
}

/// Three players in Alpha's free-for-all mini-game.
fn three_players(g: &mut Game) -> [OwnerId; 3] {
    let players = ["Alpha", "Bravo", "Charlie"].map(|name| {
        g.s.join(name.into(), Vec3::new(0.0, 0.05, 20.0), false)
            .unwrap()
    });
    g.cmd(
        players[0],
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: bri_minigames::Settings {
                loadout: Default::default(),
                ..Default::default()
            },
        }),
    )
    .unwrap();
    let game = g.s.minigame_views()[0].id;
    for &p in &players[1..] {
        g.cmd(p, Command::MiniGame(MiniGameRequest::Join { game }))
            .unwrap();
    }
    g.steps(2);
    players
}

/// The stand-in's spectating numbers: spectating 3 s after the last life,
/// keys ignored for 0.5 s after death, the auto camera gliding at 2 units a
/// second from 4 units out to 1 at 100 degrees.
const SPECTATE_TICKS: usize = 360;
/// A rule's `watch`: the frozen orbit camera at the corpse camera's 8 units.
fn watching(target: OwnerId) -> ControlObject {
    ControlObject::Orbit {
        target,
        min: 8,
        max: 8,
        distance: 8,
        body: OrbitBody::Frozen,
    }
}
const AUTO_FOV: f32 = 100.0;

#[test]
fn a_player_out_of_lives_spectates_and_changes_cameras() {
    let mut g = Game::new("spectate");
    let [a, b, c] = three_players(&mut g);
    g.set(a, &[(&key(SLAYER, "lives"), Value::Int(1))]);
    // Bravo and Charlie stand apart, facing away from the map's spawn.
    g.goto(b, Vec3::new(-10.0, 0.05, 0.0));
    g.goto(c, Vec3::new(10.0, 0.05, 0.0));
    g.cmd(a, Command::Suicide).unwrap();
    g.steps(13);
    assert!(g.s.vitals()[&a].respawn_held);
    // Keys in the first moments after death do nothing.
    g.cmd(a, Command::ObserverButton(ObserverButton::Jump)).unwrap();
    assert_eq!(g.s.control(a), Some(ControlObject::Corpse));
    // A living player can't send a spectator's keys.
    assert!(g.cmd(b, Command::ObserverButton(ObserverButton::Fire)).is_err());

    // Three seconds on: orbiting the first living player, then fire steps
    // to the next and jet back.
    g.steps(SPECTATE_TICKS);
    assert_eq!(g.s.control(a), Some(watching(b)));
    let press = |g: &mut Game, key| {
        g.cmd(a, Command::ObserverButton(key)).unwrap();
        g.steps(1);
    };
    press(&mut g, ObserverButton::Fire);
    assert_eq!(g.s.control(a), Some(watching(c)));
    press(&mut g, ObserverButton::Jet);
    assert_eq!(g.s.control(a), Some(watching(b)));

    // Jump changes the mode: a free camera, then the auto camera gliding
    // in over the next player's shoulder at a wide angle.
    press(&mut g, ObserverButton::Jump);
    assert_eq!(g.s.control(a), Some(ControlObject::Observer));
    g.s.take_private_notices();
    press(&mut g, ObserverButton::Jump);
    assert_eq!(g.s.control(a), Some(ControlObject::Path));
    let glide = g.s.vitals()[&a].camera_path.clone().expect("the glide replicates");
    assert_eq!(glide.knots.len(), 2);
    let [from, to] = [glide.knots[0].view.eye(), glide.knots[1].view.eye()];
    let target = g.feet(c);
    let flat = |v: Vec3| Vec3::new(v.x, 0.0, v.z);
    assert!((flat(from - target).length() - 4.0).abs() < 0.01, "{from} {target}");
    assert!((flat(to - target).length() - 1.0).abs() < 0.01, "{to} {target}");
    assert!(
        g.s.take_private_notices()
            .iter()
            .any(|(p, n)| *p == a && matches!(n, Notice::Fov(Some(f)) if *f == AUTO_FOV)),
        "the auto camera widens the view"
    );
    // Fire does nothing to the auto camera; at the end of its glide it
    // moves on to the next player.
    press(&mut g, ObserverButton::Fire);
    assert_eq!(g.s.control(a), Some(ControlObject::Path));
    g.steps(glide.duration_ticks() as usize + 2);
    let next = g.s.vitals()[&a].camera_path.clone().unwrap();
    assert_ne!(next.start_tick, glide.start_tick);
    assert!((flat(next.knots[1].view.eye() - g.feet(b)).length() - 1.0).abs() < 0.01);

    // The light key leaves it for the orbit camera again.
    press(&mut g, ObserverButton::Light);
    assert!(matches!(
        g.s.control(a),
        Some(ControlObject::Orbit {
            body: OrbitBody::Frozen,
            ..
        })
    ));
    assert!(g.s.vitals()[&a].camera_path.is_none());

    // A new round brings them back to their body.
    g.run(a, SLAYER, "slayer", vec![PackageArg::String("reset".into())]);
    g.steps(3);
    assert!(g.s.vitals()[&a].alive);
    assert_eq!(g.s.control(a), Some(ControlObject::Player));
    g.quiet();
}

#[test]
fn the_fly_through_camera_flies_everyone_before_the_round() {
    let mut g = Game::new("flythrough");
    let [a, b, _] = three_players(&mut g);
    g.set(a, &[(&key(SLAYER, "pre_round_seconds"), Value::Int(2))]);
    g.steps(121);
    let knot = |g: &mut Game, command: &str, line: &str| {
        g.run(a, SLAYER, command, vec![PackageArg::String(line.into())]);
        g.steps(13);
    };
    // Only the game's owner (or an admin) lays the path.
    g.run(b, SLAYER, "createflycam", vec![]);
    g.steps(13);
    g.run(a, SLAYER, "createflycam", vec![]);
    g.steps(13);
    g.goto(a, Vec3::new(0.0, 0.05, 0.0));
    g.steps(2);
    knot(&mut g, "setknot", "20 Normal Linear");
    g.goto(a, Vec3::new(10.0, 0.05, 0.0));
    g.steps(2);
    knot(&mut g, "setknot", "20 Normal Linear");
    g.goto(a, Vec3::new(10.0, 0.05, 30.0));
    g.steps(2);
    knot(&mut g, "setjump", "");

    // Testing it flies the owner alone and hands their camera back.
    g.run(a, SLAYER, "testflycam", vec![]);
    g.steps(2);
    assert_eq!(g.s.control(a), Some(ControlObject::Path));
    assert_eq!(g.s.control(b), Some(ControlObject::Player));
    let path = g.s.vitals()[&a].camera_path.clone().unwrap();
    assert_eq!(path.knots.len(), 3);
    // Ten units at twenty a second (half a second, give or take where the
    // body settled), then the jump cuts to the last knot.
    assert!((60..=61).contains(&path.duration_ticks()), "{}", path.duration_ticks());
    g.steps(64);
    assert_eq!(g.s.control(a), Some(ControlObject::Player));

    // A reset flies every member first; the countdown waits for it.
    g.run(a, SLAYER, "slayer", vec![PackageArg::String("reset".into())]);
    g.steps(3);
    for p in [a, b] {
        assert_eq!(g.s.control(p), Some(ControlObject::Path));
        assert_eq!(g.body(p), "v20.player.playerstandardarmor");
    }
    g.steps(64);
    for p in [a, b] {
        assert_eq!(g.s.control(p), Some(ControlObject::Player));
        assert_eq!(g.body(p), FROZEN, "the countdown after the fly-through");
    }

    // Without the camera a reset goes straight to the countdown.
    g.run(a, SLAYER, "deleteflycam", vec![]);
    // Past the engine's five seconds between resets.
    g.steps(5 * 120 + 13);
    g.run(a, SLAYER, "slayer", vec![PackageArg::String("reset".into())]);
    g.steps(3);
    assert_eq!(g.s.control(b), Some(ControlObject::Player));
    assert_eq!(g.body(b), FROZEN);
    g.quiet();
}

/// A wrench row on `input` aiming at `slot` (`SelfBrick`, `Client`,
/// `MiniGame`).
fn event(input: &str, slot: &str, output: &str, params: Vec<EventValue>) -> EventRow {
    EventRow {
        preserved: None,
        enabled: true,
        input: input.into(),
        delay_ms: 0,
        target: EventTarget::Slot(serde_json::from_value(serde_json::json!(slot)).unwrap()),
        output: output.into(),
        params,
    }
}

/// A wrench row on `input` aiming at one of Slayer's own targets,
/// `Team(Client)` or `Team(Brick)`.
fn team_event(input: &str, target: &str, output: &str, params: Vec<EventValue>) -> EventRow {
    EventRow {
        target: EventTarget::Derived(target.into()),
        ..event(input, "SelfBrick", output, params)
    }
}

/// Wrench events on: the host's own catalog has only the two inputs
/// Slayer's team inputs follow and one mini-game output Slayer restricts,
/// so the Add-Ons' inputs and outputs are nearly all there is.
fn with_events(g: &mut Game) {
    let input = |name: &str| {
        serde_json::json!({
            "id": format!("in/{name}"), "class_name": "fxDTSBrick", "name": name,
            "targets": [["Self", "fxDTSBrick"], ["Player", "Player"],
                        ["Client", "GameConnection"], ["MiniGame", "MiniGame"]],
            "source": "test", "source_line": 1
        })
    };
    let catalog = serde_json::json!({
        "schema_version": 1, "inputs": [input("onActivate"), input("onPlayerTouch")],
        "outputs": [{
            "id": "out/MiniGame/BottomPrintAll", "class_name": "MiniGame", "name": "BottomPrintAll",
            "params": [{ "type": "string", "max_length": 200, "width": 156 },
                       { "type": "int", "min": 1, "max": 10, "default": 3 }, { "type": "bool" }],
            "append_client": false, "source": "test", "source_line": 1
        }],
        "sources": [], "scope": null
    });
    g.s.set_event_catalog(serde_json::from_value(catalog).unwrap(), Vec::new())
        .unwrap();
}
/// A number in Slayer's state of `p` that everyone sees.
fn stat(g: &Game, p: OwnerId, key: &str) -> i64 {
    g.s.package_state().packages[SLAYER]
        .players
        .get(&p)
        .and_then(|state| state.get(key))
        .map_or(0, |v| v.as_i64().unwrap())
}
/// `p` sets off the probe's `onPoke` rows on `brick`.
fn poke(g: &mut Game, p: OwnerId, brick: BrickId) {
    g.run(p, "probe", "poke", vec![PackageArg::Int(brick as i64)]);
    g.steps(2);
}

#[test]
fn slayers_wrench_events_check_teams_hold_bricks_and_win_rounds() {
    let mut g = Game::new("events");
    with_events(&mut g);
    let (red, blue) = two_teams(&mut g);
    let owner = g.s.minigame_views()[0].owner;
    let text = |t: &str| EventValue::Text(t.into());

    // `checkTeam`: Red is on Red, so onTeamCheckTrue runs, but only the
    // rows its range "1 2" names; Blue is not, so onTeamCheckFalse.
    let checker = g.plant(owner, TEAM_SPAWN, 10.0, 0.0, 2);
    let rows = vec![
        event("onPoke", "SelfBrick", "checkTeam", vec![EventValue::Int(0), text("Red"), text("1 2")]),
        event("onTeamCheckTrue", "Client", "addKills", vec![EventValue::Int(3)]),
        event("onTeamCheckFalse", "Client", "addDeaths", vec![EventValue::Int(4)]),
        event("onTeamCheckTrue", "Client", "addDeaths", vec![EventValue::Int(5)]),
    ];
    g.s.edit_brick(owner, checker, Edit::Events(rows)).unwrap();
    poke(&mut g, red, checker);
    poke(&mut g, blue, checker);
    assert_eq!(stat(&g, red, "kills"), 3);
    assert_eq!(stat(&g, red, "deaths"), 0, "row 3 is past the range");
    assert_eq!(stat(&g, blue, "deaths"), 4);
    assert_eq!(stat(&g, blue, "kills"), 0);

    // `setTeamControl`: a Red team spawn turned over to Blue spawns Blue.
    let at = Vec3::new(-15.5, 0.2, 0.25);
    let spawn = g.plant(owner, TEAM_SPAWN, at.x, 0.0, RED);
    let rows = vec![event("onPoke", "SelfBrick", "setTeamControl", vec![EventValue::Color(BLUE)])];
    g.s.edit_brick(owner, spawn, Edit::Events(rows)).unwrap();
    poke(&mut g, blue, spawn);
    g.cmd(blue, Command::Suicide).unwrap();
    g.steps(125);
    g.cmd(blue, Command::Respawn).unwrap();
    g.steps(2);
    let feet = g.feet(blue);
    assert!(
        Vec3::new(feet.x - at.x, 0.0, feet.z - at.z).length() < 1.0,
        "Blue spawned at {feet}"
    );

    // `setTeamControlLocked`: Blue's colour may not capture the point.
    let cp = g.plant(owner, CP, 0.0, 8.0, 2);
    let rows = vec![event(
        "onPoke",
        "SelfBrick",
        "setTeamControlLocked",
        vec![EventValue::Int(1), EventValue::Color(BLUE), EventValue::Bool(true)],
    )];
    g.s.edit_brick(owner, cp, Edit::Events(rows)).unwrap();
    poke(&mut g, blue, cp);
    g.s.take_private_notices();
    g.goto(blue, Vec3::new(0.0, 0.25, 8.25));
    g.steps(CP_TICK * 8);
    let notices = g.s.take_private_notices();
    assert!(
        notices.iter().any(|(_, n)| matches!(n, Notice::Bottom { text, .. } if text.contains("Locked for now."))),
        "{notices:?}"
    );
    assert_eq!(g.s.simulation().state().bricks[&cp].color, 2, "not captured");

    // `incTimeRemaining` and `Win`, on the mini-game.
    let game = g.plant(owner, TEAM_SPAWN, 12.0, 4.0, 2);
    let rows = vec![
        event("onPoke", "MiniGame", "incTimeRemaining", vec![EventValue::Int(2), EventValue::Bool(true)]),
        event("onPoke", "MiniGame", "Win", vec![EventValue::Int(4), text("The Builders")]),
    ];
    g.s.edit_brick(owner, game, Edit::Events(rows)).unwrap();
    g.s.take_private_notices();
    poke(&mut g, red, game);
    assert!(g.heard("Extended by 2 minutes."));
    g.steps(2);
    assert!(g.round_over());
    g.quiet();
}

#[test]
fn the_drop_flag_event_drops_the_flag_and_fires_its_input() {
    let mut g = Game::new("drop-event");
    with_events(&mut g);
    let (_red, blue, red_flag, _) = capture_the_flag(&mut g, &[]);
    let owner = g.s.minigame_views()[0].owner;
    let poker = g.plant(owner, TEAM_SPAWN, 12.0, 4.0, 2);
    let rows = vec![event("onPoke", "Player", "DropFlag", vec![])];
    g.s.edit_brick(owner, poker, Edit::Events(rows)).unwrap();
    // The flag's own brick hears it was dropped, and counts it on Blue.
    let rows = vec![event("onFlagDropped", "Client", "addKills", vec![EventValue::Int(1)])];
    g.s.edit_brick(owner, red_flag, Edit::Events(rows)).unwrap();
    g.goto(blue, Vec3::new(-8.5, 0.25, 0.25));
    g.settle();
    assert!(g.carried(blue).is_some());
    g.s.take_private_notices();
    poke(&mut g, blue, poker);
    assert_eq!(g.carried(blue), None);
    assert!(g.heard("dropped the"));
    assert_eq!(stat(&g, blue, "kills"), 1);
    g.quiet();
}

/// Bottom prints `p` was sent since the last look that contain `text`.
/// The reports each player was shown or had closed since the last look.
fn reports(g: &mut Game) -> Vec<(OwnerId, Option<bri_package_runtime::report::Report>)> {
    g.s.take_private_notices()
        .into_iter()
        .filter_map(|(p, n)| match n {
            Notice::Report(r) => Some((p, r.map(|r| *r))),
            _ => None,
        })
        .collect()
}
fn row<'a>(
    report: &'a bri_package_runtime::report::Report,
    section: usize,
    key: &str,
) -> &'a bri_package_runtime::report::ReportRow {
    report.sections[section]
        .rows
        .iter()
        .find(|r| r.key == key)
        .unwrap_or_else(|| panic!("no row {key} in {report:?}"))
}
fn cell<'a>(row: &'a bri_package_runtime::report::ReportRow, column: &str) -> &'a str {
    row.cells.get(column).map_or("", String::as_str)
}

#[test]
fn the_end_of_round_report_shows_teams_and_players_with_flag_columns() {
    let mut g = Game::new("ctf-report");
    let to_win = key(CTF, "flag_returns_to_win");
    let (red, blue, _, _) = capture_the_flag(&mut g, &[(&to_win, Value::Int(1))]);
    g.s.take_private_notices();
    capture(&mut g, blue);
    g.steps(13);
    // `sendScoreListAll` to every member, with Capture the Flag's columns
    // in place of Kills and Deaths (`scoreListInit`), whichever Add-On's
    // round-end hook ran first.
    let shown = reports(&mut g);
    assert_eq!(shown.len(), 2, "{shown:?}");
    let of = |p: OwnerId| {
        shown
            .iter()
            .find(|(o, _)| *o == p)
            .and_then(|(_, r)| r.clone())
            .expect("a report")
    };
    let (won, lost) = (of(blue), of(red));
    assert_eq!(won.title, "End of Round Report");
    assert_eq!(won.banner.as_deref(), Some("VICTORY"));
    assert_eq!(lost.banner.as_deref(), Some("DEFEAT"));
    let titles: Vec<&str> = won.columns.iter().map(|c| c.title.as_str()).collect();
    assert_eq!(titles, ["Score", "Flag Pick-ups", "Flag Returns", "Rounds Won"]);
    assert_eq!(won.sections[0].title, "Teams:");
    assert_eq!(won.sections[1].title, "Players:");
    let teams: Vec<&str> = won.sections[0].rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(teams, ["Blue", "Red"], "highest score first");
    let blue_team = &won.sections[0].rows[0];
    assert_eq!(blue_team.color, Some(BLUE));
    assert_eq!(cell(blue_team, "score"), CAPTURE_POINTS.to_string());
    assert_eq!(cell(blue_team, "kills"), "1", "one flag taken from its stand");
    assert_eq!(cell(blue_team, "deaths"), "1", "one flag returned");
    assert_eq!(cell(blue_team, "wins"), "1");
    assert_eq!(cell(&won.sections[0].rows[1], "kills"), "", "none is blank");
    let me = row(&won, 1, &format!("player:{blue}"));
    assert_eq!(won.sections[1].rows[0].key, me.key, "highest score first");
    assert_eq!(cell(me, "kills"), "1");
    assert_eq!(cell(me, "deaths"), "1");
    assert_eq!(cell(me, "wins"), "", "a team player's wins are the team's");
    assert_eq!(me.color, Some(BLUE));
    // The reset closes it; the tallies start again.
    g.steps(BETWEEN_ROUNDS * 120);
    let closed = reports(&mut g);
    for p in [red, blue] {
        assert!(closed.iter().any(|(o, r)| *o == p && r.is_none()), "{closed:?}");
    }
    // Without Victory/Defeat or Team Scores: one plain list.
    let owner = g.s.minigame_views()[0].owner;
    let victory = key(SLAYER, "eorr_display_victory");
    let team_scores = key(SLAYER, "eorr_display_team_scores");
    g.set(owner, &[(&victory, Value::Bool(false)), (&team_scores, Value::Bool(false))]);
    g.steps(31);
    g.s.take_private_notices();
    capture(&mut g, blue);
    g.steps(13);
    let shown = reports(&mut g);
    let (_, report) = shown.iter().find(|(o, _)| *o == red).expect("a report");
    let report = report.as_ref().expect("shown");
    assert_eq!(report.banner, None);
    assert_eq!(report.sections.len(), 1);
    assert_eq!(report.sections[0].title, "");
    assert_eq!(cell(row(report, 0, &format!("player:{blue}")), "kills"), "1");
    // With the report off, nobody is shown one.
    let enable = key(SLAYER, "eorr_enable");
    g.steps(BETWEEN_ROUNDS * 120);
    g.set(owner, &[(&enable, Value::Bool(false))]);
    g.steps(31);
    reports(&mut g);
    capture(&mut g, blue);
    g.steps(13);
    assert!(reports(&mut g).iter().all(|(_, r)| r.is_none()));
    g.quiet();
}

fn bottom_printed(g: &mut Game, text: &str) -> bool {
    g.s.take_private_notices()
        .iter()
        .any(|(_, n)| matches!(n, Notice::Bottom { text: t, .. } if t.contains(text)))
}

#[test]
fn a_locked_flag_cannot_be_taken_nor_a_flag_returned_to_a_locked_stand() {
    let mut g = Game::new("locked-flag");
    with_events(&mut g);
    let (_red, blue, red_flag, blue_flag) = capture_the_flag(&mut g, &[]);
    let owner = g.s.minigame_views()[0].owner;
    let lock = |mode: i64, color: u8, on: bool| {
        vec![event(
            "onPoke",
            "SelfBrick",
            "setTeamControlLocked",
            vec![EventValue::Int(mode), EventValue::Color(color), EventValue::Bool(on)],
        )]
    };
    // Slayer's setTeamControlLocked on the red flag, for Blue's colour.
    g.s.edit_brick(owner, red_flag, Edit::Events(lock(1, BLUE, true))).unwrap();
    poke(&mut g, blue, red_flag);
    g.s.take_private_notices();
    g.goto(blue, Vec3::new(-8.5, 0.25, 0.25));
    g.steps(30);
    assert_eq!(g.carried(blue), None, "locked for Blue");
    assert!(bottom_printed(&mut g, "That flag is locked for now."));
    assert_eq!(g.flag_on(red_flag), Some(Some(RED)));

    // Unlocked, Blue takes it.
    g.goto(blue, Vec3::new(0.0, 0.25, 0.0));
    g.steps(4);
    g.s.edit_brick(owner, red_flag, Edit::Events(lock(1, BLUE, false))).unwrap();
    poke(&mut g, blue, red_flag);
    g.goto(blue, Vec3::new(-8.5, 0.25, 0.25));
    g.settle();
    assert!(g.carried(blue).is_some());

    // Blue locks their own flag for their team (mode 0): no capture there.
    g.s.edit_brick(owner, blue_flag, Edit::Events(lock(0, 0, true))).unwrap();
    poke(&mut g, blue, blue_flag);
    g.s.take_private_notices();
    g.goto(blue, Vec3::new(8.5, 0.25, 0.25));
    g.steps(30);
    assert!(g.carried(blue).is_some(), "still carrying");
    assert_eq!(g.score(blue), 0);
    assert!(bottom_printed(&mut g, "is locked for now."));
    g.quiet();
}

#[test]
fn the_drop_tool_key_with_tools_put_away_drops_a_carried_flag() {
    let mut g = Game::new("drop-key");
    let (_red, blue, _, _) = capture_the_flag(&mut g, &[]);
    g.s.take_private_notices();
    g.goto(blue, Vec3::new(-8.5, 0.25, 0.25));
    g.settle();
    assert!(g.carried(blue).is_some());
    assert!(bottom_printed(&mut g, "Drop Tool"), "the pickup says how to drop it");
    // Enable Manual Flag Dropping off: the key does nothing.
    let owner = g.s.minigame_views()[0].owner;
    let manual = key(CTF, "manual_flag_drop");
    g.set(owner, &[(&manual, Value::Bool(false))]);
    g.cmd(blue, Command::DropKey).unwrap();
    g.steps(2);
    assert!(g.carried(blue).is_some());

    g.set(owner, &[(&manual, Value::Bool(true))]);
    g.s.take_private_notices();
    g.cmd(blue, Command::DropKey).unwrap();
    g.steps(2);
    assert_eq!(g.carried(blue), None);
    assert!(g.heard("dropped the"));
    g.quiet();
}

#[test]
fn team_events_message_respawn_and_score_a_whole_team() {
    let mut g = Game::new("team-events");
    with_events(&mut g);
    let (red, blue) = two_teams(&mut g);
    let owner = g.s.minigame_views()[0].owner;
    g.set(owner, &[(&key(SLAYER, "points"), Value::Int(10))]);
    let text = |t: &str| EventValue::Text(t.into());
    let catalog = g.s.event_catalog().unwrap();
    let targets = &catalog.input("onTeamCheckTrue").unwrap().targets;
    for name in ["Team(Client)", "Team(Brick)"] {
        assert!(
            targets.contains(&(name.into(), "Slayer_TeamSO".into())),
            "{targets:?}"
        );
    }
    assert!(
        !catalog
            .input("onCPReset")
            .unwrap()
            .targets
            .iter()
            .any(|(t, _)| t == "Team(Client)"),
        "onCPReset has no client"
    );

    // `Team(Client)`: whoever set it off's team hears it, with `%1` as
    // their name; `Team(Brick)`: the teams of the brick's colour.
    let board = g.plant(owner, TEAM_SPAWN, 10.0, 0.0, RED);
    let rows = vec![
        team_event("onPoke", "Team(Client)", "ChatMsgAll", vec![text("%1 rallies the team")]),
        team_event(
            "onPoke",
            "Team(Brick)",
            "BottomPrintAll",
            vec![text("Red holds the board"), EventValue::Int(3), EventValue::Bool(true)],
        ),
    ];
    g.s.edit_brick(owner, board, Edit::Events(rows)).unwrap();
    g.s.take_private_notices();
    poke(&mut g, blue, board);
    let notices = g.s.take_private_notices();
    let blue_name = g.s.names()[&blue].clone();
    let chat = format!("{blue_name} rallies the team");
    let heard = |who: OwnerId, f: &dyn Fn(&Notice) -> bool| notices.iter().any(|(o, n)| *o == who && f(n));
    let rallied = |n: &Notice| matches!(n, Notice::Chat(t) if *t == chat);
    let board_print = |n: &Notice| {
        matches!(n, Notice::Bottom { text, seconds, hide_bar }
            if text == "Red holds the board" && *seconds == 3.0 && *hide_bar)
    };
    assert!(heard(blue, &rallied), "{notices:?}");
    assert!(!heard(red, &rallied), "{notices:?}");
    assert!(heard(red, &board_print), "{notices:?}");
    assert!(!heard(blue, &board_print), "{notices:?}");

    // `RespawnAll`: Blue's team comes back, wherever it was.
    let blue_board = g.plant(owner, TEAM_SPAWN, 12.0, 4.0, BLUE);
    let rows = vec![team_event("onPoke", "Team(Brick)", "RespawnAll", vec![])];
    g.s.edit_brick(owner, blue_board, Edit::Events(rows)).unwrap();
    g.cmd(blue, Command::Suicide).unwrap();
    g.steps(2);
    assert!(!g.s.vitals()[&blue].alive);
    poke(&mut g, red, blue_board);
    assert!(g.s.vitals()[&blue].alive, "respawned");

    // `IncScore`: the team's own points count toward the points to win.
    let rows = vec![team_event("onPoke", "Team(Client)", "IncScore", vec![EventValue::Int(4)])];
    g.s.edit_brick(owner, board, Edit::Events(rows)).unwrap();
    g.s.take_private_notices();
    poke(&mut g, blue, board);
    poke(&mut g, blue, board);
    assert!(!g.round_over(), "8 points");
    poke(&mut g, blue, board);
    g.steps(2);
    assert!(g.heard("Blue"), "Blue won");
    assert!(g.round_over());
    g.quiet();
}

#[test]
fn team_and_mini_game_inputs_run_and_restricted_outputs_need_rights() {
    let mut g = Game::new("team-inputs");
    with_events(&mut g);
    let (red, blue) = two_teams(&mut g);
    let owner = g.s.minigame_views()[0].owner;
    let catalog = g.s.event_catalog().unwrap();
    let death = &catalog.input("onMinigameDeath").unwrap().targets;
    assert!(death.contains(&("Client(Killer)".into(), "GameConnection".into())), "{death:?}");

    // `onActivate(Team1)` for Red, the game's first team, and
    // `onActivate(Team2)` for Blue.
    let board = g.plant(owner, TEAM_SPAWN, 10.0, 0.0, 2);
    let rounds = g.plant(owner, TEAM_SPAWN, 12.0, 4.0, 2);
    let rows = vec![
        event("onActivate(Team1)", "Client", "addKills", vec![EventValue::Int(1)]),
        event("onActivate(Team2)", "Client", "addKills", vec![EventValue::Int(2)]),
        event("onMinigameDeath", "Client", "addKills", vec![EventValue::Int(3)]),
    ];
    g.s.edit_brick(owner, board, Edit::Events(rows)).unwrap();
    for p in [red, blue] {
        g.s.fire_brick_input(board, "onActivate", Some(p));
    }
    g.steps(2);
    assert_eq!((stat(&g, red, "kills"), stat(&g, blue, "kills")), (1, 2));

    // `onMinigameDeath` on every brick of the game, for whoever died.
    g.cmd(blue, Command::Suicide).unwrap();
    g.steps(2);
    assert_eq!(stat(&g, blue, "kills"), 2 + 3);

    // `onMinigameRoundEnd` and `onMinigameRoundStart`, on the mini-game.
    let rows = vec![
        event("onMinigameRoundEnd", "MiniGame", "incTimeRemaining", vec![EventValue::Int(1), EventValue::Bool(true)]),
        event("onMinigameRoundStart", "MiniGame", "setTimeRemaining", vec![EventValue::Int(3), EventValue::Bool(true)]),
        event("onPoke", "MiniGame", "Win", vec![EventValue::Int(4), EventValue::Text("The Builders".into())]),
    ];
    g.s.edit_brick(owner, rounds, Edit::Events(rows)).unwrap();
    g.s.take_private_notices();
    poke(&mut g, red, rounds);
    g.steps(2);
    assert!(g.round_over());
    assert!(g.heard("Extended by 1 minute."));
    g.steps(BETWEEN_ROUNDS * 120 + 13);
    assert!(!g.round_over());
    assert!(g.heard("Time now 3 minutes."));

    // Restrict Output Events: rows only those who may edit the game add,
    // and the stand-in's `BottomPrintAll` (level 2) only an admin.
    let win = || event("onPoke", "MiniGame", "Win", vec![EventValue::Int(4), EventValue::Text("Me".into())]);
    let time = || event("onPoke", "MiniGame", "incTimeRemaining", vec![EventValue::Int(1), EventValue::Bool(true)]);
    let print = || {
        event("onPoke", "MiniGame", "BottomPrintAll", vec![EventValue::Text("Hi".into()), EventValue::Int(3), EventValue::Bool(false)])
    };
    let other = if owner == red { blue } else { red };
    let mut rows = vec![win(), time()];
    assert!(g.s.review_event_rows(other, rounds, &mut rows).is_empty(), "off by default here");
    assert_eq!(rows.len(), 2);
    g.set(owner, &[(&key(SLAYER, "restrict_output_events"), Value::Bool(true))]);
    let refused = g.s.review_event_rows(other, rounds, &mut rows);
    assert!(rows.is_empty(), "{rows:?}");
    assert_eq!(
        refused,
        [
            "You do not have permission to use the [MiniGame, Win] event.",
            "You do not have permission to use the [MiniGame, incTimeRemaining] event."
        ]
    );
    let mut rows = vec![win(), time(), print()];
    let refused = g.s.review_event_rows(owner, rounds, &mut rows);
    assert_eq!(rows, [win(), time()]);
    assert_eq!(refused, ["You do not have permission to use the [MiniGame, BottomPrintAll] event."]);

    // `onMinigameLeave` and `onMinigameJoin`, for whoever leaves or joins.
    let rows = vec![
        event("onMinigameLeave", "Client", "addDeaths", vec![EventValue::Int(4)]),
        event("onMinigameJoin", "Client", "addDeaths", vec![EventValue::Int(5)]),
    ];
    g.s.edit_brick(owner, board, Edit::Events(rows)).unwrap();
    let deaths = stat(&g, other, "deaths");
    g.cmd(other, Command::MiniGame(MiniGameRequest::Leave)).unwrap();
    g.steps(2);
    assert_eq!(stat(&g, other, "deaths"), deaths + 4);
    let game = g.s.minigame_views()[0].id;
    g.cmd(other, Command::MiniGame(MiniGameRequest::Join { game })).unwrap();
    g.steps(2);
    assert_eq!(stat(&g, other, "deaths"), deaths + 4 + 5);
    g.quiet();
}

/// The stand-in's skin colours (`Slayer_AiController.cs`) and its default.
const SKINS: [[f32; 4]; 5] = [
    [0.9, 0.8, 0.6, 1.0],
    [0.9, 0.8, 0.6, 1.0],
    [0.4, 0.3, 0.2, 1.0],
    [0.2, 0.1, 0.05, 1.0],
    [0.7, 0.5, 0.4, 1.0],
];
const STANDARD: &str = "v20.player.playerstandardarmor";

#[test]
fn teams_dress_their_members_and_give_them_their_kit() {
    let mut g = Game::new("uniforms");
    let own = {
        let a = g.s.join("Solo".into(), Vec3::new(0.0, 0.05, 25.0), false).unwrap();
        g.s.avatars()[&a].clone()
    };
    let (red, blue) = two_teams(&mut g);
    let owner = g.s.minigame_views()[0].owner;
    let palette = g.s.simulation().state().palette.clone();
    let (red_rgb, blue_rgb) = (palette[RED as usize], palette[BLUE as usize]);

    // The stand-in's teams wear the Custom uniform: each part by its place
    // in v20's list, TEAMCOLOR for the team's colour, its face and decal.
    let look = g.s.avatars()[&red].clone();
    let part = |slot: &str| look.parts.get(slot).map(String::as_str);
    assert_eq!(part("hat"), Some("helmet"));
    assert_eq!(part("accent"), Some("visor"), "a helmet's accent is its visor");
    assert_eq!(part("chest"), Some("femchest"));
    assert_eq!(part("pack"), Some("bucket"));
    assert_eq!(part("secondpack"), Some("epaulets"));
    assert_eq!(part("larm"), Some("larmslim"));
    assert_eq!(part("rarm"), Some("rarm"));
    assert_eq!(part("lhand"), Some("lhook"));
    assert_eq!(part("lleg"), Some("lpeg"));
    assert_eq!(part("hip"), Some("pants"));
    assert_eq!(look.colors["torso"], red_rgb);
    assert_eq!(look.colors["hat"], red_rgb);
    assert_eq!(look.colors["head"], [0.5, 0.25, 0.0, 1.0]);
    assert_eq!(look.colors["hip"], [0.0, 0.0, 1.0, 1.0]);
    assert_eq!(look.colors["secondpack"], [1.0, 1.0, 0.0, 1.0]);
    assert_eq!((look.face.as_str(), look.decal.as_str()), ("smileyEvil1", "Alyx"));
    assert_eq!(g.s.avatars()[&blue].colors["torso"], blue_rgb);
    // Allow Custom Face Decals keeps players' own faces.
    g.set(owner, &[(&key(SLAYER, "allow_custom_faces"), Value::Bool(true))]);
    assert_eq!(g.s.avatars()[&red].face, own.face);
    assert_eq!(g.s.avatars()[&red].decal, "Alyx");

    // Full: a cop hat and plain clothes in the team's colour, a skin
    // colour on the head and hands.
    g.set_team(owner, RED, &[(&key(SLAYER, "team_uniform"), Value::Int(2))]);
    let look = g.s.avatars()[&red].clone();
    assert_eq!(look.parts["hat"], "cophat");
    assert_eq!(look.parts["accent"], "none");
    assert_eq!(look.parts["pack"], "none");
    assert_eq!(look.parts["chest"], "chest");
    assert_eq!(look.parts["lleg"], "lshoe");
    for slot in ["hat", "torso", "hip", "larm", "rarm", "lleg", "rleg"] {
        assert_eq!(look.colors[slot], red_rgb, "{slot}");
    }
    let skin = look.colors["head"];
    assert!(SKINS.contains(&skin), "{skin:?}");
    assert_eq!((look.colors["lhand"], look.colors["rhand"]), (skin, skin));
    assert_eq!((look.face.as_str(), look.decal.as_str()), (own.face.as_str(), own.decal.as_str()));
    assert_eq!(g.s.avatars()[&blue].parts["hat"], "helmet", "Blue's own uniform");

    // Shirt Only: their own avatar, the torso and pack in the team's colour.
    g.set_team(owner, RED, &[(&key(SLAYER, "team_uniform"), Value::Int(1))]);
    let look = g.s.avatars()[&red].clone();
    assert_eq!(look.parts, own.parts);
    assert_eq!((look.colors["torso"], look.colors["pack"]), (red_rgb, red_rgb));
    assert_eq!(look.colors["hip"], own.colors["hip"]);

    // None: their own avatar.
    g.set_team(owner, RED, &[(&key(SLAYER, "team_uniform"), Value::Int(0))]);
    assert_eq!(g.s.avatars()[&red], own);

    // The stand-in's teams keep their own start tools, not the game's
    // (none here).
    let tools = g.tools(red);
    assert_eq!(
        &tools[..3],
        ["v20.weapon.hammeritem", "v20.weapon.wrenchitem", "v20.weapon.printgun"]
    );
    assert!(tools[3..].iter().all(String::is_empty), "{tools:?}");
    assert_eq!(g.body(red), STANDARD);

    // Changes reach members at once: a start tool where they still carry
    // the old one, the player type and the scale.
    g.set_team(
        owner,
        RED,
        &[
            (&key(SLAYER, "team_equip_1"), Value::Text(String::new())),
            (&key(SLAYER, "team_equip_3"), Value::Text("v20.weapon.wanditem".into())),
            (&key(SLAYER, "team_scale"), Value::Int(2)),
        ],
    );
    let tools = g.tools(red);
    assert_eq!(tools[1], "");
    assert_eq!(tools[3], "v20.weapon.wanditem");
    g.set_team(owner, RED, &[(&key(SLAYER, "team_player_type"), Value::Text(FROZEN.into()))]);
    assert_eq!(g.body(red), FROZEN);
    assert_eq!(g.scale(red), 2.0);
    assert_eq!(g.body(blue), STANDARD, "Blue keeps its own");
    assert_eq!(g.scale(blue), 1.0);
    // An item the server lacks is refused.
    let game = g.s.minigame_views()[0].id;
    let teams = g.s.minigame_views()[0].teams.clone();
    let bad = MiniGameRequest::AddOnSettings {
        game,
        settings: vec![],
        teams: Some(
            teams
                .iter()
                .map(|t| TeamEdit {
                    id: Some(t.id.0),
                    name: t.name.clone(),
                    color: t.color,
                    settings: vec![SettingEdit {
                        key: key(SLAYER, "team_equip_0"),
                        value: Some(Value::Text("v20.weapon.nosuchitem".into())),
                    }],
                })
                .collect(),
        ),
    };
    assert!(g.cmd(owner, Command::MiniGame(bad)).is_err());

    // Synced with the mini-game's loadout, a team spawns with the game's.
    g.set_team(owner, RED, &[(&key(SLAYER, "team_sync_loadout"), Value::Bool(true))]);
    assert_eq!(g.body(red), STANDARD);
    g.cmd(red, Command::Suicide).unwrap();
    g.steps(125);
    g.cmd(red, Command::Respawn).unwrap();
    g.steps(2);
    assert!(g.tools(red).iter().all(String::is_empty));
    assert_eq!(g.scale(red), 2.0, "the scale is the team's still");

    // The team's respawn time, at least a second.
    g.set_team(owner, RED, &[(&key(SLAYER, "team_respawn_time"), Value::Int(3))]);
    g.cmd(red, Command::Suicide).unwrap();
    g.steps(125);
    assert!(g.cmd(red, Command::Respawn).is_err(), "three seconds");
    g.steps(3 * 120);
    g.cmd(red, Command::Respawn).unwrap();
    g.steps(2);

    // Out of the game, their own avatar and body again.
    g.set_team(owner, RED, &[(&key(SLAYER, "team_uniform"), Value::Int(3))]);
    assert_ne!(g.s.avatars()[&red], own);
    g.cmd(red, Command::MiniGame(MiniGameRequest::Leave)).unwrap();
    g.steps(2);
    assert_eq!(g.s.avatars()[&red], own);
    assert_eq!(g.scale(red), 1.0);
    g.quiet();
}

/// The rules' bots in `g`'s game by team colour: (red, blue, without a
/// team).
fn bots_by_team(g: &Game) -> (usize, usize, usize) {
    let teams = g.s.minigame_views()[0].teams.clone();
    let colour = |team: u32| teams.iter().find(|t| t.id.0 == team).map(|t| t.color);
    let mut counts = (0, 0, 0);
    for (owner, v) in g.s.vitals() {
        if !g.s.is_bot(owner) {
            continue;
        }
        match v.team.and_then(colour) {
            Some(RED) => counts.0 += 1,
            Some(BLUE) => counts.1 += 1,
            _ => counts.2 += 1,
        }
    }
    counts
}

fn bot_of(g: &Game, color: u8) -> OwnerId {
    let teams = g.s.minigame_views()[0].teams.clone();
    let team = teams.iter().find(|t| t.color == color).unwrap().id.0;
    g.s.vitals()
        .into_iter()
        .find(|(o, v)| g.s.is_bot(*o) && v.team == Some(team))
        .unwrap()
        .0
}

#[test]
fn a_teams_preferred_player_count_fills_it_with_bots() {
    let mut g = Game::new("bots");
    let (red, _) = two_teams_as(&mut g, true);
    let owner = g.s.minigame_views()[0].owner;
    let fill = key(SLAYER, "team_bot_fill");

    // Without the Blockhead Bot Add-On the count is refused, and whoever
    // runs the game is told (`updateBotFillLimit`).
    g.s.take_private_notices();
    g.set_team(owner, RED, &[(&fill, Value::Int(2))]);
    g.steps(2);
    assert_eq!(bots_by_team(&g), (0, 0, 0));
    let notices = g.s.take_private_notices();
    assert!(
        notices.iter().any(|(o, n)| *o == owner
            && matches!(n, Notice::MessageBox { title, text }
                if title == "Slayer | Error" && text.contains("Blockhead Bot"))),
        "{notices:?}"
    );

    // With it, each team of one player takes one bot to make two.
    g.s.set_bot_kinds(
        bri_sim::bot_kind::BotPack::from_json(include_bytes!(
            "../../../packages/blockhead_bot/assets/bots.json"
        ))
        .unwrap()
        .bots,
    )
    .unwrap();
    g.set_team(owner, RED, &[(&fill, Value::Int(2))]);
    g.set_team(owner, BLUE, &[(&fill, Value::Int(2))]);
    g.steps(4);
    assert_eq!(bots_by_team(&g), (1, 1, 0));
    let names = g.s.names();
    for (o, _) in g.s.vitals() {
        if g.s.is_bot(o) {
            assert!(names[&o].starts_with("Bot "), "{}", names[&o]);
            assert!(g.s.vitals()[&o].alive);
        }
    }

    // Killing a bot is worth the stand-in's Kill Bot, three (past its
    // spawn protection).
    g.steps(310);
    let bot = bot_of(&g, BLUE);
    let before = g.score(red);
    g.run(red, "probe", "kill", vec![PackageArg::Int(bot as i64)]);
    g.steps(2);
    assert!(!g.s.vitals()[&bot].alive);
    assert_eq!(g.score(red) - before, 3);
    // It comes back by itself after the stand-in's bot respawn time, two
    // seconds.
    g.steps(120);
    assert!(!g.s.vitals()[&bot].alive, "not yet");
    g.steps(150);
    assert!(g.s.vitals()[&bot].alive, "back after two seconds");

    // A player joining sends a bot of their team away.
    let c =
        g.s.join("Charlie".into(), Vec3::new(0.0, 0.05, 20.0), false)
            .unwrap();
    let game = g.s.minigame_views()[0].id;
    g.cmd(c, Command::MiniGame(MiniGameRequest::Join { game }))
        .unwrap();
    g.steps(4);
    let (r, b, none) = bots_by_team(&g);
    assert_eq!((r + b, none), (1, 0), "{r} {b}");
    // And leaving brings it back.
    g.cmd(c, Command::MiniGame(MiniGameRequest::Leave)).unwrap();
    g.steps(4);
    assert_eq!(bots_by_team(&g), (1, 1, 0));

    // A mode without teams has no bots.
    g.set(owner, &[(&key(SLAYER, "mode"), Value::Text("Slayer_Deathmatch".into()))]);
    g.steps(4);
    assert_eq!(bots_by_team(&g), (0, 0, 0));
    g.quiet();
}

#[test]
fn a_saved_build_keeps_its_mini_game_and_fly_through_path() {
    let mut g = Game::new("saved-game");
    two_teams_as(&mut g, true);
    let owner = g.s.minigame_views()[0].owner;
    g.plant(owner, TEAM_SPAWN, -15.5, 0.0, RED);
    // The game's own settings, Add-On settings and a team's.
    let mut settings = g.s.minigame_views()[0].settings.clone();
    settings.title = "Trench CTF".into();
    settings.points_kill_player = 4;
    g.cmd(owner, Command::MiniGame(MiniGameRequest::Configure { settings }))
        .unwrap();
    g.set(owner, &[(&key(SLAYER, "mode"), Value::Text(CTF_MODE.into()))]);
    g.set_team(owner, BLUE, &[(&key(SLAYER, "team_respawn_time"), Value::Int(3))]);
    // A fly-through path of two knots and a jump.
    g.run(owner, SLAYER, "createflycam", vec![]);
    g.steps(13);
    for (at, command) in [
        (Vec3::new(0.0, 0.05, 0.0), "setknot"),
        (Vec3::new(10.0, 0.05, 0.0), "setknot"),
        (Vec3::new(10.0, 0.05, 30.0), "setjump"),
    ] {
        g.goto(owner, at);
        g.steps(2);
        let line = if command == "setknot" { "20 Normal Linear" } else { "" };
        g.run(owner, SLAYER, command, vec![PackageArg::String(line.into())]);
        g.steps(13);
    }
    let before = g.s.minigame_views()[0].clone();

    let build = match g
        .cmd(owner, Command::SaveBuild { events: true, ownership: false })
        .unwrap()
    {
        Reply::Saved(build) => build,
        other => panic!("{other:?}"),
    };
    assert!(build.minigame.is_some());
    // Through the file and back, as the Load screen reads it.
    let build = bri_world::build::decode(&bri_world::build::encode(&build).unwrap()).unwrap();

    // The game ends, its path with it; loading the build brings both back
    // as a new game of the loader's.
    g.cmd(owner, Command::MiniGame(MiniGameRequest::End)).unwrap();
    g.steps(5 * 120);
    assert!(g.s.minigame_views().is_empty());
    g.cmd(owner, Command::LoadBuild { build: Box::new(build), ownership: false })
        .unwrap();
    while g.s.build_loading() {
        g.steps(1);
    }
    g.steps(13);
    let after = g.s.minigame_views()[0].clone();
    assert_eq!(after.owner, owner);
    assert_eq!(after.settings, before.settings);
    assert_eq!(after.settings.title, "Trench CTF");
    assert_eq!(after.addon_settings, before.addon_settings);
    let teams = |v: &bri_sim::session::MiniGameView| -> Vec<(String, u8, BTreeMap<String, Value>)> {
        v.teams
            .iter()
            .map(|t| (t.name.clone(), t.color, t.addon_settings.clone()))
            .collect()
    };
    assert_eq!(teams(&after), teams(&before));
    assert_eq!(
        after.addon_settings[&key(SLAYER, "mode")],
        Value::Text(CTF_MODE.into())
    );
    // The path flies as it did.
    g.steps(5 * 120);
    g.run(owner, SLAYER, "testflycam", vec![]);
    g.steps(2);
    assert_eq!(g.s.control(owner), Some(ControlObject::Path));
    let path = g.s.vitals()[&owner].camera_path.clone().unwrap();
    assert_eq!(path.knots.len(), 3);
    g.quiet();
}
