//! A soak of the normal App's world-change path, every frame drawn
//! offscreen: a Brick Damage mini-game, rockets blowing out a few dozen
//! bricks of varied shapes and colours, the debris expiring and the bricks
//! returning while a building tool is out, deaths and respawns, weapon
//! switches, first and third person, and a Change Map. Everything goes
//! through ordinary UI actions to the hosted server over loopback QUIC, and
//! back through the app's own network events, chunk jobs and render
//! preparation, which the isolated chunk benchmarks bypass (the delayed
//! brick-return hitch slipped through them).
//!
//! It counts the costly kinds of work (`App::work_counters`): whole scene
//! uploads and the textures they make, world-item model builds, debris look
//! builds, passes over the whole world. Counts, unlike frame times, are the
//! same on every machine, so the bounds are exact. The first blast and the
//! first of each action warm up; the same actions again (steady play) must
//! build and upload nothing whole, and must not read the whole world.
//!
//! Runs on the made-up content root (`support::content_root`); the ignored
//! variant on the generated v20 content (`--release -- --ignored`). Needs
//! an offscreen adapter, as every App render test does; never opens a
//! window, audio device or OS input. About 40 s of game time; the long
//! variant repeats each steady action (`BRI_SOAK_ROUNDS`).
use anyhow::{Context, Result, bail, ensure};
use bri_client::{
    app::App,
    perf::WorkCounters,
    platform::{PlatformApp, RenderContext},
};
use bri_ui::{api::*, gpu::UiRenderer, models::admin::AdminAction, screens::ScreenId};
use glam::Vec3;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

#[macro_use]
mod support;
use support::{content_root::ContentRoot, wait};

synthetic_and_content!(
    ContentRoot: steady_play_builds_and_uploads_nothing_whole,
    a_tap_inside_one_frame_still_fires
);

const SIZE: (u32, u32) = (320, 240);
/// Bricks blown out by each rocket: 6 wide, 6 high, in front of the player.
const WALL: usize = 6;
/// The mini-game's tools: the hammer shows hidden bricks (`showBricks`).
const HAMMER_SLOT: usize = 0;
const ROCKET_SLOT: usize = 1;
const GUN_SLOT: usize = 2;

