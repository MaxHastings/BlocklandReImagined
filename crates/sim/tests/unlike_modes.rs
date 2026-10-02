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
                reflection: None,
                link: None,
                glass: [0.0; 4],
                bot: None,
            },
        )]
        .into(),
    }
}

/// A flat 200 x 200 floor and the packages, enabled before anyone joins.
fn mode(name: &str, packages: &[Package]) -> Session {
    mode_with(name, packages, None)
}

/// As [`mode`], resuming a host's package save.
fn mode_with(
    name: &str,
    packages: &[Package],
    save: Option<bri_sim::session::PackageSave>,
) -> Session {
    try_mode(name, packages, save).unwrap_or_else(|e| panic!("{e:#}"))
}

/// As [`mode_with`], returning why the host refused the packages.
fn try_mode(
    name: &str,
    packages: &[Package],
    save: Option<bri_sim::session::PackageSave>,
) -> anyhow::Result<Session> {
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
    let installed = session.install_packages(Arc::new(catalog), save);
    let _ = std::fs::remove_dir_all(&root);
    installed.map(|_| session)
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
    // The shared view names the running package but carries no hand.
    assert!(
        s.package_state()
            .packages
            .get("cards")
            .is_some_and(|ns| ns.players.is_empty() && ns.global.is_empty()),
        "the shared view carries no hand"
    );
}

const MOON_BEHAVIOUR: &str = r#"{
  "schema_version": 1,
  "script": "moon.rhai",
  "on_join": true
}"#;

/// A standard player under a sixth of normal gravity.
const MOON_ARCHETYPE: &str =
    r#"{ "schema_version": 1, "name": "Moonwalker", "movement": { "gravity": 3.4 } }"#;

/// Every player on this server is a moonwalker.
const MOON_SCRIPT: &str = r#"
fn on_join(player) {
    set_archetype(player, "moon:archetype/moon");
}
"#;

/// E22 (player/control model; category 9). A low-gravity mode: how players
/// move is the mode. Movement was predicted on every client from the
/// player's datablock, a closed engine enum (`PlayerType`), so a package
/// could choose among v20's seven but never describe its own (W14). Now a
/// package declares an archetype and assigns it.
#[test]
fn a_low_gravity_mode_changes_how_players_move() {
    let behaviour_manifest = manifest(
        "moon",
        &["player"],
        &[
            ("behaviour", "moon", "behaviour.json"),
            ("script", "moon", "moon.rhai"),
            ("archetype", "moon", "archetype.json"),
        ],
    );
    let mut s = mode(
        "moon",
        &[Package {
            id: "moon",
            side: Side::Server,
            files: &[
                ("package.json", &behaviour_manifest),
                ("behaviour.json", MOON_BEHAVIOUR),
                ("moon.rhai", MOON_SCRIPT),
                ("archetype.json", MOON_ARCHETYPE),
            ],
        }],
    );
    let p = s
        .join("P".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    // Settle onto the ground first: a jump needs contact.
    steps(&mut s, 120);
    assert!(
        s.package_diagnostics().is_empty(),
        "{:#?}",
        s.package_diagnostics()
    );
    s.movement(
        p,
        1,
        MoveInput {
            jump: true,
            ..Default::default()
        },
    )
    .unwrap();
    let mut peak = 0.0_f32;
    for _ in 0..240 {
        s.step().unwrap();
        peak = peak.max(position(&s, p).y);
    }
    assert!(
        peak > 5.0,
        "a moon jump rises well above a normal one: {peak}"
    );
}

const SWARM_BEHAVIOUR: &str = r#"{
  "schema_version": 1,
  "script": "swarm.rhai",
  "commands": [{ "name": "fill", "admin": true }]
}"#;

fn swarm_kind(kind: &str) -> String {
    format!(
        r#"{{ "schema_version": 1, "name": "{kind}", "model": "swarm-look:model/ant",
             "think": "think", "think_interval": 1, "speed": 0.5, "scale": 0.4,
             "health": 5.0, "max_alive": 256 }}"#
    )
}

/// 1000 ants, each thinking every tick with some real work (a small
/// neighbourhood scan), more than one tick's share of script work.
const SWARM_SCRIPT: &str = r#"
fn cmd_fill(player) {
    let kinds = ["swarm:entity/a", "swarm:entity/b", "swarm:entity/c", "swarm:entity/d"];
    for k in 0..4 {
        for i in 0..250 {
            let n = k * 250 + i;
            spawn_entity(kinds[k], (n % 32) * 3.0 - 48.0, 0.1, (n / 32) * 3.0 - 48.0, #{ thoughts: 0 });
        }
    }
}

fn think(ant) {
    let me = me();
    let sum = 0;
    for i in 0..300 { sum += i; }
    entity_set(me.id, "thoughts", entity_get(me.id, "thoughts") + 1);
    steer(me.id, 1.0, 0.0, false);
}
"#;

const SWARM_MODEL: &str = r#"{
  "schema_version": 1,
  "boxes": [{ "center": [0.0, 0.2, 0.0], "size": [0.3, 0.3, 0.3], "color": [0.1, 0.1, 0.1, 1.0] }]
}"#;

