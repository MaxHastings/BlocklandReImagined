//! Independent creator journeys. Setup authors rules, never a bot's solution.
//! All progress uses ordinary movement/activation and canonical rule outcomes.
use bri_chaos::fixture;
use bri_events::rules::{Compare, Condition, Datum, Property, Subject};
use bri_events::{Row, Slot, Target, Value};
use bri_minigames::Settings;
use bri_sim::player::MoveInput;
use bri_sim::session::{Command, MiniGameRequest, Notice, Session, TeamEdit, ToolCatalog};
use bri_world::{
    Brick, ContentRef, OwnerId, VehicleSpawn, World, authority::Edit, build::SavedBuild,
};
use glam::Vec3;
use serde_json::json;
use std::collections::BTreeSet;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Clone, Copy, Debug)]
struct Variant(bool);
impl Variant {
    fn at(self, x: f32, y: f32, z: f32) -> [f32; 3] {
        if self.0 {
            [35.25 + z, y, -35.25 - x]
        } else {
            [-24.75 + x, y, 25.25 + z]
        }
    }
    fn name(self, role: &str) -> String {
        if self.0 {
            format!("linen_{role}_81")
        } else {
            format!("violet_{role}_17")
        }
    }
    fn kind(self) -> &'static str {
        if self.0 {
            "creator-probe:bot/south"
        } else {
            "creator-probe:bot/north"
        }
    }
}

