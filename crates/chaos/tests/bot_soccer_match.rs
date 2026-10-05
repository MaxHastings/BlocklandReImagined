//! Two against two on a built soccer field: walls of stock 4x cubes, a goal
//! pocket at each end whose detection region runs the Ball goals recipe's
//! plain wrench events, one Steel Ball on a centre spawn pad and four
//! Blockhead Bot pads, two per team, each brick choosing its bot's team.
//!
//! The matches run headless for minutes of game time over fixed seeds and
//! measure what a viewer would notice: goals per team, own goals, time
//! nobody touches the ball, a ball stuck on a wall or in a corner, bots
//! standing still away from play, everyone clumped on the ball, slow
//! pick-up after a reset, and bots facing away from a ball they are
//! working. The thresholds guard the match quality.
//!
//! `BRI_BLESS=1` also writes the field as a save Max loads with Load Bricks
//! (see `docs/progress/2026-10-04-claude-bots-ball-games.md`).
use bri_events::rules::{Compare, Condition, Datum, Property, Subject};
use bri_events::{Row, Slot, Target};
use bri_sim::definitions::Definitions;
use bri_sim::player::MoveInput;
use bri_sim::session::{Command, MiniGameRequest, Reply, Session, TeamEdit, ToolCatalog};
use bri_sim::simulation::Simulation;
use bri_sim::testing::definition;
use bri_world::{Brick, ContentRef, OwnerId, VehicleSpawn, World, build::SavedBuild};
use glam::Vec3;
use rapier3d::prelude::*;
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

/// The Steel Ball Kit's ball, as the package ships it.
const BALL: &str = "steel-ball-kit:vehicle/steelball";
/// v20 bricks the field is made of (the stock 4x Cube, Vehicle Spawn and a
/// 4x4 plate), so the save loads on any copy of the game.
const CUBE: &str = "v20/brick/brick4xcubedata";
const PAD: &str = "v20/brick/brickvehiclespawndata";
const MARK: &str = "v20/brick/brick4x4fdata";
/// The Blockhead Bot Add-On's kind.
const BOT: &str = "bot.blockhead";
/// v20's Slate: a flat floor at height 0.
const SLATE: &str = "v20/add-ons/map_slate/slate.mis";
const KICKOFF: &str = "kickoff";
const TICKS_PER_SECOND: usize = 120;

/// The field's inside: x across, z from the south goal to the north one.
const HALF_X: f32 = 12.0;
const HALF_Z: f32 = 20.0;
/// Half the goal mouth's width, and the pocket's depth behind the line.
const MOUTH: f32 = 4.0;
const POCKET: f32 = 4.0;

/// The team slots the save's game makes: Blue first, then Red.
const BLUE: u32 = 1;
const RED: u32 = 2;

/// Paint: red, blue and a light grey for the walls.
const PALETTE: [[f32; 4]; 3] = [
    [0.8, 0.1, 0.1, 1.0],
    [0.1, 0.25, 0.8, 1.0],
    [0.75, 0.75, 0.75, 1.0],
];
const RED_PAINT: u8 = 0;
const BLUE_PAINT: u8 = 1;
const WALL: u8 = 2;

fn brick(id: &str, at: [f32; 3]) -> Brick {
    Brick::new(ContentRef::Resolved(id.into()), at, 0)
}

/// The jeep (a synthetic stand-in with v20's id) for the jeep run.
const JEEP: &str = bri_vehicles::testing::CAR;
/// A synthetic ramp for the obstacle variation (not in the shipped save).
const RAMP: &str = "test/brick/pitch-ramp";

/// One match's line-up and pitch.
#[derive(Clone, Debug)]
struct Setup {
    kit: Kit,
    blue: usize,
    red: usize,
    /// A short wall and a ramp in midfield.
    obstacles: bool,
    /// A jeep on a pad at each touchline.
    jeeps: bool,
    /// Per pad, Blue's then Red's: a shift of its position in world units.
    jitter: Vec<[f32; 2]>,
}

impl Setup {
    fn new(kit: Kit, blue: usize, red: usize) -> Self {
        Self {
            kit,
            blue,
            red,
            obstacles: false,
            jeeps: false,
            jitter: vec![[0.0; 2]; blue + red],
        }
    }
    /// Pad shifts on the half studs a builder could place them, from a seed.
    fn seeded(mut self, seed: u64) -> Self {
        let mut rng = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        let mut next = || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            ((rng >> 33) % 9) as f32 * 0.5 - 2.0
        };
        self.jitter = (0..self.blue + self.red)
            .map(|_| [next(), next()])
            .collect();
        self
    }
    fn jeeps(mut self) -> Self {
        self.jeeps = true;
        self
    }
    fn obstacles(mut self) -> Self {
        self.obstacles = true;
        self
    }
    /// Each bot pad: its name, where it stands and its team.
    fn pads(&self) -> Vec<(String, [f32; 2], u32)> {
        let row = |n: usize| -> Vec<f32> {
            match n {
                0 => vec![],
                1 => vec![0.0],
                n => (0..n)
                    .map(|i| -8.0 + 16.0 * i as f32 / (n - 1) as f32)
                    .collect(),
            }
        };
        let mut pads = Vec::new();
        for (team, name, n, z) in [
            (BLUE, "blue", self.blue, -10.0),
            (RED, "red", self.red, 10.0),
        ] {
            for (i, x) in row(n).into_iter().enumerate() {
                pads.push((format!("{name}_{}", i + 1), [x, z], team));
            }
        }
        for (pad, [dx, dz]) in pads.iter_mut().zip(&self.jitter) {
            pad.1[0] += dx;
            pad.1[1] += dz;
        }
        pads
    }
}

