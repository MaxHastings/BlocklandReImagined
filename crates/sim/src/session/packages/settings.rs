//! Add-On settings (`behaviour.json` `settings`): typed values a mini-game's
//! owner or an admin edits in the Mini-Game window's Add-On Settings, and
//! the Add-On's rules read with `setting(game, key)`. Slayer's preferences
//! and team preferences are these. Server-wide ones (RTB's
//! `$Pref::Server::*` preferences) are kept with the host's Server
//! Settings, which only the host changes, and read with
//! `server_setting(key)`.
//!
//! The engine keeps the values (on the mini-game and its teams), checks
//! every change against its definition and who made it, sends the
//! definitions and values to players for the menu, and tells the rules
//! with `on_minigame`'s `settings` event. The Add-On decides what each
//! setting means.
use super::*;
use bri_minigames as mg;
use bri_package::setting::{
    SettingDef, SettingEditor, SettingItem, SettingScope, SettingType, SettingValue, ShownWhen,
};
use serde::{Deserialize, Serialize};

/// Most settings all running Add-Ons declare together.
pub const MAX_ADDON_SETTINGS: usize = 512;

/// One Add-On setting as players' menus show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddOnSetting {
    /// The Add-On declaring it.
    pub package: String,
    /// That Add-On's name, which heads its settings in the menu.
    pub package_name: String,
    pub def: SettingDef,
    /// A list's choices: its own, then other Add-Ons' (`setting_items`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<SettingItem>,
}
impl AddOnSetting {
    /// `namespace:key`, how values are stored and rules name it from
    /// another Add-On.
    pub fn key(&self) -> String {
        format!("{}:{}", self.package, self.def.key)
    }
    /// Within what a host may send.
    pub fn validate(&self) -> Result<(), String> {
        self.def.validate()?;
        if self.items.len() > bri_package::setting::MAX_ITEMS
            || bri_package::id::namespace_problem(&self.package).is_some()
            || self.package_name.len() > 256
            || self.package_name.chars().any(char::is_control)
        {
            return Err(format!("invalid Add-On setting `{}`", self.key()));
        }
        Ok(())
    }
    /// Whether `value` is one it may hold.
    pub fn check(&self, value: &SettingValue) -> Result<(), String> {
        self.def.check(value, &self.items)
    }
}

/// One change a player asks for in the menu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingEdit {
    /// `namespace:key`.
    pub key: String,
    /// `None` puts it back to its default.
    pub value: Option<SettingValue>,
}
/// A team as the menu leaves it: an existing `id` keeps that team and its
/// players, none makes a new one; teams left out are removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamEdit {
    pub id: Option<u32>,
    pub name: String,
    pub color: u8,
    /// Its team settings to change.
    #[serde(default)]
    pub settings: Vec<SettingEdit>,
}