/// The offscreen frame every step draws into.
struct Screen {
    gpu: support::gpu::Turn,
    ui: UiRenderer,
    view: wgpu::TextureView,
    _target: wgpu::Texture,
}
impl Screen {
    fn new() -> Result<Self> {
        let gpu = support::gpu::turn().context("an offscreen adapter")?;
        let ui = UiRenderer::new(&gpu.device, &gpu.queue);
        let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("soak frame"),
            size: wgpu::Extent3d {
                width: SIZE.0,
                height: SIZE.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        Ok(Self {
            gpu,
            ui,
            view,
            _target: target,
        })
    }
    fn draw(&mut self, app: &mut App) -> Result<()> {
        let mut encoder = self.gpu.device.create_command_encoder(&Default::default());
        app.render_scene(&mut RenderContext {
            device: &self.gpu.device,
            queue: &self.gpu.queue,
            encoder: &mut encoder,
            target: &self.view,
            format: wgpu::TextureFormat::Rgba8Unorm,
            size: SIZE,
            ui_renderer: &mut self.ui,
        })?;
        self.gpu.queue.submit([encoder.finish()]);
        // One frame in flight, as a window's swap chain keeps it.
        self.gpu.device.poll(wgpu::PollType::wait_indefinitely())?;
        Ok(())
    }
}

/// The work counts of every frame of a phase.
#[derive(Default)]
struct Soak {
    frames: u64,
    /// The most whole scene uploads and textures any one frame made.
    worst_scenes: u64,
    worst_textures: u64,
    last: Option<WorkCounters>,
}
impl Soak {
    fn frame(&mut self, app: &App) {
        let now = app.work_counters();
        if let (Some(before), Some(after)) = (
            self.last.and_then(|w| w.uploads),
            now.uploads.filter(|_| self.last.is_some()),
        ) && after.scenes >= before.scenes
        {
            self.worst_scenes = self.worst_scenes.max(after.scenes - before.scenes);
            self.worst_textures = self.worst_textures.max(after.textures - before.textures);
        }
        self.frames += 1;
        self.last = Some(now);
    }
}

struct Run<'a> {
    app: Box<App>,
    screen: Screen,
    soak: Soak,
    content: &'a ContentRoot,
    state: PathBuf,
    /// Each shot of the player's seen: what it is, where it left and
    /// where it was last, for a blast that knocks nothing out.
    shots: std::collections::BTreeMap<u64, (String, Vec3, Vec3)>,
}
impl Run<'_> {
    fn step(&mut self, elapsed: Duration) -> Result<()> {
        self.app.tick(elapsed)?;
        self.app.ui.update(elapsed.as_millis() as u64);
        ensure!(
            self.app.pump()?.is_empty(),
            "Unexpected native window command in headless test"
        );
        if let ConnectionState::Failed { reason } = &self.app.ui.core.conn {
            bail!("Native app connection failed: {reason}");
        }
        if let Some(view) = self.app.network_view() {
            for p in view.weapons.fired().filter(|p| p.source.0 == view.owner) {
                self.shots
                    .entry(p.id)
                    .or_insert((p.definition.clone(), p.origin, p.position))
                    .2 = p.position;
            }
            self.screen.draw(&mut self.app)?;
        }
        self.soak.frame(&self.app);
        Ok(())
    }
    /// Step until `ready`, failing after `game` of game time once in game,
    /// or when the app stops advancing for [`wait::STALL`].
    fn until(&mut self, what: &str, game: Duration, ready: impl Fn(&App) -> bool) -> Result<()> {
        let mut previous = Instant::now();
        let mut moved = previous;
        let mut mark = (self.tick(), self.app.loading_revision());
        let mut ticks = 0;
        let mut last = self.app.network_view().map(|v| v.tick);
        loop {
            let now = Instant::now();
            self.step(now.duration_since(previous))?;
            previous = now;
            if ready(&self.app) {
                return Ok(());
            }
            let tick = self.app.network_view().map(|v| v.tick);
            if let (Some(a), Some(b)) = (last, tick) {
                ticks += b.saturating_sub(a);
            }
            last = tick;
            if ticks >= wait::ticks(game) {
                bail!(
                    "Timed out waiting for {what}: {game:?} of game time; {}",
                    self.state()
                );
            }
            let latest = (self.tick(), self.app.loading_revision());
            if latest != mark {
                mark = latest;
                moved = now;
            } else if now.duration_since(moved) >= wait::STALL {
                bail!(
                    "Waiting for {what}: the app stopped advancing; {}",
                    self.state()
                );
            }
            std::thread::sleep(Duration::from_millis(4));
        }
    }
    fn run_for(&mut self, seconds: f32) -> Result<()> {
        let until = self.tick() + wait::ticks(Duration::from_secs_f32(seconds));
        self.until(&format!("{seconds} s"), Duration::from_secs(60), |a| {
            a.network_view().is_some_and(|v| v.tick >= until)
        })
    }
    fn action(&mut self, action: UiAction) -> Result<()> {
        self.app.ui.core.request(action);
        self.step(Duration::ZERO)
    }
    fn tick(&self) -> u64 {
        self.app.network_view().map_or(0, |v| v.tick)
    }
    fn state(&self) -> String {
        let view = self.app.network_view();
        format!(
            "screens {:?}; chat {:?}; conn {:?}; bricks {:?}; images {:?}; vitals {:?}; \
             shots {:?}; player {:?}; wall {:?}",
            self.app.ui.stack(),
            self.app
                .ui
                .core
                .chat
                .lines
                .iter()
                .rev()
                .take(6)
                .map(|l| format!("{l:?}"))
                .collect::<Vec<_>>(),
            self.app.ui.core.conn,
            view.map(|v| (
                v.world.bricks.len(),
                v.world.bricks.values().filter(|b| !b.visible).count()
            )),
            view.and_then(|v| v.weapons.images.get(&v.owner)),
            view.and_then(|v| v.vitals.get(&v.owner)),
            self.shots,
            self.app
                .local_motion()
                .map(|(p, eye)| (p.feet, p.yaw, p.pitch, eye)),
            view.map(|v| {
                v.world.bricks.values().fold(
                    (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
                    |(lo, hi), b| {
                        let at = Vec3::from(b.position);
                        (lo.min(at), hi.max(at))
                    },
                )
            }),
        )
    }
    fn use_tool(&mut self, slot: usize) -> Result<()> {
        self.action(UiAction::UseTool { slot })?;
        self.until(
            &format!("tool {slot} in hand"),
            Duration::from_secs(10),
            |a| held_in(a, slot),
        )
    }
    fn trigger(&mut self, down: bool) -> Result<()> {
        self.action(UiAction::Game(GameAction::Held {
            control: HeldControl::Fire,
            down,
        }))
    }
    /// Fire the tool in hand as a player does: hold the trigger until the
    /// image takes the press, then let go. A press the image never takes
    /// fails here, naming it, not as a missing blast later. On a slow
    /// machine one frame can span a whole fire cycle (back to "Ready"), so
    /// a shot in flight or bricks it knocked out count as taken too.
    fn fire(&mut self) -> Result<()> {
        self.trigger(true)?;
        self.until(
            "the image in hand to take the press (leave \"Ready\")",
            Duration::from_secs(2),
            |a| {
                a.network_view().is_some_and(|v| {
                    v.weapons.images.get(&v.owner).is_some_and(|images| {
                        images.iter().any(|m| m.hand == 0 && m.state != "Ready")
                    }) || v.weapons.fired().any(|p| p.source.0 == v.owner)
                }) || knocked_out(a) > 0
            },
        )?;
        self.trigger(false)?;
        self.run_for(0.1)
    }
    /// A click with nothing to fire: a dead player's click to respawn.
    fn click(&mut self) -> Result<()> {
        for down in [true, false] {
            self.trigger(down)?;
            self.run_for(0.1)?;
        }
        Ok(())
    }
}
fn held_in(app: &App, slot: usize) -> bool {
    let Some(view) = app.network_view() else {
        return false;
    };
    let Some(item) = view
        .tools
        .get(&view.owner)
        .and_then(|t| t.slots.get(slot).cloned().flatten())
    else {
        return false;
    };
    let Some(image) = app.content.weapons.pack.items.get(&item).map(|i| &i.image) else {
        return false;
    };
    view.weapons.images.get(&view.owner).is_some_and(|images| {
        images
            .iter()
            .any(|m| &m.image == image && m.state == "Ready")
    })
}
fn knocked_out(app: &App) -> usize {
    app.network_view().map_or(0, |v| {
        v.world.bricks.values().filter(|b| !b.visible).count()
    })
}
fn alive(app: &App) -> Option<(bool, u64)> {
    let view = app.network_view()?;
    view.vitals
        .get(&view.owner)
        .map(|v| (v.alive, v.spawn_tick))
}

