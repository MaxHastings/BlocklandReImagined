//! Stress campaign categories 9 and 1: game modes nothing like the Stress
//! Lab, written only against the package seams (behaviour, state, hooks,
//! operations, HUD). Each test authors its packages from the literal files
//! below, runs them headless, and plays the mode through commands and
//! movement. A mode that cannot be expressed without engine changes has
//! found a missing seam.
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::{Definition, Definitions},
    player::MoveInput,
    session::{Command, PackageArg, PackageCommand, Session},
    simulation::Simulation,
};
use bri_world::World;
use glam::Vec3;
use rapier3d::prelude::*;
use std::sync::Arc;

/// One package: its id, side and files (path, contents).
struct Package<'a> {
    id: &'a str,
    side: Side,
    files: &'a [(&'a str, &'a str)],
}

fn definitions() -> Definitions {
    let mesh = Mesh {
        schema_version: 1,
        id: "plate".into(),
        footprint_studs: [2, 1],
        height_plates: 1,
        attachment_rows: vec!["bb".into()],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![],
    };
    let collision = CollisionBody {
        id: "plate".into(),
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
    Definitions {
        entries: [(
            "plate".into(),
            Definition {
                mesh,
                collision,
                shape,
                indestructible: false,
                special: Default::default(),
            },
        )]
        .into(),
    }
}

/// A flat 200 x 200 floor and the packages, enabled before anyone joins.
fn mode(name: &str, packages: &[Package]) -> Session {
    let root = std::env::temp_dir().join(format!("bri-unlike-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let mut entries = Vec::new();
    for package in packages {
        for (path, contents) in package.files {
            let path = root.join(package.id).join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }
        entries.push(PackageEntry {
            id: package.id.into(),
            version: "1.0.0".into(),
            side: package.side,
            dir: package.id.into(),
            role: None,
        });
    }
    let catalog = Catalog::load(
        &root,
        &PackageSet {
            schema_version: 1,
            packages: entries,
        },
        true,
    )
    .unwrap_or_else(|e| panic!("{e:#?}"));
    let world = World::new(name.into(), "flat".into(), vec![[1.0; 4], [0.0; 4]]);
    let mut session = Session::new(
        Simulation::new(
            world,
            definitions(),
            vec![
                ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0)),
            ],
        )
        .unwrap(),
    );
    session.install_packages(Arc::new(catalog), None).unwrap();
    let _ = std::fs::remove_dir_all(&root);
    session
}

fn manifest(id: &str, capabilities: &[&str], provides: &[(&str, &str, &str)]) -> String {
    serde_json::json!({
        "schema_version": 1,
        "id": id,
        "version": "1.0.0",
        "api": 1,
        "name": id,
        "license": "CC0-1.0",
        "capabilities": capabilities,
        "provides": provides.iter().map(|(kind, name, file)| serde_json::json!({
            "kind": kind, "id": format!("{id}:{kind}/{name}"), "file": file
        })).collect::<Vec<_>>(),
    })
    .to_string()
}

fn command(package: &str, name: &str, args: Vec<PackageArg>) -> Command {
    Command::Package(PackageCommand {
        package: package.into(),
        command: name.into(),
        args,
    })
}

fn value(s: &Session, package: &str, owner: u64, key: &str) -> serde_json::Value {
    s.package_value(package, owner, key).unwrap_or_default()
}

fn position(s: &Session, owner: u64) -> Vec3 {
    let p = s
        .snapshot()
        .players
        .into_iter()
        .find(|p| p.owner == owner)
        .unwrap();
    Vec3::from(p.feet)
}

/// Walk `owner` toward `target` for up to `ticks`, one input per tick.
fn walk_to(s: &mut Session, owner: u64, target: Vec3, ticks: u32, sequence: &mut u64) {
    for _ in 0..ticks {
        let here = position(s, owner);
        let to = target - here;
        if Vec3::new(to.x, 0.0, to.z).length() < 0.5 {
            break;
        }
        // Yaw 0 faces -Z; positive yaw turns toward +X.
        let yaw = to.x.atan2(-to.z);
        *sequence += 1;
        s.movement(
            owner,
            *sequence,
            MoveInput {
                forward: 1.0,
                yaw,
                ..Default::default()
            },
        )
        .unwrap();
        s.step().unwrap();
    }
}

fn steps(s: &mut Session, n: u32) {
    for _ in 0..n {
        s.step().unwrap();
    }
}

const RACE_BEHAVIOUR: &str = r#"{
  "schema_version": 1,
  "script": "race.rhai",
  "commands": [{ "name": "start" }],
  "state": {
    "player": {
      "racing": { "default": false },
      "next": { "default": 0 },
      "started": { "default": 0 },
      "laps": { "default": 0, "visible": "everyone" },
      "best": { "default": -1, "visible": "everyone", "persist": true }
    }
  },
  "on_death": true,
  "tick_interval": 1
}"#;

