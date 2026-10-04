//! Native runtime-built inventory HUD. Geometry/art follow createInvHud,
//! createPaintHud and createToolHud in the recovered v20 client scripts.
use super::*;
use crate::api::{ConnectionState, IconRef, PlantError};
use crate::geom::WHITE;
use crate::models::hud::{FX_ART, ScrollMode};
use bri_console::Clamp;

pub const SMALL_PLANT_ERRORS: &str = "$pref::Video::useSmallPlantErrors";

pub struct Play {
    view: View,
}
impl Play {
    pub fn new(core: &Core) -> Self {
        let mut view = layout_view(core, "PlayGui");
        // These authored controls are old loading/inventory placeholders replaced
        // by script at runtime; the native HUD builds its own current controls.
        for name in ["HUD_Ghosting", "HudInvBox", "HUD_PaintNameBG"] {
            if let Some(n) = view.id(name) {
                view.set_visible(n, false);
            }
        }
        // Torque's ML text grows with its content; let multi-line prints use
        // their dialog's full height.
        for (dialog, text) in [
            ("centerPrintDlg", "CenterPrintText"),
            ("bottomPrintDlg", "BottomPrintText"),
        ] {
            if let (Some(d), Some(t)) = (view.id(dialog), view.id(text)) {
                let h = view.node(d).ctrl.extent[1] - view.node(t).ctrl.position[1];
                view.nodes[t].ctrl.extent[1] = h;
            }
        }
        Self { view }
    }
}
/// `newChatText`'s authored position in NewChatHud.
const CHAT_TOP_LEFT: (i32, i32) = (2, 20);
fn chat_profile(core: &Core) -> String {
    format!(
        "BlockChatTextSize{}Profile",
        super::options::chat_size(&core.prefs)
    )
}
fn chat_text(core: &Core) -> String {
    // `NewChatSO::addLine` wraps every line in `<spush>`/`<spop>` so one
    // line's styles never leak into the next (c:14972-14982). Uncoloured
    // text is BlockChatTextProfile's base colour, `fontColors[0]` = 255 0 64
    // (see `pack::alias_font_colors`).
    core.chat
        .visible(core.time_ms)
        .iter()
        .map(|l| format!("<spush>{}<spop>", l.text))
        .collect::<Vec<_>>()
        .join("\n")
}
/// `newChatText` spans the screen width and, like Torque's ML text, grows
/// to the height of its reflowed lines.
fn chat_rect(core: &Core, chat: &str) -> Rect {
    let (x, y) = CHAT_TOP_LEFT;
    let w = (core.logical.0 - x).max(1);
    let h = View::ml_height(&core.pack, &chat_profile(core), chat, w);
    Rect::new(x, y, w, h)
}
/// `NewChatSO::displayLatest`/`update` and `toggleCursor` (c:5370-5392,
/// c:14906-14968, c:15121-15170): the tip shows while a shown chat line has
/// a link, except in single player, or while the cursor is toggled on, and
/// only with `$pref::HUD::showToolTips` and a positive chat line time.
pub fn mouse_tip(core: &Core) -> bool {
    if !core.prefs.bool_or("$pref::HUD::showToolTips", true)
        || core.prefs.i64_or("$Pref::Chat::LineTime", 6500) <= 0
    {
        return false;
    }
    let single = matches!(
        core.conn,
        ConnectionState::InGame {
            single_player: true,
            ..
        }
    );
    let links = !single
        && core
            .chat
            .visible(core.time_ms)
            .iter()
            .any(|l| l.text.contains("<a:"));
    links || core.cursor_forced
}
/// The chat link under a logical point, for a click while the cursor is
/// toggled on (`ToggleCursor`, M).
pub fn chat_link_at(core: &Core, x: i32, y: i32) -> Option<String> {
    let chat = chat_text(core);
    let rect = chat_rect(core, &chat);
    View::ml_link_at(&core.pack, &chat_profile(core), &chat, rect, (x, y))
}
/// Bottom of the chat text, where `newMessageHud::updatePosition` puts the
/// typing box.
pub fn chat_bottom(core: &Core) -> i32 {
    chat_rect(core, &chat_text(core)).bottom()
}
fn named_text(v: &mut View, style: &str, rect: Rect, label: &str) {
    v.add(v.root, text(style, rect, label));
}
fn art(v: &mut View, rect: Rect, name: &str, tint: Rgba) {
    let n = v.add(v.root, bitmap("HUDBitmapProfile", rect, name));
    v.state(n).tint = Some(tint);
}
fn icon(v: &mut View, rect: Rect, image: &IconRef, tint: Rgba) {
    match image {
        IconRef::Pack(id) => art(v, rect, id, tint),
        IconRef::External(id) => {
            let n = v.add(v.root, ctrl("GuiBitmapCtrl", "HUDBitmapProfile", rect));
            v.state(n).external_texture = Some(*id);
            v.state(n).tint = Some(tint);
        }
        IconRef::None => art(v, rect, "base/client/ui/brickicons/unknown", tint),
    }
}
fn fill(v: &mut View, rect: Rect, color: Rgba) {
    v.add(v.root, swatch(rect, color));
}
fn title_bar(v: &mut View, r: Rect, label: &str) {
    art(
        v,
        Rect::new(r.x, r.y, 10, 18),
        "base/client/ui/bluehudleftcorner",
        WHITE,
    );
    art(
        v,
        Rect::new(r.right() - 10, r.y, 10, 18),
        "base/client/ui/bluehudrightcorner",
        WHITE,
    );
    fill(v, Rect::new(r.x + 10, r.y, r.w - 20, 18), [0, 0, 128, 128]);
    named_text(v, "HUDBrickNameProfile", r, label);
}
fn markup(v: &mut View, r: Rect, label: &str, style: &str) -> NodeId {
    let mut c = text(style, r, label);
    c.class = "GuiMLTextCtrl".into();
    v.add(v.root, c)
}