/// E23 (entity/behaviour model; categories 9, 6). A thousand agents that
/// all want to think every tick. The package's share of script work is
/// less than they ask for, so thinking is rationed: every agent must still
/// think, in turn, and none may starve.
#[test]
fn a_thousand_agents_all_get_to_think() {
    let kinds: Vec<String> = ["a", "b", "c", "d"].iter().map(|k| swarm_kind(k)).collect();
    let behaviour_manifest = manifest(
        "swarm",
        &["entity"],
        &[
            ("behaviour", "swarm", "behaviour.json"),
            ("script", "swarm", "swarm.rhai"),
            ("entity", "a", "a.json"),
            ("entity", "b", "b.json"),
            ("entity", "c", "c.json"),
            ("entity", "d", "d.json"),
        ],
    );
    let look_manifest = manifest("swarm-look", &[], &[("model", "ant", "ant.json")]);
    let mut s = mode(
        "swarm",
        &[
            Package {
                id: "swarm",
                side: Side::Server,
                files: &[
                    ("package.json", &behaviour_manifest),
                    ("behaviour.json", SWARM_BEHAVIOUR),
                    ("swarm.rhai", SWARM_SCRIPT),
                    ("a.json", &kinds[0]),
                    ("b.json", &kinds[1]),
                    ("c.json", &kinds[2]),
                    ("d.json", &kinds[3]),
                ],
            },
            Package {
                id: "swarm-look",
                side: Side::Client,
                files: &[("package.json", &look_manifest), ("ant.json", SWARM_MODEL)],
            },
        ],
    );
    let admin = s
        .join("Admin".into(), Vec3::new(0.0, 0.05, 60.0), true)
        .unwrap();
    s.command(admin, 1, command("swarm", "fill", vec![]))
        .unwrap();
    assert_eq!(
        s.package_entities().len(),
        1000,
        "{:#?}",
        s.package_diagnostics()
    );
    steps(&mut s, 120);
    let total: i64 = s
        .package_entity_vars()
        .into_iter()
        .map(|(_, vars)| vars.get("thoughts").and_then(|t| t.as_i64()).unwrap_or(0))
        .sum();
    let starved = s
        .package_entity_vars()
        .into_iter()
        .filter(|(_, vars)| vars.get("thoughts").and_then(|t| t.as_i64()).unwrap_or(0) == 0)
        .count();
    assert_eq!(
        starved, 0,
        "{starved} of 1000 agents never thought in 120 ticks"
    );
    // The tick stays short because the work is rationed, counted in script
    // operations, not timed: far fewer thinks run than the swarm asks for
    // (one each per tick). About 26,000 fit the package's share.
    assert!(
        total < 1000 * 120 / 2,
        "{total} thinks in 120 ticks: the swarm was not rationed"
    );
}

const LMS_BEHAVIOUR: &str = r#"{
  "schema_version": 1,
  "script": "lms.rhai",
  "on_death": true,
  "policies": ["respawn", "build"],
  "state": {
    "player": { "out": { "default": false, "visible": "everyone" } },
    "global": { "phase": { "default": "fight", "visible": "everyone" } }
  }
}"#;

/// Last player standing: the dead stay out until the round ends, and nobody
/// builds while the round is fought.
const LMS_SCRIPT: &str = r#"
fn on_death(victim, killer) { set_player(victim, "out", true); }
fn allow_respawn(player) {
    if get_player(player, "out") { "You are out until the next round." } else { true }
}
fn allow_build(player) {
    if get("phase") == "fight" { "No building during the fight." } else { true }
}
"#;

/// E25 (game rules; categories 9, 4). An elimination mode decides whether a
/// dead player may come back and whether anyone may build. Those are engine
/// decisions (respawn readiness, build permission), and the mode's rule is
/// policy the engine must ask for.
#[test]
fn an_elimination_mode_decides_who_may_respawn_and_build() {
    let behaviour_manifest = manifest(
        "lms",
        &[],
        &[
            ("behaviour", "lms", "behaviour.json"),
            ("script", "lms", "lms.rhai"),
        ],
    );
    let mut s = mode(
        "lms",
        &[Package {
            id: "lms",
            side: Side::Server,
            files: &[
                ("package.json", &behaviour_manifest),
                ("behaviour.json", LMS_BEHAVIOUR),
                ("lms.rhai", LMS_SCRIPT),
            ],
        }],
    );
    let a = s
        .join("A".into(), Vec3::new(-2.0, 0.05, 0.0), false)
        .unwrap();
    let b = s
        .join("B".into(), Vec3::new(2.0, 0.05, 0.0), false)
        .unwrap();
    steps(&mut s, 320);
    s.command(a, 1, Command::Suicide).unwrap();
    steps(&mut s, 400);
    assert!(!alive(&s, a));
    let refused = s
        .command(a, 2, Command::Respawn)
        .expect_err("an eliminated player came back");
    assert!(
        format!("{refused:#}").contains("You are out"),
        "{refused:#}"
    );
    assert!(!alive(&s, a));
    let plant = Command::Plant {
        definition: "plate".into(),
        position: [2.5, 0.1, -1.25],
        quarter_turns: 0,
        color: 0,
    };
    let refused = s.command(b, 1, plant).expect_err("built during the fight");
    assert!(
        format!("{refused:#}").contains("No building"),
        "{refused:#}"
    );
    assert!(s.simulation().state().bricks.is_empty());
}

const TAG_BEHAVIOUR: &str = r#"{
  "schema_version": 1,
  "script": "tag.rhai",
  "commands": [{ "name": "tag" }],
  "state": { "player": { "tags": { "default": 0, "visible": "everyone" } } }
}"#;

const TAG_SCRIPT: &str = r#"
fn cmd_tag(player) { add_player(player, "tags", 1); }
"#;

/// Every player's score at once, as a scoreboard lists them.
const TAG_HUD: &str = r#"{
  "schema_version": 1, "slot": "hud.overlay", "anchor": "top_right",
  "title": "Scores", "background": [0.0, 0.0, 0.0, 0.6],
  "accent": [1.0, 1.0, 1.0, 1.0], "text": [1.0, 1.0, 1.0, 1.0],
  "rows": [{ "label": "Tags", "bind": "tag:players/tags" }]
}"#;

