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
    assert_eq!(v.open_popup, Some(pop));
    assert!(v.key(Key::Escape, Modifiers::NONE, &mut out));
    assert_eq!(v.open_popup, None);
    click(&mut v, &pack, 20, 185);
    let ev = click(&mut v, &pack, 20, 198 + 16 + 4);
    assert_eq!(v.selected(pop), Some(8));
    assert!(ev.contains(&ViewEvent {
        node: pop,
        kind: EventKind::Changed
    }));
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