/// The field's bricks: two rows of 4x cubes round the pitch, a pocket
/// behind each goal mouth, the goals' event bricks, the kickoff pad and a
/// pad per bot. Blue defends the south goal and scores in the north one.
fn field(setup: &Setup) -> Vec<Brick> {
    let mut bricks = Vec::new();
    let mut wall = |x: f32, z: f32| {
        for y in [1.0, 3.0] {
            let mut b = brick(CUBE, [x, y, z]);
            b.color = WALL;
            bricks.push(b);
        }
    };
    // Long sides, corner to corner.
    let mut z = -HALF_Z - 1.0;
    while z <= HALF_Z + 1.0 {
        wall(-HALF_X - 1.0, z);
        wall(HALF_X + 1.0, z);
        z += 2.0;
    }
    for end in [-1.0f32, 1.0] {
        let line = end * (HALF_Z + 1.0);
        let mut x = -HALF_X + 1.0;
        while x < HALF_X {
            if x.abs() > MOUTH {
                wall(x, line);
            }
            x += 2.0;
        }
        // Corner fillers, so a ball is not trapped square in a corner.
        for (dx, dz) in [(1.0, 1.0), (3.0, 1.0), (1.0, 3.0)] {
            for side in [-1.0f32, 1.0] {
                wall(side * (HALF_X - dx), end * (HALF_Z - dz));
            }
        }
        // The pocket: its sides and back.
        let mut d = 2.0;
        while d <= POCKET + 1.0 {
            wall(-MOUTH - 1.0, end * (HALF_Z + 1.0 + d));
            wall(MOUTH + 1.0, end * (HALF_Z + 1.0 + d));
            d += 2.0;
        }
        let back = end * (HALF_Z + POCKET + 3.0);
        let mut x = -MOUTH + 1.0;
        while x < MOUTH {
            wall(x, back);
            x += 2.0;
        }
    }
    // The goals: a plate in each pocket whose detection region fills it.
    for (end, name, scoring) in [(1.0f32, "north_goal", BLUE), (-1.0, "south_goal", RED)] {
        let mut goal = brick(MARK, [0.0, 0.1, end * (HALF_Z + 1.0 + POCKET / 2.0)]);
        goal.name = Some(name.into());
        goal.colliding = false;
        goal.raycast = false;
        goal.color = if scoring == BLUE {
            RED_PAINT
        } else {
            BLUE_PAINT
        };
        goal.rule_region = Some([MOUTH * 2.0, 4.0, POCKET + 1.0]);
        goal.events = recipe_rows(scoring);
        bricks.push(goal);
    }
    let pad = |kind: &str, at: [f32; 3], name: &str, team: Option<u32>| {
        let mut b = brick(PAD, at);
        b.name = Some(name.into());
        b.vehicle = Some(Box::new(VehicleSpawn {
            vehicle: ContentRef::Resolved(kind.into()),
            recolor: false,
            team,
        }));
        b
    };
    bricks.push(pad(BALL, [0.0, 0.1, 0.0], KICKOFF, None));
    for (name, [x, z], team) in setup.pads() {
        bricks.push(pad(BOT, [x, 0.1, z], &name, Some(team)));
    }
    if setup.jeeps {
        for (x, name) in [(-8.0, "west_jeep"), (8.0, "east_jeep")] {
            bricks.push(pad(JEEP, [x, 0.1, 0.0], name, None));
        }
    }
    if setup.obstacles {
        // A short wall left of centre and a low ramp right of it, facing
        // the south goal.
        for x in [-7.0, -5.0] {
            let mut b = brick(CUBE, [x, 1.0, 3.0]);
            b.color = WALL;
            bricks.push(b);
        }
        let mut ramp = brick(RAMP, [6.0, 0.3, -3.0]);
        ramp.color = WALL;
        bricks.push(ramp);
    }
    bricks
}

/// The Ball goals recipe for one goal: the ball entering it gives the
/// instigator's team a point when they play for `scoring`, wins the round
/// at five, and puts the ball back on its pad three seconds later.
fn recipe_rows(scoring: u32) -> Vec<Row> {
    let condition = |subject, property, compare, value| Condition {
        subject,
        property,
        key: String::new(),
        compare,
        value,
    };
    let object = || {
        vec![
            condition(
                Subject::Object,
                Property::Kind,
                Compare::Equal,
                Datum::Text(BALL.into()),
            ),
            condition(
                Subject::Object,
                Property::SpawnedBy,
                Compare::Equal,
                Datum::Text(KICKOFF.into()),
            ),
        ]
    };
    let guards = |extra: Option<Condition>| {
        let mut c = vec![
            condition(
                Subject::MiniGame,
                Property::RoundOver,
                Compare::Equal,
                Datum::Bool(false),
            ),
            bri_events::rules::default_condition(),
        ];
        c.extend(extra);
        c.push(condition(
            Subject::Instigator,
            Property::Team,
            Compare::Equal,
            Datum::Number(i64::from(scoring)),
        ));
        c.extend(object());
        c
    };
    let row = |output: &str, target, params, conditions, delay_ms| Row {
        enabled: true,
        input: "onObjectEnter".into(),
        output: output.into(),
        target: Target::Slot(target),
        params,
        conditions,
        delay_ms,
        preserved: None,
    };
    vec![
        row(
            "addTeamScore",
            Slot::Instigator,
            vec![bri_events::Value::Int(1)],
            guards(None),
            0,
        ),
        row(
            "winRound",
            Slot::Instigator,
            vec![],
            guards(Some(condition(
                Subject::Team,
                Property::Score,
                Compare::AtLeast,
                Datum::Number(5),
            ))),
            0,
        ),
        row("resetObject", Slot::Object, vec![], object(), 3000),
    ]
}

/// The Vehicle Spawn's footprint and height in these tests: v20's 8x8
/// plate is assumed; the real one comes with the generated content.
const PAD_SIZE: ([u8; 2], u16) = ([8, 8], 1);

/// The bricks the field uses, as boxes of their stock sizes.
fn definitions() -> Definitions {
    definitions_with(PAD_SIZE)
}

/// [`definitions`] with a Vehicle Spawn of another footprint and height.
fn definitions_with((footprint, plates): ([u8; 2], u16)) -> Definitions {
    use bri_sim::definitions::Special;
    let mut d = bri_sim::testing::definitions();
    for def in [
        definition(CUBE, [4, 4], 10, Special::None, false),
        definition(PAD, footprint, plates, Special::None, true),
        definition(MARK, [4, 4], 1, Special::None, false),
        bri_sim::testing::ramp(RAMP, [8, 8], 3),
    ] {
        d.entries.insert(def.mesh.id.clone(), def);
    }
    d
}

