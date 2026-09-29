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

/// A free port for this binary's hosts, so a game already hosting on 28000
/// (the player's own, say) does not break the test. Shared by every test
/// here, as the fixed port was.
static HOSTS: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn test_port() -> u16 {
    static PORT: std::sync::OnceLock<u16> = std::sync::OnceLock::new();
    *PORT.get_or_init(|| {
        let port = std::net::UdpSocket::bind("127.0.0.1:0")
            .and_then(|s| s.local_addr())
            .map(|a| a.port())
            .expect("a free UDP port");
        // SAFETY: set once, before any host or join in this binary starts.
        unsafe {
            std::env::set_var("BRI_TEST_HOST_PORT", port.to_string());
            std::env::set_var("BRI_TEST_DISCOVERY_PORT", "0");
        }
        port
    })
}

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
    edit_avatar(app, &[("Avatar_Name", name)])
}

/// Avatar screen: click each box, clear it, type, then Done.
fn edit_avatar(app: &mut App, fields: &[(&str, &str)]) -> Result<()> {
    if !app.ui.is_open(ScreenId::Avatar) {
        app.ui.core.push(ScreenId::Avatar);
        app.ui.update(0);
    }
    ensure!(app.ui.is_open(ScreenId::Avatar), "Avatar screen did not open");
    for (control, text) in fields {
        click(app, ScreenId::Avatar, control)?;
        key(app, Key::End);
        for _ in 0..64 {
            key(app, Key::Backspace);
        }
        for c in text.chars() {
            app.ui.handle_input(InputEvent::Char(c));
        }
        app.ui.update(0);
    }
    click(app, ScreenId::Avatar, "Avatar_Done();")?;
    Ok(())
}

/// The chat box's lines without colour escapes.
fn chat_shows(app: &App, text: &str) -> bool {
    app.ui.core.chat.lines.iter().any(|l| {
        l.text
            .chars()
            .filter(|c| !(0xE000..0xE010).contains(&(*c as u32)))
            .collect::<String>()
            .contains(text)
    })
}

