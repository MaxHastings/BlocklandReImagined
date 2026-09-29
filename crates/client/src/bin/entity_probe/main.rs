//! Headless entity stress probe: what emitters, vehicles, weapons,
//! projectiles, explosions and debris cost the host and a client.
//!
//! Every scene is built on a real map in a real `Session`: emitter and light
//! bricks, vehicles driven in circles by seated players, players holding the
//! trigger of guns and rocket launchers aimed at a brick wall, and rocket
//! volleys into a brick pile that knock out bricks (debris) as they respawn.
//! The scene's players keep their last input, so the scene runs by itself.
//!
//! Host: the scene's session is stepped at 120 Hz on this thread and each
//! step is timed. Client: the same scene is served on loopback by the real
//! server loop, and a real `App` joins it as a guest and renders offscreen at
//! 1920x1080; each frame's `tick`, scene encoding and GPU completion are
//! timed. No window, no input and no audio device.
//!
//! Usage: entity_probe <content-root> <out-dir> [scene ...]
//!   scenes: idle emitters vehicles weapons blast (default: all)
//!   BRI_PROBE_SECONDS (default 10) measured seconds per scene
//!   BRI_PROBE_ONLY=host|client runs one side
//!   BRI_PROBE_PROFILE=1 samples the measured thread (see profiler.rs)
//!
//! Other work on the machine moves wall-clock times, so every stage also
//! reports this thread's CPU cycles and heap allocations, which it does not.
#[cfg(windows)]
mod profiler;

use anyhow::{Context, Result, bail, ensure};
use bri_client::{app::App, content::ContentPaths, platform::PlatformApp};
use bri_net::server::{self, ServerOptions};
use bri_sim::{
    player::MoveInput,
    session::{Command, Session, ToolCatalog, ToolInventory},
};
use bri_ui::api::{ConnectionState, UiAction};
use bri_world::{Brick, ContentRef, Emitter, Light, VehicleSpawn, World};
use glam::Vec3;
use serde_json::json;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// Counts this thread's heap allocations and bytes, and the process's live
/// heap bytes.
struct Counting;
static LIVE: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
fn live_bytes() -> i64 {
    LIVE.load(std::sync::atomic::Ordering::Relaxed)
}
thread_local! {
    static ALLOCATED: Cell<(u64, u64)> = const { Cell::new((0, 0)) };
}
fn note(bytes: usize) {
    let _ = ALLOCATED.try_with(|c| {
        let (n, b) = c.get();
        c.set((n + 1, b + bytes as u64));
    });
}
// SAFETY: forwards to the system allocator unchanged; only counts.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        note(layout.size());
        LIVE.fetch_add(layout.size() as i64, std::sync::atomic::Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        note(layout.size());
        LIVE.fetch_add(layout.size() as i64, std::sync::atomic::Ordering::Relaxed);
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size() as i64, std::sync::atomic::Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        note(new_size);
        LIVE.fetch_add(
            new_size as i64 - layout.size() as i64,
            std::sync::atomic::Ordering::Relaxed,
        );
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}
#[global_allocator]
static ALLOCATOR: Counting = Counting;
fn allocated() -> (u64, u64) {
    ALLOCATED.with(Cell::get)
}

/// CPU cycles this thread has run: unlike wall clock, other processes do
/// not add to it.
#[cfg(windows)]
fn cycles() -> u64 {
    use windows_sys::Win32::System::{Threading::GetCurrentThread, WindowsProgramming};
    let mut c = 0u64;
    // SAFETY: the current thread's pseudo-handle and an owned counter.
    unsafe { WindowsProgramming::QueryThreadCycleTime(GetCurrentThread(), &mut c) };
    c
}
#[cfg(not(windows))]
fn cycles() -> u64 {
    0
}

/// Wall time, CPU cycles and allocations of one measured stage.
#[derive(Default)]
struct Samples {
    ms: Vec<f64>,
    mcycles: Vec<f64>,
    allocations: Vec<f64>,
    kib: Vec<f64>,
}
impl Samples {
    fn time<T>(&mut self, work: impl FnOnce() -> T) -> T {
        let (n, b) = allocated();
        let c = cycles();
        let t = Instant::now();
        let out = work();
        self.ms.push(ms(t.elapsed()));
        self.mcycles.push((cycles() - c) as f64 / 1e6);
        let (n2, b2) = allocated();
        self.allocations.push((n2 - n) as f64);
        self.kib.push((b2 - b) as f64 / 1024.0);
        out
    }
    fn report(&mut self) -> serde_json::Value {
        json!({
            "wall": percentiles(&mut self.ms),
            "mcycles": percentiles_raw(&mut self.mcycles),
            "allocations": percentiles_raw(&mut self.allocations),
            "alloc_kib": percentiles_raw(&mut self.kib),
        })
    }
}
fn percentiles_raw(samples: &mut [f64]) -> serde_json::Value {
    if samples.is_empty() {
        return json!({ "samples": 0 });
    }
    samples.sort_by(f64::total_cmp);
    let at = |p: f64| samples[((samples.len() - 1) as f64 * p).round() as usize];
    json!({
        "mean": samples.iter().sum::<f64>() / samples.len() as f64,
        "p50": at(0.5), "p95": at(0.95), "p99": at(0.99), "max": at(1.0),
    })
}

