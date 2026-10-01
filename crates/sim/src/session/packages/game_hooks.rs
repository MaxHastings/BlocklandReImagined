//! Add-On rules for team games (Slayer and its modes): `on_minigame` hears
//! what happens to mini-games, `on_pick_spawn` chooses where a player
//! appears, `zones` are touch boxes over bricks (Torque's `createTrigger`),
//! and the operations that set up teams, keep score, reset rounds and put
//! items on bricks.
//!
//! The engine owns the mechanisms (who is on which team, who may hurt whom,
//! where a box is and who stands in it); the Add-On owns the policy (how
//! many teams, what a flag does, what wins).
use super::*;
use bri_minigames as mg;
use bri_package_runtime::content::Behaviour;
use bri_package_runtime::ops::GameRule;
use bri_package_runtime::rhai::{ImmutableString, Map};
use bri_package_runtime::script::{BrickView, MinigameView, TeamView};

/// Mini-game events waiting for `on_minigame`, oldest first.
const MAX_PENDING_EVENTS: usize = 1024;
/// Most bricks one zone kind is checked over per check; more are skipped
/// (and reported) rather than stalling a tick.
const MAX_ZONE_BRICKS: usize = 4096;

/// One thing that happened to a mini-game, for `on_minigame`.
#[derive(Debug, Clone, PartialEq)]
pub(in crate::session) struct GameEvent {
    kind: &'static str,
    game: u64,
    player: Option<OwnerId>,
    team: Option<u64>,
    /// The `namespace:key`s of a `settings` event.
    keys: Vec<String>,
    /// A `round_end` event's winning teams and players.
    teams: Vec<u64>,
    players: Vec<OwnerId>,
    /// Who did it (a `kicked` event's kicker, a `settings` event's editor).
    by: Option<OwnerId>,
    /// A `rejected` event's invitation was ignored.
    ignored: bool,
    /// A `settings` event's editor asked not to tell the game's players.
    quiet: bool,
    /// A `settings` event's changes: each `namespace:key` and its team
    /// (none: the mini-game's or the server's).
    changes: Vec<(String, Option<u64>)>,
}

/// Who is changing Add-On settings, while their change is applied.
#[derive(Debug, Clone, Default)]
pub(in crate::session) struct SettingsEdit {
    pub by: Option<OwnerId>,
    pub quiet: bool,
    pub changes: Vec<(String, Option<u64>)>,
}

/// Add-On state a session keeps for these hooks.
#[derive(Default)]
pub(in crate::session) struct GameHooks {
    events: VecDeque<GameEvent>,
    /// Who stood in each zone at its last check: by package and zone, the
    /// brick and player pairs.
    inside: BTreeMap<(String, usize), BTreeSet<(BrickId, OwnerId)>>,
    /// Zone periods the rules changed (`set_zone_period`), in ticks.
    periods: BTreeMap<(String, usize), u32>,
    /// The settings change being applied, for its `settings` event.
    pub(in crate::session) editing: Option<SettingsEdit>,
    /// An `on_pick_spawn` hook is running: a spawn its operations cause
    /// (a reset) takes the engine's choice, so a hook never recurses.
    picking: bool,
}

