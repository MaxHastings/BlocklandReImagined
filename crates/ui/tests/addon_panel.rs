//! An Add-On's panel: laid out from the rules' description, its buttons,
//! list picks and close box sent with every field's text and list's pick,
//! and a row dragged from one list to another.
use bri_ui::{
    api::*,
    binds::Platform,
    input::{InputEvent, MouseButton},
    pack::Pack,
    schema::UiPack,
    screens::ScreenId,
    ui::{Ui, UiConfig},
    view::{EventKind, View, ViewEvent},
};
use std::{path::PathBuf, rc::Rc};

fn ui() -> Ui {
    Ui::new(
        Rc::new(Pack::from_parts(UiPack::default(), PathBuf::new())),
        UiConfig {
            size: (640, 480),
            scale: Some(1.0),
            platform: Platform::Windows,
        },
        Settings {
            binds: Some(vec![]),
            ..Default::default()
        },
    )
}

fn button(id: &str, label: &str) -> PanelButtonView {
    PanelButtonView {
        id: id.into(),
        label: label.into(),
        picture: None,
        hover_picture: None,
        color: None,
        rect: None,
        action: None,
    }
}

fn shop() -> PanelView {
    PanelView {
        package: "rp".into(),
        id: "shop".into(),
        title: "Gun Shop".into(),
        width: 320,
        height: 300,
        items: vec![
            PanelItemView::Text {
                text: "Pick a gun.\nPay at the till.".into(),
                style: PanelTextView::default(),
            },
            PanelItemView::List {
                id: "stock".into(),
                rows: vec!["Pistol $5".into(), "Rifle $20".into()],
                selected: Some(0),
                height: 100,
                drag: false,
                columns: vec![],
                icons: vec![],
                place: None,
            },
            PanelItemView::Field {
                id: "note".into(),
                label: "Engraving".into(),
                text: "Max".into(),
                max: 12,
            },
            PanelItemView::Buttons(vec![button("buy", "Buy"), button("sell", "Sell")]),
        ],
        border: true,
        modal: true,
        background: None,
    }
}

fn view(ui: &Ui) -> &View {
    ui.screen(ScreenId::AddOnPanel)
        .expect("the panel is open")
        .view()
}

fn event(ui: &mut Ui, name: &str, kind: EventKind) {
    let node = view(ui).id(name).unwrap();
    let i = ui
        .dialogs
        .iter()
        .rposition(|s| s.id() == ScreenId::AddOnPanel)
        .unwrap();
    let (dialogs, core) = (&mut ui.dialogs, &mut ui.core);
    dialogs[i].on_event(&ViewEvent { node, kind }, core);
}

fn actions(ui: &mut Ui) -> Vec<UiAction> {
    ui.drain_actions().into_iter().map(|(_, a)| a).collect()
}

fn sent(control: &str, row: Option<u16>, note: &str) -> UiAction {
    UiAction::PanelEvent {
        package: "rp".into(),
        panel: "shop".into(),
        control: control.into(),
        fields: vec![("note".into(), note.into())],
        lists: vec![("stock".into(), row)],
        drop: None,
    }
}