const MAP: &str = "v20/add-ons/map_slate/slate.mis";
const SPAWN_BRICK: &str = "v20/brick/brickvehiclespawndata";
const CUBE: &str = "v20/brick/brick2x2data";
const WALL: &str = "v20/brick/brick2x4data";
const WATER: &str = "v20/brick/brick32xwaterdata";
const GUN: &str = "v20.weapon.gunitem";
const ROCKETS: &str = "v20.weapon.rocketlauncheritem";
const SIZE: (u32, u32) = (1920, 1080);
/// Brick emitters players put on builds, in rotation.
const EMITTERS: [&str; 8] = [
    "v20/emitter/burnemittera",
    "v20/emitter/burnemitterb",
    "v20/emitter/fogemitter",
    "v20/emitter/laseremittera",
    "v20/emitter/playerjetemitter",
    "v20/emitter/wateremittera",
    "v20/emitter/alarmemitter",
    "v20/emitter/bsdemitter",
];
const LIGHTS: [&str; 4] = [
    "v20/light/redlight",
    "v20/light/bluelight",
    "v20/light/strobelight",
    "v20/light/rgblight",
];
const VEHICLES: [&str; 3] = [
    "v20.vehicle.jeepvehicle",
    "v20.vehicle.tankvehicle",
    "v20.vehicle.jeepvehicle",
];

#[derive(Clone, Copy, Debug, PartialEq)]
enum Scene {
    /// The empty map with the observer: the floor every scene adds to.
    Idle,
    /// 480 emitter bricks and 64 light bricks in front of the observer.
    Emitters,
    /// 48 vehicles driven in circles and 16 parked.
    Vehicles,
    /// 32 players holding the trigger of guns and rocket launchers at a wall.
    Weapons,
    /// 16 rocket launchers firing volleys into a 4,000-brick pile.
    Blast,
    /// A lake of 64 32x32 water bricks.
    Water,
}
impl Scene {
    const ALL: [Scene; 6] = [
        Scene::Idle,
        Scene::Emitters,
        Scene::Vehicles,
        Scene::Weapons,
        Scene::Blast,
        Scene::Water,
    ];
    fn name(self) -> &'static str {
        match self {
            Scene::Idle => "idle",
            Scene::Emitters => "emitters",
            Scene::Vehicles => "vehicles",
            Scene::Weapons => "weapons",
            Scene::Blast => "blast",
            Scene::Water => "water",
        }
    }
}

/// A sampling profile of the calling thread when BRI_PROBE_PROFILE is set.
struct Profile(#[cfg(windows)] Option<profiler::Profiler>);
impl Profile {
    fn start() -> Self {
        #[cfg(windows)]
        return Profile(std::env::var_os("BRI_PROBE_PROFILE").map(|_| profiler::Profiler::start()));
        #[cfg(not(windows))]
        Profile()
    }
    fn finish(self) -> serde_json::Value {
        #[cfg(windows)]
        if let Some(p) = self.0 {
            return p.finish(40);
        }
        serde_json::Value::Null
    }
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}
fn percentiles(samples: &mut [f64]) -> serde_json::Value {
    if samples.is_empty() {
        return json!({ "samples": 0 });
    }
    samples.sort_by(f64::total_cmp);
    let at = |p: f64| samples[((samples.len() - 1) as f64 * p).round() as usize];
    json!({
        "samples": samples.len(),
        "mean_ms": samples.iter().sum::<f64>() / samples.len() as f64,
        "p50_ms": at(0.5), "p95_ms": at(0.95), "p99_ms": at(0.99), "max_ms": at(1.0),
    })
}
/// Deterministic xorshift so runs are comparable.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
}
/// The yaw and pitch that look from `from` at `to` (`view_basis`: yaw 0
/// looks down -Z).
fn aim(from: Vec3, to: Vec3) -> (f32, f32) {
    let d = (to - from).normalize_or(Vec3::NEG_Z);
    (d.x.atan2(-d.z), d.y.clamp(-1.0, 1.0).asin())
}

/// A player holding a weapon's trigger at a spot, aimed at the target.
#[derive(Clone)]
struct Shooter {
    at: Vec3,
    input: MoveInput,
    slot: usize,
}
/// A scene's session and where the observer stands. Shooters are either
/// players in the session (`local`, fed their input every tick by the
/// caller, as a connection would) or left for network players to join.
struct Built {
    session: Session,
    observer: Vec3,
    shooters: Vec<Shooter>,
    local: Vec<(bri_world::OwnerId, MoveInput)>,
}
impl Built {
    /// Send each local shooter's input for the next tick, then step. From
    /// `CLICKING` on they click like a player spamming fire: press every
    /// 300 ms (36 ticks), release 150 ms later.
    fn step(&mut self, sequence: u64) -> Result<()> {
        const CLICKING: u64 = 60;
        for (owner, input) in &self.local {
            self.session.movement(*owner, sequence, *input)?;
            if sequence >= CLICKING && (sequence - CLICKING).is_multiple_of(18) {
                let down = (sequence - CLICKING).is_multiple_of(36);
                self.session
                    .command(*owner, sequence, Command::WeaponTrigger { down })?;
            }
        }
        self.session.step()
    }
}

