//! The Admin Menu's Environment window and its colour picker, laid out after
//! v21's `EnvironmentGui` and `ColorPickerGui` and built natively (none of
//! v21's GUI files are used). Simple picks a whole look; Advanced sets each
//! value. Apply sends the draft to the host, which checks the rank and every
//! value, then gives it to every player.
use super::*;
use crate::models::admin::{AdminAction, AdminFeature};
use crate::models::environment::{ColorField, NumberField, hsv, rgb};
use crate::view::EventKind;
use bri_console::Clamp;

fn named(mut c: Control, name: &str) -> Control {
    c.name = Some(name.into());
    c
}
fn dialog(title: &str, w: i32, h: i32) -> (Control, Control) {
    let root = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480));
    let mut win = ctrl(
        "GuiWindowCtrl",
        "BlockWindowProfile",
        Rect::new((640 - w) / 2, (480 - h) / 2, w, h),
    );
    win.text = Some(title.into());
    win.h_sizing = HSizing::Center;
    win.v_sizing = VSizing::Center;
    (root, win)
}
fn scroll(name: &str, r: Rect) -> Control {
    let mut c = named(ctrl("GuiScrollCtrl", "BlockScrollProfile", r), name);
    c.fields.insert("hScrollBar".into(), "alwaysOff".into());
    c.fields.insert("vScrollBar".into(), "dynamic".into());
    c
}
fn push_button(r: Rect, label: &str, name: &str) -> Control {
    named(
        button(
            "BlockButtonProfile",
            r,
            "base/client/ui/button1",
            label,
            name,
        ),
        name,
    )
}
fn slider(r: Rect, name: &str, (lo, hi): (f32, f32)) -> Control {
    let mut c = named(ctrl("GuiSliderCtrl", "GuiDefaultProfile", r), name);
    c.fields.insert("range".into(), format!("{lo} {hi}"));
    c.fields.insert("snap".into(), "0".into());
    c
}
fn popup(r: Rect, name: &str) -> Control {
    let mut c = named(ctrl("GuiPopUpMenuCtrl", "GuiPopUpMenuProfile", r), name);
    c.command = Some(name.into());
    c
}
fn check(r: Rect, label: &str, name: &str) -> Control {
    let mut c = named(ctrl("GuiCheckBoxCtrl", "GuiCheckBoxProfile", r), name);
    c.text = Some(label.into());
    c.command = Some(name.into());
    c
}
/// A colour row's button: the colour over a checkerboard (so a colour's
/// strength shows), the whole of it clickable.
fn color_button(r: Rect, name: &str) -> Control {
    let mut b = button("BlockButtonProfile", r, "base/client/ui/button1", "", name);
    b.name = Some(name.into());
    let inner = Rect::new(4, 4, r.w - 8, r.h - 8);
    b.children.extend(checker(inner));
    b.children.push(named(
        swatch(inner, rgba([0.0; 4])),
        &format!("{name}_Swatch"),
    ));
    b
}
/// Light and dark squares behind a colour with alpha.
fn checker(r: Rect) -> Vec<Control> {
    const CELL: i32 = 6;
    let mut out = vec![swatch(r, rgba([0.95, 0.95, 0.95, 1.0]))];
    let mut y = 0;
    while y < r.h {
        let mut x = if (y / CELL) % 2 == 0 { 0 } else { CELL };
        while x < r.w {
            out.push(swatch(
                Rect::new(r.x + x, r.y + y, CELL.min(r.w - x), CELL.min(r.h - y)),
                rgba([0.7, 0.7, 0.7, 1.0]),
            ));
            x += CELL * 2;
        }
        y += CELL;
    }
    out
}

