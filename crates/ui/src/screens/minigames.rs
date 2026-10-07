//! Stock v20 mini-game list, rule editor and invitation dialog.
//! Views use the original pack layouts; controls are typed and host-gated.
use super::*;
use crate::{
    api::*,
    geom::Rect,
    ui::Callback,
    view::{EventKind, Value},
};

#[derive(Clone, Copy)]
enum Kind {
    List,
    Rules,
    Invite,
}
const ADDONS_BUTTON: &str = "NativeMiniGameAddOns";
const TEAMS_BUTTON: &str = "NativeMiniGameTeams";
/// Slayer's Default column header (`JMG_Slayer_Default`), shown while a
/// listed game is the server's default one.
const DEFAULT_HEADER: &str = "JMG_DefaultHeader";
/// `JMG_List.columns` with the Default column (Slayer's).
const DEFAULT_COLUMNS: &str = "0 102 165 490 450";

pub struct MiniGameScreen {
    id: ScreenId,
    kind: Kind,
    view: View,
    selected_game: Option<MiniGameId>,
    game_ids: Vec<MiniGameId>,
    draft: MiniGameRules,
    types: Vec<MiniGameChoice>,
    items: Vec<MiniGameChoice>,
    resources: std::collections::BTreeMap<NodeId, Vec<Option<String>>>,
    loaded_revision: Option<(bool, u64)>,
    request: Option<RequestId>,
    list_columns: Option<String>,
    rules_dirty: bool,
    loaded_game: Option<MiniGameId>,
}
impl MiniGameScreen {
    pub fn list(core: &Core) -> Self {
        Self::new(core, Kind::List)
    }
    pub fn settings(core: &Core) -> Self {
        Self::new(core, Kind::Rules)
    }
    pub fn invitation(core: &Core) -> Self {
        Self::new(core, Kind::Invite)
    }
    fn new(core: &Core, kind: Kind) -> Self {
        let (id, layout) = match kind {
            Kind::List => (ScreenId::MiniGames, "joinMiniGameGui"),
            Kind::Rules => (ScreenId::MiniGameSettings, "CreateMiniGameGui"),
            Kind::Invite => (ScreenId::MiniGameInvitation, "MiniGameInviteGui"),
        };
        let mut s = Self {
            id,
            kind,
            view: layout_view(core, layout),
            selected_game: None,
            game_ids: vec![],
            draft: core.minigames.rules_draft(),
            types: vec![],
            items: vec![],
            resources: Default::default(),
            loaded_revision: None,
            request: None,
            list_columns: None,
            rules_dirty: false,
            loaded_game: None,
        };
        // The End blocker greys End out, so it must draw over the button.
        if let Some(n) = s.view.id("CMG_EndBlocker") {
            s.view.push_to_back(n);
        }
        if let Some(n) = s.view.id("JMG_List") {
            s.list_columns = s.view.node(n).ctrl.field("columns").map(str::to_owned);
        }
        if matches!(kind, Kind::List) {
            let parent = window(&s.view).unwrap_or(s.view.root);
            let mut b = button(
                "BlockButtonProfile",
                Rect::new(445, 33, 57, 19),
                "base/client/ui/button1",
                "Default",
                "JoinMiniGameGui.sortList(3);",
            );
            b.name = Some(DEFAULT_HEADER.into());
            b.visible = false;
            s.view.add(parent, b);
        }
        s.refresh(core);
        s
    }
    fn selected_game(&self) -> Option<MiniGameId> {
        self.view
            .id("JMG_List")
            .and_then(|n| self.view.selected(n))
            .and_then(|i| usize::try_from(i).ok())
            .and_then(|i| self.game_ids.get(i))
            .copied()
    }
    fn set_active(&mut self, command: &str, active: bool) {
        if let Some(n) = self.view.by_command(command) {
            self.view.set_active(n, active);
        }
    }
    fn set_var(&mut self, key: &str, value: &str) {
        let node = {
            self.view.walk().find(|&n| {
                self.view
                    .node(n)
                    .ctrl
                    .variable
                    .as_deref()
                    .is_some_and(|v| v.eq_ignore_ascii_case(key))
            })
        };
        if let Some(n) = node {
            if matches!(self.view.node(n).state.value, Value::Bool(_)) {
                self.view
                    .set_bool(n, value == "1" || value.eq_ignore_ascii_case("true"));
            } else {
                self.view.set_text(n, value);
            }
        }
    }
    fn variable(&self, key: &str) -> Option<usize> {
        self.view.walk().find(|&n| {
            self.view
                .node(n)
                .ctrl
                .variable
                .as_deref()
                .is_some_and(|v| v.eq_ignore_ascii_case(key))
        })
    }
    fn write_rules(&mut self, rules: &MiniGameRules) {
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
    }
    /// Keep saved IDs outside popup indices, including unavailable choices.
    fn fill_resource(
        &mut self,
        node: &str,
        choices: &[MiniGameChoice],
        selected: Option<&str>,
        none: bool,
    ) {
        if let Some(n) = self.view.id(node) {
            let ids = resource_choices(
                &mut self.view,
                n,
                choices
                    .iter()
                    .map(|choice| (choice.id.as_str(), choice.name.as_str())),
                selected,
                none,
            );
            self.resources.insert(n, ids);
        }
    }
    fn fill_choice(&mut self, node: &str, choices: &[MiniGameChoice], selected: Option<&str>) {
        self.fill_resource(node, choices, selected, true);
    }
    fn selected_resource(&self, name: &str) -> Option<String> {
        let node = self.view.id(name)?;
        let index = usize::try_from(self.view.selected(node)?).ok()?;
        self.resources.get(&node)?.get(index)?.clone()
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
                    "Equipment slot {}: {id} is unavailable. Enable its Add-On before hosting, or choose another item or NONE.",
                    slot + 1
                ));
            }
        }
        Ok(())
    }
    fn apply_rules_state(&mut self, core: &Core) {
        self.draft = core.minigames.rules_draft();
        self.write_rules(&self.draft.clone());
        let player_type = self.draft.player_type.clone();
        self.fill_resource(
            "CMG_PlayerDataBlock",
            &core.minigames.player_types,
            Some(&player_type),
            false,
        );
        for i in 0..5 {
            let selected = self.draft.loadout[i].clone();
            self.fill_choice(
                &format!("CMG_StartEquip{i}"),
                &core.minigames.items,
                selected.as_deref(),
            );
        }
        if let Some(n) = self.view.id("CMG_ColorList") {
            self.view.nodes[n].state.items = core
                .minigames
                .colors
                .iter()
                .map(|c| (c.name.clone(), i64::from(c.index)))
                .collect();
            let selected = core
                .minigames
                .games
                .iter()
                .find(|g| Some(g.id) == core.minigames.active_game)
                .map(|g| g.color);
            self.view.select(
                n,
                selected
                    .map(i64::from)
                    .or_else(|| core.minigames.colors.first().map(|c| i64::from(c.index))),
            );
        }
        if let (Some(n), Some(color)) = (
            self.view.id("CMG_Swatch"),
            core.minigames.colors.iter().find(|c| {
                Some(i64::from(c.index))
                    == self
                        .view
                        .id("CMG_ColorList")
                        .and_then(|v| self.view.selected(v))
            }),
        ) {
            self.view.nodes[n].state.tint = Some([color.rgb[0], color.rgb[1], color.rgb[2], 255]);
        }
    }
    fn rules_availability(&mut self, core: &Core) {
        // Edit the running game when the player may manage it: its owner,
        // or an editor the host names (an admin).
        let mode_edit = core
            .minigames
            .active_game
            .is_some_and(|g| core.minigames.can_manage(g));
        if let Some(n) = self.view.id("CMG_Window") {
            self.view.set_text(
                n,
                if mode_edit {
                    "Edit Mini-Game"
                } else {
                    "Create Mini-Game"
                },
            );
        }
        if let Some(n) = self.view.id("CMG_CreateButton") {
            self.view
                .set_text(n, if mode_edit { "Update >>" } else { "Create >>" });
        }
        if let Some(n) = self.view.id("CMG_ColorBlocker") {
            self.view.set_visible(n, mode_edit);
        }
        if let Some(n) = self.view.id("CMG_EndBlocker") {
            self.view.set_visible(n, !mode_edit);
        }
        let can_save = if mode_edit {
            core.minigames
                .can(crate::models::minigames::Operation::Configure)
        } else {
            core.minigames
                .can(crate::models::minigames::Operation::Create)
        };
        self.set_active(
            "CreateMiniGameGui.clickCreate();",
            can_save && self.request.is_none(),
        );
        // Reset and End only act on a running mini-game you own ($RunningMiniGame).
        self.set_active(
            "CreateMiniGameGui.clickReset();",
            mode_edit
                && core
                    .minigames
                    .can(crate::models::minigames::Operation::Reset)
                && self.request.is_none(),
        );
        self.set_active(
            "CreateMiniGameGui.clickEnd();",
            mode_edit
                && core.minigames.can(crate::models::minigames::Operation::End)
                && self.request.is_none(),
        );
    }
    fn read_rules(&self) -> Result<MiniGameRules, String> {
        let val = |key: &str| {
            self.variable(key)
                .map(|n| self.view.edit_text(n))
                .unwrap_or_default()
        };
        let boolean = |key: &str| self.variable(key).is_some_and(|n| self.view.bool_value(n));
        let number = |key: &str, min: u32, max: u32| -> Result<u32, String> {
            let value = val(key)
                .trim()
                .parse::<u32>()
                .map_err(|_| format!("Enter a number for {key}."))?;
            if !(min..=max).contains(&value) {
                return Err(format!("{key} must be {min}–{max}."));
            }
            Ok(value)
        };
        let mut rules = self.draft.clone();
        rules.title = val("$MiniGame::Title");
        if rules.title.trim().is_empty()
            || rules.title.chars().count() > 35
            || rules.title.chars().any(char::is_control)
        {
            return Err("Title must contain 1–35 visible characters.".into());
        }
        rules.points_break_brick = val("$MiniGame::Points::BreakBrick")
            .parse()
            .map_err(|_| "Break-brick points must be an integer.")?;
        rules.points_plant_brick = val("$MiniGame::Points::PlantBrick")
            .parse()
            .map_err(|_| "Plant-brick points must be an integer.")?;
        rules.points_kill_player = val("$MiniGame::Points::KillPlayer")
            .parse()
            .map_err(|_| "Kill-player points must be an integer.")?;
        rules.points_kill_self = val("$MiniGame::Points::KillSelf")
            .parse()
            .map_err(|_| "Kill-self points must be an integer.")?;
        rules.points_die = val("$MiniGame::Points::Die")
            .parse()
            .map_err(|_| "Death points must be an integer.")?;
        rules.respawn_seconds = number("$MiniGame::RespawnTime", 1, 30)?;
        rules.vehicle_respawn_seconds = number("$MiniGame::VehicleRespawnTime", 0, 300)?;
        rules.brick_respawn_seconds = number("$MiniGame::BrickRespawnTime", 2, 300)?;
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
        if let Some(id) = self.selected_resource("CMG_PlayerDataBlock") {
            rules.player_type = id;
        }
        for i in 0..5 {
            rules.loadout[i] = self.selected_resource(&format!("CMG_StartEquip{i}"));
        }
        Ok(rules)
    }
    /// `CreateMiniGameGui::ClickFav`: with Set Favs showing, save the form
    /// in that slot; otherwise fill the form from it.
    fn favorite(&mut self, slot: u8, core: &mut Core) {
        let helper = self.view.id("CMG_FavsHelper");
        if helper.is_some_and(|n| self.view.node(n).state.visible) {
            match self.read_rules() {
                Ok(rules) => {
                    let color = self
                        .view
                        .id("CMG_ColorList")
                        .and_then(|n| self.view.selected_text(n));
                    core.settings
                        .minigame_favorites
                        .insert(slot, MiniGameFavorite { rules, color });
                    core.save_settings();
                    if let Some(n) = helper {
                        self.view.set_visible(n, false);
                    }
                }
                Err(e) => core.minigames.status = e,
            }
            return;
        }
        let Some(fav) = core.settings.minigame_favorites.get(&slot).cloned() else {
            return;
        };
        self.rules_dirty = true;
        self.draft = fav.rules.clone();
        self.write_rules(&fav.rules);
        let types = self.types.clone();
        self.fill_resource(
            "CMG_PlayerDataBlock",
            &types,
            Some(&fav.rules.player_type),
            false,
        );
        let items = self.items.clone();
        for i in 0..5 {
            self.fill_choice(
                &format!("CMG_StartEquip{i}"),
                &items,
                fav.rules.loadout[i].as_deref(),
            );
        }
        // The colour list is locked while editing a running game.
        let editing = core
            .minigames
            .active_game
            .is_some_and(|g| core.minigames.can_manage(g));
        if let (Some(n), Some(name), false) = (self.view.id("CMG_ColorList"), fav.color, editing)
            && let Some(color) = core.minigames.colors.iter().find(|c| c.name == name)
        {
            self.view.select(n, Some(i64::from(color.index)));
            if let Some(s) = self.view.id("CMG_Swatch") {
                self.view.nodes[s].state.tint =
                    Some([color.rgb[0], color.rgb[1], color.rgb[2], 255]);
            }
        }
    }
    fn refresh(&mut self, core: &Core) {
        match self.kind {
            Kind::List => {
                let old = self.selected_game;
                let games = &core.minigames.games;
                self.selected_game = core
                    .minigames
                    .retain_game_target(old)
                    .or_else(|| games.first().map(|g| g.id));
                // Slayer's Default column, while a listed game is the default.
                let defaults = games.iter().any(|g| g.default);
                if let Some(n) = self.view.id(DEFAULT_HEADER) {
                    self.view.set_visible(n, defaults);
                }
                if let Some(n) = self.view.id("JMG_List") {
                    self.game_ids = games.iter().map(|g| g.id).collect();
                    let columns = if defaults {
                        Some(DEFAULT_COLUMNS.to_owned())
                    } else {
                        self.list_columns.clone()
                    };
                    match columns {
                        Some(c) => {
                            self.view.nodes[n].ctrl.fields.insert("columns".into(), c);
                        }
                        None => {
                            self.view.nodes[n].ctrl.fields.remove("columns");
                        }
                    }
                    // `MiniGameSO::getLine` in the game's colour: creator, BL_ID, title, invite-only.
                    self.view.nodes[n].state.items = games
                        .iter()
                        .enumerate()
                        .map(|(i, g)| {
                            let color = char::from_u32(
                                crate::text::COLOR_CODE_BASE + u32::from(g.color.min(9)),
                            )
                            .unwrap_or(' ');
                            let bl_id = core
                                .players
                                .iter()
                                .find(|p| p.id == g.owner.0)
                                .and_then(|p| p.bl_id)
                                .map(|id| id.to_string())
                                .unwrap_or_default();
                            let mut line = format!(
                                "{color}{}\t{bl_id}\t{}\t{}",
                                g.owner_name,
                                g.title,
                                u8::from(g.invite_only)
                            );
                            if defaults {
                                line.push('\t');
                                if g.default {
                                    line.push_str("Yes");
                                }
                            }
                            (line, i as i64)
                        })
                        .collect();
                    self.view.select(
                        n,
                        self.selected_game
                            .and_then(|id| self.game_ids.iter().position(|g| *g == id))
                            .map(|i| i as i64),
                    );
                }
                let selected = games.iter().find(|g| Some(g.id) == self.selected_game);
                let join = selected.is_some_and(|g| !g.invite_only)
                    && core
                        .minigames
                        .can(crate::models::minigames::Operation::Join)
                    && core.minigames.active_game.is_none()
                    && self.request.is_none();
                self.set_active("JoinMiniGameGui.clickJoin();", join);
                self.set_active(
                    "JoinMiniGameGui.clickLeave();",
                    core.minigames.active_game.is_some() && self.request.is_none(),
                );
                self.set_active(
                    "JoinMiniGameGui.clickCreate();",
                    (core
                        .minigames
                        .can(crate::models::minigames::Operation::Create)
                        || core
                            .minigames
                            .can(crate::models::minigames::Operation::Configure))
                        && self.request.is_none(),
                );
                for (name, shown) in [
                    ("JMG_JoinBlocker", !join),
                    ("JMG_LeaveBlocker", core.minigames.active_game.is_none()),
                    ("JMG_CreateBlocker", false),
                ] {
                    if let Some(n) = self.view.id(name) {
                        self.view.set_visible(n, shown);
                    }
                }
                self.status(core);
            }
            Kind::Rules => {
                if self.loaded_game != core.minigames.active_game {
                    self.rules_dirty = false;
                    self.loaded_game = core.minigames.active_game;
                    self.loaded_revision = None;
                }
                if self.loaded_revision != Some((core.minigames.ready, core.minigames.revision)) {
                    if !self.rules_dirty {
                        self.types = core.minigames.player_types.clone();
                        self.items = core.minigames.items.clone();
                        self.apply_rules_state(core);
                    }
                    self.loaded_revision = Some((core.minigames.ready, core.minigames.revision));
                }
                self.rules_availability(core);
                self.status(core);
            }
            Kind::Invite => {
                if core.minigames.invitations.is_empty() {
                    return;
                }
                if let Some(i) = core.minigames.invitations.last() {
                    for (name, value) in [
                        ("MGI_Title", i.title.as_str()),
                        ("MGI_Name", i.owner_name.as_str()),
                        ("MGI_BL_ID", i.owner_display_id.as_str()),
                    ] {
                        if let Some(n) = self.view.id(name) {
                            self.view.set_text(n, value);
                        }
                    }
                }
                for cmd in [
                    "MiniGameInviteGui.clickAccept();",
                    "MiniGameInviteGui.clickReject();",
                    "MiniGameInviteGui.clickIgnore();",
                ] {
                    self.set_active(
                        cmd,
                        core.minigames
                            .can(crate::models::minigames::Operation::AcceptInvite)
                            && self.request.is_none(),
                    );
                }
            }
        }
    }
    fn status(&mut self, core: &Core) {
        let key = match self.kind {
            Kind::List => "NativeMiniGameListStatus",
            Kind::Rules => "NativeMiniGameRulesStatus",
            Kind::Invite => "NativeMiniGameInviteStatus",
        };
        let status = if !core.minigames.ready {
            "Waiting for the server's mini-games.".to_string()
        } else {
            core.minigames.status.clone()
        };
        if let Some(n) = self.view.id(key) {
            self.view.set_text(n, &status);
        } else {
            // Below the authored window content, inside the window: the
            // window grows by the row, never past v20's 480-high canvas.
            let parent = window(&self.view).unwrap_or(self.view.root);
            let [w, h] = self.view.nodes[parent].ctrl.extent;
            let grow = 26.min((480 - h).max(0));
            self.view.nodes[parent].ctrl.extent[1] = h + grow;
            let mut c = text(
                "GuiTextProfile",
                Rect::new(12, h + grow - 28, (w - 24 - 160).max(0), 24),
                &status,
            );
            c.name = Some(key.into());
            c.class = "GuiMLTextCtrl".into();
            self.view.add(parent, c);
            // Beside it: the running Add-Ons' settings for the game (Slayer's).
            if !matches!(self.kind, Kind::Invite) {
                let mut b = button(
                    "BlockButtonProfile",
                    Rect::new(w - 12 - 154, h + grow - 30, 74, 26),
                    "base/client/ui/button1",
                    "Setup",
                    ADDONS_BUTTON,
                );
                b.name = Some(ADDONS_BUTTON.into());
                self.view.add(parent, b);
                let mut teams = button(
                    "BlockButtonProfile",
                    Rect::new(w - 12 - 76, h + grow - 30, 76, 26),
                    "base/client/ui/button1",
                    "Teams",
                    TEAMS_BUTTON,
                );
                teams.name = Some(TEAMS_BUTTON.into());
                self.view.add(parent, teams);
            }
        }
        // Which game the button opens: the picked one in the list, else the
        // player's own; before Create, the new game's draft.
        let target = self.addons_target(core);
        let draft = self.drafts_addons(core);
        let teams_draft = draft
            && (core.minigames.teams_shown_when.is_some()
                || core.minigames.addon_settings.iter().any(|s| s.team));
        if let Some(n) = self.view.id(ADDONS_BUTTON) {
            self.view.set_visible(
                n,
                target.is_some() || !core.minigames.addon_settings.is_empty(),
            );
            self.view.set_active(n, target.is_some() || draft);
        }
        if let Some(n) = self.view.id(TEAMS_BUTTON) {
            self.view.set_visible(n, target.is_some() || teams_draft);
            self.view.set_active(n, target.is_some() || teams_draft);
        }
    }
    /// The Create Mini-Game window before its game exists, with Add-On
    /// settings to set up: Setup and Teams edit a draft that Create sends.
    fn drafts_addons(&self, core: &Core) -> bool {
        matches!(self.kind, Kind::Rules)
            && core.minigames.active_game.is_none()
            && !core.minigames.addon_settings.is_empty()
            && core
                .minigames
                .can(crate::models::minigames::Operation::Create)
            && self.request.is_none()
    }
    fn addons_target(&self, core: &Core) -> Option<MiniGameId> {
        match self.kind {
            Kind::List => self.selected_game(),
            Kind::Rules => core.minigames.active_game,
            Kind::Invite => None,
        }
    }
}

