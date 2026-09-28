//! Game Mode: Start Game's choice of what a hosted game runs. Custom is the
//! base game on any map (and a package world with the Add-Ons that fit it);
//! every other entry is a mode an enabled Add-On declares, naming the
//! Add-Ons that run and, often, its map. Like v21's gamemode list, the modes
//! are data from the host; this screen only shows and stores the choice.
//! Design: `docs/architecture/mod-manager.md`.
use super::*;
use crate::api::GameModeInfo;
use crate::view::EventKind;

/// The chosen mode's content id; empty for Custom.
pub const GAME_MODE: &str = "$Pref::Server::GameMode";
const LIST: &str = "GM_List";
const DETAILS: &str = "GM_Details";
const DETAIL_SCROLL: &str = "GM_DetailScroll";
const SELECT: &str = "GM_Select";
const CANCEL: &str = "GM_Cancel";

fn named(mut c: Control, name: &str) -> Control {
    c.name = Some(name.into());
    c
}

/// The chosen mode, if it is still offered; None is Custom.
pub fn chosen(core: &Core) -> Option<&GameModeInfo> {
    let id = core.prefs.str_or(GAME_MODE, "");
    core.game_modes.iter().find(|m| m.id == id)
}

/// What a mode says about itself, for the details panel.
fn details(mode: Option<&GameModeInfo>, core: &Core) -> String {
    let Some(m) = mode else {
        let hint = if core.game_modes.is_empty() {
            "\n\nAdd-Ons can add game modes. Turn them on in Add-Ons on the main menu."
        } else {
            ""
        };
        return format!(
            "<font:Impact:18>Custom\n<font:Arial:14>\nPlay any map you like with the base game. \
             Picking a world an Add-On brings runs the Add-Ons made for that world.{hint}"
        );
    };
    let map = match &m.map {
        Some(id) => core
            .maps
            .iter()
            .find(|map| map.id == *id)
            .map_or_else(|| id.clone(), |map| map.name.clone()),
        None => "any map you pick".into(),
    };
    let mut out = format!("<font:Impact:18>{}\n<font:Arial:14>", m.name);
    if !m.description.is_empty() {
        out.push_str(&format!("\n{}\n", m.description));
    }
    out.push_str(&format!("\nPlays on: {map}"));
    out
}

pub struct GameModes {
    view: View,
    /// Mode id per list item; None is Custom.
    ids: Vec<Option<String>>,
}

impl GameModes {
    pub fn new(core: &Core) -> Self {
        let mut root = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480));
        let (w, h) = (440, 320);
        let mut win = ctrl(
            "GuiWindowCtrl",
            "BlockWindowProfile",
            Rect::new((640 - w) / 2, (480 - h) / 2, w, h),
        );
        win.text = Some("Game Mode".into());
        win.h_sizing = HSizing::Center;
        win.v_sizing = VSizing::Center;
        let scroll = |name: &str, r: Rect| {
            let mut c = named(ctrl("GuiScrollCtrl", "BlockScrollProfile", r), name);
            c.fields.insert("hScrollBar".into(), "alwaysOff".into());
            c.fields.insert("vScrollBar".into(), "dynamic".into());
            c
        };
        let mut list_scroll = scroll("GM_Scroll", Rect::new(12, 34, 170, 236));
        list_scroll.children.push(named(
            ctrl(
                "GuiTextListCtrl",
                "GuiTextListProfile",
                Rect::new(0, 0, 154, 16),
            ),
            LIST,
        ));
        let mut detail_scroll = scroll(DETAIL_SCROLL, Rect::new(190, 34, 238, 236));
        let mut info = text("GuiMLTextProfile", Rect::new(4, 2, 216, 16), "");
        info.class = "GuiMLTextCtrl".into();
        detail_scroll.children.push(named(info, DETAILS));
        win.children.push(list_scroll);
        win.children.push(detail_scroll);
        for (label, name, x) in [("Cancel", CANCEL, 12), ("Select", SELECT, 330)] {
            win.children.push(named(
                button(
                    "BlockButtonProfile",
                    Rect::new(x, 280, 98, 28),
                    "base/client/ui/button1",
                    label,
                    name,
                ),
                name,
            ));
        }
        root.children.push(win);
        let mut view = View::new(&root);
        view.measure(&core.pack);
        let mut s = Self { view, ids: vec![] };
        s.refresh(core);
        if let Some(n) = s.view.id(LIST) {
            let current = chosen(core).map(|m| m.id.clone());
            let i = s.ids.iter().position(|id| *id == current).unwrap_or(0);
            s.view.select(n, Some(i as i64));
        }
        s.show_details(core);
        s
    }
    fn refresh(&mut self, core: &Core) {
        let selected = self.selected();
        self.ids = std::iter::once(None)
            .chain(core.game_modes.iter().map(|m| Some(m.id.clone())))
            .collect();
        let items = std::iter::once("Custom".to_string())
            .chain(core.game_modes.iter().map(|m| m.name.clone()))
            .enumerate()
            .map(|(i, name)| (name, i as i64))
            .collect();
        if let Some(n) = self.view.id(LIST) {
            self.view.state(n).items = items;
            let i = self.ids.iter().position(|id| *id == selected).unwrap_or(0);
            self.view.select(n, Some(i as i64));
        }
        self.show_details(core);
    }
    fn selected(&self) -> Option<String> {
        let n = self.view.id(LIST)?;
        let i = self.view.selected(n)?;
        self.ids.get(usize::try_from(i).ok()?).cloned().flatten()
    }
    fn show_details(&mut self, core: &Core) {
        let id = self.selected();
        let mode = core.game_modes.iter().find(|m| Some(&m.id) == id.as_ref());
        let text = details(mode, core);
        if let (Some(n), Some(scroll)) = (self.view.id(DETAILS), self.view.id(DETAIL_SCROLL)) {
            let width = self.view.node(n).ctrl.extent[0];
            let h = View::ml_height(&core.pack, "GuiMLTextProfile", &text, width).max(16);
            self.view.nodes[n].ctrl.extent[1] = h;
            self.view.set_text(n, text);
            self.view.scroll_to(scroll, 0);
            self.view.relayout();
        }
    }
    fn choose(&mut self, core: &mut Core) {
        let id = self.selected().unwrap_or_default();
        core.prefs.set(GAME_MODE, id);
        core.save_settings();
        core.pop(self.id());
    }
}

impl Screen for GameModes {
    fn id(&self) -> ScreenId {
        ScreenId::GameModes
    }
    fn view(&self) -> &View {
        &self.view
    }
    fn view_mut(&mut self) -> &mut View {
        &mut self.view
    }
    fn on_update(&mut self, core: &mut Core) {
        self.refresh(core);
    }
    fn on_key(&mut self, key: Key, _mods: Modifiers, core: &mut Core) -> bool {
        match key {
            Key::Escape => core.pop(self.id()),
            Key::Return | Key::NumpadEnter => self.choose(core),
            _ => return false,
        }
        true
    }
    fn on_event(&mut self, ev: &ViewEvent, core: &mut Core) {
        let name = self
            .view
            .node(ev.node)
            .ctrl
            .name
            .clone()
            .unwrap_or_default();
        match (name.as_str(), ev.kind) {
            (_, EventKind::Close) | (CANCEL, EventKind::Click) => core.pop(self.id()),
            (SELECT, EventKind::Click) | (LIST, EventKind::Submit) => self.choose(core),
            (LIST, EventKind::Changed) => self.show_details(core),
            _ => {}
        }
    }
}
