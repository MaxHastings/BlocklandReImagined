//! Stamp the build's identity into the game: the version players see on the
//! main menu and in logs, and when the build's source was committed (the
//! update check compares it with release dates).
//!
//! `BRI_VERSION` names a release (the dist folder's version, such as
//! `2026-09-28-a13`); without it the build is a development build named by its
//! commit date. The short commit hash is always added.
use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

fn main() {
    println!("cargo:rerun-if-env-changed=BRI_VERSION");
    // Rebuild the stamp when the checked-out commit changes: HEAD itself,
    // the branch it names, and packed refs.
    for path in [
        git(&["rev-parse", "--git-path", "HEAD"]),
        git(&["symbolic-ref", "-q", "HEAD"]).and_then(|r| git(&["rev-parse", "--git-path", &r])),
        git(&["rev-parse", "--git-path", "packed-refs"]),
    ]
    .into_iter()
    .flatten()
    {
        println!("cargo:rerun-if-changed={path}");
    }
    let hash = git(&["rev-parse", "--short=9", "HEAD"]).unwrap_or_else(|| "unknown".into());
    let committed: u64 = git(&["log", "-1", "--format=%ct"])
        .and_then(|t| t.parse().ok())
        .unwrap_or(0);
    let release = std::env::var("BRI_VERSION")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let name = match &release {
        Some(version) => version.clone(),
        None => git(&["log", "-1", "--format=%cs"])
            .map(|date| format!("dev-{date}"))
            .unwrap_or_else(|| "dev".into()),
    };
    println!("cargo:rustc-env=BRI_BUILD_NAME={name}");
    println!("cargo:rustc-env=BRI_BUILD_HASH={hash}");
    println!("cargo:rustc-env=BRI_BUILD_COMMITTED={committed}");
    println!(
        "cargo:rustc-env=BRI_BUILD_RELEASE={}",
        if release.is_some() { "1" } else { "" }
    );
}