/// A server Add-On, so the game has Add-On settings to carry teams.
fn addon() -> Arc<bri_package_runtime::Catalog> {
    use bri_package::packages::{PackageEntry, PackageSet, Side};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "bri-soccer-match-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let dir = root.join("pitch");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("package.json"), json!({"schema_version":1,"id":"pitch","version":"1.0.0","api":1,"name":"Pitch fixture","license":"CC0-1.0","capabilities":["minigame"],"provides":[{"kind":"behaviour","id":"pitch:behaviour/main","file":"behaviour.json"},{"kind":"script","id":"pitch:script/main","file":"main.rhai"}]}).to_string()).unwrap();
    std::fs::write(
        dir.join("behaviour.json"),
        json!({"schema_version":1,"script":"main.rhai","settings":[{"key":"halves","title":"Halves","scope":"minigame","type":"bool","default":false}]}).to_string(),
    )
    .unwrap();
    std::fs::write(dir.join("main.rhai"), "fn unused() { }\n").unwrap();
    let catalog = bri_package_runtime::Catalog::load(
        &root,
        &PackageSet {
            schema_version: 1,
            packages: vec![PackageEntry {
                id: "pitch".into(),
                version: "1.0.0".into(),
                side: Side::Server,
                dir: "pitch".into(),
                role: None,
            }],
        },
        true,
    )
    .unwrap();
    let _ = std::fs::remove_dir_all(&root);
    Arc::new(catalog)
}

/// What the bots hold.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kit {
    Hands,
    Hammer,
    /// A rocket launcher: impulse at range.
    Rocket,
    /// A thrown spear.
    Spear,
}

struct Match {
    s: Session,
    host: OwnerId,
    seq: u64,
    command: u64,
}

impl Match {
    /// A session on the Slate with the field's bricks, the Blockhead Bot
    /// and the Steel Ball Kit, and the host in the stands; no game yet.
    fn base() -> Self {
        Self::base_with(definitions())
    }
    /// [`Self::base`] on bricks of the given sizes.
    fn base_with(definitions: Definitions) -> Self {
        let world = World::new("Soccer".into(), SLATE.into(), PALETTE.to_vec());
        let floor = vec![
            ColliderBuilder::cuboid(200.0, 0.5, 200.0).translation(Vector::new(0.0, -0.5, 0.0)),
        ];
        let mut s = Session::new(Simulation::new(world, definitions, floor).unwrap());
        // A field loads at a fixed pace, not as fast as this machine can
        // place bricks: the match then plays the same on every machine.
        s.set_load_pace(bri_sim::session::LoadPace::Bricks(4096));
        s.set_weapon_pack(bri_weapons::testing::pack()).unwrap();
        let mut vehicles = bri_vehicles::testing::pack();
        let kit_pack = bri_vehicles::Pack::load(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../packages/showcase/steel-ball-kit/assets/vehicles.json"),
        )
        .unwrap();
        vehicles.definitions.extend(kit_pack.definitions);
        s.set_vehicle_pack(
            vehicles,
            bri_sim::bot_kind::BotPack::from_json(include_bytes!(
                "../../../packages/blockhead_bot/assets/bots.json"
            ))
            .unwrap()
            .bots,
        )
        .unwrap();
        s.set_event_catalog(bri_events::testing::catalog(), Vec::<String>::new())
            .unwrap();
        s.set_tool_catalog(ToolCatalog {
            vehicles: [BOT.to_string(), BALL.to_string(), JEEP.to_string()].into(),
            vehicle_bricks: [PAD.into()].into(),
            ..Default::default()
        })
        .unwrap();
        s.install_packages(addon(), None).unwrap();
        // The host watches from the stands, out of play.
        let stands = Vec3::new(HALF_X + 12.0, 0.05, 0.0);
        s.set_spawn_points(vec![stands]).unwrap();
        let host = s.join("Max".into(), stands, true).unwrap();
        Self {
            s,
            host,
            seq: 1 << 40,
            command: 100,
        }
    }

