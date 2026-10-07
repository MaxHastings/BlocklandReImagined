//! Player health, damage, death, respawn and minigame membership.
//!
//! The deterministic `bri-minigames` world owns membership, lives and scores;
//! this adapter owns health, bodies, loadouts, spawn selection and messages.
//! Behavior follows the recovered vanilla scripts: `Armor::Damage`,
//! `Armor::onImpact`, `GameConnection::onDeath`, `createPlayer` and the
//! `MiniGameSO` membership functions.
use super::*;
use crate::player_types::PlayerType;
use bri_minigames::{
    self as mg, DamageSource, Decision, EnvironmentDamage, GameId, LifeState, MinigamesWorld,
};
use bri_weapons::{ActorId, CORE_TOOLS};

/// Who may hurt whom, over only the state those rules read: players and
/// minigames. A weapons step asks it lazily, pair by pair, while the weapon
/// world is borrowed, instead of deciding every pair of players up front.
pub(super) struct DamagePolicy<'a> {
    pub(super) peers: &'a BTreeMap<OwnerId, Peer>,
    pub(super) minigames: &'a MinigamesWorld,
    pub(super) tick: u64,
}
impl DamagePolicy<'_> {
    pub(super) fn alive(&self, owner: OwnerId) -> bool {
        self.peers.get(&owner).is_some_and(|p| p.combat.alive)
    }
    pub(super) fn game_of(&self, owner: OwnerId) -> Option<GameId> {
        let player = self.peers.get(&owner)?.combat.player;
        self.minigames.player(player).ok()?.game
    }
    /// `getSimTime() - %client.lastF8Time < ms` inside a minigame; with
    /// `weapon_damage`, only when that minigame has `weaponDamage` on.
    pub(super) fn teleport_lockout(&self, owner: OwnerId, ms: u64, weapon_damage: bool) -> bool {
        let Some(at) = self.peers.get(&owner).and_then(|p| p.last_drop_tick) else {
            return false;
        };
        let Some(game) = self.game_of(owner) else {
            return false;
        };
        if weapon_damage
            && !self
                .minigames
                .game(game)
                .is_ok_and(|g| g.settings.weapon_damage)
        {
            return false;
        }
        self.tick.saturating_sub(at) < ms * u64::from(bri_weapons::TICK_HZ) / 1000
    }
    pub(super) fn player(&self, source: OwnerId, target: OwnerId, radius: bool) -> bool {
        let (Some(s), Some(t)) = (self.peers.get(&source), self.peers.get(&target)) else {
            return false;
        };
        if !t.combat.alive
            || self.teleport_lockout(source, super::admin_players::TELEPORT_WEAPON_LOCK_MS, false)
        {
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
    /// Whether living `target` is `source`'s teammate or ally in a
    /// mini-game with weapon damage on, friendly fire or not.
    pub(super) fn ally(&self, source: OwnerId, target: OwnerId) -> bool {
        let (Some(s), Some(t)) = (self.peers.get(&source), self.peers.get(&target)) else {
            return false;
        };
        source != target
            && t.combat.alive
            && self.minigames.allied(s.combat.player, t.combat.player)
            && self
                .minigames
                .player(s.combat.player)
                .ok()
                .and_then(|p| p.game)
                .and_then(|g| self.minigames.game(g).ok())
                .is_some_and(|g| g.settings.weapon_damage)
    }
    /// `WheeledVehicle::damage`: vehicles outside minigames can be damaged;
    /// inside, the minigame's vehicle damage rule applies. `owner` is the
    /// vehicle's owner, `None` when there is no such vehicle.
    pub(super) fn vehicle(&self, source: OwnerId, owner: Option<OwnerId>) -> bool {
        let (Some(owner), Some(peer)) = (owner, self.peers.get(&source)) else {
            return false;
        };
        let Ok(source) = self.minigames.projectile_source(peer.combat.player) else {
            return false;
        };
        let target = mg::Target::Object {
            kind: mg::ObjectKind::Vehicle,
            owner: Some(mg::AccountId(owner)),
            membership: mg::Membership::Owner,
            spawn_brick: true,
        };
        matches!(
            self.minigames.can_damage(source, target),
            Decision::Allow | Decision::OutsideMinigames
        )
    }
}

/// `PlayerStandardArmor.maxDamage`.
pub const MAX_HEALTH: f32 = 100.0;
/// `$Game::PlayerInvulnerabilityTime` (2.5 s) at 120 Hz.
const INVULNERABLE_TICKS: u64 = 300;
/// `$CorpseTimeoutValue` (5 s): `Player::RemoveBody` then deletes the corpse.
const CORPSE_TICKS: u64 = 600;
/// `Armor::damage` sums hits less than 300 ms apart into one pain level.
const PAIN_TICKS: u64 = 36;
/// `Player::emote`: emotes under 1000 ms apart count, 10000 ms forgive,
/// and more than five counted are dropped.
const VOICE_QUICK_TICKS: u64 = 120;
const VOICE_FORGIVE_TICKS: u64 = 1200;
const VOICE_MAX: u32 = 5;
/// `speedDamageScale` (every stock player type sets 3.8).
const SPEED_DAMAGE_SCALE: f32 = 3.8;
/// `mass` of the standard player: impulses divide by it.
pub(super) const PLAYER_MASS: f32 = 90.0;
/// Minimum respawn delay outside minigames (`$Game::MinRespawnTime`).
const MIN_RESPAWN_TICKS: u64 = 120;
/// `GameConnection::spawnPlayer`'s effect on every join and respawn.
pub const SPAWN_PROJECTILE: &str = "v20.projectile.spawnprojectile";
/// The effect a body leaves when it disappears.
pub const DEATH_PROJECTILE: &str = "v20.projectile.deathprojectile";
/// The alarm emote's flare (`serverCmdAlarm`).
pub const ALARM_PROJECTILE: &str = "v20.projectile.alarmprojectile";
const MAX_NOTICES: usize = 256;

/// Per-player authoritative combat state.
#[derive(Debug, Clone)]
pub(super) struct Combat {
    pub player: mg::PlayerId,
    pub health: f32,
    pub alive: bool,
    /// Which body this is, from 1 on joining: each spawn is a new `Player`
    /// object in v20, and what was scheduled on the old one went with it.
    /// Event targets of class Player carry it as their generation.
    pub body: u64,
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
    /// The share of their speeds an Add-On's `set_speed_scale` gives.
    pub speed_rule: f32,
    /// A gun's slowdown on top ([`bri_weapons::Slow`]): the share kept and
    /// the tick it ends.
    pub gun_slow: Option<(f32, u64)>,
    /// `Player::emote`'s spam check (`lastVoiceTime`, `voiceCount`): the
    /// tick of the last emote let through and the quick ones counted.
    pub voice: Option<u64>,
    pub voice_count: u32,
}

impl Combat {
    /// `Player::emote` without `%skipSpam`: an emote within a second of the
    /// last counts, ten quiet seconds forgive them, and past five counted
    /// the emote does nothing (and does not move the last time on).
    pub fn emote_allowed(&mut self, tick: u64) -> bool {
        let since = self
            .voice
            .map_or(u64::MAX, |last| tick.saturating_sub(last));
        if since < VOICE_QUICK_TICKS {
            self.voice_count += 1;
        } else if since > VOICE_FORGIVE_TICKS {
            self.voice_count = 0;
        }
        if self.voice_count > VOICE_MAX {
            return false;
        }
        self.voice = Some(tick);
        true
    }
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
    /// A rule holds this player's respawn (out of lives, between rounds):
    /// no respawn prompt until it lets go or the mini-game resets.
    #[serde(default)]
    pub respawn_held: bool,
    /// The tick this body spawned. Every spawn is a new v20 `Player` object,
    /// so a new value means a new body whose animation starts over.
    pub spawn_tick: u64,
    /// The tick this player last died, if ever. With `spawn_tick` it puts
    /// death and respawn on the pose timeline, which runs on its own clock.
    pub died_tick: Option<u64>,
    pub score: i64,
    pub minigame: Option<u64>,
    /// The team of their mini-game they play for (an Add-On's teams):
    /// their name shows in its colour, as Slayer's `setShapeNameColor`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team: Option<u32>,
    pub invite: Option<u64>,
    pub light: bool,
    /// Vehicle id and seat while riding.
    pub mounted: Option<(u64, u8)>,
    /// The player this one rides, and the seat.
    pub ride: Option<super::Ride>,
    /// A rule's look limits for this body (`setLookLimits`), `[down, up]`.
    pub look_limits: Option<[f32; 2]>,
    /// What this player's moves steer.
    pub control: super::ControlObject,
    /// The path their camera flies while `control` is `Path`.
    #[serde(default)]
    pub camera_path: Option<super::CameraPath>,
    /// The point their camera circles while `control` is `Point`.
    #[serde(default)]
    pub camera_point: Option<super::OrbitPoint>,
    /// Typing in the chat box (`MsgStartTalking`).
    pub talking: bool,
    /// Seated by the sit emote.
    pub sitting: bool,
    /// The unplanted ghost brick others see.
    pub ghost: Option<super::GhostBrick>,
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
    /// Its teams, which an Add-On sets up (Slayer).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub teams: Vec<mg::Team>,
    /// Add-On settings changed from their defaults, by `namespace:key`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub addon_settings: BTreeMap<String, mg::SettingValue>,
    /// The server's default game, which players in none join (Slayer's
    /// Default Minigame): the Mini-Game list's Default column.
    #[serde(default)]
    pub default: bool,
    /// A paint palette colour the host's rules gave it in place of
    /// `color` (Slayer's Color).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paint_color: Option<u8>,
    /// Owned by the server (`owner` 0) but not a game mode's: players come
    /// and go as in a player's game.
    #[serde(default)]
    pub shared: bool,
    /// How far away its members' names show (Slayer's Name Distance), or
    /// v20's own.
    #[serde(default)]
    pub name_distance: Option<u32>,
}
impl MiniGameView {
    /// Within what a host may send (a client checks what it receives).
    pub fn is_valid(&self) -> bool {
        let settings_ok = |map: &BTreeMap<String, mg::SettingValue>| {
            map.len() <= mg::MAX_ADDON_SETTINGS
                && map.iter().all(|(k, v)| {
                    k.len() <= mg::MAX_SETTING_KEY
                        && !matches!(v, mg::SettingValue::Text(t) if t.len() > mg::MAX_SETTING_TEXT)
                })
        };
        self.members.len() <= 64
            && self.color < 10
            && self.paint_color.is_none_or(|c| c < 64)
            && self
                .name_distance
                .is_none_or(|d| d <= mg::MAX_NAME_DISTANCE)
            && self.settings.title.len() <= 256
            && !self.settings.title.chars().any(char::is_control)
            && self.teams.len() <= mg::MAX_TEAMS
            && self.teams.iter().all(|t| {
                t.name.len() <= 4 * mg::MAX_TEAM_NAME
                    && !t.name.chars().any(char::is_control)
                    && settings_ok(&t.addon_settings)
            })
            && settings_ok(&self.addon_settings)
    }
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
        /// `bottomPrintBar` hidden: the global `bottomPrint(%client, ...)`
        /// passes its line count as `hideBar`; `commandToClient` prints and
        /// `GameConnection::BottomPrint` keep the bar.
        hide_bar: bool,
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
    /// `GameConnection::play2D`: a sound profile only this client hears.
    Sound(String),
    /// `MessageBoxOK` from the server.
    MessageBox {
        title: String,
        text: String,
    },
    /// `MessageBoxYesNo` from an Add-On: yes sends `package`'s `command`.
    Question {
        title: String,
        text: String,
        package: String,
        command: String,
    },
    /// `clientCmdTrustInvite`.
    TrustInvite {
        from: OwnerId,
        name: String,
        principal: [u8; 32],
        level: u8,
    },
    /// `updateClientTrustList`: save this level in the local trust list.
    TrustSaved {
        principal: [u8; 32],
        level: u8,
        name: String,
    },
    /// `secureClientCmd_ClientTrust` for every player, as this viewer sees them.
    PlayerTrust(BTreeMap<OwnerId, super::PlayerTrust>),
    /// The music loops this host's music bricks offer (its Music Files).
    MusicTracks(BTreeSet<String>),
    /// `tempBrick.setColor` under Random Brick Color: the colour the
    /// player's next brick takes, shown on their ghost.
    TempBrickColor(u8),
    /// The build this player copied, to show and place with its tool;
    /// `None` takes it away.
    Blueprint(Option<Box<crate::blueprint::Blueprint>>),
    /// Mirror the copy this player holds, as they see and place it: across
    /// the world's z axis (north and south swap), or else across its x
    /// axis (east and west swap).
    MirrorCopy {
        across_z: bool,
    },
    /// This player's ghost brick becomes `definition` turned
    /// `quarter_turns` where it stands: its mirror image
    /// ([`super::Session::mirror_ghost`]).
    MirrorGhost {
        definition: String,
        quarter_turns: u8,
    },
    /// Put the copy this player holds against the surface at `point`
    /// facing out along `normal`, as a ghost brick is put where it is
    /// aimed.
    MoveCopy {
        point: [f32; 3],
        normal: [f32; 3],
    },
    /// Turn the copy this player holds upside down where it stands, as
    /// they see and place it.
    FlipCopy,
    /// What this player's copies turn about, are super shifted by and put
    /// against a clicked surface by from now on: the whole copy (`whole`),
    /// else the brick each was taken from first.
    PivotCopy {
        whole: bool,
    },
    /// Move the copy this player holds as their brick shift keys would.
    ShiftCopy {
        offset: [i32; 3],
        super_shift: bool,
    },
    /// Turn the copy this player holds as their rotate keys would.
    RotateCopy {
        direction: i8,
    },
    /// Plant the copy this player holds where it stands, as their plant
    /// key would.
    PlantCopy,
    /// Open the wrench for every brick of this player's copy: what they
    /// tick comes back as `Command::WrenchCopy`.
    WrenchCopy {
        bricks: u32,
    },
    /// Whether the image in this player's hand takes their paint and FX
    /// cans (its `commands.paint`) rather than the can coming out.
    TakePaint(bool),
    /// `clientCmdSetScrollMode`: what this player's mouse wheel picks.
    ScrollMode(bri_package_runtime::ops::ScrollMode),
    /// Outline a box for this player while its tool is in their hand (an
    /// Add-On's selection); `None` takes it away.
    SelectionBox(Option<Box<crate::blueprint::Outline>>),
    /// `messageClient(%client, 'MsgPlantError_…')`: the plant-error icon
    /// (and sound, where the player turned it on), as a refused plant shows.
    PlantError(crate::simulation::PlantFailure),
    /// `setControlCameraFov`: an Add-On sets this player's field of view,
    /// or hands it back to their own setting with `None`.
    Fov(Option<f32>),
    /// The host emptied this player's hand (an Add-On's `unmount_image`):
    /// bricks, spray can and tools are put away on the client too.
    PutAway,
    /// A score report in its own window (an Add-On's `show_report`, Slayer's
    /// End of Round Report), or `None` to close it.
    Report(Option<Box<bri_package_runtime::report::Report>>),
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
    Create {
        color: u8,
        settings: mg::Settings,
    },
    Configure {
        settings: mg::Settings,
    },
    Join {
        game: u64,
    },
    Leave,
    Invite {
        target: OwnerId,
    },
    Accept {
        game: u64,
    },
    Reject {
        game: u64,
        ignore_owner: bool,
    },
    Kick {
        target: OwnerId,
    },
    Reset,
    RespawnAll,
    End,
    /// Change Add-On settings of `game` (the Mini-Game window's Add-On
    /// Settings), and with `teams` its team list and team settings. The
    /// game's owner or an admin.
    AddOnSettings {
        game: u64,
        settings: Vec<super::SettingEdit>,
        teams: Option<Vec<super::TeamEdit>>,
        /// Do not tell the game's players what changed (Slayer's Notify
        /// Players on Update, off).
        #[serde(default)]
        quiet: bool,
        /// Reset the game once the change is made (Update & Reset).
        #[serde(default)]
        reset: bool,
    },
    /// `request` (Configure, Invite, Kick, Reset, RespawnAll or End) on
    /// `game`, which the player may edit but need not be in: an admin or
    /// a player the host's rules let edit it, from the Mini-Game list.
    Manage {
        game: u64,
        request: Box<MiniGameRequest>,
    },
    /// Put `target` on `team` of `game` (`None`: off its teams), bringing
    /// them into the game first: an editor moving players between teams
    /// (Slayer's Add Member / Remove Member).
    SetTeam {
        game: u64,
        target: OwnerId,
        team: Option<u32>,
    },
}

/// Damage classes from `DamageTypes.cs`; weapon types carry their own name.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum DamageKind {
    Weapon {
        name: String,
        direct: bool,
        /// Which way it was travelling as it struck (a unit vector), when a
        /// shot did it.
        direction: Option<Vec3>,
        /// The projectile that did it, when one did.
        projectile: Option<String>,
        /// The special kill it makes ([`bri_weapons::DamageType::special`]):
        /// a shot a guard sent back.
        special: Option<String>,
        /// The landings of a ricocheting shot before this one.
        bounces: u32,
    },
    Fall,
    Impact,
    Suicide,
    /// Wrench event output (`kill`, negative `addHealth`).
    Event,
    /// A package operation (explosion, direct damage), named by package.
    Package {
        name: String,
    },
}
impl DamageKind {
    /// A weapon's damage with no hit point (vehicles, the hammer).
    pub(super) fn weapon(name: impl Into<String>, direct: bool) -> Self {
        Self::Weapon {
            name: name.into(),
            direct,
            direction: None,
            projectile: None,
            special: None,
            bounces: 0,
        }
    }
    /// The projectile that did it, as `on_damage` hooks read it.
    pub(super) fn projectile(&self) -> Option<&str> {
        match self {
            Self::Weapon { projectile, .. } => projectile.as_deref(),
            _ => None,
        }
    }
    pub(super) fn direction(&self) -> Option<Vec3> {
        match self {
            Self::Weapon { direction, .. } => *direction,
            _ => None,
        }
    }
    pub(super) fn bounces(&self) -> u32 {
        match self {
            Self::Weapon { bounces, .. } => *bounces,
            _ => 0,
        }
    }
    pub(super) fn special(&self) -> Option<&str> {
        match self {
            Self::Weapon { special, .. } => special.as_deref(),
            _ => None,
        }
    }
    pub(super) fn direct(&self) -> bool {
        matches!(self, Self::Weapon { direct: true, .. })
    }
    /// How `on_damage` hooks name this kind of damage.
    pub(super) fn hook_kind(&self) -> &'static str {
        match self {
            Self::Weapon { .. } => "weapon",
            Self::Fall => "fall",
            Self::Impact => "impact",
            Self::Suicide => "suicide",
            Self::Event => "event",
            Self::Package { .. } => "package",
        }
    }
    /// The `AddDamageType` name whose kill message this death shows.
    pub(super) fn type_name(&self) -> &str {
        match self {
            Self::Weapon { name, .. } => name,
            Self::Fall => "Fall",
            Self::Impact => "Impact",
            Self::Suicide | Self::Event => "Suicide",
            Self::Package { name } => name,
        }
    }
    /// [`Self::type_name`] as hooks see it: a weapon's damage type by its
    /// name, without Torque's `$DamageType::` prefix, so a round's type and
    /// one a script passed to `damage` read the same. A type an Add-On
    /// declared under a name another already had is kept as
    /// `<package>:<name>` ([`bri_weapons::Pack::merge_with`]); hooks see
    /// the name the Add-On gave it.
    pub(super) fn hook_type(&self) -> &str {
        let name = self.type_name();
        let name = match name.get(..13) {
            Some(prefix) if prefix.eq_ignore_ascii_case("$damagetype::") => &name[13..],
            _ => name,
        };
        match self {
            Self::Weapon { .. } => name.rsplit(':').next().unwrap_or(name),
            _ => name,
        }
    }
}