impl Session {
    /// Queue what a mini-game effect means to Add-On rules. Runs before the
    /// effect is applied, so a membership change still knows the old game.
    pub(in crate::session) fn note_minigame_effect(&mut self, effect: &mg::Effect) {
        if let mg::Effect::Ended { game } = effect
            && let Some(host) = self.packages.as_mut()
        {
            host.reports.forget_game(game.0);
        }
        let Some(host) = self.packages.as_ref() else {
            return;
        };
        if !host.catalog.behaviours().any(|(_, b)| b.on_minigame) {
            return;
        }
        let event = |kind, game: mg::GameId| GameEvent {
            kind,
            game: game.0,
            player: None,
            team: None,
            keys: Vec::new(),
            teams: Vec::new(),
            players: Vec::new(),
            by: None,
            ignored: false,
            quiet: false,
            changes: Vec::new(),
        };
        let mut out = Vec::new();
        match effect {
            mg::Effect::Created { game } => out.push(event("created", *game)),
            mg::Effect::Configured { game } => out.push(event("configured", *game)),
            mg::Effect::Ended { game } => out.push(event("ended", *game)),
            mg::Effect::Reset { game, .. } => out.push(event("reset", *game)),
            mg::Effect::TeamsConfigured { game } => out.push(event("teams", *game)),
            mg::Effect::RoundEnded {
                game,
                teams,
                players,
            } => out.push(GameEvent {
                teams: teams.iter().map(|t| u64::from(t.0)).collect(),
                players: players.iter().filter_map(|p| self.owner_of(*p)).collect(),
                ..event("round_end", *game)
            }),
            mg::Effect::AddOnSettings { game, keys } => {
                let edit = host.game_hooks.editing.clone().unwrap_or_default();
                out.push(GameEvent {
                    keys: keys.clone(),
                    by: edit.by,
                    quiet: edit.quiet,
                    changes: edit
                        .changes
                        .into_iter()
                        .filter(|(k, _)| keys.contains(k))
                        .collect(),
                    ..event("settings", *game)
                })
            }
            mg::Effect::TeamChanged { player, game, team } => {
                if let Some(owner) = self.owner_of(*player) {
                    out.push(GameEvent {
                        player: Some(owner),
                        team: team.map(|t| u64::from(t.0)),
                        ..event("team", *game)
                    });
                }
            }
            mg::Effect::Membership { player, game, .. } => {
                if let Some(owner) = self.owner_of(*player) {
                    let old = self.last_membership.get(&owner).copied().flatten();
                    if old != *game {
                        if let Some(old) = old {
                            out.push(GameEvent {
                                player: Some(owner),
                                ..event("left", old)
                            });
                        }
                        if let Some(new) = game {
                            out.push(GameEvent {
                                player: Some(owner),
                                ..event("joined", *new)
                            });
                        }
                    }
                }
            }
            _ => {}
        }
        let host = self.packages.as_mut().expect("checked");
        for e in out {
            if host.game_hooks.events.len() == MAX_PENDING_EVENTS {
                host.game_hooks.events.pop_front();
            }
            host.game_hooks.events.push_back(e);
        }
    }

    /// Queue a `kind` event of `game` for `on_minigame` that no mini-game
    /// effect raises (`loaded`, a build's mini-game set up again).
    pub(in crate::session) fn queue_game_event(&mut self, kind: &'static str, game: u64) {
        self.push_game_event(GameEvent {
            kind,
            game,
            player: None,
            team: None,
            keys: Vec::new(),
            teams: Vec::new(),
            players: Vec::new(),
            by: None,
            ignored: false,
            quiet: false,
            changes: Vec::new(),
        });
    }

    /// Queue a `kind` event about `player` in `game` for `on_minigame`
    /// (`kicked`, by `by`; `rejected`, an invitation turned down).
    pub(in crate::session) fn queue_player_event(
        &mut self,
        kind: &'static str,
        game: u64,
        player: OwnerId,
        by: Option<OwnerId>,
        ignored: bool,
    ) {
        self.push_game_event(GameEvent {
            kind,
            game,
            player: Some(player),
            team: None,
            keys: Vec::new(),
            teams: Vec::new(),
            players: Vec::new(),
            by,
            ignored,
            quiet: false,
            changes: Vec::new(),
        });
    }

    /// Queue a `settings` event for changes no mini-game effect carries
    /// (server-wide settings).
    pub(in crate::session) fn queue_settings_event(&mut self, game: u64, edit: SettingsEdit) {
        self.push_game_event(GameEvent {
            kind: "settings",
            game,
            player: None,
            team: None,
            keys: edit.changes.iter().map(|(k, _)| k.clone()).collect(),
            teams: Vec::new(),
            players: Vec::new(),
            by: edit.by,
            ignored: false,
            quiet: edit.quiet,
            changes: edit.changes,
        });
    }

    fn push_game_event(&mut self, event: GameEvent) {
        let Some(host) = self.packages.as_mut() else {
            return;
        };
        if !host.catalog.behaviours().any(|(_, b)| b.on_minigame) {
            return;
        }
        if host.game_hooks.events.len() == MAX_PENDING_EVENTS {
            host.game_hooks.events.pop_front();
        }
        host.game_hooks.events.push_back(event);
    }