struct Setup {
    paths: ContentPaths,
}
impl Setup {
    fn session(&self, scene: Scene, local_shooters: bool) -> Result<Built> {
        let paths = &self.paths;
        let loaded = paths.load_map(MAP, None)?;
        let spawn = loaded.spawn_points[0];
        let weapons = paths.weapon_content()?;
        let item_physics = paths.item_physics(&weapons)?;
        let mut session = Session::new(loaded.simulation);
        // LAN hosts damage bricks outside minigames (v20 `$Server::LAN`).
        session.set_lan_host(true);
        // v20 allows 10 physics vehicles by default; big servers raise it.
        let mut settings = session.server_settings().clone();
        settings.physics_vehicles = 100;
        session.set_server_settings(settings)?;
        let mut catalog = ToolCatalog {
            vehicles: VEHICLES.iter().map(|v| v.to_string()).collect(),
            vehicle_bricks: [SPAWN_BRICK.to_string()].into(),
            emitters: EMITTERS.iter().map(|e| e.to_string()).collect(),
            lights: LIGHTS.iter().map(|l| l.to_string()).collect(),
            ..Default::default()
        };
        catalog.install_items(weapons.item_choices.iter().map(|(id, _)| id.clone()))?;
        session.set_tool_catalog(catalog)?;
        session.set_weapon_pack(weapons.pack.clone())?;
        session.set_item_bounds(item_physics.bounds)?;
        session.set_vehicle_pack(paths.vehicle_pack()?)?;
        let mut loadout = ToolInventory::default();
        loadout.slots[0] = Some(GUN.into());
        loadout.slots[1] = Some(ROCKETS.into());
        session.set_spawn_loadout(loadout)?;
        // Stand the observer on the ground at the first drop point; it
        // looks down -Z, where every scene is built.
        let ground = (spawn.y / 0.2).round() * 0.2;
        let observer = Vec3::new(spawn.x.round(), ground + 0.1, spawn.z.round());
        session.set_spawn_points(vec![observer])?;
        // The builder stands behind the observer, out of its spawn spot.
        let builder = session.join("Builder".into(), observer + Vec3::Z * 4.0, true)?;
        let ahead = |x: f32, z: f32| Vec3::new(observer.x + x, ground, observer.z - z);
        let mut world = World::new(
            "Entity probe".into(),
            session.simulation().state().map_id.clone(),
            session.simulation().state().palette.clone(),
        );
        let mut add = |brick: Brick| {
            let id = world.next_brick_id;
            world.bricks.insert(id, brick);
            world.next_brick_id += 1;
        };
        let height = |id: &str| -> Result<f32> {
            Ok(session
                .simulation()
                .definitions
                .entries
                .get(id)
                .with_context(|| format!("No brick {id}"))?
                .mesh
                .height_plates as f32
                * 0.2)
        };
        let cube = height(CUBE)?;
        let wall = height(WALL)?;
        let spawner = height(SPAWN_BRICK)?;
        let water = height(WATER)?;
        let mut shooters: Vec<Shooter> = Vec::new();
        let mut shoot = |at: Vec3, target: Vec3, slot: usize| {
            let (yaw, pitch) = aim(at + Vec3::Y * 2.3, target);
            shooters.push(Shooter {
                at,
                input: MoveInput {
                    yaw,
                    pitch,
                    ..Default::default()
                },
                slot,
            });
        };
        let mut drivers = false;
        match scene {
            Scene::Idle => {}
            Scene::Emitters => {
                // A 24 x 20 field of 2x2 bricks one stud apart, each with an
                // emitter; every eighth also carries a light.
                for i in 0..480usize {
                    let (col, row) = ((i % 24) as f32, (i / 24) as f32);
                    let at = ahead(col * 2.0 - 23.0, 8.0 + row * 2.0);
                    let mut brick = Brick::new(
                        ContentRef::Resolved(CUBE.into()),
                        [at.x, at.y + cube * 0.5, at.z],
                        builder,
                    );
                    brick.color = (i % 16) as u8;
                    brick.emitter = Some(Emitter {
                        asset: Some(ContentRef::Resolved(EMITTERS[i % EMITTERS.len()].into())),
                        direction: 0,
                    });
                    if i % 8 == 0 && i / 8 < 64 {
                        brick.light = Some(Light {
                            asset: ContentRef::Resolved(LIGHTS[(i / 8) % LIGHTS.len()].into()),
                            enabled: true,
                        });
                    }
                    add(brick);
                }
            }
            Scene::Vehicles => {
                // 8 x 8 spawn bricks 12 units apart; the first 48 get drivers.
                for i in 0..64usize {
                    let (col, row) = ((i % 8) as f32, (i / 8) as f32);
                    let at = ahead(col * 12.0 - 42.0, 16.0 + row * 12.0);
                    let mut brick = Brick::new(
                        ContentRef::Resolved(SPAWN_BRICK.into()),
                        [at.x, at.y + spawner * 0.5, at.z],
                        builder,
                    );
                    brick.vehicle = Some(VehicleSpawn {
                        vehicle: ContentRef::Resolved(VEHICLES[i % VEHICLES.len()].into()),
                        recolor: false,
                    });
                    add(brick);
                }
                drivers = true;
            }
            Scene::Weapons => {
                // A wall 48 wide and 8 high (192 2x4s turned along X)...
                let target = ahead(0.0, 30.0);
                for layer in 0..8 {
                    for i in 0..24 {
                        let at = ahead(i as f32 * 2.0 - 23.0, 30.0);
                        let mut brick = Brick::new(
                            ContentRef::Resolved(WALL.into()),
                            [at.x, at.y + wall * (layer as f32 + 0.5), at.z + 0.5],
                            builder,
                        );
                        brick.quarter_turns = 1;
                        brick.color = ((layer + i) % 16) as u8;
                        add(brick);
                    }
                }
                // ...and 32 players in two rows facing it: guns in front,
                // rocket launchers behind.
                for i in 0..32usize {
                    let rockets = i >= 16;
                    let x = (i % 16) as f32 * 3.0 - 22.5;
                    let z = if rockets { 8.0 } else { 14.0 };
                    let aim_at = target + Vec3::new(x * 0.5, wall * 3.0, 0.0);
                    shoot(ahead(x, z) + Vec3::Y * 0.1, aim_at, usize::from(rockets));
                }
            }
            Scene::Water => {
                for i in 0..64usize {
                    let (col, row) = ((i % 8) as f32, (i / 8) as f32);
                    let at = ahead(col * 16.0 - 56.0, 12.0 + row * 16.0);
                    add(Brick::new(
                        ContentRef::Resolved(WATER.into()),
                        [at.x, at.y + water * 0.5, at.z],
                        builder,
                    ));
                }
            }
            Scene::Blast => {
                // A 20 x 20 x 10 pile of 2x2 bricks...
                for layer in 0..10 {
                    for i in 0..400usize {
                        let (col, row) = ((i % 20) as f32, (i / 20) as f32);
                        let at = ahead(col - 9.5, 30.0 + row);
                        let mut brick = Brick::new(
                            ContentRef::Resolved(CUBE.into()),
                            [at.x, at.y + cube * (layer as f32 + 0.5), at.z],
                            builder,
                        );
                        brick.color = ((layer * 3 + i) % 16) as u8;
                        add(brick);
                    }
                }
                // ...and 16 rocket launchers aimed across its face.
                let target = ahead(0.0, 30.0);
                for i in 0..16usize {
                    let x = i as f32 * 2.0 - 15.0;
                    let aim_at = target + Vec3::new(x * 0.5, cube * 5.0, 2.0);
                    shoot(ahead(x, 10.0) + Vec3::Y * 0.1, aim_at, 1);
                }
            }
        }
        let bricks = world.bricks.len();
        if bricks > 0 {
            session.command(
                builder,
                1,
                Command::LoadBuild {
                    build: Box::new(bri_world::build::SavedBuild::new(world)),
                    ownership: false,
                },
            )?;
        }
        // Let the load finish and vehicles settle.
        let mut settled = 0;
        for tick in 0..120 * 60 {
            session.step()?;
            if session.simulation().state().bricks.len() >= bricks {
                settled += 1;
            }
            if settled >= 240 {
                break;
            }
            ensure!(tick < 120 * 60 - 1, "The scene's build did not load");
        }
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ scene as u64);
        if drivers {
            let poses = session.vehicle_poses();
            ensure!(poses.len() >= 48, "Only {} vehicles spawned", poses.len());
            let mut seated = Vec::new();
            for (i, pose) in poses.iter().take(48).enumerate() {
                let top = Vec3::from(pose.position) + Vec3::Y * 3.0;
                let driver = session.join(format!("Driver {i}"), top, false)?;
                seated.push((driver, pose.id));
            }
            // Fall onto the vehicles; landing from above mounts them.
            for _ in 0..360 {
                session.step()?;
                if seated.iter().all(|(d, _)| session.mounted(*d).is_some()) {
                    break;
                }
            }
            let mounted = seated
                .iter()
                .filter(|(d, _)| session.mounted(*d).is_some())
                .count();
            ensure!(mounted >= 40, "Only {mounted} of 48 drivers mounted");
            // Full throttle with a steady turn: circles that stay in the field.
            for (driver, _) in &seated {
                let turn = if rng.next() < 0.5 { -1.0 } else { 1.0 } * (0.4 + rng.next() * 0.4);
                session.movement(
                    *driver,
                    1,
                    MoveInput {
                        forward: 1.0,
                        right: turn,
                        ..Default::default()
                    },
                )?;
            }
        }
        let mut local = Vec::new();
        if local_shooters {
            for (i, shooter) in shooters.iter().enumerate() {
                let owner = session.join(format!("Shooter {i}"), shooter.at, false)?;
                session.command(
                    owner,
                    1,
                    Command::EquipTool {
                        slot: Some(shooter.slot),
                    },
                )?;
                local.push((owner, shooter.input));
            }
        }
        let mut built = Built {
            session,
            observer,
            shooters,
            local,
        };
        // Face the target with the weapon out; clicking starts next.
        for sequence in 1..60 {
            built.step(sequence)?;
        }
        Ok(built)
    }
}

