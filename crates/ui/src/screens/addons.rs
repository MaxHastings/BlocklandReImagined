//! Add-Ons: the installed packages, turned on and off in one list, and the
//! join screen that fetches a server's packages. v20 had a flat checklist of
//! `Add-Ons/` folders; ours keeps that feel (one list, one click, Defaults)
//! and adds what packages now say about themselves: what they provide, where
//! they run, what they need and what they may do. Every row is text the host
//! prepared from the package library; this screen never reads package data.
//! Design: `docs/architecture/mod-manager.md`.
use super::*;
use crate::api::{AddOnRow, ConnectionState, DownloadState, UiAction};
use crate::ui::Callback;
use crate::view::EventKind;

const LIST: &str = "AO_List";
const SEARCH: &str = "AO_Search";
const DETAILS: &str = "AO_Details";
const DETAIL_SCROLL: &str = "AO_DetailScroll";
const ENABLED: &str = "AO_Enabled";
const IMPORT: &str = "AO_Import";
const STATUS: &str = "AO_Status";
const DEFAULTS: &str = "AO_Defaults";
const DONE: &str = "AO_Done";
/// List rows that are group headings, not packages.
const HEADING: i64 = -1;

fn named(mut c: Control, name: &str) -> Control {
    c.name = Some(name.into());
    c
}

/// A native dialog window centred on a 640×480 root.
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

pub struct AddOns {
    view: View,
    /// Row index into `core.add_ons.rows` per list item id.
    shown: Vec<usize>,
    /// Package id of the selected row, kept across refreshes.
    selected: Option<String>,
    requests: Vec<RequestId>,
}

impl AddOns {
    pub fn new(core: &Core) -> Self {
        let (mut root, mut win) = dialog("Add-Ons", 600, 440);
        let mut search_label = text("GuiTextProfile", Rect::new(12, 34, 48, 18), "Search:");
        search_label.name = Some("AO_SearchLabel".into());
        win.children.push(search_label);
        win.children.push(named(
            ctrl(
                "GuiTextEditCtrl",
                "BlockTextEditProfile",
                Rect::new(62, 34, 186, 18),
            ),
            SEARCH,
        ));
        let mut list_scroll = scroll("AO_Scroll", Rect::new(12, 58, 236, 316));
        let mut list = named(
            ctrl(
                "GuiTextListCtrl",
                "GuiTextListProfile",
                Rect::new(0, 0, 220, 16),
            ),
            LIST,
        );
        list.fields.insert("columns".into(), "0 34".into());
        list_scroll.children.push(list);
        win.children.push(list_scroll);
        let mut detail_scroll = scroll(DETAIL_SCROLL, Rect::new(256, 58, 332, 290));
        let mut details = text("GuiMLTextProfile", Rect::new(4, 2, 310, 16), "");
        details.class = "GuiMLTextCtrl".into();
        detail_scroll.children.push(named(details, DETAILS));
        win.children.push(detail_scroll);
        let mut enabled = ctrl(
            "GuiCheckBoxCtrl",
            "GuiCheckBoxProfile",
            Rect::new(256, 354, 240, 20),
        );
        enabled.text = Some("Enabled".into());
        win.children.push(named(enabled, ENABLED));
        win.children.push(named(
            button(
                "BlockButtonProfile",
                Rect::new(256, 350, 98, 28),
                "base/client/ui/button1",
                "Import",
                IMPORT,
            ),
            IMPORT,
        ));
        let mut status = text("GuiMLTextProfile", Rect::new(12, 380, 576, 20), "");
        status.class = "GuiMLTextCtrl".into();
        win.children.push(named(status, STATUS));
        win.children.push(named(
            button(
                "BlockButtonProfile",
                Rect::new(12, 404, 98, 28),
                "base/client/ui/button1",
                "Defaults",
                DEFAULTS,
            ),
            DEFAULTS,
        ));
        win.children.push(named(
            button(
                "BlockButtonProfile",
                Rect::new(490, 404, 98, 28),
                "base/client/ui/button1",
                "Done",
                DONE,
            ),
            DONE,
        ));
        root.children.push(win);
        let mut view = View::new(&root);
        view.measure(&core.pack);
        view.focus = view.id(SEARCH);
        let mut s = Self {
            view,
            shown: vec![],
            selected: None,
            requests: vec![],
        };
        s.refresh(core);
        s
    }

