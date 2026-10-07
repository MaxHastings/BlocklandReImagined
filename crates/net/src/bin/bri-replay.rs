//! Replays a match recording headlessly and says whether it plays out as
//! recorded, and if not, where it first parted and what differed.
//!
//! Usage: bri-replay <content-root> <recording.brimatch>
//! Exits 0 when the replay matches, 1 when it parts from the recording,
//! 2 when it cannot replay at all.
use anyhow::{Context, Result};
use bri_net::replay;
use bri_sim::replay::{Divergence, Report};
use std::{path::PathBuf, process::ExitCode};

#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

const TICKS_PER_SECOND: u64 = bri_world::TICKS_PER_SECOND;

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(error) => {
            eprintln!("Could not replay: {error:#}");
            ExitCode::from(2)
        }
    }
}

/// Whether the replay matched its recording.
fn run() -> Result<bool> {
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
    Ok(report.divergence.is_none())
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
    if let Some(why) = &report.cut_off {
        println!("The recording ends early ({why}): the host stopped while writing it.");
    }
    match &report.divergence {
        None => println!("It plays out exactly as recorded."),
        Some(divergence) => print_divergence(divergence, started),
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
