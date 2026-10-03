//! Ordinary creator-authored physical delivery. Setup authors geometry/content;
//! after setup only the human's idle controls and normal Session ticks run.
use bri_chaos::fixture;
use bri_events::rules::{Compare, Condition, Datum, Property, Subject};
use bri_events::{Row, Slot, Target, Value};
use bri_sim::player::MoveInput;
use bri_sim::session::{Command, MiniGameRequest, Session, ToolCatalog};
use bri_world::{Brick, ContentRef, VehicleSpawn, World, build::SavedBuild};
use glam::Vec3;
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

// Reuse the shipped physical command implementation with unfamiliar authored
// weapon/package IDs. The descriptor and real command parameters stay aligned.
fn hold_content(
    namespace: &str,
    varied: bool,
) -> (bri_weapons::Pack, Arc<bri_package_runtime::Catalog>) {
    use bri_package::packages::{PackageEntry, PackageSet, Side};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Temp(std::path::PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let root = Temp(std::env::temp_dir().join(format!(
        "bri-physical-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    let tool = format!("{namespace}-instrument");
    let policy = format!("{namespace}-grip");
    let mut weapon_json =
        include_str!("../../../packages/showcase/gravity-gun-tool/assets/weapons.json")
            .replace("gravity-gun-tool", &tool)
            .replace("gravity-gun:", &format!("{policy}:"));
    if varied {
        weapon_json = weapon_json.replace("90000", "9000").replace("60", "25");
    }
    let weapons = bri_weapons::Pack::from_json(weapon_json.as_bytes()).unwrap();
    for id in [&tool, &policy] {
        std::fs::create_dir_all(root.0.join(id)).unwrap();
    }
    std::fs::write(root.0.join(&tool).join("package.json"), json!({"schema_version":1,"id":tool,"version":"1.0.0","api":1,"name":"Unfamiliar physical instrument","license":"CC0-1.0","provides":[{"kind":"weapons","id":format!("{tool}:weapons/main"),"file":"weapons.json"}]}).to_string()).unwrap();
    std::fs::write(root.0.join(&tool).join("weapons.json"), weapon_json).unwrap();
    std::fs::write(root.0.join(&policy).join("package.json"), json!({"schema_version":1,"id":policy,"version":"1.0.0","api":1,"name":"Declared physical grip","license":"CC0-1.0","capabilities":["physics","player","chat"],"provides":[{"kind":"behaviour","id":format!("{policy}:behaviour/main"),"file":"behaviour.json"},{"kind":"script","id":format!("{policy}:script/main"),"file":"gravity.rhai"}]}).to_string()).unwrap();
    std::fs::write(
        root.0.join(&policy).join("behaviour.json"),
        include_str!("../../../packages/showcase/gravity-gun/behaviour.json")
            .replace("gravity-gun-tool", &tool),
    )
    .unwrap();
    let mut script =
        include_str!("../../../packages/showcase/gravity-gun/gravity.rhai").to_string();
    if varied {
        script = script.replace("90000.0", "9000.0").replace("60.0", "25.0");
    }
    std::fs::write(root.0.join(&policy).join("gravity.rhai"), script).unwrap();
    let catalog = bri_package_runtime::Catalog::load(
        &root.0,
        &PackageSet {
            schema_version: 1,
            packages: vec![
                PackageEntry {
                    id: tool.clone(),
                    version: "1.0.0".into(),
                    side: Side::Shared,
                    dir: tool,
                    role: None,
                },
                PackageEntry {
                    id: policy.clone(),
                    version: "1.0.0".into(),
                    side: Side::Server,
                    dir: policy,
                    role: None,
                },
            ],
        },
        true,
    )
    .unwrap();
    (weapons, Arc::new(catalog))
}

struct Game {
    s: Session,
    human: u64,
    bot: u64,
    object: u64,
    seq: u64,
    start: Vec3,
}

#[derive(Default)]
struct Scene {
    bodies: usize,
    delay_ms: u32,
    repeated_entry: bool,
    initially_inside: bool,
}

impl Game {
    fn new(namespace: &str, offset: f32, drive: bool) -> Self {
        Self::variant(namespace, offset, drive, false, 0.1)
    }
    fn variant(namespace: &str, offset: f32, drive: bool, hold: bool, goal_height: f32) -> Self {
        Self::configured(
            namespace,
            offset,
            drive,
            hold,
            goal_height,
            Scene::default(),
        )
    }
    fn configured(
        namespace: &str,
        offset: f32,
        drive: bool,
        hold: bool,
        goal_height: f32,
        scene: Scene,
    ) -> Self {
        let Scene {
            bodies,
            delay_ms,
            repeated_entry,
            initially_inside,
        } = scene;
        let mut s = fixture::synthetic().unwrap().session;
        if bodies > 0 {
            // Server admission must create the actual workload before the
            // independent, unchanged provider budget can reject it.
            let mut settings = s.server_settings().clone();
            settings.per_player.vehicles = (bodies + 1) as u32;
            settings.physics_vehicles = (bodies + 1) as u32;
            s.set_server_settings(settings).unwrap();
        }
        let mut loadout = [None, None, None, None, None];
        if hold {
            let (weapons, catalog) = hold_content(namespace, offset != 0.0);
            let mut combined = bri_weapons::testing::pack();
            combined.items.extend(weapons.items);
            combined.images.extend(weapons.images);
            s.set_weapon_pack(combined).unwrap();
            s.install_packages(catalog, None).unwrap();
            loadout[0] = Some(format!("{namespace}-instrument:weapon/gravitygun"));
        }
        let mut vehicles = bri_vehicles::testing::pack();
        let mut object = bri_vehicles::testing::definition(if drive {
            bri_vehicles::testing::CAR
        } else {
            bri_vehicles::testing::BALL
        });
        object.id = format!("{namespace}:vehicle/specimen");
        object.name = format!("{namespace} unfamiliar specimen");
        if !drive {
            object.bounds_min = [-0.6; 3];
            object.bounds_max = [0.6; 3];
            object.collision_hulls = vec![bri_vehicles::testing::box_hull(
                object.bounds_min,
                object.bounds_max,
            )];
            object.mass = if offset == 0.0 { 60.0 } else { 85.0 };
            object.restitution = 0.1;
        } else if offset != 0.0 {
            object.bounds_min[2] -= 1.0;
            object.bounds_max[2] += 1.0;
            object.collision_hulls = vec![bri_vehicles::testing::box_hull(
                object.bounds_min,
                object.bounds_max,
            )];
        }
        let object_kind = object.id.clone();
        vehicles.definitions.push(object);
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
            vehicles: [fixture::BOT.to_string(), object_kind.clone()].into(),
            vehicle_bricks: [fixture::PLATE.into()].into(),
            ..Default::default()
        })
        .unwrap();
        // The real author remains near the encounter, so a useless unarmed
        // fight opportunity cannot be hidden by putting all players far away.
        let human_point = Vec3::new(offset - 7.0, 0.05, 43.0);
        s.set_spawn_points(vec![human_point]).unwrap();
        let human = s.join("Author".into(), human_point, true).unwrap();
        let mut world = World::new(
            "Unfamiliar delivery".into(),
            "chaos/map".into(),
            s.simulation().state().palette.clone(),
        );
        let spawner = |kind: &str, at: [f32; 3], name: &str| {
            let mut b = Brick::new(ContentRef::Resolved(fixture::PLATE.into()), at, human);
            b.name = Some(name.into());
            b.vehicle = Some(Box::new(VehicleSpawn {
                vehicle: ContentRef::Resolved(kind.into()),
                recolor: false,
            }));
            b
        };
        let spawn_name = format!("{namespace}_origin");
        world.bricks.insert(
            1,
            spawner(fixture::BOT, [offset + 0.25, 0.1, 40.25], "actor"),
        );
        let mut object_spawner = spawner(
            &object_kind,
            [
                offset + 0.25,
                0.1,
                if initially_inside { 56.25 } else { 47.25 },
            ],
            &spawn_name,
        );
        if drive && offset != 0.0 {
            object_spawner.quarter_turns = 1;
        }
        world.bricks.insert(2, object_spawner);
        let mut goal = Brick::new(
            ContentRef::Resolved(fixture::PLATE.into()),
            [offset + 0.25, goal_height, 57.25],
            human,
        );
        goal.name = Some(format!("{namespace}_destination"));
        goal.colliding = false;
        goal.raycast = false;
        goal.rule_region = Some(if drive {
            [6.0, 8.0, 5.0]
        } else {
            [3.0, 4.0, 3.0]
        });
        goal.events = vec![Row {
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
                    value: Datum::Text(spawn_name),
                },
                Condition {
                    subject: Subject::Instigator,
                    property: Property::Exists,
                    key: String::new(),
                    compare: Compare::Equal,
                    value: Datum::Bool(true),
                },
            ],
            delay_ms,
            preserved: None,
        }];
        if delay_ms > 0 {
            goal.events[0].conditions.push(Condition {
                subject: Subject::SelfBrick,
                property: Property::Color,
                key: String::new(),
                compare: Compare::Equal,
                value: Datum::Number(0),
            });
        }
        if repeated_entry {
            let guards = goal.events[0].conditions.clone();
            goal.events.insert(
                0,
                Row {
                    enabled: true,
                    input: "onObjectEnter".into(),
                    output: "addVariable".into(),
                    target: Target::Slot(Slot::SelfBrick),
                    params: vec![Value::Int(0), Value::Text("entries".into()), Value::Int(1)],
                    conditions: guards,
                    delay_ms: 0,
                    preserved: None,
                },
            );
            goal.events[1].conditions.push(Condition {
                subject: Subject::SelfBrick,
                property: Property::Variable,
                key: "entries".into(),
                compare: Compare::AtLeast,
                value: Datum::Number(2),
            });
        }
        world.bricks.insert(3, goal);
        for i in 0..bodies {
            let x = offset - 9.75 + (i % 5) as f32 * 4.0;
            let z = 34.25 + (i / 5) as f32 * 3.0;
            world.bricks.insert(
                4 + i as u64,
                spawner(&object_kind, [x, 0.1, z], &format!("decor_{i}")),
            );
        }
        world.next_brick_id = 4 + bodies as u64;
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
                    loadout,
                    brick_damage: false,
                    ..Default::default()
                },
            }),
        )
        .unwrap();
        // Spawn identities can be recreated by ordinary MiniGame membership;
        // capture only after the canonical setup synchronization settles.
        for _ in 0..20 {
            seq += 1;
            s.movement(human, seq, MoveInput::default()).unwrap();
            s.step().unwrap();
        }
        if delay_ms > 0 {
            let destination = format!("{namespace}_destination");
            let goal = s
                .simulation()
                .state()
                .bricks
                .values()
                .find(|b| b.name.as_deref() == Some(destination.as_str()))
                .unwrap();
            assert_eq!(goal.color, 0, "the due-time guard starts genuinely true");
        }
        if bodies > 0 {
            assert_eq!(
                s.simulation()
                    .state()
                    .bricks
                    .values()
                    .filter(|b| {
                        b.vehicle
                            .as_ref()
                            .is_some_and(|v| v.vehicle == ContentRef::Resolved(object_kind.clone()))
                    })
                    .count(),
                bodies + 1,
                "all spawners must pass ordinary build-grid admission"
            );
            let infos = s.vehicle_infos();
            let actual: Vec<_> = infos
                .iter()
                .filter(|v| v.definition == object_kind && !v.destroyed)
                .collect();
            assert_eq!(actual.len(), bodies + 1, "all authored bodies must exist");
            let snapshot = s.snapshot();
            let feet = Vec3::from(
                snapshot
                    .players
                    .iter()
                    .find(|p| p.owner == bot)
                    .unwrap()
                    .feet,
            );
            let poses = s.vehicle_poses();
            for body in actual {
                let at = Vec3::from(poses.iter().find(|p| p.id == body.id).unwrap().position);
                assert!(
                    feet.distance(at) < 24.0,
                    "body {} at {at:?} outside the actual bot discovery envelope {feet:?}",
                    body.id
                );
            }
        }
        let object = s
            .vehicle_infos()
            .iter()
            .find(|v| v.definition == object_kind)
            .unwrap()
            .id;
        let start = Vec3::from(
            s.vehicle_poses()
                .iter()
                .find(|v| v.id == object)
                .unwrap()
                .position,
        );
        Self {
            s,
            human,
            bot,
            object,
            seq,
            start,
        }
    }

    fn step(&mut self) {
        self.seq += 1;
        self.s
            .movement(self.human, self.seq, MoveInput::default())
            .unwrap();
        self.s.step().unwrap();
    }

    fn delivery(&mut self, method: &str) {
        let mut selected = false;
        let mut boarded = false;
        let mut held = false;
        let mut moved = false;
        let mut transitions = Vec::new();
        let mut previous = String::new();
        for _ in 0..120 * 40 {
            self.step();
            if let Some(thought) = self.s.bot_thoughts().iter().find(|b| b.bot == self.bot) {
                let detail = format!(
                    "{} {:?} {:?}",
                    thought.behaviour, thought.objective_detail, thought.objective_diagnostic
                );
                if detail != previous && transitions.len() < 40 {
                    transitions.push((self.seq, detail.clone(), self.s.vehicle_poses()));
                    previous = detail;
                }
            }
            held |= self.s.held_by(self.bot)
                == Some(bri_package_runtime::ops::ObjectRef::Vehicle(self.object));
            selected |= self
                .s
                .bot_thoughts()
                .iter()
                .find(|b| b.bot == self.bot)
                .and_then(|b| b.objective_detail.as_ref())
                .is_some_and(|d| d.provider == method);
            boarded |= self
                .s
                .vehicle_infos()
                .iter()
                .find(|v| v.id == self.object)
                .is_some_and(|v| v.occupants.contains(&Some(self.bot)));
            moved |= self
                .s
                .vehicle_poses()
                .iter()
                .find(|v| v.id == self.object)
                .is_some_and(|v| Vec3::from(v.position).distance(self.start) > 3.0);
            if self.s.round_results().any(|r| r.owners == vec![self.bot]) {
                assert!(
                    selected,
                    "canonical win without observing selected {method}"
                );
                assert!(moved, "delivery requires actual object displacement");
                if method == "native control seat" {
                    assert!(boarded, "actual control-seat occupancy required");
                }
                if method == "declared physical hold" {
                    assert!(held, "actual exact-object native grip required");
                }
                return;
            }
        }
        panic!(
            "no canonical delivery via {method}; transitions={transitions:?}; thoughts={:?}, poses={:?}, infos={:?}, vitals={:?}, events={:?}",
            self.s.bot_thoughts(),
            self.s.vehicle_poses(),
            self.s.vehicle_infos(),
            self.s.vitals(),
            self.s.take_event_diagnostics()
        );
    }
}

