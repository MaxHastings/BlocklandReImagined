//! Package policy and ordinary-control Shark integration proofs.
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::{
    Catalog, Dynamic, Op, PlayerKey,
    ops::ObjectRef,
    script::{BrickView, Budget, Call, PlayerView, RayHit, Runtime, Snapshot, World},
    state::Namespace,
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

const PACKAGE: &str = "bot_shark-rules";
const HOLE: &str = "bot_shark:brick/bricksharkbot_holespawndata";
struct Root(PathBuf);
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn catalog() -> Arc<Catalog> {
    catalog_fixture(true, false)
}
fn catalog_fixture(declared: bool, watch_all: bool) -> Arc<Catalog> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = Root(std::env::temp_dir().join(format!(
        "bri-shark-policy-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    let dir = root.0.join(PACKAGE);
    std::fs::create_dir_all(&dir).unwrap();
    let source =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../addon-import/ports/bot_shark/rules");
    let palette = "%color[%a++] = \"0.8 0.85 0.9 1\"; %color[%a++] = \"0.3 0.4 0.5 1\"; %color[%a++] = \"0.15 0.25 0.35 1\"; %color[%a++] = \"0.1 0.3 0.6 1\"; %color[%a++] = \"0.2 0.3 0.25 1\";";
    let mut script = std::fs::read_to_string(source.join("shark.rhai"))
        .unwrap()
        .replace("{{namespace}}", "bot_shark")
        .replace("{{palette|text}}", &serde_json::to_string(palette).unwrap())
        .replace("{{fins|text}}", "\"1 1 1 1\"");
    script.push_str("\nfn cmd_rest_probe(p, b, on) { rest_bot(b, on); }\nfn cmd_harm_probe(p, b) { damage(b, 100.0, p); }\n");
    let behaviour = std::fs::read_to_string(source.join("behaviour.json"))
        .unwrap()
        .replace("{{namespace}}", "bot_shark");
    let mut behaviour: serde_json::Value = serde_json::from_str(&behaviour).unwrap();
    behaviour["commands"] =
        json!([{"name":"rest_probe","args":["int","bool"]},{"name":"harm_probe","args":["int"]}]);
    if watch_all {
        behaviour["on_brick"] = json!(["*"]);
    }
    let manifest = json!({"schema_version":1,"id":PACKAGE,"version":"1.0.0","api":1,"name":"Policy fixture","license":"CC0-1.0","capabilities":["player","physics","damage","bots","world.edit","minigame","effects"],"provides":[{"kind":"behaviour","id":"bot_shark-rules:behaviour/main","file":"behaviour.json"},{"kind":"script","id":"bot_shark-rules:script/main","file":"shark.rhai"}]});
    std::fs::write(
        dir.join("package.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    std::fs::write(dir.join("behaviour.json"), behaviour.to_string()).unwrap();
    std::fs::write(dir.join("shark.rhai"), script).unwrap();
    let body_dir = root.0.join("bot_shark");
    std::fs::create_dir_all(&body_dir).unwrap();
    let mut provides = Vec::new();
    for (name, density) in [
        ("sharkholebot", 0.98),
        ("sharkholebottop", 0.8),
        ("sharkholebotbottom", 5.0),
    ] {
        let body = json!({"schema_version":1,"base":"v20.player.playerstandardarmor","name":"","model":"bot_shark:asset/shark.dts","max_health":300,"rideable":false,"can_ride":false,
            "movement":{"width":3.5,"stand_height":1.8,"crouch_height":1.8,"step_height":0,"forward":2,"backward":1,"sideways":1,"underwater_forward":10,"underwater_backward":5,"underwater_sideways":8,"density":density},
            "mount_points":[{"node":"mount0","position":[0,0.8,0]},{"node":"mount1","position":[0,0.8,0]},{"node":"mount2","position":[0,0.8,-2]},{"node":"mount3","position":[0,0.8,-2]}]});
        std::fs::write(
            body_dir.join(format!("{name}.json")),
            serde_json::to_vec(&body).unwrap(),
        )
        .unwrap();
        provides.push(json!({"kind":"archetype","id":format!("bot_shark:archetype/{name}"),"file":format!("{name}.json")}));
    }
    let parent = json!({"schema_version":1,"id":"bot_shark","version":"1.0.0","api":1,"name":"Physical Shark fixture","license":"CC0-1.0","capabilities":[],"companions":if declared { vec![PACKAGE] } else { vec![] },"provides":provides});
    std::fs::write(
        body_dir.join("package.json"),
        serde_json::to_vec(&parent).unwrap(),
    )
    .unwrap();
    Arc::new(
        Catalog::load(
            &root.0,
            &PackageSet {
                schema_version: 1,
                packages: vec![
                    PackageEntry {
                        id: "bot_shark".into(),
                        version: "1.0.0".into(),
                        side: Side::Server,
                        dir: "bot_shark".into(),
                        role: None,
                    },
                    PackageEntry {
                        id: PACKAGE.into(),
                        version: "1.0.0".into(),
                        side: Side::Server,
                        dir: PACKAGE.into(),
                        role: None,
                    },
                ],
            },
            true,
        )
        .unwrap_or_else(|e| panic!("{e:#?}")),
    )
}
struct Admission(bool);
impl World for Admission {
    fn raycast(&self, _: [f32; 3], _: [f32; 3], _: f32, _: Option<u64>) -> Option<RayHit> {
        None
    }
    fn can_damage(&self, _: u64, _: ObjectRef) -> bool {
        self.0
    }
    fn brick_box(&self, _: u64) -> Option<([f32; 3], [f32; 3])> {
        None
    }
    fn voxel(&self, _: u64) -> Option<([i64; 3], String)> {
        None
    }
    fn can_place_voxel(&self, _: [i64; 3]) -> bool {
        false
    }
    fn bricks_of(&self, _: &str, _: usize) -> Vec<BrickView> {
        Vec::new()
    }
}
struct Policy {
    runtime: Runtime,
    state: Namespace,
    snapshot: Snapshot,
}
fn actor(id: u64, bot: bool) -> PlayerView {
    PlayerView {
        id,
        key: PlayerKey::session(id),
        alive: true,
        bot,
        bot_kind: if bot {
            "bot_shark:bot/sharkholebot".into()
        } else {
            String::new()
        },
        scale: 1.0,
        minigame: Some(7),
        model: if bot {
            "bot_shark:asset/shark.dts".into()
        } else {
            "v20.shape.m".into()
        },
        ..Default::default()
    }
}
impl Policy {
    fn new() -> Self {
        Self {
            runtime: Runtime::compile(&catalog()).unwrap_or_else(|e| panic!("{e:#?}")),
            state: Namespace {
                global: BTreeMap::from([
                    ("grabs".into(), json!({})),
                    ("restarts".into(), json!({})),
                    ("known".into(), json!({"1":{"white":true,"cool":false}})),
                ]),
                ..Default::default()
            },
            snapshot: Snapshot {
                players: vec![actor(2, false)],
                bots: vec![actor(1, true)],
                ..Default::default()
            },
        }
    }
    fn call(&mut self, f: &str, args: Vec<Dynamic>, permission: bool) -> Vec<Op> {
        let outcome = self
            .runtime
            .call(
                PACKAGE,
                Call {
                    function: f,
                    args,
                    budget: Budget::Tick,
                    snapshot: Arc::new(self.snapshot.clone()),
                    caller: None,
                    aim: None,
                    entity: None,
                    state: self.state.clone(),
                    entity_vars: Arc::new(BTreeMap::new()),
                    world: Some(&Admission(permission)),
                },
            )
            .unwrap_or_else(|e| panic!("{f}: {e:#?}"));
        self.state = outcome.state;
        outcome.ops
    }
    fn capture(&mut self) -> Vec<Op> {
        let info = bri_package_runtime::rhai::Map::from_iter([
            ("kind".into(), Dynamic::from("weapon")),
            ("type".into(), Dynamic::from("Bite")),
        ]);
        self.call(
            "on_damage",
            vec![2_i64.into(), 1_i64.into(), 35.0.into(), info.into()],
            true,
        )
    }
}
#[test]
fn a_supported_bite_captures_once_then_kills_after_five_seconds() {
    let mut p = Policy::new();
    let ops = p.capture();
    assert!(ops.iter().any(|o|matches!(o,Op::MountObject(m) if m.mount==1 && m.rider==2 && m.node==2 && !m.can_dismount)));
    assert!(
        ops.iter()
            .any(|o| matches!(o,Op::RestBot(b) if b.bot==1 && b.rest))
    );
    assert!(!p.capture().iter().any(|o| matches!(o, Op::MountObject(_))));
    p.snapshot.players[0].mount = Some(1);
    p.snapshot.players[0].mounted = true;
    p.snapshot.tick = 599;
    assert!(
        !p.call("on_tick", vec![], true)
            .iter()
            .any(|o| matches!(o, Op::Damage(_)))
    );
    p.snapshot.tick = 600;
    let ops = p.call("on_tick", vec![], true);
    assert!(ops.iter().any(|o|matches!(o,Op::Damage(d) if d.target==ObjectRef::Player(2) && d.by==Some(1) && d.amount==1000.0)));
    assert!(
        ops.iter()
            .any(|o| matches!(o,Op::UnmountObject(u) if u.rider==2))
    );
    assert_eq!(p.state.global["grabs"], json!({}));
}
#[test]
fn death_leave_reset_team_change_and_lost_permission_release_without_damage() {
    for scenario in 0..9 {
        let mut p = Policy::new();
        p.capture();
        p.snapshot.players[0].mount = Some(1);
        p.snapshot.players[0].mounted = true;
        p.snapshot.tick = 600;
        let ops = match scenario {
            0 => p.call("on_death", vec![1_i64.into(), ().into()], true),
            1 => p.call("on_death", vec![2_i64.into(), ().into()], true),
            2 => p.call("on_leave", vec![1_i64.into()], true),
            3 => p.call("on_spawn", vec![2_i64.into()], true),
            4 | 5 => {
                let e = bri_package_runtime::rhai::Map::from_iter([
                    (
                        "kind".into(),
                        Dynamic::from(if scenario == 4 { "reset" } else { "team" }),
                    ),
                    ("game".into(), 7_i64.into()),
                    ("player".into(), 2_i64.into()),
                ]);
                p.call("on_minigame", vec![e.into()], true)
            }
            6 => {
                p.snapshot.players[0].minigame = Some(8);
                p.call("on_tick", vec![], true)
            }
            7 => {
                p.snapshot.players[0].mount = None;
                p.call("on_tick", vec![], true)
            }
            _ => p.call("on_tick", vec![], false),
        };
        assert!(
            !ops.iter().any(|o| matches!(o, Op::Damage(_))),
            "scenario {scenario}"
        );
        assert!(
            ops.iter().any(|o| matches!(o, Op::OrbitCamera(_))),
            "scenario {scenario}"
        );
        assert_eq!(p.state.global["grabs"], json!({}), "scenario {scenario}");
    }
}
#[test]
fn an_authored_cool_shark_and_random_palette_use_bounded_initialization() {
    let mut p = Policy::new();
    p.snapshot.bots[0].name = "Cool Shark".into();
    p.state.global.insert("known".into(), json!({}));
    let ops = p.call("on_tick", vec![], true);
    assert!(ops.iter().any(|o|matches!(o,Op::SetAvatarParts(a) if a.parts.get("hat").is_some_and(|v|v=="helmet") && a.parts.get("accent").is_some_and(|v|v=="visor"))));
    assert!(
        ops.iter()
            .any(|o| matches!(o,Op::SetAvatarColors(a) if a.colors["larm"]==[1.0;4]))
    );
    assert!(
        p.call("on_tick", vec![], true).is_empty(),
        "no repeated repaint or body swap"
    );
    p.snapshot.bots.clear();
    p.call("on_tick", vec![], true);
    assert_eq!(p.state.global["known"], json!({}));
    let mut colors = std::collections::BTreeSet::new();
    let mut bodies = std::collections::BTreeSet::new();
    for id in 10..138 {
        p.snapshot.bots = vec![actor(id, true)];
        p.state.global.insert("known".into(), json!({}));
        for op in p.call("on_tick", vec![], true) {
            match op {
                Op::SetAvatarColors(a) => {
                    colors.insert(serde_json::to_string(&a.colors["head"]).unwrap());
                }
                Op::SetArchetype(a) => {
                    bodies.insert(a.archetype);
                }
                _ => {}
            }
        }
    }
    assert_eq!(
        colors.len(),
        5,
        "all five authored palette positions are usable"
    );
    assert_eq!(
        bodies,
        ["sharkholebot", "sharkholebottop", "sharkholebotbottom"]
            .map(|b| format!("bot_shark:archetype/{b}"))
            .into()
    );
}
#[test]
fn a_large_hit_releases_after_the_short_capture_grace() {
    let mut p = Policy::new();
    p.capture();
    p.snapshot.players[0].mount = Some(1);
    p.snapshot.tick = 24;
    let info = bri_package_runtime::rhai::Map::from_iter([
        ("kind".into(), Dynamic::from("weapon")),
        ("type".into(), Dynamic::from("Gun")),
    ]);
    let ops = p.call(
        "on_damage",
        vec![1_i64.into(), 2_i64.into(), 50.0.into(), info.into()],
        true,
    );
    assert!(ops.iter().any(|o| matches!(o, Op::UnmountObject(_))));
    assert!(
        !ops.iter().any(|o| matches!(o, Op::RestBot(b) if !b.rest)),
        "harm release keeps the already-resting holder asleep"
    );
    assert_eq!(p.state.global["grabs"], json!({}));
    assert_eq!(p.state.global["restarts"]["1"]["at"], 264);
    p.snapshot.players[0].mount = None;
    p.snapshot.tick = 263;
    assert!(!p.capture().iter().any(|o| matches!(o, Op::MountObject(_))));
    assert!(p.call("on_tick", vec![], true).is_empty());
    p.snapshot.tick = 264;
    let ops = p.call("on_tick", vec![], true);
    assert!(
        ops.iter()
            .any(|o| matches!(o, Op::RestBot(b) if b.bot == 1 && !b.rest))
    );
    assert_eq!(p.state.global["restarts"], json!({}));
    p.snapshot.players[0].mounted = false;
    assert!(p.capture().iter().any(|o| matches!(o, Op::MountObject(_))));
}

#[test]
fn interrupted_restart_is_cancelled_by_lifecycle_identity_and_permission_changes() {
    for scenario in 0..15 {
        let mut p = Policy::new();
        p.capture();
        p.snapshot.players[0].mount = Some(1);
        p.snapshot.tick = 24;
        let info = bri_package_runtime::rhai::Map::from_iter([
            ("kind".into(), Dynamic::from("weapon")),
            ("type".into(), Dynamic::from("Gun")),
        ]);
        p.call(
            "on_damage",
            vec![1_i64.into(), 2_i64.into(), 50.0.into(), info.into()],
            true,
        );
        assert_eq!(p.state.global["restarts"]["1"]["at"], 264);
        p.snapshot.tick = 25;
        let ops = match scenario {
            0 => p.call("on_death", vec![1_i64.into(), ().into()], true),
            1 => p.call("on_death", vec![2_i64.into(), ().into()], true),
            2 => p.call("on_leave", vec![1_i64.into()], true),
            3 => p.call("on_spawn", vec![1_i64.into()], true),
            4 => p.call("on_spawn", vec![2_i64.into()], true),
            5..=7 => {
                let e = bri_package_runtime::rhai::Map::from_iter([
                    (
                        "kind".into(),
                        Dynamic::from(match scenario {
                            5 => "reset",
                            6 => "team",
                            _ => "configured",
                        }),
                    ),
                    ("game".into(), 7_i64.into()),
                    ("player".into(), 2_i64.into()),
                ]);
                p.call("on_minigame", vec![e.into()], true)
            }
            8 => {
                p.snapshot.bots[0].minigame = Some(8);
                p.call("on_tick", vec![], true)
            }
            9 => {
                p.snapshot.players[0].minigame = Some(8);
                p.call("on_tick", vec![], true)
            }
            10 => p.call("on_tick", vec![], false),
            11 => {
                p.snapshot.bots[0].alive = false;
                p.call("on_tick", vec![], true)
            }
            12 => {
                p.snapshot.players.clear();
                p.call("on_tick", vec![], true)
            }
            14 => {
                p.snapshot.bots[0].model = "v20.shape.m".into();
                p.call("on_tick", vec![], true)
            }
            _ => {
                p.snapshot.bots[0].bot_kind = "foreign:bot/borrower".into();
                p.call("on_tick", vec![], true)
            }
        };
        assert_eq!(p.state.global["restarts"], json!({}), "scenario {scenario}");
        assert!(
            !ops.iter().any(|o| matches!(o, Op::Damage(_))),
            "scenario {scenario}"
        );
        if scenario == 13 {
            assert!(
                !ops.iter().any(|o| matches!(o, Op::RestBot(_))),
                "foreign kinds receive no rest operation"
            );
        } else {
            assert!(
                ops.iter()
                    .any(|o| matches!(o, Op::RestBot(b) if b.bot == 1 && !b.rest)),
                "scenario {scenario}"
            );
        }
        p.snapshot.bots.clear();
        p.snapshot.tick = 264;
        assert!(
            p.call("on_tick", vec![], true).is_empty(),
            "no stale deferred wake: scenario {scenario}"
        );
    }
}

/// Actual actor approach with the authored collision dimensions. The scene
/// uses a headless stand-in model/mouth point; no damage-hook call or bot brain
/// injection. The shipped mesh/mount-point fidelity is importer/renderer work.
#[test]
fn an_authored_large_shark_reaches_a_human_and_finishes_a_real_capture() {
    let mut pack = bri_sim::bot_kind::BotPack::from_json(include_bytes!(
        "../../../packages/blockhead_bot/assets/bots.json"
    ))
    .unwrap();
    let kind = &mut pack.bots[0];
    kind.id = "bot_shark:bot/sharkholebot".into();
    kind.name = "Cool Shark".into();
    kind.body = Some("bot_shark:archetype/sharkholebot".into());
    kind.moves = bri_sim::bot_kind::Moves::Swim;
    kind.side = Some("shark".into());
    kind.melee = Some(
        serde_json::from_value(json!({"damage":35,"reach":2.5,"seconds":1,"name":"Bite"})).unwrap(),
    );
    kind.out_of_water_seconds = Some(9.0);
    physical_capture(catalog(), pack.bots.remove(0), 1);
}

#[test]
fn real_harm_releases_the_rider_and_delays_the_brain_restart() {
    use bri_sim::{
        player::MoveInput,
        session::{Command, PackageArg, PackageCommand},
    };
    let mut kind = bri_sim::bot_kind::BotPack::from_json(include_bytes!(
        "../../../packages/blockhead_bot/assets/bots.json"
    ))
    .unwrap()
    .bots
    .remove(0);
    kind.id = "bot_shark:bot/sharkholebot".into();
    kind.name = "Cool Shark".into();
    kind.body = Some("bot_shark:archetype/sharkholebot".into());
    kind.moves = bri_sim::bot_kind::Moves::Swim;
    kind.side = Some("shark".into());
    kind.melee = Some(
        serde_json::from_value(json!({"damage":35,"reach":2.5,"seconds":1,"name":"Bite"})).unwrap(),
    );
    let (mut s, human) = physical_scene(catalog(), kind);
    for sequence in 11..=2_000 {
        s.movement(human, sequence, MoveInput::default()).unwrap();
        s.step().unwrap();
        if s.vitals()[&human].ride.is_some() {
            break;
        }
    }
    assert!(
        s.vitals()[&human].ride.is_some(),
        "ordinary approach must first capture the swimmer"
    );
    let bot = s.names().keys().copied().find(|id| s.is_bot(*id)).unwrap();
    for _ in 0..24 {
        s.step().unwrap();
    }
    let interrupted_at = s.simulation().state().tick;
    let health_before = s.vitals()[&bot].health;
    // This fixture command submits ordinary attributed damage; it never sets
    // the brain, mount relationship or package state to manufacture a release.
    // Indirect harm is scaled to 75% on a crouched swimmer before the hook;
    // 100 raw damage remains above the source's 50-point release threshold.
    s.command(
        human,
        3,
        Command::Package(PackageCommand {
            package: PACKAGE.into(),
            command: "harm_probe".into(),
            args: vec![PackageArg::Int(bot as i64)],
        }),
    )
    .unwrap();
    assert!(
        health_before - s.vitals()[&bot].health >= 50.0 && s.is_alive(bot),
        "nonlethal harm passed the capture grace, canonical scaling and source release threshold"
    );
    assert!(
        s.vitals()[&human].ride.is_none(),
        "harm unmounts the actual rider"
    );
    while s.simulation().state().tick < interrupted_at + 240 {
        s.step().unwrap();
        assert!(
            s.vitals()[&human].ride.is_none(),
            "no early recapture during the two-second pause"
        );
        let thought = s.bot_thoughts().into_iter().find(|t| t.bot == bot).unwrap();
        assert!(
            thought.goal.is_none() && thought.visible.is_none(),
            "rest retains ordinary zero controls until restart: {thought:?}"
        );
    }
    // Session::step runs controller(t), package hooks(t), then advances the
    // public simulation tick to t+1. At public deadline, the most recent hook
    // therefore observed deadline-1. This step runs the last resting control
    // at deadline and then wakes via the exact-deadline package hook.
    s.step().unwrap();
    let thought = s.bot_thoughts().into_iter().find(|t| t.bot == bot).unwrap();
    assert!(
        thought.goal.is_none() && thought.visible.is_none() && s.vitals()[&human].ride.is_none(),
        "the deadline hook follows its resting controller phase: {thought:?}"
    );
    // The very next controller turn, at deadline+1, must observe the swimmer
    // again without injected targets or an arbitrary perception grace period.
    s.step().unwrap();
    let thought = s.bot_thoughts().into_iter().find(|t| t.bot == bot).unwrap();
    assert!(
        thought.visible == Some(human) || s.vitals()[&human].ride.is_some(),
        "the real brain resumes from observed controls after its deadline: {thought:?}"
    );
    assert!(
        s.package_diagnostics().is_empty(),
        "{:?}",
        s.package_diagnostics()
    );
}

fn physical_capture(catalog: Arc<Catalog>, kind: bri_sim::bot_kind::BotKind, max_lives: u64) {
    use bri_sim::player::MoveInput;
    use glam::Vec3;
    let (mut s, human) = physical_scene(catalog, kind);
    let mut mounted_at = None;
    let mut holder = None;
    let mut killed_at = None;
    let mut checked_sight = false;
    let mut checked_mouth = false;
    let mut previous_health = s.vitals()[&human].health;
    let mut bite_trace = Vec::new();
    let mut lives = 1;
    for tick in 0..120 * 30 * max_lives {
        s.movement(human, tick + 11, MoveInput::default()).unwrap();
        s.step().unwrap();
        let vitals = s.vitals();
        if vitals[&human].health < previous_health && bite_trace.len() < 8 {
            let snapshot = s.snapshot();
            let swimmer = snapshot.players.iter().find(|p| p.owner == human).unwrap();
            if let Some(bot) = snapshot.players.iter().find(|p| s.is_bot(p.owner)) {
                bite_trace.push((
                    s.simulation().state().tick,
                    vitals[&human].health,
                    s.archetypes().resolve(swimmer.archetype).look.model.clone(),
                    s.archetypes().resolve(bot.archetype).look.model.clone(),
                    bot.feet,
                    swimmer.feet,
                    // Native body bites run before physics advances this tick.
                    (bri_package_runtime::noise::hash3(
                        0,
                        bot.owner as i64,
                        s.simulation().state().tick as i64 - 1,
                        23,
                    ) * 3.0)
                        .floor() as i64,
                ));
            }
        }
        previous_health = vitals[&human].health;
        if !checked_sight && let Some(bot) = s.names().keys().copied().find(|id| s.is_bot(*id)) {
            let snapshot = s.snapshot();
            let from = s
                .archetypes()
                .eye(snapshot.players.iter().find(|p| p.owner == bot).unwrap());
            let to = s
                .archetypes()
                .eye(snapshot.players.iter().find(|p| p.owner == human).unwrap());
            assert!(
                s.simulation().sight(from, to, 80.0).is_some(),
                "raycasting water is a zone, not an opaque surface: {from:?} -> {to:?}"
            );
            let actors = s.snapshot();
            let swimmer = actors.players.iter().find(|p| p.owner == human).unwrap();
            let shark = actors.players.iter().find(|p| p.owner == bot).unwrap();
            assert_eq!(
                s.archetypes().resolve(swimmer.archetype).look.model,
                "v20.shape.m"
            );
            assert_eq!(
                s.archetypes().resolve(shark.archetype).look.model,
                "bot_shark:asset/shark.dts"
            );
            let hole = &s.simulation().state().bricks[&2];
            assert!(
                !hole.visible && !hole.colliding && !hole.raycast,
                "the package hides its actual hole after load"
            );
            let body = s.archetypes().resolve(shark.archetype);
            eprintln!(
                "Shark ordinary encounter: body={}, density={}, source palette roll={}, spawn_tick={}",
                body.id,
                body.movement.density,
                (bri_package_runtime::noise::hash3(
                    0,
                    bot as i64,
                    vitals[&bot].spawn_tick as i64 + 1,
                    11
                ) * 5.0)
                    .floor(),
                vitals[&bot].spawn_tick
            );
            checked_sight = true;
        }
        if vitals[&human].ride.is_some() && mounted_at.is_none() {
            eprintln!("Shark genuine capture on life {lives}, fixture tick {tick}");
            mounted_at = Some(tick);
            holder = s.names().keys().copied().find(|id| s.is_bot(*id));
        }
        if mounted_at.is_some() && !checked_mouth && tick > mounted_at.unwrap() {
            let snapshot = s.snapshot();
            let bot = snapshot
                .players
                .iter()
                .find(|p| p.owner == holder.unwrap())
                .unwrap();
            let rider = snapshot.players.iter().find(|p| p.owner == human).unwrap();
            let body = s.archetypes().resolve(bot.archetype);
            assert_eq!(body.movement.width, 3.5);
            assert!((body.movement.stand_height - 1.8).abs() < 0.001);
            let mouth = body.mount_points[2].seat(Vec3::from(bot.feet), bot.yaw, bot.scale);
            assert!(
                mouth.distance(Vec3::from(rider.feet)) < 0.01,
                "actual rider follows authored mount2: {mouth:?} != {:?}",
                rider.feet
            );
            checked_mouth = true;
        }
        if !s.is_alive(human) {
            if mounted_at.is_some() {
                killed_at = Some(tick);
                break;
            }
            if lives >= max_lives {
                break;
            }
            // Non-white source Sharks capture on one in three eligible bites.
            // Keep authentic fallback damage, then use the ordinary respawn
            // command for another bounded encounter rather than forcing odds.
            if s.simulation().state().tick >= vitals[&human].respawn_tick {
                s.command(human, lives + 2, bri_sim::session::Command::Respawn)
                    .unwrap();
                eprintln!(
                    "Shark ordinary respawn after fallback death on life {lives}, fixture tick {tick}"
                );
                lives += 1;
                previous_health = s.vitals()[&human].health;
            }
        }
    }
    assert!(
        mounted_at.is_some(),
        "real bot approach must mount the human: diagnostics {:?}; names {:?}; vitals {:?}; thoughts {:?}; players {:?}; bite trace {:?}",
        s.package_diagnostics(),
        s.names(),
        s.vitals(),
        s.bot_thoughts(),
        s.snapshot().players,
        bite_trace
    );
    assert!(
        !s.is_alive(human),
        "five-second observed capture must finish through ordinary damage"
    );
    let held_ticks = killed_at.unwrap() - mounted_at.unwrap();
    assert!(
        (599..=601).contains(&held_ticks),
        "five-second physical hold, allowing one hook/physics phase: {held_ticks}"
    );
    eprintln!("Shark credited capture finished after {held_ticks} ticks, lives={lives}");
    assert!(checked_sight && checked_mouth);
    let bot = holder.unwrap();
    assert!(
        s.vitals()[&bot].score > 0,
        "the actual holder receives kill credit"
    );
    assert!(
        s.package_diagnostics().is_empty(),
        "{:?}",
        s.package_diagnostics()
    );
}

fn physical_scene(
    catalog: Arc<Catalog>,
    kind: bri_sim::bot_kind::BotKind,
) -> (bri_sim::session::Session, u64) {
    use bri_sim::{
        player::MoveInput,
        session::{Command, MiniGameRequest, Session, ToolCatalog},
        simulation::Simulation,
    };
    use bri_world::{Brick, ContentRef, VehicleSpawn, World};
    use glam::Vec3;
    use rapier3d::prelude::*;
    let kind_id = kind.id.clone();
    let mut defs = bri_chaos::fixture::synthetic_definitions().unwrap();
    defs.entries.insert(
        "pool".into(),
        bri_sim::testing::definition(
            "pool",
            [96, 96],
            80,
            bri_sim::definitions::Special::Water,
            false,
        ),
    );
    defs.entries.insert(
        HOLE.into(),
        bri_sim::testing::definition(HOLE, [8, 8], 1, bri_sim::definitions::Special::None, true),
    );
    let mut world = World::new("Shark contact".into(), "test/map".into(), vec![[1.0; 4]]);
    world.bricks.insert(
        1,
        Brick::new(ContentRef::Resolved("pool".into()), [0.0, 8.2, 0.0], 1),
    );
    let mut hole = Brick::new(ContentRef::Resolved(HOLE.into()), [0.0, 0.1, 10.0], 1);
    hole.vehicle = Some(Box::new(VehicleSpawn {
        vehicle: ContentRef::Resolved(kind_id.clone()),
        recolor: false,
    }));
    world.bricks.insert(2, hole);
    world.next_brick_id = 3;
    let floor = ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0));
    let empty = World::new("Shark contact".into(), "test/map".into(), vec![[1.0; 4]]);
    let mut s = Session::new(Simulation::new(empty, defs, vec![floor]).unwrap());
    s.install_packages(catalog, None).unwrap();
    s.set_vehicle_pack(bri_vehicles::testing::pack(), vec![kind])
        .unwrap();
    s.set_tool_catalog(ToolCatalog {
        vehicles: [kind_id].into(),
        vehicle_bricks: [HOLE.to_string()].into(),
        ..Default::default()
    })
    .unwrap();
    s.set_spawn_points(vec![Vec3::new(0.0, 0.05, -10.0)])
        .unwrap();
    let human = s
        .join("Swimmer".into(), Vec3::new(0.0, 0.05, -10.0), true)
        .unwrap();
    assert_eq!(human, 1, "spawn brick belongs to the actual creator");
    s.command(
        human,
        1,
        Command::LoadBuild {
            build: Box::new(bri_world::build::SavedBuild::new(world)),
            ownership: false,
        },
    )
    .unwrap();
    for sequence in 1..=10 {
        s.movement(human, sequence, MoveInput::default()).unwrap();
        s.step().unwrap();
    }
    s.command(
        human,
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
    (s, human)
}

/// Reads generated original content only; no original models/scripts enter Git.
#[test]
#[ignore = "needs refreshed Bot_Shark and its companion; set BRI_CONTENT"]
fn the_imported_shark_approaches_and_uses_its_actual_mouth_node() {
    let root = bri_chaos::fixture::content_root().expect("set BRI_CONTENT to generated content");
    let addons = root.join("addons");
    let mut set = PackageSet::base();
    set.packages
        .extend(["bot_hole", "bot_shark", PACKAGE].map(|id| PackageEntry {
            id: id.into(),
            version: "1.0.0".into(),
            side: if id == PACKAGE {
                Side::Server
            } else {
                Side::Shared
            },
            dir: format!("addons/{id}"),
            role: None,
        }));
    let cat = Arc::new(Catalog::load(&root, &set, true).unwrap_or_else(|e| panic!("{e:#?}")));
    let pack = bri_sim::bot_kind::BotPack::from_json(
        &std::fs::read(addons.join("bot_shark/assets/bots.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        pack.bots.len(),
        1,
        "helper buoyancy archetypes are not extra spawn kinds"
    );
    let kind = pack.bots.into_iter().next().unwrap();
    assert_eq!(
        kind.melee.as_ref().unwrap().reach,
        2.5,
        "fresh native-size policy retains ordinary bite reach"
    );
    physical_capture(cat, kind, 8);
}

#[test]
fn declared_companion_can_rest_its_brick_kind_but_cannot_rest_a_foreign_kind() {
    use bri_sim::{
        player::MoveInput,
        session::{Command, PackageArg, PackageCommand},
    };
    for own in [true, false] {
        let mut kind = bri_sim::bot_kind::BotPack::from_json(include_bytes!(
            "../../../packages/blockhead_bot/assets/bots.json"
        ))
        .unwrap()
        .bots
        .remove(0);
        kind.id = if own {
            "bot_shark:bot/sharkholebot"
        } else {
            "unfamiliar:bot/visitor"
        }
        .into();
        kind.body = Some("bot_shark:archetype/sharkholebot".into());
        kind.moves = bri_sim::bot_kind::Moves::Swim;
        let (mut s, human) = physical_scene(catalog(), kind);
        for sequence in 11..=60 {
            s.movement(human, sequence, MoveInput::default()).unwrap();
            s.step().unwrap();
        }
        let bot = s.names().keys().copied().find(|id| s.is_bot(*id)).unwrap();
        let before = s.bot_thoughts().into_iter().find(|b| b.bot == bot).unwrap();
        let result = s.command(
            human,
            3,
            Command::Package(PackageCommand {
                package: PACKAGE.into(),
                command: "rest_probe".into(),
                args: vec![PackageArg::Int(bot as i64), PackageArg::Bool(true)],
            }),
        );
        // Command dispatch accepts a valid script call; operation admission
        // reports failures independently without applying the rejected op.
        result.unwrap();
        if own {
            s.step().unwrap();
            let after = s.bot_thoughts().into_iter().find(|b| b.bot == bot).unwrap();
            assert!(after.goal.is_none() && after.visible.is_none());
            assert!(
                s.package_diagnostics().is_empty(),
                "{:?}",
                s.package_diagnostics()
            );
        } else {
            assert!(
                s.package_diagnostics()
                    .iter()
                    .any(|d| d.code == "op.failed" && d.message.contains("not owned")),
                "{:?}",
                s.package_diagnostics()
            );
            let after = s.bot_thoughts().into_iter().find(|b| b.bot == bot).unwrap();
            assert_eq!(after.goal, before.goal);
            assert_eq!(after.visible, before.visible);
        }
    }
}

#[test]
fn a_foreign_bot_borrowing_the_shark_body_cannot_capture_or_initialize() {
    let mut p = Policy::new();
    p.snapshot.bots[0].bot_kind = "unfamiliar:bot/borrower".into();
    assert!(!p.capture().iter().any(|o| matches!(o, Op::MountObject(_))));
    let ops = p.call("on_tick", vec![], true);
    assert!(
        !ops.iter()
            .any(|o| matches!(o, Op::RestBot(_) | Op::SetArchetype(_)))
    );
    assert_eq!(p.state.global["grabs"], json!({}));
    assert_eq!(p.state.global["known"], json!({}));
}

#[test]
fn a_smaller_shark_body_remains_a_valid_victim_without_owning_its_kind() {
    let mut p = Policy::new();
    let mut victim = p.snapshot.players.remove(0);
    victim.bot = true;
    victim.model = "bot_shark:asset/shark.dts".into();
    victim.bot_kind = "unfamiliar:bot/small".into();
    victim.scale = 0.4;
    p.snapshot.bots.push(victim);
    let ops = p.capture();
    assert!(
        ops.iter()
            .any(|o| matches!(o, Op::MountObject(m) if m.node == 3))
    );
    assert!(ops.iter().any(|o| matches!(o, Op::PlayThread(t) if t.player == 1 && t.thread == 1 && t.sequence == "biteReady")));
    assert!(ops.iter().any(|o| matches!(o, Op::PlayThread(t) if t.player == 2 && t.thread == 0 && t.sequence == "biteFix")));
    assert!(
        !ops.iter()
            .any(|o| matches!(o, Op::PlayThread(t) if t.thread == 2))
    );
    p.snapshot.bots[1].mount = Some(1);
    p.snapshot.bots[1].mounted = true;
    let ops = p.call("on_leave", vec![2_i64.into()], true);
    assert!(ops.iter().any(
        |o| matches!(o, Op::PlayThread(t) if t.player == 1 && t.thread == 1 && t.sequence == "root")
    ));
    assert!(ops.iter().any(
        |o| matches!(o, Op::PlayThread(t) if t.player == 2 && t.thread == 0 && t.sequence == "root")
    ));
    assert!(
        !ops.iter()
            .any(|o| matches!(o, Op::PlayThread(t) if t.thread == 2))
    );
    assert!(
        ops.iter()
            .any(|o| matches!(o, Op::UnmountObject(u) if u.rider == 2))
    );
    assert_eq!(p.state.global["grabs"], json!({}));
}

#[test]
fn an_occupied_holder_or_a_target_carrying_riders_does_not_queue_partial_capture() {
    for parent in [1, 2] {
        let mut p = Policy::new();
        let mut rider = actor(3, false);
        rider.mount = Some(parent);
        rider.mounted = true;
        p.snapshot.players.push(rider);
        assert!(
            !p.capture()
                .iter()
                .any(|o| matches!(o, Op::MountObject(_) | Op::RestBot(_) | Op::OrbitCamera(_)))
        );
        assert_eq!(p.state.global["grabs"], json!({}));
    }
}

#[test]
fn body_contact_uses_the_authored_square_footprint_including_corners() {
    let mut p = Policy::new();
    p.snapshot.players[0].position = [2.35, 0.0, 2.35];
    assert!(p.capture().iter().any(|o| matches!(o, Op::MountObject(_))));
    let mut p = Policy::new();
    p.snapshot.players[0].position = [2.5, 0.0, 0.0];
    assert!(!p.capture().iter().any(|o| matches!(o, Op::MountObject(_))));
}

#[test]
fn only_a_definition_provider_or_its_declared_companion_can_hide_untrusted_bricks() {
    use bri_sim::{session::Session, simulation::Simulation};
    use bri_world::{Brick, ContentRef, World};
    for (declared, own) in [(true, true), (false, true), (true, false)] {
        let kind = if own { HOLE } else { "unfamiliar:brick/panel" };
        let mut defs = bri_chaos::fixture::synthetic_definitions().unwrap();
        defs.entries.insert(
            kind.into(),
            bri_sim::testing::definition(
                kind,
                [1, 1],
                1,
                bri_sim::definitions::Special::None,
                false,
            ),
        );
        let mut world = World::new(
            "Definition policy".into(),
            "test/map".into(),
            vec![[1.0; 4]],
        );
        world.bricks.insert(
            1,
            Brick::new(ContentRef::Resolved(kind.into()), [0.25, 0.1, 0.25], 9),
        );
        world.next_brick_id = 2;
        let mut s = Session::new(Simulation::new(world, defs, Vec::new()).unwrap());
        // A wildcard subscriber is test-only: force the same production
        // setter against foreign definitions without manufacturing a caller.
        s.install_packages(catalog_fixture(declared, true), None)
            .unwrap();
        s.step().unwrap();
        let b = &s.simulation().state().bricks[&1];
        if declared && own {
            assert!(!b.visible && !b.colliding && !b.raycast);
            assert!(
                s.package_diagnostics().is_empty(),
                "{:?}",
                s.package_diagnostics()
            );
        } else {
            assert!(b.visible && b.colliding && b.raycast);
            assert!(
                s.package_diagnostics()
                    .iter()
                    .any(|d| d.code == "op.failed"),
                "{:?}",
                s.package_diagnostics()
            );
        }
    }
}

#[test]
fn sandbox_foreign_game_mounted_or_nonphysical_targets_do_not_capture() {
    for scenario in 0..5 {
        let mut p = Policy::new();
        match scenario {
            0 => {
                p.snapshot.bots[0].minigame = None;
                p.snapshot.players[0].minigame = None;
            }
            1 => p.snapshot.players[0].minigame = Some(8),
            2 => p.snapshot.players[0].mounted = true,
            3 => p.snapshot.players[0].position = [20.0, 0.0, 0.0],
            _ => p.snapshot.players[0].model = "unfamiliar:asset/creature".into(),
        }
        assert!(
            !p.capture().iter().any(|o| matches!(o, Op::MountObject(_))),
            "scenario {scenario}"
        );
        assert_eq!(p.state.global["grabs"], json!({}), "scenario {scenario}");
    }
}
