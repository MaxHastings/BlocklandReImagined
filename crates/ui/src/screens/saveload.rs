//! Native save/load dialogs. Files live on the client's PC; the host authorizes loading.
use super::*;
use crate::api::{IconRef, SaveFileInfo, UiAction};
use crate::ui::Callback;
use crate::view::{EventKind, search_rows};

/// Native saves are `<name>.world.json`; the dialogs show and take the bare
/// name like v20 did with `.bls`.
const EXTENSION: &str = ".world.json";
/// Load Bricks' button showing where old `.bls` saves go.
const OPEN_FOLDER: &str = "LoadBricks_OpenFolder";
/// Load Bricks' search box. Not in v20; it finds saves across every map.
const SEARCH: &str = "LoadBricks_Search";
/// Height the search row takes from the top of the file list.
const SEARCH_ROW: i32 = 24;

fn display_name(file: &str) -> &str {
    let name = file.strip_suffix(EXTENSION).unwrap_or(file);
    // Autosaves are `autosave-<unix millis>`; the list's date column says when.
    match name.strip_prefix("autosave-") {
        Some(stamp) if !stamp.is_empty() && stamp.bytes().all(|b| b.is_ascii_digit()) => "Autosave",
        _ => name,
    }
}

pub struct SaveLoad {
    id: ScreenId,
    view: View,
    maps: Vec<String>,
    files: Vec<SaveFileInfo>,
    map: Option<String>,
    pending: Option<RequestId>,
    sort_date: bool,
    descending: bool,
    /// Load Bricks' search text; while it is not blank the list holds the
    /// saves of every map whose name or map matches it.
    search: String,
}

