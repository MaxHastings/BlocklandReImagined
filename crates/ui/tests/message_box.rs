//! Message boxes grow to fit their text, like v20's `MBSetText`.
use bri_ui::{
    api::Settings,
    binds::Platform,
    pack::Pack,
    screens::ScreenId,
    ui::{Callback, Ui, UiConfig},
};
use std::{path::PathBuf, rc::Rc};

#[test]
#[ignore = "requires content/ui-pack-004 (or BRI_CONTENT_ROOT)"]
fn a_long_yes_no_question_is_shown_in_full() -> anyhow::Result<()> {
    let root = std::env::var_os("BRI_CONTENT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content"));
    let pack = Rc::new(Pack::load(&root.join("ui-pack-004"))?);
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
    let need = bri_ui::view::View::ml_height_ctrl(&pack, &v.node(text).ctrl, &v.text_of(text), v.node(text).rect.w);
    assert!(need > 14, "the question should need several lines");
    assert!(v.node(text).rect.h >= need, "text clipped: {} < {need}", v.node(text).rect.h);
    let text_bottom = v.node(text).rect.y + v.node(text).rect.h;
    for &k in &v.node(frame).children {
        if k != text && v.node(k).ctrl.class == "GuiBitmapButtonCtrl" {
            assert!(v.node(k).rect.y >= text_bottom, "a button covers the text");
        }
    }
    let f = v.node(frame).rect;
    assert!(f.y + f.h <= 480 && f.y >= 0, "frame off screen: {f:?}");
    Ok(())
}
