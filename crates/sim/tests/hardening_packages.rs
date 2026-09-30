//! Hardening: mod-package behaviour inside the authoritative session.
//!
//! Every test writes its own small hostile package set into a temporary
//! directory, installs it into a synthetic session (flat ground, no original
//! game content) and drives it through the same `Session::command*` /
//! `Session::step` entry points the host uses, so each failure replays
//! request by request.
//!
//! Tests that expose a defect are `#[ignore = "finding H2-Fn: ..."]` and fail
//! when run with `--ignored`; the others record checks that held. Timing
//! limits are generous (debug builds); where a limit is a policy choice the
//! doc comment says what budget the test assumes.
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::{Catalog, PlayerKey, Store};
use bri_sim::{
    definitions::{Definition, Definitions},
    session::{
        ActionAim, Command, Notice, PackageArg, PackageCommand, PackageSave, Reply, Session,
    },
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
    time::Duration,
};

// ---------------------------------------------------------------- fixtures

const CUBE: &str = "v20/brick/brick4xcubedata";

fn definition(id: &str, studs: [u32; 2], plates: u32, size: [f32; 3]) -> (String, Definition) {
    let mesh = Mesh {
        schema_version: 1,
        id: id.into(),
        footprint_studs: studs,
        height_plates: plates,
        attachment_rows: vec!["b".repeat(studs[0] as usize); (studs[1] * plates) as usize],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![],
    };
    let collision = CollisionBody {
        id: id.into(),
        parts: vec![Part::Box {
            center: [0.0; 3],
            size,
        }],
    };
    let shape = bri_physics::content::collider(&collision)
        .unwrap()
        .build()
        .shared_shape()
        .clone();
    (
        id.into(),
        Definition {
            mesh,
            collision,
            shape,
            indestructible: false,
            special: Default::default(),
            reflection: None,
        },
    )
}
/// A 2x1 plate players can plant, and the 2-unit cube voxel brick.
fn definitions() -> Definitions {
    Definitions {
        entries: [
            definition("plate", [2, 1], 1, [1.0, 0.2, 0.5]),
            definition(CUBE, [4, 4], 10, [2.0, 2.0, 2.0]),
        ]
        .into(),
    }
}

/// A package directory written for one test and removed afterwards.
struct Root(PathBuf);
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn temp_root() -> Root {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "bri-hardening-packages-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    Root(dir)
}