/// The one gate every build action passes, whichever command or tool
/// performs it: a living player, in no mini-game or in one that allows the
/// action (`EnableBuilding`, `EnablePainting`, `EnableWand`).
pub(super) fn ensure_may_build(
    combat: &Combat,
    minigames: &MinigamesWorld,
    action: mg::BuildAction,
) -> Result<()> {
    ensure!(combat.alive, "You are dead");
    let denied = matches!(
        minigames.can_build(combat.player, action),
        Ok(mg::Decision::Deny(_))
    );
    ensure!(
        !denied,
        match action {
            mg::BuildAction::Build => "Building is disabled in this mini-game",
            mg::BuildAction::Paint => "Painting is disabled in this mini-game",
            mg::BuildAction::Wand => "The wand is disabled in this mini-game",
        }
    );
    Ok(())
}

pub(super) fn catalog(pack: &bri_weapons::Pack) -> mg::Catalog {
    let mut items: BTreeMap<String, Option<String>> = CORE_TOOLS
        .iter()
        .map(|id| ((*id).to_string(), None))
        .collect();
    for (id, item) in pack.items.iter().filter(|(_, i)| !i.hidden) {
        items.insert(id.clone(), item.sport.then(|| item.image.clone()));
    }
    mg::Catalog {
        schema_version: mg::SCHEMA_VERSION,
        player_types: [mg::STANDARD_PLAYER.to_string()].into(),
        items,
        limits: mg::Limits::default(),
        defaults: mg::Settings::default(),
    }
}

