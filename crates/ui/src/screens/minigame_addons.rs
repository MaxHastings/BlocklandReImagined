//! A mini-game's Add-On Settings: the settings running Add-Ons declare
//! (Slayer's game mode, lives, points, time and the rest), and its team
//! list when an Add-On uses teams. Built natively: v20 had no such window,
//! and Slayer brought its own GUI. The host sends each setting's kind,
//! range and choices, checks every change again and says who may make it.
use super::*;
use crate::api::*;
use crate::view::EventKind;
use std::collections::BTreeMap;

/// Settings by key (`None`: back to the default), and the team list.
type Changes = (
    Vec<(String, Option<MiniGameSettingValue>)>,
    Option<Vec<MiniGameTeamEdit>>,
);

const W: i32 = 460;
const H: i32 = 440;
const ROW: i32 = 26;
const ROWS: &str = "AOS_Rows";
const SCROLL: &str = "AOS_Scroll";
const STATUS: &str = "AOS_Status";
const APPLY: &str = "AOS_Apply";

fn named(mut c: Control, name: &str) -> Control {
    c.name = Some(name.into());
    c
}
fn push_button(r: Rect, label: &str, name: &str) -> Control {
    named(
        button("BlockButtonProfile", r, "base/client/ui/button1", label, name),
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

/// One team as the window leaves it.
#[derive(Debug, Clone, PartialEq)]
struct DraftTeam {
    id: Option<u32>,
    name: String,
    color: u8,
    settings: BTreeMap<String, MiniGameSettingValue>,
}

/// What a row's controls edit.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Target {
    Game(usize),
    /// Team (by place in the draft), setting.
    Team(usize, usize),
}

pub struct AddOnSettings {
    view: View,
    game: Option<MiniGameId>,
    /// Effective values (defaults filled in) of the game's own settings.
    values: BTreeMap<String, MiniGameSettingValue>,
    teams: Vec<DraftTeam>,
    /// The host's values when the window last took them, to tell what the
    /// player changed.
    base: (BTreeMap<String, MiniGameSettingValue>, Vec<DraftTeam>),
    /// Rows built, and the control each setting has.
    rows: Vec<(Target, String)>,
    request: Option<RequestId>,
    seen: Option<u64>,
}

fn value_text(v: &MiniGameSettingValue) -> String {
    match v {
        MiniGameSettingValue::Bool(b) => (if *b { "1" } else { "0" }).into(),
        MiniGameSettingValue::Int(n) => n.to_string(),
        MiniGameSettingValue::Text(t) => t.clone(),
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
        win.text = Some("Add-On Settings".into());
        win.name = Some("AOS_Window".into());
        win.h_sizing = HSizing::Center;
        win.v_sizing = VSizing::Center;
        let mut scroll = named(
            ctrl("GuiScrollCtrl", "BlockScrollProfile", Rect::new(12, 32, W - 24, H - 110)),
            SCROLL,
        );
        scroll.fields.insert("hScrollBar".into(), "alwaysOff".into());
        scroll.fields.insert("vScrollBar".into(), "dynamic".into());
        scroll.children.push(named(
            ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, W - 42, 10)),
            ROWS,
        ));
        win.children.push(scroll);
        let mut status = named(
            text("GuiMLTextProfile", Rect::new(12, H - 72, W - 24, 34), ""),
            STATUS,
        );
        status.class = "GuiMLTextCtrl".into();
        win.children.push(status);
        win.children
            .push(push_button(Rect::new(W - 220, H - 36, 98, 28), "Close", "AOS_Close"));
        win.children
            .push(push_button(Rect::new(W - 112, H - 36, 98, 28), "Apply", APPLY));
        let mut root = root;
        root.children.push(win);
        let mut view = View::new(&root);
        view.measure(&core.pack);
        let mut screen = Self {
            view,
            game: core.minigame_addons,
            values: BTreeMap::new(),
            teams: Vec::new(),
            base: (BTreeMap::new(), Vec::new()),
            rows: Vec::new(),
            request: None,
            seen: None,
        };
        screen.load(core);
        screen
    }

    fn summary<'a>(&self, core: &'a Core) -> Option<&'a MiniGameSummary> {
        self.game
            .and_then(|id| core.minigames.games.iter().find(|g| g.id == id))
    }
    fn editable(&self, core: &Core) -> bool {
        self.game
            .is_some_and(|g| core.minigames.addon_editable.contains(&g))
    }
    fn setting<'a>(core: &'a Core, key: &str) -> Option<&'a MiniGameAddOnSetting> {
        core.minigames.addon_settings.iter().find(|s| s.key == key)
    }
    fn team_setup(core: &Core) -> bool {
        core.minigames.addon_settings.iter().any(|s| s.team)
    }

    /// Take the host's values afresh.
    fn load(&mut self, core: &Core) {
        self.seen = Some(core.minigames.revision);
        let Some(g) = self.summary(core) else {
            self.values.clear();
            self.teams.clear();
            self.base = (BTreeMap::new(), Vec::new());
            self.build(core);
            return;
        };
        let mut values = BTreeMap::new();
        for s in core.minigames.addon_settings.iter().filter(|s| !s.team) {
            let v = g.addon_settings.get(&s.key).cloned().unwrap_or_else(|| s.default.clone());
            values.insert(s.key.clone(), v);
        }
        let teams: Vec<DraftTeam> = g
            .teams
            .iter()
            .map(|t| DraftTeam {
                id: Some(t.id),
                name: t.name.clone(),
                color: t.color,
                settings: core
                    .minigames
                    .addon_settings
                    .iter()
                    .filter(|s| s.team)
                    .map(|s| {
                        (
                            s.key.clone(),
                            t.settings.get(&s.key).cloned().unwrap_or_else(|| s.default.clone()),
                        )
                    })
                    .collect(),
            })
            .collect();
        if let Some(n) = self.view.id("AOS_Window") {
            self.view.set_text(n, format!("Add-On Settings: {}", g.title));
        }
        self.values = values.clone();
        self.teams = teams.clone();
        self.base = (values, teams);
        self.build(core);
    }

    /// Whether a setting shows, given the draft's other values.
    fn shown(&self, core: &Core, s: &MiniGameAddOnSetting, team: Option<usize>) -> bool {
        let Some((key, values)) = &s.shown_when else {
            return true;
        };
        let current = self
            .values
            .get(key)
            .or_else(|| team.and_then(|t| self.teams.get(t)).and_then(|t| t.settings.get(key)))
            .cloned()
            .or_else(|| Self::setting(core, key).map(|d| d.default.clone()));
        current.is_some_and(|v| values.contains(&v))
    }

    /// Lay the rows out again from the draft.
    fn build(&mut self, core: &Core) {
        let Some(rows) = self.view.id(ROWS) else {
            return;
        };
        self.view.clear_children(rows);
        self.rows.clear();
        let editable = self.editable(core);
        let width = W - 42;
        let mut y = 4;
        let heading = |view: &mut View, y: &mut i32, label: &str| {
            view.add(
                rows,
                text("GuiBigTextProfile", Rect::new(4, *y, width - 8, 22), label),
            );
            *y += 24;
        };
        if self.summary(core).is_none() {
            heading(&mut self.view, &mut y, "That mini-game has ended.");
        }
        let settings = core.minigames.addon_settings.clone();
        let mut last_group = (String::new(), String::new());
        for (i, s) in settings.iter().enumerate() {
            if s.team || self.summary(core).is_none() || !self.shown(core, s, None) {
                continue;
            }
            if last_group.0 != s.add_on {
                heading(&mut self.view, &mut y, &s.add_on);
                last_group = (s.add_on.clone(), String::new());
            }
            if last_group.1 != s.category && !s.category.is_empty() {
                self.view.add(
                    rows,
                    text("GuiTextProfile", Rect::new(8, y, width - 16, 20), &format!("{}:", s.category)),
                );
                y += 22;
                last_group.1 = s.category.clone();
            }
            let value = self.values.get(&s.key).cloned().unwrap_or_else(|| s.default.clone());
            self.row(Target::Game(i), s, &value, 20, y, editable, core);
            y += ROW;
        }
        if Self::team_setup(core) && self.summary(core).is_some() {
            heading(&mut self.view, &mut y, "Teams");
            for t in 0..self.teams.len() {
                let team = self.teams[t].clone();
                let name = format!("AOS_T{t}_Name");
                let n = self.view.add(rows, edit(Rect::new(20, y, 170, 20), &name));
                self.view.set_text(n, team.name.clone());
                self.view.set_active(n, editable);
                let color = format!("AOS_T{t}_Color");
                let n = self.view.add(rows, popup(Rect::new(196, y, 90, 20), &color));
                self.view.state(n).items = (0..core.minigames.palette.len().min(64))
                    .map(|c| (format!("Colour {}", c + 1), c as i64))
                    .collect();
                self.view.select(n, Some(i64::from(team.color)));
                self.view.set_active(n, editable);
                let rgb = core
                    .minigames
                    .palette
                    .get(usize::from(team.color))
                    .copied()
                    .unwrap_or([255; 3]);
                self.view.add(
                    rows,
                    named(
                        swatch(Rect::new(290, y + 2, 16, 16), rgba([
                            f32::from(rgb[0]) / 255.0,
                            f32::from(rgb[1]) / 255.0,
                            f32::from(rgb[2]) / 255.0,
                            1.0,
                        ])),
                        &format!("AOS_T{t}_Swatch"),
                    ),
                );
                if editable {
                    let remove = format!("AOS_T{t}_Remove");
                    self.view
                        .add(rows, push_button(Rect::new(width - 84, y - 2, 76, 24), "Remove", &remove));
                }
                y += ROW;
                for (i, s) in settings.iter().enumerate() {
                    if !s.team || !self.shown(core, s, Some(t)) {
                        continue;
                    }
                    let value = team.settings.get(&s.key).cloned().unwrap_or_else(|| s.default.clone());
                    self.row(Target::Team(t, i), s, &value, 36, y, editable, core);
                    y += ROW;
                }
                y += 6;
            }
            if editable {
                self.view
                    .add(rows, push_button(Rect::new(20, y, 110, 24), "Add Team", "AOS_AddTeam"));
                y += ROW + 4;
            }
        }
        if settings.is_empty() {
            heading(&mut self.view, &mut y, "No running Add-On has settings.");
        }
        self.view.nodes[rows].ctrl.extent[1] = y + 4;
        self.view.relayout();
        self.status(core, None);
    }

    /// One setting's label and control at `y`.
    #[allow(clippy::too_many_arguments)]
    fn row(
        &mut self,
        target: Target,
        s: &MiniGameAddOnSetting,
        value: &MiniGameSettingValue,
        x: i32,
        y: i32,
        editable: bool,
        core: &Core,
    ) {
        let Some(rows) = self.view.id(ROWS) else {
            return;
        };
        let name = match target {
            Target::Game(i) => format!("AOS_S{i}"),
            Target::Team(t, i) => format!("AOS_T{t}_S{i}"),
        };
        let width = W - 42;
        self.view.add(
            rows,
            text("GuiTextProfile", Rect::new(x, y, 200 - x + 20, 20), &s.title),
        );
        let control = Rect::new(230, y, width - 238, 20);
        let n = match &s.kind {
            MiniGameSettingKind::Bool => {
                let n = self.view.add(rows, check(Rect::new(230, y, 20, 20), &name));
                self.view.set_bool(n, value == &MiniGameSettingValue::Bool(true));
                n
            }
            MiniGameSettingKind::Int { .. } | MiniGameSettingKind::Text { .. } => {
                let n = self.view.add(rows, edit(control, &name));
                self.view.set_text(n, value_text(value));
                n
            }
            MiniGameSettingKind::PaintColor { min, max } => {
                let n = self.view.add(rows, popup(Rect::new(230, y, 90, 20), &name));
                let top = (*max).min(core.minigames.palette.len() as i64 - 1);
                self.view.state(n).items = (*min..=top)
                    .map(|c| {
                        let label = if c < 0 { "None".to_owned() } else { format!("Colour {}", c + 1) };
                        (label, c)
                    })
                    .collect();
                let current = match value {
                    MiniGameSettingValue::Int(c) => *c,
                    _ => -1,
                };
                self.view.select(n, Some(current));
                if let Some(rgb) = usize::try_from(current).ok().and_then(|c| core.minigames.palette.get(c)) {
                    self.view.add(
                        rows,
                        swatch(Rect::new(324, y + 2, 16, 16), rgba([
                            f32::from(rgb[0]) / 255.0,
                            f32::from(rgb[1]) / 255.0,
                            f32::from(rgb[2]) / 255.0,
                            1.0,
                        ])),
                    );
                }
                n
            }
            MiniGameSettingKind::List { items } => {
                let n = self.view.add(rows, popup(control, &name));
                self.view.state(n).items = items
                    .iter()
                    .enumerate()
                    .map(|(i, (_, label))| (label.clone(), i as i64))
                    .collect();
                self.view
                    .select(n, items.iter().position(|(v, _)| v == value).map(|i| i as i64));
                n
            }
        };
        let admin_only = s.admin_only && !core.minigames.members.iter().any(|m| {
            Some(m.id) == core.minigames.local_player && m.admin
        });
        self.view.set_active(n, editable && !admin_only);
        self.rows.push((target, name));
    }

    /// Read typed fields (numbers, text, team names) into the draft.
    fn read_fields(&mut self, core: &Core) -> Result<(), String> {
        for (target, name) in self.rows.clone() {
            let Some(n) = self.view.id(&name) else { continue };
            let i = match target {
                Target::Game(i) | Target::Team(_, i) => i,
            };
            let Some(s) = core.minigames.addon_settings.get(i) else { continue };
            let value = match &s.kind {
                MiniGameSettingKind::Int { min, max } => {
                    let text = self.view.edit_text(n);
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
                    let text = self.view.edit_text(n);
                    if text.chars().count() > *max_length as usize {
                        return Err(format!("{} is at most {max_length} characters.", s.title));
                    }
                    MiniGameSettingValue::Text(text)
                }
                _ => continue,
            };
            self.set(target, &s.key.clone(), value);
        }
        for t in 0..self.teams.len() {
            if let Some(n) = self.view.id(&format!("AOS_T{t}_Name")) {
                let name = self.view.edit_text(n);
                if name.trim().is_empty() || name.chars().count() > 50 {
                    return Err("A team's name is 1 to 50 characters.".into());
                }
                self.teams[t].name = name;
            }
        }
        Ok(())
    }
    fn set(&mut self, target: Target, key: &str, value: MiniGameSettingValue) {
        match target {
            Target::Game(_) => {
                self.values.insert(key.to_owned(), value);
            }
            Target::Team(t, _) => {
                if let Some(team) = self.teams.get_mut(t) {
                    team.settings.insert(key.to_owned(), value);
                }
            }
        }
    }

    /// What Apply sends: settings that differ from the host's, and the
    /// whole team list when anything about the teams changed.
    fn changes(&self, core: &Core) -> Changes {
        let to_send = |key: &str, value: &MiniGameSettingValue| {
            let default = Self::setting(core, key).map(|s| &s.default);
            if default == Some(value) {
                None
            } else {
                Some(value.clone())
            }
        };
        let settings = self
            .values
            .iter()
            .filter(|(k, v)| self.base.0.get(*k) != Some(*v))
            .map(|(k, v)| (k.clone(), to_send(k, v)))
            .collect();
        let teams = (self.teams != self.base.1).then(|| {
            self.teams
                .iter()
                .map(|t| {
                    let before = t
                        .id
                        .and_then(|id| self.base.1.iter().find(|b| b.id == Some(id)));
                    MiniGameTeamEdit {
                        id: t.id,
                        name: t.name.trim().to_owned(),
                        color: t.color,
                        settings: t
                            .settings
                            .iter()
                            .filter(|(k, v)| before.and_then(|b| b.settings.get(*k)) != Some(*v))
                            .map(|(k, v)| (k.clone(), to_send(k, v)))
                            .collect(),
                    }
                })
                .collect()
        });
        (settings, teams)
    }

    fn status(&mut self, core: &Core, message: Option<&str>) {
        let editable = self.editable(core);
        let (settings, teams) = self.changes(core);
        let changed = !settings.is_empty() || teams.is_some();
        let text = match message {
            Some(m) => m.to_owned(),
            None if !core.minigames.status.is_empty() && self.request.is_some() => {
                core.minigames.status.clone()
            }
            None if !editable && self.summary(core).is_some() => {
                "Only the mini-game's owner or an admin can change these.".into()
            }
            None if changed => "Not applied yet.".into(),
            None => String::new(),
        };
        if let Some(n) = self.view.id(STATUS) {
            self.view.set_text(n, text.replace(['<', '>'], ""));
        }
        if let Some(n) = self.view.id(APPLY) {
            self.view
                .set_active(n, editable && self.request.is_none() && self.summary(core).is_some());
            self.view.set_visible(n, editable);
        }
    }

    fn apply(&mut self, core: &mut Core) {
        if let Err(e) = self.read_fields(core) {
            self.status(core, Some(&e));
            return;
        }
        let Some(game) = self.game else { return };
        let (settings, teams) = self.changes(core);
        if settings.is_empty() && teams.is_none() {
            self.status(core, Some("Nothing has changed."));
            return;
        }
        self.request = core.minigame_request(
            MiniGameOperation::AddOnSettings,
            UiAction::EditMiniGameAddOns {
                game,
                settings,
                teams,
                quiet: false,
                reset: false,
            },
        );
        let status = core.minigames.status.clone();
        self.status(core, Some(&status));
    }

    fn close(core: &mut Core) {
        core.minigame_addons = None;
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
    fn blocks_accelerators(&self) -> bool {
        true
    }
    fn on_sleep(&mut self, core: &mut Core) {
        if let Some(id) = self.request.take() {
            core.pending.remove(&id);
        }
    }
    fn on_update(&mut self, core: &mut Core) {
        // The host's values changed (someone else applied, or ours landed):
        // start again from them, unless the player is mid-edit.
        if self.seen != Some(core.minigames.revision) {
            let (settings, teams) = self.changes(core);
            if self.request.is_none() && settings.is_empty() && teams.is_none() {
                self.load(core);
            } else {
                self.seen = Some(core.minigames.revision);
                self.status(core, None);
            }
        }
    }
    fn on_result(
        &mut self,
        id: RequestId,
        kind: Option<&Pending>,
        result: &Result<(), String>,
        core: &mut Core,
    ) -> bool {
        if self.request != Some(id) || !matches!(kind, Some(Pending::MiniGame(_))) {
            return false;
        }
        self.request = None;
        match result {
            Ok(()) => {
                core.minigames.status.clear();
                // The new values arrive with the next listing; take them now
                // as the base so the window reads as applied.
                self.base = (self.values.clone(), self.teams.clone());
                self.status(core, Some("Applied."));
            }
            Err(e) => {
                core.minigames.status.clear();
                self.status(core, Some(e));
            }
        }
        true
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
        let name = self.view.node(ev.node).ctrl.name.clone().unwrap_or_default();
        let row = self.rows.iter().find(|(_, n)| *n == name).map(|(t, _)| *t);
        if ev.kind == EventKind::Changed {
            // A pick from a list, or a team's colour.
            if let Some(target) = row {
                let i = match target {
                    Target::Game(i) | Target::Team(_, i) => i,
                };
                if let Some(s) = core.minigames.addon_settings.get(i).cloned()
                    && let MiniGameSettingKind::PaintColor { .. } = &s.kind
                    && let Some(c) = self.view.selected(ev.node)
                {
                    let _ = self.read_fields(core);
                    self.set(target, &s.key, MiniGameSettingValue::Int(c));
                    self.build(core);
                } else if let Some(s) = core.minigames.addon_settings.get(i).cloned()
                    && let MiniGameSettingKind::List { items } = &s.kind
                    && let Some((v, _)) = self
                        .view
                        .selected(ev.node)
                        .and_then(|i| usize::try_from(i).ok())
                        .and_then(|i| items.get(i))
                {
                    let _ = self.read_fields(core);
                    self.set(target, &s.key, v.clone());
                    // Other settings may show or hide with this one.
                    self.build(core);
                }
            } else if let Some(t) = name
                .strip_prefix("AOS_T")
                .and_then(|r| r.strip_suffix("_Color"))
                .and_then(|t| t.parse::<usize>().ok())
                && let Some(c) = self.view.selected(ev.node).and_then(|c| u8::try_from(c).ok())
            {
                let _ = self.read_fields(core);
                if let Some(team) = self.teams.get_mut(t) {
                    team.color = c;
                }
                self.build(core);
            } else {
                self.status(core, None);
            }
            return;
        }
        if !matches!(ev.kind, EventKind::Click | EventKind::Submit) {
            return;
        }
        let command = command_of(&self.view, ev.node);
        match command.as_str() {
            "AOS_Close" => Self::close(core),
            APPLY => self.apply(core),
            "AOS_AddTeam" => {
                let _ = self.read_fields(core);
                let used: Vec<u8> = self.teams.iter().map(|t| t.color).collect();
                let color = (0..core.minigames.palette.len().min(64) as u8)
                    .find(|c| !used.contains(c))
                    .unwrap_or(0);
                let settings = core
                    .minigames
                    .addon_settings
                    .iter()
                    .filter(|s| s.team)
                    .map(|s| (s.key.clone(), s.default.clone()))
                    .collect();
                self.teams.push(DraftTeam {
                    id: None,
                    name: format!("Team {}", self.teams.len() + 1),
                    color,
                    settings,
                });
                self.build(core);
            }
            _ => {
                if let Some(t) = command
                    .strip_prefix("AOS_T")
                    .and_then(|r| r.strip_suffix("_Remove"))
                    .and_then(|t| t.parse::<usize>().ok())
                {
                    let _ = self.read_fields(core);
                    if t < self.teams.len() {
                        self.teams.remove(t);
                    }
                    self.build(core);
                } else if let Some(Target::Game(i) | Target::Team(_, i)) = row
                    && let Some(s) = core.minigames.addon_settings.get(i).cloned()
                    && s.kind == MiniGameSettingKind::Bool
                {
                    let on = self.view.bool_value(ev.node);
                    let _ = self.read_fields(core);
                    self.set(row.expect("matched"), &s.key, MiniGameSettingValue::Bool(on));
                    self.build(core);
                }
            }
        }
    }
}
