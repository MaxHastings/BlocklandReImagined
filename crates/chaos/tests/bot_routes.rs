//! Guards for the bot route planner (`docs/architecture/bots.md`, Routes):
//! one plan of walk, swim, jet and drive legs per goal, each leg executed
//! with ordinary controls. Every scenario here failed before the planner:
//! a bot hovered under the platform its enemy stood on, floated in deep
//! water with no route, or drove round and round its target.
use bri_chaos::fixture;
use bri_minigames::Settings;
use bri_sim::player::MoveInput;
use bri_sim::session::{Command, MiniGameRequest, Session, ToolCatalog};
use bri_world::{Brick, ContentRef, OwnerId, VehicleSpawn, World, build::SavedBuild};
use glam::Vec3;
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

const JEEP: &str = bri_vehicles::testing::CAR;
const TOOLS_ONLY: [Option<String>; 5] = [None, None, None, None, None];

fn session() -> Session {
    let mut s = fixture::synthetic().unwrap().session;
    s.set_tool_catalog(ToolCatalog {
        vehicles: [fixture::BOT, JEEP].map(String::from).into(),
        vehicle_bricks: [fixture::PLATE.to_string()].into(),
        ..Default::default()
    })
    .unwrap();
    s
}

/// The Blockhead kind, changed by `change`, as the only kind.
fn only_kind(s: &mut Session, change: impl FnOnce(&mut bri_sim::bot_kind::BotKind)) {
    let mut kinds = bri_sim::bot_kind::BotPack::from_json(include_bytes!(
        "../../../packages/blockhead_bot/assets/bots.json"
    ))
    .unwrap()
    .bots;
    change(&mut kinds[0]);
    s.set_bot_kinds(kinds).unwrap();
}

fn bite(damage: f32) -> bri_sim::bot_kind::BotMelee {
    serde_json::from_value(serde_json::json!({ "damage": damage, "seconds": 1.0 })).unwrap()
}

/// A 1x1 plate spawning `kind`, a bot or a vehicle, centred in the stud
/// cell at `at`.
fn spawn_brick(kind: &str, at: [f32; 3], owner: OwnerId) -> Brick {
    let at = [at[0] + 0.25, at[1], at[2] + 0.25];
    let mut brick = Brick::new(ContentRef::Resolved(fixture::PLATE.into()), at, owner);
    brick.vehicle = Some(Box::new(VehicleSpawn {
        vehicle: ContentRef::Resolved(kind.into()),
        recolor: false,
    }));
    brick
}

fn brick(definition: &str, at: [f32; 3], owner: OwnerId) -> Brick {
    Brick::new(ContentRef::Resolved(definition.into()), at, owner)
}

fn load(s: &mut Session, owner: OwnerId, bricks: Vec<Brick>) {
    let mut world = World::new("Routes".into(), "chaos/map".into(), vec![[1.0; 4]]);
    for (i, brick) in bricks.into_iter().enumerate() {
        world.bricks.insert(i as u64 + 1, brick);
    }
    world.next_brick_id = world.bricks.len() as u64 + 1;
    let count = world.bricks.len();
    s.command(
        owner,
        100,
        Command::LoadBuild {
            build: Box::new(SavedBuild::new(world)),
            ownership: false,
        },
    )
    .unwrap();
    let mut sequence = 1 << 40;
    steps(s, &[owner], 5, &mut sequence);
    assert_eq!(
        s.simulation().state().bricks.len(),
        count,
        "every brick placed"
    );
}

fn minigame(s: &mut Session, owner: OwnerId) {
    s.command(
        owner,
        101,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: Settings {
                loadout: TOOLS_ONLY,
                ..Default::default()
            },
        }),
    )
    .unwrap();
}

/// Step the world, humans standing still.
fn steps(s: &mut Session, humans: &[OwnerId], ticks: usize, sequence: &mut u64) {
    for _ in 0..ticks {
        *sequence += 1;
        for human in humans {
            s.movement(*human, *sequence, MoveInput::default()).unwrap();
        }
        s.step().unwrap();
    }
}

fn feet(s: &Session, owner: OwnerId) -> Vec3 {
    Vec3::from(
        s.snapshot()
            .players
            .into_iter()
            .find(|p| p.owner == owner)
            .expect("player in the snapshot")
            .feet,
    )
}

fn bots(s: &Session) -> Vec<OwnerId> {
    s.names().keys().copied().filter(|o| s.is_bot(*o)).collect()
}

