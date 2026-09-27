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
mod inventory;
mod items;
mod weapons;
pub use weapons::{MountedImage, WeaponView};
mod tools;
pub use admin::{
    AdminBrickGroup, AdminCall, AdminCapability, AdminData, AdminPlayer, AdminReply, AdminSnapshot,
};
pub use bri_world::authority::WrenchProperties;
pub use inventory::{TOOL_SLOTS, ToolInventory};
pub use tools::{InspectMode, ToolAction, ToolCatalog, UNDO_PLANT_LIMIT};

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
    Move(MoveInput),
    Plant {
        definition: String,
        position: [f32; 3],
        quarter_turns: u8,
        color: u8,
    },
    Edit {
        brick: BrickId,
        edit: Edit,
    },
    Remove {
        brick: BrickId,
    },
    Tool(ToolAction),
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
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatLine {
    pub id: u64,
    pub owner: OwnerId,
    pub name: String,
    pub text: String,
    pub tick: u64,
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
    input: MoveInput,
    last_sequence: u64,
    last_move_sequence: u64,
    last_input_tick: u64,
    window_tick: u64,
    moves: u32,
    actions: u32,
    chats: u32,
    inspection: Option<tools::Inspection>,
    avatar: Option<bri_content::avatar::Appearance>,
}
pub struct Session {
    item_spawners: crate::item_spawners::ItemSpawners,
    spawn_loadout: ToolInventory,
    weapons: bri_weapons::WeaponsWorld,
    weapon_triggers: BTreeMap<OwnerId, VecDeque<weapons::Trigger>>,
    weapon_gaps: BTreeMap<String, u64>,
    cues: crate::presentation::Cues,
    simulation: Simulation,
    peers: BTreeMap<OwnerId, Peer>,
    next_owner: OwnerId,
    chat: VecDeque<ChatLine>,
    next_chat: u64,
    departed: BTreeMap<OwnerId, (String, bool, Option<bri_content::avatar::Appearance>, Option<bri_admin::Principal>)>,
    dirty: BTreeSet<BrickId>,
    notices: VecDeque<String>,
    tool_catalog: ToolCatalog,
    plant_undo: BTreeMap<OwnerId, VecDeque<BrickId>>,
    avatar_catalog: Option<bri_content::avatar::Package>,
    ownership_scope: Option<String>,
    bulk_window_tick: u64,
    bulk_requests: u32,
    admin: admin::AdminRuntime,
    admin_disconnects: VecDeque<OwnerId>,
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
            item_spawners: Default::default(),
            spawn_loadout: ToolInventory::default(),
            weapon_triggers: BTreeMap::new(),
            weapon_gaps: BTreeMap::new(),
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
            plant_undo: BTreeMap::new(),
            avatar_catalog: None,
            ownership_scope: None,
            bulk_window_tick: 0,
            bulk_requests: 0,
            admin: admin::AdminRuntime::default(),
            admin_disconnects: VecDeque::new(),
        }
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
            .filter_map(|(id, p)| p.avatar.clone().map(|a| (*id, a)))
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
        ensure!(self.peers.len() < 64, "Server is full");
        ensure!(
            !name.trim().is_empty() && name.len() <= 48 && !name.chars().any(char::is_control),
            "Invalid player name"
        );
        let owner = self.next_owner;
        let next = owner.checked_add(1).context("Owner IDs exhausted")?;
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
        self.peers.insert(
            owner,
            Peer {
                player,
                actor: Actor {
                    owner,
                    administrator: role.is_admin(),
                },
                name: name.clone(),
                principal,
                input: MoveInput::default(),
                last_sequence: 0,
                last_move_sequence: 0,
                last_input_tick: self.simulation.state().tick,
                window_tick: self.simulation.state().tick,
                moves: 0,
                actions: 0,
                chats: 0,
                inspection: None,
                avatar: self.avatar_catalog.as_ref().map(|c| c.defaults.clone()),
            },
        );
        self.next_owner = next;
        Ok(owner)
    }
    pub fn disconnect(&mut self, owner: OwnerId) -> Result<()> {
        let peer = self.peers.remove(&owner).context("Unknown connection")?;
        self.admin_disconnect(owner);
        self.weapons.remove_actor(bri_weapons::ActorId(owner));
        self.weapon_triggers.remove(&owner);
        self.departed
            .insert(owner, (peer.name, peer.actor.administrator, peer.avatar, peer.principal));
        peer.player.despawn(&mut self.simulation.physics);
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
        ensure!(saved_principal == principal, "Resume identity does not match authenticated ticket");
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
        self.peers.insert(
            owner,
            Peer {
                player,
                actor: Actor {
                    owner,
                    administrator: role.is_admin(),
                },
                name: name.clone(),
                principal,
                input: MoveInput::default(),
                last_sequence: 0,
                last_move_sequence: 0,
                last_input_tick: self.simulation.state().tick,
                window_tick: self.simulation.state().tick,
                moves: 0,
                actions: 0,
                chats: 0,
                inspection: None,
                avatar,
            },
        );
        self.departed.remove(&owner);
        Ok(())
    }
    pub fn movement(&mut self, owner: OwnerId, sequence: u64, input: MoveInput) -> Result<()> {
        input.validate()?;
        let tick = self.simulation.state().tick;
        let peer = self.peers.get_mut(&owner).context("Unknown connection")?;
        ensure!(sequence > peer.last_move_sequence, "Stale movement");
        if tick - peer.window_tick >= 120 {
            peer.window_tick = tick;
            peer.moves = 0;
            peer.actions = 0;
            peer.chats = 0;
        }
        peer.moves = peer.moves.saturating_add(1);
        ensure!(peer.moves <= 240, "Movement command rate exceeded");
        peer.last_move_sequence = sequence;
        peer.input = input;
        peer.last_input_tick = tick;
        Ok(())
    }
    pub fn motion_states(&self) -> Vec<(PlayerState, u64)> {
        self.peers
            .values()
            .map(|p| (p.player.state().clone(), p.last_move_sequence))
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
                    peer.moves = 0;
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
        let tick = self.simulation.state().tick;
        let peer = self.peers.get_mut(&owner).context("Unknown connection")?;
        ensure!(sequence > peer.last_sequence, "Stale/replayed command");
        peer.last_sequence = sequence;
        if tick - peer.window_tick >= 120 {
            peer.window_tick = tick;
            peer.moves = 0;
            peer.actions = 0;
            peer.chats = 0;
        }
        if matches!(command, Command::Move(_)) {
            peer.moves = peer.moves.saturating_add(1);
            ensure!(peer.moves <= 240, "Movement command rate exceeded");
        } else {
            peer.actions = peer.actions.saturating_add(1);
            ensure!(peer.actions <= 60, "Action command rate exceeded");
        }
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
            Command::WeaponTrigger { down } => {
                self.weapon_trigger(owner, down, direction)?;
                Ok(Reply::Accepted)
            }
            Command::EquipTool { slot } => {
                self.equip_tool(owner, slot)?;
                Ok(Reply::Accepted)
            }
            Command::SaveBuild { events, ownership } => Ok(Reply::Saved(Box::new(
                bri_world::build::SavedBuild::capture(
                    self.simulation.state(),
                    self.ownership_scope.clone(),
                    events,
                    ownership,
                )?,
            ))),
            Command::LoadBuild { build, ownership } => {
                let plan = bri_world::build::LoadPlan::prepare(
                    self.simulation.state(),
                    *build,
                    owner,
                    ownership,
                    self.ownership_scope.as_deref(),
                    self.next_owner,
                )?;
                let next_owner = plan.next_owner;
                self.item_spawners
                    .validate_append(self.simulation.state(), plan.bricks())?;
                let ids = self.simulation.load_build(&peer.actor, plan)?;
                self.next_owner = next_owner;
                let count = ids.len();
                self.dirty.extend(ids);
                Ok(Reply::Loaded { bricks: count })
            }

            Command::Avatar(appearance) => {
                self.avatar_catalog
                    .as_ref()
                    .context("Avatar catalog is not installed")?
                    .resolve(&appearance)?;
                peer.avatar = Some(appearance);
                Ok(Reply::Accepted)
            }
            Command::Move(input) => {
                input.validate()?;
                peer.input = input;
                peer.last_input_tick = tick;
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
                self.dirty.insert(id);
                let undo = self.plant_undo.entry(owner).or_default();
                if undo.len() == UNDO_PLANT_LIMIT {
                    undo.pop_front();
                }
                undo.push_back(id);
                self.cues
                    .emit(tick, crate::presentation::CueKind::Plant, position);
                Ok(Reply::Planted(id))
            }
            Command::Edit { brick, edit } => {
                inventory::require_equipment(
                    &self.weapons,
                    owner,
                    inventory::edit_equipment(&edit),
                )?;
                let hit = self.simulation.target(peer.player.eye(), direction, 10.0)?;
                ensure!(
                    hit.is_some_and(|h| h.brick == Some(brick)),
                    "Tool target is out of reach or obstructed"
                );
                self.tool_catalog
                    .validate_edit(&self.simulation.state().bricks[&brick], &edit)?;
                self.item_spawners
                    .validate_edit(self.simulation.state(), brick, &edit)?;
                self.simulation.edit(&peer.actor, brick, edit)?;
                self.dirty.insert(brick);
                Ok(Reply::Accepted)
            }
            Command::Remove { brick } => {
                inventory::require_equipment(
                    &self.weapons,
                    owner,
                    Some(bri_weapons::CORE_TOOLS[0]),
                )?;
                // Stock hammer range is 5, extended to 5.5 when aiming nearly
                // straight down. Player scaling/muzzle offsets remain adapters.
                let hit = self.simulation.target(
                    peer.player.eye(),
                    direction,
                    if direction.y < -0.9 { 5.5 } else { 5.0 },
                )?;
                ensure!(
                    hit.is_some_and(|h| h.brick == Some(brick)),
                    "Tool target is out of reach or obstructed"
                );
                let position = self.simulation.state().bricks[&brick].position;
                self.simulation.remove(&peer.actor, brick)?;
                self.dirty.insert(brick);
                self.cues
                    .emit(tick, crate::presentation::CueKind::Break, position);
                Ok(Reply::Accepted)
            }
            Command::Activate => Ok(Reply::Activated(
                self.simulation.activate(peer.player.eye(), direction)?,
            )),
            Command::Tool(action) => self.tool_action(owner, action, direction),
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
                self.chat.push_back(ChatLine {
                    id: self.next_chat,
                    owner,
                    name: peer.name.clone(),
                    text,
                    tick,
                });
                self.next_chat = next;
                if self.chat.len() > 100 {
                    self.chat.pop_front();
                }
                Ok(Reply::Accepted)
            }
        }
    }
    pub fn step(&mut self) -> Result<()> {
        let tick = self.simulation.state().tick;
        let mut touches = Vec::new();
        for peer in self.peers.values_mut() {
            // Lost connections stop driving the player after half a second.
            let input = if tick - peer.last_input_tick > 60 {
                MoveInput {
                    yaw: peer.input.yaw,
                    pitch: peer.input.pitch,
                    ..Default::default()
                }
            } else {
                peer.input
            };
            let motion = peer.player.step_in_water(
                &mut self.simulation.physics,
                input,
                &self.simulation.waters,
            )?;
            touches.extend(motion.touched);
            if motion.jumped {
                self.cues.emit(
                    tick,
                    crate::presentation::CueKind::Jump,
                    peer.player.state().feet,
                );
            }
        }
        for id in touches {
            if self.simulation.state().bricks.contains_key(&id)
                && let Err(error) = self.simulation.touch(id)
            {
                if self.notices.len() == 64 {
                    self.notices.pop_front();
                }
                self.notices
                    .push_back(format!("Brick {id} touch event rejected: {error}"));
            }
        }
        self.dirty.extend(self.simulation.step()?);
        self.step_weapons()?;
        self.step_items()?;
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
