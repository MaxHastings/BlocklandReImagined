//! Server-owned sessions and commands; transport adapters retain connection IDs.
use crate::{
    player::{MoveInput, Player, PlayerState, PlayerTuning},
    simulation::{Builder, Simulation},
};
use anyhow::{Context, Result, ensure};
use bri_world::{
    Brick, BrickId, ContentRef, OwnerId, World,
    authority::{Actor, Edit},
};
use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
mod admin;
mod bots;
mod build_load;
mod combat;
mod control;
pub use control::ControlObject;
mod debris;
mod events;
mod admin_world;
mod admin_players;
mod trust;
mod map_change;
mod inventory;
mod special;
mod tutorial;
pub use tutorial::{Abilities, BrickHand};
mod vehicles;
use vehicles::combat_input_burst;
pub use vehicles::{VehicleInfo, VehiclePose};
mod items;
mod weapons;
pub use weapons::{MountedImage, WeaponView};
mod tools;
mod undo;
mod spray;
pub use admin::{
    AdminBrickGroup, AdminCall, AdminCapability, AdminData, AdminPlayer, AdminReply, AdminSnapshot,
    MapListing,
};
pub use bri_world::authority::WrenchProperties;
pub use combat::{MAX_HEALTH, MiniGameRequest, MiniGameView, Notice, Vitals};
pub use inventory::{TOOL_SLOTS, ToolInventory};
/// Stock emotes (`Emote_*` add-ons plus the built-in sit animation).
pub const EMOTES: [&str; 5] = ["alarm", "confusion", "love", "hate", "sit"];
pub use tools::{InspectMode, ToolAction, ToolCatalog};
pub use undo::UNDO_QUEUE_SIZE;
pub use trust::{MAX_TRUST_LIST, PlayerTrust, TrustEntry, TrustLevel};

/// The surface height of water covering any part of this player's body.
fn water_surface(waters: &[bri_content::water::Water], state: &crate::player::PlayerState) -> Option<f32> {
    let tuning = state.tuning();
    let height = if state.crouched {
        tuning.crouch_height
    } else {
        tuning.stand_height
    };
    waters
        .iter()
        .find(|w| w.coverage(state.feet, height) > 0.0)
        .map(|w| w.max[1])
}

/// Queued inputs above which the server simulates extra ticks to catch up.
const INPUT_TARGET: usize = 6;
/// Bound on buffered inputs (half a second at the 120 Hz input rate).
const INPUT_QUEUE: usize = 60;
/// Ticks without any input before the motor runs idle ticks.
const INPUT_STARVED: u64 = 30;
/// Token-bucket burst for inputs; it refills at one input per server tick.
const INPUT_BURST: f32 = 48.0;

