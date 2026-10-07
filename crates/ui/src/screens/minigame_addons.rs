//! A mini-game's Add-On Settings: the settings running Add-Ons declare
//! (Slayer's game mode, lives, points, time and the rest), and its team
//! list when an Add-On uses teams. Built natively: v20 had no such window,
//! and Slayer brought its own GUI. The host sends each setting's kind,
//! range and choices, checks every change again and says who may make it.
//!
//! Opened from the Admin menu instead, the window shows the running
//! Add-Ons' server-wide settings (RTB's `$Pref::Server::*` preferences),
//! which only the host changes; they are part of its Server Settings and
//! saved with its other `$Pref::Server::*` values.
use super::*;
use crate::api::*;
use crate::view::EventKind;
use std::collections::{BTreeMap, BTreeSet};

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
const CATEGORY: &str = "AOS_Category";
const TEAM_PICK: &str = "AOS_Team";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Setup,
    Teams,
    Players,
}
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
    width: i32,
    game: Option<MiniGameId>,
    /// Showing the server-wide settings rather than a mini-game's.
    server: bool,
    /// Setting up a game the Create Mini-Game window has yet to make: Save
    /// keeps the draft for Create to send ([`Core::send_minigame_draft`]).
    draft: bool,
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
    /// A favourite applies vanilla rules before its Add-On settings/reset.
    after_rules: Option<(MiniGameOperation, UiAction)>,
    configuring_rules: bool,
    /// Keep the source slot intact when its unavailable vocabulary is omitted.
    unavailable_favorite: Option<u8>,
    /// Preserve partially typed fields across asynchronous listing refreshes.
    typed_dirty: bool,
    seen: Option<u64>,
    /// The vanilla rules a loaded favourite brings, sent with Apply when
    /// they differ from the game's.
    rules: Option<MiniGameRules>,
    page: Page,
    selected_team: usize,
    category: Option<(String, String)>,
    categories: Vec<(String, String)>,
    // Raw text survives category/team switches even while it is not a valid
    // number or team name. Apply validates every retained field, including hidden ones.
    typed: BTreeMap<(Option<usize>, String), String>,
    name_field: Option<(usize, String)>,
}

fn value_text(v: &MiniGameSettingValue) -> String {
    match v {
        MiniGameSettingValue::Bool(b) => (if *b { "1" } else { "0" }).into(),
        MiniGameSettingValue::Int(n) => n.to_string(),
        MiniGameSettingValue::Text(t) => t.clone(),
    }
}

/// Slayer's `TEAMCOLOR`: a look colour that is the team's own.
fn is_team_color(text: &str) -> bool {
    text.trim().eq_ignore_ascii_case("TEAMCOLOR")
}

/// Whether two `"r g b a"` colours are the same to a paint step.
fn same_color(a: &str, b: &str) -> bool {
    let parse = |t: &str| -> Vec<f32> {
        t.split_whitespace()
            .filter_map(|v| v.parse().ok())
            .collect()
    };
    let (a, b) = (parse(a), parse(b));
    a.len() == b.len()
        && !a.is_empty()
        && a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 0.5 / 255.0)
}