/// E26 (UI model; category 9). A scoreboard: one panel lists every
/// player's score, not only the viewer's.
#[test]
fn a_scoreboard_lists_every_players_score() {
    let server_manifest = manifest(
        "tag",
        &[],
        &[
            ("behaviour", "tag", "behaviour.json"),
            ("script", "tag", "tag.rhai"),
        ],
    );
    let hud_manifest = manifest("tag-ui", &[], &[("hud", "scores", "scores.json")]);
    let mut s = mode(
        "tag",
        &[
            Package {
                id: "tag",
                side: Side::Server,
                files: &[
                    ("package.json", &server_manifest),
                    ("behaviour.json", TAG_BEHAVIOUR),
                    ("tag.rhai", TAG_SCRIPT),
                ],
            },
            Package {
                id: "tag-ui",
                side: Side::Client,
                files: &[("package.json", &hud_manifest), ("scores.json", TAG_HUD)],
            },
        ],
    );
    let a = s
        .join("A".into(), Vec3::new(-2.0, 0.05, 0.0), false)
        .unwrap();
    let b = s
        .join("B".into(), Vec3::new(2.0, 0.05, 0.0), false)
        .unwrap();
    s.command(a, 1, command("tag", "tag", vec![])).unwrap();
    s.command(b, 1, command("tag", "tag", vec![])).unwrap();
    s.command(b, 2, command("tag", "tag", vec![])).unwrap();
    let binding = bri_package_runtime::content::Binding::parse("tag:players/tags").unwrap();
    let view = s.package_state_for(a);
    let board = view.rows(&binding);
    assert_eq!(
        board,
        vec![(a, &serde_json::json!(1)), (b, &serde_json::json!(2))]
    );
}

const HILL_BEHAVIOUR: &str = r#"{
  "schema_version": 1,
  "script": "hill.rhai",
  "tick_interval": 120,
  "state": {
    "player": {
      "held": { "default": 0, "visible": "everyone", "persist": false },
      "wins": { "default": 0, "visible": "everyone" }
    },
    "global": {
      "round": { "default": 1, "visible": "everyone", "persist": false },
      "rounds_played": { "default": 0, "visible": "everyone" }
    }
  }
}"#;

/// King of the hill: each second, whoever stands alone on the hill scores;
/// the first to 3 wins the round, which is counted forever.
const HILL_SCRIPT: &str = r#"
fn on_tick() {
    let on_hill = [];
    for p in players() {
        if p.alive && p.x * p.x + p.z * p.z < 4.0 { on_hill.push(p.id); }
    }
    if on_hill.len() != 1 { return; }
    let king = on_hill[0];
    add_player(king, "held", 1);
    if get_player(king, "held") >= 3 {
        add_player(king, "wins", 1);
        set("rounds_played", get("rounds_played") + 1);
        set("round", get("round") + 1);
        for p in players() { set_player(p.id, "held", 0); }
        broadcast(`${player(king).name} takes the hill.`);
    }
}
"#;

/// E28 (game rules, persistence; categories 9, 7). Timed scoring with a win
/// condition and a leaderboard that survives a host restart, while the
/// round in progress does not.
#[test]
fn a_king_of_the_hill_mode_keeps_its_leaderboard_across_restarts() {
    let behaviour_manifest = manifest(
        "hill",
        &["chat"],
        &[
            ("behaviour", "hill", "behaviour.json"),
            ("script", "hill", "hill.rhai"),
        ],
    );
    let files: &[(&str, &str)] = &[
        ("package.json", &behaviour_manifest),
        ("behaviour.json", HILL_BEHAVIOUR),
        ("hill.rhai", HILL_SCRIPT),
    ];
    let packages = [Package {
        id: "hill",
        side: Side::Server,
        files,
    }];
    let mut s = mode("hill", &packages);
    let principal = bri_admin::Principal([7; 32]);
    let king = s
        .join_verified(
            "King".into(),
            Vec3::new(0.0, 0.05, 0.0),
            false,
            Some(principal),
        )
        .unwrap();
    let _other = s
        .join("Other".into(), Vec3::new(20.0, 0.05, 0.0), false)
        .unwrap();
    steps(&mut s, 120 * 4);
    assert_eq!(value(&s, "hill", king, "wins"), serde_json::json!(1));
    steps(&mut s, 120);
    assert_eq!(value(&s, "hill", king, "held"), serde_json::json!(1));
    let save = s.package_save().unwrap();
    let save = bri_sim::session::PackageSave::decode(&save.encode().unwrap()).unwrap();

    let mut s = mode_with("hill-again", &packages, Some(save));
    let king = s
        .join_verified(
            "King".into(),
            Vec3::new(20.0, 0.05, 0.0),
            false,
            Some(principal),
        )
        .unwrap();
    assert_eq!(
        value(&s, "hill", king, "wins"),
        serde_json::json!(1),
        "wins persist"
    );
    assert_eq!(
        value(&s, "hill", king, "held"),
        serde_json::json!(0),
        "the round does not"
    );
    let global = &s.package_state().packages["hill"].global;
    assert_eq!(global["rounds_played"], serde_json::json!(1));
    assert_eq!(global["round"], serde_json::json!(1));
}

const KART_BEHAVIOUR: &str = r#"{
  "schema_version": 1,
  "script": "kart.rhai",
  "commands": [{ "name": "board" }, { "name": "leave" }]
}"#;

const KART_SCRIPT: &str = r#"
fn cmd_board(player) {
    let p = player(player);
    spawn_entity("kart:entity/kart", p.x + 3.0, p.y, p.z, #{ driver: player, boarded: false });
}
fn cmd_leave(player) {
    release(player);
}
fn think(kart) {
    let driver = entity_get(kart.id, "driver");
    if driver != () && !entity_get(kart.id, "boarded") {
        entity_set(kart.id, "boarded", true);
        control(driver, kart.id);
    }
}
"#;

const KART_ENTITY: &str = r#"{ "schema_version": 1, "name": "Kart", "model": "kart-look:model/kart",
  "think": "think", "think_interval": 10, "speed": 1.0, "scale": 1.0, "health": 50.0, "max_alive": 8,
  "archetype": "kart:archetype/kart" }"#;

