//! `bri-addon-run` tries an Add-On headless: the HUD sample runs with the
//! rules it needs, a player command is refused, an admin command answers.
use std::{path::PathBuf, process::Command};

#[test]
fn bri_addon_run_plays_the_samples_with_their_commands() {
    let samples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/samples");
    let out = Command::new(env!("CARGO_BIN_EXE_bri-addon-run"))
        .arg(samples.join("sample-points-hud"))
        .args([
            "--seconds",
            "6",
            "--send",
            "Guest: sample-survival-points:reset",
        ])
        .args(["--wait", "5", "--send", "sample-survival-points:top"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains("[to Guest] Survival Points: stay alive"),
        "{text}"
    );
    assert!(text.contains("refused: error[command.admin]"), "{text}");
    assert!(
        text.contains(" 7.00s  [everyone] Survival Points leader: "),
        "{text}"
    );
    assert!(
        text.contains("sample-survival-points Guest points = 1"),
        "{text}"
    );
    assert!(text.ends_with("OK: no script problems.\n"), "{text}");

    let out = Command::new(env!("CARGO_BIN_EXE_bri-addon-run"))
        .arg(samples.join("sample-survival-points"))
        .args(["--send", "top 3"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("`top` takes 0 argument(s), 1 given"));
}