#[test]
fn contact_delivery_uses_actual_motion_and_credited_round_outcome_across_renamed_layouts() {
    for (namespace, offset) in [("copper-yard", 0.0), ("violet-lab", 24.0)] {
        Game::new(namespace, offset, false).delivery("native physical contact");
    }
}

#[test]
fn unfamiliar_ground_vehicle_uses_its_own_control_seat_and_actual_round_outcome() {
    for (namespace, offset) in [("odd-wagon", 0.0), ("orbital-cart", 24.0)] {
        Game::new(namespace, offset, true).delivery("native control seat");
    }
}

#[test]
fn declared_hold_is_selected_as_cheaper_and_completes_through_native_grip() {
    for (namespace, offset) in [("amber-tractor", 0.0), ("moon-tongs", 24.0)] {
        Game::variant(namespace, offset, false, true, 0.1).delivery("declared physical hold");
    }
}

#[test]
fn declared_hold_delivers_to_elevated_region_without_actor_or_body_injection() {
    for (namespace, offset) in [("cobalt-cradle", 0.0), ("satellite-clamp", 24.0)] {
        Game::variant(namespace, offset, false, true, 6.1).delivery("declared physical hold");
    }
}

#[test]
fn delayed_hold_observes_real_admission_and_retains_native_grip_until_due_winner() {
    for (namespace, offset) in [("patient-grip", 0.0), ("slow-cradle", 24.0)] {
        let mut g = Game::configured(
            namespace,
            offset,
            false,
            true,
            6.1,
            Scene {
                delay_ms: 6000,
                ..Default::default()
            },
        );
        for _ in 0..120 * 20 {
            g.step();
            if g.s.bot_thoughts().iter().any(|b| {
                b.bot == g.bot
                    && b.objective_detail
                        .as_ref()
                        .is_some_and(|d| d.phase == "waiting")
            }) {
                break;
            }
        }
        assert!(
            g.s.bot_thoughts().iter().any(|b| b.bot == g.bot
                && b.objective_detail
                    .as_ref()
                    .is_some_and(|d| d.phase == "waiting")),
            "no real captured admission: {:?}",
            g.s.bot_thoughts()
        );
        assert_eq!(
            g.s.held_by(g.bot),
            Some(bri_package_runtime::ops::ObjectRef::Vehicle(g.object))
        );
        assert_eq!(
            g.s.round_results().count(),
            0,
            "due effect cannot execute early"
        );
        let mut winner = false;
        let mut trace = Vec::new();
        for elapsed in 0..120 * 8 {
            g.step();
            if g.s.round_results().any(|r| r.owners == vec![g.bot]) {
                winner = true;
                break;
            }
            if (elapsed % 60 == 0 || g.s.held_by(g.bot).is_none()) && trace.len() < 16 {
                let snapshot = g.s.snapshot();
                trace.push(format!("tick={} held={:?} players={:?} poses={:?} images={:?} tools={:?} packages={:?}",
                    g.s.simulation().state().tick, g.s.held_by(g.bot), snapshot.players,
                    g.s.vehicle_poses(), snapshot.weapons.images, snapshot.tools,
                    g.s.package_diagnostics()));
            }
            assert_eq!(
                g.s.held_by(g.bot),
                Some(bri_package_runtime::ops::ObjectRef::Vehicle(g.object)),
                "exact admitted grip must survive the due wait: {:?}; trace={trace:#?}",
                g.s.bot_thoughts()
            );
        }
        assert!(winner, "actual due-time canonical winner required");
    }
}

