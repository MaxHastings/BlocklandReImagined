//! Real-content printer menu probe: every converted v20 print set reaches the
//! selector for its aspect ratio. Data-only UI; never creates a window.
//! Run: cargo test -p bri-client --test print_selector -- --ignored --nocapture
use bri_client::{content::ClientContent, tool_ui::ToolUi};
use bri_content::brick_materials::Bundle;
use bri_ui::{
    api::*,
    binds::Platform,
    input::*,
    screens::ScreenId,
    ui::{Ui, UiConfig},
};
use std::path::Path;

fn click(u: &mut Ui, name: &str) {
    let (x, y) = u
        .control_center(ScreenId::PrintSelector, name)
        .unwrap_or_else(|| panic!("print selector lacks {name}"));
    for down in [true, false] {
        u.handle_input(if down {
            InputEvent::MouseDown {
                button: MouseButton::Left,
                x,
                y,
            }
        } else {
            InputEvent::MouseUp {
                button: MouseButton::Left,
                x,
                y,
            }
        });
    }
    u.update(0);
}

fn shown_buttons(u: &Ui, prefix: &str) -> usize {
    let view = u.screen(ScreenId::PrintSelector).unwrap().view();
    view.walk()
        .filter(|&n| {
            view.node(n)
                .ctrl
                .name
                .as_deref()
                .is_some_and(|name| name.starts_with(prefix))
                && view.is_shown(n)
        })
        .count()
}

#[test]
#[ignore = "requires generated native stock content, no window"]
fn every_v20_print_set_is_offered_for_its_aspect() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let content = ClientContent::load(&root).unwrap();
    let materials: Bundle = serde_json::from_slice(
        &std::fs::read(content.paths.brick_materials.join("brick-materials.json")).unwrap(),
    )
    .unwrap();
    let tool_ui = ToolUi::new(
        &content.catalog,
        &content.effects,
        &materials,
        &content.ui_pack,
    )
    .unwrap();
    let mut ui = Ui::new(
        content.ui_pack.clone(),
        UiConfig {
            size: (1024, 768),
            scale: None,
            platform: Platform::Windows,
        },
        Default::default(),
    );
    for update in tool_ui.catalog_updates() {
        ui.apply(update);
    }
    let letters = materials
        .prints
        .iter()
        .filter(|p| p.aspect == "Letters")
        .count();
    for (aspect, expected) in [("2x2f", 7), ("1x2f", 10), ("2x2r", 7), ("1x1", 0)] {
        ui.apply(UiUpdate::OpenPrintSelector {
            aspect: aspect.into(),
            current: None,
        });
        ui.update(0);
        assert!(ui.is_open(ScreenId::PrintSelector));
        if let Ok(dir) = std::env::var("BRI_PRINT_SELECTOR_PNG") {
            let gpu = bri_ui::gpu::Headless::new().unwrap();
            let mut renderer = bri_ui::gpu::UiRenderer::new(&gpu.device, &gpu.queue);
            let rgba = gpu
                .render_rgba(
                    &mut renderer,
                    &content.ui_pack,
                    &ui.draw(),
                    (1024, 768),
                    1.0,
                    [0.2, 0.2, 0.2, 1.0],
                )
                .unwrap();
            image::save_buffer(
                Path::new(&dir).join(format!("print-selector-{aspect}.png")),
                &rgba,
                1024,
                768,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
        if expected > 0 {
            click(&mut ui, "PSD_PrintsTab();");
            assert_eq!(
                shown_buttons(&ui, "PSD_Prints"),
                expected,
                "{aspect} prints"
            );
        }
        click(&mut ui, "PSD_LettersTab();");
        assert_eq!(
            shown_buttons(&ui, "PSD_Letters"),
            letters,
            "{aspect} letters"
        );
        ui.handle_input(InputEvent::KeyDown {
            key: Key::Escape,
            mods: Modifiers::NONE,
            repeat: false,
        });
        ui.update(0);
        assert!(!ui.is_open(ScreenId::PrintSelector));
        ui.drain_actions();
    }
}