/// One mod package: id, side, capabilities and provided files.
struct Spec {
    id: &'static str,
    side: Side,
    capabilities: Vec<&'static str>,
    provides: Vec<(&'static str, String, String, Vec<u8>)>,
    dependencies: Vec<&'static str>,
}
impl Spec {
    /// A server package with a behaviour and script.
    fn server(
        id: &'static str,
        capabilities: &[&'static str],
        behaviour: Value,
        script: &str,
    ) -> Self {
        let mut behaviour = behaviour;
        behaviour["schema_version"] = json!(1);
        behaviour["script"] = json!("main.rhai");
        Self {
            id,
            side: Side::Server,
            capabilities: capabilities.to_vec(),
            provides: vec![
                (
                    "behaviour",
                    "main".into(),
                    "behaviour.json".into(),
                    serde_json::to_vec(&behaviour).unwrap(),
                ),
                ("script", "main".into(), "main.rhai".into(), script.into()),
            ],
            dependencies: Vec::new(),
        }
    }
    /// Add an entity kind `<id>:entity/<name>` drawn with the shared model.
    fn entity(mut self, name: &str, think_interval: u32, max_alive: u32) -> Self {
        self.provides.push((
            "entity",
            name.into(),
            format!("{name}.json"),
            serde_json::to_vec(&json!({
                "schema_version": 1, "name": name, "model": "hxmodel:model/body",
                "think": "think", "think_interval": think_interval,
                "scale": 0.5, "health": 10.0, "max_alive": max_alive
            }))
            .unwrap(),
        ));
        if !self.dependencies.contains(&"hxmodel") {
            self.dependencies.push("hxmodel");
        }
        self
    }
    fn world(mut self, world: Value) -> Self {
        self.provides.push((
            "world",
            "ground".into(),
            "world.json".into(),
            serde_json::to_vec(&world).unwrap(),
        ));
        self
    }
    fn model() -> Self {
        Self {
            id: "hxmodel",
            side: Side::Client,
            capabilities: Vec::new(),
            provides: vec![(
                "model",
                "body".into(),
                "body.json".into(),
                serde_json::to_vec(&json!({ "schema_version": 1,
                    "boxes": [{ "center": [0.0, 0.5, 0.0], "size": [0.5, 1.0, 0.5], "color": [0.2, 0.8, 0.2, 1.0] }] }))
                .unwrap(),
            )],
            dependencies: Vec::new(),
        }
    }
    fn write(&self, root: &Path) {
        let dir = root.join(self.id);
        std::fs::create_dir_all(&dir).unwrap();
        let provides: Vec<_> = self
            .provides
            .iter()
            .map(|(kind, name, file, _)| {
                json!({ "kind": kind, "id": format!("{}:{kind}/{name}", self.id), "file": file })
            })
            .collect();
        let dependencies: serde_json::Map<String, Value> = self
            .dependencies
            .iter()
            .map(|d| (d.to_string(), json!("^1.0.0")))
            .collect();
        let manifest = json!({
            "schema_version": 1, "id": self.id, "version": "1.0.0", "api": 1,
            "name": self.id, "license": "CC0-1.0",
            "capabilities": self.capabilities, "dependencies": dependencies,
            "provides": provides,
        });
        std::fs::write(
            dir.join("package.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        for (_, _, file, bytes) in &self.provides {
            std::fs::write(dir.join(file), bytes).unwrap();
        }
    }
}

fn catalog(specs: Vec<Spec>) -> Arc<Catalog> {
    let root = temp_root();
    let mut specs = specs;
    specs.push(Spec::model());
    for s in &specs {
        s.write(&root.0);
    }
    let set = PackageSet {
        schema_version: 1,
        packages: specs
            .iter()
            .map(|s| PackageEntry {
                id: s.id.into(),
                version: "1.0.0".into(),
                side: s.side,
                dir: s.id.into(),
                role: None,
            })
            .collect(),
    };
    Arc::new(Catalog::load(&root.0, &set, true).unwrap_or_else(|e| panic!("{e:#?}")))
}
/// Flat 200 x 200 ground at y = 0 with the packages installed.
fn session_with(specs: Vec<Spec>, save: Option<PackageSave>) -> Session {
    let world = World::new(
        "Hardening".into(),
        "hardening".into(),
        vec![[1.0; 4], [0.0; 4]],
    );
    let mut s = Session::new(
        Simulation::new(
            world,
            definitions(),
            vec![
                ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0)),
            ],
        )
        .unwrap(),
    );
    s.install_packages(catalog(specs), save)
        .unwrap_or_else(|e| panic!("{e:#}"));
    s
}
fn session(specs: Vec<Spec>) -> Session {
    session_with(specs, None)
}
const SPAWN: Vec3 = Vec3::new(0.0, 0.05, 0.0);
fn spawn_at(i: u64) -> Vec3 {
    Vec3::new(i as f32 * 3.0, 0.05, 6.0)
}

fn pkg(package: &str, command: &str, args: Vec<PackageArg>) -> Command {
    Command::Package(PackageCommand {
        package: package.into(),
        command: command.into(),
        args,
    })
}
fn code(error: &anyhow::Error) -> String {
    error
        .downcast_ref::<bri_package::diag::Rejected>()
        .map(|r| r.0.0[0].code.clone())
        .unwrap_or_else(|| format!("{error:#}"))
}
/// Sends commands with increasing sequence numbers per owner.
struct Client {
    owner: OwnerId,
    seq: u64,
}
impl Client {
    fn new(owner: OwnerId) -> Self {
        Self { owner, seq: 0 }
    }
    fn send(&mut self, s: &mut Session, command: Command) -> anyhow::Result<Reply> {
        self.seq += 1;
        s.command(self.owner, self.seq, command)
    }
    fn aim(&mut self, s: &mut Session, command: Command, aim: ActionAim) -> anyhow::Result<Reply> {
        self.seq += 1;
        s.command_with_aim(self.owner, self.seq, command, Some(aim))
    }
    fn run(&mut self, s: &mut Session, package: &str, command: &str) -> anyhow::Result<Reply> {
        self.send(s, pkg(package, command, vec![]))
    }
}
fn diag_codes(s: &Session) -> Vec<String> {
    s.package_diagnostics()
        .iter()
        .map(|d| d.code.clone())
        .collect()
}
fn global(s: &Session, package: &str, key: &str) -> Option<Value> {
    s.package_save()?
        .store
        .namespace(package)?
        .global
        .get(key)
        .cloned()
}
fn steps(s: &mut Session, n: usize) {
    for _ in 0..n {
        s.step().unwrap();
    }
}
/// The longest acceptable stall of one tick caused by package work: six
/// ticks at 120 Hz. Script dependencies build with opt-level 2 even in dev
/// profiles, so measured script speed is close to release. For reference,
/// a call using its whole budget measured 4 ms (Think, 100k operations),
/// 8 ms (Command, 200k), 16 ms (Tick, 400k) and 156 ms (Generate, 4M).
///
/// Times are this thread's CPU time, where package scripts run: the work
/// one tick costs. Wall-clock time also counts the time the OS gives other
/// processes, which under the gate's parallel test binaries doubled a
/// 12-24 ms tick past the bound with no change in the work done.
const STALL: Duration = Duration::from_millis(50);
fn timed(f: impl FnOnce()) -> Duration {
    let start = thread_cpu_time();
    f();
    thread_cpu_time().saturating_sub(start)
}
#[cfg(windows)]
fn thread_cpu_time() -> Duration {
    use windows_sys::Win32::{Foundation::FILETIME, System::Threading};
    let zero = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
    // SAFETY: the pseudo-handle of the current thread and four owned FILETIMEs.
    let ok = unsafe {
        Threading::GetThreadTimes(
            Threading::GetCurrentThread(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
    };
    assert_ne!(ok, 0, "GetThreadTimes failed");
    let ticks = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
    // FILETIME counts 100 ns intervals.
    Duration::from_nanos((ticks(kernel) + ticks(user)) * 100)
}
#[cfg(unix)]
fn thread_cpu_time() -> Duration {
    let mut t = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: an owned timespec for the current thread's CPU clock.
    let ok = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut t) };
    assert_eq!(ok, 0, "clock_gettime failed");
    Duration::new(t.tv_sec as u64, t.tv_nsec as u32)
}

// ----------------------------------------- 1. script work per tick and call

/// 256 entities whose `think` each spends most of the per-call Think budget
/// (legal calls, no errors), all due on the same tick. The session must keep
/// its tick budget: package work per tick is capped or spread (one tick
/// under [`STALL`]) and the package is
/// throttled, not every player slowed. Today every due think runs in the
/// same tick, each with its own 100 000-operation budget, with no aggregate.
#[test]
fn many_heavy_thinks_keep_the_tick_budget() {
    let mut s = session(vec![
        Spec::server(
            "swarm",
            &["entity"],
            json!({ "commands": [{ "name": "fill", "admin": true }] }),
            r#"
            fn cmd_fill(p) {
                for i in 0..256 {
                    spawn_entity("swarm:entity/bug", (i % 16) * 3.0 - 24.0, 0.1, (i / 16) * 3.0 - 24.0);
                }
            }
            fn think(me) { let n = 0; for i in 0..30000 { n += i; } }
            "#,
        )
        .entity("bug", 1, 256),
    ]);
    let mut admin = Client::new(s.join("Admin".into(), SPAWN, true).unwrap());
    admin.run(&mut s, "swarm", "fill").unwrap();
    assert_eq!(s.package_stats().entities, 256, "{:?}", diag_codes(&s));
    s.step().unwrap();
    assert!(
        !diag_codes(&s).contains(&"script.budget".to_string()),
        "each think must be a legal call"
    );
    let took = timed(|| s.step().unwrap());
    assert!(took < STALL, "one tick of package thinking took {took:?}");
}

/// A world generator that spends most of its (legal) Generate budget per
/// chunk, while a player stands where no chunk exists yet. Streaming is one
/// chunk per tick, so the Generate budget is sized to fit a tick (it was
/// 4 000 000 operations, ~156 ms, when this was finding H2-F6): the tick
/// stays under [`STALL`].
#[test]
fn chunk_generation_keeps_the_tick_budget() {
    let mut s = session(vec![
        Spec::server(
            "terra",
            &[],
            json!({}),
            r#"
            fn gen(cx, cz) {
                // Near the origin: cheap, so install stays fast.
                if cx.abs() > 1 || cz.abs() > 1 {
                    let n = 0;
                    for i in 0..80000 { n += i; }
                }
                [[cx * 4, 0, cz * 4, 0]]
            }
            "#,
        )
        .world(json!({
            "schema_version": 1, "generate": "gen", "chunk_voxels": 4, "voxel_size": 2.0,
            "voxel_brick": CUBE, "view_chunks": 1, "radius_chunks": 64,
            "materials": [{ "id": "terra:material/stone", "name": "Stone", "color": [0.5, 0.5, 0.5, 1.0] }]
        })),
    ]);
    // Far from the origin: every chunk around the player is missing.
    let _far = s
        .join("Explorer".into(), Vec3::new(200.0, 5.0, 200.0), false)
        .unwrap();
    let chunks = s.package_stats().chunks;
    let took = timed(|| s.step().unwrap());
    assert_eq!(s.package_stats().chunks, chunks + 1, "one chunk per tick");
    assert!(
        !diag_codes(&s).contains(&"script.budget".to_string()),
        "the generator call is legal"
    );
    assert!(took < STALL, "one generated chunk took {took:?}");
}

/// An `on_tick` hook that always exceeds its budget. The failure is named
/// (`script.budget`) and nothing is committed; expected containment is that
/// a package failing every call is quarantined after a few failures (here:
/// at most 10 over 60 ticks) instead of burning 400 000 operations per tick
/// forever.
#[test]
fn failing_on_tick_is_quarantined() {
    let mut s = session(vec![Spec::server(
        "spinner",
        &[],
        json!({ "tick_interval": 1, "state": { "global": { "n": { "default": 0 } } } }),
        "fn on_tick() { set(\"n\", 1); loop { } }",
    )]);
    let _p = s.join("P".into(), SPAWN, false).unwrap();
    steps(&mut s, 60);
    let failures = diag_codes(&s)
        .iter()
        .filter(|c| *c == "script.budget")
        .count();
    assert!(failures > 0, "the hook failed with a named error");
    assert_eq!(
        global(&s, "spinner", "n"),
        Some(json!(0)),
        "nothing committed"
    );
    assert!(
        failures <= 10,
        "{failures} over-budget on_tick calls in 60 ticks"
    );
}

/// A `think` that fails is retried at most once per 120 ticks (the entity is
/// paused), so one broken entity costs little.
#[test]
fn failing_think_backs_off() {
    let mut s = session(vec![
        Spec::server(
            "broken",
            &["entity"],
            json!({ "commands": [{ "name": "make", "admin": true }] }),
            "fn cmd_make(p) { spawn_entity(\"broken:entity/bot\", 5.0, 0.1, 5.0); } fn think(me) { loop { } }",
        )
        .entity("bot", 1, 4),
    ]);
    let mut admin = Client::new(s.join("Admin".into(), SPAWN, true).unwrap());
    admin.run(&mut s, "broken", "make").unwrap();
    steps(&mut s, 240);
    let failures = diag_codes(&s)
        .iter()
        .filter(|c| *c == "script.budget")
        .count();
    assert!(
        (1..=3).contains(&failures),
        "{failures} failed thinks in 240 ticks"
    );
}

/// One player fires 60 legal package commands (no cooldown declared) in one
/// tick, each spending most of the 200 000-operation Command budget. The
/// generic action limit (60 per 120 ticks) admits all of them. Expected: a
/// per-player script budget per tick or window (the player is throttled with
/// a named rejection), keeping the tick under [`STALL`].
#[test]
fn rapid_fire_commands_are_throttled() {
    let mut s = session(vec![Spec::server(
        "grinder",
        &[],
        json!({ "commands": [{ "name": "burn" }] }),
        "fn cmd_burn(p) { let n = 0; for i in 0..60000 { n += i; } }",
    )]);
    let mut c = Client::new(s.join("Spammer".into(), SPAWN, false).unwrap());
    let mut accepted = 0;
    let mut rejected = Vec::new();
    let took = timed(|| {
        for _ in 0..60 {
            match c.run(&mut s, "grinder", "burn") {
                Ok(_) => accepted += 1,
                Err(e) => rejected.push(code(&e)),
            }
        }
    });
    assert!(
        !rejected.iter().any(|r| r == "script.budget"),
        "each call is legal: {rejected:?}"
    );
    assert!(
        took < STALL,
        "{accepted} accepted commands took {took:?} in one tick"
    );
}

// ---------------------------------------------------------- 2. state stores

/// One command fills 200 declared per-player keys with 4 000-byte strings
/// (every value within the per-value limit). Expected: a per-player or
/// per-package byte budget (assumed 64 KiB per player) refuses the commit,
/// so state and saves stay bounded; today ~800 KB per player is committed,
/// and ~260 such durable players exceed the 256 MiB save decode limit.
#[test]
fn player_state_has_a_byte_budget() {
    let keys: serde_json::Map<String, Value> = (0..200)
        .map(|i| (format!("k{i}"), json!({ "default": 0 })))
        .collect();
    let mut s = session(vec![Spec::server(
        "hoard",
        &[],
        json!({ "commands": [{ "name": "hoard" }], "state": { "player": keys } }),
        "fn cmd_hoard(p) { let s = \"\"; s.pad(4000, 'x'); for i in 0..200 { set_player(p, \"k\" + i, s + i); } }",
    )]);
    let owner = s
        .join_verified(
            "Hoarder".into(),
            SPAWN,
            false,
            Some(bri_admin::Principal([3; 32])),
        )
        .unwrap();
    let mut c = Client::new(owner);
    let result = c.run(&mut s, "hoard", "hoard");
    let bytes = s.package_save().unwrap().encode().unwrap().len();
    assert!(
        result.is_err() && bytes <= 64 * 1024,
        "one command committed {bytes} bytes of player state ({result:?})"
    );
}

/// State may use only declared keys: a script writing an undeclared global
/// or player key fails as a whole (`state.undeclared`) and commits nothing.
#[test]
fn undeclared_state_keys_are_refused() {
    let mut s = session(vec![Spec::server(
        "ledger",
        &[],
        json!({ "commands": [{ "name": "a" }, { "name": "b" }],
                "state": { "global": { "ok": { "default": 0 } }, "player": { "mine": { "default": 0 } } } }),
        "fn cmd_a(p) { set(\"ok\", 1); set(\"extra\", 1); } fn cmd_b(p) { set(\"ok\", 2); set_player(p, \"extra\", 1); }",
    )]);
    let mut c = Client::new(s.join("P".into(), SPAWN, false).unwrap());
    assert_eq!(
        code(&c.run(&mut s, "ledger", "a").unwrap_err()),
        "state.undeclared"
    );
    assert_eq!(
        code(&c.run(&mut s, "ledger", "b").unwrap_err()),
        "state.undeclared"
    );
    assert_eq!(global(&s, "ledger", "ok"), Some(json!(0)));
}

/// A save written when the behaviour declared a key the current version no
/// longer declares (or a hand-edited save) must not brick the package.
/// Expected: install drops or reports the stale key and commands keep
/// working. Today every later call fails with `state.undeclared`, forever.
#[test]
fn stale_saved_keys_do_not_brick_a_package() {
    let spec = || {
        Spec::server(
            "keeper",
            &[],
            json!({ "commands": [{ "name": "ping" }],
                    "state": { "global": { "pings": { "default": 0 } } } }),
            "fn cmd_ping(p) { set(\"pings\", get(\"pings\") + 1); }",
        )
    };
    let mut store = Store::default();
    store
        .namespace_mut("keeper")
        .global
        .insert("retired".into(), json!(5));
    let save = PackageSave {
        schema_version: bri_sim::session::PACKAGE_SAVE_SCHEMA,
        store,
        world: None,
    };
    let save = PackageSave::decode(&save.encode().unwrap()).unwrap();
    let mut s = session_with(vec![spec()], Some(save));
    let mut c = Client::new(s.join("P".into(), SPAWN, false).unwrap());
    let result = c.run(&mut s, "keeper", "ping");
    assert!(result.is_ok(), "{:?}", result.map_err(|e| code(&e)));
}

/// Package saves are checked like live state when loaded: a value over the
/// 4 KiB / depth-4 limits (which no script can commit) is refused. Today
/// `PackageSave::decode` does not run `check_value`, so the value loads and
/// a public one reaches `package_state()`, which clients then refuse.
#[test]
fn damaged_package_saves_are_refused() {
    let mut store = Store::default();
    let ns = store.namespace_mut("keeper");
    ns.global.insert("big".into(), json!("x".repeat(100_000)));
    ns.players.insert(
        PlayerKey::principal(&[1; 32]),
        [("deep".to_string(), json!([[[[[[[[1]]]]]]]]))].into(),
    );
    let bytes = PackageSave {
        schema_version: bri_sim::session::PACKAGE_SAVE_SCHEMA,
        store,
        world: None,
    }
    .encode()
    .unwrap();
    assert!(
        PackageSave::decode(&bytes).is_err(),
        "a save with a 100 KB value and 8-deep nesting loaded"
    );
}

// ---------------------------------------------------------------- 3. authority

/// Forged commands: unknown package or command, a client package's name,
/// wrong types, too many or too few arguments, oversize or control-character
/// strings, NaN and infinite floats, admin-only as non-admin. Each is refused
/// with a typed code and runs no script (state unchanged).
#[test]
fn forged_package_commands_are_refused() {
    let mut s = session(vec![Spec::server(
        "shop",
        &[],
        json!({ "commands": [
                    { "name": "buy", "args": ["string", "int"] },
                    { "name": "price", "args": ["float"] },
                    { "name": "wipe", "admin": true } ],
                "state": { "global": { "calls": { "default": 0 } } } }),
        r#"
        fn cmd_buy(p, what, n) { set("calls", get("calls") + 1); }
        fn cmd_price(p, f) { set("calls", get("calls") + 1); }
        fn cmd_wipe(p) { set("calls", get("calls") + 1); }
        "#,
    )]);
    let mut c = Client::new(s.join("Forger".into(), SPAWN, false).unwrap());
    let s_ = |v: &str| PackageArg::String(v.into());
    let cases: Vec<(Command, &str)> = vec![
        (pkg("nope", "buy", vec![]), "command.package"),
        (pkg("hxmodel", "buy", vec![]), "command.package"),
        (pkg("shop", "steal", vec![]), "command.unknown"),
        (pkg("shop", "cmd_buy", vec![]), "command.unknown"),
        (pkg("shop", "buy", vec![]), "command.args"),
        (
            pkg("shop", "buy", vec![PackageArg::Int(1), s_("x")]),
            "command.args",
        ),
        (
            pkg(
                "shop",
                "buy",
                vec![s_("x"), PackageArg::Int(1), PackageArg::Int(2)],
            ),
            "command.args",
        ),
        (
            pkg(
                "shop",
                "buy",
                vec![s_(&"x".repeat(100_000)), PackageArg::Int(1)],
            ),
            "command.args",
        ),
        (
            pkg("shop", "buy", vec![s_("a\u{0}b"), PackageArg::Int(1)]),
            "command.args",
        ),
        (
            pkg("shop", "price", vec![PackageArg::Float(f64::NAN)]),
            "command.args",
        ),
        (
            pkg("shop", "price", vec![PackageArg::Float(f64::INFINITY)]),
            "command.args",
        ),
        (
            pkg("shop", "price", vec![PackageArg::Int(3)]),
            "command.args",
        ),
        (pkg("shop", "wipe", vec![]), "command.admin"),
    ];
    for (command, expected) in cases {
        let shown = format!("{command:?}").chars().take(120).collect::<String>();
        let error = c.send(&mut s, command).unwrap_err();
        assert_eq!(code(&error), expected, "{shown}");
    }
    assert_eq!(global(&s, "shop", "calls"), Some(json!(0)));
    // The wire form rejects unknown fields and argument types outright.
    assert!(
        serde_json::from_value::<PackageCommand>(
            json!({ "package": "shop", "command": "wipe", "admin": true })
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<PackageArg>(json!({ "type": "object", "value": {} })).is_err()
    );
}

/// A declared cooldown holds against rapid fire on one connection.
#[test]
fn cooldown_holds_against_rapid_fire() {
    let mut s = session(vec![Spec::server(
        "slow",
        &[],
        json!({ "commands": [{ "name": "go", "cooldown_ticks": 1200 }] }),
        "fn cmd_go(p) { }",
    )]);
    let mut c = Client::new(s.join("P".into(), SPAWN, false).unwrap());
    c.run(&mut s, "slow", "go").unwrap();
    for _ in 0..5 {
        assert_eq!(
            code(&c.run(&mut s, "slow", "go").unwrap_err()),
            "command.cooldown"
        );
        s.step().unwrap();
    }
}

/// Reconnecting with the same verified identity must not reset a command
/// cooldown (cooldowns guard package economies). Finding H2-F11, fixed:
/// cooldowns were keyed by the connection's owner number.
#[test]
fn cooldown_survives_reconnecting() {
    let mut s = session(vec![Spec::server(
        "slow",
        &[],
        json!({ "commands": [{ "name": "go", "cooldown_ticks": 1200 }] }),
        "fn cmd_go(p) { }",
    )]);
    let me = Some(bri_admin::Principal([4; 32]));
    let first = s.join_verified("P".into(), SPAWN, false, me).unwrap();
    Client::new(first).run(&mut s, "slow", "go").unwrap();
    s.step().unwrap();
    s.disconnect(first).unwrap();
    s.step().unwrap();
    let second = s.join_verified("P".into(), SPAWN, false, me).unwrap();
    let again = Client::new(second).run(&mut s, "slow", "go");
    assert_eq!(
        again.map_err(|e| code(&e)).err().as_deref(),
        Some("command.cooldown"),
        "reconnected as owner {second} (was {first}) and skipped the cooldown"
    );
}

/// A dead player cannot run package commands (by default), matching the
/// session's other actions.
#[test]
fn dead_players_cannot_run_package_commands() {
    let mut s = session(vec![Spec::server(
        "shop",
        &[],
        json!({ "commands": [{ "name": "buy" }], "state": { "global": { "calls": { "default": 0 } } } }),
        "fn cmd_buy(p) { set(\"calls\", get(\"calls\") + 1); }",
    )]);
    let mut c = Client::new(s.join("Ghost".into(), SPAWN, false).unwrap());
    c.send(&mut s, Command::Suicide).unwrap();
    assert!(c.run(&mut s, "shop", "buy").is_err());
    assert_eq!(global(&s, "shop", "calls"), Some(json!(0)));
}

/// Aim for `aim_reach` commands is resolved by the server from the player's
/// own eye within the declared reach; a client cannot extend it, and a
/// malformed aim (NaN) is refused before any script runs.
#[test]
fn aim_is_resolved_by_the_server_within_reach() {
    let mut s = session(vec![Spec::server(
        "aimer",
        &[],
        json!({ "commands": [{ "name": "look", "aim_reach": 4.0 }],
                "state": { "global": { "d": { "default": 0 } } } }),
        "fn cmd_look(p) { let a = aim(); if a == () { set(\"d\", -1.0); } else { set(\"d\", a.distance); } }",
    )]);
    let mut c = Client::new(s.join("P".into(), SPAWN, false).unwrap());
    steps(&mut s, 10);
    let look = pkg("aimer", "look", vec![]);
    c.aim(
        &mut s,
        look.clone(),
        ActionAim {
            yaw: 0.0,
            pitch: -1.5,
        },
    )
    .unwrap();
    let down = global(&s, "aimer", "d").unwrap().as_f64().unwrap();
    assert!((0.0..=4.0).contains(&down), "{down}");
    c.aim(
        &mut s,
        look.clone(),
        ActionAim {
            yaw: 0.0,
            pitch: 0.0,
        },
    )
    .unwrap();
    assert_eq!(
        global(&s, "aimer", "d"),
        Some(json!(-1.0)),
        "nothing within reach"
    );
    for bad in [
        ActionAim {
            yaw: f32::NAN,
            pitch: 0.0,
        },
        ActionAim {
            yaw: 0.0,
            pitch: f32::INFINITY,
        },
    ] {
        assert!(c.aim(&mut s, look.clone(), bad).is_err(), "{bad:?}");
    }
    assert_eq!(global(&s, "aimer", "d"), Some(json!(-1.0)));
}

/// Private keys never reach `package_state()`, and a package cannot read
/// another package's state: `get`/`get_player` only see its own namespace.
#[test]
fn private_and_foreign_state_stays_private() {
    let mut s = session(vec![
        Spec::server(
            "vault",
            &[],
            json!({ "on_join": true, "state": {
                "global": { "hidden": { "default": 0 }, "count": { "default": 0, "visible": "everyone" } },
                "player": { "secret": { "default": 0 }, "shown": { "default": 0, "visible": "everyone" } } } }),
            "fn on_join(p) { set(\"hidden\", 42); set_player(p, \"secret\", 7); set_player(p, \"shown\", 1); }",
        ),
        Spec::server(
            "peeker",
            &[],
            json!({ "commands": [{ "name": "peek" }], "state": {
                "global": { "seen": { "default": "none", "visible": "everyone" } } } }),
            "fn cmd_peek(p) { set(\"seen\", `${get(\"hidden\")}/${get_player(p, \"secret\")}/${get(\"count\")}`); }",
        ),
    ]);
    let a = s.join("A".into(), SPAWN, false).unwrap();
    let b = s.join("B".into(), spawn_at(1), false).unwrap();
    assert_eq!(s.package_value("vault", a, "secret"), Some(json!(7)));
    Client::new(b).run(&mut s, "peeker", "peek").unwrap();
    assert_eq!(global(&s, "peeker", "seen"), Some(json!("//")));
    let view = s.package_state();
    let vault = &view.packages["vault"];
    assert!(!vault.global.contains_key("hidden"));
    for owner in [a, b] {
        assert!(!vault.players[&owner].contains_key("secret"));
        assert_eq!(vault.players[&owner]["shown"], json!(1));
    }
    view.validate().unwrap();
    let text = serde_json::to_string(&view).unwrap();
    assert!(
        !text.contains("secret") && !text.contains("hidden"),
        "{text}"
    );
}

/// Two packages may declare the same command name; each command is scoped to
/// its package and runs only that package's function.
#[test]
fn same_command_names_stay_scoped_to_their_package() {
    let mut s = session(vec![
        Spec::server(
            "alpha",
            &[],
            json!({ "commands": [{ "name": "go" }], "state": { "global": { "n": { "default": 0 } } } }),
            "fn cmd_go(p) { set(\"n\", get(\"n\") + 1); }",
        ),
        Spec::server(
            "beta",
            &[],
            json!({ "commands": [{ "name": "go" }], "state": { "global": { "n": { "default": 0 } } } }),
            "fn cmd_go(p) { set(\"n\", get(\"n\") + 10); }",
        ),
    ]);
    let mut c = Client::new(s.join("P".into(), SPAWN, false).unwrap());
    c.run(&mut s, "alpha", "go").unwrap();
    c.run(&mut s, "beta", "go").unwrap();
    assert_eq!(global(&s, "alpha", "n"), Some(json!(1)));
    assert_eq!(global(&s, "beta", "n"), Some(json!(10)));
}

// ------------------------------------------------------------ 4. capabilities

/// Operations requested from hooks (on_join, on_tick, think) pass the same
/// capability and ownership gate as commands: an unauthorised operation
/// fails the whole call, so its state changes are not committed either.
#[test]
fn hooks_pass_the_same_capability_gate() {
    let mut s = session(vec![
        Spec::server(
            "rogue",
            &[],
            json!({ "on_join": true, "tick_interval": 1, "state": {
                "global": { "ticks": { "default": 0 } },
                "player": { "joined": { "default": 0 } } } }),
            r#"
            fn on_join(p) { set_player(p, "joined", 1); tell(p, "welcome"); }
            fn on_tick() { set("ticks", get("ticks") + 1); remove_brick(1); }
            "#,
        ),
        Spec::server(
            "herd",
            &["entity"],
            json!({ "commands": [{ "name": "cow", "admin": true }] }),
            "fn cmd_cow(p) { spawn_entity(\"herd:entity/cow\", 4.0, 0.1, 4.0); } fn think(me) { }",
        )
        .entity("cow", 1, 4),
        Spec::server(
            "rustler",
            &["entity"],
            json!({ "commands": [
                { "name": "rope", "admin": true },
                { "name": "steer", "args": ["int"] },
                { "name": "kill", "args": ["int"] },
                { "name": "mark", "args": ["int"] },
                { "name": "clone" } ],
                "state": { "global": { "tries": { "default": 0 } } } }),
            r#"
            fn cmd_rope(p) { spawn_entity("rustler:entity/rope", -4.0, 0.1, -4.0); }
            fn cmd_steer(p, id) { set("tries", 1); steer(id, 1.0, 0.0, true); }
            fn cmd_kill(p, id) { set("tries", 1); remove_entity(id); }
            fn cmd_mark(p, id) { set("tries", 1); label(id, "mine"); }
            fn cmd_clone(p) { set("tries", 1); spawn_entity("herd:entity/cow", 0.0, 0.1, 0.0); }
            // Its own entity reaches for every other entity from think.
            fn think(me) { for e in entities() { if e.id != me.id { steer(e.id, 1.0, 0.0, false); } } }
            "#,
        )
        .entity("rope", 1, 4),
    ]);
    let admin = s.join("Admin".into(), SPAWN, true).unwrap();
    let mut a = Client::new(admin);
    assert_eq!(s.package_value("rogue", admin, "joined"), Some(json!(0)));
    steps(&mut s, 3);
    assert_eq!(global(&s, "rogue", "ticks"), Some(json!(0)));
    assert!(
        diag_codes(&s).iter().all(|c| c == "op.capability"),
        "{:?}",
        diag_codes(&s)
    );
    a.run(&mut s, "herd", "cow").unwrap();
    let cow = s.package_entities()[0].id;
    for command in ["steer", "kill", "mark"] {
        let e = a
            .send(
                &mut s,
                pkg("rustler", command, vec![PackageArg::Int(cow as i64)]),
            )
            .unwrap_err();
        assert_eq!(code(&e), "op.not_owner", "{command}");
    }
    assert_eq!(
        code(&a.run(&mut s, "rustler", "clone").unwrap_err()),
        "op.foreign_entity"
    );
    assert_eq!(global(&s, "rustler", "tries"), Some(json!(0)));
    a.run(&mut s, "rustler", "rope").unwrap();
    steps(&mut s, 3);
    assert!(diag_codes(&s).contains(&"op.not_owner".to_string()));
    assert_eq!(s.package_entities().len(), 2, "the cow is still there");
}

/// A package with `world.edit` can remove bricks on behalf of a player; it
/// must not remove another player's build that the caller has no trust on
/// (or: the capability is scoped to bricks the package's world owns).
/// Finding H2-F12, fixed: `remove_brick` applied to any brick by id.
#[test]
fn packages_cannot_remove_other_players_builds() {
    let mut s = session(vec![Spec::server(
        "wrecker",
        &["world.edit"],
        json!({ "commands": [{ "name": "zap", "args": ["int"] }] }),
        "fn cmd_zap(p, id) { remove_brick(id); }",
    )]);
    let mut builder = Client::new(s.join("Builder".into(), SPAWN, false).unwrap());
    let mut griefer = Client::new(s.join("Griefer".into(), spawn_at(2), false).unwrap());
    steps(&mut s, 5);
    let Reply::Planted(brick) = builder
        .send(
            &mut s,
            Command::Plant {
                definition: "plate".into(),
                position: [0.5, 0.1, -3.25],
                quarter_turns: 0,
                color: 0,
            },
        )
        .unwrap()
    else {
        panic!("expected a planted brick")
    };
    let _ = griefer.send(
        &mut s,
        pkg("wrecker", "zap", vec![PackageArg::Int(brick as i64)]),
    );
    assert!(
        s.simulation().state().bricks.contains_key(&brick),
        "a package removed another player's brick for a stranger"
    );
}

// ------------------------------------------- 5. operations with extreme values

/// A world of 18 432 generated voxels. One package call may request 1024
/// explosions (the per-call operation cap), each destroying up to 256 bricks.
/// Expected: a per-call or per-tick destruction budget (assumed: at most
/// 4 096 bricks per call) and the call finishing within 2 s even in debug.
#[test]
fn one_call_cannot_level_the_world() {
    let mut s = session(vec![Spec::server(
        "quarry",
        &["damage"],
        json!({ "commands": [{ "name": "quake", "admin": true }] }),
        r#"
        fn gen(cx, cz) {
            let out = [];
            for x in 0..16 { for z in 0..16 { for y in 0..8 { out.push([cx * 16 + x, y, cz * 16 + z, 0]); } } }
            out
        }
        fn cmd_quake(p) {
            for i in 0..1024 {
                explode((i % 32) * 3.0 - 32.0, 6.0 + (i % 3) * 4.0, (i / 32) * 3.0 - 32.0, 1.0, 0.0, 16.0);
            }
        }
        "#,
    )
    .world(json!({
        "schema_version": 1, "generate": "gen", "chunk_voxels": 16, "voxel_size": 2.0,
        "voxel_brick": CUBE, "view_chunks": 1, "radius_chunks": 2,
        "materials": [{ "id": "quarry:material/stone", "name": "Stone", "color": [0.5, 0.5, 0.5, 1.0] }]
    }))]);
    let before = s.simulation().state().bricks.len();
    assert!(before >= 18_000, "{before}");
    let mut admin = Client::new(
        s.join("Admin".into(), Vec3::new(0.0, 20.0, 0.0), true)
            .unwrap(),
    );
    let took = timed(|| {
        admin.run(&mut s, "quarry", "quake").unwrap();
    });
    let destroyed = before - s.simulation().state().bricks.len();
    assert!(
        destroyed <= 4096 && took < Duration::from_secs(2),
        "one package call destroyed {destroyed} of {before} bricks in {took:?}"
    );
}

/// One package may not take the whole server's entity capacity: four kinds
/// at 256 each fill the global cap of 1024, after which another package
/// cannot spawn anything. Expected: a per-package entity budget that leaves
/// room for other packages.
#[test]
fn one_package_cannot_take_every_entity_slot() {
    let mut horde = Spec::server(
        "horde",
        &["entity"],
        json!({ "commands": [{ "name": "fill", "args": ["int"], "admin": true }] }),
        r#"
        fn cmd_fill(p, k) {
            let kinds = ["horde:entity/a", "horde:entity/b", "horde:entity/c", "horde:entity/d"];
            for i in 0..256 {
                spawn_entity(kinds[k], (i % 16) * 2.5 - 90.0 + k * 45.0, 0.1, (i / 16) * 2.5 - 20.0);
            }
        }
        fn think(me) { }
        "#,
    );
    for kind in ["a", "b", "c", "d"] {
        horde = horde.entity(kind, 120, 256);
    }
    let mut s = session(vec![
        horde,
        Spec::server(
            "pet",
            &["entity"],
            json!({ "commands": [{ "name": "adopt", "admin": true }] }),
            "fn cmd_adopt(p) { spawn_entity(\"pet:entity/dog\", 0.0, 0.1, 40.0); } fn think(me) { }",
        )
        .entity("dog", 60, 1),
    ]);
    let mut admin = Client::new(
        s.join("Admin".into(), Vec3::new(0.0, 0.05, 60.0), true)
            .unwrap(),
    );
    for k in 0..4 {
        admin
            .send(&mut s, pkg("horde", "fill", vec![PackageArg::Int(k)]))
            .unwrap();
    }
    // The horde gets its share: every slot but the pet package's reserve.
    assert_eq!(
        s.package_stats().entities,
        1024 - 64,
        "{:?}",
        s.package_diagnostics().last()
    );
    admin.run(&mut s, "pet", "adopt").unwrap();
    assert!(
        s.package_entities()
            .iter()
            .any(|e| e.kind == "pet:entity/dog"),
        "another package could not spawn: {:?}",
        s.package_diagnostics().last()
    );
}

/// Legal but extreme values: spawning at the edge of the allowed range,
/// steering with a huge direction and a 32-byte label. The session keeps
/// stepping, entity positions stay finite, and a 33-byte label is refused.
#[test]
fn extreme_but_legal_entity_values_are_contained() {
    let mut s = session(vec![
        Spec::server(
            "edge",
            &["entity"],
            json!({ "commands": [{ "name": "far", "admin": true }, { "name": "long", "admin": true }] }),
            r#"
            fn cmd_far(p) {
                spawn_entity("edge:entity/e", 999999.0, 999999.0, -999999.0);
                spawn_entity("edge:entity/e", 3.0, 0.1, 3.0);
            }
            fn cmd_long(p) { for e in entities() { label(e.id, "0123456789012345678901234567890123"); } }
            fn think(me) { steer(me.id, 999999.0, -999999.0, true); label(me.id, "01234567890123456789012345678901"); }
            "#,
        )
        .entity("e", 1, 4),
    ]);
    let mut admin = Client::new(s.join("Admin".into(), SPAWN, true).unwrap());
    admin.run(&mut s, "edge", "far").unwrap();
    steps(&mut s, 60);
    for e in s.package_entities() {
        e.validate().unwrap();
        assert_eq!(e.label.len(), 32);
    }
    assert_eq!(
        code(&admin.run(&mut s, "edge", "long").unwrap_err()),
        "op.bounds"
    );
}

/// Package chat obeys a rate budget like player chat (4 lines per 120 ticks):
/// one call may not broadcast 1024 lines and push every player's chat out of
/// the 100-line history in one tick.
#[test]
fn package_broadcasts_cannot_flood_chat() {
    let mut s = session(vec![Spec::server(
        "herald",
        &["chat"],
        json!({ "commands": [{ "name": "shout" }] }),
        "fn cmd_shout(p) { for i in 0..1024 { broadcast(`news ${i}`); } }",
    )]);
    let mut speaker = Client::new(s.join("Speaker".into(), SPAWN, false).unwrap());
    let mut flooder = Client::new(s.join("Flooder".into(), spawn_at(1), false).unwrap());
    speaker
        .send(&mut s, Command::Chat("hello everyone".into()))
        .unwrap();
    flooder.run(&mut s, "herald", "shout").unwrap();
    let packaged = s
        .chat()
        .iter()
        .filter(|l| l.text.starts_with("news"))
        .count();
    assert!(
        s.chat().iter().any(|l| l.text == "hello everyone"),
        "one package command wrote {packaged}+ lines and evicted the player's line"
    );
}

/// `tell` to oneself, 1024 times per command: private notices of other
/// players (a shared 256-entry queue) must not be evicted by one player's
/// package command.
#[test]
fn package_tells_cannot_evict_other_players_notices() {
    let mut s = session(vec![Spec::server(
        "herald",
        &["chat"],
        json!({ "commands": [{ "name": "note" }, { "name": "spam" }] }),
        "fn cmd_note(p) { tell(p, \"your receipt\"); } fn cmd_spam(p) { for i in 0..1024 { tell(p, `spam ${i}`); } }",
    )]);
    let a = s.join("A".into(), SPAWN, false).unwrap();
    let b = s.join("B".into(), spawn_at(1), false).unwrap();
    let _ = s.take_private_notices();
    Client::new(a).run(&mut s, "herald", "note").unwrap();
    Client::new(b).run(&mut s, "herald", "spam").unwrap();
    let notices = s.take_private_notices();
    assert!(
        notices
            .iter()
            .any(|(to, n)| *to == a && matches!(n, Notice::Chat(t) if t == "your receipt")),
        "A's notice was evicted by B's package spam ({} queued)",
        notices.len()
    );
}

/// `while_dead` is the only way a dead player's package command runs; the
/// refusal is the typed `command.dead`.
#[test]
fn while_dead_is_declared_per_command() {
    let mut s = session(vec![Spec::server(
        "lobby",
        &[],
        json!({ "commands": [{ "name": "vote", "while_dead": true }, { "name": "buy" }],
                "state": { "global": { "votes": { "default": 0 } } } }),
        "fn cmd_vote(p) { set(\"votes\", get(\"votes\") + 1); } fn cmd_buy(p) { }",
    )]);
    let mut c = Client::new(s.join("Ghost".into(), SPAWN, false).unwrap());
    c.send(&mut s, Command::Suicide).unwrap();
    assert_eq!(
        code(&c.run(&mut s, "lobby", "buy").unwrap_err()),
        "command.dead"
    );
    c.run(&mut s, "lobby", "vote").unwrap();
    assert_eq!(global(&s, "lobby", "votes"), Some(json!(1)));
}

/// Moving and respawning players needs the `player` capability, from a
/// command and from the `on_death` hook alike; without it the call fails
/// as a whole (`op.capability`) and commits nothing. With it, a legal but
/// extreme teleport keeps positions finite and the session stepping, and an
/// `on_death` hook that kills again is delivered a tick later rather than
/// recursing.
#[test]
fn player_operations_and_on_death_pass_the_capability_gate() {
    let mut s = session(vec![
        Spec::server(
            "mover",
            &["damage"],
            json!({ "commands": [{ "name": "lift" }], "on_death": true,
                    "state": { "global": { "moves": { "default": 0 }, "seen": { "default": 0 } } } }),
            r#"
            fn cmd_lift(p) { set("moves", 1); teleport(p, 0.0, 50.0, 0.0); }
            fn on_death(victim, killer) { set("seen", get("seen") + 1); respawn(victim); }
            "#,
        ),
        Spec::server(
            "warden",
            &["player", "damage"],
            json!({ "commands": [{ "name": "exile" }], "on_death": true,
                    "state": { "global": { "deaths": { "default": 0 } } } }),
            r#"
            fn cmd_exile(p) { teleport(p, 999999.0, 999999.0, -999999.0); }
            fn on_death(victim, killer) {
                set("deaths", get("deaths") + 1);
                respawn(victim);
                damage(victim, 1000.0);
            }
            "#,
        ),
    ]);
    let mut c = Client::new(s.join("P".into(), SPAWN, false).unwrap());
    assert_eq!(
        code(&c.run(&mut s, "mover", "lift").unwrap_err()),
        "op.capability"
    );
    assert_eq!(global(&s, "mover", "moves"), Some(json!(0)));
    c.send(&mut s, Command::Suicide).unwrap();
    steps(&mut s, 3);
    assert_eq!(
        global(&s, "mover", "seen"),
        Some(json!(0)),
        "hook without capability commits nothing"
    );
    assert!(diag_codes(&s).contains(&"op.capability".to_string()));
    let deaths = global(&s, "warden", "deaths").unwrap().as_i64().unwrap();
    assert!(
        (1..=4).contains(&deaths),
        "one death per tick at most: {deaths}"
    );
    steps(&mut s, 400);
    let _ = c.run(&mut s, "warden", "exile");
    steps(&mut s, 10);
    for (state, _) in s.motion_states() {
        assert!(state.feet.iter().all(|v| v.is_finite()), "{state:?}");
    }
}

/// Night QA finding C: a guest (not the host) builds on generated ground.
#[test]
fn a_guest_plants_on_generated_ground() {
    let mut s = session(vec![Spec::server("terra", &[], json!({}), "fn gen(cx, cz) { if cx == 0 && cz == 0 { [[0, 0, 0, 0]] } else { [] } }")
        .world(json!({
            "schema_version": 1, "generate": "gen", "chunk_voxels": 4, "voxel_size": 2.0,
            "voxel_brick": CUBE, "view_chunks": 1, "radius_chunks": 4,
            "materials": [{ "id": "terra:material/stone", "name": "Stone", "color": [0.5, 0.5, 0.5, 1.0] }]
        }))]);
    let _host = s.join("Host".into(), spawn_at(1), true).unwrap();
    let guest = s.join("Guest".into(), spawn_at(2), false).unwrap();
    s.step().unwrap();
    let cube = s
        .simulation()
        .state()
        .bricks
        .values()
        .find(|b| matches!(&b.definition, bri_world::ContentRef::Resolved(id) if id == CUBE))
        .expect("generated voxel")
        .clone();
    println!("voxel {:?} owner {}", cube.position, cube.owner);
    let top = cube.position[1] + 1.0;
    let reply = Client::new(guest).send(
        &mut s,
        Command::Plant {
            definition: "plate".into(),
            position: [cube.position[0] - 0.5, top + 0.1, cube.position[2] - 0.75],
            quarter_turns: 0,
            color: 0,
        },
    );
    assert!(matches!(reply, Ok(Reply::Planted(_))), "{reply:?}");
}
