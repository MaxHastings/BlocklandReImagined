//! Coordination through published intents (`docs/architecture/bots.md`,
//! "Coordination"), checked through the authoritative session: a bot reads
//! its allies' current intents, and stops reading one the moment that ally
//! dies or changes sides. No test steers a bot or edits an intent.
use bri_chaos::fixture;
use bri_sim::player::MoveInput;
use bri_sim::session::{Command, MiniGameRequest, Session, TeamEdit, ToolCatalog};
use bri_world::{Brick, ContentRef, OwnerId, VehicleSpawn, World, build::SavedBuild};
use glam::Vec3;
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

/// A server Add-On with nothing to do with bots or teams.
fn addon() -> Arc<bri_package_runtime::Catalog> {
    use bri_package::packages::{PackageEntry, PackageSet, Side};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "bri-bot-team-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let dir = root.join("crew");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("package.json"), json!({"schema_version":1,"id":"crew","version":"1.0.0","api":1,"name":"Crew fixture","license":"CC0-1.0","capabilities":["minigame"],"provides":[{"kind":"behaviour","id":"crew:behaviour/main","file":"behaviour.json"},{"kind":"script","id":"crew:script/main","file":"main.rhai"}]}).to_string()).unwrap();
    std::fs::write(
        dir.join("behaviour.json"),
        json!({"schema_version":1,"script":"main.rhai","settings":[]}).to_string(),
    )
    .unwrap();
    std::fs::write(dir.join("main.rhai"), "fn unused() { }\n").unwrap();
    let catalog = bri_package_runtime::Catalog::load(
        &root,
        &PackageSet {
            schema_version: 1,
            packages: vec![PackageEntry {
                id: "crew".into(),
                version: "1.0.0".into(),
                side: Side::Server,
                dir: "crew".into(),
                role: None,
            }],
        },
        true,
    )
    .unwrap();
    let _ = std::fs::remove_dir_all(&root);
    Arc::new(catalog)
}

struct Field {
    s: Session,
    author: OwnerId,
    seq: u64,
    command: u64,
}

