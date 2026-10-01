//! The game's screens, used only through clicks and typing, change the
//! server in each way a player hosts or joins: single player, a LAN host
//! with a joined guest, and an internet host with a joined guest who starts
//! with no trust. Complements `bri-ui`'s `field_flow`, which checks what each
//! screen sends; this checks what the server then does with it.
//!
//! Slow (three hosted games on the first loadable map, v20's Bedroom). Runs
//! on the made-up content root, whose UI pack carries `bri_ui::testing`'s
//! screen layouts; the ignored variant runs on the generated v20 content
//! (`-- --ignored`, BRI_CONTENT or content/). Never creates a window or OS
//! input, and hosts on a free test port.
use anyhow::{Context, Result, bail, ensure};
use bri_client::{app::App, platform::PlatformApp};
use bri_ui::{
    api::*,
    input::{InputEvent, Key, Modifiers, MouseButton},
    screens::ScreenId,
    view::View,
};
use std::{
    path::Path,
    thread,
    time::{Duration, Instant},
};

#[macro_use]
mod support;
use support::content_root::ContentRoot;

synthetic_and_content!(ContentRoot: screens_reach_the_server_in_every_topology);

const SIZE: (u32, u32) = (1280, 800);

// ------------------------------------------------------------- stepping

fn step(app: &mut App, elapsed: Duration) -> Result<()> {
    app.tick(elapsed)?;
    app.ui.update(elapsed.as_millis() as u64);
    ensure!(app.pump()?.is_empty(), "Unexpected window command");
    decline_firewall(app)?;
    if let ConnectionState::Failed { reason } = &app.ui.core.conn {
        bail!("Connection failed: {reason}");
    }
    Ok(())
}

/// A hosted game asks to let the game through Windows Firewall when it is
/// blocked; answering Yes would open a Windows prompt, so the test says No.
fn decline_firewall(app: &mut App) -> Result<()> {
    let asking = app.ui.screen(ScreenId::MessageBox).is_some_and(|s| {
        let v = s.view();
        v.walk()
            .any(|n| v.text_of(n).contains("Windows Firewall would stop"))
    });
    if asking {
        click(app, ScreenId::MessageBox, "MessageBoxYesNoDlg.noCallback();")?;
    }
    Ok(())
}

/// Step every app together until `ready` holds.
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
            "Timed out waiting for {what}; screens {:?}; sounds {:?}; prints {:?}; tools {:?}; pending {:?}; chat {:?}",
            apps.iter().map(|a| a.ui.stack()).collect::<Vec<_>>(),
            apps.iter()
                .map(|a| a
                    .audio_requests()
                    .iter()
                    .filter(|(k, _)| k.contains("wrench"))
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>())
                .collect::<Vec<_>>(),
            apps.iter()
                .map(|a| a.ui.core.center_print.clone())
                .collect::<Vec<_>>(),
            apps.iter()
                .map(|a| a
                    .network_view()
                    .and_then(|v| v.tools.get(&v.owner).map(|t| t.selected)))
                .collect::<Vec<_>>(),
            apps.iter()
                .map(|a| a.pending_requests())
                .collect::<Vec<_>>(),
            apps.iter()
                .map(|a| a
                    .ui
                    .core
                    .chat
                    .lines
                    .iter()
                    .map(|l| l.text.clone())
                    .collect::<Vec<_>>())
                .collect::<Vec<_>>()
        );
        thread::sleep(Duration::from_millis(10));
    }
}

/// Step for a while regardless (to let a refused request come back).
fn settle(apps: &mut [&mut App], time: Duration) -> Result<()> {
    let start = Instant::now();
    until(apps, "time to pass", time + Duration::from_secs(5), |_| {
        start.elapsed() >= time
    })
}

// ------------------------------------------------------------ player input

fn view(app: &App, screen: ScreenId) -> Result<&View> {
    Ok(app
        .ui
        .screen(screen)
        .with_context(|| format!("{screen:?} is not open: {:?}", app.ui.stack()))?
        .view())
}

/// A control by object name, preference, command (any case) or visible text.
fn find(v: &View, control: &str) -> Option<usize> {
    v.id(control)
        .or_else(|| {
            v.walk()
                .find(|&n| v.node(n).ctrl.variable.as_deref() == Some(control))
        })
        .or_else(|| {
            v.walk().find(|&n| {
                v.node(n)
                    .ctrl
                    .command
                    .as_deref()
                    .is_some_and(|c| c.eq_ignore_ascii_case(control))
            })
        })
        .or_else(|| {
            v.walk()
                .find(|&n| v.is_shown(n) && v.text_of(n).trim() == control)
        })
}

fn mouse_click(app: &mut App, (x, y): (f32, f32)) {
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
}

