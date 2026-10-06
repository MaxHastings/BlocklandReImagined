//! Headless look at bots playing a saved ball game, as a host who loads the
//! save and applies one of their Add-On favourites with Save & Reset: the
//! save is loaded on its map with every installed Add-On, the favourite's
//! rules, settings and teams are applied, and the match runs with no window
//! and no input from the host. The state folder is only read (its saves are
//! copied to a temporary folder first).
//!
//! Usage: soccer_probe <content-root> <state-dir> <save name> [favourite slot 1-10] [seconds]
//! Prints what the bots did: goals, ball touches and travel, time near the
//! ball, time pressed against an enemy away from the ball, what they were
//! doing, and which items they held.
use anyhow::{Context, Result, ensure};
use bri_client::{content::ClientContent, minigame_ui, saves::Store};
use bri_sim::{player::MoveInput, session::Command};
use bri_ui::api::{MiniGameId, MiniGameTeamEdit, UiAction};
use bri_world::OwnerId;
use glam::Vec3;
use std::{collections::BTreeMap, path::Path};

const TICKS: u64 = 120;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    ensure!(
        (3..=5).contains(&args.len()),
        "Usage: soccer_probe <content-root> <state-dir> <save name> [favourite slot 1-10] [seconds]"
    );
    let slot: u8 = args.get(3).map_or(Ok(1), |s| s.parse())?;
    ensure!((1..=10).contains(&slot), "Favourite slots are 1 to 10");
    let seconds: u64 = args.get(4).map_or(Ok(300), |s| s.parse())?;
    let temp = std::env::temp_dir().join(format!("soccer_probe-{}", std::process::id()));
    let result = run(
        Path::new(&args[0]),
        Path::new(&args[1]),
        &args[2],
        slot - 1,
        seconds,
        &temp,
    );
    let _ = std::fs::remove_dir_all(&temp);
    result
}