/// Step the scene's session at the host rate and time every step.
fn host(setup: &Setup, scene: Scene, seconds: f64) -> Result<serde_json::Value> {
    let mut built = setup.session(scene, true)?;
    let ticks = (seconds * 120.0) as usize;
    let mut steps = Samples::default();
    let mut projectiles = 0usize;
    let mut peak_projectiles = 0usize;
    let mut cues = 0usize;
    let profile = Profile::start();
    for tick in 0..ticks {
        steps.time(|| built.step(60 + tick as u64))?;
        let session = &mut built.session;
        let flying = session.weapon_view().projectiles.len();
        projectiles += flying;
        peak_projectiles = peak_projectiles.max(flying);
        cues += session.take_cues().len();
        // The server publishes (and clears) changed bricks every tick.
        drop(session.take_dirty());
    }
    let profile = profile.finish();
    let session = &built.session;
    let over = steps.ms.iter().filter(|s| **s > 1000.0 / 120.0).count();
    let dead = session
        .simulation()
        .state()
        .bricks
        .values()
        .filter(|b| !b.colliding)
        .count();
    Ok(json!({
        "ticks": ticks,
        "ticks_over_budget": over,
        "step": steps.report(),
        "vehicles": session.vehicle_infos().len(),
        "players": session.names().len(),
        "mean_projectiles": projectiles as f64 / ticks as f64,
        "peak_projectiles": peak_projectiles,
        "cues_per_second": cues as f64 / seconds,
        "bricks_knocked_out_at_end": dead,
        "profile": profile,
    }))
}

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    info: wgpu::AdapterInfo,
}
fn gpu() -> Result<Gpu> {
    pollster::block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                ..Default::default()
            })
            .await
            .context("No headless GPU adapter")?;
        let info = adapter.get_info();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("entity probe"),
                required_limits: adapter.limits(),
                ..Default::default()
            })
            .await?;
        Ok(Gpu {
            device,
            queue,
            info,
        })
    })
}

