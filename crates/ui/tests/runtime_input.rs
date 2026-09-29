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
                node(
                    "GuiButtonCtrl",
                    "addons",
                    115,
                    "canvas.pushDialog(AddOnsGui);",
                ),
            ],
        ),
        (
            "JoinServerGui",
            vec![
                node(
                    "GuiButtonCtrl",
                    "manual",
                    10,
                    "Canvas.pushDialog(\"manualJoin\");",
                ),
                node("GuiTextListCtrl", "JS_serverList", 45, ""),
                node("GuiButtonCtrl", "internet", 80, "JoinServerGui.queryWebMaster();"),
            ],
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
            let mut crosshair = node("GuiCrossHairHud", "Crosshair", 224, "");
            crosshair.extent = [32, 32];
            vec![icon, crosshair]
        }),
        ("LoadingGui", vec![]),
        ("defaultControlsGui", vec![]),
        (
            "TrustInviteGui",
            vec![node(
                "GuiButtonCtrl",
                "accept",
                10,
                "TrustInviteGui.clickAccept();",
            )],
        ),
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
    // NMH_Type::type: talking starts with the first typed character.
    assert!(!a.contains(&UiAction::StartTyping));
    assert_eq!(u.top_id(), ScreenId::MessageInput(ChatChannel::Say));
    u.update(500);
    down(&mut u, Key::Letter('w'));
    assert!(actions(&mut u).is_empty());
    // With no cursor shown the pointer stays captured, so it keeps driving
    // the camera: Torque sends cursor-off mouse moves to the action map
    // while newMessageHud (noCursor) holds the keyboard.
    u.handle_input(InputEvent::MouseDelta { dx: 8.0, dy: 2.0 });
    assert!(matches!(
        actions(&mut u).as_slice(),
        [UiAction::Game(GameAction::Look { .. })]
    ));
    for ch in "hello".chars() {
        u.handle_input(InputEvent::Char(ch));
    }
    assert_eq!(actions(&mut u), [UiAction::StartTyping]);
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
        status: "RECEIVING WORLD".into(),
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
    // Opening the list looks for LAN games and checks saved servers.
    assert_eq!(actions(&mut u), vec![UiAction::QueryLan]);
    click(&mut u, ScreenId::JoinServer, "manual");
    // No server checks join passwords yet, so the field is not offered.
    let view = u.screen(ScreenId::ManualJoin).unwrap().view();
    assert!(!view.is_shown(view.id("MJ_txtJoinPass").unwrap()));
    for ch in "127.0.0.1:28000".chars() {
        u.handle_input(InputEvent::Char(ch));
    }
    down(&mut u, Key::Return);
    down(&mut u, Key::Return);
    let actions = actions(&mut u);
    // The typed address is remembered for the next visit, like v20.
    assert!(matches!(&actions[0], UiAction::SaveSettings(s)
        if s.prefs.get("$pref::Join::Address").map(String::as_str) == Some("127.0.0.1:28000")));
    assert_eq!(
        actions[1..],
        [UiAction::JoinServer {
            address: "127.0.0.1:28000".into(),
            password: String::new()
        }]
    );
}
#[test]
fn favorite_button_stars_the_selected_server() {
    let mut u = ui();
    click(&mut u, ScreenId::MainMenu, "join");
    actions(&mut u);
    let server = |address: &str, favorite| ServerInfo {
        address: address.into(),
        name: "Server".into(),
        password: false,
        dedicated: false,
        ping_ms: Some(20),
        players: 1,
        max_players: 8,
        bricks: 0,
        map: "Slate".into(),
        favorite,
    };
    u.apply(UiUpdate::LanServers {
        servers: vec![server("bri://203.0.113.10:28000/key", true), server("192.168.1.20:28000", false)],
        querying: false,
    });
    // Nothing selected: the button does nothing.
    click(&mut u, ScreenId::JoinServer, "JoinServerGui.queryWebMaster();");
    assert!(actions(&mut u).is_empty());
    let list = u.screen(ScreenId::JoinServer).unwrap().view().id("JS_serverList").unwrap();
    u.screen_mut(ScreenId::JoinServer).unwrap().view_mut().select(list, Some(1));
    click(&mut u, ScreenId::JoinServer, "JoinServerGui.queryWebMaster();");
    assert_eq!(
        actions(&mut u),
        vec![UiAction::ToggleFavorite {
            address: "192.168.1.20:28000".into()
        }]
    );
}
#[test]
fn platform_questions_send_their_action_only_on_yes() {
    let mut u = ui();
    let ask = || UiUpdate::Confirm {
        title: "Windows Firewall".into(),
        text: "Let the game through?".into(),
        action: Box::new(UiAction::AllowFirewall { port: 28000 }),
    };
    u.apply(ask());
    down(&mut u, Key::Escape);
    assert!(actions(&mut u).is_empty());
    u.apply(ask());
    down(&mut u, Key::Return);
    assert_eq!(actions(&mut u), vec![UiAction::AllowFirewall { port: 28000 }]);
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
            status: "RECEIVING WORLD".into(),
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
    // v20's getNextMove adds `KeyboardTurnSpeed` radians to every 32 ms move
    // (blocklandv20.exe 0x59571e): 0.25 / 0.032 s × 0.1 s (frames are capped
    // at 100 ms).
    let step = 0.25 / 0.032 * 0.1;
    let Some(UiAction::Game(GameAction::Look { yaw, pitch })) = actions(&mut u).pop() else {
        panic!("no look action");
    };
    assert!((yaw + step).abs() < 1e-5 && (pitch + step).abs() < 1e-5, "{yaw} {pitch}");
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
#[test]
fn crosshair_shows_only_in_first_person_and_hides_with_names() {
    let mut u = ui();
    u.core
        .binds
        .bind(BindInput::Key(Chord::plain(Key::F(5))), "ToggleShapeNameHud");
    play(&mut u);
    let shown = |u: &Ui| {
        let v = u.screen(ScreenId::Play).unwrap().view();
        v.node(v.id("Crosshair").unwrap()).state.visible
    };
    u.update(16);
    assert!(shown(&u));
    // GuiCrossHairHud draws nothing unless the connection is first person.
    u.apply(UiUpdate::FirstPerson(false));
    u.update(16);
    assert!(!shown(&u));
    u.apply(UiUpdate::FirstPerson(true));
    u.update(16);
    assert!(shown(&u));
    // F5 toggles player names and the crosshair together.
    down(&mut u, Key::F(5));
    up(&mut u, Key::F(5));
    u.update(16);
    assert!(!shown(&u));
}
#[test]
fn ski_crash_whiteout_takes_the_stronger_flash_and_fades() {
    let mut u = ui();
    u.apply(UiUpdate::Whiteout(0.5));
    u.apply(UiUpdate::Whiteout(0.25));
    assert_eq!(u.core.whiteout, 0.5);
    u.update(250);
    assert!((u.core.whiteout - 0.25).abs() < 1e-6);
    u.update(1000);
    assert_eq!(u.core.whiteout, 0.0);
}
#[test]
fn wheel_scrolls_the_open_brick_bar_like_scroll_inventory() {
    let mut u = ui();
    u.core.binds.bind(BindInput::Wheel, "scrollInventory");
    u.core.binds.bind(
        BindInput::Key(Chord {
            key: Key::Digit(1),
            mods: Modifiers::NONE,
        }),
        "useBricks",
    );
    play(&mut u);
    let brick = |id: &str| BrickInfo {
        id: id.into(),
        ui_name: id.into(),
        category: "Bricks".into(),
        subcategory: "Basic".into(),
        icon: IconRef::None,
    };
    u.apply(UiUpdate::Bricks(vec![brick("a"), brick("b"), brick("c")]));
    u.apply(UiUpdate::BrickInventory(vec![
        Some("a".into()),
        Some("b".into()),
        None,
        Some("c".into()),
    ]));
    down(&mut u, Key::Digit(1));
    up(&mut u, Key::Digit(1));
    assert_eq!(actions(&mut u), vec![UiAction::UseBrickSlot { slot: 0 }]);
    // Wheel down (Torque negative) moves to the next filled slot.
    u.handle_input(InputEvent::Wheel { delta: -1.0 });
    assert_eq!(actions(&mut u), vec![UiAction::UseBrickSlot { slot: 1 }]);
    u.handle_input(InputEvent::Wheel { delta: -1.0 });
    assert_eq!(actions(&mut u), vec![UiAction::UseBrickSlot { slot: 3 }]);
    u.handle_input(InputEvent::Wheel { delta: 1.0 });
    assert_eq!(actions(&mut u), vec![UiAction::UseBrickSlot { slot: 1 }]);
    // A high-resolution wheel reports eighths of a notch: one slot per
    // notch, not one per report (which lapped a full bar back to the start).
    for _ in 0..7 {
        u.handle_input(InputEvent::Wheel { delta: -0.125 });
        assert!(actions(&mut u).is_empty());
    }
    u.handle_input(InputEvent::Wheel { delta: -0.125 });
    assert_eq!(actions(&mut u), vec![UiAction::UseBrickSlot { slot: 3 }]);
    // Reversing discards the unfinished notch in the old direction.
    u.handle_input(InputEvent::Wheel { delta: -0.5 });
    u.handle_input(InputEvent::Wheel { delta: 1.0 });
    assert_eq!(actions(&mut u), vec![UiAction::UseBrickSlot { slot: 1 }]);
}
#[test]
fn start_games_add_ons_tab_opens_the_add_ons_screen() {
    let mut u = ui();
    click(&mut u, ScreenId::MainMenu, "start");
    u.drain_actions();
    click(&mut u, ScreenId::StartMission, "addons");
    assert_eq!(u.top_id(), ScreenId::AddOns);
    assert!(actions(&mut u).contains(&UiAction::RequestAddOns));
}

#[test]
fn start_game_hosts_the_chosen_game_mode_on_its_map() {
    let mut u = ui();
    let strata = "stresslab-world:world/strata";
    u.apply(UiUpdate::Maps(
        ["native/bedroom", strata]
            .map(|id| MapInfo {
                id: id.into(),
                name: id.into(),
                description: String::new(),
                preview: IconRef::None,
            })
            .to_vec(),
    ));
    u.apply(UiUpdate::GameModes(vec![GameModeInfo {
        id: "stresslab-mode:mode/stresslab".into(),
        name: "Stress Lab".into(),
        description: "Dig.".into(),
        map: Some(strata.into()),
    }]));
    click(&mut u, ScreenId::MainMenu, "start");
    // The picker opens from Start Game; Select keeps Custom.
    click(&mut u, ScreenId::StartMission, "SM_GameMode");
    assert_eq!(u.top_id(), ScreenId::GameModes);
    click(&mut u, ScreenId::GameModes, "GM_Select");
    assert_eq!(u.top_id(), ScreenId::StartMission);
    click(&mut u, ScreenId::StartMission, "host");
    let hosted: Vec<_> = actions(&mut u)
        .into_iter()
        .filter(|a| matches!(a, UiAction::HostGame { .. }))
        .collect();
    assert!(
        matches!(&hosted[..], [UiAction::HostGame { map, game_mode: None, .. }] if map == "native/bedroom"),
        "{hosted:?}"
    );
    // A chosen mode hosts on its own map, whatever the list showed.
    let mut u = ui();
    u.apply(UiUpdate::Maps(vec![MapInfo {
        id: "native/bedroom".into(),
        name: "Bedroom".into(),
        description: String::new(),
        preview: IconRef::None,
    }]));
    u.apply(UiUpdate::GameModes(vec![GameModeInfo {
        id: "stresslab-mode:mode/stresslab".into(),
        name: "Stress Lab".into(),
        description: String::new(),
        map: Some(strata.into()),
    }]));
    u.core.prefs.set(
        bri_ui::screens::modes::GAME_MODE,
        "stresslab-mode:mode/stresslab",
    );
    click(&mut u, ScreenId::MainMenu, "start");
    click(&mut u, ScreenId::StartMission, "host");
    let hosted: Vec<_> = actions(&mut u)
        .into_iter()
        .filter(|a| matches!(a, UiAction::HostGame { .. }))
        .collect();
    assert!(
        matches!(&hosted[..], [UiAction::HostGame { map, game_mode: Some(mode), .. }]
            if map == strata && mode == "stresslab-mode:mode/stresslab"),
        "{hosted:?}"
    );
}

#[test]
fn leaving_a_host_with_unsaved_changes_asks_about_them_first() {
    let mut u = ui();
    play(&mut u);
    u.apply(UiUpdate::UnsavedChanges(true));
    assert!(u.core.unsaved_changes);
    down(&mut u, Key::Escape);
    click(&mut u, ScreenId::EscapeMenu, "disconnect");
    down(&mut u, Key::Escape);
    assert!(actions(&mut u).is_empty(), "declining keeps the game");
    click(&mut u, ScreenId::EscapeMenu, "disconnect");
    down(&mut u, Key::Return);
    assert_eq!(actions(&mut u), vec![UiAction::Disconnect]);
    // The question names the unsaved build, not just leaving.
    u.core.confirm_unsaved(bri_ui::ui::Callback::Quit);
    let Some(bri_ui::ui::StackCmd::Message(message)) = u.core.cmds.last() else {
        panic!("a question was asked");
    };
    assert_eq!(message.title, "Unsaved Changes");
    assert!(message.text.contains("haven't saved"));
    assert_eq!(message.on_yes, bri_ui::ui::Callback::Quit);
}

#[test]
fn double_clicking_a_game_mode_chooses_it_like_select() {
    let mut u = ui();
    u.apply(UiUpdate::GameModes(vec![GameModeInfo {
        id: "stresslab-mode:mode/stresslab".into(),
        name: "Stress Lab".into(),
        description: String::new(),
        map: None,
    }]));
    click(&mut u, ScreenId::MainMenu, "start");
    click(&mut u, ScreenId::StartMission, "SM_GameMode");
    assert_eq!(u.top_id(), ScreenId::GameModes);
    // Lists report a double-click as Submit, as the Add-Ons list reads it.
    click(&mut u, ScreenId::GameModes, "GM_List");
    click(&mut u, ScreenId::GameModes, "GM_List");
    assert_eq!(u.top_id(), ScreenId::StartMission);
}

#[test]
fn the_player_list_key_closes_the_list_it_opened() {
    let mut u = ui();
    play(&mut u);
    u.core
        .binds
        .bind(BindInput::Key(Chord::plain(Key::F(2))), "showPlayerList");
    down(&mut u, Key::F(2));
    u.update(16);
    assert_eq!(u.top_id(), ScreenId::PlayerList);
    down(&mut u, Key::F(2));
    u.update(16);
    assert_eq!(u.top_id(), ScreenId::Play);
}

#[test]
fn trust_invites_queue_like_mini_game_invites_and_escape_closes_them() {
    let mut u = ui();
    play(&mut u);
    for (from, name) in [(2, "Alpha"), (3, "Bravo")] {
        u.apply(UiUpdate::TrustInvite(TrustInvitation {
            from,
            name: name.into(),
            bl_id: from.to_string(),
            level: 1,
        }));
    }
    u.update(16);
    assert_eq!(u.top_id(), ScreenId::TrustInvitation);
    // The second invite waits behind the first instead of replacing it.
    click(&mut u, ScreenId::TrustInvitation, "accept");
    u.update(16);
    let a = actions(&mut u);
    assert!(a.contains(&UiAction::AnswerTrustInvite {
        from: 3,
        answer: TrustAnswer::Accept
    }));
    assert_eq!(u.top_id(), ScreenId::TrustInvitation);
    assert_eq!(u.core.trust_invites.len(), 1);
    // Escape closes the dialog without answering, as for mini-game invites.
    down(&mut u, Key::Escape);
    u.update(16);
    assert_eq!(u.top_id(), ScreenId::Play);
    assert!(
        !actions(&mut u)
            .iter()
            .any(|a| matches!(a, UiAction::AnswerTrustInvite { .. }))
    );
}

fn start_join(u: &mut Ui) -> bri_ui::api::RequestId {
    click(u, ScreenId::MainMenu, "join");
    click(u, ScreenId::JoinServer, "manual");
    for ch in "10.0.0.5:28000".chars() {
        u.handle_input(InputEvent::Char(ch));
    }
    down(u, Key::Return);
    actions(u);
    u.session_request().unwrap()
}
fn question(on_no: Option<UiAction>) -> Question {
    Question {
        title: "Asked".into(),
        text: "Really?".into(),
        yes: "Continue".into(),
        no: "Cancel".into(),
        on_yes: Box::new(UiAction::TrustNewServerIdentity {
            address: "10.0.0.5:28000".into(),
        }),
        on_no: on_no.map(Box::new),
    }
}
fn message_boxes(u: &Ui) -> usize {
    u.dialogs
        .iter()
        .filter(|d| d.id() == ScreenId::MessageBox)
        .count()
}

#[test]
fn a_changed_server_identity_asks_instead_of_reporting_the_failure() {
    let mut u = ui();
    let id = start_join(&mut u);
    u.apply_session(id, UiUpdate::FailureQuestion(question(None)));
    u.apply_session(
        id,
        UiUpdate::Connection(ConnectionState::Failed {
            reason: "The server at 10.0.0.5:28000 has a different identity".into(),
        }),
    );
    u.update(0);
    // One question with its own answers, not a failure box beneath it.
    assert_eq!(message_boxes(&u), 1);
    let view = u.screen(ScreenId::MessageBox).unwrap().view();
    assert_eq!(view.text_of(view.id("yes").unwrap()), "Continue");
    assert_eq!(view.text_of(view.id("no").unwrap()), "Cancel");
    click(&mut u, ScreenId::MessageBox, "yes");
    assert_eq!(
        actions(&mut u),
        [UiAction::TrustNewServerIdentity {
            address: "10.0.0.5:28000".into()
        }]
    );
    // Continuing is a new join attempt the host's updates belong to.
    assert!(u.session_request().is_some_and(|new| new != id));
}

#[test]
fn leaving_a_join_question_cancels_the_join() {
    let mut u = ui();
    let id = start_join(&mut u);
    u.apply_session(
        id,
        UiUpdate::Question(question(Some(UiAction::CancelConnect))),
    );
    u.update(0);
    assert_eq!(message_boxes(&u), 1);
    down(&mut u, Key::Escape);
    assert_eq!(actions(&mut u), [UiAction::CancelConnect]);
}
#[test]
fn toggle_crouch_flips_on_each_press_and_ignores_release() {
    let mut u = ui();
    play(&mut u);
    u.core
        .prefs
        .set_bool(bri_ui::screens::options::TOGGLE_CROUCH, true);
    u.core.run_command("crouch", true);
    assert_eq!(actions(&mut u), vec![held(HeldControl::Crouch, true)]);
    u.core.run_command("crouch", false);
    assert!(actions(&mut u).is_empty(), "still crouching after release");
    u.core.run_command("crouch", true);
    assert_eq!(actions(&mut u), vec![held(HeldControl::Crouch, false)]);
    u.core.run_command("crouch", false);
    assert!(actions(&mut u).is_empty());
    // Held crouch, as in v20, without the setting.
    u.core
        .prefs
        .set_bool(bri_ui::screens::options::TOGGLE_CROUCH, false);
    u.core.run_command("crouch", true);
    u.core.run_command("crouch", false);
    assert_eq!(
        actions(&mut u),
        vec![held(HeldControl::Crouch, true), held(HeldControl::Crouch, false)]
    );
}
#[test]
fn side_mouse_buttons_bind_by_torque_name() {
    for (name, button, label) in [
        ("button3", MouseButton::Back, "Mouse 4"),
        ("button4", MouseButton::Forward, "Mouse 5"),
    ] {
        let input = BindInput::parse(bri_ui::schema::Device::Mouse, name).unwrap();
        assert_eq!(input, BindInput::Mouse(button));
        assert_eq!(input.label(), label);
        assert_eq!(button.torque_name(), name);
    }
}
#[test]
fn first_run_offers_the_tutorial_then_asks_for_a_name_once() {
    // Declining the Tutorial puts the one name question straight away.
    let mut u = ui();
    u.core.first_run_welcome();
    u.update(0);
    assert_eq!(u.top_id(), ScreenId::MessageBox);
    down(&mut u, Key::Escape);
    u.update(0);
    assert_eq!(u.top_id(), ScreenId::ChooseName, "the name question");
    assert_eq!(u.core.prefs.str_or(bri_ui::ui::NAME_PROMPT, ""), "done");
    // Skipping it keeps "Blockhead" and settles it: no second question,
    // this run or the next.
    down(&mut u, Key::Escape);
    u.update(0);
    assert!(!u.is_open(ScreenId::ChooseName));
    assert!(!u.is_open(ScreenId::MessageBox));
    u.core.name_prompt();
    u.update(0);
    assert!(!u.is_open(ScreenId::ChooseName), "asked only once");
    u.core.name_asked = false;
    u.core.name_prompt();
    u.update(0);
    assert!(!u.is_open(ScreenId::ChooseName), "not on the next run either");
    // Playing it starts the Tutorial and keeps the name for later.
    let mut u = ui();
    u.core.first_run_welcome();
    u.update(0);
    actions(&mut u);
    down(&mut u, Key::Return);
    assert_eq!(actions(&mut u).last(), Some(&UiAction::StartTutorial));
    assert_eq!(u.core.prefs.str_or(bri_ui::ui::NAME_PROMPT, ""), "after_tutorial");
    u.core.name_prompt();
    u.update(0);
    assert_eq!(u.top_id(), ScreenId::ChooseName);
    u.core.name_prompt();
    u.update(0);
    assert_eq!(
        u.stack().iter().filter(|s| **s == ScreenId::ChooseName).count(),
        1,
        "one question even when asked for twice"
    );
}
#[test]
fn sound_captions_show_only_when_turned_on_refresh_and_expire() {
    let mut u = ui();
    u.apply(UiUpdate::Caption("[Explosion]".into()));
    assert!(u.core.captions.is_empty(), "off by default");
    u.core
        .prefs
        .set_bool(bri_ui::screens::options::CAPTIONS, true);
    for text in ["[Explosion]", "[Weapon fire]", "[Explosion]"] {
        u.apply(UiUpdate::Caption(text.into()));
    }
    let lines: Vec<_> = u.core.captions.iter().map(|(t, _)| t.as_str()).collect();
    assert_eq!(lines, ["[Weapon fire]", "[Explosion]"]);
    u.update(3001);
    assert!(u.core.captions.is_empty());
}

/// v20's `openBSD`: with building disabled it center-prints instead of
/// opening the brick selector (whose wake would send the /bsd emote).
#[test]
fn open_bsd_with_building_disabled_only_says_so() {
    let mut u = ui();
    play(&mut u);
    u.apply(UiUpdate::BuildingAllowed(false));
    u.core.cmds.clear();
    assert!(u.core.run_command("openBSD", true));
    assert!(!u.core.cmds.contains(&bri_ui::ui::StackCmd::Push(ScreenId::BrickSelector)));
    assert!(u
        .core
        .center_print
        .as_ref()
        .is_some_and(|(t, _)| t.contains("Building is currently disabled.")));
    u.apply(UiUpdate::BuildingAllowed(true));
    assert!(u.core.run_command("openBSD", true));
    assert!(u.core.cmds.contains(&bri_ui::ui::StackCmd::Push(ScreenId::BrickSelector)));
}