/// Widen `catalog`'s bounds and change its new games' settings as an
/// Add-On's `minigame_settings` asks (Slayer's Title and respawn times).
pub(super) fn apply_minigame_settings(
    catalog: &mut mg::Catalog,
    def: &bri_package_runtime::content::MinigameSettingsDef,
) -> Result<()> {
    let ms = |range: [u32; 2]| [range[0] * 1000, range[1] * 1000];
    let mut limits = catalog.limits;
    if let Some(n) = def.title_length {
        limits.title = limits.title.max(n);
    }
    let wider = |r: [u32; 2], with: Option<[u32; 2]>| match with {
        Some(w) => {
            let w = ms(w);
            [r[0].min(w[0]), r[1].max(w[1])]
        }
        None => r,
    };
    limits.respawn_ms = wider(limits.respawn_ms, def.respawn_range);
    limits.respawn_ms[0] = limits.respawn_ms[0].max(1000);
    limits.vehicle_respawn_ms = wider(limits.vehicle_respawn_ms, def.vehicle_respawn_range);
    limits.brick_respawn_ms = wider(limits.brick_respawn_ms, def.brick_respawn_range);
    limits.brick_never |= def.brick_never;
    catalog.limits = limits;
    let d = &mut catalog.defaults;
    if let Some(title) = &def.title {
        d.title = title.clone();
    }
    if let Some(s) = def.respawn {
        d.respawn_ms = s.max(1) * 1000;
    }
    if let Some(s) = def.vehicle_respawn {
        d.vehicle_respawn_ms = s * 1000;
    }
    if let Some(s) = def.brick_respawn {
        d.brick_respawn_ms = s * 1000;
    }
    if let Some(wand) = def.enable_wand {
        d.enable_wand = wand;
    }
    if let Some(loadout) = &def.loadout {
        for (slot, item) in d.loadout.iter_mut().zip(loadout) {
            *slot = (!item.is_empty()).then(|| item.clone());
        }
    }
    // A start tool this server lacks is left out rather than refusing the
    // Add-On.
    for slot in &mut d.loadout {
        if slot
            .as_ref()
            .is_some_and(|id| !catalog.items.contains_key(id))
        {
            *slot = None;
        }
    }
    Ok(())
}

/// A game mode's mini-game (`mode.json` `minigame`) as the Mini-Game
/// dialog's settings.
pub(super) fn mode_settings(
    name: &str,
    m: &bri_package_runtime::content::ModeMiniGame,
) -> mg::Settings {
    let ms = |seconds: f32| (seconds * 1000.0).round() as u32;
    let mut loadout: [Option<String>; 5] = Default::default();
    for (slot, item) in loadout.iter_mut().zip(&m.loadout) {
        *slot = Some(item.clone());
    }
    let title = if m.title.trim().is_empty() {
        name.chars().take(35).collect()
    } else {
        m.title.clone()
    };
    mg::Settings {
        title,
        invite_only: false,
        use_all_players_bricks: m.use_all_players_bricks,
        players_use_own_bricks: false,
        use_spawn_bricks: true,
        points_break_brick: m.points_break_brick,
        points_plant_brick: m.points_plant_brick,
        points_kill_player: m.points_kill_player,
        points_kill_self: m.points_kill_self,
        points_die: m.points_die,
        respawn_ms: ms(m.respawn_seconds),
        vehicle_respawn_ms: ms(m.vehicle_respawn_seconds),
        brick_respawn_ms: ms(m.brick_respawn_seconds),
        falling_damage: m.falling_damage,
        weapon_damage: m.weapon_damage,
        self_damage: m.self_damage,
        vehicle_damage: m.vehicle_damage,
        brick_damage: m.brick_damage,
        enable_wand: false,
        enable_building: m.building,
        enable_painting: m.painting,
        player_type: if m.player_type.is_empty() {
            mg::STANDARD_PLAYER.into()
        } else {
            m.player_type.clone()
        },
        loadout,
        lives: mg::Lives::Unlimited,
    }
}

/// Every selectable archetype (v20's datablocks and packages' named
/// archetypes), whatever weapons are installed.
pub(super) fn new_world(
    mut catalog: mg::Catalog,
    archetypes: &crate::archetype::Archetypes,
) -> MinigamesWorld {
    catalog.player_types = PlayerType::ALL
        .map(|t| t.id().to_string())
        .into_iter()
        .chain(
            archetypes
                .iter()
                .skip(PlayerType::EVERY.len())
                .filter(|(_, a)| !a.name.is_empty())
                .map(|(_, a)| a.id.clone()),
        )
        .collect();
    MinigamesWorld::new(catalog, mg::PolicyMode::Internet, true).unwrap_or_else(|_| {
        MinigamesWorld::new(
            mg::Catalog::minimal_vanilla(),
            mg::PolicyMode::Internet,
            true,
        )
        .expect("minimal vanilla catalog is valid")
    })
}

