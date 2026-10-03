//! Widget interaction on a synthetic layout (no original content needed).
use bri_ui::input::{Key, Modifiers, MouseButton};
use bri_ui::pack::Pack;
use bri_ui::schema::{Control, UiPack};
use bri_ui::view::{EventKind, View, ViewEvent};

fn c(class: &str, name: &str, pos: [i32; 2], ext: [i32; 2]) -> Control {
    Control {
        class: class.into(),
        name: Some(name.into()),
        position: pos,
        extent: ext,
        visible: true,
        style: "GuiDefaultProfile".into(),
        ..Default::default()
    }
}

fn layout() -> Control {
    let mut root = c("GuiControl", "root", [0, 0], [640, 480]);
    let mut a = c("GuiTextEditCtrl", "a", [10, 10], [100, 18]);
    a.fields.insert("maxLength".into(), "5".into());
    let b = c("GuiTextEditCtrl", "b", [10, 40], [100, 18]);
    let mut ok = c("GuiBitmapButtonCtrl", "ok", [10, 70], [90, 38]);
    ok.accelerator = Some("return".into());
    let mut cancel = c("GuiBitmapButtonCtrl", "cancel", [110, 70], [90, 38]);
    cancel.accelerator = Some("escape".into());
    let mut r1 = c("GuiRadioCtrl", "r1", [10, 120], [60, 20]);
    r1.group = Some(1);
    let mut r2 = c("GuiRadioCtrl", "r2", [80, 120], [60, 20]);
    r2.group = Some(1);
    let chk = c("GuiCheckBoxCtrl", "chk", [10, 150], [60, 20]);
    let pop = c("GuiPopUpMenuCtrl", "pop", [10, 180], [100, 18]);
    root.children = vec![a, b, ok, cancel, r1, r2, chk, pop];
    root
}

fn click(v: &mut View, p: &Pack, x: i32, y: i32) -> Vec<ViewEvent> {
    let mut out = Vec::new();
    v.mouse_move(x, y, &mut out);
    out.clear();
    v.mouse_down(MouseButton::Left, x, y, p, &mut out);
    v.mouse_up(MouseButton::Left, x, y, p, &mut out);
    out
}

#[test]
fn focus_typing_tab_and_accelerators() {
    let pack = Pack::from_parts(UiPack::default(), ".".into());
    let mut v = View::new(&layout());
    v.layout(1024, 768);
    let (a, b) = (v.id("a").unwrap(), v.id("b").unwrap());
    click(&mut v, &pack, 20, 15);
    assert_eq!(v.focus, Some(a));
    let mut out = Vec::new();
    for ch in "hello world".chars() {
        v.char(ch, &mut out);
    }
    assert_eq!(v.edit_text(a), "hello", "maxLength enforced");
    assert!(v.key(Key::Backspace, Modifiers::NONE, &mut out));
    assert!(v.key(Key::Home, Modifiers::NONE, &mut out));
    v.char('>', &mut out);
    assert_eq!(v.edit_text(a), ">hell");
    // Tab / Shift+Tab move focus between edits.
    v.key(Key::Tab, Modifiers::NONE, &mut out);
    assert_eq!(v.focus, Some(b));
    v.key(
        Key::Tab,
        Modifiers {
            shift: true,
            ..Modifiers::NONE
        },
        &mut out,
    );
    assert_eq!(v.focus, Some(a));
    // Enter in an edit submits; accelerators resolve by key.
    out.clear();
    v.key(Key::Return, Modifiers::NONE, &mut out);
    assert_eq!(
        out,
        vec![ViewEvent {
            node: a,
            kind: EventKind::Submit
        }]
    );
    assert_eq!(v.accelerator(Key::Escape, Modifiers::NONE), v.id("cancel"));
    assert_eq!(v.accelerator(Key::NumpadEnter, Modifiers::NONE), v.id("ok"));
    // Hidden controls lose their accelerator.
    let cancel = v.id("cancel").unwrap();
    v.set_visible(cancel, false);
    assert_eq!(v.accelerator(Key::Escape, Modifiers::NONE), None);
    // Clicking outside clears focus.
    click(&mut v, &pack, 600, 400);
    assert_eq!(v.focus, None);
}

