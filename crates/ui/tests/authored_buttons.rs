//! Every visible authored button on the menu screens a player reaches does
//! something: none answers "Interface under construction". Runs against the
//! converted v20 UI pack, and skips when that local content is absent.
use bri_ui::{
    api::Settings,
    binds::Platform,
    pack::Pack,
    screens::ScreenId,
    ui::{StackCmd, Ui, UiConfig},
    view::{EventKind, View, ViewEvent},
};
use std::{path::Path, rc::Rc};

const SCREENS: [ScreenId; 19] = [
    ScreenId::MainMenu,
    ScreenId::StartMission,
    ScreenId::JoinServer,
    ScreenId::ManualJoin,
    ScreenId::EscapeMenu,
    ScreenId::Options,
    ScreenId::DefaultControls,
    ScreenId::About,
    ScreenId::Avatar,
    ScreenId::SaveBricks,
    ScreenId::LoadBricks,
    ScreenId::PlayerList,
    ScreenId::MiniGames,
    ScreenId::MiniGameSettings,
    ScreenId::BrickSelector,
    ScreenId::Admin,
    ScreenId::AdminBricks,
    ScreenId::AdminUnban,
    ScreenId::AdminMaps,
];
const UNBUILT: &str = "Interface under construction";

fn visible(v: &View, mut n: usize) -> bool {
    loop {
        let node = v.node(n);
        if !node.state.visible {
            return false;
        }
        match node.parent {
            Some(p) => n = p,
            None => return true,
        }
    }
}

fn ui(pack: &Rc<Pack>) -> Ui {
    Ui::new(
        pack.clone(),
        UiConfig {
            size: (1024, 768),
            scale: Some(1.0),
            platform: Platform::Windows,
        },
        Settings {
            binds: Some(vec![]),
            ..Default::default()
        },
    )
}

/// Commands on `screen` whose click answers "Interface under construction".
fn unbuilt(pack: &Rc<Pack>, screen: ScreenId) -> Vec<String> {
    let mut probe = ui(pack);
    probe.core.push(screen);
    probe.update(0);
    let Some(s) = probe.screen(screen) else {
        return vec![];
    };
    let v = s.view();
    let targets: Vec<(usize, String)> = v
        .walk()
        .filter(|&n| visible(v, n) && v.node(n).state.active)
        .filter_map(|n| v.node(n).ctrl.command.clone().map(|c| (n, c)))
        .collect();
    let mut out = vec![];
    for (node, command) in targets {
        let mut u = ui(pack);
        u.core.push(screen);
        u.update(0);
        u.core.cmds.clear();
        let i = u.dialogs.iter().rposition(|s| s.id() == screen).unwrap();
        let (dialogs, core) = (&mut u.dialogs, &mut u.core);
        dialogs[i].on_event(
            &ViewEvent {
                node,
                kind: EventKind::Click,
            },
            core,
        );
        if u.core
            .cmds
            .iter()
            .any(|c| matches!(c, StackCmd::Message(m) if m.title == UNBUILT))
        {
            out.push(command);
        }
    }
    out
}

#[test]
fn menu_buttons_are_all_built() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/ui-pack-003");
    if !dir.join("ui-pack.json").exists() {
        eprintln!("skipped: ui-pack-003 is not converted on this machine");
        return;
    }
    let pack = Rc::new(Pack::load(&dir).unwrap());
    let mut all = vec![];
    for screen in SCREENS {
        for command in unbuilt(&pack, screen) {
            all.push(format!("{screen:?}: {command}"));
        }
    }
    assert!(all.is_empty(), "unbuilt buttons:\n{}", all.join("\n"));
}

