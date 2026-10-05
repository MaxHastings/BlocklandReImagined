//! Physical/navigation feasibility evidence. Only ordinary motor inputs after
//! initialization: no runtime transforms, invisible supports or teleports.
#![allow(
    clippy::disallowed_methods,
    reason = "f32::clamp here is not yet bri_console::Clamp::clamped"
)]
use bri_chaos::fixture;
use bri_sim::player::{MoveInput, Player, PlayerTuning};
use bri_sim::{
    session::{Command, MiniGameRequest, Session, ToolCatalog},
    simulation::Simulation,
};
use bri_world::{Brick, ContentRef, VehicleSpawn, World};
use glam::{Mat3, Vec3};
use rapier3d::prelude::*;

fn floor() -> PhysicsWorld {
    let mut physics = bri_physics::new_world();
    physics.insert(
        RigidBodyBuilder::fixed(),
        ColliderBuilder::cuboid(100., 0.5, 100.).translation(Vector::new(0., -0.5, 0.)),
    );
    bri_physics::detect_collisions(&mut physics);
    physics
}

fn feet(player: &Player) -> Vec3 {
    Vec3::from(player.state().feet)
}

#[test]
fn ordinary_actors_keep_a_three_body_stack_supported() {
    let mut physics = floor();
    let tuning = PlayerTuning::default();
    let mut actors: Vec<_> = (0..3)
        .map(|i| {
            Player::spawn(
                &mut physics,
                i + 1,
                Vec3::new(0., i as f32 * (tuning.stand_height + 0.1) + 0.02, 0.),
                tuning.clone(),
            )
            .unwrap()
        })
        .collect();
    let mut lowest = [f32::MAX; 3];
    for tick in 0..120 * 10 {
        for player in &mut actors {
            player.step(&mut physics, MoveInput::default()).unwrap();
        }
        physics.step();
        if tick > 120 {
            for (i, player) in actors.iter().enumerate() {
                lowest[i] = lowest[i].min(feet(player).y);
            }
        }
    }
    eprintln!(
        "three-actor stack lowest={lowest:?} final={:?}",
        actors.iter().map(feet).collect::<Vec<_>>()
    );
    for (i, player) in actors.iter().enumerate() {
        assert!(
            lowest[i] >= i as f32 * tuning.stand_height - 0.05,
            "lost support at layer {i}"
        );
        assert!(player.state().grounded, "layer {i} has contact support");
    }
}

fn climb(support: bool) -> (bool, f32, Vec3) {
    let mut physics = floor();
    // The platform's 5-unit lip exceeds the ordinary 3.6-unit jump apex.
    physics.insert(
        RigidBodyBuilder::fixed(),
        ColliderBuilder::cuboid(1., 0.25, 2.).translation(Vector::new(3., 4.75, 0.)),
    );
    let mut base = support.then(|| {
        Player::spawn(
            &mut physics,
            1,
            Vec3::new(0., 0.02, 0.),
            PlayerTuning::default(),
        )
        .unwrap()
    });
    let mut climber = Player::spawn(
        &mut physics,
        2,
        Vec3::new(-1.35, 0.02, 0.),
        PlayerTuning::default(),
    )
    .unwrap();
    let mut mounted = false;
    let mut reached = false;
    let mut max_height = 0.0f32;
    for tick in 0..120 * 20 {
        if let Some(base) = &mut base {
            base.step(&mut physics, MoveInput::default()).unwrap();
        }
        let at = feet(&climber);
        if support && at.y > 2.5 && climber.state().grounded && at.x.abs() < 1.1 {
            mounted = true;
        }
        let destination = if mounted || !support { 3.0 } else { 0.0 };
        let right = if tick < 20 || (support && !mounted && at.y < 2.2) {
            0.0
        } else {
            ((destination - at.x) * 0.3).clamp(-0.6, 0.6)
        };
        let input = MoveInput {
            right,
            jump: tick >= 20 && !reached,
            ..Default::default()
        };
        climber.step(&mut physics, input).unwrap();
        physics.step();
        let at = feet(&climber);
        max_height = max_height.max(at.y);
        reached |= at.y > 4.95 && (2.0..4.0).contains(&at.x) && climber.state().grounded;
        if reached && tick > 120 * 3 {
            break;
        }
    }
    (reached, max_height, feet(&climber))
}