    pub(in crate::session) fn deliver_minigame_events(&mut self) {
        let Some(host) = self.packages.as_mut() else {
            return;
        };
        let events = std::mem::take(&mut host.game_hooks.events);
        if events.is_empty() {
            return;
        }
        let hooks = declaring(host, |b| b.on_minigame);
        let id = |v: Option<u64>| v.map_or(Dynamic::UNIT, |v| Dynamic::from_int(v as i64));
        for e in events {
            let mut map = Map::new();
            map.insert("kind".into(), e.kind.into());
            map.insert("game".into(), Dynamic::from_int(e.game as i64));
            map.insert("player".into(), id(e.player));
            map.insert("team".into(), id(e.team));
            map.insert("by".into(), id(e.by));
            if e.kind == "rejected" {
                map.insert("ignored".into(), e.ignored.into());
            }
            if e.kind == "round_end" {
                let ids = |v: &[u64]| {
                    Dynamic::from_array(v.iter().map(|i| Dynamic::from_int(*i as i64)).collect())
                };
                map.insert("teams".into(), ids(&e.teams));
                map.insert("players".into(), ids(&e.players));
            }
            if e.kind == "settings" {
                map.insert(
                    "keys".into(),
                    Dynamic::from_array(e.keys.iter().map(|k| k.clone().into()).collect()),
                );
                map.insert("quiet".into(), e.quiet.into());
                map.insert(
                    "changes".into(),
                    Dynamic::from_array(
                        e.changes
                            .iter()
                            .map(|(key, team)| {
                                let mut c = Map::new();
                                c.insert("key".into(), key.clone().into());
                                c.insert("team".into(), id(*team));
                                Dynamic::from_map(c)
                            })
                            .collect(),
                    ),
                );
            }
            for package in &hooks {
                let _ = self.run_package(
                    package,
                    "on_minigame",
                    vec![Dynamic::from_map(map.clone())],
                    Budget::Command,
                    None,
                    None,
                    None,
                );
                self.charge_work(package);
            }
        }
    }

    /// `on_pick_spawn(player)`: where an Add-On's rules want `owner` to
    /// appear, if any does. A brick id appears on that brick as on a spawn
    /// brick; `[x, y, z]` appears there.
    pub(in crate::session) fn package_pick_spawn(&mut self, owner: OwnerId) -> Option<(Vec3, f32)> {
        let host = self.packages.as_ref()?;
        if self.bots.is_brick_bot(owner) || host.game_hooks.picking {
            return None;
        }
        let hooks = declaring(host, |b| b.on_pick_spawn);
        if hooks.is_empty() {
            return None;
        }
        self.packages.as_mut()?.game_hooks.picking = true;
        let chosen = self.pick_spawn_from(owner, hooks);
        if let Some(host) = self.packages.as_mut() {
            host.game_hooks.picking = false;
        }
        chosen
    }
    fn pick_spawn_from(&mut self, owner: OwnerId, hooks: Vec<String>) -> Option<(Vec3, f32)> {
        for package in hooks {
            let answer = self.run_package(
                &package,
                "on_pick_spawn",
                vec![Dynamic::from_int(owner as i64)],
                Budget::Command,
                None,
                None,
                None,
            );
            self.charge_work(&package);
            let Ok(answer) = answer else {
                continue;
            };
            if answer.is_unit() {
                continue;
            }
            if let Ok(brick) = answer.as_int() {
                match self.simulation.state().bricks.get(&(brick as u64)) {
                    Some(b) => {
                        let yaw = -f32::from(b.quarter_turns) * std::f32::consts::FRAC_PI_2;
                        return Some((Vec3::from(b.position) + Vec3::Y * 0.1, yaw));
                    }
                    None => {
                        self.hook_warning(
                            &package,
                            format!("on_pick_spawn returned brick {brick}, which does not exist"),
                        );
                        continue;
                    }
                }
            }
            let point = answer.clone().into_typed_array::<f64>().ok().or_else(|| {
                answer
                    .clone()
                    .into_array()
                    .ok()?
                    .iter()
                    .map(|v| {
                        v.as_float()
                            .ok()
                            .or_else(|| v.as_int().ok().map(|i| i as f64))
                    })
                    .collect()
            });
            match point.as_deref() {
                Some(&[x, y, z]) if [x, y, z].iter().all(|v| v.is_finite() && v.abs() < 1e6) => {
                    return Some((Vec3::new(x as f32, y as f32, z as f32), 0.0));
                }
                _ => self.hook_warning(
                    &package,
                    format!(
                        "on_pick_spawn must return (), a brick id or [x, y, z], not {}",
                        answer.type_name()
                    ),
                ),
            }
        }
        None
    }

