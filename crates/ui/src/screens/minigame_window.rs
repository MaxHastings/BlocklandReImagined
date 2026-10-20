//! The Mini-Game window: v20's Create Mini-Game window (`CreateMiniGameGui`)
//! with the running Add-Ons' settings and teams beside its rules, on five
//! pages a host works through: Game, Teams, Rules, Players and More
//! Settings. It keeps the v20 window and its controls (its sections,
//! popups, check boxes, Set Favs and footer); only where things sit is
//! new. Slayer brought a second window of its own; here an Add-On says
//! which section each of its settings joins (`place`), and every setting
//! also shows on More Settings, by Add-On and category.
use super::minigame_addons::{takes, title, value_text};
use super::*;
use crate::{
    api::*,
    ui::Callback,
    view::{EventKind, Value},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Page {
    Game,
    Teams,
    Rules,
    Players,
    More,
}
const PAGES: [(Page, &str); 5] = [
    (Page::Game, "Game"),
    (Page::Teams, "Teams"),
    (Page::Rules, "Rules"),
    (Page::Players, "Players"),
    (Page::More, "More Settings"),
];
const TAB: &str = "MGW_Tab";
/// Where the v20 controls wait while their page is not showing.
const HOLDER: &str = "MGW_Holder";
const SCROLL: &str = "CMG_Scroll";
const BOX: &str = "CMG_ScrollBox";
const CATEGORY: &str = "AOS_Category";
const TEAM_PICK: &str = "AOS_Team";
const NOTIFY: &str = "AOS_Notify";
const ADD_TEAM: &str = "AOS_AddTeam";
/// The editor's choice to tell the game's players what they change
/// (Slayer's Notify Players on Update).
pub const NOTIFY_PREF: &str = "$Pref::AddOnSettings::NotifyPlayers";
/// A player row's team pick: not in the game, or on no team.
const OUT_OF_GAME: i64 = -2;
const NO_TEAM: i64 = -1;

const CREATE: &str = "createminigamegui.clickcreate();";
const RESET: &str = "createminigamegui.clickreset();";
const END: &str = "createminigamegui.clickend();";
const CANCEL: &str = "canvas.popdialog(createminigamegui);";
const SET_FAVS: &str = "createminigamegui.clicksetfavs();";

/// The v20 rules' text boxes: variable, label, words after.
const EDITS: [(&str, &str, &str); 9] = [
    ("$MiniGame::Title", "Game Title", ""),
    (
        "$MiniGame::Points::KillPlayer",
        "Killing a Player",
        "points",
    ),
    ("$MiniGame::Points::KillSelf", "Killing Yourself", "points"),
    ("$MiniGame::Points::Die", "Dying", "points"),
    (
        "$MiniGame::Points::BreakBrick",
        "Breaking a Brick",
        "points",
    ),
    (
        "$MiniGame::Points::PlantBrick",
        "Planting a Brick",
        "points",
    ),
    ("$MiniGame::RespawnTime", "Players", "seconds"),
    ("$MiniGame::VehicleRespawnTime", "Vehicles", "seconds"),
    ("$MiniGame::BrickRespawnTime", "Bricks", "seconds"),
];
/// The v20 rules' check boxes: variable, label.
const CHECKS: [(&str, &str); 12] = [
    ("$MiniGame::InviteOnly", "Invite-Only"),
    ("$MiniGame::EnableBuilding", "Enable Building"),
    ("$MiniGame::EnablePainting", "Enable Painting"),
    ("$MiniGame::EnableWand", "Enable Wand"),
    ("$MiniGame::WeaponDamage", "Weapon Damage"),
    ("$MiniGame::FallingDamage", "Falling Damage"),
    ("$MiniGame::SelfDamage", "Self Damage"),
    ("$MiniGame::VehicleDamage", "Vehicle Damage"),
    ("$MiniGame::BrickDamage", "Brick Damage"),
    ("$MiniGame::UseSpawnBricks", "Use Spawn Bricks"),
    (
        "$MiniGame::UseAllPlayersBricks",
        "Include All Player's Bricks",
    ),
    ("$MiniGame::PlayersUseOwnBricks", "Players Use Own Bricks"),
];
const COLOR: &str = "CMG_ColorList";
const SWATCH: &str = "CMG_Swatch";
const PLAYER_TYPE: &str = "CMG_PlayerDataBlock";
fn equip(i: usize) -> String {
    format!("CMG_StartEquip{i}")
}

// The v20 Create window's sections, rows and columns.
const SECTION_W: i32 = 497;
const LABEL_X: i32 = 10;
const LABEL_W: i32 = 150;
const FIELD_X: i32 = 168;
const PICK_W: i32 = 150;
const CHECK_X: i32 = 22;
const CHECK_STEP: i32 = 158;
const TOOL_STEP: i32 = 108;

fn named(mut c: Control, name: &str) -> Control {
    c.name = Some(name.into());
    c
}
fn small_button(r: Rect, label: &str, name: &str) -> Control {
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
fn edit(r: Rect, name: &str) -> Control {
    named(ctrl("GuiTextEditCtrl", "GuiTextEditProfile", r), name)
}
fn popup(r: Rect, name: &str) -> Control {
    named(ctrl("GuiPopUpMenuCtrl", "BlockButtonProfile", r), name)
}
fn check(r: Rect, name: &str, label: &str) -> Control {
    let mut c = named(ctrl("GuiCheckBoxCtrl", "GuiCheckBoxProfile", r), name);
    c.text = Some(label.into());
    c.command = Some(name.into());
    c
}
fn palette_rgba(rgb: [u8; 3]) -> Rgba {
    rgba([
        f32::from(rgb[0]) / 255.0,
        f32::from(rgb[1]) / 255.0,
        f32::from(rgb[2]) / 255.0,
        1.0,
    ])
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

/// One team as the window leaves it.
#[derive(Debug, Clone, PartialEq)]
struct DraftTeam {
    id: Option<u32>,
    name: String,
    color: u8,
    settings: BTreeMap<String, MiniGameSettingValue>,
}

/// What an Add-On row's control edits.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Target {
    Game(usize),
    /// Team (by place in the draft), setting.
    Team(usize, usize),
}

/// What the request out is for.
#[derive(Debug, Clone)]
enum Step {
    /// Create: the Add-On settings follow once the game exists.
    Create,
    /// The v20 rules, then the Add-On settings or a reset.
    Rules(Option<Box<(MiniGameOperation, UiAction)>>),
    /// The last of an Update or Reset: done closes the window.
    Last,
    /// A player moved, removed or invited: the window stays.
    Players,
}

/// A section being laid out: its control and the next row's top.
struct Section {
    node: NodeId,
    y: i32,
}

/// Settings by key (`None`: back to the default), and the team list.
type Changes = (
    Vec<(String, Option<MiniGameSettingValue>)>,
    Option<Vec<MiniGameTeamEdit>>,
);

pub struct MiniGameWindow {
    view: View,
    page: Page,
    /// The game it edits; none: the one Create makes.
    game: Option<MiniGameId>,
    /// The game the window last took its values from.
    loaded: Option<Option<MiniGameId>>,
    seen: Option<u64>,
    /// The v20 controls, kept (with what was typed) across pages.
    vanilla: Vec<NodeId>,
    resources: BTreeMap<NodeId, Vec<Option<String>>>,
    rules_dirty: bool,
    /// Set Favs is on: the next slot saves the window there.
    setting_favs: bool,
    /// Effective values (defaults filled in) of the game's own settings.
    values: BTreeMap<String, MiniGameSettingValue>,
    teams: Vec<DraftTeam>,
    /// The host's values when the window took them, to tell what changed.
    base: (BTreeMap<String, MiniGameSettingValue>, Vec<DraftTeam>),
    rows: Vec<(Target, String)>,
    /// Raw text of number, text and team name boxes, kept while it is not
    /// yet valid and across pages; Update checks every one.
    typed: BTreeMap<(Option<usize>, String), String>,
    typed_dirty: bool,
    name_fields: Vec<(usize, String)>,
    categories: Vec<(String, String, bool)>,
    category: Option<(String, String, bool)>,
    selected_team: usize,
    request: Option<RequestId>,
    step: Option<Step>,
    /// A favourite's slot whose unavailable settings were left out: Set
    /// Favs may not save over it.
    unavailable_favorite: Option<u8>,
    /// The game picked in the Mini-Game list, when the window opened from
    /// it.
    picked: Option<MiniGameId>,
}

impl MiniGameWindow {
    pub fn new(core: &mut Core) -> Self {
        let teams_route = std::mem::take(&mut core.minigame_addons_teams);
        let picked = core.minigame_addons.take();
        let mut view = layout_view(core, "CreateMiniGameGui");
        let win = window(&view).unwrap_or(view.root);
        // The End blocker greys End out, so it must draw over the button.
        if let Some(n) = view.id("CMG_EndBlocker") {
            view.push_to_back(n);
        }
        let mut holder = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 10, 10));
        holder.visible = false;
        let holder = view.add(win, named(holder, HOLDER));
        // The page tabs, under the title bar where the scroll began.
        let width = view.node(win).ctrl.extent[0].max(534);
        let count = PAGES.len() as i32;
        let tab_w = (width - 16 - 4 * (count - 1)) / count;
        for (i, (_, label)) in PAGES.iter().enumerate() {
            let name = format!("{TAB}{i}");
            view.add(
                win,
                small_button(
                    Rect::new(8 + (tab_w + 4) * i as i32, 30, tab_w, 26),
                    label,
                    &name,
                ),
            );
        }
        let scroll = match view.id(SCROLL) {
            Some(n) => n,
            None => {
                let mut s = named(
                    ctrl(
                        "GuiScrollCtrl",
                        "BlockScrollProfile",
                        Rect::new(8, 60, 519, 319),
                    ),
                    SCROLL,
                );
                s.fields.insert("hScrollBar".into(), "alwaysOff".into());
                s.fields.insert("vScrollBar".into(), "alwaysOn".into());
                view.add(win, s)
            }
        };
        view.nodes[scroll].ctrl.position = [8, 60];
        view.nodes[scroll].ctrl.extent = [519, 319];
        if view.id(BOX).is_none() {
            view.add(
                scroll,
                named(swatch(Rect::new(1, 0, 506, 10), [200, 200, 200, 255]), BOX),
            );
        }
        // Every v20 control, from the layout or made like it when the
        // layout lacks it; they wait in the holder.
        let mut vanilla = Vec::new();
        let find_var = |view: &View, var: &str| {
            view.walk().find(|&n| {
                view.node(n)
                    .ctrl
                    .variable
                    .as_deref()
                    .is_some_and(|v| v.eq_ignore_ascii_case(var))
            })
        };
        for (var, _, _) in EDITS {
            let n = match find_var(&view, var) {
                Some(n) => n,
                None => {
                    let mut c = ctrl(
                        "GuiTextEditCtrl",
                        "GuiTextEditProfile",
                        Rect::new(0, 0, 36, 18),
                    );
                    c.variable = Some(var.into());
                    view.add(holder, c)
                }
            };
            vanilla.push(n);
        }
        for (var, label) in CHECKS {
            let n = match find_var(&view, var) {
                Some(n) => n,
                None => {
                    let mut c = ctrl(
                        "GuiCheckBoxCtrl",
                        "GuiCheckBoxProfile",
                        Rect::new(0, 0, 150, 18),
                    );
                    c.variable = Some(var.into());
                    c.text = Some(label.into());
                    view.add(holder, c)
                }
            };
            vanilla.push(n);
        }
        let mut pickers = vec![COLOR.to_owned(), PLAYER_TYPE.to_owned()];
        pickers.extend((0..5).map(equip));
        for name in pickers {
            let n = match view.id(&name) {
                Some(n) => n,
                None => view.add(holder, popup(Rect::new(0, 0, PICK_W, 20), &name)),
            };
            vanilla.push(n);
        }
        let n = match view.id(SWATCH) {
            Some(n) => n,
            None => view.add(
                holder,
                named(
                    swatch(Rect::new(0, 0, 38, 16), [255, 255, 255, 255]),
                    SWATCH,
                ),
            ),
        };
        vanilla.push(n);
        for &n in &vanilla {
            view.reparent(n, holder, [0, 0]);
        }
        // The colour blocker stood over the colour list in v20; the list
        // greys itself here.
        if let Some(n) = view.id("CMG_ColorBlocker") {
            view.reparent(n, holder, [0, 0]);
        }
        view.measure(&core.pack);
        let mut screen = Self {
            view,
            page: if teams_route { Page::Teams } else { Page::Game },
            game: None,
            loaded: None,
            seen: None,
            vanilla,
            resources: BTreeMap::new(),
            rules_dirty: false,
            setting_favs: false,
            values: BTreeMap::new(),
            teams: Vec::new(),
            base: (BTreeMap::new(), Vec::new()),
            rows: Vec::new(),
            typed: BTreeMap::new(),
            typed_dirty: false,
            name_fields: Vec::new(),
            categories: Vec::new(),
            category: None,
            selected_team: 0,
            request: None,
            step: None,
            unavailable_favorite: None,
            picked,
        };
        screen.set_favs(false);
        screen.load(core);
        screen
    }

    // ------------------------------------------------------------ state

    /// The game the window edits: the player's own, when they may manage
    /// it; none, to create one.
    /// The game the window shows: the one picked in the list while it is
    /// listed, else the player's own when they may manage it, else a new one.
    fn target(&self, core: &Core) -> Option<MiniGameId> {
        self.picked
            .filter(|p| core.minigames.games.iter().any(|g| g.id == *p))
            .or_else(|| {
                core.minigames
                    .active_game
                    .filter(|g| core.minigames.can_manage(*g))
            })
    }
    fn summary<'a>(&self, core: &'a Core) -> Option<&'a MiniGameSummary> {
        self.game
            .and_then(|id| core.minigames.games.iter().find(|g| g.id == id))
    }
    fn setting<'a>(core: &'a Core, key: &str) -> Option<&'a MiniGameAddOnSetting> {
        core.minigames.addon_settings.iter().find(|s| s.key == key)
    }
    /// Whether the player may change what the window shows.
    fn editable(&self, core: &Core) -> bool {
        match self.game {
            Some(g) => core
                .minigames
                .can_on(crate::models::minigames::Operation::Configure, Some(g)),
            None => core
                .minigames
                .can(crate::models::minigames::Operation::Create),
        }
    }
    /// Whether the host lets the player change the game's Add-On settings
    /// and teams (a game's rules may be theirs to change while these are
    /// not).
    fn addons_editable(&self, core: &Core) -> bool {
        self.game
            .is_none_or(|g| core.minigames.addon_editable.contains(&g))
    }
    /// Whether the player lacks the level an Add-On setting needs.
    fn locked(&self, core: &Core, key: &str) -> bool {
        match self.game {
            None => core.minigames.addon_locked_new.iter().any(|k| k == key),
            Some(g) => {
                !self.addons_editable(core)
                    || core
                        .minigames
                        .addon_locked
                        .iter()
                        .any(|(id, keys)| *id == g && keys.iter().any(|k| k == key))
            }
        }
    }
    /// What the window shows changed when this does. A score does not
    /// count: a point scored mid-edit must not rebuild the rows.
    fn revision(core: &Core) -> u64 {
        use std::hash::{Hash, Hasher};
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
    fn dirty(&self, core: &Core) -> bool {
        let (settings, teams) = self.changes(core);
        self.rules_dirty || self.typed_dirty || !settings.is_empty() || teams.is_some()
    }

    /// Take the host's values afresh: the game's rules, settings and teams,
    /// or a new game's defaults.
    fn load(&mut self, core: &Core) {
        self.game = self.target(core);
        // The window stays on its game while it is listed, though the
        // player's rights to change it come and go.
        self.picked = self.picked.or(self.game);
        self.loaded = Some(self.game);
        self.seen = Some(Self::revision(core));
        self.rows.clear();
        self.typed.clear();
        self.name_fields.clear();
        self.typed_dirty = false;
        self.rules_dirty = false;
        self.unavailable_favorite = None;
        let rules = self
            .summary(core)
            .map_or_else(|| core.minigames.rules_draft(), |g| g.rules.clone());
        self.write_rules(core, &rules);
        self.fill_colors(core, self.summary(core).map(|g| g.color));
        let declared = || {
            core.minigames
                .addon_settings
                .iter()
                .filter(|s| !s.team && !s.server)
        };
        let team_values = |stored: &BTreeMap<String, MiniGameSettingValue>| {
            core.minigames
                .addon_settings
                .iter()
                .filter(|s| s.team)
                .map(|s| {
                    let v = stored.get(&s.key).cloned();
                    (s.key.clone(), v.unwrap_or_else(|| s.default.clone()))
                })
                .collect::<BTreeMap<_, _>>()
        };
        match self.summary(core) {
            Some(g) => {
                let values: BTreeMap<_, _> = declared()
                    .map(|s| {
                        let v = g.addon_settings.get(&s.key).cloned();
                        (s.key.clone(), v.unwrap_or_else(|| s.default.clone()))
                    })
                    .collect();
                let teams: Vec<_> = g
                    .teams
                    .iter()
                    .map(|t| DraftTeam {
                        id: Some(t.id),
                        name: t.name.clone(),
                        color: t.color,
                        settings: team_values(&t.settings),
                    })
                    .collect();
                self.values = values.clone();
                self.teams = teams.clone();
                self.base = (values, teams);
            }
            None => {
                let defaults: BTreeMap<_, _> = declared()
                    .map(|s| (s.key.clone(), s.default.clone()))
                    .collect();
                self.values = defaults.clone();
                self.teams.clear();
                // What differs from the defaults goes to the new game.
                self.base = (defaults, Vec::new());
            }
        }
        let editing = self.game.is_some();
        if let Some(n) = self.view.id("CMG_Window") {
            self.view.set_text(
                n,
                if editing {
                    "Edit Mini-Game"
                } else {
                    "Create Mini-Game"
                },
            );
        }
        if let Some(n) = self.view.id("CMG_CreateButton") {
            self.view
                .set_text(n, if editing { "Update >>" } else { "Create >>" });
        }
        self.build(core);
    }

    // -------------------------------------------------------- v20 rules

    fn variable(&self, key: &str) -> Option<NodeId> {
        self.vanilla.iter().copied().find(|&n| {
            self.view
                .node(n)
                .ctrl
                .variable
                .as_deref()
                .is_some_and(|v| v.eq_ignore_ascii_case(key))
        })
    }
    fn set_var(&mut self, key: &str, value: &str) {
        if let Some(n) = self.variable(key) {
            if matches!(self.view.node(n).state.value, Value::Bool(_)) {
                self.view
                    .set_bool(n, value == "1" || value.eq_ignore_ascii_case("true"));
            } else {
                self.view.set_text(n, value);
            }
        }
    }
    /// Keep saved ids outside popup indices, unavailable ones included.
    fn fill_resource(
        &mut self,
        name: &str,
        choices: &[MiniGameChoice],
        selected: Option<&str>,
        none: bool,
    ) {
        if let Some(n) = self.view.id(name) {
            let ids = resource_choices(
                &mut self.view,
                n,
                choices.iter().map(|c| (c.id.as_str(), c.name.as_str())),
                selected,
                none,
            );
            self.resources.insert(n, ids);
        }
    }
    fn selected_resource(&self, name: &str) -> Option<String> {
        let node = self.view.id(name)?;
        let index = usize::try_from(self.view.selected(node)?).ok()?;
        self.resources.get(&node)?.get(index)?.clone()
    }
    fn write_rules(&mut self, core: &Core, rules: &MiniGameRules) {
        for (key, value) in [
            ("$MiniGame::Title", rules.title.clone()),
            (
                "$MiniGame::Points::BreakBrick",
                rules.points_break_brick.to_string(),
            ),
            (
                "$MiniGame::Points::PlantBrick",
                rules.points_plant_brick.to_string(),
            ),
            (
                "$MiniGame::Points::KillPlayer",
                rules.points_kill_player.to_string(),
            ),
            (
                "$MiniGame::Points::KillSelf",
                rules.points_kill_self.to_string(),
            ),
            ("$MiniGame::Points::Die", rules.points_die.to_string()),
            ("$MiniGame::RespawnTime", rules.respawn_seconds.to_string()),
            (
                "$MiniGame::VehicleRespawnTime",
                rules.vehicle_respawn_seconds.to_string(),
            ),
            (
                "$MiniGame::BrickRespawnTime",
                rules.brick_respawn_seconds.to_string(),
            ),
        ] {
            self.set_var(key, &value);
        }
        for (key, value) in [
            ("$MiniGame::InviteOnly", rules.invite_only),
            (
                "$MiniGame::UseAllPlayersBricks",
                rules.use_all_players_bricks,
            ),
            (
                "$MiniGame::PlayersUseOwnBricks",
                rules.players_use_own_bricks,
            ),
            ("$MiniGame::UseSpawnBricks", rules.use_spawn_bricks),
            ("$MiniGame::FallingDamage", rules.falling_damage),
            ("$MiniGame::WeaponDamage", rules.weapon_damage),
            ("$MiniGame::SelfDamage", rules.self_damage),
            ("$MiniGame::VehicleDamage", rules.vehicle_damage),
            ("$MiniGame::BrickDamage", rules.brick_damage),
            ("$MiniGame::EnableWand", rules.enable_wand),
            ("$MiniGame::EnableBuilding", rules.enable_building),
            ("$MiniGame::EnablePainting", rules.enable_painting),
        ] {
            self.set_var(key, if value { "1" } else { "0" });
        }
        let types = core.minigames.player_types.clone();
        self.fill_resource(PLAYER_TYPE, &types, Some(&rules.player_type), false);
        let items = core.minigames.items.clone();
        for i in 0..5 {
            self.fill_resource(&equip(i), &items, rules.loadout[i].as_deref(), true);
        }
    }
    /// The colour list, at `selected` (a v20 colour index), or the first.
    fn fill_colors(&mut self, core: &Core, selected: Option<u8>) {
        if let Some(n) = self.view.id(COLOR) {
            self.view.state(n).items = core
                .minigames
                .colors
                .iter()
                .map(|c| (c.name.clone(), i64::from(c.index)))
                .collect();
            self.view.select(
                n,
                selected
                    .map(i64::from)
                    .or_else(|| core.minigames.colors.first().map(|c| i64::from(c.index))),
            );
        }
        self.tint_swatch(core);
    }
    fn tint_swatch(&mut self, core: &Core) {
        let selected = self.view.id(COLOR).and_then(|n| self.view.selected(n));
        if let (Some(n), Some(color)) = (
            self.view.id(SWATCH),
            core.minigames
                .colors
                .iter()
                .find(|c| Some(i64::from(c.index)) == selected),
        ) {
            self.view.nodes[n].state.tint = Some([color.rgb[0], color.rgb[1], color.rgb[2], 255]);
        }
    }
    fn read_rules(&self) -> Result<MiniGameRules, String> {
        let val = |key: &str| {
            self.variable(key)
                .map(|n| self.view.edit_text(n))
                .unwrap_or_default()
        };
        let boolean = |key: &str| self.variable(key).is_some_and(|n| self.view.bool_value(n));
        let number = |key: &str, what: &str, min: u32, max: u32| -> Result<u32, String> {
            let value = val(key)
                .trim()
                .parse::<u32>()
                .map_err(|_| format!("Enter a number for {what}."))?;
            if !(min..=max).contains(&value) {
                return Err(format!("{what} must be {min} to {max}."));
            }
            Ok(value)
        };
        let points = |key: &str, what: &str| -> Result<i32, String> {
            val(key)
                .trim()
                .parse()
                .map_err(|_| format!("{what} points must be a whole number."))
        };
        let mut rules = MiniGameRules {
            title: val("$MiniGame::Title"),
            ..MiniGameRules::default()
        };
        if rules.title.trim().is_empty()
            || rules.title.chars().count() > 35
            || rules.title.chars().any(char::is_control)
        {
            return Err("Title must contain 1–35 visible characters.".into());
        }
        rules.points_break_brick = points("$MiniGame::Points::BreakBrick", "Breaking a Brick")?;
        rules.points_plant_brick = points("$MiniGame::Points::PlantBrick", "Planting a Brick")?;
        rules.points_kill_player = points("$MiniGame::Points::KillPlayer", "Killing a Player")?;
        rules.points_kill_self = points("$MiniGame::Points::KillSelf", "Killing Yourself")?;
        rules.points_die = points("$MiniGame::Points::Die", "Dying")?;
        rules.respawn_seconds = number("$MiniGame::RespawnTime", "Player respawn time", 1, 30)?;
        rules.vehicle_respawn_seconds = number(
            "$MiniGame::VehicleRespawnTime",
            "Vehicle respawn time",
            0,
            300,
        )?;
        rules.brick_respawn_seconds =
            number("$MiniGame::BrickRespawnTime", "Brick respawn time", 2, 300)?;
        rules.invite_only = boolean("$MiniGame::InviteOnly");
        rules.use_all_players_bricks = boolean("$MiniGame::UseAllPlayersBricks");
        rules.players_use_own_bricks = boolean("$MiniGame::PlayersUseOwnBricks");
        rules.use_spawn_bricks = boolean("$MiniGame::UseSpawnBricks");
        rules.falling_damage = boolean("$MiniGame::FallingDamage");
        rules.weapon_damage = boolean("$MiniGame::WeaponDamage");
        rules.self_damage = boolean("$MiniGame::SelfDamage");
        rules.vehicle_damage = boolean("$MiniGame::VehicleDamage");
        rules.brick_damage = boolean("$MiniGame::BrickDamage");
        rules.enable_wand = boolean("$MiniGame::EnableWand");
        rules.enable_building = boolean("$MiniGame::EnableBuilding");
        rules.enable_painting = boolean("$MiniGame::EnablePainting");
        if let Some(id) = self.selected_resource(PLAYER_TYPE) {
            rules.player_type = id;
        }
        for i in 0..5 {
            rules.loadout[i] = self.selected_resource(&equip(i));
        }
        Ok(rules)
    }
    fn available_rules(core: &Core, rules: &MiniGameRules) -> Result<(), String> {
        if !core
            .minigames
            .player_types
            .iter()
            .any(|choice| choice.id == rules.player_type)
        {
            return Err(format!(
                "Player type {} is unavailable. Enable its Add-On before hosting, or choose another player type.",
                rules.player_type
            ));
        }
        for (slot, id) in rules.loadout.iter().enumerate() {
            if let Some(id) = id
                && !core.minigames.items.iter().any(|choice| &choice.id == id)
            {
                return Err(format!(
                    "Tool {}: {id} is unavailable. Enable its Add-On before hosting, or choose another item or NONE.",
                    slot + 1
                ));
            }
        }
        Ok(())
    }
    fn color_index(&self) -> u8 {
        self.view
            .id(COLOR)
            .and_then(|n| self.view.selected(n))
            .and_then(|v| u8::try_from(v).ok())
            .unwrap_or(0)
    }

    // ----------------------------------------------------- Add-On rows

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
    /// Whether the game's mode uses teams (Slayer's teams, in a mode with
    /// them).
    fn teams_shown(&self, core: &Core) -> bool {
        let Some(when) = &core.minigames.teams_shown_when else {
            return true;
        };
        let current = self
            .values
            .get(&when.setting)
            .cloned()
            .or_else(|| Self::setting(core, &when.setting).map(|d| d.default.clone()));
        current.is_some_and(|v| when.holds(&v))
    }
    /// What the Teams page says when the draft's mode has no teams, from
    /// the Add-On's own rule: the setting that turns teams on, and the
    /// choices of it that do.
    fn teams_hint(core: &Core) -> String {
        let setting = core
            .minigames
            .teams_shown_when
            .as_ref()
            .and_then(|w| Some((w, Self::setting(core, &w.setting)?)));
        let Some((when, setting)) = setting else {
            return "This game doesn't use teams.".into();
        };
        let title = &setting.title;
        // The page the setting sits on.
        let page = match setting.place {
            Some(
                MiniGameSettingPlace::Basics
                | MiniGameSettingPlace::Loadout
                | MiniGameSettingPlace::Win
                | MiniGameSettingPlace::Rounds
                | MiniGameSettingPlace::Bots,
            ) => "the Game page",
            Some(MiniGameSettingPlace::Teams) => "the Teams page",
            Some(
                MiniGameSettingPlace::Scoring
                | MiniGameSettingPlace::Respawn
                | MiniGameSettingPlace::Damage
                | MiniGameSettingPlace::Bricks,
            ) => "the Rules page",
            None => "More Settings",
        };
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
            [] => format!("This {title} doesn't use teams: change {title} on {page}."),
            [one] => {
                format!("This {title} doesn't use teams: set {title} on {page} to {one}.")
            }
            [rest @ .., last] => format!(
                "This {title} doesn't use teams: set {title} on {page} to {} or {last}.",
                rest.join(", ")
            ),
        }
    }

    fn capture_fields(&mut self) {
        for (target, name) in &self.rows {
            let Some(n) = self.view.id(name) else {
                continue;
            };
            if self.view.node(n).ctrl.class != "GuiTextEditCtrl" {
                continue;
            }
            if let Some(key) = self.view.node(n).ctrl.field("settingKey") {
                let team = match target {
                    Target::Game(_) => None,
                    Target::Team(t, _) => Some(*t),
                };
                self.typed
                    .insert((team, key.into()), self.view.edit_text(n));
            }
        }
        for (t, name) in &self.name_fields {
            if let Some(n) = self.view.id(name) {
                self.typed
                    .insert((Some(*t), String::new()), self.view.edit_text(n));
            }
        }
    }
    /// Check every kept box, shown or not, before taking it into the draft:
    /// leaving a page never drops what was typed.
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
            values.push((*team, key.clone(), value));
        }
        for (team, key, value) in values {
            self.set(team, &key, value);
        }
        for (t, name) in names {
            self.teams[t].name = name;
        }
        Ok(())
    }
    fn set(&mut self, team: Option<usize>, key: &str, value: MiniGameSettingValue) {
        match team {
            None => {
                self.values.insert(key.to_owned(), value);
            }
            Some(t) => {
                if let Some(team) = self.teams.get_mut(t) {
                    team.settings.insert(key.to_owned(), value);
                }
            }
        }
    }
    /// What Update sends: settings that differ from the host's, and the
    /// whole team list when anything about the teams changed.
    fn changes(&self, core: &Core) -> Changes {
        let to_send = |key: &str, value: &MiniGameSettingValue| {
            let default = Self::setting(core, key).map(|s| &s.default);
            (default != Some(value)).then(|| value.clone())
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
        self.name_fields.clear();
        self.selected_team = self.selected_team.min(self.teams.len().saturating_sub(1));
    }
    fn add_team(&mut self, core: &Core) {
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
        // The lowest "Team N" no team has: the rules rename a duplicate
        // with a random number.
        let name = (1..)
            .map(|n| format!("Team {n}"))
            .find(|name| !self.teams.iter().any(|t| t.name.eq_ignore_ascii_case(name)))
            .expect("a free team name");
        self.teams.push(DraftTeam {
            id: None,
            name,
            color,
            settings,
        });
        self.selected_team = self.teams.len() - 1;
    }

    // ------------------------------------------------------------ looks

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
    /// A team's look in `category` as the avatar editor takes it.
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

    // ----------------------------------------------------------- layout

    fn open(&mut self, y: &mut i32, title: &str, bar: Option<[u8; 3]>) -> Section {
        let box_node = self.view.id(BOX).expect("the scroll box");
        let mut s = swatch(Rect::new(2, *y, SECTION_W, 10), [255, 255, 255, 200]);
        s.children
            .push(swatch(Rect::new(2, 2, SECTION_W - 4, 21), [0, 0, 0, 255]));
        if let Some(rgb) = bar {
            s.children
                .push(swatch(Rect::new(2, 2, 6, 21), palette_rgba(rgb)));
        }
        s.children.push(text(
            "OptionsMenuTextProfile",
            Rect::new(if bar.is_some() { 13 } else { 9 }, 3, 400, 18),
            title,
        ));
        let node = self.view.add(box_node, s);
        Section { node, y: 31 }
    }
    fn close(&mut self, sec: Section, y: &mut i32) {
        self.view.nodes[sec.node].ctrl.extent[1] = sec.y + 6;
        *y += sec.y + 9;
    }
    fn label(&mut self, sec: &Section, label: &str) {
        let mut l = ctrl(
            "GuiMLTextCtrl",
            "GuiMLTextProfile",
            Rect::new(LABEL_X, sec.y + 2, LABEL_W, 14),
        );
        l.text = Some(format!("<just:right>{}", label.replace(['<', '>'], "")));
        self.view.add(sec.node, l);
    }
    fn after(&mut self, sec: &Section, x: i32, words: &str) {
        if !words.is_empty() {
            self.view.add(
                sec.node,
                text(
                    "GuiTextProfile",
                    Rect::new(x, sec.y, SECTION_W - x - 34, 18),
                    words,
                ),
            );
        }
    }
    fn note(&mut self, sec: &mut Section, words: &str) {
        let mut l = ctrl(
            "GuiMLTextCtrl",
            "GuiMLTextProfile",
            Rect::new(CHECK_X, sec.y, SECTION_W - 2 * CHECK_X, 14),
        );
        l.text = Some(format!("<color:555555>{}", words.replace(['<', '>'], "")));
        self.view.add(sec.node, l);
        sec.y += 20;
    }
    fn buttons(&mut self, sec: &mut Section, list: &[(&str, &str, bool)]) {
        let mut x = CHECK_X;
        for (label, name, active) in list {
            let w = 14 + 8 * label.len() as i32;
            let n = self.view.add(
                sec.node,
                small_button(Rect::new(x, sec.y, w, 24), label, name),
            );
            self.view.set_active(n, *active);
            x += w + 6;
        }
        sec.y += 30;
    }
    /// Show a v20 control in the section at `pos`, `size` wide and high.
    fn put(&mut self, sec: &Section, n: NodeId, pos: [i32; 2], size: [i32; 2], editable: bool) {
        self.view.reparent(n, sec.node, pos);
        self.view.nodes[n].ctrl.extent = size;
        self.view.set_visible(n, true);
        self.view.set_active(n, editable);
    }
    fn vanilla_edit(&mut self, sec: &mut Section, var: &str, editable: bool) {
        let Some((_, label, unit)) = EDITS.iter().find(|(v, _, _)| *v == var) else {
            return;
        };
        let Some(n) = self.variable(var) else {
            return;
        };
        self.label(sec, label);
        let w = if var == "$MiniGame::Title" { 200 } else { 36 };
        self.put(sec, n, [FIELD_X, sec.y], [w, 18], editable);
        self.after(sec, FIELD_X + w + 6, unit);
        sec.y += 22;
    }
    fn vanilla_pick(&mut self, sec: &mut Section, name: &str, label: &str, active: bool) {
        let Some(n) = self.view.id(name) else {
            return;
        };
        self.label(sec, label);
        self.put(sec, n, [FIELD_X - 1, sec.y - 1], [PICK_W, 20], active);
        sec.y += 23;
    }
    /// Check boxes three to a row: v20's and Add-Ons' alike.
    fn check_grid(&mut self, sec: &mut Section, nodes: &[(NodeId, bool)]) {
        for (i, (n, active)) in nodes.iter().enumerate() {
            let (x, y) = (
                CHECK_X + (i as i32 % 3) * CHECK_STEP,
                sec.y + (i as i32 / 3) * 20,
            );
            self.put(sec, *n, [x, y], [150, 18], *active);
        }
        sec.y += 20 * nodes.len().div_ceil(3) as i32 + 2;
    }
    /// Tool picks three to a row under one label, as v20's five start
    /// tools.
    fn tool_grid(&mut self, sec: &mut Section, nodes: &[(NodeId, bool)]) {
        self.label(sec, "Tools");
        for (i, (n, active)) in nodes.iter().enumerate() {
            let (x, y) = (
                FIELD_X - 1 + (i as i32 % 3) * TOOL_STEP,
                sec.y - 1 + (i as i32 / 3) * 23,
            );
            self.put(sec, *n, [x, y], [TOOL_STEP - 4, 20], *active);
        }
        sec.y += 23 * nodes.len().div_ceil(3) as i32;
    }
    fn vanilla_checks(&mut self, sec: &mut Section, vars: &[&str], editable: bool) {
        let nodes: Vec<_> = vars
            .iter()
            .filter_map(|v| self.variable(v))
            .map(|n| (n, editable))
            .collect();
        self.check_grid(sec, &nodes);
    }

    /// The control an Add-On setting has, not yet in a section.
    fn setting_control(
        &mut self,
        sec: &Section,
        target: Target,
        s: &MiniGameAddOnSetting,
        value: &MiniGameSettingValue,
        grid: bool,
        core: &Core,
    ) -> NodeId {
        let name = match target {
            Target::Game(i) => format!("AOS_S{i}"),
            Target::Team(t, i) => format!("AOS_T{t}_S{i}"),
        };
        let team = match target {
            Target::Game(_) => None,
            Target::Team(t, _) => Some(t),
        };
        let r = Rect::new(0, 0, PICK_W, 20);
        let n = match &s.kind {
            MiniGameSettingKind::Bool => {
                let label = if grid { title(s) } else { String::new() };
                let n = self
                    .view
                    .add(sec.node, check(Rect::new(0, 0, 150, 18), &name, &label));
                self.view
                    .set_bool(n, value == &MiniGameSettingValue::Bool(true));
                n
            }
            MiniGameSettingKind::Int { .. } | MiniGameSettingKind::Text { .. } => {
                let n = self.view.add(sec.node, edit(r, &name));
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
                let n = self.view.add(sec.node, popup(r, &name));
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
                n
            }
            MiniGameSettingKind::List { items }
            | MiniGameSettingKind::Item { items }
            | MiniGameSettingKind::PlayerType { items } => {
                let n = self.view.add(sec.node, popup(r, &name));
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
        self.rows.push((target, name));
        n
    }
    /// Add-On settings into a section: on the pages beside v20's rules,
    /// check boxes three to a row and tools together; on More Settings, a
    /// row each with its '?'.
    fn setting_rows(
        &mut self,
        sec: &mut Section,
        list: &[(usize, MiniGameAddOnSetting, Option<usize>)],
        grid: bool,
        editable: bool,
        core: &Core,
    ) {
        let mut i = 0;
        while i < list.len() {
            let (index, s, team) = &list[i];
            let target = match team {
                Some(t) => Target::Team(*t, *index),
                None => Target::Game(*index),
            };
            let active = |me: &Self, s: &MiniGameAddOnSetting| editable && !me.locked(core, &s.key);
            let value_of = |me: &Self, s: &MiniGameAddOnSetting, team: Option<usize>| {
                match team {
                    Some(t) => me.teams[t].settings.get(&s.key).cloned(),
                    None => me.values.get(&s.key).cloned(),
                }
                .unwrap_or_else(|| s.default.clone())
            };
            let run = |kind: fn(&MiniGameSettingKind) -> bool| {
                list[i..]
                    .iter()
                    .take_while(|(_, s, _)| kind(&s.kind))
                    .count()
            };
            if grid && s.kind == MiniGameSettingKind::Bool {
                let count = run(|k| *k == MiniGameSettingKind::Bool);
                let mut nodes = Vec::new();
                for (index, s, team) in &list[i..i + count] {
                    let target = match team {
                        Some(t) => Target::Team(*t, *index),
                        None => Target::Game(*index),
                    };
                    let value = value_of(self, s, *team);
                    let n = self.setting_control(sec, target, s, &value, true, core);
                    nodes.push((n, active(self, s)));
                }
                self.check_grid(sec, &nodes);
                i += count;
                continue;
            }
            let tools = run(|k| matches!(k, MiniGameSettingKind::Item { .. }));
            if grid && tools > 1 {
                let mut nodes = Vec::new();
                for (index, s, team) in &list[i..i + tools] {
                    let target = match team {
                        Some(t) => Target::Team(*t, *index),
                        None => Target::Game(*index),
                    };
                    let value = value_of(self, s, *team);
                    let n = self.setting_control(sec, target, s, &value, true, core);
                    nodes.push((n, active(self, s)));
                }
                self.tool_grid(sec, &nodes);
                i += tools;
                continue;
            }
            let value = value_of(self, s, *team);
            let n = self.setting_control(sec, target, s, &value, false, core);
            self.label(sec, &title(s));
            let (x, w, h) = match &s.kind {
                MiniGameSettingKind::Bool => (FIELD_X, 150, 18),
                MiniGameSettingKind::Int { min, max } => {
                    let digits = min.to_string().len().max(max.to_string().len()) as i32;
                    (FIELD_X, (8 * digits + 12).clamp(36, 100), 18)
                }
                MiniGameSettingKind::Text { .. } => (FIELD_X, 200, 18),
                _ => (FIELD_X - 1, PICK_W, 20),
            };
            let top = if h == 20 { sec.y - 1 } else { sec.y };
            let on = active(self, s);
            self.put(sec, n, [x, top], [w, h], on);
            if let MiniGameSettingKind::PaintColor { .. } = &s.kind
                && let MiniGameSettingValue::Int(c) = value
                && let Some(rgb) = usize::try_from(c)
                    .ok()
                    .and_then(|c| core.minigames.palette.get(c))
            {
                self.view.add(
                    sec.node,
                    swatch(Rect::new(x + w + 5, sec.y, 38, 16), palette_rgba(*rgb)),
                );
            } else {
                let words = s.unit.clone();
                self.after(sec, x + w + 6, &words);
            }
            if !grid && !s.help.is_empty() {
                self.view.add(
                    sec.node,
                    small_button(
                        Rect::new(SECTION_W - 30, sec.y - 1, 22, 20),
                        "?",
                        &format!("AOS_H{index}"),
                    ),
                );
            }
            sec.y += if h == 20 { 23 } else { 22 };
            i += 1;
        }
    }
    /// The game's own Add-On settings placed at `place`, shown.
    fn placed(
        &self,
        core: &Core,
        place: MiniGameSettingPlace,
    ) -> Vec<(usize, MiniGameAddOnSetting, Option<usize>)> {
        core.minigames
            .addon_settings
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                s.place == Some(place)
                    && !s.team
                    && !s.server
                    && s.avatar.is_none()
                    && self.shown(core, s, None)
            })
            .map(|(i, s)| (i, s.clone(), None))
            .collect()
    }

    /// Lay out the page: the v20 controls go back to the holder, the
    /// page's sections are made again and take the ones they show.
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
        let (Some(holder), Some(box_node)) = (self.view.id(HOLDER), self.view.id(BOX)) else {
            return;
        };
        for n in self.vanilla.clone() {
            self.view.reparent(n, holder, [0, 0]);
        }
        self.view.clear_children(box_node);
        self.rows.clear();
        self.name_fields.clear();
        let editable = self.editable(core) && self.request.is_none();
        let editing = self.game.is_some();
        let players = editing && self.summary(core).is_some();
        if self.page == Page::Players && !players {
            self.page = Page::Game;
        }
        for (i, (page, _)) in PAGES.iter().enumerate() {
            if let Some(n) = self.view.id(&format!("{TAB}{i}")) {
                self.view.nodes[n].ctrl.color = Some(if *page == self.page {
                    [150, 170, 255, 255]
                } else {
                    [255, 255, 255, 255]
                });
                self.view.set_active(n, *page != Page::Players || players);
            }
        }
        let mut y = 0;
        match self.page {
            Page::Game => self.game_page(core, &mut y, editable),
            Page::Teams => {
                let addons = editable && self.addons_editable(core);
                self.teams_page(core, &mut y, addons);
            }
            Page::Rules => self.rules_page(core, &mut y, editable),
            Page::Players => self.players_page(core, &mut y, editable),
            Page::More => self.more_page(core, &mut y, editable),
        }
        // At least the scroll's height, so its grey fills a short page.
        self.view.nodes[box_node].ctrl.extent[1] = y.max(317);
        self.view.measure(&core.pack);
        self.view.relayout();
        if let Some((name, cursor)) = focus
            && let Some(n) = self.view.id(&name)
            && self.view.node(n).state.active
            && self.view.is_shown(n)
        {
            self.view.focus = Some(n);
            self.view.state(n).cursor = cursor.min(self.view.edit_text(n).chars().count());
        } else if self.view.focus.is_some_and(|n| !self.view.is_shown(n)) {
            self.view.focus = None;
        }
        self.footer(core);
    }
    fn game_page(&mut self, core: &Core, y: &mut i32, editable: bool) {
        let editing = self.game.is_some();
        let mut sec = self.open(y, "Basics", None);
        self.vanilla_edit(&mut sec, "$MiniGame::Title", editable);
        // A running game keeps its colour, as v20's did.
        self.vanilla_pick(&mut sec, COLOR, "Game Color", editable && !editing);
        if let Some(n) = self.view.id(SWATCH) {
            self.put(&sec, n, [FIELD_X + PICK_W + 5, sec.y - 22], [38, 16], true);
        }
        let basics = self.placed(core, MiniGameSettingPlace::Basics);
        self.setting_rows(&mut sec, &basics, true, editable, core);
        self.vanilla_checks(&mut sec, &["$MiniGame::InviteOnly"], editable);
        self.close(sec, y);

        // Computer players sit beside the mode that decides their count,
        // on the page a new game has (the Players page waits for a game).
        let bots = self.placed(core, MiniGameSettingPlace::Bots);
        if !bots.is_empty() {
            let mut sec = self.open(y, "Bots", None);
            self.setting_rows(&mut sec, &bots, true, editable, core);
            self.close(sec, y);
        }

        let mut sec = self.open(y, "Everyone Starts With", None);
        self.vanilla_pick(&mut sec, PLAYER_TYPE, "Player Type", editable);
        let tools: Vec<_> = (0..5)
            .filter_map(|i| self.view.id(&equip(i)))
            .map(|n| (n, editable))
            .collect();
        self.tool_grid(&mut sec, &tools);
        self.vanilla_checks(
            &mut sec,
            &[
                "$MiniGame::EnableBuilding",
                "$MiniGame::EnablePainting",
                "$MiniGame::EnableWand",
            ],
            editable,
        );
        let loadout = self.placed(core, MiniGameSettingPlace::Loadout);
        self.setting_rows(&mut sec, &loadout, true, editable, core);
        if self.teams_shown(core)
            && core
                .minigames
                .addon_settings
                .iter()
                .any(|s| s.team && s.place == Some(MiniGameSettingPlace::Loadout))
        {
            self.note(
                &mut sec,
                "A team can have its own player type and tools on the Teams page.",
            );
        }
        self.close(sec, y);

        for (place, heading) in [
            (MiniGameSettingPlace::Win, "How to Win"),
            (MiniGameSettingPlace::Rounds, "Rounds"),
        ] {
            let list = self.placed(core, place);
            if list.is_empty() {
                continue;
            }
            let mut sec = self.open(y, heading, None);
            self.setting_rows(&mut sec, &list, true, editable, core);
            self.close(sec, y);
        }
    }
    fn rules_page(&mut self, core: &Core, y: &mut i32, editable: bool) {
        let sections: [(&str, &[&str], MiniGameSettingPlace, bool); 4] = [
            (
                "Scoring",
                &[
                    "$MiniGame::Points::KillPlayer",
                    "$MiniGame::Points::KillSelf",
                    "$MiniGame::Points::Die",
                    "$MiniGame::Points::BreakBrick",
                    "$MiniGame::Points::PlantBrick",
                ],
                MiniGameSettingPlace::Scoring,
                false,
            ),
            (
                "Respawning",
                &[
                    "$MiniGame::RespawnTime",
                    "$MiniGame::VehicleRespawnTime",
                    "$MiniGame::BrickRespawnTime",
                ],
                MiniGameSettingPlace::Respawn,
                false,
            ),
            (
                "Damage",
                &[
                    "$MiniGame::WeaponDamage",
                    "$MiniGame::FallingDamage",
                    "$MiniGame::SelfDamage",
                    "$MiniGame::VehicleDamage",
                    "$MiniGame::BrickDamage",
                ],
                MiniGameSettingPlace::Damage,
                true,
            ),
            (
                "Bricks",
                &[
                    "$MiniGame::UseSpawnBricks",
                    "$MiniGame::UseAllPlayersBricks",
                    "$MiniGame::PlayersUseOwnBricks",
                ],
                MiniGameSettingPlace::Bricks,
                true,
            ),
        ];
        for (heading, vars, place, checks) in sections {
            let mut sec = self.open(y, heading, None);
            let extra = self.placed(core, place);
            if checks {
                // v20's boxes and the Add-Ons' in one grid.
                let mut nodes: Vec<_> = vars
                    .iter()
                    .filter_map(|v| self.variable(v))
                    .map(|n| (n, editable))
                    .collect();
                let (bools, rest): (Vec<_>, Vec<_>) = extra
                    .into_iter()
                    .partition(|(_, s, _)| s.kind == MiniGameSettingKind::Bool);
                for (index, s, _) in &bools {
                    let value = self
                        .values
                        .get(&s.key)
                        .cloned()
                        .unwrap_or_else(|| s.default.clone());
                    let n = self.setting_control(&sec, Target::Game(*index), s, &value, true, core);
                    nodes.push((n, editable && !self.locked(core, &s.key)));
                }
                self.check_grid(&mut sec, &nodes);
                self.setting_rows(&mut sec, &rest, true, editable, core);
            } else {
                for var in vars {
                    self.vanilla_edit(&mut sec, var, editable);
                }
                self.setting_rows(&mut sec, &extra, true, editable, core);
            }
            self.close(sec, y);
        }
    }
    fn teams_page(&mut self, core: &Core, y: &mut i32, editable: bool) {
        if !self.teams_shown(core) {
            let mut sec = self.open(y, "Teams", None);
            let hint = Self::teams_hint(core);
            self.note(&mut sec, &hint);
            self.close(sec, y);
            return;
        }
        let settings = core.minigames.addon_settings.clone();
        for t in 0..self.teams.len() {
            let team = self.teams[t].clone();
            let shown_name = self
                .typed
                .get(&(Some(t), String::new()))
                .cloned()
                .unwrap_or_else(|| team.name.clone());
            let rgb = core
                .minigames
                .palette
                .get(usize::from(team.color))
                .copied()
                .unwrap_or([255; 3]);
            let mut sec = self.open(y, &shown_name, Some(rgb));
            self.label(&sec, "Name");
            let name = format!("AOS_T{t}_Name");
            let n = self
                .view
                .add(sec.node, edit(Rect::new(FIELD_X, sec.y, PICK_W, 18), &name));
            self.view.set_text(n, shown_name.clone());
            self.view.set_active(n, editable);
            self.name_fields.push((t, name));
            sec.y += 22;
            self.label(&sec, "Color");
            let n = self.view.add(
                sec.node,
                popup(
                    Rect::new(FIELD_X - 1, sec.y - 1, PICK_W, 20),
                    &format!("AOS_T{t}_Color"),
                ),
            );
            self.view.state(n).items = (0..core.minigames.palette.len().min(64))
                .map(|c| (format!("Colour {}", c + 1), c as i64))
                .collect();
            self.view.select(n, Some(i64::from(team.color)));
            self.view.set_active(n, editable);
            self.view.add(
                sec.node,
                swatch(
                    Rect::new(FIELD_X + PICK_W + 5, sec.y, 38, 16),
                    palette_rgba(rgb),
                ),
            );
            sec.y += 23;
            let mut details: Vec<_> = settings
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    s.team
                        && s.avatar.is_none()
                        && matches!(
                            s.place,
                            Some(MiniGameSettingPlace::Loadout | MiniGameSettingPlace::Teams)
                        )
                        && self.shown(core, s, Some(t))
                })
                .collect();
            let prerequisites: BTreeSet<_> = settings
                .iter()
                .filter_map(|s| s.shown_when.as_ref().map(|w| w.setting.clone()))
                .collect();
            // The loadout first: what turns a team's own on, its player
            // type and tools, then the rest in the Add-On's order.
            details.sort_by_key(|(_, s)| {
                if s.place != Some(MiniGameSettingPlace::Loadout) {
                    3
                } else if prerequisites.contains(&s.key) {
                    0
                } else if matches!(s.kind, MiniGameSettingKind::PlayerType { .. }) {
                    1
                } else {
                    2
                }
            });
            let list: Vec<_> = details
                .into_iter()
                .map(|(i, s)| (i, s.clone(), Some(t)))
                .collect();
            self.setting_rows(&mut sec, &list, true, editable, core);
            let mut buttons = Vec::new();
            let looks: Vec<String> = Self::look_categories(core)
                .into_iter()
                .filter(|category| {
                    settings.iter().any(|s| {
                        s.team
                            && s.avatar.is_some()
                            && &s.category == category
                            && self.shown(core, s, Some(t))
                    })
                })
                .collect();
            let look_names: Vec<(String, String)> = looks
                .iter()
                .map(|c| (format!("Edit {c}"), format!("AOS_T{t}_Look_{c}")))
                .collect();
            for (label, name) in &look_names {
                buttons.push((label.as_str(), name.as_str(), editable));
            }
            let remove = format!("AOS_T{t}_Remove");
            buttons.push(("Remove Team", remove.as_str(), editable));
            self.buttons(&mut sec, &buttons);
            self.close(sec, y);
        }
        let mut sec = self.open(y, "All Teams", None);
        if self.teams.is_empty() {
            self.note(&mut sec, "No teams yet.");
        }
        let list = self.placed(core, MiniGameSettingPlace::Teams);
        self.setting_rows(&mut sec, &list, true, editable, core);
        self.buttons(&mut sec, &[("Add Team", ADD_TEAM, editable)]);
        self.close(sec, y);
    }
    /// The game's players (and, for its editor, everyone else on the
    /// server) with the team each plays for: an editor moves them between
    /// teams, brings them in or removes them (Slayer's Add Member and
    /// Remove Member).
    fn players_page(&mut self, core: &Core, y: &mut i32, editable: bool) {
        let Some(game) = self.summary(core).cloned() else {
            return;
        };
        let teams: Vec<(String, i64)> = game
            .teams
            .iter()
            .map(|t| (t.name.clone(), i64::from(t.id)))
            .collect();
        let mut sec = self.open(y, "In the Game", None);
        if self.teams.iter().any(|t| t.id.is_none()) {
            self.note(
                &mut sec,
                "Update to make new teams before putting players on them.",
            );
        }
        for m in &game.members {
            self.label(&sec, &m.name);
            let pick = format!("AOS_P{}_Team", m.id.0);
            let n = self.view.add(
                sec.node,
                popup(Rect::new(FIELD_X - 1, sec.y - 1, PICK_W, 20), &pick),
            );
            let mut items = vec![("No team".to_owned(), NO_TEAM)];
            items.extend(teams.iter().cloned());
            self.view.state(n).items = items;
            self.view.select(n, Some(m.team.map_or(NO_TEAM, i64::from)));
            self.view.set_active(n, editable);
            if m.id != game.owner {
                let b = self.view.add(
                    sec.node,
                    small_button(
                        Rect::new(FIELD_X + PICK_W + 6, sec.y - 2, 76, 24),
                        "Remove",
                        &format!("AOS_P{}_Kick", m.id.0),
                    ),
                );
                self.view.set_active(b, editable);
            }
            sec.y += 25;
        }
        self.close(sec, y);
        if !editable && self.request.is_none() {
            return;
        }
        let others: Vec<_> = core
            .players
            .iter()
            .filter(|p| !game.members.iter().any(|m| m.id.0 == p.id))
            .cloned()
            .collect();
        let mut sec = self.open(y, "On the Server", None);
        if others.is_empty() {
            self.note(&mut sec, "Everyone on the server is playing.");
        }
        let invite = core
            .minigames
            .can(crate::models::minigames::Operation::Invite);
        for p in others {
            self.label(&sec, &p.name);
            let pick = format!("AOS_P{}_Team", p.id);
            let n = self.view.add(
                sec.node,
                popup(Rect::new(FIELD_X - 1, sec.y - 1, PICK_W, 20), &pick),
            );
            let mut items = vec![
                ("Not playing".to_owned(), OUT_OF_GAME),
                ("No team".to_owned(), NO_TEAM),
            ];
            items.extend(teams.iter().cloned());
            self.view.state(n).items = items;
            self.view.select(n, Some(OUT_OF_GAME));
            self.view.set_active(n, editable);
            if invite {
                let b = self.view.add(
                    sec.node,
                    small_button(
                        Rect::new(FIELD_X + PICK_W + 6, sec.y - 2, 76, 24),
                        "Invite",
                        &format!("AOS_P{}_Invite", p.id),
                    ),
                );
                self.view.set_active(b, editable);
            }
            sec.y += 25;
        }
        self.close(sec, y);
    }
    fn more_page(&mut self, core: &Core, y: &mut i32, editable: bool) {
        let settings = core.minigames.addon_settings.clone();
        self.categories.clear();
        for s in &settings {
            let relevant = !s.server
                && s.avatar.is_none()
                && if s.team {
                    self.teams_shown(core)
                        && !self.teams.is_empty()
                        && self.shown(core, s, Some(self.selected_team))
                } else {
                    self.shown(core, s, None)
                };
            let group = (s.add_on.clone(), s.category.clone(), s.team);
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
        let mut sec = self.open(y, "Every Add-On Setting", None);
        if self.categories.is_empty() {
            self.note(&mut sec, "No Add-On settings.");
        } else {
            self.label(&sec, "Category");
            let n = self.view.add(
                sec.node,
                popup(Rect::new(FIELD_X - 1, sec.y - 1, 300, 20), CATEGORY),
            );
            self.view.state(n).items = self
                .categories
                .iter()
                .enumerate()
                .map(|(i, (addon, category, team))| {
                    let name = if category.is_empty() {
                        addon.clone()
                    } else {
                        format!("{addon}: {category}")
                    };
                    let name = if *team {
                        format!("{name} (each team)")
                    } else {
                        name
                    };
                    (name, i as i64)
                })
                .collect();
            self.view.select(
                n,
                self.category
                    .as_ref()
                    .and_then(|g| self.categories.iter().position(|v| v == g))
                    .map(|i| i as i64),
            );
            sec.y += 23;
            let team = self.category.as_ref().is_some_and(|(_, _, team)| *team);
            if team {
                self.selected_team = self.selected_team.min(self.teams.len().saturating_sub(1));
                self.label(&sec, "Team");
                let n = self.view.add(
                    sec.node,
                    popup(Rect::new(FIELD_X - 1, sec.y - 1, PICK_W, 20), TEAM_PICK),
                );
                self.view.state(n).items = self
                    .teams
                    .iter()
                    .enumerate()
                    .map(|(t, team)| {
                        let name = self
                            .typed
                            .get(&(Some(t), String::new()))
                            .unwrap_or(&team.name);
                        (name.clone(), t as i64)
                    })
                    .collect();
                self.view.select(n, Some(self.selected_team as i64));
                sec.y += 23;
            }
            self.note(
                &mut sec,
                "Every setting the Add-Ons offer, including those the other pages show.",
            );
            let picked = self.category.clone();
            let list: Vec<_> = settings
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    picked.as_ref() == Some(&(s.add_on.clone(), s.category.clone(), s.team))
                        && !s.server
                        && s.avatar.is_none()
                        && self.shown(core, s, team.then_some(self.selected_team))
                })
                .map(|(i, s)| (i, s.clone(), team.then_some(self.selected_team)))
                .collect();
            self.setting_rows(&mut sec, &list, false, editable, core);
        }
        self.close(sec, y);
        if self.game.is_some() {
            let mut sec = self.open(y, "Updates", None);
            let n = self.view.add(
                sec.node,
                check(
                    Rect::new(CHECK_X, sec.y, 300, 18),
                    NOTIFY,
                    "Tell players what changed",
                ),
            );
            self.view.set_bool(n, core.prefs.bool_or(NOTIFY_PREF, true));
            self.view
                .set_active(n, editable && self.addons_editable(core));
            sec.y += 22;
            self.close(sec, y);
        }
    }
    /// Set Favs and the favourite slots, which wait while a change is sent.
    fn favorite_command(lower: &str) -> bool {
        lower == SET_FAVS || lower.starts_with("createminigamegui.clickfav(")
    }
    /// v20's footer: Update or Create while the player may, Reset and End
    /// on a running game.
    fn footer(&mut self, core: &Core) {
        let editing = self.game.is_some();
        let idle = self.request.is_none();
        let favorites: Vec<_> = self
            .view
            .walk()
            .filter(|&n| {
                self.view
                    .node(n)
                    .ctrl
                    .command
                    .as_deref()
                    .is_some_and(|c| Self::favorite_command(&c.to_ascii_lowercase()))
            })
            .collect();
        for n in favorites {
            self.view.set_active(n, idle);
        }
        if let Some(n) = self.view.id("CMG_EndBlocker") {
            self.view.set_visible(n, !editing);
        }
        let set = |view: &mut View, command: &str, on: bool| {
            if let Some(n) = view.by_command(command) {
                view.set_active(n, on);
            }
        };
        let can = |op| core.minigames.can_on(op, self.game);
        use crate::models::minigames::Operation;
        let editable = self.editable(core);
        set(
            &mut self.view,
            "CreateMiniGameGui.clickCreate();",
            idle && editable,
        );
        set(
            &mut self.view,
            "CreateMiniGameGui.clickReset();",
            idle && editing && can(Operation::Reset),
        );
        set(
            &mut self.view,
            "CreateMiniGameGui.clickEnd();",
            idle && editing && can(Operation::End),
        );
    }

    // ------------------------------------------------------- favourites

    /// Turn Set Favs on or off, with its helper text where the layout has
    /// one.
    fn set_favs(&mut self, on: bool) {
        self.setting_favs = on;
        if let Some(n) = self.view.id("CMG_FavsHelper") {
            self.view.set_visible(n, on);
        }
    }
    /// `CreateMiniGameGui::ClickFav`: with Set Favs on, keep the window's
    /// rules, Add-On settings and teams in that slot; otherwise fill the
    /// window from it.
    fn favorite(&mut self, slot: u8, core: &mut Core) {
        if self.setting_favs {
            self.set_favs(false);
            let rules = match self.read_rules() {
                Ok(rules) => rules,
                Err(e) => return core.message_ok("Set Favs", &e),
            };
            if let Err(e) = self.read_fields(core) {
                return core.message_ok("Set Favs", &e);
            }
            if self.unavailable_favorite == Some(slot) {
                return core.message_ok(
                    "Favorite has unavailable settings",
                    "The saved favorite is intact. Enable its Add-Ons before hosting and open this window again, or choose another slot to keep this setup.",
                );
            }
            let color = self.view.id(COLOR).and_then(|n| self.view.selected_text(n));
            core.settings.minigame_favorites.insert(
                slot,
                MiniGameFavorite {
                    rules,
                    color,
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
            return;
        }
        let Some(fav) = core.settings.minigame_favorites.get(&slot).cloned() else {
            return;
        };
        self.write_rules(core, &fav.rules);
        self.rules_dirty = true;
        // A running game keeps its colour.
        if self.game.is_none()
            && let Some(name) = &fav.color
            && let Some(color) = core.minigames.colors.iter().find(|c| &c.name == name)
        {
            let index = color.index;
            self.fill_colors(core, Some(index));
        }
        let fits = |key: &str, value: &MiniGameSettingValue, team: bool| {
            Self::setting(core, key).is_some_and(|s| s.team == team && !s.server && takes(s, value))
        };
        let mut unavailable = Vec::new();
        for (key, value) in &fav.settings {
            if fits(key, value, false) {
                self.values.insert(key.clone(), value.clone());
            } else {
                unavailable.push(key.clone());
            }
        }
        // A favourite kept before it held Add-On settings leaves them and
        // the teams alone.
        if !fav.settings.is_empty() || !fav.teams.is_empty() {
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
                    } else {
                        unavailable.push(format!("Team {}: {key}", i + 1));
                    }
                }
                // Keep the teams the game has, in order, under the
                // favourite's names and settings.
                teams.push(DraftTeam {
                    id: self.teams.get(i).and_then(|d| d.id),
                    name: t.name.clone(),
                    color: t.color,
                    settings,
                });
            }
            self.teams = teams;
        }
        self.unavailable_favorite = (!unavailable.is_empty()).then_some(slot);
        // The favourite's values replace what the boxes hold.
        self.typed.clear();
        self.rows.clear();
        self.name_fields.clear();
        self.build(core);
        if !unavailable.is_empty() {
            let keys = unavailable
                .iter()
                .take(12)
                .map(|key| key.replace(['<', '>'], ""))
                .collect::<Vec<_>>()
                .join("\n");
            core.message_ok(
                "Favorite has unavailable settings",
                &format!("These settings are not offered here, or their values are no longer accepted:\n\n{keys}\n\nThe saved favorite is intact. Update uses only the settings offered here. Enable its Add-Ons before hosting to get them back."),
            );
        }
    }

    // ------------------------------------------------------------ sending

    /// Create, or Update (and with `reset`, Reset >>): the v20 rules first,
    /// then the Add-On settings and teams.
    fn submit(&mut self, core: &mut Core, reset: bool) {
        let checked = self.read_rules().and_then(|rules| {
            Self::available_rules(core, &rules)?;
            Ok(rules)
        });
        let rules = match checked.and_then(|rules| self.read_fields(core).map(|()| rules)) {
            Ok(rules) => rules,
            Err(e) => {
                self.build(core);
                let title = if self.game.is_some() {
                    "Mini-Game Update Failure"
                } else {
                    "Mini-Game Creation Failure"
                };
                core.message_ok(title, &e);
                return;
            }
        };
        let Some(game) = self.game else {
            self.step = Some(Step::Create);
            self.request = core.minigame_request(
                MiniGameOperation::Create,
                UiAction::CreateMiniGame {
                    color: self.color_index(),
                    rules,
                },
            );
            if self.request.is_none() {
                self.step = None;
            }
            self.build(core);
            return;
        };
        // Add-On settings and teams the host keeps from this player stay
        // as they are.
        let (settings, teams) = if self.addons_editable(core) {
            self.changes(core)
        } else {
            (Vec::new(), None)
        };
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
        let rules_changed = self.summary(core).is_none_or(|g| g.rules != rules);
        if rules_changed {
            self.step = Some(Step::Rules(next.map(Box::new)));
            self.request = core.minigame_request(
                MiniGameOperation::Configure,
                UiAction::ConfigureMiniGame { game, rules },
            );
        } else if let Some((operation, action)) = next {
            self.step = Some(Step::Last);
            self.request = core.minigame_request(operation, action);
        } else {
            // Nothing changed: Update closes, as v20's did.
            core.pop(ScreenId::MiniGameSettings);
            return;
        }
        if self.request.is_none() {
            self.step = None;
            let status = core.minigames.status.clone();
            core.message_ok("Mini-Game Update Failure", &status);
        }
        self.build(core);
    }
    /// Send a player row's new team.
    fn move_player(&mut self, core: &mut Core, player: MiniGamePlayerId, pick: Option<i64>) {
        let Some(game) = self.game else { return };
        let team = match pick {
            None | Some(OUT_OF_GAME) => return,
            Some(NO_TEAM) => None,
            Some(t) => u32::try_from(t).ok(),
        };
        self.step = Some(Step::Players);
        self.request = core.minigame_request(
            MiniGameOperation::Configure,
            UiAction::SetMiniGameTeam {
                game,
                target: player,
                team,
            },
        );
        self.build(core);
    }
}

