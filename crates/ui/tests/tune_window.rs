//! The Tune window: a body's numbers by part, a moved slider sent to the
//! host once it is still, a typed number sent at once, Undo and Save.
use bri_ui::{
    api::*,
    binds::Platform,
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

fn field(group: &str, path: &str, label: &str, value: f32) -> TuneField {
    TuneField {
        group: group.into(),
        path: path.into(),
        label: label.into(),
        value,
        min: 0.0,
        max: value * 2.0,
        unsaved: false,
    }
}

fn runner(forward: f32, unsaved: bool) -> TuneView {
    TuneView {
        body: "runner:archetype/runner".into(),
        name: "Runner".into(),
        fields: vec![
            field("Movement", "movement.forward", "Forward", forward),
            field("Movement", "movement.jump", "Jump", 10.0),
            field("Hands", "sockets.right_hand.offset.0", "Right hand X", 0.5),
        ],
        status: String::new(),
        unsaved,
        cannot_save: String::new(),
        movement_note: String::new(),
    }
}

fn view(ui: &Ui) -> &View {
    ui.screen(ScreenId::Tune)
        .expect("the Tune window is open")
        .view()
}

fn event(ui: &mut Ui, name: &str, kind: EventKind) {
    let node = view(ui).id(name).unwrap();
    let i = ui
        .dialogs
        .iter()
        .rposition(|s| s.id() == ScreenId::Tune)
        .unwrap();
    let (dialogs, core) = (&mut ui.dialogs, &mut ui.core);
    dialogs[i].on_event(&ViewEvent { node, kind }, core);
}

fn actions(ui: &mut Ui) -> Vec<UiAction> {
    ui.drain_actions().into_iter().map(|(_, a)| a).collect()
}

#[test]
fn moved_numbers_go_to_the_host_and_save_waits_for_a_change() {
    let mut ui = ui();
    ui.apply(UiUpdate::Tune(Some(runner(7.0, false))));
    ui.update(0);
    assert!(ui.is_open(ScreenId::Tune));
    let v = view(&ui);
    // Movement first: two rows, values shown.
    assert!(
        v.id("TB_Slider1")
            .is_some_and(|n| v.node(n).parent.is_some())
    );
    assert!(
        v.id("TB_Slider2")
            .is_none_or(|n| v.node(n).parent.is_none())
    );
    assert_eq!(v.text_of(v.id("TB_Value0").unwrap()), "7");
    // Nothing changed: Save and Undo are off.
    assert!(!v.node(v.id("TB_Save").unwrap()).state.active);
    assert!(!v.node(v.id("TB_Undo").unwrap()).state.active);
    // A slider moves; it is sent once it has been still.
    let n = view(&ui).id("TB_Slider0").unwrap();
    ui.dialogs.last_mut().unwrap().view_mut().set_num(n, 9.5);
    event(&mut ui, "TB_Slider0", EventKind::Changed);
    assert_eq!(view(&ui).text_of(view(&ui).id("TB_Value0").unwrap()), "9.5");
    ui.update(50);
    assert!(actions(&mut ui).is_empty(), "not while it moves");
    ui.update(300);
    assert_eq!(
        actions(&mut ui),
        [UiAction::TuneSet {
            body: "runner:archetype/runner".into(),
            changes: vec![("movement.forward".into(), 9.5)],
        }]
    );
    // The host's answer: the body plays it, unsaved.
    ui.apply(UiUpdate::Tune(Some(runner(9.5, true))));
    ui.update(0);
    let v = view(&ui);
    assert!(v.node(v.id("TB_Save").unwrap()).state.active);
    // A typed number goes at once, past the slider's end too.
    let n = view(&ui).id("TB_Value1").unwrap();
    ui.dialogs.last_mut().unwrap().view_mut().set_text(n, "25");
    event(&mut ui, "TB_Value1", EventKind::Submit);
    assert_eq!(
        actions(&mut ui),
        [UiAction::TuneSet {
            body: "runner:archetype/runner".into(),
            changes: vec![("movement.jump".into(), 25.0)],
        }]
    );
    // Another part shows its own rows.
    let group = view(&ui).id("TB_Group").unwrap();
    ui.dialogs
        .last_mut()
        .unwrap()
        .view_mut()
        .select(group, Some(1));
    event(&mut ui, "TB_Group", EventKind::Changed);
    let v = view(&ui);
    assert_eq!(v.text_of(v.id("TB_Value0").unwrap()), "0.5");
    // Unsaved numbers are marked, by part and by row, and named.
    let mut tuned = runner(9.5, true);
    tuned.fields[0].unsaved = true;
    ui.apply(UiUpdate::Tune(Some(tuned)));
    ui.update(0);
    let v = view(&ui);
    assert!(
        v.text_of(v.id("TB_Status").unwrap())
            .contains("not saved (*): Forward")
    );
    assert_eq!(
        v.node(v.id("TB_Group").unwrap()).state.items[0].0,
        "Movement *"
    );
    assert_eq!(v.node(v.id("TB_Group").unwrap()).state.items[1].0, "Hands");
    assert_eq!(v.text_of(v.id("TB_Label0").unwrap()), "Right hand X");
    // Done with numbers unsaved says so first; Done again closes.
    event(&mut ui, "TB_Done", EventKind::Click);
    assert!(actions(&mut ui).is_empty());
    assert!(ui.is_open(ScreenId::Tune));
    let v = view(&ui);
    assert!(
        v.text_of(v.id("TB_Status").unwrap())
            .contains("Not saved: Forward. Save to Add-On keeps them")
    );
    event(&mut ui, "TB_Save", EventKind::Click);
    assert_eq!(
        actions(&mut ui),
        [UiAction::TuneSave {
            body: "runner:archetype/runner".into()
        }]
    );
    // Saved: nothing is marked, and Done closes at once.
    ui.apply(UiUpdate::Tune(Some(runner(9.5, false))));
    ui.update(0);
    let v = view(&ui);
    assert_eq!(
        v.node(v.id("TB_Group").unwrap()).state.items[0].0,
        "Movement"
    );
    event(&mut ui, "TB_Done", EventKind::Click);
    assert_eq!(actions(&mut ui), [UiAction::TuneClosed]);
    ui.update(0);
    assert!(!ui.is_open(ScreenId::Tune));
}

#[test]
fn a_sent_number_holds_its_row_and_a_typed_one_needs_no_enter() {
    let mut ui = ui();
    ui.apply(UiUpdate::Tune(Some(runner(7.0, false))));
    ui.update(0);
    let n = view(&ui).id("TB_Slider0").unwrap();
    ui.dialogs.last_mut().unwrap().view_mut().set_num(n, 9.5);
    event(&mut ui, "TB_Slider0", EventKind::Changed);
    ui.update(300);
    assert_eq!(actions(&mut ui).len(), 1, "sent");
    // Another update arrives before the tuned body: the row keeps 9.5
    // rather than jumping back to 7 and forward again.
    ui.apply(UiUpdate::Tune(Some(runner(7.0, false))));
    ui.update(0);
    let v = view(&ui);
    assert_eq!(v.text_of(v.id("TB_Value0").unwrap()), "9.5");
    assert_eq!(v.num(v.id("TB_Slider0").unwrap()), 9.5);
    let mut tuned = runner(9.5, true);
    tuned.fields[0].unsaved = true;
    ui.apply(UiUpdate::Tune(Some(tuned)));
    ui.update(0);
    let v = view(&ui);
    assert_eq!(v.text_of(v.id("TB_Label0").unwrap()), "Forward *");
    assert_eq!(v.text_of(v.id("TB_Label1").unwrap()), "Jump");
    ui.apply(UiUpdate::Tune(Some(runner(7.0, false))));
    ui.update(0);
    // A number the host never shows lets the row go after a while.
    ui.update(2500);
    let v = view(&ui);
    assert_eq!(v.text_of(v.id("TB_Value0").unwrap()), "7");
    // A number typed and left without Enter is sent too.
    let n = view(&ui).id("TB_Value1").unwrap();
    ui.dialogs.last_mut().unwrap().view_mut().set_text(n, "12");
    event(&mut ui, "TB_Value1", EventKind::Changed);
    ui.update(16);
    assert_eq!(
        actions(&mut ui),
        [UiAction::TuneSet {
            body: "runner:archetype/runner".into(),
            changes: vec![("movement.jump".into(), 12.0)],
        }]
    );
    // A typo says so, and sends nothing.
    ui.dialogs
        .last_mut()
        .unwrap()
        .view_mut()
        .set_text(n, "fast");
    event(&mut ui, "TB_Value1", EventKind::Submit);
    assert!(actions(&mut ui).is_empty());
    let mut controlled = runner(7.0, false);
    controlled.movement_note = "This body has its own movement controller.".into();
    ui.apply(UiUpdate::Tune(Some(controlled)));
    ui.update(0);
    let v = view(&ui);
    let text = (0..v.nodes.len())
        .map(|i| v.text_of(i))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("its own movement controller"), "{text}");
}