#[test]
fn a_panel_lays_out_its_items_and_sends_what_the_player_does() {
    let mut ui = ui();
    ui.apply(UiUpdate::Panel(Some(shop())));
    ui.update(0);
    assert!(ui.is_open(ScreenId::AddOnPanel));
    let v = view(&ui);
    let list = v.id("Panel_List_stock").unwrap();
    assert_eq!(v.selected(list), Some(0));
    assert_eq!(v.node(list).state.items.len(), 2);
    let field = v.id("Panel_Field_note").unwrap();
    assert_eq!(v.edit_text(field), "Max");
    // Top to bottom: the list over the field over the buttons, all inside
    // the window.
    let (l, f, b) = (
        v.node(list).rect,
        v.node(field).rect,
        v.node(v.id("Panel_Button_buy").unwrap()).rect,
    );
    assert!(l.y < f.y && f.y < b.y, "{l:?} {f:?} {b:?}");
    event(&mut ui, "Panel_Button_buy", EventKind::Click);
    assert_eq!(actions(&mut ui), [sent("buy", Some(0), "Max")]);
    // Picking a row is told at once, with the new pick.
    let n = view(&ui).id("Panel_List_stock").unwrap();
    ui.dialogs.last_mut().unwrap().view_mut().select(n, Some(1));
    event(&mut ui, "Panel_List_stock", EventKind::Changed);
    assert_eq!(actions(&mut ui), [sent("stock", Some(1), "Max")]);
    // The close box tells the rules, and the window goes.
    event(&mut ui, "Panel_Button_sell", EventKind::Click);
    let win = view(&ui)
        .walk()
        .find(|n| view(&ui).node(*n).ctrl.class == "GuiWindowCtrl")
        .unwrap();
    let i = ui.dialogs.len() - 1;
    let (dialogs, core) = (&mut ui.dialogs, &mut ui.core);
    dialogs[i].on_event(
        &ViewEvent {
            node: win,
            kind: EventKind::Close,
        },
        core,
    );
    assert_eq!(
        actions(&mut ui),
        [sent("sell", Some(1), "Max"), sent("close", Some(1), "Max")]
    );
    ui.update(0);
    assert!(!ui.is_open(ScreenId::AddOnPanel));
}

#[test]
fn the_rules_replace_or_close_the_panel() {
    let mut ui = ui();
    ui.apply(UiUpdate::Panel(Some(shop())));
    let mut bank = shop();
    bank.id = "bank".into();
    bank.title = "Bank".into();
    ui.apply(UiUpdate::Panel(Some(bank)));
    ui.update(0);
    let open = ui
        .dialogs
        .iter()
        .filter(|s| s.id() == ScreenId::AddOnPanel)
        .count();
    assert_eq!(open, 1, "one panel at a time");
    ui.apply(UiUpdate::Panel(None));
    ui.update(0);
    assert!(!ui.is_open(ScreenId::AddOnPanel));
    // Closed by the rules: nothing goes back to them.
    assert!(actions(&mut ui).is_empty());
}

#[test]
fn a_server_panel_received_during_loading_survives_game_entry() {
    let mut ui = ui();
    ui.apply(UiUpdate::Connection(ConnectionState::Loading {
        map: "test:map/entry".into(),
        preview: IconRef::None,
        status: "LOADING".into(),
        progress: 0.5,
    }));
    ui.apply(UiUpdate::Panel(Some(shop())));
    let game = || ConnectionState::InGame {
        server_name: "Test".into(),
        max_players: 8,
        local: true,
        single_player: false,
        admin: true,
    };
    ui.apply(UiUpdate::Connection(game()));
    assert!(
        ui.is_open(ScreenId::AddOnPanel),
        "entry lost the server's panel"
    );
    assert_eq!(ui.core.panel.as_ref().unwrap().id, "shop");
    ui.apply(UiUpdate::Connection(game()));
    assert_eq!(
        ui.dialogs
            .iter()
            .filter(|d| d.id() == ScreenId::AddOnPanel)
            .count(),
        1
    );
    assert!(actions(&mut ui).is_empty(), "loading is not a user close");

    ui.apply(UiUpdate::Connection(ConnectionState::Idle));
    assert!(
        ui.core.panel.is_none(),
        "the previous server's panel leaked"
    );
    ui.apply(UiUpdate::Connection(game()));
    assert!(!ui.is_open(ScreenId::AddOnPanel));
}

