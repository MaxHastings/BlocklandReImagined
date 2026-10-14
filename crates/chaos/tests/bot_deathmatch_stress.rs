//! Headless free-for-all bot deathmatch on real content, for finding where
//! many bots cost the host its tick and where their play breaks down.
//!
//! Diagnostic only: it needs generated v20 content (`BRI_CONTENT`) and
//! reports what it measures; nothing is thresholded except that no bot's
//! step may fail. It runs the game as a host does: the enabled Add-Ons
//! through the ordinary loader, Slayer's free-for-all bots filling the
//! game, everyone on the mini-game's loadout.
//!
//! Without `BRI_STRESS` it is the gate's short real-content round: a few
//! bots fight for a few seconds in a bundled v20 save, and every bot turn
//! must complete. `BRI_STRESS=1` makes it the measurement: 32 bots (or as
//! many as Slayer allows) for a game minute. Without generated content, or
//! with no such save in it, it says so and passes: it has nothing to run.
//!
//! - `BRI_STRESS_WORLD`: a saved world (`.world.json` or a converted save)
//!   to fight in. Otherwise `BRI_STRESS_MAP`, an empty map, or else the
//!   worlds pack's save named `BRI_STRESS_WORLD_NAME` (default Afghanistan
//!   DM, 14,091 bricks).
//! - `BRI_STRESS_BOTS`: how many bots (default 8, or 32 with `BRI_STRESS`;
//!   the default is held to what Slayer's own setting allows, and more
//!   asked for is an error).
//! - `BRI_STRESS_SECONDS`: game seconds of fighting (default 5, or 60 with
//!   `BRI_STRESS`).
//! - `BRI_STRESS_LOADOUT`: comma-separated item ids (default the v20 gun,
//!   rocket launcher and the HE grenade).
//! - `BRI_STRESS_ADDONS`: comma-separated Add-On ids to enable beside the
//!   base game (default the bots, Slayer and the HE grenade).
//! - `BRI_STRESS_DUMP`: a file to write each measured tick's step and bot
//!   think nanoseconds to, one tick a line.
//! - `BRI_STRESS_TEAMS`: a number of teams (2 or more) makes it Slayer's
//!   Team Deathmatch, each team filled with an equal share of the bots
//!   (its Preferred Player Count), instead of a free-for-all.
//! - `BRI_STRESS_VEHICLE`: `to`, or `from=to`: every vehicle spawn brick
//!   (or each whose vehicle's id or name contains `from`) spawns the
//!   vehicle `to` instead (enable its Add-On with `BRI_STRESS_ADDONS`).
//!   For playing a map with a vehicle it was not built with, the tank in
//!   Afghanistan DM say.
//!
//! What it prints: one `BRI_STRESS_SUMMARY {json}` line (the step mean,
//! p99 and worst in ms, for `tools/gate.py`'s performance budget), and the
//! host's step time (percentiles, how many ticks ran
//! over the 8.3 ms a 120 Hz tick has, the worst one-second window), the
//! bots' share of it, failed bot steps, and per-bot motion faults: ticks a
//! living bot on foot ran no input (it stood frozen for that tick) and
//! jumps no walk or fall explains (a teleport on the host itself). With
//! vehicles in the world, how the bots used them: rides, the time a
//! vehicle carried two or more, and kills made from a seat.
use anyhow::{Context, Result, ensure};
use bri_chaos::fixture;
use bri_minigames::SettingValue as Value;
use bri_net::protocol::{Datagram, POSE_INTERVAL};
use bri_net::stream::StateStream;
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_sim::session::{Command, MiniGameRequest, SettingEdit, TeamEdit};
use bri_world::{OwnerId, World};
use glam::Vec3;
use std::collections::BTreeMap;
use std::time::Instant;

const SLAYER: &str = "gamemode_slayer-rules";
const TICK_BUDGET_NS: u128 = 1_000_000_000 / 120;
/// Starts the one JSON line of the run's results (`tools/gate.py` reads
/// `step_mean_ms` from it for its performance budget).
const SUMMARY_PREFIX: &str = "BRI_STRESS_SUMMARY";
/// Farther than a body moves in one tick on foot at any speed v20 allows
/// (jets and falls included): a jump past this is a teleport.
const TELEPORT: f32 = 2.0;

fn env_list(name: &str, default: &[&str]) -> Vec<String> {
    std::env::var(name).map_or_else(
        |_| default.iter().map(|s| s.to_string()).collect(),
        |v| {
            v.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect()
        },
    )
}
fn env_num(name: &str, default: usize) -> Result<usize> {
    std::env::var(name)
        .ok()
        .map(|v| v.parse::<usize>())
        .transpose()
        .with_context(|| format!("{name} must be a whole number"))
        .map(|v| v.unwrap_or(default))
}