#[test]
fn an_actor_head_is_a_real_jump_support_for_an_unreachable_platform() {
    let alone = climb(false);
    let supported = climb(true);
    eprintln!("platform5 alone={alone:?} actor-supported={supported:?}");
    assert!(
        !alone.0,
        "the ordinary jump cannot reach the 5-unit platform alone"
    );
    assert!(
        supported.0,
        "ordinary inputs should climb through actor support"
    );
}

const DOOR_YAW: f32 = std::f32::consts::PI / 6.0;
fn room_point(local: Vec3) -> Vec3 {
    Vec3::new(0.17, 0., 20.23) + Mat3::from_rotation_y(DOOR_YAW) * local
}
fn room() -> Vec<ColliderBuilder> {
    let mut out =
        vec![ColliderBuilder::cuboid(100., 0.5, 100.).translation(Vector::new(0., -0.5, 0.))];
    // A real narrow opening, not a ray-only sensor: the box player's diagonal
    // projection is 1.708 units at this angle, within this 1.8-unit doorway.
    for (centre, half) in [
        (Vec3::new(-3.45, 2.4, 0.), Vec3::new(2.55, 2.4, 0.2)),
        (Vec3::new(3.45, 2.4, 0.), Vec3::new(2.55, 2.4, 0.2)),
        (Vec3::new(0., 3.95, 0.), Vec3::new(0.9, 0.85, 0.2)),
        (Vec3::new(-6.2, 2.4, 5.), Vec3::new(0.2, 2.4, 5.2)),
        (Vec3::new(6.2, 2.4, 5.), Vec3::new(0.2, 2.4, 5.2)),
        (Vec3::new(0., 2.4, 10.2), Vec3::new(6.4, 2.4, 0.2)),
    ] {
        let centre = room_point(centre);
        out.push(
            ColliderBuilder::cuboid(half.x, half.y, half.z)
                .translation(Vector::from_array(centre.to_array()))
                .rotation(Vector::new(0., DOOR_YAW, 0.)),
        );
    }
    out
}

fn bot_session(
    colliders: Vec<ColliderBuilder>,
    spawn: Vec3,
    target: Vec3,
    goals: Vec<Brick>,
) -> (Session, u64, u64) {
    let mut world = World::new(
        "Navigation feasibility".into(),
        "navigation/spike".into(),
        vec![[1.; 4]],
    );
    let at = Vec3::new(
        (spawn.x * 2.).round() / 2. + 0.25,
        0.1,
        (spawn.z * 2.).round() / 2. + 0.25,
    );
    let mut spawner = Brick::new(
        ContentRef::Resolved(fixture::PLATE.into()),
        at.to_array(),
        1,
    );
    spawner.vehicle = Some(Box::new(VehicleSpawn {
        vehicle: ContentRef::Resolved(fixture::BOT.into()),
        recolor: false,
        team: None,
    }));
    world.bricks.insert(1, spawner);
    for (i, mut goal) in goals.into_iter().enumerate() {
        goal.owner = 1;
        world.bricks.insert(i as u64 + 2, goal);
    }
    world.next_brick_id = world.bricks.len() as u64 + 1;
    let mut s = Session::new(
        Simulation::new(
            World::new(
                "Navigation feasibility".into(),
                "navigation/spike".into(),
                vec![[1.; 4]],
            ),
            fixture::synthetic_definitions().unwrap(),
            colliders,
        )
        .unwrap(),
    );
    let (vehicles, _) = fixture::synthetic_vehicles().unwrap();
    s.set_vehicle_pack(
        vehicles,
        bri_sim::bot_kind::BotPack::from_json(include_bytes!(
            "../../../packages/blockhead_bot/assets/bots.json"
        ))
        .unwrap()
        .bots,
    )
    .unwrap();
    s.set_tool_catalog(ToolCatalog {
        vehicles: [fixture::BOT.into()].into(),
        vehicle_bricks: [fixture::PLATE.into()].into(),
        ..Default::default()
    })
    .unwrap();
    s.set_event_catalog(bri_events::testing::catalog(), Vec::<String>::new())
        .unwrap();
    s.set_spawn_points(vec![target]).unwrap();
    let owner = s.join("Human target".into(), target, true).unwrap();
    for id in world.bricks.keys().copied().collect::<Vec<_>>() {
        world.bricks.get_mut(&id).unwrap().owner = owner;
    }
    s.command(
        owner,
        1,
        Command::LoadBuild {
            build: Box::new(bri_world::build::SavedBuild::new(world)),
            ownership: false,
        },
    )
    .unwrap();
    for tick in 0..20 {
        s.movement(owner, tick + 10, MoveInput::default()).unwrap();
        s.step().unwrap();
    }
    s.command(
        owner,
        2,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: bri_minigames::Settings {
                loadout: [None, None, None, None, None],
                ..Default::default()
            },
        }),
    )
    .unwrap();
    for tick in 0..60 {
        s.movement(owner, tick + 100, MoveInput::default()).unwrap();
        s.step().unwrap();
    }
    let bot = s.names().keys().copied().find(|id| s.is_bot(*id)).unwrap();
    (s, owner, bot)
}