/// Rebuilt in a bounded temporary view: no detached controls accumulate as
/// inventory, paint or chat changes. The authored PlayGui remains persistent.
/// `PlayGui_ShapeNameHud`: names centered above each anchor in the HUD's
/// `BlockChatTextProfile` font. `GuiShapeNameHud::drawName`
/// (blocklandv20.exe 0x527630) ignores the control's `textColor`: it draws
/// the name eight times one pixel around in [`name_outline`], then once in
/// the shape's name colour, all at the distance fade.
fn name_tags(pack: &Pack, dl: &mut DrawList, core: &Core) {
    let Some(font) = pack
        .data
        .styles
        .get("BlockChatTextProfile")
        .and_then(|s| s.font.as_deref())
        .and_then(|f| crate::text::Font::get(pack, f))
    else {
        return;
    };
    for tag in &core.name_tags {
        let alpha = (tag.opacity.clamped(0.0, 1.0) * 255.0) as u8;
        if alpha == 0 {
            continue;
        }
        let x = (tag.x - font.width(&tag.text) as f32 / 2.0).round();
        let y = (tag.y - font.line_height() as f32).round();
        let [r, g, b] = crate::api::name_outline(tag.color);
        for dx in [-1.0, 0.0, 1.0] {
            for dy in [-1.0, 0.0, 1.0] {
                if dx != 0.0 || dy != 0.0 {
                    font.draw(dl, x + dx, y + dy, &tag.text, [r, g, b, alpha], &[]);
                }
            }
        }
        let [r, g, b] = tag.color;
        font.draw(dl, x, y, &tag.text, [r, g, b, alpha], &[]);
    }
}

/// The `hud.overlay` slot: panels enabled packages declared, drawn from
/// data (title, rows of label and value, key hints) in their own colours.
/// Sound captions, newest last, centred above the bottom print.
fn captions(pack: &Pack, dl: &mut DrawList, core: &Core) {
    if core.captions.is_empty() {
        return;
    }
    let Some(font) = pack
        .data
        .styles
        .get("BlockChatTextProfile")
        .and_then(|s| s.font.as_deref())
        .and_then(|f| crate::text::Font::get(pack, f))
    else {
        return;
    };
    let (w, h) = core.logical;
    let line = font.line_height().max(1) + 4;
    let mut y = h - 140 - core.captions.len() as i32 * line;
    for (text, _) in &core.captions {
        let tw = font.width(text) + 12;
        let x = (w - tw) / 2;
        dl.fill(Rect::new(x, y, tw, line), [0, 0, 0, 170]);
        font.draw(
            dl,
            (x + 6) as f32,
            (y + 2) as f32,
            text,
            [255, 255, 255, 255],
            &[],
        );
        y += line;
    }
}