fn step_ui(app: &mut App, elapsed: Duration) -> Result<()> {
    app.tick(elapsed)?;
    app.ui.update(elapsed.as_millis() as u64);
    let commands = app.pump()?;
    ensure!(
        commands.is_empty(),
        "Unexpected window command {commands:?}"
    );
    if let ConnectionState::Failed { reason } = &app.ui.core.conn {
        bail!("Connection failed: {reason}");
    }
    Ok(())
}
fn in_game(app: &App) -> bool {
    matches!(app.ui.core.conn, ConnectionState::InGame { .. })
        && app
            .network_view()
            .is_some_and(|v| v.poses.contains_key(&v.owner))
}

/// Per-stage measurements of the client's frames.
#[derive(Default)]
struct Frames {
    /// `App::tick` plus the interface update: simulation and presentation.
    tick: Samples,
    /// `render_scene` and the interface draw, through queue submission.
    encode: Samples,
    /// Waiting for the GPU to finish the frame.
    gpu: Vec<f64>,
}
/// One frame the way the platform draws it: scene, then the interface.
fn frame(
    app: &mut App,
    gpu: &Gpu,
    ui: &mut bri_ui::gpu::UiRenderer,
    view: &wgpu::TextureView,
    elapsed: Duration,
    frames: &mut Frames,
) -> Result<()> {
    frames.tick.time(|| step_ui(app, elapsed))?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let index = frames.encode.time(|| -> Result<_> {
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        let drew = app.render_scene(&mut bri_client::platform::RenderContext {
            device: &gpu.device,
            queue: &gpu.queue,
            encoder: &mut encoder,
            target: view,
            format,
            size: SIZE,
            ui_renderer: ui,
        })?;
        ui.render(
            &gpu.device,
            &gpu.queue,
            &mut encoder,
            view,
            format,
            SIZE,
            app.ui.scale(),
            &app.ui.core.pack,
            &app.ui.draw(),
            (!drew).then_some(wgpu::Color::BLACK),
        );
        Ok(gpu.queue.submit([encoder.finish()]))
    })?;
    let t = Instant::now();
    gpu.device.poll(wgpu::PollType::Wait {
        submission_index: Some(index),
        timeout: Some(Duration::from_secs(30)),
    })?;
    frames.gpu.push(ms(t.elapsed()));
    Ok(())
}