/// The most free-for-all bots Slayer's own setting lets a game have.
fn slayer_bot_cap(root: &std::path::Path, packages: &PackageSet) -> Result<usize> {
    let entry = packages
        .packages
        .iter()
        .find(|p| p.id == SLAYER)
        .with_context(|| format!("{SLAYER} is not enabled"))?;
    let behaviour: serde_json::Value = serde_json::from_slice(&std::fs::read(
        root.join(&entry.dir).join("behaviour.json"),
    )?)?;
    behaviour["settings"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|s| s["key"] == "ffa_bots")
        .and_then(|s| s["max"].as_u64())
        .map(|max| max as usize)
        .context("Slayer has no free-for-all bot setting")
}

/// The worlds pack's save called `name` (names are trimmed: v20 save
/// names often end in a space), if the content has it.
fn bundled_world(
    root: &std::path::Path,
    packages: &PackageSet,
    name: &str,
) -> Result<Option<World>> {
    let Ok(dir) = packages.role_dir(root, "worlds") else {
        return Ok(None);
    };
    let Ok(report) = std::fs::read(dir.join("report.json")) else {
        return Ok(None);
    };
    let report: serde_json::Value = serde_json::from_slice(&report)?;
    let worlds = report["event_binding"]["worlds"].as_array();
    let Some(file) = worlds
        .into_iter()
        .flatten()
        .find(|w| w["name"].as_str().is_some_and(|n| n.trim() == name.trim()))
        .and_then(|w| w["file"].as_str())
    else {
        return Ok(None);
    };
    Ok(Some(bri_world::persistence::load(&dir.join(file))?))
}

fn packages(root: &std::path::Path, addons: &[String]) -> Result<PackageSet> {
    let mut packages = PackageSet::load_root(root)?;
    for id in addons {
        if packages.packages.iter().any(|p| &p.id == id) {
            continue;
        }
        let manifest: bri_package::library::PackageInfo = serde_json::from_slice(
            &std::fs::read(root.join(format!("addons/{id}/package.json")))
                .with_context(|| format!("Add-On {id} is not in {}", root.display()))?,
        )?;
        packages.packages.push(PackageEntry {
            id: id.clone(),
            version: manifest.version.clone(),
            side: manifest.side().unwrap_or(Side::Shared),
            dir: format!("addons/{id}"),
            role: None,
        });
    }
    bri_package::library::follow_manifest_sides(root, &mut packages);
    bri_package::library::follow_companions(root, &mut packages);
    Ok(packages)
}

#[derive(Default)]
struct Motion {
    /// Living, on foot, and no input ran this tick.
    held: u64,
    /// Living ticks on foot.
    living: u64,
    teleports: u64,
    worst_jump: f32,
}

fn percentile(sorted: &[u128], p: f64) -> f64 {
    let i = ((sorted.len().saturating_sub(1) as f64) * p).ceil() as usize;
    sorted.get(i).copied().unwrap_or_default() as f64 / 1e6
}