    /// Check every zone that is due: who entered, stayed in or left the box
    /// over each of its bricks since its last check.
    pub(in crate::session) fn step_zones(&mut self) {
        let Some(host) = self.packages.as_ref() else {
            return;
        };
        let tick = self.simulation.state().tick;
        let zones: Vec<(String, usize, bri_package_runtime::content::ZoneDef)> = host
            .catalog
            .behaviours()
            .flat_map(|(package, b)| {
                b.zones
                    .iter()
                    .enumerate()
                    .filter(|(i, z)| {
                        let period = host
                            .game_hooks
                            .periods
                            .get(&(package.clone(), *i))
                            .copied()
                            .unwrap_or_else(|| z.period_ticks());
                        tick.is_multiple_of(u64::from(period))
                    })
                    .map(|(i, z)| (package.clone(), i, z.clone()))
            })
            .collect();
        if zones.is_empty() {
            return;
        }
        let bodies: Vec<(OwnerId, Vec3, Vec3)> = self
            .peers
            .iter()
            .filter(|(o, p)| p.combat.alive && !self.bots.is_brick_bot(**o))
            .map(|(o, p)| {
                let state = p.player.state();
                let tuning = p.player.tuning();
                let half = tuning.width * 0.5 * state.scale;
                let height = if state.crouched {
                    tuning.crouch_height
                } else {
                    tuning.stand_height
                } * state.scale;
                let feet = Vec3::from(state.feet);
                (
                    *o,
                    feet - Vec3::new(half, 0.0, half),
                    feet + Vec3::new(half, height, half),
                )
            })
            .collect();
        for (package, index, zone) in zones {
            let mut now = BTreeSet::new();
            let mut skipped = 0usize;
            for kind in &zone.bricks {
                for (n, brick) in self.simulation.bricks_of(kind).enumerate() {
                    if n == MAX_ZONE_BRICKS {
                        skipped += 1;
                        break;
                    }
                    let Some((lo, mut hi)) = self.simulation.brick_box(brick) else {
                        continue;
                    };
                    hi.y += zone.above;
                    for (owner, a, b) in &bodies {
                        if a.cmple(hi).all() && b.cmpge(lo).all() {
                            now.insert((brick, *owner));
                        }
                    }
                }
            }
            if skipped > 0 {
                self.hook_warning(
                    &package,
                    format!("a zone covers more than {MAX_ZONE_BRICKS} bricks of one kind; the rest are not checked"),
                );
            }
            let host = self.packages.as_mut().expect("checked");
            let before = host
                .game_hooks
                .inside
                .insert((package.clone(), index), now.clone())
                .unwrap_or_default();
            let mut calls: Vec<(BrickId, OwnerId, &str)> = Vec::new();
            calls.extend(before.difference(&now).map(|(b, o)| (*b, *o, "leave")));
            calls.extend(now.difference(&before).map(|(b, o)| (*b, *o, "enter")));
            if zone.ticks {
                calls.extend(now.intersection(&before).map(|(b, o)| (*b, *o, "tick")));
            }
            for (brick, owner, event) in calls {
                let _ = self.run_package(
                    &package,
                    "on_zone",
                    vec![
                        Dynamic::from_int(owner as i64),
                        Dynamic::from_int(brick as i64),
                        event.into(),
                    ],
                    Budget::Command,
                    None,
                    None,
                    None,
                );
                self.charge_work(&package);
            }
        }
    }

    /// The connected members of mini-game `game`, for prints and chat to
    /// all of them.
    pub(in crate::session) fn minigame_members(&self, game: u64) -> Result<Vec<OwnerId>> {
        let g = self
            .minigames
            .game(mg::GameId(game))
            .map_err(|_| anyhow::anyhow!("No mini-game {game}"))?;
        Ok(g.members.iter().filter_map(|p| self.owner_of(*p)).collect())
    }