#[test]
fn delayed_hold_guard_change_does_not_announce_a_predicted_win() {
    let mut g = Game::configured(
        "conditional-grip",
        0.0,
        false,
        true,
        6.1,
        Scene {
            delay_ms: 6000,
            ..Default::default()
        },
    );
    for _ in 0..120 * 20 {
        g.step();
        if g.s.bot_thoughts().iter().any(|b| {
            b.bot == g.bot
                && b.objective_detail
                    .as_ref()
                    .is_some_and(|d| d.phase == "waiting")
        }) {
            break;
        }
    }
    assert!(
        g.s.bot_thoughts().iter().any(|b| b.bot == g.bot
            && b.objective_detail
                .as_ref()
                .is_some_and(|d| d.phase == "waiting")),
        "real captured admission required"
    );
    let source =
        *g.s.simulation()
            .state()
            .bricks
            .iter()
            .find(|(_, b)| b.name.as_deref() == Some("conditional-grip_destination"))
            .unwrap()
            .0;
    g.s.edit_brick(g.human, source, bri_world::authority::Edit::Color(1))
        .unwrap();
    for _ in 0..120 * 8 {
        g.step();
    }
    assert_eq!(
        g.s.round_results().count(),
        0,
        "canonical due guard rejects the stale prediction"
    );
}