pub(super) fn color_code(n: u32) -> char {
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
            body: 1,
            died_tick: 0,
            respawn_tick: 0,
            spawn_tick: tick,
            shot_once: false,
            light: false,
            last_direct: None,
            corpse_cleared: false,
            pain_level: 0.0,
            pain_tick: 0,
            speed_rule: 1.0,
            gun_slow: None,
            voice: None,
            voice_count: 0,
        })
    }
    pub(super) fn combat_disconnect(&mut self, player: mg::PlayerId) {
        if let Ok(effects) = self.minigames.disconnect(player) {
            let _ = self.apply_minigame_effects(effects);
        }
    }
    pub(super) fn owner_of(&self, player: mg::PlayerId) -> Option<OwnerId> {
        self.peers
            .iter()
            .find(|(_, p)| p.combat.player == player)
            .map(|(id, _)| *id)
    }
    pub(super) fn game_of(&self, owner: OwnerId) -> Option<GameId> {
        self.damage_policy().game_of(owner)
    }
    /// The damage rules over the state they read.
    pub(super) fn damage_policy(&self) -> DamagePolicy<'_> {
        DamagePolicy {
            peers: &self.peers,
            minigames: &self.minigames,
            tick: self.simulation.state().tick,
        }
    }

    /// The tick `owner`'s current body spawned (`Vitals::spawn_tick`).
    pub fn spawn_tick(&self, owner: OwnerId) -> Option<u64> {
        Some(self.peers.get(&owner)?.combat.spawn_tick)
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
                        respawn_held: state.is_some_and(|s| s.respawn_held),
                        spawn_tick: peer.combat.spawn_tick,
                        died_tick: (peer.combat.died_tick > 0 || !peer.combat.alive)
                            .then_some(peer.combat.died_tick),
                        score: state.map_or(0, |s| s.score),
                        minigame: state.and_then(|s| s.game).map(|g| g.0),
                        team: state.and_then(|s| s.team).map(|t| t.0),
                        invite: state.and_then(|s| s.invite).map(|g| g.0),
                        light: peer.combat.light,
                        mounted: self.mounted(*owner),
                        ride: self.ride(*owner),
                        look_limits: peer.look_limits,
                        control: peer.control,
                        camera_path: peer
                            .path
                            .as_ref()
                            .filter(|_| peer.control == super::ControlObject::Path)
                            .map(|f| f.path.clone()),
                        camera_point: peer
                            .orbit
                            .filter(|_| peer.control == super::ControlObject::Point),
                        talking: peer.talking,
                        sitting: peer.sitting,
                        ghost: self.ghost_brick(*owner),
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
                    // A game mode's mini-game belongs to the server (0).
                    owner: if game.is_server() {
                        0
                    } else {
                        self.owner_of(game.owner)?
                    },
                    color: game.color,
                    settings: game.settings.clone(),
                    members: game
                        .members
                        .iter()
                        .filter_map(|m| self.owner_of(*m))
                        .collect(),
                    teams: game.teams.list.clone(),
                    addon_settings: game.addon_settings.clone(),
                    default: self.minigames.default_game() == Some(game.id),
                    paint_color: game.paint_color,
                    shared: game.shared,
                    name_distance: game.name_distance,
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
    /// A server chat line only `owner` sees.
    pub fn private_chat(&mut self, owner: OwnerId, text: String) {
        self.notify(owner, Notice::Chat(text));
    }
    /// Server-authored chat line (owner 0) visible to everyone.
    pub(super) fn system_chat(&mut self, text: String) {
        self.system_message(None, text);
    }
    /// `MessageAll(tag, text)`: a server chat line whose v20 message type
    /// clients answer with a sound.
    pub(super) fn system_message(&mut self, tag: Option<MessageTag>, text: String) {
        let tick = self.simulation.state().tick;
        let Some(next) = self.next_chat.checked_add(1) else {
            return;
        };
        self.chat.push_back(ChatLine {
            id: self.next_chat,
            owner: 0,
            name: String::new(),
            clan: Default::default(),
            text,
            tick,
            tag,
        });
        self.next_chat = next;
        if self.chat.len() > 100 {
            self.chat.pop_front();
        }
    }
    pub(super) fn chat_game(
        &mut self,
        game: Option<GameId>,
        except: Option<OwnerId>,
        text: String,
    ) {
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
        self.damage_policy().player(source, target, radius)
    }

    /// `Armor::Damage`'s spawn protection: the first 2.5 s of a life, until
    /// the player fires (`$Game::PlayerInvulnerabilityTime`).
    pub(super) fn spawn_protected(&self, owner: OwnerId) -> bool {
        let tick = self.simulation.state().tick;
        self.peers.get(&owner).is_some_and(|peer| {
            tick.saturating_sub(peer.combat.spawn_tick) < INVULNERABLE_TICKS
                && !peer.combat.shot_once
        })
    }

    /// Which part of living player `owner` a hit at `point` strikes
    /// (`crate::player::hit_region`), or `None` for no living player.
    pub(super) fn region_of(&self, owner: OwnerId, point: Vec3) -> Option<&'static str> {
        let peer = self.peers.get(&owner).filter(|p| p.combat.alive)?;
        Some(crate::player::hit_region(&peer.player, point.to_array()))
    }

    /// `Armor::Damage`: invulnerability, crouch scaling, health and death.
    pub(super) fn damage_player(
        &mut self,
        target: OwnerId,
        amount: f32,
        kind: DamageKind,
        source: Option<OwnerId>,
    ) -> Result<()> {
        self.damage_player_at(target, amount, kind, source, None)
    }

    /// [`Self::damage_player`] from a hit at `at` (a shot's contact point
    /// or a blast's centre, as Torque's `Armor::damage` gets it), which
    /// `on_damage` hooks see with the part of the body it names.
    pub(super) fn damage_player_at(
        &mut self,
        target: OwnerId,
        amount: f32,
        kind: DamageKind,
        source: Option<OwnerId>,
        at: Option<Vec3>,
    ) -> Result<()> {
        let tick = self.simulation.state().tick;
        if !matches!(kind, DamageKind::Suicide | DamageKind::Event)
            && self.passenger_protected(
                target,
                if kind.direct() {
                    bri_vehicles::DamageKind::Direct
                } else {
                    bri_vehicles::DamageKind::Radius
                },
            )
        {
            return Ok(());
        }
        let source_observation = source.and_then(|o| {
            self.peers
                .get(&o)
                .map(|p| (o, Vec3::from(p.player.state().feet)))
        });
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
        // Where it struck, measured before any hook moves the body.
        let hit = at.map(|point| {
            (
                point,
                crate::player::hit_region(&peer.player, point.to_array()),
            )
        });
        // Add-Ons have the last word on how much it hurts.
        let (amount, renamed) = self.package_damage(target, source, amount, &kind, hit);
        if amount <= 0.0 {
            return Ok(());
        }
        // A hook may name another damage type (a crit's kill message).
        let kind = match renamed {
            Some(name) => DamageKind::Weapon {
                direct: self
                    .weapons
                    .pack
                    .damage_type(&name)
                    .is_some_and(|t| t.direct),
                direction: kind.direction(),
                projectile: kind.projectile().map(str::to_owned),
                special: kind.special().map(str::to_owned),
                bounces: kind.bounces(),
                name,
            },
            None => kind,
        };
        let Some(peer) = self.peers.get_mut(&target) else {
            return Ok(());
        };
        if !peer.combat.alive {
            return Ok(());
        }
        if let DamageKind::Weapon {
            name, direct: true, ..
        } = &kind
        {
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
        self.bots.note_hurt(target, source_observation, tick);
        let feet = peer.player.state().feet;
        self.emote_cue(
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
        let instigator = killer.filter(|k| self.peers.contains_key(k));
        // A killer from another minigame (or none) cannot be credited.
        let killer = killer.filter(|k| *k == victim || self.game_of(*k) == self.game_of(victim));
        let killer_player = killer
            .and_then(|k| self.peers.get(&k))
            .map(|p| p.combat.player);
        let game = self.game_of(victim);
        let round = game.and_then(|g| self.minigames.game(g).ok().map(|g| g.round));
        let effects = self
            .minigames
            .died(player, life, killer_player)
            .map_err(|e| anyhow::anyhow!("Death rejected: {e}"))?;
        let death = super::DeathResult {
            victim,
            life,
            killer: killer.filter(|k| self.peers.contains_key(k)),
            game: game.map(|g| g.0),
            round,
            tick,
        };
        // Radius deaths within 0.1 s of a direct hit report the direct type.
        let kind = match (&kind, &peer.combat.last_direct) {
            (
                DamageKind::Weapon {
                    direct: false,
                    special,
                    ..
                },
                Some((name, at)),
            ) if tick.saturating_sub(*at) < 12 => DamageKind::Weapon {
                name: name.clone(),
                direct: true,
                direction: None,
                projectile: None,
                special: special.clone(),
                bounces: 0,
            },
            _ => kind,
        };
        // Packages see every death and who caused it; their own policy
        // decides credit.
        self.package_death(victim, instigator);
        if let Some(game) = self.game_of(victim) {
            self.fire_rule_game_fact("onRulePlayerDied", game, Some(victim), instigator);
        }
        self.eject(victim);
        // `Armor::onDisabled` forces every rider off.
        self.release_riders(victim);
        {
            let peer = self.peers.get_mut(&victim).unwrap();
            peer.combat.alive = false;
            peer.player.set_corpse(&mut self.simulation.physics, true);
            peer.combat.health = 0.0;
            peer.combat.died_tick = tick;
            // A corpse keeps no laid-on archetypes (`pushDatablock`).
            peer.overlays = None;
            peer.combat.corpse_cleared = false;
            peer.inputs.clear();
            peer.control = super::ControlObject::Corpse;
        }
        self.observe_death_result(death);
        self.weapons.trigger(ActorId(victim), false)?;
        self.weapon_triggers.remove(&victim);
        // `armor::onDisabled` drops a held ball before the body goes limp.
        let _ = self.weapons.drop_ball(ActorId(victim));
        let _ = self.weapons.equip(ActorId(victim), None);
        // A corpse's emote slot runs nothing more. An image whose states
        // run commands comes off, as `medigunHealImage::onHeal` unmounted
        // itself from a dead wearer; an emote or pain plays out.
        let scripted = self
            .weapons
            .emote_state(ActorId(victim))
            .is_some_and(|(image, _)| {
                self.weapons
                    .pack
                    .images
                    .get(image)
                    .is_some_and(|i| !i.commands.is_empty())
            });
        if scripted {
            let feet = self.peers[&victim].player.state().feet;
            self.emote_cue(
                tick,
                crate::presentation::CueKind::Emote {
                    actor: victim,
                    name: String::new(),
                },
                feet,
            );
        } else {
            let _ = self.weapons.emote(ActorId(victim), None);
        }
        // What an Add-On hung on the body (a carried flag) goes with it; the
        // Add-On's `on_death` decides what becomes of it.
        self.weapons.clear_worn(ActorId(victim));
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
            // A killer who has left since the shot counts as no killer.
            .and_then(|k| self.peers.get(&k).map(|p| p.name.clone()));
        let pack = &self.weapons.pack;
        let base = pack.damage_type(kind.type_name()).cloned();
        // A special kill (Support_SpecialKills) lays its message over the
        // killing type's.
        let special = kind
            .special()
            .and_then(|name| pack.damage_types.get(&name.to_ascii_lowercase()))
            .filter(|t| t.special)
            .cloned();
        // The line for these names: an Add-On's death message may rename
        // the victim or killer, or hide the killer.
        let line = |victim_name: &str, killer_name: Option<&str>| match (&special, &base) {
            (Some(s), base) => s.special_message(base.as_ref(), victim_name, killer_name),
            (None, Some(t)) => t.message(victim_name, killer_name),
            (None, None) => killer_name.map_or_else(
                || victim_name.to_owned(),
                |k| format!("{k} killed {victim_name}"),
            ),
        };
        let text = line(&victim_name, killer_name.as_deref());
        let game = self.game_of(victim);
        let shown_killer = killer.filter(|k| *k != victim && killer_name.is_some());
        match self.package_death_message(
            victim,
            shown_killer,
            kind.hook_kind(),
            kind.type_name(),
            &text,
        ) {
            super::packages::DeathLine::Engine => self.chat_game(game, None, text),
            super::packages::DeathLine::Hidden => {}
            super::packages::DeathLine::Changed {
                victim: v,
                killer: k,
                hide_killer,
                suffix,
                line: whole,
                to,
            } => {
                let mut text = whole.unwrap_or_else(|| {
                    line(
                        v.as_deref().unwrap_or(&victim_name),
                        if hide_killer {
                            None
                        } else {
                            k.as_deref().or(killer_name.as_deref())
                        },
                    )
                });
                if !suffix.is_empty() {
                    text.push(' ');
                    text.push_str(&suffix);
                }
                match to {
                    Some(to) => {
                        for owner in to {
                            self.notify(owner, Notice::Chat(text.clone()));
                        }
                    }
                    None => self.chat_game(game, None, text),
                }
            }
        }
        Ok(())
    }

    /// `player.delete()` (Slayer's `/addLives` taking a living member's
    /// last life): the body goes without a death, so nobody scores, no
    /// death line is printed and no rules hear of a death. The member waits
    /// as the dead do, and the corpse is cleared at once.
    pub(super) fn remove_body(&mut self, owner: OwnerId) -> Result<()> {
        let tick = self.simulation.state().tick;
        let Some(peer) = self.peers.get(&owner) else {
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
        let effects = self
            .minigames
            .removed(player, life)
            .map_err(|e| anyhow::anyhow!("Body removal rejected: {e}"))?;
        self.eject(owner);
        self.release_riders(owner);
        {
            let peer = self.peers.get_mut(&owner).unwrap();
            peer.combat.alive = false;
            peer.player.set_corpse(&mut self.simulation.physics, true);
            peer.combat.health = 0.0;
            // Past the corpse timeout: the next step clears the body.
            peer.combat.died_tick = tick.saturating_sub(CORPSE_TICKS);
            peer.combat.corpse_cleared = false;
            peer.inputs.clear();
            peer.control = super::ControlObject::Corpse;
        }
        self.weapons.trigger(ActorId(owner), false)?;
        self.weapon_triggers.remove(&owner);
        let _ = self.weapons.drop_ball(ActorId(owner));
        let _ = self.weapons.equip(ActorId(owner), None);
        self.weapons.clear_worn(ActorId(owner));
        self.apply_minigame_effects(effects)
    }

    /// A chat line an Add-On's rules wrote: to `to`, or everyone.
    pub(super) fn send_rules_line(&mut self, line: String, to: Option<Vec<OwnerId>>) {
        let to = to.unwrap_or_else(|| self.peers.keys().copied().collect());
        for owner in to {
            self.notify(owner, Notice::Chat(line.clone()));
        }
    }

    /// Minigame team chat (`serverCmdTeamMessageSent`).
    pub(super) fn team_chat(
        &mut self,
        owner: OwnerId,
        name: &str,
        clan: &super::Clan,
        text: &str,
    ) -> Result<()> {
        let Some(game) = self.game_of(owner) else {
            self.notify(
                owner,
                Notice::Chat(format!(
                    "{}Team chat disabled - You are not in a mini-game.",
                    color_code(5)
                )),
            );
            return Ok(());
        };
        match self.package_chat(owner, text, true) {
            super::packages::ChatAnswer::Engine => {}
            super::packages::ChatAnswer::Dropped => return Ok(()),
            super::packages::ChatAnswer::Line { line, to } => {
                self.send_rules_line(line, to);
                return Ok(());
            }
        }
        // Private-use escapes are color codes; strip any the sender typed.
        let plain = |text: &str| -> String {
            text.chars()
                .filter(|c| !c.is_control() && !(0xE000..0xE010).contains(&(*c as u32)))
                .map(|c| match c {
                    '<' => '\u{2039}',
                    '>' => '\u{203A}',
                    c => c,
                })
                .collect()
        };
        let line =
            // `'\c7%1\c3%2\c7%3\c4: %4'`: clan prefix, name, clan suffix.
            format!(
                "{}{}{}{}{}{}{}: {}",
                color_code(7),
                plain(&clan.prefix),
                color_code(3),
                plain(name),
                color_code(7),
                plain(&clan.suffix),
                color_code(4),
                plain(text)
            );
        // On a team (an Add-On's teams), only teammates and allies hear it.
        let player = self.peers[&owner].combat.player;
        if self.minigames.team_of(player).is_some() {
            let hearers: Vec<_> = self
                .minigames
                .game(game)
                .map(|g| g.members.iter().copied().collect())
                .unwrap_or_default();
            for member in hearers {
                if (member == player || self.minigames.allied(player, member))
                    && let Some(to) = self.owner_of(member)
                {
                    self.notify(to, Notice::Chat(line.clone()));
                }
            }
        } else {
            self.chat_game(Some(game), None, line);
        }
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
        self.package_policy("respawn", owner)?;
        let peer = self.peers.get(&owner).context("Unknown connection")?;
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

    /// A player's mini-game request, as the Mini-Game window and its
    /// commands send it: the host's Add-On rules are asked first
    /// (`on_minigame_request`).
    pub(super) fn minigame_request(
        &mut self,
        owner: OwnerId,
        request: MiniGameRequest,
    ) -> Result<()> {
        let player = self
            .peers
            .get(&owner)
            .context("Unknown connection")?
            .combat
            .player;
        let own = self.minigames.player(player).ok().and_then(|p| p.game);
        // An editor acting on another game names it.
        let (on, request) = match request {
            MiniGameRequest::Manage { game, request } => {
                ensure!(
                    matches!(
                        *request,
                        MiniGameRequest::Configure { .. }
                            | MiniGameRequest::Invite { .. }
                            | MiniGameRequest::Kick { .. }
                            | MiniGameRequest::Reset
                            | MiniGameRequest::RespawnAll
                            | MiniGameRequest::End
                    ),
                    "Not a request about a mini-game"
                );
                (Some(GameId(game)), *request)
            }
            other => (None, other),
        };
        let mine = on.or(own);
        let (action, game, target) = match &request {
            MiniGameRequest::Create { .. } => ("create", mine, None),
            MiniGameRequest::Configure { .. } => ("edit", mine, None),
            MiniGameRequest::AddOnSettings { game, .. } => ("edit", Some(GameId(*game)), None),
            MiniGameRequest::Join { game } => ("join", Some(GameId(*game)), None),
            MiniGameRequest::Leave => ("leave", mine, None),
            MiniGameRequest::Invite { target } => ("invite", mine, Some(*target)),
            MiniGameRequest::Kick { target } => ("kick", mine, Some(*target)),
            MiniGameRequest::Reset => ("reset", mine, None),
            MiniGameRequest::RespawnAll => ("respawn_all", mine, None),
            MiniGameRequest::End => ("end", mine, None),
            MiniGameRequest::SetTeam { game, target, .. } => {
                ("team", Some(GameId(*game)), Some(*target))
            }
            MiniGameRequest::Reject {
                game,
                ignore_owner: true,
            } => ("ignore", Some(GameId(*game)), None),
            MiniGameRequest::Accept { .. } | MiniGameRequest::Reject { .. } => {
                return self.minigame_act(owner, request, false, None);
            }
            MiniGameRequest::Manage { .. } => anyhow::bail!("Not a request about a mini-game"),
        };
        // On another game, or on their own game when they do not own it
        // (an admin managing the game they play in), the engine's own rule:
        // its editors (owner or admin) may.
        let foreign = on.filter(|g| {
            Some(*g) != own
                || self
                    .minigames
                    .game(*g)
                    .is_ok_and(|game| game.owner != player)
        });
        let team = match &request {
            MiniGameRequest::SetTeam { team, .. } => *team,
            _ => None,
        };
        let teams = match &request {
            MiniGameRequest::AddOnSettings { teams, .. } => teams.as_ref().map(Vec::len),
            _ => None,
        };
        match self.package_minigame_request(owner, action, game, target, team, teams) {
            super::packages::Answer::Engine if foreign.is_some() => {
                let game = foreign.expect("checked");
                ensure!(
                    self.minigames.can_edit(player, game),
                    "Only the mini-game's owner or an admin can do that"
                );
                self.minigame_act(owner, request, true, Some(game))
            }
            super::packages::Answer::Engine => self.minigame_act(owner, request, false, None),
            super::packages::Answer::Granted => self.minigame_act(owner, request, true, on),
            super::packages::Answer::Refused { .. } if action == "ignore" => {
                let MiniGameRequest::Reject { game, .. } = request else {
                    unreachable!("ignore is a reject")
                };
                self.minigame_act(
                    owner,
                    MiniGameRequest::Reject {
                        game,
                        ignore_owner: false,
                    },
                    false,
                    None,
                )
            }
            super::packages::Answer::Refused {
                title: Some(title),
                text,
            } => {
                self.notify(owner, Notice::MessageBox { title, text });
                Ok(())
            }
            super::packages::Answer::Refused { title: None, text } => {
                self.notify(owner, Notice::Chat(format!("{}{text}", color_code(5))));
                Ok(())
            }
        }
    }

    /// Carry out a mini-game request; `granted`: the host's rules let the
    /// player do it to their game (or the game `on`) though they do not
    /// own it.
    pub(super) fn minigame_act(
        &mut self,
        owner: OwnerId,
        request: MiniGameRequest,
        granted: bool,
        on: Option<GameId>,
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
        let mine = |session: &Self| -> Result<GameId> {
            on.or_else(|| session.minigames.player(actor).ok().and_then(|p| p.game))
                .context("You are not in a mini-game")
        };
        if granted {
            let manage = |action| -> Result<mg::Command> {
                Ok(mg::Command::Manage {
                    actor,
                    game: mine(self)?,
                    action,
                })
            };
            let command = match request {
                MiniGameRequest::SetTeam { game, target, team } => {
                    return self.move_to_team(GameId(game), target, team);
                }
                MiniGameRequest::AddOnSettings {
                    game,
                    settings,
                    teams,
                    quiet,
                    reset,
                } => {
                    self.edit_settings(
                        super::packages::Editor::Granted(owner),
                        GameId(game),
                        settings,
                        teams,
                        quiet,
                        false,
                    )?;
                    if !reset {
                        return Ok(());
                    }
                    mg::Command::Manage {
                        actor,
                        game: GameId(game),
                        action: mg::Manage::Reset,
                    }
                }
                MiniGameRequest::Join { game } => {
                    let game = GameId(game);
                    ensure!(mine(self).ok() != Some(game), "Already in that mini-game");
                    let effects = self
                        .minigames
                        .host_place(actor, Some(game))
                        .map_err(|e| anyhow::anyhow!("Mini-game request rejected: {e}"))?;
                    return self.apply_minigame_effects(effects);
                }
                MiniGameRequest::Configure { settings } => manage(mg::Manage::Configure(settings))?,
                MiniGameRequest::Invite { target } => {
                    manage(mg::Manage::Invite(lookup(self, target)?))?
                }
                MiniGameRequest::Kick { target } => {
                    manage(mg::Manage::Kick(lookup(self, target)?))?
                }
                MiniGameRequest::Reset => manage(mg::Manage::Reset)?,
                MiniGameRequest::RespawnAll => manage(mg::Manage::RespawnAll)?,
                MiniGameRequest::End => manage(mg::Manage::End)?,
                // Creating, leaving and answering invitations are the
                // player's own to do.
                other => return self.minigame_act(owner, other, false, None),
            };
            return self.run_minigame_command(owner, command);
        }
        let command = match request {
            MiniGameRequest::AddOnSettings {
                game,
                settings,
                teams,
                quiet,
                reset,
            } => {
                self.edit_settings(
                    super::packages::Editor::Player(owner),
                    GameId(game),
                    settings,
                    teams,
                    quiet,
                    false,
                )?;
                if !reset {
                    return Ok(());
                }
                // Update & Reset: whoever may edit the game may reset it
                // with the change.
                mg::Command::Manage {
                    actor,
                    game: GameId(game),
                    action: mg::Manage::Reset,
                }
            }
            MiniGameRequest::Manage { .. } => anyhow::bail!("Not a request about a mini-game"),
            MiniGameRequest::SetTeam { game, target, team } => {
                ensure!(
                    self.minigames.can_edit(actor, GameId(game)),
                    "Only the mini-game's owner or an admin can do that"
                );
                return self.move_to_team(GameId(game), target, team);
            }
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
        self.run_minigame_command(owner, command)
    }

    /// Put `target` on `team` of `game`, bringing them into it first.
    fn move_to_team(&mut self, game: GameId, target: OwnerId, team: Option<u32>) -> Result<()> {
        let player = self
            .peers
            .get(&target)
            .context("Unknown player")?
            .combat
            .player;
        let team = team.map(mg::TeamId);
        if let Some(t) = team {
            ensure!(
                self.minigames
                    .game(game)
                    .is_ok_and(|g| g.teams.get(t).is_some()),
                "No such team"
            );
        }
        if self.minigames.player(player).ok().and_then(|p| p.game) != Some(game) {
            let effects = self
                .minigames
                .host_place(player, Some(game))
                .map_err(|e| anyhow::anyhow!("Mini-game request rejected: {e}"))?;
            self.apply_minigame_effects(effects)?;
        }
        let effects = self
            .minigames
            .assign_team(player, team)
            .map_err(|e| anyhow::anyhow!("Team rejected: {e}"))?;
        self.apply_minigame_effects(effects)
    }

    /// Run a player's mini-game command and tell those it concerns.
    fn run_minigame_command(&mut self, owner: OwnerId, command: mg::Command) -> Result<()> {
        let reset = matches!(
            command,
            mg::Command::Reset { .. }
                | mg::Command::Manage {
                    action: mg::Manage::Reset,
                    ..
                }
        );
        let kicked = match &command {
            mg::Command::Kick { target, .. }
            | mg::Command::Manage {
                action: mg::Manage::Kick(target),
                ..
            } => Some(*target),
            _ => None,
        };
        let rejected = match &command {
            mg::Command::Reject {
                game, ignore_owner, ..
            } => Some((*game, *ignore_owner)),
            _ => None,
        };
        let created = matches!(command, mg::Command::Create { .. });
        // MiniGameSO::endGame tells every member; they are gone afterwards.
        let ending: Vec<OwnerId> = if let mg::Command::Manage {
            action: mg::Manage::End,
            game,
            ..
        } = &command
        {
            self.minigames
                .game(*game)
                .map(|g| g.members.iter().filter_map(|&m| self.owner_of(m)).collect())
                .unwrap_or_default()
        } else if matches!(command, mg::Command::End { .. }) {
            self.game_of(owner)
                .and_then(|game| self.minigames.game(game).ok())
                .map(|g| g.members.iter().filter_map(|&m| self.owner_of(m)).collect())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let kicked =
            kicked.and_then(|t| Some((self.owner_of(t)?, self.minigames.player(t).ok()?.game?)));
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
                mg::Error::ServerGame => {
                    "This server's game mode runs the only mini-game".to_string()
                }
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
        if let Some((victim, game)) = kicked {
            self.queue_player_event("kicked", game.0, victim, Some(owner), false);
        }
        if let Some((game, ignored)) = rejected {
            self.queue_player_event("rejected", game.0, owner, None, ignored);
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
            self.observe_round_result(&effect);
            if let mg::Effect::Membership {
                player,
                game: Some(_),
                ..
            } = &effect
                && let Some(owner) = self.owner_of(*player)
                && self.is_bot(owner)
            {
                self.ensure_package_player_defaults(owner);
            }
            self.rule_minigame_effect(&effect);
            self.note_minigame_effect(&effect);
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
                        // Outside a minigame the body is a Standard Player
                        // (or the one an Add-On chose, as at respawn), and
                        // respawns at once.
                        let peer = self.peers.get_mut(&owner).unwrap();
                        peer.respawn_ms = None;
                        let body = peer
                            .package_archetype
                            .unwrap_or_else(|| PlayerType::Standard.archetype());
                        self.set_player_archetype(owner, body)?;
                        self.set_player_scale(owner, 1.0)?;
                        let max = self.max_health(owner);
                        self.peers.get_mut(&owner).unwrap().combat.health = max;
                        self.give_loadout(owner, None)?;
                    }
                }
                mg::Effect::ApplyEquipment {
                    player,
                    equipment,
                    changed_slots,
                    change_player_type,
                    ..
                } => {
                    if let Some(owner) = self.owner_of(player) {
                        // `MiniGameSO::updatePlayerDatablock` for live members.
                        // A body an Add-On chose outranks the game's player
                        // type, as it does at respawn.
                        if change_player_type
                            && self.is_alive(owner)
                            && self.peers[&owner].package_archetype.is_none()
                        {
                            let archetype = self
                                .archetypes
                                .find(&equipment.player_type)
                                .unwrap_or_default();
                            self.set_player_archetype(owner, archetype)?;
                        }
                        if changed_slots.iter().any(|c| *c) {
                            self.give_loadout(owner, Some(&equipment))?;
                        }
                    }
                }
                mg::Effect::Death {
                    player, ready_at, ..
                } => {
                    if let Some(owner) = self.owner_of(player) {
                        let peer = self.peers.get_mut(&owner).unwrap();
                        // Rule-engine ticks advance with ours; convert to world ticks.
                        let delay = match peer.respawn_ms {
                            // A rule's own time for them (`setRespawnTime`).
                            Some(ms) => {
                                (u64::from(ms) * u64::from(bri_weapons::TICK_HZ)).div_ceil(1000)
                            }
                            None => ready_at.saturating_sub(self.minigames.tick()),
                        };
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
                    // A rule's respawn time is for this player in that
                    // game (`setRespawnTime`): it ends as they leave it,
                    // whichever way, and a new game's rules set their own.
                    if previous.flatten() != game
                        && let Some(peer) = self.peers.get_mut(&owner)
                    {
                        peer.respawn_ms = None;
                    }
                    // A bot the rules add comes and goes unannounced
                    // (Slayer's `addMember` greets only connections).
                    let quiet = self.bots.rules_package(owner).is_some();
                    if let Some(Some(old)) = previous
                        && !quiet
                        && Some(old) != game
                        && self.minigames.game(old).is_ok()
                    {
                        self.chat_game(
                            Some(old),
                            Some(owner),
                            format!("{}{name} left the mini-game.", color_code(1)),
                        );
                    }
                    if let Some(new) = game.filter(|_| !quiet) {
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
                        if let Some(peer) = self.peers.get_mut(&owner) {
                            peer.respawn_ms = None;
                        }
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
                                    hide_bar: false,
                                },
                            };
                            self.notify(owner, notice);
                        }
                    }
                }
                // `MiniGameSO::Reset`: `spawnVehicle(0)` on the owners'
                // vehicle bricks and `Item.fadeIn(0)` on their item bricks.
                mg::Effect::ResetBricks {
                    owners,
                    respawn_vehicles,
                    reveal_items,
                } => {
                    let bricks: Vec<(BrickId, bool)> = self
                        .simulation
                        .state()
                        .bricks
                        .iter()
                        .filter(|(_, b)| owners.contains(&mg::AccountId(b.owner)))
                        .map(|(id, b)| (*id, b.vehicle.is_some()))
                        .collect();
                    for (brick, vehicle) in bricks {
                        // A blocked respawn must not abort the reset.
                        if respawn_vehicles
                            && vehicle
                            && let Err(error) = self.respawn_vehicle_brick(brick)
                        {
                            if self.notices.len() == 64 {
                                self.notices.pop_front();
                            }
                            self.notices
                                .push_back(format!("Reset vehicle {brick}: {error:#}"));
                        }
                        if reveal_items && let Some(item) = self.item_spawners.items.get_mut(&brick)
                        {
                            item.available_at = tick;
                        }
                    }
                }
                // Joining, leaving or resetting a minigame off LAN:
                // `ClearEventSchedules` and `resetVehicles` for the client.
                mg::Effect::Cleanup {
                    player,
                    clear_event_schedules,
                    reset_owned_vehicles,
                    clear_spawned_objects,
                } => {
                    if let Some(owner) = self.owner_of(player) {
                        if clear_event_schedules {
                            self.cancel_owner_events(owner);
                        }
                        if clear_spawned_objects {
                            self.clear_event_projectiles(owner);
                        }
                        if reset_owned_vehicles {
                            self.reset_owned_vehicles(owner);
                        }
                    }
                }
                mg::Effect::EjectVehicles { brick_owner } => {
                    self.eject_unwelcome_riders(brick_owner.0);
                }
                mg::Effect::Created { .. }
                | mg::Effect::Configured { .. }
                | mg::Effect::Score { .. }
                | mg::Effect::TeamScore { .. }
                | mg::Effect::Reset { .. }
                | mg::Effect::TeamsConfigured { .. }
                | mg::Effect::AddOnSettings { .. }
                | mg::Effect::RoundEnded { .. }
                | mg::Effect::TeamChanged { .. } => {}
                // `updatePlayerBalls`: members with empty hands get the ball.
                mg::Effect::StartBall { player, image, .. } => {
                    if let Some(owner) = self.owner_of(player)
                        && self.is_alive(owner)
                    {
                        self.weapons.start_ball(ActorId(owner), &image)?;
                    }
                }
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
        if let Some(ball) = equipment.and_then(|e| e.start_ball.as_deref())
            && self.is_alive(owner)
        {
            self.weapons.start_ball(ActorId(owner), ball)?;
        } else if self.brick_equipped(owner) && self.is_alive(owner) {
            // A new loadout empties the hands, but the client keeps its brick
            // selected (through death too).
            self.hold_brick(owner)?;
        }
        if let Some(peer) = self.peers.get_mut(&owner) {
            peer.inspection = None;
        }
        self.package_loadout(owner);
        Ok(())
    }

    /// `GameConnection::spawnPlayer`: pick a spawn, heal, equip and relocate.
    fn respawn(&mut self, owner: OwnerId, equipment: Option<mg::Equipment>) -> Result<()> {
        let tick = self.simulation.state().tick;
        // A new body is on no mount, carries nobody and wears nothing.
        self.dismount_player(owner, true);
        self.weapons.clear_worn(ActorId(owner));
        self.release_riders(owner);
        let (feet, yaw) = self.pick_spawn(owner);
        {
            let peer = self.peers.get_mut(&owner).context("Unknown connection")?;
            // The corpse is this same player: an early respawn removes it now.
            if !peer.combat.alive && !peer.combat.corpse_cleared {
                let corpse = Vec3::from(peer.player.state().feet);
                let _ =
                    self.weapons
                        .spawn(DEATH_PROJECTILE, ActorId(owner), corpse, Vec3::ZERO, 1.0);
            }
            peer.player
                .teleport(&mut self.simulation.physics, feet, yaw)?;
            // A new body: the minigame's player type, unscaled, full energy.
            let archetype = peer.package_archetype.unwrap_or_else(|| {
                equipment
                    .as_ref()
                    .and_then(|e| self.archetypes.find(&e.player_type))
                    .unwrap_or_default()
            });
            let kind = self.archetypes.resolve(archetype);
            peer.player.set_archetype(
                &mut self.simulation.physics,
                archetype,
                kind.movement.clone(),
                1.0,
            )?;
            peer.player.refill_energy();
            peer.overlays = None;
            peer.combat.speed_rule = 1.0;
            peer.combat.gun_slow = None;
            peer.player.set_speed_scale(1.0)?;
            peer.player.set_solid(&mut self.simulation.physics, true);
            peer.player.set_corpse(&mut self.simulation.physics, false);
            peer.combat.health = kind.max_health;
            peer.combat.alive = true;
            peer.combat.body += 1;
            peer.combat.spawn_tick = tick;
            peer.look_limits = None;
            peer.combat.shot_once = false;
            peer.combat.last_direct = None;
            // `lastVoiceTime` and `voiceCount` were on the old `Player`.
            peer.combat.voice = None;
            peer.combat.voice_count = 0;
            // A new life starts with full magazines and starting reserves.
            let _ = self.weapons.reset_ammo(ActorId(owner));
            let _ = self.weapons.respawned(ActorId(owner));
            peer.combat.corpse_cleared = false;
            // `serverCmdLight` mounts its fxLight on the player object, which
            // stays with the corpse: a new body starts dark.
            peer.combat.light = false;
            // Schedules on the old `Player` object went with it.
            peer.thread_timers.clear();
            // The new body wears the client's own colours (`ApplyBodyColors`).
            peer.temp_color = None;
            peer.temp_look = None;
            peer.inputs.clear();
            // `spawnPlayer` hands control back to the new body.
            peer.control = super::ControlObject::Player;
        }
        // Skiing belongs to the old Player object: a new body starts off skis.
        let _ = self.weapons.cancel_skis(ActorId(owner));
        self.give_loadout(owner, equipment.as_ref())?;
        self.package_spawn(owner);
        // `GameConnection::spawnPlayer`: a spawnProjectile at the hack position.
        let center = feet + Vec3::Y * self.peers[&owner].player.tuning().stand_height * 0.5;
        let _ = self
            .weapons
            .spawn(SPAWN_PROJECTILE, ActorId(owner), center, Vec3::ZERO, 1.0);
        Ok(())
    }

    /// `GameConnection::spawnPlayer` on joining: the same spawn choice as a
    /// respawn and the same spawn effect. The host's map drop point stands
    /// when nothing better applies.
    /// A game mode's mini-game takes every player in as they join, and
    /// failing that the default game.
    pub(super) fn join_server_game(&mut self, owner: OwnerId) -> Result<()> {
        if self.bots.is_bot(owner) {
            // Bots follow their spawn brick owner's mini-game.
            return Ok(());
        }
        let player = self
            .peers
            .get(&owner)
            .context("Unknown connection")?
            .combat
            .player;
        let Some(game) = self.minigames.server_game() else {
            // Players in no game join the default one (Slayer's Default
            // Minigame) as they first spawn.
            let effects = self
                .minigames
                .join_default(player)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            return self.apply_minigame_effects(effects);
        };
        let effects = self
            .minigames
            .host_place(player, Some(game))
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        self.apply_minigame_effects(effects)
    }
    pub(super) fn enter_world(&mut self, owner: OwnerId) -> Result<()> {
        let choice = self.spawn_choice(owner);
        let peer = self.peers.get_mut(&owner).context("Unknown connection")?;
        let mut feet = Vec3::from(peer.player.state().feet);
        if let Some((at, yaw)) = choice {
            peer.player
                .teleport(&mut self.simulation.physics, at, yaw)?;
            feet = at;
        }
        let center = feet + Vec3::Y * self.peers[&owner].player.tuning().stand_height * 0.5;
        let _ = self
            .weapons
            .spawn(SPAWN_PROJECTILE, ActorId(owner), center, Vec3::ZERO, 1.0);
        // Joining starts with the default tools: Add-Ons hand out theirs.
        self.package_loadout(owner);
        self.package_spawn(owner);
        Ok(())
    }

    /// Minigame spawn bricks per `MiniGameSO::pickSpawnPoint`, then the
    /// player's own spawn bricks outside minigames, then the map drop points.
    fn pick_spawn(&mut self, owner: OwnerId) -> (Vec3, f32) {
        if let Some(choice) = self.spawn_choice(owner) {
            return choice;
        }
        self.map_spawn()
    }

    /// One of the map's own drop points (`pickSpawnPoint()`), ignoring
    /// spawn bricks.
    pub(super) fn map_spawn(&mut self) -> (Vec3, f32) {
        let word = self.next_spawn_word();
        let points = &self.spawn_points;
        if points.is_empty() {
            return (Vec3::new(0.0, 1.0, 0.0), 0.0);
        }
        (points[(word % points.len() as u64) as usize], 0.0)
    }

    fn next_spawn_word(&mut self) -> u64 {
        self.spawn_seed = self
            .spawn_seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.spawn_seed
    }

    /// Where this player should appear, if anywhere more specific than a map
    /// drop point: a bot's home, a checkpoint, or a spawn brick.
    fn spawn_choice(&mut self, owner: OwnerId) -> Option<(Vec3, f32)> {
        if let Some(home) = self.bot_home(owner) {
            return Some((home, 0.0));
        }
        if let Some(checkpoint) = self.checkpoint_spawn(owner) {
            return Some(checkpoint);
        }
        // An Add-On's rules (Slayer's team spawns) choose before the
        // engine's spawn bricks.
        if let Some(chosen) = self.package_pick_spawn(owner) {
            return Some(chosen);
        }
        let spawn_bricks = self.spawn_bricks();
        let word = self.next_spawn_word();
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
        let brick = chosen.and_then(|id| self.simulation.state().bricks.get(&id))?;
        let yaw = -f32::from(brick.quarter_turns) * std::f32::consts::FRAC_PI_2;
        Some((Vec3::from(brick.position) + Vec3::Y * 0.1, yaw))
    }

    /// Every spawn point brick (the base Spawn Point and bricks inheriting
    /// it) with its owner, from the definition index.
    pub(super) fn spawn_bricks(&self) -> Vec<(BrickId, u64)> {
        let world = self.simulation.state();
        let mut out: Vec<_> = self
            .simulation
            .definitions
            .entries
            .iter()
            .filter(|(_, d)| d.special == crate::definitions::Special::SpawnPoint)
            .flat_map(|(id, _)| self.simulation.bricks_of(id))
            .filter_map(|id| world.bricks.get(&id).map(|b| (id, b.owner)))
            .collect();
        out.sort_unstable();
        out
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
            let Some(peer) = self.peers.get(&owner) else {
                continue;
            };
            // The engine raises `onImpact` past the datablock's
            // `minImpactSpeed` (the Horse's is 250), and `Armor::onImpact`
            // also wants `minImpactSpeed` times the player's height scale.
            let state = peer.player.state();
            let min = PlayerType::from_archetype(state.archetype)
                .unwrap_or_default()
                .min_impact_speed();
            if speed <= min || speed < min * state.scale {
                continue;
            }
            // `Armor::onImpact` never hurts a player holding the admin wand.
            if self.holds_admin_tool(owner) {
                continue;
            }
            // `Armor::onImpact`: a mini-game's own Falling Damage rule, or
            // outside mini-games the host's `$Pref::Server::FallingDamage`
            // (Advanced Config; on in v20's server/defaults.cs).
            let outside = self.admin.settings.falling_damage;
            let allowed = self
                .minigames
                .target_for_player(peer.combat.player)
                .map(|target| {
                    match self.minigames.can_damage(
                        DamageSource::Environment(EnvironmentDamage::Falling),
                        target,
                    ) {
                        Decision::Allow => true,
                        Decision::OutsideMinigames => outside,
                        _ => false,
                    }
                })
                .unwrap_or(false);
            if allowed {
                let kind = if impact.normalize_or_zero().dot(Vec3::NEG_Y) > 0.5 {
                    DamageKind::Fall
                } else {
                    DamageKind::Impact
                };
                // A guard faced the way they fell takes some of it.
                let amount = self.weapons.guard_fall(
                    ActorId(owner),
                    speed * SPEED_DAMAGE_SCALE,
                    impact.normalize_or_zero(),
                );
                self.damage_player(owner, amount, kind, None)?;
            }
        }
        Ok(())
    }

    /// Weapon knockback: `Player::AddVelocity(impulse / mass)`.
    /// A player's speeds: an Add-On's scale times any gun's slowdown.
    pub(super) fn apply_speed(&mut self, target: OwnerId) -> Result<()> {
        if let Some(peer) = self.peers.get_mut(&target) {
            let slow = peer.combat.gun_slow.map_or(1.0, |(m, _)| m);
            peer.player.set_speed_scale(peer.combat.speed_rule * slow)?;
        }
        Ok(())
    }
    /// A bullet slows the player it hit ([`bri_weapons::Slow`]): their velocity
    /// divided, their speeds lowered until a moment after the last shot.
    pub(super) fn slow_player(&mut self, target: OwnerId, slow: bri_weapons::Slow) -> Result<()> {
        let tick = self.simulation.state().tick;
        let Some(peer) = self.peers.get_mut(&target).filter(|p| p.combat.alive) else {
            return Ok(());
        };
        let velocity = Vec3::from(peer.player.state().velocity);
        peer.player.push(-velocity * (1.0 - 1.0 / slow.divisor));
        let kept = slow.after_hit(peer.combat.gun_slow.map(|(m, _)| m));
        peer.combat.gun_slow = Some((kept, tick + bri_weapons::Slow::TICKS));
        self.apply_speed(target)
    }
    /// Gun slowdowns whose time is up end.
    pub(super) fn end_gun_slows(&mut self) -> Result<()> {
        let tick = self.simulation.state().tick;
        let ended: Vec<OwnerId> = self
            .peers
            .iter_mut()
            .filter(|(_, p)| p.combat.gun_slow.is_some_and(|(_, until)| tick >= until))
            .map(|(owner, p)| {
                p.combat.gun_slow = None;
                *owner
            })
            .collect();
        for owner in ended {
            self.apply_speed(owner)?;
        }
        Ok(())
    }
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
                let max = self.max_health(owner);
                if let Some(peer) = self.peers.get_mut(&owner) {
                    peer.combat.health = (max - damage).clamp(0.0, max);
                }
                Ok(())
            }
            HealthChange::Damage(amount) => {
                self.damage_player(owner, amount, DamageKind::Event, None)
            }
        }
    }
    /// The player's archetype's `maxDamage`.
    pub(super) fn max_health(&self, owner: OwnerId) -> f32 {
        self.peers.get(&owner).map_or(MAX_HEALTH, |p| {
            self.archetypes
                .resolve(p.player.state().archetype)
                .max_health
        })
    }
    /// The archetype table clients predict with.
    pub fn archetypes(&self) -> &crate::archetype::Archetypes {
        &self.archetypes
    }
    /// `Player::setDataBlock`, keeping the player's scale and damage taken.
    pub(super) fn set_player_archetype(
        &mut self,
        owner: OwnerId,
        archetype: crate::archetype::ArchetypeId,
    ) -> Result<()> {
        let old = self.max_health(owner);
        let kind = self.archetypes.resolve(archetype);
        let (movement, max, can_ride) = (kind.movement.clone(), kind.max_health, kind.can_ride);
        let peer = self.peers.get_mut(&owner).context("Unknown connection")?;
        let scale = peer.player.state().scale;
        peer.player
            .set_archetype(&mut self.simulation.physics, archetype, movement, scale)?;
        peer.combat.health = (max - (old - peer.combat.health)).clamp(0.0, max);
        if !can_ride {
            self.eject(owner);
        }
        self.reseat_riders(owner);
        // `Armor::onNewDataBlock` swaps a held brick for the new datablock's.
        if self.holds_brick(owner) {
            self.hold_brick(owner)?;
        }
        Ok(())
    }
    /// `Player::setPlayerScale`: `setScale` on all three axes.
    pub(super) fn set_player_scale(&mut self, owner: OwnerId, scale: f32) -> Result<()> {
        let peer = self.peers.get_mut(&owner).context("Unknown connection")?;
        let archetype = peer.player.state().archetype;
        let movement = self.archetypes.resolve(archetype).movement.clone();
        peer.player
            .set_archetype(&mut self.simulation.physics, archetype, movement, scale)
    }
    pub fn is_alive(&self, owner: OwnerId) -> bool {
        self.peers.get(&owner).is_some_and(|p| p.combat.alive)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Result<Session> {
        use rapier3d::prelude::*;
        let simulation = crate::simulation::Simulation::new(
            bri_world::World::new("Games".into(), "test".into(), vec![[1.0; 4]]),
            crate::definitions::Definitions::default(),
            vec![ColliderBuilder::cuboid(100., 0.5, 100.).translation(Vector::new(0., -0.5, 0.))],
        )?;
        Ok(Session::new(simulation))
    }

    /// An admin playing in someone else's game manages it through the
    /// Add-On Settings window (`Manage` naming the game they are in); a
    /// plain member doing the same is refused.
    #[test]
    fn an_admin_manages_the_game_they_play_in_but_a_member_cannot() -> Result<()> {
        let mut s = session()?;
        let host = s.join("Host".into(), Vec3::new(0., 0.05, 0.), false)?;
        let admin = s.join("Admin".into(), Vec3::new(2., 0.05, 0.), true)?;
        let member = s.join("Member".into(), Vec3::new(-2., 0.05, 0.), false)?;
        let mut seq = 0;
        let mut send = |s: &mut Session, who, request| {
            seq += 1;
            s.command(who, seq, Command::MiniGame(request))
        };
        send(
            &mut s,
            host,
            MiniGameRequest::Create {
                color: 0,
                settings: Default::default(),
            },
        )?;
        let game = s.minigame_views()[0].id;
        send(&mut s, admin, MiniGameRequest::Join { game })?;
        send(&mut s, member, MiniGameRequest::Join { game })?;
        let manage = |request| MiniGameRequest::Manage {
            game,
            request: Box::new(request),
        };
        let refused = send(
            &mut s,
            member,
            manage(MiniGameRequest::Kick { target: admin }),
        );
        assert!(refused.is_err(), "a member may not kick");
        assert!(s.minigame_views()[0].members.contains(&admin));
        send(
            &mut s,
            admin,
            manage(MiniGameRequest::Kick { target: member }),
        )?;
        assert!(!s.minigame_views()[0].members.contains(&member));
        send(&mut s, admin, manage(MiniGameRequest::End))?;
        assert!(s.minigame_views().is_empty());
        Ok(())
    }
}