/// Gates are circles on the floor, crossed in order; a lap ends back at
/// the first. The start teleports the racer to the grid.
const RACE_SCRIPT: &str = r#"
fn gates() { [[0.0, -8.0], [8.0, -8.0], [8.0, 0.0], [0.0, 0.0]] }

fn cmd_start(player) {
    teleport(player, 0.0, 0.2, 2.0);
    set_player(player, "racing", true);
    set_player(player, "next", 0);
    set_player(player, "laps", 0);
    set_player(player, "started", tick());
    tell(player, "Go!");
}

fn on_tick() {
    let gates = gates();
    for p in players() {
        if !get_player(p.id, "racing") { continue; }
        let next = get_player(p.id, "next");
        let gate = gates[next];
        let dx = p.x - gate[0];
        let dz = p.z - gate[1];
        if dx * dx + dz * dz > 2.25 { continue; }
        if next + 1 < gates.len() {
            set_player(p.id, "next", next + 1);
            continue;
        }
        let time = tick() - get_player(p.id, "started");
        let best = get_player(p.id, "best");
        if best < 0 || time < best { set_player(p.id, "best", time); }
        add_player(p.id, "laps", 1);
        set_player(p.id, "next", 0);
        set_player(p.id, "started", tick());
    }
}

// A racer who dies is out of the race until they start again.
fn on_death(victim, killer) {
    set_player(victim, "racing", false);
}
"#;

/// E16 (category 9). A racing mode: no bricks, no weapons, no economy. A
/// racer starts on the grid, drives through four gates in order and the
/// server times the lap; dying takes them out of the race.
#[test]
fn a_racing_mode_times_laps_through_gates() {
    let mut s = mode(
        "race",
        &[Package {
            id: "race",
            side: Side::Server,
            files: &[
                (
                    "package.json",
                    &manifest(
                        "race",
                        &["player", "chat"],
                        &[
                            ("behaviour", "race", "behaviour.json"),
                            ("script", "race", "race.rhai"),
                        ],
                    ),
                ),
                ("behaviour.json", RACE_BEHAVIOUR),
                ("race.rhai", RACE_SCRIPT),
            ],
        }],
    );
    let racer = s
        .join("Racer".into(), Vec3::new(20.0, 0.05, 20.0), false)
        .unwrap();
    steps(&mut s, 10);
    let mut seq = 0;
    s.command(racer, 1, command("race", "start", vec![]))
        .unwrap();
    steps(&mut s, 2);
    let grid = position(&s, racer);
    assert!(
        grid.distance(Vec3::new(0.0, grid.y, 2.0)) < 0.5,
        "start teleports to the grid, racer at {grid}"
    );
    for gate in [
        Vec3::new(0.0, 0.0, -8.0),
        Vec3::new(8.0, 0.0, -8.0),
        Vec3::new(8.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 0.0),
    ] {
        walk_to(&mut s, racer, gate, 400, &mut seq);
        steps(&mut s, 2);
    }
    assert_eq!(value(&s, "race", racer, "laps"), serde_json::json!(1));
    let best = value(&s, "race", racer, "best").as_i64().unwrap();
    assert!(best > 30 && best < 1600, "lap took {best} ticks");
    // Death ends the run: a kill from anything reaches the package.
    s.command(racer, 2, Command::Suicide).unwrap();
    steps(&mut s, 2);
    assert_eq!(value(&s, "race", racer, "racing"), serde_json::json!(false));
}

