//! Made-up UI assets for tests that have no converted UI pack: a bitmap font
//! drawn in code, a profile for every profile name the screens use, pixels
//! for every image a layout names, and a few small authored-style layouts.
//! Nothing here is read from an original installation; every value is
//! invented.
//!
//! [`pack`] builds an in-memory [`Pack`]: font sheets and images are drawn
//! here and handed to [`Pack::insert_pixels`], so nothing is read from disk.
use crate::{
    geom::Rect,
    pack::{Pack, Pixels, TexKey},
    schema::{Control, FontEntry, Glyph, ImageEntry, Style, UiPack, VSizing},
    screens::ctrl,
};
use std::{path::PathBuf, rc::Rc};

mod screens;
pub use screens::*;

/// The body text font.
pub const FONT: &str = "fixture sans_14";
/// The window title and button font.
pub const TITLE_FONT: &str = "fixture sans_18";
/// Every profile name the screens build controls with in code.
pub const PROFILES: &[&str] = &[
    "GuiDefaultProfile",
    "BlockButtonProfile",
    "GuiTextProfile",
    "GuiMLTextProfile",
    "GuiTextListProfile",
    "GuiCheckBoxProfile",
    "ColorScrollProfile",
    "BlockScrollProfile",
    "BlockChatTextProfile",
    "HUDBrickNameProfile",
    "HUDBitmapProfile",
    "BlockWindowProfile",
    "BlockTextEditProfile",
    "BlockDefaultProfile",
    "MM_LeftProfile",
    "GuiTextEditProfile",
    "GuiPopUpMenuProfile",
    "NetGraphPacketLossProfile",
    "NetGraphLatencyProfile",
    "NetGraphGhostsActiveProfile",
    "NetGraphGhostUpdatesProfile",
    "NetGraphBitsSentProfile",
    "NetGraphBitsReceivedProfile",
    "HUDRightTextProfile",
    "HUDLeftTextProfile",
    "HUDCenterTextProfile",
    "HUDBSDNameProfile",
    "GuiWindowProfile",
    "GuiProgressProfile",
    "GuiButtonProfile",
];

/// Glyphs per sheet row.
const COLUMNS: u32 = 16;

fn cell(size: u32) -> (u32, u32) {
    (size / 2 + 2, size + 2)
}

/// A font cache of `size` pixels named `<face>_<size>`: every
/// Windows-1252 code from 32 to 255 mapped to a solid block on one sheet
/// (the space is blank).
pub fn font(face: &str, size: u32) -> FontEntry {
    let (w, h) = cell(size);
    let height = size * 7 / 10;
    let mut glyphs = vec![None; 256];
    for code in 32u32..=255 {
        let i = code - 32;
        glyphs[code as usize] = Some(Glyph {
            sheet: 0,
            x: ((i % COLUMNS) * w) as u16,
            y: ((i / COLUMNS) * h) as u16,
            w: (w - 2) as u16,
            h: height as u16,
            x_origin: 0,
            y_origin: height as i16,
            advance: (w - 1) as i16,
        });
    }
    FontEntry {
        face: face.into(),
        size,
        line_height: size + 2,
        baseline: size * 4 / 5,
        sheets: vec![format!("fixture/{face}_{size}.png")],
        glyphs,
        source: "fixture".into(),
        sha256: String::new(),
    }
}

/// The RGBA sheet `sheet` of `font`: white with full coverage inside every
/// glyph rectangle except the space's.
pub fn font_sheet(font: &FontEntry, sheet: u16) -> Pixels {
    let placed: Vec<(usize, Glyph)> = font
        .glyphs
        .iter()
        .enumerate()
        .filter_map(|(c, g)| g.filter(|g| g.sheet == sheet).map(|g| (c, g)))
        .collect();
    let width = placed
        .iter()
        .map(|(_, g)| u32::from(g.x) + u32::from(g.w))
        .max()
        .unwrap_or(1)
        .max(1);
    let height = placed
        .iter()
        .map(|(_, g)| u32::from(g.y) + u32::from(g.h))
        .max()
        .unwrap_or(1)
        .max(1);
    let mut rgba = [255, 255, 255, 0].repeat((width * height) as usize);
    for (code, g) in placed {
        if code == 32 {
            continue;
        }
        for y in g.y..g.y + g.h {
            for x in g.x..g.x + g.w {
                rgba[(u32::from(y) * width + u32::from(x)) as usize * 4 + 3] = 255;
            }
        }
    }
    Pixels {
        width,
        height,
        rgba,
    }
}

