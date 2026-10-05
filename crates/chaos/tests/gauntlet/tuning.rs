//! Bot tuning tools over the gauntlet (`docs/architecture/bots.md`,
//! "Tuning"): the off-switch check (each dial at 0, one at a time) and the
//! sweep (dial values scored against the bands). Both replay the gauntlet's
//! own scenario functions with a thread's dials and seed applied to every
//! kind they build (`apply_dials`, `seed`), and read the report each one
//! hands over (`finish`); a scenario that fails its own checks still hands
//! its report over and counts as broken. Settings: `tests/data/bot_tuning.json`.
//! Output: `target/bot-tuning/`.
use super::shares::{self, Bands};
use super::{BOTS_JSON, Report};
use bri_sim::bot_kind::{BotKind, BotPack, tuning as dials};
use serde::Deserialize;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, VecDeque};
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub const TUNING_JSON: &str = include_str!("../data/bot_tuning.json");

thread_local! {
    static DIALS: RefCell<Vec<(String, f64)>> = const { RefCell::new(Vec::new()) };
    static SEED: Cell<u64> = const { Cell::new(0) };
    static QUIET: Cell<bool> = const { Cell::new(false) };
    static SINK: RefCell<Vec<Report>> = const { RefCell::new(Vec::new()) };
}