#[test]
fn permission_change_after_actual_grip_releases_without_inventing_completion() {
    let mut g = Game::variant("revoked-grip", 0.0, false, true, 6.1);
    for _ in 0..120 * 15 {
        g.step();
        if g.s.held_by(g.bot) == Some(bri_package_runtime::ops::ObjectRef::Vehicle(g.object)) {
            break;
        }
    }
    assert_eq!(
        g.s.held_by(g.bot),
        Some(bri_package_runtime::ops::ObjectRef::Vehicle(g.object))
    );
    let mut settings = g.s.minigame_views()[0].settings.clone();
    settings.vehicle_damage = false;
    g.s.command(
        g.human,
        102,
        Command::MiniGame(MiniGameRequest::Configure { settings }),
    )
    .unwrap();
    for _ in 0..120 * 5 {
        g.step();
    }
    assert_eq!(
        g.s.held_by(g.bot),
        None,
        "ordinary native permission must release the real hold"
    );
    assert_eq!(g.s.round_results().count(), 0);
}

#[test]
fn over_limit_real_bodies_fail_with_explicit_discovery_budget() {
    let mut g = Game::configured(
        "busy-yard",
        0.0,
        false,
        false,
        0.1,
        Scene {
            bodies: 9,
            ..Default::default()
        },
    );
    let mut budget = false;
    for _ in 0..120 * 5 {
        g.step();
        budget |= g.s.bot_thoughts().iter().any(|b| {
            b.bot == g.bot && b.objective_diagnostic.is_some_and(|d| d.contains("budget"))
        });
    }
    assert!(
        budget,
        "nearby bodies exceed finite provider budget: {:?}",
        g.s.bot_thoughts()
    );
    assert_eq!(g.s.round_results().count(), 0);
}