impl Field {
    /// Three Blockhead Bot pads, a mini-game with two teams, every bot on
    /// the first team.
    fn new() -> Self {
        let mut f = Self::teamless();
        let game = f.game();
        f.mg(MiniGameRequest::AddOnSettings {
            game,
            settings: vec![],
            teams: Some(
                [("Blue", 0), ("Red", 1)]
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
        });
        let blue = f.team(0);
        for bot in f.bots() {
            f.set_team(bot, blue);
        }
        f.steps(10);
        f
    }
    /// The same pads, one builder's, in a mini-game without teams.
    fn teamless() -> Self {
        let mut s = fixture::synthetic().unwrap().session;
        s.set_vehicle_pack(
            bri_vehicles::testing::pack(),
            bri_sim::bot_kind::BotPack::from_json(include_bytes!(
                "../../../packages/blockhead_bot/assets/bots.json"
            ))
            .unwrap()
            .bots,
        )
        .unwrap();
        s.set_tool_catalog(ToolCatalog {
            vehicles: [fixture::BOT.to_string()].into(),
            vehicle_bricks: [fixture::PLATE.into()].into(),
            ..Default::default()
        })
        .unwrap();
        s.install_packages(addon(), None).unwrap();
        let at = Vec3::new(-12.0, 0.05, 43.0);
        s.set_spawn_points(vec![at]).unwrap();
        let author = s.join("Max".into(), at, true).unwrap();
        let mut f = Self {
            s,
            author,
            seq: 1 << 40,
            command: 100,
        };
        let mut world = World::new(
            "Crew".into(),
            "chaos/map".into(),
            f.s.simulation().state().palette.clone(),
        );
        for (id, x) in [(1, -4.75), (2, 0.25), (3, 5.25)] {
            let mut b = Brick::new(
                ContentRef::Resolved(fixture::PLATE.into()),
                [x, 0.1, 40.25],
                author,
            );
            b.vehicle = Some(Box::new(VehicleSpawn {
                vehicle: ContentRef::Resolved(fixture::BOT.into()),
                recolor: false,
                team: None,
            }));
            world.bricks.insert(id, b);
        }
        world.next_brick_id = 4;
        f.run(Command::LoadBuild {
            build: Box::new(SavedBuild::new(world)),
            ownership: false,
        });
        f.steps(20);
        f.mg(MiniGameRequest::Create {
            color: 0,
            settings: bri_minigames::Settings {
                loadout: [None, None, None, None, None],
                ..Default::default()
            },
        });
        f.steps(20);
        f
    }
    fn run(&mut self, command: Command) {
        self.command += 1;
        self.s.command(self.author, self.command, command).unwrap();
    }
    fn mg(&mut self, request: MiniGameRequest) {
        self.run(Command::MiniGame(request));
    }
    fn steps(&mut self, n: usize) {
        for _ in 0..n {
            self.seq += 1;
            self.s
                .movement(self.author, self.seq, MoveInput::default())
                .unwrap();
            self.s.step().unwrap();
        }
    }
    fn game(&self) -> u64 {
        self.s.minigame_views()[0].id
    }
    fn team(&self, index: usize) -> u32 {
        self.s.minigame_views()[0].teams[index].id.0
    }
    fn set_team(&mut self, bot: OwnerId, team: u32) {
        let game = self.game();
        self.mg(MiniGameRequest::SetTeam {
            game,
            target: bot,
            team: Some(team),
        });
    }
    fn bots(&self) -> Vec<OwnerId> {
        let mut bots: Vec<_> = self
            .s
            .names()
            .keys()
            .copied()
            .filter(|p| self.s.is_bot(*p))
            .collect();
        bots.sort();
        assert_eq!(bots.len(), 3, "every pad has its bot");
        bots
    }
    /// How many allies' intents each bot read on its last choice.
    fn read(&self) -> Vec<(OwnerId, usize)> {
        let mut read: Vec<_> = self
            .s
            .bot_thoughts()
            .into_iter()
            .map(|t| (t.bot, t.team.allies))
            .collect();
        read.sort();
        read
    }
}

#[test]
fn a_saved_spawn_event_assigns_the_initial_bot_to_a_team() {
    use bri_events::{Slot, Value};
    use bri_world::{EventRow, EventTarget};
    let mut f = Field::new();
    f.s.set_event_catalog(bri_events::testing::catalog_extended(), Vec::new())
        .unwrap();
    let red = f.team(1);
    let existing = f.bots();
    let mut world = World::new(
        "New spawn".into(),
        "chaos/map".into(),
        f.s.simulation().state().palette.clone(),
    );
    let mut brick = Brick::new(
        ContentRef::Resolved(fixture::PLATE.into()),
        [15.25, 0.1, 40.25],
        f.author,
    );
    brick.vehicle = Some(Box::new(VehicleSpawn {
        vehicle: ContentRef::Resolved(fixture::BOT.into()),
        recolor: false,
        team: None,
    }));
    brick.events = vec![EventRow {
        conditions: vec![],
        preserved: None,
        enabled: true,
        input: "onBotSpawn".into(),
        delay_ms: 0,
        target: EventTarget::Slot(Slot::Bot),
        output: "setTeam".into(),
        params: vec![Value::Int(i64::from(red))],
    }];
    world.bricks.insert(1, brick);
    world.next_brick_id = 2;
    let saved = SavedBuild::new(world);
    let saved = bri_world::build::decode(&bri_world::build::encode(&saved).unwrap()).unwrap();
    f.run(Command::LoadBuild {
        build: Box::new(saved),
        ownership: false,
    });
    f.steps(150);
    let added: Vec<_> =
        f.s.names()
            .keys()
            .copied()
            .filter(|bot| f.s.is_bot(*bot))
            .filter(|bot| !existing.contains(bot))
            .collect();
    assert_eq!(added.len(), 1, "exactly one bot should be added");
    let diagnostics = f.s.take_event_diagnostics();
    assert_eq!(f.s.vitals()[&added[0]].team, Some(red), "{diagnostics:?}");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn a_spawn_event_assigns_its_bot_to_a_minigame_team() {
    use bri_events::{Slot, Value};
    use bri_world::{EventRow, EventTarget, authority::Edit};
    let mut f = Field::new();
    f.s.set_event_catalog(bri_events::testing::catalog_extended(), Vec::new())
        .unwrap();
    let bot = f.bots()[0];
    let born = f.s.spawn_tick(bot).unwrap();
    let red = f.team(1);
    let row = |input: &str, target, output: &str, params| EventRow {
        conditions: vec![],
        preserved: None,
        enabled: true,
        input: input.into(),
        delay_ms: 0,
        target: EventTarget::Slot(target),
        output: output.into(),
        params,
    };
    f.s.edit_brick(
        f.author,
        1,
        Edit::Events(vec![
            row(
                "onBotSpawn",
                Slot::Bot,
                "setTeam",
                vec![Value::Int(i64::from(red))],
            ),
            row("onActivate", Slot::SelfBrick, "respawnBot", vec![]),
        ]),
    )
    .unwrap();
    f.steps(30);
    f.s.fire_brick_input(1, "onActivate", Some(f.author));
    f.steps(120);
    let diagnostics = f.s.take_event_diagnostics();
    assert!(
        f.s.spawn_tick(bot).unwrap() > born,
        "the first pad respawned its bot; bots {:?}, diagnostics {diagnostics:?}",
        f.bots()
            .iter()
            .map(|id| (*id, f.s.spawn_tick(*id)))
            .collect::<Vec<_>>()
    );
    assert_eq!(f.s.vitals()[&bot].team, Some(red), "{diagnostics:?}");
    assert!(diagnostics.is_empty());
}

#[test]
fn a_bot_reads_its_allies_intents_and_drops_a_dead_or_departed_ones() {
    let mut f = Field::new();
    let [a, b, c] = f.bots()[..] else {
        unreachable!()
    };
    assert_eq!(f.read(), vec![(a, 2), (b, 2), (c, 2)], "three allies");
    // One changes sides: its old allies no longer read it, nor it them.
    let red = f.team(1);
    f.set_team(c, red);
    f.steps(4);
    assert_eq!(f.read(), vec![(a, 1), (b, 1), (c, 0)], "after the switch");
    // One dies: no intent of it is read while it is dead.
    f.s.command(b, 1 << 50, Command::Suicide).unwrap();
    f.steps(4);
    assert!(!f.s.vitals()[&b].alive, "the bot died");
    assert_eq!(f.read()[0], (a, 0), "nothing read from a dead ally");
    // The game ends: nobody on a side, and nothing crashes.
    let game = f.game();
    f.mg(MiniGameRequest::Manage {
        game,
        request: Box::new(MiniGameRequest::End),
    });
    f.steps(240);
}