const TDM_BEHAVIOUR: &str = r#"{
  "schema_version": 1,
  "script": "tdm.rhai",
  "commands": [{ "name": "zap", "args": ["int"], "cooldown_ticks": 5 }],
  "state": {
    "player": {
      "team": { "default": "", "visible": "everyone" },
      "kills": { "default": 0, "visible": "everyone" }
    },
    "global": {
      "round": { "default": 1, "visible": "everyone" },
      "red": { "default": 0, "visible": "everyone" },
      "blue": { "default": 0, "visible": "everyone" },
      "joined": { "default": 0 },
      "ends": { "default": 600 }
    }
  },
  "on_join": true,
  "on_death": true,
  "tick_interval": 10
}"#;

/// Two teams, a package weapon, kills scored by the engine's death report,
/// and timed rounds that respawn everyone and clear the score.
const TDM_SCRIPT: &str = r#"
fn on_join(player) {
    let n = get("joined");
    set("joined", n + 1);
    set_player(player, "team", if n % 2 == 0 { "red" } else { "blue" });
}

fn cmd_zap(player, target) {
    if player(target) == () { return; }
    if get_player(target, "team") == get_player(player, "team") {
        tell(player, "Friendly fire is off.");
        return;
    }
    damage(target, 1000.0, player);
}

fn on_death(victim, killer) {
    if killer == () || killer == victim { return; }
    let team = get_player(killer, "team");
    if team == get_player(victim, "team") { return; }
    set(team, get(team) + 1);
    add_player(killer, "kills", 1);
}

fn on_tick() {
    if tick() < get("ends") { return; }
    let red = get("red");
    let blue = get("blue");
    broadcast(if red > blue { "Red wins the round" } else if blue > red { "Blue wins the round" } else { "Draw" });
    set("round", get("round") + 1);
    set("red", 0);
    set("blue", 0);
    set("ends", tick() + 600);
    for p in players() {
        set_player(p.id, "kills", 0);
        respawn(p.id);
    }
}
"#;