/// What one phase did, from its first frame to its last.
#[derive(Debug)]
#[allow(dead_code)] // Every field is reported (Debug); some are only reported.
struct Phase {
    name: &'static str,
    frames: u64,
    /// Game ticks the phase ran, which every machine runs alike.
    ticks: u64,
    /// Wall time, reported only: a slow machine takes longer.
    wall_seconds: f32,
    counted: bool,
    scenes: u64,
    textures: u64,
    worst_scenes: u64,
    item_model_builds: u64,
    debris_looks_built: u64,
    music_full_scans: u64,
    hidden_full_scans: u64,
    hidden_visited: u64,
    chunk_jobs: u64,
    chunks_rebuilt: u64,
}
fn phase(
    name: &'static str,
    run: &mut Run,
    body: impl FnOnce(&mut Run) -> Result<()>,
) -> Result<Phase> {
    run.soak = Soak::default();
    run.soak.frame(&run.app);
    let before = run.app.work_counters();
    let started = Instant::now();
    let first_tick = run.tick();
    body(run)?;
    let after = run.app.work_counters();
    let uploads = |w: &WorkCounters| w.uploads.unwrap_or_default();
    let p = Phase {
        name,
        frames: run.soak.frames,
        ticks: run.tick().saturating_sub(first_tick),
        wall_seconds: started.elapsed().as_secs_f32(),
        // One scene renderer counted the phase from end to end: a renderer
        // still compiling, or rebuilt during it, would hide its uploads.
        counted: matches!(
            (before.uploads, after.uploads),
            (Some(b), Some(a)) if a.scenes >= b.scenes && a.textures >= b.textures
        ),
        scenes: uploads(&after)
            .scenes
            .saturating_sub(uploads(&before).scenes),
        textures: uploads(&after)
            .textures
            .saturating_sub(uploads(&before).textures),
        worst_scenes: run.soak.worst_scenes,
        item_model_builds: after
            .item_model_builds
            .saturating_sub(before.item_model_builds),
        debris_looks_built: after
            .debris_looks_built
            .saturating_sub(before.debris_looks_built),
        music_full_scans: after
            .music_full_scans
            .saturating_sub(before.music_full_scans),
        hidden_full_scans: (after.hidden_outlines.full_scans)
            .saturating_sub(before.hidden_outlines.full_scans),
        hidden_visited: (after.hidden_outlines.visited)
            .saturating_sub(before.hidden_outlines.visited),
        chunk_jobs: after.chunk_jobs.saturating_sub(before.chunk_jobs),
        chunks_rebuilt: after.chunks_rebuilt.saturating_sub(before.chunks_rebuilt),
    };
    eprintln!("{p:?}");
    Ok(p)
}