fn leg(s: &Session, bot: OwnerId) -> &'static str {
    s.bot_thoughts()
        .into_iter()
        .find(|t| t.bot == bot)
        .map_or("none", |t| t.leg)
}

/// The enemy stands in the middle of a wide platform eight units up; the
/// bot starts right under it. Jetting straight up from there only presses
/// it against the platform's underside (the old Fly behaviour hovered
/// there for good): the route walks out from under the platform, jets up
/// in the open, crosses over and lands by the enemy.
#[test]
fn a_bot_jets_from_open_sky_onto_a_high_platform_instead_of_hovering_under_it() {
    let mut s = session();
    only_kind(&mut s, |k| k.melee = Some(bite(5.0)));
    let top = 8.2;
    let centre = Vec3::new(16.0, top, 30.0);
    let spawn = centre + Vec3::Y * 0.05;
    s.set_spawn_points(vec![spawn]).unwrap();
    let human = s.join("Builder".into(), spawn, true).unwrap();
    // Past the sequence numbers `load` gave the builder's moves.
    let mut sequence = 1 << 41;
    steps(&mut s, &[human], 10, &mut sequence);
    // An 8 x 8 baseplate the bot sees through; its edge is four units out.
    let mut platform = brick(fixture::BASEPLATE, [16.0, top - 0.1, 30.0], human);
    platform.raycast = false;
    let bricks = vec![
        spawn_brick(fixture::BOT, [15.5, 0.1, 29.5], human),
        platform,
    ];
    load(&mut s, human, bricks);
    minigame(&mut s, human);
    steps(&mut s, &[human], 30, &mut sequence);
    let bot = bots(&s)[0];
    let under = |p: Vec3| {
        (12.0..20.0).contains(&p.x) && (26.0..34.0).contains(&p.z) && p.y > 1.0 && p.y < top - 0.5
    };
    let mut hovering = 0;
    let mut jetted = false;
    let mut reached = None;
    for tick in 0..120 * 20 {
        steps(&mut s, &[human], 1, &mut sequence);
        let at = feet(&s, bot);
        if under(at) {
            hovering += 1;
        }
        jetted |= leg(&s, bot) == "jet";
        if reached.is_none() && at.distance(feet(&s, human)) < 3.0 {
            reached = Some(tick);
        }
    }
    assert!(jetted, "the route took a jet leg");
    assert!(
        hovering < 120,
        "it spent {hovering} ticks in the air under the platform"
    );
    assert!(
        reached.is_some(),
        "the bot reached the enemy on the platform: bot {} enemy {}",
        feet(&s, bot),
        feet(&s, human)
    );
    assert!(s.vitals()[&human].health < 100.0, "it reached melee range");
}

/// A strip of deep water lies between a bot that cannot jet and its enemy,
/// who paces its bank. A walker floating in the water used to have no
/// route at all once it planned again there (the walk grid lies on the
/// bottom, out of reach of its feet): now the water is a swim leg.
#[test]
fn a_walker_swims_across_deep_water_to_reach_its_enemy() {
    let mut s = session();
    only_kind(&mut s, |k| {
        k.melee = Some(bite(5.0));
        k.behaviours.insert("fly".into(), 0.0);
        k.behaviours.insert("interact".into(), 0.0);
    });
    let spawn = Vec3::new(0.0, 0.05, 40.0);
    s.set_spawn_points(vec![spawn]).unwrap();
    let human = s.join("Builder".into(), spawn, true).unwrap();
    // Past the sequence numbers `load` gave the builder's moves.
    let mut sequence = 1 << 41;
    steps(&mut s, &[human], 10, &mut sequence);
    // Four bricks across (sixteen units of water, six deep), twelve along.
    let mut bricks = vec![spawn_brick(fixture::BOT, [-24.0, 0.1, 40.0], human)];
    for x in 0..4 {
        for z in 0..12 {
            bricks.push(brick(
                bri_sim::testing::DEEP_WATER,
                [-18.0 + x as f32 * 4.0, 3.0, 18.0 + z as f32 * 4.0],
                human,
            ));
        }
    }
    load(&mut s, human, bricks);
    minigame(&mut s, human);
    steps(&mut s, &[human], 30, &mut sequence);
    let bot = bots(&s)[0];
    let mut swam = false;
    let mut reached = None;
    for tick in 0..120 * 30 {
        // The enemy paces up and down its bank, so the chase plans again
        // while the bot is out in the water.
        sequence += 1;
        let (forward, yaw) = match (tick / 60) % 4 {
            0 => (1.0, 0.0),
            2 => (1.0, std::f32::consts::PI),
            _ => (0.0, 0.0),
        };
        s.movement(
            human,
            sequence,
            MoveInput {
                forward,
                yaw,
                ..Default::default()
            },
        )
        .unwrap();
        s.step().unwrap();
        swam |= leg(&s, bot) == "swim";
        if reached.is_none() && feet(&s, bot).distance(feet(&s, human)) < 3.0 {
            reached = Some(tick);
            eprintln!("reached after {tick} ticks at {}", feet(&s, bot));
            break;
        }
    }
    assert!(
        reached.is_some(),
        "the bot crossed the water to its enemy: bot {} enemy {}",
        feet(&s, bot),
        feet(&s, human)
    );
    assert!(swam, "it crossed by a swim leg");
}