/// Diagnostic for audits: buttons whose click changes nothing visible and
/// asks the game for nothing. `cargo test -p bri-ui --test authored_buttons
/// -- --ignored --nocapture` lists them.
#[test]
#[ignore = "diagnostic listing, not a pass/fail check"]
fn list_inert_buttons() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/ui-pack-003");
    let Ok(pack) = Pack::load(&dir) else { return };
    let pack = Rc::new(pack);
    for screen in SCREENS {
        let mut probe = ui(&pack);
        probe.core.push(screen);
        probe.update(0);
        let Some(s) = probe.screen(screen) else {
            continue;
        };
        let v = s.view();
        let targets: Vec<(usize, String)> = v
            .walk()
            .filter(|&n| visible(v, n) && v.node(n).state.active)
            .filter_map(|n| v.node(n).ctrl.command.clone().map(|c| (n, c)))
            .collect();
        for (node, command) in targets {
            let mut u = ui(&pack);
            u.core.push(screen);
            u.update(0);
            u.core.cmds.clear();
            u.drain_actions();
            let snap = |u: &Ui| {
                let v = u.screen(screen).unwrap().view();
                v.walk()
                    .map(|n| format!("{:?}", v.node(n).state))
                    .collect::<Vec<_>>()
            };
            let before = snap(&u);
            let i = u.dialogs.iter().rposition(|s| s.id() == screen).unwrap();
            let (dialogs, core) = (&mut u.dialogs, &mut u.core);
            dialogs[i].on_event(
                &ViewEvent {
                    node,
                    kind: EventKind::Click,
                },
                core,
            );
            let quiet = u.core.cmds.is_empty() && u.drain_actions().is_empty();
            if quiet && snap(&u) == before {
                println!("INERT {screen:?}: {command}");
            }
        }
    }
}

#[test]
fn advanced_config_saves_the_next_hosts_settings() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/ui-pack-003");
    let Ok(pack) = Pack::load(&dir) else { return };
    let mut u = ui(&Rc::new(pack));
    u.core.push(ScreenId::StartMission);
    u.update(0);
    let click = |u: &mut Ui, screen: ScreenId, command: &str| {
        let node = u
            .screen(screen)
            .unwrap()
            .view()
            .by_command(command)
            .unwrap();
        let i = u.dialogs.iter().rposition(|s| s.id() == screen).unwrap();
        let (dialogs, core) = (&mut u.dialogs, &mut u.core);
        dialogs[i].on_event(
            &ViewEvent {
                node,
                kind: EventKind::Click,
            },
            core,
        );
        u.update(0);
    };
    click(
        &mut u,
        ScreenId::StartMission,
        "canvas.pushDialog(ServerconfigGui);",
    );
    assert_eq!(u.top_id(), ScreenId::ServerConfig);
    let i = u
        .dialogs
        .iter()
        .rposition(|s| s.id() == ScreenId::ServerConfig)
        .unwrap();
    let v = u.dialogs[i].view_mut();
    let n = v.id("AdminOption_maxchatlen").unwrap();
    assert_eq!(v.edit_text(n), "120");
    v.set_text(n, "64");
    click(
        &mut u,
        ScreenId::ServerConfig,
        "canvas.popDialog(ServerConfigGui);",
    );
    assert!(u.screen(ScreenId::ServerConfig).is_none());
    assert_eq!(u.core.prefs.get("$Pref::Server::MaxChatLen"), Some("64"));
}

#[test]
fn credits_and_f1_open_the_help_pages() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/ui-pack-003");
    let Ok(mut pack) = Pack::load(&dir) else {
        return;
    };
    pack.data.data.help = ["0. Credits", "1. Controls"]
        .map(|name| bri_ui::schema::HelpPage {
            name: name.into(),
            text: format!("{name} text"),
        })
        .to_vec();
    let mut u = ui(&Rc::new(pack));
    u.core.push(ScreenId::MainMenu);
    u.update(0);
    let node = u
        .screen(ScreenId::MainMenu)
        .unwrap()
        .view()
        .by_command("getHelp(\"1. Credits\");")
        .unwrap();
    let i = u
        .dialogs
        .iter()
        .rposition(|s| s.id() == ScreenId::MainMenu)
        .unwrap();
    let (dialogs, core) = (&mut u.dialogs, &mut u.core);
    dialogs[i].on_event(
        &ViewEvent {
            node,
            kind: EventKind::Click,
        },
        core,
    );
    u.update(0);
    let v = u
        .screen(ScreenId::Help)
        .expect("Credits opens HelpDlg")
        .view();
    let list = v.id("HelpFileList").unwrap();
    assert_eq!(v.node(list).state.items.len(), 2);
    assert_eq!(v.selected(list), Some(0));
    assert_eq!(v.text_of(v.id("HelpText").unwrap()), "0. Credits text");
    // F1 (`contextHelp`) closes it, and opens it again.
    u.core.context_help();
    u.update(0);
    assert!(u.screen(ScreenId::Help).is_none());
    u.core.context_help();
    u.update(0);
    assert!(u.screen(ScreenId::Help).is_some());
}