/// A plain profile on `font`: dark text, light fill, a one-pixel border.
pub fn style(font: &str) -> Style {
    Style {
        font: Some(font.into()),
        font_color: Some([20, 20, 30, 255]),
        font_color_hl: Some([40, 40, 160, 255]),
        font_color_na: Some([120, 120, 120, 255]),
        font_color_sel: Some([200, 200, 255, 255]),
        fill_color: Some([220, 220, 210, 255]),
        fill_color_hl: Some([200, 200, 240, 255]),
        fill_color_na: Some([180, 180, 180, 255]),
        border_color: Some([60, 60, 60, 255]),
        border_color_hl: Some([90, 90, 200, 255]),
        border: 1,
        can_key_focus: true,
        ..Default::default()
    }
}

/// The image ids a control draws: a bitmap button's four state images
/// (`_n`, `_h`, `_d`, `_i`), anything else's bitmap as named.
fn bitmaps(c: &Control, out: &mut Vec<(String, [i32; 2])>) {
    if let Some(b) = c.bitmap.as_deref().filter(|b| !b.is_empty()) {
        let b = b.to_ascii_lowercase();
        if c.class.contains("BitmapButton") {
            for state in ["_n", "_h", "_d", "_i"] {
                out.push((format!("{b}{state}"), c.extent));
            }
        } else {
            out.push((b, c.extent));
        }
    }
    for k in &c.children {
        bitmaps(k, out);
    }
}

fn styles_of(c: &Control, out: &mut Vec<String>) {
    if !c.style.is_empty() {
        out.push(c.style.clone());
    }
    for k in &c.children {
        styles_of(k, out);
    }
}

/// Adds image `id` (`width` x `height`) to `data` unless it is there.
pub fn add_image(data: &mut UiPack, id: &str, width: u32, height: u32) {
    data.images
        .entry(id.to_ascii_lowercase())
        .or_insert_with(|| ImageEntry {
            file: format!("fixture/{id}.png"),
            width: width.max(1),
            height: height.max(1),
            sha256: String::new(),
            source: "fixture".into(),
        });
}

/// Adds the fixture fonts, a profile for every name in [`PROFILES`] and
/// every profile a layout names, and an image for every bitmap a layout
/// names. Whatever `data` already has is kept.
pub fn add_assets(data: &mut UiPack) {
    for (face, size) in [("fixture sans", 14), ("fixture sans", 18)] {
        data.fonts
            .entry(format!("{face}_{size}"))
            .or_insert_with(|| font(face, size));
    }
    let mut names: Vec<String> = PROFILES.iter().map(|p| p.to_string()).collect();
    let mut images = vec![];
    for layout in data.layouts.values() {
        styles_of(layout, &mut names);
        bitmaps(layout, &mut images);
    }
    for name in names {
        data.styles.entry(name.clone()).or_insert_with(|| {
            let mut s = style(if name.contains("Window") || name.contains("Button") {
                TITLE_FONT
            } else {
                FONT
            });
            s.opaque = name.contains("Window") || name.contains("Edit");
            s
        });
    }
    for (id, extent) in images {
        add_image(data, &id, extent[0] as u32, extent[1] as u32);
    }
}

/// Pixels for image `id`: a colour derived from the id inside a darker
/// one-pixel frame.
pub fn image_pixels(id: &str, width: u32, height: u32) -> Pixels {
    let hash = id.bytes().fold(2166136261u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(16777619)
    });
    let fill = [
        64 + (hash & 0x7f) as u8,
        64 + ((hash >> 8) & 0x7f) as u8,
        64 + ((hash >> 16) & 0x7f) as u8,
        255,
    ];
    let edge = [fill[0] / 2, fill[1] / 2, fill[2] / 2, 255];
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let border = x == 0 || y == 0 || x + 1 == width || y + 1 == height;
            rgba.extend_from_slice(if border { &edge } else { &fill });
        }
    }
    Pixels {
        width,
        height,
        rgba,
    }
}

/// `data` with [`add_assets`], as an in-memory pack whose every font sheet
/// and image is drawn here.
pub fn pack(mut data: UiPack) -> Rc<Pack> {
    add_assets(&mut data);
    let pack = Pack::from_parts(data, PathBuf::new());
    for (id, font) in &pack.data.fonts {
        for sheet in 0..font.sheets.len() as u16 {
            pack.insert_pixels(
                TexKey::FontSheet(id.clone(), sheet),
                font_sheet(font, sheet),
            );
        }
    }
    for (id, image) in &pack.data.images {
        pack.insert_pixels(
            TexKey::Image(id.clone()),
            image_pixels(id, image.width, image.height),
        );
    }
    Rc::new(pack)
}

