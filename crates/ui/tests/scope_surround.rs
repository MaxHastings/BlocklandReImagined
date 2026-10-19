use bri_ui::{
    api::{ScopeFit, Settings, UiUpdate},
    binds::Platform,
    draw::DrawCmd,
    pack::TexKey,
    screens::ScreenId,
    ui::{Ui, UiConfig},
};

fn play(size: (u32, u32), fit: ScopeFit) -> (Vec<[i32; 4]>, [i32; 4]) {
    let mut ui = Ui::new(
        bri_ui::testing::screens_pack(),
        UiConfig {
            size,
            scale: Some(1.),
            platform: Platform::Windows,
        },
        Settings {
            binds: Some(vec![]),
            ..Default::default()
        },
    );
    ui.core.push(ScreenId::Play);
    ui.update(0);
    ui.apply(UiUpdate::ScopeFit(fit));
    ui.core.scope_overlay = Some((42, 1.));
    ui.update(0);
    let draw = ui.draw();
    let picture = draw
        .cmds
        .iter()
        .find_map(|c| match c {
            DrawCmd::Image {
                tex: TexKey::External(42),
                dst,
                ..
            } => Some([dst[0] as i32, dst[1] as i32, dst[2] as i32, dst[3] as i32]),
            _ => None,
        })
        .expect("scope picture drawn");
    let masks = draw
        .cmds
        .iter()
        .filter_map(|c| match c {
            DrawCmd::Fill { dst, color, .. }
                if *color == [0, 0, 0, 255] && dst.h == size.1 as i32 && dst.w > 0 =>
            {
                Some([dst.x, dst.y, dst.w, dst.h])
            }
            _ => None,
        })
        .collect();
    (masks, picture)
}

#[test]
fn scope_fit_chooses_black_sides_open_sides_or_stretch() {
    let (masks, picture) = play((1280, 560), ScopeFit::HeightBlack);
    assert_eq!(picture, [360, 0, 560, 560]);
    assert_eq!(masks.len(), 2);
    let (masks, picture) = play((1280, 560), ScopeFit::Height);
    assert_eq!(picture, [360, 0, 560, 560]);
    assert!(masks.is_empty(), "{masks:?}");
    let (masks, picture) = play((1280, 560), ScopeFit::Stretch);
    assert_eq!(picture, [0, 0, 1280, 560]);
    assert!(masks.is_empty(), "{masks:?}");
}