/// E17 (category 9). Round-based team PvP: teams assigned on join, a kill
/// scores only across teams, and when the round's time runs out everyone
/// respawns and the score resets. Nothing about bricks or building.
#[test]
fn a_round_based_team_mode_scores_kills_and_resets_rounds() {
    let mut s = mode(
        "tdm",
        &[Package {
            id: "tdm",
            side: Side::Server,
            files: &[
                (
                    "package.json",
                    &manifest(
                        "tdm",
                        &["damage", "player", "chat"],
                        &[
                            ("behaviour", "tdm", "behaviour.json"),
                            ("script", "tdm", "tdm.rhai"),
                        ],
                    ),
                ),
                ("behaviour.json", TDM_BEHAVIOUR),
                ("tdm.rhai", TDM_SCRIPT),
            ],
        }],
    );
    let red = s
        .join("Red".into(), Vec3::new(-10.0, 0.05, 0.0), false)
        .unwrap();
    let blue = s
        .join("Blue".into(), Vec3::new(10.0, 0.05, 0.0), false)
        .unwrap();
    let red2 = s
        .join("Red2".into(), Vec3::new(-12.0, 0.05, 0.0), false)
        .unwrap();
    // Past v20's spawn protection.
    steps(&mut s, 320);
    assert_eq!(value(&s, "tdm", red, "team"), serde_json::json!("red"));
    assert_eq!(value(&s, "tdm", blue, "team"), serde_json::json!("blue"));
    assert_eq!(value(&s, "tdm", red2, "team"), serde_json::json!("red"));
    s.command(
        red,
        1,
        command("tdm", "zap", vec![PackageArg::Int(red2 as i64)]),
    )
    .unwrap();
    s.command(
        red,
        2,
        command("tdm", "zap", vec![PackageArg::Int(blue as i64)]),
    )
    .unwrap_err();
    steps(&mut s, 6);
    s.command(
        red,
        3,
        command("tdm", "zap", vec![PackageArg::Int(blue as i64)]),
    )
    .unwrap();
    steps(&mut s, 2);
    let state = s.package_state();
    assert_eq!(
        state.packages["tdm"].global["red"],
        serde_json::json!(1),
        "{:#?}",
        s.package_diagnostics()
    );
    assert_eq!(value(&s, "tdm", red, "kills"), serde_json::json!(1));
    assert!(!alive(&s, blue), "blue died");
    assert!(alive(&s, red2), "no friendly fire");
    // The round ends at tick 600: everyone respawns, scores reset.
    assert!(s.simulation().state().tick < 590);
    while s.simulation().state().tick < 620 {
        s.step().unwrap();
    }
    let state = s.package_state();
    assert_eq!(state.packages["tdm"].global["round"], serde_json::json!(2));
    assert_eq!(state.packages["tdm"].global["red"], serde_json::json!(0));
    assert_eq!(value(&s, "tdm", red, "kills"), serde_json::json!(0));
    assert!(alive(&s, blue), "the round respawned blue");
}

fn alive(s: &Session, owner: u64) -> bool {
    s.is_alive(owner)
}

const BOARD_BEHAVIOUR: &str = r#"{
  "schema_version": 1,
  "script": "board.rhai",
  "commands": [{ "name": "sit" }, { "name": "play", "args": ["int"] }],
  "state": {
    "global": {
      "board": { "default": "---------", "visible": "everyone" },
      "x": { "default": -1 },
      "o": { "default": -1 },
      "turn": { "default": "x", "visible": "everyone" },
      "winner": { "default": "", "visible": "everyone" }
    }
  }
}"#;

/// Tic-tac-toe: the whole game is server state and two commands; nobody
/// moves, nothing physical happens.
const BOARD_SCRIPT: &str = r#"
fn cmd_sit(player) {
    if get("x") < 0 { set("x", player); return; }
    if get("o") < 0 && get("x") != player { set("o", player); return; }
    tell(player, "The table is full.");
}

fn cmd_play(player, cell) {
    if get("winner") != "" { return; }
    let mark = get("turn");
    if get(mark) != player { tell(player, "Not your turn."); return; }
    if cell < 0 || cell > 8 { tell(player, "No such cell."); return; }
    let board = get("board");
    if board.sub_string(cell, 1) != "-" { tell(player, "Taken."); return; }
    board = board.sub_string(0, cell) + mark + board.sub_string(cell + 1, 8 - cell);
    set("board", board);
    for line in [[0,1,2],[3,4,5],[6,7,8],[0,3,6],[1,4,7],[2,5,8],[0,4,8],[2,4,6]] {
        if board.sub_string(line[0], 1) == mark && board.sub_string(line[1], 1) == mark
            && board.sub_string(line[2], 1) == mark {
            set("winner", mark);
            broadcast(mark + " wins");
        }
    }
    set("turn", if mark == "x" { "o" } else { "x" });
}
"#;