#[test]
#[ignore = "needs generated v20 content (BRI_CONTENT); the gate runs it"]
fn bots_fight_a_free_for_all_on_real_content() -> Result<()> {
    let Some(root) = fixture::content_root() else {
        eprintln!("skipped: content not found (BRI_CONTENT is not set)");
        return Ok(());
    };
    let stress = std::env::var_os("BRI_STRESS").is_some();
    let addons = env_list(
        "BRI_STRESS_ADDONS",
        &["blockhead_bot", "gamemode_slayer", "weapon_hegrenade"],
    );
    let loadout = env_list(
        "BRI_STRESS_LOADOUT",
        &[
            "v20.weapon.gunitem",
            "v20.weapon.rocketlauncheritem",
            "weapon_hegrenade:weapon/hegrenadeitem",
        ],
    );
    ensure!(loadout.len() <= 5, "a loadout has five slots");
    let packages = packages(&root, &addons)?;
    let cap = slayer_bot_cap(&root, &packages)?;
    let count = match std::env::var_os("BRI_STRESS_BOTS") {
        Some(_) => env_num("BRI_STRESS_BOTS", 0)?,
        None => (if stress { 32 } else { 8 }).min(cap),
    };
    ensure!(
        count <= cap,
        "BRI_STRESS_BOTS={count}, but Slayer adds at most {cap} free-for-all bots"
    );
    let seconds = env_num("BRI_STRESS_SECONDS", if stress { 60 } else { 5 })?;
    let teams = env_num("BRI_STRESS_TEAMS", 0)?;
    ensure!(teams != 1, "BRI_STRESS_TEAMS is 2 or more teams");
    let mut world = if let Some(path) = std::env::var_os("BRI_STRESS_WORLD") {
        let bytes = std::fs::read(&path)?;
        // A saved build (Save Bricks, a converted v20 save) or a world.
        if bytes.starts_with(bri_world::build::MAGIC) {
            bri_world::build::decode(&bytes)?.world
        } else {
            bri_world::persistence::decode(&bytes)?
        }
    } else if let Ok(map) = std::env::var("BRI_STRESS_MAP") {
        World::new("Bot deathmatch".into(), map, vec![[1.0; 4]])
    } else {
        let wanted =
            std::env::var("BRI_STRESS_WORLD_NAME").unwrap_or_else(|_| "Afghanistan DM".into());
        match bundled_world(&root, &packages, &wanted)? {
            Some(world) => world,
            None => {
                eprintln!(
                    "skipped: content not found (no save named {wanted:?} in {}'s worlds pack)",
                    root.display()
                );
                return Ok(());
            }
        }
    };
    if let Ok(swap) = std::env::var("BRI_STRESS_VEHICLE") {
        let (from, to) = swap.split_once('=').unwrap_or(("", &swap));
        let mut swapped: BTreeMap<String, usize> = BTreeMap::new();
        let ids: Vec<u64> = world.bricks.keys().copied().collect();
        for id in ids {
            let Some(spawn) = world.bricks.get_mut(&id).and_then(|b| b.vehicle.as_mut()) else {
                continue;
            };
            let was = match &spawn.vehicle {
                bri_world::ContentRef::Resolved(id) => id.clone(),
                bri_world::ContentRef::Unresolved(u) => u.name.clone(),
            };
            if was.to_lowercase().contains(&from.to_lowercase()) {
                spawn.vehicle = bri_world::ContentRef::Resolved(to.into());
                *swapped.entry(was).or_default() += 1;
            }
        }
        eprintln!("vehicle spawns now spawning {to}: {swapped:?}");
    }
    let name = world.name.clone();
    let bricks = world.bricks.len();
    let map = world.map_id.clone();
    let started = Instant::now();
    let dedicated = bri_net::dedicated::load_packages(&root, &packages, world)?;
    let mut s = dedicated.session;
    let origin = dedicated
        .spawn_points
        .first()
        .copied()
        .unwrap_or(Vec3::ZERO);
    eprintln!(
        "world={name:?} map={map} bricks={bricks} load_ms={:.0} build={}",
        started.elapsed().as_secs_f64() * 1e3,
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );
    s.set_server_settings(bri_admin::ServerSettings {
        max_bots: count as u32,
        ..s.server_settings().clone()
    })?;
    let host = s.join_verified(
        "Stress host".into(),
        origin,
        true,
        Some(bri_admin::Principal([1; 32])),
    )?;
    let mut slots: [Option<String>; 5] = Default::default();
    for (slot, item) in loadout.iter().enumerate() {
        slots[slot] = Some(item.clone());
    }
    s.command(
        host,
        1,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: bri_minigames::Settings {
                loadout: slots,
                use_all_players_bricks: true,
                ..Default::default()
            },
        }),
    )?;
    for _ in 0..2 {
        s.step()?;
    }
    let game = s.minigame_views()[0].id;
    if teams == 0 {
        s.command(
            host,
            2,
            Command::MiniGame(MiniGameRequest::AddOnSettings {
                quiet: true,
                reset: false,
                game,
                settings: vec![SettingEdit {
                    key: format!("{SLAYER}:ffa_bots"),
                    value: Some(Value::Int(count as i64)),
                }],
                teams: None,
            }),
        )?;
    } else {
        // Team Deathmatch, the host on no team: twice, as a mode that
        // needs teams is refused until the teams exist.
        let setting = |key: &str, value| SettingEdit {
            key: format!("{SLAYER}:{key}"),
            value: Some(value),
        };
        let mut settings = vec![setting("mode", Value::Text("Slayer_TeamDeathmatch".into()))];
        for key in ["auto_sort", "balance_teams", "balance_new_teams"] {
            settings.push(setting(key, Value::Bool(false)));
        }
        for sequence in [2, 3] {
            let view = s.minigame_views()[0].clone();
            let edits = (0..teams)
                .map(|i| TeamEdit {
                    id: view.teams.get(i).map(|t| t.id.0),
                    name: format!("Team {}", i + 1),
                    color: i as u8,
                    settings: vec![setting(
                        "team_bot_fill",
                        Value::Int((count / teams + usize::from(i < count % teams)) as i64),
                    )],
                })
                .collect();
            s.command(
                host,
                sequence,
                Command::MiniGame(MiniGameRequest::AddOnSettings {
                    quiet: true,
                    reset: false,
                    game,
                    settings: settings.clone(),
                    teams: Some(edits),
                }),
            )?;
            for _ in 0..8 {
                s.step()?;
            }
        }
    }
    // The host watches from where they joined; bots spawn and get past
    // their spawn protection.
    for _ in 0..120 * 4 {
        s.step()?;
    }
    let bots: Vec<OwnerId> = s.names().keys().copied().filter(|o| s.is_bot(*o)).collect();
    ensure!(
        bots.len() == count,
        "expected {count} bots, {} joined (Slayer's own bot count caps them)",
        bots.len()
    );
    eprintln!("bots={} loadout={loadout:?} addons={addons:?}", bots.len());

    let ticks = seconds * 120;
    let mut steps = Vec::with_capacity(ticks);
    let mut motion: BTreeMap<OwnerId, Motion> = BTreeMap::new();
    let mut last: BTreeMap<OwnerId, (Vec3, u64, bool, u64)> = BTreeMap::new();
    let think_before = s.bot_think_nanos();
    let (failed_before, _) = s.bot_step_failures();
    let mut deaths = 0_u64;
    let mut think_windows: Vec<u64> = Vec::new();
    let mut thinks: Vec<u64> = Vec::with_capacity(ticks);
    let mut stream = StateStream::default();
    let mut sent: Vec<Sent> = Vec::new();
    let first_tick = s.simulation().state().tick + 1;
    let mut rides = Rides::default();
    // Each bot's body every tick: (feet, alive, spawn tick).
    let mut truth: BTreeMap<OwnerId, Vec<(Vec3, bool, u64)>> = BTreeMap::new();
    for i in 0..ticks {
        let at = Instant::now();
        let thought = s.bot_think_nanos();
        s.step()?;
        steps.push(at.elapsed().as_nanos());
        // Its clock, to find the step in a profile recorded with
        // `perf record -k mono`.
        if at.elapsed().as_millis() >= 30 {
            eprintln!("  slow step +{i} began {at:?}, {:?}", at.elapsed());
        }
        if i % 1200 == 0 {
            think_windows.push(0);
        }
        let thought = s.bot_think_nanos() - thought;
        *think_windows.last_mut().unwrap() += thought;
        thinks.push(thought);
        // A bot with no body this tick is not drawn.
        for bot in &bots {
            let body = truth.entry(*bot).or_default();
            if body.len() < i + 1 {
                body.push((Vec3::ZERO, false, u64::MAX));
            }
        }
        let tick = s.simulation().state().tick;
        if tick.is_multiple_of(POSE_INTERVAL) {
            // What the host's own client is sent, as the server sends it.
            let viewers = [(host, s.viewpoint(host))];
            for (datagram, audience) in stream.interval(
                tick,
                bri_net::protocol::poses(&s),
                s.vehicle_poses(),
                s.camera_orbs(),
                &viewers,
                s.simulation().passages(),
            ) {
                if let Datagram::Remote(pose) = datagram
                    && audience.includes(host)
                    && s.is_bot(pose.owner)
                {
                    sent.push(Sent {
                        step: i,
                        tick: pose.tick,
                        owner: pose.owner,
                        feet: Vec3::from(pose.feet.map(|c| c as f32 / 100.0)),
                        velocity: Vec3::from(pose.velocity.map(|c| f32::from(c) / 100.0)),
                    });
                }
            }
        }
        let vitals = s.vitals();
        rides.sample(&s, &bots);
        for (state, processed) in s.motion_states() {
            let owner = state.owner;
            if !s.is_bot(owner) {
                continue;
            }
            let alive = vitals.get(&owner).is_some_and(|v| v.alive);
            let spawn = vitals.get(&owner).map_or(0, |v| v.spawn_tick);
            let feet = Vec3::from(state.feet);
            let mounted = s.mounted(owner).is_some();
            if let Some(&(was, was_processed, was_alive, was_spawn)) = last.get(&owner) {
                let m = motion.entry(owner).or_default();
                if was_alive && !alive {
                    deaths += 1;
                }
                if alive && was_alive && spawn == was_spawn && !mounted {
                    m.living += 1;
                    if processed == was_processed {
                        m.held += 1;
                    }
                    let jump = feet.distance(was);
                    m.worst_jump = m.worst_jump.max(jump);
                    if jump > TELEPORT {
                        m.teleports += 1;
                    }
                }
            }
            last.insert(owner, (feet, processed, alive, spawn));
            truth
                .entry(owner)
                .or_default()
                .push((feet, alive && !mounted, spawn));
        }
    }
    let think = (s.bot_think_nanos() - think_before) as f64 / 1e6 / ticks as f64;
    let windows: Vec<f64> = steps
        .chunks(120)
        .map(|w| w.iter().sum::<u128>() as f64 / 1e6 / w.len() as f64)
        .collect();
    let worst_window = windows.iter().copied().fold(0.0, f64::max);
    // Whether the cost holds over the whole fight or is a burst: the mean
    // and slowest step of each ten seconds, with the bots' share.
    for (i, w) in steps.chunks(1200).enumerate() {
        let mean = w.iter().sum::<u128>() as f64 / 1e6 / w.len() as f64;
        let max = w.iter().copied().max().unwrap_or_default() as f64 / 1e6;
        let think = think_windows.get(i).copied().unwrap_or_default() as f64 / 1e6 / w.len() as f64;
        let over = w.iter().filter(|n| **n > TICK_BUDGET_NS).count();
        eprintln!(
            "  {:>4}s step_mean={mean:.3} max={max:.3} over={:.0}% bot_think={think:.3}",
            (i + 1) * 10,
            over as f64 * 100.0 / w.len() as f64
        );
    }
    let over = steps.iter().filter(|n| **n > TICK_BUDGET_NS).count();
    let mut sorted = steps.clone();
    sorted.sort_unstable();
    let mean = steps.iter().sum::<u128>() as f64 / 1e6 / steps.len() as f64;
    eprintln!(
        "step_ms mean={mean:.3} p50={:.3} p95={:.3} p99={:.3} max={:.3} over_8.3ms={over}/{} ({:.1}%) worst_1s_mean={worst_window:.3} bot_think_ms_per_tick={think:.3} deaths={deaths}",
        percentile(&sorted, 0.5),
        percentile(&sorted, 0.95),
        percentile(&sorted, 0.99),
        percentile(&sorted, 1.0),
        steps.len(),
        over as f64 * 100.0 / steps.len() as f64,
    );
    // One machine-readable line for tools/gate.py's performance budget.
    eprintln!(
        "{SUMMARY_PREFIX} {}",
        serde_json::json!({
            "world": name,
            "bricks": bricks,
            "bots": bots.len(),
            "teams": teams,
            "seconds": seconds,
            "ticks": steps.len(),
            "build": if cfg!(debug_assertions) { "debug" } else { "release" },
            "step_mean_ms": mean,
            "step_p99_ms": percentile(&sorted, 0.99),
            "step_max_ms": percentile(&sorted, 1.0),
            "over_budget_pct": over as f64 * 100.0 / steps.len() as f64,
            "bot_think_ms_per_tick": think,
            "deaths": deaths,
        })
    );
    let (sampled, arcs, edges, forgotten) = s.nav_work_counts();
    eprintln!(
        "nav_total: floor_samples={sampled} arc_sweeps={arcs} edge_sweeps={edges} forgotten={forgotten}"
    );
    // The slowest steps, and how much of each was the bots'.
    let mut worst: Vec<usize> = (0..steps.len()).collect();
    worst.sort_unstable_by_key(|i| std::cmp::Reverse(steps[*i]));
    for i in worst.into_iter().take(8) {
        eprintln!(
            "  slow tick +{i}: step={:.1}ms bot_think={:.1}ms",
            steps[i] as f64 / 1e6,
            thinks[i] as f64 / 1e6
        );
    }
    if let Some(path) = std::env::var_os("BRI_STRESS_DUMP") {
        let lines: String = steps
            .iter()
            .zip(&thinks)
            .map(|(step, think)| format!("{step} {think}\n"))
            .collect();
        std::fs::write(path, lines)?;
    }
    rides.print(&s, &bots);
    let living: u64 = motion.values().map(|m| m.living).sum();
    let held: u64 = motion.values().map(|m| m.held).sum();
    let teleports: u64 = motion.values().map(|m| m.teleports).sum();
    let worst_jump = motion.values().map(|m| m.worst_jump).fold(0.0, f32::max);
    let most_held = motion
        .iter()
        .max_by_key(|(_, m)| m.held)
        .map(|(o, m)| (*o, m.held, m.living));
    eprintln!(
        "motion living_ticks={living} held_ticks={held} ({:.2}%) most_held={most_held:?} teleports={teleports} worst_jump={worst_jump:.2}",
        held as f64 * 100.0 / living.max(1) as f64
    );
    // What the host's client draws of the bots: once as if every step took
    // no time (the pose stream and interpolation alone), once with the
    // steps as long as they were measured (the server loop's timing).
    for (label, timed) in [("network only", false), ("measured steps", true)] {
        let r = replay(&steps, &sent, &truth, first_tick, timed);
        eprintln!(
            "drawn [{label}]: bot_frames={} extrapolating={:.2}% frozen={:.2}% error_p50={:.3} error_p99={:.3} error>0.5={:.2}% error>1={:.2}% jumps={} worst_error={:.2} moving_frames={} stalled={:.2}% lurched={:.2}%",
            r.frames,
            r.extrapolating as f64 * 100.0 / r.frames.max(1) as f64,
            r.frozen as f64 * 100.0 / r.frames.max(1) as f64,
            r.p50,
            r.p99,
            r.over_half as f64 * 100.0 / r.frames.max(1) as f64,
            r.over_one as f64 * 100.0 / r.frames.max(1) as f64,
            r.jumps,
            r.worst,
            r.moving,
            r.stalled as f64 * 100.0 / r.moving.max(1) as f64,
            r.lurched as f64 * 100.0 / r.moving.max(1) as f64,
        );
    }
    let mut gaps: BTreeMap<u64, usize> = BTreeMap::new();
    let mut by_bot: BTreeMap<OwnerId, Vec<u64>> = BTreeMap::new();
    for p in &sent {
        by_bot.entry(p.owner).or_default().push(p.tick);
    }
    let mut changes = 0;
    for ticks in by_bot.values() {
        let mut was = 0;
        for w in ticks.windows(2) {
            let gap = (w[1] - w[0]).min(POSE_INTERVAL * 16);
            *gaps.entry(gap).or_default() += 1;
            if was != 0 && gap != was {
                changes += 1;
            }
            was = gap;
        }
    }
    eprintln!(
        "bot pose gaps to the host (ticks: count)={gaps:?} rate_changes={changes} sends={}",
        sent.len()
    );
    let (failed, failures) = s.bot_step_failures();
    let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
    for (_, _, error) in failures {
        *kinds.entry(error.as_str()).or_default() += 1;
    }
    eprintln!(
        "bot_step_failures={} first={kinds:#?}",
        failed - failed_before
    );
    ensure!(
        failed == failed_before,
        "{} bot steps failed: {failures:#?}",
        failed - failed_before
    );
    Ok(())
}