    fn row<'a>(&self, core: &'a Core) -> Option<&'a AddOnRow> {
        let id = self.selected.as_deref()?;
        core.add_ons.rows.iter().find(|r| r.id == id)
    }

    /// Rebuild the list from the host's rows: grouped by category in the
    /// host's order, filtered by the search text.
    fn refresh(&mut self, core: &Core) {
        let filter = self
            .view
            .id(SEARCH)
            .map(|n| self.view.edit_text(n).trim().to_ascii_lowercase())
            .unwrap_or_default();
        let rows = &core.add_ons.rows;
        let matches = |r: &AddOnRow| {
            filter.is_empty()
                || [&r.name, &r.id, &r.category, &r.description, &r.authors]
                    .iter()
                    .any(|t| t.to_ascii_lowercase().contains(&filter))
        };
        let mut categories: Vec<&str> = Vec::new();
        for r in rows.iter().filter(|r| matches(r)) {
            if !categories.contains(&r.category.as_str()) {
                categories.push(&r.category);
            }
        }
        self.shown.clear();
        let mut items = Vec::new();
        for category in categories {
            items.push((format!("\t{}", category.to_ascii_uppercase()), HEADING));
            for (i, r) in rows.iter().enumerate() {
                if r.category != category || !matches(r) {
                    continue;
                }
                let mark = if r.importing {
                    "..."
                } else if r.importable {
                    "New"
                } else if r.broken && r.enabled {
                    "!!"
                } else if r.locked {
                    "Base"
                } else if r.enabled {
                    "On"
                } else if r.broken {
                    "!"
                } else {
                    ""
                };
                items.push((
                    format!("{mark}\t{}", r.name.replace(['\t', '\n', '\r'], " ")),
                    self.shown.len() as i64,
                ));
                self.shown.push(i);
            }
        }
        if self
            .selected
            .as_ref()
            .is_some_and(|id| !rows.iter().any(|r| r.id == *id))
        {
            self.selected = None;
        }
        let selected_item = self.selected.as_ref().and_then(|id| {
            self.shown
                .iter()
                .position(|&i| rows[i].id == *id)
                .map(|p| p as i64)
        });
        if let Some(n) = self.view.id(LIST) {
            self.view.state(n).items = items;
            self.view.select(n, selected_item);
        }
        let status = core.add_ons.notice.clone();
        if let Some(n) = self.view.id(STATUS) {
            self.view.set_text(n, status);
        }
        self.show_details(core);
    }

    fn show_details(&mut self, core: &Core) {
        let row = self.row(core).cloned();
        let text = match &row {
            Some(r) => details(r),
            None if core.add_ons.rows.is_empty() => "Looking for installed add-ons...".into(),
            None => "Pick an add-on to see what it does.\n\nTick Enabled, or double-click it, to turn it on or off. Changes apply the next time you start a game.".into(),
        };
        if let (Some(n), Some(scroll)) = (self.view.id(DETAILS), self.view.id(DETAIL_SCROLL)) {
            let width = self.view.node(n).ctrl.extent[0];
            let h = View::ml_height(&core.pack, "GuiMLTextProfile", &text, width).max(16);
            self.view.nodes[n].ctrl.extent[1] = h;
            self.view.set_text(n, text);
            self.view.scroll_to(scroll, 0);
            self.view.relayout();
        }
        if let Some(n) = self.view.id(ENABLED) {
            self.view
                .set_bool(n, row.as_ref().is_some_and(|r| r.enabled));
            self.view
                .set_active(n, row.as_ref().is_some_and(|r| !r.locked));
            self.view
                .set_visible(n, row.as_ref().is_some_and(|r| !r.importable));
        }
        if let Some(n) = self.view.id(IMPORT) {
            self.view
                .set_visible(n, row.as_ref().is_some_and(|r| r.importable));
            self.view
                .set_active(n, row.as_ref().is_some_and(|r| !r.importing));
        }
    }

    fn import(&mut self, core: &mut Core) {
        let Some(row) = self.row(core).cloned() else {
            return;
        };
        if row.importable && !row.importing {
            let id = core.request(UiAction::ImportAddOn { id: row.id });
            self.requests.push(id);
        }
    }

    fn toggle(&mut self, core: &mut Core) {
        let Some(row) = self.row(core).cloned() else {
            return;
        };
        if row.importable {
            self.import(core);
            return;
        }
        if row.locked {
            core.message_ok(
                "Add-Ons",
                &format!("{} is part of the base game and stays on.", row.name),
            );
            return;
        }
        let enabled = !row.enabled;
        if !enabled && !row.needed_by.is_empty() {
            core.message_yes_no(
                "Turn Off Add-On?",
                &format!(
                    "Turning off {} also turns off what needs it:\n{}\n\nContinue?",
                    row.name,
                    row.needed_by.join(", ")
                ),
                Callback::AddOn {
                    id: row.id,
                    enabled,
                },
            );
            return;
        }
        let id = core.request(UiAction::SetAddOnEnabled {
            id: row.id,
            enabled,
        });
        self.requests.push(id);
    }
}

