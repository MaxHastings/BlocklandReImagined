//! A scope's picture while aiming (`UiUpdate::ScopeOverlay`): fitted to
//! the screen's height and centred, the screen either side of it black,
//! and gone when the aim ends.
use bri_ui::{
    api::*,
    binds::Platform,
    draw::DrawCmd,
    geom::Rect,
    pack::{Pack, TexKey},
    schema::UiPack,
    screens::ctrl,
    ui::{Ui, UiConfig},
};
use std::{path::PathBuf, rc::Rc};

const PICTURE: u64 = 0x5343_0001;

fn ui() -> Ui {
    let mut p = UiPack::default();
    for name in ["MainMenuGui", "PlayGui", "LoadingGui"] {
        p.layouts.insert(
            name.into(),
            ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480)),
        );
    }
    let mut u = Ui::new(
        Rc::new(Pack::from_parts(p, PathBuf::new())),
        UiConfig {
            size: (1600, 900),
            scale: Some(1.0),
            platform: Platform::Windows,
        },
        Settings::default(),
    );
    u.apply(UiUpdate::Connection(ConnectionState::InGame {
        server_name: "Test".into(),
        max_players: 8,
        local: true,
        single_player: true,
        admin: true,
    }));
    u.drain_actions();
    u
}

fn picture(u: &Ui) -> Option<[f32; 4]> {
    u.draw().cmds.iter().find_map(|c| match c {
        DrawCmd::Image {
            tex: TexKey::External(PICTURE),
            dst,
            ..
        } => Some(*dst),
        _ => None,
    })
}

fn black_bars(u: &Ui) -> Vec<Rect> {
    u.draw()
        .cmds
        .iter()
        .filter_map(|c| match c {
            DrawCmd::Fill { dst, color, .. } if *color == [0, 0, 0, 255] => Some(*dst),
            _ => None,
        })
        .collect()
}

#[test]
fn the_scope_fills_the_screen_height_with_black_either_side() {
    let mut u = ui();
    assert_eq!(picture(&u), None);
    u.apply(UiUpdate::ScopeOverlay(Some((PICTURE, 1.0))));
    assert_eq!(picture(&u), Some([350.0, 0.0, 900.0, 900.0]));
    let bars = black_bars(&u);
    assert!(bars.contains(&Rect::new(0, 0, 350, 900)), "{bars:?}");
    assert!(bars.contains(&Rect::new(1250, 0, 350, 900)), "{bars:?}");
    // A picture with no shape is refused rather than drawn.
    u.apply(UiUpdate::ScopeOverlay(Some((PICTURE, 0.0))));
    assert_eq!(picture(&u), None);
    u.apply(UiUpdate::ScopeOverlay(Some((PICTURE, 1.0))));
    u.apply(UiUpdate::ScopeOverlay(None));
    assert_eq!(picture(&u), None);
    assert!(black_bars(&u).iter().all(|r| r.h != 900));
}