/// Every running Add-On's settings, checked together.
#[derive(Debug, Default)]
pub(in crate::session) struct Registry {
    list: Vec<AddOnSetting>,
    by_key: BTreeMap<String, usize>,
    /// When the team list shows (`namespace:key` and its values).
    teams_shown_when: Option<ShownWhen>,
    /// Server settings by the v20 global they stand for, lower-case: the
    /// first Add-On declaring one has it, as RTB kept a preference's first
    /// registration.
    by_global: BTreeMap<String, usize>,
}
impl Registry {
    pub(in crate::session) fn build(catalog: &bri_package_runtime::package::Catalog) -> Result<Self> {
        let mut out = Self::default();
        for (id, behaviour) in catalog.behaviours() {
            // A companion (an import's host rules) goes by the Add-On that
            // names it, as players know it.
            let owner = catalog
                .packages
                .values()
                .find(|p| p.manifest.companions.iter().any(|c| c == id))
                .or_else(|| catalog.packages.get(id));
            let name = owner.map_or_else(|| id.clone(), |p| p.manifest.name.clone());
            for def in &behaviour.settings {
                if let Some(global) = &def.global {
                    out.by_global
                        .entry(global.to_ascii_lowercase())
                        .or_insert(out.list.len());
                }
                out.by_key
                    .insert(format!("{id}:{}", def.key), out.list.len());
                out.list.push(AddOnSetting {
                    package: id.clone(),
                    package_name: name.clone(),
                    def: def.clone(),
                    items: def.items.clone(),
                });
            }
        }
        ensure!(
            out.list.len() <= MAX_ADDON_SETTINGS,
            "The running Add-Ons declare more than {MAX_ADDON_SETTINGS} settings"
        );
        for (id, behaviour) in catalog.behaviours() {
            let depends = |target: &str| {
                catalog
                    .packages
                    .get(id)
                    .is_some_and(|p| p.manifest.dependencies.contains_key(target))
            };
            for more in &behaviour.setting_items {
                let (target, _) = more.setting.split_once(':').unwrap_or_default();
                ensure!(
                    depends(target),
                    "{id}: setting_items adds to `{}`, of an Add-On it does not depend on",
                    more.setting
                );
                let i = *out.by_key.get(&more.setting).with_context(|| {
                    format!("{id}: setting_items: no setting `{}`", more.setting)
                })?;
                let s = &mut out.list[i];
                ensure!(
                    s.def.kind == SettingType::List,
                    "{id}: setting_items: `{}` is not a list",
                    more.setting
                );
                s.items.extend(more.items.iter().cloned());
                ensure!(
                    s.items.len() <= bri_package::setting::MAX_ITEMS,
                    "`{}` has more than {} choices",
                    more.setting,
                    bri_package::setting::MAX_ITEMS
                );
            }
            if let Some(when) = &behaviour.teams_shown_when {
                let setting = full_key(id, &when.setting);
                let (target, _) = setting.split_once(':').unwrap_or_default();
                ensure!(
                    (target == id || depends(target)) && out.by_key.contains_key(&setting),
                    "{id}: teams_shown_when names `{setting}`, not a setting of it or an Add-On it depends on"
                );
                out.teams_shown_when = Some(ShownWhen {
                    setting,
                    ..when.clone()
                });
            }
            for def in &behaviour.settings {
                // A server-wide setting shows by another server-wide one:
                // it has no mini-game to read a mini-game's from.
                if let Some(when) = &def.shown_when
                    && def.scope == SettingScope::Server
                {
                    let target = out.get(&full_key(id, &when.setting));
                    ensure!(
                        target.is_none_or(|t| t.def.scope == SettingScope::Server),
                        "{id}: server setting `{}` is shown by `{}`, which is not the server's",
                        def.key,
                        when.setting
                    );
                }
                if let Some(when) = def.shown_when.as_ref().filter(|w| w.setting.contains(':')) {
                    let (target, _) = when.setting.split_once(':').unwrap_or_default();
                    ensure!(
                        target == id || depends(target),
                        "{id}: setting `{}` is shown by `{}`, of an Add-On it does not depend on",
                        def.key,
                        when.setting
                    );
                    ensure!(
                        out.by_key.contains_key(&when.setting),
                        "{id}: setting `{}`: no setting `{}`",
                        def.key,
                        when.setting
                    );
                }
            }
        }
        for s in &out.list {
            let mut values = std::collections::BTreeSet::new();
            for item in &s.items {
                ensure!(
                    values.insert(&item.value),
                    "`{}` offers {} twice",
                    s.key(),
                    item.value
                );
            }
            s.check(&s.def.default)
                .map_err(|e| anyhow::anyhow!("`{}`: default: {e}", s.key()))?;
        }
        Ok(out)
    }
    pub(in crate::session) fn list(&self) -> &[AddOnSetting] {
        &self.list
    }
    pub(in crate::session) fn teams_shown_when(&self) -> Option<&ShownWhen> {
        self.teams_shown_when.as_ref()
    }
    pub(in crate::session) fn get(&self, key: &str) -> Option<&AddOnSetting> {
        self.by_key.get(key).map(|&i| &self.list[i])
    }
    /// The server setting standing for the v20 global `name`.
    pub(in crate::session) fn by_global(&self, name: &str) -> Option<&AddOnSetting> {
        self.by_global
            .get(&name.to_ascii_lowercase())
            .map(|&i| &self.list[i])
    }
    pub(in crate::session) fn has_team_settings(&self) -> bool {
        self.list.iter().any(|s| s.def.scope == SettingScope::Team)
    }
}