#[test]
fn radios_checkbox_popup() {
    let pack = Pack::from_parts(UiPack::default(), ".".into());
    let mut v = View::new(&layout());
    v.layout(640, 480);
    let (r1, r2, chk, pop) = (
        v.id("r1").unwrap(),
        v.id("r2").unwrap(),
        v.id("chk").unwrap(),
        v.id("pop").unwrap(),
    );
    click(&mut v, &pack, 15, 125);
    assert!(v.bool_value(r1));
    click(&mut v, &pack, 85, 125);
    assert!(
        v.bool_value(r2) && !v.bool_value(r1),
        "radio group is exclusive"
    );
    let ev = click(&mut v, &pack, 15, 155);
    assert!(v.bool_value(chk));
    assert!(ev.contains(&ViewEvent {
        node: chk,
        kind: EventKind::Changed
    }));
    // Press on a button, release elsewhere: no click (cancelled).
    let ok = v.id("ok").unwrap();
    let mut out = Vec::new();
    v.mouse_down(MouseButton::Left, 20, 80, &pack, &mut out);
    v.mouse_up(MouseButton::Left, 500, 400, &pack, &mut out);
    assert!(
        !out.iter()
            .any(|e| e.node == ok && e.kind == EventKind::Click)
    );
    // Popup opens on click, Escape closes it, a row click selects.
    v.state(pop).items = vec![("Four".into(), 4), ("Eight".into(), 8)];
    click(&mut v, &pack, 20, 185);
    assert_eq!(v.open_popup_node(), Some(pop));
    assert!(v.key(Key::Escape, Modifiers::NONE, &mut out));
    assert_eq!(v.open_popup_node(), None);
    click(&mut v, &pack, 20, 185);
    let ev = click(&mut v, &pack, 20, 198 + 16 + 4);
    assert_eq!(v.selected(pop), Some(8));
    assert!(ev.contains(&ViewEvent {
        node: pop,
        kind: EventKind::Changed
    }));
}

#[test]
fn grouped_popup_discloses_without_changing_values_and_searches_hidden_choices() {
    let pack = Pack::from_parts(UiPack::default(), ".".into());
    let mut v = View::new(&layout());
    v.layout(640, 480);
    let pop = v.id("pop").unwrap();
    v.state(pop).items = vec![
        ("-".into(), -1),
        ("onActivate".into(), 4),
        ("onActivate(Team1)".into(), 7),
        ("onActivate(Team2)".into(), 8),
    ];
    for id in [7, 8] {
        v.state(pop)
            .popup_groups
            .insert(id, "Activate by team".into());
    }
    v.state(pop)
        .popup_aliases
        .insert(8, "Blue team button".into());
    v.select(pop, Some(4));
    click(&mut v, &pack, 20, 185);
    assert_eq!(
        v.popup_rows()
            .iter()
            .map(|(text, _)| text.as_str())
            .collect::<Vec<_>>(),
        ["-", "onActivate", "Activate by team >"]
    );
    let mut out = Vec::new();
    v.key(Key::End, Modifiers::NONE, &mut out);
    v.key(Key::Return, Modifiers::NONE, &mut out);
    assert!(out.is_empty(), "opening a family never changes the event");
    assert_eq!(v.selected(pop), Some(4));
    assert_eq!(v.popup_rows()[0].0, "< Back");
    v.key(Key::End, Modifiers::NONE, &mut out);
    v.key(Key::Return, Modifiers::NONE, &mut out);
    assert_eq!(v.selected(pop), Some(8));
    assert_eq!(
        out,
        [ViewEvent {
            node: pop,
            kind: EventKind::Changed
        }]
    );

    click(&mut v, &pack, 20, 185);
    assert!(
        v.popup_rows().iter().any(|(_, id)| *id == 8),
        "keep authored grouped value visible"
    );
    out.clear();
    for ch in "blue".chars() {
        v.char(ch, &mut out);
    }
    assert_eq!(v.popup_highlight().map(|(_, id)| id), Some(8));
    v.key(Key::Return, Modifiers::NONE, &mut out);
    assert_eq!(v.selected(pop), Some(8));

    click(&mut v, &pack, 20, 185);
    v.key(Key::End, Modifiers::NONE, &mut out);
    v.key(Key::Return, Modifiers::NONE, &mut out);
    out.clear();
    v.key(Key::Escape, Modifiers::NONE, &mut out);
    assert!(
        v.open_popup_node().is_some(),
        "first Escape returns from the family"
    );
    v.key(Key::Escape, Modifiers::NONE, &mut out);
    assert!(v.open_popup_node().is_none());
    assert!(out.is_empty());
    assert_eq!(v.selected(pop), Some(8));
}