/// A server Add-On declaring a team setting, as Slayer does: teams need a
/// running Add-On.
fn slayer_like(team_setting: bool) -> Arc<bri_package_runtime::Catalog> {
    use bri_package::packages::{PackageEntry, PackageSet, Side};
    struct Temp(std::path::PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = Temp(std::env::temp_dir().join(format!(
        "bri-routes-soccer-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    let dir = root.0.join("pitch");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("package.json"), json!({"schema_version":1,"id":"pitch","version":"1.0.0","api":1,"name":"Pitch fixture","license":"CC0-1.0","capabilities":["minigame"],"provides":[{"kind":"behaviour","id":"pitch:behaviour/main","file":"behaviour.json"},{"kind":"script","id":"pitch:script/main","file":"main.rhai"}]}).to_string()).unwrap();
    let settings = if team_setting {
        json!([{"key":"kit","title":"Kit","scope":"team","type":"bool","default":false}])
    } else {
        json!([{"key":"halves","title":"Halves","scope":"minigame","type":"bool","default":false}])
    };
    std::fs::write(
        dir.join("behaviour.json"),
        json!({"schema_version":1,"script":"main.rhai","settings":settings}).to_string(),
    )
    .unwrap();
    std::fs::write(dir.join("main.rhai"), "fn unused() { }\n").unwrap();
    Arc::new(
        bri_package_runtime::Catalog::load(
            &root.0,
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
        .unwrap(),
    )
}

/// Max's Slayer soccer with empty loadouts: four unarmed bots a side and a
/// ball to put in the other side's goal, all of it on flat ground, plus
/// `extra` bricks. Returns the session, its author and their next move
/// sequence.
fn soccer(extra: impl FnOnce(OwnerId) -> Vec<Brick>) -> (Session, OwnerId, u64) {
    use bri_events::rules::{Compare, Condition, Datum, Property, Subject};
    use bri_events::{Row, Slot, Target};
    use bri_sim::session::TeamEdit;
    let mut s = fixture::synthetic().unwrap().session;
    s.set_event_catalog(bri_events::testing::catalog(), Vec::<String>::new())
        .unwrap();
    s.set_tool_catalog(ToolCatalog {
        vehicles: [fixture::BOT, bri_vehicles::testing::BALL, JEEP]
            .map(String::from)
            .into(),
        vehicle_bricks: [fixture::PLATE.to_string()].into(),
        ..Default::default()
    })
    .unwrap();
    s.install_packages(slayer_like(true), None).unwrap();
    let spawn = Vec3::new(-14.0, 0.05, 45.0);
    s.set_spawn_points(vec![spawn]).unwrap();
    let human = s.join("Max".into(), spawn, true).unwrap();
    let mut sequence = 1 << 41;
    steps(&mut s, &[human], 10, &mut sequence);
    let mut bricks = Vec::new();
    for i in 0..4 {
        let z = 41.0 + i as f32 * 2.0;
        bricks.push(spawn_brick(fixture::BOT, [-6.0, 0.1, z], human));
        bricks.push(spawn_brick(fixture::BOT, [6.0, 0.1, z], human));
    }
    let mut ball = spawn_brick(bri_vehicles::testing::BALL, [0.0, 0.1, 45.0], human);
    ball.name = Some("match_ball".into());
    bricks.push(ball);
    for (z, name) in [(57.25, "north_goal"), (33.25, "south_goal")] {
        let mut goal = brick(fixture::PLATE, [0.25, 0.1, z], human);
        goal.name = Some(name.into());
        goal.colliding = false;
        goal.raycast = false;
        goal.rule_region = Some([3.0, 4.0, 3.0]);
        bricks.push(goal);
    }
    bricks.extend(extra(human));
    load(&mut s, human, bricks);
    minigame(&mut s, human);
    steps(&mut s, &[human], 20, &mut sequence);
    let game = s.minigame_views()[0].id;
    s.command(
        human,
        102,
        Command::MiniGame(MiniGameRequest::AddOnSettings {
            game,
            settings: vec![],
            teams: Some(
                [("Blue", 0), ("Red", 1)]
                    .map(|(name, color)| TeamEdit {
                        id: None,
                        name: name.into(),
                        color,
                        settings: vec![],
                    })
                    .into(),
            ),
            quiet: true,
            reset: false,
        }),
    )
    .unwrap();
    steps(&mut s, &[human], 5, &mut sequence);
    let teams: Vec<u32> = s.minigame_views()[0].teams.iter().map(|t| t.id.0).collect();
    assert_eq!(teams.len(), 2, "both teams saved");
    let players = bots(&s);
    assert_eq!(players.len(), 8, "every spawn brick has its bot");
    for (i, bot) in players.iter().enumerate() {
        let east = feet(&s, *bot).x > 0.0;
        s.command(
            human,
            103 + i as u64,
            Command::MiniGame(MiniGameRequest::SetTeam {
                game,
                target: *bot,
                team: Some(teams[usize::from(east)]),
            }),
        )
        .unwrap();
    }
    // Each goal is scored in by the side attacking it.
    for (name, scoring) in ["south_goal", "north_goal"].into_iter().zip(teams.clone()) {
        let id = *s
            .simulation()
            .state()
            .bricks
            .iter()
            .find(|(_, b)| b.name.as_deref() == Some(name))
            .unwrap()
            .0;
        let rows = vec![Row {
            enabled: true,
            input: "onObjectEnter".into(),
            output: "winRound".into(),
            target: Target::Slot(Slot::Instigator),
            params: vec![],
            conditions: vec![
                Condition {
                    subject: Subject::Object,
                    property: Property::SpawnedBy,
                    key: String::new(),
                    compare: Compare::Equal,
                    value: Datum::Text("match_ball".into()),
                },
                Condition {
                    subject: Subject::Player,
                    property: Property::Team,
                    key: String::new(),
                    compare: Compare::Equal,
                    value: Datum::Number(i64::from(scoring)),
                },
            ],
            delay_ms: 0,
            preserved: None,
        }];
        s.edit_brick(human, id, bri_world::authority::Edit::Events(rows))
            .unwrap();
    }
    (s, human, sequence)
}

/// In that soccer nothing needs a jet leg, so no bot ever takes off: a jet
/// leg is only ever a way to reach a point the ground does not, never
/// something to do with nothing to reach.
#[test]
fn unarmed_bots_with_only_a_ball_to_play_never_take_off() {
    let (mut s, human, mut sequence) = soccer(|_| Vec::new());
    let mut played = false;
    for tick in 0..120 * 30 {
        steps(&mut s, &[human], 1, &mut sequence);
        let snapshot = s.snapshot();
        for p in snapshot.players.iter().filter(|p| s.is_bot(p.owner)) {
            // A jump clears a couple of units; only jets climb past four.
            assert!(
                !p.jetting && p.feet[1] < 4.0,
                "bot {} took off at tick {tick}: jetting {}, feet {:?}, thoughts {:?}",
                p.owner,
                p.jetting,
                p.feet,
                s.bot_thoughts().into_iter().find(|t| t.bot == p.owner)
            );
        }
        for t in s.bot_thoughts() {
            played |= t.objective_detail.is_some();
            assert_ne!(t.leg, "jet", "bot {} planned a jet leg: {t:?}", t.bot);
        }
    }
    assert!(played, "the bots played the ball");
}

/// A jeep parked beside that soccer pitch does not serve putting the ball
/// in a goal, so it attracts nobody: no bot boards it or goes for a seat.
#[test]
fn a_vehicle_that_does_not_serve_the_ball_game_attracts_no_bot() {
    let (mut s, human, mut sequence) =
        soccer(|human| vec![spawn_brick(JEEP, [-2.0, 0.1, 52.0], human)]);
    let mut played = false;
    for tick in 0..120 * 30 {
        steps(&mut s, &[human], 1, &mut sequence);
        for t in s.bot_thoughts() {
            played |= t.objective_detail.is_some();
            assert!(
                t.behaviour != "interact" && s.mounted(t.bot).is_none(),
                "bot {} went for the jeep at tick {tick}: {t:?}",
                t.bot
            );
        }
    }
    assert!(played, "the bots played the ball");
}