/// A connected player that walks from the drop point to its spot, equips
/// its weapon, faces the target and holds the trigger, sending its input
/// 100 times a second, until `stop`. `arrived` counts players in place.
async fn hold_trigger(
    mut client: bri_net::client::Client,
    shooter: Shooter,
    arrived: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<()> {
    use std::sync::atomic::Ordering;
    client
        .command(Command::EquipTool {
            slot: Some(shooter.slot),
        })
        .await?;
    let owner = client.owner;
    let mut sequence = 0u64;
    let mut interval = tokio::time::interval(Duration::from_millis(10));
    let mut in_place: Option<Instant> = None;
    let mut pulled = false;
    let mut seen = 0;
    let started = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        tokio::select! {
            _ = interval.tick() => {
                let input = match (in_place, client.replica.poses.get(&owner)) {
                    (None, Some(pose)) => {
                        let feet = Vec3::from(pose.player.feet);
                        let to = (shooter.at - feet) * Vec3::new(1.0, 0.0, 1.0);
                        if to.length() < 0.3 || started.elapsed() > Duration::from_secs(20) {
                            in_place = Some(Instant::now());
                            arrived.fetch_add(1, Ordering::Relaxed);
                            shooter.input
                        } else {
                            MoveInput {
                                yaw: to.x.atan2(-to.z),
                                forward: (to.length() * 0.5).min(1.0),
                                ..Default::default()
                            }
                        }
                    }
                    _ => shooter.input,
                };
                sequence += 1;
                client.movement(sequence, &[input], None)?;
                // Click like a player spamming fire: press every 300 ms,
                // release 150 ms later; the weapon's own states pace it.
                if let Some(at) = in_place.filter(|t| t.elapsed() > Duration::from_millis(500)) {
                    let phase = (at.elapsed().as_millis() / 150) % 2 == 0;
                    if phase != pulled {
                        pulled = phase;
                        client.command(Command::WeaponTrigger { down: phase }).await?;
                    }
                }
            }
            event = client.receive() => { event?; }
        }
        // Nothing presents these players' cues; drop them as a player
        // would play them.
        drop(client.replica.take_cues());
        seen = seen.max(client.replica.weapons.projectiles.len());
    }
    if std::env::var_os("BRI_PROBE_DEBUG").is_some() {
        eprintln!(
            "{owner} saw at most {seen} projectiles; images {:?}",
            client.replica.weapons.images.get(&owner)
        );
    }
    client.close();
    Ok(())
}

