//! Headless regression for multiline tutorial prints at explicit UI scales.
use bri_ui::{
    api::{ConnectionState, Settings, UiUpdate},
    binds::Platform,
    pack::Pack,
    screens::ScreenId,
    ui::{Ui, UiConfig},
    view::View,
};
use std::{path::PathBuf, rc::Rc};

const COMPLETION: &str = "Congratulations!\nYou completed the Tutorial!\n\nTotal Time: 20:56\nGoals Completed: 17/18\nShooting Targets: 52/58\nShooting Accuracy: 11%\n\nStay and build, or leave from the menu";

fn check(pack: Rc<Pack>) {
    for (size, scale) in [((803, 530), 1.0), ((803, 530), 1.5), ((1280, 960), 2.0)] {
        let mut ui = Ui::new(
            pack.clone(),
            UiConfig {
                size,
                scale: Some(scale),
                platform: Platform::Linux,
            },
            Settings::default(),
        );
        ui.apply(UiUpdate::Connection(ConnectionState::InGame {
            server_name: "Print regression".into(),
            max_players: 8,
            local: true,
            single_player: true,
            admin: true,
        }));
        for message in [COMPLETION, "Short print.", COMPLETION] {
            ui.apply(UiUpdate::CenterPrint {
                text: message.into(),
                seconds: 12.0,
            });
            for _ in 0..3 {
                ui.update(0);
                fits(&ui, &pack, message == COMPLETION);
            }
        }
        ui.resize((1280, 960), Some(2.0));
        ui.update(0);
        fits(&ui, &pack, true);
        ui.resize(size, Some(scale));
        ui.update(0);
        fits(&ui, &pack, true);
        ui.update(12_001);
        let view = ui.screen(ScreenId::Play).unwrap().view();
        assert!(!view.is_shown(view.id("centerPrintDlg").unwrap()));
    }
}

fn fits(ui: &Ui, pack: &Pack, multiline: bool) {
    let view = ui.screen(ScreenId::Play).unwrap().view();
    let text = view.id("CenterPrintText").unwrap();
    let dialog = view.id("centerPrintDlg").unwrap();
    let node = view.node(text);
    let need = View::ml_height(pack, &node.ctrl.style, &view.text_of(text), node.rect.w);
    if multiline {
        assert!(need > 100, "a real multiline font layout is required");
    }
    assert!(node.rect.h >= need, "text height {} < {need}", node.rect.h);
    assert!(view.node(dialog).rect.h >= need, "parent clips the print");
    assert!(
        node.rect.y >= 0 && node.rect.bottom() <= ui.logical_size().1,
        "print extends beyond the canvas: {:?}",
        node.rect
    );
}

#[test]
fn tutorial_completion_fits_at_explicit_scales_synthetic() {
    check(bri_ui::testing::screens_pack());
}

#[test]
#[ignore = "requires generated v20 content"]
fn tutorial_completion_fits_at_explicit_scales_content() -> anyhow::Result<()> {
    let root = std::env::var_os("BRI_CONTENT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content"));
    check(Rc::new(Pack::load(&bri_package::testing::pack_dir(
        &root, "ui_pack",
    ))?));
    Ok(())
}
