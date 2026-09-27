//! Player health, damage, death, respawn and minigame membership.
//!
//! The deterministic `bri-minigames` world owns membership, lives and scores;
//! this adapter owns health, bodies, loadouts, spawn selection and messages.
//! Behavior follows the recovered vanilla scripts: `Armor::Damage`,
//! `Armor::onImpact`, `GameConnection::onDeath`, `createPlayer` and the
//! `MiniGameSO` membership functions.
use super::*;
use bri_minigames::{
    self as mg, DamageSource, Decision, EnvironmentDamage, GameId, LifeState, MinigamesWorld,
};
use bri_weapons::{ActorId, CORE_TOOLS};

/// `PlayerStandardArmor.maxDamage`.
pub const MAX_HEALTH: f32 = 100.0;
/// `$Game::PlayerInvulnerabilityTime` (2.5 s) at 120 Hz.
const INVULNERABLE_TICKS: u64 = 300;
/// `$CorpseTimeoutValue` (5 s): `Player::RemoveBody` then deletes the corpse.
const CORPSE_TICKS: u64 = 600;
/// `Armor::damage` sums hits less than 300 ms apart into one pain level.
const PAIN_TICKS: u64 = 36;
/// `minImpactSpeed` and `speedDamageScale`.
const MIN_IMPACT_SPEED: f32 = 30.0;
const SPEED_DAMAGE_SCALE: f32 = 3.8;
/// `mass` of the standard player: impulses divide by it.
const PLAYER_MASS: f32 = 90.0;
/// Minimum respawn delay outside minigames (`$Game::MinRespawnTime`).
const MIN_RESPAWN_TICKS: u64 = 120;
const SPAWN_BRICK: &str = "v20/brick/brickspawnpointdata";
const SPAWN_PROJECTILE: &str = "v20.projectile.spawnprojectile";
const DEATH_PROJECTILE: &str = "v20.projectile.deathprojectile";
const MAX_NOTICES: usize = 256;

/// Per-player authoritative combat state.
#[derive(Debug, Clone)]
pub(super) struct Combat {
    pub player: mg::PlayerId,
    pub health: f32,
    pub alive: bool,
    pub died_tick: u64,
    pub respawn_tick: u64,
    pub spawn_tick: u64,
    pub shot_once: bool,
    pub light: bool,
    /// Last direct damage type and tick (death messages prefer it).
    pub last_direct: Option<(String, u64)>,
    pub corpse_cleared: bool,
    pub pain_level: f32,
    pub pain_tick: u64,
}

/// Replicated per-player status. Health drives the damage flash; the rest
/// drives the death camera, respawn prompt, player list and lights.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vitals {
    pub health: f32,
    pub alive: bool,
    /// Earliest tick at which "click to respawn" is accepted while dead.
    pub respawn_tick: u64,
    pub score: i64,
    pub minigame: Option<u64>,
    pub invite: Option<u64>,
    pub light: bool,
    /// Vehicle id and seat while riding.
    pub mounted: Option<(u64, u8)>,
    /// What this player's moves steer.
    pub control: super::ControlObject,
}

/// Replicated minigame listing for the Mini-Games dialog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MiniGameView {
    pub id: u64,
    pub owner: OwnerId,
    pub color: u8,
    pub settings: mg::Settings,
    pub members: Vec<OwnerId>,
}

/// A message for one player only (minigame chat, prints, invitations).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Notice {
    /// Chat line; may contain color escapes and `<bitmap:...>` death icons.
    Chat(String),
    Center {
        text: String,
        seconds: f32,
    },
    Bottom {
        text: String,
        seconds: f32,
    },
    /// Invitation from a minigame owner, answered with Accept/Reject.
    Invite {
        game: u64,
        owner_name: String,
        title: String,
    },
    /// `openWrenchDlg` / `openPrintSelectorDlg` after a wrench or printer hit.
    Inspected {
        brick_id: bri_world::BrickId,
        brick: Box<bri_world::Brick>,
        mode: super::InspectMode,
    },
    /// Movement the player may use now; the client predicts with the same mask.
    Abilities(super::Abilities),
}

/// Minigame requests. The actor is always the authenticated connection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum MiniGameRequest {
    Create { color: u8, settings: mg::Settings },
    Configure { settings: mg::Settings },
    Join { game: u64 },
    Leave,
    Invite { target: OwnerId },
    Accept { game: u64 },
    Reject { game: u64, ignore_owner: bool },
    Kick { target: OwnerId },
    Reset,
    RespawnAll,
    End,
}

