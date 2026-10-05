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

/// Two teams, Blue and Red, saved in the author's game (an Add-On must be
/// running), with each bot on the side of the pitch it stands on: west
/// Blue, east Red. Returns the team ids, Blue first.
fn sides(s: &mut Session, human: OwnerId, sequence: &mut u64) -> Vec<u32> {
    use bri_sim::session::TeamEdit;
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
    steps(s, &[human], 5, sequence);
    let teams: Vec<u32> = s.minigame_views()[0].teams.iter().map(|t| t.id.0).collect();
    assert_eq!(teams.len(), 2, "both teams saved");
    for (i, bot) in bots(s).iter().enumerate() {
        let east = feet(s, *bot).x > 0.0;
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
    teams
}

/// Max's Slayer soccer with empty loadouts: four unarmed bots a side and a
/// ball to put in the other side's goal, all of it on flat ground, plus
/// `extra` bricks. Returns the session, its author and their next move
/// sequence.
fn soccer(extra: impl FnOnce(OwnerId) -> Vec<Brick>) -> (Session, OwnerId, u64) {
    use bri_events::rules::{Compare, Condition, Datum, Property, Subject};
    use bri_events::{Row, Slot, Target};
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
    let teams = sides(&mut s, human, &mut sequence);
    assert_eq!(bots(&s).len(), 8, "every spawn brick has its bot");
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

/// A jeep beside its bot and a goal it is to be driven into at `goal`,
/// `region` across (each authored as ordinary bricks: the jeep's spawn is
/// named, and the goal wins the round when that jeep enters it). Returns
/// the session, its author, the bot, the jeep and the next move sequence.
fn drive_scene(goal: [f32; 3], region: [f32; 3]) -> (Session, OwnerId, OwnerId, u64, u64) {
    use bri_events::rules::{Compare, Condition, Datum, Property, Subject};
    use bri_events::{Row, Slot, Target};
    let mut s = session();
    s.set_event_catalog(bri_events::testing::catalog(), Vec::<String>::new())
        .unwrap();
    // The author watches from well off, out of the way.
    let spawn = Vec3::new(-24.0, 0.05, 30.0);
    s.set_spawn_points(vec![spawn]).unwrap();
    let human = s.join("Builder".into(), spawn, true).unwrap();
    let mut sequence = 1 << 41;
    steps(&mut s, &[human], 10, &mut sequence);
    let mut jeep = spawn_brick(JEEP, [0.0, 0.1, 47.0], human);
    jeep.name = Some("pool_car".into());
    let mut target = brick(fixture::PLATE, goal, human);
    target.name = Some("car_park".into());
    target.colliding = false;
    target.raycast = false;
    target.rule_region = Some(region);
    target.events = vec![Row {
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
                value: Datum::Text("pool_car".into()),
            },
            Condition {
                subject: Subject::Instigator,
                property: Property::Exists,
                key: String::new(),
                compare: Compare::Equal,
                value: Datum::Bool(true),
            },
        ],
        delay_ms: 0,
        preserved: None,
    }];
    let bricks = vec![
        spawn_brick(fixture::BOT, [0.0, 0.1, 40.0], human),
        jeep,
        target,
    ];
    load(&mut s, human, bricks);
    minigame(&mut s, human);
    steps(&mut s, &[human], 20, &mut sequence);
    let bot = bots(&s)[0];
    let car = s
        .vehicle_infos()
        .iter()
        .find(|v| v.definition == JEEP)
        .expect("the jeep")
        .id;
    (s, human, bot, car, sequence)
}

/// The jeep's heading about the vertical, radians.
fn heading(s: &Session, car: u64) -> f32 {
    let pose = s
        .vehicle_poses()
        .into_iter()
        .find(|p| p.id == car)
        .expect("the jeep's pose");
    let forward = glam::Quat::from_array(pose.rotation) * Vec3::NEG_Z;
    forward.x.atan2(-forward.z)
}