/// Turn the player to face along `yaw` exactly (Look is relative and
/// scaled by the view's settings, so it is corrected until it holds).
fn aim(run: &mut Run, yaw: f32) -> Result<()> {
    for _ in 0..20 {
        let (player, _) = run.app.local_motion().context("local player")?;
        let off = (yaw - player.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        if off.abs() < 0.002 && player.pitch.abs() < 0.002 {
            return Ok(());
        }
        run.action(UiAction::Game(GameAction::Look {
            yaw: off * 0.5 / 0.0174,
            pitch: player.pitch * 0.5 / 0.0174,
        }))?;
        run.run_for(0.15)?;
    }
    let (player, _) = run.app.local_motion().context("local player")?;
    bail!(
        "could not aim at yaw {yaw}: {} / {}",
        player.yaw,
        player.pitch
    )
}

/// A wall of bricks of every menu shape and paint, square across the axis
/// the player faces most nearly, the player turned to face its middle;
/// loaded as a save owned by them.
fn load_wall(run: &mut Run, name: &str) -> Result<usize> {
    let f = run.content;
    let (player, _) = run.app.local_motion().context("local player")?;
    let yaw = (player.yaw / std::f32::consts::FRAC_PI_2).round() * std::f32::consts::FRAC_PI_2;
    aim(run, yaw)?;
    let (player, eye) = run.app.local_motion().context("local player")?;
    let view = run.app.network_view().context("view")?;
    let forward = Vec3::new(yaw.sin(), 0.0, -yaw.cos()).round();
    let side = Vec3::new(forward.z.abs(), 0.0, forward.x.abs());
    let feet = Vec3::from(player.feet);
    let eye = eye.unwrap_or(feet + Vec3::Y * 1.8);
    let map_id = view.world.map_id.clone();
    let palette = view.world.palette.clone();
    // Each shape with its footprint in studs and height in plates.
    let shapes: Vec<(String, [u8; 2], u16)> = if f.content {
        vec![(f.brick.clone(), [2, 2], 3)]
    } else {
        bri_client::testing::content_root::MENU_BRICKS
            .iter()
            .map(|(id, _, studs, plates)| (id.to_string(), *studs, *plates))
            .collect()
    };
    let mut world = bri_world::World::new(name.into(), map_id.clone(), palette.clone());
    let mut id = 1;
    // Columns a metre apart across the line of fire, centred on it, 7 m
    // ahead; rows 0.6 m apart round the eye.
    let ahead = (feet + forward * 7.0).round() * (Vec3::ONE - side);
    let across = (feet * side).round();
    let base = ((eye.y - 1.5) / 0.2).round() * 0.2;
    for row in 0..WALL {
        for column in 0..WALL {
            let lateral = column as f32 - (WALL / 2) as f32;
            let at = ahead + across + side * lateral;
            let (shape, [w, d], plates) = &shapes[(row + column) % shapes.len()];
            // An odd number of studs centres between grid lines.
            let half = |studs: u8| if studs % 2 == 1 { 0.25 } else { 0.0 };
            let mut brick = bri_world::Brick::new(
                bri_world::ContentRef::Resolved(shape.clone()),
                [
                    at.x + half(*w),
                    base + row as f32 * 0.6 + f32::from(*plates) * 0.1,
                    at.z + half(*d),
                ],
                view.owner,
            );
            brick.color = ((row * WALL + column) % palette.len()) as u8;
            world.bricks.insert(id, brick);
            id += 1;
        }
    }
    world.next_brick_id = id;
    let count = world.bricks.len();
    let build = bri_world::build::SavedBuild::new(world);
    use sha2::Digest;
    let state = run.state.clone();
    let folder = state
        .join("saves")
        .join(format!("map-{:x}", sha2::Sha256::digest(map_id.as_bytes())));
    std::fs::create_dir_all(&folder)?;
    let file = format!("{name}.world.json");
    std::fs::write(folder.join(&file), serde_json::to_vec(&build)?)?;
    let content = bri_client::content::ClientContent::load(&f.root)?;
    let map = bri_client::saves::Store::new(&state, &content, None).map_name(&map_id);
    let before = run.app.network_view().map_or(0, |v| v.world.bricks.len());
    run.action(UiAction::LoadBricks {
        map,
        name: file,
        ownership: true,
    })?;
    run.until("the wall to load", Duration::from_secs(30), |a| {
        a.network_view()
            .is_some_and(|v| v.world.bricks.len() >= before + count)
            && a.world_render_ready()
    })?;
    Ok(count)
}

/// A rocket into the wall, then out with the hammer until every brick is
/// back and (`expire`) its debris gone.
fn blast(run: &mut Run, bricks: usize, expire: bool) -> Result<usize> {
    run.use_tool(ROCKET_SLOT)?;
    run.fire()?;
    run.until("bricks knocked out", Duration::from_secs(5), |a| {
        knocked_out(a) >= bricks / 2
    })?;
    let out = knocked_out(&run.app);
    // The hammer shows hidden bricks while they come back.
    run.use_tool(HAMMER_SLOT)?;
    run.until(
        "bricks back and debris gone",
        Duration::from_secs(60),
        |a| {
            knocked_out(a) == 0
                && (!expire || a.brick_debris_count() == 0)
                && a.world_render_ready()
        },
    )
    .with_context(|| format!("debris {}", run.app.brick_debris_count()))?;
    Ok(out)
}
/// One round of each steady action, about a minute of game time.
fn steady_play_builds_and_uploads_nothing_whole(f: &ContentRoot) -> Result<()> {
    soak(f, 1)
}

/// The long soak: `BRI_SOAK_ROUNDS` (default 5) rounds of each steady
/// action on the made-up root (`--release -- --ignored`).
#[test]
#[ignore = "long soak, several minutes; the default variant runs one round"]
fn long_soak_builds_and_uploads_nothing_whole() -> Result<()> {
    let rounds = std::env::var("BRI_SOAK_ROUNDS").map_or(Ok(5), |r| r.parse())?;
    soak(&ContentRoot::synthetic()?, rounds)
}

/// A tap shorter than a frame: on a slow machine or in a hitch, the press
/// and the release reach the game in one frame, so they leave in one send
/// and can reach the host inside one of its ticks. The host takes each
/// edge on its own tick (`Session::step_weapons`), so the press is held
/// for at least one tick, as v20 registers a click: the rocket flies.
fn a_tap_inside_one_frame_still_fires(f: &ContentRoot) -> Result<()> {
    let state = f.state()?;
    let (mut run, bricks) = start(f, state.path())?;
    run.use_tool(ROCKET_SLOT)?;
    for down in [true, false] {
        run.app.ui.core.request(UiAction::Game(GameAction::Held {
            control: HeldControl::Fire,
            down,
        }));
    }
    run.step(Duration::ZERO)?;
    run.until(
        "the tap's rocket to knock bricks out",
        Duration::from_secs(5),
        |a| knocked_out(a) >= bricks / 2,
    )?;
    run.app.gpu_stopped();
    Ok(())
}

/// A hosted Brick Damage mini-game with the soak's tools, the player
/// standing still before a wall of bricks (their count returned), the
/// map's lighting finished.
fn start<'a>(f: &'a ContentRoot, state: &std::path::Path) -> Result<(Run<'a>, usize)> {
    let mut app = App::load(&f.root, state, SIZE)?;
    app.ui.core.pop(ScreenId::DefaultControls);
    let screen = Screen::new()?;
    app.gpu_ready(
        &screen.gpu.device,
        &screen.gpu.queue,
        wgpu::TextureFormat::Rgba8Unorm,
    )?;
    let mut run = Run {
        app,
        screen,
        soak: Soak::default(),
        shots: Default::default(),
        content: f,
        state: state.to_path_buf(),
    };
    run.action(UiAction::HostGame {
        map: f.map.0.clone(),
        mode: ServerMode::SinglePlayer,
        game_mode: None,
        max_players: 1,
        server_name: "Soak".into(),
        password: String::new(),
        admin_password: String::new(),
        super_admin_password: String::new(),
    })?;
    run.until("host and player", Duration::from_secs(120), |a| {
        matches!(a.ui.core.conn, ConnectionState::InGame { .. })
            && a.network_view()
                .is_some_and(|v| v.poses.contains_key(&v.owner))
            && a.world_render_ready()
    })?;
    let (rocket, gun) = if f.content {
        ("v20.weapon.rocketlauncheritem", "v20.weapon.gunitem")
    } else {
        (
            bri_weapons::testing::ROCKET_ITEM,
            bri_weapons::testing::GUN_ITEM,
        )
    };
    let mut loadout: [Option<String>; 5] = Default::default();
    loadout[HAMMER_SLOT] = Some(bri_weapons::HAMMER.into());
    loadout[ROCKET_SLOT] = Some(rocket.into());
    loadout[GUN_SLOT] = Some(gun.into());
    let rules = support::minigame::offered(
        &run.app,
        MiniGameRules {
            title: "Soak".into(),
            respawn_seconds: 1,
            brick_respawn_seconds: 2,
            brick_damage: true,
            self_damage: false,
            loadout,
            ..Default::default()
        },
    );
    ensure!(
        rules.loadout.iter().flatten().count() == 3,
        "the content offers the soak's tools: {:?}",
        rules.loadout
    );
    run.action(UiAction::CreateMiniGame { color: 0, rules })?;
    run.until("the mini-game's tools", Duration::from_secs(10), |a| {
        a.network_view().is_some_and(|v| {
            v.tools
                .get(&v.owner)
                .is_some_and(|t| t.slots[ROCKET_SLOT].is_some())
        })
    })?;
    run.until("standing still", Duration::from_secs(10), |a| {
        a.local_motion()
            .is_some_and(|(p, _)| p.grounded && Vec3::from(p.velocity).length() < 0.01)
    })?;
    let bricks = load_wall(&mut run, "soak")?;
    // The map's lighting bake re-uploads its scene when it lands, on its
    // worker's time: it must not land in a phase.
    lighting_settled(&mut run)?;
    Ok((run, bricks))
}

