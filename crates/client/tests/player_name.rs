//! Player name from the Avatar screen to the server, over loopback.
//! Drives the real Avatar screen (click the Name box, type, click Done), then
//! hosts a LAN game and joins it from a second native client.
//! Run: cargo test -p bri-client --test player_name -- --ignored --nocapture
use anyhow::{Context, Result, bail, ensure};
use bri_client::{app::App, platform::PlatformApp, settings};
use bri_ui::{
    api::*,
    input::{InputEvent, Key, Modifiers, MouseButton},
    screens::ScreenId,
};
use std::{
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const SIZE: (u32, u32) = (960, 720);
const BEDROOM: &str = "v20/add-ons/map_bedroom/bedroom.mis";

fn step(app: &mut App, elapsed: Duration) -> Result<()> {
    app.tick(elapsed)?;
    app.ui.update(elapsed.as_millis() as u64);
    ensure!(app.pump()?.is_empty(), "Unexpected window command");
    if let ConnectionState::Failed { reason } = &app.ui.core.conn {
        bail!("Connection failed: {reason}");
    }
    Ok(())
}

/// Advances every app together so host and joiner both keep running.
fn until(
    apps: &mut [&mut App],
    what: &str,
    timeout: Duration,
    ready: impl Fn(&[&mut App]) -> bool,
) -> Result<()> {
    let start = Instant::now();
    let mut previous = start;
    loop {
        let now = Instant::now();
        for app in apps.iter_mut() {
            step(app, now.duration_since(previous))?;
        }
        previous = now;
        if ready(apps) {
            return Ok(());
        }
        ensure!(
            start.elapsed() < timeout,
            "Timed out waiting for {what}; states {:?}; names {:?}",
            apps.iter().map(|a| a.ui.core.conn.clone()).collect::<Vec<_>>(),
            apps.iter()
                .map(|a| a.network_view().map(|v| v.names.clone()))
                .collect::<Vec<_>>()
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn click(app: &mut App, screen: ScreenId, control: &str) -> Result<()> {
    let (x, y) = app
        .ui
        .control_center(screen, control)
        .with_context(|| format!("{control} is missing on {screen:?}"))?;
    app.ui.handle_input(InputEvent::MouseMove { x, y });
    app.ui.handle_input(InputEvent::MouseDown {
        button: MouseButton::Left,
        x,
        y,
    });
    app.ui.handle_input(InputEvent::MouseUp {
        button: MouseButton::Left,
        x,
        y,
    });
    app.ui.update(0);
    Ok(())
}

fn key(app: &mut App, key: Key) {
    let mods = Modifiers::NONE;
    app.ui.handle_input(InputEvent::KeyDown {
        key,
        mods,
        repeat: false,
    });
    app.ui.handle_input(InputEvent::KeyUp { key, mods });
}

/// Main menu (or in-game Options) → Avatar → type in the Name box → Done.
fn rename_through_avatar(app: &mut App, name: &str) -> Result<()> {
    if !app.ui.is_open(ScreenId::Avatar) {
        app.ui.core.push(ScreenId::Avatar);
        app.ui.update(0);
    }
    ensure!(app.ui.is_open(ScreenId::Avatar), "Avatar screen did not open");
    click(app, ScreenId::Avatar, "Avatar_Name")?;
    key(app, Key::End);
    for _ in 0..64 {
        key(app, Key::Backspace);
    }
    for c in name.chars() {
        app.ui.handle_input(InputEvent::Char(c));
    }
    app.ui.update(0);
    click(app, ScreenId::Avatar, "Avatar_Done();")?;
    Ok(())
}

fn load(workspace: &Path, state: &Path) -> Result<App> {
    std::fs::create_dir_all(state)?;
    let mut app = App::load(&workspace.join("content"), state, SIZE)?;
    app.ui.core.pop(ScreenId::DefaultControls);
    app.ui.update(0);
    Ok(app)
}

fn fresh_state(root: &Path, label: &str) -> Result<PathBuf> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    Ok(root.join(format!("{label}-{}-{stamp}", std::process::id())))
}

fn names(app: &App) -> Vec<String> {
    app.network_view()
        .map(|v| v.names.values().cloned().collect())
        .unwrap_or_default()
}

fn chat_has(app: &App, text: &str) -> bool {
    app.network_view()
        .is_some_and(|v| v.chat.iter().any(|l| format!("{l:?}").contains(text)))
}

#[test]
#[ignore = "requires converted native content and loopback QUIC on port 28000; no window"]
fn avatar_name_reaches_server_on_join_and_live_rename() -> Result<()> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = workspace.join("artifacts/native-player-name");
    let host_state = fresh_state(&root, "host")?;
    let join_state = fresh_state(&root, "join")?;

    // 1. Name chosen on the main menu's Avatar screen is saved.
    let mut host = load(&workspace, &host_state)?;
    rename_through_avatar(&mut host, "Hosty")?;
    until(&mut [&mut host], "host avatar saved", Duration::from_secs(5), |a| {
        !a[0].ui.is_open(ScreenId::Avatar) && a[0].pending_requests() == 0
    })?;
    ensure!(
        host.ui.settings().avatar.lan_name == "Hosty",
        "Done did not keep the name: {:?}",
        host.ui.settings().avatar.lan_name
    );
    let file = host_state.join("settings.json");
    until(&mut [&mut host], "name in settings.json", Duration::from_secs(5), |_| {
        settings::load(&file).is_ok_and(|s| s.avatar.lan_name == "Hosty")
    })?;

    let mut joiner = load(&workspace, &join_state)?;
    rename_through_avatar(&mut joiner, "Joiny")?;
    until(&mut [&mut joiner], "joiner avatar saved", Duration::from_secs(5), |a| {
        !a[0].ui.is_open(ScreenId::Avatar) && a[0].pending_requests() == 0
    })?;
    let file = join_state.join("settings.json");
    until(&mut [&mut joiner], "joiner name saved", Duration::from_secs(5), |_| {
        settings::load(&file).is_ok_and(|s| s.avatar.lan_name == "Joiny")
    })?;
    // A restart reads the saved name back.
    drop(joiner);
    let mut joiner = load(&workspace, &join_state)?;
    ensure!(
        joiner.ui.settings().avatar.lan_name == "Joiny",
        "Restart lost the name: {:?}",
        joiner.ui.settings().avatar.lan_name
    );

    // 2. Host a LAN game and join it over loopback.
    host.ui.core.request(UiAction::HostGame {
        map: BEDROOM.into(),
        mode: ServerMode::Lan,
        max_players: 4,
        server_name: "Name test".into(),
        password: String::new(),
        admin_password: String::new(),
        super_admin_password: String::new(),
    });
    until(&mut [&mut host], "host in game", Duration::from_secs(120), |a| {
        a[0].ui.core.in_game()
    })?;
    joiner.ui.core.request(UiAction::JoinServer {
        address: "127.0.0.1:28000".into(),
        password: String::new(),
    });
    until(
        &mut [&mut host, &mut joiner],
        "joiner in game with both names",
        Duration::from_secs(120),
        |a| {
            a[1].ui.core.in_game()
                && names(a[0]).contains(&"Joiny".to_string())
                && names(a[1]).contains(&"Hosty".to_string())
        },
    )?;
    println!("after join: host {:?} joiner {:?}", names(&host), names(&joiner));

    // 3. Rename while connected, through the in-game Avatar screen.
    rename_through_avatar(&mut joiner, "Renamed Joiny")?;
    until(
        &mut [&mut host, &mut joiner],
        "live rename on both clients",
        Duration::from_secs(10),
        |a| {
            names(a[0]).contains(&"Renamed Joiny".to_string())
                && names(a[1]).contains(&"Renamed Joiny".to_string())
                && !names(a[0]).contains(&"Joiny".to_string())
                && chat_has(a[0], "Joiny is now known as Renamed Joiny")
        },
    )?;
    // 4. Taking the host's name gets a number suffix.
    rename_through_avatar(&mut joiner, "Hosty")?;
    until(
        &mut [&mut host, &mut joiner],
        "duplicate rename suffixed",
        Duration::from_secs(10),
        |a| names(a[0]).contains(&"Hosty 2".to_string()),
    )?;
    println!("after renames: host {:?} joiner {:?}", names(&host), names(&joiner));
    joiner.ui.core.request(UiAction::Disconnect);
    host.ui.core.request(UiAction::Disconnect);
    let _ = step(&mut joiner, Duration::ZERO);
    let _ = step(&mut host, Duration::ZERO);
    Ok(())
}

fn render_png(app: &App, path: &Path) -> Result<()> {
    let gpu = bri_ui::gpu::Headless::new()?;
    let mut renderer = bri_ui::gpu::UiRenderer::new(&gpu.device, &gpu.queue);
    let pixels = gpu.render_rgba(
        &mut renderer,
        &app.ui.core.pack,
        &app.ui.draw(),
        SIZE,
        app.ui.scale(),
        [0.16, 0.22, 0.3, 1.0],
    )?;
    image::save_buffer(path, &pixels, SIZE.0, SIZE.1, image::ColorType::Rgba8)?;
    Ok(())
}

#[test]
#[ignore = "requires converted native content and an offscreen GPU; no window"]
fn first_open_asks_for_a_name_once() -> Result<()> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = workspace.join("artifacts/native-player-name");
    let state = fresh_state(&root, "first-open")?;
    std::fs::create_dir_all(&state)?;
    let mut app = App::load(&workspace.join("content"), &state, SIZE)?;
    app.prompt_for_name();
    ensure!(
        app.ui.top_id() == ScreenId::ChooseName,
        "No name prompt on first open: {:?}",
        app.ui.stack()
    );
    render_png(&app, &root.join("first-open-name-prompt.png"))?;
    // The box is focused and prefilled; replace the suggestion and press Enter.
    key(&mut app, Key::End);
    for _ in 0..32 {
        key(&mut app, Key::Backspace);
    }
    for c in "Max".chars() {
        app.ui.handle_input(InputEvent::Char(c));
    }
    key(&mut app, Key::Return);
    app.ui.update(0);
    ensure!(
        !app.ui.is_open(ScreenId::ChooseName) && app.ui.settings().avatar.lan_name == "Max",
        "Prompt did not take the name: {:?} {:?}",
        app.ui.stack(),
        app.ui.settings().avatar.lan_name
    );
    let file = state.join("settings.json");
    until(&mut [&mut app], "prompted name saved", Duration::from_secs(5), |_| {
        settings::load(&file).is_ok_and(|s| s.avatar.lan_name == "Max")
    })?;
    drop(app);
    let mut app = App::load(&workspace.join("content"), &state, SIZE)?;
    app.prompt_for_name();
    ensure!(
        !app.ui.is_open(ScreenId::ChooseName),
        "Prompt came back after a name was chosen"
    );
    Ok(())
}