/// E27 (player and control; category 9). A player drives something that is
/// not their avatar: a package's kart takes the player's movement input
/// while the avatar stays behind. What a player controls was the closed
/// `ControlObject` enum (player, camera, spy, corpse), so this was the open
/// rest of W14. Now a package hands a player one of its own entities
/// (`control(player, entity)`, back with `release(player)`); the entity's
/// archetype says how it moves (the kart turns rather than strafes).
#[test]
fn a_player_drives_a_package_kart() {
    let behaviour_manifest = manifest(
        "kart",
        &["entity", "player"],
        &[
            ("behaviour", "kart", "behaviour.json"),
            ("script", "kart", "kart.rhai"),
            ("entity", "kart", "kart.json"),
            ("archetype", "kart", "archetype.json"),
        ],
    );
    let look_manifest = manifest("kart-look", &[], &[("model", "kart", "kart.json")]);
    let archetype = KART_ARCHETYPE.replace("bodies-look:model/kart", "kart-look:model/kart");
    let mut s = mode(
        "kart",
        &[
            Package {
                id: "kart",
                side: Side::Server,
                files: &[
                    ("package.json", &behaviour_manifest),
                    ("behaviour.json", KART_BEHAVIOUR),
                    ("kart.rhai", KART_SCRIPT),
                    ("kart.json", KART_ENTITY),
                    ("archetype.json", &archetype),
                ],
            },
            Package {
                id: "kart-look",
                side: Side::Client,
                files: &[("package.json", &look_manifest), ("kart.json", RTS_MODEL)],
            },
        ],
    );
    let p = s
        .join("Driver".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    steps(&mut s, 5);
    s.command(p, 1, command("kart", "board", vec![]))
        .unwrap_or_else(|e| panic!("{e:#}"));
    steps(&mut s, 12);
    let kart = s.package_entities()[0].id;
    assert_eq!(
        s.control(p),
        Some(bri_sim::session::ControlObject::Entity(kart))
    );
    let avatar = s.snapshot().players[0].feet;
    let start = s.package_entities()[0].clone();
    let mut sequence = 0;
    let mut drive = |s: &mut Session, input: MoveInput, ticks: u32| {
        for _ in 0..ticks {
            sequence += 1;
            s.movement(p, sequence, input).unwrap();
            s.step().unwrap();
        }
    };
    drive(
        &mut s,
        MoveInput {
            forward: 1.0,
            ..Default::default()
        },
        120,
    );
    let ahead = s.package_entities()[0].clone();
    assert!(
        Vec3::from(ahead.position).distance(Vec3::from(start.position)) > 5.0,
        "the kart moved under the player's input"
    );
    assert!(
        Vec3::from(s.snapshot().players[0].feet).distance(Vec3::from(avatar)) < 0.5,
        "the avatar stayed behind"
    );
    // Right turns a kart (its archetype steers like a vehicle).
    drive(
        &mut s,
        MoveInput {
            forward: 1.0,
            right: 1.0,
            ..Default::default()
        },
        60,
    );
    let turned = s.package_entities()[0].clone();
    assert!(
        (turned.yaw - ahead.yaw).abs() > 0.5,
        "the kart turned: {} to {}",
        ahead.yaw,
        turned.yaw
    );
    // Released, the player walks again and the kart stops taking input.
    s.command(p, 2, command("kart", "leave", vec![]))
        .unwrap_or_else(|e| panic!("{e:#}"));
    assert_eq!(s.control(p), Some(bri_sim::session::ControlObject::Player));
    // Let it coast to a stop.
    steps(&mut s, 240);
    let parked = s.package_entities()[0].position;
    drive(
        &mut s,
        MoveInput {
            forward: 1.0,
            ..Default::default()
        },
        60,
    );
    assert!(
        Vec3::from(s.package_entities()[0].position).distance(Vec3::from(parked)) < 0.5,
        "a released kart takes no input"
    );
    assert!(
        Vec3::from(s.snapshot().players[0].feet).distance(Vec3::from(avatar)) > 2.0,
        "the avatar walks again"
    );
}

const ZOMBIE_BEHAVIOUR: &str = r#"{
  "schema_version": 1,
  "script": "zombies.rhai",
  "commands": [{ "name": "wave", "admin": true }],
  "state": { "player": { "bitten": { "default": 0, "visible": "owner" } } }
}"#;

const ZOMBIE_ENTITY: &str = r#"{ "schema_version": 1, "name": "Zombie", "model": "zombies-look:model/zombie",
  "think": "think", "think_interval": 4, "speed": 0.6, "scale": 1.0, "health": 30.0, "max_alive": 32 }"#;

/// Zombies shamble toward the nearest living player and bite on contact.
/// The wave is spread out: eight zombies shoulder to shoulder jammed each
/// other short of the survivor on Windows (see HANDOFF's next steps).
const ZOMBIE_SCRIPT: &str = r#"
fn cmd_wave(player) {
    for i in 0..4 { spawn_entity("zombies:entity/zombie", i * 6.0 - 9.0, 0.1, -20.0); }
}
fn think(z) {
    let me = me();
    let best = (); let best_d = 1e9;
    for p in players() {
        if !p.alive { continue; }
        let d = (p.x - me.x) * (p.x - me.x) + (p.z - me.z) * (p.z - me.z);
        if d < best_d { best_d = d; best = p; }
    }
    if best == () { steer(me.id, 0.0, 0.0, false); return; }
    if best_d < 2.25 {
        damage(best.id, 5.0, ());
        add_player(best.id, "bitten", 1);
    }
    steer(me.id, best.x - me.x, best.z - me.z, false);
}
"#;

