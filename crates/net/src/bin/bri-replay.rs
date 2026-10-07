//! Replays a match recording headlessly and says whether it plays out as
//! recorded, and if not, where it first parted and what differed.
//!
//! Usage: bri-replay <content-root> <recording.brimatch>
//! Exits 0 when the replay matches the whole recording, 1 when it parts
//! from the recording, 2 when it cannot replay at all (a damaged file, other
//! content), and 3 when it matches as far as a file that was cut short (the
//! host stopped while writing it) goes.
use anyhow::{Context, Result};
use bri_net::replay;
use bri_sim::replay::{Divergence, Report};
use std::{path::PathBuf, process::ExitCode};

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
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    anyhow::ensure!(
        args.len() == 2,
        "Usage: bri-replay <content-root> <recording.brimatch>
         Replays a match a host recorded and checks it plays out the same, tick for tick.
         <content-root> must hold the same packages the host ran (its own content folder)."
    );
    let content_root = PathBuf::from(&args[0]);
    let path = PathBuf::from(&args[1]);
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
    let report = bri_sim::replay::replay(&mut session, &mut opened.frames, &mut |map, save| {
        replay::load_map(&setup, map, save)
    })?;
    print_report(&report, started);
    Ok(match (&report.divergence, &report.cut_off) {
        (Some(_), _) => Ending::Diverged,
        (None, Some(_)) => Ending::MatchedUntilCut,
        (None, None) => Ending::Matched,
    })
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
