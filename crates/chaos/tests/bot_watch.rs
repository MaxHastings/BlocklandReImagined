//! Watch bots play a real save: a headless host on the save's map with the
//! player's own Add-On choices and a saved Mini-Game preset (Add-Ons window
//! slot), no window and no input. Every tick is read from the public
//! session; nothing here steers a bot. Writes a summary, the odd moments it
//! saw (stuck, circling, team kills, falls, long goofs, flip-flops, fighting
//! with a tool) and a trace for top-down frames.
//!
//! The save, settings and choices are only read.
//!
//! BRI_WATCH_SAVE=<.world.json> BRI_WATCH_SETTINGS=<settings.json> BRI_WATCH_SLOT=0
//! BRI_WATCH_CHOICES=<add-on-choices.json> BRI_WATCH_SECONDS=180 BRI_WATCH_OUT=<dir>
//! [BRI_WATCH_MODE=Slayer_TeamDeathmatch] [BRI_WATCH_FILL=4] [BRI_WATCH_HOST=leave|stay]
//! cargo test --release -p bri-chaos --test bot_watch -- --ignored --nocapture
use anyhow::{Context, Result};
use bri_minigames::SettingValue;
use bri_package::packages::PackageSet;
use bri_sim::player::MoveInput;
use bri_sim::session::{Command, MiniGameRequest, Session, SettingEdit, TeamEdit};
use bri_world::OwnerId;
use glam::Vec3;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::Write;
use std::path::{Path, PathBuf};

const TPS: u64 = 120;

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

fn content_root() -> PathBuf {
    env("BRI_CONTENT").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"),
        PathBuf::from,
    )
}

/// The packages the player's game runs: the release defaults, then their
/// saved choices on top, with each Add-On's companions following it.
fn packages(root: &Path, choices: Option<&Path>) -> Result<PackageSet> {
    let mut set = PackageSet::load_root(root)?;
    let Some(choices) = choices else {
        return Ok(set);
    };
    let choices: Value = serde_json::from_slice(&std::fs::read(choices)?)?;
    let library = bri_package::library::Library::scan(root)?;
    for (id, wanted) in choices["packages"].as_object().context("choices")? {
        let wanted = wanted.as_bool().unwrap_or(false);
        let present = set.packages.iter().any(|p| &p.id == id);
        if wanted && !present {
            match library.entries.iter().find(|e| e.id() == id) {
                Some(e) => set.packages.push(e.package.clone()),
                None => eprintln!("WATCH choice {id} is not installed here"),
            }
        } else if !wanted && present {
            set.packages.retain(|p| &p.id != id);
        }
    }
    bri_package::library::follow_companions(root, &mut set);
    Ok(set)
}

fn setting_value(v: &Value) -> Option<SettingValue> {
    serde_json::from_value(v.clone()).ok()
}

/// The UI's saved rules (seconds) as the mini-game's settings (ms).
fn rules(r: &Value) -> bri_minigames::Settings {
    let mut s = bri_minigames::Settings::default();
    let b = |k: &str, d: bool| r[k].as_bool().unwrap_or(d);
    let i = |k: &str, d: i32| r[k].as_i64().map_or(d, |v| v as i32);
    let ms = |k: &str, d: u32| r[k].as_f64().map_or(d, |v| (v * 1000.0) as u32);
    s.title = r["title"].as_str().unwrap_or("Bot Watch").into();
    s.invite_only = b("invite_only", false);
    s.use_all_players_bricks = b("use_all_players_bricks", s.use_all_players_bricks);
    s.players_use_own_bricks = b("players_use_own_bricks", s.players_use_own_bricks);
    s.use_spawn_bricks = b("use_spawn_bricks", s.use_spawn_bricks);
    s.points_break_brick = i("points_break_brick", s.points_break_brick);
    s.points_plant_brick = i("points_plant_brick", s.points_plant_brick);
    s.points_kill_player = i("points_kill_player", s.points_kill_player);
    s.points_kill_self = i("points_kill_self", s.points_kill_self);
    s.points_die = i("points_die", s.points_die);
    s.respawn_ms = ms("respawn_seconds", s.respawn_ms);
    s.vehicle_respawn_ms = ms("vehicle_respawn_seconds", s.vehicle_respawn_ms);
    s.brick_respawn_ms = ms("brick_respawn_seconds", s.brick_respawn_ms);
    s.falling_damage = b("falling_damage", s.falling_damage);
    s.weapon_damage = b("weapon_damage", s.weapon_damage);
    s.self_damage = b("self_damage", s.self_damage);
    s.vehicle_damage = b("vehicle_damage", s.vehicle_damage);
    s.brick_damage = b("brick_damage", s.brick_damage);
    s.enable_wand = b("enable_wand", s.enable_wand);
    s.enable_building = b("enable_building", s.enable_building);
    s.enable_painting = b("enable_painting", s.enable_painting);
    if let Some(p) = r["player_type"].as_str() {
        s.player_type = p.into();
    }
    if let Some(l) = r["loadout"].as_array() {
        for (slot, item) in l.iter().take(5).enumerate() {
            s.loadout[slot] = item.as_str().map(str::to_string);
        }
    }
    if let Some(l) = env("BRI_WATCH_LOADOUT") {
        s.loadout = Default::default();
        for (slot, item) in l.split(',').take(5).enumerate() {
            s.loadout[slot] = Some(item.trim().to_string());
        }
    }
    s
}

