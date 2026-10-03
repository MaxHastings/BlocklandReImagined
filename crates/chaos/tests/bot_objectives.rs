//! Actual control/rule compositions; no direct objective or input injection.
use bri_chaos::fixture;
use bri_events::rules::{Compare, Condition, Datum, Property, Subject};
use bri_events::{Row, Slot, Target, Value};
use bri_minigames::Settings;
use bri_sim::player::MoveInput;
use bri_sim::session::{Command, MiniGameRequest, Session, ToolCatalog};
use bri_world::{Brick, ContentRef, OwnerId, VehicleSpawn, World, build::SavedBuild};
use glam::Vec3;

fn row(
    input: &str,
    output: &str,
    target: Slot,
    params: Vec<Value>,
    conditions: Vec<Condition>,
    delay_ms: u32,
) -> Row {
    Row {
        enabled: true,
        input: input.into(),
        output: output.into(),
        target: Target::Slot(target),
        params,
        conditions,
        delay_ms,
        preserved: None,
    }
}
fn variable(key: &str, value: i64) -> Condition {
    Condition {
        subject: Subject::Player,
        property: Property::Variable,
        key: key.into(),
        compare: Compare::Equal,
        value: Datum::Number(value),
    }
}
fn game(bricks: Vec<Brick>) -> (Session, OwnerId, u64) {
    let mut s = fixture::synthetic().unwrap().session;
    s.set_spawn_points(vec![Vec3::new(-40.0, 0.05, -40.0)])
        .unwrap();
    s.set_event_catalog(bri_events::testing::catalog(), Vec::<String>::new())
        .unwrap();
    s.set_tool_catalog(ToolCatalog {
        vehicles: [fixture::BOT.into()].into(),
        vehicle_bricks: [fixture::PLATE.into()].into(),
        ..Default::default()
    })
    .unwrap();
    let owner = s
        .join("Author".into(), Vec3::new(-40.0, 0.05, -40.0), true)
        .unwrap();
    let mut world = World::new(
        "Unfamiliar rules".into(),
        "chaos/map".into(),
        vec![[1.0; 4]; 8],
    );
    let mut spawn = Brick::new(
        ContentRef::Resolved(fixture::PLATE.into()),
        [0.25, 0.1, 20.25],
        owner,
    );
    spawn.vehicle = Some(Box::new(VehicleSpawn {
        vehicle: ContentRef::Resolved(fixture::BOT.into()),
        recolor: false,
    }));
    world.bricks.insert(1, spawn);
    for (i, mut b) in bricks.into_iter().enumerate() {
        b.owner = owner;
        world.bricks.insert(i as u64 + 2, b);
    }
    world.next_brick_id = world.bricks.len() as u64 + 1;
    s.command(
        owner,
        100,
        Command::LoadBuild {
            build: Box::new(SavedBuild::new(world)),
            ownership: false,
        },
    )
    .unwrap();
    let mut sequence = 10000;
    ticks(&mut s, owner, &mut sequence, 20);
    s.command(
        owner,
        101,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: Settings {
                loadout: [None, None, None, None, None],
                ..Default::default()
            },
        }),
    )
    .unwrap();
    ticks(&mut s, owner, &mut sequence, 20);
    (s, owner, sequence)
}
fn ticks(s: &mut Session, owner: OwnerId, sequence: &mut u64, count: usize) {
    for _ in 0..count {
        *sequence += 1;
        s.movement(owner, *sequence, MoveInput::default()).unwrap();
        s.step().unwrap();
    }
}
fn brick(at: [f32; 3], name: &str, rows: Vec<Row>) -> Brick {
    let mut b = Brick::new(ContentRef::Resolved(fixture::PLATE.into()), at, 0);
    b.name = Some(name.into());
    b.events = rows;
    b
}
fn run_score(s: &mut Session, owner: OwnerId, sequence: &mut u64, score: i64) -> OwnerId {
    let bot = s
        .names()
        .keys()
        .copied()
        .find(|id| s.is_bot(*id))
        .expect("spawned actual bot");
    for _ in 0..120 * 40 {
        ticks(s, owner, sequence, 1);
        if s.vitals()[&bot].score == score {
            return bot;
        }
    }
    panic!(
        "expected score {score}, vitals {:?}, thought {:?}, diagnostics {:?}",
        s.vitals(),
        s.bot_thoughts(),
        s.take_event_diagnostics()
    );
}