/// How the bots used the vehicles: each seat taken and for how long, the
/// ticks a vehicle carried two or more bots, and the seat each killer sat
/// in when its kill landed.
#[derive(Default)]
struct Rides {
    /// Each bot's ride under way: its vehicle, since when, and whether
    /// another bot rode with it at some point.
    open: BTreeMap<OwnerId, (u64, u64, bool)>,
    /// Finished rides: ticks, and whether crewed.
    done: Vec<(u64, bool)>,
    crewed: u64,
    /// Kills by a bot in a seat, and by its own side, with the tick of the
    /// last death counted.
    from_seat: u64,
    team_kills: u64,
    kills: u64,
    last_death: u64,
}
impl Rides {
    fn sample(&mut self, s: &bri_sim::session::Session, bots: &[OwnerId]) {
        let tick = s.simulation().state().tick;
        let mut aboard: BTreeMap<u64, usize> = BTreeMap::new();
        for bot in bots {
            if let Some((vehicle, _)) = s.mounted(*bot) {
                *aboard.entry(vehicle).or_default() += 1;
            }
        }
        for bot in bots {
            match s.mounted(*bot) {
                Some((vehicle, _)) => {
                    let crewed = aboard[&vehicle] >= 2;
                    let ride = self.open.entry(*bot).or_insert((vehicle, tick, false));
                    if ride.0 != vehicle {
                        self.done.push((tick - ride.1, ride.2));
                        *ride = (vehicle, tick, false);
                    }
                    ride.2 |= crewed;
                }
                None => {
                    if let Some((_, since, crewed)) = self.open.remove(bot) {
                        self.done.push((tick - since, crewed));
                    }
                }
            }
        }
        self.crewed += aboard.values().filter(|n| **n >= 2).count() as u64;
        let vitals = s.vitals();
        let deaths: Vec<_> = s
            .death_results()
            .filter(|d| d.tick > self.last_death)
            .cloned()
            .collect();
        for d in deaths {
            self.last_death = self.last_death.max(d.tick);
            let Some(killer) = d.killer.filter(|k| *k != d.victim && bots.contains(k)) else {
                continue;
            };
            let team = |o: OwnerId| vitals.get(&o).and_then(|v| v.team);
            if team(killer).is_some() && team(killer) == team(d.victim) {
                self.team_kills += 1;
            } else {
                self.kills += 1;
                self.from_seat += u64::from(s.mounted(killer).is_some());
            }
        }
    }

