//! Headless step-time profile for a Bedroom bot battle.
//!
//! This is diagnostic only: timings are reported, never thresholded. It needs
//! generated v20 content and does not launch a client or renderer.
//! `BRI_BATTLE_WORLD` selects a world by its exact name; `BRI_BATTLE_TICKS`
//! changes each weapon phase length (quiet warm-up remains 600 ticks).
//! `BRI_BATTLE_STOP_AFTER` ends after an exact phase label, for bounded profiles.
//! Snapshot encoding diagnostics are outside `Session::step` and are not
//! a claim that the host encodes a full world on every update.
use anyhow::{Result, ensure};
use bri_chaos::{fixture, scan::ensure_finite};
use bri_sim::session::{Command, MiniGameRequest, Session};
use bri_world::{Brick, ContentRef, VehicleSpawn, World};
use glam::Vec3;
use std::collections::{BTreeSet, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::time::Instant;

const HOST: u64 = 1;
const TICKS_PER_PHASE: usize = 120 * 8;

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
    let mut result = Profile::default();
    for tick in 0..ticks {
        let start = Instant::now();
        s.step()?;
        let nanos = start.elapsed().as_nanos();
        result.slow_5ms += u64::from(nanos > 5_000_000);
        result.slow_50ms += u64::from(nanos > 50_000_000);
        result.step_nanos.push(nanos);
        let weapons = s.weapon_view();
        for projectile in weapons.fired() {
            result.active_projectile_samples += 1;
            result.observed_projectiles.insert(projectile.id);
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
    result.report(phase);
    Ok(())
}

fn loadout(s: &mut Session, owner: u64, command_id: u64, item: &str) -> Result<f64> {
    let settings = bri_minigames::Settings {
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
) -> World {
    let first = world.next_brick_id;
    for side in 0..2 {
        for i in 0..8 {
            let offset = Vec3::new(
                (i % 4) as f32 * (size[0] + 1.0),
                0.0,
                side as f32 * (size[2] * 2.0 + 8.0) + (i / 4) as f32 * (size[2] + 1.0),
            );
            let mut brick = Brick::new(
                ContentRef::Resolved(plate.to_owned()),
                (anchor + offset).into(),
                HOST,
            );
            brick.vehicle = Some(Box::new(VehicleSpawn {
                vehicle: ContentRef::Resolved(if side == 0 {
                    blockhead.to_owned()
                } else {
                    zombie.to_owned()
                }),
                recolor: false,
            }));
            world.bricks.insert(first + (side * 8 + i) as u64, brick);
        }
    }
    world.next_brick_id = first + 16;
    world.owners.insert(
        HOST,
        bri_world::OwnerRecord::new([1; 32], "Battle profile host".into()),
    );
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
    let brick_count = world.bricks.len() + 16;
    let world = add_bots(world, plate, size, anchor, blockhead, zombie);
    let mut s = bri_net::dedicated::load_packages(root, packages, world)?.session;
    s.set_spawn_points(vec![origin])?;
    let host = s.join_verified(
        "Battle profile host".into(),
        origin,
        true,
        Some(bri_admin::Principal([1; 32])),
    )?;
    for _ in 0..30 {
        s.step()?;
    }
    let bots = s.names().keys().filter(|o| s.is_bot(**o)).count();
    ensure!(bots == 16, "expected 16 bots, got {bots}");
    eprintln!(
        "world={} bricks={} bots={bots} teams=8+8 step_hz=120 profile={build}-headless; timings include the full Session::step and make no accuracy claim",
        world_name, brick_count,
    );

    profile(&mut s, "quiet wander", 120 * 5)?;
    let gun = bri_minigames::Settings {
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
            break;
        }
    }
    Ok(())
}

fn stop_after(phase: &str) -> bool {
    std::env::var("BRI_BATTLE_STOP_AFTER").is_ok_and(|wanted| wanted == phase)
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
                "gun battle" | "rocket battle" | "gravity-gun battle" | "spear battle"
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
    let plate = "v20/brick/brickvehiclespawndata".to_owned();
    let mesh = &initial.session.simulation().definitions.entries[&plate].mesh;
    let size = [
        mesh.footprint_studs[0] as f32 * 0.5,
        mesh.height_plates as f32 * 0.2,
        mesh.footprint_studs[1] as f32 * 0.5,
    ];
    let cells = [0.5, 0.2, 0.5];
    let anchor = Vec3::from_array(std::array::from_fn(|axis| {
        ((origin[axis] - size[axis] * 0.5) / cells[axis]).round() * cells[axis] + size[axis] * 0.5
    }));
    let empty_world = World::new(
        "Bot battle profile".into(),
        "v20/add-ons/map_bedroom/bedroom.mis".into(),
        vec![[1.0; 4]],
    );
    run_battle(
        &root,
        &packages,
        empty_world,
        origin,
        &plate,
        size,
        anchor,
        &blockhead,
        &zombie,
    )?;
    for name in ["Beta City 16", "ACM City"] {
        if let Some(city) = saved_world(&root, name)? {
            eprintln!("loading {name} converted save read-only from worlds-pass-006");
            run_battle(
                &root, &packages, city, origin, &plate, size, anchor, &blockhead, &zombie,
            )?;
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