/// Damage classes from `DamageTypes.cs`; weapon types carry their own name.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum DamageKind {
    Weapon {
        name: String,
        direct: bool,
    },
    Fall,
    Impact,
    Suicide,
    /// Wrench event output (`kill`, negative `addHealth`).
    Event,
}
impl DamageKind {
    fn direct(&self) -> bool {
        matches!(self, Self::Weapon { direct: true, .. })
    }
    /// The `AddDamageType` name whose kill message this death shows.
    fn type_name(&self) -> &str {
        match self {
            Self::Weapon { name, .. } => name,
            Self::Fall => "Fall",
            Self::Impact => "Impact",
            Self::Suicide | Self::Event => "Suicide",
        }
    }
}

pub(super) fn catalog(pack: &bri_weapons::Pack) -> mg::Catalog {
    let mut items: BTreeMap<String, Option<String>> = CORE_TOOLS
        .iter()
        .map(|id| ((*id).to_string(), None))
        .collect();
    for (id, item) in &pack.items {
        items.insert(id.clone(), item.sport.then(|| item.image.clone()));
    }
    mg::Catalog {
        schema_version: mg::SCHEMA_VERSION,
        player_types: [mg::STANDARD_PLAYER.to_string()].into(),
        items,
    }
}

pub(super) fn new_world(catalog: mg::Catalog) -> MinigamesWorld {
    MinigamesWorld::new(catalog, mg::PolicyMode::Internet, true).unwrap_or_else(|_| {
        MinigamesWorld::new(
            mg::Catalog::minimal_vanilla(),
            mg::PolicyMode::Internet,
            true,
        )
        .expect("minimal vanilla catalog is valid")
    })
}

fn color_code(n: u32) -> char {
    char::from_u32(0xE000 + n).unwrap_or(' ')
}

impl Session {
    /// Map spawn points (feet positions) for respawns outside spawn bricks.
    pub fn set_spawn_points(&mut self, points: Vec<Vec3>) -> Result<()> {
        ensure!(
            !points.is_empty() && points.len() <= 256 && points.iter().all(|p| p.is_finite()),
            "Invalid spawn points"
        );
        self.spawn_points = points;
        Ok(())
    }
    pub(super) fn combat_connect(
        &mut self,
        owner: OwnerId,
        name: &str,
        admin: bool,
    ) -> Result<Combat> {
        let player = self
            .minigames
            .connect(mg::AccountId(owner), name.to_string(), admin)
            .map_err(|e| anyhow::anyhow!("Minigame registration failed: {e}"))?;
        self.minigames
            .set_ready(player, true)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let tick = self.simulation.state().tick;
        Ok(Combat {
            player,
            health: MAX_HEALTH,
            alive: true,
            died_tick: 0,
            respawn_tick: 0,
            spawn_tick: tick,
            shot_once: false,
            light: false,
            last_direct: None,
            corpse_cleared: false,
            pain_level: 0.0,
            pain_tick: 0,
        })
    }
    pub(super) fn combat_disconnect(&mut self, player: mg::PlayerId) {
        if let Ok(effects) = self.minigames.disconnect(player) {
            let _ = self.apply_minigame_effects(effects);
        }
    }
    fn owner_of(&self, player: mg::PlayerId) -> Option<OwnerId> {
        self.peers
            .iter()
            .find(|(_, p)| p.combat.player == player)
            .map(|(id, _)| *id)
    }
    pub(super) fn game_of(&self, owner: OwnerId) -> Option<GameId> {
        let player = self.peers.get(&owner)?.combat.player;
        self.minigames.player(player).ok()?.game
    }