fn player_feet(s: &Session, id: u64) -> Vec3 {
    Vec3::from(
        s.snapshot()
            .players
            .iter()
            .find(|p| p.owner == id)
            .unwrap()
            .feet,
    )
}

#[test]
fn a_rotated_off_grid_narrow_door_is_passable_by_human_and_pursuing_bot() {
    let start = room_point(Vec3::new(0., 0.02, -6.));
    let target = room_point(Vec3::new(0., 0.02, 7.));
    let mut physics = bri_physics::new_world();
    for collider in room() {
        physics.insert(RigidBodyBuilder::fixed(), collider);
    }
    bri_physics::detect_collisions(&mut physics);
    let mut human = Player::spawn(&mut physics, 1, start, PlayerTuning::default()).unwrap();
    for _ in 0..120 * 6 {
        let local = Mat3::from_rotation_y(-DOOR_YAW) * (target - feet(&human));
        human
            .step(
                &mut physics,
                MoveInput {
                    yaw: std::f32::consts::PI - DOOR_YAW,
                    forward: (local.z * 0.3).clamp(-1., 1.),
                    right: (-local.x * 0.3).clamp(-1., 1.),
                    ..Default::default()
                },
            )
            .unwrap();
        physics.step();
    }
    assert!(
        feet(&human).distance(target) < 0.5,
        "human cleared real opening: {}",
        feet(&human)
    );
    // Acquire the visible target outside before it walks into the room.
    let (mut s, owner, bot) =
        bot_session(room(), start, room_point(Vec3::new(0., 0.02, -3.)), vec![]);
    let mut closest = f32::MAX;
    for tick in 0..120 * 35 {
        let local = Mat3::from_rotation_y(-DOOR_YAW) * (target - player_feet(&s, owner));
        s.movement(
            owner,
            tick + 1000,
            MoveInput {
                yaw: std::f32::consts::PI - DOOR_YAW,
                forward: (local.z * 0.3).clamp(-1., 1.),
                right: (-local.x * 0.3).clamp(-1., 1.),
                ..Default::default()
            },
        )
        .unwrap();
        s.step().unwrap();
        closest = closest.min(player_feet(&s, bot).distance(target));
        if closest < 2.5 {
            break;
        }
    }
    eprintln!(
        "rotated narrow door human={} bot={} closest={closest} thoughts={:?}",
        feet(&human),
        player_feet(&s, bot),
        s.bot_thoughts()
    );
    assert!(
        closest < 2.5,
        "bot must traverse the same human-passable opening"
    );
}