/// A control named `name`.
pub fn named(mut c: Control, name: &str) -> Control {
    c.name = Some(name.into());
    c
}

/// v20-shaped message boxes: `MessageBoxOKDlg` and `MessageBoxYesNoDlg`,
/// each a window (`MB<kind>Frame`) with a one-line ML text
/// (`MB<kind>Text`) and bottom buttons that move down when the text grows.
pub fn add_message_boxes(data: &mut UiPack) {
    for (layout, prefix, buttons) in [
        (
            "MessageBoxOKDlg",
            "MBOK",
            &[("OK", "MessageBoxOKDlg.okCallback();")][..],
        ),
        (
            "MessageBoxYesNoDlg",
            "MBYesNo",
            &[
                ("Yes", "MessageBoxYesNoDlg.yesCallback();"),
                ("No", "MessageBoxYesNoDlg.noCallback();"),
            ][..],
        ),
    ] {
        let mut root = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480));
        let mut frame = named(
            ctrl(
                "GuiWindowCtrl",
                "BlockWindowProfile",
                Rect::new(170, 185, 300, 110),
            ),
            &format!("{prefix}Frame"),
        );
        frame.children.push(named(
            ctrl(
                "GuiMLTextCtrl",
                "GuiMLTextProfile",
                Rect::new(12, 34, 276, 16),
            ),
            &format!("{prefix}Text"),
        ));
        for (i, (label, command)) in buttons.iter().enumerate() {
            let mut b = ctrl(
                "GuiBitmapButtonCtrl",
                "BlockButtonProfile",
                Rect::new(40 + i as i32 * 130, 70, 90, 28),
            );
            b.v_sizing = VSizing::Top;
            b.bitmap = Some("fixture/ui/button".into());
            b.text = Some((*label).into());
            b.command = Some((*command).into());
            frame.children.push(b);
        }
        root.children.push(frame);
        data.layouts.insert(layout.into(), root);
    }
}

/// A full-screen layout holding one window (`window` names it, `title`
/// captions it) with `children`.
pub fn dialog(window: &str, title: &str, rect: Rect, children: Vec<Control>) -> Control {
    let mut root = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480));
    let mut w = named(ctrl("GuiWindowCtrl", "BlockWindowProfile", rect), window);
    w.text = Some(title.into());
    w.children = children;
    root.children.push(w);
    root
}

/// A text label.
pub fn label(r: Rect, text: &str) -> Control {
    crate::screens::text("GuiTextProfile", r, text)
}

/// A labelled bitmap button running `command`.
pub fn text_button(r: Rect, text: &str, command: &str) -> Control {
    crate::screens::button("BlockButtonProfile", r, "fixture/ui/button", text, command)
}