/// Wheel the scroll holding `node` until the control is in view.
fn reveal(app: &mut App, screen: ScreenId, node: usize) -> Result<()> {
    for _ in 0..200 {
        let v = view(app, screen)?;
        let mut scroll = v.node(node).parent;
        while let Some(p) = scroll {
            if v.node(p).ctrl.class == "GuiScrollCtrl" {
                break;
            }
            scroll = v.node(p).parent;
        }
        let Some(scroll) = scroll else { return Ok(()) };
        let (outer, r) = (v.node(scroll).rect, v.node(node).rect);
        let delta = if r.y < outer.y {
            1.0
        } else if r.bottom() > outer.bottom() {
            -1.0
        } else {
            return Ok(());
        };
        let s = app.ui.scale();
        let at = ((outer.x + 4) as f32 * s, (outer.y + outer.h / 2) as f32 * s);
        app.ui
            .handle_input(InputEvent::MouseMove { x: at.0, y: at.1 });
        let before = r;
        app.ui.handle_input(InputEvent::Wheel { delta });
        app.ui.update(0);
        if view(app, screen)?.node(node).rect == before {
            return Ok(());
        }
    }
    Ok(())
}

/// Click a control where a player would, after checking the click reaches it.
fn click(app: &mut App, screen: ScreenId, control: &str) -> Result<()> {
    let node = find(view(app, screen)?, control)
        .with_context(|| format!("{control} is not on {screen:?}"))?;
    reveal(app, screen, node)?;
    let v = view(app, screen)?;
    let r = v.node(node).rect;
    let boxed = matches!(
        v.node(node).ctrl.class.as_str(),
        "GuiCheckBoxCtrl" | "GuiRadioCtrl"
    );
    let x = if boxed {
        r.x + (r.h / 2).min(r.w / 2)
    } else {
        r.x + r.w / 2
    };
    let y = r.y + r.h / 2;
    let mut hit = v.hit(x, y);
    while let Some(h) = hit {
        if h == node {
            break;
        }
        hit = v.node(h).parent;
    }
    ensure!(
        hit == Some(node),
        "{control} on {screen:?} is covered by {:?}",
        v.hit(x, y).map(|h| v.node(h).ctrl.name.clone())
    );
    ensure!(
        v.node(node).state.active,
        "{control} on {screen:?} is greyed out"
    );
    let s = app.ui.scale();
    mouse_click(app, (x as f32 * s, y as f32 * s));
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
    app.ui.update(0);
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        app.ui.handle_input(InputEvent::Char(c));
    }
    app.ui.update(0);
}

/// Click into a text box, clear it and type `text`.
fn fill(app: &mut App, screen: ScreenId, control: &str, text: &str) -> Result<()> {
    click(app, screen, control)?;
    key(app, Key::End);
    for _ in 0..80 {
        key(app, Key::Backspace);
    }
    type_text(app, text);
    let v = view(app, screen)?;
    let n = find(v, control).unwrap();
    ensure!(
        v.edit_text(n) == text,
        "{control} holds {:?} after typing {text:?}",
        v.edit_text(n)
    );
    Ok(())
}

/// Pick a dropdown entry by clicking it open and typing to filter.
fn pick(app: &mut App, screen: ScreenId, popup: &str, entry: &str) -> Result<()> {
    click(app, screen, popup)?;
    type_text(app, entry);
    key(app, Key::Return);
    let v = view(app, screen)?;
    let n = find(v, popup).unwrap();
    ensure!(
        v.selected_text(n).as_deref() == Some(entry),
        "{popup} shows {:?} after picking {entry:?}",
        v.selected_text(n)
    );
    Ok(())
}

/// Click the row of a list that shows `shown`.
fn pick_row(app: &mut App, screen: ScreenId, list: &str, shown: &str) -> Result<()> {
    let v = view(app, screen)?;
    let n = v
        .id(list)
        .with_context(|| format!("{list} is not on {screen:?}"))?;
    let items = &v.node(n).state.items;
    let Some((row, &(_, id))) = items
        .iter()
        .enumerate()
        .find(|(_, (t, _))| t.contains(shown))
    else {
        bail!(
            "{list} has no row showing {shown:?}: {:?}",
            items.iter().map(|i| &i.0).collect::<Vec<_>>()
        );
    };
    let r = v.node(n).rect;
    let h = v.node(n).state.row_height.max(1);
    let s = app.ui.scale();
    mouse_click(
        app,
        (
            (r.x + 8) as f32 * s,
            (r.y + h * row as i32 + h / 2) as f32 * s,
        ),
    );
    ensure!(
        view(app, screen)?.selected(n) == Some(id),
        "clicking {shown:?} in {list} did not select it"
    );
    Ok(())
}

fn open(app: &mut App, screen: ScreenId) {
    if !app.ui.is_open(screen) {
        app.ui.core.push(screen);
        app.ui.update(0);
    }
}

// ------------------------------------------------------------ the world

fn in_game(app: &App) -> bool {
    matches!(app.ui.core.conn, ConnectionState::InGame { .. })
        && app
            .network_view()
            .is_some_and(|v| v.poses.contains_key(&v.owner))
}