impl AddOnSettings {
    pub fn new(core: &mut Core) -> Self {
        let teams_route = std::mem::take(&mut core.minigame_addons_teams);
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
            ctrl(
                "GuiScrollCtrl",
                "BlockScrollProfile",
                Rect::new(12, 32, W - 24, H - 140),
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
        for (x, label, name) in [
            (12, "Setup", "AOS_Setup"),
            (92, "Teams", "AOS_Teams"),
            (172, "Players", "AOS_Players"),
        ] {
            win.children
                .push(push_button(Rect::new(x, 32, 76, 24), label, name));
        }
        win.children.push(push_button(
            Rect::new(W - 122, 32, 110, 24),
            "Add Team",
            "AOS_AddTeam",
        ));
        win.children
            .push(popup(Rect::new(12, 62, W - 24, 20), CATEGORY));
        win.children
            .push(popup(Rect::new(12, 62, 180, 20), TEAM_PICK));
        let mut status = named(
            text("GuiMLTextProfile", Rect::new(12, H - 104, W - 24, 34), ""),
            STATUS,
        );
        status.class = "GuiMLTextCtrl".into();
        win.children.push(status);
        // Favourites: ten slots of the whole setup.
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
        win.children
            .push(check(Rect::new(326, H - 66, 20, 20), NOTIFY));
        win.children.push(named(
            text(
                "GuiTextProfile",
                Rect::new(348, H - 66, W - 360, 20),
                "Tell players",
            ),
            "AOS_NotifyLabel",
        ));
        win.children
            .push(push_button(Rect::new(12, H - 36, 64, 28), "Reset", RESET));
        win.children
            .push(push_button(Rect::new(80, H - 36, 64, 28), "End", END));
        win.children.push(push_button(
            Rect::new(W - 316, H - 36, 80, 28),
            "Cancel",
            "AOS_Close",
        ));
        win.children.push(push_button(
            Rect::new(W - 232, H - 36, 120, 28),
            "Save & Reset",
            APPLY_RESET,
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
            game: core.minigame_addons,
            server: core.server_addon_settings,
            draft: !core.server_addon_settings && core.minigame_addons.is_none(),
            values: BTreeMap::new(),
            teams: Vec::new(),
            base: (BTreeMap::new(), Vec::new()),
            rows: Vec::new(),
            request: None,
            applying: false,
            after_rules: None,
            configuring_rules: false,
            unavailable_favorite: None,
            typed_dirty: false,
            seen: None,
            rules: None,
            page: if teams_route {
                Page::Teams
            } else {
                Page::Setup
            },
            selected_team: 0,
            category: None,
            categories: Vec::new(),
            typed: BTreeMap::new(),
            name_field: None,
        };
        if let Some(n) = screen.view.id(NOTIFY) {
            let on = core.prefs.bool_or(NOTIFY_PREF, true);
            screen.view.set_bool(n, on);
        }
        screen.fill_favorites(core);
        if !screen.server
            && core
                .minigames
                .addon_settings
                .iter()
                .all(|s| s.team || s.server)
        {
            screen.page = Page::Teams;
        }
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
        if self
            .unavailable_favorite
            .as_ref()
            .is_some_and(|source| *source == slot)
        {
            self.status(
                core,
                Some("This slot has unavailable settings. Choose another slot to Store."),
            );
            core.message_ok("Favorite has unavailable settings", "The saved favorite is intact. Enable its Add-Ons before hosting and reopen this editor, or choose another slot to store the compatible draft.");
            return;
        }
        // The game's vanilla rules go with it (Slayer's favourites kept
        // every preference), or the ones a loaded favourite brought.
        let rules = self
            .rules
            .clone()
            .or_else(|| self.summary(core).map(|g| g.rules.clone()));
        core.settings.addon_favorites.insert(
            slot,
            AddOnFavorite {
                rules,
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
        let mut unavailable = Vec::new();
        for (key, value) in &fav.settings {
            if !fits(key, value, false) {
                unavailable.push(key.clone());
            }
        }
        for (index, team) in fav.teams.iter().enumerate() {
            for (key, value) in &team.settings {
                if !fits(key, value, true) {
                    unavailable.push(format!("Team {}: {key}", index + 1));
                }
            }
        }
        self.unavailable_favorite = (!unavailable.is_empty()).then_some(slot);
        for (key, value) in &fav.settings {
            if fits(key, value, false) {
                self.values.insert(key.clone(), value.clone());
            }
        }
        if !self.server {
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
        if fav.rules.is_some() {
            self.rules = fav.rules;
        }
        self.rows.clear();
        self.typed.clear();
        self.name_field = None;
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
            core.message_ok("Favorite has unavailable settings", &format!("These settings are not offered here, or their values are no longer accepted:\n\n{keys}\n\nThe saved favorite is intact. Save applies only compatible settings. Enable its Add-Ons before hosting to recover them. Choose another slot to Store this draft."));
        }
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
            (
                MiniGameSettingKind::List { items }
                | MiniGameSettingKind::Item { items }
                | MiniGameSettingKind::PlayerType { items },
                v,
            ) => items.iter().any(|(i, _)| i == v),
            _ => false,
        }
    }
    /// Categories of team settings that hold a look's parts.
    fn look_categories(core: &Core) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for s in core
            .minigames
            .addon_settings
            .iter()
            .filter(|s| s.team && s.avatar.is_some())
        {
            if !out.contains(&s.category) {
                out.push(s.category.clone());
            }
        }
        out
    }
    /// The look colour (`"r g b a"`, 0 to 1) of paint colour `color`, which
    /// a colour value `TEAMCOLOR` stands for.
    fn team_color_text(core: &Core, color: u8) -> Option<String> {
        let [r, g, b] = *core.minigames.palette.get(usize::from(color))?;
        let c = |v: u8| f32::from(v) / 255.0;
        Some(format!("{} {} {} 1", c(r), c(g), c(b)))
    }
    /// A team's look in `category` as the avatar editor takes it, from
    /// `values` (the draft, or the defaults); a colour that is the text
    /// `TEAMCOLOR` shows as `team_color`.
    fn look(
        core: &Core,
        category: &str,
        team_color: Option<&str>,
        values: &dyn Fn(&MiniGameAddOnSetting) -> MiniGameSettingValue,
    ) -> AvatarPrefs {
        let mut look = AvatarPrefs::default();
        for s in core
            .minigames
            .addon_settings
            .iter()
            .filter(|s| s.team && s.category == category)
        {
            let Some(part) = &s.avatar else { continue };
            let text = value_text(&values(s));
            match team_color {
                Some(color) if is_team_color(&text) => look.set(part, color),
                _ => look.set(part, text),
            }
        }
        look.name_parts(&core.pack.data.data.avatar);
        look
    }
    /// Open the avatar editor on team `t`'s look in `category`.
    fn edit_look(&mut self, core: &mut Core, t: usize, category: &str) {
        let _ = self.read_fields(core);
        let Some(team) = self.teams.get(t) else {
            return;
        };
        let team_color = Self::team_color_text(core, team.color);
        let look = Self::look(core, category, team_color.as_deref(), &|s| {
            team.settings
                .get(&s.key)
                .cloned()
                .unwrap_or_else(|| s.default.clone())
        });
        let default = Self::look(core, category, team_color.as_deref(), &|s| {
            s.default.clone()
        });
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
        let Some(value) = core.avatar_value.as_ref() else {
            return;
        };
        let Some(look) = value.done.clone() else {
            return;
        };
        let key = value.key.clone();
        core.avatar_value = None;
        let Some((t, category)) = key.split_once(':') else {
            return;
        };
        let Some(t) = t.parse::<usize>().ok().filter(|t| *t < self.teams.len()) else {
            return;
        };
        let positions = look.part_positions(&core.pack.data.data.avatar);
        let team_color = Self::team_color_text(core, self.teams[t].color);
        for s in core
            .minigames
            .addon_settings
            .iter()
            .filter(|s| s.team && s.category == category)
        {
            let Some(part) = &s.avatar else { continue };
            let value = match &s.kind {
                MiniGameSettingKind::Int { min, max } => match positions.get(part.as_str()) {
                    Some(&i) => MiniGameSettingValue::Int((i as i64).clamp(*min, *max)),
                    None => continue,
                },
                MiniGameSettingKind::Text { max_length } => {
                    let Some(text) = look.get(part) else { continue };
                    // Left at the team's colour: it stays `TEAMCOLOR`, so
                    // it follows the team's colour when that changes.
                    let before = self.teams[t].settings.get(&s.key).unwrap_or(&s.default);
                    if matches!(before, MiniGameSettingValue::Text(b) if is_team_color(b))
                        && team_color.as_deref().is_some_and(|c| same_color(c, text))
                    {
                        continue;
                    }
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
        let Some(when) = &core.minigames.teams_shown_when else {
            return true;
        };
        let key = &when.setting;
        let current = self
            .values
            .get(key)
            .cloned()
            .or_else(|| Self::setting(core, key).map(|d| d.default.clone()));
        current.is_some_and(|v| when.holds(&v))
    }
    /// What the Teams page says when the draft's choice has no teams, from
    /// the Add-On's own rule: the setting that turns teams on, and the
    /// choices of it that do.
    fn teams_hint(&self, core: &Core) -> String {
        let setting = core
            .minigames
            .teams_shown_when
            .as_ref()
            .and_then(|w| Some((w, Self::setting(core, &w.setting)?)));
        let Some((when, setting)) = setting else {
            return "This game doesn't use teams.".into();
        };
        let title = &setting.title;
        let choices: Vec<&str> = match &setting.kind {
            MiniGameSettingKind::List { items }
            | MiniGameSettingKind::Item { items }
            | MiniGameSettingKind::PlayerType { items } => items
                .iter()
                .filter(|(v, _)| when.holds(v))
                .map(|(_, name)| name.as_str())
                .collect(),
            _ => Vec::new(),
        };
        match choices.as_slice() {
            [] => format!("This {title} doesn't use teams: change {title} on Setup."),
            [one] => format!("This {title} doesn't use teams: set {title} on Setup to {one}."),
            [rest @ .., last] => format!(
                "This {title} doesn't use teams: set {title} on Setup to {} or {last}.",
                rest.join(", ")
            ),
        }
    }

    fn summary<'a>(&self, core: &'a Core) -> Option<&'a MiniGameSummary> {
        self.game
            .and_then(|id| core.minigames.games.iter().find(|g| g.id == id))
    }
    fn editable(&self, core: &Core) -> bool {
        if self.server {
            return core
                .admin
                .available(crate::models::admin::AdminFeature::HostOptions)
                && core
                    .admin
                    .snapshot
                    .as_ref()
                    .is_some_and(|s| s.local_host && s.options.is_some());
        }
        if self.draft {
            return core
                .minigames
                .can(crate::models::minigames::Operation::Create);
        }
        self.game
            .is_some_and(|g| core.minigames.addon_editable.contains(&g))
    }
    /// Whether there is something to show: the server, a running game, or
    /// the one Create will make.
    fn open(&self, core: &Core) -> bool {
        self.server || self.draft || self.summary(core).is_some()
    }
    /// Whether the window shows `s`, by where its value lives.
    fn mine(&self, s: &MiniGameAddOnSetting) -> bool {
        s.server == self.server && !s.team
    }
    /// The host's revision of what the window shows.
    /// What the window shows changed when this does. For a mini-game, the
    /// listing's own revision also counts every score: a point scored
    /// while the player edits must not rebuild the rows under their mouse.
    fn revision(&self, core: &Core) -> u64 {
        use std::hash::{Hash, Hasher};
        if self.server {
            return core.admin.revision;
        }
        // The status line is this client's own request text, not the host's.
        let mut shown = core.minigames.clone();
        shown.revision = 0;
        shown.status.clear();
        for m in &mut shown.members {
            m.score = 0;
        }
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        serde_json::to_vec(&shown)
            .unwrap_or_default()
            .hash(&mut hash);
        hash.finish()
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

    /// Take the host's values afresh.
    fn load(&mut self, core: &Core) {
        self.seen = Some(self.revision(core));
        self.rows.clear();
        self.typed.clear();
        self.name_field = None;
        self.typed_dirty = false;
        self.rules = None;
        self.unavailable_favorite = None;
        if self.server {
            let stored = Self::server_values(core);
            let values: BTreeMap<_, _> = core
                .minigames
                .addon_settings
                .iter()
                .filter(|s| s.server)
                .map(|s| {
                    let v = stored.and_then(|m| m.get(&s.key)).cloned();
                    (s.key.clone(), v.unwrap_or_else(|| s.default.clone()))
                })
                .collect();
            if let Some(n) = self.view.id("AOS_Window") {
                self.view.set_text(n, "Add-On Settings: Server");
            }
            self.values = values.clone();
            self.teams.clear();
            self.base = (values, Vec::new());
            self.build(core);
            return;
        }
        if self.draft {
            let kept = core.minigame_addon_draft.as_ref();
            let declared = || {
                core.minigames
                    .addon_settings
                    .iter()
                    .filter(|s| !s.team && !s.server)
            };
            let defaults: BTreeMap<_, _> = declared()
                .map(|s| (s.key.clone(), s.default.clone()))
                .collect();
            let values = declared()
                .map(|s| {
                    let v = kept.and_then(|d| d.settings.get(&s.key)).cloned();
                    (s.key.clone(), v.unwrap_or_else(|| s.default.clone()))
                })
                .collect();
            let teams = kept
                .map(|d| {
                    d.teams
                        .iter()
                        .map(|t| DraftTeam {
                            id: None,
                            name: t.name.clone(),
                            color: t.color,
                            settings: core
                                .minigames
                                .addon_settings
                                .iter()
                                .filter(|s| s.team)
                                .map(|s| {
                                    let v = t.settings.get(&s.key).cloned();
                                    (s.key.clone(), v.unwrap_or_else(|| s.default.clone()))
                                })
                                .collect(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            if let Some(n) = self.view.id("AOS_Window") {
                self.view.set_text(n, "MiniGame Settings: New Mini-Game");
            }
            self.values = values;
            self.teams = teams;
            // Changed from the defaults is what Create sends.
            self.base = (defaults, Vec::new());
            self.build(core);
            return;
        }
        let Some(g) = self.summary(core) else {
            self.values.clear();
            self.teams.clear();
            self.base = (BTreeMap::new(), Vec::new());
            self.build(core);
            return;
        };
        let mut values = BTreeMap::new();
        for s in core
            .minigames
            .addon_settings
            .iter()
            .filter(|s| !s.team && !s.server)
        {
            let v = g
                .addon_settings
                .get(&s.key)
                .cloned()
                .unwrap_or_else(|| s.default.clone());
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
                            t.settings
                                .get(&s.key)
                                .cloned()
                                .unwrap_or_else(|| s.default.clone()),
                        )
                    })
                    .collect(),
            })
            .collect();
        if let Some(n) = self.view.id("AOS_Window") {
            self.view
                .set_text(n, format!("MiniGame Settings: {}", g.title));
        }
        self.values = values.clone();
        self.teams = teams.clone();
        self.base = (values, teams);
        self.build(core);
    }

    /// Whether a setting shows, given the draft's other values.
    fn shown(&self, core: &Core, s: &MiniGameAddOnSetting, team: Option<usize>) -> bool {
        let Some(when) = &s.shown_when else {
            return true;
        };
        let key = &when.setting;
        let current = self
            .values
            .get(key)
            .or_else(|| {
                team.and_then(|t| self.teams.get(t))
                    .and_then(|t| t.settings.get(key))
            })
            .cloned()
            .or_else(|| Self::setting(core, key).map(|d| d.default.clone()));
        current.is_some_and(|v| when.holds(&v))
    }

    /// Settings which control dependent rows stay at the top of Setup. This
    /// uses declared dependencies, never Add-On names or setting-name guesses.
    fn controller(core: &Core, key: &str) -> bool {
        core.minigames
            .teams_shown_when
            .as_ref()
            .is_some_and(|w| w.setting == key)
            || core
                .minigames
                .addon_settings
                .iter()
                .any(|s| s.shown_when.as_ref().is_some_and(|w| w.setting == key))
    }

    fn capture_fields(&mut self) {
        for (target, name) in &self.rows {
            let Some(n) = self.view.id(name) else {
                continue;
            };
            if self.view.node(n).ctrl.class != "GuiTextEditCtrl" {
                continue;
            }
            // The stable setting key is stored on the control, since incoming
            // catalogs can reorder indexes while an existing draft is open.
            if let Some(key) = self.view.node(n).ctrl.field("settingKey") {
                let team = match target {
                    Target::Game(_) => None,
                    Target::Team(t, _) => Some(*t),
                };
                self.typed
                    .insert((team, key.into()), self.view.edit_text(n));
            }
        }
        if let Some((t, name)) = &self.name_field
            && let Some(n) = self.view.id(name)
        {
            self.typed
                .insert((Some(*t), String::new()), self.view.edit_text(n));
        }
    }

    fn remove_team(&mut self, t: usize) {
        if t >= self.teams.len() {
            return;
        }
        self.capture_fields();
        self.teams.remove(t);
        self.typed = std::mem::take(&mut self.typed)
            .into_iter()
            .filter_map(|((team, key), value)| match team {
                Some(n) if n == t => None,
                Some(n) if n > t => Some(((Some(n - 1), key), value)),
                _ => Some(((team, key), value)),
            })
            .collect();
        self.rows.clear();
        self.name_field = None;
        self.selected_team = self.selected_team.min(self.teams.len().saturating_sub(1));
    }

    /// Lay out one task at a time. Navigation and Add Team stay outside the
    /// scrolling details; category switches do not commit or discard a draft.
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
        self.name_field = None;
        let editable = self.editable(core);
        let width = self.width - 42;
        let mut y = 4;
        let heading = |view: &mut View, y: &mut i32, label: &str| {
            view.add(
                rows,
                text("GuiTextProfile", Rect::new(4, *y, width - 8, 22), label),
            );
            *y += 24;
        };
        // The Teams page stays open in a mode without teams, to say so and
        // how to change it; the Players page has nothing to assign.
        let has_game = !self.server && (self.draft || self.summary(core).is_some());
        let teams_used = self.teams_shown(core);
        let teams_available = has_game && teams_used;
        // Before Create there is nobody in the game to assign.
        let players_available = teams_available && !self.draft;
        if self.server {
            self.page = Page::Setup;
        }
        let page_open = match self.page {
            Page::Setup => true,
            Page::Teams => has_game,
            Page::Players => players_available,
        };
        if !page_open {
            self.page = Page::Setup;
        }
        if let Some(n) = self.view.id("AOS_AddTeam") {
            self.view
                .set_visible(n, self.page == Page::Teams && teams_available && editable);
            self.view.set_active(n, editable && self.request.is_none());
        }
        for (name, page) in [
            ("AOS_Setup", Page::Setup),
            ("AOS_Teams", Page::Teams),
            ("AOS_Players", Page::Players),
        ] {
            if let Some(n) = self.view.id(name) {
                self.view
                    .set_visible(n, !(self.server || (self.draft && page == Page::Players)));
                self.view.set_active(
                    n,
                    match page {
                        Page::Setup => true,
                        Page::Teams => has_game,
                        Page::Players => players_available,
                    },
                );
                self.view.set_text(
                    n,
                    match page {
                        Page::Setup => "Setup",
                        Page::Teams => "Teams",
                        Page::Players => "Players",
                    },
                );
            }
        }
        if !self.open(core) {
            heading(&mut self.view, &mut y, "That mini-game has ended.");
        }
        let settings = core.minigames.addon_settings.clone();
        self.selected_team = self.selected_team.min(self.teams.len().saturating_sub(1));
        self.categories.clear();
        for s in &settings {
            let relevant = match self.page {
                Page::Setup => {
                    self.mine(s)
                        && s.avatar.is_none()
                        && !Self::controller(core, &s.key)
                        && self.shown(core, s, None)
                }
                Page::Teams => {
                    s.team && teams_used && self.shown(core, s, Some(self.selected_team))
                }
                Page::Players => false,
            };
            let group = (s.add_on.clone(), s.category.clone());
            if relevant && !self.categories.contains(&group) {
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
            let rect = if self.page == Page::Teams {
                Rect::new(18 + (self.width - 30) / 2, 62, (self.width - 30) / 2, 20)
            } else {
                Rect::new(12, 62, self.width - 24, 20)
            };
            self.view.nodes[n].ctrl.position = [rect.x, rect.y];
            self.view.nodes[n].ctrl.extent = [rect.w, rect.h];
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
            self.view
                .set_visible(n, self.page != Page::Players && !self.categories.is_empty());
            self.view.set_active(n, true);
        }
        if let Some(n) = self.view.id(TEAM_PICK) {
            self.view.state(n).items = self
                .teams
                .iter()
                .enumerate()
                .map(|(t, team)| {
                    let name = self
                        .typed
                        .get(&(Some(t), String::new()))
                        .unwrap_or(&team.name);
                    (format!("{}: {}", t + 1, name), t as i64)
                })
                .collect();
            self.view.select(n, Some(self.selected_team as i64));
            self.view.set_visible(
                n,
                self.page == Page::Teams && teams_used && !self.teams.is_empty(),
            );
        }
        if self.open(core) {
            match self.page {
                Page::Setup => {
                    for (i, s) in settings
                        .iter()
                        .enumerate()
                        .filter(|(_, s)| Self::controller(core, &s.key))
                        .chain(
                            settings
                                .iter()
                                .enumerate()
                                .filter(|(_, s)| !Self::controller(core, &s.key)),
                        )
                    {
                        if !self.mine(s) || s.avatar.is_some() || !self.shown(core, s, None) {
                            continue;
                        }
                        if !Self::controller(core, &s.key)
                            && self.category.as_ref()
                                != Some(&(s.add_on.clone(), s.category.clone()))
                        {
                            continue;
                        }
                        let value = self
                            .values
                            .get(&s.key)
                            .cloned()
                            .unwrap_or_else(|| s.default.clone());
                        self.row(Target::Game(i), s, &value, 20, y, editable, core);
                        y += ROW;
                    }
                    if self.categories.is_empty() && self.rows.is_empty() {
                        heading(&mut self.view, &mut y, "No Add-On settings.");
                    }
                }
                Page::Teams if !teams_used => {
                    let hint = self.teams_hint(core);
                    heading(&mut self.view, &mut y, &hint);
                }
                Page::Teams if !self.teams.is_empty() => {
                    let t = self.selected_team;
                    let team = self.teams[t].clone();
                    self.view.add(
                        rows,
                        text("GuiTextProfile", Rect::new(4, y, 58, 20), "Name"),
                    );
                    let name = format!("AOS_T{t}_Name");
                    self.name_field = Some((t, name.clone()));
                    let n = self
                        .view
                        .add(rows, edit(Rect::new(62, y, width - 150, 20), &name));
                    self.view.set_text(
                        n,
                        self.typed
                            .get(&(Some(t), String::new()))
                            .cloned()
                            .unwrap_or_else(|| team.name.clone()),
                    );
                    self.view.set_active(n, editable);
                    if editable {
                        self.view.add(
                            rows,
                            push_button(
                                Rect::new(width - 84, y - 2, 76, 24),
                                "Remove",
                                &format!("AOS_T{t}_Remove"),
                            ),
                        );
                    }
                    y += ROW;
                    self.view.add(
                        rows,
                        text("GuiTextProfile", Rect::new(4, y, 58, 20), "Color"),
                    );
                    let n = self.view.add(
                        rows,
                        popup(Rect::new(62, y, 130, 20), &format!("AOS_T{t}_Color")),
                    );
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
                            swatch(
                                Rect::new(198, y + 2, 16, 16),
                                rgba([
                                    f32::from(rgb[0]) / 255.0,
                                    f32::from(rgb[1]) / 255.0,
                                    f32::from(rgb[2]) / 255.0,
                                    1.0,
                                ]),
                            ),
                            &format!("AOS_T{t}_Swatch"),
                        ),
                    );
                    y += ROW + 4;
                    let mut details: Vec<_> = settings
                        .iter()
                        .enumerate()
                        .filter(|(_, s)| {
                            s.team
                                && s.avatar.is_none()
                                && self.shown(core, s, Some(t))
                                && self.category.as_ref()
                                    == Some(&(s.add_on.clone(), s.category.clone()))
                        })
                        .collect();
                    let prerequisites: BTreeSet<_> = details
                        .iter()
                        .filter_map(|(_, s)| s.shown_when.as_ref().map(|w| w.setting.clone()))
                        .collect();
                    // Authored semantic types keep common body/loadout picks
                    // ahead of deeper settings. Stable sort retains authored
                    // order within each group and the original control IDs.
                    details.sort_by_key(|(_, s)| {
                        if prerequisites.contains(&s.key) {
                            0
                        } else if matches!(
                            s.kind,
                            MiniGameSettingKind::Item { .. }
                                | MiniGameSettingKind::PlayerType { .. }
                        ) {
                            1
                        } else {
                            2
                        }
                    });
                    for (i, s) in details {
                        let value = team
                            .settings
                            .get(&s.key)
                            .cloned()
                            .unwrap_or_else(|| s.default.clone());
                        self.row(Target::Team(t, i), s, &value, 20, y, editable, core);
                        y += ROW;
                    }
                    for category in Self::look_categories(core) {
                        if !settings.iter().any(|s| {
                            s.team
                                && s.avatar.is_some()
                                && s.category == category
                                && self.shown(core, s, Some(t))
                                && self.category.as_ref()
                                    == Some(&(s.add_on.clone(), s.category.clone()))
                        }) {
                            continue;
                        }
                        let n = self.view.add(
                            rows,
                            push_button(
                                Rect::new(20, y - 2, 180, 24),
                                &format!("Edit {category}"),
                                &format!("AOS_T{t}_Look_{category}"),
                            ),
                        );
                        self.view.set_active(n, editable);
                        y += ROW;
                    }
                }
                Page::Teams => heading(&mut self.view, &mut y, "No teams yet."),
                Page::Players => {
                    if self.teams.iter().any(|t| t.id.is_none()) {
                        heading(
                            &mut self.view,
                            &mut y,
                            "Save new teams before assigning players.",
                        );
                    }
                    y = self.player_rows(core, y, editable);
                }
            }
        }
        if self.server && settings.iter().any(|s| s.server && s.restart) {
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
        let width = self.width - 42;
        self.view.add(
            rows,
            text("GuiTextProfile", Rect::new(4, y, width - 8, 22), "Players"),
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
                text("GuiTextProfile", Rect::new(20, y, width - 234, 20), name),
            );
            let pick = format!("AOS_P{}_Team", id.0);
            let n = self
                .view
                .add(rows, popup(Rect::new(width - 208, y, 118, 20), &pick));
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
        let width = self.width - 42;
        self.view.add(
            rows,
            text(
                "GuiTextProfile",
                Rect::new(x, y, 200 - x + 20, 20),
                &title(s),
            ),
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
                let team = match target {
                    Target::Game(_) => None,
                    Target::Team(t, _) => Some(t),
                };
                self.view.set_text(
                    n,
                    self.typed
                        .get(&(team, s.key.clone()))
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
        // The host changes server settings, whoever may change a game's.
        self.view
            .set_active(n, editable && (self.server || !self.locked(core, &s.key)));
        if !s.help.is_empty() {
            let i = match target {
                Target::Game(i) | Target::Team(_, i) => i,
            };
            self.view.add(
                rows,
                push_button(
                    Rect::new(width - 30, y - 1, 22, 22),
                    "?",
                    &format!("AOS_H{i}"),
                ),
            );
        }
        self.rows.push((target, name));
    }

    /// Validate visible and retained hidden text together before committing it
    /// to the typed draft. Browsing away never silently replaces invalid input.
    fn read_fields(&mut self, core: &Core) -> Result<(), String> {
        self.capture_fields();
        let mut values = Vec::new();
        let mut names = Vec::new();
        for ((team, key), text) in &self.typed {
            if key.is_empty() {
                if let Some(t) = team.filter(|t| *t < self.teams.len()) {
                    if text.trim().is_empty() || text.chars().count() > 50 {
                        return Err(format!(
                            "Team {} needs a name of 1 to 50 characters.",
                            t + 1
                        ));
                    }
                    names.push((t, text.clone()));
                }
                continue;
            }
            let Some((i, s)) = core
                .minigames
                .addon_settings
                .iter()
                .enumerate()
                .find(|(_, s)| &s.key == key)
            else {
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
            let target = match team {
                Some(t) => Target::Team(*t, i),
                None => Target::Game(i),
            };
            values.push((target, key.clone(), value));
        }
        for (target, key, value) in values {
            self.set(target, &key, value);
        }
        for (t, name) in names {
            self.teams[t].name = name;
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
                    let before =
                        t.id.and_then(|id| self.base.1.iter().find(|b| b.id == Some(id)));
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

    /// Editing a captured draft while its acknowledgment is outstanding would
    /// let that acknowledgment mark a different draft as applied.
    fn mutation_control(name: &str) -> bool {
        matches!(
            name,
            FAVS | "AOS_FavLoad"
                | "AOS_FavSave"
                | NOTIFY
                | APPLY
                | APPLY_RESET
                | RESET
                | END
                | "AOS_AddTeam"
        ) || ["AOS_S", "AOS_T", "AOS_P"].into_iter().any(|prefix| {
            name.strip_prefix(prefix)
                .and_then(|suffix| suffix.chars().next())
                .is_some_and(|c| c.is_ascii_digit())
        })
    }
    fn status(&mut self, core: &Core, message: Option<&str>) {
        let editable = self.editable(core);
        let (settings, teams) = self.changes(core);
        let changed = self.typed_dirty
            || !settings.is_empty()
            || teams.is_some()
            || self
                .rules
                .as_ref()
                .is_some_and(|r| self.summary(core).is_some_and(|g| g.rules != *r));
        let text = match message {
            Some(m) => m.to_owned(),
            None if !core.minigames.status.is_empty() && self.request.is_some() => {
                core.minigames.status.clone()
            }
            None if !core.admin.status.is_empty() && self.server && self.request.is_some() => {
                core.admin.status.clone()
            }
            None if !editable && self.server => "Only the host can change these.".into(),
            None if !editable && self.summary(core).is_some() => {
                "Only the mini-game's owner or an admin can change these.".into()
            }
            None if changed && self.draft => "Save keeps these for Create.".into(),
            None if changed => "Not applied yet.".into(),
            None => String::new(),
        };
        if let Some(n) = self.view.id(STATUS) {
            self.view.set_text(n, text.replace(['<', '>'], ""));
        }
        let ready = editable && self.request.is_none() && self.open(core);
        for node in self.view.walk().collect::<Vec<_>>() {
            let name = self.view.node(node).ctrl.name.as_deref().unwrap_or("");
            if !Self::mutation_control(name) {
                continue;
            }
            let favorite = matches!(name, FAVS | "AOS_FavLoad" | "AOS_FavSave");
            let locked = self
                .view
                .node(node)
                .ctrl
                .field("settingKey")
                .is_some_and(|key| !self.server && self.locked(core, key));
            self.view.set_active(
                node,
                if favorite {
                    self.request.is_none()
                } else {
                    ready && !locked
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

        for button in [APPLY, APPLY_RESET, RESET, END, NOTIFY] {
            if let Some(n) = self.view.id(button) {
                // Server settings only apply: there is no game to reset. Nor
                // before Create: Save keeps the draft.
                let shown = editable && (button == APPLY || !(self.server || self.draft));
                self.view.set_active(n, ready && shown);
                self.view.set_visible(n, shown);
            }
        }
        if let Some(n) = self.view.id("AOS_NotifyLabel") {
            self.view
                .set_visible(n, editable && !(self.server || self.draft));
        }
        if let Some(n) = self.view.id("AOS_AddTeam") {
            self.view.set_active(n, ready && self.teams_shown(core));
        }
    }

    fn apply(&mut self, core: &mut Core, reset: bool) {
        if let Err(e) = self.read_fields(core) {
            self.status(core, Some(&e));
            return;
        }
        if self.draft {
            core.minigame_addon_draft = Some(AddOnFavorite {
                rules: None,
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
            });
            core.minigames.status = "Add-On settings are sent when you press Create.".into();
            Self::close(core);
            return;
        }
        let (settings, teams) = self.changes(core);
        if self.server {
            if settings.is_empty() {
                self.status(core, Some("Nothing has changed."));
            } else {
                self.apply_server(core, settings);
            }
            return;
        }
        let Some(game) = self.game else { return };
        let rules = self
            .rules
            .clone()
            .filter(|r| self.summary(core).is_some_and(|g| g.rules != *r));
        let next = if settings.is_empty() && teams.is_none() {
            reset.then_some((MiniGameOperation::Reset, UiAction::ResetMiniGame { game }))
        } else {
            Some((
                MiniGameOperation::AddOnSettings,
                UiAction::EditMiniGameAddOns {
                    game,
                    settings,
                    teams,
                    quiet: !core.prefs.bool_or(NOTIFY_PREF, true),
                    reset,
                },
            ))
        };
        if let Some(rules) = rules {
            self.after_rules = next;
            self.configuring_rules = true;
            self.request = core.minigame_request(
                MiniGameOperation::Configure,
                UiAction::ConfigureMiniGame { game, rules },
            );
        } else if let Some((operation, action)) = next {
            self.after_rules = None;
            self.configuring_rules = false;
            self.request = core.minigame_request(operation, action);
        } else {
            self.status(core, Some("Nothing has changed."));
            return;
        }
        self.applying = self.request.is_some();
        if !self.applying {
            self.after_rules = None;
            self.configuring_rules = false;
        }
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

    /// Server-wide changes go with the host's Server Settings, saved as
    /// its `$Pref::Server::*` like the rest.
    fn apply_server(
        &mut self,
        core: &mut Core,
        settings: Vec<(String, Option<MiniGameSettingValue>)>,
    ) {
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
    fn layout(&mut self, w: i32, h: i32, core: &mut Core) {
        let _ = self.read_fields(core);
        let width = (w - 20).clamp(380, W);
        let height = (h - 10).clamp(280, H);
        let compact = width < W;
        self.width = width;
        if let Some(n) = self.view.id("AOS_Window") {
            let c = &mut self.view.nodes[n].ctrl;
            c.position = [(w - width) / 2, (h - height) / 2];
            c.extent = [width, height];
            c.h_sizing = HSizing::Right;
            c.v_sizing = VSizing::Bottom;
        }
        let footer = if compact { 152 } else { 140 };
        let fav_y = height - if compact { 90 } else { 66 };
        let notify_x = if compact { 158 } else { 326 };
        let notify_y = if compact { height - 68 } else { fav_y };
        let positions = [
            (SCROLL, Rect::new(12, 94, width - 24, height - footer - 62)),
            ("AOS_AddTeam", Rect::new(width - 122, 32, 110, 24)),
            (TEAM_PICK, Rect::new(12, 62, (width - 30) / 2, 20)),
            (
                CATEGORY,
                if self.page == Page::Teams {
                    Rect::new(18 + (width - 30) / 2, 62, (width - 30) / 2, 20)
                } else {
                    Rect::new(12, 62, width - 24, 20)
                },
            ),
            (STATUS, Rect::new(12, height - footer + 36, width - 24, 26)),
            ("AOS_FavLabel", Rect::new(12, fav_y, 70, 20)),
            (FAVS, Rect::new(84, fav_y, 110, 20)),
            ("AOS_FavLoad", Rect::new(198, fav_y - 2, 56, 24)),
            ("AOS_FavSave", Rect::new(258, fav_y - 2, 56, 24)),
            (NOTIFY, Rect::new(notify_x, notify_y, 20, 20)),
            (
                "AOS_NotifyLabel",
                Rect::new(notify_x + 22, notify_y, width - notify_x - 34, 20),
            ),
            (
                RESET,
                Rect::new(12, height - if compact { 68 } else { 36 }, 64, 28),
            ),
            (
                END,
                Rect::new(80, height - if compact { 68 } else { 36 }, 64, 28),
            ),
            (
                "AOS_Close",
                Rect::new(if compact { 12 } else { width - 316 }, height - 36, 80, 28),
            ),
            (
                APPLY_RESET,
                Rect::new(if compact { 96 } else { width - 232 }, height - 36, 120, 28),
            ),
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
        self.after_rules = None;
        self.configuring_rules = false;
        self.applying = false;
    }
    fn on_wake(&mut self, core: &mut Core) {
        self.take_look(core);
    }
    fn on_update(&mut self, core: &mut Core) {
        // A server change is answered through the admin state: done once
        // the host no longer has it pending.
        if self.server
            && let Some(id) = self.request
            && !core.admin.pending.contains_key(&id)
        {
            self.request = None;
            if core.admin.status.starts_with("Rejected") {
                let status = core.admin.status.clone();
                self.status(core, Some(&status));
            } else {
                self.typed_dirty = false;
                self.base = (self.values.clone(), Vec::new());
                self.status(core, Some("Applied."));
            }
        }
        self.take_look(core);
        // The host's values changed (someone else applied, or ours landed):
        // start again from them, unless the player is mid-edit.
        let revision = self.revision(core);
        if self.seen != Some(revision) {
            let (settings, teams) = self.changes(core);
            let rules_dirty = self
                .rules
                .as_ref()
                .is_some_and(|r| self.summary(core).is_some_and(|g| g.rules != *r));
            if self.request.is_none()
                && !self.typed_dirty
                && settings.is_empty()
                && teams.is_none()
                && !rules_dirty
            {
                self.load(core);
            } else {
                self.seen = Some(revision);
                self.build(core);
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
        let configuring_rules = std::mem::take(&mut self.configuring_rules);
        if result.is_ok()
            && configuring_rules
            && let Some((operation, action)) = self.after_rules.take()
        {
            self.request = core.minigame_request(operation, action);
            if self.request.is_none() {
                self.applying = false;
                let reason = format!(
                    "Vanilla rules applied; remaining settings/reset could not be sent: {}. Review permissions, then Save again.",
                    core.minigames.status
                );
                self.status(
                    core,
                    Some("Rules applied; remaining changes were not sent. Review permissions."),
                );
                core.message_ok("MiniGame changes incomplete", &reason);
            } else {
                self.status(
                    core,
                    Some("Rules applied. Waiting for remaining changes..."),
                );
            }
            return true;
        }
        self.after_rules = None;
        match result {
            Ok(()) if std::mem::take(&mut self.applying) => {
                core.minigames.status.clear();
                self.typed_dirty = false;
                self.base = (self.values.clone(), self.teams.clone());
                self.status(core, Some("Applied."));
            }
            Ok(()) => {
                core.minigames.status.clear();
                self.status(core, Some("Done."));
            }
            Err(e) => {
                let applying = std::mem::take(&mut self.applying);
                core.minigames.status.clear();
                self.build(core);
                let reason = if configuring_rules {
                    format!(
                        "Vanilla rules were not confirmed: {e}. Remaining settings/reset were not sent. Review the favorite's player type and equipment, then Save again."
                    )
                } else if applying {
                    format!(
                        "Settings/reset were not confirmed: {e}. Review the draft and Save again."
                    )
                } else {
                    e.clone()
                };
                self.status(
                    core,
                    Some(if configuring_rules {
                        "Rules were not confirmed. Remaining changes were not sent."
                    } else if applying {
                        "Settings/reset were not confirmed. Review the draft and try Save again."
                    } else {
                        e.as_str()
                    }),
                );
                if applying || configuring_rules {
                    core.message_ok("MiniGame changes incomplete", &reason);
                }
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
        if key == Key::Delete && self.request.is_some() {
            return true;
        }
        if key == Key::Delete && self.editable(core) {
            let focused = self.view.focus.and_then(|n| {
                let ctrl = &self.view.node(n).ctrl;
                (ctrl.class != "GuiTextEditCtrl")
                    .then(|| ctrl.name.clone())
                    .flatten()
            });
            if let Some(t) = focused
                .as_deref()
                .and_then(|name| name.strip_prefix("AOS_T"))
                .and_then(|r| r.split('_').next())
                .and_then(|t| t.parse::<usize>().ok())
                && t < self.teams.len()
            {
                let _ = self.read_fields(core);
                self.remove_team(t);
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
        let row = self.rows.iter().find(|(_, n)| *n == name).map(|(t, _)| *t);
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
            if name == TEAM_PICK {
                self.capture_fields();
                self.selected_team = self
                    .view
                    .selected(ev.node)
                    .and_then(|i| usize::try_from(i).ok())
                    .unwrap_or(0);
                if let Some(n) = self.view.id(SCROLL) {
                    self.view.state(n).scroll_y = 0;
                }
                self.build(core);
                return;
            }
            if let Some(player) = name
                .strip_prefix("AOS_P")
                .and_then(|r| r.strip_suffix("_Team"))
                .and_then(|p| p.parse::<u64>().ok())
            {
                let _ = self.read_fields(core);
                self.move_player(core, MiniGamePlayerId(player), self.view.selected(ev.node));
                return;
            }
            if name == FAVS {
                return;
            }
            if self.view.node(ev.node).ctrl.class == "GuiTextEditCtrl" {
                self.typed_dirty = true;
                let _ = self.read_fields(core);
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
                    && let MiniGameSettingKind::List { items }
                    | MiniGameSettingKind::Item { items }
                    | MiniGameSettingKind::PlayerType { items } = &s.kind
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
                && let Some(c) = self
                    .view
                    .selected(ev.node)
                    .and_then(|c| u8::try_from(c).ok())
            {
                let _ = self.read_fields(core);
                if let Some(team) = self.teams.get_mut(t) {
                    team.color = c;
                }
                self.build(core);
            }
            self.status(core, None);
            return;
        }
        if !matches!(ev.kind, EventKind::Click | EventKind::Submit) {
            return;
        }
        let command = command_of(&self.view, ev.node);
        match command.as_str() {
            "AOS_Setup" | "AOS_Teams" | "AOS_Players" => {
                self.capture_fields();
                self.page = match command.as_str() {
                    "AOS_Teams" => Page::Teams,
                    "AOS_Players" => Page::Players,
                    _ => Page::Setup,
                };
                self.category = None;
                if let Some(n) = self.view.id(SCROLL) {
                    self.view.state(n).scroll_y = 0;
                }
                self.build(core);
            }
            "AOS_Close" => Self::close(core),
            APPLY => self.apply(core, false),
            APPLY_RESET => self.apply(core, true),
            RESET => {
                if let Some(game) = self.game {
                    self.request = core.minigame_request(
                        MiniGameOperation::Reset,
                        UiAction::ResetMiniGame { game },
                    );
                    self.status(core, None);
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
                self.selected_team = self.teams.len() - 1;
                self.page = Page::Teams;
                if let Some(n) = self.view.id(SCROLL) {
                    self.view.state(n).scroll_y = 0;
                }
                self.build(core);
            }
            _ => {
                if let Some((t, category)) = command
                    .strip_prefix("AOS_T")
                    .and_then(|r| r.split_once("_Look_"))
                    .and_then(|(t, c)| Some((t.parse::<usize>().ok()?, c.to_owned())))
                {
                    self.edit_look(core, t, &category);
                } else if let Some(i) = command
                    .strip_prefix("AOS_H")
                    .and_then(|i| i.parse::<usize>().ok())
                {
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
                        self.remove_team(t);
                    }
                    self.build(core);
                } else if let Some(Target::Game(i) | Target::Team(_, i)) = row
                    && let Some(s) = core.minigames.addon_settings.get(i).cloned()
                    && s.kind == MiniGameSettingKind::Bool
                {
                    let on = self.view.bool_value(ev.node);
                    let _ = self.read_fields(core);
                    self.set(
                        row.expect("matched"),
                        &s.key,
                        MiniGameSettingValue::Bool(on),
                    );
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
fn title(s: &MiniGameAddOnSetting) -> String {
    if s.restart {
        format!("{} *", s.title)
    } else {
        s.title.clone()
    }
}