#[test]
fn server_list_rows_are_drawn_without_the_profile_outline() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/ui-pack-003");
    let Ok(pack) = Pack::load(&dir) else {
        return;
    };
    let pack = Rc::new(pack);
    let glyphs = |servers: usize| {
        let mut u = ui(&pack);
        u.core.servers = (0..servers)
            .map(|i| bri_ui::api::ServerInfo {
                address: format!("10.0.0.{i}:28000"),
                name: "Maxs Server".into(),
                password: false,
                dedicated: false,
                ping_ms: Some(9),
                players: 1,
                max_players: 8,
                bricks: 0,
                map: "Bedroom".into(),
                favorite: false,
            })
            .collect();
        u.core.push(ScreenId::JoinServer);
        u.update(0);
        let v = u.screen(ScreenId::JoinServer).unwrap().view();
        let mut dl = bri_ui::draw::DrawList::new(bri_ui::geom::Rect::new(0, 0, 1024, 768));
        v.draw(&pack, &mut dl);
        dl.cmds.len()
    };
    // "Maxs Server" "9" "1" "/" "8" "0" "Bedroom": 23 glyphs. ServerListProfile's
    // doFontOutline would draw each five times.
    let row = glyphs(1) - glyphs(0);
    assert!(row < 23 * 2, "{row} draws for one row");
}