    pub fn vitals(&self) -> BTreeMap<OwnerId, Vitals> {
        self.peers
            .iter()
            .map(|(owner, peer)| {
                let state = self.minigames.player(peer.combat.player).ok();
                (
                    *owner,
                    Vitals {
                        health: peer.combat.health,
                        alive: peer.combat.alive,
                        respawn_tick: peer.combat.respawn_tick,
                        score: state.map_or(0, |s| s.score),
                        minigame: state.and_then(|s| s.game).map(|g| g.0),
                        invite: state.and_then(|s| s.invite).map(|g| g.0),
                        light: peer.combat.light,
                        mounted: self.mounted(*owner),
                        control: peer.control,
                    },
                )
            })
            .collect()
    }
    pub fn minigame_views(&self) -> Vec<MiniGameView> {
        self.minigames
            .games()
            .filter_map(|game| {
                Some(MiniGameView {
                    id: game.id.0,
                    owner: self.owner_of(game.owner)?,
                    color: game.color,
                    settings: game.settings.clone(),
                    members: game
                        .members
                        .iter()
                        .filter_map(|m| self.owner_of(*m))
                        .collect(),
                })
            })
            .collect()
    }
    pub fn take_private_notices(&mut self) -> Vec<(OwnerId, Notice)> {
        self.private_notices.drain(..).collect()
    }
    pub(super) fn notify(&mut self, owner: OwnerId, notice: Notice) {
        if self.private_notices.len() == MAX_NOTICES {
            self.private_notices.pop_front();
        }
        self.private_notices.push_back((owner, notice));
    }
    /// Server-authored chat line (owner 0) visible to everyone.
    pub(super) fn system_chat(&mut self, text: String) {
        let tick = self.simulation.state().tick;
        let Some(next) = self.next_chat.checked_add(1) else {
            return;
        };
        self.chat.push_back(ChatLine {
            id: self.next_chat,
            owner: 0,
            name: String::new(),
            text,
            tick,
        });
        self.next_chat = next;
        if self.chat.len() > 100 {
            self.chat.pop_front();
        }
    }
    fn chat_game(&mut self, game: Option<GameId>, except: Option<OwnerId>, text: String) {
        match game {
            None => self.system_chat(text),
            Some(game) => {
                let members: Vec<_> = self
                    .minigames
                    .game(game)
                    .map(|g| g.members.iter().copied().collect())
                    .unwrap_or_default();
                for member in members {
                    if let Some(owner) = self.owner_of(member)
                        && Some(owner) != except
                    {
                        self.notify(owner, Notice::Chat(text.clone()));
                    }
                }
            }
        }
    }

    /// `miniGameCanDamage` for player targets: only members of the same
    /// minigame with weapon damage (and self damage for oneself) can hurt a
    /// player. Players outside minigames are never damaged by weapons.
    pub(super) fn can_damage_player(&self, source: OwnerId, target: OwnerId, radius: bool) -> bool {
        let (Some(s), Some(t)) = (self.peers.get(&source), self.peers.get(&target)) else {
            return false;
        };
        if !t.combat.alive {
            return false;
        }
        let Ok(source) = self.minigames.projectile_source(s.combat.player) else {
            return false;
        };
        let Ok(target) = self.minigames.target_for_player(t.combat.player) else {
            return false;
        };
        let decision = if radius {
            self.minigames.can_radius_damage(source, target)
        } else {
            self.minigames.can_damage(source, target)
        };
        decision == Decision::Allow
    }

    /// `Armor::Damage`: invulnerability, crouch scaling, health and death.
    pub(super) fn damage_player(
        &mut self,
        target: OwnerId,
        amount: f32,
        kind: DamageKind,
        source: Option<OwnerId>,
    ) -> Result<()> {
        let tick = self.simulation.state().tick;
        let Some(peer) = self.peers.get_mut(&target) else {
            return Ok(());
        };
        if !peer.combat.alive || !amount.is_finite() || amount <= 0.0 {
            return Ok(());
        }
        if tick.saturating_sub(peer.combat.spawn_tick) < INVULNERABLE_TICKS
            && !peer.combat.shot_once
            && !matches!(kind, DamageKind::Suicide | DamageKind::Event)
        {
            return Ok(());
        }
        let mut amount = amount;
        if peer.player.state().crouched {
            amount *= if kind.direct() { 2.1 } else { 0.75 };
        }
        if let DamageKind::Weapon { name, direct: true } = &kind {
            peer.combat.last_direct = Some((name.clone(), tick));
        }
        peer.combat.health = (peer.combat.health - amount).max(0.0);
        peer.combat.pain_level = if tick.saturating_sub(peer.combat.pain_tick) > PAIN_TICKS {
            amount
        } else {
            peer.combat.pain_level + amount
        };
        peer.combat.pain_tick = tick;
        let alive = peer.combat.health > 0.0;
        let level = peer.combat.pain_level;
        let feet = peer.player.state().feet;
        self.cues.emit(
            tick,
            crate::presentation::CueKind::Pain {
                actor: target,
                level,
                cry: alive && amount > 10.0,
            },
            feet,
        );
        if alive {
            return Ok(());
        }
        self.kill(target, source, kind)
    }