fn valid_name(name: &str) -> bool {
    let stem = name.strip_suffix(".world.json").unwrap_or("");
    let reserved = stem.split('.').next().unwrap_or("").to_ascii_uppercase();
    !stem.trim().is_empty()
        && name.len() < 255
        && name.trim() == name
        && !stem.ends_with(['.', ' '])
        && !name
            .chars()
            .any(|c| c.is_control() || "\\/:*?\"<>|".contains(c))
        && ![
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
            "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ]
        .contains(&reserved.as_str())
}

impl SaveLoad {
    pub fn new(id: ScreenId, core: &Core) -> Self {
        let save = id == ScreenId::SaveBricks;
        let mut s = Self {
            id,
            view: layout_view(
                core,
                if save {
                    "saveBricksGui"
                } else {
                    "LoadBricksGui"
                },
            ),
            maps: vec![],
            files: vec![],
            map: core.save_context.as_ref().map(|c| c.0.clone()),
            pending: None,
            sort_date: false,
            descending: false,
            search: String::new(),
        };
        for name in [
            "SaveBricks_DownloadWindow",
            "LoadBricks_PreviewDemoBlocker",
            "LoadBricks_LoadBlocker",
        ] {
            if let Some(n) = s.view.id(name) {
                s.view.set_visible(n, false);
            }
        }
        for n in s.view.walk().collect::<Vec<_>>() {
            if let Some(var) = s.view.node(n).ctrl.variable.clone() {
                s.view.set_bool(n, core.prefs.bool_or(&var, true));
                // Torque's Fast Load skipped brick ghosting; native loading
                // has no equivalent.
                if var.eq_ignore_ascii_case("$pref::FastLoad") {
                    s.view.set_visible(n, false);
                }
            }
        }
        if !save {
            // Beside Load Brick Ownership, whose box keeps its text's width.
            if let Some(n) = s.view.id("LoadBricks_DoOwnership") {
                s.view.nodes[n].ctrl.extent[0] = 175;
            }
            if let Some(window) = s.view.id("LoadBricks_Window") {
                let mut open = button(
                    "BlockButtonProfile",
                    Rect::new(520, 259, 106, 28),
                    "base/client/ui/button1",
                    "Saves Folder",
                    OPEN_FOLDER,
                );
                open.name = Some(OPEN_FOLDER.into());
                s.view.add(window, open);
            }
            s.add_search();
            s.view.measure(&core.pack);
            s.view.focus = s.view.id(SEARCH);
        }
        if save {
            s.view.focus = s.view.id("SaveBricks_FileName");
            // The recovered description edit extends below its authored form.
            if let Some(n) = s.view.id("SaveBricks_Description") {
                s.view.nodes[n].ctrl.extent[1] = 90;
            }
        } else if let Some(n) = s.view.id("LoadBricks_Description") {
            s.view.nodes[n].ctrl.extent[1] = 43;
        }
        s.refresh(core);
        s
    }
    /// Puts the search row where the file list's column headers were and
    /// moves the headers and the list down to make room.
    fn add_search(&mut self) {
        // v20's file list sits in a scroll control beside its headers.
        let Some((scroll, parent)) = self
            .view
            .id("LoadBricks_FileList")
            .and_then(|list| self.view.node(list).parent)
            .filter(|&p| self.view.node(p).ctrl.class == "GuiScrollCtrl")
            .and_then(|scroll| Some((scroll, self.view.node(scroll).parent?)))
        else {
            return;
        };
        let headers: Vec<NodeId> = self
            .view
            .walk()
            .filter(|&n| {
                let c = &self.view.node(n).ctrl;
                c.command
                    .as_ref()
                    .is_some_and(|c| c.to_ascii_lowercase().starts_with("sortlist("))
            })
            .collect();
        let [x, list_y] = self.view.node(scroll).ctrl.position;
        let width = self.view.node(scroll).ctrl.extent[0];
        // The row goes where the headers were when they share the list's
        // parent, else where the list was.
        let y = headers
            .iter()
            .filter(|&&n| self.view.node(n).parent == Some(parent))
            .map(|&n| self.view.node(n).ctrl.position[1])
            .min()
            .unwrap_or(list_y);
        for n in headers.into_iter().chain([scroll]) {
            self.view.nodes[n].ctrl.position[1] += SEARCH_ROW;
        }
        self.view.nodes[scroll].ctrl.extent[1] -= SEARCH_ROW;
        self.view.add(
            parent,
            text("GuiTextProfile", Rect::new(x, y + 1, 48, 18), "Search:"),
        );
        let mut edit = ctrl(
            "GuiTextEditCtrl",
            "BlockTextEditProfile",
            Rect::new(x + 50, y, width - 50, 18),
        );
        edit.name = Some(SEARCH.into());
        self.view.add(parent, edit);
    }
    fn searching(&self) -> bool {
        !self.save() && !self.search.trim().is_empty()
    }
    fn clear_search(&mut self, core: &Core) {
        self.search.clear();
        if let Some(n) = self.view.id(SEARCH) {
            self.view.set_text(n, "");
        }
        self.refresh(core);
    }
    /// Moves the list's selection `step` rows, from the top when none is
    /// selected.
    fn step_selection(&mut self, step: i64, core: &Core) {
        let Some(list) = self.view.id(self.list_name()) else {
            return;
        };
        if self.files.is_empty() {
            return;
        }
        let last = self.files.len() as i64 - 1;
        let row = self
            .view
            .selected(list)
            .map_or(0, |i| (i + step).clamp(0, last));
        self.view.select(list, Some(row));
        self.view.reveal_row(list, row);
        self.refresh(core);
    }
    fn save(&self) -> bool {
        self.id == ScreenId::SaveBricks
    }
    fn list_name(&self) -> &'static str {
        if self.save() {
            "SaveBricks_FileList"
        } else {
            "LoadBricks_FileList"
        }
    }
    fn set(&mut self, name: &str, text: &str) {
        if let Some(n) = self.view.id(name) {
            self.view.set_text(n, text);
        }
    }
    fn title(&self) -> &'static str {
        if self.save() {
            "Save Bricks"
        } else {
            "Load Bricks"
        }
    }
    fn edit(&self, name: &str) -> String {
        self.view
            .id(name)
            .map(|n| self.view.edit_text(n))
            .unwrap_or_default()
    }
    fn checked(&self, name: &str) -> bool {
        self.view.id(name).is_some_and(|n| self.view.bool_value(n))
    }
    fn selected(&self) -> Option<&SaveFileInfo> {
        self.view
            .id(self.list_name())
            .and_then(|n| self.view.selected(n))
            .and_then(|i| usize::try_from(i).ok())
            .and_then(|i| self.files.get(i))
    }
    fn refresh(&mut self, core: &Core) {
        let previous = self.selected().map(|f| (f.map.clone(), f.name.clone()));
        self.maps = core.save_maps.clone();
        self.maps
            .extend(core.save_files.iter().map(|f| f.map.clone()));
        self.maps.sort_by_key(|m| m.to_ascii_lowercase());
        self.maps.dedup();
        if self.save() {
            self.map = core.save_context.as_ref().map(|c| c.0.clone());
        } else if self.map.as_ref().is_none_or(|m| !self.maps.contains(m)) {
            self.map = self.maps.first().cloned();
        }
        let searching = self.searching();
        self.files = core
            .save_files
            .iter()
            .filter(|f| searching || self.map.as_ref().is_some_and(|m| m == &f.map))
            .cloned()
            .collect();
        self.files.sort_by(|a, b| {
            let order = if self.sort_date {
                a.modified.cmp(&b.modified)
            } else {
                a.name
                    .to_ascii_lowercase()
                    .cmp(&b.name.to_ascii_lowercase())
            };
            if self.descending {
                order.reverse()
            } else {
                order
            }
        });
        if searching {
            // Ranked like every other search box, each rank in the list's order.
            let keys: Vec<String> = self
                .files
                .iter()
                .map(|f| format!("{} {}", display_name(&f.name), f.map))
                .collect();
            self.files = search_rows(&keys, &self.search)
                .into_iter()
                .map(|i| self.files[i].clone())
                .collect();
        }
        if let Some(n) = self.view.id(self.list_name()) {
            self.view.state(n).items = self
                .files
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    (
                        format!(
                            "{}{}{}\t{}",
                            display_name(&f.name),
                            if searching {
                                format!(" ({})", f.map)
                            } else {
                                String::new()
                            },
                            if f.damaged { " (damaged)" } else { "" },
                            f.modified
                        ),
                        i as i64,
                    )
                })
                .collect();
            let kept = previous.and_then(|p| {
                self.files
                    .iter()
                    .position(|f| (f.map.clone(), f.name.clone()) == p)
            });
            // A search always has its best match picked, so Enter loads it.
            let row = kept.or_else(|| (searching && !self.files.is_empty()).then_some(0));
            self.view.select(n, row.map(|i| i as i64));
        }
        if searching && let Some(f) = self.selected() {
            // The map menu and preview follow the picked result, and the
            // list shows that map once the search is cleared.
            self.map = Some(f.map.clone());
        }
        if let Some(n) = self.view.id("LoadBricks_MapMenu") {
            self.view.state(n).items = self
                .maps
                .iter()
                .enumerate()
                .map(|(i, m)| (m.clone(), i as i64))
                .collect();
            self.view.select(
                n,
                self.map
                    .as_ref()
                    .and_then(|m| self.maps.iter().position(|x| x == m))
                    .map(|i| i as i64),
            );
        }
        if self.save() {
            self.set(
                "SaveBricks_Window",
                &format!(
                    "Save Bricks - {}",
                    self.map.as_deref().unwrap_or("awaiting map")
                ),
            );
            if let Some(n) = self.view.id("SaveBricks_Preview") {
                self.view.state(n).bitmap = match core.save_context.as_ref().map(|c| &c.1) {
                    Some(IconRef::Pack(p)) => Some(p.clone()),
                    _ => None,
                };
                self.view.state(n).external_texture = match core.save_context.as_ref().map(|c| &c.1)
                {
                    Some(IconRef::External(id)) => Some(*id),
                    _ => None,
                };
            }
        } else {
            let preview = self
                .map
                .as_ref()
                .and_then(|name| {
                    core.maps
                        .iter()
                        .find(|m| m.name.eq_ignore_ascii_case(name))
                        .map(|m| m.preview.clone())
                        .or_else(|| {
                            core.pack
                                .data
                                .maps
                                .iter()
                                .find(|m| m.display_name.eq_ignore_ascii_case(name))
                                .and_then(|m| m.preview.clone())
                                .map(IconRef::Pack)
                        })
                })
                .unwrap_or(IconRef::None);
            if let Some(n) = self.view.id("LoadBricks_Preview") {
                self.view.state(n).bitmap = match &preview {
                    IconRef::Pack(p) => Some(p.clone()),
                    _ => None,
                };
                self.view.state(n).external_texture = match preview {
                    IconRef::External(id) => Some(id),
                    _ => None,
                };
            }
            self.description();
        }
        self.lock();
    }
    fn description(&mut self) {
        let description = self
            .selected()
            .map(|f| {
                format!(
                    "{}\n{}",
                    f.description,
                    f.brick_count
                        .map(|n| format!("{n} bricks"))
                        .unwrap_or_default()
                )
            })
            .unwrap_or_default();
        self.set("LoadBricks_Description", &description);
    }
    fn lock(&mut self) {
        for n in self.view.walk().collect::<Vec<_>>() {
            let c = self.view.node(n).ctrl.clone();
            if matches!(
                c.class.as_str(),
                "GuiButtonCtrl"
                    | "GuiBitmapButtonCtrl"
                    | "GuiTextEditCtrl"
                    | "GuiMLTextEditCtrl"
                    | "GuiCheckBoxCtrl"
                    | "GuiPopUpMenuCtrl"
                    | "GuiTextListCtrl"
            ) {
                self.view.set_active(n, self.pending.is_none());
            }
        }
        if let Some(n) = self.view.by_command(if self.save() {
            "SaveBricks_Save();"
        } else {
            "LoadBricks_ClickLoadButton();"
        }) {
            self.view.set_active(
                n,
                self.pending.is_none()
                    && if self.save() {
                        self.map.is_some()
                    } else {
                        self.selected().is_some()
                    },
            );
        }
    }
    fn submit(&mut self, core: &mut Core) {
        if self.pending.is_some() {
            return;
        }
        if self.save() {
            if self.map.is_none() {
                return;
            }
            let base = self.edit("SaveBricks_FileName");
            if base.trim().is_empty() {
                core.message_ok("No Filename", "You must enter a filename.");
                return;
            }
            let name = format!("{base}{EXTENSION}");
            if !valid_name(&name) {
                core.message_ok(
                    "Invalid Filename",
                    "Filenames cannot contain any of these characters \\ / : * ? \" < > |",
                );
                return;
            }
            let description = self.edit("SaveBricks_Description");
            let events = self.checked("SaveBricks_ExtendedInfo");
            let ownership = self.checked("SaveBricks_Ownership");
            if self
                .files
                .iter()
                .any(|f| f.name.eq_ignore_ascii_case(&name))
            {
                core.message_yes_no(
                    "File Exists, Overwrite?",
                    &format!("Are you sure you want to overwrite the file \"{base}\"?"),
                    Callback::OverwriteSave {
                        name,
                        description,
                        events,
                        ownership,
                    },
                );
                return;
            }
            self.pending = Some(core.request_pending(
                UiAction::SaveBricks {
                    name,
                    description,
                    events,
                    ownership,
                    overwrite: false,
                },
                Pending::Save,
            ));
        } else {
            if !core.is_admin() {
                core.message_ok(
                    "Load Bricks",
                    "Loading requires host or administrator permission.",
                );
                return;
            }
            let Some(file) = self.selected().cloned() else {
                return;
            };
            self.pending = Some(core.request_pending(
                UiAction::LoadBricks {
                    map: file.map,
                    name: file.name,
                    ownership: self.checked("LoadBricks_DoOwnership"),
                },
                Pending::Load,
            ));
        }
        self.lock();
    }
    fn cancel(&mut self, core: &mut Core) {
        if self.pending.is_none() {
            core.pop(self.id);
        }
    }
}
impl Screen for SaveLoad {
    fn id(&self) -> ScreenId {
        self.id
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
        core.request(UiAction::RequestSaveList {
            map: if self.save() { self.map.clone() } else { None },
        });
    }
    fn on_update(&mut self, core: &mut Core) {
        self.refresh(core);
    }
    fn on_key(&mut self, key: Key, _mods: Modifiers, core: &mut Core) -> bool {
        if key == Key::Escape {
            // Escape clears a search first, then closes.
            if self.search.is_empty() {
                self.cancel(core);
            } else {
                self.clear_search(core);
            }
            return true;
        }
        let search = self.view.id(SEARCH);
        if search.is_none() || self.view.focus != search || self.pending.is_some() {
            return false;
        }
        // In the search box Enter loads the picked result and the arrows
        // walk the results; other keys are text.
        match key {
            Key::Return | Key::NumpadEnter => self.submit(core),
            Key::Up => self.step_selection(-1, core),
            Key::Down => self.step_selection(1, core),
            _ => return false,
        }
        true
    }
    fn on_event(&mut self, ev: &ViewEvent, core: &mut Core) {
        if self.pending.is_some() || !self.view.node(ev.node).state.active {
            return;
        }
        if ev.kind == EventKind::Close {
            self.cancel(core);
            return;
        }
        if ev.kind == EventKind::Changed {
            if self.view.id(SEARCH) == Some(ev.node) {
                self.search = self.view.edit_text(ev.node);
                self.refresh(core);
                if let Some(list) = self.view.id(self.list_name()) {
                    let row = self.view.selected(list).unwrap_or(0);
                    self.view.reveal_row(list, row);
                }
            } else if self.view.id("LoadBricks_MapMenu") == Some(ev.node) {
                // Picking a map ends the search and browses that map.
                self.search.clear();
                if let Some(n) = self.view.id(SEARCH) {
                    self.view.set_text(n, "");
                }
                self.map = self
                    .view
                    .selected(ev.node)
                    .and_then(|i| self.maps.get(i as usize))
                    .cloned();
                core.request(UiAction::RequestSaveList {
                    map: self.map.clone(),
                });
                self.refresh(core);
            } else if self.view.id(self.list_name()) == Some(ev.node) {
                if self.save() {
                    if let Some(f) = self.selected().cloned() {
                        self.set("SaveBricks_FileName", display_name(&f.name));
                        // A damaged file's description is our notice, not the player's text.
                        let description = if f.damaged { "" } else { f.description.as_str() };
                        self.set("SaveBricks_Description", description);
                    }
                } else if self.searching() {
                    // The map and preview follow the picked result.
                    self.refresh(core);
                } else {
                    self.description();
                }
                self.lock();
            }
            return;
        }
        if !matches!(ev.kind, EventKind::Click | EventKind::Submit) {
            return;
        }
        let cmd = command_of(&self.view, ev.node).to_ascii_lowercase();
        if cmd.starts_with("sortlist(") {
            let date = cmd.contains(", 2");
            self.descending = if self.sort_date == date {
                !self.descending
            } else {
                date
            };
            self.sort_date = date;
            self.refresh(core);
            return;
        }
        match cmd.as_str() {
            "savebricks_save();" | "loadbricks_clickloadbutton();" => self.submit(core),
            "canvas.popdialog(\"savebricksgui\");" | "canvas.popdialog(\"loadbricksgui\");" => {
                self.cancel(core)
            }
            c if c.eq_ignore_ascii_case(OPEN_FOLDER) => {
                core.request(UiAction::OpenSavesFolder);
            }
            "savebricks_description.settext(\"\");" => {
                self.set("SaveBricks_Description", "");
            }
            _ => {}
        }
    }
    fn on_result(
        &mut self,
        id: RequestId,
        kind: Option<&Pending>,
        result: &Result<(), String>,
        core: &mut Core,
    ) -> bool {
        // A confirmed overwrite is requested by the message box; this dialog
        // still owns its result.
        let confirmed_overwrite = self.save() && kind == Some(&Pending::Save);
        if self.pending != Some(id) && !confirmed_overwrite {
            return false;
        }
        self.pending = None;
        match result {
            Ok(()) => {
                core.pop(self.id);
                if !self.save() {
                    core.pop(ScreenId::EscapeMenu);
                }
            }
            Err(e) if e == crate::api::LOAD_CANCELED => self.lock(),
            Err(e) => {
                core.message_ok(self.title(), &format!("Request rejected: {e}"));
                self.lock();
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{ConnectionState, Settings};
    use crate::binds::Platform;
    use crate::schema::UiPack;
    use crate::ui::{Ui, UiConfig};
    use std::rc::Rc;
    fn fixture() -> Ui {
        let mut pack = UiPack::default();
        for (layout, names) in [
            (
                "saveBricksGui",
                vec![
                    ("GuiTextEditCtrl", "SaveBricks_FileName"),
                    ("GuiMLTextEditCtrl", "SaveBricks_Description"),
                    ("GuiCheckBoxCtrl", "SaveBricks_ExtendedInfo"),
                    ("GuiCheckBoxCtrl", "SaveBricks_Ownership"),
                    ("GuiTextListCtrl", "SaveBricks_FileList"),
                ],
            ),
            (
                "LoadBricksGui",
                vec![
                    ("GuiPopUpMenuCtrl", "LoadBricks_MapMenu"),
                    ("GuiCheckBoxCtrl", "LoadBricks_DoOwnership"),
                    ("GuiMLTextCtrl", "LoadBricks_Description"),
                ],
            ),
        ] {
            let mut c = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480));
            for (class, name) in names {
                let mut child = ctrl(class, "GuiDefaultProfile", Rect::new(0, 0, 100, 20));
                child.name = Some(name.into());
                c.children.push(child);
            }
            if layout == "LoadBricksGui" {
                // Like v20's: the list in a scroll control under two sort headers.
                let mut window = ctrl(
                    "GuiWindowCtrl",
                    "GuiDefaultProfile",
                    Rect::new(0, 0, 640, 480),
                );
                window.name = Some("LoadBricks_Window".into());
                for (x, column) in [(14, 0), (200, 2)] {
                    let mut header = ctrl(
                        "GuiButtonCtrl",
                        "GuiDefaultProfile",
                        Rect::new(x, 80, 80, 16),
                    );
                    header.command = Some(format!("sortList(LoadBricks_FileList, {column});"));
                    window.children.push(header);
                }
                let mut scroll = ctrl(
                    "GuiScrollCtrl",
                    "GuiDefaultProfile",
                    Rect::new(14, 96, 310, 370),
                );
                let mut list = ctrl(
                    "GuiTextListCtrl",
                    "GuiDefaultProfile",
                    Rect::new(0, 0, 294, 16),
                );
                list.name = Some("LoadBricks_FileList".into());
                scroll.children.push(list);
                window.children.push(scroll);
                c.children.push(window);
            }
            pack.layouts.insert(layout.into(), c);
        }
        let mut ui = Ui::new(
            Rc::new(Pack::from_parts(pack, Default::default())),
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
        ui.core.conn = ConnectionState::InGame {
            server_name: "Test".into(),
            max_players: 8,
            local: true,
            single_player: true,
            admin: true,
        };
        ui.core.save_context = Some(("Bedroom".into(), IconRef::None));
        ui.core.save_maps = vec!["Bedroom".into(), "Kitchen".into()];
        ui.core.save_files = vec![
            SaveFileInfo {
                name: "House.world.json".into(),
                map: "Bedroom".into(),
                modified: "2026-09-26".into(),
                description: "My house".into(),
                brick_count: Some(25),
                damaged: false,
            },
            SaveFileInfo {
                name: "Table.world.json".into(),
                map: "Kitchen".into(),
                modified: "2026-09-25".into(),
                description: "My table".into(),
                brick_count: Some(10),
                damaged: false,
            },
        ];
        ui.drain_actions();
        ui
    }
    fn add_save(ui: &mut Ui, name: &str, map: &str) {
        ui.core.save_files.push(SaveFileInfo {
            name: format!("{name}.world.json"),
            map: map.into(),
            modified: "2026-09-27".into(),
            description: String::new(),
            brick_count: None,
            damaged: false,
        });
    }
    fn search(s: &mut SaveLoad, ui: &mut Ui, text: &str) {
        let node = s.view.id(SEARCH).unwrap();
        s.view.set_text(node, text);
        s.on_event(
            &ViewEvent {
                node,
                kind: EventKind::Changed,
            },
            &mut ui.core,
        );
    }
    fn rows(s: &SaveLoad) -> Vec<String> {
        let list = s.view.id("LoadBricks_FileList").unwrap();
        s.view
            .node(list)
            .state
            .items
            .iter()
            .map(|(t, _)| t.clone())
            .collect()
    }
    #[test]
    fn search_row_takes_the_headers_place_above_the_list() {
        let ui = fixture();
        let s = SaveLoad::new(ScreenId::LoadBricks, &ui.core);
        let search = s.view.id(SEARCH).expect("search box");
        assert_eq!(
            s.view.focus,
            Some(search),
            "typing goes straight into the search"
        );
        let at = |n: NodeId| {
            let c = &s.view.node(n).ctrl;
            (c.position, c.extent)
        };
        assert_eq!(at(search), ([64, 80], [260, 18]));
        let header = s
            .view
            .by_command("sortList(LoadBricks_FileList, 0);")
            .unwrap();
        assert_eq!(at(header).0, [14, 104]);
        let list = s.view.id("LoadBricks_FileList").unwrap();
        let scroll = s.view.node(list).parent.unwrap();
        assert_eq!(at(scroll), ([14, 120], [310, 346]));
        // Save Bricks lists one map and has no search.
        let s = SaveLoad::new(ScreenId::SaveBricks, &ui.core);
        assert!(s.view.id(SEARCH).is_none());
    }
    #[test]
    fn search_finds_saves_of_every_map_by_name_or_map_and_enter_loads_the_best() {
        let mut ui = fixture();
        add_save(&mut ui, "Slate Race", "Kitchen");
        add_save(&mut ui, "Castle", "Slate");
        add_save(&mut ui, "autosave-1", "Slate");
        let mut s = SaveLoad::new(ScreenId::LoadBricks, &ui.core);
        assert_eq!(
            rows(&s),
            ["House\t2026-09-26"],
            "a blank search lists one map"
        );
        search(&mut s, &mut ui, "  SLATE ");
        // A name starting with the words ranks above a map containing them.
        assert_eq!(
            rows(&s),
            [
                "Slate Race (Kitchen)\t2026-09-27",
                "Autosave (Slate)\t2026-09-27",
                "Castle (Slate)\t2026-09-27",
            ]
        );
        // The best match is picked and the map menu follows it.
        let menu = s.view.id("LoadBricks_MapMenu").unwrap();
        assert_eq!(s.map.as_deref(), Some("Kitchen"));
        assert_eq!(s.view.selected_text(menu).as_deref(), Some("Kitchen"));
        // The arrows walk the results; the map follows again.
        assert!(s.on_key(Key::Down, Modifiers::NONE, &mut ui.core));
        assert_eq!(s.selected().unwrap().name, "autosave-1.world.json");
        assert_eq!(s.map.as_deref(), Some("Slate"));
        assert!(s.on_key(Key::Up, Modifiers::NONE, &mut ui.core));
        ui.drain_actions();
        assert!(s.on_key(Key::Return, Modifiers::NONE, &mut ui.core));
        let actions = ui.drain_actions();
        assert!(
            matches!(&actions[..], [(_, UiAction::LoadBricks { map, name, .. })]
                if map == "Kitchen" && name == "Slate Race.world.json"),
            "{actions:?}"
        );
    }
    #[test]
    fn escape_clears_a_search_first_and_a_map_pick_ends_it() {
        let mut ui = fixture();
        add_save(&mut ui, "Garden", "Kitchen");
        let mut s = SaveLoad::new(ScreenId::LoadBricks, &ui.core);
        search(&mut s, &mut ui, "gard");
        assert_eq!(rows(&s), ["Garden (Kitchen)\t2026-09-27"]);
        search(&mut s, &mut ui, "nothing like it");
        assert!(rows(&s).is_empty());
        assert!(s.selected().is_none(), "no result, nothing to load");
        search(&mut s, &mut ui, "gard");
        assert!(s.on_key(Key::Escape, Modifiers::NONE, &mut ui.core));
        assert!(ui.core.cmds.is_empty(), "the first Escape only clears");
        assert_eq!(s.view.edit_text(s.view.id(SEARCH).unwrap()), "");
        // The cleared list is the found save's map, with it still picked.
        assert_eq!(rows(&s), ["Garden\t2026-09-27", "Table\t2026-09-25"]);
        assert_eq!(s.selected().unwrap().name, "Garden.world.json");
        search(&mut s, &mut ui, "house");
        let menu = s.view.id("LoadBricks_MapMenu").unwrap();
        s.view.select(menu, Some(1));
        s.on_event(
            &ViewEvent {
                node: menu,
                kind: EventKind::Changed,
            },
            &mut ui.core,
        );
        assert!(s.search.is_empty(), "picking a map ends the search");
        assert_eq!(rows(&s), ["Garden\t2026-09-27", "Table\t2026-09-25"]);
        assert!(s.on_key(Key::Escape, Modifiers::NONE, &mut ui.core));
        assert!(
            ui.core
                .cmds
                .iter()
                .any(|c| matches!(c, crate::ui::StackCmd::Pop(ScreenId::LoadBricks)))
        );
    }
    #[test]
    fn load_bricks_opens_the_saves_folder_old_saves_go_in() {
        let mut ui = fixture();
        let mut s = SaveLoad::new(ScreenId::LoadBricks, &ui.core);
        let node = s.view.id(OPEN_FOLDER).expect("Saves Folder button");
        assert_eq!(s.view.node(node).ctrl.text.as_deref(), Some("Saves Folder"));
        s.on_event(
            &ViewEvent {
                node,
                kind: EventKind::Click,
            },
            &mut ui.core,
        );
        let actions = ui.drain_actions();
        assert!(matches!(actions[..], [(_, UiAction::OpenSavesFolder)]), "{actions:?}");
        // Save Bricks has no such button.
        let s = SaveLoad::new(ScreenId::SaveBricks, &ui.core);
        assert!(s.view.id(OPEN_FOLDER).is_none());
    }
    #[test]
    fn filenames_are_explicit_native_leaf_names() {
        assert!(valid_name("My Build.world.json"));
        for n in [
            "house.bls",
            "../escape.world.json",
            "C:\\bad.world.json",
            "CON.world.json",
            "LPT9.world.json",
            ".world.json",
            "bad .world.json",
            "bad\n.world.json",
        ] {
            assert!(!valid_name(n), "{n}");
        }
    }
    #[test]
    fn save_takes_bare_names_and_confirms_overwrite_with_a_message_box() {
        let mut ui = fixture();
        let mut s = SaveLoad::new(ScreenId::SaveBricks, &ui.core);
        let list = s.view.id("SaveBricks_FileList").unwrap();
        assert_eq!(s.view.node(list).state.items[0].0, "House\t2026-09-26");
        s.submit(&mut ui.core);
        assert!(ui.drain_actions().is_empty(), "an empty name is refused");
        ui.core.cmds.clear();
        s.set("SaveBricks_FileName", "House");
        s.set("SaveBricks_Description", "new description");
        s.submit(&mut ui.core);
        assert!(ui.drain_actions().is_empty());
        let confirm = ui.core.cmds.iter().find_map(|c| match c {
            crate::ui::StackCmd::Message(m) => Some(m.on_yes.clone()),
            _ => None,
        });
        assert!(matches!(
            confirm,
            Some(Callback::OverwriteSave { ref name, ref description, .. })
                if name == "House.world.json" && description == "new description"
        ));
        ui.core.cmds.clear();
        let id = ui.core.request_pending(
            UiAction::SaveBricks {
                name: "House.world.json".into(),
                description: String::new(),
                events: true,
                ownership: true,
                overwrite: true,
            },
            Pending::Save,
        );
        ui.drain_actions();
        assert!(s.on_result(id, Some(&Pending::Save), &Ok(()), &mut ui.core));
        assert!(
            ui.core
                .cmds
                .iter()
                .any(|c| matches!(c, crate::ui::StackCmd::Pop(ScreenId::SaveBricks)))
        );
        ui.core.cmds.clear();
        s.set("SaveBricks_FileName", "Tower");
        s.submit(&mut ui.core);
        let actions = ui.drain_actions();
        assert!(matches!(
            &actions[0].1,
            UiAction::SaveBricks { name, overwrite: false, .. } if name == "Tower.world.json"
        ));
    }
    #[test]
    fn load_map_selection_permissions_and_pending_result() {
        let mut ui = fixture();
        let mut s = SaveLoad::new(ScreenId::LoadBricks, &ui.core);
        s.on_wake(&mut ui.core);
        assert!(matches!(
            ui.drain_actions()[0].1,
            UiAction::RequestSaveList { map: None }
        ));
        let menu = s.view.id("LoadBricks_MapMenu").unwrap();
        s.view.select(menu, Some(1));
        s.on_event(
            &ViewEvent {
                node: menu,
                kind: EventKind::Changed,
            },
            &mut ui.core,
        );
        assert_eq!(s.files[0].name, "Table.world.json");
        ui.drain_actions();
        let list = s.view.id("LoadBricks_FileList").unwrap();
        s.view.select(list, Some(0));
        ui.core.conn = ConnectionState::InGame {
            server_name: "Test".into(),
            max_players: 8,
            local: false,
            single_player: false,
            admin: false,
        };
        s.submit(&mut ui.core);
        assert!(ui.drain_actions().is_empty());
        assert!(
            ui.core
                .cmds
                .iter()
                .any(|c| matches!(c, crate::ui::StackCmd::Message(_))),
            "a refused load explains why"
        );
        ui.core.cmds.clear();
        ui.core.conn = ConnectionState::InGame {
            server_name: "Test".into(),
            max_players: 8,
            local: false,
            single_player: false,
            admin: true,
        };
        s.submit(&mut ui.core);
        let actions = ui.drain_actions();
        let (id, a) = &actions[0];
        assert!(
            matches!(a,UiAction::LoadBricks{map,name,..}if map=="Kitchen"&&name=="Table.world.json")
        );
        s.cancel(&mut ui.core);
        assert!(ui.core.cmds.is_empty());
        s.on_result(*id, None, &Ok(()), &mut ui.core);
        assert!(
            ui.core
                .cmds
                .iter()
                .any(|c| matches!(c, crate::ui::StackCmd::Pop(ScreenId::LoadBricks)))
        );
    }
}