fn package_panels(pack: &Pack, dl: &mut DrawList, core: &Core) {
    use crate::api::PanelAnchor;
    let Some(font) = pack
        .data
        .styles
        .get("BlockChatTextProfile")
        .and_then(|s| s.font.as_deref())
        .and_then(|f| crate::text::Font::get(pack, f))
    else {
        return;
    };
    let (w, h) = core.logical;
    let line = font.line_height().max(1);
    let pad = 6;
    let mut offsets = [0_i32; 4];
    // Below the net graph and performance overlay when they show.
    let top_right = crate::screens::perf::top_right_bottom(pack, core);
    for panel in &core.package_panels {
        let hints: String = panel
            .keys
            .iter()
            .map(|(k, label)| format!("[{}] {label}", k.to_ascii_uppercase()))
            .collect::<Vec<_>>()
            .join("   ");
        let row_width = panel
            .rows
            .iter()
            .map(|(l, v, _)| font.width(l) + font.width(v) + 24)
            .max()
            .unwrap_or(0);
        let pw = (font
            .width(&panel.title)
            .max(row_width)
            .max(font.width(&hints))
            + pad * 2)
            .max(140);
        let ph = line
            + 4
            + panel.rows.len() as i32 * line
            + if hints.is_empty() { 0 } else { line + 4 }
            + pad * 2;
        let slot = panel.anchor as usize;
        let (x, top) = match panel.anchor {
            PanelAnchor::TopLeft => (8, 8 + offsets[slot]),
            PanelAnchor::TopRight => (w - pw - 8, top_right + offsets[slot]),
            PanelAnchor::BottomLeft => (8, h - ph - 120 - offsets[slot]),
            PanelAnchor::BottomRight => (w - pw - 8, h - ph - 120 - offsets[slot]),
        };
        offsets[slot] += ph + 6;
        dl.fill(Rect::new(x, top, pw, ph), panel.background);
        dl.fill(Rect::new(x, top, pw, line + 4), panel.accent);
        dl.fill(Rect::new(x, top + ph - 2, pw, 2), panel.accent);
        let dark = [16, 16, 24, 255];
        font.draw(
            dl,
            (x + pad) as f32,
            (top + 2) as f32,
            &panel.title,
            dark,
            &[],
        );
        let mut y = top + line + 4 + pad;
        for (label, value, color) in &panel.rows {
            font.draw(dl, (x + pad) as f32, y as f32, label, panel.text, &[]);
            let vx = x + pw - pad - font.width(value);
            font.draw(dl, vx as f32, y as f32, value, *color, &[]);
            y += line;
        }
        if !hints.is_empty() {
            font.draw(
                dl,
                (x + pad) as f32,
                (y + 4) as f32,
                &hints,
                panel.accent,
                &[],
            );
        }
    }
}

