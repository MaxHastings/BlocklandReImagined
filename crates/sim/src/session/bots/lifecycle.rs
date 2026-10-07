//! A bot's life: kinds installed, brick and rules bots made, embodied,
//! named, put on teams and removed, and what the rest of the session asks
//! about them.
use super::*;

impl Session {
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
                brain.kind = kind.clone();
            }
        }
        self.vehicles.scanned = false;
        Ok(())
    }
    /// Whether a spawn brick choice names a bot kind this server has.
    pub fn is_bot_kind(&self, id: &str) -> bool {
        self.bots.kind(id).is_some()
    }
    pub(in crate::session) fn bot_home(&self, owner: OwnerId) -> Option<Vec3> {
        self.bots.home(owner)
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
        let (home, builder) = (Vec3::from(brick.position) + Vec3::Y * 0.3, brick.owner);
        let name = self.brick_bot_name(&kind, brick_id, None);
        // A refused bot is never silent: the brick's builder is told why,
        // as for a vehicle the server has no room for.
        if self.bots.brains.len() >= MAX_BOTS {
            self.notify(
                builder,
                Notice::Center {
                    text: format!("\u{E000}Server is limited to {MAX_BOTS} bots"),
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
            brain.named = name;
            self.bots.brains.insert(bot, brain);
            self.weapons.set_bot(bri_weapons::ActorId(bot), true)?;
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
        peer.package_archetype = body;
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
    pub(super) fn drop_bot(&mut self, bot: OwnerId) -> Result<()> {
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
        ensure!(
            self.bots.brains.len() < MAX_BOTS,
            "Server is limited to {MAX_BOTS} bots"
        );
        let kind = self
            .bots
            .kind(kind)
            .with_context(|| format!("No bot kind `{kind}`: its Add-On is not enabled"))?
            .clone();
        let game = bri_minigames::GameId(game);
        self.minigames
            .game(game)
            .map_err(|_| anyhow::anyhow!("No mini-game {}", game.0))?;
        let team = team
            .map(|t| u32::try_from(t).map(bri_minigames::TeamId))
            .transpose()
            .ok()
            .context("No such team")?;
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
    /// The bot, if `package`'s rules added it.
    pub(super) fn own_bot(&self, package: &str, bot: OwnerId) -> Result<()> {
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
        self.own_bot(package, bot)?;
        ensure!(self.is_alive(bot), "Only a living bot holds things");
        self.equip_tool(bot, slot.map(usize::from))
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
    pub(in crate::session) fn rest_rules_bot(
        &mut self,
        package: &str,
        bot: OwnerId,
        rest: bool,
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
    /// The game a spawn brick's bot plays in: its builder's.
    pub(super) fn spawn_brick_game(&self, brick: BrickId) -> Option<bri_minigames::GameId> {
        let owner = self.simulation.state().bricks.get(&brick)?.owner;
        let player = self.peers.get(&owner)?.combat.player;
        self.minigames.player(player).ok()?.game
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
    /// Minigame membership follows the spawn brick owner, and team the
    /// brick's Team choice.
    pub(super) fn sync_bot_minigames(&mut self) -> Result<()> {
        let bots: Vec<(OwnerId, BrickId)> = self
            .bots
            .brains
            .iter()
            .filter_map(|(o, b)| Some((*o, b.brick?)))
            .collect();
        for (bot, brick) in bots {
            let Some((owner, team)) = self
                .simulation
                .state()
                .bricks
                .get(&brick)
                .map(|b| (b.owner, b.vehicle.as_ref().and_then(|v| v.team)))
            else {
                continue;
            };
            let wanted = self
                .peers
                .get(&owner)
                .and_then(|p| self.minigames.player(p.combat.player).ok())
                .and_then(|p| p.game);
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