/// E29 (entities and behaviour; categories 9, 1). Hostile agents that chase
/// and hurt players, written only against the existing seams.
#[test]
fn a_zombie_wave_chases_and_bites_players() {
    let behaviour_manifest = manifest(
        "zombies",
        &["entity", "damage"],
        &[
            ("behaviour", "zombies", "behaviour.json"),
            ("script", "zombies", "zombies.rhai"),
            ("entity", "zombie", "zombie.json"),
        ],
    );
    let look_manifest = manifest("zombies-look", &[], &[("model", "zombie", "zombie.json")]);
    let mut s = mode(
        "zombies",
        &[
            Package {
                id: "zombies",
                side: Side::Server,
                files: &[
                    ("package.json", &behaviour_manifest),
                    ("behaviour.json", ZOMBIE_BEHAVIOUR),
                    ("zombies.rhai", ZOMBIE_SCRIPT),
                    ("zombie.json", ZOMBIE_ENTITY),
                ],
            },
            Package {
                id: "zombies-look",
                side: Side::Client,
                files: &[("package.json", &look_manifest), ("zombie.json", RTS_MODEL)],
            },
        ],
    );
    let survivor = s
        .join("Survivor".into(), Vec3::new(0.0, 0.05, 0.0), true)
        .unwrap();
    steps(&mut s, 320);
    s.command(survivor, 1, command("zombies", "wave", vec![]))
        .unwrap();
    for _ in 0..(120 * 20) {
        s.step().unwrap();
        if !alive(&s, survivor) {
            break;
        }
    }
    assert!(
        !alive(&s, survivor),
        "the horde reached and killed the survivor: bitten {:?}, zombies {:?}, {:#?}",
        value(&s, "zombies", survivor, "bitten"),
        s.package_entities()
            .iter()
            .map(|e| e.position)
            .collect::<Vec<_>>(),
        s.package_diagnostics()
    );
    assert!(
        value(&s, "zombies", survivor, "bitten")
            .as_i64()
            .unwrap_or(0)
            >= 1,
        "{:#?}",
        s.package_diagnostics()
    );
}

const BODIES_BEHAVIOUR: &str = r#"{
  "schema_version": 1,
  "script": "bodies.rhai",
  "commands": [
    { "name": "become", "args": ["string"], "while_dead": true },
    { "name": "fall" }
  ]
}"#;

/// Players pick a body; `fall` kills them so the next life shows the body
/// is kept.
const BODIES_SCRIPT: &str = r#"
fn cmd_become(player, body) { set_archetype(player, body); }
fn cmd_fall(player) { damage(player, 1000.0, ()); }
"#;

/// A non-humanoid body: a fast ball with almost no air control.
const BALL_ARCHETYPE: &str = r#"{
  "schema_version": 1,
  "name": "Rolling Ball",
  "movement": {
    "body": "ball", "width": 1.5, "stand_height": 1.5, "crouch_height": 1.5,
    "stand_eye": 1.4, "crouch_eye": 1.4,
    "forward": 14.0, "backward": 14.0, "sideways": 14.0,
    "crouch_forward": 14.0, "crouch_backward": 14.0, "crouch_sideways": 14.0,
    "acceleration": 30.0, "air_control": 0.02, "jump_speed": 6.0, "can_jet": false
  },
  "max_health": 60.0,
  "can_ride": false,
  "model": "bodies-look:model/ball",
  "camera_distance": 6.0
}"#;

/// A kart: wide and low, steered like a vehicle (left and right turn it),
/// fast forward, no jump.
const KART_ARCHETYPE: &str = r#"{
  "schema_version": 1,
  "name": "Kart",
  "movement": {
    "steering": "turn", "turn_rate": 2.0,
    "width": 2.0, "stand_height": 1.0, "crouch_height": 1.0,
    "stand_eye": 0.9, "crouch_eye": 0.9,
    "forward": 20.0, "backward": 5.0, "sideways": 0.0,
    "crouch_forward": 20.0, "crouch_backward": 5.0, "crouch_sideways": 0.0,
    "jump_speed": 0.0, "can_jet": false
  },
  "can_ride": false,
  "model": "bodies-look:model/kart"
}"#;

fn bodies() -> [String; 2] {
    [
        manifest(
            "bodies",
            &["player", "damage"],
            &[
                ("behaviour", "bodies", "behaviour.json"),
                ("script", "bodies", "bodies.rhai"),
                ("archetype", "ball", "ball.json"),
                ("archetype", "kart", "kart.json"),
            ],
        ),
        manifest(
            "bodies-look",
            &[],
            &[
                ("model", "ball", "ball.json"),
                ("model", "kart", "kart.json"),
            ],
        ),
    ]
}

fn bodies_mode(name: &str, ball: &str) -> anyhow::Result<Session> {
    let [behaviour, look] = bodies();
    try_mode(
        name,
        &[
            Package {
                id: "bodies",
                side: Side::Server,
                files: &[
                    ("package.json", &behaviour),
                    ("behaviour.json", BODIES_BEHAVIOUR),
                    ("bodies.rhai", BODIES_SCRIPT),
                    ("ball.json", ball),
                    ("kart.json", KART_ARCHETYPE),
                ],
            },
            Package {
                id: "bodies-look",
                side: Side::Client,
                files: &[
                    ("package.json", &look),
                    ("ball.json", RTS_MODEL),
                    ("kart.json", RTS_MODEL),
                ],
            },
        ],
        None,
    )
}

fn heading(s: &Session, owner: u64) -> f32 {
    s.snapshot()
        .players
        .into_iter()
        .find(|p| p.owner == owner)
        .unwrap()
        .yaw
}

fn archetype_of(s: &Session, owner: u64) -> &bri_sim::archetype::Archetype {
    let state = s
        .snapshot()
        .players
        .into_iter()
        .find(|p| p.owner == owner)
        .unwrap();
    s.archetypes().resolve(state.archetype)
}