/// Serve the scene on loopback and time a joined guest's frames.
#[allow(clippy::too_many_arguments)] // probe harness inputs
fn client(
    setup: &Setup,
    runtime: &tokio::runtime::Runtime,
    app: &mut App,
    gpu: &Gpu,
    ui: &mut bri_ui::gpu::UiRenderer,
    scene: Scene,
    seconds: f64,
    snapshot: &Path,
) -> Result<serde_json::Value> {
    let Built {
        session,
        observer,
        shooters,
        ..
    } = setup.session(scene, false)?;
    let environment = setup.paths.environment()?;
    let packages = environment.client_packages();
    // Everyone joins at the observer's spot; shooters walk to theirs first.
    let spawn_points = vec![observer];
    let server = {
        let _guard = runtime.enter();
        server::start(
            session,
            ServerOptions {
                bind: "127.0.0.1:0".parse()?,
                environment,
                spawn_points,
                certificate: None,
                map_loader: None,
                autosave: None,
                packages: None,
            },
        )?
    };
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("entity probe frame"),
        size: wgpu::Extent3d {
            width: SIZE.0,
            height: SIZE.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    // Network players hold the trigger, sending input like real clients.
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let arrived = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut players = Vec::new();
    let expected = shooters.len();
    for (i, shooter) in shooters.into_iter().enumerate() {
        let client = runtime.block_on(bri_net::client::Client::connect(
            server.address,
            &server.certificate,
            format!("Shooter {i}"),
            packages.clone(),
            None,
        ))?;
        players.push(runtime.spawn(hold_trigger(client, shooter, arrived.clone(), stop.clone())));
    }
    let walking = Instant::now();
    while arrived.load(std::sync::atomic::Ordering::Relaxed) < expected {
        if let Some(i) = players.iter().position(|p| p.is_finished()) {
            runtime
                .block_on(players.remove(i))?
                .context("A shooter stopped before reaching its spot")?;
        }
        ensure!(
            walking.elapsed() < Duration::from_secs(60),
            "Shooters did not reach their spots"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    app.ui.core.request(UiAction::JoinServer {
        address: format!("127.0.0.1:{}", server.address.port()),
        password: String::new(),
    });
    let start = Instant::now();
    let mut previous = Instant::now();
    let mut scratch = Frames::default();
    loop {
        let now = Instant::now();
        let elapsed = now.duration_since(previous).min(Duration::from_millis(100));
        previous = now;
        frame(app, gpu, ui, &view, elapsed, &mut scratch)?;
        if in_game(app) {
            break;
        }
        ensure!(start.elapsed() < Duration::from_secs(180), "Join timed out");
        std::thread::sleep(Duration::from_millis(4));
    }
    let join_s = start.elapsed().as_secs_f64();
    // Frames start on a 60 Hz clock, like a vsynced display, so each frame
    // advances the same game time from run to run; a slow frame starts the
    // next one late. BRI_PROBE_UNCAPPED=1 runs flat out instead.
    let uncapped = std::env::var_os("BRI_PROBE_UNCAPPED").is_some();
    let period = Duration::from_secs_f64(1.0 / 60.0);
    let mut next = Instant::now();
    let mut paced = |previous: &mut Instant| {
        if !uncapped {
            let now = Instant::now();
            if next > now {
                std::thread::sleep(next - now);
            }
            next = next.max(Instant::now() - period) + period;
        }
        let now = Instant::now();
        let elapsed = now.duration_since(*previous);
        *previous = now;
        (now, elapsed)
    };
    // Warm up: settle the world, uploads and effects.
    let warm = Instant::now();
    while warm.elapsed() < Duration::from_secs(3) {
        let (_, elapsed) = paced(&mut previous);
        frame(app, gpu, ui, &view, elapsed, &mut scratch)?;
    }
    let mut frames = Frames::default();
    let mut totals = Vec::new();
    let mut host_tick_mean = Vec::new();
    let mut host_tick_max = 0f32;
    let mut host_rate = Vec::new();
    let profile = Profile::start();
    let measured = Instant::now();
    let mut next_perf = Instant::now() + Duration::from_secs(1);
    while measured.elapsed().as_secs_f64() < seconds {
        let (now, elapsed) = paced(&mut previous);
        frame(app, gpu, ui, &view, elapsed, &mut frames)?;
        totals.push(ms(now.elapsed()));
        if Instant::now() >= next_perf {
            next_perf += Duration::from_secs(1);
            let perf = server
                .perf
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            if perf.ticks_per_second > 0.0 {
                host_tick_mean.push(f64::from(perf.tick_ms_mean));
                host_tick_max = host_tick_max.max(perf.tick_ms_max);
                host_rate.push(f64::from(perf.ticks_per_second));
            }
        }
    }
    let wall = measured.elapsed().as_secs_f64();
    let profile = profile.finish();
    let count = totals.len();
    let over = totals.iter().filter(|t| **t > 1000.0 / 60.0).count();
    let counts = app.entity_counts();
    if let Some(parent) = snapshot.parent() {
        std::fs::create_dir_all(parent)?;
    }
    save_frame(gpu, &target, snapshot)?;
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    for player in players {
        runtime.block_on(player)??;
    }
    app.ui.core.request(UiAction::Disconnect);
    for _ in 0..30 {
        let _ = step_ui(app, Duration::from_millis(16));
        std::thread::sleep(Duration::from_millis(10));
    }
    let _ = runtime.block_on(server.stop());
    for _ in 0..10 {
        let _ = step_ui(app, Duration::from_millis(16));
    }
    app.ui.core.pop(bri_ui::screens::ScreenId::MessageBox);
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len().max(1) as f64;
    Ok(json!({
        "join_seconds": join_s,
        "paced_60hz": !uncapped,
        "frames": count,
        "fps": count as f64 / wall,
        "frames_over_16_7ms": over,
        "frame": percentiles(&mut totals),
        "tick": frames.tick.report(),
        "scene_encode": frames.encode.report(),
        "gpu_wait": percentiles(&mut frames.gpu),
        "host_while_serving": {
            "tick_ms_mean": mean(&host_tick_mean),
            "tick_ms_max": host_tick_max,
            "ticks_per_second": mean(&host_rate),
        },
        "entities": counts,
        "profile": profile,
    }))
}

fn save_frame(gpu: &Gpu, target: &wgpu::Texture, path: &Path) -> Result<()> {
    let (width, height) = (target.width(), target.height());
    let row = (width * 4).div_ceil(256) * 256;
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("entity probe readback"),
        size: u64::from(row) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(height),
            },
        },
        target.size(),
    );
    gpu.queue.submit([encoder.finish()]);
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    gpu.device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(Duration::from_secs(30)),
    })?;
    let mapped = buffer
        .slice(..)
        .get_mapped_range()
        .map_err(|e| anyhow::anyhow!("readback: {e:?}"))?;
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for line in mapped.chunks_exact(row as usize) {
        pixels.extend_from_slice(&line[..width as usize * 4]);
    }
    image::save_buffer(path, &pixels, width, height, image::ColorType::Rgba8)?;
    Ok(())
}

