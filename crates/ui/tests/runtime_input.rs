//! Tests send data to the UI only; no OS window/input is created.
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
fn node(class: &str, name: &str, y: i32, command: &str) -> Control {
    let mut c = ctrl(class, "GuiDefaultProfile", Rect::new(10, y, 150, 25));
    c.name = Some(name.into());
    c.command = Some(command.into());
    c
}
fn fixture() -> Rc<Pack> {
    let mut p = UiPack::default();
    for (name, children) in [
        (
            "MainMenuGui",
            vec![
                node(
                    "GuiButtonCtrl",
                    "start",
                    10,
                    "Canvas.pushDialog(startMissionGui);",
                ),
                node(
                    "GuiButtonCtrl",
                    "join",
                    45,
                    "Canvas.PushDialog(JoinServerGui);",
                ),
            ],
        ),
        (
            "startMissionGui",
            vec![
                node(
                    "GuiTextListCtrl",
                    "SM_missionList",
                    10,
                    "SM_missionList.select();",
                ),
                node("GuiButtonCtrl", "host", 45, "SM_StartMission();"),
                node("GuiTextEditCtrl", "TxtServerName", 80, ""),
            ],
        ),
        (
            "JoinServerGui",
            vec![node(
                "GuiButtonCtrl",
                "manual",
                10,
                "Canvas.pushDialog(\"manualJoin\");",
            )],
        ),
        (
            "ManualJoin",
            vec![
                node("GuiTextEditCtrl", "MJ_txtIP", 10, ""),
                node("GuiTextEditCtrl", "MJ_txtJoinPass", 45, ""),
                node("GuiButtonCtrl", "connect", 80, "MJ_connect();"),
            ],
        ),
        (
            "MessageBoxOKDlg",
            vec![node(
                "GuiButtonCtrl",
                "ok",
                10,
                "MessageCallback(MessageBoxOKDlg,MessageBoxOKDlg.callback);",
            )],
        ),
        (
            "MessageBoxYesNoDlg",
            vec![
                node(
                    "GuiButtonCtrl",
                    "yes",
                    10,
                    "MessageCallback(MessageBoxYesNoDlg,MessageBoxYesNoDlg.yesCallback);",
                ),
                node(
                    "GuiButtonCtrl",
                    "no",
                    45,
                    "MessageCallback(MessageBoxYesNoDlg,MessageBoxYesNoDlg.noCallback);",
                ),
            ],
        ),
        (
            "newMessageHud",
            vec![node("GuiTextEditCtrl", "NMH_Type", 10, "")],
        ),
        (
            "escapeMenu",
            vec![node("GuiButtonCtrl", "disconnect", 10, "escapeFromGame();")],
        ),
        (
            "connectingGui",
            vec![node(
                "GuiButtonCtrl",
                "cancel",
                10,
                "ConnectingGui::cancel();",
            )],
        ),
        ("PlayGui", {
            let mut icon = node("GuiBitmapCtrl", "HUD_SuperShift", 731, "");
            icon.extent = [184, 37];
            icon.visible = false;
            vec![icon]
        }),
        ("LoadingGui", vec![]),
        ("defaultControlsGui", vec![]),
    ] {
        let mut c = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480));
        c.children = children;
        p.layouts.insert(name.into(), c);
    }
    Rc::new(Pack::from_parts(p, PathBuf::new()))
}
fn ui() -> Ui {
    let mut u = Ui::new(
        fixture(),
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
    for (key, cmd) in [
        (Key::Letter('w'), "moveforward"),
        (Key::Up, "moveforward"),
        (Key::Escape, "escapeMenu.toggle();"),
        (Key::Letter('t'), "globalChat"),
        (Key::Numpad(8), "shiftBrickAway"),
        (Key::Space, "jump"),
        (Key::Letter('j'), "jet"),
    ] {
        u.core.binds.bind(
            BindInput::Key(Chord {
                key,
                mods: Modifiers::NONE,
            }),
            cmd,
        );
    }
    u
}
fn down(u: &mut Ui, key: Key) {
    u.handle_input(InputEvent::KeyDown {
        key,
        mods: Modifiers::NONE,
        repeat: false,
    });
}
fn up(u: &mut Ui, key: Key) {
    u.handle_input(InputEvent::KeyUp {
        key,
        mods: Modifiers::NONE,
    });
}
fn click(u: &mut Ui, id: ScreenId, name: &str) {
    let (x, y) = u.control_center(id, name).unwrap();
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
}
fn play(u: &mut Ui) {
    u.apply(UiUpdate::Connection(ConnectionState::InGame {
        server_name: "Test".into(),
        max_players: 8,
        local: true,
        single_player: true,
        admin: true,
    }));
    u.drain_actions();
}
fn actions(u: &mut Ui) -> Vec<UiAction> {
    u.drain_actions().into_iter().map(|(_, a)| a).collect()
}
fn held(control: HeldControl, down: bool) -> UiAction {
    UiAction::Game(GameAction::Held { control, down })
}

#[test]
fn modal_open_releases_controls_and_repeat_before_typing() {
    let mut u = ui();
    play(&mut u);
    down(&mut u, Key::Letter('w'));
    down(&mut u, Key::Numpad(8));
    assert!(actions(&mut u).contains(&held(HeldControl::Forward, true)));
    down(&mut u, Key::Letter('t'));
    let a = actions(&mut u);
    assert!(a.contains(&held(HeldControl::Forward, false)));
    assert!(a.contains(&UiAction::StartTyping));
    assert_eq!(u.top_id(), ScreenId::MessageInput(ChatChannel::Say));
    u.update(500);
    down(&mut u, Key::Letter('w'));
    u.handle_input(InputEvent::MouseDelta { dx: 8.0, dy: 2.0 });
    assert!(actions(&mut u).is_empty());
    for ch in "hello".chars() {
        u.handle_input(InputEvent::Char(ch));
    }
    down(&mut u, Key::Return);
    let a = actions(&mut u);
    assert!(a.contains(&UiAction::Chat {
        channel: ChatChannel::Say,
        text: "hello".into()
    }));
    assert!(a.contains(&UiAction::StopTyping));
    assert_eq!(u.top_id(), ScreenId::Play);
    up(&mut u, Key::Letter('w'));
    assert!(actions(&mut u).is_empty());
}
#[test]
fn focus_loss_and_loading_cancel_never_leave_held_controls() {
    let mut u = ui();
    play(&mut u);
    down(&mut u, Key::Space);
    actions(&mut u);
    u.handle_input(InputEvent::FocusLost);
    assert_eq!(actions(&mut u), vec![held(HeldControl::Jump, false)]);
    u.apply(UiUpdate::Connection(ConnectionState::Loading {
        map: "Bedroom".into(),
        preview: IconRef::None,
        phase: LoadPhase::Ghosting,
        progress: 0.5,
    }));
    down(&mut u, Key::Letter('w'));
    assert!(actions(&mut u).is_empty());
    down(&mut u, Key::Escape);
    assert_eq!(actions(&mut u), vec![UiAction::CancelConnect]);
}
#[test]
fn duplicate_keys_and_one_button_jet_holds_release_independently() {
    let mut u = ui();
    play(&mut u);
    down(&mut u, Key::Letter('w'));
    down(&mut u, Key::Up);
    actions(&mut u);
    up(&mut u, Key::Letter('w'));
    assert!(actions(&mut u).is_empty());
    up(&mut u, Key::Up);
    assert_eq!(actions(&mut u), vec![held(HeldControl::Forward, false)]);
    u.core.settings.mouse_type = 0;
    down(&mut u, Key::Space);
    down(&mut u, Key::Letter('j'));
    actions(&mut u);
    up(&mut u, Key::Space);
    assert_eq!(actions(&mut u), vec![held(HeldControl::Jump, false)]);
    up(&mut u, Key::Letter('j'));
    assert_eq!(actions(&mut u), vec![held(HeldControl::Jet, false)]);
}
#[test]
fn host_uses_current_catalog_once_and_rejection_reenables_form() {
    let mut u = ui();
    u.apply(UiUpdate::Maps(vec![MapInfo {
        id: "native/bedroom".into(),
        name: "Bedroom".into(),
        description: "Original".into(),
        preview: IconRef::None,
    }]));
    click(&mut u, ScreenId::MainMenu, "start");
    click(&mut u, ScreenId::StartMission, "host");
    // Hosting also saves the chosen server type, as v20's prefs did.
    let a: Vec<_> = u
        .drain_actions()
        .into_iter()
        .filter(|(_, a)| !matches!(a, UiAction::SaveSettings(_)))
        .collect();
    assert_eq!(a.len(), 1);
    assert!(matches!(&a[0].1,UiAction::HostGame{map,..} if map=="native/bedroom"));
    click(&mut u, ScreenId::StartMission, "host");
    assert!(actions(&mut u).is_empty());
    u.apply(UiUpdate::ActionResult {
        id: a[0].0,
        result: Err("Port occupied".into()),
    });
    assert_eq!(u.top_id(), ScreenId::MessageBox);
    down(&mut u, Key::Return);
    assert_eq!(u.top_id(), ScreenId::StartMission);
    click(&mut u, ScreenId::StartMission, "host");
    assert_eq!(
        actions(&mut u)
            .iter()
            .filter(|a| matches!(a, UiAction::HostGame { .. }))
            .count(),
        1
    );
    u.apply(UiUpdate::Connection(ConnectionState::Connecting {
        text: "Hosting".into(),
    }));
    down(&mut u, Key::Escape);
    assert_eq!(actions(&mut u), vec![UiAction::CancelConnect]);
}
#[test]
fn direct_join_accepts_text_and_blocks_duplicate_request() {
    let mut u = ui();
    click(&mut u, ScreenId::MainMenu, "join");
    click(&mut u, ScreenId::JoinServer, "manual");
    for ch in "127.0.0.1:28000".chars() {
        u.handle_input(InputEvent::Char(ch));
    }
    down(&mut u, Key::Return);
    down(&mut u, Key::Return);
    assert_eq!(
        actions(&mut u),
        vec![UiAction::JoinServer {
            address: "127.0.0.1:28000".into(),
            password: String::new()
        }]
    );
}
#[test]
fn confirmation_is_modal_and_escape_declines_without_underlying_action() {
    let mut u = ui();
    play(&mut u);
    down(&mut u, Key::Escape);
    click(&mut u, ScreenId::EscapeMenu, "disconnect");
    down(&mut u, Key::Letter('w'));
    down(&mut u, Key::Escape);
    assert!(actions(&mut u).is_empty());
    assert_eq!(u.top_id(), ScreenId::EscapeMenu);
    click(&mut u, ScreenId::EscapeMenu, "disconnect");
    down(&mut u, Key::Return);
    assert_eq!(actions(&mut u), vec![UiAction::Disconnect]);
}
#[test]
fn ui_scaling_falls_back_for_nonfinite_values_and_first_run_uses_wheel_mouse() {
    let mut u = Ui::new(
        fixture(),
        UiConfig {
            size: (1920, 1080),
            scale: Some(f32::INFINITY),
            platform: Platform::Windows,
        },
        Settings::default(),
    );
    assert_eq!(u.scale(), 2.0);
    assert_eq!(u.core.settings.mouse_type, 2);
    u.resize((1280, 960), Some(f32::NAN));
    assert_eq!(u.logical_size(), (640, 480));
}

#[test]
fn event_capabilities_are_class_qualified_and_closed_by_default() {
    use bri_ui::schema::{EventTables, OutputEventDef};
    let tables = EventTables {
        inputs: vec![],
        outputs: ["fxDTSBrick", "Player", "MiniGame"]
            .into_iter()
            .map(|class| OutputEventDef {
                class: class.into(),
                name: "setColor".into(),
                params: vec![],
                append_client: true,
                source_line: 1,
            })
            .collect(),
    };
    let c = EventCatalog::from_capabilities(&tables, &[], &[("Player", "setColor")]);
    assert_eq!(
        c.outputs.iter().map(|o| o.supported).collect::<Vec<_>>(),
        [false, true, false]
    );
    assert!(
        EventCatalog::from_capabilities(&tables, &[], &[])
            .outputs
            .iter()
            .all(|o| !o.supported)
    );
}

#[test]
fn canceled_attempt_cannot_reopen_and_new_session_clears_host_data() {
    let mut u = ui();
    let token = u.core.request_pending(
        UiAction::JoinServer {
            address: "127.0.0.1:28000".into(),
            password: String::new(),
        },
        bri_ui::ui::Pending::Other,
    );
    assert!(u.apply_session(
        token,
        UiUpdate::Connection(ConnectionState::Connecting {
            text: "Waiting".into()
        })
    ));
    u.core.request(UiAction::CancelConnect);
    u.update(0);
    assert!(!u.apply_session(
        token,
        UiUpdate::Connection(ConnectionState::InGame {
            server_name: "stale".into(),
            max_players: 8,
            local: false,
            single_player: false,
            admin: false
        })
    ));
    play(&mut u);
    u.apply(UiUpdate::Chat {
        text: "old host message".into(),
    });
    u.apply(UiUpdate::Tools(vec![Some(ToolInfo {
        id: "old".into(),
        name: "Old".into(),
        icon: IconRef::None,
        tint: None,
    })]));
    u.apply(UiUpdate::BuildingAllowed(false));
    u.core.selector.favorites.insert(3, vec!["1x1".into()]);
    u.apply(UiUpdate::Connection(ConnectionState::Failed {
        reason: "Closed".into(),
    }));
    assert!(u.core.chat.lines.is_empty());
    assert!(u.core.hud.tools.iter().all(Option::is_none));
    assert!(!u.core.hud.building_disabled);
    assert!(u.core.pending.is_empty());
    assert!(u.core.selector.favorites.contains_key(&3));
}
#[test]
fn authoritative_hud_updates_cannot_echo_or_panic_on_stale_slots() {
    let mut u = ui();
    play(&mut u);
    u.apply(UiUpdate::SetActiveTool(Some(usize::MAX)));
    u.apply(UiUpdate::SetActiveBrick(Some(usize::MAX)));
    u.apply(UiUpdate::BrickInventory(vec![]));
    u.apply(UiUpdate::Tools(vec![]));
    u.apply(UiUpdate::Tools(vec![Some(ToolInfo {
        id: "hammer".into(),
        name: "Hammer".into(),
        icon: IconRef::None,
        tint: None,
    })]));
    u.apply(UiUpdate::SetActiveTool(Some(0)));
    assert!(u.core.hud.tool_active);
    u.apply(UiUpdate::SetActiveTool(None));
    u.apply(UiUpdate::SetActiveBrick(None));
    assert!(!u.core.hud.tool_active && !u.core.hud.brick_active);
    assert!(actions(&mut u).is_empty());
}
#[test]
fn pending_direct_join_cancel_emits_transport_cancellation() {
    let mut u = ui();
    click(&mut u, ScreenId::MainMenu, "join");
    click(&mut u, ScreenId::JoinServer, "manual");
    u.handle_input(InputEvent::Char('x'));
    down(&mut u, Key::Return);
    let token = u.session_request().unwrap();
    actions(&mut u);
    down(&mut u, Key::Escape);
    assert_eq!(actions(&mut u), vec![UiAction::CancelConnect]);
    assert!(u.session_request().is_none());
    assert!(!u.apply_session(
        token,
        UiUpdate::Connection(ConnectionState::Loading {
            map: "stale".into(),
            preview: IconRef::None,
            phase: LoadPhase::Ghosting,
            progress: 1.0
        })
    ));
}
#[test]
fn keyboard_turn_looks_at_the_preferred_rate_while_held() {
    let mut u = ui();
    u.core
        .binds
        .bind(BindInput::Key(Chord::plain(Key::Left)), "turnLeft");
    u.core
        .binds
        .bind(BindInput::Key(Chord::plain(Key::PageUp)), "panUp");
    u.core.prefs.set("$pref::Input::KeyboardTurnSpeed", "0.25");
    play(&mut u);
    down(&mut u, Key::Left);
    down(&mut u, Key::PageUp);
    assert!(actions(&mut u).is_empty(), "turning is a rate, not a hold");
    u.update(500);
    // 0.25 × 4 rad/s × 0.1 s (frames are capped at 100 ms).
    assert_eq!(
        actions(&mut u),
        vec![UiAction::Game(GameAction::Look {
            yaw: -0.1,
            pitch: -0.1
        })]
    );
    up(&mut u, Key::Left);
    up(&mut u, Key::PageUp);
    u.update(50);
    assert!(actions(&mut u).is_empty());
}
#[test]
fn super_shift_toggle_shows_the_hud_icon_on_the_bottom_edge() {
    let mut u = ui();
    u.core
        .binds
        .bind(BindInput::Key(Chord::plain(Key::LAlt)), "toggleSuperShift");
    play(&mut u);
    let shown = |u: &Ui| {
        let v = u.screen(ScreenId::Play).unwrap().view();
        let n = v.id("HUD_SuperShift").unwrap();
        (v.node(n).state.visible, v.node(n).rect.y)
    };
    assert!(!shown(&u).0);
    down(&mut u, Key::LAlt);
    up(&mut u, Key::LAlt);
    u.update(16);
    // 640x480 logical: below 1024 wide it sits above the inventory bar.
    assert_eq!(shown(&u), (true, 480 - (87 + 37)));
    down(&mut u, Key::LAlt);
    up(&mut u, Key::LAlt);
    u.update(16);
    assert!(!shown(&u).0);
}