/// E30 (player and control; Max's "custom player controllers and models
/// beyond just everyone being a Blockhead"). Four players, four bodies on
/// one server: the Blockhead, v20's horse, a package's rolling ball and a
/// package's kart. Each moves by its own constants, the server keeps a
/// package's choice across death, and a client predicting from the
/// checkpoint's archetype table moves the ball exactly as the server does.
#[test]
fn players_can_be_bodies_beyond_the_blockhead() {
    let mut s = bodies_mode("bodies", BALL_ARCHETYPE).unwrap();
    let names = ["Blockhead", "Horse", "Ball", "Kart"];
    let players: Vec<u64> = names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            s.join(
                (*name).into(),
                Vec3::new(i as f32 * 12.0 - 18.0, 0.05, 0.0),
                false,
            )
            .unwrap()
        })
        .collect();
    steps(&mut s, 10);
    let bodies = [
        "",
        "v20.player.horsearmor",
        "bodies:archetype/ball",
        "bodies:archetype/kart",
    ];
    let mut sequence = [0_u64; 4];
    for (i, body) in bodies.iter().enumerate().skip(1) {
        sequence[i] += 1;
        s.command(
            players[i],
            sequence[i],
            command("bodies", "become", vec![PackageArg::String((*body).into())]),
        )
        .unwrap_or_else(|e| panic!("{e:#}"));
    }
    steps(&mut s, 2);
    assert!(
        s.package_diagnostics().is_empty(),
        "{:#?}",
        s.package_diagnostics()
    );
    assert_eq!(
        archetype_of(&s, players[0]).id,
        "v20.player.playerstandardarmor"
    );
    assert_eq!(archetype_of(&s, players[1]).id, "v20.player.horsearmor");
    let ball = archetype_of(&s, players[2]);
    assert_eq!(ball.movement.body, bri_sim::player::Body::Ball);
    assert_eq!(ball.look.model, "bodies-look:model/ball");
    assert_eq!(s.vitals()[&players[2]].health, 60.0);
    assert_eq!(archetype_of(&s, players[3]).name, "Kart");

    // Everyone holds forward and right for a second.
    let start: Vec<Vec3> = players.iter().map(|p| position(&s, *p)).collect();
    let mut ticks = 0;
    for _ in 0..120 {
        for (i, p) in players.iter().enumerate() {
            sequence[i] += 1;
            s.movement(
                *p,
                sequence[i],
                MoveInput {
                    forward: 1.0,
                    right: 1.0,
                    ..Default::default()
                },
            )
            .unwrap();
        }
        s.step().unwrap();
        ticks += 1;
    }
    assert_eq!(ticks, 120);
    let moved: Vec<Vec3> = players
        .iter()
        .zip(&start)
        .map(|(p, a)| position(&s, *p) - *a)
        .collect();
    let run = |v: Vec3| v.with_y(0.0).length();
    assert!(
        run(moved[2]) > run(moved[1]) && run(moved[1]) > run(moved[0]),
        "the ball outruns the horse, which outruns the Blockhead: {moved:?}"
    );
    // The Blockhead ran diagonally and still faces where it looks; the kart
    // turned right as it drove.
    assert!(moved[0].x > 1.0, "the Blockhead strafes: {moved:?}");
    assert_eq!(heading(&s, players[0]), 0.0);
    assert!(
        (heading(&s, players[3]) - 2.0).abs() < 0.1,
        "the kart turned at its turn rate: {}",
        heading(&s, players[3])
    );
    assert!(run(moved[3]) > 10.0, "the kart drives: {moved:?}");
    // Right alone turns a stopped kart on the spot; it never slides
    // sideways.
    steps(&mut s, 120);
    let before = position(&s, players[3]);
    let facing = heading(&s, players[3]);
    for _ in 0..60 {
        sequence[3] += 1;
        s.movement(
            players[3],
            sequence[3],
            MoveInput {
                right: 1.0,
                ..Default::default()
            },
        )
        .unwrap();
        s.step().unwrap();
    }
    assert!(
        run(position(&s, players[3]) - before) < 0.01,
        "a kart does not strafe"
    );
    assert!(heading(&s, players[3]) != facing, "it turned in place");

    // A package's choice outlives the body: the ball falls and comes back
    // as a ball.
    sequence[2] += 1;
    s.command(players[2], sequence[2], command("bodies", "fall", vec![]))
        .unwrap_or_else(|e| panic!("{e:#}"));
    steps(&mut s, 2);
    assert!(!alive(&s, players[2]));
    for _ in 0..40 {
        steps(&mut s, 30);
        sequence[2] += 1;
        if s.command(players[2], sequence[2], Command::Respawn).is_ok() {
            break;
        }
    }
    steps(&mut s, 2);
    assert!(alive(&s, players[2]));
    assert_eq!(archetype_of(&s, players[2]).id, "bodies:archetype/ball");
    assert_eq!(s.vitals()[&players[2]].health, 60.0);

    // A client predicting the ball from the host's table agrees with it.
    let (state, _) = s
        .motion_states()
        .into_iter()
        .find(|(p, _)| p.owner == players[2])
        .unwrap();
    let mirror = bri_sim::prediction::CollisionMirror::new(
        definitions(),
        vec![ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0))],
        vec![],
    );
    let mut predictor =
        bri_sim::prediction::Predictor::new(mirror, state, s.archetypes().clone()).unwrap();
    for tick in 0..90 {
        let input = MoveInput {
            forward: 1.0,
            jump: tick == 30,
            ..Default::default()
        };
        sequence[2] += 1;
        s.movement(players[2], sequence[2], input).unwrap();
        s.step().unwrap();
        predictor.step(input).unwrap();
    }
    steps(&mut s, 5);
    let server = position(&s, players[2]);
    let client = Vec3::from(predictor.state().feet);
    assert!(
        server.distance(client) < 0.01,
        "prediction matches the server: {server} vs {client}"
    );

    // A named package archetype is a mini-game player type like v20's.
    let settings = bri_minigames::Settings {
        player_type: "bodies:archetype/ball".into(),
        ..Default::default()
    };
    sequence[0] += 1;
    s.command(
        players[0],
        sequence[0],
        Command::MiniGame(bri_sim::session::MiniGameRequest::Create { color: 0, settings }),
    )
    .unwrap_or_else(|e| panic!("{e:#}"));
    steps(&mut s, 2);
    assert_eq!(archetype_of(&s, players[0]).id, "bodies:archetype/ball");
}

/// Hostile archetypes are refused at load with the reason, never half
/// applied: unknown constants, impossible bodies, bad numbers.
#[test]
fn a_broken_archetype_is_refused_with_its_reason() {
    for (ball, reason) in [
        (
            r#"{ "schema_version": 1, "movement": { "warp_drive": 9.0 } }"#,
            "warp_drive",
        ),
        (
            r#"{ "schema_version": 1, "movement": { "gravity": -20.0 } }"#,
            "Invalid player tuning",
        ),
        (
            r#"{ "schema_version": 1, "movement": { "body": "ball" } }"#,
            "Invalid player tuning",
        ),
        (
            r#"{ "schema_version": 1, "movement": { "forward": "fast" } }"#,
            "movement",
        ),
        (
            r#"{ "schema_version": 1, "max_health": 1e30 }"#,
            "Invalid archetype",
        ),
        (
            r#"{ "schema_version": 1, "base": "bodies:archetype/nowhere" }"#,
            "not a known archetype",
        ),
    ] {
        let error = format!("{:#}", bodies_mode("broken-body", ball).err().unwrap());
        assert!(error.contains(reason), "{reason}: {error}");
    }
}