fn eye(app: &App) -> glam::Vec3 {
    let v = app.network_view().unwrap();
    v.poses[&v.owner]
        .player
        .eye(&bri_sim::player::PlayerTuning::default())
}

/// Turn the player's view onto `point`, as mouse movement would. A look
/// is a mouse turn (positive pitch is the mouse moving down) that the game
/// scales by the field of view, so turn until the view settles on it.
fn aim(app: &mut App, point: glam::Vec3) {
    let d = point - eye(app);
    // `PlayerState::forward`: positive pitch looks up.
    let yaw = d.x.atan2(-d.z);
    let pitch = d.y.atan2((d.x * d.x + d.z * d.z).sqrt());
    let wrap = |mut t: f32| {
        while t > std::f32::consts::PI {
            t -= std::f32::consts::TAU;
        }
        while t < -std::f32::consts::PI {
            t += std::f32::consts::TAU;
        }
        t
    };
    for _ in 0..20 {
        let turn = wrap(yaw - app.controls.yaw);
        let tilt = pitch - app.controls.pitch;
        if turn.abs() < 1e-4 && tilt.abs() < 1e-4 {
            break;
        }
        app.ui.core.request(UiAction::Game(GameAction::Look {
            yaw: turn,
            pitch: -tilt,
        }));
        let _ = app.pump();
        app.ui.update(0);
    }
}

/// A mouse click on the fire button: pressed for a tenth of a second.
fn fire(app: &mut App) -> Result<()> {
    for down in [true, false] {
        app.ui.core.request(UiAction::Game(GameAction::Held {
            control: HeldControl::Fire,
            down,
        }));
        let _ = app.pump();
        if down {
            settle(&mut [&mut *app], Duration::from_millis(100))?;
        }
    }
    app.ui.update(0);
    Ok(())
}

fn bricks(app: &App) -> Vec<(u64, bri_world::Brick)> {
    app.network_view()
        .map(|v| {
            v.world
                .bricks
                .iter()
                .map(|(id, b)| (*id, b.clone()))
                .collect()
        })
        .unwrap_or_default()
}

fn names(app: &App) -> Vec<String> {
    app.network_view()
        .map(|v| v.names.values().cloned().collect())
        .unwrap_or_default()
}

fn chat_has(app: &App, text: &str) -> bool {
    app.network_view()
        .is_some_and(|v| v.chat.iter().any(|l| l.text.contains(text)))
}

fn load(content: &Path, state: &Path, name: &str) -> Result<App> {
    let mut app = App::load(content, state, SIZE)?;
    app.ui.core.pop(ScreenId::DefaultControls);
    app.ui
        .core
        .prefs
        .set_bool(bri_ui::screens::name::PROMPTED, true);
    app.ui.core.settings.avatar.lan_name = name.into();
    app.ui.update(0);
    Ok(app)
}

/// Host and discovery ports for this process, away from the game's own.
fn use_test_ports() -> u16 {
    let port = std::net::UdpSocket::bind("127.0.0.1:0")
        .and_then(|s| s.local_addr())
        .map(|a| a.port())
        .expect("a free UDP port");
    // SAFETY: set before any host or join starts, under the GPU turn, which
    // every test here holding a port takes first.
    unsafe {
        std::env::set_var("BRI_TEST_HOST_PORT", port.to_string());
        std::env::set_var("BRI_TEST_DISCOVERY_PORT", "0");
    }
    port
}

/// Run one check's steps, which may stop early with `?`.
fn attempt(steps: impl FnOnce() -> Result<()>) -> Result<()> {
    steps()
}

/// Problems found so far; a run keeps going past independent ones.
#[derive(Default)]
struct Findings(Vec<String>);