/// E18 (category 9). A turn-based board game with no physical play at all:
/// seats, turns, rules and a winner are package state; out-of-turn and
/// invalid moves change nothing.
#[test]
fn a_turn_based_board_game_runs_on_state_and_commands_alone() {
    let mut s = mode(
        "board",
        &[Package {
            id: "board",
            side: Side::Server,
            files: &[
                (
                    "package.json",
                    &manifest(
                        "board",
                        &["chat"],
                        &[
                            ("behaviour", "board", "behaviour.json"),
                            ("script", "board", "board.rhai"),
                        ],
                    ),
                ),
                ("behaviour.json", BOARD_BEHAVIOUR),
                ("board.rhai", BOARD_SCRIPT),
            ],
        }],
    );
    let x = s
        .join("X".into(), Vec3::new(-2.0, 0.05, 0.0), false)
        .unwrap();
    let o = s
        .join("O".into(), Vec3::new(2.0, 0.05, 0.0), false)
        .unwrap();
    let mut seq = [0_u64; 2];
    let mut play = |s: &mut Session, who: u64, args: Vec<PackageArg>, name: &str| {
        let i = usize::from(who == o);
        seq[i] += 1;
        s.command(who, seq[i], command("board", name, args))
            .unwrap();
        s.step().unwrap();
    };
    play(&mut s, x, vec![], "sit");
    play(&mut s, o, vec![], "sit");
    for (who, cell) in [(x, 0), (x, 1), (o, 4), (x, 1), (o, 3), (x, 2)] {
        play(&mut s, who, vec![PackageArg::Int(cell)], "play");
    }
    let global = &s.package_state().packages["board"].global;
    assert_eq!(global["board"], serde_json::json!("xxxoo----"));
    assert_eq!(global["winner"], serde_json::json!("x"));
}

const RTS_BEHAVIOUR: &str = r#"{
  "schema_version": 1,
  "script": "rts.rhai",
  "commands": [
    { "name": "train", "cooldown_ticks": 2 },
    { "name": "order", "aim_reach": 64.0 }
  ],
  "state": {
    "player": {
      "units": { "default": 0, "visible": "everyone" },
      "tx": { "default": 0.0 },
      "tz": { "default": 0.0 }
    }
  }
}"#;

const RTS_UNIT: &str = r#"{
  "schema_version": 1,
  "name": "Worker",
  "model": "rts-look:model/worker",
  "think": "think",
  "think_interval": 2,
  "speed": 1.0,
  "scale": 0.6,
  "health": 10.0,
  "max_alive": 16
}"#;

const RTS_MODEL: &str = r#"{
  "schema_version": 1,
  "boxes": [{ "center": [0.0, 0.5, 0.0], "size": [0.6, 1.0, 0.6], "color": [0.2, 0.5, 0.9, 1.0] }]
}"#;