/// A package body two riders can mount (`numMountPoints = 2`).
const CAMEL_ARCHETYPE: &str = r#"{
  "schema_version": 1,
  "name": "Camel",
  "base": "v20.player.horsearmor",
  "rideable": true,
  "can_ride": false,
  "mount_points": [
    { "node": "hump0", "position": [0.0, 2.0, 0.5] },
    { "node": "hump1", "position": [0.0, 2.0, -0.5], "pose": "sit" }
  ]
}"#;

fn ride(s: &Session, owner: u64) -> Option<bri_sim::session::Ride> {
    s.vitals()[&owner].ride
}

/// Step until `owner` rides, at most `ticks`, returning the ticks it took.
fn until_riding(s: &mut Session, owner: u64, ticks: u32) -> Option<u32> {
    for tick in 0..ticks {
        if ride(s, owner).is_some() {
            return Some(tick);
        }
        s.step().unwrap();
    }
    ride(s, owner).is_some().then_some(ticks)
}

fn become_body(s: &mut Session, owner: u64, sequence: u64, body: &str) {
    s.command(
        owner,
        sequence,
        command("bodies", "become", vec![PackageArg::String(body.into())]),
    )
    .unwrap_or_else(|e| panic!("{e:#}"));
}

/// Where a rider in `seat` of `mount` sits, from the mount's archetype.
fn seat_of(s: &Session, mount: u64, seat: usize) -> Vec3 {
    let state = s
        .snapshot()
        .players
        .into_iter()
        .find(|p| p.owner == mount)
        .unwrap();
    s.archetypes().resolve(state.archetype).mount_points[seat].seat(
        Vec3::from(state.feet),
        state.yaw,
        state.scale,
    )
}

/// Max's a19 playtest: the Horse Ray turned him into a horse and the other
/// player could not get on. v20's `Armor::onCollision` seats a `canRide`
/// player who lands on top of a `rideable` player with mount points; the
/// horse keeps its own controls, the rider moves with it, and jet gets off
/// (`doDismount`, 2.2 up) with `$Game::MinMountTime` before remounting.
#[test]
fn a_player_rides_a_horse_player_and_jets_off() {
    let mut s = bodies_mode("ride-horse", BALL_ARCHETYPE).unwrap();
    let horse = s
        .join("Horse".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    steps(&mut s, 10);
    // A Blockhead is not rideable: landing on one stays on foot.
    let rider = s
        .join("Rider".into(), Vec3::new(0.0, 5.0, 0.0), false)
        .unwrap();
    assert_eq!(until_riding(&mut s, rider, 240), None);
    assert!(position(&s, rider).y > 2.0, "stands on the Blockhead");
    become_body(&mut s, horse, 1, "v20.player.horsearmor");
    // Standing on top as it becomes a horse, the rider takes the one seat.
    s.take_cues();
    assert!(until_riding(&mut s, rider, 240).is_some());
    assert_eq!(
        ride(&s, rider),
        Some(bri_sim::session::Ride {
            mount: horse,
            seat: 0,
            steers: false,
        })
    );
    // Every client checks each cue and drops the connection over a bad one
    // (Max's "Invalid vehicle cue" on v0.1.0-alpha): the mount sound of a
    // player mount must pass that check with no vehicle.
    let cues = s.take_cues();
    for cue in &cues {
        cue.validate().unwrap_or_else(|e| panic!("{e:#}: {cue:?}"));
    }
    assert!(cues.iter().any(|c| matches!(
        &c.kind,
        bri_sim::presentation::CueKind::VehicleSound { sound, .. } if sound == "player.mount"
    )));
    // The horse runs where it looks; the rider's own keys move nothing.
    // The horse's client predicts its run exactly, colliding with players
    // on foot but not with its rider, who is a sensor on the host.
    let (state, _) = s
        .motion_states()
        .into_iter()
        .find(|(p, _)| p.owner == horse)
        .unwrap();
    let mirror = bri_sim::prediction::CollisionMirror::new(
        definitions(),
        vec![ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0))],
        vec![],
    );
    let mut predictor =
        bri_sim::prediction::Predictor::new(mirror, state, s.archetypes().clone()).unwrap();
    let (mut hs, mut rs) = (0, 0);
    let start = position(&s, horse);
    for _ in 0..120 {
        let vitals = s.vitals();
        let others: Vec<_> = s
            .snapshot()
            .players
            .into_iter()
            .filter(|p| {
                let v = &vitals[&p.owner];
                v.alive && v.mounted.is_none() && v.ride.is_none()
            })
            .collect();
        predictor.set_others(&others).unwrap();
        predictor
            .step(MoveInput {
                forward: 1.0,
                yaw: 1.0,
                ..Default::default()
            })
            .unwrap();
        hs += 1;
        rs += 1;
        s.movement(
            horse,
            hs,
            MoveInput {
                forward: 1.0,
                yaw: 1.0,
                ..Default::default()
            },
        )
        .unwrap();
        s.movement(
            rider,
            rs,
            MoveInput {
                right: 1.0,
                yaw: -2.0,
                ..Default::default()
            },
        )
        .unwrap();
        s.step().unwrap();
        assert!(
            position(&s, rider).distance(seat_of(&s, horse, 0)) < 1e-3,
            "the rider sits on the mount node"
        );
    }
    assert!(position(&s, horse).distance(start) > 5.0, "the horse ran");
    let client = Vec3::from(predictor.state().feet);
    assert!(
        position(&s, horse).distance(client) < 0.01,
        "the horse's prediction matches the host: {} vs {client}",
        position(&s, horse)
    );
    // The rider has no control object: its mouse turns its body on the
    // seat by the turn it sends (`mRot.z`, `Player::setPosition`).
    let turn = (heading(&s, rider) - heading(&s, horse) + std::f32::consts::PI)
        .rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    assert!((turn + 2.0).abs() < 1e-4, "turned {turn}");
    // The horse stops; jet gets off, 2.2 above the seat, and landing back
    // on the horse does not remount at once.
    steps(&mut s, 60);
    let seat = seat_of(&s, horse, 0);
    for jet in [false, true] {
        rs += 1;
        s.movement(
            rider,
            rs,
            MoveInput {
                jet,
                ..Default::default()
            },
        )
        .unwrap();
        s.step().unwrap();
    }
    assert_eq!(ride(&s, rider), None);
    assert!(
        position(&s, rider).y > seat.y + 1.5,
        "{}",
        position(&s, rider)
    );
    for _ in 0..100 {
        s.step().unwrap();
        assert_eq!(ride(&s, rider), None);
    }
}