/// Drive the scene until the round is won or `seconds` pass; returns the
/// tick it was won at (if it was) and how far the jeep turned in all.
fn drive_until_won(
    s: &mut Session,
    human: OwnerId,
    bot: OwnerId,
    car: u64,
    sequence: &mut u64,
    seconds: usize,
) -> (Option<usize>, f32) {
    let mut turned = 0.0;
    let mut last = heading(s, car);
    let mut boarded = false;
    for tick in 0..120 * seconds {
        steps(s, &[human], 1, sequence);
        let now = heading(s, car);
        if s.mounted(bot).is_some() {
            boarded = true;
            let delta = (now - last + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            turned += delta.abs();
        }
        last = now;
        if s.round_results().any(|r| r.owners == vec![bot]) {
            assert!(boarded, "the jeep was driven there");
            return (Some(tick), turned);
        }
    }
    (None, turned)
}

/// A goal well ahead: driven straight to, no weaving, and no circling at
/// the end.
#[test]
fn a_jeep_drives_straight_to_a_point_ahead_and_stops_there() {
    let (mut s, human, bot, car, mut sequence) = drive_scene([0.25, 0.1, 17.25], [3.0, 4.0, 3.0]);
    let (won, turned) = drive_until_won(&mut s, human, bot, car, &mut sequence, 25);
    assert!(
        won.is_some(),
        "the jeep never got to the goal ahead (turned {turned:.1} rad): {:?}",
        s.bot_thoughts()
    );
    assert!(
        turned < std::f32::consts::FRAC_PI_2,
        "it weaved or circled: turned {turned:.1} rad on the way"
    );
}

/// A bot far from its enemy takes the jeep beside it, since driving gets
/// there sooner. As the jeep closes in, the enemy sidesteps to stand just
/// off its flank, inside its turning circle, where steering at them only
/// circles them (the old driver went round and round, as in the
/// gauntlet's jeep duel). The driver backs out of the circle and runs
/// them down instead.
#[test]
fn a_driver_runs_down_an_enemy_who_sidesteps_inside_its_turning_circle() {
    let mut s = session();
    let spawn = Vec3::new(0.0, 0.05, 0.0);
    s.set_spawn_points(vec![spawn]).unwrap();
    let human = s.join("Builder".into(), spawn, true).unwrap();
    let mut sequence = 1 << 41;
    steps(&mut s, &[human], 10, &mut sequence);
    let bricks = vec![
        spawn_brick(fixture::BOT, [0.0, 0.1, 44.0], human),
        spawn_brick(JEEP, [0.0, 0.1, 40.0], human),
    ];
    load(&mut s, human, bricks);
    minigame(&mut s, human);
    steps(&mut s, &[human], 20, &mut sequence);
    let bot = bots(&s)[0];
    let car = s
        .vehicle_infos()
        .iter()
        .find(|v| v.definition == JEEP)
        .expect("the jeep")
        .id;
    let position = |s: &Session| {
        Vec3::from(
            s.vehicle_poses()
                .into_iter()
                .find(|p| p.id == car)
                .unwrap()
                .position,
        )
    };
    let mut dodged: Option<(usize, Vec3)> = None;
    let mut turned = 0.0;
    let mut last = heading(&s, car);
    for tick in 0..120 * 40 {
        let jeep = position(&s);
        let me = feet(&s, human);
        if dodged.is_none() && s.mounted(bot).is_some() && flat(jeep - me).length() < 14.0 {
            // Step off the jeep's line: to its left, three units across.
            let forward = flat(
                glam::Quat::from_array(
                    s.vehicle_poses()
                        .into_iter()
                        .find(|p| p.id == car)
                        .unwrap()
                        .rotation,
                ) * Vec3::NEG_Z,
            )
            .normalize();
            dodged = Some((tick, me + Vec3::new(forward.z, 0.0, -forward.x) * 3.0));
        }
        // Walk to the sidestep point, then stand there.
        let input = match dodged {
            Some((_, to)) if flat(to - me).length() > 0.3 => {
                let d = flat(to - me);
                MoveInput {
                    forward: 1.0,
                    yaw: d.x.atan2(-d.z),
                    ..Default::default()
                }
            }
            _ => MoveInput::default(),
        };
        sequence += 1;
        s.movement(human, sequence, input).unwrap();
        s.step().unwrap();
        let now = heading(&s, car);
        if dodged.is_some() {
            let delta = (now - last + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            turned += delta.abs();
        }
        last = now;
        if s.vitals()[&human].health < 100.0 {
            assert_eq!(
                s.mounted(bot).map(|(v, _)| v),
                Some(car),
                "run down by the jeep"
            );
            assert!(dodged.is_some(), "the enemy sidestepped first");
            assert!(
                turned < std::f32::consts::TAU,
                "it circled the enemy: turned {turned:.1} rad after the sidestep"
            );
            return;
        }
    }
    panic!(
        "never ran the enemy down: boarded {:?}, sidestepped {dodged:?}, turned {turned:.1} rad, jeep at {}, enemy at {}: {:?}",
        s.mounted(bot),
        position(&s),
        feet(&s, human),
        s.bot_thoughts()
    );
}

fn flat(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}

/// Two brawlers forty apart on opposing sides, each with a jeep at its
/// side, and nobody else in sight (the gauntlet's jeep duel). Driving gets
/// each to the other sooner, but a jeep with no gun cannot hurt someone
/// in another vehicle: the old drivers chased each other about for good
/// and nobody was ever hit. The drive is now only a leg: close to the
/// enemy, the driver stops and gets out to fight on foot, without circling.
#[test]
fn two_drivers_who_cannot_hurt_each_other_get_out_and_fight() {
    let mut s = session();
    only_kind(&mut s, |k| k.melee = Some(bite(5.0)));
    s.install_packages(slayer_like(true), None).unwrap();
    // The author stands far out of everybody's sight.
    let spawn = Vec3::new(0.0, 0.05, -160.0);
    s.set_spawn_points(vec![spawn]).unwrap();
    let human = s.join("Builder".into(), spawn, true).unwrap();
    let mut sequence = 1 << 41;
    steps(&mut s, &[human], 10, &mut sequence);
    let bricks = vec![
        spawn_brick(fixture::BOT, [-20.0, 0.1, 40.0], human),
        spawn_brick(JEEP, [-16.0, 0.1, 40.0], human),
        spawn_brick(fixture::BOT, [20.0, 0.1, 40.0], human),
        spawn_brick(JEEP, [16.0, 0.1, 40.0], human),
    ];
    load(&mut s, human, bricks);
    minigame(&mut s, human);
    steps(&mut s, &[human], 20, &mut sequence);
    sides(&mut s, human, &mut sequence);
    let fighters = bots(&s);
    assert_eq!(fighters.len(), 2);
    let cars: Vec<u64> = s
        .vehicle_infos()
        .iter()
        .filter(|v| v.definition == JEEP)
        .map(|v| v.id)
        .collect();
    assert_eq!(cars.len(), 2);
    let mut turned = [0.0f32; 2];
    let mut last = [heading(&s, cars[0]), heading(&s, cars[1])];
    let mut drove = false;
    for tick in 0..120 * 40 {
        steps(&mut s, &[human], 1, &mut sequence);
        for (i, car) in cars.iter().enumerate() {
            let now = heading(&s, *car);
            let delta = (now - last[i] + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            turned[i] += delta.abs();
            last[i] = now;
        }
        drove |= fighters.iter().any(|b| s.mounted(*b).is_some());
        assert!(
            turned.iter().all(|t| *t < 3.0 * std::f32::consts::TAU),
            "a jeep went round and round: turned {turned:?} rad by tick {tick}: {:?}",
            s.bot_thoughts()
        );
        if fighters.iter().any(|b| s.vitals()[b].health < 100.0) {
            assert!(drove, "they drove at each other");
            return;
        }
    }
    panic!(
        "nobody was hit in 40 s (turned {turned:?} rad): {:?}",
        s.bot_thoughts()
    );
}

/// The enemy stands behind a long wall, out of the sight of a bot with a
/// jeep forty-five away; only its ally across the field sees them. The
/// ally's dated sighting becomes the driver's search, the driver takes
/// the jeep (driving there is sooner) and drives round the wall to where
/// it can see the spot the enemy was last seen at, without getting out
/// first.
#[test]
fn a_driver_searches_round_a_wall_from_an_allys_sighting() {
    let mut s = session();
    only_kind(&mut s, |k| k.melee = Some(bite(0.0)));
    s.install_packages(slayer_like(true), None).unwrap();
    let spot = Vec3::new(0.0, 0.05, 0.0);
    s.set_spawn_points(vec![spot]).unwrap();
    let human = s.join("Builder".into(), spot, true).unwrap();
    let mut sequence = 1 << 41;
    steps(&mut s, &[human], 10, &mut sequence);
    // An opaque wall three high, from x -12 to 12 along z 6.
    let mut bricks: Vec<Brick> = (0..48)
        .map(|i| {
            brick(
                fixture::TALL,
                [-12.0 + i as f32 * 0.5 + 0.25, 1.5, 6.25],
                human,
            )
        })
        .collect();
    // The spotter, off to the side with a clear view; the driver and its
    // jeep beyond the wall. Both stand west, on one side.
    bricks.push(spawn_brick(fixture::BOT, [-30.0, 0.1, 0.0], human));
    bricks.push(spawn_brick(fixture::BOT, [-2.0, 0.1, 45.0], human));
    bricks.push(spawn_brick(JEEP, [-6.0, 0.1, 45.0], human));
    load(&mut s, human, bricks);
    minigame(&mut s, human);
    steps(&mut s, &[human], 20, &mut sequence);
    sides(&mut s, human, &mut sequence);
    let driver = *bots(&s)
        .iter()
        .find(|b| feet(&s, **b).z > 20.0)
        .expect("the driver");
    let look = spot + Vec3::Y * 1.5;
    let mut drove = false;
    for tick in 0..120 * 40 {
        steps(&mut s, &[human], 1, &mut sequence);
        let seated = s.mounted(driver);
        drove |= seated.is_some();
        if let Some((car, _)) = seated {
            let at = Vec3::from(
                s.vehicle_poses()
                    .into_iter()
                    .find(|p| p.id == car)
                    .unwrap()
                    .position,
            );
            if s.simulation()
                .sight(at + Vec3::Y * 1.5, look, 80.0)
                .is_some()
            {
                return;
            }
        } else {
            assert!(
                !drove,
                "the driver got out at tick {tick} before seeing the spot: {:?}",
                s.bot_thoughts().into_iter().find(|t| t.bot == driver)
            );
        }
    }
    panic!(
        "the driver never saw the spot (drove: {drove}, at {}): {:?}",
        feet(&s, driver),
        s.bot_thoughts().into_iter().find(|t| t.bot == driver)
    );
}