/// `kinds` with this thread's dials set on each kind that has them.
pub fn apply_dials(kinds: Vec<BotKind>) -> Vec<BotKind> {
    DIALS.with(|d| {
        let d = d.borrow();
        kinds
            .into_iter()
            .map(|mut k| {
                for (path, value) in d.iter() {
                    // A kind without the dial keeps its own; the tools
                    // check values on the shipped kind first.
                    if let Ok(changed) = dials::with_dial(&k, path, *value) {
                        k = changed;
                    }
                }
                k
            })
            .collect()
    })
}
/// `run` with this thread's dials set to `dials`.
pub fn with_dials<T>(dials: Vec<(String, f64)>, run: impl FnOnce() -> T) -> T {
    let old = DIALS.with(|d| std::mem::replace(&mut *d.borrow_mut(), dials));
    let out = run();
    DIALS.with(|d| *d.borrow_mut() = old);
    out
}
/// This thread's seed: 0 plays the gauntlet as its tests do.
pub fn seed() -> u64 {
    SEED.with(Cell::get)
}
pub fn quiet() -> bool {
    QUIET.with(Cell::get)
}
/// A scenario's report, kept for a tool running it on this thread.
pub fn hand_over(report: &Report) {
    SINK.with(|s| s.borrow_mut().push(report.clone()));
}

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub schema_version: u32,
    #[serde(default)]
    pub note: String,
    pub dials: Vec<String>,
    pub on_value: f64,
    #[serde(default)]
    pub on_note: String,
    pub grid: BTreeMap<String, Vec<f64>>,
    pub unchanged: Unchanged,
    pub weights: Weights,
    pub fair: FairConfig,
    /// Each dial's ON value for the all-on run, where its shipped value
    /// is off (0); others keep theirs.
    pub on: BTreeMap<String, f64>,
    pub perf: PerfConfig,
}
#[derive(Clone, Debug, Deserialize)]
pub struct FairConfig {
    #[serde(default)]
    pub note: String,
    pub min: f32,
    pub max: f32,
    pub first_seconds: f32,
    pub steady_after: f32,
    pub seconds: usize,
    pub ranges: Vec<f32>,
    pub dials: Vec<FairDial>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct FairDial {
    pub path: String,
    /// 1: the hit rate rises with the dial; -1: it falls.
    pub sign: f32,
}
#[derive(Clone, Debug, Deserialize)]
pub struct PerfConfig {
    #[serde(default)]
    pub note: String,
    /// Most bot think time per tick, in microseconds, in a debug build and
    /// in release.
    pub debug_us: f64,
    pub release_us: f64,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Unchanged {
    #[serde(default)]
    pub note: String,
    pub objective_share: f64,
    pub variety: f64,
    pub goof: f64,
    pub goof_waves: f64,
    pub stuck: f64,
    pub team_kills: f64,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Weights {
    #[serde(default)]
    pub note: String,
    pub band: f64,
    pub broken: f64,
    pub variety: f64,
    pub goof_waves: f64,
    pub stuck: f64,
    pub frame: f64,
}
impl Config {
    pub fn shipped() -> Self {
        let c: Self = serde_json::from_str(TUNING_JSON).expect("bot_tuning.json");
        assert_eq!(c.schema_version, 1, "bot_tuning.json schema");
        c
    }
}

/// The kind the dials are read from: the shipped file's first.
pub fn shipped_kind() -> BotKind {
    BotPack::from_json(BOTS_JSON).unwrap().bots.remove(0)
}

/// The dials to tune and their shipped values: every section's
/// `strength`, then the config's list (`BRI_TUNING_DIALS` replaces both).
/// Paths bots.json does not have are returned apart, to report.
pub fn resolve_dials(config: &Config) -> (Vec<(String, f64)>, Vec<String>) {
    let kind = shipped_kind();
    let wanted: Vec<String> = match std::env::var("BRI_TUNING_DIALS") {
        Ok(list) if !list.trim().is_empty() => {
            list.split(',').map(|s| s.trim().to_string()).collect()
        }
        _ => {
            let mut w: Vec<String> = dials::dials(&kind)
                .into_iter()
                .map(|(p, _)| p)
                .filter(|p| p.ends_with(".strength"))
                .collect();
            for d in &config.dials {
                if !w.contains(d) {
                    w.push(d.clone());
                }
            }
            w
        }
    };
    let mut found = Vec::new();
    let mut missing = Vec::new();
    for path in wanted {
        match dials::dial(&kind, &path) {
            Some(v) => found.push((path, v)),
            None => missing.push(path),
        }
    }
    (found, missing)
}

/// Every dial at its ON value together: a dial shipped off (0) at the
/// config's `on` value (else `on_value`), the rest as shipped.
pub fn all_on(config: &Config) -> Vec<(String, f64)> {
    resolve_dials(config)
        .0
        .into_iter()
        .filter(|(_, v)| *v == 0.0)
        .map(|(p, _)| {
            let on = config.on.get(&p).copied().unwrap_or(config.on_value);
            (p, on)
        })
        .collect()
}

/// One gauntlet scenario, as its test function.
pub type Scenario = (&'static str, fn());

/// What one run measured.
#[derive(Clone, Debug, Default)]
pub struct Metrics {
    /// Captures, laps, or else kills.
    pub objective: f64,
    pub variety: f64,
    pub goof: f64,
    pub goof_waves: f64,
    pub stuck: f64,
    pub team_kills: f64,
    /// Microseconds of session step per bot per tick.
    pub frame_us: f64,
    /// Bands flagged (any band, enforced or not), and which.
    pub bands: usize,
    pub flagged: Vec<String>,
    /// The scenario's own check that failed, if one did.
    pub broken: Option<String>,
    pub rows: Vec<shares::Row>,
}
impl Metrics {
    fn of(report: &Report, bands: &Bands) -> Self {
        let p = &report.progress;
        let captures: i64 = p
            .iter()
            .filter(|(k, _)| k.starts_with("captures"))
            .map(|(_, v)| *v)
            .sum();
        let objective = if p.keys().any(|k| k.starts_with("captures")) {
            captures as f64
        } else if let Some(laps) = p.get("laps_total") {
            *laps as f64
        } else {
            report.kills as f64
        };
        let rows = shares::rows(report, bands);
        let flagged: Vec<String> = rows
            .iter()
            .filter(|r| r.flag.bad())
            .map(|r| format!("{} {}", r.kind, r.flag.word()))
            .collect();
        Self {
            objective,
            variety: shares::variety(report) as f64,
            goof: shares::share(report, "goof") as f64,
            goof_waves: shares::goof_waves(report) as f64,
            stuck: report.share(report.stuck) as f64,
            team_kills: report.team_kills as f64,
            frame_us: report.step_nanos as f64 / 1000.0 / report.kind_ticks.max(1) as f64,
            bands: flagged.len(),
            flagged,
            broken: None,
            rows,
        }
    }
}

/// One setting of the dials, by label.
#[derive(Clone, Debug)]
pub struct Setting {
    pub label: String,
    pub dials: Vec<(String, f64)>,
}

/// Every scenario × seed × setting, on worker threads.
/// `BRI_TUNING_JOBS` workers (default: the machine's cores).
pub fn run_all(
    scenarios: &[Scenario],
    settings: &[Setting],
    seeds: u64,
) -> Vec<(usize, &'static str, u64, Metrics)> {
    let mut jobs = VecDeque::new();
    for (si, _) in settings.iter().enumerate() {
        for seed in 0..seeds {
            for scenario in scenarios {
                jobs.push_back((si, *scenario, seed));
            }
        }
    }
    let total = jobs.len();
    let jobs = Arc::new(Mutex::new(jobs));
    let out = Arc::new(Mutex::new(Vec::new()));
    let workers = std::env::var("BRI_TUNING_JOBS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(2, |n| n.get()))
        .max(1);
    // A tool's runs fail quietly (they are counted as broken); every
    // other thread's panics print as before.
    static HOOK: std::sync::Once = std::sync::Once::new();
    HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if !quiet() {
                previous(info)
            }
        }));
    });
    let started = std::time::Instant::now();
    std::thread::scope(|scope| {
        for _ in 0..workers {
            let jobs = jobs.clone();
            let out = out.clone();
            scope.spawn(move || {
                let bands = Bands::shipped();
                loop {
                    let Some((si, (name, run), seed)) = jobs.lock().unwrap().pop_front() else {
                        break;
                    };
                    DIALS.with(|d| *d.borrow_mut() = settings[si].dials.clone());
                    SEED.with(|s| s.set(seed));
                    QUIET.with(|q| q.set(true));
                    SINK.with(|s| s.borrow_mut().clear());
                    let result = std::panic::catch_unwind(run);
                    let report = SINK.with(|s| s.borrow_mut().pop());
                    let mut m = report.map_or_else(Metrics::default, |r| Metrics::of(&r, &bands));
                    if let Err(e) = result {
                        let text = e
                            .downcast_ref::<String>()
                            .cloned()
                            .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                            .unwrap_or_else(|| "panicked".into());
                        m.broken = Some(text.lines().next().unwrap_or("").to_string());
                    }
                    let mut out = out.lock().unwrap();
                    out.push((si, name, seed, m));
                    eprintln!(
                        "TUNING {}/{total} {} seed {seed} {name}{} ({:.0}s)",
                        out.len(),
                        settings[si].label,
                        if out.last().unwrap().3.broken.is_some() {
                            " BROKEN"
                        } else {
                            ""
                        },
                        started.elapsed().as_secs_f32()
                    );
                }
            });
        }
    });
    let mut out = Arc::try_unwrap(out).unwrap().into_inner().unwrap();
    out.sort_by(|a, b| (a.0, a.1, a.2).cmp(&(b.0, b.1, b.2)));
    out
}