fn soak(f: &ContentRoot, rounds: usize) -> Result<()> {
    let state = f.state()?;
    let (mut run, bricks) = start(f, state.path())?;

    // The first of each action warms up (models, looks, uploads); the
    // same again must build and upload nothing whole.
    let mut phases = vec![];
    let mut steady = vec![];
    phases.push(phase("first blast", &mut run, |run| {
        blast(run, bricks, false).map(|_| ())
    })?);
    for _ in 0..rounds {
        steady.push(phases.len());
        phases.push(phase("blast to expiry and return", &mut run, |run| {
            let out = blast(run, bricks, true)?;
            ensure!(out >= bricks / 2, "{out} of {bricks} knocked out");
            Ok(())
        })?);
    }
    phases.push(phase(
        "first death and switches",
        &mut run,
        deaths_and_switches,
    )?);
    for _ in 0..rounds {
        steady.push(phases.len());
        phases.push(phase("death and switches", &mut run, deaths_and_switches)?);
    }
    let map = phases.len();
    // A new map starts its scene renderer and world items over: the first
    // use of each tool there warms up again.
    phases.push(phase("change map", &mut run, |run| {
        change_map(run)?;
        every_tool(run)
    })?);
    steady.push(phases.len());
    phases.push(phase("play on the new map", &mut run, every_tool)?);
    let report: Vec<_> = phases.iter().map(|p| format!("{p:?}")).collect();
    std::fs::write(f.out("app-soak")?.join("phases.txt"), report.join("\n"))?;

    // Steady play: nothing built or uploaded whole, no whole-world passes.
    for p in steady.iter().map(|&i| &phases[i]) {
        ensure!(p.frames > 10, "{}: only {} frames", p.name, p.frames);
        ensure!(
            p.counted,
            "{}: no one scene renderer counted the whole phase",
            p.name
        );
        ensure!(
            p.scenes == 0 && p.textures == 0,
            "{}: {} whole scene uploads with {} textures ({} in one frame)",
            p.name,
            p.scenes,
            p.textures,
            p.worst_scenes
        );
        ensure!(
            p.item_model_builds == 0,
            "{}: {} world-item models built again",
            p.name,
            p.item_model_builds
        );
        ensure!(
            p.debris_looks_built == 0,
            "{}: {} debris looks built again",
            p.name,
            p.debris_looks_built
        );
        ensure!(
            p.music_full_scans == 0 && p.hidden_full_scans == 0,
            "{}: whole-world passes (music {}, hidden outlines {})",
            p.name,
            p.music_full_scans,
            p.hidden_full_scans
        );
    }
    // Bricks going and coming back with the hammer out redraw the hidden
    // outlines from the changed bricks, not from every brick each time.
    let blast = &phases[steady[0]];
    ensure!(
        blast.hidden_visited <= 8 * bricks as u64,
        "{}: {} bricks examined for hidden outlines",
        blast.name,
        blast.hidden_visited
    );
    // A blast and its return rebuild the wall's chunk for each change the
    // replica reports, not for every frame.
    ensure!(
        blast.chunk_jobs <= 4,
        "{}: {} chunk rebuild jobs",
        blast.name,
        blast.chunk_jobs
    );
    // The new map reads its world once.
    let map = &phases[map];
    ensure!(
        map.hidden_full_scans <= 1 && map.music_full_scans <= 1,
        "{map:?}"
    );
    run.app.gpu_stopped();
    Ok(())
}

