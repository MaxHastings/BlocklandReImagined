//! Native runtime-built inventory HUD. Geometry/art follow createInvHud,
//! createPaintHud and createToolHud in the recovered v20 client scripts.
use super::*;
use crate::api::{IconRef, PlantError};
use crate::geom::WHITE;
use crate::models::hud::{FX_ART, ScrollMode};

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
    core.chat
        .visible(core.time_ms)
        .iter()
        .map(|l| format!("\u{E006}{}", l.text))
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
fn markup(v: &mut View, r: Rect, label: &str, style: &str) {
    let mut c = text(style, r, label);
    c.class = "GuiMLTextCtrl".into();
    v.add(v.root, c);
}

/// Rebuilt in a bounded temporary view: no detached controls accumulate as
/// inventory, paint or chat changes. The authored PlayGui remains persistent.
/// `PlayGui_ShapeNameHud`: names centered above each anchor in the HUD's
/// `BlockChatTextProfile` font and `textColor` (1 1 0.909), faded by distance.
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
        let alpha = (tag.opacity.clamp(0.0, 1.0) * 255.0) as u8;
        if alpha == 0 {
            continue;
        }
        let x = tag.x - font.width(&tag.text) as f32 / 2.0;
        let y = tag.y - font.line_height() as f32;
        font.draw(dl, x.round(), y.round(), &tag.text, [255, 255, 232, alpha], &[]);
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
        color[3] = color[3].clamp(0.1, 1.0);
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
        named_text(&mut v, "MM_LeftProfile", Rect::new(-1, 0, w - 10, 18), &names);
    }
    let chat = chat_text(core);
    let rect = chat_rect(core, &chat);
    markup(&mut v, rect, &chat, &chat_profile(core));
    if core.chat.scrolled_up() {
        named_text(
            &mut v,
            "MM_LeftProfile",
            Rect::new(4, rect.bottom() - 18, 27, 18),
            "VVV",
        );
    }
    if let Some(text) = &core.net_graph {
        fill(&mut v, Rect::new(w - 220, 4, 216, 20), [0, 0, 0, 128]);
        markup(
            &mut v,
            Rect::new(w - 216, 6, 212, 18),
            text,
            "BlockChatTextProfile",
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
        // clientCmdCenterPrint / clientCmdBottomPrint on the authored dialogs.
        let center = core.center_print.as_ref().map(|(text, _)| text);
        self.print(
            "centerPrintDlg",
            "CenterPrintText",
            center.map(|t| {
                format!(
                    "<just:center>{t}
"
                )
            }),
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
        self.view.draw(pack, dl);
        if core.shape_names {
            name_tags(pack, dl, core);
        }
        hud(core).draw(pack, dl);
    }
}
