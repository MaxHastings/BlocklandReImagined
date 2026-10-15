//! A rule's `screen_flash` tints the screen and fades away over its
//! seconds.
use bri_ui::{
    api::*,
    binds::Platform,
    pack::Pack,
    schema::UiPack,
    ui::{Ui, UiConfig},
};
use std::{path::PathBuf, rc::Rc};

#[test]
fn a_screen_flash_fades_over_its_seconds() {
    let mut ui = Ui::new(
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
    );
    ui.apply(UiUpdate::Flash {
        color: [0.2, 0.5, 1.0, 0.6],
        seconds: 0.5,
    });
    ui.apply(UiUpdate::Flash {
        color: [f32::NAN, 0.0, 0.0, 1.0],
        seconds: 1.0,
    });
    assert_eq!(ui.core.flashes.len(), 1, "a broken colour is dropped");
    ui.update(300);
    let (_, left, all) = ui.core.flashes[0];
    assert!((left - 0.2).abs() < 1e-4 && all == 0.5);
    ui.update(300);
    assert!(ui.core.flashes.is_empty(), "gone once faded");
}