    /// The host makes the game (teams Blue and Red), then loads the field.
    fn new(setup: &Setup) -> Self {
        let kit = setup.kit;
        let mut m = Self::base();
        m.run(Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: bri_minigames::Settings {
                title: "Soccer".into(),
                loadout: match kit {
                    Kit::Hands => [None, None, None, None, None],
                    Kit::Hammer => [Some(bri_weapons::HAMMER.into()), None, None, None, None],
                    Kit::Rocket => [
                        Some(bri_weapons::testing::ROCKET_ITEM.into()),
                        None,
                        None,
                        None,
                        None,
                    ],
                    Kit::Spear => [
                        Some(bri_weapons::testing::SPEAR_ITEM.into()),
                        None,
                        None,
                        None,
                        None,
                    ],
                },
                weapon_damage: false,
                // Moving the ball is vehicle damage policy: on, or nobody may push it.
                vehicle_damage: true,
                brick_damage: false,
                falling_damage: false,
                ..Default::default()
            },
        }));
        let game = m.game();
        m.run(Command::MiniGame(MiniGameRequest::AddOnSettings {
            game,
            settings: vec![],
            teams: Some(
                [("Blue", 3u8), ("Red", 0u8)]
                    .into_iter()
                    .map(|(name, color)| TeamEdit {
                        id: None,
                        name: name.into(),
                        color,
                        settings: vec![],
                    })
                    .collect(),
            ),
            quiet: true,
            reset: false,
        }));
        let teams: Vec<_> = m.s.minigame_views()[0]
            .teams
            .iter()
            .map(|t| (t.name.clone(), t.id.0))
            .collect();
        assert_eq!(
            teams,
            [("Blue".to_string(), BLUE), ("Red".to_string(), RED)],
            "the field's rows name the game's first two team slots"
        );
        let mut world = World::new(
            format!("Soccer {}v{}", setup.blue, setup.red),
            SLATE.into(),
            m.s.simulation().state().palette.clone(),
        );
        for (i, mut b) in field(setup).into_iter().enumerate() {
            b.owner = m.host;
            world.bricks.insert(i as u64 + 1, b);
        }
        world.next_brick_id = world.bricks.len() as u64 + 1;
        m.run(Command::LoadBuild {
            build: Box::new(SavedBuild::new(world)),
            ownership: false,
        });
        m.steps(TICKS_PER_SECOND);
        let mut placed: BTreeMap<String, usize> = BTreeMap::new();
        for b in m.s.simulation().state().bricks.values() {
            let ContentRef::Resolved(id) = &b.definition else {
                continue;
            };
            *placed.entry(id.clone()).or_default() += 1;
        }
        assert_eq!(
            placed.values().sum::<usize>(),
            field(setup).len(),
            "every field brick is placed: {placed:?} {:?}",
            m.s.snapshot().chat
        );
        m
    }
    /// The host loads a saved field with Load Bricks, running no game: the
    /// save brings its own.
    fn from_save(bytes: &[u8]) -> Self {
        let mut m = Self::base();
        let build = bri_world::build::decode(bytes).expect("the save decodes");
        let bricks = build.world.bricks.len();
        m.run(Command::LoadBuild {
            build: Box::new(build),
            ownership: false,
        });
        m.steps(TICKS_PER_SECOND);
        assert_eq!(
            m.s.simulation().state().bricks.len(),
            bricks,
            "every brick of the save is placed: {:?}",
            m.s.snapshot().chat
        );
        m
    }
    /// The field and its game as Save Bricks writes them.
    fn save(&mut self) -> Vec<u8> {
        let Reply::Saved(build) = self.run(Command::SaveBuild {
            events: true,
            ownership: false,
        }) else {
            panic!("Save Bricks saves");
        };
        assert!(build.minigame.is_some(), "the save carries its game");
        bri_world::build::encode(&build).unwrap()
    }
    fn run(&mut self, command: Command) -> Reply {
        self.command += 1;
        self.s.command(self.host, self.command, command).unwrap()
    }
    fn game(&self) -> u64 {
        self.s.minigame_views()[0].id
    }
    fn steps(&mut self, n: usize) {
        for _ in 0..n {
            self.seq += 1;
            self.s
                .movement(self.host, self.seq, MoveInput::default())
                .unwrap();
            self.s.step().unwrap();
        }
    }
    fn bots(&self) -> Vec<OwnerId> {
        self.s
            .names()
            .keys()
            .copied()
            .filter(|o| self.s.is_bot(*o))
            .collect()
    }
    fn ball(&self) -> Option<(u64, Vec3, Vec3)> {
        let id = self
            .s
            .vehicle_infos()
            .iter()
            .find(|v| v.definition == BALL && !v.destroyed)?
            .id;
        let pose = self.s.vehicle_poses().into_iter().find(|v| v.id == id)?;
        Some((id, pose.position.into(), pose.velocity.into()))
    }
}

/// What a match looked like.
#[derive(Debug, Default, Clone)]
struct Report {
    seconds: f32,
    goals: BTreeMap<u32, u32>,
    own_goals: u32,
    /// Seconds no bot was within `ATTENDED` of a ball in play.
    untouched: f32,
    /// Longest such stretch.
    longest_untouched: f32,
    /// Share of ball-in-play time some bot was in contact range of it.
    contact: f32,
    /// Seconds the ball sat slow against a wall or in a corner.
    ball_walled: f32,
    longest_ball_walled: f32,
    /// Seconds the ball was missing or out of the pitch outside a reset.
    ball_lost: f32,
    /// Bot-seconds wanting to go somewhere more than a stride away while
    /// making no headway over `WINDOW` seconds.
    stuck: f32,
    /// Bot-seconds turning a full circle or more within `WINDOW` seconds
    /// while getting nowhere.
    circling: f32,
    /// Quick turn reversals (left then right, each sharp), per bot-minute.
    jitter: f32,
    /// Bot-seconds wandering or walking home with a ball in play.
    ignoring: f32,
    /// Seconds two teammates stood on each other at the ball.
    clumped: f32,
    /// Bot-seconds standing still well away from the ball.
    idle: f32,
    /// Slowest pick-up of a fresh ball after a reset, in seconds.
    slowest_kickoff: f32,
    /// Share of contact time a bot faced away from the ball.
    facing_away: f32,
    /// Share of contact time a bot was behind the ball, pushing it toward
    /// its own goal.
    wrong_way: f32,
    /// Bot-seconds spent fighting or chasing an opponent.
    combat: f32,
    /// Rounds won (five goals); the host resets the game after each, as
    /// the Mini-Game window's Reset does.
    rounds: u32,
}

/// Contact range: a Blockhead's half width plus the ball's radius, with
/// some slack.
const CONTACT: f32 = 2.4;
/// A ball with a bot this near is being played.
const ATTENDED: f32 = 6.0;
/// Seconds over which a bot's headway and turning are judged.
const WINDOW: f32 = 2.0;
/// A sharp turn, in degrees per sample, for jitter.
const SHARP: f32 = 8.0;

fn play(setup: &Setup, seconds: usize) -> Report {
    play_match(Match::new(setup), setup, seconds)
}

