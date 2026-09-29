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
mod breakables;
mod build_load;
mod combat;
mod control;
pub use control::{CameraView, ControlObject};
mod debris;
mod events;
mod quotas;
use quotas::Quota;
mod admin_players;
mod admin_world;
mod inventory;
mod map_change;
mod special;
mod trust;
mod tutorial;
pub use tutorial::{Abilities, BRICK_HAND_IMAGES, BrickHand};
mod riding;
pub use riding::Ride;
mod vehicles;
use vehicles::combat_input_burst;
pub use vehicles::{VehicleInfo, VehiclePose};
mod items;
mod weapons;
pub use weapons::{MountedImage, WeaponView};
mod blueprints;
mod movables;
mod packages;
mod spray;
mod tools;
mod undo;
pub use admin::{
    AdminBrickGroup, AdminCall, AdminCapability, AdminData, AdminPlayer, AdminReply, AdminSnapshot,
    MapListing, disconnect_message,
};
pub use bri_world::authority::WrenchProperties;
pub use combat::{
    DEATH_PROJECTILE, MAX_HEALTH, MiniGameRequest, MiniGameView, Notice, SPAWN_PROJECTILE, Vitals,
};
pub use inventory::{TOOL_SLOTS, ToolInventory};
pub use packages::{
    ENTITY_TAG, EntityInfo, NamespaceView, PACKAGE_SAVE_SCHEMA, PackageArg, PackageCommand,
    PackageSave, PackageStateView, PackageStats, WorldSave,
};
/// Stock emotes: the `Emote_*` add-ons (`/alarm`, `/love`, `/hate`,
/// `/confusion`) and v20's built-in `/bsd`, `/sit` and `/hug` (`/zombie` is
/// the same `playThread(1, armReadyBoth)`).
pub const EMOTES: [&str; 7] = ["alarm", "bsd", "confusion", "hate", "hug", "love", "sit"];
/// Height of m.dts's `Eye` node above the feet in the root pose
/// (avatar-rig-001), where `Player::emote` spawns its projectiles
/// (`%player.getEyePoint()`).
const V20_EYE_NODE: f32 = 2.156;
pub use tools::{InspectMode, ToolAction, ToolCatalog};
pub use trust::{MAX_TRUST_LIST, PlayerTrust, TrustEntry, TrustLevel};
pub use undo::UNDO_QUEUE_SIZE;

/// Queued inputs above which the server simulates extra ticks to catch up.
const INPUT_TARGET: usize = 6;
/// Bound on buffered inputs (half a second at the 120 Hz input rate).
const INPUT_QUEUE: usize = 60;
/// Ticks without any input before the motor runs idle ticks.
const INPUT_STARVED: u64 = 30;
/// Token-bucket burst for inputs; it refills at one input per server tick.
const INPUT_BURST: f32 = 48.0;

/// Runs off a standing input backlog. Consuming one input per tick keeps
/// whatever backlog a jitter burst or a slightly fast client clock left
/// behind, and each queued input is a tick of added latency. The smallest
/// queue length seen over a window is backlog that no jitter needed, so the
/// next window runs it off with at most one extra input per tick (the same
/// idea as Overwatch's adaptive input buffer, done on the server).
#[derive(Default)]
struct InputDrain {
    ticks: u32,
    floor: Option<usize>,
    extra: usize,
}
impl InputDrain {
    /// Half a second of 120 Hz ticks.
    const WINDOW: u32 = 60;
    /// Inputs left queued to absorb jitter.
    const KEEP: usize = 1;
    /// Inputs to run this tick beyond the usual one, given the queue length
    /// at the start of the tick. Called once every tick.
    fn extra(&mut self, queued: usize) -> usize {
        self.floor = Some(self.floor.map_or(queued, |floor| floor.min(queued)));
        self.ticks += 1;
        if self.ticks == Self::WINDOW {
            self.extra = self.floor.take().unwrap_or(0).saturating_sub(Self::KEEP);
            self.ticks = 0;
        }
        if self.extra > 0 && queued > Self::KEEP + 1 {
            self.extra -= 1;
            1
        } else {
            0
        }
    }
}

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
    /// Place the copied build this player holds (`Session::copy_build`)
    /// with its pivot at `position`, turned `quarter_turns`.
    PlaceBlueprint {
        position: [f32; 3],
        quarter_turns: u8,
    },
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
    /// Admin `dropPlayerAtCamera` (F7): the body, or the vehicle it rides,
    /// goes to the camera and takes control back. Carries the client's camera
    /// at the key press while one is flying or orbiting.
    DropPlayerAtCamera(Option<CameraView>),
    /// `setControlObject(player)`: leave the admin free or spy camera.
    ControlPlayer,
    /// The client's brick inventory state, which only it knows.
    BrickHand(BrickHand),
    /// The client's unplanted ghost brick moved, or went away.
    GhostBrick(Option<GhostBrick>),
    /// `serverCmdWand` (`/wand`): hold the player wand.
    Wand,
    /// `serverCmdStartTalking` / `serverCmdStopTalking`: the chat box is
    /// being typed in, shown to everyone above the chat.
    Talking(bool),
    /// `SteeringPrefsEvent`: the client's `$pref::Input::UseStrafeSteering`
    /// and `$pref::Input::UseAutoReturnSteering` (both on until it says).
    SteeringPrefs {
        strafe: bool,
        auto_return: bool,
    },
    /// A ghost-brick move, which stays client-side; the server only animates
    /// the builder.
    BuildGesture(BuildGesture),
    /// A command declared by an enabled package (v20 `commandToServer`).
    Package(PackageCommand),
    /// Avatar screen Done while connected: take this name now. v20 only
    /// applied `$pref::Player::LANName` on the next join.
    SetName(String),
}