    pub(super) fn kill(
        &mut self,
        victim: OwnerId,
        killer: Option<OwnerId>,
        kind: DamageKind,
    ) -> Result<()> {
        let tick = self.simulation.state().tick;
        let Some(peer) = self.peers.get(&victim) else {
            return Ok(());
        };
        let player = peer.combat.player;
        let LifeState::Alive { life } = self
            .minigames
            .player(player)
            .map_err(|e| anyhow::anyhow!("{e}"))?
            .life
        else {
            return Ok(());
        };
        // A killer from another minigame (or none) cannot be credited.
        let killer = killer.filter(|k| *k == victim || self.game_of(*k) == self.game_of(victim));
        let killer_player = killer
            .and_then(|k| self.peers.get(&k))
            .map(|p| p.combat.player);
        let effects = self
            .minigames
            .died(player, life, killer_player)
            .map_err(|e| anyhow::anyhow!("Death rejected: {e}"))?;
        // Radius deaths within 0.1 s of a direct hit report the direct type.
        let kind = match (&kind, &peer.combat.last_direct) {
            (DamageKind::Weapon { direct: false, .. }, Some((name, at)))
                if tick.saturating_sub(*at) < 12 =>
            {
                DamageKind::Weapon {
                    name: name.clone(),
                    direct: true,
                }
            }
            _ => kind,
        };
        self.eject(victim);
        {
            let peer = self.peers.get_mut(&victim).unwrap();
            peer.combat.alive = false;
            peer.combat.health = 0.0;
            peer.combat.died_tick = tick;
            peer.combat.corpse_cleared = false;
            peer.inputs.clear();
            peer.control = super::ControlObject::Corpse;
        }
        self.weapons.trigger(ActorId(victim), false)?;
        self.weapon_triggers.remove(&victim);
        let _ = self.weapons.equip(ActorId(victim), None);
        let feet = self.peers[&victim].player.state().feet;
        self.cues.emit(
            tick,
            crate::presentation::CueKind::Death { actor: victim },
            feet,
        );
        self.apply_minigame_effects(effects)?;
        // `GameConnection::onDeath`: the type's suicide or murder message.
        let victim_name = self.peers[&victim].name.clone();
        let killer_name = killer
            .filter(|k| *k != victim)
            .map(|k| self.peers[&k].name.clone());
        let text = match self.weapons.pack.damage_type(kind.type_name()) {
            Some(t) => t.message(&victim_name, killer_name.as_deref()),
            None => killer_name.map_or_else(
                || victim_name.clone(),
                |k| format!("{k} killed {victim_name}"),
            ),
        };
        let game = self.game_of(victim);
        self.chat_game(game, None, text);
        Ok(())
    }

    /// Minigame team chat (`serverCmdTeamMessageSent`).
    pub(super) fn team_chat(&mut self, owner: OwnerId, name: &str, text: &str) -> Result<()> {
        let game = self.game_of(owner).context("You are not in a mini-game")?;
        // Private-use escapes are color codes; strip any the sender typed.
        let clean: String = text
            .chars()
            .filter(|c| !(0xE000..0xE010).contains(&(*c as u32)))
            .map(|c| match c {
                '<' => '\u{2039}',
                '>' => '\u{203A}',
                c => c,
            })
            .collect();
        let name: String = name.chars().filter(|c| !c.is_control()).collect();
        self.chat_game(
            Some(game),
            None,
            format!("{}{name}{}: {clean}", color_code(7), color_code(4)),
        );
        Ok(())
    }
    /// Self-inflicted death (`serverCmdSuicide`).
    pub(super) fn suicide(&mut self, owner: OwnerId) -> Result<()> {
        let peer = self.peers.get(&owner).context("Unknown connection")?;
        ensure!(peer.combat.alive, "You are already dead");
        self.kill(owner, Some(owner), DamageKind::Suicide)
    }