impl Findings {
    fn check(&mut self, topology: &str, what: &str, result: Result<()>) {
        if let Err(e) = result {
            let line = format!("{topology}: {what}: {e:#}");
            println!("FINDING {line}");
            self.0.push(line);
        } else {
            println!("ok {topology}: {what}");
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Topology {
    SinglePlayer,
    Lan,
    Internet,
}

impl Topology {
    fn label(self) -> &'static str {
        match self {
            Topology::SinglePlayer => "single player",
            Topology::Lan => "LAN",
            Topology::Internet => "internet",
        }
    }
    fn radio(self) -> &'static str {
        match self {
            Topology::SinglePlayer => "SM_OptSinglePlayer",
            Topology::Lan => "SM_OptLAN",
            Topology::Internet => "SM_OptInternet",
        }
    }
}

// ----------------------------------------------------------- the steps

/// Start Game: pick the type and `map`, type the name and passwords,
/// set the chat length in Advanced Config, launch.
fn start_game(host: &mut App, t: Topology, map: &str) -> Result<()> {
    open(host, ScreenId::StartMission);
    click(host, ScreenId::StartMission, t.radio())?;
    pick_row(host, ScreenId::StartMission, "SM_missionList", map)?;
    if t != Topology::SinglePlayer {
        fill(
            host,
            ScreenId::StartMission,
            "TxtServerName",
            &format!("Topo {}", t.label()),
        )?;
        fill(
            host,
            ScreenId::StartMission,
            "TxtServerAdminPasswordCRAP",
            "adminpw",
        )?;
        pick(host, ScreenId::StartMission, "SM_PlayerCountMenu", "4")?;
    }
    click(
        host,
        ScreenId::StartMission,
        "canvas.pushDialog(ServerconfigGui);",
    )?;
    fill(host, ScreenId::ServerConfig, "AdminOption_maxchatlen", "40")?;
    click(
        host,
        ScreenId::ServerConfig,
        "canvas.popDialog(ServerConfigGui);",
    )?;
    click(host, ScreenId::StartMission, "SM_StartMission();")?;
    until(
        &mut [&mut *host],
        "the host in game",
        Duration::from_secs(120),
        |a| in_game(a[0]),
    )?;
    // A LAN or internet host may first be asked about Windows Firewall.
    settle(&mut [&mut *host], Duration::from_secs(3))?;
    let ConnectionState::InGame {
        server_name,
        max_players,
        single_player,
        ..
    } = host.ui.core.conn.clone()
    else {
        unreachable!()
    };
    ensure!(
        single_player == (t == Topology::SinglePlayer),
        "single_player is {single_player}"
    );
    if t != Topology::SinglePlayer {
        ensure!(
            server_name == format!("Topo {}", t.label()),
            "server name {server_name:?}"
        );
        ensure!(max_players == 4, "max players {max_players}");
    }
    Ok(())
}

/// Say something through the chat box; returns once the server echoed it.
fn say(apps: &mut [&mut App], who: usize, text: &str, shown: &str) -> Result<()> {
    open(apps[who], ScreenId::MessageInput(ChatChannel::Say));
    type_text(apps[who], text);
    key(apps[who], Key::Return);
    let shown = shown.to_string();
    until(
        apps,
        &format!("chat {shown:?}"),
        Duration::from_secs(10),
        |a| a.iter().all(|app| chat_has(app, &shown)),
    )
}

/// A 60-character message is cut to Advanced Config's 40.
fn chat_length(apps: &mut [&mut App], who: usize, limit: usize) -> Result<()> {
    let long: String = (0..60).map(|i| char::from(b'a' + (i % 26) as u8)).collect();
    let tag = format!("Q{who}{limit}");
    let text = format!("{tag}{}", &long[tag.len()..]);
    say(apps, who, &text, &text[..limit])?;
    let line = apps[who]
        .network_view()
        .unwrap()
        .chat
        .iter()
        .rev()
        .find(|l| l.text.contains(&tag))
        .unwrap()
        .text
        .clone();
    ensure!(
        !line.contains(&text[..limit + 1]),
        "the server kept more than {limit} characters: {line:?}"
    );
    Ok(())
}

/// Brick selector: search "2x4", Enter puts it in the hand; plant it at
/// the player's feet.
fn plant_searched_brick(app: &mut App) -> Result<u64> {
    let before: Vec<u64> = bricks(app).iter().map(|(id, _)| *id).collect();
    open(app, ScreenId::BrickSelector);
    fill(app, ScreenId::BrickSelector, "BSD_Search", "2x4")?;
    key(app, Key::Return);
    until(
        &mut [&mut *app],
        "the searched brick in hand",
        Duration::from_secs(5),
        |a| a[0].pending_requests() == 0 && !a[0].ui.is_open(ScreenId::BrickSelector),
    )?;
    let e = eye(app);
    aim(app, e + glam::Vec3::new(0.0, -2.0, -1.5));
    fire(app)?;
    let ghost = app
        .building()
        .and_then(|b| b.ghost())
        .with_context(|| {
            format!(
                "Fire did not deploy a ghost brick; look ({}, {}), eye {:?}",
                app.controls.yaw,
                app.controls.pitch,
                eye(app)
            )
        })?
        .clone();
    ensure!(
        format!("{:?}", ghost.definition).contains("2x4"),
        "the ghost is {:?}, not the searched 2x4",
        ghost.definition
    );
    app.ui.core.request(UiAction::Game(GameAction::PlantBrick));
    let _ = app.pump();
    until(&mut [&mut *app], "the plant", Duration::from_secs(8), |a| {
        bricks(a[0]).len() > before.len()
    })?;
    app.ui.core.request(UiAction::Game(GameAction::CancelBrick));
    let _ = app.pump();
    Ok(bricks(app)
        .into_iter()
        .map(|(id, _)| id)
        .find(|id| !before.contains(id))
        .unwrap())
}

/// Close message boxes with their OK button, as a player reads them.
fn close_messages(app: &mut App) -> Result<()> {
    for _ in 0..8 {
        if app.ui.top_id() != ScreenId::MessageBox {
            return Ok(());
        }
        let v = view(app, ScreenId::MessageBox)?;
        let text: Vec<String> = v
            .walk()
            .map(|n| v.text_of(n))
            .filter(|t| !t.trim().is_empty())
            .collect();
        println!("message box: {text:?}");
        let ok = ["OK", "Ok", "Close"]
            .into_iter()
            .find(|label| find(v, label).is_some_and(|n| v.is_shown(n)))
            .with_context(|| format!("a message box without OK: {text:?}"))?;
        click(app, ScreenId::MessageBox, ok)?;
    }
    Ok(())
}

/// Aim the wrench at `brick` and fire it.
fn wrench(app: &mut App, brick: u64) -> Result<()> {
    let centre = bricks(app)
        .into_iter()
        .find(|(id, _)| *id == brick)
        .context("the brick is gone")?
        .1
        .position;
    // The wrench swings for half a second (wrenchImage's Fire state); a
    // click during a swing does nothing, in v20 too.
    settle(&mut [&mut *app], Duration::from_millis(600))?;
    // A click reaches the world only once every dialog is closed.
    close_messages(app)?;
    until(
        &mut [&mut *app],
        "nothing but play on screen",
        Duration::from_secs(5),
        |a| a[0].ui.stack() == [ScreenId::Play] && a[0].pending_requests() == 0,
    )?;
    // Picking the slot already in hand would put the wrench away.
    let held = app
        .network_view()
        .and_then(|v| v.tools.get(&v.owner)?.selected);
    if held != Some(1) {
        app.ui.core.request(UiAction::UseTool { slot: 1 });
        let _ = app.pump();
        until(
            &mut [&mut *app],
            "the wrench in hand",
            Duration::from_secs(5),
            |a| {
                a[0].network_view()
                    .and_then(|v| v.tools.get(&v.owner)?.selected)
                    == Some(1)
            },
        )?;
    }
    aim(app, glam::Vec3::from(centre));
    fire(app)?;
    Ok(())
}

/// Wrench a brick through the screen: type its name, send.
fn name_brick(apps: &mut [&mut App], who: usize, brick: u64, name: &str) -> Result<()> {
    wrench(apps[who], brick)?;
    until(apps, "the wrench dialog", Duration::from_secs(8), |a| {
        a[who].ui.is_open(ScreenId::Wrench(WrenchVariant::Normal))
    })?;
    let screen = ScreenId::Wrench(WrenchVariant::Normal);
    fill(apps[who], screen, "Wrench_Name", name)?;
    click(apps[who], screen, "wrenchDlg.send();")?;
    let name = name.to_string();
    until(
        apps,
        "the brick named on every client",
        Duration::from_secs(8),
        |a| {
            a.iter().all(|app| {
                bricks(app)
                    .iter()
                    .any(|(id, b)| *id == brick && b.name.as_deref() == Some(&name))
            })
        },
    )
}

/// Build an onActivate → Self → setColor row in the events dialog.
fn add_event(apps: &mut [&mut App], who: usize, brick: u64) -> Result<()> {
    wrench(apps[who], brick)?;
    let screen = ScreenId::Wrench(WrenchVariant::Normal);
    until(apps, "the wrench dialog", Duration::from_secs(8), |a| {
        a[who].ui.is_open(screen)
    })?;
    click(apps[who], screen, "canvas.pushDialog(WrenchEventsDlg);")?;
    until(apps, "the events dialog", Duration::from_secs(8), |a| {
        a[who].ui.is_open(ScreenId::WrenchEvents)
    })?;
    for (popup, entry) in [
        ("WrenchEvent_0_input", "onActivate"),
        ("WrenchEvent_0_target", "Self"),
        ("WrenchEvent_0_output", "setColor"),
    ] {
        pick(apps[who], ScreenId::WrenchEvents, popup, entry)?;
    }
    click(apps[who], ScreenId::WrenchEvents, "wrenchEventsDlg.send();")?;
    until(
        apps,
        "the event on every client",
        Duration::from_secs(8),
        |a| {
            a.iter().all(|app| {
                bricks(app)
                    .iter()
                    .any(|(id, b)| *id == brick && b.events.len() == 1)
            })
        },
    )
}

fn save_bricks(app: &mut App, name: &str) -> Result<()> {
    open(app, ScreenId::SaveBricks);
    until(
        &mut [&mut *app],
        "the save list",
        Duration::from_secs(5),
        |a| a[0].pending_requests() == 0,
    )?;
    fill(app, ScreenId::SaveBricks, "SaveBricks_FileName", name)?;
    click(app, ScreenId::SaveBricks, "SaveBricks_Save();")?;
    let file = format!("{name}.world.json");
    until(&mut [&mut *app], "the save", Duration::from_secs(10), |a| {
        a[0].pending_requests() == 0
            && a[0].ui.core.save_files.iter().any(|f| f.name == file)
            && !a[0].ui.is_open(ScreenId::SaveBricks)
    })
}

fn load_bricks(apps: &mut [&mut App], who: usize, name: &str) -> Result<()> {
    open(apps[who], ScreenId::LoadBricks);
    let who_ = who;
    until(apps, "the save list", Duration::from_secs(5), move |a| {
        a[who_].pending_requests() == 0
    })?;
    pick_row(apps[who], ScreenId::LoadBricks, "LoadBricks_FileList", name)?;
    click(
        apps[who],
        ScreenId::LoadBricks,
        "LoadBricks_ClickLoadButton();",
    )?;
    Ok(())
}

/// The whole run for one topology. `guest` joins when present.
fn topology(
    t: Topology,
    map: &str,
    port: u16,
    host: &mut App,
    guest: Option<&mut App>,
    found: &mut Findings,
) -> Result<()> {
    let label = t.label();
    start_game(host, t, map).with_context(|| format!("{label}: Start Game"))?;
    found.check(
        label,
        "Advanced Config's chat length reaches the server",
        chat_length(&mut [&mut *host], 0, 40),
    );
    let hosted = plant_searched_brick(host);
    found.check(
        label,
        "brick selector search, then plant",
        hosted
            .as_ref()
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!("{e:#}")),
    );
    let Ok(brick) = hosted else { return Ok(()) };
    found.check(
        label,
        "wrench name from the wrench dialog",
        name_brick(&mut [&mut *host], 0, brick, "HostBrick"),
    );
    found.check(
        label,
        "an event built in the events dialog",
        add_event(&mut [&mut *host], 0, brick),
    );
    let save = format!("Topo {label}");
    found.check(label, "Save Bricks", save_bricks(host, &save));
    // Undo the plant, then Load Bricks brings it back with its name.
    host.ui.core.request(UiAction::Game(GameAction::UndoBrick));
    let loaded = load_bricks(&mut [&mut *host], 0, &save).and_then(|_| {
        until(
            &mut [&mut *host],
            "the loaded brick",
            Duration::from_secs(15),
            |a| {
                bricks(a[0])
                    .iter()
                    .any(|(_, b)| b.name.as_deref() == Some("HostBrick"))
                    && a[0].pending_requests() == 0
            },
        )
    });
    found.check(label, "Load Bricks restores the saved brick", loaded);
    let Some(guest) = guest else {
        // Single player: the host is the administrator.
        found.check(
            label,
            "console /timescale as the host",
            attempt(|| {
                open(host, ScreenId::Console);
                type_text(host, "/timescale 2");
                key(host, Key::Return);
                host.ui.core.pop(ScreenId::Console);
                until(
                    &mut [&mut *host],
                    "time scale 2",
                    Duration::from_secs(5),
                    |a| {
                        a[0].network_view()
                            .is_some_and(|v| (v.time_scale - 2.0).abs() < 0.01)
                    },
                )
            }),
        );
        found.check(
            label,
            "mini-game created from the Create Mini-Game screen",
            create_minigame(&mut [&mut *host], 0, "Solo Game"),
        );
        return Ok(());
    };
    // A guest renames through Avatar, then joins through Connect to IP.
    found.check(
        label,
        "guest name from the Avatar screen",
        attempt(|| {
            open(guest, ScreenId::Avatar);
            fill(guest, ScreenId::Avatar, "Avatar_Name", "Guesty")?;
            click(guest, ScreenId::Avatar, "Avatar_Done();")
        }),
    );
    open(guest, ScreenId::ManualJoin);
    fill(
        guest,
        ScreenId::ManualJoin,
        "MJ_txtIP",
        &format!("127.0.0.1:{port}"),
    )?;
    click(guest, ScreenId::ManualJoin, "MJ_connect();")?;
    until(
        &mut [&mut *host, &mut *guest],
        "the guest in game",
        Duration::from_secs(120),
        |a| in_game(a[1]) && names(a[0]).contains(&"Guesty".to_string()),
    )
    .with_context(|| format!("{label}: joining"))?;
    found.check(
        label,
        "the guest sees the typed server name and size",
        attempt(|| match &guest.ui.core.conn {
            ConnectionState::InGame {
                server_name,
                max_players: 4,
                ..
            } if *server_name == format!("Topo {label}") => Ok(()),
            other => bail!("{other:?}"),
        }),
    );
    found.check(
        label,
        "the guest's chat is cut to the host's length",
        chat_length(&mut [&mut *host, &mut *guest], 1, 40),
    );
    // Bring the guest over with a chat command, then try the host's brick.
    found.check(
        label,
        "/fetch from the chat box",
        attempt(|| {
            say(&mut [&mut *host, &mut *guest], 0, "/fetch Guesty", "")?;
            let target = eye(host);
            until(
                &mut [&mut *host, &mut *guest],
                "the guest fetched",
                Duration::from_secs(10),
                |a| eye(a[1]).distance(target) < 3.0,
            )
        }),
    );
    let host_brick = bricks(host)
        .into_iter()
        .find(|(_, b)| b.name.as_deref() == Some("HostBrick"))
        .map(|(id, _)| id);
    if let Some(brick) = host_brick {
        wrench(guest, brick)?;
        settle(&mut [&mut *host, &mut *guest], Duration::from_secs(2))?;
        let opened = guest.ui.is_open(ScreenId::Wrench(WrenchVariant::Normal));
        found.check(label, "the guest's wrench on the host's brick", match (t, opened) {
            // v20 getTrustLevel: everyone on a LAN server is trusted.
            (Topology::Lan, true) | (Topology::Internet, false) => Ok(()),
            (_, opened) => Err(anyhow::anyhow!(
                "the wrench dialog opened: {opened}; sounds {:?}; guest eye {:?}, host eye {:?}; prints {:?}",
                guest.audio_requests().iter().filter(|(k, _)| k.contains("wrench")).collect::<Vec<_>>(),
                eye(guest),
                eye(host),
                guest.ui.core.center_print
            )),
        });
        if opened {
            guest.ui.core.pop(ScreenId::Wrench(WrenchVariant::Normal));
            guest.ui.core.request(UiAction::CancelWrench { brick });
        }
        if t == Topology::Internet {
            found.check(label, "full trust from the Player List", trust(host, guest));
            found.check(
                label,
                "the guest's wrench after trust",
                name_brick(&mut [&mut *host, &mut *guest], 1, brick, "TrustedEdit"),
            );
        } else {
            found.check(
                label,
                "the guest renames the host's brick",
                name_brick(&mut [&mut *host, &mut *guest], 1, brick, "GuestEdit"),
            );
        }
    }
    found.check(
        label,
        "the guest's mini-game reaches the host",
        create_minigame(&mut [&mut *host, &mut *guest], 1, "Guest Game"),
    );
    found.check(
        label,
        "the host joins it from Join Mini-Game",
        join_minigame(&mut [&mut *host, &mut *guest], 0, "Guest Game"),
    );
    // v20 SaveBricks_Save: any player saves the bricks they see to their
    // own saves; LoadBricks_ClickLoadButton uploads a guest's own file, and
    // serverCmdInitUploadHandshake takes it only from an administrator.
    let copy = format!("Guest copy {label}");
    found.check(label, "Save Bricks as a guest", save_bricks(guest, &copy));
    // Administrator commands: refused, then allowed after logging in.
    found.check(
        label,
        "the guest's console /timescale before login",
        attempt(|| {
            open(guest, ScreenId::Console);
            type_text(guest, "/timescale 3");
            key(guest, Key::Return);
            guest.ui.core.pop(ScreenId::Console);
            settle(&mut [&mut *host, &mut *guest], Duration::from_secs(2))?;
            let scale = host.network_view().unwrap().time_scale;
            ensure!((scale - 3.0).abs() > 0.01, "a guest changed the time scale");
            Ok(())
        }),
    );
    found.check(
        label,
        "admin login from the password box",
        attempt(|| {
            open(guest, ScreenId::AdminLogin);
            until(
                &mut [&mut *host, &mut *guest],
                "the password box",
                Duration::from_secs(5),
                |a| {
                    a[1].ui.screen(ScreenId::AdminLogin).is_some_and(|s| {
                        let v = s.view();
                        v.id("txtAdminPass").is_some_and(|n| v.node(n).state.active)
                    })
                },
            )?;
            fill(guest, ScreenId::AdminLogin, "txtAdminPass", "adminpw")?;
            key(guest, Key::Return);
            until(
                &mut [&mut *host, &mut *guest],
                "the guest an administrator",
                Duration::from_secs(10),
                |a| a[1].network_view().is_some_and(|v| v.administrator),
            )
        }),
    );
    found.check(
        label,
        "Host options' chat length reaches the server",
        attempt(|| {
            open(host, ScreenId::AdminOptions);
            until(
                &mut [&mut *host, &mut *guest],
                "host options",
                Duration::from_secs(5),
                |a| {
                    a[0].ui.screen(ScreenId::AdminOptions).is_some_and(|s| {
                        let v = s.view();
                        v.id("AdminOption_maxchatlen")
                            .is_some_and(|n| v.node(n).state.active)
                    })
                },
            )?;
            fill(host, ScreenId::AdminOptions, "AdminOption_maxchatlen", "30")?;
            click(
                host,
                ScreenId::AdminOptions,
                "canvas.popDialog(ServerConfigGui);",
            )?;
            settle(&mut [&mut *host, &mut *guest], Duration::from_secs(1))?;
            chat_length(&mut [&mut *host, &mut *guest], 1, 30)
        }),
    );
    found.check(
        label,
        "Load Bricks as an administrator guest",
        attempt(|| {
            open(host, ScreenId::AdminBricks);
            until(
                &mut [&mut *host, &mut *guest],
                "the brick owners list",
                Duration::from_secs(5),
                |a| a[0].pending_requests() == 0 && !a[0].ui.core.admin.groups.is_empty(),
            )?;
            click(host, ScreenId::AdminBricks, "BrickManGui.clickClearAll();")?;
            click(host, ScreenId::AdminConfirm, "MessageBoxYesNoDlg.yesCallback();")?;
            until(
                &mut [&mut *host, &mut *guest],
                "every brick cleared",
                Duration::from_secs(10),
                |a| a.iter().all(|app| bricks(app).is_empty()),
            )?;
            host.ui.core.pop(ScreenId::AdminBricks);
            load_bricks(&mut [&mut *host, &mut *guest], 1, &copy)?;
            until(
                &mut [&mut *host, &mut *guest],
                "the guest's bricks on the host",
                Duration::from_secs(20),
                |a| !bricks(a[0]).is_empty() && a[0].pending_requests() == 0,
            )
        }),
    );
    guest.ui.core.request(UiAction::Disconnect);
    let _ = guest.pump();
    Ok(())
}

