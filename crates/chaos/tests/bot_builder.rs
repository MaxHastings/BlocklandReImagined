//! Builder-authored controls exercised through live event programs, movement,
//! damage and new lives. All fixture content is invented.
use bri_chaos::fixture;
use bri_events::{Slot, Value};
use bri_sim::{
    bot_kind::{BotMelee, BotPack},
    player::MoveInput,
    session::{Command, MiniGameRequest, Session, ToolCatalog},
};
use bri_world::{
    Brick, ContentRef, EventRow, EventTarget, VehicleSpawn, World, authority::Edit,
    build::SavedBuild,
};
use glam::Vec3;

const HOME: Vec3 = Vec3::new(-40.0, 0.1, 30.0);
const ENEMY: Vec3 = Vec3::new(-40.0, 0.05, 48.0);

fn row(input: &str, slot: Slot, output: &str, params: Vec<Value>) -> EventRow {
    EventRow {
        conditions: vec![],
        preserved: None,
        enabled: true,
        input: input.into(),
        delay_ms: 0,
        target: EventTarget::Slot(slot),
        output: output.into(),
        params,
    }
}

struct Game {
    s: Session,
    human: u64,
    bot: u64,
    seq: u64,
}
impl Game {
    fn new(rows: Vec<EventRow>) -> Self {
        let mut s = fixture::synthetic().unwrap().session;
        let mut kinds = BotPack::from_json(include_bytes!(
            "../../../packages/blockhead_bot/assets/bots.json"
        ))
        .unwrap()
        .bots;
        let k = &mut kinds[0];
        k.sight = 32.0;
        k.chase_radius = 8.0;
        k.wander_radius = 2.0;
        k.reaction_seconds = 0.0;
        k.alerts_allies = false;
        k.weapons = false;
        for name in ["interact", "explore", "fly"] {
            k.behaviours.insert(name.into(), 0.0);
        }
        k.melee = Some(BotMelee {
            damage: 1.0,
            reach: 2.5,
            seconds: 0.5,
            action: None,
            name: "Fixture bite".into(),
            converts_below: None,
        });
        s.set_bot_kinds(kinds).unwrap();
        s.set_tool_catalog(ToolCatalog {
            vehicles: [fixture::BOT.to_string()].into(),
            vehicle_bricks: [fixture::PLATE.to_string()].into(),
            ..Default::default()
        })
        .unwrap();
        let mut catalog = bri_events::testing::catalog_extended();
        catalog.inputs.push(bri_events::InputDef {
            id: "fixture/death".into(),
            class_name: "fxDTSBrick".into(),
            name: "fixtureDeath".into(),
            targets: vec![
                ("Bot".into(), "Player".into()),
                ("Self".into(), "fxDTSBrick".into()),
            ],
            source: "invented fixture".into(),
            source_line: 0,
        });
        catalog.outputs.push(bri_events::OutputDef {
            id: "fixture/setHealth".into(),
            class_name: "Player".into(),
            name: "setHealth".into(),
            params: vec![bri_events::Param::Int {
                min: 0,
                max: 1000,
                default: 100,
            }],
            append_client: false,
            source: "invented fixture".into(),
            source_line: 0,
            package: None,
        });
        s.set_event_catalog(catalog, Vec::new()).unwrap();
        s.set_spawn_points(vec![ENEMY]).unwrap();
        let human = s.join("Builder".into(), ENEMY, true).unwrap();
        let mut brick = Brick::new(
            ContentRef::Resolved(fixture::PLATE.into()),
            (HOME + Vec3::new(0.25, 0.0, 0.25)).to_array(),
            human,
        );
        brick.vehicle = Some(Box::new(VehicleSpawn {
            vehicle: ContentRef::Resolved(fixture::BOT.into()),
            recolor: false,
            team: None,
        }));
        brick.events = rows;
        brick.name = Some("spawner".into());
        let mut world = World::new(
            "Builder bots".into(),
            "chaos/map".into(),
            vec![[1.0; 4], [0.0, 1.0, 0.0, 1.0]],
        );
        world.bricks.insert(1, brick);
        world.next_brick_id = 2;
        s.command(
            human,
            1,
            Command::LoadBuild {
                build: Box::new(SavedBuild::new(world)),
                ownership: false,
            },
        )
        .unwrap();
        for _ in 0..5 {
            s.step().unwrap();
        }
        let bot = *s.names().keys().find(|id| s.is_bot(**id)).unwrap();
        let mut g = Self {
            s,
            human,
            bot,
            seq: 1 << 40,
        };
        g.send(Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: bri_minigames::Settings {
                loadout: Default::default(),
                ..Default::default()
            },
        }));
        g.steps(610);
        let diagnostics = g.s.take_event_diagnostics();
        assert!(
            diagnostics.is_empty(),
            "event programs must install and run: {diagnostics:?}"
        );
        g
    }
    fn send(&mut self, c: Command) {
        self.seq += 1;
        self.s.command(self.human, self.seq, c).unwrap();
    }
    fn steps(&mut self, n: usize) {
        for _ in 0..n {
            self.seq += 1;
            self.s
                .movement(self.human, self.seq, MoveInput::default())
                .unwrap();
            self.s.step().unwrap();
        }
        assert_eq!(self.s.bot_step_failures().0, 0);
    }
    fn at(&self) -> Vec3 {
        self.s
            .snapshot()
            .players
            .iter()
            .find(|p| p.owner == self.bot)
            .unwrap()
            .feet
            .into()
    }
    fn edit_and_fire(&mut self, rows: Vec<EventRow>) {
        self.s
            .edit_brick(self.human, 1, Edit::Events(rows))
            .unwrap();
        self.s.fire_brick_input(1, "onActivate", Some(self.human));
        self.steps(1);
        assert!(self.s.take_event_diagnostics().is_empty());
    }
}

