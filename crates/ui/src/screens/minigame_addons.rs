//! The running Add-Ons' server-wide settings (RTB's `$Pref::Server::*`
//! preferences), opened from the Admin menu. Only the host changes them;
//! they are part of its Server Settings and saved with its other
//! `$Pref::Server::*` values. Built natively: v20 had no such window.
//!
//! A mini-game's own Add-On settings and teams are in the Mini-Game window
//! ([`super::minigame_window`]).
use super::*;
use crate::api::*;
use crate::view::EventKind;
use std::collections::BTreeMap;

const W: i32 = 480;
const H: i32 = 470;
const ROW: i32 = 26;
const ROWS: &str = "AOS_Rows";
const SCROLL: &str = "AOS_Scroll";
const STATUS: &str = "AOS_Status";
const APPLY: &str = "AOS_Apply";
const FAVS: &str = "AOS_Favs";
const CATEGORY: &str = "AOS_Category";

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
fn check(r: Rect, name: &str) -> Control {
    let mut c = named(ctrl("GuiCheckBoxCtrl", "GuiCheckBoxProfile", r), name);
    c.text = Some(String::new());
    c.command = Some(name.into());
    c
}
fn edit(r: Rect, name: &str) -> Control {
    named(ctrl("GuiTextEditCtrl", "GuiTextEditProfile", r), name)
}
fn popup(r: Rect, name: &str) -> Control {
    named(ctrl("GuiPopUpMenuCtrl", "GuiPopUpMenuProfile", r), name)
}

pub struct AddOnSettings {
    view: View,
    width: i32,
    /// Effective values (defaults filled in) of the server's settings.
    values: BTreeMap<String, MiniGameSettingValue>,
    /// The host's values when the window last took them, to tell what the
    /// player changed.
    base: BTreeMap<String, MiniGameSettingValue>,
    /// Rows built: the setting each shows, and its control.
    rows: Vec<(usize, String)>,
    request: Option<RequestId>,
    /// Keep the source slot intact when its unavailable vocabulary is omitted.
    unavailable_favorite: Option<u8>,
    /// Preserve partially typed fields across asynchronous refreshes.
    typed_dirty: bool,
    seen: Option<u64>,
    category: Option<(String, String)>,
    categories: Vec<(String, String)>,
    // Raw text survives category switches even while it is not a valid
    // number. Save validates every retained field, including hidden ones.
    typed: BTreeMap<String, String>,
}

pub(super) fn value_text(v: &MiniGameSettingValue) -> String {
    match v {
        MiniGameSettingValue::Bool(b) => (if *b { "1" } else { "0" }).into(),
        MiniGameSettingValue::Int(n) => n.to_string(),
        MiniGameSettingValue::Text(t) => t.clone(),
    }
}

/// Whether a setting may hold `value` (a favourite's, kept from before).
pub(super) fn takes(s: &MiniGameAddOnSetting, value: &MiniGameSettingValue) -> bool {
    match (&s.kind, value) {
        (MiniGameSettingKind::Bool, MiniGameSettingValue::Bool(_)) => true,
        (MiniGameSettingKind::Int { min, max }, MiniGameSettingValue::Int(n))
        | (MiniGameSettingKind::PaintColor { min, max }, MiniGameSettingValue::Int(n)) => {
            (*min..=*max).contains(n)
        }
        (MiniGameSettingKind::Text { max_length }, MiniGameSettingValue::Text(t)) => {
            t.chars().count() <= *max_length as usize
        }
        (
            MiniGameSettingKind::List { items }
            | MiniGameSettingKind::Item { items }
            | MiniGameSettingKind::PlayerType { items },
            v,
        ) => items.iter().any(|(i, _)| i == v),
        _ => false,
    }
}

