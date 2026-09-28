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
