//! Native options and input capture. Script strings identify authored widgets;
//! they are never evaluated. Settings that only configured Torque's renderer,
//! audio drivers or network stack are hidden and the remaining authored rows
//! close up, so every visible control does something.
use super::*;
use crate::api::{BindInput, UiAction};
use crate::binds::{BindMap, RemapOutcome};
use crate::input::Chord;
use crate::prefs::Prefs;
use crate::ui::Callback;
use crate::view::EventKind;
use std::collections::HashMap;

const FULLSCREEN: &str = "$pref::Video::fullScreen";
const NO_VSYNC: &str = "$pref::Video::disableVerticalSync";
const RESOLUTION: &str = "$pref::Video::resolution";
pub const CHAT_SIZE: &str = "$Pref::Gui::ChatSize";
pub const KEYBOARD_TURN_SPEED: &str = "$pref::Input::KeyboardTurnSpeed";
/// Checkbox preferences the native game honours.
const CHECKBOX_PREFS: &[&str] = &[
    FULLSCREEN,
    NO_VSYNC,
    "$pref::precipitationOn",
    "$Pref::Audio::PlayMusic",
    "$Pref::Audio::MenuSounds",
    "$Pref::Audio::PlayBrickPlantSound",
    "$Pref::Audio::PlayBrickMoveSound",
    "$Pref::Audio::PlantErrorSound",
    "$pref::HUD::showToolTips",
    "$pref::HUD::HidePaintBox",
    "$pref::HUD::HideToolBox",
    "$pref::HUD::HideBrickBox",
    "$pref::Hud::RecolorBrickIcons",
    "$pref::Gui::ShowBrickSlotNumbers",
    "$pref::Input::FastFirstThirdPerson",
    "$pref::Input::UseSuperShiftSmartToggle",
    "$pref::Input::UseSuperShiftToggle",
    "$pref::Input::QueueBrickBuying",
    "$pref::Input::ReverseBrickScroll",
    "$pref::Input::noobjet",
    "$pref::Input::MouseInvert",
];
/// Other authored controls with native behaviour.
const SUPPORTED_CONTROLS: &[&str] = &[
    "OptGraphicsResolutionMenu",
    "OptAudioVolumeMaster",
    "OptAudioVolumeShell",
    "OptAudioVolumeSim",
    "SliderControlsMouseSensitivity",
    "slider_KeyboardTurnSpeed",
    "Opt_ChatLineTime",
    "Opt_MaxChatLines",
    "OptRemapList",
];
const CHAT_SIZE_RADIO: &str = "OPT_ChatSize";
const VALUE_CLASSES: &[&str] = &[
    "GuiCheckBoxCtrl",
    "GuiRadioCtrl",
    "GuiSliderCtrl",
    "GuiPopUpMenuCtrl",
    "GuiTextEditCtrl",
    "GuiTextListCtrl",
];
const VOLUMES: &[(&str, &str, &str)] = &[
    (
        "OptAudioVolumeMaster",
        "$pref::Audio::masterVolume",
        "master",
    ),
    (
        "OptAudioVolumeShell",
        "$pref::Audio::channelVolume1",
        "shell",
    ),
    ("OptAudioVolumeSim", "$pref::Audio::channelVolume2", "sim"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DisplaySettings {
    resolution: (u32, u32),
    fullscreen: bool,
    vsync: bool,
}

fn display(p: &Prefs, fallback: (i32, i32)) -> DisplaySettings {
    let words: Vec<_> = p.str_or(RESOLUTION, "").split_whitespace().collect();
    let resolution = words
        .first()
        .and_then(|w| w.parse().ok())
        .zip(words.get(1).and_then(|h| h.parse().ok()))
        .filter(|&(w, h)| w >= 640 && h >= 480)
        .unwrap_or((fallback.0.max(640) as u32, fallback.1.max(480) as u32));
    DisplaySettings {
        resolution,
        fullscreen: p.bool_or(FULLSCREEN, false),
        vsync: !p.bool_or(NO_VSYNC, false),
    }
}
fn put_display(p: &mut Prefs, d: DisplaySettings) {
    p.set(
        RESOLUTION,
        format!("{} {} 32", d.resolution.0, d.resolution.1),
    );
    p.set_bool(FULLSCREEN, d.fullscreen);
    p.set_bool(NO_VSYNC, !d.vsync);
}

/// `$Pref::Gui::ChatSize` (0–10, v20 default 4) selects the chat HUD font
/// profiles `BlockChatTextSize<n>Profile` and friends.
pub fn chat_size(p: &Prefs) -> i64 {
    p.i64_or(CHAT_SIZE, 4).clamp(0, 10)
}

fn supported(v: &View, n: NodeId) -> bool {
    let c = &v.node(n).ctrl;
    let name = c.name.as_deref().unwrap_or_default();
    c.variable
        .as_deref()
        .is_some_and(|var| CHECKBOX_PREFS.iter().any(|p| p.eq_ignore_ascii_case(var)))
        || SUPPORTED_CONTROLS.contains(&name)
        || name.starts_with(CHAT_SIZE_RADIO)
}

fn is_value(v: &View, n: NodeId) -> bool {
    VALUE_CLASSES.contains(&v.node(n).ctrl.class.as_str())
}

/// Authored option sections are swatches whose first row is a title bar
/// swatch at (2, 2).
fn is_section(v: &View, n: NodeId) -> bool {
    let node = v.node(n);
    node.ctrl.class == "GuiSwatchCtrl"
        && node.children.iter().any(|&k| {
            let c = &v.node(k).ctrl;
            c.class == "GuiSwatchCtrl" && c.position == [2, 2]
        })
}

fn section_title(v: &View, n: NodeId) -> String {
    v.node(n)
        .children
        .iter()
        .find(|&&k| v.node(k).ctrl.class == "GuiTextCtrl" && v.node(k).ctrl.position[1] <= 3)
        .map(|&k| v.text_of(k))
        .unwrap_or_default()
}

fn find_section(v: &View, title: &str) -> Option<NodeId> {
    v.walk()
        .find(|&n| is_section(v, n) && section_title(v, n) == title)
}

fn shows_values(v: &View, n: NodeId) -> bool {
    v.node(n).state.visible
        && (is_value(v, n) || v.node(n).children.iter().any(|&k| shows_values(v, k)))
}

/// A label names the control that starts just right of it on its row.
fn labels(label: &Control, value: &Control) -> bool {
    let gap = value.position[0] - (label.position[0] + label.extent[0]);
    (-4..=24).contains(&gap)
        && label.position[1] < value.position[1] + value.extent[1]
        && value.position[1] < label.position[1] + label.extent[1]
}

/// Hide a section's labels whose control is gone, then move the
/// remaining rows up over the rows that are now empty. Returns the bottom
/// of the visible content in section coordinates.
fn close_rows(v: &mut View, section: NodeId) -> i32 {
    let body: Vec<NodeId> = v
        .node(section)
        .children
        .iter()
        .copied()
        .filter(|&k| v.node(k).ctrl.position[1] > 3)
        .collect();
    for &k in &body {
        if v.node(k).ctrl.class == "GuiTextCtrl"
            && !body.iter().any(|&o| {
                is_value(v, o)
                    && v.node(o).state.visible
                    && labels(&v.node(k).ctrl, &v.node(o).ctrl)
            })
        {
            v.set_visible(k, false);
        }
    }
    let mut rows: Vec<i32> = body.iter().map(|&k| v.node(k).ctrl.position[1]).collect();
    rows.sort_unstable();
    rows.dedup();
    let mut shifts = Vec::with_capacity(rows.len());
    let mut shift = 0;
    for (i, &row) in rows.iter().enumerate() {
        shifts.push((row, shift));
        let kept = body
            .iter()
            .any(|&k| v.node(k).ctrl.position[1] == row && v.node(k).state.visible);
        if !kept && let Some(next) = rows.get(i + 1) {
            shift += next - row;
        }
    }
    let mut bottom = 23;
    for &k in &body {
        if !v.node(k).state.visible {
            continue;
        }
        let c = &mut v.nodes[k].ctrl;
        c.position[1] -= shifts
            .iter()
            .find(|(row, _)| *row == c.position[1])
            .map_or(0, |(_, s)| *s);
        bottom = bottom.max(c.position[1] + c.extent[1]);
    }
    bottom
}

pub struct Options {
    view: View,
    draft: Prefs,
    initial: Prefs,
    saved_binds: BindMap,
    saved_hardware: (u8, u8),
    applied_display: DisplaySettings,
    pending_display: Option<(RequestId, DisplaySettings, bool)>,
    pending_enabled: Vec<NodeId>,
    resolutions: Vec<(u32, u32)>,
    committed: bool,
}

impl Options {
    pub fn new(core: &Core) -> Self {
        let current = display(&core.prefs, core.logical);
        let mut resolutions = vec![
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 720),
            (1280, 800),
            (1366, 768),
            (1600, 900),
            (1920, 1080),
            (2560, 1440),
            (3840, 2160),
            current.resolution,
        ];
        resolutions.sort_unstable();
        resolutions.dedup();
        let mut s = Self {
            view: layout_view(core, "optionsDlg"),
            draft: core.prefs.clone(),
            initial: core.prefs.clone(),
            saved_binds: core.binds.clone(),
            saved_hardware: (core.settings.mouse_type, core.settings.keyboard_type),
            applied_display: current,
            pending_display: None,
            pending_enabled: Vec::new(),
            resolutions,
            committed: false,
        };
        s.native_layout();
        // Duplicate authored names occur throughout Options. Preference identity
        // is the variable on each node, never the last matching widget name.
        for n in s.view.walk().collect::<Vec<_>>() {
            if let Some(var) = s.view.node(n).ctrl.variable.clone()
                && var.starts_with('$')
            {
                s.view.set_bool(n, core.prefs.bool_or(&var, false));
            }
        }
        s.menu(
            "OptGraphicsResolutionMenu",
            s.resolutions
                .iter()
                .enumerate()
                .map(|(i, (w, h))| (format!("{w} x {h}"), i as i64))
                .collect(),
            s.resolutions
                .iter()
                .position(|r| *r == current.resolution)
                .unwrap_or(0) as i64,
        );
        for &(name, pref, _) in VOLUMES {
            s.slider(name, core.prefs.f32_or(pref, 1.0).clamp(0.0, 1.0));
        }
        s.slider(
            "SliderControlsMouseSensitivity",
            core.prefs
                .f32_or("$pref::Input::MouseSensitivity", 0.75)
                .clamp(0.02, 2.0),
        );
        s.slider(
            "slider_KeyboardTurnSpeed",
            core.prefs.f32_or(KEYBOARD_TURN_SPEED, 0.5).clamp(0.02, 1.0),
        );
        for (name, pref, fallback) in [
            ("Opt_ChatLineTime", "$Pref::Chat::LineTime", 6500),
            ("Opt_MaxChatLines", "$Pref::Chat::MaxDisplayLines", 8),
        ] {
            if let Some(n) = s.view.id(name) {
                s.view
                    .set_text(n, core.prefs.i64_or(pref, fallback).to_string());
            }
        }
        s.set_chat_size(chat_size(&core.prefs));
        s.pane("Graphics");
        s.refresh_binds(core);
        s.smart_toggle();
        s
    }

    /// Hide the Torque-only settings and close up the authored layout
    /// around the ones that remain.
    fn native_layout(&mut self) {
        let v = &mut self.view;
        for n in v.walk().collect::<Vec<_>>() {
            let c = &v.node(n).ctrl;
            let hide = (is_value(v, n) && !supported(v, n))
                || c.name.as_deref().is_some_and(|n| n.ends_with("Blocker"))
                || c.name.as_deref() == Some("OptNetworkPane")
                || c.command.as_deref() == Some("optionsDlg.setPane(Network);")
                // The Advanced pane's Apply only applied Torque renderer state.
                || (c.class == "GuiBitmapButtonCtrl"
                    && c.command.as_deref() == Some("optionsDlg.applyGraphics();"));
            if hide {
                v.set_visible(n, false);
            }
        }
        let sections: Vec<NodeId> = v.walk().filter(|&n| is_section(v, n)).collect();
        for &n in &sections {
            if !shows_values(v, n) {
                v.set_visible(n, false);
            }
        }
        let mut bottoms = HashMap::new();
        for &n in &sections {
            if v.node(n).state.visible {
                bottoms.insert(n, close_rows(v, n));
            }
        }
        // Graphics: the quality row is gone; its side fillers go with it and
        // the two settings sections reach down to the footer strip.
        if let Some(pane) = v.id("OptGraphicsPane") {
            for k in v.node(pane).children.clone() {
                let c = &v.node(k).ctrl;
                if c.class == "GuiSwatchCtrl"
                    && v.node(k).children.is_empty()
                    && c.position[1] == 192
                {
                    v.set_visible(k, false);
                }
            }
        }
        for title in ["Display Settings", "Gui Settings"] {
            if let Some(n) = find_section(v, title) {
                v.nodes[n].ctrl.extent[1] = 322;
            }
        }
        // Gui Settings keeps two columns; the right one lost its first row.
        let show_hud = v
            .walk()
            .find(|&n| v.node(n).ctrl.variable.as_deref() == Some("$pref::HUD::showToolTips"));
        if let Some(n) = show_hud {
            v.nodes[n].ctrl.position[1] = 27;
        }
        // Audio: Volume takes the driver section's place.
        if let Some(n) = find_section(v, "Volume") {
            v.nodes[n].ctrl.position[1] = 7;
            v.nodes[n].ctrl.extent[1] = 301;
        }
        // Advanced: stack the remaining sections of the scrolled page.
        let page = v.walk().find(|&n| {
            v.node(n)
                .children
                .iter()
                .any(|&k| is_section(v, k) && section_title(v, k) == "Gui Options")
        });
        if let Some(page) = page {
            let mut kids = v.node(page).children.clone();
            kids.sort_by_key(|&k| v.node(k).ctrl.position[1]);
            let mut y = 0;
            for k in kids {
                if !v.node(k).state.visible {
                    continue;
                }
                let bottom = bottoms.get(&k).copied().unwrap_or(v.node(k).ctrl.extent[1]);
                let c = &mut v.nodes[k].ctrl;
                c.position[1] = y;
                c.extent[1] = bottom + 6;
                y += c.extent[1] + 3;
            }
            v.nodes[page].ctrl.extent[1] = y;
        }
        // Tabs close ranks without Network.
        let mut tabs: Vec<NodeId> = v
            .walk()
            .filter(|&n| {
                v.node(n).state.visible
                    && v.node(n)
                        .ctrl
                        .command
                        .as_deref()
                        .is_some_and(|c| c.starts_with("optionsDlg.setPane("))
            })
            .collect();
        tabs.sort_by_key(|&n| v.node(n).ctrl.position[0]);
        for (i, n) in tabs.into_iter().enumerate() {
            v.nodes[n].ctrl.position[0] = 12 + 90 * i as i32;
        }
    }
    fn menu(&mut self, name: &str, items: Vec<(String, i64)>, selected: i64) {
        if let Some(n) = self.view.id(name) {
            self.view.state(n).items = items;
            self.view.select(n, Some(selected));
        }
    }
    fn slider(&mut self, name: &str, value: f32) {
        if let Some(n) = self.view.id(name) {
            self.view.set_num(n, value);
        }
    }
    fn set_chat_size(&mut self, size: i64) {
        self.draft.set(CHAT_SIZE, size.to_string());
        if let Some(n) = self.view.id(&format!("{CHAT_SIZE_RADIO}{size}")) {
            self.view.select_radio(n);
        }
        if let Some(n) = self.view.id("ExampleChat") {
            self.view.nodes[n].ctrl.style = format!("HUDChatTextEditSize{size}Profile");
        }
    }
    fn pane(&mut self, name: &str) {
        for p in ["Graphics", "Audio", "Controls", "AdvGraphics"] {
            if let Some(n) = self.view.id(&format!("Opt{p}Pane")) {
                self.view.set_visible(n, p == name);
            }
        }
        self.view.close_popup();
        self.view.focus = None;
    }
    fn smart_toggle(&mut self) {
        if let Some(n) = self.view.id("Opt_SSSmartToggle") {
            self.view.set_visible(
                n,
                self.draft
                    .bool_or("$pref::Input::UseSuperShiftToggle", false),
            );
        }
    }
    fn refresh_binds(&mut self, core: &Core) {
        if let Some(n) = self.view.id("OptRemapList") {
            let mut rows = Vec::new();
            for (i, r) in core.pack.data.data.remap.iter().enumerate() {
                if let Some(division) = &r.division {
                    rows.push((format!("   {division}"), -(i as i64) - 1));
                }
                rows.push((
                    format!("{}\t{}", r.name, core.binds.display(&r.command)),
                    i as i64,
                ));
            }
            if self.view.node(n).state.items != rows {
                self.view.state(n).items = rows;
            }
        }
    }
    fn collect(&mut self) -> Result<(), String> {
        for n in self.view.walk().collect::<Vec<_>>() {
            let c = &self.view.node(n).ctrl;
            if self.view.node(n).state.visible
                && c.class == "GuiCheckBoxCtrl"
                && let Some(var) = c.variable.clone()
            {
                self.draft.set_bool(&var, self.view.bool_value(n));
            }
        }
        for (name, pref, lo, hi) in [
            (
                "SliderControlsMouseSensitivity",
                "$pref::Input::MouseSensitivity",
                0.02,
                2.0,
            ),
            ("slider_KeyboardTurnSpeed", KEYBOARD_TURN_SPEED, 0.02, 1.0),
        ] {
            if let Some(n) = self.view.id(name) {
                let v = self.view.num(n);
                if !v.is_finite() {
                    return Err("Sliders must hold finite numbers.".into());
                }
                self.draft.set(pref, v.clamp(lo, hi).to_string());
            }
        }
        for &(name, pref, _) in VOLUMES {
            if let Some(n) = self.view.id(name) {
                let v = self.view.num(n);
                if !v.is_finite() {
                    return Err("Volume must be a finite number.".into());
                }
                self.draft.set(pref, v.clamp(0.0, 1.0).to_string());
            }
        }
        for (name, label, pref, min, max) in [
            (
                "Opt_ChatLineTime",
                "Chat Line Time",
                "$Pref::Chat::LineTime",
                0,
                30000,
            ),
            (
                "Opt_MaxChatLines",
                "Max Chat Lines",
                "$Pref::Chat::MaxDisplayLines",
                4,
                100,
            ),
        ] {
            if let Some(n) = self.view.id(name) {
                let v = self
                    .view
                    .edit_text(n)
                    .trim()
                    .parse::<i64>()
                    .map_err(|_| format!("{label} must be a whole number."))?
                    .clamp(min, max);
                self.draft.set(pref, v.to_string());
                self.view.set_text(n, v.to_string());
            }
        }
        Ok(())
    }
    fn selected_display(&self) -> DisplaySettings {
        let mut d = display(
            &self.draft,
            (
                self.applied_display.resolution.0 as i32,
                self.applied_display.resolution.1 as i32,
            ),
        );
        d.resolution = self
            .view
            .id("OptGraphicsResolutionMenu")
            .and_then(|n| self.view.selected(n))
            .and_then(|i| usize::try_from(i).ok())
            .and_then(|i| self.resolutions.get(i))
            .copied()
            .unwrap_or(self.applied_display.resolution);
        d
    }
    /// Apply (`close` = false) or Done/Escape (`close` = true). A changed
    /// display mode is requested first; the dialog commits once it lands.
    fn apply(&mut self, core: &mut Core, close: bool) {
        if self.pending_display.is_some() {
            return;
        }
        if let Err(e) = self.collect() {
            core.message_ok("Options", &e);
            return;
        }
        let d = self.selected_display();
        if d != self.applied_display {
            let id = core.request_pending(
                UiAction::ApplyDisplay {
                    resolution: d.resolution,
                    fullscreen: d.fullscreen,
                    vsync: d.vsync,
                },
                Pending::Other,
            );
            self.pending_display = Some((id, d, close));
            self.pending_enabled = self
                .view
                .walk()
                .filter(|&n| {
                    self.view.node(n).state.active
                        && (is_value(&self.view, n)
                            || matches!(
                                self.view.node(n).ctrl.class.as_str(),
                                "GuiButtonCtrl" | "GuiBitmapButtonCtrl"
                            ))
                })
                .collect();
            for &n in &self.pending_enabled {
                self.view.set_active(n, false);
            }
            self.view.focus = None;
            self.view.close_popup();
        } else if close {
            self.commit(core);
        }
    }
    fn commit(&mut self, core: &mut Core) {
        // Copy only supported edited values. Preserve concurrent avatar/favorite
        // changes made by other dialogs and preferences owned by the host.
        for (key, value) in self.draft.overrides() {
            if self.initial.get(&key) != Some(value.as_str()) {
                core.prefs.set(&key, value);
            }
        }
        put_display(&mut core.prefs, self.applied_display);
        core.hud.prefs = core.hud_prefs();
        core.selector.queue_brick_buying =
            core.prefs.bool_or("$pref::Input::QueueBrickBuying", true);
        core.chat.max_lines = core
            .prefs
            .i64_or("$Pref::Chat::MaxDisplayLines", 8)
            .clamp(4, 100) as usize;
        core.chat.line_time_ms = core
            .prefs
            .i64_or("$Pref::Chat::LineTime", 6500)
            .clamp(0, 30000);
        for &(_, pref, channel) in VOLUMES {
            core.request(UiAction::SetVolume {
                channel: channel.into(),
                value: core.prefs.f32_or(pref, 1.0).clamp(0.0, 1.0),
            });
        }
        self.committed = true;
        core.save_settings();
        core.pop(ScreenId::Options);
    }
    fn begin_remap(&mut self, core: &mut Core, all: bool) {
        let index = if all {
            Some(0)
        } else {
            self.view
                .id("OptRemapList")
                .and_then(|n| self.view.selected(n))
                .and_then(|i| usize::try_from(i).ok())
        };
        if let Some(i) = index.filter(|i| *i < core.remap_commands.len()) {
            core.remap_target = Some(i);
            core.remap_all = all;
            core.push(ScreenId::Remap);
        }
    }
}
impl Screen for Options {
    fn id(&self) -> ScreenId {
        ScreenId::Options
    }
    fn view(&self) -> &View {
        &self.view
    }
    fn view_mut(&mut self) -> &mut View {
        &mut self.view
    }
    fn blocks_accelerators(&self) -> bool {
        true
    }
    fn on_wake(&mut self, core: &mut Core) {
        core.options_open = true;
    }
    fn on_sleep(&mut self, core: &mut Core) {
        if !self.committed {
            core.binds = self.saved_binds.clone();
            core.settings.mouse_type = self.saved_hardware.0;
            core.settings.keyboard_type = self.saved_hardware.1;
        }
        core.options_open = false;
        core.remap_all = false;
        core.remap_target = None;
    }
    fn tick(&mut self, _dt: u64, core: &mut Core) {
        self.refresh_binds(core);
    }
    fn on_update(&mut self, core: &mut Core) {
        self.refresh_binds(core);
    }
    fn on_event(&mut self, ev: &ViewEvent, core: &mut Core) {
        if self.pending_display.is_some() || !self.view.node(ev.node).state.active {
            return;
        }
        if ev.kind == EventKind::Close {
            self.apply(core, true);
            return;
        }
        if ev.kind == EventKind::Changed {
            if let Some(var) = self
                .view
                .node(ev.node)
                .ctrl
                .variable
                .clone()
                .filter(|v| v.starts_with('$'))
            {
                self.draft.set_bool(&var, self.view.bool_value(ev.node));
                self.smart_toggle();
            }
            return;
        }
        if !matches!(ev.kind, EventKind::Click | EventKind::Submit) {
            return;
        }
        if self.view.id("OptRemapList") == Some(ev.node) {
            if ev.kind == EventKind::Submit {
                self.begin_remap(core, false);
            }
            return;
        }
        let cmd = command_of(&self.view, ev.node);
        if let Some(pane) = cmd
            .strip_prefix("optionsDlg.setPane(")
            .and_then(|c| c.strip_suffix(");"))
        {
            self.pane(pane);
            return;
        }
        if let Some(size) = cmd
            .strip_prefix("OPT_SetChatSize(")
            .and_then(|c| c.strip_suffix(");"))
            .and_then(|c| c.parse().ok())
        {
            self.set_chat_size(size);
            return;
        }
        match cmd.as_str() {
            "Canvas.popDialog(optionsDlg);" => self.apply(core, true),
            "optionsDlg.applyGraphics();" => self.apply(core, false),
            "optionsDlg.RemapAll();" => self.begin_remap(core, true),
            "optionsDlg.clearAllBinds();" => core.message_yes_no(
                "Clear All Binds?",
                "Are you sure you want to clear your control configuration?",
                Callback::ClearBinds,
            ),
            "canvas.pushDialog(DefaultControlsGui);" => core.push(ScreenId::DefaultControls),
            "Canvas.pushDialog(AvatarGui);" => {
                // Remaps are live, as in v20; Player Appearance saves settings
                // with the current controls, so they become the baseline.
                self.saved_binds = core.binds.clone();
                self.saved_hardware = (core.settings.mouse_type, core.settings.keyboard_type);
                core.push(ScreenId::Avatar);
            }
            _ => {}
        }
    }
    fn on_result(
        &mut self,
        id: RequestId,
        _kind: Option<&Pending>,
        result: &Result<(), String>,
        core: &mut Core,
    ) -> bool {
        let Some((pending, d, close)) = self.pending_display else {
            return false;
        };
        if pending != id {
            return false;
        }
        self.pending_display = None;
        for n in self.pending_enabled.drain(..) {
            self.view.set_active(n, true);
        }
        match result {
            Ok(()) => {
                self.applied_display = d;
                put_display(&mut self.draft, d);
                put_display(&mut core.prefs, d);
                if close {
                    self.commit(core);
                } else {
                    // Applying display is its own committed boundary.
                    let mut settings = core.settings.clone();
                    let mut prefs = core.prefs.clone();
                    put_display(&mut prefs, d);
                    settings.prefs = prefs.overrides();
                    settings.binds = Some(self.saved_binds.entries.clone());
                    settings.mouse_type = self.saved_hardware.0;
                    settings.keyboard_type = self.saved_hardware.1;
                    core.request(UiAction::SaveSettings(Box::new(settings)));
                }
            }
            Err(reason) => core.message_ok("Display Settings", reason),
        }
        true
    }
}

pub struct Remap {
    view: View,
    index: Option<usize>,
    conflict: Option<BindInput>,
}
impl Remap {
    pub fn new(core: &Core) -> Self {
        let mut s = Self {
            view: layout_view(core, "RemapDlg"),
            index: core.remap_target,
            conflict: None,
        };
        // Keep the authored skin and footer, but allow enough room for native
        // conflict instructions without clipping or an ambiguous hidden answer.
        if let Some(n) = window(&s.view) {
            s.view.nodes[n].ctrl.position = [120, 165];
            s.view.nodes[n].ctrl.extent = [400, 150];
        }
        for n in s.view.walk().collect::<Vec<_>>() {
            match s.view.text_of(n).as_str() {
                "Escape to cancel" => s.view.nodes[n].ctrl.position = [8, 130],
                "Backspace to clear" => s.view.nodes[n].ctrl.position = [298, 130],
                _ => {}
            }
        }
        if let Some(n) = s.view.id("OptRemapText") {
            s.view.nodes[n].ctrl.position = [10, 28];
            s.view.nodes[n].ctrl.extent = [380, 96];
            s.view.nodes[n].ctrl.class = "GuiMLTextCtrl".into();
        }
        s.prompt(core);
        s
    }
    fn set_text(&mut self, t: String) {
        if let Some(n) = self.view.id("OptRemapText") {
            self.view.set_text(n, t);
        }
    }
    fn prompt(&mut self, core: &Core) {
        let name = self
            .index
            .and_then(|i| core.pack.data.data.remap.get(i))
            .map(|r| r.name.as_str())
            .unwrap_or("No control selected");
        self.set_text(format!("REMAP \"{name}\""));
    }
    fn next(&mut self, core: &mut Core) {
        if core.remap_all
            && let Some(i) = self
                .index
                .and_then(|i| i.checked_add(1))
                .filter(|i| *i < core.remap_commands.len())
        {
            self.index = Some(i);
            core.remap_target = Some(i);
            self.conflict = None;
            self.prompt(core);
            return;
        }
        self.cancel(core);
    }
    fn cancel(&mut self, core: &mut Core) {
        core.remap_target = None;
        core.remap_all = false;
        core.pop(ScreenId::Remap);
    }
    fn capture(&mut self, input: BindInput, core: &mut Core) -> bool {
        if let BindInput::Key(c) = input {
            if c.key == Key::Escape {
                self.cancel(core);
                return true;
            }
            if self.conflict.is_some() {
                match c.key {
                    Key::Return | Key::Letter('y') => {
                        if let Some(command) =
                            self.index.and_then(|i| core.remap_commands.get(i)).cloned()
                        {
                            core.binds
                                .force_remap(&command, self.conflict.take().unwrap());
                            self.next(core);
                        }
                    }
                    Key::Letter('n') => {
                        self.conflict = None;
                        self.prompt(core);
                    }
                    _ => {}
                }
                return true;
            }
            if c.key == Key::Backspace {
                if let Some(command) = self.index.and_then(|i| core.remap_commands.get(i)) {
                    core.binds.unbind_command(command);
                }
                self.next(core);
                return true;
            }
        }
        if self.conflict.is_some() {
            return true;
        }
        let Some(command) = self.index.and_then(|i| core.remap_commands.get(i)).cloned() else {
            self.cancel(core);
            return true;
        };
        let reserved = match input {
            BindInput::Key(c) => core.globals.command_for_key(c.key, c.mods).is_some(),
            _ => core.globals.command_for(&input).is_some(),
        };
        if reserved {
            self.set_text(format!(
                "{} is reserved. Choose another input, or Esc.",
                input.label()
            ));
            return true;
        }
        match core.binds.remap(&command, input, &core.remap_commands) {
            RemapOutcome::Bound => {
                core.binds.force_remap(&command, input);
                self.next(core);
            }
            RemapOutcome::Conflict { other } => {
                let name = core
                    .pack
                    .data
                    .data
                    .remap
                    .iter()
                    .find(|r| r.command.eq_ignore_ascii_case(&other))
                    .map_or(other.as_str(), |r| r.name.as_str());
                self.conflict = Some(input);
                self.set_text(format!(
                    "{} is bound to {name}. Replace? Enter/Y = yes; N = no; Esc = cancel.",
                    input.label()
                ));
            }
            RemapOutcome::NotRemappable { .. } => self.set_text(format!(
                "{} is reserved. Choose another input, or Esc.",
                input.label()
            )),
        }
        true
    }
}
impl Screen for Remap {
    fn id(&self) -> ScreenId {
        ScreenId::Remap
    }
    fn view(&self) -> &View {
        &self.view
    }
    fn view_mut(&mut self) -> &mut View {
        &mut self.view
    }
    fn captures_keyboard(&self) -> bool {
        true
    }
    fn blocks_accelerators(&self) -> bool {
        true
    }
    fn on_bind_input(&mut self, input: BindInput, core: &mut Core) -> bool {
        self.capture(input, core)
    }
    fn on_key(&mut self, key: Key, mods: Modifiers, core: &mut Core) -> bool {
        self.capture(
            BindInput::Key(if key.is_modifier() {
                Chord::plain(key)
            } else {
                Chord { key, mods }
            }),
            core,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{Settings, UiUpdate};
    use crate::binds::Platform;
    use crate::input::{InputEvent, MouseButton};
    use crate::schema::{RemapEntry, UiPack};
    use crate::ui::{Ui, UiConfig};
    use std::rc::Rc;

    fn fixture() -> Ui {
        let mut data = UiPack::default();
        let mut layout = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480));
        for (class, name, var, command) in [
            ("GuiCheckBoxCtrl", "duplicate", FULLSCREEN, ""),
            ("GuiCheckBoxCtrl", "duplicate", NO_VSYNC, ""),
            (
                "GuiCheckBoxCtrl",
                "audioDuplicate",
                "$Pref::Audio::PlayMusic",
                "",
            ),
            (
                "GuiCheckBoxCtrl",
                "audioDuplicate",
                "$Pref::Audio::MenuSounds",
                "",
            ),
            (
                "GuiCheckBoxCtrl",
                "audioDuplicate",
                "$Pref::Audio::PlayBrickPlantSound",
                "",
            ),
            (
                "GuiCheckBoxCtrl",
                "audioDuplicate",
                "$Pref::Audio::PlayBrickMoveSound",
                "",
            ),
            (
                "GuiCheckBoxCtrl",
                "audioDuplicate",
                "$Pref::Audio::PlantErrorSound",
                "",
            ),
            (
                "GuiCheckBoxCtrl",
                "duplicate",
                "$pref::HUD::HideBrickBox",
                "",
            ),
            (
                "GuiCheckBoxCtrl",
                "unsupported",
                "$pref::OpenGL::doAnimatedLights",
                "",
            ),
            ("GuiPopUpMenuCtrl", "OptGraphicsResolutionMenu", "", ""),
            (
                "GuiSliderCtrl",
                "SliderControlsMouseSensitivity",
                "value",
                "",
            ),
            ("GuiSliderCtrl", "OptAudioVolumeMaster", "value", ""),
            ("GuiSliderCtrl", "OptAudioVolumeShell", "value", ""),
            ("GuiSliderCtrl", "OptAudioVolumeSim", "value", ""),
            ("GuiTextEditCtrl", "Opt_ChatLineTime", "", ""),
            ("GuiTextEditCtrl", "Opt_MaxChatLines", "", ""),
            ("GuiTextListCtrl", "OptRemapList", "", ""),
            ("GuiButtonCtrl", "apply", "", "optionsDlg.applyGraphics();"),
            ("GuiButtonCtrl", "done", "", "Canvas.popDialog(optionsDlg);"),
            ("GuiButtonCtrl", "clear", "", "optionsDlg.clearAllBinds();"),
            ("GuiRadioCtrl", "OPT_ChatSize2", "", "OPT_SetChatSize(2);"),
            ("GuiRadioCtrl", "OPT_ChatSize4", "", "OPT_SetChatSize(4);"),
        ] {
            let mut c = ctrl(class, "GuiDefaultProfile", Rect::new(0, 0, 100, 20));
            c.name = Some(name.into());
            if !var.is_empty() {
                c.variable = Some(var.into());
            }
            if !command.is_empty() {
                c.command = Some(command.into());
            }
            if name == "done" {
                c.accelerator = Some("escape".into());
            }
            layout.children.push(c);
        }
        data.layouts.insert("optionsDlg".into(), layout);
        data.data.remap = vec![
            RemapEntry {
                division: Some("Movement".into()),
                name: "Forward".into(),
                command: "moveforward".into(),
            },
            RemapEntry {
                division: None,
                name: "Backward".into(),
                command: "movebackward".into(),
            },
        ];
        let mut ui = Ui::new(
            Rc::new(Pack::from_parts(data, Default::default())),
            UiConfig {
                size: (640, 480),
                scale: Some(1.0),
                platform: Platform::Windows,
            },
            Settings {
                binds: Some(vec![]),
                ..Default::default()
            },
        );
        ui.core.binds.bind(
            BindInput::Key(Chord::plain(Key::Letter('w'))),
            "moveforward",
        );
        ui.core.binds.bind(
            BindInput::Key(Chord::plain(Key::Letter('s'))),
            "movebackward",
        );
        ui.core.remap_commands = vec!["moveforward".into(), "movebackward".into()];
        ui.core.prefs.set(RESOLUTION, "640 480 32");
        ui.drain_actions();
        ui
    }
    fn click(s: &mut Options, name: &str, ui: &mut Ui) {
        let node = s.view.id(name).unwrap();
        s.on_event(
            &ViewEvent {
                node,
                kind: EventKind::Click,
            },
            &mut ui.core,
        );
    }
    fn key(key: Key) -> BindInput {
        BindInput::Key(Chord::plain(key))
    }

    const AUDIO_PREFS: [&str; 5] = [
        "$Pref::Audio::PlayMusic",
        "$Pref::Audio::MenuSounds",
        "$Pref::Audio::PlayBrickPlantSound",
        "$Pref::Audio::PlayBrickMoveSound",
        "$Pref::Audio::PlantErrorSound",
    ];
    fn audio_node(options: &Options, pref: &str) -> NodeId {
        options
            .view
            .walk()
            .find(|&n| {
                options
                    .view
                    .node(n)
                    .ctrl
                    .variable
                    .as_deref()
                    .is_some_and(|v| v.eq_ignore_ascii_case(pref))
            })
            .unwrap()
    }
    fn toggle_audio(options: &mut Options, ui: &mut Ui, pref: &str, value: bool) {
        let n = audio_node(options, pref);
        assert!(options.view.node(n).state.active);
        options.view.set_bool(n, value);
        options.on_event(
            &ViewEvent {
                node: n,
                kind: EventKind::Changed,
            },
            &mut ui.core,
        );
    }

    #[test]
    fn audio_preferences_use_seeded_defaults_and_apply_only_on_done() {
        let mut ui = fixture();
        for pref in AUDIO_PREFS {
            ui.core
                .prefs
                .set_bool(pref, pref != "$Pref::Audio::PlantErrorSound");
        }
        let before = ui.core.prefs.clone();
        let mut options = Options::new(&ui.core);
        for pref in AUDIO_PREFS {
            let initial = pref != "$Pref::Audio::PlantErrorSound";
            assert_eq!(options.view.bool_value(audio_node(&options, pref)), initial);
            toggle_audio(&mut options, &mut ui, pref, !initial);
        }
        ui.apply(UiUpdate::PlantError(crate::api::PlantError::Overlap));
        assert!(ui.drain_sounds().is_empty());
        assert!(ui.drain_actions().is_empty());
        assert_eq!(ui.core.prefs, before);
        options.on_sleep(&mut ui.core);
        assert_eq!(ui.core.prefs, before);
        assert!(ui.drain_actions().is_empty());
    }

    #[test]
    fn done_commits_all_five_audio_toggles_once_through_save_settings() {
        let mut ui = fixture();
        for pref in AUDIO_PREFS {
            ui.core.prefs.set_bool(pref, false);
        }
        let mut options = Options::new(&ui.core);
        for pref in AUDIO_PREFS {
            toggle_audio(&mut options, &mut ui, pref, true);
        }
        assert!(
            !options
                .view
                .node(options.view.id("unsupported").unwrap())
                .state
                .visible
        );
        assert!(ui.drain_actions().is_empty());
        click(&mut options, "done", &mut ui);
        let actions = ui.drain_actions();
        assert_eq!(
            actions
                .iter()
                .filter(|(_, a)| matches!(a, UiAction::SetVolume { .. }))
                .count(),
            3
        );
        let saves: Vec<_> = actions
            .iter()
            .filter_map(|(_, a)| {
                if let UiAction::SaveSettings(s) = a {
                    Some(s)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(saves.len(), 1);
        let saved = Prefs::new(&Default::default(), &saves[0].prefs);
        for pref in AUDIO_PREFS {
            assert!(ui.core.prefs.bool_or(pref, false));
            assert!(saved.bool_or(pref, false));
        }
        ui.apply(UiUpdate::PlantError(crate::api::PlantError::Overlap));
        assert_eq!(ui.drain_sounds().len(), 1);
    }

    #[test]
    fn closing_without_done_discards_draft_audio_prefs_and_remaps() {
        let mut ui = fixture();
        let before = ui.core.binds.clone();
        let mut s = Options::new(&ui.core);
        s.on_wake(&mut ui.core);
        assert!(ui.core.options_open);
        let n = s.view.id("OptAudioVolumeMaster").unwrap();
        s.view.set_num(n, 0.25);
        ui.core.binds.force_remap("moveforward", key(Key::Up));
        s.on_sleep(&mut ui.core);
        assert_eq!(ui.core.binds, before);
        assert!(!ui.core.options_open);
        assert!(ui.drain_actions().is_empty());
        assert_eq!(ui.core.prefs.get("$pref::Audio::masterVolume"), None);
    }
    #[test]
    fn done_commits_supported_values_and_emits_typed_audio() {
        let mut ui = fixture();
        let mut s = Options::new(&ui.core);
        let n = s.view.id("OptAudioVolumeMaster").unwrap();
        s.view.set_num(n, 0.25);
        let n = s.view.id("Opt_MaxChatLines").unwrap();
        s.view.set_text(n, "900");
        // Three controls intentionally share one authored name.
        let n = s
            .view
            .walk()
            .find(|&n| s.view.node(n).ctrl.variable.as_deref() == Some("$pref::HUD::HideBrickBox"))
            .unwrap();
        s.view.set_bool(n, true);
        assert!(!s.view.node(s.view.id("unsupported").unwrap()).state.visible);
        click(&mut s, "done", &mut ui);
        assert_eq!(ui.core.chat.max_lines, 100);
        assert!(ui.core.hud.prefs.hide_brick_box);
        let actions = ui.drain_actions();
        assert!(actions.iter().any(|(_,a)|matches!(a,UiAction::SetVolume{channel,value} if channel=="master" && *value==0.25)));
        assert!(
            actions
                .iter()
                .any(|(_, a)| matches!(a, UiAction::SaveSettings(_)))
        );
        assert!(
            !actions
                .iter()
                .any(|(_, a)| matches!(a, UiAction::ApplyDisplay { .. }))
        );
    }
    #[test]
    fn escape_is_done_as_in_v20() {
        let mut ui = fixture();
        ui.core.push(ScreenId::Options);
        ui.apply(UiUpdate::Maps(vec![]));
        assert_eq!(ui.top_id(), ScreenId::Options);
        ui.core.binds.force_remap("moveforward", key(Key::Up));
        ui.handle_input(InputEvent::KeyDown {
            key: Key::Escape,
            mods: Modifiers::NONE,
            repeat: false,
        });
        assert!(!ui.is_open(ScreenId::Options));
        assert_eq!(ui.core.binds.binding_of("moveforward"), Some(key(Key::Up)));
        assert!(
            ui.drain_actions()
                .iter()
                .any(|(_, a)| matches!(a, UiAction::SaveSettings(_)))
        );
    }
    #[test]
    fn clear_all_confirms_then_unbinds_only_remappable_controls() {
        let mut ui = fixture();
        ui.core.binds.bind(key(Key::F(9)), "toggleConsole");
        let mut s = Options::new(&ui.core);
        click(&mut s, "clear", &mut ui);
        ui.apply(UiUpdate::Maps(vec![]));
        assert_eq!(ui.top_id(), ScreenId::MessageBox);
        assert!(ui.core.binds.binding_of("moveforward").is_some());
        ui.handle_input(InputEvent::KeyDown {
            key: Key::Return,
            mods: Modifiers::NONE,
            repeat: false,
        });
        assert!(!ui.is_open(ScreenId::MessageBox));
        assert_eq!(ui.core.binds.binding_of("moveforward"), None);
        assert_eq!(ui.core.binds.binding_of("movebackward"), None);
        assert_eq!(
            ui.core.binds.binding_of("toggleConsole"),
            Some(key(Key::F(9)))
        );
    }
    #[test]
    fn chat_size_radio_restyles_example_and_saves_on_done() {
        let mut ui = fixture();
        let mut s = Options::new(&ui.core);
        let four = s.view.id("OPT_ChatSize4").unwrap();
        assert!(s.view.bool_value(four), "v20 default chat size is 4");
        click(&mut s, "OPT_ChatSize2", &mut ui);
        assert!(s.view.bool_value(s.view.id("OPT_ChatSize2").unwrap()));
        assert_eq!(ui.core.prefs.get(CHAT_SIZE), None);
        click(&mut s, "done", &mut ui);
        assert_eq!(chat_size(&ui.core.prefs), 2);
    }
    #[test]
    fn display_rejection_does_not_commit_then_success_retains_applied_boundary() {
        let mut ui = fixture();
        let mut s = Options::new(&ui.core);
        let n = s.view.id("OptGraphicsResolutionMenu").unwrap();
        let i = s
            .resolutions
            .iter()
            .position(|r| *r == (1280, 720))
            .unwrap();
        s.view.select(n, Some(i as i64));
        click(&mut s, "apply", &mut ui);
        let actions = ui.drain_actions();
        let (id, a) = actions.first().unwrap();
        assert!(matches!(
            a,
            UiAction::ApplyDisplay {
                resolution: (1280, 720),
                ..
            }
        ));
        assert_eq!(ui.core.prefs.get(RESOLUTION), Some("640 480 32"));
        assert!(s.on_result(*id, None, &Err("unsupported mode".into()), &mut ui.core));
        assert!(ui.drain_actions().is_empty());
        assert_eq!(ui.core.prefs.get(RESOLUTION), Some("640 480 32"));
        click(&mut s, "apply", &mut ui);
        let id = ui.drain_actions()[0].0;
        assert!(s.on_result(id, None, &Ok(()), &mut ui.core));
        assert_eq!(ui.core.prefs.get(RESOLUTION), Some("1280 720 32"));
        s.on_sleep(&mut ui.core);
        assert_eq!(ui.core.prefs.get(RESOLUTION), Some("1280 720 32"));
    }
    #[test]
    fn invalid_numbers_do_not_emit_settings_and_do_not_overwrite_concurrent_prefs() {
        let mut ui = fixture();
        ui.core.prefs.set("$pref::Avatar::Hat", "1");
        let mut s = Options::new(&ui.core);
        ui.core.prefs.set("$pref::Avatar::Hat", "3");
        let n = s.view.id("Opt_ChatLineTime").unwrap();
        s.view.set_text(n, "not a number");
        click(&mut s, "done", &mut ui);
        assert!(ui.drain_actions().is_empty());
        s.view.set_text(n, "6500");
        click(&mut s, "done", &mut ui);
        assert_eq!(ui.core.prefs.get("$pref::Avatar::Hat"), Some("3"));
    }
    #[test]
    fn remap_conflict_decline_confirm_clear_and_reserved_inputs() {
        let mut ui = fixture();
        ui.core.remap_target = Some(0);
        let mut r = Remap::new(&ui.core);
        r.capture(key(Key::Letter('s')), &mut ui.core);
        assert!(r.conflict.is_some());
        assert_eq!(
            ui.core.binds.binding_of("moveforward"),
            Some(key(Key::Letter('w')))
        );
        r.capture(key(Key::Letter('n')), &mut ui.core);
        assert!(r.conflict.is_none());
        r.capture(key(Key::Letter('s')), &mut ui.core);
        r.capture(key(Key::Return), &mut ui.core);
        assert_eq!(
            ui.core.binds.binding_of("moveforward"),
            Some(key(Key::Letter('s')))
        );
        assert_eq!(ui.core.binds.binding_of("movebackward"), None);
        ui.core.remap_target = Some(0);
        let mut r = Remap::new(&ui.core);
        ui.core.globals.bind(key(Key::F(1)), "help");
        r.capture(key(Key::F(1)), &mut ui.core);
        assert_eq!(
            ui.core.binds.binding_of("moveforward"),
            Some(key(Key::Letter('s')))
        );
        r.capture(key(Key::Backspace), &mut ui.core);
        assert_eq!(ui.core.binds.binding_of("moveforward"), None);
    }
    #[test]
    fn manager_captures_mouse_wheel_and_reserved_keys_before_globals() {
        let mut ui = fixture();
        ui.core.remap_target = Some(0);
        ui.core.remap_all = true;
        ui.core.push(ScreenId::Remap);
        // A model update flushes the stack; no desktop input or window exists.
        ui.apply(UiUpdate::Maps(vec![]));
        ui.core.globals.bind(key(Key::F(1)), "toggleConsole");
        ui.handle_input(InputEvent::KeyDown {
            key: Key::F(1),
            mods: Modifiers::NONE,
            repeat: false,
        });
        assert!(ui.drain_actions().is_empty());
        ui.handle_input(InputEvent::MouseDown {
            button: MouseButton::Right,
            x: 200.0,
            y: 200.0,
        });
        assert_eq!(
            ui.core.binds.binding_of("moveforward"),
            Some(BindInput::Mouse(MouseButton::Right))
        );
        assert_eq!(ui.top_id(), ScreenId::Remap);
        ui.handle_input(InputEvent::Wheel { delta: 1.0 });
        assert_eq!(
            ui.core.binds.binding_of("movebackward"),
            Some(BindInput::Wheel)
        );
        assert!(!ui.is_open(ScreenId::Remap));
    }

    #[test]
    #[cfg(feature = "gpu")]
    #[ignore = "bounded headless rendering requires local converted original content and GPU"]
    fn authored_options_save_players_offscreen() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let pack = Rc::new(Pack::load(&root.join("content/ui-pack-001")).unwrap());
        let mut ui = fixture();
        ui.core.pack = pack.clone();
        ui.core.remap_commands = pack
            .data
            .data
            .remap
            .iter()
            .map(|r| r.command.clone())
            .collect();
        ui.core.prefs = Prefs::new(&pack.data.data.prefs, &Default::default());
        ui.core.save_context = Some(("Bedroom".into(), crate::api::IconRef::None));
        ui.core.save_maps = vec!["Bedroom".into()];
        ui.core.save_files = vec![crate::api::SaveFileInfo {
            name: "Test.world.json".into(),
            map: "Bedroom".into(),
            modified: "2026-09-26".into(),
            description: "Native save test".into(),
            brick_count: Some(42),
        }];
        let output = root.join("artifacts/ui-native-dialogs");
        std::fs::create_dir_all(&output).unwrap();
        let gpu = crate::gpu::Headless::new().unwrap();
        let mut renderer = crate::gpu::UiRenderer::new(&gpu.device, &gpu.queue);
        let mut render = |name: &str, screen: &mut dyn Screen, core: &mut Core| {
            screen.layout(640, 480, core);
            let mut dl = DrawList::new(Rect::new(0, 0, 640, 480));
            screen.draw(&pack, &mut dl, core);
            assert!(dl.glyph_count() > 20, "{name}");
            let rgba = gpu
                .render_rgba(
                    &mut renderer,
                    &pack,
                    &dl,
                    (640, 480),
                    1.0,
                    [0.12, 0.12, 0.15, 1.0],
                )
                .unwrap();
            image::save_buffer(
                output.join(format!("{name}.png")),
                &rgba,
                640,
                480,
                image::ColorType::Rgba8,
            )
            .unwrap();
        };
        let mut options = Options::new(&ui.core);
        for pane in ["Graphics", "Controls", "Audio", "Network", "AdvGraphics"] {
            options.pane(pane);
            render(&format!("options-{pane}"), &mut options, &mut ui.core);
        }
        ui.core.remap_target = Some(0);
        let mut remap = Remap::new(&ui.core);
        remap.capture(key(Key::Letter('s')), &mut ui.core);
        render("remap-conflict", &mut remap, &mut ui.core);
        for id in [ScreenId::SaveBricks, ScreenId::LoadBricks] {
            let mut screen = crate::screens::saveload::SaveLoad::new(id, &ui.core);
            render(&format!("{id:?}"), &mut screen, &mut ui.core);
        }
        let mut players = crate::screens::players::Players::new(&ui.core);
        render("players", &mut players, &mut ui.core);
    }
}