#[test]
fn a_builder_can_pause_configure_release_and_stop_a_real_pursuing_bot() {
    let mut g = Game::new(vec![row(
        "onBotSpawn",
        Slot::SelfBrick,
        "setBotPowered",
        vec![Value::Bool(false)],
    )]);
    assert_eq!(g.s.vitals()[&g.human].health, 100.0);
    let home = g.at();
    g.steps(120);
    assert!(
        g.at().distance(home) < 0.5,
        "spawn initializer kept the bot still"
    );
    g.edit_and_fire(vec![
        row(
            "onActivate",
            Slot::SelfBrick,
            "setBotSightRadius",
            vec![Value::Float(0.0)],
        ),
        row(
            "onActivate",
            Slot::SelfBrick,
            "setBotWanderRadius",
            vec![Value::Float(0.0)],
        ),
        row(
            "onActivate",
            Slot::SelfBrick,
            "setBotReturnHome",
            vec![Value::Bool(false)],
        ),
        row(
            "onActivate",
            Slot::SelfBrick,
            "setBotPowered",
            vec![Value::Bool(true)],
        ),
    ]);
    g.steps(240);
    assert!(
        g.at().distance(home) < 0.5,
        "disabled vision and wandering are independent of power"
    );
    assert_eq!(g.s.vitals()[&g.human].health, 100.0);
    g.edit_and_fire(vec![
        row(
            "onActivate",
            Slot::SelfBrick,
            "setBotSightRadius",
            vec![Value::Float(32.0)],
        ),
        row(
            "onActivate",
            Slot::SelfBrick,
            "setBotMeleeDamage",
            vec![Value::Float(5.0)],
        ),
    ]);
    for _ in 0..120 * 10 {
        g.steps(1);
        if g.s.vitals()[&g.human].health < 100.0 {
            break;
        }
    }
    assert!(
        g.at().distance(HOME) > 9.0,
        "active pursuit crosses the home range"
    );
    assert_eq!(
        g.s.vitals()[&g.human].health,
        95.0,
        "the configured attack landed"
    );
    g.edit_and_fire(vec![row(
        "onActivate",
        Slot::SelfBrick,
        "setBotPowered",
        vec![Value::Bool(false)],
    )]);
    let health = g.s.vitals()[&g.human].health;
    g.steps(240);
    assert_eq!(
        g.s.vitals()[&g.human].health,
        health,
        "paused bot must release its attack"
    );
    let away = g.at();
    g.edit_and_fire(vec![
        row(
            "onActivate",
            Slot::SelfBrick,
            "setBotSightRadius",
            vec![Value::Float(0.0)],
        ),
        row(
            "onActivate",
            Slot::SelfBrick,
            "setBotPowered",
            vec![Value::Bool(true)],
        ),
    ]);
    g.steps(3 * 120);
    assert!(
        g.at().distance(away) < 1.0,
        "without a fight, wandering or a home preference it stays where it is"
    );
    assert!(g.at().distance(HOME) > 9.0);
    g.edit_and_fire(vec![row(
        "onActivate",
        Slot::SelfBrick,
        "setBotReturnHome",
        vec![Value::Bool(true)],
    )]);
    g.steps(8 * 120);
    assert!(
        g.at().distance(HOME) < 3.0,
        "restoring the idle home preference sends it back through ordinary movement"
    );
    g.edit_and_fire(vec![row(
        "onActivate",
        Slot::SelfBrick,
        "setBotSightRadius",
        vec![Value::Float(32.0)],
    )]);
    for _ in 0..120 * 10 {
        g.steps(1);
        if g.s.vitals()[&g.human].health < health {
            break;
        }
    }
    assert!(
        g.s.vitals()[&g.human].health < health,
        "restored perception reacquires and attacks again"
    );
}