fn trust(host: &mut App, guest: &mut App) -> Result<()> {
    open(host, ScreenId::PlayerList);
    pick_row(host, ScreenId::PlayerList, "NPL_List", "Guesty")?;
    click(
        host,
        ScreenId::PlayerList,
        "NewPlayerListGui.clickTrustInviteFull();",
    )?;
    until(
        &mut [&mut *host, &mut *guest],
        "the trust invitation",
        Duration::from_secs(10),
        |a| a[1].ui.is_open(ScreenId::TrustInvitation),
    )?;
    click(
        guest,
        ScreenId::TrustInvitation,
        "TrustInviteGui.clickAccept();",
    )?;
    host.ui.core.pop(ScreenId::PlayerList);
    until(
        &mut [&mut *host, &mut *guest],
        "mutual full trust",
        Duration::from_secs(10),
        |a| {
            let row = |app: &App, name: &str| {
                app.ui
                    .core
                    .players
                    .iter()
                    .find(|p| p.name == name)
                    .map(|p| p.trust.clone())
            };
            row(a[0], "Guesty").as_deref() == Some("Full")
        },
    )
}

fn create_minigame(apps: &mut [&mut App], who: usize, title: &str) -> Result<()> {
    open(apps[who], ScreenId::MiniGames);
    until(apps, "the mini-game list", Duration::from_secs(5), |a| {
        a[who].pending_requests() == 0
    })?;
    click(
        apps[who],
        ScreenId::MiniGames,
        "JoinMiniGameGui.clickCreate();",
    )?;
    fill(
        apps[who],
        ScreenId::MiniGameSettings,
        "$MiniGame::Title",
        title,
    )?;
    click(
        apps[who],
        ScreenId::MiniGameSettings,
        "CreateMiniGameGui.clickCreate();",
    )?;
    let title = title.to_string();
    until(
        apps,
        "the mini-game on every client",
        Duration::from_secs(10),
        |a| {
            a.iter()
                .all(|app| app.ui.core.minigames.games.iter().any(|g| g.title == title))
        },
    )
}

