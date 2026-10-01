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

const W: i32 = 480;
const H: i32 = 470;
const ROW: i32 = 26;
const ROWS: &str = "AOS_Rows";
const SCROLL: &str = "AOS_Scroll";
const STATUS: &str = "AOS_Status";
const APPLY: &str = "AOS_Apply";
const APPLY_RESET: &str = "AOS_ApplyReset";
const RESET: &str = "AOS_Reset";
const END: &str = "AOS_End";
const NOTIFY: &str = "AOS_Notify";
const FAVS: &str = "AOS_Favs";
/// The editor's choice to tell the game's players what they change
/// (Slayer's Notify Players on Update).
pub const NOTIFY_PREF: &str = "$Pref::AddOnSettings::NotifyPlayers";
/// A player row's team pick: not in the game, or on no team.
const OUT_OF_GAME: i64 = -2;
const NO_TEAM: i64 = -1;

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
    /// The request sends the draft (Apply), so its success makes the
    /// draft the host's.
    applying: bool,
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
            ctrl("GuiScrollCtrl", "BlockScrollProfile", Rect::new(12, 32, W - 24, H - 140)),
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
            text("GuiMLTextProfile", Rect::new(12, H - 104, W - 24, 34), ""),
            STATUS,
        );
        status.class = "GuiMLTextCtrl".into();
        win.children.push(status);
        // Favourites: ten slots of the whole setup.
        win.children.push(text("GuiTextProfile", Rect::new(12, H - 66, 70, 20), "Favourites:"));
        let mut favs = popup(Rect::new(84, H - 66, 110, 20), FAVS);
        favs.command = Some(FAVS.into());
        win.children.push(favs);
        win.children
            .push(push_button(Rect::new(198, H - 68, 56, 24), "Load", "AOS_FavLoad"));
        win.children
            .push(push_button(Rect::new(258, H - 68, 56, 24), "Save", "AOS_FavSave"));
        win.children.push(check(Rect::new(326, H - 66, 20, 20), NOTIFY));
        win.children
            .push(text("GuiTextProfile", Rect::new(348, H - 66, W - 360, 20), "Tell players"));
        win.children
            .push(push_button(Rect::new(12, H - 36, 64, 28), "Reset", RESET));
        win.children
            .push(push_button(Rect::new(80, H - 36, 64, 28), "End", END));
        win.children
            .push(push_button(Rect::new(W - 316, H - 36, 80, 28), "Close", "AOS_Close"));
        win.children
            .push(push_button(Rect::new(W - 232, H - 36, 120, 28), "Apply & Reset", APPLY_RESET));
        win.children
            .push(push_button(Rect::new(W - 108, H - 36, 94, 28), "Apply", APPLY));
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
            applying: false,
            seen: None,
        };
        if let Some(n) = screen.view.id(NOTIFY) {
            let on = core.prefs.bool_or(NOTIFY_PREF, true);
            screen.view.set_bool(n, on);
        }
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
    /// Keep the window's setup in the picked slot.
    fn save_favorite(&mut self, core: &mut Core) {
        if let Err(e) = self.read_fields(core) {
            self.status(core, Some(&e));
            return;
        }
        let slot = self.favorite_slot();
        core.settings.addon_favorites.insert(
            slot,
            AddOnFavorite {
                settings: self.values.clone(),
                teams: self
                    .teams
                    .iter()
                    .map(|t| AddOnFavoriteTeam {
                        name: t.name.clone(),
                        color: t.color,
                        settings: t.settings.clone(),
                    })
                    .collect(),
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
        let fits = |key: &str, value: &MiniGameSettingValue, team: bool| {
            Self::setting(core, key).is_some_and(|s| s.team == team && Self::takes(s, value))
        };
        for (key, value) in &fav.settings {
            if fits(key, value, false) {
                self.values.insert(key.clone(), value.clone());
            }
        }
        if Self::team_setup(core) {
            // Keep the teams the game has, in order, under the favourite's
            // names and settings; more are added, fewer removed.
            let mut teams = Vec::new();
            for (i, t) in fav.teams.iter().enumerate() {
                let mut settings: BTreeMap<String, MiniGameSettingValue> = core
                    .minigames
                    .addon_settings
                    .iter()
                    .filter(|s| s.team)
                    .map(|s| (s.key.clone(), s.default.clone()))
                    .collect();
                for (key, value) in &t.settings {
                    if fits(key, value, true) {
                        settings.insert(key.clone(), value.clone());
                    }
                }
                teams.push(DraftTeam {
                    id: self.teams.get(i).and_then(|d| d.id),
                    name: t.name.clone(),
                    color: t.color,
                    settings,
                });
            }
            self.teams = teams;
        }
        self.build(core);
        self.status(core, Some(&format!("Loaded slot {}. Apply to use it.", slot + 1)));
    }
    /// Whether a setting may hold `value` (a favourite's, kept from before).
    fn takes(s: &MiniGameAddOnSetting, value: &MiniGameSettingValue) -> bool {
        match (&s.kind, value) {
            (MiniGameSettingKind::Bool, MiniGameSettingValue::Bool(_)) => true,
            (MiniGameSettingKind::Int { min, max }, MiniGameSettingValue::Int(n))
            | (MiniGameSettingKind::PaintColor { min, max }, MiniGameSettingValue::Int(n)) => {
                (*min..=*max).contains(n)
            }
            (MiniGameSettingKind::Text { max_length }, MiniGameSettingValue::Text(t)) => {
                t.chars().count() <= *max_length as usize
            }
            (MiniGameSettingKind::List { items }, v) => items.iter().any(|(i, _)| i == v),
            _ => false,
        }
    }
    /// Categories of team settings that hold a look's parts.
    fn look_categories(core: &Core) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for s in core.minigames.addon_settings.iter().filter(|s| s.team && s.avatar.is_some()) {
            if !out.contains(&s.category) {
                out.push(s.category.clone());
            }
        }
        out
    }
    /// A team's look in `category` as the avatar editor takes it, from
    /// `values` (the draft, or the defaults).
    fn look(
        core: &Core,
        category: &str,
        values: &dyn Fn(&MiniGameAddOnSetting) -> MiniGameSettingValue,
    ) -> AvatarPrefs {
        let mut look = AvatarPrefs::default();
        for s in core.minigames.addon_settings.iter().filter(|s| s.team && s.category == category) {
            let Some(part) = &s.avatar else { continue };
            look.set(part, value_text(&values(s)));
        }
        look.name_parts(&core.pack.data.data.avatar);
        look
    }
    /// Open the avatar editor on team `t`'s look in `category`.
    fn edit_look(&mut self, core: &mut Core, t: usize, category: &str) {
        let _ = self.read_fields(core);
        let Some(team) = self.teams.get(t) else { return };
        let look = Self::look(core, category, &|s| {
            team.settings.get(&s.key).cloned().unwrap_or_else(|| s.default.clone())
        });
        let default = Self::look(core, category, &|s| s.default.clone());
        core.avatar_value = Some(crate::ui::AvatarValue {
            title: format!("Edit {category}: {}", team.name),
            look,
            default,
            key: format!("{t}:{category}"),
            done: None,
        });
        core.push(ScreenId::Avatar);
    }
    /// Take the look the avatar editor left, if it was one of ours.
    fn take_look(&mut self, core: &mut Core) {
        let Some(value) = core.avatar_value.as_ref() else { return };
        let Some(look) = value.done.clone() else { return };
        let key = value.key.clone();
        core.avatar_value = None;
        let Some((t, category)) = key.split_once(':') else { return };
        let Some(t) = t.parse::<usize>().ok().filter(|t| *t < self.teams.len()) else { return };
        let positions = look.part_positions(&core.pack.data.data.avatar);
        for s in core.minigames.addon_settings.iter().filter(|s| s.team && s.category == category) {
            let Some(part) = &s.avatar else { continue };
            let value = match &s.kind {
                MiniGameSettingKind::Int { min, max } => match positions.get(part.as_str()) {
                    Some(&i) => MiniGameSettingValue::Int((i as i64).clamp(*min, *max)),
                    None => continue,
                },
                MiniGameSettingKind::Text { max_length } => {
                    let Some(text) = look.get(part) else { continue };
                    MiniGameSettingValue::Text(text.chars().take(*max_length as usize).collect())
                }
                _ => continue,
            };
            self.teams[t].settings.insert(s.key.clone(), value);
        }
        self.build(core);
    }
    /// Whether the local player lacks the level `key` needs in this game.
    fn locked(&self, core: &Core, key: &str) -> bool {
        self.game.is_some_and(|g| {
            core.minigames
                .addon_locked
                .iter()
                .any(|(id, keys)| *id == g && keys.iter().any(|k| k == key))
        })
    }
    /// Whether the team list shows (Slayer's teams, in a mode with them).
    fn teams_shown(&self, core: &Core) -> bool {
        let Some((key, values)) = &core.minigames.teams_shown_when else {
            return true;
        };
        let current = self
            .values
            .get(key)
            .cloned()
            .or_else(|| Self::setting(core, key).map(|d| d.default.clone()));
        current.is_some_and(|v| values.contains(&v))
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
            if s.team || s.avatar.is_some() || self.summary(core).is_none() || !self.shown(core, s, None) {
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
        if Self::team_setup(core) && self.summary(core).is_some() && self.teams_shown(core) {
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
                    if !s.team || s.avatar.is_some() || !self.shown(core, s, Some(t)) {
                        continue;
                    }
                    let value = team.settings.get(&s.key).cloned().unwrap_or_else(|| s.default.clone());
                    self.row(Target::Team(t, i), s, &value, 36, y, editable, core);
                    y += ROW;
                }
                // A look's parts open together in the avatar editor.
                for category in Self::look_categories(core) {
                    let shown = settings.iter().any(|s| {
                        s.team && s.avatar.is_some() && s.category == category && self.shown(core, s, Some(t))
                    });
                    if !shown {
                        continue;
                    }
                    let name = format!("AOS_T{t}_Look_{category}");
                    let n = self.view.add(
                        rows,
                        push_button(Rect::new(36, y - 2, 150, 24), &format!("Edit {category}"), &name),
                    );
                    self.view.set_active(n, editable);
                    y += ROW;
                }
                y += 6;
            }
            if editable {
                self.view
                    .add(rows, push_button(Rect::new(20, y, 110, 24), "Add Team", "AOS_AddTeam"));
                y += ROW + 4;
            }
            y = self.player_rows(core, y, editable);
        }
        if settings.is_empty() {
            heading(&mut self.view, &mut y, "No running Add-On has settings.");
        }
        self.view.nodes[rows].ctrl.extent[1] = y + 4;
        self.view.relayout();
        self.status(core, None);
    }

    /// The game's players (and, for its editor, everyone else on the
    /// server) with the team each plays for: an editor moves them between
    /// teams, brings them in or removes them (Slayer's Add Member and
    /// Remove Member).
    fn player_rows(&mut self, core: &Core, mut y: i32, editable: bool) -> i32 {
        let Some(rows) = self.view.id(ROWS) else {
            return y;
        };
        let Some(game) = self.summary(core).cloned() else {
            return y;
        };
        let width = W - 42;
        self.view.add(
            rows,
            text("GuiBigTextProfile", Rect::new(4, y, width - 8, 22), "Players"),
        );
        y += 24;
        let mut people: Vec<(MiniGamePlayerId, String, Option<Option<u32>>)> = game
            .members
            .iter()
            .map(|m| (m.id, m.name.clone(), Some(m.team)))
            .collect();
        if editable {
            for p in &core.players {
                let id = MiniGamePlayerId(p.id);
                if !people.iter().any(|(m, _, _)| *m == id) {
                    people.push((id, p.name.clone(), None));
                }
            }
        }
        // Only teams the host already has can take players; new ones
        // need Apply first.
        let teams: Vec<(String, i64)> = game
            .teams
            .iter()
            .map(|t| (t.name.clone(), i64::from(t.id)))
            .collect();
        for (i, (id, name, team)) in people.iter().enumerate() {
            self.view.add(
                rows,
                text("GuiTextProfile", Rect::new(20, y, 200, 20), name),
            );
            let pick = format!("AOS_P{}_Team", id.0);
            let n = self.view.add(rows, popup(Rect::new(230, y, 150, 20), &pick));
            let mut items = Vec::new();
            if team.is_none() {
                items.push(("Not playing".to_owned(), OUT_OF_GAME));
            }
            items.push(("No team".to_owned(), NO_TEAM));
            items.extend(teams.iter().cloned());
            self.view.state(n).items = items;
            let current = match team {
                None => OUT_OF_GAME,
                Some(None) => NO_TEAM,
                Some(Some(t)) => i64::from(*t),
            };
            self.view.select(n, Some(current));
            self.view.set_active(n, editable && self.request.is_none());
            if editable && team.is_some() && Some(*id) != Some(game.owner) {
                self.view.add(
                    rows,
                    push_button(
                        Rect::new(width - 84, y - 2, 76, 24),
                        "Remove",
                        &format!("AOS_P{}_Kick", id.0),
                    ),
                );
            }
            let _ = i;
            y += ROW;
        }
        y
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
        let control = Rect::new(230, y, width - 266, 20);
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
        self.view.set_active(n, editable && !self.locked(core, &s.key));
        if !s.help.is_empty() {
            let i = match target {
                Target::Game(i) | Target::Team(_, i) => i,
            };
            self.view.add(
                rows,
                push_button(Rect::new(width - 30, y - 1, 22, 22), "?", &format!("AOS_H{i}")),
            );
        }
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
        let ready = editable && self.request.is_none() && self.summary(core).is_some();
        for button in [APPLY, APPLY_RESET, RESET, END, NOTIFY] {
            if let Some(n) = self.view.id(button) {
                self.view.set_active(n, ready);
                self.view.set_visible(n, editable);
            }
        }
    }

    fn apply(&mut self, core: &mut Core, reset: bool) {
        if let Err(e) = self.read_fields(core) {
            self.status(core, Some(&e));
            return;
        }
        let Some(game) = self.game else { return };
        let (settings, teams) = self.changes(core);
        if settings.is_empty() && teams.is_none() {
            if reset {
                self.request =
                    core.minigame_request(MiniGameOperation::Reset, UiAction::ResetMiniGame { game });
            } else {
                self.status(core, Some("Nothing has changed."));
            }
            return;
        }
        let quiet = !core.prefs.bool_or(NOTIFY_PREF, true);
        self.applying = true;
        self.request = core.minigame_request(
            MiniGameOperation::AddOnSettings,
            UiAction::EditMiniGameAddOns {
                game,
                settings,
                teams,
                quiet,
                reset,
            },
        );
        let status = core.minigames.status.clone();
        self.status(core, Some(&status));
    }

    /// Send a player row's new team.
    fn move_player(&mut self, core: &mut Core, player: MiniGamePlayerId, pick: Option<i64>) {
        let Some(game) = self.game else { return };
        let team = match pick {
            None | Some(OUT_OF_GAME) => return,
            Some(NO_TEAM) => None,
            Some(t) => u32::try_from(t).ok(),
        };
        self.request = core.minigame_request(
            MiniGameOperation::Configure,
            UiAction::SetMiniGameTeam {
                game,
                target: player,
                team,
            },
        );
        self.status(core, None);
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
    fn on_wake(&mut self, core: &mut Core) {
        self.take_look(core);
    }
    fn on_update(&mut self, core: &mut Core) {
        self.take_look(core);
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
            Ok(()) if std::mem::take(&mut self.applying) => {
                core.minigames.status.clear();
                // The new values arrive with the next listing; take them now
                // as the base so the window reads as applied.
                self.base = (self.values.clone(), self.teams.clone());
                self.status(core, Some("Applied."));
            }
            Ok(()) => {
                core.minigames.status.clear();
                self.status(core, Some("Done."));
            }
            Err(e) => {
                self.applying = false;
                core.minigames.status.clear();
                self.status(core, Some(e));
                // A refused team move shows the player where they still are.
                self.build(core);
            }
        }
        true
    }
    fn on_key(&mut self, key: Key, _: Modifiers, core: &mut Core) -> bool {
        if key == Key::Escape {
            Self::close(core);
            return true;
        }
        // Delete removes the team whose row has the focus (Slayer's team
        // list), unless the focus is in a text field.
        if key == Key::Delete && self.editable(core) {
            let focused = self.view.focus.and_then(|n| {
                let ctrl = &self.view.node(n).ctrl;
                (ctrl.class != "GuiTextEditCtrl").then(|| ctrl.name.clone()).flatten()
            });
            if let Some(t) = focused
                .as_deref()
                .and_then(|name| name.strip_prefix("AOS_T"))
                .and_then(|r| r.split('_').next())
                .and_then(|t| t.parse::<usize>().ok())
                && t < self.teams.len()
            {
                let _ = self.read_fields(core);
                self.teams.remove(t);
                self.build(core);
                return true;
            }
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
            if let Some(player) = name
                .strip_prefix("AOS_P")
                .and_then(|r| r.strip_suffix("_Team"))
                .and_then(|p| p.parse::<u64>().ok())
            {
                self.move_player(core, MiniGamePlayerId(player), self.view.selected(ev.node));
                return;
            }
            if name == FAVS {
                return;
            }
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
            APPLY => self.apply(core, false),
            APPLY_RESET => self.apply(core, true),
            RESET => {
                if let Some(game) = self.game {
                    self.request =
                        core.minigame_request(MiniGameOperation::Reset, UiAction::ResetMiniGame { game });
                }
            }
            END => {
                if let Some(game) = self.game {
                    core.message_yes_no(
                        "End Mini-Game?",
                        "Are you sure you want to end the mini-game?",
                        crate::ui::Callback::MiniGame {
                            game,
                            operation: MiniGameOperation::End,
                        },
                    );
                }
            }
            NOTIFY => {
                let on = self.view.bool_value(ev.node);
                core.prefs.set(NOTIFY_PREF, if on { "1" } else { "0" });
                core.save_settings();
            }
            "AOS_FavSave" => self.save_favorite(core),
            "AOS_FavLoad" => self.load_favorite(core),
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
                if let Some((t, category)) = command
                    .strip_prefix("AOS_T")
                    .and_then(|r| r.split_once("_Look_"))
                    .and_then(|(t, c)| Some((t.parse::<usize>().ok()?, c.to_owned())))
                {
                    self.edit_look(core, t, &category);
                } else if let Some(i) = command.strip_prefix("AOS_H").and_then(|i| i.parse::<usize>().ok()) {
                    if let Some(s) = core.minigames.addon_settings.get(i) {
                        let (title, help) = (s.title.clone(), s.help.clone());
                        core.message_ok(&title, &help);
                    }
                } else if let Some(player) = command
                    .strip_prefix("AOS_P")
                    .and_then(|r| r.strip_suffix("_Kick"))
                    .and_then(|p| p.parse::<u64>().ok())
                {
                    if let Some(game) = self.game {
                        self.request = core.minigame_request(
                            MiniGameOperation::RemoveMember,
                            UiAction::RemoveMiniGameMember {
                                target: MiniGamePlayerId(player),
                                game: Some(game),
                            },
                        );
                        self.status(core, None);
                    }
                } else if let Some(t) = command
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