/// `key` as `package` names it: its own, or `namespace:key`.
fn full_key(package: &str, key: &str) -> String {
    if key.contains(':') {
        key.to_owned()
    } else {
        format!("{package}:{key}")
    }
}

/// How far a player may go in a mini-game's settings.
struct EditorLevels {
    host: bool,
    super_admin: bool,
    admin: bool,
    /// Their trust with the game's creator (3: they made it).
    trust: u8,
}
impl EditorLevels {
    fn allows(&self, editor: SettingEditor) -> bool {
        use bri_world::authority::trust;
        match editor {
            SettingEditor::Owner => true,
            SettingEditor::Admin => self.admin,
            SettingEditor::SuperAdmin => self.super_admin,
            SettingEditor::Host => self.host,
            SettingEditor::Creator => self.host || self.trust >= trust::YOU,
            SettingEditor::FullTrust => self.host || self.trust >= trust::FULL,
            SettingEditor::BuildTrust => self.host || self.trust >= trust::BUILD,
        }
    }
}

/// Who is changing settings.
#[derive(Debug, Clone, Copy)]
pub(in crate::session) enum Editor<'a> {
    /// A player, through the menu: the game's owner or an admin.
    Player(OwnerId),
    /// A player the host's rules let edit the game (`on_minigame_request`'s
    /// `edit`), owner or not.
    Granted(OwnerId),
    /// An Add-On's rules (`set_setting`).
    Rules(&'a str),
}

impl Session {
    /// Every running Add-On's settings, for players' menus.
    pub fn addon_settings(&self) -> Vec<AddOnSetting> {
        self.packages
            .as_ref()
            .map(|h| h.settings.list().to_vec())
            .unwrap_or_default()
    }
    /// When the Add-On Settings window shows the team list: while this
    /// setting (`namespace:key`) holds one of these values.
    pub fn addon_teams_shown_when(&self) -> Option<ShownWhen> {
        self.packages
            .as_ref()
            .and_then(|h| h.settings.teams_shown_when().cloned())
    }

    /// A setting's value in `game` (or its `team`), or the server's (no
    /// game), or its default.
    pub(in crate::session) fn setting_value(
        &self,
        package: &str,
        game: Option<u64>,
        team: Option<u64>,
        key: &str,
    ) -> Result<SettingValue, String> {
        let key = full_key(package, key);
        let host = self.packages.as_ref().ok_or("No Add-Ons are running")?;
        let s = host
            .settings
            .get(&key)
            .ok_or_else(|| format!("No setting `{key}`"))?;
        let stored = match (s.def.scope, game) {
            // One read only as the server starts or loads a map has the
            // value it had then.
            (SettingScope::Server, None) if s.def.restart => self.started_settings.get(&key),
            (SettingScope::Server, None) => self.admin.settings.addon_settings.get(&key),
            (SettingScope::Server, Some(_)) => {
                return Err(format!("`{key}` is the server's: server_setting(key)"));
            }
            (_, None) => {
                return Err(format!(
                    "`{key}` is {}",
                    match s.def.scope {
                        SettingScope::Team => "each team's: team_setting(game, team, key)",
                        _ => "each mini-game's: setting(game, key)",
                    }
                ));
            }
            (_, Some(game)) => {
                let g = self
                    .minigames
                    .game(mg::GameId(game))
                    .map_err(|_| format!("No mini-game {game}"))?;
                self.game_setting(g, s, team, &key)?
            }
        };
        // A stored value an Add-On update no longer allows reads as the
        // default.
        let value = stored
            .filter(|v| s.check(v).is_ok())
            .cloned()
            .unwrap_or_else(|| s.def.default.clone());
        // An item or player type this server lacks reads as none.
        Ok(if self.has_content(s.def.kind, &value) {
            value
        } else {
            SettingValue::Text(String::new())
        })
    }

