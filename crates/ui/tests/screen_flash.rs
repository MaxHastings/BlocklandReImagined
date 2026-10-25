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

#[test]
fn overlays_add_and_end_at_their_seconds() {
    use bri_content::screen_overlay::{Ease, Overlay, Segment};
    let overlay = |peak| Overlay {
        seconds: 1.0,
        peak,
        light: [1.0; 3],
        dark: [0.0; 3],
        curve: vec![Segment {
            from: 0.0,
            to: None,
            start: 1.0,
            end: 1.0,
            ease: Ease::Linear,
        }],
    };
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
    ui.apply(UiUpdate::Overlay(overlay(0.25)));
    ui.apply(UiUpdate::Overlay(overlay(0.25)));
    ui.apply(UiUpdate::Overlay(overlay(f32::NAN)));
    assert_eq!(ui.core.overlays.len(), 2, "a broken one is dropped");
    ui.update(500);
    let sum: f32 = ui.core.overlays.iter().map(|(o, ran)| o.value(*ran)).sum();
    assert!((sum - 0.5).abs() < 1e-6);
    ui.update(600);
    assert!(ui.core.overlays.is_empty(), "gone at its seconds");
    // At most sixteen run; one more takes the place of the one nearest its end.
    for _ in 0..20 {
        ui.apply(UiUpdate::Overlay(overlay(1.0)));
    }
    assert_eq!(ui.core.overlays.len(), 16);
}