    /// Every mini-game as scripts see it.
    pub(in crate::session) fn script_minigames(&self) -> Vec<MinigameView> {
        self.minigames
            .games()
            .map(|g| MinigameView {
                id: g.id.0,
                title: g.settings.title.clone(),
                owner: (!g.is_server()).then(|| self.owner_of(g.owner)).flatten(),
                members: g.members.iter().filter_map(|p| self.owner_of(*p)).collect(),
                round: g.round,
                teams: g
                    .teams
                    .list
                    .iter()
                    .map(|t| TeamView {
                        id: u64::from(t.id.0),
                        name: t.name.clone(),
                        color: t.color,
                    })
                    .collect(),
                friendly_fire: g.teams.friendly_fire,
                ally_same_color: g.teams.ally_same_color,
                round_over: g.round_over,
                player_type: g.settings.player_type.clone(),
                loadout: g.settings.loadout.iter().map(|i| i.clone().unwrap_or_default()).collect(),
                points_kill_player: i64::from(g.settings.points_kill_player),
                settings: serde_json::to_value(&g.settings).unwrap_or_default(),
                default: self.minigames.default_game() == Some(g.id),
                color: g.color,
                paint_color: g.paint_color,
            })
            .collect()
    }

    /// The mini-game whose bricks `owner`'s bricks are: the game they run,
    /// or the one they play in when it uses every player's bricks.
    pub(in crate::session) fn brick_game(&self, owner: u64) -> Option<mg::GameId> {
        let account = mg::AccountId(owner);
        self.minigames
            .games()
            .find(|g| {
                g.owner.account == account
                    || (g.settings.use_all_players_bricks
                        && g.members.iter().any(|m| m.account == account))
            })
            .map(|g| g.id)
    }

    /// A brick as scripts see it.
    pub(in crate::session) fn brick_view(&self, id: BrickId) -> Option<BrickView> {
        let b = self.simulation.state().bricks.get(&id)?;
        let kind = match &b.definition {
            bri_world::ContentRef::Resolved(kind) => kind.clone(),
            bri_world::ContentRef::Unresolved { .. } => return None,
        };
        let (lo, hi) = self.simulation.brick_box(id)?;
        let item = match &b.item_spawn.item {
            Some(bri_world::ContentRef::Resolved(item)) => item.clone(),
            _ => String::new(),
        };
        Some(BrickView {
            id,
            kind,
            position: ((lo + hi) * 0.5).to_array(),
            turns: b.quarter_turns,
            min: lo.to_array(),
            max: hi.to_array(),
            color: b.color,
            owner: b.owner,
            game: self.brick_game(b.owner).map(|g| g.0),
            name: b.name.clone().unwrap_or_default(),
            item,
        })
    }

    /// `set_zone_period`: check zone `zone` of `package` every `period_ms`.
    pub(in crate::session) fn package_set_zone_period(
        &mut self,
        package: &str,
        zone: u32,
        period_ms: u32,
    ) -> Result<()> {
        let host = self.packages.as_mut().context("No packages are enabled")?;
        let zones = host
            .catalog
            .behaviours()
            .find(|(id, _)| *id == package)
            .map_or(0, |(_, b)| b.zones.len());
        ensure!(
            (zone as usize) < zones,
            "`{package}` has no zone {zone} (it has {zones})"
        );
        host.game_hooks.periods.insert(
            (package.to_owned(), zone as usize),
            bri_package_runtime::content::ZoneDef::ticks_of(period_ms),
        );
        Ok(())
    }
    /// Whether a game's rules may change `brick`: the world's, a
    /// mini-game's (v20's `minigameCanUse`), or one `caller` fully trusts.
    fn package_may_edit(&self, brick: BrickId, caller: Option<OwnerId>) -> Result<()> {
        let b = self
            .simulation
            .state()
            .bricks
            .get(&brick)
            .context("No such brick")?;
        let trusted = b.owner == 0
            || self.brick_game(b.owner).is_some()
            || caller
                .and_then(|c| self.peers.get(&c))
                .is_some_and(|p| p.actor.trusted(b.owner, bri_world::authority::trust::FULL));
        ensure!(
            trusted,
            "Brick {brick} is not a mini-game's and the caller has no trust on it"
        );
        Ok(())
    }
    /// `set_brick_color`: repaint a brick, as a game's `setColor` did.
    pub(in crate::session) fn package_set_brick_color(
        &mut self,
        brick: BrickId,
        color: u8,
        caller: Option<OwnerId>,
    ) -> Result<()> {
        self.package_may_edit(brick, caller)?;
        ensure!(
            usize::from(color) < self.simulation.state().palette.len(),
            "That colour is not in this server's palette"
        );
        self.simulation.mutate(brick, |b| b.color = color)?;
        self.dirty.insert(brick);
        Ok(())
    }
    /// `set_brick_item`: the item a brick holds out, as v20's
    /// `fxDTSBrick::setItem`. The brick must be the world's or one the
    /// calling player has full trust on.
    pub(in crate::session) fn package_set_brick_item(
        &mut self,
        brick: BrickId,
        item: Option<String>,
        caller: Option<OwnerId>,
    ) -> Result<()> {
        // A game's rules may stock its own bricks (a flag stand on the
        // field its owner built), as v20's `minigameCanUse` let them.
        self.package_may_edit(brick, caller)?;
        if let Some(item) = &item {
            ensure!(
                self.item_spawners.bounds.contains_key(item),
                "No item `{item}` to put on a brick"
            );
        }
        let restock = item.is_some();
        self.simulation.mutate(brick, |b| {
            b.item_spawn.item = item.map(bri_world::ContentRef::Resolved)
        })?;
        self.dirty.insert(brick);
        if restock {
            let tick = self.simulation.state().tick;
            self.item_spawners.restock(brick, tick);
        }
        Ok(())
    }

