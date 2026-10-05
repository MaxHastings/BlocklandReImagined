//! Ordinary authorized Add-On rest/resume must not consume objective work or
//! monopolize the shared planner. No private brain/input/position injection.
use bri_chaos::fixture;
use bri_events::rules::{Compare, Condition, Datum, Property, Subject};
use bri_events::{Row, Slot, Target, Value};
use bri_sim::player::MoveInput;
use bri_sim::session::{
    Command, MiniGameRequest, PackageArg, PackageCommand, Session, ToolCatalog,
};
use bri_world::{Brick, ContentRef, VehicleSpawn, World, build::SavedBuild};
use glam::Vec3;
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn catalog() -> Arc<bri_package_runtime::Catalog> {
    use bri_package::packages::{PackageEntry, PackageSet, Side};
    struct Temp(std::path::PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = Temp(std::env::temp_dir().join(format!(
        "bri-objective-rest-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    let dir = root.0.join("objective-rest");
    let data = root.0.join("objective-rest-kind");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(
        data.join("package.json"),
        json!({
            "schema_version":1,"id":"objective-rest-kind","version":"1.0.0","api":1,
            "name":"Recovery controller data","license":"CC0-1.0","companions":["objective-rest"],
            "provides":[{"kind":"bots","id":"objective-rest-kind:bots/main","file":"bots.json"}]
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(dir.join("package.json"), json!({
        "schema_version":1,"id":"objective-rest","version":"1.0.0","api":1,
        "name":"Objective rest fixture","license":"CC0-1.0","capabilities":["bots","player","minigame"],
        "provides":[{"kind":"behaviour","id":"objective-rest:behaviour/main","file":"behaviour.json"},
                    {"kind":"script","id":"objective-rest:script/main","file":"main.rhai"}]
    }).to_string()).unwrap();
    std::fs::write(
        data.join("bots.json"),
        serde_json::to_vec(&kind_pack()).unwrap(),
    )
    .unwrap();
    std::fs::write(dir.join("behaviour.json"), json!({"schema_version":1,"script":"main.rhai","commands":[{"name":"rest","args":["int","bool"]}]}).to_string()).unwrap();
    std::fs::write(
        dir.join("main.rhai"),
        r#"
fn cmd_rest(p, b, on) { rest_bot(b, on); }
"#,
    )
    .unwrap();
    Arc::new(
        bri_package_runtime::Catalog::load(
            &root.0,
            &PackageSet {
                schema_version: 1,
                packages: vec![
                    PackageEntry {
                        id: "objective-rest-kind".into(),
                        version: "1.0.0".into(),
                        side: Side::Shared,
                        dir: "objective-rest-kind".into(),
                        role: None,
                    },
                    PackageEntry {
                        id: "objective-rest".into(),
                        version: "1.0.0".into(),
                        side: Side::Server,
                        dir: "objective-rest".into(),
                        role: None,
                    },
                ],
            },
            true,
        )
        .unwrap(),
    )
}
const KIND: &str = "objective-rest-kind:bot/recovery";
fn kind_pack() -> bri_sim::bot_kind::BotPack {
    let mut pack = bri_sim::bot_kind::BotPack::from_json(include_bytes!(
        "../../../packages/blockhead_bot/assets/bots.json"
    ))
    .unwrap();
    pack.bots[0].id = KIND.into();
    pack.bots[0].name = "Recovery controller".into();
    pack
}
fn row(
    input: &str,
    output: &str,
    target: Slot,
    params: Vec<Value>,
    conditions: Vec<Condition>,
) -> Row {
    Row {
        enabled: true,
        input: input.into(),
        output: output.into(),
        target: Target::Slot(target),
        params,
        conditions,
        delay_ms: 0,
        preserved: None,
    }
}
struct Game {
    s: Session,
    owner: u64,
    movement: u64,
    command: u64,
}
impl Game {
    fn new(impossible: bool) -> Self {
        let mut s = fixture::synthetic().unwrap().session;
        s.set_bot_kinds(kind_pack().bots).unwrap();
        s.set_tool_catalog(ToolCatalog {
            vehicles: [KIND.into()].into(),
            vehicle_bricks: [fixture::PLATE.into()].into(),
            ..Default::default()
        })
        .unwrap();
        s.set_event_catalog(bri_events::testing::catalog(), Vec::<String>::new())
            .unwrap();
        s.install_packages(catalog(), None).unwrap();
        s.set_spawn_points(vec![Vec3::new(-40.0, 0.05, -40.0)])
            .unwrap();
        let owner = s
            .join("Author".into(), Vec3::new(-40.0, 0.05, -40.0), true)
            .unwrap();
        let mut world = World::new(
            "Suspended objective".into(),
            "chaos/map".into(),
            vec![[1.0; 4]; 8],
        );
        let mut b = Brick::new(
            ContentRef::Resolved(fixture::PLATE.into()),
            [0.25, 0.1, 55.25],
            owner,
        );
        if !impossible {
            b.events.push(row(
                "onActivate",
                "addPlayerScore",
                Slot::Player,
                vec![Value::Int(29)],
                vec![],
            ));
        }
        b.events.push(row(
            "onActivate",
            "winRound",
            Slot::Player,
            vec![],
            if impossible {
                vec![Condition {
                    subject: Subject::Player,
                    property: Property::Variable,
                    key: "unprovided".into(),
                    compare: Compare::Equal,
                    value: Datum::Number(1),
                }]
            } else {
                vec![]
            },
        ));
        b.events.push(row(
            "onRuleRoundEnd",
            "setColorFX",
            Slot::SelfBrick,
            vec![Value::Int(1)],
            vec![],
        ));
        world.bricks.insert(1, b);
        world.next_brick_id = 2;
        s.command(
            owner,
            100,
            Command::LoadBuild {
                build: Box::new(SavedBuild::new(world)),
                ownership: false,
            },
        )
        .unwrap();
        s.command(
            owner,
            101,
            Command::MiniGame(MiniGameRequest::Create {
                color: 0,
                settings: bri_minigames::Settings {
                    loadout: [None, None, None, None, None],
                    ..Default::default()
                },
            }),
        )
        .unwrap();
        s.set_spawn_points(vec![Vec3::new(0.25, 0.05, 20.25)])
            .unwrap();
        let mut game = Self {
            s,
            owner,
            movement: 1 << 40,
            command: 101,
        };
        // LoadBuild is an ordinary scheduled operation, not an immediate
        // world assignment. Finish this tiny initial build before adding one.
        game.steps(40);
        game
    }
    fn steps(&mut self, count: usize) {
        for _ in 0..count {
            self.movement += 1;
            self.s
                .movement(self.owner, self.movement, MoveInput::default())
                .unwrap();
            self.s.step().unwrap();
        }
    }
    fn command(&mut self, command: &str, args: Vec<PackageArg>) {
        self.command += 1;
        self.s
            .command(
                self.owner,
                self.command,
                Command::Package(PackageCommand {
                    package: "objective-rest".into(),
                    command: command.into(),
                    args,
                }),
            )
            .unwrap();
        self.steps(2);
    }
    fn add(&mut self) -> u64 {
        let count = self.s.names().keys().filter(|o| self.s.is_bot(**o)).count();
        let mut world = World::new(
            "Another controller".into(),
            "chaos/map".into(),
            vec![[1.0; 4]],
        );
        let mut spawn = Brick::new(
            ContentRef::Resolved(fixture::PLATE.into()),
            [0.25 + count as f32 * 5.0, 0.1, 20.25],
            self.owner,
        );
        spawn.vehicle = Some(Box::new(VehicleSpawn {
            vehicle: ContentRef::Resolved(KIND.into()),
            recolor: false,
            team: None,
        }));
        world.bricks.insert(1, spawn);
        world.next_brick_id = 2;
        self.command += 1;
        self.s
            .command(
                self.owner,
                self.command,
                Command::LoadBuild {
                    build: Box::new(SavedBuild::new(world)),
                    ownership: false,
                },
            )
            .unwrap();
        self.steps(40);
        *self
            .s
            .names()
            .keys()
            .filter(|o| self.s.is_bot(**o))
            .max()
            .unwrap()
    }
    fn rest(&mut self, bot: u64, on: bool) {
        self.command(
            "rest",
            vec![PackageArg::Int(bot as i64), PackageArg::Bool(on)],
        );
    }
}

#[test]
fn a_resting_failed_model_does_not_starve_other_ready_controllers() {
    let mut g = Game::new(true);
    let first = g.add();
    g.steps(120);
    assert_eq!(
        g.s.bot_thoughts()
            .iter()
            .find(|t| t.bot == first)
            .unwrap()
            .objective_searches,
        1
    );
    g.rest(first, true);
    let second = g.add();
    g.steps(120 * 5);
    let thoughts = g.s.bot_thoughts();
    let active = thoughts.iter().find(|t| t.bot == second).unwrap();
    assert!(
        active.objective_reused >= 3,
        "resting earlier owner monopolized the global turn: {thoughts:?}"
    );
    assert_eq!(
        thoughts
            .iter()
            .find(|t| t.bot == first)
            .unwrap()
            .objective_searches,
        1
    );
    assert!(g.s.package_diagnostics().is_empty());
}

#[test]
fn authorized_long_rest_preserves_the_approach_and_resumes_to_a_real_round_end() {
    let mut g = Game::new(false);
    let bot = g.add();
    for _ in 0..240 {
        if g.s.bot_thoughts()[0].objective.is_some() {
            break;
        }
        g.steps(1);
    }
    assert!(g.s.bot_thoughts()[0].objective.is_some());
    g.rest(bot, true);
    g.steps(120 * 34);
    assert_eq!(g.s.vitals()[&bot].score, 0);
    g.rest(bot, false);
    for _ in 0..120 * 40 {
        if g.s.vitals()[&bot].score == 29 {
            break;
        }
        g.steps(1);
    }
    g.steps(5);
    assert_eq!(
        g.s.vitals()[&bot].score,
        29,
        "long pause falsely cooled down an unchanged approach: {:?}",
        g.s.bot_thoughts()
    );
    assert!(
        g.s.simulation()
            .state()
            .bricks
            .values()
            .any(|b| b.color_effect == 1),
        "canonical onRuleRoundEnd observer did not execute"
    );
    assert!(g.s.package_diagnostics().is_empty());
    assert!(g.s.take_event_diagnostics().is_empty());
}
