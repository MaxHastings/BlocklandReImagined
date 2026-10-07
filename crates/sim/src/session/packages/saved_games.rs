//! A build's mini-game (Slayer's saved mini-game configs, `.mgame.csv` and
//! `.teams.csv`, and its fly-through camera's `.pathcam` beside them):
//! saving a build keeps the mini-game its saver runs, with its settings,
//! Add-On settings, teams and the Add-On state kept per mini-game
//! (`per_minigame` state keys); loading the build sets that up again in
//! the game its loader runs, or a new one of theirs, once its bricks are
//! in. Settings of Add-Ons this server does not run are left out, and the
//! build loads whatever happens to its mini-game.
use super::*;
use bri_minigames as mg;
use bri_package::setting::{SettingScope, SettingValue};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SavedMiniGame {
    #[serde(default)]
    color: u8,
    #[serde(default)]
    settings: mg::Settings,
    /// Add-On settings apart from their defaults, by `namespace:key`.
    #[serde(default)]
    addon_settings: BTreeMap<String, SettingValue>,
    #[serde(default)]
    teams: Vec<SavedTeam>,
    /// By package, its `per_minigame` keys' values for this game.
    #[serde(default)]
    packages: BTreeMap<String, BTreeMap<String, serde_json::Value>>,
    /// Who ran it (their principal, hex), in a host's recovery and shutdown
    /// saves only: a restarted host sets it up again for them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    owner: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SavedTeam {
    /// Its slot, which the build's rules name it by (`Team` conditions,
    /// `setTeam`); a build saved without one takes the lowest free slot.
    #[serde(default)]
    id: Option<u32>,
    name: String,
    color: u8,
    #[serde(default)]
    addon_settings: BTreeMap<String, SettingValue>,
}

fn principal_hex(principal: &bri_admin::Principal) -> String {
    principal.0.iter().map(|b| format!("{b:02x}")).collect()
}

impl Session {
    /// The mini-game `owner` runs (owns, or may edit as an admin), as a
    /// build saves it; none when they run none.
    pub(in crate::session) fn saved_minigame(&self, owner: OwnerId) -> Option<serde_json::Value> {
        let player = self.peers.get(&owner)?.combat.player;
        let game = self.minigames.player(player).ok()?.game?;
        if !self.minigames.can_edit(player, game) {
            return None;
        }
        self.minigame_snapshot(game)
    }

    /// `game` as a build saves it: its settings, Add-On settings, teams
    /// and per-game Add-On state (rules' presets: `minigame_snapshot`).
    pub(in crate::session) fn minigame_snapshot(
        &self,
        game: mg::GameId,
    ) -> Option<serde_json::Value> {
        let g = self.minigames.game(game).ok()?;
        let mut packages: BTreeMap<String, BTreeMap<String, serde_json::Value>> = BTreeMap::new();
        if let Some(host) = self.packages.as_ref() {
            let id = game.0.to_string();
            for (package, behaviour) in host.catalog.behaviours() {
                for (key, def) in &behaviour.state.global {
                    if let Some(value) = def
                        .per_minigame
                        .then(|| host.store.global(package, key))
                        .flatten()
                        .and_then(|games| games.get(&id))
                    {
                        packages
                            .entry(package.clone())
                            .or_default()
                            .insert(key.clone(), value.clone());
                    }
                }
            }
        }
        let saved = SavedMiniGame {
            color: g.color,
            settings: g.settings.clone(),
            addon_settings: g.addon_settings.clone(),
            teams: g
                .teams
                .list
                .iter()
                .map(|t| SavedTeam {
                    id: Some(t.id.0),
                    name: t.name.clone(),
                    color: t.color,
                    addon_settings: t.addon_settings.clone(),
                })
                .collect(),
            packages,
            owner: None,
        };
        serde_json::to_value(saved).ok()
    }

    /// What a host keeps of itself to come back from a crash or a restart:
    /// the world as saved, and one mini-game (the server's, else the
    /// oldest running one, with who runs it; else one held since the
    /// restart for a player who has not come back yet). Cheap: the world's
    /// bricks are a persistent map, so this copies no brick.
    pub fn recovery_snapshot(&self) -> (bri_world::World, Option<serde_json::Value>) {
        let game = self
            .minigames
            .server_game()
            .or_else(|| self.minigames.games().map(|g| g.id).min());
        let minigame = match game {
            Some(game) => self.minigame_snapshot(game).map(|mut saved| {
                let owner = self.minigames.game(game).ok().and_then(|g| {
                    self.peers
                        .values()
                        .find(|p| p.combat.player == g.owner)
                        .and_then(|p| p.principal)
                });
                if let (Some(owner), Some(fields)) = (owner, saved.as_object_mut()) {
                    fields.insert("owner".into(), principal_hex(&owner).into());
                }
                saved
            }),
            None => self.held_minigame.clone(),
        };
        (self.saved_world(), minigame)
    }