#[test]
fn spawn_and_death_events_configure_each_life_and_can_schedule_a_wave_respawn() {
    let mut revive = row("onBotDeath", Slot::SelfBrick, "respawnBot", vec![]);
    revive.delay_ms = 10_000;
    let mut g = Game::new(vec![
        row(
            "onBotSpawn",
            Slot::SelfBrick,
            "setColor",
            vec![Value::Color(1)],
        ),
        row(
            "onBotSpawn",
            Slot::SelfBrick,
            "setBotPowered",
            vec![Value::Bool(false)],
        ),
        row("onBotSpawn", Slot::Bot, "setHealth", vec![Value::Int(30)]),
        row(
            "onBotSpawn",
            Slot::SelfBrick,
            "setBotAutoRespawn",
            vec![Value::Bool(false)],
        ),
        row(
            "onBotDeath",
            Slot::SelfBrick,
            "setColor",
            vec![Value::Color(0)],
        ),
        row("fixtureDeath", Slot::Bot, "kill", vec![]),
        revive,
    ]);
    assert_eq!(g.s.vitals()[&g.bot].health, 30.0);
    let color = |g: &Game| {
        let w = g.s.simulation().state();
        w.palette[w.bricks[&1].color as usize]
    };
    assert_eq!(color(&g), [0.0, 1.0, 0.0, 1.0]);
    let born = g.s.spawn_tick(g.bot).unwrap();
    g.s.fire_brick_input(1, "fixtureDeath", Some(g.bot));
    // A lifecycle input caused inside an event output uses the existing
    // bounded deferred-input phase; its Self rows run on the next tick.
    g.steps(2);
    assert!(!g.s.is_alive(g.bot));
    assert_eq!(color(&g), [1.0; 4]);
    g.steps(8 * 120);
    assert!(
        !g.s.is_alive(g.bot),
        "automatic respawn must not preempt the authored wave timer"
    );
    g.steps(3 * 120);
    assert!(g.s.is_alive(g.bot), "the death event's delayed respawn ran");
    assert!(g.s.spawn_tick(g.bot).unwrap() > born);
    assert_eq!(
        g.s.vitals()[&g.bot].health,
        30.0,
        "spawn policy reapplied to the new body"
    );
    assert_eq!(color(&g), [0.0, 1.0, 0.0, 1.0]);
    assert_eq!(g.s.names().keys().filter(|id| g.s.is_bot(**id)).count(), 1);
    assert!(g.s.take_event_diagnostics().is_empty());
}