/// What removing bricks costs the host in a world of `count` bricks: one
/// brick at a time (a hammer hit each), and a batch of 64 (a collapse or
/// blast). `BRI_PROBE_REMOVE=100000,1000000` runs it.
fn removal(setup: &Setup, count: usize) -> Result<serde_json::Value> {
    let loaded = setup.paths.load_map(MAP, None)?;
    let definitions = loaded.simulation.definitions.clone();
    let spawn = loaded.spawn_points[0];
    let height = definitions
        .entries
        .get(CUBE)
        .context("No cube brick")?
        .mesh
        .height_plates as f32
        * 0.2;
    let mut world = World::new(
        "Removal probe".into(),
        loaded.simulation.state().map_id.clone(),
        loaded.simulation.state().palette.clone(),
    );
    // Layers of a square of 2x2 bricks, one unit apart, on the ground.
    let side = ((count as f32 / 16.0).sqrt().ceil() as usize).max(1);
    let ground = (spawn.y / 0.2).round() * 0.2;
    for i in 0..count {
        let (layer, cell) = (i / (side * side), i % (side * side));
        let (x, z) = ((cell % side) as f32, (cell / side) as f32);
        let brick = Brick::new(
            ContentRef::Resolved(CUBE.into()),
            [
                spawn.x.round() + x - side as f32 * 0.5,
                ground + height * (layer as f32 + 0.5),
                spawn.z.round() + z - side as f32 * 0.5,
            ],
            1,
        );
        world.bricks.insert(i as u64 + 1, brick);
    }
    world.next_brick_id = count as u64 + 1;
    // Live heap of the world alone, then with its collision built.
    let world_bytes = live_bytes();
    let started = Instant::now();
    let mut simulation = bri_sim::simulation::Simulation::new(world, definitions, Vec::new())?;
    let build_ms = ms(started.elapsed());
    let built_bytes = live_bytes();
    let actor = bri_world::authority::Actor {
        administrator: true,
        ..Default::default()
    };
    // Top-layer bricks, so nothing rests on what is removed.
    let top: Vec<u64> = (count - (count % (side * side)).max(side * side).min(count)..count)
        .map(|i| i as u64 + 1)
        .collect();
    let mut singles = Samples::default();
    let profile = Profile::start();
    // The world's first step after loading settles its broad phase; take
    // it before timing removals.
    simulation.step()?;
    for id in top.iter().take(20) {
        singles.time(|| simulation.remove(&actor, *id))?;
    }
    let profile = profile.finish();
    let first_mcycles = singles.mcycles.first().copied();
    let mut batch = Samples::default();
    for chunk in top[20..].chunks(64).take(5) {
        batch.time(|| simulation.remove_many(&actor, chunk))?;
    }
    Ok(json!({
        "bricks": count,
        "build_ms": build_ms,
        "simulation_bytes_per_brick": (built_bytes - world_bytes) as f64 / count as f64,
        "remove_one_first_mcycles": first_mcycles,
        "remove_one": singles.report(),
        "remove_64": batch.report(),
        "profile": profile,
    }))
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    ensure!(
        args.len() >= 2,
        "Usage: entity_probe <content-root> <out-dir> [scene ...]"
    );
    let root = PathBuf::from(&args[0]);
    let out = PathBuf::from(&args[1]);
    let scenes: Vec<Scene> = if args.len() > 2 {
        args[2..]
            .iter()
            .map(|name| {
                Scene::ALL
                    .into_iter()
                    .find(|s| s.name() == name)
                    .with_context(|| format!("Unknown scene {name}"))
            })
            .collect::<Result<_>>()?
    } else {
        Scene::ALL.to_vec()
    };
    let seconds: f64 = std::env::var("BRI_PROBE_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10.0);
    let only = std::env::var("BRI_PROBE_ONLY").ok();
    std::fs::create_dir_all(&out)?;
    let packages = bri_package::packages::PackageSet::load_root(&root)?;
    let setup = Setup {
        paths: ContentPaths::resolve(&root, &packages)?,
    };
    let mut report = serde_json::Map::new();
    if let Ok(counts) = std::env::var("BRI_PROBE_REMOVE") {
        let mut removals = serde_json::Map::new();
        for count in counts.split(',') {
            let count: usize = count.trim().parse()?;
            println!("removal {count}");
            let result = removal(&setup, count)?;
            println!("{}", serde_json::to_string(&result)?);
            removals.insert(count.to_string(), result);
        }
        report.insert("removal".into(), removals.into());
        let path = out.join("report.json");
        std::fs::write(&path, serde_json::to_vec_pretty(&report)?)?;
        return Ok(());
    }
    if only.as_deref() != Some("client") {
        let mut hosts = serde_json::Map::new();
        for &scene in &scenes {
            println!("host {}", scene.name());
            let result = host(&setup, scene, seconds)?;
            println!("{}", serde_json::to_string(&result)?);
            hosts.insert(scene.name().into(), result);
        }
        report.insert("host".into(), hosts.into());
    }
    if only.as_deref() != Some("host") {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        let gpu = gpu()?;
        let state = out.join("client-state");
        let _ = std::fs::remove_dir_all(&state);
        std::fs::create_dir_all(&state)?;
        let mut app = App::load(&root, &state, SIZE)?;
        app.ui.core.pop(bri_ui::screens::ScreenId::DefaultControls);
        app.ui.core.settings.avatar.lan_name = "Observer".into();
        let mut ui = bri_ui::gpu::UiRenderer::new(&gpu.device, &gpu.queue);
        app.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
        let mut clients = serde_json::Map::new();
        clients.insert(
            "adapter".into(),
            format!("{} ({:?})", gpu.info.name, gpu.info.backend).into(),
        );
        clients.insert("resolution".into(), json!(SIZE));
        for &scene in &scenes {
            println!("client {}", scene.name());
            let result = client(
                &setup,
                &runtime,
                &mut app,
                &gpu,
                &mut ui,
                scene,
                seconds,
                &out.join(format!("{}.png", scene.name())),
            )?;
            println!("{}", serde_json::to_string(&result)?);
            clients.insert(scene.name().into(), result);
        }
        report.insert("client".into(), clients.into());
    }
    let path = out.join("report.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&report)?)?;
    println!("Wrote {}", path.display());
    Ok(())
}
