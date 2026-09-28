use anyhow::{Context, Result, ensure};
use bri_client::{
    app::App,
    content::REGENERATE_HINT,
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
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.is_empty() || args[0] == "--help" {
        println!(
            "Blockland ReImagined building playtest\nUsage: bri-client --run [native-content-directory] [client-state-directory]\n       bri-client --check [native-content-directory] [client-state-directory]\n--check validates startup content/settings silently without a window or audio device.\nA visible game window is created only with --run. This build is not the complete alpha."
        );
        return Ok(());
    }
    ensure!(
        (args[0] == "--run" || args[0] == "--check") && args.len() <= 3,
        "Use --help for usage"
    );
    let content = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("content"));
    let state = match args.get(2) {
        Some(path) => PathBuf::from(path),
        None => default_state_directory()?,
    };
    // Every run keeps a session log; a crash leaves a report (and on Windows
    // a minidump) in logs/ next to the game for the player to send.
    match bri_crash::install("bri-client", &bri_crash::default_directories(&state)) {
        Ok(capture) => bri_console::echo(format!("Log: {}", capture.session_log.display())),
        Err(error) => bri_console::warn(format!("Crash capture unavailable: {error}")),
    }
    if args[0] == "--check" {
        let app = App::load(&content, &state, (1280, 720)).context(REGENERATE_HINT)?;
        println!(
            "Startup validation passed: {} maps, {} brick definitions. No window or audio device opened.",
            app.content.maps.len(),
            app.content.catalog.bricks.len()
        );
        return Ok(());
    }
    // Executing --run explicitly opts into the normal game window and audio device.
    // Library/headless callers use App::load, which always selects silent output.
    let app = App::load_with_audio(&content, &state, (1280, 720), bri_audio::OutputKind::Device)
        .context(REGENERATE_HINT)?;
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