    /// Apply a mini-game operation an Add-On's rules asked for.
    pub(in crate::session) fn apply_minigame_op(&mut self, op: Op) -> Result<()> {
        let player_of = |s: &Self, owner: u64| -> Result<mg::PlayerId> {
            Ok(s.peers.get(&owner).context("No such player")?.combat.player)
        };
        let effects = match op {
            Op::SetTeams {
                game,
                teams,
                friendly_fire,
                ally_same_color,
            } => {
                let specs = teams
                    .into_iter()
                    .map(|t| {
                        Ok(mg::TeamSpec {
                            id: t
                                .id
                                .map(|id| u32::try_from(id).map(mg::TeamId))
                                .transpose()
                                .ok()
                                .context("No such team")?,
                            name: t.name,
                            color: t.color,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                self.minigames
                    .set_teams(mg::GameId(game), specs, friendly_fire, ally_same_color)
                    .map_err(|e| anyhow::anyhow!("Teams rejected: {e}"))?
                    .1
            }
            Op::SetTeam { player, team } => {
                let target = player_of(self, player)?;
                let team = team
                    .map(|t| u32::try_from(t).map(mg::TeamId))
                    .transpose()
                    .ok()
                    .context("No such team")?;
                self.minigames
                    .assign_team(target, team)
                    .map_err(|e| anyhow::anyhow!("Team rejected: {e}"))?
            }
            Op::SetScore { player, value, add } => {
                let target = player_of(self, player)?;
                let value = i32::try_from(value).context("Score out of range")?;
                self.minigames
                    .event_score(target, value, add)
                    .map_err(|e| anyhow::anyhow!("Score rejected: {e}"))?
            }
            Op::ResetMinigame { game } => self
                .minigames
                .execute(mg::Command::Reset {
                    game: mg::GameId(game),
                    authority: mg::EventAuthority::System,
                })
                .map_err(|e| anyhow::anyhow!("Reset rejected: {e}"))?,
            Op::EndRound {
                game,
                teams,
                players,
            } => {
                let teams = teams
                    .into_iter()
                    .map(|t| u32::try_from(t).map(mg::TeamId))
                    .collect::<Result<Vec<_>, _>>()
                    .ok()
                    .context("No such team")?;
                let players = players
                    .into_iter()
                    .map(|p| player_of(self, p))
                    .collect::<Result<Vec<_>>>()?;
                self.minigames
                    .end_round(mg::GameId(game), teams, players)
                    .map_err(|e| anyhow::anyhow!("Round end rejected: {e}"))?
            }
            Op::SetGameRule { game, rule } => {
                let game = mg::GameId(game);
                let rejected = |e: mg::Error| anyhow::anyhow!("Mini-game rule rejected: {e}");
                match rule {
                    GameRule::Default(on) => {
                        let now = self.minigames.default_game();
                        let next = match (on, now) {
                            (true, _) => Some(game),
                            (false, Some(g)) if g == game => None,
                            (false, other) => other,
                        };
                        self.minigames.set_default_game(next).map_err(rejected)?
                    }
                    GameRule::PaintColor(paint) => {
                        self.minigames.set_paint_color(game, paint).map_err(rejected)?
                    }
                    GameRule::Region(region) => {
                        self.minigames
                            .set_region(game, region.map(|[min, max]| mg::Region { min, max }))
                            .map_err(rejected)?;
                        Vec::new()
                    }
                    GameRule::NameDistance(d) => {
                        self.minigames.set_name_distance(game, d).map_err(rejected)?;
                        Vec::new()
                    }
                    GameRule::KeepScores(keep) => {
                        self.minigames.set_keep_scores(game, keep).map_err(rejected)?;
                        Vec::new()
                    }
                    GameRule::Cleanup { leave } => {
                        self.minigames
                            .set_cleanup(game, mg::CleanupRules { leave })
                            .map_err(rejected)?;
                        Vec::new()
                    }
                    GameRule::ClaimsBricks(on) => {
                        self.minigames.set_claims_bricks(game, on).map_err(rejected)?;
                        Vec::new()
                    }
                    GameRule::Settings(patch) => {
                        let current = &self.minigames.game(game).map_err(rejected)?.settings;
                        let settings = patched_settings(current, &patch)?;
                        self.minigames.host_configure(game, settings).map_err(rejected)?
                    }
                    GameRule::End => self.minigames.host_end(game).map_err(rejected)?,
                }
            }
            Op::CreateMinigame {
                owner,
                settings,
                paint,
            } => {
                let defaults = self.minigames.catalog().defaults.clone();
                let settings = patched_settings(&defaults, &settings)?;
                let color = *self
                    .minigames
                    .free_colors()
                    .first()
                    .context("Every mini-game colour is taken")?;
                let (game, effects) = match owner {
                    Some(owner) => {
                        let actor = player_of(self, owner)?;
                        let effects = self
                            .minigames
                            .execute(mg::Command::Create {
                                actor,
                                color,
                                settings,
                            })
                            .map_err(|e| anyhow::anyhow!("Mini-game not made: {e}"))?;
                        let game = self
                            .minigames
                            .player(actor)
                            .ok()
                            .and_then(|p| p.game)
                            .context("No mini-game was made")?;
                        (game, effects)
                    }
                    None => {
                        let game = self
                            .minigames
                            .host_create_shared(color, settings)
                            .map_err(|e| anyhow::anyhow!("Mini-game not made: {e}"))?;
                        (game, vec![mg::Effect::Created { game }])
                    }
                };
                let mut effects = effects;
                if let Some(paint) = paint {
                    effects.extend(
                        self.minigames
                            .set_paint_color(game, Some(paint))
                            .map_err(|e| anyhow::anyhow!("Mini-game colour: {e}"))?,
                    );
                }
                effects
            }
            Op::PlaceMember { player, game } => {
                let target = player_of(self, player)?;
                self.minigames
                    .host_place(target, game.map(mg::GameId))
                    .map_err(|e| anyhow::anyhow!("Placing rejected: {e}"))?
            }
            Op::HoldRespawn { player, held } => {
                let target = player_of(self, player)?;
                self.minigames
                    .hold_respawn(target, held)
                    .map_err(|e| anyhow::anyhow!("Respawn hold rejected: {e}"))?;
                Vec::new()
            }
            _ => unreachable!("not a mini-game operation"),
        };
        self.apply_minigame_effects(effects)
    }
}

impl Session {
    /// `on_ride`: whether the rules let `owner` board `vehicle`.
    pub(in crate::session) fn package_ride(&mut self, owner: OwnerId, vehicle: u64) -> bool {
        let Some(host) = self.packages.as_ref() else {
            return true;
        };
        let hooks = declaring(host, |b| b.on_ride);
        if hooks.is_empty() || self.bots.is_bot(owner) {
            return true;
        }
        let id = |v: Option<u64>| v.map_or(Dynamic::UNIT, |v| Dynamic::from_int(v as i64));
        let mut info = Map::new();
        info.insert("vehicle".into(), Dynamic::from_int(vehicle as i64));
        info.insert(
            "owner".into(),
            id(self.vehicle_owner_and_mass(vehicle).map(|(o, _)| o)),
        );
        info.insert(
            "spawn_brick".into(),
            id(self.vehicles.brick_of.get(&bri_vehicles::VehicleId(vehicle)).copied()),
        );
        for package in hooks {
            let reply = self.run_package(
                &package,
                "on_ride",
                vec![Dynamic::from_int(owner as i64), Dynamic::from_map(info.clone())],
                Budget::Command,
                Some(owner),
                None,
                None,
            );
            self.charge_work(&package);
            let Ok(reply) = reply else {
                continue;
            };
            if reply.is_unit() || reply.clone().try_cast::<bool>() == Some(true) {
                continue;
            }
            if let Some(text) = reply.clone().try_cast::<ImmutableString>() {
                self.notify(
                    owner,
                    Notice::Center {
                        text: text.to_string(),
                        seconds: 2.0,
                    },
                );
            }
            return false;
        }
        true
    }
}

/// What `on_minigame_request` made of a player's mini-game request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::session) enum Answer {
    /// The engine's own rules decide.
    Engine,
    /// The rules let the player do it, owner or not.
    Granted,
    /// Refused: a chat line, or a message box with a title.
    Refused { title: Option<String>, text: String },
}

impl Session {
    /// Ask the rules declaring `on_minigame_request` whether `owner` may
    /// `action` mini-game `game` (to `target`). `teams` is how many teams an
    /// edit leaves the game with, when it changes them.
    pub(in crate::session) fn package_minigame_request(
        &mut self,
        owner: OwnerId,
        action: &str,
        game: Option<mg::GameId>,
        target: Option<OwnerId>,
        team: Option<u32>,
        teams: Option<usize>,
    ) -> Answer {
        let Some(host) = self.packages.as_ref() else {
            return Answer::Engine;
        };
        if self.bots.is_bot(owner) {
            return Answer::Engine;
        }
        let hooks = declaring(host, |b| b.on_minigame_request);
        let id = |v: Option<u64>| v.map_or(Dynamic::UNIT, |v| Dynamic::from_int(v as i64));
        let mut info = Map::new();
        info.insert("game".into(), id(game.map(|g| g.0)));
        info.insert("target".into(), id(target));
        info.insert("team".into(), id(team.map(u64::from)));
        info.insert("teams".into(), id(teams.map(|n| n as u64)));
        let mut answer = Answer::Engine;
        for package in hooks {
            let reply = self.run_package(
                &package,
                "on_minigame_request",
                vec![
                    Dynamic::from_int(owner as i64),
                    action.into(),
                    Dynamic::from_map(info.clone()),
                ],
                Budget::Command,
                Some(owner),
                None,
                None,
            );
            self.charge_work(&package);
            let Ok(reply) = reply else {
                continue;
            };
            if reply.is_unit() {
                continue;
            }
            if let Some(allowed) = reply.clone().try_cast::<bool>() {
                if allowed {
                    answer = Answer::Granted;
                    continue;
                }
                return Answer::Refused {
                    title: None,
                    text: "You don't have permission to do that.".into(),
                };
            }
            if let Some(text) = reply.clone().try_cast::<ImmutableString>() {
                return Answer::Refused {
                    title: None,
                    text: text.to_string(),
                };
            }
            if let Some(map) = reply.clone().try_cast::<Map>() {
                let field = |k: &str| map.get(k).and_then(|v| v.clone().try_cast::<ImmutableString>());
                if let Some(text) = field("text") {
                    return Answer::Refused {
                        title: Some(field("title").map_or_else(String::new, |t| t.to_string())),
                        text: text.to_string(),
                    };
                }
            }
            self.hook_warning(
                &package,
                format!(
                    "on_minigame_request must return (), true, false, a reason or #{{ title, text }}, not {}",
                    reply.type_name()
                ),
            );
        }
        answer
    }
}

/// `current` with the fields of `patch` over it (`set_minigame`,
/// `create_minigame`): a mini-game's own settings as JSON.
fn patched_settings(current: &mg::Settings, patch: &serde_json::Value) -> Result<mg::Settings> {
    let mut json = serde_json::to_value(current)?;
    let (Some(fields), Some(over)) = (json.as_object_mut(), patch.as_object()) else {
        anyhow::bail!("mini-game settings are a map");
    };
    for (key, value) in over {
        ensure!(
            fields.contains_key(key),
            "mini-game settings have no `{key}`"
        );
        fields.insert(key.clone(), value.clone());
    }
    serde_json::from_value(json).context("mini-game settings")
}

/// Packages whose behaviour declares a hook, in catalog order.
pub(super) fn declaring(host: &PackageHost, declares: fn(&Behaviour) -> bool) -> Vec<String> {
    host.catalog
        .behaviours()
        .filter(|(_, b)| declares(b))
        .map(|(id, _)| id.clone())
        .collect()
}