fn backpack_and_chest() -> PanelView {
    let list = |id: &str, rows: &[&str]| PanelItemView::List {
        id: id.into(),
        rows: rows.iter().map(|r| (*r).to_owned()).collect(),
        selected: None,
        height: 80,
        drag: true,
        columns: vec![],
        icons: vec![],
        place: None,
    };
    PanelView {
        package: "rp".into(),
        id: "chest".into(),
        title: "Chest".into(),
        width: 320,
        height: 300,
        items: vec![
            list("pack", &["Bread", "Rope", "Lamp"]),
            list("chest", &["Gold"]),
            PanelItemView::List {
                id: "log".into(),
                rows: vec!["opened".into()],
                selected: None,
                height: 40,
                drag: false,
                columns: vec![],
                icons: vec![],
                place: None,
            },
        ],
        border: true,
        modal: true,
        background: None,
    }
}

fn press_drag_release(ui: &mut Ui, from: (i32, i32), to: (i32, i32)) {
    let p = |(x, y): (i32, i32)| (x as f32, y as f32);
    let ((fx, fy), (tx, ty)) = (p(from), p(to));
    ui.handle_input(InputEvent::MouseMove { x: fx, y: fy });
    ui.handle_input(InputEvent::MouseDown {
        button: MouseButton::Left,
        x: fx,
        y: fy,
    });
    ui.handle_input(InputEvent::MouseMove { x: tx, y: ty });
    ui.handle_input(InputEvent::MouseUp {
        button: MouseButton::Left,
        x: tx,
        y: ty,
    });
}

fn dropped(from: &str, row: u16, at: Option<u16>) -> Option<PanelDropView> {
    Some(PanelDropView {
        from: from.into(),
        row,
        at,
    })
}

#[test]
fn a_row_dragged_from_the_backpack_to_the_chest_goes_to_the_rules() {
    let mut ui = ui();
    ui.apply(UiUpdate::Panel(Some(backpack_and_chest())));
    ui.update(0);
    let v = view(&ui);
    let row = |name: &str, i: i32| {
        let r = v.node(v.id(name).unwrap()).rect;
        let h = r.h / v.node(v.id(name).unwrap()).state.items.len() as i32;
        (r.x + 10, r.y + i * h + h / 2)
    };
    let rope = row("Panel_List_pack", 1);
    let gold = row("Panel_List_chest", 0);
    let log = row("Panel_List_log", 0);
    let chest = v.node(v.id("Panel_List_chest").unwrap()).rect;
    // Below the chest's only row, still inside its scroll box.
    let chest_floor = (chest.x + 10, chest.bottom() + 20);
    let lamp = row("Panel_List_pack", 2);

    press_drag_release(&mut ui, rope, gold);
    let drops: Vec<_> = actions(&mut ui)
        .into_iter()
        .filter_map(|a| match a {
            UiAction::PanelEvent { control, drop, .. } => Some((control, drop)),
            _ => None,
        })
        .collect();
    // The press picks Rope; letting go over Gold drops it there.
    assert_eq!(
        drops,
        [
            ("pack".to_owned(), None),
            ("chest".to_owned(), dropped("pack", 1, Some(0)))
        ]
    );

    press_drag_release(&mut ui, lamp, chest_floor);
    let last = actions(&mut ui).pop();
    assert!(
        matches!(&last, Some(UiAction::PanelEvent { control, drop, .. })
            if control == "chest" && *drop == dropped("pack", 2, None)),
        "{last:?}"
    );

    // A list the rules did not let drag takes nothing, and a press that
    // does not move is a plain pick.
    press_drag_release(&mut ui, rope, log);
    assert!(
        actions(&mut ui)
            .iter()
            .all(|a| matches!(a, UiAction::PanelEvent { drop: None, .. })),
    );
    press_drag_release(&mut ui, gold, gold);
    assert!(
        actions(&mut ui)
            .iter()
            .all(|a| matches!(a, UiAction::PanelEvent { drop: None, .. })),
    );
}

