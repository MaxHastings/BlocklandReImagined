//! Replays a match recording headlessly and says whether it plays out as
//! recorded, and if not, where it first parted and what differed.
//!
//! Usage: bri-replay <content-root> <recording.brimatch> [--bots FROM[-TO]] [--every TICKS]
//! `--bots` also prints what every bot was doing and thinking between two
//! moments of the match (`m:ss` or seconds; TO defaults to the end), every
//! `--every` ticks (12 by default, ten times a second), and every chat line
//! said meanwhile.
//! Exits 0 when the replay matches the whole recording, 1 when it parts
//! from the recording, 2 when it cannot replay at all (a damaged file, other
//! content), and 3 when it matches as far as a file that was cut short (the
//! host stopped while writing it) goes.
use anyhow::{Context, Result};
use bri_net::replay;
use bri_sim::{
    replay::{Divergence, Report},
    session::Session,
};
use std::{ffi::OsString, path::PathBuf, process::ExitCode};

#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

const TICKS_PER_SECOND: u64 = bri_world::TICKS_PER_SECOND;

/// How a replay ended, as the process's exit code.
enum Ending {
    Matched,
    Diverged,
    MatchedUntilCut,
}
const DIVERGED: u8 = 1;
const FAILED: u8 = 2;
const MATCHED_UNTIL_CUT: u8 = 3;

fn main() -> ExitCode {
    match run() {
        Ok(Ending::Matched) => ExitCode::SUCCESS,
        Ok(Ending::Diverged) => ExitCode::from(DIVERGED),
        Ok(Ending::MatchedUntilCut) => ExitCode::from(MATCHED_UNTIL_CUT),
        Err(error) => {
            eprintln!("Could not replay: {error:#}");
            ExitCode::from(FAILED)
        }
    }
}

fn run() -> Result<Ending> {
    let args = Args::parse(std::env::args_os().skip(1).collect())?;
    let content_root = args.content_root;
    let path = args.recording;
    let mut opened = replay::open(&path)?;
    let header = &opened.header;
    println!(
        "{}: {} on {}{}, recorded by {}.",
        path.display(),
        header.host.world.name,
        header.host.map,
        header
            .host
            .mode
            .as_deref()
            .map_or(String::new(), |m| format!(" ({m})")),
        header
            .host
            .game_version
            .as_deref()
            .unwrap_or("an unnamed build")
    );
    let replay::Rebuilt { mut session, setup } =
        replay::rebuild(&content_root, header).context("Setting the match up again")?;
    let started = session.simulation().state().tick;
    let mut watcher = args
        .bots
        .map(|window| BotWatcher::new(window, args.every, started));
    let report = bri_sim::replay::replay_watching(
        &mut session,
        &mut opened.frames,
        &mut |map, save| replay::load_map(&setup, map, save),
        &mut |s| {
            if let Some(watcher) = watcher.as_mut() {
                watcher.watch(s);
            }
        },
    )?;
    if watcher.is_some_and(|w| w.diverged_before_end(&report)) {
        println!(
            "(The replay parted from the recording, so bot lines after that point are not what happened.)"
        );
    }
    print_report(&report, started);
    Ok(match (&report.divergence, &report.cut_off) {
        (Some(_), _) => Ending::Diverged,
        (None, Some(_)) => Ending::MatchedUntilCut,
        (None, None) => Ending::Matched,
    })
}

const USAGE: &str =
    "Usage: bri-replay <content-root> <recording.brimatch> [--bots FROM[-TO]] [--every TICKS]
Replays a match a host recorded and checks it plays out the same, tick for tick.
<content-root> must hold the same packages the host ran (its own content folder).
--bots FROM[-TO]  also print every bot's state and thinking, and the chat, between
                  two moments of the match (m:ss or seconds; TO defaults to the end).
--every TICKS     how often to print bots within that window (default 12: ten a second).";

struct Args {
    content_root: PathBuf,
    recording: PathBuf,
    /// The moments to print bots between, in seconds into the match.
    bots: Option<(f64, Option<f64>)>,
    every: u64,
}
impl Args {
    fn parse(args: Vec<OsString>) -> Result<Self> {
        let mut positional = Vec::new();
        let mut bots = None;
        let mut every = 12;
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--bots") => {
                    let window = args.next().context(USAGE)?;
                    let window = window.to_str().context(USAGE)?;
                    bots = Some(match window.split_once('-') {
                        Some((from, to)) => (moment(from)?, Some(moment(to)?)),
                        None => (moment(window)?, None),
                    });
                }
                Some("--every") => {
                    let ticks = args.next().context(USAGE)?;
                    every = ticks
                        .to_str()
                        .and_then(|t| t.parse().ok())
                        .filter(|t| *t > 0)
                        .context("--every takes a whole number of ticks above 0")?;
                }
                _ => positional.push(PathBuf::from(arg)),
            }
        }
        let [content_root, recording] =
            <[PathBuf; 2]>::try_from(positional).map_err(|_| anyhow::anyhow!(USAGE))?;
        Ok(Self {
            content_root,
            recording,
            bots,
            every,
        })
    }
}

