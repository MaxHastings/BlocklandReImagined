// Release builds are desktop apps: no console window behind the game.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
use anyhow::{Context, Result, ensure};
use bri_client::{
    app::App,
    platform::{self, PlatformConfig},
};
use std::path::PathBuf;

fn default_state_directory() -> Result<PathBuf> {
    #[cfg(windows)]
    let path = PathBuf::from(
        std::env::var_os("LOCALAPPDATA")
            .context("LOCALAPPDATA is missing; supply an explicit client-state-directory")?,
    )
    .join("BlocklandReImagined");
    #[cfg(target_os = "macos")]
    let path = PathBuf::from(
        std::env::var_os("HOME")
            .context("HOME is missing; supply an explicit client-state-directory")?,
    )
    .join("Library/Application Support/BlocklandReImagined");
    #[cfg(not(any(windows, target_os = "macos")))]
    let path = match std::env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
        Some(root) => PathBuf::from(root).join("blockland-reimagined"),
        None => PathBuf::from(
            std::env::var_os("HOME")
                .context("HOME is missing; supply an explicit client-state-directory")?,
        )
        .join(".local/share/blockland-reimagined"),
    };
    ensure!(
        path.is_absolute(),
        "Default client state path is not absolute; supply an explicit client-state-directory"
    );
    Ok(path)
}
/// Content shipped beside the executable (a packaged game), else the
/// working directory's `content` (a source checkout).
fn default_content_directory() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("content")))
        .filter(|dir| dir.is_dir())
        .unwrap_or_else(|| PathBuf::from("content"))
}
fn main() -> Result<()> {
    // A release build has no console window; when started from a terminal,
    // use that terminal for --help, --check and the echoed log.
    bri_crash::attach_parent_console();
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|a| a == "--help") {
        println!(
            "Blockland ReImagined\nUsage: bri-client [--run] [native-content-directory] [client-state-directory]\n       bri-client --check [native-content-directory] [client-state-directory]\nWith no arguments the game opens with the content beside it.\n--check validates startup content/settings silently without a window or audio device."
        );
        return Ok(());
    }
    // Double-clicking the game runs it.
    let (mode, rest) = match args.first().and_then(|a| a.to_str()) {
        Some("--run") | Some("--check") => (args[0].to_str().unwrap_or("--run"), &args[1..]),
        _ => ("--run", &args[..]),
    };
    ensure!(rest.len() <= 2, "Use --help for usage");
    let content = rest
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(default_content_directory);
    let state = match rest.get(1) {
        Some(path) => PathBuf::from(path),
        None => default_state_directory()?,
    };
    // Every run keeps a session log; a crash leaves a report (and on Windows
    // a minidump) in logs/ next to the game for the player to send.
    let capture = match bri_crash::install("bri-client", &bri_crash::default_directories(&state)) {
        Ok(capture) => {
            bri_console::echo(format!("Log: {}", capture.session_log.display()));
            Some(capture)
        }
        Err(error) => {
            bri_console::warn(format!("Crash capture unavailable: {error}"));
            None
        }
    };
    if mode == "--check" {
        let app = App::load(&content, &state, (1280, 720))?;
        println!(
            "Startup validation passed: {} maps, {} brick definitions. No window or audio device opened.",
            app.content.maps.len(),
            app.content.catalog.bricks.len()
        );
        return Ok(());
    }
    bri_crash::enable_dialogs();
    let logs = capture.as_ref().map(|c| c.directory.as_path());
    if let Some(report) = capture.as_ref().and_then(|c| c.previous_crash.as_deref()) {
        bri_crash::acknowledge(report);
        bri_crash::alert(
            &format!("{} closed unexpectedly", bri_crash::PRODUCT),
            &format!(
                "{} crashed last time it ran. A crash report was saved as {}; sending it helps get the problem fixed.",
                bri_crash::PRODUCT,
                report.file_name().unwrap_or_default().to_string_lossy()
            ),
            logs,
        );
    }
    let result = run(&content, &state);
    if let Err(error) = &result {
        // Developers find the content regeneration hint in the log.
        bri_console::error(format!("{error:#}"));
        bri_console::echo(bri_client::content::REGENERATE_HINT);
        bri_crash::alert(
            &format!("{} could not continue", bri_crash::PRODUCT),
            &bri_crash::summarize(&format!("{error:#}")),
            logs,
        );
    }
    result
}
fn run(content: &std::path::Path, state: &std::path::Path) -> Result<()> {
    // Executing the game opts into the normal game window and audio device.
    // Library/headless callers use App::load, which always selects silent output.
    let app = App::load_with_audio(content, state, (1280, 720), bri_audio::OutputKind::Device)
        .context("Loading the game")?;
    for warning in app.audio_warnings() {
        bri_console::warn(warning);
    }
    let display = bri_client::settings::startup_display(&app.ui.settings());
    platform::run(PlatformConfig {
        title: "Blockland ReImagined — building playtest".into(),
        size: display.size,
        fullscreen: display.fullscreen,
        vsync: display.vsync,
        max_fps: display.max_fps,
        app: Box::new(app),
    })
}