/// The details panel for one package.
pub fn details(r: &AddOnRow) -> String {
    let mut out = format!("<font:Impact:18>{}\n<font:Arial:14>", r.name);
    let mut subtitle = Vec::new();
    if !r.version.is_empty() {
        subtitle.push(format!("Version {}", r.version));
    }
    if r.locked {
        subtitle.push("Part of the base game".to_string());
    }
    if r.importable {
        subtitle.push("Old Blockland add-on, not imported yet".to_string());
    }
    out.push_str(&subtitle.join(" - "));
    out.push('\n');
    if !r.problems.is_empty() {
        out.push_str(if r.broken {
            "\nWon't load:\n"
        } else {
            "\nNote:\n"
        });
        for p in &r.problems {
            out.push_str(&format!("  - {p}\n"));
        }
    }
    if !r.description.is_empty() {
        out.push_str(&format!("\n{}\n", r.description));
    }
    if !r.runs.is_empty() {
        out.push_str(&format!("\nRuns: {}\n", r.runs));
    }
    for (label, list) in [
        ("Adds", &r.provides),
        ("Needs", &r.needs),
        ("Needed by", &r.needed_by),
        ("Allowed to", &r.allowed),
    ] {
        if !list.is_empty() {
            out.push_str(&format!("{label}: {}\n", list.join(", ")));
        }
    }
    let credits: Vec<String> = [
        (!r.authors.is_empty()).then(|| format!("By {}", r.authors)),
        (!r.license.is_empty()).then(|| format!("License {}", r.license)),
        (!r.source.is_empty()).then(|| r.source.clone()),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !credits.is_empty() {
        out.push_str(&format!("\n{}\n", credits.join(" - ")));
    }
    out.push_str(&format!("\nId: {}", r.id));
    out
}

impl Screen for AddOns {
    fn id(&self) -> ScreenId {
        ScreenId::AddOns
    }
    fn view(&self) -> &View {
        &self.view
    }
    fn view_mut(&mut self) -> &mut View {
        &mut self.view
    }
    fn on_wake(&mut self, core: &mut Core) {
        let id = core.request(UiAction::RequestAddOns);
        self.requests.push(id);
    }
    fn on_update(&mut self, core: &mut Core) {
        self.refresh(core);
    }
    fn on_result(
        &mut self,
        id: RequestId,
        _kind: Option<&Pending>,
        result: &Result<(), String>,
        core: &mut Core,
    ) -> bool {
        let Some(i) = self.requests.iter().position(|r| *r == id) else {
            return false;
        };
        self.requests.remove(i);
        if let Err(reason) = result {
            core.message_ok("Add-Ons", reason);
        }
        self.refresh(core);
        true
    }
    fn on_key(&mut self, key: Key, _mods: Modifiers, core: &mut Core) -> bool {
        if key == Key::Escape {
            core.pop(self.id());
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
        match (name.as_str(), ev.kind) {
            (_, EventKind::Close) | (DONE, EventKind::Click) => core.pop(self.id()),
            (SEARCH, EventKind::Changed) => self.refresh(core),
            (LIST, EventKind::Changed | EventKind::Submit) => {
                let item = self.view.selected(ev.node);
                match item.filter(|&i| i != HEADING) {
                    Some(i) => {
                        self.selected = self
                            .shown
                            .get(i as usize)
                            .and_then(|&r| core.add_ons.rows.get(r))
                            .map(|r| r.id.clone());
                    }
                    None => {
                        // Headings are not selectable.
                        self.view.select(ev.node, None);
                        self.selected = None;
                    }
                }
                self.show_details(core);
                if ev.kind == EventKind::Submit {
                    self.toggle(core);
                }
            }
            (IMPORT, EventKind::Click) => self.import(core),
            (ENABLED, EventKind::Click) => {
                self.toggle(core);
                // The box shows the host's answer, not the click.
                self.show_details(core);
            }
            (DEFAULTS, EventKind::Click) => core.message_yes_no(
                "Default Add-Ons",
                // The message box does not wrap; keep it to one line.
                "Turn off all but the base game?",
                Callback::DefaultAddOns,
            ),
            _ => {}
        }
    }
}

/// Joining a server that needs packages this player lacks: what is being
/// fetched, how far along, and Cancel. The host drives it through
/// [`ConnectionState::DownloadingPackages`].
pub struct PackageDownload {
    view: View,
}

const DL_LIST: &str = "PD_List";
const DL_PROGRESS: &str = "PD_Progress";
const DL_BYTES: &str = "PD_Bytes";
const DL_CANCEL: &str = "PD_Cancel";
/// Characters of "name version" that fit the first column (230 px of
/// Arial 14); the list does not clip columns, so longer names are cut.
const NAME_CHARS: usize = 34;
/// The same for the Can't Join dialog's 200 px name column.
const MISMATCH_NAME_CHARS: usize = 29;

/// `text` cut to `max` characters with "..." when it is longer.
fn fit(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let cut: String = text.chars().take(max.saturating_sub(3)).collect();
    format!("{}...", cut.trim_end())
}

impl PackageDownload {
    pub fn new(core: &Core) -> Self {
        let mut root = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480));
        root.children
            .push(swatch(Rect::new(0, 0, 640, 480), [0, 0, 0, 255]));
        root.children[0].h_sizing = HSizing::Width;
        root.children[0].v_sizing = VSizing::Height;
        let (_, mut win) = dialog("Joining Server", 440, 300);
        let mut intro = text("GuiMLTextProfile", Rect::new(12, 32, 416, 34), "");
        intro.class = "GuiMLTextCtrl".into();
        win.children.push(named(intro, "PD_Intro"));
        let mut list_scroll = scroll("PD_Scroll", Rect::new(12, 70, 416, 140));
        let mut list = named(
            ctrl(
                "GuiTextListCtrl",
                "GuiTextListProfile",
                Rect::new(0, 0, 400, 16),
            ),
            DL_LIST,
        );
        list.fields.insert("columns".into(), "0 230 300".into());
        list_scroll.children.push(list);
        win.children.push(list_scroll);
        win.children.push(named(
            ctrl(
                "GuiProgressCtrl",
                "GuiProgressProfile",
                Rect::new(12, 218, 416, 18),
            ),
            DL_PROGRESS,
        ));
        win.children.push(named(
            text("GuiTextProfile", Rect::new(12, 238, 416, 18), ""),
            DL_BYTES,
        ));
        win.children.push(named(
            button(
                "BlockButtonProfile",
                Rect::new(330, 262, 98, 28),
                "base/client/ui/button1",
                "Cancel",
                DL_CANCEL,
            ),
            DL_CANCEL,
        ));
        root.children.push(win);
        let mut view = View::new(&root);
        view.measure(&core.pack);
        let mut s = Self { view };
        s.refresh(core);
        s
    }

    fn refresh(&mut self, core: &Core) {
        let ConnectionState::DownloadingPackages(d) = &core.conn else {
            return;
        };
        let fetching = d
            .packages
            .iter()
            .filter(|p| p.state != DownloadState::Cached)
            .count();
        if let Some(n) = self.view.id("PD_Intro") {
            self.view.set_text(
                n,
                format!(
                    "{} uses {} add-on{} you don't have yet. They are only used on servers that need them and don't change your own Add-Ons.",
                    d.server,
                    fetching,
                    if fetching == 1 { "" } else { "s" }
                ),
            );
        }
        if let Some(n) = self.view.id(DL_LIST) {
            self.view.state(n).items = d
                .packages
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let state = match p.state {
                        DownloadState::Cached => "Ready",
                        DownloadState::Waiting => "Waiting",
                        DownloadState::Downloading => "Downloading",
                        DownloadState::Done => "Done",
                    };
                    (
                        format!(
                            "{}\t{}\t{state}",
                            fit(&format!("{} {}", p.name, p.version), NAME_CHARS),
                            megabytes(p.bytes)
                        ),
                        i as i64,
                    )
                })
                .collect();
        }
        let fraction = if d.total_bytes == 0 {
            1.0
        } else {
            (d.done_bytes as f64 / d.total_bytes as f64).clamp(0.0, 1.0) as f32
        };
        if let Some(n) = self.view.id(DL_PROGRESS) {
            self.view.set_num(n, fraction);
        }
        if let Some(n) = self.view.id(DL_BYTES) {
            self.view.set_text(
                n,
                format!(
                    "{} of {}",
                    megabytes(d.done_bytes),
                    megabytes(d.total_bytes)
                ),
            );
        }
    }
}

