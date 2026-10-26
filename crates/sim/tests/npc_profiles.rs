//! Bot behaviour profiles, intents, abilities and per-body reach, played
//! through the authoritative session with a made-up Add-On: its rules add
//! bots of kinds it declares as data and give them goals (`bot_intent`),
//! and its `on_bot_ability` hook says what a kind's ability does. The
//! shared brain carries every goal and attack out; nothing here is a brain
//! loop of the Add-On's own (`docs/architecture/bots.md`, Profiles,
//! Intents, Attacks). The controller bodies are the samples'
//! (`packages/samples`): the Wall Kicker, the Diver and the Trooper.
use bri_minigames::Settings;
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::Definitions,
    session::{Command, MiniGameRequest, PackageArg, PackageCommand, Session},
    simulation::Simulation,
};
use bri_world::{OwnerId, World};
use glam::Vec3;
use rapier3d::prelude::*;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    sync::atomic::{AtomicUsize, Ordering},
};

const SCRIPT: &str = r#"
fn cmd_add(p, kind, name) { add_bot(player(p).minigame, #{ kind: kind, name: name }); }
fn cmd_guard(p, bot, x, z, r) { bot_intent(bot, #{ guard: [x, 0.0, z], radius: r }); }
fn cmd_patrol(p, bot, ax, az, bx, bz) {
    bot_intent(bot, #{ patrol: [[ax, 0.0, az], [bx, 0.0, bz]], radius: 6.0 });
}
fn cmd_follow(p, bot, leader) { bot_intent(bot, #{ follow: leader, distance: 3.0 }); }
fn cmd_flee(p, bot, from) { bot_intent(bot, #{ flee: from, seconds: 6.0 }); }
fn cmd_clear(p, bot) { bot_intent(bot, ()); }
fn cmd_hurt(p, bot, amount) { damage(bot, amount, p); }
fn cmd_pair(p, a, b) {
    let game = minigames()[0].id;
    set_teams(game, [#{ name: "Red", color: 0 }], #{ friendly_fire: true });
    set_team(a, 1);
    set_team(b, 1);
}
fn on_bot_spotted(bot, enemy) {
    set("spotted", `${get("spotted")}${bot}>${enemy};`);
}
fn on_bot_friend_died(bot, friend) {
    set("friend", `${get("friend")}${bot}:${friend};`);
}
fn on_bot_lost_enemy(bot, enemy) {
    set("lost", `${get("lost")}${bot}:${enemy};`);
}
fn on_bot_sees_grenade(bot, grenade) {
    set("grenade", `${get("grenade")}${bot};`);
}
fn on_bot_ability(bot, kind, ability, target) {
    set("ability", `${kind}|${ability}|${target}`);
    if target != () { damage(target, 25.0, bot); }
}
"#;

fn behaviour() -> Value {
    let command = |name: &str, args: &[&str]| json!({ "name": name, "args": args });
    json!({
        "schema_version": 1,
        "script": "main.rhai",
        "on_bot_ability": true,
        "on_bot_spotted": true,
        "on_bot_friend_died": true,
        "on_bot_lost_enemy": true,
        "on_bot_sees_grenade": true,
        "commands": [
            command("add", &["string", "string"]),
            command("guard", &["int", "float", "float", "float"]),
            command("patrol", &["int", "float", "float", "float", "float"]),
            command("follow", &["int", "int"]),
            command("flee", &["int", "int"]),
            command("clear", &["int"]),
            command("hurt", &["int", "float"]),
            command("pair", &["int", "int"]),
        ],
        "state": { "global": {
            "ability": { "default": "", "visible": "everyone" },
            "spotted": { "default": "", "visible": "everyone" },
            "friend": { "default": "", "visible": "everyone" },
            "lost": { "default": "", "visible": "everyone" },
            "grenade": { "default": "", "visible": "everyone" }
        } }
    })
}

/// A body that cannot jump: its only climb is its step.
const LEGS: &str = "probe:archetype/legs";
const KICKER: &str = "sample-wall-kick:archetype/kicker";
const DIVER: &str = "sample-dive:archetype/diver";
const TROOPER: &str = "sample-trooper:archetype/trooper";

/// The kinds, as an Add-On's `bots.json` declares them: one side, so they
/// never fight each other, and nearly everything off.
fn kinds() -> Vec<bri_sim::bot_kind::BotKind> {
    let minimal = |id: &str, body: Option<&str>, extra: Value| {
        let mut kind = json!({
            "id": id,
            "name": id,
            "side": "probe",
            "sight": 12,
            "wander_radius": 4,
            "weapons": false,
            "tactics": { "cover": false, "retreat": false, "flank": false, "dodge": false },
            "vehicles": { "drive": false, "gun": false, "ride": false },
            "behaviours": { "arm": 0, "carry": 0, "interact": 0, "objective": 0, "explore": 0, "fly": 0 },
            "surprise": { "strength": 0 },
            "team": { "teamwork": 0 },
            "perception": { "strength": 0 },
            "first_names": ["Probe"]
        });
        if let Some(body) = body {
            kind["body"] = body.into();
        }
        for (k, v) in extra.as_object().unwrap() {
            kind[k] = v.clone();
        }
        kind
    };
    let pack = json!({
        "schema_version": 1,
        "bots": [
            minimal("probe:bot/walker", None, json!({})),
            minimal("probe:bot/legs", Some(LEGS), json!({})),
            minimal("probe:bot/kicker", Some(KICKER), json!({})),
            minimal("probe:bot/diver", Some(DIVER), json!({})),
            minimal("probe:bot/trooper", Some(TROOPER), json!({})),
            minimal("probe:bot/biter", None, json!({
                "reaction_seconds": 0.2,
                "ability": { "name": "Bite", "range": 2.5, "fuse_seconds": 0.5, "cancel_range": 6 }
            })),
            minimal("probe:bot/prickle", None, json!({
                "contact": { "damage": 5, "seconds": 0.5, "name": "Prick" }
            })),
            minimal("probe:bot/coward", None, json!({ "flee_below": 0.5 }))
        ]
    });
    bri_sim::bot_kind::BotPack::from_json(&serde_json::to_vec(&pack).unwrap())
        .unwrap_or_else(|e| panic!("{e:#}"))
        .bots
}

struct Root(PathBuf);
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let target = to.join(path.file_name().unwrap());
        if path.is_dir() {
            copy(&path, &target);
        } else {
            std::fs::copy(&path, &target).unwrap();
        }
    }
}
/// The probe Add-On beside the three controller samples.
fn catalog() -> Arc<Catalog> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = Root(std::env::temp_dir().join(format!(
        "bri-npc-profiles-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    let dir = root.0.join("probe");
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = json!({
        "schema_version": 1, "id": "probe", "version": "1.0.0", "api": 1,
        "name": "probe", "license": "CC0-1.0",
        "capabilities": ["damage", "minigame", "bots"],
        "provides": [
            { "kind": "behaviour", "id": "probe:behaviour/main", "file": "behaviour.json" },
            { "kind": "script", "id": "probe:script/main", "file": "main.rhai" },
            { "kind": "archetype", "id": LEGS, "file": "legs.json" }
        ]
    });
    std::fs::write(dir.join("package.json"), manifest.to_string()).unwrap();
    std::fs::write(dir.join("behaviour.json"), behaviour().to_string()).unwrap();
    std::fs::write(dir.join("main.rhai"), SCRIPT).unwrap();
    let legs = json!({
        "schema_version": 1, "name": "Legs",
        "movement": { "can_jet": false, "jump_speed": 0 }
    });
    std::fs::write(dir.join("legs.json"), legs.to_string()).unwrap();
    let samples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/samples");
    let ids = ["probe", "sample-wall-kick", "sample-dive", "sample-trooper"];
    for id in &ids[1..] {
        copy(&samples.join(id), &root.0.join(id));
    }
    let set = PackageSet {
        schema_version: 1,
        packages: ids
            .iter()
            .map(|id| PackageEntry {
                id: (*id).into(),
                version: "1.0.0".into(),
                side: Side::Server,
                dir: (*id).into(),
                role: None,
            })
            .collect(),
    };
    Arc::new(Catalog::load(&root.0, &set, true).unwrap_or_else(|e| panic!("{e:#?}")))
}

struct Game {
    s: Session,
    seq: u64,
    me: OwnerId,
}
impl Game {
    /// Flat ground (and `extra` colliders), the probe installed, and a
    /// player at `at` running a mini-game for the bots to join (where
    /// nobody can hurt anybody unless `harm`); bots come in at `spawn`. The
    /// player's spawn protection has run out.
    fn new(extra: Vec<ColliderBuilder>, at: Vec3, spawn: Vec3) -> Self {
        Self::with(extra, at, spawn, true)
    }
    fn peaceful(extra: Vec<ColliderBuilder>, at: Vec3, spawn: Vec3) -> Self {
        Self::with(extra, at, spawn, false)
    }
    fn with(extra: Vec<ColliderBuilder>, at: Vec3, spawn: Vec3, harm: bool) -> Self {
        let mut map = vec![
            ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0)),
        ];
        map.extend(extra);
        let map = map.into_iter().map(|c| c.user_data(u128::MAX)).collect();
        let mut s = Session::new(
            Simulation::new(
                World::new("Probe".into(), "probe".into(), vec![[1.0; 4]]),
                Definitions::default(),
                map,
            )
            .unwrap(),
        );
        s.install_packages(catalog(), None).unwrap();
        s.set_bot_kinds(kinds()).unwrap();
        let me = s.join("P".into(), at, true).unwrap();
        let mut g = Self { s, seq: 0, me };
        let settings = Settings {
            loadout: Default::default(),
            self_damage: false,
            weapon_damage: harm,
            ..Settings::default()
        };
        g.send(Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings,
        }))
        .unwrap();
        g.s.set_spawn_points(vec![spawn]).unwrap();
        g.steps(600);
        g
    }
    fn send(&mut self, command: Command) -> anyhow::Result<()> {
        self.seq += 1;
        self.s.command(self.me, self.seq, command).map(drop)
    }
    fn run(&mut self, command: &str, args: Vec<PackageArg>) {
        self.send(Command::Package(PackageCommand {
            package: "probe".into(),
            command: command.into(),
            args,
        }))
        .unwrap_or_else(|e| panic!("{command}: {e:#}"));
        self.steps(1);
        assert!(
            self.s.package_diagnostics().is_empty(),
            "{command}: {:#?}",
            self.s.package_diagnostics()
        );
    }
    /// Adds a bot of `kind` and gives back its id.
    fn add(&mut self, kind: &str, name: &str) -> OwnerId {
        self.run(
            "add",
            vec![
                PackageArg::String(kind.into()),
                PackageArg::String(name.into()),
            ],
        );
        self.steps(2);
        self.s
            .names()
            .into_iter()
            .find(|(o, n)| n == name && self.s.is_bot(*o))
            .unwrap_or_else(|| panic!("no bot {name}: {:#?}", self.s.package_diagnostics()))
            .0
    }
    fn steps(&mut self, n: usize) {
        for _ in 0..n {
            self.s.step().unwrap();
        }
    }
    fn feet(&self, owner: OwnerId) -> Vec3 {
        self.s
            .motion_states()
            .into_iter()
            .find(|(p, _)| p.owner == owner)
            .unwrap()
            .0
            .feet
            .into()
    }
    fn body(&self, owner: OwnerId) -> String {
        let state = self
            .s
            .motion_states()
            .into_iter()
            .find(|(p, _)| p.owner == owner)
            .unwrap()
            .0;
        self.s.archetypes().resolve(state.archetype).id.clone()
    }
    fn health(&self, owner: OwnerId) -> f32 {
        let v = &self.s.vitals()[&owner];
        if v.alive { v.health } else { 0.0 }
    }
    fn text(&self, key: &str) -> String {
        self.s
            .package_state()
            .packages
            .get("probe")
            .and_then(|ns| ns.global.get(key).cloned())
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default()
    }
}
fn int(v: OwnerId) -> PackageArg {
    PackageArg::Int(v as i64)
}
fn float(v: f32) -> PackageArg {
    PackageArg::Float(v.into())
}
fn flat(v: Vec3) -> f32 {
    Vec3::new(v.x, 0.0, v.z).length()
}

/// Far from the bots, out of their sight.
const AWAY: Vec3 = Vec3::new(-70.0, 0.05, -70.0);

/// A guard walks to its spot and keeps to it; a patrol walks its round
/// from point to point; a follower keeps by its leader as it goes; and
/// with its intent cleared a bot is its kind again. The Add-On only says
/// what; the walking is the brain's.
#[test]
fn intents_guard_patrol_and_follow_are_carried_out_by_the_brain() {
    let mut g = Game::peaceful(vec![], AWAY, Vec3::new(0.0, 0.05, 0.0));
    let guard = g.add("probe:bot/walker", "Guard");
    let patrol = g.add("probe:bot/walker", "Patrol");
    let follower = g.add("probe:bot/walker", "Follower");
    let post = Vec3::new(20.0, 0.0, 10.0);
    g.run(
        "guard",
        vec![int(guard), float(post.x), float(post.z), float(3.0)],
    );
    let (a, b) = (Vec3::new(-12.0, 0.0, -12.0), Vec3::new(12.0, 0.0, -12.0));
    g.run(
        "patrol",
        vec![int(patrol), float(a.x), float(a.z), float(b.x), float(b.z)],
    );
    g.run("follow", vec![int(follower), int(patrol)]);

    let (mut at_a, mut at_b) = (f32::MAX, f32::MAX);
    let mut guard_far = 0.0f32;
    let mut follow_far = 0.0f32;
    for second in 0..60 {
        g.steps(120);
        let p = g.feet(patrol);
        at_a = at_a.min(flat(p - a));
        at_b = at_b.min(flat(p - b));
        // Settled after the first stretch: the guard on its post, the
        // follower caught up.
        if second >= 20 {
            guard_far = guard_far.max(flat(g.feet(guard) - post));
            follow_far = follow_far.max(flat(g.feet(follower) - p));
        }
    }
    assert!(guard_far < 3.0, "the guard kept to its post: {guard_far}");
    assert!(
        at_a < 2.0 && at_b < 2.0,
        "the patrol walked to both points: {at_a} {at_b}"
    );
    assert!(
        follow_far < 8.0,
        "the follower kept by its leader: {follow_far}"
    );

    // Cleared, the guard goes back to strolling where it is.
    g.run("clear", vec![int(guard)]);
    g.steps(120);
}

/// Explicit guard orders remain a combat boundary, unlike a spawn home.
#[test]
fn an_explicit_guard_radius_still_limits_pursuit_and_clearing_it_allows_attack() {
    let mut g = Game::new(
        vec![],
        Vec3::new(0.0, 0.05, 10.0),
        Vec3::new(0.0, 0.05, 0.0),
    );
    // Minigame creation resets members at the configured spawn; establish
    // the enemy's actual position after that transition.
    g.send(Command::DropPlayerAtCamera(Some(
        bri_sim::session::CameraView {
            eye: [0.0, 1.65, 10.0],
            yaw: 0.0,
            pitch: 0.0,
        },
    )))
    .unwrap();
    let bot = g.add("probe:bot/biter", "Guard");
    assert!(
        flat(g.feet(g.me)) > 9.0,
        "enemy starts outside the guard post"
    );
    g.run("guard", vec![int(bot), float(0.0), float(0.0), float(3.0)]);
    let mut far = 0.0f32;
    for _ in 0..120 * 8 {
        g.steps(1);
        far = far.max(flat(g.feet(bot)));
    }
    assert_eq!(
        g.health(g.me),
        100.0,
        "guard must not abandon its post to bite"
    );
    assert!(far < 5.0, "guard exceeded its post boundary: {far}");
    g.run("clear", vec![int(bot)]);
    for _ in 0..120 * 8 {
        g.steps(1);
        if g.health(g.me) < 100.0 {
            break;
        }
    }
    assert!(
        g.health(g.me) < 100.0,
        "clearing the order permits an ordinary chase and bite"
    );
    assert_eq!(g.s.bot_step_failures().0, 0);
}

/// A flee intent sends a bot running from the player it names, and a kind
/// with `flee_below` runs on its own once hurt that far.
#[test]
fn a_bot_flees_from_who_its_intent_names_and_when_hurt() {
    let mut g = Game::new(vec![], Vec3::new(0.0, 0.05, 0.0), Vec3::new(3.0, 0.05, 0.0));
    let bot = g.add("probe:bot/walker", "Runner");
    g.steps(60);
    let start = flat(g.feet(bot) - g.feet(g.me));
    let me = g.me;
    g.run("flee", vec![int(bot), int(me)]);
    g.steps(4 * 120);
    let fled = flat(g.feet(bot) - g.feet(g.me));
    assert!(fled > start + 8.0, "it ran away: {start} to {fled}");

    // Hurt below half, the coward runs from the player it sees.
    let coward = g.add("probe:bot/coward", "Coward");
    // Past its spawn protection.
    g.steps(600);
    let start = flat(g.feet(coward) - g.feet(g.me));
    g.run("hurt", vec![int(coward), float(60.0)]);
    assert!(g.health(coward) <= 50.0, "hurt: {}", g.health(coward));
    g.steps(4 * 120);
    let fled = flat(g.feet(coward) - g.feet(g.me));
    assert!(fled > start + 8.0, "hurt, it ran away: {start} to {fled}");
}

/// A kind's ability: the brain walks into its range, winds it up for its
/// fuse and calls the Add-On's `on_bot_ability` with the target, which
/// decides what happens (here a bite). A kind that hurts on touch hurts
/// whoever it walks into. Neither holds a weapon.
#[test]
fn an_ability_runs_the_add_ons_hook_and_contact_hurts_on_touch() {
    let mut g = Game::new(vec![], Vec3::new(0.0, 0.05, 0.0), Vec3::new(6.0, 0.05, 0.0));
    let biter = g.add("probe:bot/biter", "Biter");
    let mut bitten = false;
    for _ in 0..20 {
        g.steps(120);
        if !g.text("ability").is_empty() {
            bitten = true;
            break;
        }
    }
    assert!(bitten, "the ability went off within 20 seconds");
    assert_eq!(
        g.text("ability"),
        format!("probe:bot/biter|Bite|{}", g.me),
        "the hook heard the kind, the ability and the target"
    );
    assert!(g.health(g.me) < 100.0, "the Add-On's bite hurt");
    assert!(g.s.vitals()[&biter].alive);
    assert_eq!(
        g.text("spotted"),
        format!("{biter}>{};", g.me),
        "`on_bot_spotted` fired once, when it took the player as its target"
    );

    let mut g = Game::new(vec![], Vec3::new(0.0, 0.05, 0.0), Vec3::new(6.0, 0.05, 0.0));
    g.add("probe:bot/prickle", "Prickle");
    let mut pricked = false;
    for _ in 0..20 {
        g.steps(120);
        if g.health(g.me) < 100.0 {
            pricked = true;
            break;
        }
    }
    assert!(pricked, "touching it hurt within 20 seconds");
    assert!(g.text("ability").is_empty(), "no ability of its own");
}

/// A bot sees an ally die and the rules hear it once; a bot whose enemy
/// goes out of its sight loses them, and the rules hear that once too.
#[test]
fn bots_tell_the_rules_of_an_ally_dying_and_an_enemy_lost() {
    let mut g = Game::new(vec![], Vec3::new(0.0, 0.05, 0.0), Vec3::new(6.0, 0.05, 0.0));
    let watcher = g.add("probe:bot/walker", "Watcher");
    let victim = g.add("probe:bot/walker", "Victim");
    // Past their spawn protection.
    g.steps(600);
    g.run("pair", vec![int(watcher), int(victim)]);
    g.run("hurt", vec![int(victim), float(100.0)]);
    g.steps(60);
    assert_eq!(
        g.text("friend"),
        format!("{watcher}:{victim};"),
        "the watcher saw its ally die, once"
    );
    assert!(
        g.text("spotted").contains(&format!("{watcher}>{};", g.me)),
        "the watcher had the player in sight: {}",
        g.text("spotted")
    );
    // The player moves out of its sight.
    g.send(Command::DropPlayerAtCamera(Some(
        bri_sim::session::CameraView {
            eye: [AWAY.x, 1.65, AWAY.z],
            yaw: 0.0,
            pitch: 0.0,
        },
    )))
    .unwrap();
    g.steps(240);
    assert_eq!(
        g.text("lost"),
        format!("{watcher}:{};", g.me),
        "the watcher lost the player, once"
    );
}

/// Each body plans with its own reach, measured with its own motor and
/// controller: one that cannot jump walks around a low wall to its post,
/// while the samples' Wall Kicker, Diver and Trooper jump it.
#[test]
fn each_body_plans_with_its_own_reach() {
    // A wall 1.1 high from z = -8 to 8 at x = 5; the way around is past
    // its ends.
    let wall = ColliderBuilder::cuboid(0.25, 0.55, 8.0).translation(Vector::new(5.0, 0.55, 0.0));
    let post = Vec3::new(10.0, 0.0, 0.0);
    for (kind, body, jumps) in [
        ("probe:bot/legs", LEGS, false),
        ("probe:bot/kicker", KICKER, true),
        ("probe:bot/diver", DIVER, true),
        ("probe:bot/trooper", TROOPER, true),
    ] {
        let mut g = Game::peaceful(vec![wall.clone()], AWAY, Vec3::new(0.0, 0.05, 0.0));
        let bot = g.add(kind, "Climber");
        assert_eq!(g.body(bot), body, "{kind} plays in its body");
        g.run(
            "guard",
            vec![int(bot), float(post.x), float(post.z), float(2.0)],
        );
        let mut widest = 0.0f32;
        let mut arrived = None;
        for second in 0..60 {
            g.steps(120);
            let feet = g.feet(bot);
            widest = widest.max(feet.z.abs());
            if flat(feet - post) < 2.5 {
                arrived = Some(second);
                break;
            }
        }
        assert!(
            arrived.is_some(),
            "{kind} reached its post: {:?}",
            g.feet(bot)
        );
        if jumps {
            assert!(widest < 6.0, "{kind} went over the wall: out to {widest}");
        } else {
            assert!(widest > 8.0, "{kind} went around the wall: out to {widest}");
        }
    }
}

/// Minimal-profile creatures count a quarter of a full bot against the
/// server's bot limit (`BotKind::cost`): a server limited to 16 bots runs
/// 20 of them.
#[test]
fn minimal_creatures_have_their_own_cost_derived_limit() {
    let mut g = Game::new(vec![], AWAY, Vec3::new(0.0, 0.05, 0.0));
    assert_eq!(g.s.bot_limit(), 16);
    for i in 0..20 {
        g.add("probe:bot/walker", &format!("Walker {i}"));
    }
    let bots = g.s.vitals().keys().filter(|o| g.s.is_bot(**o)).count();
    assert_eq!(bots, 20);
}