#[test]
fn repeated_object_entry_requires_real_exit_and_reentry_for_each_physical_method() {
    for (drive, hold, method) in [
        (false, false, "native physical contact"),
        (false, true, "declared physical hold"),
        (true, false, "native control seat"),
    ] {
        for (namespace, offset, initially_inside) in [
            ("counted-courtyard", 0.0, false),
            ("echo-chamber", 24.0, true),
        ] {
            let mut g = Game::configured(
                namespace,
                offset,
                drive,
                hold,
                0.1,
                Scene {
                    repeated_entry: true,
                    initially_inside,
                    ..Default::default()
                },
            );
            let source =
                *g.s.simulation()
                    .state()
                    .bricks
                    .iter()
                    .find(|(_, b)| {
                        b.name.as_deref() == Some(format!("{namespace}_destination").as_str())
                    })
                    .unwrap()
                    .0;
            let bounds = bri_world::regions::bounds(
                g.s.simulation().state().bricks[&source].rule_region,
                g.s.simulation().brick_box(source).unwrap(),
            );
            let inside = |point: Vec3| point.cmpge(bounds.0).all() && point.cmple(bounds.1).all();
            let mut was_inside = inside(g.start);
            assert_eq!(
                was_inside, initially_inside,
                "actual authored starting state"
            );
            let mut entries = 0;
            let mut selected = false;
            let mut rearmed = false;
            let mut winner = false;
            let mut transitions = Vec::new();
            let mut previous = String::new();
            for _ in 0..120 * 40 {
                g.step();
                let point = Vec3::from(
                    g.s.vehicle_poses()
                        .iter()
                        .find(|v| v.id == g.object)
                        .unwrap()
                        .position,
                );
                let now_inside = inside(point);
                entries += usize::from(now_inside && !was_inside);
                was_inside = now_inside;
                if let Some(detail) =
                    g.s.bot_thoughts()
                        .iter()
                        .find(|b| b.bot == g.bot)
                        .and_then(|b| b.objective_detail.as_ref())
                {
                    selected |= detail.provider == method;
                    rearmed |= detail.phase == "rearm";
                }
                let thought =
                    g.s.bot_thoughts()
                        .into_iter()
                        .find(|b| b.bot == g.bot)
                        .unwrap();
                let state = format!(
                    "{} {:?} {:?}",
                    thought.behaviour, thought.objective_detail, thought.objective_diagnostic
                );
                if state != previous && transitions.len() < 40 {
                    transitions.push((
                        g.s.simulation().state().tick,
                        state.clone(),
                        point,
                        g.s.snapshot().players,
                    ));
                    previous = state;
                }
                if g.s.round_results().any(|r| r.owners == vec![g.bot]) {
                    winner = true;
                    break;
                }
            }
            assert!(
                winner,
                "two real inputs required via {method}: {:?}, poses={:?}, transitions={transitions:?}",
                g.s.bot_thoughts(),
                g.s.vehicle_poses()
            );
            assert!(selected, "the selected ordinary method must execute");
            if initially_inside {
                assert!(rearmed, "an initially occupied region must rearm first");
            }
            assert!(
                entries >= 2,
                "counter result requires two physically observed entries, got {entries}"
            );
        }
    }
}