#[test]
fn resize_rules_scale_layouts() {
    use bri_ui::geom::Rect;
    use bri_ui::schema::{HSizing, VSizing};
    use bri_ui::view::resize;
    let a = Rect::new(209, 29, 221, 421);
    assert_eq!(
        resize(
            a,
            HSizing::Center,
            VSizing::Center,
            [8, 2],
            (640, 480),
            (1920, 1080)
        ),
        Rect::new(849, 329, 221, 421)
    );
    let m = Rect::new(0, 160, 224, 40);
    assert_eq!(
        resize(
            m,
            HSizing::Relative,
            VSizing::Relative,
            [8, 2],
            (640, 480),
            (1280, 960)
        ),
        Rect::new(0, 320, 448, 80)
    );
    assert_eq!(
        resize(
            m,
            HSizing::Width,
            VSizing::Top,
            [8, 2],
            (640, 480),
            (1024, 768)
        ),
        Rect::new(0, 448, 608, 40)
    );
}

#[test]
fn long_popup_scrolls_and_takes_keyboard_and_drag_selection() {
    let pack = Pack::from_parts(UiPack::default(), ".".into());
    let mut v = View::new(&layout());
    v.layout(640, 480);
    let pop = v.id("pop").unwrap();
    v.state(pop).items = (0..30).map(|i| (format!("Item {i}"), i * 10)).collect();
    // Rows are 16px; the default 200px popup height shows 12 of 30.
    click(&mut v, &pack, 20, 185);
    assert_eq!(
        v.open_popup_node(),
        Some(pop),
        "release over the control keeps it open"
    );
    assert!(v.wheel(-3), "wheel scrolls the open list");
    let ev = click(&mut v, &pack, 20, 198 + 1 + 8);
    assert_eq!(
        v.selected(pop),
        Some(30),
        "first visible row after scrolling"
    );
    assert!(ev.contains(&ViewEvent {
        node: pop,
        kind: EventKind::Changed
    }));
    assert_eq!(v.open_popup_node(), None);

    let mut out = Vec::new();
    click(&mut v, &pack, 20, 185);
    for k in [Key::Down, Key::Down, Key::Return] {
        assert!(v.key(k, Modifiers::NONE, &mut out));
    }
    assert_eq!(
        v.selected(pop),
        Some(50),
        "keyboard starts at the selection"
    );
    click(&mut v, &pack, 20, 185);
    for k in [Key::End, Key::Return] {
        v.key(k, Modifiers::NONE, &mut out);
    }
    assert_eq!(v.selected(pop), Some(290), "End reaches the last item");
    click(&mut v, &pack, 20, 185);
    assert!(
        v.key(Key::Letter('q'), Modifiers::NONE, &mut out),
        "an open list owns the keyboard"
    );
    v.key(Key::Escape, Modifiers::NONE, &mut out);
    assert_eq!(v.selected(pop), Some(290));

    // Press on the control, drag onto a row, release: Torque drag-select.
    let mut out = Vec::new();
    v.mouse_move(20, 185, &mut out);
    v.mouse_down(MouseButton::Left, 20, 185, &pack, &mut out);
    let row = v.open_popup_node().map(|_| 198 + 1 + 16 * 2 + 8).unwrap();
    v.mouse_move(20, row, &mut out);
    v.mouse_up(MouseButton::Left, 20, row, &pack, &mut out);
    assert_eq!(v.open_popup_node(), None);
    assert_eq!(
        v.selected(pop),
        Some(200),
        "End scrolled the list to items 18-29"
    );
}

#[test]
fn scroll_panes_move_their_content_by_wheel_arrows_track_and_thumb() {
    let pack = Pack::from_parts(UiPack::default(), ".".into());
    let mut root = c("GuiControl", "root", [0, 0], [640, 480]);
    let mut scroll = c("GuiScrollCtrl", "scroll", [10, 10], [200, 100]);
    scroll.fields.insert("rowHeight".into(), "20".into());
    scroll.children = vec![c("GuiControl", "page", [0, 0], [188, 400])];
    root.children = vec![scroll];
    let mut v = View::new(&root);
    v.layout(640, 480);
    let (scroll, page) = (v.id("scroll").unwrap(), v.id("page").unwrap());
    let mut out = Vec::new();
    v.mouse_move(50, 50, &mut out);
    assert!(v.wheel(-2));
    assert_eq!(v.node(page).rect.y, 10 - 40, "content moves with the wheel");
    // The bar is the right 12px (fallback skin); arrows are 12px tall.
    click(&mut v, &pack, 205, 105);
    assert_eq!(v.node(scroll).state.scroll_y, 60, "down arrow steps a row");
    click(&mut v, &pack, 205, 15);
    assert_eq!(v.node(scroll).state.scroll_y, 40, "up arrow steps back");
    click(&mut v, &pack, 205, 95);
    assert_eq!(
        v.node(scroll).state.scroll_y,
        140,
        "track pages by the view"
    );
    // Drag the thumb to the bottom of the track.
    v.mouse_move(205, 60, &mut out);
    // Track 22..98, thumb 76 * 100 / 400 = 19px tall.
    let thumb_y = 22 + (76 - 19) * 140 / 300;
    v.mouse_down(MouseButton::Left, 205, thumb_y + 2, &pack, &mut out);
    v.mouse_move(205, 200, &mut out);
    v.mouse_up(MouseButton::Left, 205, 200, &pack, &mut out);
    assert_eq!(v.node(scroll).state.scroll_y, 300);
    assert_eq!(v.node(page).rect.y, 10 - 300);
}