/// A join refused because add-ons differ: which ones, the server's version
/// beside this player's, and a way to the Add-Ons screen.
pub struct Mismatch {
    view: View,
}

const MM_LIST: &str = "MM_List";
const MM_OPEN: &str = "MM_OpenAddOns";
const MM_OK: &str = "MM_Ok";

impl Mismatch {
    pub fn new(core: &Core) -> Self {
        let (mut root, mut win) = dialog("Can't Join", 440, 300);
        let rows = core.add_on_mismatch.clone().unwrap_or_default().rows;
        let mut intro = text(
            "GuiMLTextProfile",
            Rect::new(12, 32, 416, 34),
            "This server's add-ons don't match yours. Everyone in a game needs the same versions of these:",
        );
        intro.class = "GuiMLTextCtrl".into();
        win.children.push(named(intro, "MM_Intro"));
        let mut list_scroll = scroll("MM_Scroll", Rect::new(12, 70, 416, 180));
        let mut list = named(
            ctrl(
                "GuiTextListCtrl",
                "GuiTextListProfile",
                Rect::new(0, 0, 400, 16),
            ),
            MM_LIST,
        );
        list.fields.insert("columns".into(), "0 200 300".into());
        list_scroll.children.push(list);
        win.children.push(list_scroll);
        win.children.push(named(
            button(
                "BlockButtonProfile",
                Rect::new(12, 262, 98, 28),
                "base/client/ui/button1",
                "Add-Ons",
                MM_OPEN,
            ),
            MM_OPEN,
        ));
        win.children.push(named(
            button(
                "BlockButtonProfile",
                Rect::new(330, 262, 98, 28),
                "base/client/ui/button1",
                "OK",
                MM_OK,
            ),
            MM_OK,
        ));
        root.children.push(win);
        let mut view = View::new(&root);
        view.measure(&core.pack);
        if let Some(n) = view.id(MM_LIST) {
            let side = |v: &str| {
                if v.is_empty() {
                    "not on".to_string()
                } else {
                    v.to_string()
                }
            };
            let mut items = vec![("Add-On\tServer\tYou".to_string(), -1)];
            items.extend(rows.iter().enumerate().map(|(i, r)| {
                (
                    format!(
                        "{}\t{}\t{}",
                        fit(&r.name, MISMATCH_NAME_CHARS),
                        side(&r.server),
                        side(&r.yours)
                    ),
                    i as i64,
                )
            }));
            view.state(n).items = items;
        }
        Self { view }
    }
}

