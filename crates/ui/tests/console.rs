//! The `~` console (ConsoleDlg): toggling, entry, history, completion and
//! routing. Tests send data to the UI only; no OS window/input is created.
use bri_ui::{
    api::*,
    binds::Platform,
    geom::Rect,
    input::*,
    pack::Pack,
    schema::{Control, UiPack},
    screens::{ScreenId, ctrl},
    ui::{Ui, UiConfig},
};
use std::{path::PathBuf, rc::Rc};

/// ConsoleDlg as authored in v20 (allClientGuis.gui:1).
fn console_layout() -> Control {
    let mut log = ctrl("GuiConsole", "GuiConsoleProfile", Rect::new(1, 1, 8, 2));
    log.name = Some("testArrayCtrl".into());
    let mut scroll = ctrl("GuiScrollCtrl", "GuiScrollProfile", Rect::new(0, 0, 640, 353));
    scroll.children = vec![log];
    let mut entry = ctrl("GuiConsoleEditCtrl", "GuiTextEditProfile", Rect::new(0, 352, 640, 18));
    entry.name = Some("ConsoleEntry".into());
    let mut window = ctrl("GuiWindowCtrl", "GuiWindowProfile", Rect::new(0, 0, 640, 370));
    window.text = Some("Console".into());
    window.children = vec![scroll, entry];
    let mut dlg = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480));
    dlg.children = vec![window];
    dlg
}

fn ui() -> Ui {
    let mut p = UiPack::default();
    for name in ["MainMenuGui", "PlayGui", "LoadingGui", "MessageBoxOKDlg"] {
        p.layouts.insert(
            name.into(),
            ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480)),
        );
    }
    p.layouts.insert("ConsoleDlg".into(), console_layout());
    let mut u = Ui::new(
        Rc::new(Pack::from_parts(p, PathBuf::new())),
        UiConfig {
            size: (1280, 960),
            scale: Some(2.0),
            platform: Platform::Windows,
        },
        Settings {
            binds: Some(vec![]),
            mouse_type: 2,
            ..Default::default()
        },
    );
    u.core
        .globals
        .bind(BindInput::Key(Chord::plain(Key::Tilde)), "toggleConsole");
    u.set_console_commands(vec![bri_console::CommandInfo {
        name: "stats".into(),
        usage: String::new(),
        help: "Host statistics.".into(),
    }]);
    u
}

fn down(u: &mut Ui, key: Key) {
    u.handle_input(InputEvent::KeyDown {
        key,
        mods: Modifiers::NONE,
        repeat: false,
    });
    u.handle_input(InputEvent::KeyUp {
        key,
        mods: Modifiers::NONE,
    });
}

/// `~` as the platform delivers it: key down, then the character it types.
fn tilde(u: &mut Ui) {
    u.handle_input(InputEvent::KeyDown {
        key: Key::Tilde,
        mods: Modifiers::NONE,
        repeat: false,
    });
    u.handle_input(InputEvent::Char('`'));
    u.update(150);
}

fn type_text(u: &mut Ui, text: &str) {
    for c in text.chars() {
        u.handle_input(InputEvent::Char(c));
    }
}

fn entry(u: &Ui) -> String {
    let s = u.screen(ScreenId::Console).expect("console open");
    let v = s.view();
    v.edit_text(v.id("ConsoleEntry").unwrap())
}

fn submit(u: &mut Ui, text: &str) {
    type_text(u, text);
    down(u, Key::Return);
    u.update(16);
}

fn log_has(text: &str) -> bool {
    bri_console::log::lines().iter().any(|l| l.text == text)
}

fn actions(u: &mut Ui) -> Vec<UiAction> {
    u.drain_actions().into_iter().map(|(_, a)| a).collect()
}