fn typed(v: &mut View, s: &str) {
    let mut out = Vec::new();
    for ch in s.chars() {
        assert!(v.char(ch, &mut out), "an open list takes typed characters");
    }
}

fn row_texts(v: &View) -> Vec<String> {
    v.popup_rows().into_iter().map(|(t, _)| t).collect()
}

#[test]
fn popup_filter_ranks_prefix_then_substring_and_pins_none() {
    use bri_ui::view::filter_popup_items;
    let items: Vec<(String, i64)> = [
        " NONE",
        "Blue Light",
        "Vehicle Bubbles",
        "Bubbles",
        "Vehicle Smoke",
        "vehicle fire",
    ]
    .iter()
    .enumerate()
    .map(|(i, t)| (t.to_string(), i as i64))
    .collect();
    assert_eq!(
        filter_popup_items(&items, ""),
        vec![0, 1, 2, 3, 4, 5],
        "empty query keeps the list"
    );
    assert_eq!(
        filter_popup_items(&items, "VEHICLE"),
        vec![0, 2, 4, 5],
        "case-insensitive, NONE pinned"
    );
    assert_eq!(
        filter_popup_items(&items, "bub"),
        vec![0, 3, 2],
        "prefix matches before substring matches"
    );
    assert_eq!(
        filter_popup_items(&items, "zzz"),
        vec![0],
        "only NONE when nothing matches"
    );
    let events: Vec<(String, i64)> = ["-", "fakeKillBrick", "fireRelay", "toggle"]
        .iter()
        .enumerate()
        .map(|(i, t)| (t.to_string(), i as i64))
        .collect();
    assert_eq!(
        filter_popup_items(&events, "f"),
        vec![0, 1, 2],
        "the event editor's '-' is pinned too"
    );
}

#[test]
fn list_search_ranks_like_the_popups_without_pinning() {
    use bri_ui::view::search_rows;
    let rows = ["none", "Slate Race", "Castle Slate", "Bubbles"];
    assert_eq!(
        search_rows(&rows, "  "),
        vec![0, 1, 2, 3],
        "blank keeps the list"
    );
    assert_eq!(
        search_rows(&rows, " SLATE"),
        vec![1, 2],
        "prefix, then substring"
    );
    assert!(search_rows(&rows, "zzz").is_empty(), "no row is pinned");
}