#[derive(Default)]
struct Track {
    window: VecDeque<(Vec3, bool)>,
    path: VecDeque<Vec3>,
    stuck_since: Option<(u64, Vec3, &'static str, Option<[f32; 3]>)>,
    circling_since: Option<(u64, Vec3, &'static str)>,
    idle_since: Option<(u64, Vec3)>,
    goof_since: Option<(u64, Vec3, String)>,
    clump_since: Option<(u64, Vec3, OwnerId)>,
    still_fight_since: Option<(u64, Vec3, String)>,
    tool_fight_since: Option<(u64, Vec3, String)>,
    switches: VecDeque<(u64, &'static str)>,
    /// Where and when this life began.
    spawned: Option<(u64, Vec3)>,
    flip_reported: u64,
    was_alive: bool,
    last_behaviour: Option<&'static str>,
}

struct Moment {
    kind: &'static str,
    bot: OwnerId,
    start: u64,
    end: u64,
    at: Vec3,
    what: String,
}

fn held(images: &BTreeMap<OwnerId, Vec<bri_sim::session::MountedImage>>, o: OwnerId) -> String {
    images
        .get(&o)
        .and_then(|v| v.iter().find(|m| m.hand == 0).or(v.first()))
        .map(|m| {
            m.image
                .rsplit(['.', '/', ':'])
                .next()
                .unwrap_or(&m.image)
                .to_string()
        })
        .unwrap_or_default()
}

fn is_tool(item: &str) -> bool {
    let i = item.to_ascii_lowercase();
    ["hammer", "wrench", "printgun", "spraycan", "brick", "wand"]
        .iter()
        .any(|t| i.contains(t))
}

fn secs(t: u64, t0: u64) -> f32 {
    t.saturating_sub(t0) as f32 / TPS as f32
}

#[test]
#[ignore = "watching tool: needs generated content and a real save"]
fn watch_bots_play_a_real_save() -> Result<()> {
    let Some(save) = env("BRI_WATCH_SAVE") else {
        eprintln!("skipped: needs BRI_WATCH_SAVE");
        return Ok(());
    };
    let save = PathBuf::from(save);
    let root = content_root();
    let out = PathBuf::from(env("BRI_WATCH_OUT").unwrap_or_else(|| "target/bot-watch".into()));
    std::fs::create_dir_all(&out)?;
    let seconds: u64 = env("BRI_WATCH_SECONDS").map_or(180, |s| s.parse().unwrap());
    let mut build = bri_world::build::decode(&std::fs::read(&save)?)?;
    // A save without a mini-game borrows another save's (Slayer config,
    // teams), as if the host had loaded that one first; Save & Reset then
    // applies the preset over it.
    if build.minigame.is_none()
        && let Some(from) = env("BRI_WATCH_GAME_FROM")
    {
        let other = bri_world::build::decode(&std::fs::read(&from)?)?;
        let mut game = other.minigame.context("that save has no mini-game")?;
        if let Some(mode) = env("BRI_WATCH_MODE") {
            game = serde_json::from_str(&game.to_string().replace("Slayer_TeamDeathmatch", &mode))?;
        }
        build.minigame = Some(game);
    }
    if let Some(g) = &build.minigame {
        std::fs::create_dir_all(&out)?;
        std::fs::write(
            out.join("saved-minigame.json"),
            serde_json::to_vec_pretty(g)?,
        )?;
    }
    eprintln!(
        "WATCH save {} map {} bricks {} saved minigame {}",
        save.display(),
        build.world.map_id,
        build.world.bricks.len(),
        build.minigame.is_some()
    );
    let choices = env("BRI_WATCH_CHOICES").map(PathBuf::from);
    let set = packages(&root, choices.as_deref())?;
    eprintln!(
        "WATCH packages: {}",
        set.packages
            .iter()
            .filter(|p| p.role.is_none())
            .map(|p| p.id.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    );
    let schema = bri_world::World::new(
        "Bot Watch".into(),
        build.world.map_id.clone(),
        build.world.palette.clone(),
    );
    let dedicated = bri_net::dedicated::load_packages(&root, &set, schema)?;
    let spawn = dedicated.spawn_points[0];
    let mut s: Session = dedicated.session;
    let host = s.join("Host".into(), spawn, true)?;
    let mut seq = 0u64;
    let mut cmd = 0u64;
    let step = |s: &mut Session, n: u64, seq: &mut u64| -> Result<()> {
        for _ in 0..n {
            *seq += 1;
            s.movement(host, *seq, MoveInput::default())?;
            s.step()?;
        }
        Ok(())
    };
    let bricks_saved = build.world.bricks.len();
    let brick_points: Vec<[f32; 3]> = build.world.bricks.values().map(|b| b.position).collect();
    cmd += 1;
    s.command(
        host,
        cmd,
        Command::LoadBuild {
            build: Box::new(build),
            ownership: false,
        },
    )?;
    for i in 0..(TPS * 120) {
        step(&mut s, 1, &mut seq)?;
        if i > 10 && !s.build_loading() {
            break;
        }
    }
    eprintln!(
        "WATCH loaded {} of {} bricks; games {}",
        s.simulation().state().bricks.len(),
        bricks_saved,
        s.minigame_views().len()
    );

    // The preset, as Load slot then Save & Reset sends it.
    let mut fill_override = env("BRI_WATCH_FILL").map(|f| f.parse::<i64>().unwrap());
    if let Some(path) = env("BRI_WATCH_SETTINGS") {
        let slot = env("BRI_WATCH_SLOT").unwrap_or_else(|| "0".into());
        let settings: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
        let fav = &settings["settings"]["addon_favorites"][&slot];
        anyhow::ensure!(fav.is_object(), "slot {slot} is empty");
        if s.minigame_views().iter().all(|g| g.owner != host) {
            cmd += 1;
            s.command(
                host,
                cmd,
                Command::MiniGame(MiniGameRequest::Create {
                    color: 0,
                    settings: rules(&fav["rules"]),
                }),
            )?;
            step(&mut s, 4, &mut seq)?;
        } else {
            cmd += 1;
            s.command(
                host,
                cmd,
                Command::MiniGame(MiniGameRequest::Configure {
                    settings: rules(&fav["rules"]),
                }),
            )?;
            step(&mut s, 4, &mut seq)?;
        }
        s.minigame_views()
            .into_iter()
            .find(|g| g.owner == host)
            .context("host's game")?;
        let mut edits: Vec<SettingEdit> = fav["settings"]
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(k, v)| {
                Some(SettingEdit {
                    key: k.clone(),
                    value: Some(setting_value(v)?),
                })
            })
            .collect();
        if let Some(mode) = env("BRI_WATCH_MODE") {
            edits.retain(|e| !e.key.ends_with(":mode"));
            edits.push(SettingEdit {
                key: "gamemode_slayer-rules:mode".into(),
                value: Some(SettingValue::Text(mode)),
            });
        }
        let teams_for = |view: &bri_sim::session::MiniGameView| {
            fav["teams"].as_array().map(|teams| {
                teams
                    .iter()
                    .enumerate()
                    .map(|(i, t)| TeamEdit {
                        id: view.teams.get(i).map(|v| v.id.0),
                        name: t["name"].as_str().unwrap_or("Team").into(),
                        color: t["color"].as_u64().unwrap_or(i as u64) as u8,
                        settings: t["settings"]
                            .as_object()
                            .into_iter()
                            .flatten()
                            .filter_map(|(k, v)| {
                                let value = if k.ends_with(":team_bot_fill")
                                    && let Some(f) = fill_override
                                {
                                    SettingValue::Int(f)
                                } else {
                                    setting_value(v)?
                                };
                                Some(SettingEdit {
                                    key: k.clone(),
                                    value: Some(value),
                                })
                            })
                            .collect(),
                    })
                    .collect()
            })
        };
        // Twice: a mode that needs teams is refused until the teams exist.
        for _ in 0..2 {
            let view = s
                .minigame_views()
                .into_iter()
                .find(|g| g.owner == host)
                .context("host's game")?;
            cmd += 1;
            let reply = s.command(
                host,
                cmd,
                Command::MiniGame(MiniGameRequest::AddOnSettings {
                    game: view.id,
                    settings: edits.clone(),
                    teams: teams_for(&view),
                    quiet: false,
                    reset: env("BRI_WATCH_RESET").is_some(),
                }),
            )?;
            eprintln!("WATCH settings reply: {reply:?}");
            step(&mut s, 8, &mut seq)?;
            for g in s.minigame_views() {
                eprintln!(
                    "WATCH pass: game {} teams {:?} mode {:?} members {}",
                    g.id,
                    g.teams
                        .iter()
                        .map(|t| (t.id.0, t.name.clone(), t.addon_settings.len()))
                        .collect::<Vec<_>>(),
                    g.addon_settings.get("gamemode_slayer-rules:mode"),
                    g.members.len()
                );
            }
            for (_, n) in s.take_private_notices() {
                eprintln!("WATCH notice: {n:?}");
            }
        }
        fill_override = None;
    }
    let _ = fill_override;
    for (o, n) in s.take_private_notices() {
        eprintln!("WATCH notice to {o}: {n:?}");
    }
    if env("BRI_WATCH_HOST").as_deref() == Some("leave") {
        cmd += 1;
        let _ = s.command(host, cmd, Command::MiniGame(MiniGameRequest::Leave));
        step(&mut s, 4, &mut seq)?;
    }
    // Warm-up: the round's start.
    step(&mut s, TPS * 2, &mut seq)?;
    let games = s.minigame_views();
    eprintln!(
        "WATCH games {:?}",
        games
            .iter()
            .map(|g| (
                g.id,
                g.members.len(),
                g.teams.len(),
                g.addon_settings.get("gamemode_slayer-rules:mode").cloned()
            ))
            .collect::<Vec<_>>()
    );

    let t0 = s.simulation().state().tick;
    let mut tracks: BTreeMap<OwnerId, Track> = BTreeMap::new();
    let mut moments: Vec<Moment> = vec![];
    let mut seen_shots = BTreeSet::new();
    let mut last_death = t0;
    let mut kills: BTreeMap<OwnerId, i64> = BTreeMap::new();
    let mut deaths: BTreeMap<OwnerId, i64> = BTreeMap::new();
    let (mut team_kills, mut self_kills, mut accidents) = (0, 0, 0);
    let mut shots_by: BTreeMap<String, u64> = BTreeMap::new();
    let mut held_ticks: BTreeMap<String, u64> = BTreeMap::new();
    let mut behaviours: BTreeMap<&'static str, u64> = BTreeMap::new();
    let (mut bot_ticks, mut stuck_ticks, mut idle_ticks, mut goof_ticks, mut circling_ticks) =
        (0u64, 0u64, 0u64, 0u64, 0u64);
    let mut at_ally = 0u64;
    let mut chat_after = s.chat().last().map_or(0, |c| c.id);
    let mut chat_log = vec![];
    let mut trace = std::io::BufWriter::new(std::fs::File::create(out.join("trace.jsonl"))?);
    let floor = brick_points
        .iter()
        .map(|p| p[1])
        .fold(f32::INFINITY, f32::min)
        .min(spawn.y)
        - 8.0;
    let wall = std::time::Instant::now();
    let mut think_nanos = 0u64;

    for _ in 0..(seconds * TPS) {
        let before = s.bot_think_nanos();
        step(&mut s, 1, &mut seq)?;
        think_nanos += s.bot_think_nanos().saturating_sub(before);
        let tick = s.simulation().state().tick;
        let vitals = s.vitals();
        let states: BTreeMap<OwnerId, bri_sim::player::PlayerState> = s
            .motion_states()
            .into_iter()
            .map(|(p, _)| (p.owner, p))
            .collect();
        let thoughts: BTreeMap<OwnerId, bri_sim::session::BotThought> =
            s.bot_thoughts().into_iter().map(|t| (t.bot, t)).collect();
        let view = s.weapon_view();
        let alive = |o: &OwnerId| vitals.get(o).is_some_and(|v| v.alive);
        let team = |o: &OwnerId| vitals.get(o).and_then(|v| v.team);
        let names = s.names();
        let name = |o: &OwnerId| names.get(o).cloned().unwrap_or_else(|| format!("#{o}"));
        // Opponents alive in the game: work a wandering bot is skipping.
        let enemies_of = |o: &OwnerId| {
            vitals.iter().any(|(e, v)| {
                e != o
                    && v.alive
                    && v.minigame.is_some()
                    && v.minigame == vitals[o].minigame
                    && (v.team.is_none() || v.team != vitals[o].team)
            })
        };
        for (bot, th) in &thoughts {
            let Some(st) = states.get(bot) else { continue };
            let feet = Vec3::from(st.feet);
            let item = held(&view.images, *bot);
            if tick.is_multiple_of(30) {
                let line = json!({
                    "t": secs(tick, t0), "bot": bot, "name": name(bot), "team": team(bot),
                    "alive": alive(bot), "hp": vitals.get(bot).map(|v| v.health),
                    "at": [feet.x, feet.y, feet.z], "yaw": st.yaw, "b": th.behaviour,
                    "leg": th.leg, "held": item, "vis": th.visible, "goal": th.goal,
                    "next": th.next, "goof": th.surprise.interrupt.as_ref().map(|i| format!("{i:?}")),
                    "obj": th.objective_detail.as_ref().map(|d| format!("{} {}", d.action, d.phase)),
                    "objdiag": th.objective_diagnostic,
                    "mounted": vitals.get(bot).and_then(|v| v.mounted).is_some(),
                    "score": vitals.get(bot).map(|v| v.score),
                });
                writeln!(trace, "{line}")?;
            }
            let tr = tracks.entry(*bot).or_default();
            let close = |tr: &mut Track, moments: &mut Vec<Moment>| {
                for (kind, since, min) in [
                    (
                        "stuck",
                        tr.stuck_since
                            .take()
                            .map(|(t, a, b, n)| (t, a, format!("{b}, next waypoint {n:?}"))),
                        3.0,
                    ),
                    (
                        "circling",
                        tr.circling_since
                            .take()
                            .map(|(t, a, b)| (t, a, b.to_string())),
                        6.0,
                    ),
                    (
                        "idle with enemies about",
                        tr.idle_since.take().map(|(t, a)| (t, a, "wander".into())),
                        6.0,
                    ),
                    ("long goof", tr.goof_since.take(), 8.0),
                    (
                        "standing inside an ally",
                        tr.clump_since
                            .take()
                            .map(|(t, a, o)| (t, a, format!("with #{o}"))),
                        3.0,
                    ),
                    (
                        "standing still in a fight",
                        tr.still_fight_since.take(),
                        4.0,
                    ),
                    (
                        "fighting with a building tool",
                        tr.tool_fight_since.take(),
                        3.0,
                    ),
                ] {
                    if let Some((start, at, what)) = since
                        && secs(tick, start) >= min
                    {
                        moments.push(Moment {
                            kind,
                            bot: *bot,
                            start,
                            end: tick,
                            at,
                            what,
                        });
                    }
                }
            };
            if !alive(bot) || vitals[bot].mounted.is_some() {
                tr.window.clear();
                tr.path.clear();
                tr.was_alive = false;
                close(tr, &mut moments);
                continue;
            }
            if !tr.was_alive {
                tr.last_behaviour = None;
                tr.spawned = Some((tick, feet));
            }
            tr.was_alive = true;
            bot_ticks += 1;
            *behaviours.entry(th.behaviour).or_default() += 1;
            if !item.is_empty() {
                *held_ticks.entry(item.clone()).or_default() += 1;
            }
            if feet.y < floor {
                tr.window.clear();
                tr.path.clear();
                continue;
            }
            let flat = Vec3::new(feet.x, 0.0, feet.z);
            let wants = th.next.is_some()
                || matches!(th.behaviour, "chase" | "search" | "return")
                    && th.goal.is_some_and(|g| Vec3::from(g).distance(feet) > 1.5);
            tr.window.push_back((flat, wants));
            if tr.window.len() > TPS as usize {
                tr.window.pop_front();
            }
            let stuck = tr.window.len() == TPS as usize
                && tr.window.iter().all(|(_, w)| *w)
                && tr.window.front().unwrap().0.distance(flat) < 0.5;
            if stuck {
                stuck_ticks += 1;
                tr.stuck_since
                    .get_or_insert((tick, feet, th.behaviour, th.next));
            } else if let Some((start, at, b, n)) = tr.stuck_since.take()
                && secs(tick, start) >= 3.0
            {
                let life = tr.spawned.map_or(String::new(), |(t, p)| {
                    format!(
                        ", {:.1}s after spawning {:.1} away at ({:.1},{:.1},{:.1})",
                        secs(start, t),
                        p.distance(at),
                        p.x,
                        p.y,
                        p.z
                    )
                });
                moments.push(Moment {
                    kind: "stuck",
                    bot: *bot,
                    start,
                    end: tick,
                    at,
                    what: format!("{b}, next waypoint {n:?}{life}"),
                });
            }
            tr.path.push_back(flat);
            if tr.path.len() > 4 * TPS as usize {
                tr.path.pop_front();
            }
            let circling = tr.path.len() == 4 * TPS as usize && {
                let length: f32 = tr
                    .path
                    .iter()
                    .zip(tr.path.iter().skip(1))
                    .map(|(a, b)| a.distance(*b))
                    .sum();
                length > 6.0 && tr.path.front().unwrap().distance(flat) < 0.2 * length
            };
            if circling {
                circling_ticks += 1;
                tr.circling_since.get_or_insert((tick, feet, th.behaviour));
            } else if let Some((start, at, b)) = tr.circling_since.take()
                && secs(tick, start) >= 6.0
            {
                moments.push(Moment {
                    kind: "circling",
                    bot: *bot,
                    start,
                    end: tick,
                    at,
                    what: b.into(),
                });
            }
            let idle = th.behaviour == "wander" && enemies_of(bot);
            if idle {
                idle_ticks += 1;
                tr.idle_since.get_or_insert((tick, feet));
            } else if let Some((start, at)) = tr.idle_since.take()
                && secs(tick, start) >= 6.0
            {
                moments.push(Moment {
                    kind: "idle with enemies about",
                    bot: *bot,
                    start,
                    end: tick,
                    at,
                    what: "wander".into(),
                });
            }
            if let Some(i) = &th.surprise.interrupt {
                goof_ticks += 1;
                tr.goof_since.get_or_insert((tick, feet, format!("{i:?}")));
            } else if let Some((start, at, what)) = tr.goof_since.take()
                && secs(tick, start) >= 8.0
            {
                moments.push(Moment {
                    kind: "long goof",
                    bot: *bot,
                    start,
                    end: tick,
                    at,
                    what,
                });
            }
            let ally = states.iter().find(|(o, p)| {
                *o != bot
                    && alive(o)
                    && team(o).is_some()
                    && team(o) == team(bot)
                    && vitals.get(o).is_some_and(|v| v.mounted.is_none())
                    && Vec3::from(p.feet).distance(feet) < 0.9
            });
            if let Some((o, _)) = ally {
                tr.clump_since.get_or_insert((tick, feet, *o));
            } else if let Some((start, at, o)) = tr.clump_since.take()
                && secs(tick, start) >= 3.0
            {
                moments.push(Moment {
                    kind: "standing inside an ally",
                    bot: *bot,
                    start,
                    end: tick,
                    at,
                    what: format!("with {}", name(&o)),
                });
            }
            let speed = Vec3::new(st.velocity[0], 0.0, st.velocity[2]).length();
            if th.behaviour == "fight" && speed < 0.3 {
                tr.still_fight_since.get_or_insert((
                    tick,
                    feet,
                    format!("holding {item}, target {:?}", th.visible.map(|v| name(&v))),
                ));
            } else if let Some((start, at, what)) = tr.still_fight_since.take()
                && secs(tick, start) >= 4.0
            {
                moments.push(Moment {
                    kind: "standing still in a fight",
                    bot: *bot,
                    start,
                    end: tick,
                    at,
                    what,
                });
            }
            if th.behaviour == "fight" && is_tool(&item) {
                tr.tool_fight_since
                    .get_or_insert((tick, feet, format!("holding {item}")));
            } else if let Some((start, at, what)) = tr.tool_fight_since.take()
                && secs(tick, start) >= 3.0
            {
                moments.push(Moment {
                    kind: "fighting with a building tool",
                    bot: *bot,
                    start,
                    end: tick,
                    at,
                    what,
                });
            }
            if tr.last_behaviour.is_some_and(|b| b != th.behaviour) {
                tr.switches.push_back((tick, th.behaviour));
            }
            while tr.switches.front().is_some_and(|(t, _)| tick - t > 5 * TPS) {
                tr.switches.pop_front();
            }
            if tr.switches.len() >= 8 && tick > tr.flip_reported + 10 * TPS {
                tr.flip_reported = tick;
                let seq: Vec<_> = tr.switches.iter().map(|(_, b)| *b).collect();
                moments.push(Moment {
                    kind: "flip-flopping",
                    bot: *bot,
                    start: tick - 5 * TPS,
                    end: tick,
                    at: feet,
                    what: seq.join(">"),
                });
            }
            tr.last_behaviour = Some(th.behaviour);
        }
        for p in view.fired() {
            if !seen_shots.insert(p.id) {
                continue;
            }
            let shooter = p.source.0;
            if !thoughts.contains_key(&shooter) {
                continue;
            }
            *shots_by.entry(format!("{:?}", p.definition)).or_default() += 1;
            let dir = p.velocity.normalize_or_zero();
            if dir == Vec3::ZERO {
                continue;
            }
            let nearest = states
                .iter()
                .filter(|(o, _)| **o != shooter && alive(o))
                .filter_map(|(o, st)| {
                    let c = Vec3::from(st.feet) + Vec3::Y;
                    let along = (c - p.origin).dot(dir);
                    if !(0.0..40.0).contains(&along) {
                        return None;
                    }
                    ((c - p.origin - dir * along).length() < 1.0).then_some((along, *o))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((_, hit)) = nearest
                && team(&hit).is_some()
                && team(&hit) == team(&shooter)
            {
                at_ally += 1;
                if at_ally % 10 == 1 {
                    moments.push(Moment {
                        kind: "shooting into an ally",
                        bot: shooter,
                        start: tick,
                        end: tick,
                        at: p.origin,
                        what: format!("{:?} toward {}", p.definition, name(&hit)),
                    });
                }
            }
        }
        let new: Vec<_> = s
            .death_results()
            .filter(|d| d.tick > last_death)
            .cloned()
            .collect();
        for d in new {
            last_death = last_death.max(d.tick);
            *deaths.entry(d.victim).or_default() += 1;
            let at = states
                .get(&d.victim)
                .map_or(Vec3::ZERO, |p| Vec3::from(p.feet));
            match d.killer {
                None => {
                    accidents += 1;
                    if s.is_bot(d.victim) {
                        moments.push(Moment {
                            kind: "died with no killer (fall/void/water)",
                            bot: d.victim,
                            start: d.tick,
                            end: d.tick,
                            at,
                            what: String::new(),
                        });
                    }
                }
                Some(k) if k == d.victim => {
                    self_kills += 1;
                    moments.push(Moment {
                        kind: "killed itself",
                        bot: k,
                        start: d.tick,
                        end: d.tick,
                        at,
                        what: format!("holding {}", held(&view.images, k)),
                    });
                }
                Some(k) if team(&k).is_some() && team(&k) == team(&d.victim) => {
                    team_kills += 1;
                    moments.push(Moment {
                        kind: "team kill",
                        bot: k,
                        start: d.tick,
                        end: d.tick,
                        at,
                        what: format!(
                            "killed {} holding {}",
                            name(&d.victim),
                            held(&view.images, k)
                        ),
                    });
                }
                Some(k) => *kills.entry(k).or_default() += 1,
            }
        }
        for c in s.chat_after(chat_after) {
            chat_after = c.id;
            chat_log.push(format!("{:.1}s {}: {}", secs(tick, t0), c.name, c.text));
        }
    }
    trace.flush()?;
    // Close what is still open at the end.
    let end = s.simulation().state().tick;
    for (bot, tr) in tracks.iter_mut() {
        for (kind, since, min) in [
            (
                "stuck",
                tr.stuck_since
                    .take()
                    .map(|(t, a, b, n)| (t, a, format!("{b}, next waypoint {n:?}"))),
                3.0,
            ),
            (
                "idle with enemies about",
                tr.idle_since.take().map(|(t, a)| (t, a, "wander".into())),
                6.0,
            ),
            ("long goof", tr.goof_since.take(), 8.0),
            (
                "standing still in a fight",
                tr.still_fight_since.take(),
                4.0,
            ),
            (
                "fighting with a building tool",
                tr.tool_fight_since.take(),
                3.0,
            ),
        ] {
            if let Some((start, at, what)) = since
                && secs(end, start) >= min
            {
                moments.push(Moment {
                    kind,
                    bot: *bot,
                    start,
                    end,
                    at,
                    what: format!("{what} (still at the end)"),
                });
            }
        }
    }
    let vitals = s.vitals();
    let names = s.names();
    let name = |o: &OwnerId| names.get(o).cloned().unwrap_or_else(|| format!("#{o}"));
    let bots: Vec<OwnerId> = vitals.keys().copied().filter(|o| s.is_bot(*o)).collect();
    let share = |t: u64| 100.0 * t as f64 / bot_ticks.max(1) as f64;
    let per_bot: Vec<Value> = bots
        .iter()
        .map(|b| {
            json!({"bot": b, "name": name(b), "team": vitals[b].team, "score": vitals[b].score,
                   "kills": kills.get(b), "deaths": deaths.get(b)})
        })
        .collect();
    let mut by_kind: BTreeMap<&str, usize> = BTreeMap::new();
    for m in &moments {
        *by_kind.entry(m.kind).or_default() += 1;
    }
    let summary = json!({
        "save": save.display().to_string(),
        "seconds": seconds,
        "bots": bots.len(),
        "teams": s.minigame_views().iter().map(|g| g.teams.iter().map(|t| (t.id.0, t.name.clone(), bots.iter().filter(|b| vitals[b].team == Some(t.id.0)).map(|b| vitals[b].score).sum::<i64>())).collect::<Vec<_>>()).collect::<Vec<_>>(),
        // Rounds won during the watch, by winning teams (Slayer's points
        // limit ends a round: goals in a ball game, kills otherwise).
        "rounds_won": s.round_results().filter(|r| r.tick > t0).map(|r| r.teams.iter().map(|t| t.0).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "kills": kills.values().sum::<i64>(), "team_kills": team_kills, "self_kills": self_kills,
        "deaths_no_killer": accidents, "shots_at_ally": at_ally,
        "stuck_pct": share(stuck_ticks), "idle_pct": share(idle_ticks),
        "goof_pct": share(goof_ticks), "circling_pct": share(circling_ticks),
        "behaviours_pct": behaviours.iter().map(|(b, t)| (b.to_string(), (share(*t) * 10.0).round() / 10.0)).collect::<BTreeMap<_, _>>(),
        "held_pct": held_ticks.iter().map(|(b, t)| (b.clone(), (share(*t) * 10.0).round() / 10.0)).collect::<BTreeMap<_, _>>(),
        "shots": shots_by,
        "moments_by_kind": by_kind,
        "per_bot": per_bot,
        "bot_think_ms_per_tick": think_nanos as f64 / 1e6 / (seconds * TPS) as f64,
        "wall_seconds": wall.elapsed().as_secs_f64(),
        "chat_lines": chat_log.len(),
    });
    std::fs::write(
        out.join("summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    moments.sort_by_key(|m| m.start);
    let moments_json: Vec<Value> = moments
        .iter()
        .map(|m| {
            json!({"kind": m.kind, "bot": m.bot, "name": name(&m.bot), "team": vitals.get(&m.bot).and_then(|v| v.team),
                   "start": secs(m.start, t0), "end": secs(m.end, t0), "at": [m.at.x, m.at.y, m.at.z], "what": m.what})
        })
        .collect();
    std::fs::write(
        out.join("moments.json"),
        serde_json::to_vec_pretty(&moments_json)?,
    )?;
    std::fs::write(out.join("chat.txt"), chat_log.join("\n"))?;
    std::fs::write(out.join("bricks.json"), serde_json::to_vec(&brick_points)?)?;
    eprintln!("WATCH {}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}