fn play_match(mut m: Match, setup: &Setup, seconds: usize) -> Report {
    let trace = std::env::var_os("BRI_SOCCER_TRACE").is_some();
    let bots = m.bots();
    assert_eq!(
        bots.len(),
        setup.blue + setup.red,
        "a bot per pad: {:?}",
        m.s.names()
    );
    let team = |m: &Match, bot| m.s.vitals().get(&bot).and_then(|v| v.team);
    let teams: BTreeMap<OwnerId, u32> = bots
        .iter()
        .map(|b| (*b, team(&m, *b).expect("each bot plays for its pad's team")))
        .collect();
    assert_eq!(
        teams.values().filter(|t| **t == BLUE).count(),
        setup.blue,
        "each pad's team: {teams:?}"
    );
    let goal_z = |t: u32| if t == BLUE { HALF_Z } else { -HALF_Z };
    let mut r = Report::default();
    let sample = 4usize;
    let dt = sample as f32 / TICKS_PER_SECOND as f32;
    let mut last_score = BTreeMap::new();
    let mut won_at: Option<f32> = None;
    let mut untouched_run = 0.0f32;
    let mut walled_run = 0.0f32;
    let mut in_play = 0.0f32;
    let mut touching = 0.0f32;
    let mut absent_since: Option<f32> = None;
    let window = (WINDOW / dt).round() as usize;
    // Per bot: recent (feet, yaw) samples and its last turn.
    let mut history: BTreeMap<OwnerId, std::collections::VecDeque<(Vec3, f32)>> = BTreeMap::new();
    let mut last_turn: BTreeMap<OwnerId, f32> = BTreeMap::new();
    let mut reversals = 0u32;
    let mut ball_id = None;
    let mut fresh_since: Option<f32> = None;
    let mut in_pocket = false;
    // The team of the bot that last had the ball in contact range: a ball
    // it sends into the goal it defends is an own goal.
    let mut last_touch: Option<u32> = None;
    let mut last_seen: Option<String> = None;
    // A pocket entry with no goal within a second and a half either side
    // of it was credited to the defending side: an own goal.
    let mut entered: Option<f32> = None;
    let mut last_goal = f32::NEG_INFINITY;
    let mut contact = 0.0f32;
    let mut facing_away = 0.0f32;
    let mut wrong_way = 0.0f32;
    let mut last_feet: BTreeMap<OwnerId, Vec3> = BTreeMap::new();
    for step in 0..seconds * TICKS_PER_SECOND / sample {
        m.steps(sample);
        let now = step as f32 * dt;
        let snapshot = m.s.snapshot();
        let feet: BTreeMap<OwnerId, (Vec3, f32)> = snapshot
            .players
            .iter()
            .filter(|p| teams.contains_key(&p.owner))
            .map(|p| (p.owner, (Vec3::from(p.feet), p.yaw)))
            .collect();
        let thoughts = m.s.bot_thoughts();
        if trace && step % (TICKS_PER_SECOND / sample) == 0 {
            let ball = m
                .ball()
                .map(|(_, at, v)| (at.x as i32, at.z as i32, v.length() as i32));
            let line: Vec<String> = thoughts
                .iter()
                .map(|t| {
                    let f = feet.get(&t.bot).map(|(f, _)| (f.x as i32, f.z as i32));
                    format!(
                        "{}@{f:?} {} {:?} {:?}",
                        t.bot,
                        t.behaviour,
                        t.objective_detail.as_ref().map(|d| (d.provider, d.phase)),
                        t.objective_diagnostic
                    )
                })
                .collect();
            println!("t={now:.0} ball={ball:?} | {}", line.join(" | "));
        }
        for t in &thoughts {
            if matches!(t.behaviour, "fight" | "chase") {
                r.combat += dt;
            }
            let Some(&(f, yaw)) = feet.get(&t.bot) else {
                continue;
            };
            let past = history.entry(t.bot).or_default();
            if let Some(&(_, before)) = past.back() {
                let turn = wrap(yaw - before).to_degrees();
                let last = last_turn.insert(t.bot, turn).unwrap_or(0.0);
                if turn.abs() > SHARP && last.abs() > SHARP && turn.signum() != last.signum() {
                    reversals += 1;
                }
            }
            past.push_back((f, yaw));
            if past.len() > window + 1 {
                past.pop_front();
            }
            if past.len() > window {
                let (start, _) = past[0];
                let headway = Vec3::new(f.x - start.x, 0.0, f.z - start.z).length();
                let turned: f32 = past
                    .iter()
                    .zip(past.iter().skip(1))
                    .map(|(a, b)| wrap(b.1 - a.1).abs())
                    .sum();
                let wants = t
                    .goal
                    .is_some_and(|g| Vec3::new(g[0] - f.x, 0.0, g[2] - f.z).length() > 1.5);
                if wants && headway < 0.5 {
                    r.stuck += dt;
                }
                if turned > std::f32::consts::TAU && headway < 2.0 {
                    r.circling += dt;
                }
            }
        }
        let wandering = thoughts
            .iter()
            .filter(|t| matches!(t.behaviour, "wander" | "return"))
            .count() as f32;
        // Scores: a goal is a team score rising.
        let vitals = m.s.vitals();
        let mut scores: BTreeMap<u32, i64> = BTreeMap::new();
        for (bot, t) in &teams {
            *scores.entry(*t).or_default() += vitals.get(bot).map_or(0, |v| v.score);
        }
        for (t, s) in &scores {
            let before = last_score.insert(*t, *s).unwrap_or(0);
            if *s > before {
                *r.goals.entry(*t).or_default() += (*s - before) as u32;
                last_goal = now;
            }
        }
        if let Some(since) = entered
            && now - since > 1.5
        {
            entered = None;
            if (now - last_goal) > 3.0 {
                r.own_goals += 1;
                if trace {
                    println!("own goal t={now:.1} last touch {last_touch:?} {last_seen:?}");
                }
            }
        }
        // Five wins the round; the host presses Reset three seconds later.
        if won_at.is_none() && scores.values().any(|s| *s >= 5) {
            won_at = Some(now);
            r.rounds += 1;
        }
        if won_at.is_some_and(|at| now - at >= 3.0) {
            won_at = None;
            m.run(Command::MiniGame(MiniGameRequest::Reset));
        }
        let Some((id, at, velocity)) = m.ball() else {
            // Between a goal and its reset (3 s) the ball may be gone; any
            // longer is a lost ball.
            let since = *absent_since.get_or_insert(now);
            if now - since > 3.5 {
                r.ball_lost += dt;
            }
            continue;
        };
        absent_since = None;
        if ball_id != Some(id) {
            ball_id = Some(id);
            fresh_since = Some(now);
            in_pocket = false;
            last_touch = None;
        }
        let pocket = at.z.abs() > HALF_Z + 0.5 && at.x.abs() < MOUTH;
        if pocket && !in_pocket {
            in_pocket = true;
            if won_at.is_none() {
                entered = Some(now);
            }
        }
        if in_pocket {
            continue;
        }
        if at.x.abs() > HALF_X + 1.0 || at.z.abs() > HALF_Z + 1.0 || at.y < -1.0 {
            r.ball_lost += dt;
        }
        in_play += dt;
        if won_at.is_none() {
            r.ignoring += dt * wandering;
        }
        let near: Vec<OwnerId> = feet
            .iter()
            .filter(|(_, (f, _))| Vec3::new(f.x - at.x, 0.0, f.z - at.z).length() < CONTACT)
            .map(|(b, _)| *b)
            .collect();
        if !near.is_empty() {
            touching += dt;
            let closest = near.iter().min_by(|a, b| {
                let d =
                    |o: &OwnerId| Vec3::new(feet[o].0.x - at.x, 0.0, feet[o].0.z - at.z).length();
                d(a).total_cmp(&d(b))
            });
            last_touch = closest.map(|b| teams[b]);
            if trace {
                last_seen = closest
                    .and_then(|b| thoughts.iter().find(|t| t.bot == *b))
                    .map(|t| {
                        format!(
                            "{} {} goal {:?} feet {:?} ball {at:?} v {velocity:?}",
                            t.bot, t.behaviour, t.goal, feet[&t.bot].0
                        )
                    });
            }
        }
        let attended = feet
            .values()
            .any(|(f, _)| Vec3::new(f.x - at.x, 0.0, f.z - at.z).length() < ATTENDED);
        if !attended {
            r.untouched += dt;
            untouched_run += dt;
            r.longest_untouched = r.longest_untouched.max(untouched_run);
        } else {
            untouched_run = 0.0;
            if let Some(since) = fresh_since.take() {
                r.slowest_kickoff = r.slowest_kickoff.max(now - since);
            }
        }
        for b in &near {
            let (f, yaw) = feet[b];
            contact += dt;
            let to_ball = Vec3::new(at.x - f.x, 0.0, at.z - f.z).normalize_or_zero();
            let facing = Vec3::new(yaw.sin(), 0.0, -yaw.cos());
            if facing.dot(to_ball) < 0.0 {
                facing_away += dt;
            }
            // Pushing it toward its own goal: behind the ball as it moves
            // away from the goal it attacks.
            let attack = goal_z(teams[b]);
            let moving = Vec3::new(velocity.x, 0.0, velocity.z);
            if moving.length() > 1.0
                && to_ball.dot(moving.normalize()) > 0.5
                && (attack - at.z).signum() * velocity.z < -0.5
            {
                wrong_way += dt;
            }
        }
        // Teammates stacked on each other at the ball.
        let at_ball = |f: Vec3| Vec3::new(f.x - at.x, 0.0, f.z - at.z).length() < 5.0;
        if feet.iter().any(|(a, (fa, _))| {
            feet.iter().any(|(b, (fb, _))| {
                a < b
                    && teams[a] == teams[b]
                    && at_ball(*fa)
                    && at_ball(*fb)
                    && Vec3::new(fa.x - fb.x, 0.0, fa.z - fb.z).length() < 2.0
            })
        }) {
            r.clumped += dt;
            if trace {
                let who: Vec<String> = thoughts
                    .iter()
                    .map(|t| {
                        format!(
                            "{} {} {:?}",
                            t.bot,
                            t.behaviour,
                            t.goal.map(|g| (g[0] as i32, g[2] as i32))
                        )
                    })
                    .collect();
                println!("clump t={now:.2} ball={at:?} {who:?} feet={feet:?}");
            }
        }
        let walled = at.x.abs() > HALF_X - 1.6 || at.z.abs() > HALF_Z - 1.6;
        if walled && velocity.length() < 0.5 {
            r.ball_walled += dt;
            walled_run += dt;
            r.longest_ball_walled = r.longest_ball_walled.max(walled_run);
        } else {
            walled_run = 0.0;
        }
        for (b, (f, _)) in &feet {
            if let Some(prev) = last_feet.insert(*b, *f)
                && prev.distance(*f) < 0.02
                && Vec3::new(f.x - at.x, 0.0, f.z - at.z).length() > 8.0
            {
                r.idle += dt;
                if trace {
                    let t = thoughts.iter().find(|t| t.bot == *b);
                    println!(
                        "idle t={now:.2} {b} at {f:?} ball {at:?} {:?} {:?} {:?}",
                        t.map(|t| t.behaviour),
                        t.and_then(|t| t.goal),
                        t.and_then(|t| t.objective_diagnostic)
                    );
                }
            }
        }
    }
    r.seconds = seconds as f32;
    r.contact = touching / in_play.max(1e-3);
    r.jitter = reversals as f32 / (teams.len() as f32 * seconds as f32 / 60.0);
    r.facing_away = facing_away / contact.max(1e-3);
    r.wrong_way = wrong_way / contact.max(1e-3);
    r
}