pub fn seeds() -> u64 {
    std::env::var("BRI_TUNING_SEEDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n| *n > 0)
        .unwrap_or(1)
}
/// The scenarios `BRI_TUNING_SCENARIOS` (comma separated parts of names)
/// picks, or all.
pub fn pick(scenarios: &[Scenario]) -> Vec<Scenario> {
    match std::env::var("BRI_TUNING_SCENARIOS") {
        Ok(f) if !f.trim().is_empty() => scenarios
            .iter()
            .filter(|(n, _)| f.split(',').any(|p| n.contains(p.trim())))
            .copied()
            .collect(),
        _ => scenarios.to_vec(),
    }
}
pub fn out_dir() -> PathBuf {
    let dir = std::env::var_os("BRI_TUNING_OUT").map_or_else(
        || {
            PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
                .parent()
                .unwrap()
                .join("bot-tuning")
        },
        PathBuf::from,
    );
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Whether `kind` takes every dial of `setting`.
fn valid(kind: &BotKind, setting: &[(String, f64)]) -> Result<(), String> {
    let mut k = kind.clone();
    for (path, value) in setting {
        k = dials::with_dial(&k, path, *value).map_err(|e| format!("{e:#}"))?;
    }
    Ok(())
}

const CSV_HEAD: &str = "setting,dials,scenario,seed,objective,variety_bits,goof_share,goof_waves,stuck_share,team_kills,frame_us_per_bot,bands_flagged,broken,flags";
fn csv_row(setting: &Setting, scenario: &str, seed: u64, m: &Metrics) -> String {
    let dials = setting
        .dials
        .iter()
        .map(|(p, v)| format!("{p}={v}"))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "{},{},{scenario},{seed},{},{:.4},{:.4},{:.4},{:.4},{},{:.2},{},\"{}\",\"{}\"",
        setting.label,
        dials,
        m.objective,
        m.variety,
        m.goof,
        m.goof_waves,
        m.stuck,
        m.team_kills,
        m.frame_us,
        m.bands,
        m.broken.as_deref().unwrap_or("").replace('"', "'"),
        m.flagged.join("; ")
    )
}

/// Means over a setting's runs, against the base setting's same runs.
#[derive(Clone, Debug, Default)]
pub struct Summary {
    pub objective_change: f64,
    pub variety: f64,
    pub goof: f64,
    pub goof_waves: f64,
    pub stuck: f64,
    pub team_kills: f64,
    pub frame_us: f64,
    pub bands: f64,
    pub broken: f64,
}
fn summarize(
    runs: &[(usize, &'static str, u64, Metrics)],
    setting: usize,
    base: usize,
    seeds: u64,
) -> Summary {
    let mine: Vec<_> = runs.iter().filter(|r| r.0 == setting).collect();
    let n = mine.len().max(1) as f64;
    let mut s = Summary::default();
    for (_, name, seed, m) in &mine {
        let b = runs
            .iter()
            .find(|r| r.0 == base && r.1 == *name && r.2 == *seed)
            .map(|r| &r.3);
        let b_obj = b.map_or(m.objective, |b| b.objective);
        s.objective_change += (m.objective - b_obj) / b_obj.max(1.0);
        s.variety += m.variety;
        s.goof += m.goof;
        s.goof_waves += m.goof_waves;
        s.stuck += m.stuck;
        s.team_kills += m.team_kills;
        s.frame_us += m.frame_us;
        s.bands += m.bands as f64;
        s.broken += m.broken.is_some() as u8 as f64;
    }
    for v in [
        &mut s.objective_change,
        &mut s.variety,
        &mut s.goof,
        &mut s.goof_waves,
        &mut s.stuck,
        &mut s.team_kills,
        &mut s.frame_us,
    ] {
        *v /= n;
    }
    // Bands and broken scenarios: per seed, summed over scenarios.
    s.bands /= seeds as f64;
    s.broken /= seeds as f64;
    s
}

/// The base setting's share tables, one per scenario, with its flags.
fn share_tables(runs: &[(usize, &'static str, u64, Metrics)], base: usize) -> String {
    let mut out = String::new();
    for (_, name, _, m) in runs.iter().filter(|r| r.0 == base && r.2 == 0) {
        let _ = writeln!(
            out,
            "{name}: variety {:.2} bits, goof waves {:.3}{}",
            m.variety,
            m.goof_waves,
            m.broken
                .as_ref()
                .map_or(String::new(), |b| format!(", BROKEN: {b}"))
        );
        for row in &m.rows {
            if row.share == 0.0 && !row.flag.bad() {
                continue;
            }
            let band = row.band.as_ref().map_or(String::new(), |b| {
                format!("{:.0}-{:.0}%", 100.0 * b.min, 100.0 * b.max)
            });
            let _ = writeln!(
                out,
                "  {:<14} {:>5.1}%  {:<9} {}",
                row.kind,
                100.0 * row.share,
                band,
                row.flag.word()
            );
        }
    }
    out
}

/// The off-switch check: each dial at 0 (or, for a dial already 0, at
/// the config's `on_value`), one at a time, against the shipped values.
pub fn ablation(scenarios: &[Scenario]) -> String {
    let config = Config::shipped();
    let (found, missing) = resolve_dials(&config);
    let kind = shipped_kind();
    let mut settings = vec![Setting {
        label: "base".into(),
        dials: Vec::new(),
    }];
    let mut notes = Vec::new();
    for (path, value) in &found {
        let to = if *value == 0.0 { config.on_value } else { 0.0 };
        let dial = vec![(path.clone(), to)];
        match valid(&kind, &dial) {
            Ok(()) => settings.push(Setting {
                label: format!("{path}={to}"),
                dials: dial,
            }),
            Err(e) => notes.push(format!("{path}: cannot be {to}: {e}")),
        }
    }
    let scenarios = pick(scenarios);
    let seeds = seeds();
    let runs = run_all(&scenarios, &settings, seeds);
    let mut csv = vec![CSV_HEAD.to_string()];
    for (si, name, seed, m) in &runs {
        csv.push(csv_row(&settings[*si], name, *seed, m));
    }
    let base = summarize(&runs, 0, 0, seeds);
    let mut text = String::new();
    let _ = writeln!(
        text,
        "Bot off-switch check: {} scenarios x {seeds} seeds, {} dials.\n",
        scenarios.len(),
        settings.len() - 1
    );
    let _ = writeln!(
        text,
        "{:<26} {:>12} {:>8} {:>8} {:>8} {:>8} {:>8} {:>6} {:>8} {:>6} {:>6}  verdict",
        "dial",
        "base -> test",
        "obj",
        "variety",
        "goof",
        "waves",
        "stuck",
        "tk",
        "frame",
        "bands",
        "broken"
    );
    let _ = writeln!(
        text,
        "{:<26} {:>12} {:>8} {:>8.3} {:>8.4} {:>8.4} {:>8.4} {:>6.1} {:>7.1}us {:>6.1} {:>6.1}",
        "(base)",
        "",
        "",
        base.variety,
        base.goof,
        base.goof_waves,
        base.stuck,
        base.team_kills,
        base.frame_us,
        base.bands,
        base.broken
    );
    let u = &config.unchanged;
    for (si, setting) in settings.iter().enumerate().skip(1) {
        let s = summarize(&runs, si, 0, seeds);
        let (path, to) = &setting.dials[0];
        let was = found
            .iter()
            .find(|(p, _)| p == path)
            .map_or(0.0, |(_, v)| *v);
        let moved = s.objective_change.abs() >= u.objective_share
            || (s.variety - base.variety).abs() >= u.variety
            || (s.goof - base.goof).abs() >= u.goof
            || (s.goof_waves - base.goof_waves).abs() >= u.goof_waves
            || (s.stuck - base.stuck).abs() >= u.stuck
            || (s.team_kills - base.team_kills).abs() >= u.team_kills
            || s.broken != base.broken
            || s.bands != base.bands;
        let verdict = match (moved, was == 0.0) {
            (false, true) => "OFF AT BASE; turning it on changes nothing: CUT CANDIDATE",
            (false, false) => "no measurable change: CUT CANDIDATE",
            (true, true) => "OFF AT BASE (dormant dial); turning it on moves play",
            (true, false) => "moves play",
        };
        let _ = writeln!(
            text,
            "{:<26} {:>12} {:>+7.1}% {:>+8.3} {:>+8.4} {:>+8.4} {:>+8.4} {:>+6.1} {:>+7.1}% {:>+6.1} {:>+6.1}  {verdict}",
            path,
            format!("{} -> {}", short(was), short(*to)),
            100.0 * s.objective_change,
            s.variety - base.variety,
            s.goof - base.goof,
            s.goof_waves - base.goof_waves,
            s.stuck - base.stuck,
            s.team_kills - base.team_kills,
            100.0 * (s.frame_us / base.frame_us.max(1e-9) - 1.0),
            s.bands - base.bands,
            s.broken - base.broken,
        );
        for (_, name, _, m) in runs.iter().filter(|r| r.0 == si) {
            if let Some(b) = &m.broken {
                let _ = writeln!(text, "    broke {name}: {b}");
            }
        }
    }
    for n in &notes {
        let _ = writeln!(text, "  {n}");
    }
    for m in &missing {
        let _ = writeln!(text, "  not in bots.json, skipped: {m}");
    }
    let _ = writeln!(text, "\nBase shares (seed 0):\n{}", share_tables(&runs, 0));
    let dir = out_dir();
    std::fs::write(dir.join("ablation.csv"), csv.join("\n") + "\n").unwrap();
    std::fs::write(dir.join("ablation.txt"), &text).unwrap();
    let _ = writeln!(text, "Wrote {}", dir.join("ablation.{csv,txt}").display());
    text
}

/// The grid: `BRI_TUNING_POINTS` (`a.b=0,0.5;c.d=1,2`), else the config's
/// grid for a dial, else 0, half, the shipped value and double (0, 0.5
/// and 1 for a dial shipped at 0).
pub fn grid(found: &[(String, f64)], config: &Config) -> Vec<(String, Vec<f64>)> {
    if let Ok(points) = std::env::var("BRI_TUNING_POINTS")
        && !points.trim().is_empty()
    {
        return points
            .split(';')
            .filter_map(|part| {
                let (path, values) = part.split_once('=')?;
                Some((
                    path.trim().to_string(),
                    values
                        .split(',')
                        .filter_map(|v| v.trim().parse().ok())
                        .collect(),
                ))
            })
            .collect();
    }
    found
        .iter()
        .map(|(path, base)| {
            let values = config.grid.get(path).cloned().unwrap_or_else(|| {
                if *base == 0.0 {
                    vec![0.0, 0.5, 1.0]
                } else {
                    vec![0.0, base / 2.0, *base, base * 2.0]
                }
            });
            (path.clone(), values)
        })
        .collect()
}

/// The sweep: every point of the grid (one dial at a time from the
/// shipped values, or every combination with `BRI_TUNING_GRID=full`),
/// `BRI_TUNING_SEEDS` seeds each, scored and ranked.
pub fn sweep(scenarios: &[Scenario]) -> String {
    let config = Config::shipped();
    let (found, missing) = resolve_dials(&config);
    let kind = shipped_kind();
    let grid = grid(&found, &config);
    let full = std::env::var("BRI_TUNING_GRID").is_ok_and(|v| v == "full");
    let mut points: Vec<Vec<(String, f64)>> = vec![Vec::new()];
    if full {
        for (path, values) in &grid {
            points = points
                .into_iter()
                .flat_map(|p| {
                    values.iter().map(move |v| {
                        let mut p = p.clone();
                        p.push((path.clone(), *v));
                        p
                    })
                })
                .collect();
        }
    } else {
        for (path, values) in &grid {
            let base = dials::dial(&kind, path).unwrap_or(f64::NAN);
            for v in values {
                if (*v - base).abs() > 1e-6 {
                    points.push(vec![(path.clone(), *v)]);
                }
            }
        }
    }
    let mut notes = Vec::new();
    let mut settings = Vec::new();
    for p in points {
        match valid(&kind, &p) {
            Ok(()) => settings.push(Setting {
                label: if p.is_empty() {
                    "base".into()
                } else {
                    p.iter()
                        .map(|(d, v)| format!("{d}={v}"))
                        .collect::<Vec<_>>()
                        .join("&")
                },
                dials: p,
            }),
            Err(e) => notes.push(format!("left out: {e}")),
        }
    }
    let scenarios = pick(scenarios);
    let seeds = seeds();
    let runs = run_all(&scenarios, &settings, seeds);
    let mut csv = vec![CSV_HEAD.to_string()];
    for (si, name, seed, m) in &runs {
        csv.push(csv_row(&settings[*si], name, *seed, m));
    }
    let base_index = settings.iter().position(|s| s.dials.is_empty());
    let base_frame = base_index.map_or(1.0, |b| summarize(&runs, b, b, seeds).frame_us);
    let w = &config.weights;
    let mut ranked: Vec<(f64, usize, Summary)> = settings
        .iter()
        .enumerate()
        .map(|(si, _)| {
            let s = summarize(&runs, si, base_index.unwrap_or(si), seeds);
            let score = -w.band * s.bands - w.broken * s.broken
                + w.variety * s.variety
                + w.goof_waves * s.goof_waves
                - w.stuck * s.stuck
                - w.frame * s.frame_us / base_frame.max(1e-9);
            (score, si, s)
        })
        .collect();
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut text = format!(
        "Bot sweep: {} points x {} scenarios x {seeds} seeds ({}).\n\n",
        settings.len(),
        scenarios.len(),
        if full {
            "every combination"
        } else {
            "one dial at a time"
        }
    );
    let _ = writeln!(
        text,
        "{:>4} {:>8}  {:<40} {:>6} {:>6} {:>8} {:>7} {:>7} {:>7} {:>8}",
        "rank", "score", "point", "bands", "broken", "variety", "waves", "goof", "stuck", "frame"
    );
    for (rank, (score, si, s)) in ranked.iter().enumerate() {
        let _ = writeln!(
            text,
            "{:>4} {:>8.3}  {:<40} {:>6.1} {:>6.1} {:>8.3} {:>7.4} {:>7.4} {:>7.4} {:>6.1}us",
            rank + 1,
            score,
            settings[*si].label,
            s.bands,
            s.broken,
            s.variety,
            s.goof_waves,
            s.goof,
            s.stuck,
            s.frame_us
        );
    }
    for n in &notes {
        let _ = writeln!(text, "  {n}");
    }
    for m in &missing {
        let _ = writeln!(text, "  not in bots.json, skipped: {m}");
    }
    let dir = out_dir();
    std::fs::write(dir.join("sweep.csv"), csv.join("\n") + "\n").unwrap();
    std::fs::write(dir.join("sweep_summary.txt"), &text).unwrap();
    let _ = writeln!(
        text,
        "Wrote {}",
        dir.join("sweep{.csv,_summary.txt}").display()
    );
    text
}

fn short(v: f64) -> String {
    format!("{}", (v * 1e4).round() / 1e4)
}