/// The packages the player's game runs: the release defaults, then their
/// saved Add-On choices (`add-on-choices.json`) on top, with each Add-On's
/// companions following it. Nothing is written.
fn packages(root: &Path, state: &Path) -> Result<bri_package::packages::PackageSet> {
    let mut set = bri_package::packages::PackageSet::load_root(root)?;
    let path = state.join("add-on-choices.json");
    if !path.exists() {
        eprintln!("No {}: the release's default Add-Ons", path.display());
        return Ok(set);
    }
    let choices: serde_json::Value = serde_json::from_slice(&std::fs::read(&path)?)?;
    let library = bri_package::library::Library::scan(root)?;
    for (id, wanted) in choices["packages"]
        .as_object()
        .context("Add-On choices without packages")?
    {
        let wanted = wanted.as_bool().unwrap_or(false);
        let present = set.packages.iter().any(|p| &p.id == id);
        if wanted && !present {
            match library.entries.iter().find(|e| e.id() == id) {
                Some(e) => set.packages.push(e.package.clone()),
                None => eprintln!("Add-On choice {id} is not installed here"),
            }
        } else if !wanted && present {
            set.packages.retain(|p| &p.id != id);
        }
    }
    bri_package::library::follow_companions(root, &mut set);
    Ok(set)
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn run(root: &Path, state: &Path, name: &str, slot: u8, seconds: u64, temp: &Path) -> Result<()> {
    let content = ClientContent::load(root)?;
    copy_dir(&state.join("saves"), &temp.join("saves")).context("Copying the saves folder")?;
    let settings = bri_client::settings::load(&state.join("settings.json"))?;
    let favourite = settings
        .addon_favorites
        .get(&slot)
        .with_context(|| format!("Favourite slot {} is empty", slot + 1))?
        .clone();
    let store = Store::new(temp, &content, None);
    let entries = store.list()?;
    let wanted = name.to_ascii_lowercase();
    let entry = entries
        .iter()
        .find(|e| {
            e.info
                .name
                .to_ascii_lowercase()
                .trim_end_matches(".world.json")
                == wanted
        })
        .with_context(|| {
            let names: Vec<_> = entries.iter().map(|e| e.info.name.as_str()).collect();
            format!("No save called {name:?}; saves: {names:?}")
        })?;
    let build = Store::read(entry)?;
    let packages = packages(root, state)?;
    let schema = bri_world::World::new(
        "Probe".into(),
        entry.map_id.clone(),
        build.world.palette.clone(),
    );
    let dedicated = bri_net::dedicated::load_packages(root, &packages, schema)?;
    let spawn = dedicated.spawn_points[0];
    let mut s = dedicated.session;
    let host = s.join("Host".into(), spawn, true)?;
    let mut seq = 1u64;
    s.command(
        host,
        seq,
        Command::LoadBuild {
            build: Box::new(build),
            ownership: false,
        },
    )?;
    let mut step = |s: &mut bri_sim::session::Session| -> Result<()> {
        seq += 1;
        s.movement(host, seq, MoveInput::default())?;
        s.step()?;
        Ok(())
    };
    while s.build_loading() {
        step(&mut s)?;
    }
    // Each command takes the next sequence number: a repeated one is
    // rejected as replayed.
    let sent = std::cell::Cell::new(1_000_000u64);
    let send = |s: &mut bri_sim::session::Session, action: UiAction, game: Option<u64>| {
        sent.set(sent.get() + 1);
        let command = minigame_ui::command(&action, game, true)?.context("a mini-game command")?;
        let reply = s.command(host, sent.get(), command)?;
        eprintln!("{action:?}\n  -> {reply:?}");
        Ok::<_, anyhow::Error>(())
    };
    let own = |s: &bri_sim::session::Session| {
        s.minigame_views()
            .into_iter()
            .find(|g| g.members.contains(&host))
            .map(|g| g.id)
    };
    for _ in 0..4 {
        step(&mut s)?;
    }
    if own(&s).is_none() {
        let rules = favourite.rules.clone().unwrap_or_default();
        send(&mut s, UiAction::CreateMiniGame { color: 0, rules }, None)?;
        for _ in 0..4 {
            step(&mut s)?;
        }
    } else if let Some(rules) = favourite.rules.clone() {
        let game = own(&s).context("the host's game")?;
        send(
            &mut s,
            UiAction::ConfigureMiniGame {
                game: MiniGameId(game),
                rules,
            },
            Some(game),
        )?;
    }
    let game = own(&s).context("the host is in no mini-game")?;
    let edits = |m: &BTreeMap<String, bri_ui::api::MiniGameSettingValue>| {
        m.iter()
            .map(|(k, v)| (k.clone(), Some(v.clone())))
            .collect()
    };
    send(
        &mut s,
        UiAction::EditMiniGameAddOns {
            game: MiniGameId(game),
            settings: edits(&favourite.settings),
            teams: Some(
                favourite
                    .teams
                    .iter()
                    .map(|t| MiniGameTeamEdit {
                        id: None,
                        name: t.name.clone(),
                        color: t.color,
                        settings: edits(&t.settings),
                    })
                    .collect(),
            ),
            quiet: true,
            reset: true,
        },
        Some(game),
    )?;
    for _ in 0..(TICKS * 4) {
        step(&mut s)?;
    }
    let view = s
        .minigame_views()
        .into_iter()
        .find(|g| g.id == game)
        .context("the game")?;
    let team_name: BTreeMap<u32, String> = view
        .teams
        .iter()
        .map(|t| (t.id.0, t.name.clone()))
        .collect();
    let bots: Vec<OwnerId> = s
        .vitals()
        .keys()
        .copied()
        .filter(|o| s.is_bot(*o))
        .collect();
    println!(
        "save {:?} on {}, favourite slot {}: {} bots, teams {:?}",
        entry.info.name,
        entry.map_id,
        slot + 1,
        bots.len(),
        team_name.values().collect::<Vec<_>>()
    );
    let balls: Vec<u64> = s
        .vehicle_infos()
        .into_iter()
        .filter(|v| v.definition.to_ascii_lowercase().contains("ball"))
        .map(|v| v.id)
        .collect();
    println!("balls in play: {}", balls.len());

    #[derive(Default)]
    struct Bot {
        alive: u64,
        near_ball: u64,
        pressed: u64,
        airborne: u64,
        touches: u64,
        doing: BTreeMap<&'static str, u64>,
        held: BTreeMap<String, u64>,
    }
    let mut per: BTreeMap<OwnerId, Bot> = BTreeMap::new();
    let mut ball_travel = 0.0f32;
    let mut ball_moving = 0u64;
    let mut ball_was: BTreeMap<u64, (Vec3, Vec3)> = BTreeMap::new();
    let mut respawns = 0u64;
    let team_scores = |s: &bri_sim::session::Session| {
        let mut out: BTreeMap<u32, i64> = BTreeMap::new();
        for v in s.vitals().values() {
            if let Some(t) = v.team {
                *out.entry(t).or_default() += v.score;
            }
        }
        out
    };
    let start = team_scores(&s);
    for tick in 0..seconds * TICKS {
        step(&mut s)?;
        // What the first bot's objective planner sees, a few seconds in
        // and again later: why it does or does not go for the ball.
        if (tick == 5 * TICKS || tick == 60 * TICKS)
            && let Some(bot) = bots.first()
        {
            eprintln!("--- objective planner of {bot:?} at {}s ---", tick / TICKS);
            eprintln!("{}", s.bot_objective_report(*bot));
        }
        let vitals = s.vitals();
        let feet: BTreeMap<OwnerId, (Vec3, bool)> = s
            .motion_states()
            .into_iter()
            .map(|(p, _)| (p.owner, (Vec3::from(p.feet), p.grounded)))
            .collect();
        let poses: BTreeMap<u64, (Vec3, Vec3)> = s
            .vehicle_poses()
            .into_iter()
            .filter(|p| balls.contains(&p.id) || !ball_was.contains_key(&p.id))
            .map(|p| (p.id, (Vec3::from(p.position), Vec3::from(p.velocity))))
            .collect();
        let infos = s.vehicle_infos();
        let ball_now: Vec<(u64, Vec3, Vec3)> = infos
            .iter()
            .filter(|v| v.definition.to_ascii_lowercase().contains("ball"))
            .filter_map(|v| poses.get(&v.id).map(|(p, vel)| (v.id, *p, *vel)))
            .collect();
        for (id, at, vel) in &ball_now {
            if let Some((was, was_vel)) = ball_was.get(id) {
                {
                    let moved = (*at - *was).length();
                    if moved > 2.0 {
                        respawns += 1;
                    } else {
                        ball_travel += moved;
                    }
                    if vel.length() > 0.5 {
                        ball_moving += 1;
                    }
                    // A kick: the ball's speed jumps with a bot beside it.
                    if (*vel - *was_vel).length() > 2.0 {
                        let toucher = bots
                            .iter()
                            .filter_map(|b| feet.get(b).map(|(f, _)| (*b, (*f - *at).length())))
                            .filter(|(_, d)| *d < 3.0)
                            .min_by(|a, b| a.1.total_cmp(&b.1));
                        if let Some((b, _)) = toucher {
                            per.entry(b).or_default().touches += 1;
                        }
                    }
                }
            }
        }
        ball_was = ball_now.iter().map(|(id, p, v)| (*id, (*p, *v))).collect();
        let held: BTreeMap<OwnerId, String> = s
            .tool_inventories()
            .into_iter()
            .filter_map(|(o, inv)| {
                inv.selected
                    .and_then(|i| inv.slots.get(i).cloned().flatten())
                    .map(|i| (o, i))
            })
            .collect();
        let thoughts: BTreeMap<OwnerId, &'static str> = s
            .bot_thoughts()
            .into_iter()
            .map(|t| (t.bot, t.behaviour))
            .collect();
        for b in &bots {
            let Some(v) = vitals.get(b) else { continue };
            if !v.alive {
                continue;
            }
            let Some((at, grounded)) = feet.get(b).copied() else {
                continue;
            };
            let e = per.entry(*b).or_default();
            e.alive += 1;
            if !grounded {
                e.airborne += 1;
            }
            let ball_dist = ball_now
                .iter()
                .map(|(_, p, _)| (*p - at).length())
                .fold(f32::INFINITY, f32::min);
            if ball_dist < 4.0 {
                e.near_ball += 1;
            }
            // Pressed against an enemy with the ball well away: butting heads.
            let enemy_close = bots.iter().any(|o| {
                o != b
                    && vitals.get(o).is_some_and(|w| w.alive && w.team != v.team)
                    && feet
                        .get(o)
                        .is_some_and(|(f, _)| Vec3::new(f.x - at.x, 0.0, f.z - at.z).length() < 1.5)
            });
            if enemy_close && ball_dist > 6.0 {
                e.pressed += 1;
            }
            if tick % TICKS == 0 {
                if let Some(doing) = thoughts.get(b) {
                    *e.doing.entry(doing).or_default() += 1;
                }
                *e.held
                    .entry(held.get(b).cloned().unwrap_or_else(|| "(hands)".into()))
                    .or_default() += 1;
            }
        }
    }
    let end = team_scores(&s);
    println!("ran {seconds} s");
    for (team, score) in &end {
        println!(
            "team {:?}: score {} (+{})",
            team_name.get(team).map_or("?", String::as_str),
            score,
            score - start.get(team).copied().unwrap_or(0)
        );
    }
    println!(
        "ball: travelled {ball_travel:.0} units, moving {:.0}% of the time, reset {respawns} times (goals or out)",
        100.0 * ball_moving as f32 / (seconds * TICKS) as f32
    );
    for (b, e) in &per {
        let team = vitals_team(&s, *b)
            .and_then(|t| team_name.get(&t).cloned())
            .unwrap_or_default();
        let share = |n: u64| 100.0 * n as f32 / e.alive.max(1) as f32;
        println!(
            "bot {b} [{team}]: near ball {:.0}%, butting an enemy away from it {:.0}%, airborne {:.0}%, touches {}\n  doing {:?}\n  held {:?}",
            share(e.near_ball),
            share(e.pressed),
            share(e.airborne),
            e.touches,
            e.doing,
            e.held
        );
    }
    Ok(())
}

fn vitals_team(s: &bri_sim::session::Session, owner: OwnerId) -> Option<u32> {
    s.vitals().get(&owner).and_then(|v| v.team)
}