/// `m:ss`, `m:ss.mmm` or plain seconds, as seconds.
fn moment(text: &str) -> Result<f64> {
    let seconds = match text.split_once(':') {
        Some((minutes, seconds)) => minutes
            .trim()
            .parse::<f64>()
            .ok()
            .map(|m| m * 60.0)
            .zip(seconds.trim().parse::<f64>().ok())
            .map(|(m, s)| m + s),
        None => text.trim().parse().ok(),
    };
    seconds
        .filter(|s: &f64| s.is_finite() && *s >= 0.0)
        .with_context(|| format!("{text:?} is not a moment of the match (m:ss or seconds)"))
}

/// Prints the bots and the chat within a window of the replay.
struct BotWatcher {
    from: u64,
    to: Option<u64>,
    every: u64,
    started: u64,
    /// The last chat line printed (or skipped as before the window).
    chat_seen: u64,
}
impl BotWatcher {
    fn new((from, to): (f64, Option<f64>), every: u64, started: u64) -> Self {
        let tick = |seconds: f64| started + (seconds * TICKS_PER_SECOND as f64) as u64;
        Self {
            from: tick(from),
            to: to.map(tick),
            every,
            started,
            chat_seen: 0,
        }
    }
    fn watch(&mut self, s: &Session) {
        let tick = s.simulation().state().tick;
        let chat = s.chat_after(self.chat_seen);
        if let Some(last) = chat.last() {
            self.chat_seen = last.id;
        }
        if tick < self.from || self.to.is_some_and(|to| tick > to) {
            return;
        }
        for line in chat {
            println!(
                "{} chat {} ({}): {}",
                clock(line.tick, self.started),
                line.name,
                line.owner,
                line.text
            );
        }
        if !(tick - self.from).is_multiple_of(self.every) {
            return;
        }
        let names = s.names();
        let vitals = s.vitals();
        let bodies: std::collections::BTreeMap<_, _> = s
            .motion_states()
            .into_iter()
            .map(|(state, _)| (state.owner, state))
            .collect();
        for thought in s.bot_thoughts() {
            let bot = thought.bot;
            let body = bodies.get(&bot).map_or("nowhere".into(), |b| {
                format!(
                    "at ({:.1}, {:.1}, {:.1}) moving ({:.1}, {:.1}, {:.1}) yaw {:.2}",
                    b.feet[0],
                    b.feet[1],
                    b.feet[2],
                    b.velocity[0],
                    b.velocity[1],
                    b.velocity[2],
                    b.yaw
                )
            });
            let health = vitals.get(&bot).map_or("no vitals".into(), |v| {
                format!(
                    "health {:.0}{}",
                    v.health,
                    if v.alive { "" } else { " (dead)" }
                )
            });
            println!(
                "{} tick {tick} bot {bot} {:?} {body} {health}",
                clock(tick, self.started),
                names.get(&bot).map_or("", String::as_str),
            );
            for line in thought.why(tick) {
                println!("    {line}");
            }
            println!("    {thought:?}");
        }
    }
    /// Whether the replay parted from the recording before the window ended.
    fn diverged_before_end(&self, report: &Report) -> bool {
        report
            .divergence
            .as_ref()
            .is_some_and(|d| self.to.is_none_or(|to| d.tick <= to))
    }
}

/// `tick` as minutes and seconds into the match.
fn clock(tick: u64, started: u64) -> String {
    let millis = tick.saturating_sub(started) * 1000 / TICKS_PER_SECOND;
    format!(
        "{}:{:02}.{:03}",
        millis / 60_000,
        millis / 1000 % 60,
        millis % 1000
    )
}

fn print_report(report: &Report, started: u64) {
    println!(
        "Replayed {} ticks ({} of play) and {} calls.",
        report.ticks,
        clock(report.last_tick, started),
        report.calls
    );
    match (&report.divergence, &report.cut_off) {
        (Some(divergence), _) => print_divergence(divergence, started),
        (None, Some(why)) => println!(
            "It matches the recording up to tick {} ({}), where the file ends early ({why}): the host stopped while writing it.",
            report.last_tick,
            clock(report.last_tick, started)
        ),
        (None, None) => println!("It plays out exactly as recorded."),
    }
}

fn print_divergence(divergence: &Divergence, started: u64) {
    println!(
        "It parts from the recording at tick {} ({} into the match): {}",
        divergence.tick,
        clock(divergence.tick, started),
        divergence.what
    );
    let Some(later) = &divergence.later else {
        println!("The recording ends before the next full check.");
        return;
    };
    if later.parts.is_empty() {
        println!(
            "At the next full check (tick {}) the state matched again.",
            later.tick
        );
    } else {
        println!(
            "At the next full check (tick {}) these differed: {}.",
            later.tick,
            later.parts.join(", ")
        );
    }
    for bot in &later.bots {
        println!("Bot {} thought differently:", bot.bot);
        println!("  recorded: {}", lines(&bot.recorded));
        println!("  replayed: {}", lines(&bot.replayed));
    }
}

fn lines(lines: &[String]) -> String {
    if lines.is_empty() {
        "(not there)".into()
    } else {
        lines.join(" | ")
    }
}