#[test]
fn fixed_navigation_does_not_discover_the_physically_valid_actor_support() {
    use bri_sim::nav::{Body, Found, Ground, Nav, Search};
    let mut physics = floor();
    physics.insert(
        RigidBodyBuilder::fixed(),
        ColliderBuilder::cuboid(1., 0.25, 2.).translation(Vector::new(3., 4.75, 0.)),
    );
    let tuning = PlayerTuning::default();
    let mut support =
        Player::spawn(&mut physics, 1, Vec3::new(0., 0.02, 0.), tuning.clone()).unwrap();
    for _ in 0..120 {
        support.step(&mut physics, MoveInput::default()).unwrap();
        physics.step();
    }
    let passages = bri_content::passage::Passages::default();
    let ground = Ground {
        physics: &physics,
        terrain: &|_, _, _| None,
        passages: &passages,
        waters: &[],
        bodies: &[],
    };
    let body = Body::of(&tuning, 1.);
    let mut nav = Nav::default();
    let mut search = Search::new(Vec3::new(-1.35, 0., 0.), Vec3::new(3., 5., 0.), 20.);
    let found = (0..10000)
        .find_map(|_| {
            nav.begin_tick();
            search.step(&mut nav, &ground, &body)
        })
        .expect("bounded search completes");
    eprintln!("actor-supported platform fixed-nav result={found:?}");
    assert!(
        matches!(found, Found::Partial(_)),
        "fixed-only mechanism does not invent actor support"
    );
}

fn scoring_goal(at: [f32; 3]) -> Brick {
    let mut goal = Brick::new(ContentRef::Resolved(fixture::PLATE.into()), at, 1);
    goal.name = Some("distant_authored_finish".into());
    goal.rule_region = Some([3., 4., 3.]);
    goal.colliding = false;
    goal.visible = false;
    goal.raycast = false;
    for (output, params) in [
        ("addPlayerScore", vec![bri_events::Value::Int(17)]),
        ("winRound", vec![]),
    ] {
        goal.events.push(bri_events::Row {
            enabled: true,
            input: "onRegionEnter".into(),
            delay_ms: 0,
            target: bri_events::Target::Slot(bri_events::Slot::Player),
            output: output.into(),
            params,
            conditions: vec![],
            preserved: None,
        });
    }
    goal
}

#[test]
fn a_distant_authored_objective_continues_multiple_bounded_navigation_segments() {
    let goal = Vec3::new(80.25, 0.1, 0.25);
    let start = Vec3::new(-75., 0.02, 0.);
    let (mut s, owner, bot) = bot_session(
        vec![ColliderBuilder::cuboid(100., 0.5, 100.).translation(Vector::new(0., -0.5, 0.))],
        start,
        Vec3::new(-90., 0.02, -70.),
        vec![scoring_goal(goal.to_array())],
    );
    let mut max_x = player_feet(&s, bot).x;
    let mut saw_objective = false;
    for tick in 0..120 * 60 {
        s.movement(owner, tick + 1000, MoveInput::default())
            .unwrap();
        s.step().unwrap();
        max_x = max_x.max(player_feet(&s, bot).x);
        saw_objective |= s
            .bot_thoughts()
            .iter()
            .any(|b| b.bot == bot && b.objective == Some(2));
        if s.vitals()[&bot].score == 17 {
            break;
        }
    }
    eprintln!(
        "distant objective start={start} max_x={max_x} final={} score={} thoughts={:?}",
        player_feet(&s, bot),
        s.vitals()[&bot].score,
        s.bot_thoughts()
    );
    assert!(saw_objective, "authored rules selected an actual objective");
    assert_eq!(
        s.vitals()[&bot].score,
        17,
        "physically entered the goal after >155 units, exceeding the 72-unit search bound"
    );
    assert!(max_x > 78., "ordinary movement reached the finish region");
}