#[test]
fn four_specific_activation_bricks_compose_without_injected_goals() {
    let flags = ["blueLatch", "violetLatch", "copperLatch", "linenLatch"];
    let guards: Vec<_> = flags.iter().map(|key| variable(key, 1)).collect();
    let mut bricks = Vec::new();
    for (i, key) in flags.iter().enumerate() {
        let mut rows = vec![row(
            "onActivate",
            "setVariable",
            Slot::SelfBrick,
            vec![Value::Int(1), Value::Text((*key).into()), Value::Int(1)],
            vec![variable(key, 0)],
            0,
        )];
        if i == 3 {
            rows.push(row(
                "onActivate",
                "addPlayerScore",
                Slot::Player,
                vec![Value::Int(17)],
                guards.clone(),
                450,
            ));
            rows.push(row(
                "onActivate",
                "winRound",
                Slot::Player,
                vec![],
                guards.clone(),
                450,
            ));
        }
        bricks.push(brick(
            [0.25, 0.1, 26.25 + i as f32 * 5.0],
            &format!("ornament_{i}"),
            rows,
        ));
    }
    // World order and authored names carry no objective policy.
    bricks.swap(0, 2);
    let (mut s, owner, mut sequence) = game(bricks);
    let bot = run_score(&mut s, owner, &mut sequence, 17);
    assert_eq!(
        s.vitals()[&owner].score,
        0,
        "NPC Player target must not award author"
    );
    ticks(&mut s, owner, &mut sequence, 300);
    assert_eq!(
        s.vitals()[&bot].score,
        17,
        "round completion stops duplicate awards"
    );
    assert!(s.take_event_diagnostics().is_empty());
}

#[test]
fn ordered_regions_observe_delayed_partial_progress_before_continuing() {
    let mut bricks = Vec::new();
    for i in 0..3 {
        let mut rows = vec![row(
            "onRegionEnter",
            "setVariable",
            Slot::SelfBrick,
            vec![
                Value::Int(1),
                Value::Text("itinerary".into()),
                Value::Int(i + 1),
            ],
            vec![variable("itinerary", i)],
            250,
        )];
        if i == 2 {
            rows.push(row(
                "onRegionEnter",
                "addPlayerScore",
                Slot::Player,
                vec![Value::Int(23)],
                vec![variable("itinerary", 3)],
                500,
            ));
            rows.push(row(
                "onRegionEnter",
                "winRound",
                Slot::Player,
                vec![],
                vec![variable("itinerary", 3)],
                500,
            ));
        }
        let mut b = brick(
            [0.25, 0.1, 27.25 + i as f32 * 6.0],
            &format!("sensor_{}", 9 - i),
            rows,
        );
        b.rule_region = Some([3.0, 4.0, 3.0]);
        b.visible = false;
        b.colliding = false;
        b.raycast = false;
        bricks.push(b);
    }
    bricks.reverse();
    let (mut s, owner, mut sequence) = game(bricks);
    let bot = run_score(&mut s, owner, &mut sequence, 23);
    ticks(&mut s, owner, &mut sequence, 240);
    assert_eq!(s.vitals()[&bot].score, 23);
    assert_eq!(s.vitals()[&owner].score, 0);
    assert!(s.take_event_diagnostics().is_empty());
}

#[test]
fn repeated_region_counter_rearms_by_physically_leaving_and_entering() {
    let rows = vec![
        row(
            "onRegionEnter",
            "addVariable",
            Slot::SelfBrick,
            vec![Value::Int(1), Value::Text("visits".into()), Value::Int(1)],
            vec![],
            0,
        ),
        row(
            "onRegionEnter",
            "addPlayerScore",
            Slot::Player,
            vec![Value::Int(31)],
            vec![variable("visits", 3)],
            0,
        ),
        row(
            "onRegionEnter",
            "winRound",
            Slot::Player,
            vec![],
            vec![variable("visits", 3)],
            0,
        ),
    ];
    let mut sensor = brick([0.25, 0.1, 28.25], "counted_window", rows);
    sensor.rule_region = Some([3.0, 4.0, 3.0]);
    sensor.visible = false;
    sensor.colliding = false;
    sensor.raycast = false;
    let (mut s, owner, mut sequence) = game(vec![sensor]);
    let bot = run_score(&mut s, owner, &mut sequence, 31);
    assert_eq!(s.vitals()[&owner].score, 0);
    assert!(
        s.snapshot()
            .players
            .iter()
            .find(|p| p.owner == bot)
            .unwrap()
            .feet[2]
            > 25.0,
        "actually moved to the authored sensor"
    );
}