/// A death and click to respawn (a new body), then every tool and both
/// views.
fn deaths_and_switches(run: &mut Run) -> Result<()> {
    let (_, spawned) = alive(&run.app).context("vitals")?;
    run.action(UiAction::Game(GameAction::Suicide))?;
    run.until("dead", Duration::from_secs(5), |a| {
        alive(a).is_some_and(|(alive, _)| !alive)
    })?;
    run.until("respawn ready", Duration::from_secs(5), |a| {
        a.network_view().is_some_and(|v| {
            v.vitals
                .get(&v.owner)
                .is_some_and(|vitals| !vitals.respawn_held && v.tick >= vitals.respawn_tick)
        })
    })?;
    // Click to respawn, as a player does.
    run.click()?;
    run.until("respawned", Duration::from_secs(10), |a| {
        alive(a).is_some_and(|(alive, at)| alive && at != spawned)
    })?;
    for slot in [ROCKET_SLOT, GUN_SLOT, HAMMER_SLOT, GUN_SLOT] {
        run.use_tool(slot)?;
    }
    for _ in 0..2 {
        run.action(UiAction::Game(GameAction::ToggleFirstPerson {
            fast: false,
        }))?;
        run.run_for(0.3)?;
    }
    Ok(())
}

fn every_tool(run: &mut Run) -> Result<()> {
    for slot in [HAMMER_SLOT, GUN_SLOT, ROCKET_SLOT, HAMMER_SLOT] {
        run.use_tool(slot)?;
        run.run_for(0.5)?;
    }
    Ok(())
}

