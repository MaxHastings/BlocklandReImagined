//! Tune Body: `/tune` opens it for the host, over the body they play (or the
//! one named). Each number of the body, by part (Movement, Hands, Camera,
//! Seats, Animation, Health), is a slider with its exact value beside it.
//! A change goes to the host once the slider has been still for a moment,
//! and everyone in the game plays the new number at once. Save to Add-On
//! writes the changes into the body's file; Undo Changes puts back what the
//! window changed since it opened or last saved. Built natively in the
//! Environment window's style. Design: `docs/modding/tuning.md`.
use super::*;
use crate::api::{TuneField, TuneView, UiAction};
use crate::view::EventKind;

const WINDOW: &str = "TB_Window";
const GROUP: &str = "TB_Group";
const ROWS: &str = "TB_Rows";
const SCROLL: &str = "TB_Scroll";
const STATUS: &str = "TB_Status";
const UNDO: &str = "TB_Undo";
const SAVE: &str = "TB_Save";
const DONE: &str = "TB_Done";
const ROW_HEIGHT: i32 = 26;
/// How long a slider stays still before its number goes to the host.
const LIVE_DELAY_MS: u64 = 200;
const WIDTH: i32 = 480;
const HEIGHT: i32 = 400;

fn named(mut c: Control, name: &str) -> Control {
    c.name = Some(name.into());
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
fn slider_name(i: usize) -> String {
    format!("TB_Slider{i}")
}
fn value_name(i: usize) -> String {
    format!("TB_Value{i}")
}
/// A number as the value box shows it: short, without trailing zeros.
fn shown(value: f32) -> String {
    let text = format!("{value:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".into()
    } else {
        text.into()
    }
}

pub struct Tune {
    view: View,
    /// The body the rows show.
    body: String,
    /// Each part's name, in the order the popup lists them.
    groups: Vec<String>,
    group: usize,
    /// The shown rows' fields, by row.
    rows: Vec<TuneField>,
    /// Numbers moved but not yet sent, and how long the sliders have been
    /// still.
    moved: Vec<(String, f32)>,
    idle_ms: u64,
}

impl Tune {
    pub fn new(core: &mut Core) -> Self {
        let mut root = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480));
        let mut win = ctrl(
            "GuiWindowCtrl",
            "BlockWindowProfile",
            Rect::new((640 - WIDTH) / 2, (480 - HEIGHT) / 2, WIDTH, HEIGHT),
        );
        win.text = Some("Tune Body".into());
        win.name = Some(WINDOW.into());
        win.h_sizing = HSizing::Center;
        win.v_sizing = VSizing::Center;
        win.children
            .push(text("GuiTextProfile", Rect::new(12, 32, 40, 18), "Part:"));
        let mut group = named(
            ctrl(
                "GuiPopUpMenuCtrl",
                "GuiPopUpMenuProfile",
                Rect::new(52, 30, 150, 20),
            ),
            GROUP,
        );
        group.command = Some(GROUP.into());
        win.children.push(group);
        let mut scroll = named(
            ctrl(
                "GuiScrollCtrl",
                "BlockScrollProfile",
                Rect::new(12, 58, WIDTH - 24, 256),
            ),
            SCROLL,
        );
        scroll
            .fields
            .insert("hScrollBar".into(), "alwaysOff".into());
        scroll.fields.insert("vScrollBar".into(), "dynamic".into());
        scroll.children.push(named(
            ctrl(
                "GuiControl",
                "GuiDefaultProfile",
                Rect::new(0, 0, WIDTH - 42, ROW_HEIGHT),
            ),
            ROWS,
        ));
        win.children.push(scroll);
        let mut status = named(
            text("GuiMLTextProfile", Rect::new(12, 320, WIDTH - 24, 34), ""),
            STATUS,
        );
        status.class = "GuiMLTextCtrl".into();
        win.children.push(status);
        for (x, w, label, name) in [
            (12, 120, "Undo Changes", UNDO),
            (WIDTH - 236, 120, "Save to Add-On", SAVE),
            (WIDTH - 110, 98, "Done", DONE),
        ] {
            win.children
                .push(push_button(Rect::new(x, 360, w, 28), label, name));
        }
        root.children.push(win);
        let mut view = View::new(&root);
        view.measure(&core.pack);
        let mut screen = Self {
            view,
            body: String::new(),
            groups: Vec::new(),
            group: 0,
            rows: Vec::new(),
            moved: Vec::new(),
            idle_ms: 0,
        };
        screen.refresh(core);
        screen
    }
    /// Show the model: a new body rebuilds the rows; the same body keeps
    /// the part picked and only moves the numbers.
    fn refresh(&mut self, core: &Core) {
        let Some(tune) = &core.tune else {
            return;
        };
        let groups: Vec<String> = tune.fields.iter().fold(Vec::new(), |mut out, f| {
            if !out.contains(&f.group) {
                out.push(f.group.clone());
            }
            out
        });
        if tune.body != self.body || groups != self.groups {
            let picked = self.groups.get(self.group).cloned();
            self.body = tune.body.clone();
            self.groups = groups;
            self.group = picked
                .and_then(|g| self.groups.iter().position(|n| *n == g))
                .unwrap_or(0);
            if let Some(n) = self.view.id(GROUP) {
                self.view.state(n).items = self
                    .groups
                    .iter()
                    .enumerate()
                    .map(|(i, g)| (g.clone(), i as i64))
                    .collect();
                self.view.select(n, Some(self.group as i64));
            }
            self.build_rows(tune);
        } else {
            self.show_values(tune);
        }
        if let Some(win) = self.view.id(WINDOW) {
            self.view.nodes[win].ctrl.text = Some(format!("Tune Body: {}", tune.name));
        }
        if let Some(n) = self.view.id(STATUS) {
            let status = if !tune.status.is_empty() {
                tune.status.clone()
            } else if !tune.cannot_save.is_empty() {
                tune.cannot_save.clone()
            } else if tune.unsaved {
                "Everyone plays these numbers now. Save to Add-On keeps them in the Add-On's file."
                    .into()
            } else {
                "Move a slider or type a number: everyone in the game plays it at once.".into()
            };
            self.view.set_text(n, format!("<font:Arial:14>{status}"));
        }
        if let Some(n) = self.view.id(SAVE) {
            self.view
                .set_active(n, tune.cannot_save.is_empty() && tune.unsaved);
        }
        if let Some(n) = self.view.id(UNDO) {
            self.view.set_active(n, tune.unsaved);
        }
    }
    fn build_rows(&mut self, tune: &TuneView) {
        let Some(rows) = self.view.id(ROWS) else {
            return;
        };
        self.view.clear_children(rows);
        let group = self.groups.get(self.group).cloned().unwrap_or_default();
        self.rows = tune
            .fields
            .iter()
            .filter(|f| f.group == group)
            .cloned()
            .collect();
        let width = WIDTH - 42;
        for (i, field) in self.rows.iter().enumerate() {
            let y = i as i32 * ROW_HEIGHT + 4;
            self.view.add(
                rows,
                text("GuiTextProfile", Rect::new(6, y + 1, 170, 20), &field.label),
            );
            let mut slider = named(
                ctrl(
                    "GuiSliderCtrl",
                    "GuiDefaultProfile",
                    Rect::new(180, y, width - 260, 20),
                ),
                &slider_name(i),
            );
            slider
                .fields
                .insert("range".into(), format!("{} {}", field.min, field.max));
            slider.fields.insert("snap".into(), "0".into());
            self.view.add(rows, slider);
            let mut value = named(
                ctrl(
                    "GuiTextEditCtrl",
                    "GuiTextEditProfile",
                    Rect::new(width - 72, y, 66, 20),
                ),
                &value_name(i),
            );
            value.command = Some(value_name(i));
            self.view.add(rows, value);
        }
        self.view.nodes[rows].ctrl.extent[1] = self.rows.len() as i32 * ROW_HEIGHT + 8;
        if let Some(scroll) = self.view.id(SCROLL) {
            self.view.scroll_to(scroll, 0);
        }
        self.view.relayout();
        self.show_values(tune);
    }
    /// Each row's number from the model, except a value box being typed in.
    fn show_values(&mut self, tune: &TuneView) {
        for (i, row) in self.rows.iter_mut().enumerate() {
            let Some(field) = tune.fields.iter().find(|f| f.path == row.path) else {
                continue;
            };
            // A number moved here and not yet answered stays where it is.
            if self.moved.iter().any(|(p, _)| *p == row.path) {
                continue;
            }
            row.value = field.value;
            if let Some(n) = self.view.id(&slider_name(i)) {
                self.view.set_num(n, field.value.clamp(row.min, row.max));
            }
            if let Some(n) = self.view.id(&value_name(i))
                && self.view.focus != Some(n)
            {
                self.view.set_text(n, shown(field.value));
            }
        }
    }
    /// Note a moved number; it goes to the host once things are still.
    fn moved(&mut self, row: usize, value: f32) {
        let Some(field) = self.rows.get_mut(row) else {
            return;
        };
        field.value = value;
        let path = field.path.clone();
        self.moved.retain(|(p, _)| *p != path);
        self.moved.push((path, value));
        self.idle_ms = 0;
    }
    fn send(&mut self, core: &mut Core) {
        if self.moved.is_empty() {
            return;
        }
        let changes = std::mem::take(&mut self.moved);
        core.request(UiAction::TuneSet {
            body: self.body.clone(),
            changes,
        });
    }
    fn close(&mut self, core: &mut Core) {
        self.send(core);
        core.tune = None;
        core.request(UiAction::TuneClosed);
        core.pop(ScreenId::Tune);
    }
    fn row_of(name: &str, prefix: &str) -> Option<usize> {
        name.strip_prefix(prefix)?.parse().ok()
    }
}

