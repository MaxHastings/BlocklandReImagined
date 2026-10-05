//! `bri-server` asked to stop the way a service manager, `kill` or a
//! closing terminal asks (SIGTERM, SIGHUP) saves its world, with its
//! mini-game, as it does for Ctrl+C, and leaves no recovery snapshot.
#![cfg(unix)]
use anyhow::{Context, Result, ensure};
use std::{
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

/// A build to start from: one brick and a two-team mini-game run by a
/// player who has not joined yet (as a restarted server holds it).
fn start_build(path: &Path) -> Result<()> {
    let mut world = bri_world::World::new(
        "Soccer".into(),
        bri_net::testing::MAP.into(),
        vec![[1.0; 4]],
    );
    world.bricks.insert(
        1,
        bri_world::Brick::new(
            bri_world::ContentRef::Resolved(bri_net::testing::MENU_BRICKS[0].0.into()),
            [0.25, 5.1, 0.25],
            0,
        ),
    );
    world.next_brick_id = 2;
    let mut build = bri_world::build::SavedBuild::new(world);
    build.minigame = Some(serde_json::json!({
        "color": 2,
        "teams": [{"name": "Red", "color": 0}, {"name": "Blue", "color": 1}],
        "owner": "ab".repeat(32),
    }));
    std::fs::write(path, bri_world::build::encode(&build)?)?;
    Ok(())
}

fn wait(child: &mut Child, until: Instant) -> Result<std::process::ExitStatus> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() > until {
            let _ = child.kill();
            anyhow::bail!("bri-server did not stop");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn stops_and_saves(signal: &str) -> Result<()> {
    let root = bri_net::testing::ScratchRoot::new()?;
    let start = root.path().join("start.json");
    start_build(&start)?;
    let state = root.path().join("state");
    let mut child = Command::new(env!("CARGO_BIN_EXE_bri-server"))
        .arg(root.path())
        .arg(&start)
        .arg(&state)
        .arg("127.0.0.1:0")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let until = Instant::now() + Duration::from_secs(60);
    while !state.join("host.json").is_file() {
        if let Some(status) = child.try_wait()? {
            anyhow::bail!("bri-server exited while starting: {status}");
        }
        ensure!(Instant::now() < until, "bri-server did not start");
        std::thread::sleep(Duration::from_millis(50));
    }
    let sent = Command::new("kill")
        .arg(format!("-{signal}"))
        .arg(child.id().to_string())
        .status()?;
    ensure!(sent.success(), "kill -{signal} failed");
    let status = wait(&mut child, Instant::now() + Duration::from_secs(60))?;
    ensure!(
        status.success(),
        "bri-server ended with {status} on {signal}"
    );
    let saved = std::fs::read_dir(&state)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("world-"))
        })
        .context("no world saved")?;
    let build = bri_world::persistence::load_startup(&saved)?;
    assert_eq!(build.world.bricks.len(), 1, "the brick was not saved");
    let minigame = build.minigame.context("the mini-game was not saved")?;
    let teams: Vec<_> = minigame["teams"]
        .as_array()
        .context("no teams")?
        .iter()
        .map(|t| t["name"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(teams, ["Red", "Blue"]);
    assert_eq!(minigame["owner"], "ab".repeat(32));
    assert!(
        !bri_net::dedicated::recovery_path(&state).exists(),
        "a clean stop left its recovery snapshot"
    );
    Ok(())
}

#[test]
fn sigterm_saves_the_world_and_its_minigame() -> Result<()> {
    stops_and_saves("TERM")
}

#[test]
fn sighup_saves_the_world_and_its_minigame() -> Result<()> {
    stops_and_saves("HUP")
}

/// A run that ended without stopping (killed outright, a crash, a power
/// cut) leaves its recovery snapshot; the next start makes it that run's
/// world, the one `resume` continues from.
#[test]
fn a_left_recovery_snapshot_becomes_the_world_resume_continues_from() -> Result<()> {
    let state = tempfile::tempdir()?;
    let slot = bri_net::dedicated::recovery_path(state.path());
    start_build(&slot)?;
    std::fs::write(state.path().join("world-1.json"), b"an older shutdown save")?;
    let adopted = bri_net::dedicated::adopt_recovery(state.path())?.context("not adopted")?;
    assert!(!slot.exists());
    assert_eq!(
        bri_world::persistence::newest_world(state.path())?.as_deref(),
        Some(adopted.as_path())
    );
    let build = bri_world::persistence::load_startup(&adopted)?;
    assert_eq!(build.world.bricks.len(), 1);
    assert!(build.minigame.is_some());
    assert!(bri_net::dedicated::adopt_recovery(state.path())?.is_none());
    Ok(())
}