#[test]
fn a_menu_panel_draws_its_pictures_with_no_frame_and_steps_aside_when_not_modal() {
    let picture = |key: u64| PanelPicture {
        key,
        size: [64, 64],
    };
    let mut menu = shop();
    menu.width = 640;
    menu.height = 480;
    menu.border = false;
    menu.modal = false;
    menu.background = Some(PanelBackgroundView {
        picture: Some(picture(1)),
        slice: Some([8, 8, 8, 8]),
        tint: [1.0; 4],
    });
    menu.items = vec![
        PanelItemView::Image {
            picture: Some(picture(2)),
            rect: [20, 10, 200, 60],
            tint: [1.0, 1.0, 1.0, 0.5],
        },
        PanelItemView::Buttons(vec![PanelButtonView {
            picture: Some(picture(3)),
            hover_picture: Some(picture(4)),
            color: Some([1.0, 1.0, 0.0, 1.0]),
            rect: Some([30, 100, 120, 30]),
            ..button("buy", "Buy")
        }]),
    ];
    let mut ui = ui();
    ui.apply(UiUpdate::Panel(Some(menu)));
    ui.update(0);
    let v = view(&ui);
    assert!(
        v.walk().all(|n| v.node(n).ctrl.class != "GuiWindowCtrl"),
        "no frame"
    );
    let background = v.node(v.id("Panel_Background").unwrap());
    assert_eq!(background.state.external_texture, Some(1));
    assert_eq!(background.state.slice, Some([8, 8, 8, 8]));
    assert_eq!(background.rect.w, 640);
    let image = v.node(v.id("Panel_Image_0").unwrap());
    assert_eq!(image.state.external_texture, Some(2));
    assert_eq!((image.rect.w, image.rect.h), (200, 60));
    let buy = v.node(v.id("Panel_Button_buy").unwrap());
    assert_eq!(
        (buy.state.external_texture, buy.state.hover_texture),
        (Some(3), Some(4))
    );
    assert_eq!((buy.rect.w, buy.rect.h), (120, 30));
    let panel = ui.screen(ScreenId::AddOnPanel).unwrap();
    assert!(!panel.modal() && panel.wants_mouse());
    event(&mut ui, "Panel_Button_buy", EventKind::Click);
    assert_eq!(actions(&mut ui).len(), 1);
}

fn menu() -> PanelView {
    let mut buttons = vec![
        button("options", "Settings"),
        button("leave", "Disconnect"),
        button("exit", "Quit"),
        button("help", "Help"),
    ];
    buttons[0].action = Some(PanelAction::Settings);
    buttons[1].action = Some(PanelAction::Disconnect);
    buttons[2].action = Some(PanelAction::Quit);
    PanelView {
        package: "rp".into(),
        id: "shop".into(),
        title: "Menu".into(),
        width: 320,
        height: 300,
        items: vec![
            PanelItemView::Text {
                text: "Orders <b>of the day</b>".into(),
                style: PanelTextView {
                    size: Some(18),
                    font: Some("impact".into()),
                    color: Some([1.0, 0.0, 0.0, 1.0]),
                    align: PanelAlign::Center,
                    height: Some(60),
                    place: None,
                },
            },
            PanelItemView::List {
                id: "team".into(),
                rows: vec!["Ann\tRifleman".into(), "Bo\tMedic".into()],
                selected: None,
                height: 80,
                drag: false,
                columns: vec![0, 100],
                icons: vec![
                    Some(PanelPicture {
                        key: 7,
                        size: [16, 16],
                    }),
                    None,
                ],
                place: None,
            },
            PanelItemView::Buttons(buttons),
        ],
        border: true,
        modal: true,
        background: None,
    }
}

