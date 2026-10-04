//! Max's soccer setup through ordinary commands: two teams, one ball, a goal
//! for each team (a detection region whose `onObjectEnter` row wins the
//! round for the last mover when they play for the other team) and two
//! spawn-brick bots put on opposite teams in the MiniGame window. Each
//! team, each bot's side and each goal's team guard must hold across Save &
//! Reset and across saving and loading the build.
use bri_chaos::fixture;
use bri_events::rules::{Compare, Condition, Datum, Property, Subject};
use bri_events::{Row, Slot, Target};
use bri_sim::player::MoveInput;
use bri_sim::session::{Command, MiniGameRequest, Reply, Session, TeamEdit, ToolCatalog};
use bri_world::{
    Brick, ContentRef, OwnerId, VehicleSpawn, World, authority::Edit, build::SavedBuild,
};
use glam::Vec3;
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

const BALL: &str = "pitch:vehicle/steel_ball";
const BALL_SPAWN: &str = "match_ball";

/// A server Add-On with nothing to do with teams, or one declaring a team
/// setting (as Slayer does): the team mechanism must not depend on it.
fn addon(team_setting: bool) -> Arc<bri_package_runtime::Catalog> {
    use bri_package::packages::{PackageEntry, PackageSet, Side};
    struct Temp(std::path::PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = Temp(std::env::temp_dir().join(format!(
        "bri-soccer-teams-{}-{}",
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

struct Pitch {
    s: Session,
    author: OwnerId,
    seq: u64,
    command: u64,
}

impl Pitch {
    fn new(team_setting: bool, bricks: bool) -> Self {
        let mut s = fixture::synthetic().unwrap().session;
        let mut vehicles = bri_vehicles::testing::pack();
        let mut ball = bri_vehicles::testing::definition(bri_vehicles::testing::BALL);
        ball.id = BALL.into();
        ball.name = "Steel Ball".into();
        vehicles.definitions.push(ball);
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
            vehicles: [fixture::BOT.to_string(), BALL.to_string()].into(),
            vehicle_bricks: [fixture::PLATE.into()].into(),
            ..Default::default()
        })
        .unwrap();
        s.install_packages(addon(team_setting), None).unwrap();
        let at = Vec3::new(-12.0, 0.05, 43.0);
        s.set_spawn_points(vec![at]).unwrap();
        let author = s.join("Max".into(), at, true).unwrap();
        let mut p = Self {
            s,
            author,
            seq: 1 << 40,
            command: 100,
        };
        if bricks {
            let mut world = World::new(
                "Soccer".into(),
                "chaos/map".into(),
                p.s.simulation().state().palette.clone(),
            );
            let spawner = |kind: &str, at: [f32; 3], name: &str| {
                let mut b = Brick::new(ContentRef::Resolved(fixture::PLATE.into()), at, author);
                b.name = Some(name.into());
                b.vehicle = Some(Box::new(VehicleSpawn {
                    vehicle: ContentRef::Resolved(kind.into()),
                    recolor: false,
                }));
                b
            };
            world
                .bricks
                .insert(1, spawner(fixture::BOT, [-3.75, 0.1, 40.25], "west_bot"));
            world
                .bricks
                .insert(2, spawner(fixture::BOT, [4.25, 0.1, 40.25], "east_bot"));
            world
                .bricks
                .insert(3, spawner(BALL, [0.25, 0.1, 47.25], BALL_SPAWN));
            for (id, z, name) in [(4, 57.25, "north_goal"), (5, 33.25, "south_goal")] {
                let mut goal = Brick::new(
                    ContentRef::Resolved(fixture::PLATE.into()),
                    [0.25, 0.1, z],
                    author,
                );
                goal.name = Some(name.into());
                goal.colliding = false;
                goal.raycast = false;
                goal.rule_region = Some([3.0, 4.0, 3.0]);
                world.bricks.insert(id, goal);
            }
            world.next_brick_id = 6;
            p.run(Command::LoadBuild {
                build: Box::new(SavedBuild::new(world)),
                ownership: false,
            });
            p.steps(20);
        }
        p
    }
    fn run(&mut self, command: Command) -> Reply {
        self.command += 1;
        self.s.command(self.author, self.command, command).unwrap()
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
    /// The game's teams by name, as the MiniGame window lists them.
    fn teams(&self) -> Vec<(String, u32)> {
        self.s.minigame_views()[0]
            .teams
            .iter()
            .map(|t| (t.name.clone(), t.id.0))
            .collect()
    }
    fn team(&self, name: &str) -> u32 {
        self.teams()
            .into_iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("no team {name}: {:?}", self.teams()))
            .1
    }
    fn brick(&self, name: &str) -> u64 {
        *self
            .s
            .simulation()
            .state()
            .bricks
            .iter()
            .find(|(_, b)| b.name.as_deref() == Some(name))
            .unwrap()
            .0
    }
    /// The bots, west (by its brick) first.
    fn bots(&self) -> [OwnerId; 2] {
        let snapshot = self.s.snapshot();
        let mut bots: Vec<_> = snapshot
            .players
            .iter()
            .filter(|p| self.s.is_bot(p.owner))
            .map(|p| (p.feet[0], p.owner))
            .collect();
        bots.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert_eq!(bots.len(), 2, "both spawn bricks have their bot");
        [bots[0].1, bots[1].1]
    }
    fn team_of(&self, owner: OwnerId) -> Option<u32> {
        self.s.vitals().get(&owner).and_then(|v| v.team)
    }
    /// The MiniGame window's Save: the edited team list (existing teams by
    /// id, new ones without) with `reset` for Save & Reset.
    fn save_teams(&mut self, teams: Vec<(Option<u32>, &str, u8)>, reset: bool) {
        let game = self.game();
        self.mg(MiniGameRequest::AddOnSettings {
            game,
            settings: vec![],
            teams: Some(
                teams
                    .into_iter()
                    .map(|(id, name, color)| TeamEdit {
                        id,
                        name: name.into(),
                        color,
                        settings: vec![],
                    })
                    .collect(),
            ),
            quiet: true,
            reset,
        });
    }
    /// A goal: the ball entering it wins the round for its last mover when
    /// they play for `scoring`.
    fn goal_rows(scoring: u32) -> Vec<Row> {
        vec![Row {
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
                    value: Datum::Text(BALL_SPAWN.into()),
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
        }]
    }
    fn team_guard(&self, goal: &str) -> Option<i64> {
        self.s.simulation().state().bricks[&self.brick(goal)]
            .events
            .iter()
            .flat_map(|r| &r.conditions)
            .find(|c| c.property == Property::Team)
            .and_then(|c| match c.value {
                Datum::Number(n) => Some(n),
                _ => None,
            })
    }
    /// Max's setup: a MiniGame, two teams saved, each bot put on its side
    /// in the Players page, and each goal guarded by the scoring team.
    fn set_up(&mut self) -> [OwnerId; 2] {
        self.mg(MiniGameRequest::Create {
            color: 0,
            settings: bri_minigames::Settings {
                loadout: [None, None, None, None, None],
                brick_damage: false,
                ..Default::default()
            },
        });
        self.steps(20);
        self.save_teams(vec![(None, "Blue", 0), (None, "Red", 1)], false);
        let (blue, red) = (self.team("Blue"), self.team("Red"));
        let [west, east] = self.bots();
        let game = self.game();
        for (bot, team) in [(west, blue), (east, red)] {
            self.mg(MiniGameRequest::SetTeam {
                game,
                target: bot,
                team: Some(team),
            });
        }
        // Blue defends the south goal, so Red scores there, and the reverse.
        for (goal, scoring) in [("south_goal", red), ("north_goal", blue)] {
            let id = self.brick(goal);
            self.s
                .edit_brick(self.author, id, Edit::Events(Self::goal_rows(scoring)))
                .unwrap();
        }
        [west, east]
    }
}

#[test]
fn team_sides_and_goal_guards_survive_save_and_reset() {
    let mut p = Pitch::new(true, true);
    let [west, east] = p.set_up();
    let (blue, red) = (p.team("Blue"), p.team("Red"));
    assert!(
        blue != 0 && red != 0,
        "0 is the No team value of a Team condition: {:?}",
        p.teams()
    );
    assert_eq!(p.team_of(p.author), None, "the author picked no side");
    assert_eq!((p.team_of(west), p.team_of(east)), (Some(blue), Some(red)));
    // Save & Reset with a renamed team, then Reset alone, as the window
    // sends them.
    p.steps(240);
    p.save_teams(vec![(Some(blue), "Blues", 0), (Some(red), "Red", 1)], true);
    p.steps(120);
    assert_eq!(
        p.bots(),
        [west, east],
        "a reset brings each spawn brick's bot back as the same player"
    );
    assert_eq!(
        (p.team_of(west), p.team_of(east)),
        (Some(blue), Some(red)),
        "each bot keeps the side it was put on across Save & Reset"
    );
    for _ in 0..2 {
        p.steps(700);
        let game = p.game();
        p.mg(MiniGameRequest::Manage {
            game,
            request: Box::new(MiniGameRequest::Reset),
        });
        p.steps(60);
        assert_eq!(p.bots(), [west, east]);
        assert_eq!((p.team_of(west), p.team_of(east)), (Some(blue), Some(red)));
    }
    assert_eq!(p.team("Blues"), blue, "a renamed team keeps its id");
    assert_eq!(p.team_guard("south_goal"), Some(i64::from(red)));
    assert_eq!(p.team_guard("north_goal"), Some(i64::from(blue)));
}

#[test]
fn a_saved_build_brings_its_teams_back_in_the_slots_its_rules_name() {
    // No Add-On declares a team setting: teams are the engine's.
    let mut p = Pitch::new(false, true);
    p.set_up();
    // Remove Blue and add Green: Red keeps 2 and Green takes the free 1, so
    // the saved slots are not simply 1, 2 in order.
    let red = p.team("Red");
    p.save_teams(vec![(Some(red), "Red", 1), (None, "Green", 2)], false);
    let green = p.team("Green");
    let id = p.brick("north_goal");
    p.s.edit_brick(p.author, id, Edit::Events(Pitch::goal_rows(green)))
        .unwrap();
    let Reply::Saved(build) = p.run(Command::SaveBuild {
        events: true,
        ownership: false,
    }) else {
        panic!("not saved")
    };
    let saved = p.teams();
    let mut fresh = Pitch::new(false, false);
    fresh.run(Command::LoadBuild {
        build,
        ownership: false,
    });
    fresh.steps(60);
    assert_eq!(
        fresh.teams(),
        saved,
        "the loaded build's game has its teams in their saved slots"
    );
    assert_eq!(fresh.team_guard("north_goal"), Some(i64::from(green)));
    assert_eq!(fresh.team_guard("south_goal"), Some(i64::from(red)));
}

/// After Save & Reset the bots play: the round is won by a bot through the
/// goal its own team scores in (the ball is in that goal's region), never
/// through the goal it defends.
#[test]
fn a_bot_scores_in_its_own_teams_goal_after_save_and_reset() {
    let mut p = Pitch::new(true, true);
    let bots = p.set_up();
    let (blue, red) = (p.team("Blue"), p.team("Red"));
    p.save_teams(vec![(Some(blue), "Blue", 0), (Some(red), "Red", 1)], true);
    let mut trace = Vec::new();
    for _ in 0..120 * 90 {
        p.steps(1);
        for t in p.s.bot_thoughts() {
            let line = format!(
                "bot {} {} {:?} {:?}",
                t.bot,
                t.behaviour,
                t.objective_detail.map(|d| (d.action, d.phase)),
                t.objective_diagnostic
            );
            if !trace.contains(&line) && trace.len() < 40 {
                trace.push(line);
            }
        }
        let Some(r) = p.s.round_results().next() else {
            continue;
        };
        let [winner] = r.owners[..] else {
            panic!("one winner: {:?}", r.owners)
        };
        assert!(bots.contains(&winner), "a bot won: {winner} {trace:#?}");
        let team = p.team_of(winner).expect("the winner plays for a team");
        assert_eq!(r.teams, vec![bri_minigames::TeamId(team)]);
        let goal = if team == red {
            "south_goal"
        } else {
            "north_goal"
        };
        let at = p.s.simulation().state().bricks[&p.brick(goal)].position;
        let ball =
            p.s.vehicle_poses()
                .into_iter()
                .find(|v| {
                    p.s.vehicle_infos()
                        .iter()
                        .any(|i| i.id == v.id && i.definition == BALL)
                })
                .expect("the ball")
                .position;
        assert!(
            (ball[0] - at[0]).abs() <= 2.5 && (ball[2] - at[2]).abs() <= 2.5,
            "team {team} scored with the ball at {ball:?}, not in {goal} at {at:?}: {trace:#?}"
        );
        return;
    }
    panic!(
        "no goal in 90 s: {trace:#?}; poses {:?}",
        p.s.vehicle_poses()
    );
}
