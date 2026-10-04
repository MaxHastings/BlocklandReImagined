//! Genuine sight loss, ordinary evidence-driven search and credited elimination.
use bri_chaos::fixture;
use bri_events::rules::{Compare, Condition, Datum, Property, Subject};
use bri_events::{Row, Slot, Target};
use bri_sim::player::MoveInput;
use bri_sim::session::{Command, MiniGameRequest, Session, TeamEdit, ToolCatalog};
use bri_world::{Brick, ContentRef, VehicleSpawn, World, build::SavedBuild};
use glam::Vec3;

fn team_catalog() -> std::sync::Arc<bri_package_runtime::Catalog> {
    use bri_package::packages::{PackageEntry, PackageSet, Side};
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Temp(std::path::PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = Temp(std::env::temp_dir().join(format!(
        "bri-search-team-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    let dir = root.0.join("search-teams");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("package.json"),json!({"schema_version":1,"id":"search-teams","version":"1.0.0","api":1,"name":"Ordinary team authoring","license":"CC0-1.0","capabilities":[],"provides":[{"kind":"behaviour","id":"search-teams:behaviour/main","file":"behaviour.json"},{"kind":"script","id":"search-teams:script/main","file":"main.rhai"}]}).to_string()).unwrap();
    std::fs::write(dir.join("behaviour.json"),json!({"schema_version":1,"script":"main.rhai","settings":[{"key":"badge","title":"Badge","scope":"team","type":"bool","default":false}]}).to_string()).unwrap();
    std::fs::write(
        dir.join("main.rhai"),
        "// No gameplay hooks; teams use native authoring.\n",
    )
    .unwrap();
    std::sync::Arc::new(
        bri_package_runtime::Catalog::load(
            &root.0,
            &PackageSet {
                schema_version: 1,
                packages: vec![PackageEntry {
                    id: "search-teams".into(),
                    version: "1.0.0".into(),
                    side: Side::Server,
                    dir: "search-teams".into(),
                    role: None,
                }],
            },
            true,
        )
        .unwrap(),
    )
}

struct Game {
    s: Session,
    human: u64,
    bot: u64,
    seq: u64,
    commands: u64,
}
impl Game {
    fn new(offset: f32) -> Self {
        Self::with_bot_opponent(offset, false)
    }
    fn with_bot_opponent(offset: f32, duel: bool) -> Self {
        let variant = !duel && offset != 0.0;
        let mut s = fixture::synthetic().unwrap().session;
        let mut kinds = bri_sim::bot_kind::BotPack::from_json(include_bytes!(
            "../../../packages/blockhead_bot/assets/bots.json"
        ))
        .unwrap()
        .bots;
        // Authored patient ground fighter, retaining ordinary aim/reaction.
        kinds[0].reaction_seconds = 0.6;
        kinds[0].memory_seconds = 14.0;
        kinds[0].chase_radius = 64.0;
        if duel {
            // Explicit teams must outrank legacy kind/builder defaults.
            kinds[0].fights_bots = false;
        }
        kinds[0].behaviours.insert("fly".into(), 0.0);
        if variant {
            kinds[0].id = "quiet-lookout:bot/patient".into();
            kinds[0].name = "Patient lookout".into();
            kinds[0].first_names = vec!["Sable".into()];
        }
        let bot_kind = kinds[0].id.clone();
        s.set_bot_kinds(kinds).unwrap();
        s.set_event_catalog(bri_events::testing::catalog(), Vec::<String>::new())
            .unwrap();
        s.set_tool_catalog(ToolCatalog {
            vehicles: [bot_kind.clone()].into(),
            vehicle_bricks: [fixture::PLATE.into()].into(),
            ..Default::default()
        })
        .unwrap();
        if duel {
            s.install_packages(team_catalog(), None).unwrap();
        }
        let spawn = Vec3::new(-24.75 + offset, 0.05, 30.25);
        s.set_spawn_points(vec![spawn]).unwrap();
        let human = s
            .join(
                if variant {
                    "Courier under cover"
                } else {
                    "Moving opponent"
                }
                .into(),
                spawn,
                true,
            )
            .unwrap();
        let mut world = World::new(
            if variant {
                "Oblique lookout exercise"
            } else {
                "Evidence search"
            }
            .into(),
            "chaos/map".into(),
            vec![[1.0; 4]; 8],
        );
        // Fifty units away: ordinary pursuit must cross the attack band and
        // the three-second anchor budget before it can see around this screen.
        let mut hole = Brick::new(
            ContentRef::Resolved(fixture::PLATE.into()),
            [-24.75 + offset, 0.1, -19.75],
            human,
        );
        hole.vehicle = Some(Box::new(VehicleSpawn {
            vehicle: ContentRef::Resolved(bot_kind.clone()),
            recolor: false,
            team: None,
        }));
        if variant {
            hole.name = Some("Patient post".into());
        }
        world.bricks.insert(if variant { 801 } else { 1 }, hole);
        if duel {
            let mut second = Brick::new(
                ContentRef::Resolved(fixture::PLATE.into()),
                [-24.75 + offset, 0.1, -9.75],
                human,
            );
            second.vehicle = Some(Box::new(VehicleSpawn {
                vehicle: ContentRef::Resolved(fixture::BOT.into()),
                recolor: false,
                team: None,
            }));
            world.bricks.insert(23, second);
        }
        let mut rule = Brick::new(
            ContentRef::Resolved(fixture::PLATE.into()),
            [-35.75 + offset, 0.1, 30.25],
            human,
        );
        rule.events.push(Row {
            enabled: true,
            input: "onRulePlayerDied".into(),
            output: "winRound".into(),
            target: Target::Slot(Slot::Instigator),
            params: vec![],
            conditions: vec![
                Condition {
                    subject: Subject::Player,
                    property: Property::Alive,
                    key: String::new(),
                    compare: Compare::Equal,
                    value: Datum::Bool(false),
                },
                Condition {
                    subject: Subject::Instigator,
                    property: Property::Score,
                    key: String::new(),
                    compare: Compare::AtLeast,
                    value: Datum::Number(11),
                },
            ],
            delay_ms: 150,
            preserved: None,
        });
        if variant {
            rule.name = Some("Accepted departure".into());
        }
        world.bricks.insert(if variant { 7 } else { 2 }, rule);
        // Solid horizontal screen: initial x=-24.75 sight is outside its left
        // edge. The opponent's ordinary right walk passes behind that edge.
        for i in 0..20 {
            world.bricks.insert(
                if variant { 120 - i } else { i + 3 },
                Brick::new(
                    ContentRef::Resolved(fixture::TALL.into()),
                    [-23.25 + offset + (i as f32) * 0.5, 1.5, 24.25],
                    human,
                ),
            );
        }
        if variant {
            // Irrelevant named decoration is away from both pursuit and cover.
            let mut decor = Brick::new(
                ContentRef::Resolved(fixture::PLATE.into()),
                [-35.75 + offset, 0.1, -65.75],
                human,
            );
            decor.name = Some("Unused survey marker".into());
            decor.color = 3;
            world.bricks.insert(43, decor);
        }
        world.next_brick_id = if variant {
            802
        } else if duel {
            24
        } else {
            23
        };
        s.command(
            human,
            100,
            Command::LoadBuild {
                build: Box::new(SavedBuild::new(world)),
                ownership: false,
            },
        )
        .unwrap();
        let mut seq = 1 << 40;
        for _ in 0..20 {
            seq += 1;
            s.movement(human, seq, MoveInput::default()).unwrap();
            s.step().unwrap();
        }
        let bot = *s.names().keys().find(|id| s.is_bot(**id)).unwrap();
        s.command(
            human,
            101,
            Command::MiniGame(MiniGameRequest::Create {
                color: 0,
                settings: bri_minigames::Settings {
                    player_type: "v20.player.playernojet".into(),
                    loadout: [
                        Some(bri_weapons::testing::GUN_ITEM.into()),
                        None,
                        None,
                        None,
                        None,
                    ],
                    points_kill_player: 11,
                    points_die: 0,
                    ..Default::default()
                },
            }),
        )
        .unwrap();
        Self {
            s,
            human,
            bot,
            seq,
            commands: 101,
        }
    }
    fn step(&mut self, input: MoveInput) {
        self.seq += 1;
        self.s.movement(self.human, self.seq, input).unwrap();
        self.s.step().unwrap();
    }
}
#[test]
fn loss_of_sight_search_reacquires_and_actually_wins_elimination() {
    // Both translations remain west of the synthetic map's fixed z=-15 wall.
    // The second also changes kind/player/world/brick names, sparse insertion
    // IDs and screen traversal order, plus unrelated named decoration.
    for offset in [0.0, -38.0] {
        let mut g = Game::new(offset);
        let mut saw = false;
        for _ in 0..30 {
            g.step(MoveInput::default());
            saw |=
                g.s.bot_thoughts()
                    .iter()
                    .any(|t| t.bot == g.bot && t.visible == Some(g.human));
        }
        assert!(
            saw,
            "must actually observe opponent first: offset={offset}; thoughts={:?}; actors={:?}",
            g.s.bot_thoughts(),
            g.s.snapshot().players
        );
        // Spawn protection is canonical and remains enabled. Walk behind cover
        // before it expires; do not inject an AI memory or actor transform.
        for _ in 0..58 {
            g.step(MoveInput {
                right: 1.0,
                ..Default::default()
            });
        }
        let mut lost = false;
        let mut probe = false;
        let mut reacquired = false;
        let mut elimination_plan = false;
        let mut planned_life = None;
        let mut hidden_evidence = None::<bri_sim::session::BotEvidence>;
        for _ in 0..120 * 20 {
            g.step(MoveInput::default());
            let t =
                g.s.bot_thoughts()
                    .into_iter()
                    .find(|t| t.bot == g.bot)
                    .unwrap();
            lost |= t.visible.is_none() && t.remembered.is_some_and(|k| k.subject == g.human);
            if t.visible.is_none()
                && let Some(evidence) = t.remembered.filter(|k| k.subject == g.human)
            {
                if let Some(old) = hidden_evidence.filter(|k| k.observed == evidence.observed) {
                    assert_eq!(
                        evidence.expires, old.expires,
                        "hidden evidence cannot renew"
                    );
                    assert_eq!(evidence.position, old.position, "hidden pose cannot leak");
                }
                hidden_evidence = Some(evidence);
            }
            probe |= t.search_phase == "unchecked-space probe";
            reacquired |= lost && t.visible == Some(g.human);
            elimination_plan |= t
                .objective_detail
                .as_ref()
                .is_some_and(|d| d.provider == "native combat/death rules");
            if let Some(detail) = t
                .objective_detail
                .as_ref()
                .filter(|d| d.provider == "native combat/death rules")
            {
                planned_life = detail
                    .action
                    .rsplit(':')
                    .next()
                    .and_then(|id| id.parse::<u64>().ok());
            }
            if g.s.round_results().any(|r| r.owners.contains(&g.bot)) {
                break;
            }
        }
        assert!(lost, "sight never lost: {:?}", g.s.bot_thoughts());
        assert!(
            probe,
            "unchecked-space stage missing: {:?}",
            g.s.bot_thoughts()
        );
        assert!(
            reacquired,
            "no actual reacquisition: {:?}",
            g.s.bot_thoughts()
        );
        assert!(elimination_plan, "real death objective not projected");
        let death =
            g.s.death_results()
                .find(|r| r.victim == g.human && r.killer == Some(g.bot))
                .expect("canonical credited death");
        assert_eq!(
            Some(death.life.0),
            planned_life,
            "actual accepted death must match selected exact life"
        );
        assert!(
            g.s.round_results().any(|r| r.owners.contains(&g.bot)),
            "no actual winner: {:?}",
            g.s.bot_thoughts()
        );
    }
}
#[test]
fn leaving_the_game_invalidates_dated_enemy_search_without_refresh() {
    let mut g = Game::new(0.0);
    for _ in 0..30 {
        g.step(MoveInput::default());
    }
    for _ in 0..58 {
        g.step(MoveInput {
            right: 1.0,
            ..Default::default()
        });
    }
    assert!(
        g.s.bot_thoughts()
            .iter()
            .any(|t| t.remembered.is_some_and(|k| k.subject == g.human))
    );
    g.commands += 1;
    g.s.command(
        g.human,
        g.commands,
        Command::MiniGame(MiniGameRequest::Leave),
    )
    .unwrap();
    for _ in 0..120 {
        g.step(MoveInput::default());
    }
    let t =
        g.s.bot_thoughts()
            .into_iter()
            .find(|t| t.bot == g.bot)
            .unwrap();
    assert!(t.remembered.is_none());
    assert!(
        t.objective_detail
            .as_ref()
            .is_none_or(|d| d.provider != "native combat/death rules")
    );
    assert!(!g.s.round_results().any(|r| r.owners.contains(&g.bot)));
}

#[test]
fn a_different_visible_hostile_preempts_then_resumes_the_intended_dated_subject() {
    let mut g = Game::new(0.0);
    for _ in 0..30 {
        g.step(MoveInput::default());
    }
    for _ in 0..58 {
        g.step(MoveInput {
            right: 1.0,
            ..Default::default()
        });
    }
    let mut original = None;
    for _ in 0..120 {
        let thought =
            g.s.bot_thoughts()
                .into_iter()
                .find(|t| t.bot == g.bot)
                .unwrap();
        if thought.visible.is_none()
            && thought
                .objective_detail
                .as_ref()
                .is_some_and(|d| d.provider == "native combat/death rules")
            && let Some(evidence) = thought.remembered.filter(|k| k.subject == g.human)
        {
            original = Some(evidence);
            break;
        }
        g.step(MoveInput::default());
    }
    let original = original.expect("intended real A must be hidden with a retained Enemy plan");
    let game = g.s.minigame_views()[0].id;
    let b =
        g.s.join(
            "New visible hostile".into(),
            Vec3::new(-24.75, 0.05, 30.25),
            false,
        )
        .unwrap();
    g.s.command(b, 1, Command::MiniGame(MiniGameRequest::Join { game }))
        .unwrap();
    let mut b_sequence = 1u64 << 41;
    let mut preempted = false;
    for _ in 0..60 {
        b_sequence += 1;
        g.s.movement(b, b_sequence, MoveInput::default()).unwrap();
        g.step(MoveInput::default());
        preempted |= g.s.bot_thoughts().iter().any(|t| {
            t.bot == g.bot && t.visible == Some(b) && t.remembered.is_some_and(|k| k.subject == b)
        });
    }
    assert!(
        preempted,
        "new genuinely visible hostile did not preempt: {:?}",
        g.s.bot_thoughts()
    );
    assert!(
        g.s.is_alive(b),
        "ordinary spawn immunity preserved during the short interruption"
    );
    g.s.command(b, 2, Command::MiniGame(MiniGameRequest::Leave))
        .unwrap();
    let mut resumed = false;
    for _ in 0..120 * 20 {
        g.step(MoveInput::default());
        let thought =
            g.s.bot_thoughts()
                .into_iter()
                .find(|t| t.bot == g.bot)
                .unwrap();
        if thought.visible.is_none()
            && let Some(evidence) = thought.remembered.filter(|k| k.subject == g.human)
        {
            assert!(
                evidence.observed <= original.observed,
                "resumption invented a new hidden observation"
            );
            assert!(
                evidence.expires <= original.expires,
                "resumption renewed A's expiry from B"
            );
            resumed = true;
        }
        if g.s.round_results().any(|r| r.owners.contains(&g.bot)) {
            break;
        }
    }
    assert!(
        resumed,
        "retained A did not resume with its own dated knowledge: {:?}",
        g.s.bot_thoughts()
    );
    assert!(
        g.s.death_results()
            .any(|r| r.victim == g.human && r.killer == Some(g.bot)),
        "real intended A elimination missing"
    );
    assert!(
        !g.s.death_results().any(|r| r.victim == b),
        "context departure must not be mistaken for eliminating B"
    );
    assert!(
        g.s.round_results().any(|r| r.owners.contains(&g.bot)),
        "canonical winner missing after interruption"
    );
}

#[test]
fn explicit_teams_override_shared_builder_and_kind_defaults_for_real_elimination() {
    for opposed in [false, true] {
        let mut g = Game::with_bot_opponent(if opposed { -17.0 } else { 0.0 }, true);
        let other =
            *g.s.names()
                .keys()
                .find(|id| **id != g.bot && g.s.is_bot(**id))
                .unwrap();
        let game = g.s.minigame_views()[0].id;
        g.commands += 1;
        g.s.command(
            g.human,
            g.commands,
            Command::MiniGame(MiniGameRequest::AddOnSettings {
                game,
                settings: vec![],
                teams: Some(vec![
                    TeamEdit {
                        id: None,
                        name: "Copper".into(),
                        color: 1,
                        settings: vec![],
                    },
                    TeamEdit {
                        id: None,
                        name: "Indigo".into(),
                        color: 2,
                        settings: vec![],
                    },
                ]),
                quiet: true,
                reset: false,
            }),
        )
        .unwrap();
        let teams = &g.s.minigame_views()[0].teams;
        let a = teams[0].id.0;
        let b = teams[1].id.0;
        for (target, team) in [
            (g.human, a),
            (g.bot, a),
            (other, if opposed { b } else { a }),
        ] {
            g.commands += 1;
            g.s.command(
                g.human,
                g.commands,
                Command::MiniGame(MiniGameRequest::SetTeam {
                    game,
                    target,
                    team: Some(team),
                }),
            )
            .unwrap();
        }
        let mut observed_opponent = false;
        for _ in 0..120 * 15 {
            g.step(MoveInput::default());
            observed_opponent |= g.s.bot_thoughts().iter().any(|t| {
                (t.bot == g.bot && t.visible == Some(other))
                    || (t.bot == other && t.visible == Some(g.bot))
            });
            if g.s.round_results().any(|r| r.game == game) {
                break;
            }
        }
        if opposed {
            assert!(
                observed_opponent,
                "explicit opposed bots never saw each other: {:?}",
                g.s.bot_thoughts()
            );
            let death =
                g.s.death_results()
                    .find(|r| {
                        (r.victim == g.bot && r.killer == Some(other))
                            || (r.victim == other && r.killer == Some(g.bot))
                    })
                    .expect("ordinary gun must credit a real opposed same-builder bot death");
            let killer = death.killer.unwrap();
            assert!(
                g.s.round_results()
                    .any(|r| r.game == game && r.owners.contains(&killer)),
                "canonical opposed-bot winner missing"
            );
        } else {
            assert!(
                !observed_opponent,
                "same-team bots treated each other as enemies"
            );
            assert!(
                !g.s.death_results().any(|r| r.game == Some(game)),
                "same-team bot shot a teammate"
            );
            assert!(!g.s.round_results().any(|r| r.game == game));
        }
    }
}
