//! A bot's life: kinds installed, brick and rules bots made, embodied,
//! named, put on teams and removed, and what the rest of the session asks
//! about them.
use super::*;

/// Motor yaw is positive right; derive it from the actual authored brick frame
/// rather than copying the opposite-sign native rotation angle.
fn spawn_yaw(brick: &bri_world::Brick) -> f32 {
    let forward = brick.transform().transform_vector3(Vec3::NEG_Z);
    forward.x.atan2(-forward.z)
}

impl Session {
    pub(in crate::session) fn bot_spawn_properties(
        &self,
        bot: OwnerId,
    ) -> Option<(BrickId, Option<u32>)> {
        let id = self.bots.brains.get(&bot)?.brick?;
        let brick = self.simulation.state().bricks.get(&id)?;
        Some((id, brick.vehicle.as_ref().and_then(|v| v.team)))
    }

    pub fn is_bot(&self, owner: OwnerId) -> bool {
        self.bots.is_bot(owner)
    }
    /// Install the bot kinds the enabled Add-Ons provide. Bots whose kind
    /// is gone leave at the next reconcile.
    pub fn set_bot_kinds(&mut self, kinds: Vec<BotKind>) -> Result<()> {
        ensure!(
            kinds.len() <= crate::bot_kind::MAX_KINDS,
            "Too many bot kinds"
        );
        for kind in &kinds {
            kind.validate()?;
        }
        self.bots.kinds = kinds;
        for brain in self.bots.brains.values_mut() {
            if let Some(kind) = self.bots.kinds.iter().find(|k| k.id == brain.kind.id) {
                brain.kind = kind.clone().into();
            }
        }
        self.vehicles.scanned = false;
        Ok(())
    }
    /// Whether a spawn brick choice names a bot kind this server has.
    /// Bots the server runs at once, from spawn bricks and rules together:
    /// the host's Max bots. Lowering it keeps the bots already running.
    pub fn bot_limit(&self) -> usize {
        (self.admin.settings.max_bots as usize).clamp(1, MAX_BOTS)
    }
    /// Whether one more bot of `kind` fits under the bot limit: each bot
    /// counts what its kind costs (`BotKind::cost`), so a server limited to
    /// 16 full bots runs 64 minimal creatures.
    pub(in crate::session) fn bot_room(&self, kind: &BotKind) -> bool {
        let load: f32 = self.bots.brains.values().map(|b| b.kind.cost()).sum();
        load + kind.cost() <= self.bot_limit() as f32 + 0.001
    }
    pub fn is_bot_kind(&self, id: &str) -> bool {
        self.bots.kind(id).is_some()
    }
    pub(in crate::session) fn bot_spawn_choice(&self, owner: OwnerId) -> Option<(Vec3, f32)> {
        let home = self.bots.home(owner)?;
        let yaw = self
            .bots
            .spawn_brick(owner)
            .and_then(|id| self.simulation.state().bricks.get(&id))
            .map_or(0.0, spawn_yaw);
        Some((home, yaw))
    }
    pub(in crate::session) fn reset_bot_facing(&mut self, owner: OwnerId) {
        if let Some(brain) = self.bots.brains.get_mut(&owner) {
            // A surviving brain starts its next control in the new body's frame.
            brain.yaw = self.peers[&owner].player.state().yaw;
            brain.pitch = 0.0;
        }
    }
    /// The owner of the spawn brick that placed this bot.
    pub(in crate::session) fn bot_brick_owner(&self, bot: OwnerId) -> Option<OwnerId> {
        let brick = self.bots.brains.get(&bot)?.brick?;
        Some(self.simulation.state().bricks.get(&brick)?.owner)
    }
    /// A rider in a bot mount's first seat moves it in place of its brain
    /// (`setControlObject` on a mount with no controlling client).
    pub(in crate::session) fn drive_bot(&mut self, bot: OwnerId, input: MoveInput) -> Result<()> {
        let Some(brain) = self.bots.brains.get_mut(&bot) else {
            return Ok(());
        };
        brain.sequence += 1;
        let sequence = brain.sequence;
        self.movement(bot, sequence, input)
    }
    /// Reconcile bots with spawn bricks naming a bot kind.
    pub(in crate::session) fn reconcile_bot_brick(
        &mut self,
        brick_id: BrickId,
        wanted: Option<&str>,
    ) -> Result<()> {
        let current = self.bots.by_brick.get(&brick_id).copied();
        let kind = wanted.and_then(|id| self.bots.kind(id)).cloned();
        let same = current
            .and_then(|bot| self.bots.brains.get(&bot))
            .is_some_and(|b| {
                let born = b.born.as_ref().unwrap_or(&b.kind);
                kind.as_ref().is_some_and(|k| k.id == born.id)
            });
        if let Some(bot) = current.filter(|_| !same) {
            self.drop_bot(bot)?;
        }
        if same {
            return Ok(());
        }
        let Some(kind) = kind else {
            return Ok(());
        };
        let Some(brick) = self.simulation.state().bricks.get(&brick_id) else {
            return Ok(());
        };
        if brick.damage_hidden {
            return Ok(());
        }
        let (home, builder) = (Vec3::from(brick.position) + Vec3::Y * 0.3, brick.owner);
        let yaw = spawn_yaw(brick);
        let name = self.brick_bot_name(&kind, brick_id, None);
        // A refused bot is never silent: the brick's builder is told why,
        // as for a vehicle the server has no room for.
        let limit = self.bot_limit();
        if !self.bot_room(&kind) {
            self.notify(
                builder,
                Notice::Center {
                    text: format!("\u{E000}Server is limited to {limit} bots"),
                    seconds: 2.0,
                },
            );
            return Ok(());
        }
        let joined = self.join_inner(name.clone(), home, false, true, None);
        if joined.is_err() {
            self.notify(
                builder,
                Notice::Center {
                    text: "\u{E000}Server is full".into(),
                    seconds: 2.0,
                },
            );
        }
        let crossed = self.crossings.count();
        if let Ok(bot) = joined {
            if let Err(e) = self.embody_bot(bot, &kind) {
                self.drop_bot(bot)?;
                self.notify(
                    builder,
                    Notice::Center {
                        text: format!("\u{E000}{e}"),
                        seconds: 3.0,
                    },
                );
                return Ok(());
            }
            self.bots.by_brick.insert(brick_id, bot);
            let mut brain = Brain::new(Some(brick_id), kind, home, bot, crossed);
            let peer = self.peers.get_mut(&bot).expect("newly embodied bot");
            peer.player
                .teleport(&mut self.simulation.physics, home, yaw)?;
            brain.yaw = peer.player.state().yaw;
            brain.named = name;
            self.bots.brains.insert(bot, brain);
            self.weapons.set_bot(bri_weapons::ActorId(bot), true)?;
            // Join the brick's game before announcing a life. Placement emits
            // the spawn event through respawn (or holds it until the game
            // actually releases the bot); firing here as well would announce
            // the same life twice and give the first event no game context.
            if let Some(game) = self.spawn_brick_game(brick_id) {
                let player = self.peers[&bot].combat.player;
                let effects = self
                    .minigames
                    .host_place(player, Some(game))
                    .map_err(|e| anyhow::anyhow!("Bot minigame: {e}"))?;
                self.apply_minigame_effects(effects)?;
            } else {
                self.fire_bot_spawn(bot);
            }
        }
        Ok(())
    }
    /// A spawn brick's bot comes back fresh at its brick (`spawnVehicle`,
    /// a mini-game reset): a new brain and a new life for the same player,
    /// so its mini-game membership and team stay as they were set. Only a
    /// missing bot, or one of another kind, is made again.
    pub(in crate::session) fn respawn_brick_bot(
        &mut self,
        brick_id: BrickId,
        kind: &str,
    ) -> Result<()> {
        if self
            .simulation
            .state()
            .bricks
            .get(&brick_id)
            .is_some_and(|b| b.damage_hidden)
        {
            return Ok(());
        }
        let current = self.bots.by_brick.get(&brick_id).copied().filter(|bot| {
            self.peers.contains_key(bot)
                && self
                    .bots
                    .brains
                    .get(bot)
                    .is_some_and(|b| b.born.as_ref().unwrap_or(&b.kind).id == kind)
        });
        let (Some(bot), Some(brick), Some(kind)) = (
            current,
            self.simulation.state().bricks.get(&brick_id),
            self.bots.kind(kind).cloned(),
        ) else {
            self.reconcile_bot_brick(brick_id, None)?;
            return self.reconcile_bot_brick(brick_id, Some(kind));
        };
        let home = Vec3::from(brick.position) + Vec3::Y * 0.3;
        let crossed = self.crossings.count();
        self.bots.claims.release_owner(bot);
        self.bots.hurt.remove(&bot);
        let mut brain = Brain::new(Some(brick_id), kind, home, bot, crossed);
        if let Some(old) = self.bots.brains.get(&bot) {
            // Its movement sequence only moves forward.
            brain.sequence = old.sequence;
            brain.named = old.named.clone();
        }
        self.bots.brains.insert(bot, brain);
        let player = self.peers[&bot].combat.player;
        let effects = self
            .minigames
            .execute(bri_minigames::Command::ForceRespawn { target: player })
            .map_err(|e| anyhow::anyhow!("Respawn rejected: {e}"))?;
        self.apply_minigame_effects(effects)
    }
    /// A new bot takes its kind's body, and keeps it through respawns and
    /// mini-games as a body an Add-On chose does (`set_archetype`).
    pub(super) fn embody_bot(&mut self, bot: OwnerId, kind: &BotKind) -> Result<()> {
        let body = match &kind.body {
            Some(body) => Some(self.archetypes.find(body).with_context(|| {
                format!("{}: its body {body} is in no enabled Add-On", kind.name)
            })?),
            None => None,
        };
        let pack = self.avatar_catalog.as_ref();
        let mut avatar = pack.map(|c| c.defaults.clone());
        // Each bot its own seeded look (`looks`); its kind's look on top.
        if let (Some(avatar), Some(pack)) = (avatar.as_mut(), pack) {
            looks::seeded_look(bot, avatar, pack, &self.simulation.state().palette);
        }
        if let (Some(look), Some(avatar)) = (&kind.look, avatar.as_mut()) {
            UniformParts {
                parts: look.parts.clone(),
                face: look.face.clone(),
                decal: look.decal.clone(),
            }
            .dress(avatar, pack);
            avatar.colors.extend(look.colors.clone());
        }
        let peer = self.peers.get_mut(&bot).context("No such bot")?;
        peer.avatar = avatar;
        peer.chosen_body = body.map(|archetype| super::bodies::ChosenBody {
            archetype,
            by: super::bodies::Chooser::AddOn,
        });
        let body = body.unwrap_or_else(|| crate::player_types::PlayerType::Standard.archetype());
        self.set_player_archetype(bot, body)
    }
    /// Every bot kind's body is an archetype the enabled Add-Ons provide.
    pub(in crate::session) fn check_bot_bodies(&self) -> Result<()> {
        for kind in &self.bots.kinds {
            ensure!(
                kind.emote
                    .as_deref()
                    .is_none_or(|e| BOT_EMOTES.contains(&e)),
                "Bot {}: its emote is one of {}",
                kind.id,
                BOT_EMOTES.join(", ")
            );
            if let Some(body) = &kind.body {
                ensure!(
                    self.archetypes.find(body).is_some(),
                    "Bot {}: its body {body} is in no enabled Add-On",
                    kind.id
                );
            }
        }
        Ok(())
    }
    /// A bot leaves the server, whatever made it.
    pub(in crate::session) fn drop_bot(&mut self, bot: OwnerId) -> Result<()> {
        self.bots.claims.release_owner(bot);
        if self.peers.contains_key(&bot) {
            self.disconnect(bot)?;
            self.departed.remove(&bot);
        }
        if let Some(brick) = self.bots.brains.remove(&bot).and_then(|b| b.brick) {
            self.bots.by_brick.remove(&brick);
        }
        self.bots.by_rules.remove(&bot);
        self.bots.hurt.remove(&bot);
        self.forget_player_state(bot);
        Ok(())
    }
    /// `add_bot`: a bot of `kind` joins `game` for `package`'s rules, on
    /// `team` when given (Slayer's `addBotToGame` and `addMember`).
    pub(in crate::session) fn add_rules_bot(
        &mut self,
        package: &str,
        game: u64,
        team: Option<u64>,
        kind: &str,
        name: &str,
    ) -> Result<()> {
        self.check_rules_bot(game, team, kind)?;
        let kind = self.bots.kind(kind).expect("checked").clone();
        let game = bri_minigames::GameId(game);
        let team = team.map(|t| bri_minigames::TeamId(t as u32));
        let drop = self.spawn_points.first().copied().unwrap_or(Vec3::Y);
        let bot = self.join_inner(name.to_owned(), drop, false, true, None)?;
        if let Err(e) = self.embody_bot(bot, &kind) {
            self.drop_bot(bot)?;
            return Err(e);
        }
        let crossed = self.crossings.count();
        self.bots
            .brains
            .insert(bot, Brain::new(None, kind, drop, bot, crossed));
        self.weapons.set_bot(bri_weapons::ActorId(bot), true)?;
        self.bots.by_rules.insert(bot, (package.to_owned(), game.0));
        let placed = (|| -> Result<()> {
            let player = self.peers[&bot].combat.player;
            let effects = self
                .minigames
                .host_place(player, Some(game))
                .map_err(|e| anyhow::anyhow!("Bot minigame: {e}"))?;
            self.apply_minigame_effects(effects)?;
            if let Some(team) = team {
                let effects = self
                    .minigames
                    .assign_team(player, Some(team))
                    .map_err(|e| anyhow::anyhow!("Team rejected: {e}"))?;
                self.apply_minigame_effects(effects)?;
                // It came in before it had a side: it appears where its
                // team does (`Slayer_TeamSO::addMember` spawns it again).
                let effects = self
                    .minigames
                    .execute(bri_minigames::Command::ForceRespawn { target: player })
                    .map_err(|e| anyhow::anyhow!("Respawn rejected: {e}"))?;
                self.apply_minigame_effects(effects)?;
            }
            Ok(())
        })();
        if placed.is_err() {
            self.drop_bot(bot)?;
        }
        placed
    }
    /// Whether `add_bot` can add a bot of `kind` to `game` now: room on
    /// the server and under the bot limit, a kind an enabled Add-On
    /// provides, a mini-game that exists and, when given, a team of it.
    /// What only happens as it joins (being given a body, being placed in
    /// the game) is not foreseen.
    pub(in crate::session) fn check_rules_bot(
        &self,
        game: u64,
        team: Option<u64>,
        kind: &str,
    ) -> Result<()> {
        let limit = self.bot_limit();
        let kind = self
            .bots
            .kind(kind)
            .with_context(|| format!("No bot kind `{kind}`: its Add-On is not enabled"))?;
        ensure!(self.bot_room(kind), "Server is limited to {limit} bots");
        ensure!(
            self.peers.len() + self.arrivals.len() < MAX_PARTICIPANTS,
            "Server is full"
        );
        let found = self
            .minigames
            .game(bri_minigames::GameId(game))
            .map_err(|_| anyhow::anyhow!("No mini-game {game}"))?;
        if let Some(team) = team {
            let team = u32::try_from(team)
                .ok()
                .map(bri_minigames::TeamId)
                .context("No such team")?;
            found.teams.get(team).context("No such team")?;
        }
        Ok(())
    }
    /// The bot, if `package`'s rules added it.
    pub(in crate::session) fn own_bot(&self, package: &str, bot: OwnerId) -> Result<()> {
        ensure!(
            self.bots.rules_package(bot) == Some(package),
            "Bot {bot} is not one `{package}` added"
        );
        Ok(())
    }
    pub(in crate::session) fn remove_rules_bot(
        &mut self,
        package: &str,
        bot: OwnerId,
    ) -> Result<()> {
        self.own_bot(package, bot)?;
        self.drop_bot(bot)
    }
    pub(in crate::session) fn rules_bot_tool(
        &mut self,
        package: &str,
        bot: OwnerId,
        slot: Option<u8>,
    ) -> Result<()> {
        self.check_rules_bot_tool(package, bot)?;
        self.equip_tool(bot, slot.map(usize::from))
    }
    /// Whether `bot_tool` can reach `bot`: one `package` added, alive.
    pub(in crate::session) fn check_rules_bot_tool(
        &self,
        package: &str,
        bot: OwnerId,
    ) -> Result<()> {
        self.own_bot(package, bot)?;
        ensure!(self.is_alive(bot), "Only a living bot holds things");
        Ok(())
    }
    /// Trigger release is a shot for a charged image. Abandoning hostile
    /// intent instead clears queued input and restarts the held image safely.
    pub(super) fn abort_bot_hand_charge(&mut self, bot: OwnerId) -> Result<bool> {
        if !self
            .weapons
            .image_state(ActorId(bot), 0)
            .is_some_and(|(i, _)| i.charges())
        {
            return Ok(false);
        }
        let release_can_fire = self
            .weapons
            .image_state(ActorId(bot), 0)
            .is_some_and(|(image, state)| charged_control::release_may_fire(image, state));
        self.release_trigger(bot)?;
        if release_can_fire {
            self.weapons.cancel_charge(ActorId(bot))
        } else {
            // A released shot's native Fire/cooldown must finish normally.
            Ok(true)
        }
    }
    /// Whether `rest_bot` may rest or wake `bot`: a bot `package` added,
    /// or one of a kind it (or an Add-On it is a companion of) provides.
    pub(in crate::session) fn check_rest_rules_bot(
        &self,
        package: &str,
        bot: OwnerId,
    ) -> Result<()> {
        let own_kind = self.bots.brains.get(&bot).is_some_and(|brain| {
            let Some((owner, _)) = brain.kind.id.split_once(':') else {
                return false;
            };
            owner == package
                || self.packages.as_ref().is_some_and(|host| {
                    host.catalog.packages.get(owner).is_some_and(|provider| {
                        provider.manifest.companions.iter().any(|id| id == package)
                    })
                })
        });
        ensure!(
            self.bots.rules_package(bot) == Some(package) || own_kind,
            "Bot {bot} was not added by `{package}` and its kind is not owned by that package or its companion"
        );
        ensure!(self.bots.brains.contains_key(&bot), "No such bot");
        Ok(())
    }
    pub(in crate::session) fn rest_rules_bot(
        &mut self,
        package: &str,
        bot: OwnerId,
        rest: bool,
    ) -> Result<()> {
        self.check_rest_rules_bot(package, bot)?;
        self.set_bot_resting(bot, rest)
    }
    pub(in crate::session) fn set_bot_resting(&mut self, bot: OwnerId, rest: bool) -> Result<()> {
        let brain = self.bots.brains.get_mut(&bot).context("No such bot")?;
        if rest && !brain.resting {
            brain.objective_threat = None;
            brain.set_goal(None);
            brain.target = None;
            brain.memory = None;
            brain.evidence_search.clear();
        }
        brain.resting = rest;
        if rest {
            self.bots.claims.release_owner(bot);
            if self.seated(bot) {
                self.dismount_vehicle(bot)?;
            }
        }
        Ok(())
    }
    pub(in crate::session) fn bot_bricks(&self) -> Vec<BrickId> {
        self.bots.by_brick.keys().copied().collect()
    }
    /// Bots whose brick vanished; reconcile removes them.
    pub(super) fn bot_bricks_pending(&self) -> Vec<BrickId> {
        self.bots
            .brains
            .values()
            .filter_map(|b| b.brick)
            .filter(|brick| !self.simulation.state().bricks.contains_key(brick))
            .collect()
    }
    /// Rules bots whose game ended, or who were put out of it: they leave
    /// with it (`Slayer_MiniGameSO::endGame` deletes its bots).
    pub(super) fn rules_bots_gone(&self) -> Vec<OwnerId> {
        self.bots
            .by_rules
            .iter()
            .filter(|(bot, (_, game))| {
                self.peers
                    .get(bot)
                    .and_then(|p| self.minigames.player(p.combat.player).ok())
                    .and_then(|p| p.game)
                    != Some(bri_minigames::GameId(*game))
            })
            .map(|(bot, _)| *bot)
            .collect()
    }
    /// The game a spawn brick's bot plays in: a game mode's own mini-game,
    /// which holds everyone on the server (bots of the map's own bricks
    /// too, which have no builder), and otherwise its builder's.
    pub(super) fn spawn_brick_game(&self, brick: BrickId) -> Option<bri_minigames::GameId> {
        if let Some(game) = self.minigames.server_game() {
            return Some(game);
        }
        let owner = self.simulation.state().bricks.get(&brick)?.owner;
        self.resource_game(owner)
    }
    /// What a spawn brick's bot is called. A brick its builder named gives
    /// "Kind (name)", so the Players list tells one brick's bot from
    /// another's. Otherwise a first name of its kind no other player goes
    /// by, kept while it lives (`looks`); a kind without a free one gives
    /// its kind and the team its Team choice names ("Blockhead Bot (Red)").
    pub(super) fn brick_bot_name(
        &self,
        kind: &BotKind,
        brick: BrickId,
        bot: Option<OwnerId>,
    ) -> String {
        let Some(b) = self.simulation.state().bricks.get(&brick) else {
            return kind.name.clone();
        };
        if let Some(name) = bot
            .and_then(|o| self.bots.brains.get(&o))
            .and_then(|b| b.name_override.as_ref())
        {
            return name.clone();
        }
        let named = b
            .name
            .as_deref()
            .map(|n| n.trim().trim_start_matches('_').trim())
            .filter(|n| !n.is_empty())
            .map(str::to_owned);
        if named.is_none() {
            if let Some(own) = bot
                .and_then(|o| self.bots.brains.get(&o))
                .map(|b| &b.named)
                .filter(|n| kind.first_names.contains(n))
            {
                return own.clone();
            }
            if let Some(first) = self.bot_first_name(kind, brick, bot) {
                return first;
            }
        }
        let label = named.or_else(|| {
            let team = bri_minigames::TeamId(b.vehicle.as_ref()?.team?);
            let game = self.minigames.game(self.spawn_brick_game(brick)?).ok()?;
            Some(game.teams.get(team)?.name.clone())
        });
        // The label is shortened, not the kind or the closing bracket.
        let room = MAX_PLAYER_NAME.saturating_sub(kind.name.chars().count() + 3);
        match label {
            Some(label) if room > 0 => {
                let label: String = label.chars().take(room).collect();
                format!("{} ({})", kind.name, label.trim_end())
            }
            _ => kind.name.clone(),
        }
    }
    /// A brick bot takes the name its brick now gives it, without the
    /// announcement a player's own rename makes.
    pub(super) fn rename_brick_bot(&mut self, bot: OwnerId, wanted: String) -> Result<()> {
        let name = self.unique_name_except(&clean_player_name(&wanted), Some(bot));
        if let Some(brain) = self.bots.brains.get_mut(&bot) {
            brain.named = wanted;
        }
        let peer = self.peers.get(&bot).context("No such bot")?;
        if peer.name == name {
            return Ok(());
        }
        let player = peer.combat.player;
        self.admin.rename(bot, name.clone())?;
        let _ = self.minigames.rename(player, name.clone());
        self.peers.get_mut(&bot).context("No such bot")?.name = name;
        Ok(())
    }
    /// The spawn brick's Team choice puts its bot on that team of the game
    /// it plays in: when it joins, after each new life (a reset or a
    /// respawn) and whenever the choice or the game changes. In between,
    /// the game's own commands may move it. A team the game has not got
    /// (yet) is tried again on the next pass.
    pub(super) fn apply_brick_team(
        &mut self,
        bot: OwnerId,
        game: Option<bri_minigames::GameId>,
        team: Option<u32>,
    ) -> Result<()> {
        let wanted = game.zip(team);
        if self
            .bots
            .brains
            .get(&bot)
            .is_none_or(|b| b.brick_team == wanted)
        {
            return Ok(());
        }
        if let Some((game, slot)) = wanted {
            let team = bri_minigames::TeamId(slot);
            let has = self
                .minigames
                .game(game)
                .is_ok_and(|g| g.teams.get(team).is_some());
            if !has {
                return Ok(());
            }
            let player = self.peers.get(&bot).context("No such bot")?.combat.player;
            if self.minigames.team_of(player) != Some(team) {
                let effects = self
                    .minigames
                    .assign_team(player, Some(team))
                    .map_err(|e| anyhow::anyhow!("Team rejected: {e}"))?;
                self.apply_minigame_effects(effects)?;
                // It appears where its team does, as a rules bot put on a
                // team does.
                let effects = self
                    .minigames
                    .execute(bri_minigames::Command::ForceRespawn { target: player })
                    .map_err(|e| anyhow::anyhow!("Respawn rejected: {e}"))?;
                self.apply_minigame_effects(effects)?;
            }
        }
        if let Some(brain) = self.bots.brains.get_mut(&bot) {
            brain.brick_team = wanted;
        }
        Ok(())
    }
    /// Minigame membership follows the spawn brick ([`Self::spawn_brick_game`]),
    /// and team the brick's Team choice.
    pub(super) fn sync_bot_minigames(&mut self) -> Result<()> {
        let bots: Vec<(OwnerId, BrickId)> = self
            .bots
            .brains
            .iter()
            .filter_map(|(o, b)| Some((*o, b.brick?)))
            .collect();
        for (bot, brick) in bots {
            let Some(team) = self
                .simulation
                .state()
                .bricks
                .get(&brick)
                .map(|b| b.vehicle.as_ref().and_then(|v| v.team))
            else {
                continue;
            };
            let wanted = self.spawn_brick_game(brick);
            let Some(peer) = self.peers.get(&bot) else {
                continue;
            };
            let player = peer.combat.player;
            let current = self.minigames.player(player).ok().and_then(|p| p.game);
            if current != wanted {
                let effects = self
                    .minigames
                    .host_place(player, wanted)
                    .map_err(|e| anyhow::anyhow!("Bot minigame: {e}"))?;
                self.apply_minigame_effects(effects)?;
            }
            self.apply_brick_team(bot, wanted, team)?;
            let named = self.bots.brains.get(&bot).and_then(|b| {
                let name =
                    self.brick_bot_name(b.born.as_ref().unwrap_or(&b.kind), brick, Some(bot));
                (name != b.named).then_some(name)
            });
            if let Some(name) = named {
                self.rename_brick_bot(bot, name)?;
            }
        }
        Ok(())
    }
    /// The bot went through an opening since it last looked: its heading,
    /// leash and plan go with it. A path leading through that opening
    /// walks on from where it let out; any other is planned again.
    pub(super) fn bot_crossed(&mut self, bot: OwnerId) {
        let Some(brain) = self.bots.brains.get_mut(&bot) else {
            return;
        };
        let seen = std::mem::replace(&mut brain.crossed, self.crossings.count());
        let Some(carry) = self
            .crossings
            .since(seen)
            .filter(|c| c.object == ObjectRef::Player(bot))
            .map(|c| c.carry)
            .reduce(|before, then| then * before)
        else {
            return;
        };
        brain.yaw = carried_yaw(&carry, brain.yaw);
        brain.leash = carry.transform_point3(brain.leash);
        if let Some((_, at)) = brain.mount_anchor.as_mut() {
            *at = carry.transform_point3(*at);
        }
        brain.progress.reset();
        match brain.plan.iter().take(2).position(|w| w.through.is_some()) {
            Some(at) => {
                brain.plan.drain(..at);
                brain.plan[0].through = None;
            }
            None => {
                brain.plan.clear();
                brain.search = None;
                brain.settled = false;
            }
        }
    }
    /// The bot kinds as rules see them (`bot_kinds()`).
    pub(in crate::session) fn bot_kind_id(&self, bot: OwnerId) -> Option<&str> {
        self.bots
            .brains
            .get(&bot)
            .map(|brain| brain.kind.id.as_str())
    }
    pub(in crate::session) fn bot_kind_views(
        &self,
    ) -> Vec<bri_package_runtime::script::BotKindView> {
        self.bots
            .kinds
            .iter()
            .map(|k| bri_package_runtime::script::BotKindView {
                id: k.id.clone(),
                name: k.name.clone(),
                first_names: k.first_names.clone(),
            })
            .collect()
    }
    /// Bot kinds for the Vehicle Spawn list, as (id, name).
    pub fn bot_choices(&self) -> Vec<(String, String)> {
        self.bots
            .kinds
            .iter()
            .map(|k| (k.id.clone(), k.name.clone()))
            .collect()
    }
}

