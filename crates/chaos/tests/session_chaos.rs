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

/// The content soak is split into one test per map and seed shard, so the
/// test harness runs them in parallel instead of one long serial run (about
/// 36 s per map and seed in a debug build). Together the shards cover every
/// map and seed exactly once.
const SHARDS: usize = 4;

fn maps() -> Vec<String> {
    std::env::var("BRI_CHAOS_MAP").map_or_else(
        |_| {
            vec![
                "v20/add-ons/map_slate/slate.mis".to_string(),
                "v20/add-ons/map_bedroom/bedroom.mis".to_string(),
            ]
        },
        |m| vec![m],
    )
}

fn content_chaos(map_slot: usize, shard: usize) -> Result<()> {
    let Some(map) = maps().into_iter().nth(map_slot) else {
        return Ok(());
    };
    let root = fixture::content_root()
        .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"));
    for seed in seeds().into_iter().skip(shard).step_by(SHARDS) {
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
    Ok(())
}

macro_rules! content_chaos_shards {
    ($($name:ident: $map_slot:literal, $shard:literal;)*) => {$(
        #[test]
        #[ignore = "needs generated content; set BRI_CONTENT"]
        fn $name() -> Result<()> {
            content_chaos($map_slot, $shard)
        }
    )*};
}

content_chaos_shards! {
    content_chaos_never_panics_fails_or_replicates_nan_map0_shard0: 0, 0;
    content_chaos_never_panics_fails_or_replicates_nan_map0_shard1: 0, 1;
    content_chaos_never_panics_fails_or_replicates_nan_map0_shard2: 0, 2;
    content_chaos_never_panics_fails_or_replicates_nan_map0_shard3: 0, 3;
    content_chaos_never_panics_fails_or_replicates_nan_map1_shard0: 1, 0;
    content_chaos_never_panics_fails_or_replicates_nan_map1_shard1: 1, 1;
    content_chaos_never_panics_fails_or_replicates_nan_map1_shard2: 1, 2;
    content_chaos_never_panics_fails_or_replicates_nan_map1_shard3: 1, 3;
}

#[test]
fn content_chaos_shards_cover_every_seed_once() {
    assert!(maps().len() <= 2, "add shard tests for the new map slot");
    let mut covered: Vec<u64> = (0..SHARDS)
        .flat_map(|shard| seeds().into_iter().skip(shard).step_by(SHARDS))
        .collect();
    covered.sort_unstable();
    let mut all = seeds();
    all.sort_unstable();
    assert_eq!(covered, all);
}