    /// A setting's value as players read it (Slayer's `getDisplayValue`).
    pub(in crate::session) fn setting_text(
        &self,
        package: &str,
        game: u64,
        team: Option<u64>,
        key: &str,
    ) -> Result<String, String> {
        let value = self.setting_value(package, Some(game), team, key)?;
        let full = full_key(package, key);
        let s = self
            .packages
            .as_ref()
            .and_then(|h| h.settings.get(&full))
            .ok_or_else(|| format!("No setting `{full}`"))?;
        Ok(match (s.def.kind, &value) {
            (SettingType::Bool, SettingValue::Bool(b)) => (if *b { "True" } else { "False" }).into(),
            (SettingType::List, v) => s
                .items
                .iter()
                .find(|i| &i.value == v)
                .map_or_else(|| v.to_string(), |i| i.name.clone()),
            (SettingType::Item, SettingValue::Text(id)) if id.is_empty() => "NONE".into(),
            (SettingType::Item, SettingValue::Text(id)) => self
                .weapons
                .pack
                .items
                .get(id)
                .map_or_else(|| id.clone(), |i| i.ui_name.clone()),
            (SettingType::PlayerType, SettingValue::Text(id)) => self
                .archetypes
                .find(id)
                .map_or_else(|| id.clone(), |a| self.archetypes.resolve(a).name.clone()),
            (_, v) => v.to_string(),
        })
    }

    /// How setting `key` (the package's own or `namespace:key`) is
    /// declared, for its rules: title, category, scope, kind, quiet and
    /// resets.
    pub(in crate::session) fn setting_info(&self, package: &str, key: &str) -> Option<serde_json::Value> {
        let s = self.packages.as_ref()?.settings.get(&full_key(package, key))?;
        Some(serde_json::json!({
            "title": s.def.title,
            "category": s.def.category,
            "scope": s.def.scope,
            "type": s.def.kind,
            "quiet": s.def.quiet,
            "resets": s.def.resets,
        }))
    }

    /// As the server starts or loads a map with its Add-Ons: the server
    /// settings read only then ([`SettingDef::restart`]) take the values the
    /// host has set, until the next start.
    pub(in crate::session) fn start_settings(&mut self) {
        let Some(host) = self.packages.as_ref() else {
            return;
        };
        self.started_settings = host
            .settings
            .list()
            .iter()
            .filter(|s| s.def.restart)
            .filter_map(|s| {
                let key = s.key();
                Some((
                    key.clone(),
                    self.admin.settings.addon_settings.get(&key)?.clone(),
                ))
            })
            .collect();
    }

    /// The server setting standing for the v20 global `name`
    /// (`$Pref::Server::TT::Ammo`), whichever running Add-On declares it,
    /// or `None` when none does: an unset global.
    pub(in crate::session) fn pref_value(&self, name: &str) -> Option<SettingValue> {
        let s = self.packages.as_ref()?.settings.by_global(name)?;
        self.setting_value(&s.package, None, None, &s.def.key).ok()
    }