fn infection_encounter(health: i64) -> (Game, u64, u64) {
    let mut g = Game::new(vec![row(
        "onBotSpawn",
        Slot::SelfBrick,
        "setBotPowered",
        vec![Value::Bool(false)],
    )]);
    let mut attacker = BotPack::from_json(include_bytes!(
        "../../../packages/blockhead_bot/assets/bots.json"
    ))
    .unwrap()
    .bots
    .remove(0);
    attacker.side = Some("infectors".into());
    attacker.sight = 32.0;
    attacker.weapons = false;
    attacker.reaction_seconds = 0.0;
    attacker.alerts_allies = false;
    for name in ["interact", "explore", "fly"] {
        attacker.behaviours.insert(name.into(), 0.0);
    }
    attacker.melee = Some(BotMelee {
        damage: 15.0,
        reach: 2.5,
        seconds: 1.0,
        action: None,
        name: "Fixture infection bite".into(),
        converts_below: Some(0.5),
    });
    let mut victim = attacker.clone();
    victim.id = "fixture.survivor".into();
    victim.name = "Survivor".into();
    victim.side = Some("survivors".into());
    victim.melee = None;
    g.s.set_bot_kinds(vec![attacker, victim]).unwrap();
    g.s.set_tool_catalog(ToolCatalog {
        vehicles: [fixture::BOT.to_string(), "fixture.survivor".into()].into(),
        vehicle_bricks: [fixture::PLATE.to_string()].into(),
        ..Default::default()
    })
    .unwrap();
    g.seq += 1;
    let bri_sim::session::Reply::Planted(brick) =
        g.s.command(
            g.human,
            g.seq,
            Command::Plant {
                definition: fixture::PLATE.into(),
                position: [-39.75, 0.1, 34.25],
                quarter_turns: 0,
                color: 0,
            },
        )
        .unwrap()
    else {
        panic!("survivor pad did not plant")
    };
    g.s.edit_brick(
        g.human,
        brick,
        Edit::Events(vec![
            row(
                "onBotSpawn",
                Slot::SelfBrick,
                "setBotPowered",
                vec![Value::Bool(false)],
            ),
            row(
                "onBotSpawn",
                Slot::Bot,
                "setHealth",
                vec![Value::Int(health)],
            ),
        ]),
    )
    .unwrap();
    g.s.edit_brick(
        g.human,
        brick,
        Edit::Properties(bri_world::authority::WrenchProperties {
            vehicle: Some("fixture.survivor".into()),
            raycast: true,
            colliding: true,
            visible: true,
            ..Default::default()
        }),
    )
    .unwrap();
    g.steps(610);
    let survivor =
        *g.s.names()
            .keys()
            .find(|id| g.s.is_bot(**id) && **id != g.bot)
            .unwrap();
    assert_eq!(g.s.vitals()[&survivor].health, health as f32);
    let born = g.s.spawn_tick(survivor).unwrap();
    g.edit_and_fire(vec![row(
        "onActivate",
        Slot::SelfBrick,
        "setBotPowered",
        vec![Value::Bool(true)],
    )]);
    for _ in 0..120 * 8 {
        g.steps(1);
        if g.s.vitals()[&survivor].health != health as f32 {
            return (g, survivor, born);
        }
    }
    panic!("the encounter never reached its first contact");
}

#[test]
fn infection_waits_until_the_victim_was_already_at_half_health_before_contact() {
    let (mut g, victim, born) = infection_encounter(61);
    assert_eq!(
        g.s.vitals()[&victim].health,
        46.0,
        "crossing half health on this bite must not convert early"
    );
    for _ in 0..120 * 3 {
        g.steps(1);
        if g.s.vitals()[&victim].health == 100.0 {
            break;
        }
    }
    assert!(g.s.is_alive(victim));
    assert_eq!(
        g.s.vitals()[&victim].health,
        100.0,
        "the next eligible contact converts and restores health"
    );
    assert_eq!(
        g.s.spawn_tick(victim),
        Some(born),
        "conversion did not kill and respawn it"
    );
}