impl Screen for MiniGameScreen {
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
        if matches!(self.kind, Kind::List)
            && core
                .minigames
                .can(crate::models::minigames::Operation::List)
        {
            self.request =
                core.minigame_request(MiniGameOperation::List, UiAction::RequestMiniGameList);
        }
        self.refresh(core);
    }
    fn on_sleep(&mut self, core: &mut Core) {
        if let Some(id) = self.request.take() {
            core.pending.remove(&id);
        }
        // Closed without creating: the Setup draft goes with the window.
        if matches!(self.kind, Kind::Rules) && !core.minigame_draft_created {
            core.minigame_addon_draft = None;
        }
    }
    fn on_update(&mut self, core: &mut Core) {
        if matches!(self.kind, Kind::Invite) && core.minigames.invitations.is_empty() {
            core.pop(self.id);
            return;
        }
        self.refresh(core);
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
        core.minigames.status = result
            .as_ref()
            .map_or_else(|e| e.clone(), |_| "Mini-game request completed.".into());
        // The game Create made gets the Setup draft (`Core::send_minigame_draft`).
        if matches!(self.kind, Kind::Rules)
            && result.is_ok()
            && matches!(kind, Some(Pending::MiniGame(MiniGameOperation::Create)))
            && core.minigame_addon_draft.is_some()
        {
            core.minigame_draft_created = true;
            core.send_minigame_draft();
        }
        // clientCmdCreateMiniGameSuccess and clickReset close the editor.
        if matches!(self.kind, Kind::Rules) && result.is_ok() {
            core.pop(self.id);
            return true;
        }
        if let (Kind::Rules, Err(e)) = (self.kind, result) {
            core.message_ok(
                "Mini-Game Creation Failure",
                &format!(
                    "Mini-Game Creation Failed.  Reason:

{e}"
                ),
            );
        }
        self.refresh(core);
        true
    }
    fn on_key(&mut self, key: Key, _: Modifiers, core: &mut Core) -> bool {
        if key == Key::Escape {
            core.pop(self.id);
            true
        } else {
            false
        }
    }
    fn on_event(&mut self, ev: &ViewEvent, core: &mut Core) {
        if !self.view.node(ev.node).state.active {
            return;
        }
        if matches!(self.kind, Kind::Rules)
            && (ev.kind == EventKind::Changed
                || (ev.kind == EventKind::Click && self.view.node(ev.node).ctrl.variable.is_some()))
        {
            self.rules_dirty = true;
        }
        if ev.kind == EventKind::Close {
            core.pop(self.id);
            return;
        }
        if ev.kind == EventKind::Changed {
            if self.view.node(ev.node).ctrl.name.as_deref() == Some("JMG_List") {
                self.selected_game = self.selected_game();
                self.refresh(core);
            } else if self.view.node(ev.node).ctrl.name.as_deref() == Some("CMG_ColorList") {
                let selected = self
                    .view
                    .id("CMG_ColorList")
                    .and_then(|n| self.view.selected(n));
                if let (Some(n), Some(color)) = (
                    self.view.id("CMG_Swatch"),
                    core.minigames
                        .colors
                        .iter()
                        .find(|c| Some(i64::from(c.index)) == selected),
                ) {
                    self.view.nodes[n].state.tint =
                        Some([color.rgb[0], color.rgb[1], color.rgb[2], 255]);
                }
            }
            return;
        }
        if !matches!(
            ev.kind,
            EventKind::Click | EventKind::Submit | EventKind::DoubleClick
        ) {
            return;
        }
        let destination = command_of(&self.view, ev.node);
        if destination == ADDONS_BUTTON || destination == TEAMS_BUTTON {
            let target = self.addons_target(core);
            if target.is_some() || self.drafts_addons(core) {
                // No game: the window edits the draft Create sends.
                core.minigame_addons = target;
                core.minigame_addons_teams = destination == TEAMS_BUTTON;
                core.push(ScreenId::MiniGameAddOns);
            }
            return;
        }
        let cmd = command_of(&self.view, ev.node).to_ascii_lowercase();
        match self.kind {
            Kind::List => match cmd.as_str() {
                "canvas.popdialog(joinminigamegui);" => core.pop(self.id),
                "joinminigamegui.clicklist();" => {
                    self.selected_game = self.selected_game();
                    self.refresh(core);
                }
                "joinminigamegui.clickjoin();" => {
                    if let Some(game) = self.selected_game() {
                        self.request = core.minigame_request(
                            MiniGameOperation::Join,
                            UiAction::JoinMiniGame { game },
                        );
                        self.refresh(core);
                    }
                }
                "joinminigamegui.clickleave();" => {
                    if let Some(game) = core.minigames.active_game {
                        if core.minigames.owns_active_game {
                            core.message_yes_no(
                                "End Mini-Game?",
                                "Are you sure you want to end the mini-game?",
                                Callback::MiniGame {
                                    game,
                                    operation: MiniGameOperation::End,
                                },
                            );
                        } else if let Some(id) = core.minigame_request(
                            MiniGameOperation::Leave,
                            UiAction::LeaveMiniGame { game },
                        ) {
                            self.request = Some(id);
                            self.refresh(core);
                        }
                    }
                }
                "joinminigamegui.clickcreate();" => {
                    core.pop(self.id);
                    core.push(ScreenId::MiniGameSettings);
                }
                _ => {
                    let col = cmd
                        .strip_prefix("joinminigamegui.sortlist(")
                        .or_else(|| cmd.strip_prefix("joinminigamegui.sortnumlist("))
                        .and_then(|s| s.strip_suffix(");"))
                        .and_then(|s| s.parse::<usize>().ok());
                    if col.is_some() {
                        self.refresh(core);
                    }
                }
            },
            Kind::Rules => match cmd.as_str() {
                "canvas.popdialog(createminigamegui);" => core.pop(self.id),
                "createminigamegui.clickcreate();" => match self.read_rules().and_then(|rules| {
                    Self::available_rules(core, &rules)?;
                    Ok(rules)
                }) {
                    Err(e) => core.minigames.status = e,
                    Ok(rules) => {
                        let editing = core
                            .minigames
                            .active_game
                            .is_some_and(|g| core.minigames.can_manage(g));
                        let color = self
                            .view
                            .id("CMG_ColorList")
                            .and_then(|n| self.view.selected(n))
                            .and_then(|v| u8::try_from(v).ok())
                            .unwrap_or(0);
                        let request = if editing {
                            core.minigames.active_game.and_then(|game| {
                                core.minigame_request(
                                    MiniGameOperation::Configure,
                                    UiAction::ConfigureMiniGame { game, rules },
                                )
                            })
                        } else {
                            core.minigame_request(
                                MiniGameOperation::Create,
                                UiAction::CreateMiniGame { color, rules },
                            )
                        };
                        self.request = request;
                        self.refresh(core);
                    }
                },
                "createminigamegui.clickreset();" => {
                    if let Some(game) = core
                        .minigames
                        .active_game
                        .filter(|g| core.minigames.can_manage(*g))
                    {
                        self.request = core.minigame_request(
                            MiniGameOperation::Reset,
                            UiAction::ResetMiniGame { game },
                        );
                    }
                }
                "createminigamegui.clickend();" => {
                    if let Some(game) = core
                        .minigames
                        .active_game
                        .filter(|g| core.minigames.can_manage(*g))
                    {
                        core.message_yes_no(
                            "End Mini-Game?",
                            "Are you sure you want to end the mini-game?",
                            Callback::MiniGame {
                                game,
                                operation: MiniGameOperation::End,
                            },
                        );
                    }
                }
                "createminigamegui.clickcolorlist();" => self.refresh(core),
                "createminigamegui.clicksetfavs();" => {
                    if let Some(n) = self.view.id("CMG_FavsHelper") {
                        let shown = self.view.node(n).state.visible;
                        self.view.set_visible(n, !shown);
                    }
                }
                _ => {
                    if let Some(slot) = cmd
                        .strip_prefix("createminigamegui.clickfav(")
                        .and_then(|s| s.strip_suffix(");"))
                        .and_then(|s| s.parse::<u8>().ok())
                        .filter(|s| *s < 10)
                    {
                        self.favorite(slot, core);
                    }
                }
            },
            Kind::Invite => {
                let Some(invite) = core.minigames.invitations.last().cloned() else {
                    return;
                };
                match cmd.as_str() {
                    "minigameinvitegui.clickaccept();" => {
                        self.request = core.minigame_request(
                            MiniGameOperation::AcceptInvite,
                            UiAction::AcceptMiniGameInvite { game: invite.game },
                        );
                    }
                    "minigameinvitegui.clickreject();" => {
                        self.request = core.minigame_request(
                            MiniGameOperation::RejectInvite,
                            UiAction::RejectMiniGameInvite {
                                game: invite.game,
                                ignore_owner: false,
                            },
                        );
                    }
                    "minigameinvitegui.clickignore();" => core.message_yes_no(
                        "Ignore User?",
                        "Are you sure you want to ignore mini-game invites from this user?",
                        Callback::MiniGame {
                            game: invite.game,
                            operation: MiniGameOperation::IgnoreInvite,
                        },
                    ),
                    _ => {}
                }
            }
        }
        self.refresh(core);
    }
}