/// The Advanced tab's rows, in v21's order.
#[derive(Clone, Copy)]
enum Row {
    DayCycle,
    Number(NumberField),
    Color(ColorField),
    VignetteMultiply,
}
const ROWS: [Row; 16] = [
    Row::DayCycle,
    Row::Number(NumberField::DayLength),
    Row::Number(NumberField::TimeOfDay),
    Row::Number(NumberField::SunAzimuth),
    Row::Number(NumberField::SunElevation),
    Row::Color(ColorField::DirectLight),
    Row::Color(ColorField::AmbientLight),
    Row::Color(ColorField::ShadowColor),
    Row::Color(ColorField::SunFlare),
    Row::Number(NumberField::FlareSize),
    Row::Number(NumberField::VisibleDistance),
    Row::Number(NumberField::FogDistance),
    Row::Color(ColorField::FogColor),
    Row::Color(ColorField::SkyColor),
    Row::Color(ColorField::Vignette),
    Row::VignetteMultiply,
];
const ROW_HEIGHT: i32 = 28;
/// The favourites slots' list (Add-On Settings keeps its favourites the
/// same way: a slot list with Load and Store).
const FAVS: &str = "EnvFavs";
const FAV_SLOTS: u8 = 10;
const COLOR_FIELDS: [ColorField; 7] = [
    ColorField::DirectLight,
    ColorField::AmbientLight,
    ColorField::ShadowColor,
    ColorField::SunFlare,
    ColorField::FogColor,
    ColorField::SkyColor,
    ColorField::Vignette,
];
fn number_name(f: NumberField, page: &str) -> String {
    format!("Env{page}_{f:?}")
}
fn color_name(f: ColorField) -> String {
    format!("Env_{f:?}")
}