/// An angle brought into -pi..pi.
fn wrap(a: f32) -> f32 {
    let t = std::f32::consts::TAU;
    (a + std::f32::consts::PI).rem_euclid(t) - std::f32::consts::PI
}

fn env<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

/// What a viewer would call a clean match. Shares are of bot-time (bots x
/// seconds) or of the match, as each field says.
/// Own goals: about one in five of a match's goals (one more than a fifth
/// rounds down, so a single own goal in a four-goal match passes), and a
/// fifth over all seeds, checked by the caller. A defender blocking on its goal line is
/// credited with a ball an attacker drives in off it, since the engine
/// credits the last body to move the ball. A lone
/// defender blocking in its own goal mouth is credited with the ball the
/// attacker drives in (the engine credits the last body to move it), so
/// line-ups with a one-bot side bound only the rest.
fn assert_clean(label: &str, setup: &Setup, r: &Report) {
    let own_goals_bounded = setup.blue.min(setup.red) >= 2;
    // Three a side on this pitch crowd it: a cover walking past the
    // teammate working the ball counts as stacked for a moment.
    let clump_share = if setup.blue.max(setup.red) >= 3 {
        0.15
    } else {
        0.05
    };
    let bots = (setup.blue + setup.red) as f32;
    let bot_time = bots * r.seconds;
    let total: u32 = r.goals.values().sum();
    let problems: Vec<String> = [
        (total >= 3, format!("only {total} goals")),
        (
            setup.blue != setup.red || r.goals.len() == 2,
            format!("one side never scored: {:?}", r.goals),
        ),
        (
            !own_goals_bounded || r.own_goals * 5 <= total.max(1) + 1,
            format!("{} own goals of {total}", r.own_goals),
        ),
        (
            r.longest_untouched <= 6.0,
            format!("ball unattended {:.1} s", r.longest_untouched),
        ),
        (
            r.longest_ball_walled <= 5.0,
            format!("ball on a wall {:.1} s", r.longest_ball_walled),
        ),
        (
            r.ball_lost == 0.0,
            format!("ball lost {:.1} s", r.ball_lost),
        ),
        (
            r.stuck <= 0.05 * bot_time,
            format!("stuck {:.1} bot-s", r.stuck),
        ),
        (
            r.circling <= 0.01 * bot_time,
            format!("circling {:.1} bot-s", r.circling),
        ),
        (r.jitter <= 3.0, format!("jitter {:.1}/bot-min", r.jitter)),
        (
            r.ignoring <= 0.10 * bot_time,
            format!("ignoring the ball {:.1} bot-s", r.ignoring),
        ),
        (
            r.idle <= 0.03 * bot_time,
            format!("idle {:.1} bot-s", r.idle),
        ),
        (
            r.clumped <= clump_share * r.seconds,
            format!("clumped {:.1} s", r.clumped),
        ),
        (
            r.slowest_kickoff <= 6.0,
            format!("slow kickoff {:.1} s", r.slowest_kickoff),
        ),
        (
            r.facing_away <= 0.05,
            format!("facing away {:.2}", r.facing_away),
        ),
        (r.wrong_way <= 0.10, format!("wrong way {:.2}", r.wrong_way)),
        (r.combat == 0.0, format!("fought {:.1} bot-s", r.combat)),
    ]
    .into_iter()
    .filter(|(ok, _)| !ok)
    .map(|(_, why)| why)
    .collect();
    assert!(problems.is_empty(), "{label}: {problems:?}\n{r:?}");
}