#[test]
fn unrelated_named_decor_and_timer_do_not_disable_the_creator_objective() {
    let rows = vec![
        row(
            "onActivate",
            "setVariable",
            Slot::SelfBrick,
            vec![Value::Int(1), Value::Text("latch".into()), Value::Int(1)],
            vec![variable("latch", 0)],
            0,
        ),
        row(
            "onActivate",
            "setColor",
            Slot::SelfBrick,
            vec![Value::Color(2)],
            vec![],
            0,
        ),
        row(
            "onActivate",
            "addPlayerScore",
            Slot::Player,
            vec![Value::Int(37)],
            vec![variable("latch", 1)],
            150,
        ),
        row(
            "onActivate",
            "winRound",
            Slot::Player,
            vec![],
            vec![variable("latch", 1)],
            150,
        ),
    ];
    let mut bricks = vec![brick([0.25, 0.1, 28.25], "goal_panel", rows)];
    bricks.push(brick(
        [-20.25, 0.1, 28.25],
        "unrelated_clock",
        vec![
            row(
                "onRuleTimer",
                "addVariable",
                Slot::SelfBrick,
                vec![
                    Value::Int(2),
                    Value::Text("clock_ticks".into()),
                    Value::Int(1),
                ],
                vec![],
                0,
            ),
            row(
                "onRuleTimer",
                "setColor",
                Slot::SelfBrick,
                vec![Value::Color(3)],
                vec![],
                0,
            ),
        ],
    ));
    for i in 0..96 {
        bricks.push(brick(
            [20.25, 0.1, 30.25 + i as f32 * 0.5],
            &format!("decoration_{i}"),
            vec![],
        ));
    }
    let (mut s, owner, mut sequence) = game(bricks);
    run_score(&mut s, owner, &mut sequence, 37);
    assert!(s.take_event_diagnostics().is_empty());
}

#[test]
fn ungrounded_collateral_on_the_same_input_is_not_omitted() {
    let rows = vec![
        row("onActivate", "winRound", Slot::Player, vec![], vec![], 0),
        row(
            "onActivate",
            "setColliding",
            Slot::SelfBrick,
            vec![Value::Bool(false)],
            vec![],
            0,
        ),
    ];
    let (mut s, owner, mut sequence) =
        game(vec![brick([0.25, 0.1, 28.25], "unsupported_mix", rows)]);
    ticks(&mut s, owner, &mut sequence, 360);
    assert!(s.bot_thoughts().iter().all(|t| t.objective.is_none()));
    assert!(
        s.bot_thoughts()
            .iter()
            .any(|t| t.objective_diagnostic == Some("unsupported rule semantics")),
        "unsupported side effects must stay explicit: {:?}",
        s.bot_thoughts()
    );
    let source = *s
        .simulation()
        .state()
        .bricks
        .iter()
        .find(|(_, b)| b.name.as_deref() == Some("unsupported_mix"))
        .unwrap()
        .0;
    assert!(s.simulation().state().bricks[&source].colliding);
    s.take_private_notices();
    s.explain_rules(owner, source).unwrap();
    let notices = s.take_private_notices();
    assert!(notices.iter().any(|(who, notice)| *who == owner
        && matches!(notice, bri_sim::session::Notice::Chat(text) if text.starts_with("[NPC ") && text.contains("unsupported rule semantics"))), "creator Explain exposes bounded bot reason: {notices:?}");
}

