//! A writer killed mid-snapshot leaves its staging file beside the slot. A
//! host that starts keeping the slot clears the old ones, and only those.
mod common;

use anyhow::{Result, ensure};
use bri_net::{recovery, server};
use std::time::{Duration, Instant, SystemTime};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_host_clears_staging_files_a_killed_writer_left_behind() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let slot = dir.path().join("recovery.json");
    let old = SystemTime::now() - Duration::from_secs(3600);
    let stale = dir.path().join(".recovery.json.31337.0.tmp");
    let fresh = dir.path().join(".recovery.json.31337.1.tmp");
    let other = dir.path().join(".settings.json.31337.0.tmp");
    for file in [&stale, &fresh, &other] {
        std::fs::write(file, b"half a snapshot")?;
    }
    for file in [&stale, &other] {
        std::fs::File::options()
            .write(true)
            .open(file)?
            .set_modified(old)?;
    }
    let host = server::start(common::session(), common::options())?;
    host.keep_recovery(recovery::Recovery {
        path: slot,
        every: Duration::from_secs(60),
    })?;
    let until = Instant::now() + Duration::from_secs(30);
    while stale.exists() {
        ensure!(
            Instant::now() < until,
            "the old staging file is still there"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    ensure!(fresh.exists(), "a staging file in use was removed");
    ensure!(other.exists(), "another file's staging file was removed");
    host.stop().await?;
    Ok(())
}