#[cfg(test)]
mod spawn_tests {
    use super::*;

    #[test]
    fn a_brick_bot_keeps_fighting_after_its_creator_leaves_and_joins_another_game() -> Result<()> {
        use bri_minigames as mg;
        use rapier3d::prelude::*;
        let mut world =
            bri_world::World::new("Persistent battle".into(), "fixture".into(), vec![[1.; 4]]);
        let mut brick = bri_world::Brick::new(
            bri_world::ContentRef::Resolved(crate::testing::VEHICLE_SPAWN.into()),
            [0., 0.1, 0.],
            0,
        );
        brick.vehicle = Some(Box::new(bri_world::VehicleSpawn {
            vehicle: bri_world::ContentRef::Resolved("fixture:bot/melee".into()),
            recolor: false,
            team: None,
        }));
        world.bricks.insert(1, brick);
        world.next_brick_id = 2;
        let floor = ColliderBuilder::cuboid(30., 0.5, 30.).translation(Vector::new(0., -0.5, 0.));
        let mut s = Session::new(crate::simulation::Simulation::new(
            world,
            crate::testing::definitions(),
            vec![floor],
        )?);
        s.set_lan_host(false);
        s.spawn_points = vec![Vec3::new(-12., 0.05, -8.)];
        let principal = bri_admin::Principal([0x5b; 32]);
        let creator = s.join_verified(
            "Creator".into(),
            Vec3::new(-8., 0.05, 0.),
            false,
            Some(principal),
        )?;
        let fighter = s.join("Fighter".into(), Vec3::new(6., 0.05, 0.), false)?;
        let other = s.join("Other owner".into(), Vec3::new(-20., 0.05, 0.), false)?;
        s.simulation.mutate(1, |b| b.owner = creator)?;
        s.set_bot_kinds(crate::bot_kind::BotPack::from_json(br#"{"schema_version":1,"bots":[{"id":"fixture:bot/melee","name":"Melee bot","sight":80,"wander_radius":0,"reaction_seconds":0,"weapons":false,"side":"hostile","melee":{"damage":25,"reach":2.5,"seconds":0.3},"behaviours":{"fly":0},"perception":{"strength":0},"surprise":{"strength":0}}]}"#)?.bots)?;
        s.minigame_request(
            creator,
            crate::session::MiniGameRequest::Create {
                color: 0,
                settings: Default::default(),
            },
        )?;
        let encounter = s.game_of(creator).unwrap();
        s.minigame_request(
            fighter,
            crate::session::MiniGameRequest::Join { game: encounter.0 },
        )?;
        s.peers.get_mut(&fighter).unwrap().player.place(
            &mut s.simulation.physics,
            Vec3::new(6., 0.05, 0.),
            0.,
            Vec3::ZERO,
        );
        s.reconcile_bot_brick(1, Some("fixture:bot/melee"))?;
        s.sync_bot_minigames()?;
        let bot = s.bots.by_brick[&1];
        s.control_brick_bot(1, &bri_events::bots::BotOp::Name("Guard".into()))?;
        assert_eq!(s.peers[&bot].name, "Guard");
        s.sync_bot_minigames()?;
        assert_eq!(
            s.peers[&bot].name, "Guard",
            "reconciliation keeps an authored name"
        );
        s.control_brick_bot(1, &bri_events::bots::BotOp::Name(String::new()))?;
        assert_ne!(
            s.peers[&bot].name, "Guard",
            "empty name restores the derived name"
        );
        let life = s
            .minigames
            .player(s.peers[&bot].combat.player)
            .unwrap()
            .life;
        s.minigame_request(creator, crate::session::MiniGameRequest::Leave)?;
        s.sync_bot_minigames()?;
        assert_eq!(
            s.game_of(bot),
            Some(encounter),
            "the creator's departure must not eject the bot from combat"
        );
        assert_eq!(
            s.minigames
                .player(s.peers[&bot].combat.player)
                .unwrap()
                .life,
            life,
            "leaving must not respawn the running bot"
        );
        s.minigame_request(
            other,
            crate::session::MiniGameRequest::Create {
                color: 1,
                settings: Default::default(),
            },
        )?;
        let other_game = s.game_of(other).unwrap();
        s.minigame_request(
            creator,
            crate::session::MiniGameRequest::Join { game: other_game.0 },
        )?;
        s.sync_bot_minigames()?;
        assert_eq!(s.game_of(bot), Some(encounter));
        assert!(!s.damage_policy().player(creator, fighter, false));
        assert!(!s.damage_policy().player(bot, creator, false));
        let initial = s.peers[&fighter].combat.health;
        for _ in 0..1440 {
            s.step()?;
            if s.peers[&fighter].combat.health < initial {
                break;
            }
        }
        assert!(
            s.peers[&fighter].combat.health < initial,
            "ordinary bot controls and melee must damage the remaining participant"
        );
        s.disconnect(creator)?;
        s.sync_bot_minigames()?;
        assert_eq!(s.game_of(bot), Some(encounter));
        assert!(s.minigames.game(mg::GameId(encounter.0)).is_ok());

        // A restart restores the saved brick and the creator's held game when
        // their verified identity returns; bot membership follows that game.
        let (world, saved_game) = s.recovery_snapshot();
        let floor = ColliderBuilder::cuboid(30., 0.5, 30.).translation(Vector::new(0., -0.5, 0.));
        let mut restarted = Session::new(crate::simulation::Simulation::new(
            world,
            crate::testing::definitions(),
            vec![floor],
        )?);
        restarted.set_lan_host(false);
        restarted.set_vehicle_pack(
            bri_vehicles::Pack {
                schema_version: bri_vehicles::schema::SCHEMA_VERSION,
                definitions: vec![],
                assets: vec![],
                evidence: vec![],
                unresolved: vec![],
                animation_aliases: Default::default(),
            },
            s.bots.kinds.clone(),
        )?;
        restarted.hold_minigame(saved_game.expect("creator encounter is saved"));
        let returned = restarted.join_verified(
            "Creator returned".into(),
            Vec3::new(-8., 0.05, 0.),
            false,
            Some(principal),
        )?;
        assert_eq!(returned, creator);
        let restored_game = restarted
            .game_of(returned)
            .expect("held encounter restored");
        assert_eq!(restarted.simulation.state().bricks[&1].owner, creator);
        restarted.step()?;
        let restored_bot = *restarted
            .bots
            .by_brick
            .get(&1)
            .expect("saved source spawns automatically");
        // Membership synchronization runs on its ordinary 30-tick cadence.
        for _ in 0..30 {
            if restarted.game_of(restored_bot) == Some(restored_game) {
                break;
            }
            restarted.step()?;
        }
        assert_eq!(restarted.game_of(restored_bot), Some(restored_game));
        restarted.peers.get_mut(&returned).unwrap().player.place(
            &mut restarted.simulation.physics,
            Vec3::new(-12., 0.05, -8.),
            0.,
            Vec3::ZERO,
        );
        let target = restarted.join("New fighter".into(), Vec3::new(6., 0.05, 0.), false)?;
        restarted.minigame_request(
            target,
            crate::session::MiniGameRequest::Join {
                game: restored_game.0,
            },
        )?;
        restarted.peers.get_mut(&target).unwrap().player.place(
            &mut restarted.simulation.physics,
            Vec3::new(6., 0.05, 0.),
            0.,
            Vec3::ZERO,
        );
        let health = restarted.peers[&target].combat.health;
        for _ in 0..1440 {
            restarted.step()?;
            if restarted.peers[&target].combat.health < health {
                break;
            }
        }
        assert!(
            restarted.peers[&target].combat.health < health,
            "restored bot must resume combat"
        );
        Ok(())
    }

    #[test]
    fn brick_bots_inherit_the_spawn_frame_on_creation_and_reset() -> Result<()> {
        for turns in 0..4 {
            let mut world = bri_world::World::new("Facing".into(), "test".into(), vec![[1.0; 4]]);
            let mut brick = bri_world::Brick::new(
                bri_world::ContentRef::Resolved(crate::testing::VEHICLE_SPAWN.into()),
                [4.0, 0.1, 8.0],
                0,
            );
            brick.quarter_turns = turns;
            let forward = brick.transform().transform_vector3(Vec3::NEG_Z);
            world.bricks.insert(1, brick);
            world.next_brick_id = 2;
            let mut session = Session::new(crate::simulation::Simulation::new(
                world,
                crate::testing::definitions(),
                vec![],
            )?);
            session.set_bot_kinds(
                crate::bot_kind::BotPack::from_json(include_bytes!(
                    "../../../../../packages/blockhead_bot/assets/bots.json"
                ))?
                .bots,
            )?;
            session.reconcile_bot_brick(1, Some("bot.blockhead"))?;
            let bot = session.bots.by_brick[&1];
            let check = |session: &Session| {
                let yaw = session.peers[&bot].player.state().yaw;
                let facing = Vec3::new(yaw.sin(), 0.0, -yaw.cos());
                assert!(
                    facing.dot(forward) > 0.999,
                    "turns {turns}: yaw {yaw}, forward {forward}"
                );
                assert!(
                    wrap(session.bots.brains[&bot].yaw - yaw).abs() < 0.001,
                    "the brain must keep the body's new facing on its next control"
                );
            };
            check(&session);
            // Reset an already-running brain, whose sequence must not restart.
            session.bots.brains.get_mut(&bot).unwrap().sequence = 40;
            session.bots.brains.get_mut(&bot).unwrap().yaw = 0.37;
            let player = session.peers[&bot].combat.player;
            let effects = session
                .minigames
                .execute(bri_minigames::Command::ForceRespawn { target: player })
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            session.apply_minigame_effects(effects)?;
            check(&session);
            session.bots.brains.get_mut(&bot).unwrap().yaw = -0.37;
            session.respawn_brick_bot(1, "bot.blockhead")?;
            check(&session);
            assert_eq!(session.bots.brains[&bot].sequence, 40);
        }
        Ok(())
    }
}
