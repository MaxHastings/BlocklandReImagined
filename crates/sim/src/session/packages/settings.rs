//! Add-On settings (`behaviour.json` `settings`): typed values a mini-game's
//! owner or an admin edits in the Mini-Game window's Add-On Settings, and
//! the Add-On's rules read with `setting(game, key)`. Slayer's preferences
//! and team preferences are these.
//!
//! The engine keeps the values (on the mini-game and its teams), checks
//! every change against its definition and who made it, sends the
//! definitions and values to players for the menu, and tells the rules
//! with `on_minigame`'s `settings` event. The Add-On decides what each
//! setting means.
use super::*;
use bri_minigames as mg;
use bri_package::setting::{
    SettingDef, SettingEditor, SettingItem, SettingScope, SettingType, SettingValue,
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
                out.by_key.insert(format!("{id}:{}", def.key), out.list.len());
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
            for def in &behaviour.settings {
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
    pub(in crate::session) fn get(&self, key: &str) -> Option<&AddOnSetting> {
        self.by_key.get(key).map(|&i| &self.list[i])
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

    /// A setting's value in `game` (or its `team`), or its default.
    pub(in crate::session) fn setting_value(
        &self,
        package: &str,
        game: u64,
        team: Option<u64>,
        key: &str,
    ) -> Result<SettingValue, String> {
        let key = full_key(package, key);
        let host = self.packages.as_ref().ok_or("No Add-Ons are running")?;
        let s = host
            .settings
            .get(&key)
            .ok_or_else(|| format!("No setting `{key}`"))?;
        let g = self
            .minigames
            .game(mg::GameId(game))
            .map_err(|_| format!("No mini-game {game}"))?;
        let server = self.server_addon_settings();
        let stored = match (s.def.scope, team) {
            (SettingScope::Server, None) => server.get(&key),
            (SettingScope::Server, Some(_)) => {
                return Err(format!("`{key}` is the server's: setting(game, key)"));
            }
            (SettingScope::Minigame, None) => g.addon_settings.get(&key),
            (SettingScope::Team, Some(team)) => {
                let t = u32::try_from(team)
                    .ok()
                    .and_then(|t| g.teams.get(mg::TeamId(t)))
                    .ok_or_else(|| format!("No team {team} in mini-game {game}"))?;
                t.addon_settings.get(&key)
            }
            (SettingScope::Minigame, Some(_)) => {
                return Err(format!("`{key}` is the mini-game's: setting(game, key)"));
            }
            (SettingScope::Team, None) => {
                return Err(format!("`{key}` is each team's: team_setting(game, team, key)"));
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
                (scope == SettingScope::Team) == team,
                "`{key}` is {}",
                match scope {
                    SettingScope::Minigame => "the mini-game's, not a team's",
                    SettingScope::Server => "the server's, not a team's",
                    SettingScope::Team => "each team's, not the mini-game's",
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
        let mut server = Vec::new();
        for edit in &settings {
            let (key, scope) = check(edit, false)?;
            if scope == SettingScope::Server {
                server.push((key, edit.value.clone()));
                continue;
            }
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
        let mut edit = super::game_hooks::SettingsEdit {
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
        let mut server_changed = Vec::new();
        for (key, value) in server {
            let before = self.server_addon_settings().get(&key).cloned();
            if before != value {
                server_changed.push((key.clone(), None));
            }
            self.set_server_setting(&key, value)?;
        }
        if let Some(host) = self.packages.as_mut() {
            host.game_hooks.editing = Some(edit.clone());
        }
        let result = self.apply_minigame_effects(effects);
        if let Some(host) = self.packages.as_mut() {
            host.game_hooks.editing = None;
        }
        if !server_changed.is_empty() {
            edit.changes = server_changed;
            self.queue_settings_event(game.0, edit);
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

    /// Every server-wide setting's stored value, by `namespace:key`.
    pub(in crate::session) fn server_addon_settings(&self) -> BTreeMap<String, SettingValue> {
        let Some(host) = self.packages.as_ref() else {
            return BTreeMap::new();
        };
        let mut out = BTreeMap::new();
        for (package, data) in &host.host_data {
            let Some(serde_json::Value::Object(map)) = data.get(host_data::SERVER_SETTINGS) else {
                continue;
            };
            for (key, value) in map {
                let full = format!("{package}:{key}");
                let Some(s) = host.settings.get(&full) else { continue };
                if s.def.scope != SettingScope::Server {
                    continue;
                }
                if let Ok(v) = serde_json::from_value::<SettingValue>(value.clone())
                    && s.check(&v).is_ok()
                {
                    out.insert(full, v);
                }
            }
        }
        out
    }

    /// Keep a server-wide setting's value, or forget it with `None`.
    fn set_server_setting(&mut self, key: &str, value: Option<SettingValue>) -> Result<()> {
        let (package, name) = key.split_once(':').context("No such setting")?;
        let mut map = match self.host_data(package, host_data::SERVER_SETTINGS) {
            Some(serde_json::Value::Object(m)) => m.clone(),
            _ => serde_json::Map::new(),
        };
        match value {
            Some(v) => {
                map.insert(name.to_owned(), serde_json::to_value(v)?);
            }
            None => {
                map.remove(name);
            }
        }
        let package = package.to_owned();
        self.set_host_data(&package, host_data::SERVER_SETTINGS, Some(serde_json::Value::Object(map)))
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
                    "`{key}` is the mini-game's: set_setting(game, key, value)"
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
