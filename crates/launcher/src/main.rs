// A desktop app: no console window behind the game.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
//! `BlocklandReImagined.exe`: unpack the game it carries into the per-user
//! data folder (first run, or a new version), then run it there.
//!
//! With no arguments, `--run` or `--check`, the game gets its installed
//! content and the per-user data folder. `--extract-only` installs and prints
//! the game's folder. Anything else goes to the game unchanged.
use anyhow::{Context, Result};
use std::{ffi::OsString, process::Command};

fn main() {
    bri_crash::attach_parent_console();
    match launch() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("{error:#}");
            bri_crash::alert(
                &format!("{} could not start", bri_crash::PRODUCT),
                &format!("{error:#}"),
                None,
            );
            std::process::exit(1);
        }
    }
}

fn launch() -> Result<i32> {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let root = bri_launcher::default_root()?;
    let exe = std::env::current_exe().context("finding this exe")?;
    let payload = bri_launcher::find_payload(&exe)?;
    let game = bri_launcher::install(&payload, &root)?;
    let first = args.first().and_then(|a| a.to_str());
    if first == Some("--extract-only") {
        println!("{}", game.display());
        return Ok(0);
    }
    let mut command = Command::new(game.join(bri_launcher::CLIENT));
    command.current_dir(&game);
    match first {
        None | Some("--run") | Some("--check") if args.len() <= 1 => {
            command
                .arg(first.unwrap_or("--run"))
                .arg(game.join("content"))
                .arg(&root);
        }
        _ => {
            command.args(&args);
        }
    }
    let status = command.status().context("starting the game")?;
    Ok(status.code().unwrap_or(1))
}
