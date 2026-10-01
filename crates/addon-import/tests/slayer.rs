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
        Command, ControlObject, MiniGameRequest, Notice, ObserverButton, PackageArg,
        PackageCommand, Reply, Session, SettingEdit, TeamEdit,
    },
    simulation::Simulation,
};
use bri_world::{BrickId, OwnerId, World};
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
            plate(CP, Special::None),
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
    let inputs: Vec<_> = g.s.package_brick_inputs().into_iter().map(|i| i.name).collect();
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
        assert_eq!(g.s.control(p), Some(ControlObject::Corpse));
        assert!(g.cmd(p, Command::WeaponTrigger { down: true }).is_err());
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
    assert_eq!(g.s.control(blue), Some(ControlObject::Corpse));

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
    assert_eq!(g.s.control(a), Some(ControlObject::Spy(b)));
    let press = |g: &mut Game, key| {
        g.cmd(a, Command::ObserverButton(key)).unwrap();
        g.steps(1);
    };
    press(&mut g, ObserverButton::Fire);
    assert_eq!(g.s.control(a), Some(ControlObject::Spy(c)));
    press(&mut g, ObserverButton::Jet);
    assert_eq!(g.s.control(a), Some(ControlObject::Spy(b)));

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
    assert!(matches!(g.s.control(a), Some(ControlObject::Spy(_))));
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