fn join_minigame(apps: &mut [&mut App], who: usize, title: &str) -> Result<()> {
    open(apps[who], ScreenId::MiniGames);
    until(apps, "the mini-game list", Duration::from_secs(5), |a| {
        a[who].pending_requests() == 0
    })?;
    pick_row(apps[who], ScreenId::MiniGames, "JMG_List", title)?;
    click(
        apps[who],
        ScreenId::MiniGames,
        "JoinMiniGameGui.clickJoin();",
    )?;
    until(apps, "membership", Duration::from_secs(10), |a| {
        a[who].ui.core.minigames.active_game.is_some()
    })
}

fn screens_reach_the_server_in_every_topology(f: &ContentRoot) -> Result<()> {
    let _turn = support::gpu::turn()?;
    let port = use_test_ports();
    let (host_state, guest_state) = (f.state()?, f.state()?);
    let mut host = load(&f.root, host_state.path(), "Hosty")?;
    let mut guest = load(&f.root, guest_state.path(), "Blockhead")?;
    let mut found = Findings::default();
    for t in [Topology::SinglePlayer, Topology::Lan, Topology::Internet] {
        let joiner = (t != Topology::SinglePlayer).then_some(&mut guest);
        if let Err(e) = topology(t, &f.map.1, port, &mut host, joiner, &mut found) {
            found.0.push(format!("{}: stopped: {e:#}", t.label()));
        }
        host.ui.core.request(UiAction::Disconnect);
        let _ = host.pump();
        until(
            &mut [&mut host, &mut guest],
            "both back at the menu",
            Duration::from_secs(20),
            |a| a.iter().all(|app| !in_game(app)),
        )?;
        // The closed server lets go of its port a moment after the
        // players are back at the menu; the next topology hosts on it.
        until(
            &mut [&mut host, &mut guest],
            "the host port free",
            Duration::from_secs(10),
            |_| std::net::UdpSocket::bind(("0.0.0.0", port)).is_ok(),
        )?;
        for app in [&mut host, &mut guest] {
            for screen in app.ui.stack() {
                if screen != ScreenId::MainMenu {
                    app.ui.core.pop(screen);
                }
            }
            app.ui.update(0);
        }
    }
    ensure!(found.0.is_empty(), "\n{}", found.0.join("\n"));
    Ok(())
}