fn say(app: &mut App, text: &str) {
    app.ui.core.request(UiAction::Chat {
        channel: ChatChannel::Say,
        text: text.into(),
    });
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
    // Both hosting tests share the port: one at a time.
    let _turn = HOSTS.lock().unwrap_or_else(|e| e.into_inner());
    let port = test_port();
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
        game_mode: None,
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
        address: format!("127.0.0.1:{port}"),
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
    // A fresh install asks for the controls first, then welcomes the
    // player, and only then asks for the name: once.
    app.prompt_for_name();
    ensure!(
        app.ui.top_id() == ScreenId::DefaultControls && !app.ui.is_open(ScreenId::ChooseName),
        "First open should start with the controls: {:?}",
        app.ui.stack()
    );
    let apply = app
        .ui
        .screen(ScreenId::DefaultControls)
        .and_then(|s| {
            let v = s.view();
            v.walk().find_map(|n| {
                v.node(n)
                    .ctrl
                    .command
                    .clone()
                    .filter(|c| c.eq_ignore_ascii_case("defaultControlsGui.apply();"))
            })
        })
        .context("The controls screen has no OK button")?;
    click(&mut app, ScreenId::DefaultControls, &apply)?;
    ensure!(
        app.ui.top_id() == ScreenId::MessageBox,
        "No welcome after the controls: {:?}",
        app.ui.stack()
    );
    click(&mut app, ScreenId::MessageBox, "Not Now")?;
    ensure!(
        app.ui.top_id() == ScreenId::ChooseName,
        "No name prompt after the welcome: {:?}",
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
    // Nothing else asks: no second name question or box.
    app.ui.core.name_prompt();
    app.ui.update(0);
    ensure!(
        !app.ui.is_open(ScreenId::ChooseName) && !app.ui.is_open(ScreenId::MessageBox),
        "Asked for the name again: {:?}",
        app.ui.stack()
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

/// Clan tags typed on the Avatar screen (4-character boxes, trimmed like
/// v20's `onConnectRequest`) show around the name in chat
/// (`serverCmdMessageSent`'s `'\c7%1\c3%2\c7%3\c6: %4'`), for a single
/// player and for a guest, and Done while connected changes them.
#[test]
#[ignore = "requires converted native content and loopback QUIC on port 28000; no window"]
fn avatar_clan_tags_show_in_chat_as_single_player_and_guest() -> Result<()> {
    // Both hosting tests share the port: one at a time.
    let _turn = HOSTS.lock().unwrap_or_else(|e| e.into_inner());
    let port = test_port();
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = workspace.join("artifacts/native-player-name");

    // 1. Single player.
    let mut solo = load(&workspace, &fresh_state(&root, "clan-solo")?)?;
    edit_avatar(
        &mut solo,
        &[("Avatar_Name", "Solo"), ("Avatar_Prefix", "[SP] "), ("Avatar_Suffix", " ~")],
    )?;
    until(&mut [&mut solo], "solo avatar saved", Duration::from_secs(5), |a| {
        !a[0].ui.is_open(ScreenId::Avatar) && a[0].pending_requests() == 0
    })?;
    solo.ui.core.request(UiAction::HostGame {
        map: BEDROOM.into(),
        mode: ServerMode::SinglePlayer,
        game_mode: None,
        max_players: 1,
        server_name: "Clan test".into(),
        password: String::new(),
        admin_password: String::new(),
        super_admin_password: String::new(),
    });
    until(&mut [&mut solo], "single player in game", Duration::from_secs(120), |a| {
        a[0].ui.core.in_game()
    })?;
    say(&mut solo, "alone");
    until(&mut [&mut solo], "single player chat tagged", Duration::from_secs(10), |a| {
        chat_shows(a[0], "[SP]Solo~: alone")
    })?;
    solo.ui.core.request(UiAction::Disconnect);
    until(&mut [&mut solo], "single player left", Duration::from_secs(30), |a| {
        !a[0].ui.core.in_game()
    })?;
    drop(solo);

    // 2. LAN host and a guest with default trust.
    let mut host = load(&workspace, &fresh_state(&root, "clan-host")?)?;
    rename_through_avatar(&mut host, "Hosty")?;
    let mut guest = load(&workspace, &fresh_state(&root, "clan-guest")?)?;
    edit_avatar(
        &mut guest,
        &[("Avatar_Name", "Guesty"), ("Avatar_Prefix", "[G] "), ("Avatar_Suffix", "")],
    )?;
    until(
        &mut [&mut host, &mut guest],
        "avatars saved",
        Duration::from_secs(5),
        |a| a.iter().all(|a| !a.ui.is_open(ScreenId::Avatar) && a.pending_requests() == 0),
    )?;
    host.ui.core.request(UiAction::HostGame {
        map: BEDROOM.into(),
        mode: ServerMode::Lan,
        game_mode: None,
        max_players: 4,
        server_name: "Clan test".into(),
        password: String::new(),
        admin_password: String::new(),
        super_admin_password: String::new(),
    });
    until(&mut [&mut host], "host in game", Duration::from_secs(120), |a| {
        a[0].ui.core.in_game()
    })?;
    guest.ui.core.request(UiAction::JoinServer {
        address: format!("127.0.0.1:{port}"),
        password: String::new(),
    });
    until(&mut [&mut host, &mut guest], "guest in game", Duration::from_secs(120), |a| {
        a[1].ui.core.in_game() && names(a[0]).contains(&"Guesty".to_string())
    })?;
    say(&mut guest, "hello");
    until(
        &mut [&mut host, &mut guest],
        "guest chat tagged on both",
        Duration::from_secs(10),
        |a| a.iter().all(|a| chat_shows(a, "[G]Guesty: hello")),
    )?;
    // Done in game with new tags; the next line carries them.
    edit_avatar(&mut guest, &[("Avatar_Prefix", ""), ("Avatar_Suffix", "[NW]")])?;
    until(&mut [&mut host, &mut guest], "tags sent", Duration::from_secs(10), |a| {
        a[1].pending_requests() == 0
    })?;
    say(&mut guest, "again");
    until(
        &mut [&mut host, &mut guest],
        "new tags on the host",
        Duration::from_secs(10),
        |a| chat_shows(a[0], "Guesty[NW]: again"),
    )?;
    guest.ui.core.request(UiAction::Disconnect);
    host.ui.core.request(UiAction::Disconnect);
    let _ = step(&mut guest, Duration::ZERO);
    let _ = step(&mut host, Duration::ZERO);
    Ok(())
}