#[test]
fn an_objective_recovers_when_a_real_actor_walks_out_of_its_blocked_corridor() {
    let colliders = vec![
        ColliderBuilder::cuboid(60., 0.5, 20.).translation(Vector::new(0., -0.5, 0.)),
        ColliderBuilder::cuboid(40., 1.5, 0.2).translation(Vector::new(0., 1.5, 1.35)),
        ColliderBuilder::cuboid(40., 1.5, 0.2).translation(Vector::new(0., 1.5, -0.85)),
        ColliderBuilder::cuboid(40., 0.25, 1.3).translation(Vector::new(0., 3.25, 0.25)),
    ];
    let (mut s, owner, bot) = bot_session(
        colliders,
        Vec3::new(-12., 0.02, 0.),
        Vec3::new(-30., 0.02, -10.),
        vec![scoring_goal([12.25, 0.1, 0.25])],
    );
    // A real human actor outside the bot's MiniGame cannot be targeted as an
    // enemy. It blocks physical movement, then clears it through normal input.
    let blocker = s
        .join("Passing builder".into(), Vec3::new(0., 0.02, 0.25), false)
        .unwrap();
    let mut blocked_x = None;
    for tick in 0..120 * 20 {
        let leaving = tick >= 120 * 5;
        s.movement(owner, tick + 1000, MoveInput::default())
            .unwrap();
        s.movement(
            blocker,
            tick + 1000,
            MoveInput {
                right: if leaving { 1. } else { 0. },
                ..Default::default()
            },
        )
        .unwrap();
        s.step().unwrap();
        if tick == 120 * 4 {
            blocked_x = Some(player_feet(&s, bot).x);
        }
        if s.vitals()[&bot].score == 17 {
            break;
        }
    }
    eprintln!(
        "actor obstruction before release={blocked_x:?} bot={} blocker={} score={} thoughts={:?}",
        player_feet(&s, bot),
        player_feet(&s, blocker),
        s.vitals()[&bot].score,
        s.bot_thoughts()
    );
    assert!(
        blocked_x.unwrap() < 0.5,
        "physical actor held the approach until it moved"
    );
    assert_eq!(
        s.vitals()[&bot].score,
        17,
        "recovered ordinary approach after support/clearance changed"
    );
}

fn named_variable_reaction_case(reaction: bool) -> (i64, bool, bool) {
    use bri_events::rules::{Compare, Condition, Datum, Property, Subject};
    use bri_events::{Row, Slot, Target, Value};
    let row = |input: &str, target, output: &str, params| Row {
        enabled: true,
        input: input.into(),
        delay_ms: 0,
        target,
        output: output.into(),
        params,
        conditions: vec![],
        preserved: None,
    };
    let mut writer = scoring_goal([6.25, 0.1, 0.25]);
    writer.events = vec![
        row(
            "onRegionEnter",
            Target::Named("latch".into()),
            "addVariable",
            vec![Value::Int(0), Value::Text("pulse".into()), Value::Int(1)],
        ),
        row(
            "onRegionEnter",
            Target::Slot(Slot::SelfBrick),
            "setVariable",
            vec![Value::Int(2), Value::Text("progress".into()), Value::Int(1)],
        ),
    ];
    let mut latch = Brick::new(
        ContentRef::Resolved(fixture::PLATE.into()),
        [12.25, 0.1, 8.25],
        0,
    );
    latch.name = Some("latch".into());
    if reaction {
        latch.events = vec![
            row(
                "onRuleVariableChanged",
                Target::Slot(Slot::SelfBrick),
                "setVariable",
                vec![Value::Int(2), Value::Text("progress".into()), Value::Int(0)],
            ),
            row(
                "onRuleVariableChanged",
                Target::Slot(Slot::SelfBrick),
                "setRendering",
                vec![Value::Bool(false)],
            ),
        ];
    }
    let mut finish = scoring_goal([18.25, 0.1, 0.25]);
    for event in &mut finish.events {
        event.conditions.push(Condition {
            subject: Subject::MiniGame,
            property: Property::Variable,
            key: "progress".into(),
            compare: Compare::Equal,
            value: Datum::Number(1),
        });
    }
    let (mut s, owner, bot) = bot_session(
        vec![ColliderBuilder::cuboid(60., 0.5, 60.).translation(Vector::new(0., -0.5, 0.))],
        Vec3::new(-8., 0.02, 0.),
        Vec3::new(-30., 0.02, -20.),
        vec![writer, latch, finish],
    );
    let mut selected_writer = false;
    for tick in 0..120 * 20 {
        s.movement(owner, tick + 1000, MoveInput::default())
            .unwrap();
        s.step().unwrap();
        selected_writer |= s
            .bot_thoughts()
            .iter()
            .any(|b| b.bot == bot && b.objective == Some(2));
        if s.vitals()[&bot].score == 17 {
            break;
        }
    }
    let result = (
        s.vitals()[&bot].score,
        s.simulation().state().bricks[&3].visible,
        selected_writer,
    );
    eprintln!(
        "named target reaction={reaction} result={result:?} thoughts={:?}",
        s.bot_thoughts()
    );
    result
}

