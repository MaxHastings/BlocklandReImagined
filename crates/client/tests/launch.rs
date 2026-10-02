//! Launching the game binary the way players do. Never opens a window: every
//! case here fails or finishes before the platform starts.
use std::process::Command;

fn client() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bri-client"));
    // Report to stderr instead of a modal dialog nobody can click in a test.
    command.env("BRI_NO_DIALOGS", "1");
    command
}

#[test]
fn help_describes_double_click_launch() {
    let out = client().arg("--help").output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("With no arguments the game opens"), "{text}");
}

#[test]
fn a_startup_failure_tells_the_player_and_points_at_the_logs() {
    let state = tempfile::tempdir().unwrap();
    let missing = state.path().join("no-content-here");
    // No mode flag: exactly what double-clicking with bad content does.
    let out = client().arg(&missing).arg(state.path()).output().unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Blockland ReImagined could not continue"),
        "the player-facing dialog text: {stderr}"
    );
    assert!(stderr.contains("Loading the game"), "{stderr}");
}

/// Windows gives the main thread 1 MB of stack, and the game starts on it.
/// Run the same start-up failure inside a 1 MB stack here too, so a frame
/// that grows past it (an App moved by value, say) fails on every platform
/// instead of overflowing only on players' machines.
#[cfg(unix)]
#[test]
fn a_startup_failure_fits_in_the_windows_main_thread_stack() {
    let state = tempfile::tempdir().unwrap();
    let missing = state.path().join("no-content-here");
    let out = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(r#"ulimit -s 1024 && exec "$0" "$1" "$2""#)
        .arg(env!("CARGO_BIN_EXE_bri-client"))
        .arg(&missing)
        .arg(state.path())
        .env("BRI_NO_DIALOGS", "1")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Blockland ReImagined could not continue"),
        "start-up overflowed a 1 MB stack: {stderr}"
    );
}