/// What a command needs of its sender, checked once before dispatch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Preconditions {
    /// Refused while the sender is dead.
    pub alive: bool,
    /// Refused where the sender's mini-game denies this build action.
    pub build: Option<bri_minigames::BuildAction>,
}
impl Command {
    /// Every command declares its preconditions here, so a new command
    /// cannot skip the living or mini-game checks by omission (stress
    /// campaign W4). Checks that depend on the command's fields (firing only
    /// on trigger down) or must follow a role check stay in the handler.
    pub fn preconditions(&self) -> Preconditions {
        use bri_minigames::BuildAction;
        let (alive, build) = match self {
            Command::Plant { .. } | Command::PlaceBlueprint { .. } => {
                (true, Some(BuildAction::Build))
            }
            Command::UseSprayCan { .. }
            | Command::UseFxCan { .. }
            | Command::EquipTool { .. }
            | Command::Activate
            | Command::ToggleLight
            | Command::Wand
            | Command::BuildGesture(_) => (true, None),
            // The package's command declaration decides (`while_dead`);
            // checked with the rest of the declaration in `package_command`.
            Command::Package(_)
            | Command::Admin(_)
            | Command::Tool(_)
            | Command::DropTool { .. }
            | Command::WeaponTrigger { .. }
            | Command::Avatar(_)
            | Command::SaveBuild { .. }
            | Command::LoadBuild { .. }
            | Command::Chat(_)
            | Command::Suicide
            | Command::Respawn
            | Command::MiniGame(_)
            | Command::SwitchSeat(_)
            | Command::TeamChat(_)
            | Command::ClearCheckpoint
            | Command::TreasureStatus
            | Command::TrustInvite { .. }
            | Command::AcceptTrust { .. }
            | Command::RejectTrust { .. }
            | Command::IgnoreTrust { .. }
            | Command::DemoteTrust { .. }
            | Command::UnIgnore { .. }
            | Command::TrustList(_)
            | Command::DropPlayerAtCamera(_)
            | Command::ControlPlayer
            | Command::BrickHand(_)
            | Command::GhostBrick(_)
            // v20's emote commands quietly do nothing without a body.
            | Command::Emote(_)
            | Command::Talking(_)
            | Command::SteeringPrefs { .. }
            | Command::SetName(_) => (false, None),
        };
        Preconditions { alive, build }
    }
}
/// A player's unplanted ghost brick (`tempBrick`). v20 ghosted it to every
/// client: others see it translucent, in its colour and shape, following the
/// owner's moves and turns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GhostBrick {
    pub definition: String,
    pub position: [f32; 3],
    pub quarter_turns: u8,
    pub color: u8,
    pub print: Option<String>,
}
impl GhostBrick {
    pub fn validate(&self) -> Result<()> {
        let id = |s: &str| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control);
        ensure!(
            id(&self.definition)
                && self.print.as_deref().is_none_or(id)
                && self
                    .position
                    .iter()
                    .all(|v| v.is_finite() && v.abs() <= 1_000_000.0)
                && self.quarter_turns < 4,
            "Invalid ghost brick"
        );
        Ok(())
    }
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
    input_drain: InputDrain,
    /// Highest input sequence consumed by the motor; acknowledged in poses.
    processed_move: u64,
    input_budget: f32,
    last_sequence: u64,
    last_move_sequence: u64,
    last_input_tick: u64,
    /// The datablock a basketball shot swapped for `BallShootPlayer`.
    sport_datablock: Option<crate::archetype::ArchetypeId>,
    /// A package's choice of archetype, kept across respawns; otherwise
    /// the mini-game's player type decides.
    package_archetype: Option<crate::archetype::ArchetypeId>,
    window_tick: u64,
    actions: u32,
    chats: u32,
    /// Bricks planted in the current one-second window
    /// (`$Pref::Server::MaxBricksPerSecond`).
    plants: u32,
    /// The colour Random Brick Color gave this builder's temp brick after
    /// their last plant, until they pick a paint.
    random_color: Option<u8>,
    saves: u32,
    /// Ghost brick reports this window; they have their own budget so a
    /// builder moving a ghost never starves real actions.
    ghost_reports: u32,
    inspection: Option<tools::Inspection>,
    avatar: Option<bri_content::avatar::Appearance>,
    /// `SetTempColor` spray paint over the avatar's own colours.
    temp_color: Option<spray::TempColor>,
    /// `%client.currentColor`: the palette index of the last colour spray
    /// can picked (index 0 until one is).
    current_color: u8,
    combat: combat::Combat,
    special: special::Progress,
    control: ControlObject,
    /// `%client.Camera`'s last transform; `None` until a camera is used.
    camera: Option<CameraView>,
    /// `%client.lastF8Time`: when an admin teleport last moved this player.
    last_drop_tick: Option<u64>,
    tutorial: tutorial::Progress,
    /// `%client.isTalking`.
    talking: bool,
    /// Seated by the sit emote until they move, mount or die. Lasting state,
    /// so it replicates in vitals and late joiners see it.
    sitting: bool,
    /// The unplanted ghost brick the client last reported (`tempBrick`).
    ghost: Option<GhostBrick>,
    /// `lastActivateTime` and `activateLevel` for the activate swing.
    last_activate: Option<u64>,
    activate_level: u32,
    /// Ticks of pending `schedule(strlen(%text) * 50, playThread, 3, root)`
    /// calls from chat, one per message.
    talk_stops: VecDeque<u64>,
    /// v20's splash arming and `inLiquid` exit-sound state.
    water: crate::water::SplashState,
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
    riding: riding::Riding,
    minigames: bri_minigames::MinigamesWorld,
    spawn_points: Vec<Vec3>,
    spawn_seed: u64,
    private_notices: VecDeque<(OwnerId, Notice)>,
    /// When each builder no longer here left (for the Public Domain Timeout).
    abandoned_at: BTreeMap<OwnerId, u64>,
    /// When each player last ran `/clearBricks` (the tick).
    cleared_bricks_at: BTreeMap<OwnerId, u64>,
    last_membership: BTreeMap<OwnerId, Option<bri_minigames::GameId>>,
    item_spawners: crate::item_spawners::ItemSpawners,
    spawn_loadout: ToolInventory,
    weapons: bri_weapons::WeaponsWorld,
    weapon_triggers: BTreeMap<OwnerId, weapons::Triggers>,
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
    /// Each player's copied build (`copy_build`), waiting to be placed.
    blueprints: BTreeMap<OwnerId, crate::blueprint::Blueprint>,
    /// v20 `%client.lastPrint[%ar]`: each player's last applied print per
    /// lowercase aspect ratio, used for the next brick of that aspect.
    last_prints: BTreeMap<OwnerId, BTreeMap<String, String>>,
    avatar_catalog: Option<bri_content::avatar::Package>,
    bulk_window_tick: u64,
    bulk_requests: u32,
    save_requests: u32,
    admin: admin::AdminRuntime,
    admin_disconnects: VecDeque<OwnerId>,
    /// Plain-words close message for a pending admin disconnect.
    admin_disconnect_messages: BTreeMap<OwnerId, String>,
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
    /// Enabled mod packages and the gameplay they define.
    packages: Option<Box<packages::PackageHost>>,
    /// Bumped whenever package state a client sees may have changed
    /// (`package_state_revision`).
    package_revision: u64,
    /// v20's player datablocks, then every enabled package's archetypes.
    /// Clients receive the table with the checkpoint.
    archetypes: crate::archetype::Archetypes,
    breakables: breakables::Breakables,
    /// Holds, pushes and Add-On vehicles (`physics` operations).
    movables: movables::Movables,
}
impl Session {
    pub fn new(simulation: Simulation) -> Self {
        // Existing native owners, and every number the world has recorded
        // for a player, stay out of reach of new joins.
        let world = simulation.state();
        let next_owner = world
            .bricks
            .values()
            .map(|b| b.owner)
            .chain(world.owners.keys().copied())
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let mut weapons = inventory::core_runtime();
        weapons.tick = simulation.state().tick;
        Self {
            events: Default::default(),
            archetypes: Default::default(),
            breakables: Default::default(),
            movables: Default::default(),
            specials: Default::default(),
            highlights: BTreeMap::new(),
            tutorial: None,
            bots: Default::default(),
            vehicles: Default::default(),
            riding: Default::default(),
            minigames: combat::new_world(
                bri_minigames::Catalog::minimal_vanilla(),
                &Default::default(),
            ),
            spawn_points: Vec::new(),
            spawn_seed: 0x9E37_79B9_7F4A_7C15,
            private_notices: VecDeque::new(),
            abandoned_at: BTreeMap::new(),
            cleared_bricks_at: BTreeMap::new(),
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
            blueprints: BTreeMap::new(),
            last_prints: BTreeMap::new(),
            avatar_catalog: None,
            bulk_window_tick: 0,
            bulk_requests: 0,
            save_requests: 0,
            admin: admin::AdminRuntime::default(),
            admin_disconnects: VecDeque::new(),
            admin_disconnect_messages: BTreeMap::new(),
            lan_host: false,
            loading: None,
            time_scale: 1.0,
            trust: Default::default(),
            map_list: Vec::new(),
            map_change: None,
            packages: None,
            package_revision: 0,
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
    /// The one writer of a player's administrator flag: the actor that
    /// guards bricks and commands and the minigame roster read the same role.
    pub(crate) fn set_role(&mut self, owner: OwnerId, administrator: bool) -> Result<()> {
        let peer = self
            .peers
            .get_mut(&owner)
            .context("Administration target is not in the session")?;
        peer.actor.administrator = administrator;
        self.minigames
            .set_admin(peer.combat.player, administrator)
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
    pub fn is_administrator(&self, owner: OwnerId) -> bool {
        self.peers
            .get(&owner)
            .is_some_and(|p| p.actor.administrator)
    }
    /// Spawn/admin are local server decisions, never fields from a join packet.
    /// A new body for `owner`: at `spawn` when it is clear, else at the
    /// first clear map spawn point, else at `spawn` regardless. Builds over
    /// every spawn point never lock players out (v20 `spawnPlayer` places the
    /// body whatever is there, as a respawn here does). A session that was
    /// given no spawn points refuses an obstructed `spawn`, so its caller can
    /// try its own next candidate.
    fn place_player(&mut self, owner: OwnerId, spawn: Vec3) -> Result<Player> {
        let tuning = PlayerTuning::default();
        let physics = &mut self.simulation.physics;
        if self.spawn_points.is_empty() {
            return Player::spawn(physics, owner, spawn, tuning);
        }
        let clear = std::iter::once(spawn)
            .chain(self.spawn_points.iter().copied())
            .find(|at| Player::clear(physics, *at, &tuning));
        match clear {
            Some(at) => Player::spawn(physics, owner, at, tuning),
            None => Player::spawn_overlapping(physics, owner, spawn, tuning),
        }
    }
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
    /// Replace the host's Server Settings (v20's `$Pref::Server::*`); the
    /// same as the host's Admin > Server Settings. Tests and tools that plant
    /// faster than a player could raise the limits here.
    pub fn set_server_settings(&mut self, settings: bri_admin::ServerSettings) -> Result<()> {
        settings.validate()?;
        self.admin.settings = settings;
        Ok(())
    }
    /// The host's current Server Settings.
    pub fn server_settings(&self) -> &bri_admin::ServerSettings {
        &self.admin.settings
    }
    /// `name`, or `name 2`, `name 3`... when a connected player has it.
    fn unique_name(&self, name: String) -> String {
        self.unique_name_except(&name, None)
    }
    /// `wanted` if no other connected player uses it (ignoring case), else
    /// the first free "wanted 2", "wanted 3"... within the 48-byte limit.
    fn unique_name_except(&self, wanted: &str, except: Option<OwnerId>) -> String {
        let wanted = wanted.trim();
        let taken = |candidate: &str| {
            self.peers.iter().any(|(id, p)| {
                Some(*id) != except && p.name.trim().eq_ignore_ascii_case(candidate)
            })
        };
        if !taken(wanted) {
            return wanted.to_string();
        }
        (2..=65u32)
            .map(|n| {
                let suffix = format!(" {n}");
                let mut base = wanted.to_string();
                while base.len() + suffix.len() > 48 {
                    base.pop();
                }
                format!("{}{suffix}", base.trim_end())
            })
            .find(|candidate| !taken(candidate))
            .unwrap_or_else(|| wanted.to_string())
    }
    /// A connected player changed their name (Avatar screen Done).
    fn rename(&mut self, owner: OwnerId, wanted: &str) -> Result<()> {
        ensure!(
            !wanted.trim().is_empty()
                && wanted.len() <= 48
                && !wanted.chars().any(char::is_control),
            "Invalid player name"
        );
        let name = self.unique_name_except(wanted, Some(owner));
        let peer = self.peers.get(&owner).context("Unknown connection")?;
        if peer.name == name {
            return Ok(());
        }
        let old = peer.name.clone();
        let player = peer.combat.player;
        // Bricks show the owner record's name while the player is away.
        let record = peer
            .principal
            .map(|p| bri_world::OwnerRecord::new(p.0, name.clone()))
            .filter(|r| self.simulation.state().owner_of(&r.principal) == Some(owner));
        self.admin.rename(owner, name.clone())?;
        if let Some(record) = record {
            self.simulation.claim_owner(owner, record)?;
        }
        let _ = self.minigames.rename(player, name.clone());
        self.peers.get_mut(&owner).context("Unknown connection")?.name = name.clone();
        self.system_chat(format!("{old} is now known as {name}."));
        Ok(())
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
        // Two players with one name cannot be told apart in chat or the
        // player list (everyone starts as "Blockhead"): the later gets a number.
        let name = self.unique_name(name);
        // A returning player builds under the owner number they had in this
        // world, so their bricks are theirs again after leaving or a
        // restart. A number held by a live connection is not handed out
        // twice; that connection gets a fresh number instead. A number left
        // by a dropped connection of the same principal is taken over.
        let record = principal
            .filter(|_| !is_bot)
            .map(|p| bri_world::OwnerRecord::new(p.0, name.clone()));
        let returning = record
            .as_ref()
            .and_then(|r| self.simulation.state().owner_of(&r.principal))
            .filter(|n| !self.peers.contains_key(n));
        let owner = match returning {
            Some(owner) => owner,
            None => {
                // Spent even if the join fails below, so a claimed number is
                // never offered again.
                let owner = self.next_owner;
                self.next_owner = owner.checked_add(1).context("Owner IDs exhausted")?;
                owner
            }
        };
        if let Some(record) = record {
            let known = self
                .simulation
                .state()
                .owner_of(&record.principal)
                .is_some();
            if returning.is_some() || !known {
                self.simulation.claim_owner(owner, record)?;
            }
        }
        let role = self.admin_connect(owner, name.clone(), trusted_host, is_bot, principal)?;
        let player = match self.place_player(owner, spawn) {
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
                camera: None,
                last_drop_tick: None,
                tutorial: Default::default(),
                temp_color: None,
                current_color: 0,
                talking: false,
                sitting: false,
                ghost: None,
                input: MoveInput::default(),
                inputs: VecDeque::new(),
                input_drain: InputDrain::default(),
                processed_move: 0,
                input_budget: INPUT_BURST,
                last_sequence: 0,
                last_move_sequence: 0,
                last_input_tick: self.simulation.state().tick,
                sport_datablock: None,
                package_archetype: None,
                window_tick: self.simulation.state().tick,
                actions: 0,
                chats: 0,
                plants: 0,
                random_color: None,
                saves: 0,
                ghost_reports: 0,
                inspection: None,
                last_activate: None,
                activate_level: 0,
                talk_stops: VecDeque::new(),
                water: Default::default(),
                avatar: self.avatar_catalog.as_ref().map(|c| c.defaults.clone()),
            },
        );
        // A fresh join replaces the dropped connection it took the number from.
        self.departed.remove(&owner);
        self.abandoned_at.remove(&owner);
        self.enter_world(owner)?;
        if !is_bot {
            self.greet(owner, &name, role, trusted_host);
        }
        self.refresh_trust();
        self.packages_joined(owner);
        if !is_bot {
            let music = self.tool_catalog.sounds.clone();
            self.notify(owner, Notice::MusicTracks(music));
        }
        Ok(owner)
    }
    /// `GameConnection::startLoad` and `spawnPlayer`'s first spawn: the
    /// default `$Pref::Server::WelcomeMessage` to the joiner, "connected."
    /// and "spawned." to everyone else, then the auto-admin line to all.
    fn greet(&mut self, owner: OwnerId, name: &str, role: bri_admin::Role, host: bool) {
        self.notify(
            owner,
            Notice::Chat(format!("\u{E002}Welcome to Blockland {name}.")),
        );
        self.announce(owner, "connected.", "ClientJoinSound");
        for other in self.human_peers_except(owner) {
            self.notify(other, Notice::Chat(format!("\u{E001}{name} spawned.")));
        }
        let how = match (role, host) {
            (bri_admin::Role::SuperAdmin, true) => "Super Admin (Host)",
            (bri_admin::Role::SuperAdmin, false) => "Super Admin (Auto)",
            (bri_admin::Role::Admin, _) => "Admin (Auto)",
            _ => return,
        };
        self.admin_announce(format!("\u{E002}{name} has become {how}"));
    }
    fn human_peers_except(&self, owner: OwnerId) -> Vec<OwnerId> {
        self.peers
            .keys()
            .copied()
            .filter(|o| *o != owner && !self.bots.is_bot(*o))
            .collect()
    }
    /// `MessageAll('MsgAdminForce', ...)`: a server line for everyone.
    pub(super) fn admin_announce(&mut self, text: String) {
        for other in self.human_peers_except(0) {
            self.notify(other, Notice::Chat(text.clone()));
        }
    }
    /// `MsgClientJoin` / `onDrop` lines and sounds for everyone else.
    fn announce(&mut self, owner: OwnerId, what: &str, sound: &str) {
        let Some(name) = self.peers.get(&owner).map(|p| p.name.clone()) else {
            return;
        };
        for other in self.human_peers_except(owner) {
            self.notify(other, Notice::Chat(format!("\u{E001}{name} {what}")));
            self.notify(other, Notice::Sound(sound.into()));
        }
    }
    pub fn disconnect(&mut self, owner: OwnerId) -> Result<()> {
        if !self.bots.is_bot(owner) {
            self.announce(owner, "has left the game.", "ClientDropSound");
        }
        self.eject(owner);
        self.release_riders(owner);
        let peer = self.peers.remove(&owner).context("Unknown connection")?;
        self.admin_disconnect(owner);
        self.weapons.remove_actor(bri_weapons::ActorId(owner));
        self.weapon_triggers.remove(&owner);
        self.last_prints.remove(&owner);
        self.abandoned_at
            .insert(owner, self.simulation.state().tick);
        self.forget_blueprint(owner);
        self.forget_mover(owner);
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
        self.set_steering_prefs(owner, true, true);
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
        // Someone may have taken the name while this player was away.
        let name = self.unique_name(name);
        let role = self.admin_connect(owner, name.clone(), trusted_host, false, principal)?;
        let player = match self.place_player(owner, spawn) {
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
                camera: None,
                last_drop_tick: None,
                tutorial: Default::default(),
                temp_color: None,
                current_color: 0,
                talking: false,
                sitting: false,
                ghost: None,
                input: MoveInput::default(),
                inputs: VecDeque::new(),
                input_drain: InputDrain::default(),
                processed_move: 0,
                input_budget: INPUT_BURST,
                last_sequence: 0,
                last_move_sequence: 0,
                last_input_tick: self.simulation.state().tick,
                sport_datablock: None,
                package_archetype: None,
                window_tick: self.simulation.state().tick,
                actions: 0,
                chats: 0,
                plants: 0,
                random_color: None,
                saves: 0,
                ghost_reports: 0,
                inspection: None,
                last_activate: None,
                activate_level: 0,
                talk_stops: VecDeque::new(),
                water: Default::default(),
                avatar,
            },
        );
        self.departed.remove(&owner);
        self.abandoned_at.remove(&owner);
        self.enter_world(owner)?;
        self.announce(owner, "connected.", "ClientJoinSound");
        self.refresh_trust();
        self.packages_joined(owner);
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
    /// The world as every save keeps it: manual saves, autosaves, the save
    /// before a map change and the host's final world alike. A blown-up brick
    /// is only fake-dead; v20 saves it as it will respawn, not hidden.
    pub fn saved_world(&self) -> bri_world::World {
        let mut world = self.simulation.state().clone();
        for id in self.events.respawns.keys() {
            if let Some(b) = world.bricks.get_mut(id) {
                b.visible = true;
                b.raycast = true;
                b.colliding = true;
            }
        }
        world
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
    /// Chat lines newer than line `after` (ids only grow), without copying
    /// the rest of the history.
    pub fn chat_after(&self, after: u64) -> Vec<ChatLine> {
        let start = self.chat.partition_point(|line| line.id <= after);
        self.chat.range(start..).cloned().collect()
    }
    /// Replication takes the changed bricks. Gameplay systems that reconcile
    /// against changes early in a tick keep the ones they have not seen yet.
    pub fn take_dirty(&mut self) -> BTreeSet<BrickId> {
        let dirty = std::mem::take(&mut self.dirty);
        self.remember_unreconciled_vehicles(&dirty);
        dirty
    }
    pub fn take_notices(&mut self) -> Vec<String> {
        self.notices.drain(..).collect()
    }
    pub fn take_admin_disconnects(&mut self) -> Vec<OwnerId> {
        self.admin_disconnects.drain(..).collect()
    }
    /// What to tell `owner`, taken from [`Self::take_admin_disconnects`], as
    /// the connection closes.
    pub fn take_admin_disconnect_message(&mut self, owner: OwnerId) -> String {
        self.admin_disconnect_messages
            .remove(&owner)
            .unwrap_or_else(|| "You were removed from the server.".into())
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
        mut command: Command,
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
            return Ok(Reply::Admin(Box::new(call.reply)));
        }
        // Cheap admission (sequence, rate) runs before any per-element work,
        // so a replayed or rate-limited request costs nothing to refuse.
        let tick = self.simulation.state().tick;
        let (alive, player) = {
            let peer = self.peers.get_mut(&owner).context("Unknown connection")?;
            ensure!(sequence > peer.last_sequence, "Stale/replayed command");
            peer.last_sequence = sequence;
            if tick - peer.window_tick >= 120 {
                peer.window_tick = tick;
                peer.actions = 0;
                peer.chats = 0;
                peer.plants = 0;
                peer.saves = 0;
                peer.ghost_reports = 0;
            }
            if matches!(command, Command::GhostBrick(_)) {
                peer.ghost_reports = peer.ghost_reports.saturating_add(1);
                ensure!(peer.ghost_reports <= 30, "Ghost brick report rate exceeded");
            } else {
                peer.actions = peer.actions.saturating_add(1);
                ensure!(peer.actions <= 60, "Action command rate exceeded");
            }
            (peer.combat.alive, peer.combat.player)
        };
        let needs = command.preconditions();
        ensure!(alive || !needs.alive, "Dead players cannot do that");
        if let Some(action) = needs.build {
            ensure!(
                !matches!(
                    self.minigames.can_build(player, action),
                    Ok(bri_minigames::Decision::Deny(_))
                ),
                "Building is disabled in this mini-game"
            );
            self.package_policy("build", owner)?;
        }
        if let Command::Tool(ToolAction::SetEvents { events: rows, .. }) = &command {
            ensure!(
                rows.len() <= bri_world::MAX_EVENTS_PER_BRICK,
                "Brick exceeds the native {}-event admission limit",
                bri_world::MAX_EVENTS_PER_BRICK
            );
            self.validate_event_rows(rows)?;
        }
        if !self.is_administrator(owner)
            && let Command::Tool(ToolAction::SetEvents { events: rows, .. }) = &mut command
        {
            events::clamp_relay_delays(rows);
        }
        self.tutorial_check(&command)?;
        let peer = self.peers.get_mut(&owner).context("Unknown connection")?;
        if let Some(aim) = aim {
            aim.validate()?;
        }
        let direction = aim.map_or_else(|| peer.player.state().forward(), ActionAim::direction);
        if tick.saturating_sub(self.bulk_window_tick) >= 120 {
            self.bulk_window_tick = tick;
            self.bulk_requests = 0;
            self.save_requests = 0;
        }
        // Loads (administrators) and saves (anyone) have separate budgets, and
        // a player who is not an administrator may take only one of the
        // shared save slots, so players can neither starve the administrator
        // nor each other. An administrator saving again at once (save, load,
        // save over it) is the host's own work.
        if matches!(command, Command::LoadBuild { .. }) {
            ensure!(
                peer.actor.administrator,
                "Only the host/administrator may load builds"
            );
            self.bulk_requests = self.bulk_requests.saturating_add(1);
            ensure!(
                self.bulk_requests <= 4,
                "Build load rate exceeded; retry shortly"
            );
        }
        if matches!(command, Command::SaveBuild { .. }) {
            ensure!(
                peer.actor.administrator || peer.saves == 0,
                "Build save rate exceeded; retry shortly"
            );
            ensure!(
                self.save_requests < 4,
                "Build save rate exceeded; retry shortly"
            );
            peer.saves += 1;
            self.save_requests += 1;
        }
        match command {
            Command::Admin(_) => unreachable!("handled by the authenticated admin branch above"),
            Command::DropTool { slot } => {
                self.drop_tool(owner, slot, direction)?;
                Ok(Reply::Accepted)
            }
            // Riders fire their own tools (`Player::processTick` hands fire
            // to the rider), except in a gun seat, where fire shoots the
            // mount's gun and puts tools away.
            Command::WeaponTrigger { down } if self.vehicles.weapon_seat(owner) => {
                // An image mid-fire stays up, as `unmountImage` waits on
                // `allowImageChange`.
                if down {
                    let _ = self.equip_tool(owner, None);
                }
                self.vehicles.set_fire(owner, down);
                Ok(Reply::Accepted)
            }
            Command::WeaponTrigger { down } => {
                ensure!(!down || peer.combat.alive, "Dead players cannot fire");
                self.weapon_trigger(owner, down, direction, aim.is_some())?;
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
                // Every v20 emote command checks `isObject(%client.player)`
                // and quietly does nothing without one; the brick selector
                // sends `/bsd` whenever it opens, dead or alive.
                if !peer.combat.alive {
                    return Ok(Reply::Accepted);
                }
                if name == "sit" {
                    peer.sitting = true;
                }
                let feet = peer.player.state().feet;
                let eye = if peer.player.state().crouched {
                    peer.player.eye()
                } else {
                    Vec3::from(feet) + Vec3::Y * V20_EYE_NODE
                };
                match name.as_str() {
                    // `serverCmdAlarm`: an AlarmProjectile at the eye point,
                    // `initialVelocity = "0 0 1"`, exploding on death.
                    "alarm" => {
                        let _ = self.weapons.spawn(
                            "v20.projectile.alarmprojectile",
                            bri_weapons::ActorId(owner),
                            eye,
                            Vec3::Y,
                            1.0,
                        );
                    }
                    // `serverCmdBSD`: BSDProjectile (lifetime 10 ms) explodes
                    // where it spawns, so its BSDExplosion plays at the eye.
                    "bsd" => self.cues.emit(
                        tick,
                        crate::presentation::CueKind::WeaponEffect {
                            source: bri_weapons::TargetId::Actor(bri_weapons::ActorId(owner)),
                            definition: "BSDExplosion".into(),
                            node: String::new(),
                            seconds: 0.0,
                            image: None,
                            hand: None,
                            direction: None,
                            scale: 1.0,
                        },
                        eye.to_array(),
                    ),
                    _ => {}
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
                if self.chat_filtered(owner, &text) {
                    return Ok(Reply::Accepted);
                }
                self.start_talking(tick, owner, text.len());
                self.team_chat(owner, &name, &text)?;
                Ok(Reply::Accepted)
            }
            Command::DropPlayerAtCamera(view) => {
                self.drop_player_at_camera(owner, view)?;
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
            Command::GhostBrick(ghost) => {
                self.set_ghost_brick(owner, ghost)?;
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
            Command::SaveBuild { events, ownership } => Ok(Reply::Saved(Box::new(
                bri_world::build::SavedBuild::capture(&self.saved_world(), events, ownership)?,
            ))),
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
            Command::SteeringPrefs {
                strafe,
                auto_return,
            } => {
                self.set_steering_prefs(owner, strafe, auto_return);
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
            Command::SetName(name) => {
                self.rename(owner, &name)?;
                Ok(Reply::Accepted)
            }
            Command::Plant {
                definition,
                position,
                quarter_turns,
                color,
            } => {
                combat::ensure_may_build(
                    &peer.combat,
                    &self.minigames,
                    bri_minigames::BuildAction::Build,
                )?;
                let default_print = self
                    .tool_catalog
                    .brick_print_aspects
                    .get(&definition)
                    .and_then(|aspect| {
                        self.last_prints
                            .get(&owner)
                            .and_then(|last| last.get(&aspect.to_ascii_lowercase()))
                            .or(self.tool_catalog.default_print.as_ref())
                    })
                    .cloned();
                let mut brick = Brick::new(ContentRef::Resolved(definition), position, owner);
                brick.quarter_turns = quarter_turns;
                brick.color = color;
                brick.print = default_print.map(ContentRef::Resolved);
                // `$Pref::Server::RandomBrickColor`: a brick takes its temp
                // brick's colour, which each plant sets to one of six of the
                // palette's first eight (`getRandom(5)` in
                // `serverCmdPlantBrick`); the first takes the builder's paint.
                let random = self.admin.settings.random_brick_color;
                if random && let Some(temp) = peer.random_color {
                    brick.color = temp;
                }
                // `ServerCmdPlantBrick`: the server's brick limit, then the
                // plant rate for non-administrators, then TooFarDistance.
                let settings = &self.admin.settings;
                if self.simulation.state().bricks.len() >= settings.brick_limit as usize
                    || (!peer.actor.administrator && peer.plants >= settings.bricks_per_second)
                {
                    return Err(crate::simulation::PlantFailure::Limit.into());
                }
                let builder = Builder {
                    actor: &peer.actor,
                    position: Vec3::from(peer.player.state().feet),
                    reach: settings.too_far_distance.clamp(0.0, 100.0),
                };
                let id = self.simulation.plant(&builder, brick)?;
                if let Some(peer) = self.peers.get_mut(&owner) {
                    peer.plants = peer.plants.saturating_add(1);
                }
                let palette = self.simulation.state().palette.len();
                let choices: Vec<u8> = [0, 1, 3, 4, 5, 7]
                    .into_iter()
                    .filter(|&c| usize::from(c) < palette)
                    .collect();
                if random && !choices.is_empty() {
                    self.spawn_seed = self
                        .spawn_seed
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407);
                    let next = choices[(self.spawn_seed >> 33) as usize % choices.len()];
                    if let Some(peer) = self.peers.get_mut(&owner) {
                        peer.random_color = Some(next);
                    }
                    self.notify(owner, Notice::TempBrickColor(next));
                }
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
                if let Some(peer) = self.peers.get_mut(&owner) {
                    peer.random_color = None;
                }
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
                if self.teleport_lockout(owner, admin_players::TELEPORT_PICKUP_LOCK_MS, true) {
                    return Ok(Reply::Activated(None));
                }
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
            Command::PlaceBlueprint {
                position,
                quarter_turns,
            } => self.place_blueprint(owner, position, quarter_turns),
            Command::Package(request) => self.package_command(owner, request, direction),
            Command::Chat(text) => {
                peer.chats = peer.chats.saturating_add(1);
                ensure!(peer.chats <= 4, "Chat rate exceeded");
                ensure!(
                    !text.trim().is_empty()
                        && text.len() <= 256
                        && !text.chars().any(char::is_control),
                    "Invalid chat message"
                );
                // `$Pref::Server::MaxChatLen` cuts a long message.
                let text: String = text
                    .chars()
                    .take(self.admin.settings.max_chat_length as usize)
                    .collect();
                peer.talking = false;
                let name = peer.name.clone();
                if self.chat_filtered(owner, &text) {
                    return Ok(Reply::Accepted);
                }
                let next = self
                    .next_chat
                    .checked_add(1)
                    .context("Chat IDs exhausted")?;
                let text_len = text.len();
                self.chat.push_back(ChatLine {
                    id: self.next_chat,
                    owner,
                    name,
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
    /// `serverCmdMessageSent`'s E-Tard Filter (`$Pref::Server::ETardFilter`,
    /// on in v20): a line using one of `$Pref::Server::ETardList`'s words
    /// is not sent, and its sender is told why.
    fn chat_filtered(&mut self, owner: OwnerId, text: &str) -> bool {
        if !self.admin.settings.chat_filter || etard_word(text).is_none() {
            return false;
        }
        self.notify(
            owner,
            Notice::Chat("\u{E005}This is a civilized game.  Please use full words.".into()),
        );
        true
    }
    /// `WebCom_PostServer`'s Public Domain Timeout: a builder away that many
    /// minutes (`$Pref::Server::BrickPublicDomainTimeout`, off at -1) leaves
    /// their bricks to everyone, as full trust.
    pub(super) fn public_domain(&self, owner: OwnerId) -> bool {
        let minutes = self.admin.settings.public_domain_timeout_minutes;
        if minutes <= 0 || self.peers.contains_key(&owner) {
            return false;
        }
        let since = self.abandoned_at.get(&owner).copied().unwrap_or(0);
        let tick = self.simulation.state().tick;
        tick.saturating_sub(since) >= u64::try_from(minutes).unwrap_or(0) * 60 * 120
    }
    pub fn step(&mut self) -> Result<()> {
        let tick = self.simulation.state().tick;
        // Abandoned builds turn public on the minute (v20 checked every
        // five, with each server post).
        if self.admin.settings.public_domain_timeout_minutes > 0 && tick.is_multiple_of(60 * 120) {
            self.refresh_trust();
        }
        self.stop_talking(tick);
        // Each system contains its own failure: the rest of the tick still
        // runs and every failure is reported together at the end.
        let mut failures = Vec::new();
        let mut contain = |system: &str, result: Result<()>| {
            if let Err(error) = result {
                failures.push(format!("{system}: {error:#}"));
            }
        };
        contain("events", self.start_event_tick(tick + 1));
        contain("bots", self.step_bots());
        let mut touches = Vec::new();
        let mut impacts = Vec::new();
        let mut glass_hits = Vec::new();
        let mut driving = Vec::new();
        let mut triggers = Vec::new();
        // Moves of players driving a package entity, for `step_packages`.
        let mut entity_moves = Vec::new();
        let liquids = self.simulation.liquids();
        let mut riding = Vec::new();
        for (&owner, peer) in self.peers.iter_mut() {
            let on_vehicle = self.vehicles.is_mounted(owner);
            let on_player = self.riding.is_riding(owner);
            if !peer.combat.alive || on_vehicle || on_player {
                peer.sitting = false;
            }
            if on_vehicle || on_player {
                peer.input_budget = (peer.input_budget + 1.0).min(combat_input_burst());
                // Seated players drive; consume inputs without the walking motor.
                while let Some((sequence, input)) = peer.inputs.pop_front() {
                    peer.processed_move = sequence;
                    peer.last_input_tick = tick;
                    peer.input = peer.body_input(input);
                }
                if on_vehicle {
                    driving.push((owner, peer.input));
                } else {
                    riding.push((owner, peer.input));
                }
                peer.player.look(&peer.input);
                peer.player.hold(&mut self.simulation.physics);
                continue;
            }
            peer.input_budget = (peer.input_budget + 1.0).min(INPUT_BURST);
            // Normally consume one queued input. A large backlog (a burst
            // after a network stall) is drained a little faster, and a small
            // standing one is run off gently (`InputDrain`). An empty queue
            // holds the player briefly to absorb jitter; players who have not
            // sent input yet, or whose connection starved, run idle ticks so
            // they cannot hang mid-air.
            let extra = peer.input_drain.extra(peer.inputs.len());
            let runs = if peer.inputs.len() > INPUT_TARGET {
                3
            } else if !peer.inputs.is_empty()
                || peer.processed_move == 0
                || tick - peer.last_input_tick > INPUT_STARVED
            {
                1 + extra
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
                    if let ControlObject::Entity(entity) = peer.control {
                        entity_moves.push((entity, input));
                    }
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
                    // A tool that takes the jet button keeps it from jetting.
                    let tool_jet = self
                        .weapons
                        .image_state(bri_weapons::ActorId(owner), 0)
                        .is_some_and(|(image, _)| image.commands.jet.is_some());
                    crate::prediction::motor_input(
                        peer.tutorial.abilities().apply(peer.input),
                        tool_jet,
                    )
                } else {
                    MoveInput {
                        yaw: peer.input.yaw,
                        pitch: peer.input.pitch,
                        ..Default::default()
                    }
                };
                let before = Vec3::from(peer.player.state().feet);
                let motion =
                    match peer
                        .player
                        .step_in_water(&mut self.simulation.physics, input, &liquids)
                    {
                        Ok(motion) => motion,
                        Err(error) => {
                            contain("movement", Err(error));
                            continue;
                        }
                    };
                let state = peer.player.state();
                if Vec3::from(state.velocity).length() > 0.5 {
                    peer.sitting = false;
                }
                let height = crate::water::body_height(state, peer.player.tuning());
                let deepest = crate::water::deepest(&liquids, state.feet, height);
                let speed = Vec3::from(state.velocity).length();
                let moved = Vec3::from(state.feet) != before;
                if let Some(crossing) =
                    peer.water
                        .step(deepest.map_or(0.0, |(_, c)| c), speed, moved)
                {
                    // The splash sits on the surface at `pos.z + height * coverage`.
                    let surface = deepest.map_or(state.feet[1], |(i, _)| liquids[i].max[1]);
                    self.cues.emit(
                        tick,
                        crate::presentation::CueKind::Water {
                            actor: owner,
                            entered: crossing == crate::water::Crossing::Splash,
                            speed: speed.min(10000.0),
                        },
                        [state.feet[0], surface, state.feet[2]],
                    );
                }
                // Corpses also call `Armor::onImpact`, so they break glass too.
                if !motion.hits.is_empty() {
                    glass_hits.push((owner, motion.hits));
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
        let smashers = match self.smash_breakables(glass_hits) {
            Ok(smashers) => smashers,
            Err(error) => {
                contain("breakables", Err(error));
                Default::default()
            }
        };
        impacts.retain(|(owner, _)| !smashers.contains(owner));
        self.fire_touches(touches);
        for (owner, trigger, down) in triggers {
            // An Add-On tool's jet command (v20 `onTrigger` slot 4).
            if trigger == 4
                && down
                && let Some(command) = self
                    .weapons
                    .image_state(bri_weapons::ActorId(owner), 0)
                    .and_then(|(image, _)| image.commands.jet.clone())
            {
                self.addon_tool_fire(owner, &command);
            }
            // The sports balls' `onBallTrigger` alternate actions.
            if self.weapons.holds_ball(bri_weapons::ActorId(owner)) {
                let _ = self
                    .weapons
                    .sport_trigger(bri_weapons::ActorId(owner), trigger, down);
            }
        }
        for (owner, input) in driving {
            contain("vehicle input", self.vehicle_input(owner, input));
        }
        for (owner, input) in riding {
            contain("rider input", self.ride_input(owner, input));
        }
        // Riders move with where their mounts walked this tick.
        self.follow_player_mounts();
        self.drive_package_entities(entity_moves);
        contain("packages", self.step_packages());
        self.step_holds();
        contain("vehicles", self.vehicle_pre_step());
        contain("physics", self.simulation.step());
        contain("vehicles", self.vehicle_post_step());
        self.player_mount_contacts();
        contain("weapons", self.step_weapons());
        self.step_temp_colors();
        contain("items", self.step_items());
        contain("combat", self.step_combat(impacts));
        contain("breakables", self.step_breakables());
        contain("special bricks", self.step_specials());
        contain("highlights", self.step_highlights());
        contain("tutorial", self.step_tutorial());
        contain("build loading", self.step_build_load());
        let changed = self.dirty.clone();
        contain("events", self.step_events(&changed));
        contain("items", self.reconcile_items());
        ensure!(failures.is_empty(), "{}", failures.join("; "));
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

/// `chatFilter` with v20's `$Pref::Server::ETardList`: the first listed
/// word found in the line, spaced and with `.`, `?`, `!` and `/` read as
/// spaces.
fn etard_word(text: &str) -> Option<&'static str> {
    const LIST: [&str; 10] = [
        " u ", " r ", " ur ", " wat ", " wut ", " wuts ", " wit ", " dat ", " loel ", " y ",
    ];
    let lower = format!(" {} ", text.to_ascii_lowercase())
        .replace(".dat", "")
        .replace("/u/", "")
        .replace(['?', '!', '.', '/'], " ");
    LIST.into_iter().find(|w| lower.contains(w))
}

#[cfg(test)]
mod etard_tests {
    #[test]
    fn etard_filter_catches_v20s_words_only_as_words() {
        assert_eq!(super::etard_word("r u there?"), Some(" u "));
        assert_eq!(super::etard_word("wat."), Some(" wat "));
        assert_eq!(super::etard_word("you are there"), None);
        assert_eq!(super::etard_word("the map.dat file"), None);
    }
}

#[cfg(test)]
mod input_drain_tests {
    use super::InputDrain;

    /// Queue lengths at the start of each tick for `arrivals` inputs per
    /// tick, starting from `backlog`, consuming as the session does.
    fn run(backlog: usize, arrivals: impl Iterator<Item = usize>) -> Vec<usize> {
        let (mut drain, mut queued, mut seen) = (InputDrain::default(), backlog, Vec::new());
        for arriving in arrivals {
            queued += arriving;
            seen.push(queued);
            let runs = if queued > 0 {
                1 + drain.extra(queued)
            } else {
                drain.extra(0)
            };
            queued -= runs.min(queued);
        }
        seen
    }

    #[test]
    fn a_standing_backlog_drains_back_to_one_queued_input() {
        let seen = run(5, std::iter::repeat_n(1, 240));
        // It stays for the first window, then drains within the next.
        assert!(seen[..60].iter().all(|q| *q == 6), "{seen:?}");
        assert!(seen[120..].iter().all(|q| *q == 2), "{seen:?}");
        // Never below what arrives, so the player never waits on input.
        assert!(seen.iter().all(|q| *q >= 1));
    }

    #[test]
    fn jitter_that_empties_the_queue_is_left_alone() {
        // Two inputs every other tick: the queue touches empty each pair.
        let seen = run(0, (0..240).map(|t| if t % 2 == 0 { 2 } else { 0 }));
        assert!(seen.iter().all(|q| *q <= 2), "{seen:?}");
    }
}