impl AddOnSettings {
    pub fn new(core: &mut Core) -> Self {
        let root = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480));
        let mut win = ctrl(
            "GuiWindowCtrl",
            "BlockWindowProfile",
            Rect::new((640 - W) / 2, (480 - H) / 2, W, H),
        );
        win.text = Some("Add-On Settings: Server".into());
        win.name = Some("AOS_Window".into());
        win.h_sizing = HSizing::Center;
        win.v_sizing = VSizing::Center;
        let mut scroll = named(
            ctrl(
                "GuiScrollCtrl",
                "BlockScrollProfile",
                Rect::new(12, 62, W - 24, H - 170),
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
                Rect::new(0, 0, W - 42, 10),
            ),
            ROWS,
        ));
        win.children.push(scroll);
        win.children
            .push(popup(Rect::new(12, 32, W - 24, 20), CATEGORY));
        let mut status = named(
            text("GuiMLTextProfile", Rect::new(12, H - 104, W - 24, 34), ""),
            STATUS,
        );
        status.class = "GuiMLTextCtrl".into();
        win.children.push(status);
        // Favourites: ten slots of the server's settings.
        win.children.push(named(
            text(
                "GuiTextProfile",
                Rect::new(12, H - 66, 70, 20),
                "Favourites:",
            ),
            "AOS_FavLabel",
        ));
        let mut favs = popup(Rect::new(84, H - 66, 110, 20), FAVS);
        favs.command = Some(FAVS.into());
        win.children.push(favs);
        win.children.push(push_button(
            Rect::new(198, H - 68, 56, 24),
            "Load",
            "AOS_FavLoad",
        ));
        win.children.push(push_button(
            Rect::new(258, H - 68, 56, 24),
            "Store",
            "AOS_FavSave",
        ));
        win.children.push(push_button(
            Rect::new(W - 192, H - 36, 80, 28),
            "Cancel",
            "AOS_Close",
        ));
        win.children.push(push_button(
            Rect::new(W - 108, H - 36, 94, 28),
            "Save",
            APPLY,
        ));
        let mut root = root;
        root.children.push(win);
        let mut view = View::new(&root);
        view.measure(&core.pack);
        let mut screen = Self {
            view,
            width: W,
            values: BTreeMap::new(),
            base: BTreeMap::new(),
            rows: Vec::new(),
            request: None,
            unavailable_favorite: None,
            typed_dirty: false,
            seen: None,
            category: None,
            categories: Vec::new(),
            typed: BTreeMap::new(),
        };
        screen.fill_favorites(core);
        screen.load(core);
        screen
    }

    fn fill_favorites(&mut self, core: &Core) {
        if let Some(n) = self.view.id(FAVS) {
            let current = self.view.selected(n).unwrap_or(0);
            self.view.state(n).items = (0..10u8)
                .map(|slot| {
                    let label = if core.settings.addon_favorites.contains_key(&slot) {
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
            .filter(|s| *s < 10)
            .unwrap_or(0)
    }
    /// Keep the window's settings in the picked slot.
    fn save_favorite(&mut self, core: &mut Core) {
        if let Err(e) = self.read_fields(core) {
            self.status(core, Some(&e));
            return;
        }
        let slot = self.favorite_slot();
        if self.unavailable_favorite == Some(slot) {
            self.status(
                core,
                Some("This slot has unavailable settings. Choose another slot to Store."),
            );
            core.message_ok("Favorite has unavailable settings", "The saved favorite is intact. Enable its Add-Ons before hosting and reopen this window, or choose another slot to store these settings.");
            return;
        }
        core.settings.addon_favorites.insert(
            slot,
            AddOnFavorite {
                rules: None,
                settings: self.values.clone(),
                teams: Vec::new(),
            },
        );
        core.save_settings();
        self.fill_favorites(core);
        self.status(core, Some(&format!("Saved in slot {}.", slot + 1)));
    }
    /// Fill the window from the picked slot; settings the running Add-Ons
    /// no longer declare, or values they no longer take, are left out.
    fn load_favorite(&mut self, core: &mut Core) {
        let slot = self.favorite_slot();
        let Some(fav) = core.settings.addon_favorites.get(&slot).cloned() else {
            self.status(core, Some(&format!("Slot {} is empty.", slot + 1)));
            return;
        };
        let fits = |key: &str, value: &MiniGameSettingValue| {
            Self::setting(core, key).is_some_and(|s| s.server && !s.team && takes(s, value))
        };
        let mut unavailable = Vec::new();
        for (key, value) in &fav.settings {
            if fits(key, value) {
                self.values.insert(key.clone(), value.clone());
            } else {
                unavailable.push(key.clone());
            }
        }
        self.unavailable_favorite = (!unavailable.is_empty()).then_some(slot);
        self.rows.clear();
        self.typed.clear();
        self.build(core);
        self.status(
            core,
            Some(&if unavailable.is_empty() {
                format!("Loaded slot {}. Save to use it.", slot + 1)
            } else {
                format!("Loaded compatible settings from slot {}. Unavailable settings remain in that slot.", slot + 1)
            }),
        );
        if !unavailable.is_empty() {
            let keys = unavailable
                .iter()
                .take(12)
                .map(|key| key.replace(['<', '>'], ""))
                .collect::<Vec<_>>()
                .join("\n");
            core.message_ok("Favorite has unavailable settings", &format!("These settings are not offered here, or their values are no longer accepted:\n\n{keys}\n\nThe saved favorite is intact. Save applies only compatible settings. Enable its Add-Ons before hosting to recover them. Choose another slot to Store these settings."));
        }
    }

    fn editable(core: &Core) -> bool {
        core.admin
            .available(crate::models::admin::AdminFeature::HostOptions)
            && core
                .admin
                .snapshot
                .as_ref()
                .is_some_and(|s| s.local_host && s.options.is_some())
    }
    /// The host's server-wide values, when it shows them to this player.
    fn server_values(core: &Core) -> Option<&BTreeMap<String, MiniGameSettingValue>> {
        core.admin
            .snapshot
            .as_ref()
            .and_then(|s| s.options.as_ref())
            .map(|o| &o.addon_settings)
    }
    fn setting<'a>(core: &'a Core, key: &str) -> Option<&'a MiniGameAddOnSetting> {
        core.minigames.addon_settings.iter().find(|s| s.key == key)
    }
    fn mine(s: &MiniGameAddOnSetting) -> bool {
        s.server && !s.team
    }

    /// Take the host's values afresh.
    fn load(&mut self, core: &Core) {
        self.seen = Some(core.admin.revision);
        self.rows.clear();
        self.typed.clear();
        self.typed_dirty = false;
        self.unavailable_favorite = None;
        let stored = Self::server_values(core);
        let values: BTreeMap<_, _> = core
            .minigames
            .addon_settings
            .iter()
            .filter(|s| Self::mine(s))
            .map(|s| {
                let v = stored.and_then(|m| m.get(&s.key)).cloned();
                (s.key.clone(), v.unwrap_or_else(|| s.default.clone()))
            })
            .collect();
        self.values = values.clone();
        self.base = values;
        self.build(core);
    }

    fn capture_fields(&mut self) {
        for (_, name) in &self.rows {
            let Some(n) = self.view.id(name) else {
                continue;
            };
            if self.view.node(n).ctrl.class != "GuiTextEditCtrl" {
                continue;
            }
            // The stable setting key is stored on the control, since incoming
            // catalogs can reorder indexes while an existing draft is open.
            if let Some(key) = self.view.node(n).ctrl.field("settingKey") {
                self.typed.insert(key.into(), self.view.edit_text(n));
            }
        }
    }

    /// One category of settings at a time; switching keeps what was typed.
    fn build(&mut self, core: &Core) {
        self.capture_fields();
        let focus = self.view.focus.and_then(|n| {
            self.view
                .node(n)
                .ctrl
                .name
                .clone()
                .map(|name| (name, self.view.node(n).state.cursor))
        });
        let Some(rows) = self.view.id(ROWS) else {
            return;
        };
        self.view.clear_children(rows);
        self.rows.clear();
        let editable = Self::editable(core);
        let width = self.width - 42;
        let mut y = 4;
        let heading = |view: &mut View, y: &mut i32, label: &str| {
            view.add(
                rows,
                text("GuiTextProfile", Rect::new(4, *y, width - 8, 22), label),
            );
            *y += 24;
        };
        let settings = core.minigames.addon_settings.clone();
        self.categories.clear();
        for s in settings.iter().filter(|s| Self::mine(s)) {
            let group = (s.add_on.clone(), s.category.clone());
            if !self.categories.contains(&group) {
                self.categories.push(group);
            }
        }
        if !self
            .category
            .as_ref()
            .is_some_and(|g| self.categories.contains(g))
        {
            self.category = self.categories.first().cloned();
        }
        if let Some(n) = self.view.id(CATEGORY) {
            self.view.state(n).items = self
                .categories
                .iter()
                .enumerate()
                .map(|(i, (addon, category))| {
                    (
                        if category.is_empty() {
                            addon.clone()
                        } else {
                            format!("{addon}: {category}")
                        },
                        i as i64,
                    )
                })
                .collect();
            self.view.select(
                n,
                self.category
                    .as_ref()
                    .and_then(|g| self.categories.iter().position(|v| v == g))
                    .map(|i| i as i64),
            );
            self.view.set_visible(n, !self.categories.is_empty());
            self.view.set_active(n, true);
        }
        for (i, s) in settings.iter().enumerate() {
            if !Self::mine(s)
                || self.category.as_ref() != Some(&(s.add_on.clone(), s.category.clone()))
            {
                continue;
            }
            let value = self
                .values
                .get(&s.key)
                .cloned()
                .unwrap_or_else(|| s.default.clone());
            self.row(i, s, &value, y, editable, core);
            y += ROW;
        }
        if self.categories.is_empty() {
            heading(&mut self.view, &mut y, "No Add-On settings.");
        }
        if settings.iter().any(|s| Self::mine(s) && s.restart) {
            heading(&mut self.view, &mut y, RESTART_NOTE);
        }
        self.view.nodes[rows].ctrl.extent[1] = y + 4;
        self.view.relayout();
        if let Some((name, cursor)) = focus
            && let Some(n) = self.view.id(&name)
            && self.view.node(n).state.active
            && self.view.node(n).state.visible
        {
            self.view.focus = Some(n);
            self.view.state(n).cursor = cursor.min(self.view.edit_text(n).chars().count());
        }
        self.status(core, None);
    }

    /// One setting's label and control at `y`.
    fn row(
        &mut self,
        i: usize,
        s: &MiniGameAddOnSetting,
        value: &MiniGameSettingValue,
        y: i32,
        editable: bool,
        core: &Core,
    ) {
        let Some(rows) = self.view.id(ROWS) else {
            return;
        };
        let name = format!("AOS_S{i}");
        let width = self.width - 42;
        self.view.add(
            rows,
            text("GuiTextProfile", Rect::new(20, y, 200, 20), &title(s)),
        );
        let control = Rect::new(230, y, width - 266, 20);
        let n = match &s.kind {
            MiniGameSettingKind::Bool => {
                let n = self.view.add(rows, check(Rect::new(230, y, 20, 20), &name));
                self.view
                    .set_bool(n, value == &MiniGameSettingValue::Bool(true));
                n
            }
            MiniGameSettingKind::Int { .. } | MiniGameSettingKind::Text { .. } => {
                let n = self.view.add(rows, edit(control, &name));
                self.view.set_text(
                    n,
                    self.typed
                        .get(&s.key)
                        .cloned()
                        .unwrap_or_else(|| value_text(value)),
                );
                n
            }
            MiniGameSettingKind::PaintColor { min, max } => {
                let n = self.view.add(rows, popup(Rect::new(230, y, 90, 20), &name));
                let top = (*max).min(core.minigames.palette.len() as i64 - 1);
                self.view.state(n).items = (*min..=top)
                    .map(|c| {
                        let label = if c < 0 {
                            "None".to_owned()
                        } else {
                            format!("Colour {}", c + 1)
                        };
                        (label, c)
                    })
                    .collect();
                let current = match value {
                    MiniGameSettingValue::Int(c) => *c,
                    _ => -1,
                };
                self.view.select(n, Some(current));
                if let Some(rgb) = usize::try_from(current)
                    .ok()
                    .and_then(|c| core.minigames.palette.get(c))
                {
                    self.view.add(
                        rows,
                        swatch(
                            Rect::new(324, y + 2, 16, 16),
                            rgba([
                                f32::from(rgb[0]) / 255.0,
                                f32::from(rgb[1]) / 255.0,
                                f32::from(rgb[2]) / 255.0,
                                1.0,
                            ]),
                        ),
                    );
                }
                n
            }
            MiniGameSettingKind::List { items }
            | MiniGameSettingKind::Item { items }
            | MiniGameSettingKind::PlayerType { items } => {
                let n = self.view.add(rows, popup(control, &name));
                self.view.state(n).items = items
                    .iter()
                    .enumerate()
                    .map(|(i, (_, label))| (label.clone(), i as i64))
                    .collect();
                self.view.select(
                    n,
                    items.iter().position(|(v, _)| v == value).map(|i| i as i64),
                );
                n
            }
        };
        self.view.nodes[n]
            .ctrl
            .fields
            .insert("settingKey".into(), s.key.clone());
        self.view.set_active(n, editable);
        if !s.help.is_empty() {
            self.view.add(
                rows,
                push_button(
                    Rect::new(width - 30, y - 1, 22, 22),
                    "?",
                    &format!("AOS_H{i}"),
                ),
            );
        }
        self.rows.push((i, name));
    }

    /// Validate visible and retained hidden text together before committing it
    /// to the draft. Browsing away never silently replaces invalid input.
    fn read_fields(&mut self, core: &Core) -> Result<(), String> {
        self.capture_fields();
        let mut values = Vec::new();
        for (key, text) in &self.typed {
            let Some(s) = Self::setting(core, key) else {
                continue;
            };
            let value = match &s.kind {
                MiniGameSettingKind::Int { min, max } => {
                    let n: i64 = text
                        .trim()
                        .parse()
                        .map_err(|_| format!("{} must be a whole number.", s.title))?;
                    if !(*min..=*max).contains(&n) {
                        return Err(format!("{} must be {min} to {max}.", s.title));
                    }
                    MiniGameSettingValue::Int(n)
                }
                MiniGameSettingKind::Text { max_length } => {
                    if text.chars().count() > *max_length as usize {
                        return Err(format!("{} is at most {max_length} characters.", s.title));
                    }
                    MiniGameSettingValue::Text(text.clone())
                }
                _ => continue,
            };
            values.push((key.clone(), value));
        }
        self.values.extend(values);
        Ok(())
    }

    /// What Save sends: settings that differ from the host's (`None`: back
    /// to the default).
    fn changes(&self, core: &Core) -> Vec<(String, Option<MiniGameSettingValue>)> {
        self.values
            .iter()
            .filter(|(k, v)| self.base.get(*k) != Some(*v))
            .map(|(k, v)| {
                let default = Self::setting(core, k).map(|s| &s.default);
                (k.clone(), (default != Some(v)).then(|| v.clone()))
            })
            .collect()
    }

    /// What waits while a change is on its way to the host.
    fn mutation_control(name: &str) -> bool {
        matches!(name, FAVS | "AOS_FavLoad" | "AOS_FavSave" | APPLY)
            || name
                .strip_prefix("AOS_S")
                .and_then(|suffix| suffix.chars().next())
                .is_some_and(|c| c.is_ascii_digit())
    }
    fn status(&mut self, core: &Core, message: Option<&str>) {
        let editable = Self::editable(core);
        let changed = self.typed_dirty || !self.changes(core).is_empty();
        let text = match message {
            Some(m) => m.to_owned(),
            None if !core.admin.status.is_empty() && self.request.is_some() => {
                core.admin.status.clone()
            }
            None if !editable => "Only the host can change these.".into(),
            None if changed => "Not applied yet.".into(),
            None => String::new(),
        };
        if let Some(n) = self.view.id(STATUS) {
            self.view.set_text(n, text.replace(['<', '>'], ""));
        }
        let ready = editable && self.request.is_none();
        for node in self.view.walk().collect::<Vec<_>>() {
            let name = self.view.node(node).ctrl.name.as_deref().unwrap_or("");
            if !Self::mutation_control(name) {
                continue;
            }
            let favorite = matches!(name, FAVS | "AOS_FavLoad" | "AOS_FavSave");
            self.view.set_active(
                node,
                if favorite {
                    self.request.is_none()
                } else {
                    ready
                },
            );
        }
        // View text input follows its focus. Clear stale edit/popup captures
        // when their controls have just become inactive.
        if self
            .view
            .focus
            .is_some_and(|node| !self.view.node(node).state.active)
        {
            self.view.focus = None;
        }
        if self
            .view
            .open_popup_node()
            .is_some_and(|node| !self.view.node(node).state.active)
        {
            self.view.close_popup();
        }
        if let Some(n) = self.view.id(APPLY) {
            self.view.set_visible(n, editable);
        }
    }

    fn apply(&mut self, core: &mut Core) {
        if let Err(e) = self.read_fields(core) {
            self.status(core, Some(&e));
            return;
        }
        let settings = self.changes(core);
        if settings.is_empty() {
            self.status(core, Some("Nothing has changed."));
            return;
        }
        // Server-wide changes go with the host's Server Settings, saved as
        // its `$Pref::Server::*` like the rest.
        let Some(mut options) = core.admin.snapshot.as_ref().and_then(|s| s.options.clone()) else {
            self.status(core, Some("Host settings unavailable."));
            return;
        };
        for (key, value) in settings {
            match value {
                Some(v) => options.addon_settings.insert(key, v),
                None => options.addon_settings.remove(&key),
            };
        }
        crate::models::admin::options_to_prefs(&options, &mut core.prefs);
        core.save_settings();
        self.request = core.admin_request(crate::models::admin::AdminAction::ConfigureHost {
            options: Box::new(options),
        });
        let status = core.admin.status.clone();
        self.status(core, Some(&status));
    }

    fn close(core: &mut Core) {
        core.server_addon_settings = false;
        core.pop(ScreenId::MiniGameAddOns);
    }
}

impl Screen for AddOnSettings {
    fn id(&self) -> ScreenId {
        ScreenId::MiniGameAddOns
    }
    fn view(&self) -> &View {
        &self.view
    }
    fn view_mut(&mut self) -> &mut View {
        &mut self.view
    }
    fn layout(&mut self, w: i32, h: i32, core: &mut Core) {
        let _ = self.read_fields(core);
        let width = (w - 20).clamp(380, W);
        let height = (h - 10).clamp(280, H);
        self.width = width;
        if let Some(n) = self.view.id("AOS_Window") {
            let c = &mut self.view.nodes[n].ctrl;
            c.position = [(w - width) / 2, (h - height) / 2];
            c.extent = [width, height];
            c.h_sizing = HSizing::Right;
            c.v_sizing = VSizing::Bottom;
        }
        let footer = 140;
        let fav_y = height - 66;
        let positions = [
            (SCROLL, Rect::new(12, 62, width - 24, height - footer - 30)),
            (CATEGORY, Rect::new(12, 32, width - 24, 20)),
            (STATUS, Rect::new(12, height - footer + 36, width - 24, 26)),
            ("AOS_FavLabel", Rect::new(12, fav_y, 70, 20)),
            (FAVS, Rect::new(84, fav_y, 110, 20)),
            ("AOS_FavLoad", Rect::new(198, fav_y - 2, 56, 24)),
            ("AOS_FavSave", Rect::new(258, fav_y - 2, 56, 24)),
            ("AOS_Close", Rect::new(width - 192, height - 36, 80, 28)),
            (APPLY, Rect::new(width - 108, height - 36, 94, 28)),
            (ROWS, Rect::new(0, 0, width - 42, 10)),
        ];
        for (name, rect) in positions {
            if let Some(n) = self.view.id(name) {
                self.view.nodes[n].ctrl.position = [rect.x, rect.y];
                self.view.nodes[n].ctrl.extent = [rect.w, rect.h];
            }
        }
        self.build(core);
        self.view.measure(&core.pack);
        self.view.layout(w, h);
    }
    fn blocks_accelerators(&self) -> bool {
        true
    }
    fn on_sleep(&mut self, core: &mut Core) {
        if let Some(id) = self.request.take() {
            core.abandon(id);
        }
    }
    fn on_update(&mut self, core: &mut Core) {
        // A server change is answered through the admin state: done once
        // the host no longer has it pending.
        if let Some(id) = self.request
            && !core.admin.pending.contains_key(&id)
        {
            self.request = None;
            if core.admin.status.starts_with("Rejected") {
                let status = core.admin.status.clone();
                self.status(core, Some(&status));
            } else {
                self.typed_dirty = false;
                self.base = self.values.clone();
                self.status(core, Some("Applied."));
            }
        }
        // The host's values changed (someone else applied, or ours landed):
        // start again from them, unless the player is mid-edit.
        if self.seen != Some(core.admin.revision) {
            if self.request.is_none() && !self.typed_dirty && self.changes(core).is_empty() {
                self.load(core);
            } else {
                self.seen = Some(core.admin.revision);
                self.build(core);
            }
        }
    }
    fn on_key(&mut self, key: Key, _: Modifiers, core: &mut Core) -> bool {
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
        if self.request.is_some() && Self::mutation_control(&name) {
            return;
        }
        let row = self.rows.iter().find(|(_, n)| *n == name).map(|(i, _)| *i);
        if ev.kind == EventKind::Changed {
            if name == CATEGORY {
                self.capture_fields();
                self.category = self
                    .view
                    .selected(ev.node)
                    .and_then(|i| usize::try_from(i).ok())
                    .and_then(|i| self.categories.get(i))
                    .cloned();
                if let Some(n) = self.view.id(SCROLL) {
                    self.view.state(n).scroll_y = 0;
                }
                self.build(core);
                return;
            }
            if name == FAVS {
                return;
            }
            if self.view.node(ev.node).ctrl.class == "GuiTextEditCtrl" {
                self.typed_dirty = true;
                let _ = self.read_fields(core);
            }
            // A pick from a list, or a colour.
            if let Some(i) = row
                && let Some(s) = core.minigames.addon_settings.get(i).cloned()
            {
                let picked = self.view.selected(ev.node);
                let value = match &s.kind {
                    MiniGameSettingKind::PaintColor { .. } => picked.map(MiniGameSettingValue::Int),
                    MiniGameSettingKind::List { items }
                    | MiniGameSettingKind::Item { items }
                    | MiniGameSettingKind::PlayerType { items } => picked
                        .and_then(|i| usize::try_from(i).ok())
                        .and_then(|i| items.get(i))
                        .map(|(v, _)| v.clone()),
                    _ => None,
                };
                if let Some(value) = value {
                    let _ = self.read_fields(core);
                    self.values.insert(s.key.clone(), value);
                    self.build(core);
                }
            }
            self.status(core, None);
            return;
        }
        if !matches!(ev.kind, EventKind::Click | EventKind::Submit) {
            return;
        }
        let command = command_of(&self.view, ev.node);
        match command.as_str() {
            "AOS_Close" => Self::close(core),
            APPLY => self.apply(core),
            "AOS_FavSave" => self.save_favorite(core),
            "AOS_FavLoad" => self.load_favorite(core),
            _ => {
                if let Some(i) = command
                    .strip_prefix("AOS_H")
                    .and_then(|i| i.parse::<usize>().ok())
                {
                    if let Some(s) = core.minigames.addon_settings.get(i) {
                        let (title, help) = (s.title.clone(), s.help.clone());
                        core.message_ok(&title, &help);
                    }
                } else if let Some(i) = row
                    && let Some(s) = core.minigames.addon_settings.get(i).cloned()
                    && s.kind == MiniGameSettingKind::Bool
                {
                    let on = self.view.bool_value(ev.node);
                    let _ = self.read_fields(core);
                    self.values
                        .insert(s.key.clone(), MiniGameSettingValue::Bool(on));
                    self.build(core);
                }
            }
        }
    }
}

/// What marks a setting the game reads only as it starts.
pub const RESTART_NOTE: &str = "* Takes effect when the server starts or loads a map.";

/// A setting's label: its title, marked when a change waits for the next
/// start ([`RESTART_NOTE`]).
pub(super) fn title(s: &MiniGameAddOnSetting) -> String {
    if s.restart {
        format!("{} *", s.title)
    } else {
        s.title.clone()
    }
}