#[test]
fn infection_converts_a_critically_hurt_bot_before_a_lethal_bite_and_at_the_boundary() {
    for health in [10, 50] {
        let (mut g, victim, born) = infection_encounter(health);
        assert!(
            g.s.is_alive(victim),
            "eligible contact converts before lethal damage"
        );
        assert_eq!(g.s.vitals()[&victim].health, 100.0);
        assert_eq!(g.s.spawn_tick(victim), Some(born));
        g.steps(240);
        assert_eq!(
            g.s.vitals()[&victim].health,
            100.0,
            "the converted bot is now on the attacker's side"
        );
    }
}

#[test]
fn an_infecting_attack_still_damages_and_kills_a_human_normally() {
    let (mut g, _, _) = infection_encounter(10);
    g.edit_and_fire(vec![
        row(
            "onActivate",
            Slot::SelfBrick,
            "setBotPowered",
            vec![Value::Bool(false)],
        ),
        row(
            "onActivate",
            Slot::Player,
            "setHealth",
            vec![Value::Int(10)],
        ),
    ]);
    assert_eq!(g.s.vitals()[&g.human].health, 10.0);
    g.edit_and_fire(vec![row(
        "onActivate",
        Slot::SelfBrick,
        "setBotPowered",
        vec![Value::Bool(true)],
    )]);
    for _ in 0..120 * 12 {
        g.steps(1);
        if !g.s.is_alive(g.human) {
            break;
        }
    }
    assert!(
        !g.s.is_alive(g.human),
        "human targets are damaged, never converted by this bot policy"
    );
    assert_eq!(g.s.vitals()[&g.human].health, 0.0);
}

#[test]
fn a_spawn_event_cannot_loop_respawning_its_own_bot_in_one_tick() {
    let mut g = Game::new(vec![row(
        "onBotSpawn",
        Slot::SelfBrick,
        "setBotPowered",
        vec![Value::Bool(false)],
    )]);
    g.edit_and_fire(vec![
        row("onActivate", Slot::SelfBrick, "respawnBot", vec![]),
        row("onBotSpawn", Slot::SelfBrick, "respawnBot", vec![]),
    ]);
    let born = g.s.spawn_tick(g.bot).unwrap();
    g.steps(3);
    assert_eq!(g.s.spawn_tick(g.bot), Some(born));
    assert_eq!(g.s.names().keys().filter(|id| g.s.is_bot(**id)).count(), 1);
    assert_eq!(g.s.last_event_work().pending, 0);
    let diagnostics = g.s.take_event_diagnostics();
    assert!(
        diagnostics.iter().any(|d| d.contains("respawn cooldown")),
        "{diagnostics:?}"
    );
}

#[test]
fn a_deferred_death_callback_keeps_the_dead_body_not_its_immediate_replacement() {
    let mut g = Game::new(vec![row(
        "onBotSpawn",
        Slot::SelfBrick,
        "setBotPowered",
        vec![Value::Bool(false)],
    )]);
    let mut late = row("onBotDeath", Slot::Bot, "kill", vec![]);
    // Run after the replacement's ordinary spawn shield expires.
    late.delay_ms = 4000;
    g.edit_and_fire(vec![
        row(
            "onBotSpawn",
            Slot::SelfBrick,
            "setBotPowered",
            vec![Value::Bool(false)],
        ),
        row(
            "onBotSpawn",
            Slot::SelfBrick,
            "setBotAutoRespawn",
            vec![Value::Bool(false)],
        ),
        row("fixtureDeath", Slot::Bot, "kill", vec![]),
        row("fixtureDeath", Slot::SelfBrick, "respawnBot", vec![]),
        late,
    ]);
    let before = g.s.spawn_tick(g.bot).unwrap();
    g.s.fire_brick_input(1, "fixtureDeath", Some(g.bot));
    g.steps(2);
    assert!(g.s.is_alive(g.bot));
    let replacement = g.s.spawn_tick(g.bot).unwrap();
    assert!(replacement > before);
    g.steps(5 * 120);
    assert!(
        g.s.is_alive(g.bot),
        "the old death's Bot target must never bind to a new life"
    );
    assert_eq!(g.s.spawn_tick(g.bot), Some(replacement));
}