    fn print(&self, s: &bri_sim::session::Session, bots: &[OwnerId]) {
        let tick = s.simulation().state().tick;
        let mut all = self.done.clone();
        all.extend(
            self.open
                .values()
                .map(|(_, since, crewed)| (tick - since, *crewed)),
        );
        let crewed_rides = all.iter().filter(|(_, crewed)| *crewed).count();
        let mut rides: Vec<u64> = all.iter().map(|(t, _)| *t).collect();
        rides.sort_unstable();
        let seconds = |t: u64| t as f64 / 120.0;
        eprintln!(
            "vehicles rides={} crewed_rides={crewed_rides} median_s={:.1} under_5s={} riding_s={:.1} crewed_s={:.1} kills={} from_seat={} team_kills={} bots={}",
            rides.len(),
            rides.get(rides.len() / 2).map_or(0.0, |t| seconds(*t)),
            rides.iter().filter(|t| seconds(**t) < 5.0).count(),
            seconds(rides.iter().sum()),
            seconds(self.crewed),
            self.kills,
            self.from_seat,
            self.team_kills,
            bots.len(),
        );
    }
}

/// A bot pose the host's client was sent: at which measured step.
struct Sent {
    step: usize,
    tick: u64,
    owner: OwnerId,
    feet: Vec3,
    velocity: Vec3,
}