#[test]
fn tilde_toggles_the_console_and_releases_the_cursor() {
    let mut u = ui();
    u.apply(UiUpdate::Connection(ConnectionState::InGame {
        server_name: "Test".into(),
        max_players: 8,
        local: true,
        single_player: false,
        admin: true,
    }));
    assert!(!u.cursor_visible());
    tilde(&mut u);
    assert_eq!(u.top_id(), ScreenId::Console);
    assert!(u.cursor_visible());
    assert_eq!(entry(&u), "", "the ~ that opened the console is not typed");
    // A second press inside toggleConsole's 100 ms window is ignored.
    u.handle_input(InputEvent::KeyDown {
        key: Key::Tilde,
        mods: Modifiers::NONE,
        repeat: false,
    });
    u.update(50);
    u.handle_input(InputEvent::KeyDown {
        key: Key::Tilde,
        mods: Modifiers::NONE,
        repeat: false,
    });
    assert!(!u.is_open(ScreenId::Console));
    assert!(!u.cursor_visible());
}

#[test]
fn entry_echoes_runs_cvars_and_forwards_host_commands() {
    let mut u = ui();
    tilde(&mut u);
    actions(&mut u);
    submit(&mut u, "volume 0.25; stats");
    assert!(log_has("==>volume 0.25; stats"));
    assert!(log_has("volume = \"0.25\""));
    assert_eq!(u.core.prefs.get("$pref::Audio::masterVolume"), Some("0.25"));
    let sent = actions(&mut u);
    assert!(
        sent.iter()
            .any(|a| matches!(a, UiAction::SaveSettings(s) if s.prefs.get("$pref::Audio::masterVolume").map(String::as_str) == Some("0.25")))
    );
    assert!(sent.contains(&UiAction::Console { line: "stats".into() }));
    assert_eq!(entry(&u), "");
    // Unknown names are errors, never evaluated.
    submit(&mut u, "schedule(1, 0, quit)");
    assert!(actions(&mut u).is_empty());
}

#[test]
fn history_completion_and_secrets() {
    let mut u = ui();
    tilde(&mut u);
    submit(&mut u, "echo first");
    submit(&mut u, "echo second");
    submit(&mut u, "connect 10.0.0.2 hunter2");
    assert!(log_has("==>connect 10.0.0.2 ***"));
    assert!(!bri_console::log::lines().iter().any(|l| l.text.contains("hunter2")));
    assert!(actions(&mut u).contains(&UiAction::JoinServer {
        address: "10.0.0.2".into(),
        password: "hunter2".into()
    }));
    // The console stays open when the connection changes the content screen.
    assert!(u.is_open(ScreenId::Console));
    type_text(&mut u, "draft");
    down(&mut u, Key::Up);
    assert_eq!(entry(&u), "echo second", "passwords never enter history");
    down(&mut u, Key::Up);
    assert_eq!(entry(&u), "echo first");
    down(&mut u, Key::Down);
    down(&mut u, Key::Down);
    assert_eq!(entry(&u), "draft");
    down(&mut u, Key::Escape);
    tilde(&mut u);
    type_text(&mut u, "vol");
    down(&mut u, Key::Tab);
    assert_eq!(entry(&u), "volume ");
}

#[test]
fn slash_commands_and_admin_go_through_host_requests() {
    let mut u = ui();
    tilde(&mut u);
    submit(&mut u, "/sit");
    assert!(log_has("Not in a game."));
    u.apply(UiUpdate::Connection(ConnectionState::InGame {
        server_name: "Test".into(),
        max_players: 8,
        local: false,
        single_player: false,
        admin: false,
    }));
    assert!(u.is_open(ScreenId::Console));
    actions(&mut u);
    submit(&mut u, "/timescale 2; kick Nobody");
    let sent = actions(&mut u);
    assert_eq!(
        sent,
        [UiAction::ChatCommand {
            name: "timescale".into(),
            args: vec!["2".into()]
        }]
    );
    assert!(log_has("Administration status not received yet; try again."));
}