pub struct Environment {
    view: View,
    advanced: bool,
    seen: Option<u64>,
}
impl Environment {
    pub fn new(core: &mut Core) -> Self {
        core.environment.begin();
        let (mut root, mut win) = dialog("Environment", 440, 470);
        win.children.push(push_button(
            Rect::new(12, 30, 100, 24),
            "Simple",
            "EnvTabSimple",
        ));
        win.children.push(push_button(
            Rect::new(114, 30, 100, 24),
            "Advanced",
            "EnvTabAdvanced",
        ));
        // Simple: a look, and the day/night cycle.
        let mut simple = named(
            ctrl(
                "GuiControl",
                "GuiDefaultProfile",
                Rect::new(12, 60, 416, 306),
            ),
            "EnvSimplePage",
        );
        simple
            .children
            .push(text("GuiTextProfile", Rect::new(0, 0, 200, 18), "Look:"));
        let mut looks = scroll("EnvPresetScroll", Rect::new(0, 20, 200, 200));
        let mut list = named(
            ctrl(
                "GuiTextListCtrl",
                "GuiTextListProfile",
                Rect::new(0, 0, 184, 16),
            ),
            "EnvPresets",
        );
        list.fields.insert("columns".into(), "0".into());
        looks.children.push(list);
        simple.children.push(looks);
        simple.children.push(check(
            Rect::new(215, 20, 200, 20),
            "Day and night cycle",
            "EnvDayCycleSimple",
        ));
        for (i, f) in [NumberField::DayLength, NumberField::TimeOfDay]
            .into_iter()
            .enumerate()
        {
            let y = 52 + i as i32 * 50;
            simple.children.push(text(
                "GuiTextProfile",
                Rect::new(215, y, 120, 18),
                f.label(),
            ));
            simple.children.push(named(
                text("GuiTextProfile", Rect::new(335, y, 70, 18), ""),
                &format!("{}_Value", number_name(f, "S")),
            ));
            simple.children.push(slider(
                Rect::new(215, y + 20, 190, 20),
                &number_name(f, "S"),
                f.range(),
            ));
        }
        let mut hint = text(
            "GuiMLTextProfile",
            Rect::new(0, 232, 416, 60),
            "Pick a look, then Apply. Everyone in the server sees it. \
             Advanced sets each value; Reset gives the map its own look back.",
        );
        hint.class = "GuiMLTextCtrl".into();
        simple.children.push(hint);
        win.children.push(simple);
        // Advanced: every value.
        let mut advanced = scroll("EnvAdvancedPage", Rect::new(12, 60, 416, 306));
        let mut rows = named(
            ctrl(
                "GuiControl",
                "GuiDefaultProfile",
                Rect::new(0, 0, 398, ROWS.len() as i32 * ROW_HEIGHT + 4),
            ),
            "EnvRows",
        );
        for (i, row) in ROWS.iter().enumerate() {
            let y = i as i32 * ROW_HEIGHT + 4;
            let label = match row {
                Row::DayCycle => "Day Cycle",
                Row::Number(f) => f.label(),
                Row::Color(f) => f.label(),
                Row::VignetteMultiply => "Vignette Multiply",
            };
            rows.children
                .push(text("GuiTextProfile", Rect::new(6, y, 130, 20), label));
            match *row {
                Row::DayCycle => {
                    rows.children
                        .push(check(Rect::new(140, y, 200, 20), "", "EnvDayCycleAdvanced"))
                }
                Row::VignetteMultiply => {
                    rows.children
                        .push(check(Rect::new(140, y, 200, 20), "", "EnvVignetteMultiply"))
                }
                Row::Number(f) => {
                    rows.children.push(slider(
                        Rect::new(140, y, 180, 20),
                        &number_name(f, "A"),
                        f.range(),
                    ));
                    rows.children.push(named(
                        text("GuiTextProfile", Rect::new(326, y, 70, 20), ""),
                        &format!("{}_Value", number_name(f, "A")),
                    ));
                }
                Row::Color(f) => rows
                    .children
                    .push(color_button(Rect::new(140, y - 2, 80, 24), &color_name(f))),
            }
        }
        advanced.children.push(rows);
        win.children.push(advanced);
        // Favourites: a slot to Load the rows from or Store them in.
        win.children.push(text(
            "GuiTextProfile",
            Rect::new(12, 372, 70, 20),
            "Favorites:",
        ));
        win.children.push(popup(Rect::new(84, 372, 110, 20), FAVS));
        win.children.push(push_button(
            Rect::new(198, 370, 56, 24),
            "Load",
            "EnvFavLoad",
        ));
        win.children.push(push_button(
            Rect::new(258, 370, 56, 24),
            "Store",
            "EnvFavSave",
        ));
        let mut status = named(
            text("GuiMLTextProfile", Rect::new(12, 400, 416, 20), ""),
            "EnvStatus",
        );
        status.class = "GuiMLTextCtrl".into();
        win.children.push(status);
        for (x, label, name) in [
            (12, "Reset", "EnvReset"),
            (228, "Close", "EnvClose"),
            (330, "Apply", "EnvApply"),
        ] {
            win.children
                .push(push_button(Rect::new(x, 428, 98, 28), label, name));
        }
        root.children.push(win);
        let mut view = View::new(&root);
        view.measure(&core.pack);
        if let Some(n) = view.id("EnvPresets") {
            view.state(n).items = bri_content::atmosphere::presets()
                .iter()
                .enumerate()
                .map(|(i, (name, _))| ((*name).to_string(), i as i64))
                .collect();
        }
        let mut screen = Self {
            view,
            advanced: false,
            seen: None,
        };
        screen.fill_favorites(core);
        screen.refresh(core);
        screen
    }
    /// The slot list, each slot saying whether it holds a favourite; the
    /// picked slot stays picked.
    fn fill_favorites(&mut self, core: &Core) {
        if let Some(n) = self.view.id(FAVS) {
            let current = self.view.selected(n).unwrap_or(0);
            self.view.state(n).items = (0..FAV_SLOTS)
                .map(|slot| {
                    let label = if core.settings.environment_favorites.contains_key(&slot) {
                        format!("Slot {}", slot + 1)
                    } else {
                        format!("Slot {} (empty)", slot + 1)
                    };
                    (label, i64::from(slot))
                })
                .collect();
            self.view.select(n, Some(current));
        }
    }
    fn favorite_slot(&self) -> u8 {
        self.view
            .id(FAVS)
            .and_then(|n| self.view.selected(n))
            .and_then(|s| u8::try_from(s).ok())
            .filter(|s| *s < FAV_SLOTS)
            .unwrap_or(0)
    }
    /// Keep the rows in the picked slot (what they show, applied or not).
    fn save_favorite(&mut self, core: &mut Core) {
        let slot = self.favorite_slot();
        let favorite = core.environment.favorite();
        core.settings.environment_favorites.insert(slot, favorite);
        core.save_settings();
        self.fill_favorites(core);
        core.admin.status = format!("Saved in slot {}.", slot + 1);
    }
    /// Fill the rows from the picked slot; Apply then sends them.
    fn load_favorite(&mut self, core: &mut Core) {
        let slot = self.favorite_slot();
        let Some(favorite) = core.settings.environment_favorites.get(&slot).cloned() else {
            core.admin.status = format!("Slot {} is empty.", slot + 1);
            return;
        };
        if let Err(e) = favorite.validate() {
            core.admin.status = format!("Slot {} cannot be used: {e}", slot + 1);
            return;
        }
        core.environment.load_favorite(favorite);
        core.admin.status = format!("Loaded slot {}. Not applied yet.", slot + 1);
    }
    fn refresh(&mut self, core: &Core) {
        self.seen = Some(core.environment.revision);
        let m = &core.environment;
        let v = &mut self.view;
        let busy = core.admin.busy();
        for (name, shown) in [
            ("EnvSimplePage", !self.advanced),
            ("EnvAdvancedPage", self.advanced),
        ] {
            if let Some(n) = v.id(name) {
                v.set_visible(n, shown);
            }
        }
        for (name, current) in [
            ("EnvTabSimple", !self.advanced),
            ("EnvTabAdvanced", self.advanced),
        ] {
            if let Some(n) = v.id(name) {
                v.set_active(n, !current);
            }
        }
        let cycle = m.day_cycle();
        for name in ["EnvDayCycleSimple", "EnvDayCycleAdvanced"] {
            if let Some(n) = v.id(name) {
                v.set_bool(n, cycle);
            }
        }
        if let Some(n) = v.id("EnvVignetteMultiply") {
            v.set_bool(n, m.vignette_multiply());
        }
        for f in NumberField::ALL {
            let value = m.number(f);
            let active = cycle || !matches!(f, NumberField::DayLength | NumberField::TimeOfDay);
            for page in ["S", "A"] {
                let name = number_name(f, page);
                if let Some(n) = v.id(&name) {
                    v.set_num(n, value);
                    v.set_active(n, active);
                }
                if let Some(n) = v.id(&format!("{name}_Value")) {
                    v.set_text(n, if active { f.format(value) } else { "-".into() });
                }
            }
        }
        for f in COLOR_FIELDS {
            if let Some(n) = v.id(&format!("{}_Swatch", color_name(f))) {
                v.state(n).tint = Some(rgba(m.color(f)));
            }
        }
        if let Some(n) = v.id("EnvPresets") {
            v.select(n, m.current_preset().map(|i| i as i64));
        }
        if let Some(n) = v.id("EnvApply") {
            v.set_active(
                n,
                !busy
                    && m.changed()
                    && core.admin.allowed(&AdminAction::SetEnvironment {
                        settings: Box::new(m.settings()),
                    }),
            );
        }
        for name in ["EnvReset", "EnvFavLoad", "EnvFavSave"] {
            if let Some(n) = v.id(name) {
                v.set_active(n, !busy);
            }
        }
        let status = if !core.admin.status.is_empty() {
            core.admin.status.clone()
        } else if m.changed() {
            "Not applied yet.".into()
        } else {
            String::new()
        };
        if let Some(n) = v.id("EnvStatus") {
            v.set_text(n, status.replace(['<', '>'], ""));
        }
    }
    fn close(core: &mut Core) {
        core.environment.end();
        core.pop(ScreenId::AdminColorPicker);
        core.pop(ScreenId::AdminEnvironment);
    }
}
impl Screen for Environment {
    fn id(&self) -> ScreenId {
        ScreenId::AdminEnvironment
    }
    fn view(&self) -> &View {
        &self.view
    }
    fn view_mut(&mut self) -> &mut View {
        &mut self.view
    }
    fn on_update(&mut self, core: &mut Core) {
        if core.admin.snapshot.is_some() && !core.admin.available(AdminFeature::Environment) {
            Self::close(core);
            return;
        }
        self.refresh(core);
    }
    fn tick(&mut self, _dt_ms: u64, core: &mut Core) {
        if self.seen != Some(core.environment.revision) {
            self.refresh(core);
        }
    }
    fn on_key(&mut self, key: Key, _mods: Modifiers, core: &mut Core) -> bool {
        if key == Key::Escape {
            Self::close(core);
            return true;
        }
        false
    }
    fn on_event(&mut self, ev: &ViewEvent, core: &mut Core) {
        if ev.kind == EventKind::Close {
            Self::close(core);
            return;
        }
        if !self.view.node(ev.node).state.active {
            return;
        }
        let name = self
            .view
            .node(ev.node)
            .ctrl
            .name
            .clone()
            .unwrap_or_default();
        // A check box flips on `Changed`, before its `Click`; it is read
        // here, because the refresh below shows the model's value again.
        if ev.kind == EventKind::Changed {
            if matches!(name.as_str(), "EnvDayCycleSimple" | "EnvDayCycleAdvanced") {
                let on = self.view.bool_value(ev.node);
                core.environment.set_day_cycle(on);
            } else if name == "EnvVignetteMultiply" {
                let on = self.view.bool_value(ev.node);
                core.environment.set_vignette_multiply(on);
            } else if name == "EnvPresets" {
                if let Some(i) = self.view.selected(ev.node) {
                    core.environment.preset(i as usize);
                }
            } else if let Some(f) = NumberField::ALL
                .into_iter()
                .find(|f| name == number_name(*f, "S") || name == number_name(*f, "A"))
            {
                let value = self.view.num(ev.node);
                core.environment.set_number(f, value);
            }
            self.refresh(core);
            return;
        }
        if !matches!(ev.kind, EventKind::Click | EventKind::Submit) {
            return;
        }
        match command_of(&self.view, ev.node).as_str() {
            "EnvTabSimple" => self.advanced = false,
            "EnvTabAdvanced" => self.advanced = true,
            "EnvReset" => {
                core.admin.status.clear();
                core.environment.reset();
            }
            "EnvFavSave" => self.save_favorite(core),
            "EnvFavLoad" => self.load_favorite(core),
            "EnvClose" => {
                Self::close(core);
                return;
            }
            "EnvApply" => {
                let settings = core.environment.settings();
                core.admin_request(AdminAction::SetEnvironment {
                    settings: Box::new(settings),
                });
            }
            command => {
                if let Some(f) = COLOR_FIELDS.into_iter().find(|f| command == color_name(*f)) {
                    core.environment.picking = Some(f);
                    core.push(ScreenId::AdminColorPicker);
                }
            }
        }
        self.refresh(core);
    }
}