#[test]
fn relevant_timer_reaction_is_reported_until_its_closure_is_grounded() {
    let goal = brick(
        [0.25, 0.1, 28.25],
        "goal_window",
        vec![
            row(
                "onActivate",
                "setVariable",
                Slot::SelfBrick,
                vec![
                    Value::Int(2),
                    Value::Text("shared_latch".into()),
                    Value::Int(1),
                ],
                vec![],
                0,
            ),
            row(
                "onActivate",
                "winRound",
                Slot::Player,
                vec![],
                vec![Condition {
                    subject: Subject::MiniGame,
                    property: Property::Variable,
                    key: "shared_latch".into(),
                    compare: Compare::Equal,
                    value: Datum::Number(1),
                }],
                100,
            ),
        ],
    );
    let reaction = brick(
        [-20.25, 0.1, 28.25],
        "real_interference",
        vec![row(
            "onRuleTimer",
            "setVariable",
            Slot::SelfBrick,
            vec![
                Value::Int(2),
                Value::Text("shared_latch".into()),
                Value::Int(0),
            ],
            vec![],
            0,
        )],
    );
    let (mut s, owner, mut sequence) = game(vec![goal, reaction]);
    ticks(&mut s, owner, &mut sequence, 360);
    assert!(
        s.bot_thoughts()
            .iter()
            .any(|t| t.objective_diagnostic == Some("unsupported rule semantics")),
        "a relevant writer must not be ignored: {:?}",
        s.bot_thoughts()
    );
}

#[test]
fn inaccessible_cheaper_region_cools_down_and_the_reachable_alternative_wins() {
    let winning = |input| {
        vec![
            row(
                input,
                "addPlayerScore",
                Slot::Player,
                vec![Value::Int(43)],
                vec![],
                0,
            ),
            row(input, "winRound", Slot::Player, vec![], vec![], 0),
        ]
    };
    // This authored region has no physical route from the floor. Its approach
    // point is nearby, but being near never proves the actual enter input.
    let mut unreachable = brick(
        [0.25, 100.1, 23.25],
        "ceiling checkpoint",
        winning("onRegionEnter"),
    );
    unreachable.rule_region = Some([2.0, 1.0, 2.0]);
    unreachable.colliding = false;
    unreachable.visible = false;
    unreachable.raycast = false;
    let alternative = brick(
        [0.25, 0.1, 36.25],
        "ordinary reachable choice",
        winning("onActivate"),
    );
    let (mut s, owner, mut sequence) = game(vec![unreachable, alternative]);
    let bot = run_score(&mut s, owner, &mut sequence, 43);
    assert_eq!(s.vitals()[&owner].score, 0);
    ticks(&mut s, owner, &mut sequence, 240);
    assert_eq!(s.vitals()[&bot].score, 43);
    assert!(s.take_event_diagnostics().is_empty());
}

#[test]
fn named_target_fanout_hits_the_grounding_budget_before_per_target_projection() {
    let mut paint = row(
        "onActivate",
        "setColor",
        Slot::SelfBrick,
        vec![Value::Color(2)],
        vec![],
        0,
    );
    paint.target = Target::Named("sharedDecoration".into());
    let mut bricks = vec![brick(
        [0.25, 0.1, 27.25],
        "fanout writer",
        vec![
            paint,
            row("onActivate", "winRound", Slot::Player, vec![], vec![], 0),
        ],
    )];
    for i in 0..4096 {
        let mut decoration = brick(
            [100.25 + (i % 64) as f32, 0.1, 100.25 + (i / 64) as f32],
            "sharedDecoration",
            vec![],
        );
        decoration.color = 0;
        decoration.colliding = false;
        decoration.raycast = false;
        bricks.push(decoration);
    }
    let (mut s, owner, mut sequence) = game(bricks);
    let before: Vec<_> = s
        .simulation()
        .state()
        .bricks
        .iter()
        .filter(|(_, b)| b.name.as_deref() == Some("sharedDecoration"))
        .map(|(id, b)| (*id, b.color))
        .collect();
    ticks(&mut s, owner, &mut sequence, 240);
    assert!(
        s.bot_thoughts()
            .iter()
            .any(|t| t.objective_diagnostic == Some("objective planning budget/model limit")),
        "fanout diagnosed before projection: {:?}",
        s.bot_thoughts()
    );
    assert!(s.bot_thoughts().iter().all(|t| t.objective.is_none()));
    assert!(
        before
            .iter()
            .all(|(id, color)| s.simulation().state().bricks[id].color == *color),
        "unsupported fanout did not execute any color prefix"
    );
}