#[test]
fn an_unmodelled_named_target_reaction_rejects_the_whole_objective_action() {
    let plain = named_variable_reaction_case(false);
    assert_eq!(
        plain.0, 17,
        "ordinary NPC movement executes the two-step supported rule chain"
    );
    assert!(plain.2, "the supported writer is selected");
    let reactive = named_variable_reaction_case(true);
    assert_eq!(
        reactive.0, 0,
        "a named target child input invalidates the projected win"
    );
    assert!(
        reactive.1,
        "NPC does not execute the supported prefix and hide the latch as collateral"
    );
    assert!(
        !reactive.2,
        "the entire unknown action is rejected before approach/activation"
    );
}

#[test]
fn workshop_race_recipe_finishes_three_laps_through_actual_npc_region_crossings() {
    let (mut s, owner, bot) = bot_session(
        vec![ColliderBuilder::cuboid(100., 0.5, 100.).translation(Vector::new(0., -0.5, 0.))],
        Vec3::new(-8., 0.02, 0.),
        Vec3::new(-30., 0.02, -20.),
        vec![],
    );
    s.command(
        owner,
        10000,
        Command::Package(bri_sim::session::PackageCommand {
            package: String::new(),
            command: "rulelab".into(),
            args: vec![bri_sim::session::PackageArg::String("race".into())],
        }),
    )
    .unwrap();
    let checkpoints: Vec<_> = s
        .simulation()
        .state()
        .bricks
        .iter()
        .filter(|(_, b)| {
            b.name
                .as_deref()
                .is_some_and(|n| n.starts_with("lab_race_"))
        })
        .map(|(id, b)| (*id, b.position))
        .collect();
    assert_eq!(
        checkpoints.len(),
        3,
        "normal Workshop command creates its shipping recipe"
    );
    let finish = checkpoints.last().unwrap().0;
    s.explain_rules(owner, finish).unwrap();
    s.take_private_notices();
    let mut scores = std::collections::BTreeSet::new();
    let mut selected = std::collections::BTreeSet::new();
    for tick in 0..120 * 90 {
        s.movement(owner, tick + 20000, MoveInput::default())
            .unwrap();
        s.step().unwrap();
        scores.insert(s.vitals()[&bot].score);
        selected.extend(
            s.bot_thoughts()
                .iter()
                .filter(|b| b.bot == bot)
                .filter_map(|b| b.objective),
        );
        if s.vitals()[&bot].score == 3 {
            break;
        }
    }
    eprintln!(
        "Workshop race checkpoints={checkpoints:?} selected={selected:?} scores={scores:?} final={} thoughts={:?}",
        player_feet(&s, bot),
        s.bot_thoughts()
    );
    assert!(
        scores.contains(&1) && scores.contains(&2),
        "three actual laps produce separate score transitions"
    );
    assert_eq!(
        s.vitals()[&bot].score,
        3,
        "shipping recipe completes all three laps"
    );
    assert!(
        checkpoints.iter().all(|(id, _)| selected.contains(id)),
        "actual planner approaches all authored checkpoints"
    );
    s.explain_rules(owner, finish).unwrap();
    let trace: Vec<_> = s
        .take_private_notices()
        .into_iter()
        .filter_map(|(_, n)| match n {
            bri_sim::session::Notice::Chat(t) => Some(t),
            _ => None,
        })
        .collect();
    assert!(
        trace
            .iter()
            .any(|t| t.contains("winRound -> Player") && t.ends_with(": ran")),
        "canonical winRound executed, not score alone: {trace:?}"
    );
    // Running guards cease after the real round end: further locomotion may
    // continue but cannot award a fourth lap.
    for tick in 0..120 * 5 {
        s.movement(owner, tick + 40000, MoveInput::default())
            .unwrap();
        s.step().unwrap();
    }
    assert_eq!(
        s.vitals()[&bot].score,
        3,
        "round-over gating stops further lap awards"
    );
}
