//! Message boxes grow to fit their text, like v20's `MBSetText`.
use bri_ui::{
    api::Settings,
    binds::Platform,
    pack::Pack,
    schema::UiPack,
    screens::ScreenId,
    ui::{Callback, Ui, UiConfig},
};
use std::{path::PathBuf, rc::Rc};

fn a_long_yes_no_question_is_shown_in_full(pack: Rc<Pack>) {
    let mut ui = Ui::new(
        pack.clone(),
        UiConfig {
            size: (640, 480),
            scale: Some(1.0),
            platform: Platform::Windows,
        },
        Settings::default(),
    );
    ui.core.message_yes_no(
        "Windows Firewall",
        "Windows Firewall would stop friends from joining your game. Let Blockland ReImagined through? Windows will ask for permission once.",
        Callback::None,
    );
    ui.update(0);
    let screen = ui.screen(ScreenId::MessageBox).expect("message box");
    let v = screen.view();
    let text = v.id("MBYesNoText").unwrap();
    let frame = v.id("MBYesNoFrame").unwrap();
    let need = bri_ui::view::View::ml_height(
        &pack,
        &v.node(text).ctrl.style,
        &v.text_of(text),
        v.node(text).rect.w,
    );
    let line = pack
        .data
        .styles
        .get(&v.node(text).ctrl.style)
        .and_then(|s| s.font.as_deref())
        .and_then(|f| pack.font(f))
        .unwrap()
        .line_height as i32;
    assert!(need > line, "the question should need several lines");
    assert!(
        v.node(text).rect.h >= need,
        "text clipped: {} < {need}",
        v.node(text).rect.h
    );
    let text_bottom = v.node(text).rect.y + v.node(text).rect.h;
    let mut buttons = 0;
    for &k in &v.node(frame).children {
        if k != text && v.node(k).ctrl.class == "GuiBitmapButtonCtrl" {
            buttons += 1;
            assert!(v.node(k).rect.y >= text_bottom, "a button covers the text");
        }
    }
    assert!(buttons > 0);
    let f = v.node(frame).rect;
    assert!(f.y + f.h <= 480 && f.y >= 0, "frame off screen: {f:?}");
}

#[test]
fn a_long_yes_no_question_is_shown_in_full_synthetic() {
    let mut data = UiPack::default();
    bri_ui::testing::add_message_boxes(&mut data);
    a_long_yes_no_question_is_shown_in_full(bri_ui::testing::pack(data));
}

#[test]
#[ignore = "requires generated v20 content"]
fn a_long_yes_no_question_is_shown_in_full_content() -> anyhow::Result<()> {
    let root = std::env::var_os("BRI_CONTENT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content"));
    a_long_yes_no_question_is_shown_in_full(Rc::new(Pack::load(&root.join("ui-pack-004"))?));
    Ok(())
}
