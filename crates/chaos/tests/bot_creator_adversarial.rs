//! Creator adversaries after real control progress: identity, contention and loss.
//! No post-setup actor/body transforms or fabricated input/results are written.
use bri_chaos::fixture;
use bri_events::rules::{Compare, Condition, Datum, Property, Subject};
use bri_events::{Row, Slot, Target, Value};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::ops::ObjectRef;
use bri_sim::player::MoveInput;
use bri_sim::session::{BotTask, Command, MiniGameRequest, Notice, Session, ToolCatalog};
use bri_world::{
    Brick, ContentRef, VehicleSpawn, World,
    authority::{Edit, WrenchProperties},
    build::SavedBuild,
};
use glam::Vec3;
use serde_json::json;
use std::collections::{BTreeSet, VecDeque};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Clone, Copy)]
struct Variant(bool);
impl Variant {
    fn ns(self) -> &'static str {
        if self.0 {
            "linen-chamber"
        } else {
            "violet-foundry"
        }
    }
    fn at(self, x: f32, y: f32, z: f32) -> [f32; 3] {
        if self.0 {
            [35.25 + z, y, -35.25 - x]
        } else {
            [-24.75 + x, y, 25.25 + z]
        }
    }
    fn name(self, name: &str) -> String {
        format!("{}_{name}", self.ns())
    }
}
struct Temp(std::path::PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn hold_content(
    v: Variant,
    vanish: bool,
    revoke_goal: bool,
) -> (bri_weapons::Pack, Arc<bri_package_runtime::Catalog>) {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = Temp(std::env::temp_dir().join(format!(
        "bri-creator-adversary-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    let tool = format!("{}-instrument", v.ns());
    let grip = format!("{}-grip", v.ns());
    let disturbance = format!("{}-disturbance", v.ns());
    let data = include_str!("../../../packages/showcase/gravity-gun-tool/assets/weapons.json")
        .replace("gravity-gun-tool", &tool)
        .replace("gravity-gun:", &format!("{grip}:"));
    let weapons = bri_weapons::Pack::from_json(data.as_bytes()).unwrap();
    for id in [&tool, &grip, &disturbance] {
        std::fs::create_dir_all(root.0.join(id)).unwrap();
    }
    std::fs::write(root.0.join(&tool).join("package.json"),json!({"schema_version":1,"id":tool,"version":"1.0.0","api":1,"name":"Invented grip instrument","license":"CC0-1.0","provides":[{"kind":"weapons","id":format!("{tool}:weapons/main"),"file":"weapons.json"}]}).to_string()).unwrap();
    std::fs::write(root.0.join(&tool).join("weapons.json"), data).unwrap();
    std::fs::write(root.0.join(&grip).join("package.json"),json!({"schema_version":1,"id":grip,"version":"1.0.0","api":1,"name":"Native grip adapter","license":"CC0-1.0","capabilities":["physics","player","chat"],"provides":[{"kind":"behaviour","id":format!("{grip}:behaviour/main"),"file":"behaviour.json"},{"kind":"script","id":format!("{grip}:script/main"),"file":"gravity.rhai"}]}).to_string()).unwrap();
    std::fs::write(
        root.0.join(&grip).join("behaviour.json"),
        include_str!("../../../packages/showcase/gravity-gun/behaviour.json")
            .replace("gravity-gun-tool", &tool)
            // This fixture gives the tool through its ordinary MiniGame
            // loadout. The optional outside-game automatic grant is unrelated
            // and would attempt to add it to the author's full stock slots.
            .replace("\"on_loadout\": true", "\"on_loadout\": false"),
    )
    .unwrap();
    std::fs::write(
        root.0.join(&grip).join("gravity.rhai"),
        include_str!("../../../packages/showcase/gravity-gun/gravity.rhai"),
    )
    .unwrap();
    let mut entries = vec![
        PackageEntry {
            id: tool.clone(),
            version: "1.0.0".into(),
            side: Side::Shared,
            dir: tool.clone(),
            role: None,
        },
        PackageEntry {
            id: grip.clone(),
            version: "1.0.0".into(),
            side: Side::Server,
            dir: grip,
            role: None,
        },
    ];
    if vanish {
        let capabilities = if revoke_goal {
            vec!["physics", "player", "world.edit"]
        } else {
            vec!["physics", "player"]
        };
        std::fs::write(root.0.join(&disturbance).join("package.json"),json!({"schema_version":1,"id":disturbance,"version":"1.0.0","api":1,"name":"Authored tool outage","license":"CC0-1.0","capabilities":capabilities,"provides":[{"kind":"behaviour","id":format!("{disturbance}:behaviour/main"),"file":"behaviour.json"},{"kind":"script","id":format!("{disturbance}:script/main"),"file":"main.rhai"}]}).to_string()).unwrap();
        std::fs::write(root.0.join(&disturbance).join("behaviour.json"),json!({"schema_version":1,"script":"main.rhai","tick_interval":1,"state":{"global":{"tracked":{"default":{},"visible":"everyone","persist":false},"tripped":{"default":0,"visible":"everyone","persist":false},"loss_sample":{"default":{},"visible":"everyone","persist":false}}}}).to_string()).unwrap();
        std::fs::write(
            root.0.join(&disturbance).join("main.rhai"),
            r#"
fn on_tick() {
    if get("tripped") != 0 { return; }
    let tracked=get("tracked");
    for p in bots() {
        if !p.alive || p.minigame == () { continue; }
        let h=held(p.id); if h == () { continue; }
        let o=object(h); if o == () { continue; }
        let key=`${p.id}/${o.ref}`;
        if !(key in tracked) {tracked[key]=[o.x,o.y,o.z,o.ref];continue;}
        let old=tracked[key];let dx=o.x-old[0];let dy=o.y-old[1];let dz=o.z-old[2];
        if dx*dx+dy*dy+dz*dz > 1.0 {
            set("loss_sample",#{actor:p.id,baseline_ref:old[3],current_ref:o.ref,baseline:[old[0],old[1],old[2]],current:[o.x,o.y,o.z],distance2:dx*dx+dy*dy+dz*dz});
            take_item(p.id,"TOOL:weapon/gravitygun");
            if REVOKE {
                for b in bricks("PLATE") {
                    if b.name == "GOAL" {
                        if b.color == 0 {set_brick_color(b.id,1);}
                        else {set_brick_color(b.id,0);}
                    }
                }
            }
            set("tripped",p.id);break;
        }
    }
    set("tracked",tracked);
}
"#
            .replace("TOOL", &tool)
            .replace("REVOKE", if revoke_goal { "true" } else { "false" })
            .replace("PLATE", fixture::PLATE)
            .replace("GOAL", &v.name("goal")),
        )
        .unwrap();
        entries.push(PackageEntry {
            id: disturbance.clone(),
            version: "1.0.0".into(),
            side: Side::Server,
            dir: disturbance,
            role: None,
        });
    }
    (
        weapons,
        Arc::new(
            bri_package_runtime::Catalog::load(
                &root.0,
                &PackageSet {
                    schema_version: 1,
                    packages: entries,
                },
                true,
            )
            .unwrap(),
        ),
    )
}
struct Game {
    s: Session,
    author: u64,
    bots: Vec<u64>,
    target: u64,
    decoy: u64,
    v: Variant,
    kind: String,
    seq: u64,
    start: Vec3,
    trace: VecDeque<String>,
}
impl Game {
    fn new(
        v: Variant,
        hold: bool,
        elevated: bool,
        two_bots: bool,
        vanish: bool,
        revoke_goal: bool,
    ) -> Self {
        let mut s = fixture::synthetic().unwrap().session;
        let mut loadout = [None, None, None, None, None];
        if hold {
            let (pack, catalog) = hold_content(v, vanish, revoke_goal);
            let mut combined = bri_weapons::testing::pack();
            combined.items.extend(pack.items);
            combined.images.extend(pack.images);
            s.set_weapon_pack(combined).unwrap();
            s.install_packages(catalog, None).unwrap();
            loadout[0] = Some(format!("{}-instrument:weapon/gravitygun", v.ns()));
        }
        let mut pack = bri_vehicles::testing::pack();
        let mut object = bri_vehicles::testing::definition(bri_vehicles::testing::BALL);
        object.id = format!("{}:vehicle/specimen", v.ns());
        object.name = v.name("specimen");
        object.bounds_min = [-0.6; 3];
        object.bounds_max = [0.6; 3];
        object.collision_hulls = vec![bri_vehicles::testing::box_hull(
            object.bounds_min,
            object.bounds_max,
        )];
        object.mass = 60.;
        object.restitution = 0.1;
        let kind = object.id.clone();
        pack.definitions.push(object);
        let mut kinds = bri_sim::bot_kind::BotPack::from_json(include_bytes!(
            "../../../packages/blockhead_bot/assets/bots.json"
        ))
        .unwrap()
        .bots;
        kinds[0].id = format!("{}:bot/controller", v.ns());
        let bot_kind = kinds[0].id.clone();
        s.set_vehicle_pack(pack, kinds).unwrap();
        s.set_event_catalog(bri_events::testing::catalog(), Vec::<String>::new())
            .unwrap();
        s.set_tool_catalog(ToolCatalog {
            vehicles: [bot_kind.clone(), kind.clone()].into(),
            vehicle_bricks: [fixture::PLATE.into()].into(),
            ..Default::default()
        })
        .unwrap();
        let at = Vec3::new(-60., 0.05, -60.);
        s.set_spawn_points(vec![at]).unwrap();
        let author = s.join(v.name("author"), at, true).unwrap();
        let spawn = |id: &str, x: f32, z: f32, name: &str| {
            let mut b = Brick::new(
                ContentRef::Resolved(fixture::PLATE.into()),
                v.at(x, 0.1, z),
                author,
            );
            b.name = Some(v.name(name));
            b.vehicle = Some(Box::new(VehicleSpawn {
                vehicle: ContentRef::Resolved(id.into()),
                recolor: false,
            }));
            b
        };
        let mut bricks = vec![
            spawn(&bot_kind, 0., 0., "actor"),
            spawn(&kind, 0., 7., "intended"),
            spawn(&kind, -3., 4., "identical_decoy"),
        ];
        if two_bots {
            bricks.push(spawn(&bot_kind, -3., 0., "second_actor"));
        }
        let guards = vec![
            Condition {
                subject: Subject::Object,
                property: Property::SpawnedBy,
                key: String::new(),
                compare: Compare::Equal,
                value: Datum::Text(v.name("intended")),
            },
            Condition {
                subject: Subject::Instigator,
                property: Property::Exists,
                key: String::new(),
                compare: Compare::Equal,
                value: Datum::Bool(true),
            },
        ];
        let mut goal = Brick::new(
            ContentRef::Resolved(fixture::PLATE.into()),
            v.at(0., if elevated { 6.1 } else { 0.1 }, 19.),
            author,
        );
        goal.name = Some(v.name("goal"));
        goal.colliding = false;
        goal.raycast = false;
        goal.rule_region = Some([3., 4., 3.]);
        goal.events = vec![
            Row {
                enabled: true,
                input: "onObjectEnter".into(),
                output: "addPlayerScore".into(),
                target: Target::Slot(Slot::Instigator),
                params: vec![Value::Int(31)],
                conditions: guards.clone(),
                delay_ms: 80,
                preserved: None,
            },
            Row {
                enabled: true,
                input: "onObjectEnter".into(),
                output: "winRound".into(),
                target: Target::Slot(Slot::Instigator),
                params: vec![],
                conditions: guards.clone(),
                delay_ms: 80,
                preserved: None,
            },
            Row {
                enabled: true,
                input: "onObjectEnter".into(),
                output: "setColorFX".into(),
                target: Target::Slot(Slot::SelfBrick),
                params: vec![Value::Int(1)],
                conditions: guards,
                delay_ms: 80,
                preserved: None,
            },
        ];
        bricks.push(goal);
        for i in 0..if v.0 { 7 } else { 2 } {
            let mut b = Brick::new(
                ContentRef::Resolved(fixture::PLATE.into()),
                v.at(-12., 0.1, i as f32 + 2.),
                author,
            );
            b.colliding = false;
            b.raycast = false;
            bricks.push(b);
        }
        if v.0 {
            bricks.reverse();
        }
        let mut world = World::new(v.name("world"), "chaos/map".into(), vec![[1.; 4]; 8]);
        for (i, b) in bricks.into_iter().enumerate() {
            world.bricks.insert(i as u64 + 1, b);
        }
        world.next_brick_id = world.bricks.len() as u64 + 1;
        s.command(
            author,
            100,
            Command::LoadBuild {
                build: Box::new(SavedBuild::new(world)),
                ownership: false,
            },
        )
        .unwrap();
        let mut seq = 1 << 40;
        for _ in 0..40 {
            seq += 1;
            s.movement(author, seq, MoveInput::default()).unwrap();
            s.step().unwrap();
        }
        s.command(
            author,
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
        for _ in 0..20 {
            seq += 1;
            s.movement(author, seq, MoveInput::default()).unwrap();
            s.step().unwrap();
        }
        let bots = s
            .names()
            .keys()
            .filter(|o| s.is_bot(**o))
            .copied()
            .collect();
        // Setup identifies actual spawn results from authored positions.
        // Runtime guards, not this observer, select the right body.
        let poses = s.vehicle_poses();
        let nearest = |at: [f32; 3]| {
            s.vehicle_infos()
                .into_iter()
                .filter(|o| o.definition == kind)
                .min_by(|a, b| {
                    let distance = |id| {
                        let p = Vec3::from(poses.iter().find(|p| p.id == id).unwrap().position);
                        let mut delta = p - Vec3::from(at);
                        delta.y = 0.0;
                        delta.length_squared()
                    };
                    distance(a.id).total_cmp(&distance(b.id))
                })
                .unwrap()
                .id
        };
        let target = nearest(v.at(0., 0.1, 7.));
        let decoy = nearest(v.at(-3., 0.1, 4.));
        assert_ne!(target, decoy, "setup produced two distinct real bodies");
        let start = s
            .vehicle_poses()
            .iter()
            .find(|o| o.id == target)
            .unwrap()
            .position
            .into();
        Self {
            s,
            author,
            bots,
            target,
            decoy,
            v,
            kind,
            seq,
            start,
            trace: VecDeque::new(),
        }
    }
    fn step(&mut self) {
        self.seq += 1;
        self.s
            .movement(self.author, self.seq, MoveInput::default())
            .unwrap();
        self.s.step().unwrap();
        let tick = self.s.simulation().state().tick;
        if tick.is_multiple_of(120) || tick < 660 && tick.is_multiple_of(60) {
            if self.trace.len() == 64 {
                self.trace.pop_front();
            }
            let actors: Vec<_> = self
                .s
                .snapshot()
                .players
                .iter()
                .filter(|p| self.bots.contains(&p.owner))
                .map(|p| (p.owner, p.feet))
                .collect();
            self.trace.push_back(format!(
                "tick={tick} actors={actors:?} target={:?} held={:?} goal={:?} thoughts={:?}",
                self.s.vehicle_poses().iter().find(|p| p.id == self.target),
                self.bots
                    .iter()
                    .map(|b| (*b, self.s.held_by(*b)))
                    .collect::<Vec<_>>(),
                self.s.simulation().state().bricks[&self.id("goal")].position,
                self.s.bot_thoughts(),
            ));
        }
    }
    fn position(&self, id: u64) -> Option<Vec3> {
        self.s
            .vehicle_poses()
            .iter()
            .find(|o| o.id == id)
            .map(|o| o.position.into())
    }
    fn loss_frame(&self, loss_tick: Option<u64>) -> String {
        let source = &self.s.simulation().state().bricks[&self.id("goal")];
        let bounds = bri_world::regions::bounds(
            source.rule_region,
            self.s.simulation().brick_box(self.id("goal")).unwrap(),
        );
        let inside = self
            .position(self.target)
            .is_some_and(|p| p.cmpge(bounds.0).all() && p.cmple(bounds.1).all());
        format!(
            "tick={} loss_observed_tick={loss_tick:?} target={:?} bounds={bounds:?} inside={inside} held={:?} inventory={:?} images={:?} scores={:?} rounds={:?} thoughts={:?}",
            self.s.simulation().state().tick,
            self.s.vehicle_poses().iter().find(|p| p.id == self.target),
            self.bots
                .iter()
                .map(|b| (*b, self.s.held_by(*b)))
                .collect::<Vec<_>>(),
            self.s.tool_inventories(),
            self.s.weapon_view().images,
            self.bots
                .iter()
                .map(|b| (*b, self.s.vitals()[b].score))
                .collect::<Vec<_>>(),
            self.s.round_results().collect::<Vec<_>>(),
            self.s.bot_thoughts(),
        )
    }
    fn id(&self, role: &str) -> u64 {
        *self
            .s
            .simulation()
            .state()
            .bricks
            .iter()
            .find(|(_, b)| b.name.as_deref() == Some(self.v.name(role).as_str()))
            .unwrap()
            .0
    }
    fn finish(&mut self, expected: u64) -> u64 {
        let mut held = false;
        let mut moved = false;
        let mut providers = BTreeSet::new();
        let baseline = self.position(expected).unwrap();
        for _ in 0..120 * 50 {
            self.step();
            held |= self
                .bots
                .iter()
                .any(|b| self.s.held_by(*b) == Some(ObjectRef::Vehicle(expected)));
            moved |= self
                .position(expected)
                .is_some_and(|p| p.distance(baseline) > 3.);
            for t in self.s.bot_thoughts() {
                if let Some(d) = t.objective_detail {
                    providers.insert(d.provider);
                }
            }
            let thoughts = self.s.bot_thoughts();
            let owners = thoughts
                .iter()
                .filter(|t| matches!(t.task,Some(BotTask::Push{vehicle,..}) if vehicle==expected))
                .count();
            assert!(
                owners <= 1,
                "same physical body has multiple claimed controllers: {thoughts:?}"
            );
            assert!(
                self.bots
                    .iter()
                    .all(|b| self.s.held_by(*b) != Some(ObjectRef::Vehicle(self.decoy))),
                "identical wrong-spawner body was acquired"
            );
            if self.s.round_results().next_back().is_some() {
                break;
            }
        }
        let result=self.s.round_results().next_back().unwrap_or_else(||panic!("no actual outcome: thoughts={:?}; poses={:?}; providers={providers:?}; diagnostics={:?}; trace={:#?}",self.s.bot_thoughts(),self.s.vehicle_poses(),self.s.package_diagnostics(), self.trace));
        assert_eq!(result.owners.len(), 1);
        let winner = result.owners[0];
        assert!(self.bots.contains(&winner));
        assert_eq!(result.players.len(), 1);
        assert!(moved, "new intended object must really move");
        if providers.contains("declared physical hold") {
            assert!(held, "actual exact-object hold required");
        }
        assert_eq!(self.s.vitals()[&winner].score, 31);
        assert_eq!(self.s.vitals()[&self.author].score, 0);
        assert_eq!(
            self.s.simulation().state().bricks[&self.id("goal")].color_effect,
            1
        );
        assert!(
            self.s.package_diagnostics().is_empty(),
            "{:?}; trace={:#?}",
            self.s.package_diagnostics(),
            self.trace
        );
        winner
    }
    fn await_progress(&mut self, held: bool) {
        for _ in 0..120 * 25 {
            self.step();
            let selected = self.s.bot_thoughts().iter().any(|b| {
                b.objective_detail.as_ref().is_some_and(|d| {
                    d.action
                        .contains(&format!("/onObjectEnter/{}/", self.target))
                })
            });
            let gripping = !held
                || self
                    .bots
                    .iter()
                    .any(|b| self.s.held_by(*b) == Some(ObjectRef::Vehicle(self.target)));
            if selected
                && gripping
                && self
                    .position(self.target)
                    .is_some_and(|p| p.distance(self.start) > 1.)
            {
                assert!(
                    self.s.round_results().next_back().is_none(),
                    "interruption must precede real completion"
                );
                return;
            }
        }
        panic!(
            "no real progress before interruption: {:?}, {:?}; trace={:#?}",
            self.s.bot_thoughts(),
            self.s.vehicle_poses(),
            self.trace
        );
    }
}

#[test]
fn exact_named_incarnation_wins_with_two_identical_bodies_in_transformed_worlds() {
    for v in [Variant(false), Variant(true)] {
        let mut g = Game::new(v, false, false, false, false, false);
        let intended = g.target;
        g.finish(intended);
    }
}

#[test]
fn authored_replacement_after_real_progress_requires_a_new_object_incarnation() {
    for v in [Variant(false), Variant(true)] {
        let mut g = Game::new(v, false, false, false, false, false);
        g.await_progress(false);
        let old = g.target;
        let source = g.id("intended");
        let old_brick = g.s.simulation().state().bricks[&source].clone();
        let props = |vehicle| WrenchProperties {
            name: old_brick.name.clone(),
            vehicle,
            raycast: old_brick.raycast,
            colliding: old_brick.colliding,
            visible: old_brick.visible,
            ..Default::default()
        };
        g.s.edit_brick(g.author, source, Edit::Properties(props(None)))
            .unwrap();
        for _ in 0..4 {
            g.step();
        }
        assert!(
            g.position(old).is_none(),
            "ordinary source removal must retire the old body"
        );
        assert!(
            g.s.round_results().next_back().is_none(),
            "stale object cannot win"
        );
        g.s.edit_brick(
            g.author,
            source,
            Edit::Properties(props(Some(g.kind.clone()))),
        )
        .unwrap();
        let mut new = None;
        for _ in 0..120 {
            g.step();
            new =
                g.s.vehicle_infos()
                    .iter()
                    .find(|o| o.definition == g.kind && o.id != old && o.id != g.decoy)
                    .map(|o| o.id);
            if new.is_some() {
                break;
            }
        }
        let new = new.expect("ordinary reassignment recreates a real body");
        assert_ne!(old, new);
        g.target = new;
        g.finish(new);
    }
}

fn saved_rule_trace(g: &mut Game, goal: u64) -> Vec<String> {
    g.s.explain_rules(g.author, goal).unwrap();
    g.s.take_private_notices()
        .into_iter()
        .filter_map(|(who, notice)| match notice {
            Notice::Chat(text) if who == g.author => Some(text),
            _ => None,
        })
        .collect()
}

fn tool_loss_case(v: Variant, revoke_goal: bool) -> bool {
    let mut g = Game::new(v, true, true, false, true, revoke_goal);
    let goal = g.id("goal");
    let original_color = g.s.simulation().state().bricks[&goal].color;
    let revoked_color = if original_color == 0 { 1 } else { 0 };
    if revoke_goal {
        assert!(
            g.s.simulation().state().palette.len() >= 2,
            "authored color revocation requires two actual palette entries"
        );
        let mut rows = g.s.simulation().state().bricks[&goal].events.clone();
        for row in &mut rows {
            row.conditions.push(Condition {
                subject: Subject::SelfBrick,
                property: Property::Color,
                key: String::new(),
                compare: Compare::Equal,
                value: Datum::Number(i64::from(original_color)),
            });
        }
        // Ordinary setup authoring: this check is actually true initially.
        // The same loss callback changes only its color, never body physics.
        g.s.edit_brick(g.author, goal, Edit::Events(rows)).unwrap();
    }
    g.s.explain_rules(g.author, goal).unwrap();
    g.s.take_private_notices();
    let mut selected = false;
    let mut gripped = false;
    let mut rejected = false;
    let mut loss_tick = None;
    let mut first_grip_frame = None;
    let mut first_loss_frame = None;
    let mut first_hold_clear = None;
    let mut first_method_clear = None;
    let mut first_image_clear = None;
    let mut first_entry = None;
    let mut actual_winner = false;
    let mut causal_trace = VecDeque::new();
    let mut previous_holds = vec![None; g.bots.len()];
    let mut grip_transitions = VecDeque::new();
    let mut first_wrong_grip = None;
    let mut last_wrong_release = None;
    let mut first_intended_grip = None;
    // Removal follows this tick's brain decision. The next brain tick must
    // invalidate the method. The copied native policy checks armed state on
    // its authored tick interval; one end-of-step observation accounts for
    // ordinary hook/queue delivery, without waiting for unrelated snag expiry.
    let hook_ticks = serde_json::from_str::<serde_json::Value>(include_str!(
        "../../../packages/showcase/gravity-gun/behaviour.json"
    ))
    .unwrap()["tick_interval"]
        .as_u64()
        .unwrap();
    let release_bound = hook_ticks + 1;
    for _ in 0..120 * 40 {
        if causal_trace.len() >= 64 {
            causal_trace.pop_front();
        }
        causal_trace.push_back(format!("before {}", g.loss_frame(loss_tick)));
        g.step();
        let tick = g.s.simulation().state().tick;
        for (index, bot) in g.bots.iter().enumerate() {
            let current = g.s.held_by(*bot);
            let previous = previous_holds[index];
            if current != previous {
                if grip_transitions.len() >= 16 {
                    grip_transitions.pop_front();
                }
                grip_transitions.push_back(format!(
                    "tick={tick} actor={bot} held={previous:?}->{current:?} thoughts={:?}",
                    g.s.bot_thoughts()
                ));
                if current == Some(ObjectRef::Vehicle(g.decoy)) {
                    first_wrong_grip.get_or_insert(tick);
                }
                if previous == Some(ObjectRef::Vehicle(g.decoy)) {
                    last_wrong_release = Some(tick);
                }
                if current == Some(ObjectRef::Vehicle(g.target)) {
                    first_intended_grip.get_or_insert(tick);
                }
                previous_holds[index] = current;
            }
        }
        let held_now = g.bots.iter().any(|b| g.s.held_by(*b).is_some());
        gripped |= g
            .bots
            .iter()
            .any(|b| g.s.held_by(*b) == Some(ObjectRef::Vehicle(g.target)));
        if gripped && first_grip_frame.is_none() {
            first_grip_frame = Some(g.loss_frame(loss_tick));
        }
        let thoughts = g.s.bot_thoughts();
        let hold_method = thoughts.iter().any(|t| {
            t.objective_detail
                .as_ref()
                .is_some_and(|d| d.provider == "declared physical hold")
        });
        selected |= hold_method;
        for thought in &thoughts {
            if thought
                .objective_detail
                .as_ref()
                .is_some_and(|d| d.provider == "declared physical hold")
            {
                assert_ne!(
                    thought.behaviour, "carry",
                    "legacy carry must not override the current objective controls: {grip_transitions:#?}"
                );
            }
        }
        let tripped =
            g.s.package_state().packages[&format!("{}-disturbance", v.ns())].global["tripped"]
                .as_u64()
                .is_some_and(|p| g.bots.contains(&p));
        let bounds = bri_world::regions::bounds(
            g.s.simulation().state().bricks[&goal].rule_region,
            g.s.simulation().brick_box(goal).unwrap(),
        );
        let inside = g
            .position(g.target)
            .is_some_and(|p| p.cmpge(bounds.0).all() && p.cmple(bounds.1).all());
        if tripped && loss_tick.is_none() {
            let package_state = g.s.package_state();
            let sample =
                &package_state.packages[&format!("{}-disturbance", v.ns())].global["loss_sample"];
            let expected_ref = format!("vehicle:{}", g.target);
            assert_eq!(sample["baseline_ref"].as_str(), Some(expected_ref.as_str()));
            assert_eq!(sample["current_ref"].as_str(), Some(expected_ref.as_str()));
            assert!(sample["distance2"].as_f64().is_some_and(|d| d > 1.0));
            assert!(
                gripped && g.position(g.target).unwrap().distance(g.start) > 1.,
                "loss must follow genuine grip and motion: first_grip={first_grip_frame:?}; current={}; package={:?}; grips={grip_transitions:#?}; causal_trace={causal_trace:#?}",
                g.loss_frame(None),
                g.s.package_state()
            );
            assert!(
                first_entry.is_none() && !inside && g.s.round_results().next_back().is_none(),
                "the loss must precede input admission, not cancel an accepted input"
            );
            loss_tick = Some(tick);
            first_loss_frame = Some(g.loss_frame(loss_tick));
        }
        if inside && first_entry.is_none() {
            first_entry = Some(tick);
        }
        if let Some(lost) = loss_tick {
            rejected |= thoughts.iter().any(|t| {
                t.objective_diagnostic.is_some_and(|r| {
                    r.contains("invalidated")
                        || r.contains("changed")
                        || r.contains("no grounded")
                        || r.contains("unsupported")
                })
            });
            let tool_image = format!("{}-instrument:image/gravitygun", v.ns());
            let mounted_tool = g.s.weapon_view().images.iter().any(|(owner, images)| {
                g.bots.contains(owner) && images.iter().any(|image| image.image == tool_image)
            });
            if !mounted_tool && first_image_clear.is_none() {
                first_image_clear = Some(tick);
            }
            if !hold_method && first_method_clear.is_none() {
                first_method_clear = Some(tick);
            }
            if !held_now && first_hold_clear.is_none() {
                first_hold_clear = Some(tick);
            }
            if tick > lost {
                assert!(
                    !hold_method,
                    "removed tool remains selected beyond next brain tick: {causal_trace:#?}"
                );
                assert!(
                    !mounted_tool,
                    "removed tool's command image remains mounted beyond next tick: {causal_trace:#?}"
                );
            }
            if tick >= lost + release_bound {
                assert!(
                    !held_now,
                    "native hold survives ordinary hook/queue release bound: {causal_trace:#?}"
                );
            }
            if first_hold_clear.is_some() {
                assert!(
                    !held_now,
                    "removed tool reacquired native grip: {causal_trace:#?}"
                );
            }
            for bot in &g.bots {
                assert!(
                    g.s.tool_inventories()[bot]
                        .slots
                        .iter()
                        .all(Option::is_none)
                );
            }
            if revoke_goal {
                assert_eq!(
                    g.s.simulation().state().bricks[&goal].color,
                    revoked_color,
                    "actual loss callback must revoke the initially true guard; diagnostics={:?}",
                    g.s.package_diagnostics()
                );
            }
        }
        if causal_trace.len() >= 64 {
            causal_trace.pop_front();
        }
        causal_trace.push_back(format!("after {}", g.loss_frame(loss_tick)));
        let result = g.s.round_results().next_back().cloned();
        if let Some(result) = result {
            assert!(
                !revoke_goal,
                "revoked due guard awarded actual win: first_loss={first_loss_frame:?}; {causal_trace:#?}"
            );
            assert!(
                first_entry.is_some_and(|entered| entered < result.tick),
                "exact-body native entry must precede delayed canonical winner: {causal_trace:#?}"
            );
            assert_eq!(result.owners, g.bots);
            assert_eq!(result.players.len(), 1);
            assert!(result.teams.is_empty());
            assert_eq!(g.s.round_results().count(), 1);
            assert_eq!(g.s.vitals()[&g.bots[0]].score, 31);
            if !actual_winner {
                let trace = saved_rule_trace(&mut g, goal);
                assert!(
                    trace.iter().any(|t| t.contains("Object Spawned by")
                        && t.contains(&v.name("intended"))
                        && t.ends_with("- pass")),
                    "{trace:?}"
                );
                assert!(
                    trace.iter().any(|t| t.contains("addPlayerScore -> Player")
                        && t.ends_with("after 80ms: ran")),
                    "{trace:?}"
                );
                assert!(trace.iter().any(|t|t.contains("winRound -> Player")&&t.ends_with("after 80ms: ran")),"{trace:?}");
            }
            actual_winner = true;
        }
    }
    assert!(
        selected && gripped && loss_tick.is_some(),
        "test must follow genuine hold/motion/authorized loss: {causal_trace:#?}"
    );
    assert!(rejected, "lost capability needs honest diagnostics");
    let lost = loss_tick.unwrap();
    assert!(
        first_method_clear.is_some_and(|at| at <= lost + 1),
        "method invalidation timing: {causal_trace:#?}"
    );
    assert!(
        first_image_clear.is_some_and(|at| at <= lost + 1),
        "tool command image invalidation timing: {causal_trace:#?}"
    );
    assert!(
        first_hold_clear.is_some_and(|at| at <= lost + release_bound),
        "native release timing: {causal_trace:#?}"
    );
    for bot in &g.bots {
        assert_eq!(g.s.vitals()[bot].score, if actual_winner { 31 } else { 0 });
        assert!(g.s.held_by(*bot).is_none());
        assert!(
            g.s.tool_inventories()[bot]
                .slots
                .iter()
                .all(Option::is_none)
        );
    }
    assert_eq!(
        g.s.simulation().state().bricks[&goal].color_effect,
        if actual_winner { 1 } else { 0 }
    );
    assert_eq!(g.s.vitals()[&g.author].score, 0);
    if revoke_goal {
        assert!(g.s.round_results().next_back().is_none());
        if first_entry.is_some() {
            let trace = saved_rule_trace(&mut g, goal);
            assert!(
                trace
                    .iter()
                    .any(|t| t.contains("Self Color") && t.ends_with("- skipped")),
                "real coast entry must produce actual due-time guard skip: {trace:?}"
            );
        }
    }
    assert!(
        g.s.package_diagnostics().is_empty(),
        "{:?}; {causal_trace:#?}",
        g.s.package_diagnostics()
    );
    eprintln!(
        "loss chronology {}, guarded={revoke_goal}: first_wrong_grip={first_wrong_grip:?}, last_wrong_release={last_wrong_release:?}, first_intended_grip={first_intended_grip:?}, tool_loss={lost}, method_clear={first_method_clear:?}, image_clear={first_image_clear:?}, hold_clear={first_hold_clear:?}, native_entry={first_entry:?}, canonical_winner={actual_winner}; grips={grip_transitions:#?}",
        v.ns()
    );
    first_entry.is_some()
}

#[test]
fn authored_tool_loss_releases_the_method_and_accepts_only_real_native_outcomes() {
    for v in [Variant(false), Variant(true)] {
        tool_loss_case(v, false);
    }
}

#[test]
fn authored_due_guard_rejects_coasted_entry_after_genuine_tool_loss() {
    let mut entered = false;
    for v in [Variant(false), Variant(true)] {
        entered |= tool_loss_case(v, true);
    }
    assert!(
        entered,
        "paired negative must include real coast entry and actual guard evaluation"
    );
}

#[test]
fn two_allied_bots_contend_for_the_same_body_without_duplicate_controllers_or_awards() {
    for v in [Variant(false), Variant(true)] {
        let mut g = Game::new(v, true, false, true, false, false);
        assert_eq!(g.bots.len(), 2);
        let object = g.target;
        g.await_progress(true);
        let thoughts = g.s.bot_thoughts();
        assert_eq!(
            thoughts
                .iter()
                .filter(|t| matches!(t.task,Some(BotTask::Push{vehicle,..}) if vehicle==object))
                .count(),
            1,
            "genuine progressed action has one resource owner"
        );
        let winner = g.finish(object);
        let loser = *g.bots.iter().find(|b| **b != winner).unwrap();
        assert_eq!(g.s.vitals()[&loser].score, 0);
        for _ in 0..120 {
            g.step();
        }
        assert_eq!(g.s.round_results().count(), 1);
        assert_eq!(
            g.s.vitals()[&winner].score,
            31,
            "same-object contention cannot duplicate the delayed award"
        );
    }
}