/// Riders get off whenever the mount stops being one: a new body that is
/// not rideable, death, a disconnect. A rider who dies is off too, and the
/// mount carries on.
#[test]
fn riders_are_put_down_when_the_mount_changes_dies_or_leaves() {
    let mut s = bodies_mode("ride-cleanup", CAMEL_ARCHETYPE).unwrap();
    let horse = s
        .join("Horse".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    become_body(&mut s, horse, 1, "v20.player.horsearmor");
    steps(&mut s, 10);
    let rider = s
        .join("Rider".into(), Vec3::new(0.0, 6.0, 0.0), false)
        .unwrap();
    assert!(until_riding(&mut s, rider, 240).is_some());
    // A body with seats keeps the rider in theirs (`onNewDataBlock`).
    become_body(&mut s, horse, 2, "bodies:archetype/ball");
    steps(&mut s, 1);
    assert_eq!(ride(&s, rider).map(|r| r.seat), Some(0));
    assert!(position(&s, rider).distance(seat_of(&s, horse, 0)) < 1e-3);
    // One that is not rideable puts them down.
    become_body(&mut s, horse, 3, "v20.player.playerstandardarmor");
    steps(&mut s, 1);
    assert_eq!(ride(&s, rider), None);
    // The mount dies: `Armor::onDisabled` forces its riders off.
    become_body(&mut s, horse, 4, "v20.player.horsearmor");
    assert!(until_riding(&mut s, rider, 400).is_some());
    s.command(horse, 5, Command::Suicide).unwrap();
    steps(&mut s, 1);
    assert!(!s.is_alive(horse));
    assert_eq!(ride(&s, rider), None);
    assert!(s.is_alive(rider));
    // A rider who dies is off; the mount is unharmed.
    let mount = s
        .join("Mount".into(), Vec3::new(10.0, 0.05, 0.0), false)
        .unwrap();
    become_body(&mut s, mount, 1, "v20.player.horsearmor");
    let second = s
        .join("Second".into(), Vec3::new(10.0, 6.0, 0.0), false)
        .unwrap();
    assert!(until_riding(&mut s, second, 240).is_some());
    s.command(second, 1, Command::Suicide).unwrap();
    steps(&mut s, 1);
    assert_eq!(ride(&s, second), None);
    assert!(s.is_alive(mount));
    // The mount leaves: its rider stays behind.
    let mount = s
        .join("Leaver".into(), Vec3::new(20.0, 0.05, 0.0), false)
        .unwrap();
    become_body(&mut s, mount, 1, "v20.player.horsearmor");
    let third = s
        .join("Third".into(), Vec3::new(20.0, 6.0, 0.0), false)
        .unwrap();
    assert!(until_riding(&mut s, third, 240).is_some());
    s.disconnect(mount).unwrap();
    steps(&mut s, 1);
    assert_eq!(ride(&s, third), None);
    steps(&mut s, 120);
    assert!(s.is_alive(third));
}

/// `numMountPoints` from a package body: two humps, two riders, and a third
/// who finds no free seat stays on foot.
#[test]
fn a_package_mount_seats_as_many_riders_as_it_has_mount_points() {
    let mut s = bodies_mode("ride-camel", CAMEL_ARCHETYPE).unwrap();
    let camel = s
        .join("Camel".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    become_body(&mut s, camel, 1, "bodies:archetype/ball");
    steps(&mut s, 10);
    let riders: Vec<u64> = ["A", "B", "C"]
        .iter()
        .map(|name| {
            let rider = s
                .join((*name).into(), Vec3::new(0.0, 7.0, 0.0), false)
                .unwrap();
            until_riding(&mut s, rider, 240);
            rider
        })
        .collect();
    assert_eq!(ride(&s, riders[0]).map(|r| r.seat), Some(0));
    assert_eq!(ride(&s, riders[1]).map(|r| r.seat), Some(1));
    assert_eq!(ride(&s, riders[2]), None);
    for (seat, rider) in riders[..2].iter().enumerate() {
        assert!(position(&s, *rider).distance(seat_of(&s, camel, seat)) < 1e-3);
    }
}

/// `miniGameCanUse`: a horse in a minigame does not carry a player outside
/// it.
#[test]
fn a_horse_in_a_minigame_only_carries_its_own_players() {
    let mut s = bodies_mode("ride-minigame", BALL_ARCHETYPE).unwrap();
    s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 0.0)]).unwrap();
    let horse = s
        .join("Horse".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    become_body(&mut s, horse, 1, "v20.player.horsearmor");
    s.command(
        horse,
        2,
        Command::MiniGame(bri_sim::session::MiniGameRequest::Create {
            color: 1,
            settings: Default::default(),
        }),
    )
    .unwrap();
    steps(&mut s, 10);
    assert_eq!(archetype_of(&s, horse).id, "v20.player.horsearmor");
    let rider = s
        .join("Rider".into(), Vec3::new(0.0, 6.0, 0.0), false)
        .unwrap();
    assert_eq!(until_riding(&mut s, rider, 240), None);
    assert!(position(&s, rider).y > 2.0, "stands on the horse");
}