/// Two against two with bare hands and with hammers, over fixed seeds
/// (`BRI_SOCCER_SEEDS`, default 3) of `BRI_SOCCER_SECONDS` (default 150):
/// both sides score, the ball keeps moving, nobody idles, sticks, circles,
/// jitters, clumps or pushes toward its own goal.
#[test]
fn two_against_two_play_a_clean_match_across_seeds() {
    let seeds: u64 = env("BRI_SOCCER_SEEDS", 3);
    let seconds: usize = env("BRI_SOCCER_SECONDS", 150);
    let kits: Vec<Kit> = match std::env::var("BRI_SOCCER_KIT").as_deref() {
        Ok("hands") => vec![Kit::Hands],
        Ok("hammer") => vec![Kit::Hammer],
        _ => vec![Kit::Hands, Kit::Hammer],
    };
    let mut failures = Vec::new();
    let (mut goals, mut own_goals) = (0, 0);
    let first: u64 = env("BRI_SOCCER_FIRST", 1);
    for kit in kits {
        for seed in first..=seeds {
            let setup = Setup::new(kit, 2, 2).seeded(seed);
            let r = play(&setup, seconds);
            println!("{kit:?} seed {seed}: {r:?}");
            goals += r.goals.values().sum::<u32>();
            own_goals += r.own_goals;
            let label = format!("{kit:?} 2v2 seed {seed}");
            if let Err(e) = std::panic::catch_unwind(|| assert_clean(&label, &setup, &r)) {
                failures.push(e.downcast_ref::<String>().cloned().unwrap_or_default());
            }
        }
    }
    println!("{own_goals} own goals of {goals} goals");
    if own_goals * 5 > goals {
        failures.push(format!("{own_goals} own goals of {goals} over all seeds"));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// What a player tries while filming: one on one, three a side, uneven
/// sides, and a field with a wall and a ramp in midfield.
#[test]
fn other_line_ups_and_an_obstacle_field_play_on() {
    let seconds: usize = env("BRI_SOCCER_SECONDS", 90);
    let only = std::env::var("BRI_SOCCER_ONLY").ok();
    let mut failures = Vec::new();
    for (label, setup) in [
        ("1v1", Setup::new(Kit::Hands, 1, 1)),
        ("3v3", Setup::new(Kit::Hands, 3, 3)),
        ("1v2", Setup::new(Kit::Hands, 1, 2)),
        ("2v3 hammer", Setup::new(Kit::Hammer, 2, 3)),
        ("2v2 obstacles", Setup::new(Kit::Hands, 2, 2).obstacles()),
    ] {
        if only.as_deref().is_some_and(|o| o != label) {
            continue;
        }
        for seed in 1..=2 {
            let setup = setup.clone().seeded(seed);
            let r = play(&setup, seconds);
            println!("{label} seed {seed}: {r:?}");
            let label = format!("{label} seed {seed}");
            if let Err(e) = std::panic::catch_unwind(|| assert_clean(&label, &setup, &r)) {
                failures.push(e.downcast_ref::<String>().cloned().unwrap_or_default());
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The field Max loads: `saves/Soccer 2v2.world.json`, written by
/// `BRI_BLESS=1`. Loaded with Load Bricks by a host running no game, it
/// sets up the game and its teams, its bots join their pads' teams, and
/// the match plays cleanly.
#[test]
fn the_shipped_soccer_save_loads_a_ready_match() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../saves/Slate/Soccer 2v2.world.json");
    let setup = Setup::new(Kit::Hands, 2, 2);
    if std::env::var_os("BRI_BLESS").is_some() {
        let mut m = Match::new(&setup);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, m.save()).unwrap();
    }
    let bytes = std::fs::read(&path).expect("the shipped save exists (BRI_BLESS=1 writes it)");
    let build = bri_world::build::decode(&bytes).unwrap();
    // Only stock v20 bricks: it loads on any copy of the game.
    for b in build.world.bricks.values() {
        let ContentRef::Resolved(id) = &b.definition else {
            panic!("unresolved brick {:?}", b.definition);
        };
        assert!(
            [CUBE, PAD, MARK].contains(&id.as_str()),
            "{id} is not a stock brick"
        );
    }
    let m = Match::from_save(&bytes);
    let teams: Vec<_> = m.s.minigame_views()[0]
        .teams
        .iter()
        .map(|t| (t.name.clone(), t.id.0))
        .collect();
    assert_eq!(
        teams,
        [("Blue".to_string(), BLUE), ("Red".to_string(), RED)],
        "the save brings its game's teams"
    );
    let seconds: usize = env("BRI_SOCCER_SECONDS", 90);
    let r = play_match(m, &setup, seconds);
    println!("shipped save: {r:?}");
    assert_clean("shipped save", &setup, &r);
}

/// What a placed or saved brick of the field is, for comparing the two:
/// its brick, name, spawn and team, event rows and detection region.
fn identity(b: &Brick) -> String {
    format!(
        "{:?} {:?} {:?} {} {:?} {:?}",
        b.definition,
        b.name,
        b.vehicle.as_ref().map(|v| (&v.vehicle, v.team)),
        b.events.len(),
        b.rule_region,
        b.colliding
    )
}

/// The shipped field read as Load Bricks reads it (`Store::read` decodes
/// the file) and loaded with the host's Load Bricks command, on a Vehicle
/// Spawn of whatever size the installed game has. Every brick survives,
/// within half a stud and half a plate of where it was saved: each pad
/// with its bot or ball and team, both goals with their events and
/// regions, and the game with its teams. Before loads moved a brick saved
/// off its grid onto it, an odd-stud or even-plate Vehicle Spawn lost
/// every pad.
#[test]
fn the_shipped_field_survives_load_bricks_whatever_the_pads_size() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../saves/Slate/Soccer 2v2.world.json");
    let bytes = std::fs::read(&path).expect("the shipped save exists (BRI_BLESS=1 writes it)");
    let saved = bri_world::build::decode(&bytes).unwrap();
    let mut expected: Vec<String> = saved.world.bricks.values().map(identity).collect();
    expected.sort();
    assert_eq!(
        saved
            .world
            .bricks
            .values()
            .filter(|b| b.vehicle.is_some())
            .count(),
        5,
        "the ball pad and four bot pads"
    );
    assert_eq!(
        saved
            .world
            .bricks
            .values()
            .filter(|b| !b.events.is_empty())
            .count(),
        2,
        "two goals"
    );
    for size in [
        ([8, 8], 1),
        ([8, 8], 2),
        ([7, 7], 3),
        ([6, 6], 1),
        ([5, 3], 2),
    ] {
        let mut m = Match::base_with(definitions_with(size));
        m.run(Command::LoadBuild {
            build: Box::new(bri_world::build::decode(&bytes).unwrap()),
            ownership: false,
        });
        m.steps(TICKS_PER_SECOND);
        let state = m.s.simulation().state();
        let mut placed: Vec<String> = state.bricks.values().map(identity).collect();
        placed.sort();
        assert_eq!(
            placed,
            expected,
            "pad {size:?}: every brick placed: {:?}",
            m.s.snapshot().chat
        );
        for b in saved.world.bricks.values() {
            let near = state.bricks.values().any(|p| {
                identity(p) == identity(b)
                    && (0..3).all(|a| {
                        (p.position[a] - b.position[a]).abs() <= [0.25, 0.1, 0.25][a] + 1e-4
                    })
            });
            assert!(near, "pad {size:?}: {:?} stays where it was saved", b.name);
        }
        let teams: Vec<_> = m.s.minigame_views()[0]
            .teams
            .iter()
            .map(|t| (t.name.clone(), t.id.0))
            .collect();
        assert_eq!(
            teams,
            [("Blue".to_string(), BLUE), ("Red".to_string(), RED)],
            "pad {size:?}: the game and its teams"
        );
    }
}

/// Short runs with a rocket launcher, a spear, and jeeps parked at the
/// touchlines: the match keeps going. Nobody's brain sticks or circles,
/// the ball is never lost or left alone for long, and goals are scored.
#[test]
fn rockets_spears_and_jeeps_keep_the_match_going() {
    let seconds: usize = env("BRI_SOCCER_SHORT", 60);
    let mut failures = Vec::new();
    for (label, setup) in [
        ("rocket", Setup::new(Kit::Rocket, 2, 2)),
        ("spear", Setup::new(Kit::Spear, 2, 2)),
        ("jeeps", Setup::new(Kit::Hands, 2, 2).jeeps()),
    ] {
        let setup = setup.seeded(1);
        let r = play(&setup, seconds);
        println!("{label}: {r:?}");
        let bot_time = (setup.blue + setup.red) as f32 * r.seconds;
        let problems: Vec<String> = [
            (r.goals.values().sum::<u32>() >= 1, "no goals".to_string()),
            (
                r.ball_lost == 0.0,
                format!("ball lost {:.1} s", r.ball_lost),
            ),
            (
                r.longest_untouched <= 10.0,
                format!("ball unattended {:.1} s", r.longest_untouched),
            ),
            (
                r.longest_ball_walled <= 8.0,
                format!("ball on a wall {:.1} s", r.longest_ball_walled),
            ),
            (
                r.stuck <= 0.10 * bot_time,
                format!("stuck {:.1} bot-s", r.stuck),
            ),
            (
                r.circling <= 0.02 * bot_time,
                format!("circling {:.1} bot-s", r.circling),
            ),
            (
                r.idle <= 0.05 * bot_time,
                format!("idle {:.1} bot-s", r.idle),
            ),
            (
                r.slowest_kickoff <= 8.0,
                format!("slow kickoff {:.1} s", r.slowest_kickoff),
            ),
        ]
        .into_iter()
        .filter(|(ok, _)| !ok)
        .map(|(_, why)| why)
        .collect();
        if !problems.is_empty() {
            failures.push(format!("{label}: {problems:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