fn hud(core: &Core) -> View {
    let (w, h) = core.logical;
    let m = &core.hud;
    let mut v = View::new(&ctrl(
        "GuiControl",
        "GuiDefaultProfile",
        Rect::new(0, 0, w, h),
    ));
    if core.damage_flash > 0.0 {
        fill(
            &mut v,
            Rect::new(0, 0, w, h),
            [255, 0, 0, (core.damage_flash * 255.0) as u8],
        );
    }
    if core.whiteout > 0.0 {
        fill(
            &mut v,
            Rect::new(0, 0, w, h),
            [255, 255, 255, (core.whiteout * 255.0) as u8],
        );
    }
    for tint in &core.underwater {
        let byte = |v: f32| (v.clamped(0.0, 1.0) * 255.0).round() as u8;
        fill(
            &mut v,
            Rect::new(0, 0, w, h),
            [byte(tint[0]), byte(tint[1]), byte(tint[2]), byte(tint[3])],
        );
    }
    if m.boxes_visible {
        let cell = (w / 10).clamp(1, 64);
        let width = cell * 10;
        let (x, y) = ((w - width) / 2, h - cell + m.brick_slide.offset);
        fill(&mut v, Rect::new(x, y, width, cell), [0, 0, 0, 64]);
        let mut color = m
            .paint
            .iter()
            .flat_map(|d| d.colors.iter())
            .nth(m.spray_index as usize)
            .copied()
            .unwrap_or([1.0; 4]);
        color[3] = color[3].clamped(0.1, 1.0);
        let tint = if m.prefs.recolor_brick_icons {
            rgba(color)
        } else {
            WHITE
        };
        for (i, b) in m.bricks.iter().take(10).enumerate() {
            let r = Rect::new(x + i as i32 * cell, y, cell, cell);
            if m.brick_active && m.cur_brick == Some(i) {
                art(
                    &mut v,
                    r,
                    "base/client/ui/brickicons/brickiconactive",
                    WHITE,
                );
            }
            fill(
                &mut v,
                Rect::new(r.x + 2, r.y + 4, cell - 4, cell - 8),
                [0, 0, 0, 64],
            );
            if let Some(b) = b {
                icon(&mut v, r, &b.icon, tint);
            }
            if m.prefs.show_slot_numbers {
                named_text(
                    &mut v,
                    "HUDBrickNameProfile",
                    Rect::new(r.x, r.y + 2, 16, 18),
                    &((i + 1) % 10).to_string(),
                );
            }
        }
        title_bar(&mut v, Rect::new(x, y - 18, width, 18), &m.brick_name);
        if m.prefs.show_tooltips && m.mode != ScrollMode::Bricks {
            named_text(
                &mut v,
                "HUDRightTextProfile",
                Rect::new(x, y - 18, width, 18),
                &format!("Press {} for more bricks   ", core.key_name("openBSD")),
            );
            named_text(
                &mut v,
                "HUDLeftTextProfile",
                Rect::new(x, y - 18, width, 18),
                &format!("  Press {} to use bricks", core.key_name("useBricks")),
            );
        }
        let ty = -m.tool_slide.offset;
        let tx = w - cell;
        for (i, t) in m.tools.iter().take(64).enumerate() {
            let r = Rect::new(tx, ty + i as i32 * cell, cell, cell);
            art(&mut v, r, "base/client/ui/itemicons/toolbg", WHITE);
            if m.tool_active && m.cur_tool == Some(i) {
                art(&mut v, r, "base/client/ui/itemicons/itemactive", WHITE);
            }
            if let Some(t) = t {
                icon(&mut v, r, &t.icon, t.tint.unwrap_or(WHITE));
            }
        }
        let label = Rect::new(tx, ty + m.tools.len().min(64) as i32 * cell, cell, 18);
        art(&mut v, label, "base/client/ui/itemicons/toollabelbg", WHITE);
        let tools_label = if m.mode == ScrollMode::Tools {
            m.tool_name.clone()
        } else if m.prefs.show_tooltips {
            format!("{} = tools", core.key_name("useTools"))
        } else {
            String::new()
        };
        named_text(&mut v, "HUDCenterTextProfile", label, &tools_label);
        let px = -m.paint_slide.offset;
        let rows = m.paint_rows.iter().copied().max().unwrap_or(9).max(9) as i32;
        let py = h - (rows * 17 + 1) - 18;
        let pw = m.paint_box_width();
        art(
            &mut v,
            Rect::new(px + pw - 14, py, 100, 100),
            "base/client/ui/paintlabelbg",
            WHITE,
        );
        art(
            &mut v,
            Rect::new(px + pw - 14, py + 100, 100, (rows * 17 + 1 - 100).max(0)),
            "base/client/ui/paintlabelbgloop",
            WHITE,
        );
        art(
            &mut v,
            Rect::new(px + pw - 14, py, 100, 100),
            "base/client/ui/paintlabel",
            WHITE,
        );
        for (col, division) in m.paint.iter().enumerate() {
            for (row, c) in division.colors.iter().enumerate() {
                fill(
                    &mut v,
                    Rect::new(px + col as i32 * 17 + 1, py + row as i32 * 17 + 1, 16, 16),
                    rgba(*c),
                );
            }
        }
        for (row, name) in FX_ART.iter().enumerate() {
            let r = Rect::new(
                px + m.paint.len() as i32 * 17 + 1,
                py + row as i32 * 17 + 1,
                16,
                16,
            );
            fill(&mut v, r, [64, 64, 64, 255]);
            if !name.is_empty() {
                art(&mut v, r, &format!("base/client/ui/{name}"), WHITE);
            }
        }
        if m.paint_active {
            art(
                &mut v,
                Rect::new(
                    px + m.paint_row as i32 * 17,
                    py + m.paint_swatch as i32 * 17,
                    18,
                    18,
                ),
                "base/client/ui/paintactive",
                WHITE,
            );
        }
        title_bar(&mut v, Rect::new(px, py - 18, pw + 100, 18), &m.paint_name);
    }
    if core
        .plant_error
        .is_some_and(|(e, _)| e == PlantError::Forbidden)
    {
        named_text(
            &mut v,
            "HUDBrickNameProfile",
            Rect::new(0, h * 2 / 3, w, 30),
            "You do not have permission to build here.",
        );
    }
    // NewChatHud: original cached font + ML markup, with chat fade/page
    // rules from ChatModel, and the "VVV" indicator on its last line while
    // paged up (newChatHud_UpdateIndicatorPosition).
    // chatWhosTalkingText above it: " name name" (WhoTalkSO::Display).
    if !core.talking.is_empty() {
        let names: String = core.talking.iter().map(|n| format!(" {n}")).collect();
        named_text(
            &mut v,
            "MM_LeftProfile",
            Rect::new(-1, 0, w - 10, 18),
            &names,
        );
    }
    let chat = chat_text(core);
    let rect = chat_rect(core, &chat);
    markup(&mut v, rect, &chat, &chat_profile(core));
    // MouseToolTip (g:2083, 336x18 at x 2, BlockChatTextProfile): one tip
    // height below the chat text.
    if mouse_tip(core) {
        let tip = format!(
            "\u{E006}TIP: Press {} to toggle mouse and click on links",
            core.key_name("toggleCursor")
        );
        markup(
            &mut v,
            Rect::new(2, rect.bottom() + 18, 336, 18),
            &tip,
            "BlockChatTextProfile",
        );
    }
    if core.chat.scrolled_up() {
        named_text(
            &mut v,
            "MM_LeftProfile",
            Rect::new(4, rect.bottom() - 18, 27, 18),
            "VVV",
        );
    }
    v.layout(w, h);
    v
}
impl Play {
    fn print(&mut self, dialog: &str, text: &str, message: Option<String>) {
        if let Some(n) = self.view.id(dialog) {
            self.view.set_visible(n, message.is_some());
        }
        if let (Some(n), Some(message)) = (self.view.id(text), message)
            && self.view.text_of(n) != message
        {
            self.view.set_text(n, message);
        }
    }
}
impl Screen for Play {
    fn id(&self) -> ScreenId {
        ScreenId::Play
    }
    fn view(&self) -> &View {
        &self.view
    }
    fn view_mut(&mut self) -> &mut View {
        &mut self.view
    }
    fn modal(&self) -> bool {
        false
    }
    fn cursor(&self) -> bool {
        false
    }
    fn on_wake(&mut self, core: &mut Core) {
        core.hud.reset_layout();
        self.on_update(core);
    }
    fn on_update(&mut self, core: &mut Core) {
        if let Some(n) = self.view.id("HUD_EnergyBar") {
            self.view.set_visible(n, core.energy.is_some());
            self.view.set_num(n, core.energy.unwrap_or(0.0));
        }
        if let Some(n) = self.view.id("LagIcon") {
            self.view.set_visible(n, core.lagging);
        }
        // GuiCrossHairHud::onRender draws only while a first-person player or
        // vehicle is the control object; ToggleShapeNameHud (F5) hides it too.
        if let Some(n) = self.view.id("Crosshair") {
            self.view.set_visible(
                n,
                core.shape_names && core.first_person && !core.hide_crosshair,
            );
        }
        // clientCmdCenterPrint / clientCmdBottomPrint on the authored dialogs
        // (c:6921-6969): center prints get `<just:center>` and a trailing
        // newline, bottom prints are set as sent.
        let center = core.center_print.as_ref().map(|(text, _)| text);
        self.print(
            "centerPrintDlg",
            "CenterPrintText",
            center.map(|t| format!("<just:center>{t}\n")),
        );
        let bottom = core.bottom_print.as_ref();
        self.print(
            "bottomPrintDlg",
            "BottomPrintText",
            bottom.map(|(t, ..)| t.clone()),
        );
        if let Some(n) = self.view.id("bottomPrintBar") {
            self.view
                .set_visible(n, bottom.is_some_and(|(_, _, hide)| !hide));
        }
        // handlePlantError: the small icons replace the large ones when
        // $pref::Video::useSmallPlantErrors is set.
        let small = core.prefs.bool_or(SMALL_PLANT_ERRORS, false);
        for (name, folder, shown) in [
            ("HUD_PlantError", "planterrors", !small),
            ("HUD_PlantErrorSmall", "planterrors_small", small),
        ] {
            let Some(n) = self.view.id(name) else {
                continue;
            };
            let error = core.plant_error.map(|(e, _)| e).filter(|_| shown);
            self.view
                .set_visible(n, error.is_some_and(|e| e != PlantError::Forbidden));
            if let Some(e) = error {
                let name = match e {
                    PlantError::Overlap => "overlap",
                    PlantError::Float => "float",
                    PlantError::Stuck => "stuck",
                    PlantError::Unstable => "unstable",
                    PlantError::Buried => "buried",
                    PlantError::Forbidden => "stuck",
                    PlantError::TooFar => "toofar",
                    PlantError::Limit => "limit",
                };
                self.view.state(n).bitmap =
                    Some(format!("base/client/ui/{folder}/planterror_{name}"));
            }
        }
        // toggleSuperShift: HUD_SuperShift.setVisible($SuperShift), kept on
        // the bottom edge from 1024 wide and above the inventory below it.
        if let Some(n) = self.view.id("HUD_SuperShift") {
            self.view.set_visible(n, core.super_shift);
            let h = self.view.node(n).ctrl.extent[1];
            let (_, height) = core.logical;
            let y = if core.logical.0 >= 1024 {
                height - h
            } else {
                height - (87 + h)
            };
            // Authored bottom sizing keeps this absolute position.
            if self.view.node(n).rect.y != y {
                self.view.nodes[n].ctrl.position[1] = y;
                self.view.layout(core.logical.0, core.logical.1);
            }
        }
    }
    fn tick(&mut self, _dt: u64, core: &mut Core) {
        self.on_update(core);
    }
    fn draw(&self, pack: &Pack, dl: &mut DrawList, core: &Core) {
        let lens = scope_overlay(dl, core);
        self.view.draw(pack, dl);
        // Through a scope, names show only in its lens.
        if core.shape_names && lens.is_none_or(|lens| dl.push_clip(lens)) {
            name_tags(pack, dl, core);
            if lens.is_some() {
                dl.pop_clip();
            }
        }
        hud(core).draw(pack, dl);
        package_panels(pack, dl, core);
        captions(pack, dl, core);
    }
}

/// A scope's picture over the whole screen while aiming (`Zoom::overlay`),
/// beneath the HUD: fitted to the screen's height and centred, with the
/// screen either side of it black. Returns the picture's rectangle.
fn scope_overlay(dl: &mut DrawList, core: &Core) -> Option<Rect> {
    let (key, aspect) = core.scope_overlay?;
    let (w, h) = core.logical;
    if w <= 0 || h <= 0 {
        return None;
    }
    let side = h as f32 * aspect;
    let x = (w as f32 - side) / 2.0;
    let lens = Rect::new(x.floor() as i32, 0, side.ceil() as i32, h);
    let black = [0, 0, 0, 255];
    dl.fill(Rect::new(0, 0, lens.x.max(0), h), black);
    dl.fill(
        Rect::new(lens.right(), 0, (w - lens.right()).max(0), h),
        black,
    );
    dl.image(
        crate::pack::TexKey::External(key),
        [0.0, 0.0, 1.0, 1.0],
        [x, 0.0, side, h as f32],
        [255; 4],
        crate::draw::Filter::Linear,
    );
    Some(lens)
}