/// Units belong to the player who trained them and walk where that player
/// points; the player's own body is only a cursor.
const RTS_SCRIPT: &str = r#"
fn cmd_train(player) {
    let p = player(player);
    spawn_entity("rts:entity/worker", p.x + 2.0, p.y, p.z, #{ owner: player });
    add_player(player, "units", 1);
}

fn cmd_order(player) {
    let hit = aim();
    if hit == () { tell(player, "Point at the ground."); return; }
    set_player(player, "tx", hit.x);
    set_player(player, "tz", hit.z);
}

fn think(unit) {
    let me = me();
    let owner = entity_get(me.id, "owner");
    if owner == () || player(owner) == () { return; }
    let dx = get_player(owner, "tx") - me.x;
    let dz = get_player(owner, "tz") - me.z;
    if dx * dx + dz * dz < 1.0 {
        steer(me.id, 0.0, 0.0, false);
    } else {
        steer(me.id, dx, dz, false);
    }
}
"#;

/// E19 (categories 9, 1). A strategy mode: a player trains units that are
/// theirs, points at the ground and their units walk there. Units must know
/// who they belong to from the moment they exist.
#[test]
fn a_strategy_mode_commands_owned_units_by_pointing() {
    let mut s = mode(
        "rts",
        &[
            Package {
                id: "rts",
                side: Side::Server,
                files: &[
                    (
                        "package.json",
                        &manifest(
                            "rts",
                            &["entity", "chat"],
                            &[
                                ("behaviour", "rts", "behaviour.json"),
                                ("script", "rts", "rts.rhai"),
                                ("entity", "worker", "worker.json"),
                            ],
                        ),
                    ),
                    ("behaviour.json", RTS_BEHAVIOUR),
                    ("rts.rhai", RTS_SCRIPT),
                    ("worker.json", RTS_UNIT),
                ],
            },
            Package {
                id: "rts-look",
                side: Side::Client,
                files: &[
                    (
                        "package.json",
                        &manifest("rts-look", &[], &[("model", "worker", "worker.json")]),
                    ),
                    ("worker.json", RTS_MODEL),
                ],
            },
        ],
    );
    let a = s
        .join("A".into(), Vec3::new(-20.0, 0.05, 0.0), false)
        .unwrap();
    let b = s
        .join("B".into(), Vec3::new(20.0, 0.05, 0.0), false)
        .unwrap();
    steps(&mut s, 10);
    let mut seq = [0_u64; 2];
    for (who, i) in [(a, 0), (a, 0), (b, 1)] {
        seq[i] += 1;
        s.command(who, seq[i], command("rts", "train", vec![]))
            .unwrap();
        steps(&mut s, 3);
    }
    assert_eq!(
        s.package_entities().len(),
        3,
        "{:#?}",
        s.package_diagnostics()
    );
    // A points at the ground 10 units ahead of themselves.
    seq[0] += 1;
    s.command_with_aim(
        a,
        seq[0],
        command("rts", "order", vec![]),
        Some(bri_sim::session::ActionAim {
            yaw: std::f32::consts::FRAC_PI_2,
            pitch: -0.4,
        }),
    )
    .unwrap();
    let target = Vec3::new(
        value(&s, "rts", a, "tx").as_f64().unwrap() as f32,
        0.0,
        value(&s, "rts", a, "tz").as_f64().unwrap() as f32,
    );
    assert!(target.x > -18.0, "the order points ahead of A: {target}");
    steps(&mut s, 600);
    let units = s.package_entities();
    let near = |p: [f32; 3]| Vec3::new(p[0], 0.0, p[2]).distance(target) < 2.5;
    assert_eq!(
        units.iter().filter(|u| near(u.position)).count(),
        2,
        "A's two units went to the target, B's stayed: {units:#?}"
    );
}

const ARENA_BEHAVIOUR: &str = r#"{
  "schema_version": 1,
  "script": "arena.rhai",
  "commands": [{ "name": "build" }, { "name": "sink" }],
  "state": { "global": { "round": { "default": 0, "visible": "everyone" } } }
}"#;

/// The round builds a 2 x 2 m floating floor, puts the player on it, and
/// later sinks it: the world itself is the game's moving part.
const ARENA_SCRIPT: &str = r#"
fn cmd_build(player) {
    for x in 0..4 {
        for z in 0..4 {
            place_brick("plate", x * 1.0, 2.1, z * 0.5 - 10.25, 0.9, 0.2, 0.2);
        }
    }
    teleport(player, 1.5, 2.4, -9.5);
    set("round", get("round") + 1);
}

fn cmd_sink(player) {
    explode(1.5, 2.1, -9.5, 0.1, 0.0, 4.0);
}
"#;