impl Screen for Mismatch {
    fn id(&self) -> ScreenId {
        ScreenId::AddOnMismatch
    }
    fn view(&self) -> &View {
        &self.view
    }
    fn view_mut(&mut self) -> &mut View {
        &mut self.view
    }
    fn on_key(&mut self, key: Key, _mods: Modifiers, core: &mut Core) -> bool {
        if matches!(key, Key::Escape | Key::Return | Key::NumpadEnter) {
            core.add_on_mismatch = None;
            core.pop(self.id());
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
        match (name.as_str(), ev.kind) {
            (_, EventKind::Close) | (MM_OK, EventKind::Click) => {
                core.add_on_mismatch = None;
                core.pop(self.id());
            }
            (MM_OPEN, EventKind::Click) => {
                core.add_on_mismatch = None;
                core.pop(self.id());
                core.push(ScreenId::AddOns);
            }
            _ => {}
        }
    }
}

/// `12.3 MB`, or KB below a megabyte.
pub fn megabytes(bytes: u64) -> String {
    if bytes < 1024 * 1024 {
        format!("{} KB", bytes.div_ceil(1024))
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

impl Screen for PackageDownload {
    fn id(&self) -> ScreenId {
        ScreenId::PackageDownload
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
    fn wants_mouse(&self) -> bool {
        true
    }
    fn on_update(&mut self, core: &mut Core) {
        self.refresh(core);
    }
    fn on_key(&mut self, key: Key, _mods: Modifiers, core: &mut Core) -> bool {
        if key == Key::Escape {
            core.request(UiAction::CancelConnect);
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
        if (name == DL_CANCEL && ev.kind == EventKind::Click) || ev.kind == EventKind::Close {
            core.request(UiAction::CancelConnect);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{AddOnsView, DownloadRow, PackageDownload as Download, Settings, UiUpdate};
    use crate::binds::Platform;
    use crate::input::InputEvent;
    use crate::schema::UiPack;
    use crate::ui::{Ui, UiConfig};
    use std::rc::Rc;

    fn ui() -> Ui {
        Ui::new(
            Rc::new(Pack::from_parts(UiPack::default(), Default::default())),
            UiConfig {
                size: (640, 480),
                scale: Some(1.0),
                platform: Platform::Windows,
            },
            Settings {
                binds: Some(vec![]),
                ..Default::default()
            },
        )
    }
    fn row(id: &str, category: &str, enabled: bool) -> AddOnRow {
        AddOnRow {
            id: id.into(),
            name: format!("The {id}"),
            version: "1.0.0".into(),
            category: category.into(),
            enabled,
            runs: "Everyone".into(),
            ..Default::default()
        }
    }
    fn view() -> AddOnsView {
        let mut base = row("base", "Base Game", true);
        base.locked = true;
        let mut world = row("lab-world", "Game Modes", true);
        world.needed_by = vec!["The lab-hud".into()];
        AddOns_rows(vec![
            base,
            world,
            row("lab-hud", "Looks & HUD", false),
            row("gun", "Weapons & Items", false),
        ])
    }
    #[allow(non_snake_case)]
    fn AddOns_rows(rows: Vec<AddOnRow>) -> AddOnsView {
        AddOnsView {
            rows,
            notice: String::new(),
        }
    }
    fn select(s: &mut AddOns, ui: &mut Ui, name: &str) {
        let list = s.view.id(LIST).unwrap();
        let item = s
            .view
            .node(list)
            .state
            .items
            .iter()
            .find(|(t, _)| t.ends_with(name))
            .map(|(_, i)| *i)
            .unwrap();
        s.view.select(list, Some(item));
        s.on_event(
            &ViewEvent {
                node: list,
                kind: EventKind::Changed,
            },
            &mut ui.core,
        );
    }

    #[test]
    fn the_list_groups_by_category_and_marks_state() {
        let mut ui = ui();
        ui.apply(UiUpdate::AddOns(view()));
        let s = AddOns::new(&ui.core);
        let list = s.view.id(LIST).unwrap();
        let items: Vec<_> = s
            .view
            .node(list)
            .state
            .items
            .iter()
            .map(|(t, _)| t.as_str())
            .collect();
        assert_eq!(
            items,
            [
                "\tBASE GAME",
                "Base\tThe base",
                "\tGAME MODES",
                "On\tThe lab-world",
                "\tLOOKS & HUD",
                "\tThe lab-hud",
                "\tWEAPONS & ITEMS",
                "\tThe gun",
            ]
        );
    }

    #[test]
    fn toggling_asks_the_host_and_confirms_dependents() {
        let mut ui = ui();
        ui.apply(UiUpdate::AddOns(view()));
        let mut s = AddOns::new(&ui.core);
        ui.drain_actions();
        // Turning something on is one request.
        select(&mut s, &mut ui, "The gun");
        let check = s.view.id(ENABLED).unwrap();
        assert!(!s.view.bool_value(check));
        s.on_event(
            &ViewEvent {
                node: check,
                kind: EventKind::Click,
            },
            &mut ui.core,
        );
        let actions: Vec<_> = ui.drain_actions().into_iter().map(|(_, a)| a).collect();
        assert_eq!(
            actions,
            [UiAction::SetAddOnEnabled {
                id: "gun".into(),
                enabled: true
            }]
        );
        // Turning off something others need asks first.
        select(&mut s, &mut ui, "The lab-world");
        s.on_event(
            &ViewEvent {
                node: check,
                kind: EventKind::Click,
            },
            &mut ui.core,
        );
        assert!(ui.drain_actions().is_empty());
        assert!(ui.core.cmds.iter().any(|c| matches!(
            c,
            crate::ui::StackCmd::Message(m)
                if m.on_yes == Callback::AddOn { id: "lab-world".into(), enabled: false }
        )));
        // The base game stays on.
        ui.core.cmds.clear();
        select(&mut s, &mut ui, "The base");
        assert!(!s.view.node(check).state.active);
        s.on_event(
            &ViewEvent {
                node: s.view.id(LIST).unwrap(),
                kind: EventKind::Submit,
            },
            &mut ui.core,
        );
        assert!(ui.drain_actions().is_empty());
    }

    #[test]
    fn old_add_ons_offer_import_instead_of_enabled() {
        let mut ui = ui();
        let mut v = view();
        let mut old = row("legacy:Weapon_Shotgun", "Not Imported Yet", false);
        old.version = String::new();
        old.importable = true;
        v.rows.push(old);
        ui.apply(UiUpdate::AddOns(v));
        let mut s = AddOns::new(&ui.core);
        ui.drain_actions();
        select(&mut s, &mut ui, "The legacy:Weapon_Shotgun");
        let import = s.view.id(IMPORT).unwrap();
        assert!(s.view.node(import).state.visible);
        assert!(!s.view.node(s.view.id(ENABLED).unwrap()).state.visible);
        let text = s.view.text_of(s.view.id(DETAILS).unwrap());
        assert!(
            text.contains("Old Blockland add-on, not imported yet"),
            "{text}"
        );
        s.on_event(
            &ViewEvent {
                node: import,
                kind: EventKind::Click,
            },
            &mut ui.core,
        );
        let actions: Vec<_> = ui.drain_actions().into_iter().map(|(_, a)| a).collect();
        assert_eq!(
            actions,
            [UiAction::ImportAddOn {
                id: "legacy:Weapon_Shotgun".into()
            }]
        );
        // While it runs, the list says so and Import waits.
        let mut v = ui.core.add_ons.clone();
        v.rows.last_mut().unwrap().importing = true;
        ui.apply(UiUpdate::AddOns(v));
        s.on_update(&mut ui.core);
        assert!(!s.view.node(import).state.active);
        let list = s.view.id(LIST).unwrap();
        assert!(
            s.view
                .node(list)
                .state
                .items
                .iter()
                .any(|(t, _)| t == "...\tThe legacy:Weapon_Shotgun")
        );
    }

    #[test]
    fn search_filters_and_details_explain() {
        let mut ui = ui();
        let mut v = view();
        v.rows[2].problems = vec!["needs `lab-economy` ^1.0, which is not installed".into()];
        v.rows[2].broken = true;
        v.rows[2].allowed = vec!["send chat messages".into()];
        ui.apply(UiUpdate::AddOns(v));
        let mut s = AddOns::new(&ui.core);
        let search = s.view.id(SEARCH).unwrap();
        s.view.set_text(search, "hud");
        s.on_event(
            &ViewEvent {
                node: search,
                kind: EventKind::Changed,
            },
            &mut ui.core,
        );
        let list = s.view.id(LIST).unwrap();
        let items: Vec<_> = s
            .view
            .node(list)
            .state
            .items
            .iter()
            .map(|(t, _)| t.clone())
            .collect();
        assert_eq!(items, ["\tLOOKS & HUD", "!\tThe lab-hud"]);
        select(&mut s, &mut ui, "The lab-hud");
        let text = s.view.text_of(s.view.id(DETAILS).unwrap());
        assert!(
            text.contains("Won't load:\n  - needs `lab-economy`"),
            "{text}"
        );
        assert!(text.contains("Allowed to: send chat messages"), "{text}");
    }

    #[test]
    fn the_main_menu_opens_add_ons_and_the_screen_asks_for_rows() {
        let mut data = UiPack::default();
        let mut menu = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480));
        // v20's column: Options is the lowest main button, Quit sits at the
        // bottom of the screen.
        for (name, y) in [
            ("MM_StartButton", 160),
            ("MM_OptionsButton", 280),
            ("MM_QuitButton", 422),
        ] {
            menu.children.push(named(
                ctrl(
                    "GuiBitmapButtonCtrl",
                    "BlockButtonProfile",
                    Rect::new(0, y, 224, 40),
                ),
                name,
            ));
        }
        data.layouts.insert("MainMenuGui".into(), menu);
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
        assert_eq!(ui.top_id(), ScreenId::MainMenu);
        let (x, y) = ui
            .control_center(ScreenId::MainMenu, "MM_AddOnsButton")
            .unwrap();
        assert_eq!((x, y), (80.0, 341.0), "directly under Options");
        ui.handle_input(InputEvent::MouseMove { x, y });
        ui.handle_input(InputEvent::MouseDown {
            button: crate::input::MouseButton::Left,
            x,
            y,
        });
        ui.handle_input(InputEvent::MouseUp {
            button: crate::input::MouseButton::Left,
            x,
            y,
        });
        assert_eq!(ui.top_id(), ScreenId::AddOns);
        let actions: Vec<_> = ui.drain_actions().into_iter().map(|(_, a)| a).collect();
        assert!(actions.contains(&UiAction::RequestAddOns), "{actions:?}");
    }

    #[test]
    fn a_refused_join_lists_the_differing_add_ons() {
        let mut ui = ui();
        ui.apply(UiUpdate::AddOnMismatch(crate::api::AddOnMismatch {
            rows: vec![
                crate::api::MismatchRow {
                    name: "Creeper".into(),
                    server: "1.0.0".into(),
                    yours: String::new(),
                },
                crate::api::MismatchRow {
                    name: "v20-weapons".into(),
                    server: "9.0.0".into(),
                    yours: "8.0.0".into(),
                },
            ],
        }));
        ui.apply(UiUpdate::Connection(ConnectionState::Failed {
            reason: "Your content does not match the server: ...".into(),
        }));
        ui.handle_input(InputEvent::MouseMove { x: 1.0, y: 1.0 });
        assert_eq!(ui.top_id(), ScreenId::AddOnMismatch);
        let mut s = Mismatch::new(&ui.core);
        let list = s.view.id(MM_LIST).unwrap();
        let items: Vec<_> = s
            .view
            .node(list)
            .state
            .items
            .iter()
            .map(|(t, _)| t.as_str())
            .collect();
        assert_eq!(
            items,
            [
                "Add-On\tServer\tYou",
                "Creeper\t1.0.0\tnot on",
                "v20-weapons\t9.0.0\t8.0.0"
            ]
        );
        let open = s.view.id(MM_OPEN).unwrap();
        s.on_event(
            &ViewEvent {
                node: open,
                kind: EventKind::Click,
            },
            &mut ui.core,
        );
        assert!(ui.core.add_on_mismatch.is_none());
        ui.handle_input(InputEvent::MouseMove { x: 2.0, y: 2.0 });
        assert_eq!(ui.top_id(), ScreenId::AddOns);
        // Other failures stay a plain message box.
        ui.apply(UiUpdate::Connection(ConnectionState::Failed {
            reason: "Timed out".into(),
        }));
        ui.handle_input(InputEvent::MouseMove { x: 3.0, y: 3.0 });
        assert_eq!(ui.top_id(), ScreenId::MessageBox);
    }

    #[test]
    fn the_join_screen_shows_download_progress_and_cancels() {
        let mut ui = ui();
        ui.apply(UiUpdate::Connection(ConnectionState::DownloadingPackages(
            Download {
                server: "Creeper Hill".into(),
                packages: vec![
                    DownloadRow {
                        name: "Creeper".into(),
                        version: "1.0.0".into(),
                        bytes: 3 * 1024 * 1024,
                        state: DownloadState::Downloading,
                    },
                    DownloadRow {
                        name: "Creeper Model".into(),
                        version: "1.0.0".into(),
                        bytes: 2048,
                        state: DownloadState::Cached,
                    },
                ],
                done_bytes: 1024 * 1024,
                total_bytes: 3 * 1024 * 1024,
            },
        )));
        ui.handle_input(InputEvent::MouseMove { x: 1.0, y: 1.0 });
        assert_eq!(ui.top_id(), ScreenId::PackageDownload);
        let mut s = PackageDownload::new(&ui.core);
        let bytes = s.view.text_of(s.view.id(DL_BYTES).unwrap());
        assert_eq!(bytes, "1.0 MB of 3.0 MB");
        assert!((s.view.num(s.view.id(DL_PROGRESS).unwrap()) - 1.0 / 3.0).abs() < 1e-4);
        let intro = s.view.text_of(s.view.id("PD_Intro").unwrap());
        assert!(
            intro.starts_with("Creeper Hill uses 1 add-on you don't have yet"),
            "{intro}"
        );
        ui.drain_actions();
        let cancel = s.view.id(DL_CANCEL).unwrap();
        s.on_event(
            &ViewEvent {
                node: cancel,
                kind: EventKind::Click,
            },
            &mut ui.core,
        );
        let actions: Vec<_> = ui.drain_actions().into_iter().map(|(_, a)| a).collect();
        assert_eq!(actions, [UiAction::CancelConnect]);
    }
}