/// v21's colour picker: RGB or HSV sliders, alpha for colours with a
/// strength, the old and new colour side by side.
pub struct ColorPicker {
    view: View,
    field: Option<ColorField>,
    before: [f32; 4],
    color: [f32; 4],
    hsv: bool,
}
const CHANNELS: [&str; 4] = ["EnvPick0", "EnvPick1", "EnvPick2", "EnvPick3"];
impl ColorPicker {
    pub fn new(core: &mut Core) -> Self {
        let field = core.environment.picking;
        let title = field.map_or("Color", ColorField::label);
        let (mut root, mut win) = dialog(title, 300, 310);
        win.children
            .push(push_button(Rect::new(15, 30, 60, 22), "RGB", "EnvPickRgb"));
        win.children
            .push(push_button(Rect::new(77, 30, 60, 22), "HSV", "EnvPickHsv"));
        for (i, name) in CHANNELS.iter().enumerate() {
            let y = 62 + i as i32 * 30;
            win.children.push(named(
                text("GuiTextProfile", Rect::new(15, y, 22, 20), ""),
                &format!("{name}_Label"),
            ));
            win.children
                .push(slider(Rect::new(40, y, 180, 20), name, (0.0, 1.0)));
            win.children.push(named(
                text("GuiTextProfile", Rect::new(226, y, 60, 20), ""),
                &format!("{name}_Value"),
            ));
        }
        for (x, name) in [(15, "EnvPickBefore"), (150, "EnvPickAfter")] {
            let r = Rect::new(x, 186, 135, 60);
            win.children.extend(checker(r));
            win.children.push(named(swatch(r, rgba([0.0; 4])), name));
        }
        win.children.push(push_button(
            Rect::new(15, 262, 98, 28),
            "Cancel",
            "EnvPickCancel",
        ));
        win.children.push(push_button(
            Rect::new(187, 262, 98, 28),
            "Done >>",
            "EnvPickDone",
        ));
        root.children.push(win);
        let mut view = View::new(&root);
        view.measure(&core.pack);
        let color = field.map_or([1.0; 4], |f| core.environment.color(f));
        let mut picker = Self {
            view,
            field,
            before: color,
            color,
            hsv: false,
        };
        picker.show();
        picker
    }
    fn show(&mut self) {
        let alpha = self.field.is_some_and(ColorField::has_alpha);
        let rgb3 = [self.color[0], self.color[1], self.color[2]];
        let values = if self.hsv {
            let [h, s, v] = hsv(rgb3);
            [h, s, v, self.color[3]]
        } else {
            self.color
        };
        let labels = if self.hsv {
            ["H", "S", "V", "A"]
        } else {
            ["R", "G", "B", "A"]
        };
        for (i, name) in CHANNELS.iter().enumerate() {
            let shown = i < 3 || alpha;
            let hue = self.hsv && i == 0;
            if let Some(n) = self.view.id(name) {
                self.view.set_visible(n, shown);
                let range = if hue { "0 360" } else { "0 1" };
                self.view.nodes[n]
                    .ctrl
                    .fields
                    .insert("range".into(), range.into());
                self.view.set_num(n, values[i]);
            }
            if let Some(n) = self.view.id(&format!("{name}_Label")) {
                self.view.set_visible(n, shown);
                self.view.set_text(n, labels[i]);
            }
            if let Some(n) = self.view.id(&format!("{name}_Value")) {
                self.view.set_visible(n, shown);
                let text = if hue {
                    format!("{:.0}°", values[i])
                } else {
                    format!("{:.0}", values[i] * 255.0)
                };
                self.view.set_text(n, text);
            }
        }
        for (name, c) in [("EnvPickBefore", self.before), ("EnvPickAfter", self.color)] {
            if let Some(n) = self.view.id(name) {
                let c = if alpha { c } else { [c[0], c[1], c[2], 1.0] };
                self.view.state(n).tint = Some(rgba(c));
            }
        }
        for (name, on) in [("EnvPickRgb", !self.hsv), ("EnvPickHsv", self.hsv)] {
            if let Some(n) = self.view.id(name) {
                self.view.set_active(n, !on);
            }
        }
    }
    /// The sliders' colour.
    fn read(&mut self) {
        let mut v = [0.0; 4];
        for (i, name) in CHANNELS.iter().enumerate() {
            if let Some(n) = self.view.id(name) {
                v[i] = self.view.num(n);
            }
        }
        let [a, b, c, alpha] = v;
        let rgb3 = if self.hsv {
            rgb([a, b.clamped(0.0, 1.0), c.clamped(0.0, 1.0)])
        } else {
            [a, b, c]
        };
        self.color = [rgb3[0], rgb3[1], rgb3[2], alpha].map(|x| x.clamped(0.0, 1.0));
        if !self.field.is_some_and(ColorField::has_alpha) {
            self.color[3] = 1.0;
        }
    }
}
impl Screen for ColorPicker {
    fn id(&self) -> ScreenId {
        ScreenId::AdminColorPicker
    }
    fn view(&self) -> &View {
        &self.view
    }
    fn view_mut(&mut self) -> &mut View {
        &mut self.view
    }
    fn on_key(&mut self, key: Key, _mods: Modifiers, core: &mut Core) -> bool {
        if key == Key::Escape {
            core.environment.picking = None;
            core.pop(ScreenId::AdminColorPicker);
            return true;
        }
        false
    }
    fn on_event(&mut self, ev: &ViewEvent, core: &mut Core) {
        if ev.kind == EventKind::Close {
            core.environment.picking = None;
            core.pop(ScreenId::AdminColorPicker);
            return;
        }
        if !self.view.node(ev.node).state.active {
            return;
        }
        if ev.kind == EventKind::Changed {
            self.read();
            self.show();
            return;
        }
        if ev.kind != EventKind::Click {
            return;
        }
        match command_of(&self.view, ev.node).as_str() {
            "EnvPickRgb" => self.hsv = false,
            "EnvPickHsv" => self.hsv = true,
            "EnvPickCancel" => {
                core.environment.picking = None;
                core.pop(ScreenId::AdminColorPicker);
                return;
            }
            "EnvPickDone" => {
                if let Some(f) = self.field {
                    core.environment.set_color(f, self.color);
                }
                core.environment.picking = None;
                core.pop(ScreenId::AdminColorPicker);
                return;
            }
            _ => {}
        }
        self.show();
    }
}