    /// A restarted host's saved mini-game: it is set up again for the
    /// player who ran it when they join, as loading their build would.
    /// One with no recorded owner (a server's game mode sets its own up)
    /// is not.
    pub fn hold_minigame(&mut self, saved: serde_json::Value) {
        self.held_minigame = Some(saved);
    }
    /// The mini-game [`Self::hold_minigame`] holds, until its player joins.
    pub fn held_minigame(&self) -> Option<&serde_json::Value> {
        self.held_minigame.as_ref()
    }

    pub(in crate::session) fn restore_held_minigame(&mut self, owner: OwnerId) {
        let Some(principal) = self.peers.get(&owner).and_then(|p| p.principal) else {
            return;
        };
        let theirs = self
            .held_minigame
            .as_ref()
            .and_then(|saved| saved.get("owner"))
            .and_then(|o| o.as_str())
            == Some(principal_hex(&principal).as_str());
        if let Some(saved) = self.held_minigame.take_if(|_| theirs) {
            let note = match self.try_restore_minigame(owner, saved) {
                Ok(()) => {
                    "Your mini-game from before the server restarted was set up again.".into()
                }
                Err(error) => format!(
                    "Your mini-game from before the server restarted was not set up: {error:#}"
                ),
            };
            self.private_chat(owner, note);
        }
    }

    /// Set up a loaded build's mini-game for whoever loaded it, telling
    /// them if it could not be.
    pub(in crate::session) fn restore_saved_minigame(
        &mut self,
        owner: OwnerId,
        saved: serde_json::Value,
    ) {
        if !self.peers.contains_key(&owner) {
            return;
        }
        if let Err(error) = self.try_restore_minigame(owner, saved) {
            self.notify(
                owner,
                Notice::Chat(format!(
                    "{}The build's mini-game was not set up: {error:#}",
                    combat::color_code(0)
                )),
            );
        }
    }

    fn try_restore_minigame(&mut self, owner: OwnerId, saved: serde_json::Value) -> Result<()> {
        let saved: SavedMiniGame =
            serde_json::from_value(saved).context("this server cannot read it")?;
        let player = self
            .peers
            .get(&owner)
            .context("Unknown player")?
            .combat
            .player;
        let game = match self.minigames.player(player).ok().and_then(|p| p.game) {
            Some(game) => {
                ensure!(
                    self.minigames.can_edit(player, game),
                    "you are in a mini-game you do not run"
                );
                // A game mode's game keeps the mode's own settings; a
                // player's game takes the build's (its owner may change
                // them).
                let g = self
                    .minigames
                    .game(game)
                    .ok()
                    .context("No such mini-game")?;
                if !g.is_server() && g.owner == player {
                    self.minigame_act(
                        owner,
                        MiniGameRequest::Configure {
                            settings: saved.settings.clone(),
                        },
                        false,
                        None,
                    )?;
                }
                game
            }
            None => {
                let free = self.minigames.free_colors();
                let color = std::iter::once(saved.color)
                    .chain(free.iter().copied())
                    .find(|c| free.contains(c))
                    .context("every mini-game colour is taken")?;
                self.minigame_act(
                    owner,
                    MiniGameRequest::Create {
                        color,
                        settings: saved.settings.clone(),
                    },
                    false,
                    None,
                )?;
                self.minigames
                    .player(player)
                    .ok()
                    .and_then(|p| p.game)
                    .context("No mini-game was made")?
            }
        };
        self.restore_addon_settings(Editor::Player(owner), game, &saved)?;
        self.restore_per_minigame(game, saved.packages);
        self.queue_game_event("loaded", game.0);
        Ok(())
    }

    /// Put a snapshot (`minigame_snapshot`) into `game` for `package`'s
    /// rules: Slayer loading a saved config into a running game.
    pub(in crate::session) fn restore_minigame_snapshot(
        &mut self,
        package: &str,
        game: mg::GameId,
        saved: serde_json::Value,
    ) -> Result<()> {
        let saved: SavedMiniGame =
            serde_json::from_value(saved).context("not a mini-game snapshot")?;
        let effects = self
            .minigames
            .host_configure(game, saved.settings.clone())
            .map_err(|e| anyhow::anyhow!("Settings rejected: {e}"))?;
        self.apply_minigame_effects(effects)?;
        self.restore_addon_settings(Editor::Rules(package), game, &saved)?;
        self.restore_per_minigame(game, saved.packages);
        self.queue_game_event("loaded", game.0);
        Ok(())
    }