fn event(
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
fn number(property: Property, key: &str, n: i64) -> Condition {
    Condition {
        subject: Subject::Player,
        property,
        key: key.into(),
        compare: Compare::Equal,
        value: Datum::Number(n),
    }
}
fn stage(key: &str, n: i64) -> Condition {
    number(Property::Variable, key, n)
}
fn marker(input: &str, guards: Vec<Condition>, delay: u32) -> Row {
    event(
        input,
        "setColorFX",
        Slot::SelfBrick,
        vec![Value::Int(1)],
        guards,
        delay,
    )
}
fn winning(input: &str, score: i64, guards: Vec<Condition>, delay: u32) -> Vec<Row> {
    vec![
        event(
            input,
            "addPlayerScore",
            Slot::Player,
            vec![Value::Int(score)],
            guards.clone(),
            delay,
        ),
        event(input, "winRound", Slot::Player, vec![], guards, delay),
    ]
}
fn source(v: Variant, x: f32, y: f32, z: f32, name: &str, rows: Vec<Row>, sensor: bool) -> Brick {
    let mut b = Brick::new(
        ContentRef::Resolved(fixture::PLATE.into()),
        v.at(x, y, z),
        0,
    );
    b.name = Some(v.name(name));
    b.events = rows;
    if sensor {
        b.rule_region = Some([3.0, 4.0, 3.0]);
        b.visible = false;
        b.colliding = false;
        b.raycast = false;
    }
    b
}

struct Game {
    s: Session,
    owner: OwnerId,
    bot: OwnerId,
    variant: Variant,
    move_sequence: u64,
    command_sequence: u64,
    initial: Vec3,
    travel: f32,
    reasons: BTreeSet<String>,
    phases: BTreeSet<String>,
}
impl Game {
    fn new(v: Variant, mut bricks: Vec<Brick>, teams: bool) -> Self {
        let mut s = fixture::synthetic().unwrap().session;
        let mut kinds = bri_sim::bot_kind::BotPack::from_json(include_bytes!(
            "../../../packages/blockhead_bot/assets/bots.json"
        ))
        .unwrap()
        .bots;
        kinds[0].id = v.kind().into();
        kinds[0].name = v.name("actor");
        s.set_bot_kinds(kinds).unwrap();
        s.set_event_catalog(bri_events::testing::catalog(), Vec::<String>::new())
            .unwrap();
        s.set_tool_catalog(ToolCatalog {
            vehicles: [v.kind().into()].into(),
            vehicle_bricks: [fixture::PLATE.into()].into(),
            ..Default::default()
        })
        .unwrap();
        if teams {
            s.install_packages(authoring_catalog(v), None).unwrap();
        }
        let at = Vec3::new(-60.0, 0.05, -60.0);
        s.set_spawn_points(vec![at]).unwrap();
        let owner = s.join(v.name("author"), at, true).unwrap();
        // An irrelevant observer is separate from the action's success output.
        bricks.push(source(
            v,
            -15.0,
            0.1,
            -10.0,
            "round_observer",
            vec![marker("onRuleRoundEnd", vec![], 0)],
            false,
        ));
        for i in 0..if v.0 { 9 } else { 3 } {
            let mut b = source(
                v,
                -10.0,
                0.1,
                3.0 + i as f32,
                &format!("decoration_{i}"),
                vec![],
                false,
            );
            b.colliding = false;
            b.raycast = false;
            bricks.push(b);
        }
        let mut spawn = source(v, 0.0, 0.1, 0.0, "actor_source", vec![], false);
        spawn.vehicle = Some(Box::new(VehicleSpawn {
            vehicle: ContentRef::Resolved(v.kind().into()),
            recolor: false,
        }));
        bricks.push(spawn);
        if v.0 {
            bricks.reverse();
        }
        let mut world = World::new(v.name("world"), "chaos/map".into(), vec![[1.0; 4]; 8]);
        for (i, mut b) in bricks.into_iter().enumerate() {
            b.owner = owner;
            world.bricks.insert(i as u64 + 1, b);
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
        let mut move_sequence = 1 << 40;
        for _ in 0..40 {
            move_sequence += 1;
            s.movement(owner, move_sequence, MoveInput::default())
                .unwrap();
            s.step().unwrap();
        }
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
        let bot = s
            .names()
            .keys()
            .copied()
            .find(|o| s.is_bot(*o))
            .expect("actual spawn-brick bot");
        let initial = feet(&s, bot);
        Self {
            s,
            owner,
            bot,
            variant: v,
            move_sequence,
            command_sequence: 101,
            initial,
            travel: 0.0,
            reasons: BTreeSet::new(),
            phases: BTreeSet::new(),
        }
    }
    fn id(&self, role: &str) -> u64 {
        *self
            .s
            .simulation()
            .state()
            .bricks
            .iter()
            .find(|(_, b)| b.name.as_ref() == Some(&self.variant.name(role)))
            .unwrap()
            .0
    }
    fn marked(&self, role: &str) -> bool {
        self.s.simulation().state().bricks[&self.id(role)].color_effect == 1
    }
    fn steps(&mut self, count: usize) {
        for _ in 0..count {
            self.move_sequence += 1;
            self.s
                .movement(self.owner, self.move_sequence, MoveInput::default())
                .unwrap();
            self.s.step().unwrap();
            let at = feet(&self.s, self.bot);
            assert!(at.is_finite(), "native actor became nonfinite");
            self.travel = self.travel.max(at.distance(self.initial));
            for t in self
                .s
                .bot_thoughts()
                .into_iter()
                .filter(|t| t.bot == self.bot)
            {
                if let Some(reason) = t.objective_diagnostic {
                    self.reasons.insert(reason.into());
                }
                if let Some(detail) = t.objective_detail {
                    assert!(!detail.desired.is_empty() && !detail.action.is_empty());
                    assert!(!detail.provider.is_empty() && !detail.phase.is_empty());
                    assert!(!detail.route.is_empty() && detail.route.len() <= 12);
                    assert_eq!(detail.route.first(), Some(&detail.action));
                    self.phases.insert(detail.phase.into());
                }
            }
        }
    }
    fn explain_selected(&mut self) {
        for _ in 0..240 {
            if self
                .s
                .bot_thoughts()
                .iter()
                .any(|t| t.bot == self.bot && t.objective.is_some())
            {
                break;
            }
            self.steps(1);
        }
        let thought = self
            .s
            .bot_thoughts()
            .into_iter()
            .find(|t| t.bot == self.bot)
            .unwrap();
        let source = thought.objective.expect("selected actual action");
        let detail = thought
            .objective_detail
            .expect("derived selected-action detail");
        self.s.take_private_notices();
        self.s.explain_rules(self.owner, source).unwrap();
        let text: Vec<_> = self
            .s
            .take_private_notices()
            .into_iter()
            .filter_map(|(who, n)| match n {
                Notice::Chat(text)
                    if who == self.owner
                        && text.starts_with(&format!("[Events {source}] [NPC ")) =>
                {
                    Some(text)
                }
                _ => None,
            })
            .collect();
        assert!(text.len() <= 4);
        assert!(
            text.iter().any(|t| t.contains(&detail.desired)
                && t.contains(&detail.action)
                && t.contains(detail.provider)
                && t.contains(detail.phase)),
            "Explain omits selected semantics: {text:?}, detail={detail:?}"
        );
    }
    fn acquire(&mut self, role: &str) {
        let id = self.id(role);
        for _ in 0..240 {
            if self
                .s
                .bot_thoughts()
                .iter()
                .any(|t| t.bot == self.bot && t.objective == Some(id))
            {
                return;
            }
            self.steps(1);
        }
        panic!("did not acquire {role}: {:?}", self.s.bot_thoughts());
    }
    fn command(&mut self, request: MiniGameRequest) {
        self.command_sequence += 1;
        self.s
            .command(
                self.owner,
                self.command_sequence,
                Command::MiniGame(request),
            )
            .unwrap();
    }
    fn edit_rows(&mut self, role: &str, rows: Vec<Row>) {
        self.s
            .edit_brick(self.owner, self.id(role), Edit::Events(rows))
            .unwrap();
    }
    fn finish(&mut self, score: i64) {
        for _ in 0..120 * 60 {
            if self.s.vitals()[&self.bot].score == score && self.marked("round_observer") {
                break;
            }
            self.steps(1);
        }
        assert_eq!(
            self.s.vitals()[&self.bot].score,
            score,
            "thoughts={:?} reasons={:?}",
            self.s.bot_thoughts(),
            self.reasons
        );
        assert_eq!(
            self.s.vitals()[&self.owner].score,
            0,
            "author did not perform the actions"
        );
        assert!(
            self.marked("round_observer"),
            "canonical RoundEnded was not admitted"
        );
        assert!(
            self.travel > 4.0,
            "real bot never approached the authored sources"
        );
        self.assert_winner();
        self.steps(600);
        assert_eq!(
            self.s.vitals()[&self.bot].score,
            score,
            "no duplicate success after round end"
        );
        assert!(self.s.take_event_diagnostics().is_empty());
    }
    fn assert_winner(&self) {
        // Root supplies the bounded observer of the existing authoritative
        // RoundEnded effect. Score/ColorFX are not a substitute for identity.
        let game = self.s.vitals()[&self.bot].minigame.unwrap();
        let outcomes: Vec<_> = self.s.round_results().filter(|r| r.game == game).collect();
        assert_eq!(outcomes.len(), 1, "one actual round result");
        assert_eq!(outcomes[0].owners, vec![self.bot]);
        assert_eq!(outcomes[0].players.len(), 1, "one canonical player winner");
        assert_eq!(
            outcomes[0].teams,
            self.s.vitals()[&self.bot]
                .team
                .map(bri_minigames::TeamId)
                .into_iter()
                .collect::<Vec<_>>(),
            "canonical winner is the bot's current team, if any"
        );
    }
    fn assert_no_success(&mut self) {
        assert_eq!(self.s.vitals()[&self.bot].score, 0);
        assert_eq!(self.s.vitals()[&self.owner].score, 0);
        assert!(!self.marked("round_observer"));
        assert_eq!(self.s.round_results().count(), 0);
    }
    fn explain_reason(&mut self, role: &str, word: &str) {
        self.s.take_private_notices();
        let source = self.id(role);
        self.s.explain_rules(self.owner, source).unwrap();
        let summaries: Vec<_> = self
            .s
            .take_private_notices()
            .into_iter()
            .filter_map(|(who, n)| match n {
                Notice::Chat(text)
                    if who == self.owner
                        && text.starts_with(&format!("[Events {source}] [NPC ")) =>
                {
                    Some(text)
                }
                _ => None,
            })
            .collect();
        assert!(summaries.len() <= 4, "bounded existing Explain output");
        assert!(
            summaries
                .iter()
                .any(|text| text.to_ascii_lowercase().contains(word)),
            "missing precise {word} reason: {summaries:?}"
        );
    }
}
fn feet(s: &Session, bot: OwnerId) -> Vec3 {
    Vec3::from(
        s.snapshot()
            .players
            .iter()
            .find(|p| p.owner == bot)
            .unwrap()
            .feet,
    )
}

fn authoring_catalog(v: Variant) -> Arc<bri_package_runtime::Catalog> {
    use bri_package::packages::{PackageEntry, PackageSet, Side};
    struct Temp(std::path::PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = Temp(std::env::temp_dir().join(format!(
        "bri-creator-team-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    let dir = root.0.join("creator-teams");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("package.json"), json!({"schema_version":1,"id":"creator-teams","version":"1.0.0","api":1,"name":"Independent authoring fixture","license":"CC0-1.0","capabilities":["brick_events"],"provides":[{"kind":"behaviour","id":"creator-teams:behaviour/main","file":"behaviour.json"},{"kind":"script","id":"creator-teams:script/main","file":"main.rhai"}]}).to_string()).unwrap();
    std::fs::write(dir.join("behaviour.json"), json!({"schema_version":1,"script":"main.rhai","settings":[{"key":"hint","title":"Team hint","scope":"team","type":"bool","default":false}],"brick_outputs":[{"name":v.name("opaque"),"class":"fxDTSBrick"}],"state":{"global":{"visited":{"default":false,"visible":"everyone"}}}}).to_string()).unwrap();
    std::fs::write(
        dir.join("main.rhai"),
        "fn on_brick_output(output, target, params, info) { set(\"visited\", true); }\n",
    )
    .unwrap();
    Arc::new(
        bri_package_runtime::Catalog::load(
            &root.0,
            &PackageSet {
                schema_version: 1,
                packages: vec![PackageEntry {
                    id: "creator-teams".into(),
                    version: "1.0.0".into(),
                    side: Side::Server,
                    dir: "creator-teams".into(),
                    role: None,
                }],
            },
            true,
        )
        .unwrap(),
    )
}

#[test]
fn ordered_delayed_checkpoints_have_real_scoped_progress_in_metamorphic_worlds() {
    for v in [Variant(false), Variant(true)] {
        let key = v.name("itinerary");
        let mut bricks = Vec::new();
        for i in 0..4 {
            let mut rows = vec![
                event(
                    "onRegionEnter",
                    "setVariable",
                    Slot::SelfBrick,
                    vec![Value::Int(1), Value::Text(key.clone()), Value::Int(i + 1)],
                    vec![stage(&key, i)],
                    150,
                ),
                marker("onRegionEnter", vec![stage(&key, i + 1)], 300),
            ];
            if i == 3 {
                rows.extend(winning("onRegionEnter", 53, vec![stage(&key, 4)], 450));
            }
            // The final checkpoint is nearer than the first. Crossing it out
            // of order must not count; completion needs a later real re-entry.
            let z = [9.0, 21.0, 15.0, 4.0][i as usize];
            bricks.push(source(
                v,
                0.0,
                0.1,
                z,
                &format!("checkpoint_{i}"),
                rows,
                true,
            ));
        }
        let mut decoy = vec![marker("onRegionEnter", vec![stage(&key, 99)], 0)];
        decoy.extend(winning("onRegionEnter", 901, vec![stage(&key, 99)], 0));
        bricks.push(source(v, 3.0, 0.1, 3.0, "near_decoy", decoy, true));
        let mut g = Game::new(v, bricks, false);
        g.explain_selected();
        g.finish(53);
        assert!(
            (0..4).all(|i| g.marked(&format!("checkpoint_{i}"))),
            "every canonical ordered stage caused its guarded witness"
        );
        assert!(!g.marked("near_decoy"));
        assert!(
            g.phases.contains("approach") && g.phases.contains("waiting"),
            "real admission/delay phases: {:?}",
            g.phases
        );
    }
}

#[test]
fn distinct_switches_compose_a_puzzle_in_metamorphic_worlds() {
    for v in [Variant(false), Variant(true)] {
        let keys: Vec<_> = (0..4).map(|i| v.name(&format!("latch_{i}"))).collect();
        let guards: Vec<_> = keys.iter().map(|key| stage(key, 1)).collect();
        let mut bricks = Vec::new();
        for (i, key) in keys.iter().enumerate() {
            let mut rows = vec![
                event(
                    "onActivate",
                    "setVariable",
                    Slot::SelfBrick,
                    vec![Value::Int(1), Value::Text(key.clone()), Value::Int(1)],
                    vec![stage(key, 0)],
                    0,
                ),
                marker("onActivate", vec![stage(key, 1)], 0),
            ];
            if i == 3 {
                rows.extend(winning("onActivate", 59, guards.clone(), 300));
            }
            bricks.push(source(
                v,
                0.0,
                0.1,
                7.0 + i as f32 * 5.0,
                &format!("switch_{i}"),
                rows,
                false,
            ));
        }
        let mut decoy = vec![marker("onActivate", vec![stage(&keys[0], 99)], 0)];
        decoy.extend(winning("onActivate", 901, vec![stage(&keys[0], 99)], 0));
        bricks.push(source(v, 3.0, 0.1, 3.0, "near_decoy", decoy, false));
        let mut g = Game::new(v, bricks, false);
        g.explain_selected();
        g.finish(59);
        assert!(
            (0..4).all(|i| g.marked(&format!("switch_{i}"))),
            "actual guarded rule progress at all switches"
        );
        assert!(!g.marked("near_decoy"));
    }
}

#[test]
fn changing_the_active_authored_rule_selects_a_new_real_goal() {
    for v in [Variant(false), Variant(true)] {
        let old = source(
            v,
            0.0,
            0.1,
            28.0,
            "old_goal",
            winning("onActivate", 61, vec![], 0),
            false,
        );
        let other = source(v, -7.0, 0.1, 21.0, "new_goal", vec![], false);
        let mut g = Game::new(v, vec![old, other], false);
        g.acquire("old_goal");
        assert_eq!(
            g.s.vitals()[&g.bot].score,
            0,
            "edit happens during actual approach"
        );
        g.edit_rows("old_goal", vec![marker("onActivate", vec![], 0)]);
        let mut replacement = vec![marker("onActivate", vec![], 0)];
        replacement.extend(winning("onActivate", 67, vec![], 0));
        g.edit_rows("new_goal", replacement);
        g.finish(67);
        assert!(g.marked("new_goal"));
        assert!(
            !g.marked("old_goal"),
            "obsolete action never activated a stale source"
        );
    }
}

#[test]
fn a_real_team_change_invalidates_approach_and_due_time_success_guards() {
    for (v, admitted) in [
        (Variant(false), false),
        (Variant(true), false),
        (Variant(false), true),
        (Variant(true), true),
    ] {
        let mut g = Game::new(
            v,
            vec![
                source(v, 0.0, 0.1, 16.0, "first_team", vec![], false),
                source(v, -7.0, 0.1, 21.0, "second_team", vec![], false),
            ],
            true,
        );
        let game = g.s.minigame_views()[0].id;
        g.command(MiniGameRequest::AddOnSettings {
            game,
            settings: vec![],
            teams: Some(
                (0..2)
                    .map(|i| TeamEdit {
                        id: None,
                        name: v.name(&format!("team_{i}")),
                        color: i,
                        settings: vec![],
                    })
                    .collect(),
            ),
            quiet: true,
            reset: false,
        });
        let teams = &g.s.minigame_views()[0].teams;
        let first = teams[0].id.0;
        let second = teams[1].id.0;
        g.command(MiniGameRequest::SetTeam {
            game,
            target: g.bot,
            team: Some(first),
        });
        let guard = |team| vec![number(Property::Team, "", i64::from(team))];
        let mut rows = vec![marker("onActivate", guard(first), 0)];
        rows.extend(winning("onActivate", 71, guard(first), 6000));
        g.edit_rows("first_team", rows);
        let mut rows = vec![marker("onActivate", guard(second), 0)];
        rows.extend(winning("onActivate", 73, guard(second), 0));
        g.edit_rows("second_team", rows);
        g.acquire("first_team");
        if admitted {
            for _ in 0..120 * 10 {
                if g.marked("first_team") {
                    break;
                }
                g.steps(1);
            }
            assert!(
                g.marked("first_team"),
                "native activation occurred before delayed guards"
            );
            assert_eq!(
                g.s.bot_thoughts()
                    .into_iter()
                    .find(|t| t.bot == g.bot)
                    .unwrap()
                    .objective_detail
                    .unwrap()
                    .phase,
                "waiting",
                "actual admitted action is waiting through its authored delay"
            );
        }
        assert_eq!(g.s.vitals()[&g.bot].score, 0);
        g.command(MiniGameRequest::SetTeam {
            game,
            target: g.bot,
            team: Some(second),
        });
        g.finish(73);
        g.steps(120 * 7);
        assert_eq!(
            g.s.vitals()[&g.bot].score,
            73,
            "old delayed team-qualified award stayed rejected"
        );
        assert!(g.marked("second_team"));
        assert_eq!(g.s.vitals()[&g.bot].team, Some(second));
        if !admitted {
            assert!(!g.marked("first_team"));
        }
    }
}

#[test]
fn an_unreachable_sensor_produces_a_bounded_failure_without_success() {
    for v in [Variant(false), Variant(true)] {
        let mut high = source(
            v,
            0.0,
            100.1,
            7.0,
            "unreachable",
            winning("onRegionEnter", 79, vec![], 0),
            true,
        );
        high.rule_region = Some([2.0, 1.0, 2.0]);
        let mut g = Game::new(v, vec![high], false);
        g.steps(120 * 34);
        g.assert_no_success();
        assert!(feet(&g.s, g.bot).y < 10.0, "no synthetic elevation");
        assert!(
            g.reasons
                .iter()
                .any(|r| r.contains("timed out") || r.contains("unreachable")),
            "missing actual execution failure: {:?}",
            g.reasons
        );
    }
}

#[test]
fn an_unknown_collateral_effect_rejects_the_complete_action() {
    for v in [Variant(false), Variant(true)] {
        let mut rows = winning("onActivate", 83, vec![], 0);
        rows.push(event(
            "onActivate",
            &v.name("opaque"),
            Slot::SelfBrick,
            vec![],
            vec![],
            0,
        ));
        let mut g = Game::new(
            v,
            vec![source(v, 0.0, 0.1, 7.0, "unknown", rows, false)],
            true,
        );
        g.steps(480);
        g.assert_no_success();
        assert_eq!(
            g.s.package_state_for(g.owner).packages["creator-teams"].global["visited"],
            json!(false),
            "opaque Add-On effect never ran"
        );
        assert!(g.s.bot_thoughts().iter().all(|t| t.objective.is_none()));
        g.explain_reason("unknown", "unsupported");
    }
}

#[test]
fn a_sixteen_step_dependency_is_an_explicit_depth_negative_not_partial_success() {
    for v in [Variant(false), Variant(true)] {
        let key = v.name("sixteen_steps");
        let mut bricks = Vec::new();
        for i in 0..16 {
            let mut rows = vec![
                event(
                    "onActivate",
                    "setVariable",
                    Slot::SelfBrick,
                    vec![Value::Int(1), Value::Text(key.clone()), Value::Int(i + 1)],
                    vec![stage(&key, i)],
                    0,
                ),
                marker("onActivate", vec![stage(&key, i + 1)], 0),
            ];
            if i == 15 {
                rows.extend(winning("onActivate", 89, vec![stage(&key, 16)], 0));
            }
            bricks.push(source(
                v,
                (i % 4) as f32 * 3.0,
                0.1,
                7.0 + (i / 4) as f32 * 3.0,
                &format!("depth_{i}"),
                rows,
                false,
            ));
        }
        let mut g = Game::new(v, bricks, false);
        g.steps(480);
        g.assert_no_success();
        assert!(
            (0..16).all(|i| !g.marked(&format!("depth_{i}"))),
            "no speculative successful prefix"
        );
        g.explain_reason("depth_15", "depth");
    }
}

#[test]
fn too_many_real_action_sources_report_the_specific_grounding_bound() {
    for v in [Variant(false), Variant(true)] {
        let key = v.name("missing_fact");
        let mut bricks = Vec::new();
        for i in 0..33 {
            bricks.push(source(
                v,
                (i % 6) as f32 * 3.0,
                0.1,
                7.0 + (i / 6) as f32 * 3.0,
                &format!("model_{i}"),
                winning("onActivate", 97, vec![stage(&key, 1)], 0),
                false,
            ));
        }
        let mut g = Game::new(v, bricks, false);
        g.steps(480);
        g.assert_no_success();
        assert!(g.s.bot_thoughts().iter().all(|t| t.objective.is_none()));
        g.explain_reason("model_0", "action");
    }
}
