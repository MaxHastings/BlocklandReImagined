//! Headless tests of the exact vendored X11 legacy-button mapping and real UI.
mod event {
    pub use winit::event::{ElementState, MouseScrollDelta};
}
#[path = "../../../vendor/winit/src/platform_impl/linux/x11/scroll_button.rs"]
mod scroll_button;

use bri_ui::{
    api::{BindInput, ConnectionState, IconRef, PaintDivision, Settings, ToolInfo, UiUpdate},
    binds::Platform,
    input::InputEvent,
    models::hud::Outbox,
    ui::{Ui, UiConfig},
};
use event::{ElementState, MouseScrollDelta};

fn notch(ui: &mut Ui, button: u32) {
    for state in [ElementState::Pressed, ElementState::Released] {
        if let Some(MouseScrollDelta::LineDelta(_, delta)) =
            scroll_button::scroll_button(button, state)
        {
            ui.handle_input(InputEvent::Wheel { delta });
        }
    }
}

#[test]
fn legacy_scroll_release_emits_nothing_in_every_direction() {
    for button in 4..=7 {
        assert!(scroll_button::scroll_button(button, ElementState::Pressed).is_some());
        assert!(scroll_button::scroll_button(button, ElementState::Released).is_none());
    }
    for button in [0, 1, 2, 3, 8, 9, u32::MAX] {
        assert!(scroll_button::scroll_button(button, ElementState::Pressed).is_none());
    }
}

#[test]
fn picked_up_fourth_tool_is_reachable_at_every_scale() {
    for scale in [1.0, 1.5, 2.0] {
        let mut ui = Ui::new(
            bri_ui::testing::screens_pack(),
            UiConfig {
                size: (1280, 960),
                scale: Some(scale),
                platform: Platform::Linux,
            },
            Settings::default(),
        );
        ui.apply(UiUpdate::Connection(ConnectionState::InGame {
            server_name: "Wheel regression".into(),
            max_players: 1,
            local: true,
            single_player: true,
            admin: true,
        }));
        ui.core.binds.bind(BindInput::Wheel, "scrollInventory");
        let tools = |count| {
            (0..4)
                .map(|i| {
                    (i < count).then(|| ToolInfo {
                        id: format!("tool{i}"),
                        name: format!("Tool {i}"),
                        icon: IconRef::None,
                        tint: None,
                    })
                })
                .collect()
        };
        ui.apply(UiUpdate::Tools(tools(3)));
        ui.core.hud.use_tools(&mut Outbox::default());
        ui.apply(UiUpdate::Tools(tools(4))); // Simulate the newly replicated pickup.
        for expected in [1, 2, 3, 0] {
            notch(&mut ui, 5);
            assert_eq!(ui.core.hud.cur_tool, Some(expected));
        }
    }
}

#[test]
fn adjacent_paint_swatch_is_reachable_at_every_scale() {
    for scale in [1.0, 1.5, 2.0] {
        let mut ui = Ui::new(
            bri_ui::testing::screens_pack(),
            UiConfig {
                size: (1280, 960),
                scale: Some(scale),
                platform: Platform::Linux,
            },
            Settings::default(),
        );
        ui.apply(UiUpdate::Connection(ConnectionState::InGame {
            server_name: "Paint wheel regression".into(),
            max_players: 1,
            local: true,
            single_player: true,
            admin: true,
        }));
        ui.core.binds.bind(BindInput::Wheel, "scrollInventory");
        ui.apply(UiUpdate::Colorset(vec![PaintDivision {
            name: "Paint".into(),
            colors: vec![[1.0; 4]; 8],
        }]));
        ui.core.hud.use_spray_can(&mut Outbox::default());
        let swatch = ui.core.hud.paint_swatch;
        notch(&mut ui, 5);
        assert_eq!(ui.core.hud.paint_swatch, (swatch + 1) % 8);
    }
}