    /// The build's Add-On settings and teams, all at once; settings of
    /// Add-Ons not running here, or values they no longer allow, are left
    /// at their defaults.
    fn restore_addon_settings(
        &mut self,
        editor: Editor,
        game: mg::GameId,
        saved: &SavedMiniGame,
    ) -> Result<()> {
        let Some(host) = self.packages.as_ref() else {
            return Ok(());
        };
        let fits = |key: &str, value: &SettingValue, scope: SettingScope| {
            host.settings.get(key).is_some_and(|s| {
                s.def.scope == scope
                    && s.check(value).is_ok()
                    && self.has_content(s.def.kind, value)
            })
        };
        let edits = |values: &BTreeMap<String, SettingValue>, scope| -> Vec<SettingEdit> {
            values
                .iter()
                .filter(|(key, value)| fits(key, value, scope))
                .map(|(key, value)| SettingEdit {
                    key: key.clone(),
                    value: Some(value.clone()),
                })
                .collect()
        };
        let current = self
            .minigames
            .game(game)
            .ok()
            .context("No such mini-game")?;
        let mut settings = edits(&saved.addon_settings, SettingScope::Minigame);
        // What the build left at its default goes back to it.
        settings.extend(
            current
                .addon_settings
                .keys()
                .filter(|key| !saved.addon_settings.contains_key(*key))
                .filter(|key| host.settings.get(key).is_some())
                .map(|key| SettingEdit {
                    key: key.clone(),
                    value: None,
                }),
        );
        // The build's teams replace the game's, each in its saved slot, so
        // its rules' team references and a kept team's members stay put.
        let teams = (!saved.teams.is_empty() || !current.teams.list.is_empty()).then(|| {
            saved
                .teams
                .iter()
                .map(|t| TeamEdit {
                    id: t.id,
                    name: t.name.clone(),
                    color: t.color,
                    settings: edits(&t.addon_settings, SettingScope::Team),
                })
                .collect::<Vec<_>>()
        });
        if settings.is_empty() && teams.is_none() {
            return Ok(());
        }
        self.edit_settings(editor, game, settings, teams, true, true)
    }

    /// Replace the chosen game's declared `per_minigame` entries. Absence
    /// clears an entry; a rejected value retains it with a diagnostic.
    /// Other games and ordinary state stay as they are.
    fn restore_per_minigame(
        &mut self,
        game: mg::GameId,
        packages: BTreeMap<String, BTreeMap<String, serde_json::Value>>,
    ) {
        let Some(host) = self.packages.as_mut() else {
            return;
        };
        let id = game.0.to_string();
        let mut changed = false;
        let catalog = host.catalog.clone();
        for (package, behaviour) in catalog.behaviours() {
            for (key, _) in behaviour
                .state
                .global
                .iter()
                .filter(|(_, d)| d.per_minigame)
            {
                let saved = packages.get(package).and_then(|values| values.get(key));
                let ns = host.store.namespace_mut(package);
                let before = state::stored_size(&ns.global);
                let mut global = ns.global.clone();
                let Some(value) = global.get_mut(key) else {
                    continue;
                };
                let Some(games) = value.as_object_mut() else {
                    note(
                        host,
                        Diagnostic::warning(
                            "state.restore",
                            format!("Mini-game {id}: `{key}` is not a per-mini-game map"),
                        )
                        .at(package.clone()),
                    );
                    continue;
                };
                // Absence in a snapshot means no authored value for this
                // game. Other games and ordinary state keys stay as they are.
                match saved {
                    Some(saved) => {
                        games.insert(id.clone(), saved.clone());
                    }
                    None => {
                        games.remove(&id);
                    }
                }
                // Admit the entire declared key, including its game-id map.
                // Individually valid entries can otherwise exceed the value
                // size/depth limits and break replication or save decoding.
                if let Err(error) = state::check_value(value) {
                    note(
                        host,
                        Diagnostic::warning(
                            "state.restore",
                            format!("Mini-game {id}: saved `{key}` was not restored: {error:#}"),
                        )
                        .at(package.clone()),
                    );
                    continue;
                }
                if global == ns.global {
                    continue;
                }
                let after = state::stored_size(&global);
                let total = (host.state_bytes + after).saturating_sub(before);
                if after > before
                    && (after > state::MAX_GLOBAL_STATE_BYTES || total > state::MAX_STATE_BYTES)
                {
                    note(
                        host,
                        Diagnostic::error(
                            "state.budget",
                            format!(
                                "Mini-game {id}: saved `{key}` would exceed package state limits"
                            ),
                        )
                        .at(package.clone()),
                    );
                    continue;
                }
                ns.global = global;
                host.state_bytes = total;
                changed = true;
            }
        }
        if changed {
            self.package_revision += 1;
        }
    }
}