/// Aim captured with a reliable action. It affects that action's ray only;
/// movement and the authoritative player position are never rewound by it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionAim {
    pub yaw: f32,
    pub pitch: f32,
}
impl ActionAim {
    pub fn validate(self) -> Result<()> {
        MoveInput {
            yaw: self.yaw,
            pitch: self.pitch,
            ..Default::default()
        }
        .validate()
    }
    pub fn direction(self) -> Vec3 {
        Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            -self.yaw.cos() * self.pitch.cos(),
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Command {
    Admin(bri_admin::Request),
    Plant {
        definition: String,
        position: [f32; 3],
        quarter_turns: u8,
        color: u8,
    },
    Tool(ToolAction),
    /// `serverCmdUseSprayCan`: hold the colour can for a palette index.
    UseSprayCan {
        color: u8,
    },
    /// `serverCmdUseFXCan`: hold an FX can (0-6 colour effects, 7-8 shape).
    UseFxCan {
        fx: u8,
    },
    EquipTool {
        slot: Option<usize>,
    },
    DropTool {
        slot: usize,
    },
    WeaponTrigger {
        down: bool,
    },
    Avatar(bri_content::avatar::Appearance),
    SaveBuild {
        events: bool,
        ownership: bool,
    },
    LoadBuild {
        build: Box<bri_world::build::SavedBuild>,
        ownership: bool,
    },
    Activate,
    Chat(String),
    /// `serverCmdSuicide`.
    Suicide,
    /// Click to respawn while dead.
    Respawn,
    /// `serverCmdLight`.
    ToggleLight,
    Emote(String),
    MiniGame(MiniGameRequest),
    /// Next (+1) or previous (-1) free vehicle seat.
    SwitchSeat(i8),
    /// `teamChat`: only the sender's minigame members see it.
    TeamChat(String),
    /// `/clearCheckpoint`: forget the checkpoint brick and respawn.
    ClearCheckpoint,
    /// `/treasureStatus`: how many treasure chests this player has found.
    TreasureStatus,
    /// `serverCmdTrust_Invite` (level 1 build, 2 full).
    TrustInvite {
        target: OwnerId,
        level: u8,
    },
    /// `serverCmdAcceptTrustInvite`.
    AcceptTrust {
        from: OwnerId,
    },
    /// `serverCmdRejectTrustInvite`.
    RejectTrust {
        from: OwnerId,
    },
    /// `serverCmdIgnoreTrustInvite`.
    IgnoreTrust {
        from: OwnerId,
    },
    /// `serverCmdTrust_Demote` (level 0 none, 1 build).
    DemoteTrust {
        target: OwnerId,
        level: u8,
    },
    /// `serverCmdUnIgnore`.
    UnIgnore {
        target: OwnerId,
    },
    /// `TrustListUpload`: the client's saved trust list, sent after joining.
    TrustList(Vec<TrustEntry>),
    /// Admin `dropPlayerAtCamera`: move the player to the free camera's eye
    /// and return control to it.
    DropPlayerAt {
        eye: [f32; 3],
        yaw: f32,
    },
    /// `setControlObject(player)`: leave the admin free or spy camera.
    ControlPlayer,
    /// The client's brick inventory state, which only it knows.
    BrickHand(BrickHand),
    /// `serverCmdWand` (`/wand`): hold the player wand.
    Wand,
    /// `serverCmdStartTalking` / `serverCmdStopTalking`: the chat box is
    /// being typed in, shown to everyone above the chat.
    Talking(bool),
    /// A ghost-brick move, which stays client-side; the server only animates
    /// the builder.
    BuildGesture(BuildGesture),
}

/// `ServerCmdShiftBrick`, `ServerCmdSuperShiftBrick` and
/// `ServerCmdRotateBrick` play these on the builder's thread 3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuildGesture {
    ShiftUp,
    ShiftDown,
    ShiftLeft,
    ShiftRight,
    ShiftAway,
    ShiftTowards,
    RotateCw,
    RotateCcw,
}
impl BuildGesture {
    /// The v20 shift test order: z, then y, then x. `x` is away (+) or
    /// towards (-), `y` left (+) or right (-), `z` up (+) or down (-).
    pub fn shift(x: i32, y: i32, z: i32) -> Option<Self> {
        Some(match (z.signum(), y.signum(), x.signum()) {
            (1, _, _) => Self::ShiftUp,
            (-1, _, _) => Self::ShiftDown,
            (0, 1, _) => Self::ShiftLeft,
            (0, -1, _) => Self::ShiftRight,
            (0, 0, 1) => Self::ShiftAway,
            (0, 0, -1) => Self::ShiftTowards,
            _ => return None,
        })
    }
    /// Positive turns are clockwise.
    pub fn rotate(dir: i32) -> Option<Self> {
        match dir.signum() {
            1 => Some(Self::RotateCw),
            -1 => Some(Self::RotateCcw),
            _ => None,
        }
    }
    /// The original avatar sequence name.
    pub fn sequence(self) -> &'static str {
        match self {
            Self::ShiftUp => "shiftUp",
            Self::ShiftDown => "shiftDown",
            Self::ShiftLeft => "shiftLeft",
            Self::ShiftRight => "shiftRight",
            Self::ShiftAway => "shiftAway",
            Self::ShiftTowards => "shiftTO",
            Self::RotateCw => "rotCW",
            Self::RotateCcw => "rotCCW",
        }
    }
}
/// The v20 message type of a server chat line (`MessageAll('MsgUploadStart',
/// ...)`), which clients answer with its GUI sound (`addMessageCallback`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageTag {
    UploadStart,
    UploadEnd,
    ProcessComplete,
    ClearBricks,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatLine {
    pub id: u64,
    pub owner: OwnerId,
    pub name: String,
    pub text: String,
    pub tick: u64,
    pub tag: Option<MessageTag>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub weapons: WeaponView,
    pub schema_version: u32,
    pub world: World,
    pub players: Vec<PlayerState>,
    pub names: BTreeMap<OwnerId, String>,
    pub avatars: BTreeMap<OwnerId, bri_content::avatar::Appearance>,
    pub chat: Vec<ChatLine>,
    pub tools: BTreeMap<OwnerId, ToolInventory>,
}
/// A rejected command as sent to its client: a typed reason where the client
/// has a specific presentation for it, plus the readable message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rejection {
    pub plant: Option<crate::simulation::PlantFailure>,
    pub message: String,
}
impl Rejection {
    pub fn from_error(error: &anyhow::Error) -> Self {
        Self {
            plant: error
                .downcast_ref::<crate::simulation::PlantFailure>()
                .copied(),
            message: format!("{error:#}"),
        }
    }
    pub fn message(message: impl Into<String>) -> Self {
        Self {
            plant: None,
            message: message.into(),
        }
    }
}
impl std::fmt::Display for Rejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for Rejection {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Reply {
    Accepted,
    Planted(BrickId),
    Activated(Option<BrickId>),
    Inspected {
        brick_id: BrickId,
        brick: Box<Brick>,
        mode: InspectMode,
    },
    Undone(Option<BrickId>),
    Saved(Box<bri_world::build::SavedBuild>),
    Loaded {
        bricks: usize,
    },
    Admin(Box<AdminReply>),
}
struct Peer {
    player: Player,
    actor: Actor,
    name: String,
    principal: Option<bri_admin::Principal>,
    /// Last consumed input; its look angles persist while the queue is empty.
    input: MoveInput,
    /// Received but not yet simulated inputs, one per client prediction tick.
    inputs: VecDeque<(u64, MoveInput)>,
    /// Highest input sequence consumed by the motor; acknowledged in poses.
    processed_move: u64,
    input_budget: f32,
    last_sequence: u64,
    last_move_sequence: u64,
    last_input_tick: u64,
    /// The datablock a basketball shot swapped for `BallShootPlayer`.
    sport_datablock: Option<crate::player_types::PlayerType>,
    window_tick: u64,
    actions: u32,
    chats: u32,
    inspection: Option<tools::Inspection>,
    avatar: Option<bri_content::avatar::Appearance>,
    /// `SetTempColor` spray paint over the avatar's own colours.
    temp_color: Option<spray::TempColor>,
    combat: combat::Combat,
    special: special::Progress,
    control: ControlObject,
    tutorial: tutorial::Progress,
    /// `%client.isTalking`.
    talking: bool,
    /// `lastActivateTime` and `activateLevel` for the activate swing.
    last_activate: Option<u64>,
    activate_level: u32,
    /// Ticks of pending `schedule(strlen(%text) * 50, playThread, 3, root)`
    /// calls from chat, one per message.
    talk_stops: VecDeque<u64>,
}
/// `serverCmdActivateStuff`'s 320 ms repeat window at 120 ticks per second.
const ACTIVATE_REPEAT_TICKS: u64 = 38;
/// Chat talks for 50 ms per character: 6 ticks at 120 ticks per second.
const TALK_TICKS_PER_CHAR: u64 = 6;
pub struct Session {
    events: events::Events,
    specials: special::Specials,
    highlights: BTreeMap<OwnerId, admin_world::Highlight>,
    /// Installed only on the Tutorial map.
    tutorial: Option<Box<tutorial::Tutorial>>,
    bots: bots::Bots,
    vehicles: vehicles::Vehicles,
    minigames: bri_minigames::MinigamesWorld,
    spawn_points: Vec<Vec3>,
    spawn_seed: u64,
    private_notices: VecDeque<(OwnerId, Notice)>,
    last_membership: BTreeMap<OwnerId, Option<bri_minigames::GameId>>,
    item_spawners: crate::item_spawners::ItemSpawners,
    spawn_loadout: ToolInventory,
    weapons: bri_weapons::WeaponsWorld,
    weapon_triggers: BTreeMap<OwnerId, VecDeque<weapons::Trigger>>,
    weapon_gaps: BTreeMap<String, u64>,
    /// `$Pref::Server::FootballRecord`, in feet, for this server run.
    football_record: u32,
    cues: crate::presentation::Cues,
    simulation: Simulation,
    peers: BTreeMap<OwnerId, Peer>,
    next_owner: OwnerId,
    chat: VecDeque<ChatLine>,
    next_chat: u64,
    departed: BTreeMap<
        OwnerId,
        (
            String,
            bool,
            Option<bri_content::avatar::Appearance>,
            Option<bri_admin::Principal>,
        ),
    >,
    dirty: BTreeSet<BrickId>,
    notices: VecDeque<String>,
    tool_catalog: ToolCatalog,
    undo: BTreeMap<OwnerId, undo::UndoStack>,
    avatar_catalog: Option<bri_content::avatar::Package>,
    ownership_scope: Option<String>,
    bulk_window_tick: u64,
    bulk_requests: u32,
    admin: admin::AdminRuntime,
    admin_disconnects: VecDeque<OwnerId>,
    /// v20 `$Server::LAN`: single-player and LAN hosts use the looser brick
    /// damage rules.
    lan_host: bool,
    /// The save being loaded brick batch by brick batch.
    loading: Option<Box<build_load::Loading>>,
    /// Admin `/timeScale` (`setTimeScale`), 0.2 to 2.
    time_scale: f32,
    trust: trust::TrustBook,
    /// Admin Change Map choices and the pending request.
    map_list: Vec<MapListing>,
    map_change: Option<(OwnerId, String)>,
}
impl Session {
    pub fn new(simulation: Simulation) -> Self {
        // Existing native owners cannot be claimed by the next unauthenticated join.
        let next_owner = simulation
            .state()
            .bricks
            .values()
            .map(|b| b.owner)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let mut weapons = inventory::core_runtime();
        weapons.tick = simulation.state().tick;
        Self {
            events: Default::default(),
            specials: Default::default(),
            highlights: BTreeMap::new(),
            tutorial: None,
            bots: Default::default(),
            vehicles: Default::default(),
            minigames: combat::new_world(bri_minigames::Catalog::minimal_vanilla()),
            spawn_points: Vec::new(),
            spawn_seed: 0x9E37_79B9_7F4A_7C15,
            private_notices: VecDeque::new(),
            last_membership: BTreeMap::new(),
            item_spawners: Default::default(),
            spawn_loadout: ToolInventory::default(),
            weapon_triggers: BTreeMap::new(),
            weapon_gaps: BTreeMap::new(),
            football_record: 0,
            weapons,
            cues: Default::default(),
            simulation,
            peers: BTreeMap::new(),
            next_owner,
            chat: VecDeque::new(),
            next_chat: 1,
            departed: BTreeMap::new(),
            dirty: BTreeSet::new(),
            notices: VecDeque::new(),
            tool_catalog: ToolCatalog::default(),
            undo: BTreeMap::new(),
            avatar_catalog: None,
            ownership_scope: None,
            bulk_window_tick: 0,
            bulk_requests: 0,
            admin: admin::AdminRuntime::default(),
            admin_disconnects: VecDeque::new(),
            lan_host: false,
            loading: None,
            time_scale: 1.0,
            trust: Default::default(),
            map_list: Vec::new(),
            map_change: None,
        }
    }
    /// Mark a single-player or LAN host (v20 `$Server::LAN`).
    pub fn set_lan_host(&mut self, lan: bool) {
        self.lan_host = lan;
        self.refresh_trust();
    }
    pub fn simulation(&self) -> &Simulation {
        &self.simulation
    }
    pub fn cue_cursor(&self) -> u64 {
        self.cues.cursor()
    }
    pub fn dropped_cues(&self) -> u64 {
        self.cues.dropped()
    }
    pub fn take_cues(&mut self) -> Vec<crate::presentation::Cue> {
        self.cues.take()
    }
    pub fn set_avatar_catalog(&mut self, catalog: bri_content::avatar::Package) -> Result<()> {
        ensure!(
            self.peers.is_empty() && self.departed.is_empty(),
            "Cannot replace a live avatar catalog"
        );
        catalog.validate()?;
        self.avatar_catalog = Some(catalog);
        Ok(())
    }
    /// Install administrator login credentials before clients enter the session.
    /// Join passwords belong to transport admission and are configured separately.
    pub fn set_admin_passwords(
        &mut self,
        admin: bri_admin::Secret,
        super_admin: bri_admin::Secret,
    ) -> Result<()> {
        ensure!(
            self.peers.is_empty() && self.departed.is_empty(),
            "Administrator passwords can only be configured before clients connect"
        );
        // Validate both values before changing either slot so configuration is atomic.
        admin.validate()?;
        super_admin.validate()?;
        self.admin.set_passwords(admin, super_admin);
        Ok(())
    }
    pub fn avatars(&self) -> BTreeMap<OwnerId, bri_content::avatar::Appearance> {
        self.peers
            .iter()
            .filter_map(|(id, p)| {
                let mut avatar = p.avatar.clone()?;
                if let Some(temp) = &p.temp_color {
                    temp.apply(&mut avatar);
                }
                Some((*id, avatar))
            })
            .collect()
    }
    pub fn is_administrator(&self, owner: OwnerId) -> bool {
        self.peers
            .get(&owner)
            .is_some_and(|p| p.actor.administrator)
    }
    pub fn set_ownership_scope(&mut self, scope: String) -> Result<()> {
        ensure!(
            self.peers.is_empty()
                && self.departed.is_empty()
                && !scope.is_empty()
                && scope.len() <= 128,
            "Invalid/live ownership scope change"
        );
        self.ownership_scope = Some(scope);
        Ok(())
    }
    /// Spawn/admin are local server decisions, never fields from a join packet.
    pub fn join(&mut self, name: String, spawn: Vec3, administrator: bool) -> Result<OwnerId> {
        self.join_verified(name, spawn, administrator, None)
    }
    /// Register a transport-verified persistent principal. Callers must verify
    /// proof of possession before passing a principal here.
    pub fn join_verified(
        &mut self,
        name: String,
        spawn: Vec3,
        trusted_host: bool,
        principal: Option<bri_admin::Principal>,
    ) -> Result<OwnerId> {
        self.join_inner(name, spawn, trusted_host, false, principal)
    }
    fn join_inner(
        &mut self,
        name: String,
        spawn: Vec3,
        trusted_host: bool,
        is_bot: bool,
        principal: Option<bri_admin::Principal>,
    ) -> Result<OwnerId> {
        ensure!(self.peers.len() < 64, "Server is full");
        ensure!(
            !name.trim().is_empty() && name.len() <= 48 && !name.chars().any(char::is_control),
            "Invalid player name"
        );
        let owner = self.next_owner;
        let next = owner.checked_add(1).context("Owner IDs exhausted")?;
        let role = self.admin_connect(owner, name.clone(), trusted_host, is_bot, principal)?;
        let player = match Player::spawn(
            &mut self.simulation.physics,
            owner,
            spawn,
            PlayerTuning::default(),
        ) {
            Ok(player) => player,
            Err(error) => {
                self.admin_disconnect(owner);
                return Err(error);
            }
        };
        if let Err(error) = self.spawn_inventory(owner) {
            player.despawn(&mut self.simulation.physics);
            self.admin_disconnect(owner);
            return Err(error);
        }
        let combat = match self.combat_connect(owner, &name, role.is_admin()) {
            Ok(combat) => combat,
            Err(error) => {
                self.weapons.remove_actor(bri_weapons::ActorId(owner));
                player.despawn(&mut self.simulation.physics);
                self.admin_disconnect(owner);
                return Err(error);
            }
        };
        self.peers.insert(
            owner,
            Peer {
                player,
                actor: Actor {
                    owner,
                    administrator: role.is_admin(),
                    ..Default::default()
                },
                name: name.clone(),
                principal,
                combat,
                special: Default::default(),
                control: ControlObject::Player,
                tutorial: Default::default(),
                temp_color: None,
                talking: false,
                input: MoveInput::default(),
                inputs: VecDeque::new(),
                processed_move: 0,
                input_budget: INPUT_BURST,
                last_sequence: 0,
                last_move_sequence: 0,
                last_input_tick: self.simulation.state().tick,
                sport_datablock: None,
                window_tick: self.simulation.state().tick,
                actions: 0,
                chats: 0,
                inspection: None,
                last_activate: None,
                activate_level: 0,
                talk_stops: VecDeque::new(),
                avatar: self.avatar_catalog.as_ref().map(|c| c.defaults.clone()),
            },
        );
        self.next_owner = next;
        if !is_bot {
            self.announce(owner, "connected.", "ClientJoinSound");
        }
        self.refresh_trust();
        Ok(owner)
    }
    /// `MsgClientJoin` / `onDrop` lines and sounds for everyone else.
    fn announce(&mut self, owner: OwnerId, what: &str, sound: &str) {
        let Some(name) = self.peers.get(&owner).map(|p| p.name.clone()) else {
            return;
        };
        let others: Vec<OwnerId> = self
            .peers
            .keys()
            .copied()
            .filter(|o| *o != owner && !self.bots.is_bot(*o))
            .collect();
        for other in others {
            self.notify(other, Notice::Chat(format!("\u{E001}{name} {what}")));
            self.notify(other, Notice::Sound(sound.into()));
        }
    }
    pub fn disconnect(&mut self, owner: OwnerId) -> Result<()> {
        if !self.bots.is_bot(owner) {
            self.announce(owner, "has left the game.", "ClientDropSound");
        }
        self.eject(owner);
        let peer = self.peers.remove(&owner).context("Unknown connection")?;
        self.admin_disconnect(owner);
        self.weapons.remove_actor(bri_weapons::ActorId(owner));
        self.weapon_triggers.remove(&owner);
        self.departed.insert(
            owner,
            (
                peer.name,
                peer.actor.administrator,
                peer.avatar,
                peer.principal,
            ),
        );
        peer.player.despawn(&mut self.simulation.physics);
        self.release_spies(owner);
        self.combat_disconnect(peer.combat.player);
        self.last_membership.remove(&owner);
        self.trust_disconnect(owner);
        Ok(())
    }
    /// Call only after the transport authenticates its server-issued resume token.
    pub fn resume(&mut self, owner: OwnerId, spawn: Vec3) -> Result<()> {
        self.resume_trusted(owner, spawn, false)
    }
    /// Resume an authenticated transport ticket, preserving host authority only
    /// when the separate host capability was verified again at admission.
    pub fn resume_trusted(
        &mut self,
        owner: OwnerId,
        spawn: Vec3,
        trusted_host: bool,
    ) -> Result<()> {
        let principal = self.departed.get(&owner).and_then(|departed| departed.3);
        self.resume_verified(owner, spawn, trusted_host, principal)
    }
    /// Resume only when the transport has bound the ticket to this verified principal.
    pub fn resume_verified(
        &mut self,
        owner: OwnerId,
        spawn: Vec3,
        trusted_host: bool,
        principal: Option<bri_admin::Principal>,
    ) -> Result<()> {
        ensure!(
            !self.peers.contains_key(&owner) && self.peers.len() < 64,
            "Player is connected or server is full"
        );
        let (name, _previous_administrator, avatar, saved_principal) = self
            .departed
            .get(&owner)
            .context("Unknown disconnected owner")?
            .clone();
        ensure!(
            saved_principal == principal,
            "Resume identity does not match authenticated ticket"
        );
        let role = self.admin_connect(owner, name.clone(), trusted_host, false, principal)?;
        let player = match Player::spawn(
            &mut self.simulation.physics,
            owner,
            spawn,
            PlayerTuning::default(),
        ) {
            Ok(player) => player,
            Err(error) => {
                self.admin_disconnect(owner);
                return Err(error);
            }
        };
        if let Err(error) = self.spawn_inventory(owner) {
            player.despawn(&mut self.simulation.physics);
            self.admin_disconnect(owner);
            return Err(error);
        }
        let combat = match self.combat_connect(owner, &name, role.is_admin()) {
            Ok(combat) => combat,
            Err(error) => {
                self.weapons.remove_actor(bri_weapons::ActorId(owner));
                player.despawn(&mut self.simulation.physics);
                self.admin_disconnect(owner);
                return Err(error);
            }
        };
        self.peers.insert(
            owner,
            Peer {
                player,
                actor: Actor {
                    owner,
                    administrator: role.is_admin(),
                    ..Default::default()
                },
                name: name.clone(),
                principal,
                combat,
                special: Default::default(),
                control: ControlObject::Player,
                tutorial: Default::default(),
                temp_color: None,
                talking: false,
                input: MoveInput::default(),
                inputs: VecDeque::new(),
                processed_move: 0,
                input_budget: INPUT_BURST,
                last_sequence: 0,
                last_move_sequence: 0,
                last_input_tick: self.simulation.state().tick,
                sport_datablock: None,
                window_tick: self.simulation.state().tick,
                actions: 0,
                chats: 0,
                inspection: None,
                last_activate: None,
                activate_level: 0,
                talk_stops: VecDeque::new(),
                avatar,
            },
        );
        self.departed.remove(&owner);
        self.announce(owner, "connected.", "ClientJoinSound");
        self.refresh_trust();
        Ok(())
    }
    /// Queue one client input. Each input drives exactly one motor tick, so the
    /// client's prediction replays the same sequence the server simulates.
    /// Redundant copies of already-received inputs are ignored.
    pub fn movement(&mut self, owner: OwnerId, sequence: u64, input: MoveInput) -> Result<()> {
        input.validate()?;
        let peer = self.peers.get_mut(&owner).context("Unknown connection")?;
        if sequence <= peer.last_move_sequence {
            return Ok(());
        }
        ensure!(peer.input_budget >= 1.0, "Movement input rate exceeded");
        peer.input_budget -= 1.0;
        if peer.inputs.len() == INPUT_QUEUE {
            peer.inputs.pop_front();
        }
        peer.inputs.push_back((sequence, input));
        peer.last_move_sequence = sequence;
        Ok(())
    }
    /// Authoritative player states with the last input sequence each consumed.
    pub fn motion_states(&self) -> Vec<(PlayerState, u64)> {
        self.peers
            .values()
            .map(|p| (p.player.state().clone(), p.processed_move))
            .collect()
    }
    pub fn names(&self) -> BTreeMap<OwnerId, String> {
        self.peers
            .iter()
            .map(|(id, p)| (*id, p.name.clone()))
            .collect()
    }
    pub fn chat(&self) -> Vec<ChatLine> {
        self.chat.iter().cloned().collect()
    }
    pub fn take_dirty(&mut self) -> BTreeSet<BrickId> {
        std::mem::take(&mut self.dirty)
    }
    pub fn take_notices(&mut self) -> Vec<String> {
        self.notices.drain(..).collect()
    }
    pub fn take_admin_disconnects(&mut self) -> Vec<OwnerId> {
        self.admin_disconnects.drain(..).collect()
    }
    /// `owner` is resolved from the established connection, not deserialized here.
    /// `%player.playThread(3, ...)`: a builder or chat animation every
    /// client sees, carried as an avatar animation cue.
    fn play_thread_three(&mut self, tick: u64, owner: OwnerId, sequence: &str) {
        let Some(peer) = self.peers.get(&owner) else {
            return;
        };
        let position = peer.player.state().feet;
        self.cues.emit(
            tick,
            crate::presentation::CueKind::WeaponAnimation {
                actor: owner,
                thread: 3,
                sequence: sequence.into(),
                image_hand: None,
            },
            position,
        );
    }
    /// `serverCmdMessageSent` and `serverCmdTeamMessageSent`: talk, then
    /// return thread 3 to root after 50 ms per character of the message.
    fn start_talking(&mut self, tick: u64, owner: OwnerId, text_len: usize) {
        self.play_thread_three(tick, owner, "talk");
        if let Some(peer) = self.peers.get_mut(&owner) {
            let chars = u64::try_from(text_len).unwrap_or(u64::MAX);
            peer.talk_stops
                .push_back(tick.saturating_add(chars.saturating_mul(TALK_TICKS_PER_CHAR)));
        }
    }
    /// Fires due chat `root` schedules. Each message stops thread 3 on its own
    /// timer, whatever plays on it by then, as the original schedules do.
    fn stop_talking(&mut self, tick: u64) {
        let due: Vec<_> = self
            .peers
            .iter_mut()
            .flat_map(|(owner, peer)| {
                let mut stops = 0;
                while peer.talk_stops.front().is_some_and(|stop| *stop <= tick) {
                    peer.talk_stops.pop_front();
                    stops += 1;
                }
                std::iter::repeat_n(*owner, stops)
            })
            .collect();
        for owner in due {
            self.play_thread_three(tick, owner, "root");
        }
    }
    pub fn command(&mut self, owner: OwnerId, sequence: u64, command: Command) -> Result<Reply> {
        self.command_with_aim(owner, sequence, command, None)
    }
    pub fn command_with_aim(
        &mut self,
        owner: OwnerId,
        sequence: u64,
        command: Command,
        aim: Option<ActionAim>,
    ) -> Result<Reply> {
        self.command_with_aim_and_admin_persistence(owner, sequence, command, aim, |_| {
            anyhow::bail!("Persistent administration storage is not configured")
        })
    }
    /// Network host adapter supplies an atomic durable-state commit callback.
    /// Ban/unban changes are published only after this callback succeeds.
    pub fn command_with_aim_and_admin_persistence(
        &mut self,
        owner: OwnerId,
        sequence: u64,
        command: Command,
        aim: Option<ActionAim>,
        mut persist: impl FnMut(&bri_admin::DurableState) -> Result<()>,
    ) -> Result<Reply> {
        if let Command::Admin(request) = &command {
            ensure!(
                aim.is_none(),
                "Administration requests do not accept aim data"
            );
            let tick = self.simulation.state().tick;
            {
                let peer = self.peers.get_mut(&owner).context("Unknown connection")?;
                ensure!(sequence > peer.last_sequence, "Stale/replayed command");
                peer.last_sequence = sequence;
                if tick - peer.window_tick >= 120 {
                    peer.window_tick = tick;
                    peer.actions = 0;
                    peer.chats = 0;
                }
                peer.actions = peer.actions.saturating_add(1);
                ensure!(peer.actions <= 60, "Action command rate exceeded");
            }
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let call = self.admin_request(owner, request.clone(), now, &mut persist)?;
            self.admin_disconnects.extend(call.disconnects);
            return Ok(Reply::Admin(Box::new(call.reply)));
        }
        if let Command::Tool(ToolAction::SetEvents { events: rows, .. }) = &command
        {
            self.validate_event_rows(rows)?;
        }
        self.tutorial_check(&command)?;
        let tick = self.simulation.state().tick;
        let peer = self.peers.get_mut(&owner).context("Unknown connection")?;
        ensure!(sequence > peer.last_sequence, "Stale/replayed command");
        peer.last_sequence = sequence;
        if tick - peer.window_tick >= 120 {
            peer.window_tick = tick;
            peer.actions = 0;
            peer.chats = 0;
        }
        peer.actions = peer.actions.saturating_add(1);
        ensure!(peer.actions <= 60, "Action command rate exceeded");
        if let Some(aim) = aim {
            aim.validate()?;
        }
        let direction = aim.map_or_else(|| peer.player.state().forward(), ActionAim::direction);
        if matches!(
            command,
            Command::SaveBuild { .. } | Command::LoadBuild { .. }
        ) {
            if matches!(command, Command::LoadBuild { .. }) {
                ensure!(
                    peer.actor.administrator,
                    "Only the host/administrator may load builds"
                );
            }
            if tick.saturating_sub(self.bulk_window_tick) >= 120 {
                self.bulk_window_tick = tick;
                self.bulk_requests = 0;
            }
            self.bulk_requests = self.bulk_requests.saturating_add(1);
            ensure!(
                self.bulk_requests <= 4,
                "Build save/load rate exceeded; retry shortly"
            );
        }
        match command {
            Command::Admin(_) => unreachable!("handled by the authenticated admin branch above"),
            Command::DropTool { slot } => {
                self.drop_tool(owner, slot, direction)?;
                Ok(Reply::Accepted)
            }
            // Skiers keep their hands: the ski item stops skiing.
            Command::WeaponTrigger { down }
                if self.vehicles.is_mounted(owner)
                    && self.vehicles.mounted_family(owner) != Some(bri_vehicles::Family::Skis) =>
            {
                self.vehicles.set_fire(owner, down);
                Ok(Reply::Accepted)
            }
            Command::WeaponTrigger { down } => {
                ensure!(!down || peer.combat.alive, "Dead players cannot fire");
                self.weapon_trigger(owner, down, direction)?;
                if down {
                    self.note_shot(owner);
                }
                Ok(Reply::Accepted)
            }
            Command::Suicide => {
                self.suicide(owner)?;
                Ok(Reply::Accepted)
            }
            Command::Respawn => {
                self.request_respawn(owner)?;
                Ok(Reply::Accepted)
            }
            Command::ToggleLight => {
                self.toggle_light(owner)?;
                Ok(Reply::Accepted)
            }
            Command::Emote(name) => {
                ensure!(EMOTES.contains(&name.as_str()), "Unknown emote");
                ensure!(peer.combat.alive, "Dead players cannot emote");
                let feet = peer.player.state().feet;
                // `serverCmdAlarm`: the emote is an AlarmProjectile at the eye.
                if name == "alarm" {
                    let eye = peer.player.eye();
                    let _ = self.weapons.spawn(
                        "v20.projectile.alarmprojectile",
                        bri_weapons::ActorId(owner),
                        eye,
                        Vec3::Y,
                        1.0,
                    );
                }
                self.cues.emit(
                    tick,
                    crate::presentation::CueKind::Emote { actor: owner, name },
                    feet,
                );
                Ok(Reply::Accepted)
            }
            Command::MiniGame(request) => {
                self.minigame_request(owner, request)?;
                Ok(Reply::Accepted)
            }
            Command::TeamChat(text) => {
                peer.chats = peer.chats.saturating_add(1);
                ensure!(peer.chats <= 4, "Chat rate exceeded");
                ensure!(
                    !text.trim().is_empty()
                        && text.len() <= 256
                        && !text.chars().any(char::is_control),
                    "Invalid chat message"
                );
                peer.talking = false;
                let name = peer.name.clone();
                self.start_talking(tick, owner, text.len());
                self.team_chat(owner, &name, &text)?;
                Ok(Reply::Accepted)
            }
            Command::DropPlayerAt { eye, yaw } => {
                ensure!(peer.actor.administrator, "Only administrators can do that");
                ensure!(peer.combat.alive, "You are dead");
                ensure!(!self.vehicles.is_mounted(owner), "Leave the vehicle first");
                let eye = Vec3::from(eye);
                ensure!(
                    eye.is_finite() && eye.abs().max_element() < 1_000_000.0 && yaw.is_finite(),
                    "Invalid camera position"
                );
                let feet = eye - Vec3::Y * peer.player.tuning().stand_eye;
                peer.player
                    .teleport(&mut self.simulation.physics, feet, yaw)?;
                peer.inputs.clear();
                peer.control = ControlObject::Player;
                // `serverCmdDropPlayerAtCamera` costs a point inside minigames.
                let player = peer.combat.player;
                if self
                    .minigames
                    .player(player)
                    .is_ok_and(|p| p.game.is_some())
                    && let Ok(effects) = self.minigames.event_score(player, -1, true)
                {
                    self.apply_minigame_effects(effects)?;
                }
                Ok(Reply::Accepted)
            }
            Command::ControlPlayer => {
                self.return_to_body(owner)?;
                Ok(Reply::Accepted)
            }
            Command::ClearCheckpoint => {
                self.clear_checkpoint(owner)?;
                Ok(Reply::Accepted)
            }
            Command::TrustInvite { target, level } => {
                self.trust_invite(owner, target, level)?;
                Ok(Reply::Accepted)
            }
            Command::AcceptTrust { from } => {
                self.trust_accept(owner, from)?;
                Ok(Reply::Accepted)
            }
            Command::RejectTrust { from } => {
                self.trust_reject(owner, from)?;
                Ok(Reply::Accepted)
            }
            Command::IgnoreTrust { from } => {
                self.trust_ignore(owner, from)?;
                Ok(Reply::Accepted)
            }
            Command::DemoteTrust { target, level } => {
                self.trust_demote(owner, target, level)?;
                Ok(Reply::Accepted)
            }
            Command::UnIgnore { target } => {
                self.trust_unignore(owner, target)?;
                Ok(Reply::Accepted)
            }
            Command::TrustList(list) => {
                self.trust_list(owner, list)?;
                Ok(Reply::Accepted)
            }
            Command::TreasureStatus => {
                self.treasure_status(owner)?;
                Ok(Reply::Accepted)
            }
            Command::BrickHand(hand) => {
                self.set_brick_hand(owner, hand)?;
                Ok(Reply::Accepted)
            }
            Command::BuildGesture(gesture) => {
                ensure!(peer.combat.alive, "Dead players cannot build");
                self.play_thread_three(tick, owner, gesture.sequence());
                Ok(Reply::Accepted)
            }
            Command::SwitchSeat(step) => {
                ensure!(step == 1 || step == -1, "Invalid seat step");
                self.switch_seat(owner, i32::from(step))?;
                Ok(Reply::Accepted)
            }
            Command::EquipTool { slot } => {
                ensure!(peer.combat.alive, "Dead players cannot use tools");
                self.equip_tool(owner, slot)?;
                Ok(Reply::Accepted)
            }
            Command::SaveBuild { events, ownership } => {
                // A blown-up brick is only fake-dead; v20 saves it as it will
                // respawn, not hidden.
                let mut world = self.simulation.state().clone();
                for id in self.events.respawns.keys() {
                    if let Some(b) = world.bricks.get_mut(id) {
                        b.visible = true;
                        b.raycast = true;
                        b.colliding = true;
                    }
                }
                Ok(Reply::Saved(Box::new(bri_world::build::SavedBuild::capture(
                    &world,
                    self.ownership_scope.clone(),
                    events,
                    ownership,
                )?)))
            }
            Command::LoadBuild { build, ownership } => {
                let bricks = self.start_build_load(owner, *build, ownership)?;
                Ok(Reply::Loaded { bricks })
            }
            Command::Wand => {
                self.use_wand(owner)?;
                Ok(Reply::Accepted)
            }
            Command::Talking(talking) => {
                peer.talking = talking;
                Ok(Reply::Accepted)
            }

            Command::Avatar(appearance) => {
                self.avatar_catalog
                    .as_ref()
                    .context("Avatar catalog is not installed")?
                    .resolve(&appearance)?;
                peer.avatar = Some(appearance);
                Ok(Reply::Accepted)
            }
            Command::Plant {
                definition,
                position,
                quarter_turns,
                color,
            } => {
                let default_print = self
                    .tool_catalog
                    .brick_print_aspects
                    .contains_key(&definition)
                    .then(|| self.tool_catalog.default_print.clone())
                    .flatten();
                let mut brick = Brick::new(ContentRef::Resolved(definition), position, owner);
                brick.quarter_turns = quarter_turns;
                brick.color = color;
                brick.print = default_print.map(ContentRef::Resolved);
                let builder = Builder {
                    actor: &peer.actor,
                    position: Vec3::from(peer.player.state().feet),
                    reach: 50.0,
                };
                let id = self.simulation.plant(&builder, brick)?;
                self.special_planted(owner, id)?;
                self.dirty.insert(id);
                self.push_undo(owner, undo::UndoEntry::Plant(id));
                self.cues
                    .emit(tick, crate::presentation::CueKind::Plant, position);
                self.play_thread_three(tick, owner, "plant");
                Ok(Reply::Planted(id))
            }
            Command::UseSprayCan { color } => {
                self.use_spray_can(owner, tools::SPRAY_CAN_IMAGE, Some(color))?;
                Ok(Reply::Accepted)
            }
            Command::UseFxCan { fx } => {
                let image = tools::FX_CAN_IMAGES
                    .get(usize::from(fx))
                    .context("Unknown FX can")?;
                self.use_spray_can(owner, image, None)?;
                Ok(Reply::Accepted)
            }
            Command::Activate => {
                ensure!(peer.combat.alive, "Dead players cannot activate bricks");
                // `serverCmdActivateStuff`: clicks within 320 ms build up a
                // level, and the fifth repeat plays the bigger swing.
                peer.activate_level = if peer
                    .last_activate
                    .is_some_and(|last| tick - last <= ACTIVATE_REPEAT_TICKS)
                {
                    peer.activate_level.saturating_add(1)
                } else {
                    0
                };
                peer.last_activate = Some(tick);
                let swing = if peer.activate_level >= 5 {
                    "activate2"
                } else {
                    "activate"
                };
                let eye = peer.player.eye();
                self.play_thread_three(tick, owner, swing);
                let brick_distance = self
                    .simulation
                    .target(eye, direction, 5.0)?
                    .filter(|hit| hit.brick.is_some())
                    .map(|hit| hit.distance);
                if self.flip_vehicle(owner, eye, direction, brick_distance) {
                    return Ok(Reply::Activated(None));
                }
                let hit = self.simulation.activate(eye, direction)?;
                if let Some(brick) = hit
                    && self.special_activate(owner, brick)?
                {
                    self.fire_input(brick, "onActivate", Some(owner));
                }
                Ok(Reply::Activated(hit))
            }
            Command::Tool(action) => self.tool_action(owner, action),
            Command::Chat(text) => {
                peer.chats = peer.chats.saturating_add(1);
                ensure!(peer.chats <= 4, "Chat rate exceeded");
                ensure!(
                    !text.trim().is_empty()
                        && text.len() <= 256
                        && !text.chars().any(char::is_control),
                    "Invalid chat message"
                );
                let next = self
                    .next_chat
                    .checked_add(1)
                    .context("Chat IDs exhausted")?;
                peer.talking = false;
                let text_len = text.len();
                self.chat.push_back(ChatLine {
                    id: self.next_chat,
                    owner,
                    name: peer.name.clone(),
                    text,
                    tick,
                    tag: None,
                });
                self.next_chat = next;
                if self.chat.len() > 100 {
                    self.chat.pop_front();
                }
                self.start_talking(tick, owner, text_len);
                Ok(Reply::Accepted)
            }
        }
    }
    pub fn step(&mut self) -> Result<()> {
        let tick = self.simulation.state().tick;
        self.stop_talking(tick);
        self.step_bots()?;
        let mut touches = Vec::new();
        let mut impacts = Vec::new();
        let mut driving = Vec::new();
        let mut triggers = Vec::new();
        let liquids = self.simulation.liquids();
        for (&owner, peer) in self.peers.iter_mut() {
            if self.vehicles.is_mounted(owner) {
                peer.input_budget = (peer.input_budget + 1.0).min(combat_input_burst());
                // Seated players drive; consume inputs without the walking motor.
                while let Some((sequence, input)) = peer.inputs.pop_front() {
                    peer.processed_move = sequence;
                    peer.last_input_tick = tick;
                    peer.input = peer.body_input(input);
                }
                driving.push((owner, peer.input));
                peer.player.hold(&mut self.simulation.physics);
                continue;
            }
            peer.input_budget = (peer.input_budget + 1.0).min(INPUT_BURST);
            // Normally consume one queued input. A backlog (client clock ahead,
            // or a burst after a network stall) is drained a little faster. An
            // empty queue holds the player briefly to absorb jitter; players
            // who have not sent input yet, or whose connection starved, run
            // idle ticks so they cannot hang mid-air.
            let runs = if peer.inputs.len() > INPUT_TARGET {
                3
            } else if !peer.inputs.is_empty()
                || peer.processed_move == 0
                || tick - peer.last_input_tick > INPUT_STARVED
            {
                1
            } else {
                0
            };
            if runs == 0 {
                peer.player.hold(&mut self.simulation.physics);
            }
            for _ in 0..runs {
                let input = if let Some((sequence, input)) = peer.inputs.pop_front() {
                    peer.processed_move = sequence;
                    peer.last_input_tick = tick;
                    // Corpses fall and camera operators stand, ignoring controls.
                    let previous = peer.input;
                    peer.input = peer.body_input(input);
                    // `armor::onTrigger` for jump (2), crouch (3) and jet (4).
                    for (trigger, was, now) in [
                        (2, previous.jump, peer.input.jump),
                        (3, previous.crouch, peer.input.crouch),
                        (4, previous.jet, peer.input.jet),
                    ] {
                        if was != now {
                            triggers.push((owner, trigger, now));
                        }
                    }
                    peer.tutorial.abilities().apply(peer.input)
                } else {
                    MoveInput {
                        yaw: peer.input.yaw,
                        pitch: peer.input.pitch,
                        ..Default::default()
                    }
                };
                let wet_before = water_surface(&liquids, peer.player.state());
                let motion = peer.player.step_in_water(
                    &mut self.simulation.physics,
                    input,
                    &liquids,
                )?;
                let state = peer.player.state();
                let wet = water_surface(&liquids, state);
                if let Some(surface) = wet.or(wet_before).filter(|_| wet.is_some() != wet_before.is_some()) {
                    let speed = Vec3::from(state.velocity).length();
                    self.cues.emit(
                        tick,
                        crate::presentation::CueKind::Water {
                            actor: owner,
                            entered: wet.is_some(),
                            speed: speed.min(10000.0),
                        },
                        [state.feet[0], surface, state.feet[2]],
                    );
                }
                if peer.combat.alive {
                    touches.extend(motion.touched.into_iter().map(|brick| (owner, brick)));
                    impacts.push((owner, motion.impact));
                }
                if motion.jumped {
                    self.cues.emit(
                        tick,
                        crate::presentation::CueKind::Jump,
                        peer.player.state().feet,
                    );
                }
            }
        }
        self.fire_touches(touches);
        for (owner, trigger, down) in triggers {
            // The sports balls' `onBallTrigger` alternate actions.
            if self.weapons.holds_ball(bri_weapons::ActorId(owner)) {
                let _ = self
                    .weapons
                    .sport_trigger(bri_weapons::ActorId(owner), trigger, down);
            }
        }
        for (owner, input) in driving {
            self.vehicle_input(owner, input)?;
        }
        self.vehicle_pre_step()?;
        self.simulation.step()?;
        self.vehicle_post_step()?;
        self.step_weapons()?;
        self.step_temp_colors();
        self.step_items()?;
        self.step_combat(impacts)?;
        self.step_specials()?;
        self.step_highlights()?;
        self.step_tutorial()?;
        self.step_build_load()?;
        let changed = self.dirty.clone();
        self.step_events(&changed)?;
        Ok(())
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            weapons: self.weapon_view(),
            tools: self.tool_inventories(),
            avatars: self.avatars(),
            schema_version: 5,
            world: self.simulation.state().clone(),
            players: self
                .peers
                .values()
                .map(|p| p.player.state().clone())
                .collect(),
            names: self
                .peers
                .iter()
                .map(|(id, p)| (*id, p.name.clone()))
                .collect(),
            chat: self.chat.iter().cloned().collect(),
        }
    }
}
