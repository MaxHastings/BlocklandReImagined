//! A details pane the player scrolled down (Add-Ons, Game Modes) stays
//! where it is while unrelated updates arrive (chat, a conversion note the
//! client prints): it goes back to the top only for a new selection or new
//! text, as the Report window does. Runs on the made-up screens of
//! `bri_ui::testing::screens_pack`. No OS window or input is created.
use bri_ui::{
    api::*,
    binds::Platform,
    input::{InputEvent, MouseButton},
    screens::ScreenId,
    ui::{Ui, UiConfig},
};

fn ui() -> Ui {
    Ui::new(
        bri_ui::testing::screens_pack(),
        UiConfig {
            size: (1024, 768),
            scale: Some(1.0),
            platform: Platform::Windows,
        },
        Settings {
            binds: Some(vec![]),
            mouse_type: 2,
            ..Default::default()
        },
    )
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
fn scroll_of(u: &Ui, screen: ScreenId, name: &str) -> i32 {
    let view = u.screen(screen).unwrap().view();
    view.node(view.id(name).unwrap()).state.scroll_y
}
fn scroll_down(u: &mut Ui, screen: ScreenId, name: &str) {
    let view = u.screen_mut(screen).unwrap().view_mut();
    let id = view.id(name).unwrap();
    view.scroll_to(id, 120);
}
fn text_of(u: &Ui, screen: ScreenId, name: &str) -> String {
    let view = u.screen(screen).unwrap().view();
    view.text_of(view.id(name).unwrap())
}
fn long_text(what: &str) -> String {
    (0..80).map(|i| format!("{what} line {i}\n")).collect()
}
fn chat(u: &mut Ui) {
    u.apply(UiUpdate::Chat {
        text: "Converting add-on notes...".into(),
    });
    u.update(16);
}

#[test]
fn the_add_ons_details_keep_their_scroll_through_unrelated_updates() {
    let mut u = ui();
    let row = |id: &str| AddOnRow {
        id: id.into(),
        name: id.into(),
        category: "Weapons".into(),
        description: long_text(id),
        ..Default::default()
    };
    let rows = |first: AddOnRow| {
        UiUpdate::AddOns(AddOnsView {
            rows: vec![first, row("second")],
            notice: String::new(),
        })
    };
    u.apply(rows(row("first")));
    u.core.push(ScreenId::AddOns);
    u.update(16);
    assert_eq!(u.top_id(), ScreenId::AddOns);
    // Heading, first, second: the middle of the list is the first row.
    click(&mut u, ScreenId::AddOns, "AO_List");
    let details = text_of(&u, ScreenId::AddOns, "AO_Details");
    assert!(details.contains("first line 79"), "{details}");
    scroll_down(&mut u, ScreenId::AddOns, "AO_DetailScroll");
    let scrolled = scroll_of(&u, ScreenId::AddOns, "AO_DetailScroll");
    assert!(scrolled > 0, "the pane scrolls");
    chat(&mut u);
    assert_eq!(scroll_of(&u, ScreenId::AddOns, "AO_DetailScroll"), scrolled);
    // The same rows again (the host answering a request) change nothing.
    u.apply(rows(row("first")));
    u.update(16);
    assert_eq!(scroll_of(&u, ScreenId::AddOns, "AO_DetailScroll"), scrolled);
    // New text for the selected row shows from its top.
    u.apply(rows(AddOnRow {
        description: long_text("changed"),
        ..row("first")
    }));
    u.update(16);
    assert_eq!(scroll_of(&u, ScreenId::AddOns, "AO_DetailScroll"), 0);
}

#[test]
fn the_game_mode_details_keep_their_scroll_through_unrelated_updates() {
    let mut u = ui();
    u.apply(UiUpdate::GameModes(vec![GameModeInfo {
        id: "long:mode".into(),
        name: "Long".into(),
        description: long_text("mode"),
        map: None,
    }]));
    u.core
        .prefs
        .set(bri_ui::screens::modes::GAME_MODE, "long:mode");
    u.core.push(ScreenId::GameModes);
    u.update(16);
    assert_eq!(u.top_id(), ScreenId::GameModes);
    let details = text_of(&u, ScreenId::GameModes, "GM_Details");
    assert!(details.contains("mode line 79"), "{details}");
    scroll_down(&mut u, ScreenId::GameModes, "GM_DetailScroll");
    let scrolled = scroll_of(&u, ScreenId::GameModes, "GM_DetailScroll");
    assert!(scrolled > 0, "the pane scrolls");
    chat(&mut u);
    assert_eq!(
        scroll_of(&u, ScreenId::GameModes, "GM_DetailScroll"),
        scrolled
    );
}