#[derive(Default)]
struct Drawn {
    frames: u64,
    extrapolating: u64,
    frozen: u64,
    over_half: u64,
    over_one: u64,
    jumps: u64,
    /// Frames the body moved and the drawn bot all but stood still, and
    /// frames the drawn bot lurched well past the body's own step: a
    /// visible hitch, too small to count as a jump.
    moving: u64,
    stalled: u64,
    lurched: u64,
    p50: f32,
    p99: f32,
    worst: f32,
}

// The client's remote interpolation (`crates/client/src/motion.rs`), as it
// stands: render delay from the pose rate, its slew, bounded extrapolation.
const INTERPOLATION_TICKS: f64 = (POSE_INTERVAL * 3) as f64;
const SLOWEST_INTERVAL: u64 = POSE_INTERVAL * 8;
const DELAY_SLEW_UP: f64 = 0.1;
const DELAY_SLEW_DOWN: f64 = 0.05;
const EXTRAPOLATION_TICKS: f64 = 6.0;
const POSE_HISTORY: usize = 32;
const FPS: f64 = 144.0;

fn remote_delay(history: &[(u64, Vec3, Vec3)]) -> f64 {
    let interval = history
        .iter()
        .rev()
        .zip(history.iter().rev().skip(1))
        .take(4)
        .map(|(b, a)| b.0.saturating_sub(a.0))
        .min()
        .unwrap_or(POSE_INTERVAL)
        .clamp(POSE_INTERVAL, SLOWEST_INTERVAL);
    INTERPOLATION_TICKS + (2 * (interval - POSE_INTERVAL)) as f64
}

