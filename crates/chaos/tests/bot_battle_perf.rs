//! Headless step-time profile for a Bedroom bot battle.
//!
//! This is diagnostic only: timings are reported, never thresholded. It needs
//! generated v20 content and does not launch a client or renderer.
//! `BRI_BATTLE_WORLD` selects a world by its exact name; `BRI_BATTLE_TICKS`
//! changes each weapon phase length (quiet warm-up remains 600 ticks).
//! `BRI_BATTLE_STOP_AFTER` ends after an exact phase label, for bounded profiles.
//! `BRI_BATTLE_BOTS=12|16`, `BRI_BATTLE_MIXED=1` and
//! `BRI_BATTLE_REQUIRE_ACTIVE=1` enable the sustained hardening diagnostic.
//! `BRI_BATTLE_RANGED_SIDES=1` uses the same published Blockhead policy for
//! both sides, with two ordinary creator owners joined to the MiniGame.
//! `BRI_BATTLE_BRICK_DAMAGE=0` explicitly disables ordinary MiniGame brick
//! damage for a stable-world diagnostic; the default remains enabled.
//! `BRI_BATTLE_CTF_PROVIDER=1` selects installed Slayer/CTF Add-Ons through the
//! ordinary package loader, requiring their query provider before timing.
//! Without authored flag sources this measures empty-offer discovery only.
//! Default retains the stock Blockhead-versus-converting-Zombie encounter.
//! `BRI_BATTLE_RELOADS=1..4` repeats data/collision/session construction in the
//! same process, reporting sampled RSS. This is not a network map-change test.
//! Snapshot encoding diagnostics are outside `Session::step` and are not
//! a claim that the host encodes a full world on every update.
use anyhow::{Result, ensure};
use bri_chaos::{fixture, scan::ensure_finite};
use bri_sim::session::{Command, MiniGameRequest, Session};
use bri_world::{Brick, ContentRef, VehicleSpawn, World};
use glam::Vec3;
use std::collections::{BTreeMap, BTreeSet, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::time::Instant;

const HOST: u64 = 1;
const TICKS_PER_PHASE: usize = 120 * 8;

/// Sampled process RSS outside the measured simulation phase. POSIX ps is not
/// available on every platform; absence is explicit, not zero memory usage.
fn resident_kib() -> Option<u64> {
    let pid = std::process::id().to_string();
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    std::str::from_utf8(&output.stdout)
        .ok()?
        .trim()
        .parse()
        .ok()
}

#[derive(Default)]
struct Profile {
    step_nanos: Vec<u128>,
    active_projectile_samples: u64,
    observed_projectiles: BTreeSet<u64>,
    snapshot_nanos: Vec<u128>,
    encode_nanos: Vec<u128>,
    evidence: DefaultHasher,
    search_samples: u64,
    searching: u64,
    path_steps: u64,
    slow_5ms: u64,
    slow_50ms: u64,
    active_step_nanos: Vec<u128>,
    fight_controller_ticks: u64,
    living_controller_ticks: u64,
    participating: BTreeSet<u64>,
    health_loss_observed: f64,
    deaths_observed: u64,
    objective_searches: u64,
    objective_reused: u64,
    rss_kib: Vec<u64>,
    windows: Vec<ActivityWindow>,
    controllers: BTreeSet<u64>,
}

#[derive(Debug, Default)]
struct ControllerActivity {
    living_ticks: u64,
    dead_ticks: u64,
    visible_ticks: u64,
    participating_ticks: u64,
    spawn_transitions: u64,
    health_loss_observed: f64,
    behaviour_ticks: BTreeMap<&'static str, u64>,
    mounted_state_ticks: BTreeMap<String, u64>,
}

#[derive(Default)]
struct ActivityWindow {
    ticks: usize,
    active_steps: usize,
    fight_controller_ticks: u64,
    living_controller_ticks: u64,
    health_loss_observed: f64,
    deaths_observed: u64,
    participating: BTreeSet<u64>,
    controllers: BTreeMap<u64, ControllerActivity>,
    observed_projectiles: BTreeSet<u64>,
    projectile_source_counts: BTreeMap<u64, u64>,
    failure_endpoints: Vec<String>,
}

impl Profile {
    fn report(mut self, phase: &str) {
        let max_tick = self
            .step_nanos
            .iter()
            .enumerate()
            .max_by_key(|(_, n)| *n)
            .map(|(tick, _)| tick)
            .unwrap_or_default();
        self.step_nanos.sort_unstable();
        let percentile = |p: f64| {
            let i = ((self.step_nanos.len().saturating_sub(1) as f64) * p).ceil() as usize;
            self.step_nanos.get(i).copied().unwrap_or_default() as f64 / 1_000_000.0
        };
        let max = self.step_nanos.last().copied().unwrap_or_default() as f64 / 1_000_000.0;
        let mean_path = self.path_steps as f64 / self.search_samples.max(1) as f64;
        let search_pct = self.searching as f64 * 100.0 / self.search_samples.max(1) as f64;
        eprintln!(
            "{phase}: ticks={} step_ms[p50={:.3},p95={:.3},p99={:.3},max={max:.3}] >5ms={} >50ms={} active_projectile_samples={} observed_projectiles={} max_tick={max_tick} evidence={:016x} searching_samples={:.1}% mean_path_steps={mean_path:.2}",
            self.step_nanos.len(),
            percentile(0.50),
            percentile(0.95),
            percentile(0.99),
            self.slow_5ms,
            self.slow_50ms,
            self.active_projectile_samples,
            self.observed_projectiles.len(),
            self.evidence.finish(),
            search_pct,
        );
        eprintln!(
            "{phase} activity: active_steps={} fight_controller_ticks={} living_controller_ticks={} participating_bot_ids={} health_loss_observed={:.1} deaths_observed={} objective_searches={} objective_reused={} rss_kib[start={:?},last={:?},sampled_max={:?}]",
            self.active_step_nanos.len(),
            self.fight_controller_ticks,
            self.living_controller_ticks,
            self.participating.len(),
            self.health_loss_observed,
            self.deaths_observed,
            self.objective_searches,
            self.objective_reused,
            self.rss_kib.first(),
            self.rss_kib.last(),
            self.rss_kib.iter().max()
        );
        for (i, window) in self.windows.iter().enumerate() {
            let missing: Vec<_> = self.controllers.difference(&window.participating).collect();
            eprintln!(
                "{phase} window[{i}] ticks={} active_steps={} fight_controller_ticks={} living_controller_ticks={} health_loss_observed={:.1} deaths_observed={} participating_bot_ids={} participating={:?} missing={missing:?} distinct_live_projectile_ids={} distinct_live_projectile_ids_by_source={:?}",
                window.ticks,
                window.active_steps,
                window.fight_controller_ticks,
                window.living_controller_ticks,
                window.health_loss_observed,
                window.deaths_observed,
                window.participating.len(),
                window.participating,
                window.observed_projectiles.len(),
                window.projectile_source_counts,
            );
            if window.active_steps == 0 || window.health_loss_observed == 0.0 || !missing.is_empty()
            {
                for (bot, activity) in &window.controllers {
                    eprintln!("{phase} window[{i}] bot={bot} lifecycle={activity:?}");
                }
                for endpoint in &window.failure_endpoints {
                    eprintln!("{phase} window[{i}] end {endpoint}");
                }
            }
        }
        report_auxiliary(phase, "active_combat_step", &mut self.active_step_nanos);
        report_auxiliary(phase, "snapshot", &mut self.snapshot_nanos);
        report_auxiliary(phase, "snapshot_messagepack", &mut self.encode_nanos);
    }
}

fn report_auxiliary(phase: &str, label: &str, samples: &mut [u128]) {
    samples.sort_unstable();
    let percentile = |p: f64| {
        let index = ((samples.len().saturating_sub(1) as f64) * p).ceil() as usize;
        samples.get(index).copied().unwrap_or_default() as f64 / 1_000_000.0
    };
    eprintln!(
        "{phase} {label}: samples={} ms[p50={:.3},p95={:.3},p99={:.3},max={:.3}]",
        samples.len(),
        percentile(0.5),
        percentile(0.95),
        percentile(0.99),
        percentile(1.0)
    );
}

fn profile(s: &mut Session, phase: &str, ticks: usize) -> Result<()> {
    eprintln!("phase_begin={phase} ticks={ticks}");
    // Existing VM telemetry is included in Session::step; draining outside
    // timing gives this phase's totals without counting earlier setup calls.
    drop(s.take_package_script_time());
    let mut result = Profile::default();
    let mut health = BTreeMap::new();
    let mut counters = BTreeMap::new();
    let mut spawn_ticks = BTreeMap::new();
    let initial_vitals = s.vitals();
    for thought in s.bot_thoughts() {
        result.controllers.insert(thought.bot);
        health.insert(thought.bot, initial_vitals[&thought.bot].health);
        spawn_ticks.insert(thought.bot, initial_vitals[&thought.bot].spawn_tick);
        counters.insert(
            thought.bot,
            (thought.objective_searches, thought.objective_reused),
        );
    }
    if let Some(rss) = resident_kib() {
        result.rss_kib.push(rss);
    }
    for tick in 0..ticks {
        if tick % 1200 == 0 {
            result.windows.push(ActivityWindow::default());
        }
        let window = result.windows.last_mut().unwrap();
        window.ticks += 1;
        let start = Instant::now();
        s.step()?;
        let nanos = start.elapsed().as_nanos();
        result.slow_5ms += u64::from(nanos > 5_000_000);
        result.slow_50ms += u64::from(nanos > 50_000_000);
        result.step_nanos.push(nanos);
        let thoughts = s.bot_thoughts();
        let vitals = s.vitals();
        let mut active = false;
        let mut current_health = BTreeMap::new();
        let mut current_counters = BTreeMap::new();
        for thought in &thoughts {
            let vital = &vitals[&thought.bot];
            let now = vital.health;
            let controller = window.controllers.entry(thought.bot).or_default();
            controller.living_ticks += u64::from(vital.alive);
            controller.dead_ticks += u64::from(!vital.alive);
            controller.visible_ticks += u64::from(vital.alive && thought.visible.is_some());
            controller.spawn_transitions += u64::from(
                spawn_ticks.insert(thought.bot, vital.spawn_tick) != Some(vital.spawn_tick),
            );
            *controller
                .behaviour_ticks
                .entry(thought.behaviour)
                .or_default() += 1;
            result.living_controller_ticks += u64::from(now > 0.0);
            window.living_controller_ticks += u64::from(now > 0.0);
            if let Some(old) = health.get(&thought.bot) {
                let loss = f64::from((old - now).max(0.0));
                let death = u64::from(*old > 0.0 && now <= 0.0);
                result.health_loss_observed += loss;
                result.deaths_observed += death;
                window.health_loss_observed += loss;
                window.deaths_observed += death;
                controller.health_loss_observed += loss;
            }
            current_health.insert(thought.bot, now);
            let next = (thought.objective_searches, thought.objective_reused);
            let before = counters.get(&thought.bot).copied().unwrap_or_default();
            result.objective_searches += if next.0 >= before.0 {
                next.0 - before.0
            } else {
                next.0
            };
            result.objective_reused += if next.1 >= before.1 {
                next.1 - before.1
            } else {
                next.1
            };
            current_counters.insert(thought.bot, next);
            if now > 0.0
                && thought.visible.is_some()
                && matches!(thought.behaviour, "fight" | "chase" | "fly")
            {
                active = true;
                result.participating.insert(thought.bot);
                window.participating.insert(thought.bot);
                controller.participating_ticks += 1;
            }
            let fighting = now > 0.0 && thought.behaviour == "fight";
            result.fight_controller_ticks += u64::from(fighting);
            window.fight_controller_ticks += u64::from(fighting);
        }
        health = current_health;
        counters = current_counters;
        if active {
            result.active_step_nanos.push(nanos);
            window.active_steps += 1;
        }
        if tick % 1200 == 0
            && let Some(rss) = resident_kib()
        {
            result.rss_kib.push(rss);
        }
        let weapons = s.weapon_view();
        for projectile in weapons.fired() {
            result.active_projectile_samples += 1;
            result.observed_projectiles.insert(projectile.id);
            if window.observed_projectiles.insert(projectile.id) {
                *window
                    .projectile_source_counts
                    .entry(projectile.source.0)
                    .or_default() += 1;
            }
        }
        for (bot, controller) in &mut window.controllers {
            let image = weapons
                .images
                .get(bot)
                .and_then(|images| images.iter().find(|i| i.hand == 0));
            let state = image.map_or("unmounted".to_owned(), |image| {
                format!("{}:{}", image.image, image.state)
            });
            *controller.mounted_state_ticks.entry(state).or_default() += 1;
        }
        if (tick + 1) % 1200 == 0 || tick + 1 == ticks {
            let missing = result
                .controllers
                .difference(&window.participating)
                .next()
                .is_some();
            if window.active_steps == 0 || window.health_loss_observed == 0.0 || missing {
                let snapshot = s.snapshot();
                for thought in &thoughts {
                    window.failure_endpoints.push(format!(
                        "bot={} state={:?} vitals={:?} tools={:?} mounts={:?} thought={thought:?}",
                        thought.bot,
                        snapshot.players.iter().find(|p| p.owner == thought.bot),
                        vitals.get(&thought.bot),
                        snapshot.tools.get(&thought.bot),
                        weapons.images.get(&thought.bot),
                    ));
                }
            }
        }
        if tick % 120 == 0 {
            // Keep diagnostics outside the measured `Session::step` window.
            let start = Instant::now();
            let snapshot = s.snapshot();
            result.snapshot_nanos.push(start.elapsed().as_nanos());
            let start = Instant::now();
            let bytes = rmp_serde::to_vec_named(&snapshot)?;
            result.encode_nanos.push(start.elapsed().as_nanos());
            bytes.hash(&mut result.evidence);
            ensure_finite("battle snapshot", &snapshot)?;
            for thought in s.bot_thoughts() {
                format!("{thought:?}").hash(&mut result.evidence);
                result.search_samples += 1;
                result.searching += u64::from(thought.searching);
                result.path_steps += thought.path_steps as u64;
            }
        }
    }
    if let Some(rss) = resident_kib() {
        result.rss_kib.push(rss);
    }
    let package_vm_ms: BTreeMap<_, _> = s
        .take_package_script_time()
        .into_iter()
        .map(|(package, duration)| (package, duration.as_secs_f64() * 1000.0))
        .collect();
    eprintln!(
        "{phase} package_vm_ms={package_vm_ms:?}; included_in_step=true; excludes_native_snapshot_state_and_query_setup=true"
    );
    let snapshot = s.snapshot();
    eprintln!(
        "{phase} phase_end bricks={} vehicle_spawners={} minigames={:?}",
        snapshot.world.bricks.len(),
        snapshot
            .world
            .bricks
            .values()
            .filter(|b| b.vehicle.is_some())
            .count(),
        s.minigame_views(),
    );
    let vitals = s.vitals();
    for thought in s.bot_thoughts() {
        eprintln!(
            "{phase} phase_end bot={} state={:?} vitals={:?} tools={:?} mounts={:?} thought={thought:?}",
            thought.bot,
            snapshot.players.iter().find(|p| p.owner == thought.bot),
            vitals.get(&thought.bot),
            snapshot.tools.get(&thought.bot),
            snapshot.weapons.images.get(&thought.bot),
        );
    }
    if std::env::var("BRI_BATTLE_REQUIRE_ACTIVE").is_ok_and(|v| v == "1")
        && matches!(
            phase,
            "gun battle" | "rocket battle" | "spear battle" | "mixed battle"
        )
    {
        let required = s.bot_thoughts().len();
        let inactive = result
            .windows
            .iter()
            .enumerate()
            .filter_map(|(i, w)| {
                (w.active_steps == 0
                    || w.health_loss_observed == 0.0
                    || ranged_sides() && w.participating.len() != required)
                    .then_some(i)
            })
            .collect::<Vec<_>>();
        let damage = result.health_loss_observed;
        result.report(phase);
        ensure!(
            inactive.is_empty() && damage > 0.0,
            "{phase} needs combat/damage in every 10-second window and all {required} controllers participating for ranged sides; inactive_windows={inactive:?} health_loss_observed={damage}"
        );
        return Ok(());
    }
    result.report(phase);
    Ok(())
}

fn loadout(s: &mut Session, owner: u64, command_id: u64, item: &str) -> Result<f64> {
    let settings = bri_minigames::Settings {
        brick_damage: brick_damage(),
        loadout: [Some(item.to_owned()), None, None, None, None],
        ..Default::default()
    };
    let start = Instant::now();
    s.command(
        owner,
        command_id,
        Command::MiniGame(MiniGameRequest::Configure { settings }),
    )?;
    Ok(start.elapsed().as_secs_f64() * 1000.0)
}

fn add_bots(
    mut world: World,
    plate: &str,
    size: [f32; 3],
    anchor: Vec3,
    blockhead: &str,
    zombie: &str,
    bots: usize,
) -> World {
    let ranged = ranged_sides();
    let first = world.next_brick_id;
    let per_side = bots / 2;
    for side in 0..2 {
        for i in 0..per_side {
            let offset = Vec3::new(
                (i % 4) as f32 * (size[0] + 1.0),
                0.0,
                side as f32 * (size[2] * 2.0 + 8.0) + (i / 4) as f32 * (size[2] + 1.0),
            );
            let mut brick = Brick::new(
                ContentRef::Resolved(plate.to_owned()),
                (anchor + offset).into(),
                if ranged && side == 1 { HOST + 1 } else { HOST },
            );
            brick.vehicle = Some(Box::new(VehicleSpawn {
                vehicle: ContentRef::Resolved(if ranged || side == 0 {
                    blockhead.to_owned()
                } else {
                    zombie.to_owned()
                }),
                recolor: false,
                team: None,
            }));
            world
                .bricks
                .insert(first + (side * per_side + i) as u64, brick);
        }
    }
    world.next_brick_id = first + bots as u64;
    world.owners.insert(
        HOST,
        bri_world::OwnerRecord::new([1; 32], "Battle profile host".into()),
    );
    if ranged {
        world.owners.insert(
            HOST + 1,
            bri_world::OwnerRecord::new([2; 32], "Opposing creator".into()),
        );
    }
    world
}

// Keep the independent fixture inputs explicit in this diagnostic helper.
#[allow(clippy::too_many_arguments)]
fn run_battle(
    root: &std::path::Path,
    packages: &bri_package::packages::PackageSet,
    world: World,
    origin: Vec3,
    plate: &str,
    size: [f32; 3],
    anchor: Vec3,
    blockhead: &str,
    zombie: &str,
) -> Result<()> {
    let world_name = world.name.clone();
    if std::env::var("BRI_BATTLE_WORLD").is_ok_and(|wanted| wanted != world_name) {
        return Ok(());
    }
    let ticks = std::env::var("BRI_BATTLE_TICKS")
        .ok()
        .map(|n| n.parse::<usize>())
        .transpose()?
        .unwrap_or(TICKS_PER_PHASE);
    ensure!(ticks > 0, "BRI_BATTLE_TICKS must be positive");
    let build = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let count = std::env::var("BRI_BATTLE_BOTS")
        .ok()
        .map(|v| v.parse::<usize>())
        .transpose()?
        .unwrap_or(16);
    ensure!(matches!(count, 12 | 16), "BRI_BATTLE_BOTS must be 12 or 16");
    let brick_count = world.bricks.len() + count;
    let world = add_bots(world, plate, size, anchor, blockhead, zombie, count);
    let before_rss = resident_kib();
    let load_started = Instant::now();
    let mut s = bri_net::dedicated::load_packages(root, packages, world)?.session;
    eprintln!(
        "world={world_name} session_load_ms={:.3} rss_kib[before={before_rss:?},after={:?}]",
        load_started.elapsed().as_secs_f64() * 1000.0,
        resident_kib()
    );
    s.set_spawn_points(vec![origin])?;
    let host = s.join_verified(
        "Battle profile host".into(),
        origin,
        true,
        Some(bri_admin::Principal([1; 32])),
    )?;
    let other_creator = if ranged_sides() {
        let owner = s.join_verified(
            "Opposing creator".into(),
            origin,
            false,
            Some(bri_admin::Principal([2; 32])),
        )?;
        ensure!(owner == HOST + 1, "authored opposing creator ownership");
        Some(owner)
    } else {
        None
    };
    for _ in 0..30 {
        s.step()?;
    }
    let bots = s.names().keys().filter(|o| s.is_bot(**o)).count();
    ensure!(bots == count, "expected {count} bots, got {bots}");
    let side = bots / 2;
    eprintln!(
        "world={} bricks={} bots={bots} teams={side}+{side} step_hz=120 profile={build}-headless; timings include the full Session::step and make no accuracy claim",
        world_name, brick_count,
    );
    eprintln!(
        "encounter_policy={} Blockhead_kind_runtime_override=false",
        if ranged_sides() {
            "two-ordinary-creator-Blockhead-sides"
        } else {
            "stock-Blockhead-vs-converting-Zombie"
        }
    );

    profile(&mut s, "quiet wander", 120 * 5)?;
    let gun = bri_minigames::Settings {
        brick_damage: brick_damage(),
        loadout: [Some("v20.weapon.gunitem".into()), None, None, None, None],
        ..Default::default()
    };
    s.command(
        host,
        2,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: gun,
        }),
    )?;
    if let Some(creator) = other_creator {
        let game = s.minigame_views()[0].id;
        s.command(
            creator,
            1,
            Command::MiniGame(MiniGameRequest::Join { game }),
        )?;
    }
    profile(&mut s, "gun battle", ticks)?;
    if stop_after("gun battle") {
        return Ok(());
    }
    for (command_id, label, item) in [
        (3, "rocket battle", "v20.weapon.rocketlauncheritem"),
        (
            4,
            "gravity-gun battle",
            "gravity-gun-tool:weapon/gravitygun",
        ),
        (5, "spear battle", "v20.weapon.spearitem"),
    ] {
        let ms = loadout(&mut s, host, command_id, item)?;
        eprintln!("{label} loadout_change_ms={ms:.3}");
        profile(&mut s, label, ticks)?;
        if stop_after(label) {
            return Ok(());
        }
    }
    if std::env::var("BRI_BATTLE_MIXED").is_ok_and(|v| v == "1") {
        let settings = bri_minigames::Settings {
            brick_damage: brick_damage(),
            loadout: [
                Some("v20.weapon.gunitem".into()),
                Some("v20.weapon.rocketlauncheritem".into()),
                Some("v20.weapon.spearitem".into()),
                Some("v20.weapon.sworditem".into()),
                None,
            ],
            ..Default::default()
        };
        s.command(
            host,
            6,
            Command::MiniGame(MiniGameRequest::Configure { settings }),
        )?;
        profile(&mut s, "mixed battle", ticks)?;
    }
    Ok(())
}