fn change_map(run: &mut Run) -> Result<()> {
    let to = run.content.open_map.0.clone();
    run.app.ui.core.push(ScreenId::AdminMaps);
    run.app.ui.update(0);
    run.until("map list", Duration::from_secs(10), |a| {
        a.ui.core.admin.maps.iter().any(|m| m.id == to)
    })?;
    run.app.ui.core.pop(ScreenId::AdminMaps);
    run.action(UiAction::Admin(AdminAction::ChangeMap { map: to.clone() }))?;
    run.until("the new map", Duration::from_secs(120), |a| {
        a.network_view().is_some_and(|v| v.world.map_id == to)
            && a.scene_map() == Some(to.as_str())
            && matches!(a.ui.core.conn, ConnectionState::InGame { .. })
            && a.world_render_ready()
            && a.network_view()
                .is_some_and(|v| v.poses.contains_key(&v.owner))
    })?;
    run.until("tools again", Duration::from_secs(10), |a| {
        a.network_view().is_some_and(|v| {
            v.tools
                .get(&v.owner)
                .is_some_and(|t| t.slots[ROCKET_SLOT].is_some())
        })
    })?;
    // The new map's lighting finishes in this phase, not the next.
    lighting_settled(run)?;
    run.run_for(1.0)
}

/// Wait for the map's lighting to finish loading (a completed load, not a
/// span of time).
fn lighting_settled(run: &mut Run) -> Result<()> {
    run.until(
        "the map's lighting to finish loading",
        Duration::from_secs(120),
        |a| a.map_lighting_settled(),
    )
}
