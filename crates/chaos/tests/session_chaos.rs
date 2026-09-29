//! Bots doing everything at once inside one session: planting inside each
//! other and themselves, firing every weapon (including from inside bricks
//! and straight down), spawning, driving and wrecking vehicles, loading
//! overlapping builds, joining, leaving and chatting. Nothing may panic, no
//! system may fail its step, and nothing the host replicates may hold NaN.
//!
//! `BRI_CHAOS_SEEDS` (default 4) and `BRI_CHAOS_TICKS` (default 1200, ten
//! seconds of play) scale the run; `BRI_CHAOS_SEED` replays one seed.
use anyhow::Result;
use bri_chaos::{
    env, fixture,
    local::{Chaos, Options},
};

fn seeds() -> Vec<u64> {
    match std::env::var("BRI_CHAOS_SEED")
        .ok()
        .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())
    {
        Some(seed) => vec![seed],
        None => (1..=env("BRI_CHAOS_SEEDS", 4))
            .map(|i| i * 0x9e37_79b9)
            .collect(),
    }
}

#[test]
fn synthetic_chaos_never_panics_fails_or_replicates_nan() -> Result<()> {
    for seed in seeds() {
        let options = Options {
            seed,
            ticks: env("BRI_CHAOS_TICKS", 1200),
            ..Default::default()
        };
        let report = Chaos::new(fixture::synthetic()?, options)?.run()?;
        eprintln!("seed {seed:#x}: {report:?}");
        anyhow::ensure!(
            report.step_errors.is_empty(),
            "seed {seed:#x}: the host's step failed: {:#?}",
            report.step_errors
        );
        // The run must actually exercise what it claims to.
        anyhow::ensure!(
            report.planted > 0 && report.loads > 0,
            "nothing was built: {report:?}"
        );
        anyhow::ensure!(
            report.most_vehicles > 0,
            "no vehicle ever spawned: {report:?}"
        );
        anyhow::ensure!(
            report.most_projectiles > 0,
            "nothing was ever fired: {report:?}"
        );
    }
    Ok(())
}

#[test]
#[ignore = "needs generated content; set BRI_CONTENT"]
fn content_chaos_never_panics_fails_or_replicates_nan() -> Result<()> {
    let root = fixture::content_root()
        .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"));
    let maps = std::env::var("BRI_CHAOS_MAP").map_or_else(
        |_| {
            vec![
                "v20/add-ons/map_slate/slate.mis".to_string(),
                "v20/add-ons/map_bedroom/bedroom.mis".to_string(),
            ]
        },
        |m| vec![m],
    );
    for map in maps {
        for seed in seeds() {
            let options = Options {
                seed,
                ticks: env("BRI_CHAOS_TICKS", 1200),
                ..Default::default()
            };
            let report = Chaos::new(fixture::content(&root, &map)?, options)?.run()?;
            eprintln!("{map} seed {seed:#x}: {report:?}");
            anyhow::ensure!(
                report.step_errors.is_empty(),
                "{map} seed {seed:#x}: the host's step failed: {:#?}",
                report.step_errors
            );
        }
    }
    Ok(())
}