fn stop_after(phase: &str) -> bool {
    std::env::var("BRI_BATTLE_STOP_AFTER").is_ok_and(|wanted| wanted == phase)
}

fn ranged_sides() -> bool {
    std::env::var("BRI_BATTLE_RANGED_SIDES").is_ok_and(|v| v == "1")
}

fn brick_damage() -> bool {
    !std::env::var("BRI_BATTLE_BRICK_DAMAGE").is_ok_and(|v| v == "0")
}

fn saved_world(root: &std::path::Path, wanted: &str) -> Result<Option<World>> {
    let worlds = root.join("worlds-pass-006");
    if !worlds.is_dir() {
        return Ok(None);
    }
    for entry in std::fs::read_dir(worlds)? {
        let path = entry?.path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".world.json"))
        {
            let bytes = std::fs::read(&path)?;
            let world: World = serde_json::from_slice(&bytes)?;
            if world.name == wanted {
                return Ok(Some(world));
            }
        }
    }
    Ok(None)
}

#[test]
#[ignore = "diagnostic profile requires generated v20 content; timing is machine-specific"]
fn profile_sixteen_bots_in_the_real_bedroom() -> Result<()> {
    if let Ok(world) = std::env::var("BRI_BATTLE_WORLD") {
        ensure!(
            matches!(
                world.as_str(),
                "Bot battle profile" | "Beta City 16" | "ACM City"
            ),
            "BRI_BATTLE_WORLD must be Bot battle profile, Beta City 16 or ACM City"
        );
    }
    if let Ok(phase) = std::env::var("BRI_BATTLE_STOP_AFTER") {
        ensure!(
            matches!(
                phase.as_str(),
                "gun battle"
                    | "rocket battle"
                    | "gravity-gun battle"
                    | "spear battle"
                    | "mixed battle"
            ),
            "BRI_BATTLE_STOP_AFTER must name a weapon phase exactly"
        );
    }
    let root = fixture::content_root().expect("set BRI_CONTENT to generated content");
    let mut packages = bri_package::packages::PackageSet::load_root(&root)?;
    for id in [
        "bot_hole",
        "blockhead_bot",
        "bot_zombie",
        "gravity-gun",
        "gravity-gun-tool",
    ] {
        if !packages.packages.iter().any(|p| p.id == id) {
            packages.packages.push(bri_package::packages::PackageEntry {
                id: id.into(),
                version: "1.0.0".into(),
                side: bri_package::packages::Side::Shared,
                dir: format!("addons/{id}"),
                role: None,
            });
        }
    }
    let query_ctf = std::env::var("BRI_BATTLE_CTF_PROVIDER").is_ok_and(|v| v == "1");
    if query_ctf {
        // Select the same installed dependencies a creator enables. Their
        // declared server companions follow through the ordinary loader below.
        for id in ["gamemode_slayer", "gamemode_slayer_ctf"] {
            if !packages.packages.iter().any(|p| p.id == id) {
                let manifest: bri_package::library::PackageInfo = serde_json::from_slice(
                    &std::fs::read(root.join(format!("addons/{id}/package.json")))?,
                )?;
                ensure!(
                    manifest.id == id,
                    "selected package manifest must identify {id}"
                );
                let side = manifest.side().ok_or_else(|| {
                    anyhow::anyhow!("selected package {id} has mixed loading sides")
                })?;
                packages.packages.push(bri_package::packages::PackageEntry {
                    id: id.into(),
                    version: manifest.version,
                    side,
                    dir: format!("addons/{id}"),
                    role: None,
                });
            }
        }
    }
    bri_package::library::follow_manifest_sides(&root, &mut packages);
    bri_package::library::follow_companions(&root, &mut packages);
    let initial = bri_net::dedicated::load_packages(
        &root,
        &packages,
        World::new(
            "Bot battle profile".into(),
            "v20/add-ons/map_bedroom/bedroom.mis".into(),
            vec![[1.0; 4]],
        ),
    )?;
    let origin = initial.spawn_points[0];
    let choices = initial.session.bot_choices();
    let blockhead = choices
        .iter()
        .find(|(_, name)| name == "Blockhead Bot")
        .map(|(id, _)| id.clone())
        .expect("Blockhead Bot enabled");
    let zombie = choices
        .iter()
        .find(|(_, name)| name == "Zombie")
        .map(|(id, _)| id.clone())
        .expect("Zombie enabled");
    let query_providers = initial
        .setup
        .add_ons
        .as_ref()
        .map(|host| {
            host.server
                .behaviours()
                .filter(|(_, behaviour)| behaviour.bot_objectives)
                .map(|(package, _)| package.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    ensure!(
        !query_ctf
            || query_providers
                .iter()
                .any(|p| p == "gamemode_slayer_ctf-rules"),
        "requested CTF discovery provider must actually be loaded before timing"
    );
    let objective_weights: Vec<_> = initial
        .setup
        .content
        .bot_kinds
        .iter()
        .filter(|kind| kind.id == blockhead || kind.id == zombie)
        .map(|kind| {
            (
                kind.id.clone(),
                kind.behaviours.get("objective").copied().unwrap_or(0.0),
            )
        })
        .collect();
    eprintln!(
        "ctf_provider_selected={query_ctf} loaded_objective_query_providers={query_providers:?} loaded_bot_kind_objective_weights={objective_weights:?}; populated_package_objective_actions_not_implied=true"
    );
    let plate = "v20/brick/brickvehiclespawndata".to_owned();
    let mesh = &initial.session.simulation().definitions.entries[&plate].mesh;
    let size = [
        mesh.footprint_studs[0] as f32 * 0.5,
        mesh.height_plates as f32 * 0.2,
        mesh.footprint_studs[1] as f32 * 0.5,
    ];
    // The catalog-probe session is not a second active host in the RSS run.
    drop(initial);
    let cells = [0.5, 0.2, 0.5];
    let anchor = Vec3::from_array(std::array::from_fn(|axis| {
        ((origin[axis] - size[axis] * 0.5) / cells[axis]).round() * cells[axis] + size[axis] * 0.5
    }));
    let empty_world = World::new(
        "Bot battle profile".into(),
        "v20/add-ons/map_bedroom/bedroom.mis".into(),
        vec![[1.0; 4]],
    );
    let reloads = std::env::var("BRI_BATTLE_RELOADS")
        .ok()
        .map(|v| v.parse::<usize>())
        .transpose()?
        .unwrap_or(1);
    ensure!(
        (1..=4).contains(&reloads),
        "BRI_BATTLE_RELOADS must be 1..4"
    );
    for reload in 0..reloads {
        eprintln!("session_construction_repeat={reload}");
        run_battle(
            &root,
            &packages,
            empty_world.clone(),
            origin,
            &plate,
            size,
            anchor,
            &blockhead,
            &zombie,
        )?;
        eprintln!(
            "session_construction_complete={reload} idle_rss_kib={:?}",
            resident_kib()
        );
    }
    for name in ["Beta City 16", "ACM City"] {
        if let Some(city) = saved_world(&root, name)? {
            eprintln!("loading {name} converted save read-only from worlds-pass-006");
            for reload in 0..reloads {
                eprintln!("world={name} session_construction_repeat={reload}");
                run_battle(
                    &root,
                    &packages,
                    city.clone(),
                    origin,
                    &plate,
                    size,
                    anchor,
                    &blockhead,
                    &zombie,
                )?;
                eprintln!(
                    "world={name} session_construction_complete={reload} idle_rss_kib={:?}",
                    resident_kib()
                );
            }
        } else {
            ensure!(
                !std::env::var("BRI_BATTLE_WORLD").is_ok_and(|wanted| wanted == name),
                "Requested {name} is missing from this content root"
            );
            eprintln!("{name} is not in this content root; saved-world comparison skipped");
        }
    }
    Ok(())
}