/// E20 (world model; categories 9, 1). A floor-is-lava round: the mode
/// builds its arena at round start and removes it later. Nothing about the
/// world comes from a generator or a player's build.
#[test]
fn a_mode_builds_and_sinks_its_own_arena() {
    let mut s = mode(
        "arena",
        &[Package {
            id: "arena",
            side: Side::Server,
            files: &[
                (
                    "package.json",
                    &manifest(
                        "arena",
                        &["world.edit", "player", "damage"],
                        &[
                            ("behaviour", "arena", "behaviour.json"),
                            ("script", "arena", "arena.rhai"),
                        ],
                    ),
                ),
                ("behaviour.json", ARENA_BEHAVIOUR),
                ("arena.rhai", ARENA_SCRIPT),
            ],
        }],
    );
    let p = s
        .join("P".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    steps(&mut s, 5);
    s.command(p, 1, command("arena", "build", vec![]))
        .unwrap_or_else(|e| panic!("{e:#} {:#?}", s.package_diagnostics()));
    assert_eq!(
        s.simulation().state().bricks.len(),
        16,
        "{:#?}",
        s.package_diagnostics()
    );
    steps(&mut s, 60);
    let on_floor = position(&s, p);
    assert!(on_floor.y > 2.0, "standing on the arena: {on_floor}");
    s.command(p, 2, command("arena", "sink", vec![])).unwrap();
    steps(&mut s, 120);
    assert!(s.simulation().state().bricks.is_empty());
    let fallen = position(&s, p);
    assert!(fallen.y < 0.5, "the floor is gone: {fallen}");
}

const CARDS_BEHAVIOUR: &str = r#"{
  "schema_version": 1,
  "script": "cards.rhai",
  "commands": [{ "name": "deal" }],
  "state": {
    "player": { "hand": { "default": "", "visible": "owner" } }
  }
}"#;

const CARDS_SCRIPT: &str = r#"
fn cmd_deal(player) {
    let n = 0;
    for p in players() {
        set_player(p.id, "hand", if n == 0 { "AS KD 7H" } else { "2C 2D 9S" });
        n += 1;
    }
}
"#;

const CARDS_HUD: &str = r#"{
  "schema_version": 1, "slot": "hud.overlay", "anchor": "bottom_left",
  "title": "Your hand", "background": [0.0, 0.0, 0.0, 0.6],
  "accent": [1.0, 1.0, 1.0, 1.0], "text": [1.0, 1.0, 1.0, 1.0],
  "rows": [{ "label": "Hand", "bind": "cards:player/hand" }]
}"#;

/// E21 (UI model; categories 9, 1). A card game with hidden hands: each
/// player's HUD shows their own hand, and no other player's client may
/// receive it.
#[test]
fn a_card_game_shows_each_player_only_their_own_hand() {
    let server_manifest = manifest(
        "cards",
        &[],
        &[
            ("behaviour", "cards", "behaviour.json"),
            ("script", "cards", "cards.rhai"),
        ],
    );
    let hud_manifest = manifest("cards-ui", &[], &[("hud", "hand", "hand.json")]);
    let mut s = mode(
        "cards",
        &[
            Package {
                id: "cards",
                side: Side::Server,
                files: &[
                    ("package.json", &server_manifest),
                    ("behaviour.json", CARDS_BEHAVIOUR),
                    ("cards.rhai", CARDS_SCRIPT),
                ],
            },
            Package {
                id: "cards-ui",
                side: Side::Client,
                files: &[("package.json", &hud_manifest), ("hand.json", CARDS_HUD)],
            },
        ],
    );
    let a = s
        .join("A".into(), Vec3::new(-2.0, 0.05, 0.0), false)
        .unwrap();
    let b = s
        .join("B".into(), Vec3::new(2.0, 0.05, 0.0), false)
        .unwrap();
    s.command(a, 1, command("cards", "deal", vec![])).unwrap();
    let hands = |viewer| {
        s.package_state_for(viewer)
            .packages
            .get("cards")
            .map(|ns| ns.players.clone())
            .unwrap_or_default()
    };
    let (seen_by_a, seen_by_b) = (hands(a), hands(b));
    assert_eq!(seen_by_a.keys().collect::<Vec<_>>(), [&a], "{seen_by_a:?}");
    assert_eq!(seen_by_a[&a]["hand"], "AS KD 7H");
    assert_eq!(seen_by_b.keys().collect::<Vec<_>>(), [&b], "{seen_by_b:?}");
    assert_eq!(seen_by_b[&b]["hand"], "2C 2D 9S");
    assert!(
        !s.package_state().packages.contains_key("cards"),
        "the shared view carries no hand"
    );
}
