//! Loading HUD state/render controls; no window or OS input.
use bri_ui::{
    api::*,
    binds::Platform,
    ui::{Ui, UiConfig},
};

#[test]
fn accepted_loading_screen_draws_and_clears_authoritative_typing_names() {
    let mut ui = Ui::new(
        bri_ui::testing::screens_pack(),
        UiConfig {
            size: (800, 600),
            scale: Some(1.0),
            platform: Platform::Windows,
        },
        Settings::default(),
    );
    ui.apply(UiUpdate::Connection(ConnectionState::Loading {
        map: "Bedroom".into(),
        preview: IconRef::None,
        status: "DOWNLOADING ADD-ONS".into(),
        progress: 0.25,
    }));
    ui.apply(UiUpdate::LoadingChat(true));
    let empty = ui.draw().cmds;
    ui.apply(UiUpdate::Talking(vec!["Bob".into()]));
    assert_ne!(
        ui.draw().cmds,
        empty,
        "accepted typing name must be visible before spawning"
    );
    ui.apply(UiUpdate::Talking(vec![]));
    assert_eq!(ui.draw().cmds, empty);
}