fn sample(history: &[(u64, Vec3, Vec3)], tick: f64) -> (Vec3, bool, bool) {
    let first = history[0];
    if tick <= first.0 as f64 {
        return (first.1, false, false);
    }
    for w in history.windows(2) {
        if tick <= w[1].0 as f64 {
            let span = (w[1].0 - w[0].0).max(1) as f64;
            let t = ((tick - w[0].0 as f64) / span) as f32;
            return (w[0].1.lerp(w[1].1, t), false, false);
        }
    }
    let last = history[history.len() - 1];
    let beyond = tick - last.0 as f64;
    let ahead = (beyond.min(EXTRAPOLATION_TICKS) / 120.0) as f32;
    (last.1 + last.2 * ahead, true, beyond > EXTRAPOLATION_TICKS)
}

/// Replay what the host's client draws of each bot at 144 frames a second.
/// `timed`: the server loop runs each step for as long as it was measured
/// (a 120 Hz wakeup, catch-up bounded to 8 steps as `TickClock` does);
/// otherwise every step lands on time.
fn replay(
    steps: &[u128],
    sent: &[Sent],
    truth: &BTreeMap<OwnerId, Vec<(Vec3, bool, u64)>>,
    first_tick: u64,
    timed: bool,
) -> Drawn {
    // When each measured step ended, in seconds.
    let mut ended = vec![0.0f64; steps.len()];
    if timed {
        let (mut now, mut previous, mut remainder, mut i) = (0.0f64, 0.0f64, 0.0f64, 0);
        let mut wake: f64 = 1.0 / 120.0;
        while i < steps.len() {
            let at = wake.max(now);
            remainder += (at - previous) * 120.0;
            previous = at;
            let due = remainder.floor();
            remainder -= due;
            now = at;
            for _ in 0..(due as usize).min(8) {
                if i == steps.len() {
                    break;
                }
                now += steps[i] as f64 / 1e9;
                ended[i] = now;
                i += 1;
            }
            // The next 120 Hz wakeup after the work (missed ones skipped).
            wake = (((now * 120.0).floor() + 1.0) / 120.0).max(wake + 1.0 / 120.0);
        }
    } else {
        for (i, end) in ended.iter_mut().enumerate() {
            *end = (i + 1) as f64 / 120.0;
        }
    }
    let end = ended.last().copied().unwrap_or_default();
    let mut histories: BTreeMap<OwnerId, Vec<(u64, Vec3, Vec3)>> = BTreeMap::new();
    let mut delays: BTreeMap<OwnerId, f64> = BTreeMap::new();
    let mut drawn_last: BTreeMap<OwnerId, (Vec3, Vec3)> = BTreeMap::new();
    let mut offset: Option<f64> = None;
    let mut newest = 0;
    let mut next = 0;
    let mut errors = Vec::new();
    let mut out = Drawn::default();
    let at = |owner: OwnerId, tick: f64| -> Option<(Vec3, u64)> {
        let body = truth.get(&owner)?;
        let i = tick - first_tick as f64;
        if i < 0.0 || i + 1.0 >= body.len() as f64 {
            return None;
        }
        let (a, b) = (body[i as usize], body[i as usize + 1]);
        // Dead, seated, or a new body within the last second: not drawn
        // by interpolation.
        let settled = |k: usize| {
            body.get(k.saturating_sub(120)..=k)
                .is_some_and(|w| w.iter().all(|t| t.1 && t.2 == b.2))
        };
        (settled(i as usize + 1) && a.2 == b.2).then(|| (a.0.lerp(b.0, i.fract() as f32), b.2))
    };
    let mut frame = 0u64;
    loop {
        frame += 1;
        let now = frame as f64 / FPS;
        if now > end {
            break;
        }
        while next < sent.len() && ended[sent[next].step] <= now {
            let p = &sent[next];
            next += 1;
            if p.tick > newest {
                newest = p.tick;
                let sample = p.tick as f64 - now * 120.0;
                offset = Some(match offset {
                    Some(o) if sample < o => o + (sample - o) * 0.02,
                    _ => sample,
                });
            }
            let h = histories.entry(p.owner).or_default();
            if h.last().is_none_or(|l| l.0 < p.tick) {
                h.push((p.tick, p.feet, p.velocity));
                if h.len() > POSE_HISTORY {
                    h.remove(0);
                }
            }
        }
        let Some(offset) = offset else { continue };
        let server = now * 120.0 + offset;
        for (owner, history) in &histories {
            let target = remote_delay(history);
            let delay = delays.entry(*owner).or_insert(target);
            let error = target - *delay;
            let rate = if error > 0.0 {
                DELAY_SLEW_UP
            } else {
                DELAY_SLEW_DOWN
            };
            let step = rate * 120.0 / FPS;
            *delay += error.max(-step).min(step);
            let tick = server - *delay;
            let (shown, extrapolating, frozen) = sample(history, tick);
            let Some((real, _)) = at(*owner, tick) else {
                drawn_last.remove(owner);
                continue;
            };
            out.frames += 1;
            out.extrapolating += u64::from(extrapolating);
            out.frozen += u64::from(frozen);
            let e = shown.distance(real);
            errors.push(e);
            out.over_half += u64::from(e > 0.5);
            out.over_one += u64::from(e > 1.0);
            out.worst = out.worst.max(e);
            // A drawn step much longer than the body's real one: a visible
            // jump.
            if let Some((was_shown, was_real)) = drawn_last.get(owner) {
                let (drawn, moved) = (shown.distance(*was_shown), real.distance(*was_real));
                if drawn > 0.75 && drawn > 3.0 * moved {
                    out.jumps += 1;
                }
                if moved > 0.01 {
                    out.moving += 1;
                    out.stalled += u64::from(drawn < 0.1 * moved);
                    out.lurched += u64::from(drawn > 2.0 * moved + 0.05);
                }
            }
            drawn_last.insert(*owner, (shown, real));
        }
    }
    errors.sort_by(f32::total_cmp);
    let pick = |p: f64| {
        errors
            .get(((errors.len().saturating_sub(1)) as f64 * p) as usize)
            .copied()
            .unwrap_or_default()
    };
    out.p50 = pick(0.5);
    out.p99 = pick(0.99);
    out
}