/// [`add_message_boxes`] plus v20-shaped remap prompt (`RemapDlg`), save
/// and load dialogs (`saveBricksGui`, `LoadBricksGui`) and player list
/// (`NewPlayerListGui`), with the control names the screens look up.
pub fn add_dialogs(data: &mut UiPack) {
    add_message_boxes(data);
    let edit =
        |class: &str, name: &str, r: Rect| named(ctrl(class, "BlockTextEditProfile", r), name);
    data.layouts.insert(
        "RemapDlg".into(),
        dialog(
            "Remap_Window",
            "Remap",
            Rect::new(170, 190, 300, 100),
            vec![
                named(
                    ctrl("GuiTextCtrl", "GuiTextProfile", Rect::new(10, 30, 280, 18)),
                    "OptRemapText",
                ),
                label(Rect::new(8, 70, 130, 18), "Escape to cancel"),
                label(Rect::new(160, 70, 130, 18), "Backspace to clear"),
            ],
        ),
    );
    data.layouts.insert(
        "saveBricksGui".into(),
        dialog(
            "SaveBricks_Window",
            "Save Bricks",
            Rect::new(70, 40, 500, 400),
            vec![
                label(Rect::new(14, 30, 80, 18), "File name:"),
                edit(
                    "GuiTextEditCtrl",
                    "SaveBricks_FileName",
                    Rect::new(100, 30, 250, 20),
                ),
                label(Rect::new(14, 56, 80, 18), "Description:"),
                edit(
                    "GuiMLTextEditCtrl",
                    "SaveBricks_Description",
                    Rect::new(100, 56, 250, 60),
                ),
                named(
                    ctrl(
                        "GuiCheckBoxCtrl",
                        "GuiCheckBoxProfile",
                        Rect::new(14, 176, 160, 20),
                    ),
                    "SaveBricks_ExtendedInfo",
                ),
                named(
                    ctrl(
                        "GuiCheckBoxCtrl",
                        "GuiCheckBoxProfile",
                        Rect::new(14, 200, 160, 20),
                    ),
                    "SaveBricks_Ownership",
                ),
                named(
                    ctrl(
                        "GuiTextListCtrl",
                        "GuiTextListProfile",
                        Rect::new(14, 230, 330, 120),
                    ),
                    "SaveBricks_FileList",
                ),
                text_button(Rect::new(380, 360, 100, 28), "Save", "SaveBricks_Save();"),
                text_button(
                    Rect::new(270, 360, 100, 28),
                    "Cancel",
                    "canvas.popDialog(\"saveBricksGui\");",
                ),
            ],
        ),
    );
    let mut scroll = ctrl(
        "GuiScrollCtrl",
        "BlockScrollProfile",
        Rect::new(14, 96, 310, 250),
    );
    scroll.children.push(named(
        ctrl(
            "GuiTextListCtrl",
            "GuiTextListProfile",
            Rect::new(0, 0, 290, 16),
        ),
        "LoadBricks_FileList",
    ));
    data.layouts.insert(
        "LoadBricksGui".into(),
        dialog(
            "LoadBricks_Window",
            "Load Bricks",
            Rect::new(0, 0, 640, 480),
            vec![
                named(
                    ctrl(
                        "GuiPopUpMenuCtrl",
                        "GuiPopUpMenuProfile",
                        Rect::new(14, 30, 200, 20),
                    ),
                    "LoadBricks_MapMenu",
                ),
                text_button(
                    Rect::new(14, 80, 80, 16),
                    "Name",
                    "sortList(LoadBricks_FileList, 0);",
                ),
                text_button(
                    Rect::new(200, 80, 80, 16),
                    "Date",
                    "sortList(LoadBricks_FileList, 2);",
                ),
                scroll,
                named(
                    ctrl(
                        "GuiMLTextCtrl",
                        "GuiMLTextProfile",
                        Rect::new(340, 300, 280, 40),
                    ),
                    "LoadBricks_Description",
                ),
                named(
                    ctrl(
                        "GuiBitmapCtrl",
                        "GuiDefaultProfile",
                        Rect::new(340, 96, 280, 190),
                    ),
                    "LoadBricks_Preview",
                ),
                named(
                    ctrl(
                        "GuiCheckBoxCtrl",
                        "GuiCheckBoxProfile",
                        Rect::new(340, 350, 160, 20),
                    ),
                    "LoadBricks_DoOwnership",
                ),
                text_button(
                    Rect::new(520, 420, 100, 28),
                    "Load",
                    "LoadBricks_ClickLoadButton();",
                ),
            ],
        ),
    );
    let mut players = vec![];
    for (i, (header, column)) in [("Admin", 0), ("Name", 1), ("Score", 2), ("BL_ID", 3)]
        .into_iter()
        .enumerate()
    {
        players.push(text_button(
            Rect::new(14 + i as i32 * 100, 30, 96, 18),
            header,
            &format!("NewPlayerListGui.sortList({column});"),
        ));
    }
    players.push(named(
        ctrl(
            "GuiTextListCtrl",
            "GuiTextListProfile",
            Rect::new(14, 52, 400, 200),
        ),
        "NPL_List",
    ));
    for (i, (label, command)) in [
        ("Build", "NewPlayerListGui.clickTrustInviteBuild();"),
        ("Full", "NewPlayerListGui.clickTrustInviteFull();"),
    ]
    .into_iter()
    .enumerate()
    {
        players.push(text_button(
            Rect::new(14 + i as i32 * 100, 270, 90, 28),
            label,
            command,
        ));
    }
    players.push(text_button(
        Rect::new(320, 270, 90, 28),
        "Close",
        "canvas.popDialog(NewPlayerListGui);",
    ));
    data.layouts.insert(
        "NewPlayerListGui".into(),
        dialog(
            "NPL_Window",
            "Player List",
            Rect::new(100, 80, 430, 310),
            players,
        ),
    );
}
