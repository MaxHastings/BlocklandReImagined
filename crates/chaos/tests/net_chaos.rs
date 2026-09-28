//! The same bots over real QUIC connections to a real host on loopback:
//! administrators and players joining, leaving, building, loading, firing
//! and driving at once. No client may be dropped unasked, nothing a client
//! receives may hold NaN, no host system may fail its step, and the host
//! must still answer a newcomer at the end.
//!
//! `BRI_CHAOS_SECONDS` (default 8) lengthens the run, `BRI_CHAOS_SEEDS`
//! (default 1) adds seeds and `BRI_CHAOS_SEED` replays one.
use anyhow::{Result, ensure};
use bri_chaos::{
    env, fixture,
    net::{NetChaos, Options},
};

fn seeds() -> Vec<u64> {
    match std::env::var("BRI_CHAOS_SEED")
        .ok()
        .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())
    {
        Some(seed) => vec![seed],
        None => (1..=env("BRI_CHAOS_SEEDS", 1))
            .map(|i| i * 0x51_7cc1_b727)
            .collect(),
    }
}

fn options(seed: u64) -> Options {
    Options {
        seed,
        seconds: env("BRI_CHAOS_SECONDS", 8),
        ..Default::default()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn synthetic_net_chaos_keeps_every_client_connected_and_finite() -> Result<()> {
    for seed in seeds() {
        let report = NetChaos::start(fixture::synthetic()?, options(seed))
            .await?
            .run()
            .await?;
        eprintln!("seed {seed:#x}: {report:?}");
        ensure!(
            report.server_step_errors == 0,
            "seed {seed:#x}: the host's step failed {} times",
            report.server_step_errors
        );
        ensure!(report.most_bricks > 0, "nothing was built: {report:?}");
        // Shots are short-lived between updates; their cues always arrive.
        ensure!(
            report.most_projectiles > 0 || report.cues.contains_key("WeaponShell"),
            "nothing was ever fired: {report:?}"
        );
        ensure!(report.checks > 0, "the replicas were never checked");
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs generated content; set BRI_CONTENT"]
async fn content_net_chaos_keeps_every_client_connected_and_finite() -> Result<()> {
    let root = fixture::content_root()
        .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"));
    let map = std::env::var("BRI_CHAOS_MAP")
        .unwrap_or_else(|_| "v20/add-ons/map_slate/slate.mis".to_string());
    for seed in seeds() {
        let report = NetChaos::start(fixture::content(&root, &map)?, options(seed))
            .await?
            .run()
            .await?;
        eprintln!("{map} seed {seed:#x}: {report:?}");
        ensure!(
            report.server_step_errors == 0,
            "{map} seed {seed:#x}: the host's step failed {} times",
            report.server_step_errors
        );
    }
    Ok(())
}