impl Screen for Tune {
    fn id(&self) -> ScreenId {
        ScreenId::Tune
    }
    fn view(&self) -> &View {
        &self.view
    }
    fn view_mut(&mut self) -> &mut View {
        &mut self.view
    }
    fn no_shift(&self) -> bool {
        true
    }
    fn on_update(&mut self, core: &mut Core) {
        if core.tune.is_none() {
            core.pop(ScreenId::Tune);
            return;
        }
        self.refresh(core);
    }
    fn tick(&mut self, dt_ms: u64, core: &mut Core) {
        self.idle_ms = self.idle_ms.saturating_add(dt_ms);
        if self.idle_ms >= LIVE_DELAY_MS {
            self.send(core);
        }
    }
    fn on_key(&mut self, key: Key, _mods: Modifiers, core: &mut Core) -> bool {
        if key == Key::Escape {
            self.close(core);
            return true;
        }
        false
    }
    fn on_event(&mut self, ev: &ViewEvent, core: &mut Core) {
        let name = self
            .view
            .node(ev.node)
            .ctrl
            .name
            .clone()
            .unwrap_or_default();
        if ev.kind == EventKind::Close {
            self.close(core);
            return;
        }
        if ev.kind == EventKind::Changed {
            if name == GROUP {
                if let Some(i) = self.view.selected(ev.node) {
                    self.group = usize::try_from(i).unwrap_or(0);
                    if let Some(tune) = core.tune.clone() {
                        self.build_rows(&tune);
                    }
                }
            } else if let Some(i) = Self::row_of(&name, "TB_Slider") {
                let value = self.view.num(ev.node);
                self.moved(i, value);
                if let Some(n) = self.view.id(&value_name(i)) {
                    self.view.set_text(n, shown(value));
                }
            }
            return;
        }
        if !matches!(ev.kind, EventKind::Click | EventKind::Submit) {
            return;
        }
        if let Some(i) = Self::row_of(&name, "TB_Value") {
            // A typed number may go past the slider's ends: the host checks
            // it against what the body allows.
            let typed = self.view.text_of(ev.node);
            match typed.trim().parse::<f32>() {
                Ok(value) if value.is_finite() => {
                    self.moved(i, value);
                    if let Some(n) = self.view.id(&slider_name(i))
                        && let Some(row) = self.rows.get(i)
                    {
                        self.view.set_num(n, value.clamp(row.min, row.max));
                    }
                    self.idle_ms = LIVE_DELAY_MS;
                    self.send(core);
                }
                _ => {
                    if let Some(tune) = &mut core.tune {
                        tune.status = format!("{} is not a number.", typed.trim());
                    }
                    self.refresh(core);
                }
            }
            return;
        }
        match name.as_str() {
            UNDO => {
                self.moved.clear();
                core.request(UiAction::TuneUndo {
                    body: self.body.clone(),
                });
            }
            SAVE => {
                self.send(core);
                core.request(UiAction::TuneSave {
                    body: self.body.clone(),
                });
            }
            DONE => self.close(core),
            _ => {}
        }
    }
}
