// Release builds are desktop apps: no console window behind the game.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
use anyhow::{Context, Result, ensure};
use bri_client::{
    app::App,
    platform::{self, PlatformConfig},
};
use std::path::PathBuf;

/// mimalloc: the persistent world maps and replication allocate heavily.
/// On a 200k-brick world it cut world build 17%, wire decode 20%, JSON
/// load 16% and collider inserts 18% against the system allocator (Linux;
/// Windows' heap usually gains more).
/// `bri_net::allocator::tune` keeps its purges off the tick.
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

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
    #[cfg(target_os = "macos")]
    if let Some(content) = mac_bundle::content_directory() {
        return content;
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("content")))
        .filter(|dir| dir.is_dir())
        .unwrap_or_else(|| PathBuf::from("content"))
}
fn main() -> Result<()> {
    bri_net::allocator::tune();
    bri_client::perf::startup::begin();
    let result = game();
    // A startup error's message must reach the log and terminal before exit.
    bri_crash::finish();
    result
}
fn game() -> Result<()> {
    // A release build has no console window; when started from a terminal,
    // use that terminal for --help, --check and the echoed log.
    bri_crash::attach_parent_console();
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    // The elevated helper the host starts to let the game through Windows
    // Firewall (one Windows permission prompt); it does nothing else.
    if args.first().is_some_and(|a| a == bri_client::firewall::ALLOW_FLAG) {
        let port = args.get(1).and_then(|a| a.to_str()).unwrap_or_default();
        return bri_client::firewall::run_helper(port);
    }
    if args.first().is_some_and(|a| a == "--version") {
        println!("{}", bri_client::updates::version());
        return Ok(());
    }
    if args.first().is_some_and(|a| a == "--help") {
        println!(
            "Blockland ReImagined {}\nUsage: bri-client [--run] [native-content-directory] [client-state-directory]\n       bri-client --check [native-content-directory] [client-state-directory]\nWith no arguments the game opens with the content beside it.\n--check validates startup content/settings silently without a window or audio device; it changes nothing unless BRI_INSTALL_DEFAULT_ADD_ONS=1 lets it install a checkout's default Add-Ons first.\n--version prints the build's version.",
            bri_client::updates::version()
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
    let program = format!("bri-client {}", bri_client::updates::version());
    let capture = match bri_crash::install(&program, &bri_crash::default_directories(&state)) {
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
        // Validation leaves the content as it is (the push gate checks the
        // shared main checkout's); a fresh checkout's own check can opt in.
        report_installed(bri_package::defaults::install_when_asked(&content)?);
        let app = App::load(&content, &state, (1280, 720))?;
        println!(
            "Startup validation passed: {} maps, {} brick definitions. No window or audio device opened.",
            app.content.maps.len(),
            app.content.catalog.bricks.len()
        );
        // The release gate reads the full list from logs/add-on-health.json.
        let health = app.add_on_health();
        println!(
            "Add-On health: {}",
            health.summary().unwrap_or_else(|| "no problems".into())
        );
        return Ok(());
    }
    bri_crash::enable_dialogs();
    let logs = capture.as_ref().map(|c| c.directory.as_path());
    if let Some(report) = capture.as_ref().and_then(|c| c.previous_crash.as_deref()) {
        bri_crash::acknowledge(report);
        bri_crash::alert(
            &format!("{} closed unexpectedly", bri_crash::PRODUCT),
            &bri_crash::previous_crash_message(report),
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
/// Running the game from a source checkout gives its generated content the
/// default Add-Ons (packages/default-addons.json) the first time, as a
/// release has them, and keeps them in step with the checkout. A release's
/// content is left alone.
fn install_default_add_ons(content: &std::path::Path) -> Result<()> {
    report_installed(bri_package::defaults::install_from_checkout(content)?);
    Ok(())
}
fn report_installed(done: Option<bri_package::defaults::Installed>) {
    if let Some(done) = done
        && !done.is_empty()
    {
        bri_console::echo(format!(
            "Installed the default Add-Ons {}.",
            done.ids().join(", ")
        ));
    }
}
fn run(content: &std::path::Path, state: &std::path::Path) -> Result<()> {
    // The GPU opens while the content loads.
    let early_gpu = platform::EarlyGpu::start();
    install_default_add_ons(content)?;
    // Executing the game opts into the normal game window and audio device.
    // Library/headless callers use App::load, which always selects silent output.
    let mut app = App::load_with_audio(content, state, (1280, 720), bri_audio::OutputKind::Device)
        .context("Loading the game")?;
    bri_client::perf::startup::mark("content loaded");
    app.player_session();
    app.prompt_for_name();
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
        app,
        early_gpu,
    })
}

/// The game packaged as `BlocklandReImagined.app` (tools/package_mac.sh)
/// carries its content in `Contents/Resources/content`. The game writes to
/// its content folder (the Add-On list, imports, a server's downloaded
/// Add-Ons), and an app bundle must stay unchanged for its signature (macOS
/// may even run it from a read-only copy). So the first launch of each build
/// copies the bundled content into the player's Application Support, one
/// folder per build like one Windows release folder, and plays from there.
#[cfg(target_os = "macos")]
mod mac_bundle {
    use anyhow::{Context, Result};
    use std::path::{Path, PathBuf};

    /// Written last, so a half-finished copy is never used.
    const COMPLETE: &str = ".bundled-content-complete";

    /// `None` when the executable is not inside an app bundle with content.
    pub fn content_directory() -> Option<PathBuf> {
        let bundled = bundled_content()?;
        match installed_copy(&bundled) {
            Ok(content) => Some(content),
            Err(error) => {
                // Play from the bundle; only writing Add-Ons will fail.
                bri_console::warn(format!(
                    "Could not copy the game content out of the app, playing from the app itself: {error:#}"
                ));
                Some(bundled)
            }
        }
    }

    /// `<name>.app/Contents/Resources/content` for an executable in
    /// `<name>.app/Contents/MacOS`.
    fn bundled_content() -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
        let contents = exe.parent().filter(|d| d.ends_with("Contents/MacOS"))?.parent()?;
        let content = contents.join("Resources/content");
        content.is_dir().then_some(content)
    }

    fn installed_copy(bundled: &Path) -> Result<PathBuf> {
        let home = std::env::var_os("HOME").context("HOME is missing")?;
        // One folder per build: a new build starts from its own content.
        let build: String = bri_client::updates::version()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' })
            .collect();
        let root = PathBuf::from(home).join("Library/Application Support/BlocklandReImagined/content");
        let target = root.join(&build);
        if target.join(COMPLETE).is_file() {
            return Ok(target);
        }
        bri_console::echo(format!("Copying the game content to {} (first launch of this build)", target.display()));
        std::fs::create_dir_all(&root).with_context(|| format!("Creating {}", root.display()))?;
        let staging = root.join(format!(".{build}.partial"));
        if staging.exists() {
            std::fs::remove_dir_all(&staging).with_context(|| format!("Removing {}", staging.display()))?;
        }
        // std::fs::copy clones files on APFS, so this is quick on one volume.
        copy_tree(bundled, &staging)?;
        std::fs::write(staging.join(COMPLETE), bri_client::updates::version())?;
        if target.exists() {
            std::fs::remove_dir_all(&target).with_context(|| format!("Removing {}", target.display()))?;
        }
        std::fs::rename(&staging, &target).with_context(|| format!("Finishing {}", target.display()))?;
        Ok(target)
    }

    fn copy_tree(from: &Path, to: &Path) -> Result<()> {
        std::fs::create_dir_all(to).with_context(|| format!("Creating {}", to.display()))?;
        for entry in std::fs::read_dir(from).with_context(|| format!("Reading {}", from.display()))? {
            let entry = entry?;
            let kind = entry.file_type()?;
            let destination = to.join(entry.file_name());
            if kind.is_dir() {
                copy_tree(&entry.path(), &destination)?;
            } else if kind.is_file() {
                std::fs::copy(entry.path(), &destination)
                    .with_context(|| format!("Copying {}", entry.path().display()))?;
            }
            // The packager refuses links, so there are none to follow.
        }
        Ok(())
    }
}