    /// Click-to-respawn after death.
    pub(super) fn request_respawn(&mut self, owner: OwnerId) -> Result<()> {
        let tick = self.simulation.state().tick;
        let peer = self.peers.get(&owner).context("Unknown connection")?;
        ensure!(!peer.combat.alive, "You are alive");
        ensure!(tick >= peer.combat.respawn_tick, "Not ready to respawn yet");
        let effects = self
            .minigames
            .execute(mg::Command::Respawn {
                actor: peer.combat.player,
            })
            .map_err(|e| anyhow::anyhow!("Respawn rejected: {e}"))?;
        self.apply_minigame_effects(effects)
    }

    pub(super) fn toggle_light(&mut self, owner: OwnerId) -> Result<()> {
        let peer = self.peers.get_mut(&owner).context("Unknown connection")?;
        ensure!(peer.combat.alive, "Dead players cannot use lights");
        peer.combat.light = !peer.combat.light;
        Ok(())
    }

    pub(super) fn minigame_request(
        &mut self,
        owner: OwnerId,
        request: MiniGameRequest,
    ) -> Result<()> {
        let actor = self
            .peers
            .get(&owner)
            .context("Unknown connection")?
            .combat
            .player;
        let lookup = |session: &Self, target: OwnerId| -> Result<mg::PlayerId> {
            Ok(session
                .peers
                .get(&target)
                .context("Unknown player")?
                .combat
                .player)
        };
        let owned = |session: &Self| -> Result<GameId> {
            let game = session
                .minigames
                .player(actor)
                .ok()
                .and_then(|p| p.game)
                .context("You are not in a mini-game")?;
            ensure!(
                session.minigames.game(game).is_ok_and(|g| g.owner == actor),
                "Only the mini-game owner can do that"
            );
            Ok(game)
        };
        let command = match request {
            MiniGameRequest::Create { color, settings } => mg::Command::Create {
                actor,
                color,
                settings,
            },
            MiniGameRequest::Configure { settings } => mg::Command::Configure { actor, settings },
            MiniGameRequest::Join { game } => mg::Command::Join {
                actor,
                game: GameId(game),
            },
            MiniGameRequest::Leave => mg::Command::Leave { actor },
            MiniGameRequest::Invite { target } => mg::Command::Invite {
                actor,
                target: lookup(self, target)?,
            },
            MiniGameRequest::Accept { game } => mg::Command::Accept {
                actor,
                game: GameId(game),
            },
            MiniGameRequest::Reject { game, ignore_owner } => mg::Command::Reject {
                actor,
                game: GameId(game),
                ignore_owner,
            },
            MiniGameRequest::Kick { target } => mg::Command::Kick {
                actor,
                target: lookup(self, target)?,
            },
            MiniGameRequest::Reset => mg::Command::Reset {
                game: owned(self)?,
                authority: mg::EventAuthority::Owner(actor),
            },
            MiniGameRequest::RespawnAll => mg::Command::RespawnAll {
                game: owned(self)?,
                authority: mg::EventAuthority::Owner(actor),
            },
            MiniGameRequest::End => mg::Command::End { actor },
        };
        let reset = matches!(command, mg::Command::Reset { .. });
        let created = matches!(command, mg::Command::Create { .. });
        // MiniGameSO::endGame tells every member; they are gone afterwards.
        let ending: Vec<OwnerId> = if matches!(command, mg::Command::End { .. }) {
            self.game_of(owner)
                .and_then(|game| self.minigames.game(game).ok())
                .map(|g| g.members.iter().filter_map(|&m| self.owner_of(m)).collect())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let effects = self.minigames.execute(command).map_err(|e| {
            anyhow::anyhow!(match e {
                mg::Error::Cooldown => "Please wait before doing that again".to_string(),
                mg::Error::InviteOnly => "That mini-game is invite only".to_string(),
                mg::Error::ColorUnavailable => "That color is already taken".to_string(),
                mg::Error::AlreadyOwner => "You already own a mini-game".to_string(),
                mg::Error::NotOwner => "Only the mini-game owner can do that".to_string(),
                mg::Error::AlreadyMember => "Already in that mini-game".to_string(),
                mg::Error::InvalidSettings => "Invalid mini-game settings".to_string(),
                mg::Error::UnknownContent => "Unknown item or player type".to_string(),
                other => format!("Mini-game request rejected: {other}"),
            })
        })?;
        if created {
            self.notify(
                owner,
                Notice::Chat(format!("{}Mini-game created.", color_code(5))),
            );
        }
        for member in ending {
            self.notify(
                member,
                Notice::Chat(format!("{}The mini-game ended.", color_code(5))),
            );
        }
        if reset {
            let name = self.peers[&owner].name.clone();
            let game = self.game_of(owner);
            self.chat_game(
                game,
                None,
                format!(
                    "{}{name}{} reset the mini-game",
                    color_code(3),
                    color_code(5)
                ),
            );
        }
        self.apply_minigame_effects(effects)
    }

    /// Apply rule-engine side effects in order.
    pub(super) fn apply_minigame_effects(&mut self, effects: Vec<mg::Effect>) -> Result<()> {
        let tick = self.simulation.state().tick;
        for effect in effects {
            match effect {
                mg::Effect::Spawn {
                    player, equipment, ..
                } => {
                    if let Some(owner) = self.owner_of(player) {
                        self.respawn(owner, equipment)?;
                    }
                }
                mg::Effect::RestoreOwner { player, .. } => {
                    if let Some(owner) = self.owner_of(player) {
                        let peer = self.peers.get_mut(&owner).unwrap();
                        peer.combat.health = MAX_HEALTH;
                        self.give_loadout(owner, None)?;
                    }
                }
                mg::Effect::ApplyEquipment {
                    player,
                    equipment,
                    changed_slots,
                    ..
                } => {
                    if changed_slots.iter().any(|c| *c)
                        && let Some(owner) = self.owner_of(player)
                    {
                        self.give_loadout(owner, Some(&equipment))?;
                    }
                }
                mg::Effect::Death {
                    player, ready_at, ..
                } => {
                    if let Some(owner) = self.owner_of(player) {
                        let peer = self.peers.get_mut(&owner).unwrap();
                        // Rule-engine ticks advance with ours; convert to world ticks.
                        let delay = ready_at.saturating_sub(self.minigames.tick());
                        peer.combat.respawn_tick = tick + delay.max(MIN_RESPAWN_TICKS);
                    }
                }
                mg::Effect::RespawnDeadline {
                    player, ready_at, ..
                } => {
                    if let Some(owner) = self.owner_of(player) {
                        let delay = ready_at.saturating_sub(self.minigames.tick());
                        self.peers.get_mut(&owner).unwrap().combat.respawn_tick = tick + delay;
                    }
                }
                mg::Effect::Membership { player, game, .. } => {
                    let Some(owner) = self.owner_of(player) else {
                        continue;
                    };
                    let name = self.peers[&owner].name.clone();
                    let previous = self.last_membership.insert(owner, game);
                    if let Some(Some(old)) = previous
                        && Some(old) != game
                        && self.minigames.game(old).is_ok()
                    {
                        self.chat_game(
                            Some(old),
                            Some(owner),
                            format!("{}{name} left the mini-game.", color_code(1)),
                        );
                    }
                    if let Some(new) = game {
                        self.chat_game(
                            Some(new),
                            Some(owner),
                            format!("{}{name} joined the mini-game.", color_code(1)),
                        );
                    }
                }
                mg::Effect::Invitation { player, game } => {
                    if let (Some(owner), Some(game)) = (self.owner_of(player), game)
                        && let Ok(g) = self.minigames.game(game)
                    {
                        let owner_name = self
                            .owner_of(g.owner)
                            .map(|o| self.peers[&o].name.clone())
                            .unwrap_or_default();
                        let title = g.settings.title.clone();
                        self.notify(
                            owner,
                            Notice::Invite {
                                game: game.0,
                                owner_name,
                                title,
                            },
                        );
                    }
                }
                mg::Effect::Ended { game } => {
                    let members: Vec<_> = self
                        .last_membership
                        .iter()
                        .filter(|(_, g)| **g == Some(game))
                        .map(|(o, _)| *o)
                        .collect();
                    for owner in members {
                        self.notify(
                            owner,
                            Notice::Chat(format!("{}The mini-game ended.", color_code(5))),
                        );
                        self.last_membership.insert(owner, None);
                    }
                }
                mg::Effect::Message {
                    recipients,
                    kind,
                    text,
                } => {
                    for player in recipients {
                        if let Some(owner) = self.owner_of(player) {
                            let notice = match kind {
                                mg::MessageKind::Chat => Notice::Chat(text.clone()),
                                mg::MessageKind::Center { seconds } => Notice::Center {
                                    text: text.clone(),
                                    seconds: f32::from(seconds),
                                },
                                mg::MessageKind::Bottom { seconds } => Notice::Bottom {
                                    text: text.clone(),
                                    seconds: f32::from(seconds),
                                },
                            };
                            self.notify(owner, notice);
                        }
                    }
                }
                mg::Effect::Created { .. }
                | mg::Effect::Configured { .. }
                | mg::Effect::Score { .. }
                | mg::Effect::Reset { .. }
                | mg::Effect::Cleanup { .. }
                | mg::Effect::EjectVehicles { .. }
                | mg::Effect::ResetBricks { .. }
                | mg::Effect::StartBall { .. } => {}
            }
        }
        Ok(())
    }

    /// Install the minigame loadout, or the default building tools outside.
    fn give_loadout(&mut self, owner: OwnerId, equipment: Option<&mg::Equipment>) -> Result<()> {
        let slots: Vec<Option<String>> = match equipment {
            Some(e) => e.tools.to_vec(),
            None => self.spawn_loadout.slots.clone(),
        };
        let slots: Vec<_> = slots
            .into_iter()
            .map(|s| s.filter(|id| self.weapons.contains_item(id)))
            .collect();
        self.weapons.set_inventory(ActorId(owner), &slots)?;
        self.weapon_triggers.remove(&owner);
        if let Some(peer) = self.peers.get_mut(&owner) {
            peer.inspection = None;
        }
        Ok(())
    }

    /// `GameConnection::spawnPlayer`: pick a spawn, heal, equip and relocate.
    fn respawn(&mut self, owner: OwnerId, equipment: Option<mg::Equipment>) -> Result<()> {
        let tick = self.simulation.state().tick;
        let (feet, yaw) = self.pick_spawn(owner);
        {
            let peer = self.peers.get_mut(&owner).context("Unknown connection")?;
            // The corpse is this same player: an early respawn removes it now.
            if !peer.combat.alive && !peer.combat.corpse_cleared {
                let corpse = Vec3::from(peer.player.state().feet);
                let _ = self
                    .weapons
                    .spawn(DEATH_PROJECTILE, ActorId(owner), corpse, Vec3::ZERO, 1.0);
            }
            peer.player.teleport(&mut self.simulation.physics, feet, yaw)?;
            peer.player.set_solid(&mut self.simulation.physics, true);
            peer.combat.health = MAX_HEALTH;
            peer.combat.alive = true;
            peer.combat.spawn_tick = tick;
            peer.combat.shot_once = false;
            peer.combat.last_direct = None;
            peer.combat.corpse_cleared = false;
            peer.inputs.clear();
            // `spawnPlayer` hands control back to the new body.
            peer.control = super::ControlObject::Player;
        }
        self.give_loadout(owner, equipment.as_ref())?;
        // `GameConnection::spawnPlayer`: a spawnProjectile at the hack position.
        let center = feet + Vec3::Y * crate::player::PlayerTuning::default().stand_height * 0.5;
        let _ = self
            .weapons
            .spawn(SPAWN_PROJECTILE, ActorId(owner), center, Vec3::ZERO, 1.0);
        Ok(())
    }

    /// Minigame spawn bricks per `MiniGameSO::pickSpawnPoint`, then the
    /// player's own spawn bricks outside minigames, then the map drop points.
    fn pick_spawn(&mut self, owner: OwnerId) -> (Vec3, f32) {
        if let Some(home) = self.bot_home(owner) {
            return (home, 0.0);
        }
        if let Some(checkpoint) = self.checkpoint_spawn(owner) {
            return checkpoint;
        }
        let world = self.simulation.state();
        let spawn_bricks: Vec<_> = world
            .bricks
            .iter()
            .filter(|(_, b)| {
                matches!(&b.definition, bri_world::ContentRef::Resolved(id) if id == SPAWN_BRICK)
            })
            .map(|(id, b)| (*id, b.owner))
            .collect();
        self.spawn_seed = self
            .spawn_seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let word = self.spawn_seed;
        let chosen = self.peers.get(&owner).and_then(|peer| {
            if self
                .minigames
                .player(peer.combat.player)
                .ok()?
                .game
                .is_some()
            {
                let points: Vec<_> = spawn_bricks
                    .iter()
                    .map(|(id, owner)| mg::SpawnPoint {
                        id: *id,
                        owner: mg::AccountId(*owner),
                    })
                    .collect();
                self.minigames
                    .pick_spawn(peer.combat.player, &points, word)
                    .ok()
                    .flatten()
            } else {
                let own: Vec<_> = spawn_bricks
                    .iter()
                    .filter(|(_, o)| *o == owner)
                    .map(|(id, _)| *id)
                    .collect();
                (!own.is_empty()).then(|| own[(word % own.len() as u64) as usize])
            }
        });
        if let Some(brick) = chosen.and_then(|id| world.bricks.get(&id)) {
            let yaw = -f32::from(brick.quarter_turns) * std::f32::consts::FRAC_PI_2;
            return (Vec3::from(brick.position) + Vec3::Y * 0.1, yaw);
        }
        let points = &self.spawn_points;
        if points.is_empty() {
            return (Vec3::new(0.0, 1.0, 0.0), 0.0);
        }
        (points[(word % points.len() as u64) as usize], 0.0)
    }

    /// Per tick: rule clock, corpse timeouts and falling damage.
    pub(super) fn step_combat(&mut self, impacts: Vec<(OwnerId, Vec3)>) -> Result<()> {
        let effects = self
            .minigames
            .step()
            .map_err(|e| anyhow::anyhow!("Minigame clock: {e}"))?;
        self.apply_minigame_effects(effects)?;
        let tick = self.simulation.state().tick;
        let mut bodies = Vec::new();
        for (&owner, peer) in self.peers.iter_mut() {
            if !peer.combat.alive
                && !peer.combat.corpse_cleared
                && tick.saturating_sub(peer.combat.died_tick) >= CORPSE_TICKS
            {
                peer.player.set_solid(&mut self.simulation.physics, false);
                peer.combat.corpse_cleared = true;
                bodies.push((ActorId(owner), Vec3::from(peer.player.state().feet)));
            }
        }
        // `Player::RemoveBody`: a deathProjectile where the corpse lay.
        for (actor, feet) in bodies {
            let _ = self
                .weapons
                .spawn(DEATH_PROJECTILE, actor, feet, Vec3::ZERO, 1.0);
        }
        for (owner, impact) in impacts {
            let speed = impact.length();
            if speed < MIN_IMPACT_SPEED {
                continue;
            }
            let Some(peer) = self.peers.get(&owner) else {
                continue;
            };
            // Falling damage follows the minigame rule; the sandbox default
            // (`$pref::Server::FallingDamage`) is off.
            let allowed = self
                .minigames
                .target_for_player(peer.combat.player)
                .map(|target| {
                    self.minigames.can_damage(
                        DamageSource::Environment(EnvironmentDamage::Falling),
                        target,
                    ) == Decision::Allow
                })
                .unwrap_or(false);
            if allowed {
                let kind = if impact.normalize_or_zero().dot(Vec3::NEG_Y) > 0.5 {
                    DamageKind::Fall
                } else {
                    DamageKind::Impact
                };
                self.damage_player(owner, speed * SPEED_DAMAGE_SCALE, kind, None)?;
            }
        }
        Ok(())
    }

    /// Weapon knockback: `Player::AddVelocity(impulse / mass)`.
    pub(super) fn push_player(&mut self, target: OwnerId, impulse: Vec3) {
        if let Some(peer) = self.peers.get_mut(&target)
            && peer.combat.alive
        {
            peer.player.push(impulse / PLAYER_MASS);
        }
    }
    pub(super) fn note_shot(&mut self, owner: OwnerId) {
        if let Some(peer) = self.peers.get_mut(&owner) {
            peer.combat.shot_once = true;
        }
    }
    /// `addHealth` / `setHealth` event outputs.
    pub(super) fn change_health(
        &mut self,
        owner: OwnerId,
        change: bri_events::semantics::HealthChange,
    ) -> Result<()> {
        use bri_events::semantics::HealthChange;
        match change {
            HealthChange::Unchanged => Ok(()),
            HealthChange::SetDamage(damage) => {
                if let Some(peer) = self.peers.get_mut(&owner) {
                    peer.combat.health = (MAX_HEALTH - damage).clamp(0.0, MAX_HEALTH);
                }
                Ok(())
            }
            HealthChange::Damage(amount) => {
                self.damage_player(owner, amount, DamageKind::Event, None)
            }
        }
    }
    pub fn is_alive(&self, owner: OwnerId) -> bool {
        self.peers.get(&owner).is_some_and(|p| p.combat.alive)
    }
}