#[test]
fn typing_into_an_open_popup_filters_completes_and_keys_pick() {
    let pack = Pack::from_parts(UiPack::default(), ".".into());
    let mut v = View::new(&layout());
    v.layout(640, 480);
    let pop = v.id("pop").unwrap();
    v.state(pop).items = [
        " NONE",
        "Blue Light",
        "Vehicle Bubbles",
        "Bubbles",
        "Vehicle Smoke",
    ]
    .iter()
    .enumerate()
    .map(|(i, t)| (t.to_string(), i as i64))
    .collect();
    v.select(pop, Some(1));
    let mut out = Vec::new();

    // Opens with an empty query, the full list and the choice highlighted.
    click(&mut v, &pack, 20, 185);
    assert_eq!(v.popup_query(), Some(""));
    assert_eq!(row_texts(&v).len(), 5);
    assert_eq!(v.popup_highlight().map(|(_, i)| i), Some(1));
    assert_eq!(v.popup_ghost(), None);

    // Typing filters at once; the top match is highlighted and ghost-completed.
    typed(&mut v, "veh");
    assert_eq!(row_texts(&v), [" NONE", "Vehicle Bubbles", "Vehicle Smoke"]);
    assert_eq!(v.popup_highlight().map(|(_, i)| i), Some(2));
    assert_eq!(v.popup_ghost().as_deref(), Some("icle Bubbles"));
    // Down moves to the next match; Up then Up reaches the pinned NONE.
    v.key(Key::Down, Modifiers::NONE, &mut out);
    assert_eq!(v.popup_highlight().map(|(_, i)| i), Some(4));
    assert_eq!(v.popup_ghost().as_deref(), Some("icle Smoke"));
    v.key(Key::Up, Modifiers::NONE, &mut out);
    v.key(Key::Up, Modifiers::NONE, &mut out);
    assert_eq!(v.popup_highlight().map(|(_, i)| i), Some(0));
    assert_eq!(v.popup_ghost(), None, "NONE does not start with the query");

    // Backspace widens the filter; Escape clears it, then closes.
    v.key(Key::Backspace, Modifiers::NONE, &mut out);
    assert_eq!(v.popup_query(), Some("ve"));
    typed(&mut v, "hicle s");
    assert_eq!(
        row_texts(&v),
        [" NONE", "Vehicle Smoke", "Vehicle Bubbles"],
        "exact phrase first, then every query word anywhere in the label"
    );
    assert_eq!(v.popup_highlight().map(|(_, id)| id), Some(4));
    assert_eq!(v.popup_ghost().as_deref(), Some("moke"));
    typed(&mut v, "mo");
    assert_eq!(row_texts(&v), [" NONE", "Vehicle Smoke"]);
    assert!(v.key(Key::Escape, Modifiers::NONE, &mut out));
    assert_eq!(v.popup_query(), Some(""), "Escape clears the query first");
    assert_eq!(row_texts(&v).len(), 5, "empty query restores the list");
    assert_eq!(v.popup_highlight().map(|(_, i)| i), Some(1));
    assert!(v.key(Key::Escape, Modifiers::NONE, &mut out));
    assert_eq!(v.open_popup_node(), None, "a second Escape closes");
    assert_eq!(v.selected(pop), Some(1), "closing changes nothing");

    // Enter takes the highlighted match.
    click(&mut v, &pack, 20, 185);
    typed(&mut v, "BUB");
    assert_eq!(row_texts(&v), [" NONE", "Bubbles", "Vehicle Bubbles"]);
    out.clear();
    assert!(v.key(Key::Return, Modifiers::NONE, &mut out));
    assert_eq!(v.selected(pop), Some(3));
    assert!(out.contains(&ViewEvent {
        node: pop,
        kind: EventKind::Changed
    }));

    // Tab accepts too, and a reopened list starts unfiltered.
    click(&mut v, &pack, 20, 185);
    assert_eq!(v.popup_query(), Some(""));
    typed(&mut v, "smo");
    assert!(v.key(Key::Tab, Modifiers::NONE, &mut out));
    assert_eq!(v.selected(pop), Some(4));

    // Clicking a row of the filtered list picks that row's item.
    click(&mut v, &pack, 20, 185);
    typed(&mut v, "vehicle");
    let ev = click(&mut v, &pack, 20, 198 + 16 + 4);
    assert_eq!(
        v.selected(pop),
        Some(2),
        "second shown row, not second item"
    );
    assert!(ev.contains(&ViewEvent {
        node: pop,
        kind: EventKind::Changed
    }));

    // Nothing matching: Enter closes without changing the choice.
    click(&mut v, &pack, 20, 185);
    typed(&mut v, "zzz");
    assert_eq!(v.popup_highlight(), None);
    v.key(Key::Return, Modifiers::NONE, &mut out);
    assert_eq!(v.open_popup_node(), None);
    assert_eq!(v.selected(pop), Some(2));
}

#[test]
fn popup_filter_stays_fast_on_long_lists() {
    let pack = Pack::from_parts(UiPack::default(), ".".into());
    let mut v = View::new(&layout());
    v.layout(640, 480);
    let pop = v.id("pop").unwrap();
    v.state(pop).items = (0..20_000)
        .map(|i| (format!("Emitter {i:05}"), i))
        .collect();
    click(&mut v, &pack, 20, 185);
    let start = std::time::Instant::now();
    typed(&mut v, "emitter 1999");
    assert_eq!(
        v.popup_rows()
            .into_iter()
            .map(|(_, id)| id)
            .collect::<Vec<_>>(),
        (19_990..20_000).chain([1_999, 11_999]).collect::<Vec<_>>(),
        "ten exact-phrase matches precede the two token-only matches"
    );
    assert_eq!(v.popup_highlight().map(|(_, id)| id), Some(19_990));
    assert!(
        start.elapsed().as_millis() < 500,
        "12 refilters of 20k rows took {:?}",
        start.elapsed()
    );
    let mut out = Vec::new();
    v.key(Key::End, Modifiers::NONE, &mut out);
    v.key(Key::Return, Modifiers::NONE, &mut out);
    assert_eq!(
        v.selected(pop),
        Some(11_999),
        "End selects the final ranked token match's authored ID"
    );
}