/// Max's a16 report: at a short, wide window the Graphics tab clipped its
/// menus and left an empty band. Every tab, at every window shape, keeps
/// each control inside its section, every section above Done, and the
/// dialog on screen.
#[test]
fn options_tabs_fit_short_and_wide_windows() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/ui-pack-003");
    let Ok(pack) = Pack::load(&dir) else {
        return;
    };
    let pack = Rc::new(pack);
    const CONTROLS: [&str; 7] = [
        "GuiCheckBoxCtrl",
        "GuiRadioCtrl",
        "GuiSliderCtrl",
        "GuiPopUpMenuCtrl",
        "GuiTextEditCtrl",
        "GuiTextCtrl",
        "GuiBitmapButtonCtrl",
    ];
    let mut problems = vec![];
    // v20 let Options be resized; ours keeps its fitted size, so the last
    // two try to widen and heighten it and it must not move.
    for (size, grow) in [
        ((1999, 800), (0, 0)),
        ((800, 450), (0, 0)),
        ((640, 480), (0, 0)),
        ((1920, 1080), (0, 0)),
        ((2560, 1080), (0, 0)),
        ((1920, 1080), (120, 0)),
        ((2560, 1080), (150, 30)),
    ] {
        let mut u = Ui::new(
            pack.clone(),
            UiConfig {
                size,
                scale: None,
                platform: Platform::Windows,
            },
            Settings {
                binds: Some(vec![]),
                ..Default::default()
            },
        );
        u.core.push(ScreenId::Options);
        u.update(0);
        let (w, h) = u.logical_size();
        if grow != (0, 0) {
            use bri_ui::input::{InputEvent, MouseButton};
            let v = u.screen(ScreenId::Options).unwrap().view();
            let win = v
                .walk()
                .find(|&n| v.node(n).ctrl.class == "GuiWindowCtrl")
                .unwrap();
            let r = v.node(win).rect;
            let s = u.scale();
            let at = |x: i32, y: i32| (x as f32 * s, y as f32 * s);
            let (x, y) = at(r.right() - 2, r.bottom() - 2);
            let (tx, ty) = at(r.right() - 2 + grow.0, r.bottom() - 2 + grow.1);
            u.handle_input(InputEvent::MouseMove { x, y });
            u.handle_input(InputEvent::MouseDown {
                button: MouseButton::Left,
                x,
                y,
            });
            u.handle_input(InputEvent::MouseMove { x: tx, y: ty });
            u.handle_input(InputEvent::MouseUp {
                button: MouseButton::Left,
                x: tx,
                y: ty,
            });
            u.update(0);
            let v = u.screen(ScreenId::Options).unwrap().view();
            let grown = v.node(win).rect;
            assert_eq!((grown.w, grown.h), (r.w, r.h));
        }
        for pane in ["Graphics", "Audio", "Controls", "AdvGraphics"] {
            let i = u
                .dialogs
                .iter()
                .rposition(|s| s.id() == ScreenId::Options)
                .unwrap();
            let tab = u.dialogs[i]
                .view()
                .by_command(&format!("optionsDlg.setPane({pane});"))
                .unwrap();
            let (dialogs, core) = (&mut u.dialogs, &mut u.core);
            dialogs[i].on_event(
                &ViewEvent {
                    node: tab,
                    kind: EventKind::Click,
                },
                core,
            );
            u.update(0);
            let v = u.screen(ScreenId::Options).unwrap().view();
            let done = v.by_command("Canvas.popDialog(optionsDlg);").unwrap();
            let done_top = v.node(done).rect.y;
            let pane_node = v.id(&format!("Opt{pane}Pane")).unwrap();
            // A scroll clips what it holds (the key list runs long).
            let scrolled = |mut n: usize| {
                while let Some(p) = v.node(n).parent {
                    if v.node(p).ctrl.class == "GuiScrollCtrl" {
                        return true;
                    }
                    n = p;
                }
                false
            };
            for n in v.walk().filter(|&n| visible(v, n) && !scrolled(n)) {
                let node = v.node(n);
                let r = node.rect;
                if r.x < 0 || r.y < 0 || r.right() > w || r.bottom() > h {
                    problems.push(format!("{size:?} {pane}: {:?} off screen", node.ctrl.text));
                }
                let Some(parent) = node.parent else { continue };
                let mut up = Some(parent);
                let in_pane = std::iter::from_fn(|| {
                    let at = up?;
                    up = v.node(at).parent;
                    Some(at)
                })
                .any(|a| a == pane_node);
                if in_pane && r.bottom() > done_top {
                    problems.push(format!(
                        "{size:?} {pane}: {:?} runs under Done",
                        node.ctrl.text
                    ));
                }
                let p = v.node(parent);
                if CONTROLS.contains(&node.ctrl.class.as_str())
                    && p.ctrl.class == "GuiSwatchCtrl"
                    && (r.y < p.rect.y || r.bottom() > p.rect.bottom() + 8)
                {
                    problems.push(format!(
                        "{size:?} {pane}: {} {:?} clipped by its section",
                        node.ctrl.class, node.ctrl.text
                    ));
                }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// Max: in v20 the Escape menu (any window) could be dragged by its title
/// bar. It moves with the mouse and stays on screen.
#[test]
fn windows_drag_by_their_title_bar_and_stay_on_screen() {
    use bri_ui::input::{InputEvent, MouseButton};
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/ui-pack-003");
    let Ok(pack) = Pack::load(&dir) else {
        return;
    };
    let mut u = ui(&Rc::new(pack));
    u.core.push(ScreenId::EscapeMenu);
    u.update(0);
    let window = |u: &Ui| {
        let v = u.screen(ScreenId::EscapeMenu).unwrap().view();
        let n = v
            .walk()
            .find(|&n| v.node(n).ctrl.class == "GuiWindowCtrl")
            .expect("escapeMenu has a window");
        v.node(n).rect
    };
    let start = window(&u);
    let drag = |u: &mut Ui, from: (i32, i32), to: (i32, i32)| {
        let (fx, fy) = (from.0 as f32, from.1 as f32);
        u.handle_input(InputEvent::MouseMove { x: fx, y: fy });
        u.handle_input(InputEvent::MouseDown {
            button: MouseButton::Left,
            x: fx,
            y: fy,
        });
        let (tx, ty) = (to.0 as f32, to.1 as f32);
        u.handle_input(InputEvent::MouseMove { x: tx, y: ty });
        u.handle_input(InputEvent::MouseUp {
            button: MouseButton::Left,
            x: tx,
            y: ty,
        });
    };
    let grab = (start.x + 30, start.y + 6);
    drag(&mut u, grab, (grab.0 - 100, grab.1 + 50));
    let moved = window(&u);
    assert_eq!((moved.x, moved.y), (start.x - 100, start.y + 50));
    // Pulled far past the corner, it stops at the screen's edge.
    let grab = (moved.x + 30, moved.y + 6);
    drag(&mut u, grab, (grab.0 - 5000, grab.1 - 5000));
    let edge = window(&u);
    assert_eq!((edge.x, edge.y), (0, 0));
    // Pressing inside the window, off its title bar, moves nothing.
    drag(
        &mut u,
        (edge.x + 30, edge.bottom() - 4),
        (edge.x + 300, edge.bottom() + 100),
    );
    assert_eq!(window(&u), edge);
}

#[test]
fn music_files_turns_tracks_off_for_the_next_hosted_game() {
    use bri_ui::screens::music::music_enabled;
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/ui-pack-003");
    let Ok(pack) = Pack::load(&dir) else {
        return;
    };
    let mut u = ui(&Rc::new(pack));
    u.core.music_tracks = vec!["Bass 1".into(), "Rock".into()];
    u.core.push(ScreenId::StartMission);
    u.update(0);
    let click = |u: &mut Ui, screen: ScreenId, command: &str| {
        let node = u
            .screen(screen)
            .unwrap()
            .view()
            .by_command(command)
            .unwrap();
        let i = u.dialogs.iter().rposition(|s| s.id() == screen).unwrap();
        let (dialogs, core) = (&mut u.dialogs, &mut u.core);
        dialogs[i].on_event(
            &ViewEvent {
                node,
                kind: EventKind::Click,
            },
            core,
        );
        u.update(0);
    };
    click(
        &mut u,
        ScreenId::StartMission,
        "canvas.pushDialog(MusicFilesGui);",
    );
    assert_eq!(u.top_id(), ScreenId::MusicFiles);
    click(&mut u, ScreenId::MusicFiles, "MusicFilesGui.clickNone();");
    click(
        &mut u,
        ScreenId::MusicFiles,
        "canvas.popDialog(MusicFilesGui);",
    );
    assert!(u.screen(ScreenId::MusicFiles).is_none());
    assert!(!music_enabled(&u.core.prefs, "Bass 1"));
    assert_eq!(u.core.prefs.get("$Music__Rock"), Some("-1"));
}

#[test]
fn press_up_to_repeat_chat_recalls_sent_lines() {
    use bri_ui::api::ChatChannel;
    use bri_ui::input::{InputEvent, Key, Modifiers};
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/ui-pack-003");
    let Ok(pack) = Pack::load(&dir) else {
        return;
    };
    let mut u = ui(&Rc::new(pack));
    u.core.prefs.set("$pref::Chat::ChatRepeat", "1");
    u.core.chat.remember_sent("hi");
    u.core.chat.remember_sent("there");
    u.core.push(ScreenId::MessageInput(ChatChannel::Say));
    u.update(0);
    let press = |u: &mut Ui, key: Key| {
        u.handle_input(InputEvent::KeyDown {
            key,
            mods: Modifiers::NONE,
            repeat: false,
        });
        u.handle_input(InputEvent::KeyUp {
            key,
            mods: Modifiers::NONE,
        });
        let v = u
            .screen(ScreenId::MessageInput(ChatChannel::Say))
            .unwrap()
            .view();
        v.edit_text(v.id("NMH_Type").unwrap())
    };
    assert_eq!(press(&mut u, Key::Up), "there");
    assert_eq!(press(&mut u, Key::Up), "hi");
    assert_eq!(press(&mut u, Key::Up), "hi");
    assert_eq!(press(&mut u, Key::Down), "there");
    assert_eq!(press(&mut u, Key::Down), "");
}

#[test]
fn ml_text_switches_fonts_colours_and_margins_like_the_help_pages() {
    use bri_ui::text::{MlRun, font_named, layout_ml, ml_rich_runs};
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/ui-pack-003");
    let Ok(pack) = Pack::load(&dir) else {
        return;
    };
    let (Some(small), Some(bold)) = (
        font_named(&pack, "arial_14"),
        font_named(&pack, "arial bold_20"),
    ) else {
        return;
    };
    let lines = layout_ml(
        &pack,
        &small,
        "<font:Arial Bold:20>Title\n<lmargin%:10><font:Arial:14>Press <color:0000FF>B<color:000000> now",
        300,
        bri_ui::schema::Justify::Left,
    );
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].height, bold.line_height());
    assert_eq!((lines[0].indent, lines[1].indent), (0, 30));
    // The second line carries the bold font it starts in, then switches.
    let runs = ml_rich_runs(&lines[1].text);
    assert_eq!(runs[0], MlRun::Font("arial bold_20"));
    assert!(runs.contains(&MlRun::Rgb([0, 0, 255, 255])));
    // A font the pack lacks keeps the current one.
    let odd = layout_ml(
        &pack,
        &small,
        "<font:Nope:99>x",
        300,
        bri_ui::schema::Justify::Left,
    );
    assert_eq!(ml_rich_runs(&odd[0].text), vec![MlRun::Text("x")]);
}

/// v20's resizable windows (Join Server here) grow from their right and
/// bottom edges, and never shrink below their authored size.
#[test]
fn resizable_windows_grow_from_their_edges() {
    use bri_ui::input::{InputEvent, MouseButton};
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/ui-pack-003");
    let Ok(pack) = Pack::load(&dir) else {
        return;
    };
    let mut u = ui(&Rc::new(pack));
    u.core.push(ScreenId::JoinServer);
    u.update(0);
    let window = |u: &Ui| {
        let v = u.screen(ScreenId::JoinServer).unwrap().view();
        let n = v
            .walk()
            .find(|&n| v.node(n).ctrl.class == "GuiWindowCtrl")
            .unwrap();
        v.node(n).rect
    };
    let drag = |u: &mut Ui, from: (i32, i32), to: (i32, i32)| {
        let (fx, fy, tx, ty) = (from.0 as f32, from.1 as f32, to.0 as f32, to.1 as f32);
        u.handle_input(InputEvent::MouseMove { x: fx, y: fy });
        u.handle_input(InputEvent::MouseDown {
            button: MouseButton::Left,
            x: fx,
            y: fy,
        });
        u.handle_input(InputEvent::MouseMove { x: tx, y: ty });
        u.handle_input(InputEvent::MouseUp {
            button: MouseButton::Left,
            x: tx,
            y: ty,
        });
    };
    let start = window(&u);
    let corner = (start.right() - 2, start.bottom() - 2);
    drag(&mut u, corner, (corner.0 + 120, corner.1 + 80));
    let grown = window(&u);
    assert_eq!((grown.w, grown.h), (start.w + 120, start.h + 80));
    // The list inside follows its sizing flags.
    let v = u.screen(ScreenId::JoinServer).unwrap().view();
    assert!(v.node(v.id("JS_serverList").unwrap()).rect.w > 500);
    let corner = (grown.right() - 2, grown.bottom() - 2);
    drag(&mut u, corner, (corner.0 - 900, corner.1 - 900));
    let back = window(&u);
    assert_eq!((back.w, back.h), (start.w, start.h));
}

/// v20's title bar boxes: Join Server maximizes to the screen and back;
/// Help minimizes to its title bar and back.
#[test]
fn maximize_and_minimize_boxes_toggle_the_window() {
    use bri_ui::input::{InputEvent, MouseButton};
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/ui-pack-003");
    let Ok(pack) = Pack::load(&dir) else {
        return;
    };
    let pack = Rc::new(pack);
    let window = |u: &Ui, screen: ScreenId| {
        let v = u.screen(screen).unwrap().view();
        let n = v
            .walk()
            .find(|&n| v.node(n).ctrl.class == "GuiWindowCtrl")
            .unwrap();
        v.node(n).rect
    };
    let click = |u: &mut Ui, (x, y): (i32, i32)| {
        let (x, y) = (x as f32, y as f32);
        u.handle_input(InputEvent::MouseMove { x, y });
        u.handle_input(InputEvent::MouseDown {
            button: MouseButton::Left,
            x,
            y,
        });
        u.handle_input(InputEvent::MouseUp {
            button: MouseButton::Left,
            x,
            y,
        });
    };
    // The box `slot` places from the right of the title bar.
    let boxed = |r: bri_ui::geom::Rect, slot: i32| (r.right() - 20 - slot * 18 + 8, r.y + 11);
    let mut u = ui(&pack);
    u.core.push(ScreenId::JoinServer);
    u.update(0);
    let start = window(&u, ScreenId::JoinServer);
    click(&mut u, boxed(start, 1));
    let (w, h) = u.logical_size();
    let big = window(&u, ScreenId::JoinServer);
    assert_eq!((big.x, big.y, big.w, big.h), (0, 0, w, h));
    click(&mut u, boxed(big, 1));
    assert_eq!(window(&u, ScreenId::JoinServer), start);

    let mut u = ui(&pack);
    u.core.get_help(None);
    u.update(0);
    let start = window(&u, ScreenId::Help);
    click(&mut u, boxed(start, 2));
    let small = window(&u, ScreenId::Help);
    assert!(small.h < 40, "{small:?}");
    click(&mut u, boxed(small, 2));
    assert_eq!(window(&u, ScreenId::Help), start);
}

#[test]
fn differing_save_colours_ask_to_match_or_add_them() {
    use bri_ui::api::{ColorLoad, UiAction, UiUpdate};
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/ui-pack-003");
    let Ok(pack) = Pack::load(&dir) else {
        return;
    };
    let pack = Rc::new(pack);
    for append in [true, false] {
        let mut u = ui(&pack);
        u.apply(UiUpdate::ColorWarning { append });
        u.update(0);
        assert_eq!(u.top_id(), ScreenId::LoadBricksColor);
        let view = u.screen(ScreenId::LoadBricksColor).unwrap().view();
        let shown = |command: &str| visible(view, view.by_command(command).unwrap());
        assert!(shown("ColorWarning_ClickMatch();"));
        assert!(!shown("ColorWarning_ClickReplace();"));
        assert_eq!(shown("ColorWarning_ClickAppend();"), append);
        // Add More Colors takes Replace's place, right under Nearest Match.
        let y = |command: &str| view.node(view.by_command(command).unwrap()).rect.y;
        assert_eq!(
            y("ColorWarning_ClickAppend();"),
            y("ColorWarning_ClickMatch();") + 40
        );
        let node = view.by_command("ColorWarning_ClickMatch();").unwrap();
        let i = u
            .dialogs
            .iter()
            .rposition(|s| s.id() == ScreenId::LoadBricksColor)
            .unwrap();
        let (dialogs, core) = (&mut u.dialogs, &mut u.core);
        dialogs[i].on_event(
            &ViewEvent {
                node,
                kind: EventKind::Click,
            },
            core,
        );
        u.update(0);
        assert!(u.screen(ScreenId::LoadBricksColor).is_none());
        assert!(
            u.drain_actions()
                .iter()
                .any(|(_, a)| *a == UiAction::LoadBricksColors(ColorLoad::Match))
        );
    }
}