impl Screen for MiniGameWindow {
    fn id(&self) -> ScreenId {
        ScreenId::MiniGameSettings
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
            core.abandon(id);
        }
        self.step = None;
    }
    fn on_wake(&mut self, core: &mut Core) {
        self.take_look(core);
    }
    fn on_update(&mut self, core: &mut Core) {
        self.take_look(core);
        if self.loaded != Some(self.target(core)) && self.request.is_none() {
            self.load(core);
            return;
        }
        // The host's values changed (someone else's edit, or ours landed):
        // start again from them, unless the player is mid-edit.
        let revision = Self::revision(core);
        if self.seen != Some(revision) {
            if self.request.is_none() && !self.dirty(core) {
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
        let step = self.step.take();
        match (result, step) {
            (Ok(()), Some(Step::Create)) => {
                // The game Create made gets the Add-On settings and teams
                // (`Core::send_minigame_draft`).
                if self.dirty(core) {
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
                    core.minigame_draft_created = true;
                    core.send_minigame_draft();
                }
                core.minigames.status.clear();
                core.pop(ScreenId::MiniGameSettings);
            }
            (Ok(()), Some(Step::Rules(Some(next)))) => {
                let (operation, action) = *next;
                self.step = Some(Step::Last);
                self.request = core.minigame_request(operation, action);
                if self.request.is_none() {
                    self.step = None;
                    let status = core.minigames.status.clone();
                    core.message_ok(
                        "Mini-Game changes incomplete",
                        &format!(
                            "The rules were updated, but the rest could not be sent: {status}"
                        ),
                    );
                }
                self.build(core);
            }
            (Ok(()), Some(Step::Rules(None) | Step::Last)) => {
                // clientCmdCreateMiniGameSuccess and clickReset close the
                // editor.
                core.minigames.status.clear();
                core.pop(ScreenId::MiniGameSettings);
            }
            (Ok(()), _) => {
                core.minigames.status.clear();
                self.build(core);
            }
            (Err(e), step) => {
                core.minigames.status.clear();
                self.build(core);
                let message = match step {
                    Some(Step::Create) => (
                        "Mini-Game Creation Failure",
                        format!("Mini-Game Creation Failed.  Reason:\n\n{e}"),
                    ),
                    Some(Step::Rules(_)) => (
                        "Mini-Game Update Failure",
                        format!("The rules were not updated: {e}"),
                    ),
                    Some(Step::Last) => (
                        "Mini-Game Update Failure",
                        format!("The changes were not saved: {e}"),
                    ),
                    _ => ("Mini-Game", e.clone()),
                };
                core.message_ok(message.0, &message.1);
            }
        }
        true
    }
    fn on_key(&mut self, key: Key, _: Modifiers, core: &mut Core) -> bool {
        if key == Key::Escape {
            core.pop(ScreenId::MiniGameSettings);
            return true;
        }
        false
    }
    fn on_event(&mut self, ev: &ViewEvent, core: &mut Core) {
        if ev.kind == EventKind::Close {
            core.pop(ScreenId::MiniGameSettings);
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
        let vanilla = self.vanilla.contains(&ev.node);
        if vanilla
            && (ev.kind == EventKind::Changed
                || (ev.kind == EventKind::Click && self.view.node(ev.node).ctrl.variable.is_some()))
        {
            self.rules_dirty = true;
            if name == COLOR {
                self.tint_swatch(core);
            }
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
            if self.view.node(ev.node).ctrl.class == "GuiTextEditCtrl" {
                self.typed_dirty = true;
                let _ = self.read_fields(core);
                // A team's name heads its section.
                if name.ends_with("_Name") {
                    return;
                }
            }
            if let Some(target) = row {
                let (i, team) = match target {
                    Target::Game(i) => (i, None),
                    Target::Team(t, i) => (i, Some(t)),
                };
                let Some(s) = core.minigames.addon_settings.get(i).cloned() else {
                    return;
                };
                let picked = self.view.selected(ev.node);
                let value = match (&s.kind, picked) {
                    (MiniGameSettingKind::PaintColor { .. }, Some(c)) => {
                        Some(MiniGameSettingValue::Int(c))
                    }
                    (
                        MiniGameSettingKind::List { items }
                        | MiniGameSettingKind::Item { items }
                        | MiniGameSettingKind::PlayerType { items },
                        Some(i),
                    ) => usize::try_from(i)
                        .ok()
                        .and_then(|i| items.get(i))
                        .map(|(v, _)| v.clone()),
                    _ => None,
                };
                if let Some(value) = value {
                    let _ = self.read_fields(core);
                    self.set(team, &s.key, value);
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
            return;
        }
        if !matches!(
            ev.kind,
            EventKind::Click | EventKind::Submit | EventKind::DoubleClick
        ) {
            return;
        }
        let command = command_of(&self.view, ev.node);
        if let Some(i) = command
            .strip_prefix(TAB)
            .and_then(|i| i.parse::<usize>().ok())
            .filter(|i| *i < PAGES.len())
        {
            self.capture_fields();
            self.page = PAGES[i].0;
            if let Some(n) = self.view.id(SCROLL) {
                self.view.state(n).scroll_y = 0;
            }
            self.build(core);
            return;
        }
        let lower = command.to_ascii_lowercase();
        if self.request.is_some() && Self::favorite_command(&lower) {
            return;
        }
        match lower.as_str() {
            CANCEL => {
                core.pop(ScreenId::MiniGameSettings);
                return;
            }
            CREATE => return self.submit(core, false),
            RESET => {
                if self.game.is_some() {
                    self.submit(core, true);
                }
                return;
            }
            END => {
                if let Some(game) = self.game {
                    core.message_yes_no(
                        "End Mini-Game?",
                        "Are you sure you want to end the mini-game?",
                        Callback::MiniGame {
                            game,
                            operation: MiniGameOperation::End,
                        },
                    );
                }
                return;
            }
            SET_FAVS => {
                self.set_favs(!self.setting_favs);
                return;
            }
            "createminigamegui.clickcolorlist();" => return,
            _ => {}
        }
        if let Some(slot) = lower
            .strip_prefix("createminigamegui.clickfav(")
            .and_then(|s| s.strip_suffix(");"))
            .and_then(|s| s.parse::<u8>().ok())
            .filter(|s| *s < 10)
        {
            self.favorite(slot, core);
            return;
        }
        if self.request.is_some() {
            return;
        }
        if command == NOTIFY {
            let on = self.view.bool_value(ev.node);
            core.prefs.set(NOTIFY_PREF, if on { "1" } else { "0" });
            core.save_settings();
            return;
        }
        if command == ADD_TEAM {
            let _ = self.read_fields(core);
            self.add_team(core);
            self.build(core);
            return;
        }
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
                self.step = Some(Step::Players);
                self.request = core.minigame_request(
                    MiniGameOperation::RemoveMember,
                    UiAction::RemoveMiniGameMember {
                        target: MiniGamePlayerId(player),
                        game: Some(game),
                    },
                );
                self.build(core);
            }
        } else if let Some(player) = command
            .strip_prefix("AOS_P")
            .and_then(|r| r.strip_suffix("_Invite"))
            .and_then(|p| p.parse::<u64>().ok())
        {
            self.step = Some(Step::Players);
            self.request = core.minigame_request(
                MiniGameOperation::Invite,
                UiAction::InviteMiniGame {
                    target: MiniGamePlayerId(player),
                },
            );
            self.build(core);
        } else if let Some(t) = command
            .strip_prefix("AOS_T")
            .and_then(|r| r.strip_suffix("_Remove"))
            .and_then(|t| t.parse::<usize>().ok())
        {
            let _ = self.read_fields(core);
            self.remove_team(t);
            self.build(core);
        } else if let Some(Target::Game(i) | Target::Team(_, i)) = row
            && let Some(s) = core.minigames.addon_settings.get(i).cloned()
            && s.kind == MiniGameSettingKind::Bool
        {
            let on = self.view.bool_value(ev.node);
            let _ = self.read_fields(core);
            let team = match row {
                Some(Target::Team(t, _)) => Some(t),
                _ => None,
            };
            self.set(team, &s.key, MiniGameSettingValue::Bool(on));
            self.build(core);
        }
    }
}