#[test]
fn panel_text_has_its_own_look_and_scrolls_and_lists_have_columns_and_row_pictures() {
    let mut ui = ui();
    ui.apply(UiUpdate::Panel(Some(menu())));
    ui.update(0);
    let v = view(&ui);
    // The styled text sits in a scroll box of the asked height, its markup
    // set by the engine and the rules' own text still escaped.
    let text = v
        .walk()
        .find(|n| v.node(*n).ctrl.class == "GuiMLTextCtrl")
        .unwrap();
    assert_eq!(
        v.node(text).ctrl.text.as_deref(),
        Some("<font:impact:18><color:ff0000ff><just:center>Orders ‹b›of the day‹/b›")
    );
    let scroll = v.node(text).parent.unwrap();
    assert_eq!(v.node(scroll).ctrl.class, "GuiScrollCtrl");
    assert_eq!(v.node(scroll).rect.h, 60);
    // Columns start after the row picture, which the list draws per row.
    let list = v.id("Panel_List_team").unwrap();
    let lead = View::profile_row_height(&ui.core.pack, "GuiTextListProfile");
    assert_eq!(
        v.node(list).ctrl.field("columns"),
        Some(format!("{} {}", lead, 100 + lead).as_str())
    );
    assert_eq!(v.node(list).state.row_icons, [Some(7), None]);
    assert_eq!(v.node(list).state.items[0].0, "Ann\tRifleman");
}

#[test]
fn a_button_with_an_action_is_done_by_the_client_and_sends_no_event() {
    let mut ui = ui();
    ui.apply(UiUpdate::Panel(Some(menu())));
    ui.update(0);
    event(&mut ui, "Panel_Button_leave", EventKind::Click);
    assert_eq!(actions(&mut ui), [UiAction::Disconnect]);
    event(&mut ui, "Panel_Button_exit", EventKind::Click);
    assert_eq!(actions(&mut ui), [UiAction::Quit]);
    event(&mut ui, "Panel_Button_options", EventKind::Click);
    assert!(actions(&mut ui).is_empty());
    ui.update(0);
    assert!(ui.is_open(ScreenId::Options));
    // A button without one still tells the rules.
    event(&mut ui, "Panel_Button_help", EventKind::Click);
    let sent = actions(&mut ui);
    assert!(matches!(&sent[..], [UiAction::PanelEvent { control, .. }] if control == "help"));
}

#[test]
fn a_placed_text_or_list_stands_where_asked_and_leaves_the_flow_alone() {
    use bri_ui::api::PanelPlace;
    let button_y = |panel: PanelView| {
        let mut ui = ui();
        ui.apply(UiUpdate::Panel(Some(panel)));
        ui.update(0);
        let v = view(&ui);
        v.node(v.id("Panel_Button_leave").unwrap()).rect.y
    };
    let mut panel = menu();
    let flowed = button_y(panel.clone());
    let PanelItemView::Text { style, .. } = &mut panel.items[0] else {
        panic!("the first item is text")
    };
    style.place = Some(PanelPlace {
        x: 200,
        y: 250,
        width: Some(90),
    });
    let PanelItemView::List { place, .. } = &mut panel.items[1] else {
        panic!("the second item is the list")
    };
    *place = Some(PanelPlace {
        x: 10,
        y: 260,
        width: None,
    });
    // The 320x300 panel stands centred in 640x480.
    let (left, top) = (160, 90);
    let mut ui = ui();
    ui.apply(UiUpdate::Panel(Some(panel.clone())));
    ui.update(0);
    let v = view(&ui);
    let text = v
        .walk()
        .find(|n| v.node(*n).ctrl.class == "GuiMLTextCtrl")
        .unwrap();
    let scroll = v.node(text).parent.unwrap();
    let r = v.node(scroll).rect;
    assert_eq!((r.x, r.y, r.w), (left + 200, top + 250, 90));
    let list = v.node(v.id("Panel_List_team").unwrap()).parent.unwrap();
    let r = v.node(list).rect;
    assert_eq!((r.x, r.y), (left + 10, top + 260));
    // The buttons flow up into the room the placed items left.
    assert!(button_y(panel) < flowed);
}