    /// A mini-game's (or its team's) stored value of `s`.
    fn game_setting<'g>(
        &self,
        g: &'g mg::MiniGame,
        s: &AddOnSetting,
        team: Option<u64>,
        key: &str,
    ) -> Result<Option<&'g SettingValue>, String> {
        let game = g.id.0;
        Ok(match (s.def.scope, team) {
            (SettingScope::Minigame, None) => g.addon_settings.get(key),
            (SettingScope::Team, Some(team)) => {
                let t = u32::try_from(team)
                    .ok()
                    .and_then(|t| g.teams.get(mg::TeamId(t)))
                    .ok_or_else(|| format!("No team {team} in mini-game {game}"))?;
                t.addon_settings.get(key)
            }
            (SettingScope::Minigame, Some(_)) => {
                return Err(format!("`{key}` is the mini-game's: setting(game, key)"));
            }
            (SettingScope::Team, None) => {
                return Err(format!(
                    "`{key}` is each team's: team_setting(game, team, key)"
                ));
            }
            (SettingScope::Server, _) => {
                return Err(format!("`{key}` is the server's: server_setting(key)"));
            }
        })
    }

    /// Check the host's new server-wide Add-On settings against the running
    /// Add-Ons' definitions. Keys no running Add-On declares are kept
    /// unchecked for when it runs again; they never reach a rule.
    pub(in crate::session) fn check_server_addon_settings(
        &self,
        values: &BTreeMap<String, SettingValue>,
    ) -> Result<()> {
        let Some(host) = self.packages.as_ref() else {
            return Ok(());
        };
        for (key, value) in values {
            let Some(s) = host.settings.get(key) else {
                continue;
            };
            ensure!(
                s.def.scope == SettingScope::Server,
                "`{key}` is not a server-wide setting"
            );
            s.check(value).map_err(anyhow::Error::msg)?;
            ensure!(
                self.has_content(s.def.kind, value),
                "This server has no {value}"
            );
        }
        Ok(())
    }

    /// Whether an item or player type setting's `value` names one this
    /// server has (none, `""`, always does). Other kinds always do.
    pub(super) fn has_content(&self, kind: SettingType, value: &SettingValue) -> bool {
        let Some(id) = value.as_text().filter(|id| !id.is_empty()) else {
            return true;
        };
        match kind {
            SettingType::Item => self.weapons.contains_item(id),
            SettingType::PlayerType => self.archetypes.find(id).is_some(),
            _ => true,
        }
    }

    /// Change settings of `game` and its teams, and (from the menu) its
    /// team list, all or nothing. Values are checked against their
    /// definitions; a player must be the game's owner or an admin, and an
    /// admin for an admins-only setting.
    pub(in crate::session) fn edit_settings(
        &mut self,
        editor: Editor,
        game: mg::GameId,
        settings: Vec<SettingEdit>,
        teams: Option<Vec<TeamEdit>>,
        quiet: bool,
    ) -> Result<()> {
        let host = self.packages.as_ref().context("No Add-Ons are running")?;
        let g = self.minigames.game(game).ok().context("No such mini-game")?;
        // Who may change what: Slayer's permission levels.
        let may = match editor {
            Editor::Player(owner) | Editor::Granted(owner) => {
                let player = self.peers.get(&owner).context("Unknown player")?.combat.player;
                ensure!(
                    matches!(editor, Editor::Granted(_)) || self.minigames.can_edit(player, game),
                    "Only the mini-game's owner or an admin can change its settings"
                );
                Some(self.editor_levels(owner, g))
            }
            Editor::Rules(_) => None,
        };
        let package = match editor {
            Editor::Rules(p) => p,
            Editor::Player(_) | Editor::Granted(_) => "",
        };
        let check = |edit: &SettingEdit, team: bool| -> Result<(String, SettingScope)> {
            let key = full_key(package, &edit.key);
            let s = host.settings.get(&key).with_context(|| format!("No setting `{key}`"))?;
            let scope = s.def.scope;
            ensure!(
                scope != SettingScope::Server && (scope == SettingScope::Team) == team,
                "`{key}` is {}",
                match scope {
                    SettingScope::Minigame => "the mini-game's, not a team's",
                    SettingScope::Team => "each team's, not the mini-game's",
                    SettingScope::Server =>
                        "the server's: the host changes it in the Admin menu's Add-On Settings",
                }
            );
            if let Some(levels) = &may {
                ensure!(levels.allows(s.def.editor), "You may not change {}", s.def.title);
            }
            if let Some(v) = &edit.value {
                s.check(v).map_err(anyhow::Error::msg)?;
                ensure!(self.has_content(s.def.kind, v), "This server has no {v}");
            }
            Ok((key, scope))
        };
        let mut changes = Vec::new();
        for edit in &settings {
            let (key, _) = check(edit, false)?;
            changes.push(mg::SettingChange {
                team: None,
                key,
                value: edit.value.clone(),
            });
        }
        // Team settings, by the team's place in the edited list until the
        // new teams have ids.
        let mut team_changes: Vec<(usize, mg::SettingChange)> = Vec::new();
        let specs = match &teams {
            Some(list) => {
                ensure!(
                    host.settings.has_team_settings(),
                    "No running Add-On uses teams"
                );
                let mut specs = Vec::with_capacity(list.len());
                for (i, t) in list.iter().enumerate() {
                    if let Some(id) = t.id {
                        ensure!(g.teams.get(mg::TeamId(id)).is_some(), "No such team");
                    }
                    ensure!(t.color < 64, "A team's colour is one of the 64 paint colours");
                    for edit in &t.settings {
                        team_changes.push((
                            i,
                            mg::SettingChange {
                                team: None,
                                key: check(edit, true)?.0,
                                value: edit.value.clone(),
                            },
                        ));
                    }
                    specs.push(mg::TeamSpec {
                        id: t.id.map(mg::TeamId),
                        name: t.name.trim().to_owned(),
                        color: t.color,
                    });
                }
                Some(specs)
            }
            None => None,
        };
        let by = match editor {
            Editor::Player(o) | Editor::Granted(o) => Some(o),
            Editor::Rules(_) => None,
        };
        let (friendly_fire, ally_same_color) = (g.teams.friendly_fire, g.teams.ally_same_color);
        let mut effects = Vec::new();
        if let Some(specs) = specs {
            let (ids, out) = self
                .minigames
                .set_teams(game, specs, friendly_fire, ally_same_color)
                .map_err(|e| anyhow::anyhow!("Teams rejected: {e}"))?;
            effects.extend(out);
            for (i, mut change) in team_changes {
                change.team = Some(ids[i]);
                changes.push(change);
            }
        }
        // Each change by its team, now that new teams have ids.
        let edit = super::game_hooks::SettingsEdit {
            by,
            quiet,
            changes: changes
                .iter()
                .map(|c| (c.key.clone(), c.team.map(|t| u64::from(t.0))))
                .collect(),
        };
        effects.extend(
            self.minigames
                .set_addon_settings(game, changes)
                .map_err(|e| anyhow::anyhow!("Settings rejected: {e}"))?,
        );
        if let Some(host) = self.packages.as_mut() {
            host.game_hooks.editing = Some(edit.clone());
        }
        let result = self.apply_minigame_effects(effects);
        if let Some(host) = self.packages.as_mut() {
            host.game_hooks.editing = None;
        }
        result
    }

    /// What `owner` may change in `game`'s settings.
    fn editor_levels(&self, owner: OwnerId, game: &mg::MiniGame) -> EditorLevels {
        let (super_admin, host) = self.admin.rank(owner);
        let peer = self.peers.get(&owner);
        let admin = peer.is_some_and(|p| self.minigames.player(p.combat.player).is_ok_and(|p| p.admin));
        // The host owns a shared or game mode's mini-game.
        let creator = if game.is_server() || game.shared { None } else { self.owner_of(game.owner) };
        let trust = match (peer, creator) {
            (_, Some(c)) if c == owner => bri_world::authority::trust::YOU,
            (Some(p), Some(c)) => p.actor.trust_level(c),
            _ => 0,
        };
        EditorLevels { host, super_admin: super_admin || host, admin: admin || super_admin || host, trust: if host { 3 } else { trust } }
    }

    /// `set_setting` / `set_team_setting` from `package`'s rules.
    pub(in crate::session) fn package_set_setting(
        &mut self,
        package: &str,
        game: u64,
        team: Option<u64>,
        key: String,
        value: Option<SettingValue>,
    ) -> Result<()> {
        let game = mg::GameId(game);
        let edit = SettingEdit { key, value };
        match team {
            None => self.edit_settings(Editor::Rules(package), game, vec![edit], None, false),
            Some(team) => {
                let team = u32::try_from(team).ok().map(mg::TeamId).context("No such team")?;
                let key = full_key(package, &edit.key);
                let host = self.packages.as_ref().context("No Add-Ons are running")?;
                let s = host.settings.get(&key).with_context(|| format!("No setting `{key}`"))?;
                ensure!(
                    s.def.scope == SettingScope::Team,
                    "`{key}` is not each team's"
                );
                if let Some(v) = &edit.value {
                    s.check(v).map_err(anyhow::Error::msg)?;
                    ensure!(self.has_content(s.def.kind, v), "This server has no {v}");
                }
                let effects = self
                    .minigames
                    .set_addon_settings(
                        game,
                        vec![mg::SettingChange {
                            team: Some(team),
                            key,
                            value: edit.value,
                        }],
                    )
                    .map_err(|e| anyhow::anyhow!("Setting rejected: {e}"))?;
                self.apply_minigame_effects(effects)
            }
        }
    }
}
