use bri_sim::{
    player::{MoveInput, PlayerState},
    session::{CameraView, ChatLine, Command, Reply, SeatSince, Session},
};
use bri_world::{Brick, BrickId, OwnerId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
/// 34: `Challenge` carries the server `Listing` (join list and reachability probes).
/// 35: player archetypes, control targets, block looks, per-viewer package
/// state (`PackageState`), typed package refusals and package downloads.
/// 36: `Vitals::sitting`, the sit emote as replicated state.
/// 37: admin camera `Orb` datagrams, the camera view in movement datagrams
/// and the `Teleport` cue.
/// 38: `bsd` and `hug` emote cues, which older clients reject as invalid.
/// 39: `VehiclePose::wheel_contact` for the tire emitters.
/// 40: the host's Server Settings in the admin snapshot.
/// 41: `Command::GhostBrick` and `Vitals::ghost`, so others see a ghost brick.
/// 42: copied builds (`Notice::Blueprint`, `Command::PlaceBlueprint`) and
/// Add-On tool images (`Image::command`).
/// 43: `Notice::Bottom::hide_bar` and `Projectile::heading` (a stuck
/// arrow's direction).
/// 44: `Command::SteeringPrefs`, v20's strafe and auto-return steering.
/// 45: `Notice::TempBrickColor`, Random Brick Color's next colour.
/// 46: compact, batched state datagrams; other players' poses as
/// `RemotePose`; still items sent only to settle and keep alive; empty world
/// updates at 10 Hz with absent fields left out; per-player vitals, tools and
/// avatars; `WeaponDelta` with coasted projectiles; `EntityDelta`.
/// 47: `Notice::MusicTracks`, the host's Music Files for a joiner's wrench.
/// 48: `PlayerState::tick`: players move on v20's 32 ms ticks.
/// 49: `Vitals::ride` and `Archetype::mount_points`: players ride
/// rideable players.
/// 50: `Hello::accept_differences`: a join after downloading the server's
/// Add-Ons is let in without what it could not get.
/// 51: `Command::SetName`: a rename applies live.
/// 52: admin ranks (`/admin`, `/superAdmin`, `/deAdmin`) in the admin
/// messages and the Player List.
/// 53: `CueKind::BrickKill::cause`: tool kills hop and fall like v20.
/// 54: bricks in world chunks and updates travel packed (`crate::wire`);
/// `Request::upload` carries a `LoadBuild`'s bricks packed; large requests
/// may be zstd compressed (`codec::COMPRESSED`); `Checkpoint::world_chunks`.
/// 55: the Tutorial's targets (`Checkpoint::targets`, `Delta::targets`) and
/// `TargetId::Shape` in weapon cues.
/// 56: `Hello::clan` and `Command::SetClan`: clan prefix and suffix from the Avatar screen.
/// 57: `TargetId::Entity` in weapon cues, `EntityInfo::scale`, and weapon
/// packs' own sounds in the weapons content identity.
/// 58: `Checkpoint::world_near_chunks`: world transfers go nearest first
/// and a joiner plays once the nearby chunks are in.
/// 59: `CueKind::Beam` and `Notice::Fov` for the modding script API.
/// 60: client-predicted vehicles: vehicle updates carry the state to reconcile against.
/// 61: a passenger's turn is sent relative to their seat.
/// 62: predicted horses, rowboats, cannons and turrets; passengers turn in any seat.
/// 63: vehicle poses carry the driver's steering prefs (Tank mouse or A/D).
/// 64: v20 jump timing (bunny hops keep speed); client and host must predict alike.
/// 65: vehicle poses carry tyre state (v20 spring-and-slip tyres).
/// 66: vitals carry spawn and death ticks and the own pose its tick state, so death and respawn draw on the pose timeline.
/// 67: `Checkpoint::map_lights` and `Delta::map_lights`: Add-Ons switch,
/// dim and recolour map lights (`set_map_lights`).
///     Also mirrored copies (`PlaceBlueprint::mirrored`, `Notice::MirrorCopy`) and Add-On selection boxes (`Notice::SelectionBox`).
/// 68: `Command::CancelBrick`: the cancel key reaches the host, for Add-On
/// images that take it (`commands.cancel`).
/// 69: `Checkpoint::environment` and `Delta::environment`: the live
/// environment (Admin Menu Environment, `set_environment`).
/// Later changes are one file each in `crates/net/protocol-changes/` (see its
/// README); each one adds one to the version.
pub const VERSION: u32 = FROZEN_VERSION + PROTOCOL_CHANGES.len() as u32;
/// The last hand-numbered version, before per-change files.
const FROZEN_VERSION: u32 = 69;
include!(concat!(env!("OUT_DIR"), "/protocol_changes.rs"));
/// Inputs repeated in every movement datagram so isolated losses cost nothing.
pub const MOVEMENT_REDUNDANCY: usize = 6;
/// Most inputs one frame may hand the transport (split across datagrams).
pub const MAX_MOVEMENT_BATCH: usize = 48;
/// Least time between two movement datagrams (about 70 Hz): a client
/// drawing faster than 60 frames a second holds a frame's inputs for the
/// next datagram instead of doubling its upload.
pub const MOVEMENT_GAP: std::time::Duration = std::time::Duration::from_millis(14);
/// Join a held movement batch with the next one (each the newest sequence
/// and its consecutive inputs, oldest first), keeping every input the newer
/// batch does not repeat, up to [`MAX_MOVEMENT_BATCH`].
pub fn merge_movement(
    older: (u64, Vec<MoveInput>),
    newer: (u64, Vec<MoveInput>),
) -> (u64, Vec<MoveInput>) {
    let ((old_newest, old), (newest, new)) = (older, newer);
    let old_first = (old_newest + 1).saturating_sub(old.len() as u64);
    let new_first = (newest + 1).saturating_sub(new.len() as u64);
    if newest <= old_newest || new_first > old_newest + 1 {
        return (newest, new);
    }
    let keep = (new_first.saturating_sub(old_first) as usize).min(old.len());
    let mut inputs = old[..keep].to_vec();
    inputs.extend(new);
    let excess = inputs.len().saturating_sub(MAX_MOVEMENT_BATCH);
    inputs.drain(..excess);
    (newest, inputs)
}
/// Unreliable datagram payload bound. QUIC's minimum 1200-byte path MTU less
/// packet and frame overhead still carries it, so an encodable datagram is
/// always sendable.
pub const MAX_DATAGRAM: usize = 1100;
/// Server ticks between unreliable pose broadcasts (40 Hz at 120 Hz).
pub const POSE_INTERVAL: u64 = 3;
/// Server ticks between world updates (20 Hz).
pub const UPDATE_INTERVAL: u64 = 6;
/// Server ticks between world updates that carry nothing but the tick
/// (10 Hz), which clients' respawn countdowns and item fades read.
pub const HEARTBEAT_INTERVAL: u64 = 12;
#[derive(Clone, Serialize, Deserialize)]
pub struct ResumeToken(pub [u8; 32]);
impl std::fmt::Debug for ResumeToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ResumeToken([redacted])")
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hello {
    pub version: u32,
    pub name: String,
    /// Every shared and client package this client loaded, compared with the
    /// server's environment package by package.
    pub packages: Vec<bri_package::environment::PackageRef>,
    pub resume: Option<ResumeToken>,
    pub host: Option<ResumeToken>,
    pub identity: Option<IdentityProof>,
    /// Set on the join after downloading what the server offers: whatever
    /// shared content still differs could not be had, and the server lets
    /// the player in without it rather than refusing again.
    #[serde(default)]
    pub accept_differences: bool,
    /// `$Pref::Player::ClanPrefix` and `ClanSuffix`, which v20's
    /// `GameConnection::onConnectRequest` receives beside the name. The
    /// host cleans them like names (`Clan::cleaned`).
    #[serde(default)]
    pub clan: bri_sim::session::Clan,
}
/// The name a client joins as: its player name and clan tags.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JoinName {
    pub name: String,
    pub clan: bri_sim::session::Clan,
}
impl From<String> for JoinName {
    fn from(name: String) -> Self {
        Self {
            name,
            clan: Default::default(),
        }
    }
}
impl From<&str> for JoinName {
    fn from(name: &str) -> Self {
        name.to_string().into()
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityProof {
    pub public_key: [u8; 32],
    pub signature: Vec<u8>,
}
/// What the join list shows about a server. Public: sent before identity.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Listing {
    pub name: String,
    pub map: String,
    pub players: u32,
    pub max_players: u32,
}
impl Listing {
    /// Bounds a client applies before showing a listing.
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.name.len() <= 128
                && self.map.len() <= 256
                && self.players <= 64
                && self.max_players <= 64
                && !self.name.chars().any(char::is_control)
                && !self.map.chars().any(char::is_control),
            "Invalid server listing"
        );
        Ok(())
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JoinBegin {
    pub version: u32,
    /// Defaulted so an older client still decodes and hears the version
    /// refusal.
    #[serde(default)]
    pub purpose: Purpose,
}
/// What a new connection is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Purpose {
    /// Identity challenge, Hello, then the game.
    #[default]
    Join,
    /// Fetch packages this client lacks; no identity, no game state.
    Download,
}
impl JoinBegin {
    pub fn join() -> Self {
        Self {
            version: VERSION,
            purpose: Purpose::Join,
        }
    }
}
/// Largest object range one download request may ask for.
pub const MAX_OBJECT_CHUNK: u32 = 1024 * 1024;
/// Requests of a download connection (`Purpose::Download`), answered in
/// order with one [`DownloadReply`] each.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum DownloadRequest {
    /// The shared and client packages the server loads.
    Environment,
    /// The file listing of one offered package, by package hash.
    Listing { hash: String },
    /// Bytes of one file of an offered package, by file hash.
    Object {
        sha256: String,
        offset: u64,
        length: u32,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DownloadReply {
    Environment(Vec<bri_package::environment::PackageRef>),
    Listing(Box<bri_package::sync::Listing>),
    Object(#[serde(with = "serde_bytes")] Vec<u8>),
    Refused(String),
}
/// Longest name a Hello may carry, in bytes. Names are shortened to
/// `bri_sim::session::MAX_PLAYER_NAME` on joining.
pub const MAX_HELLO_NAME: usize = 1024;
impl Hello {
    pub fn validate_bounds(&self) -> anyhow::Result<()> {
        anyhow::ensure!(self.version == VERSION, "Incompatible protocol version");
        // The host cleans and shortens the name (`clean_player_name`); only
        // a name no client would send is refused.
        anyhow::ensure!(self.name.len() <= MAX_HELLO_NAME, "Invalid player name");
        anyhow::ensure!(
            self.clan.prefix.len() <= MAX_HELLO_NAME && self.clan.suffix.len() <= MAX_HELLO_NAME,
            "Invalid clan tags"
        );
        bri_package::environment::Environment::validate_refs(&self.packages)
            .map_err(|e| anyhow::anyhow!("Invalid package list: {e}"))?;
        if let Some(proof) = &self.identity {
            anyhow::ensure!(
                proof.signature.len() == 64,
                "Invalid identity signature length"
            );
        }
        Ok(())
    }
}
/// Unambiguous connection proof covering the server challenge, pinned server
/// certificate, join context, and optional host/resume capabilities.
pub fn identity_transcript(
    hello: &Hello,
    challenge: &[u8; 32],
    server_fingerprint: &[u8; 32],
) -> anyhow::Result<Vec<u8>> {
    hello.validate_bounds()?;
    let mut transcript = b"BRI-QUIC-CLIENT-IDENTITY\0v1".to_vec();
    transcript.extend_from_slice(&hello.version.to_be_bytes());
    transcript.extend_from_slice(challenge);
    transcript.extend_from_slice(server_fingerprint);
    append_text(&mut transcript, &hello.name)?;
    append_text(&mut transcript, &hello.clan.prefix)?;
    append_text(&mut transcript, &hello.clan.suffix)?;
    // Binds the claimed package set into the signed join context.
    transcript.extend_from_slice(&<sha2::Sha256 as sha2::Digest>::digest(rmp_serde::to_vec(
        &hello.packages,
    )?));
    append_token(&mut transcript, hello.resume.as_ref());
    append_token(&mut transcript, hello.host.as_ref());
    Ok(transcript)
}
fn append_text(out: &mut Vec<u8>, text: &str) -> anyhow::Result<()> {
    let bytes = text.as_bytes();
    let len = u16::try_from(bytes.len())?;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}
fn append_token(out: &mut Vec<u8>, token: Option<&ResumeToken>) {
    match token {
        Some(token) => {
            out.push(1);
            out.extend_from_slice(&token.0);
        }
        None => out.push(0),
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub sequence: u64,
    pub command: Command,
    pub aim: Option<bri_sim::session::ActionAim>,
    /// A `LoadBuild`'s bricks, packed; its build travels without them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upload: Option<Box<crate::wire::Upload>>,
}
impl Request {
    pub fn new(sequence: u64, command: Command, aim: Option<bri_sim::session::ActionAim>) -> Self {
        let mut request = Self {
            sequence,
            command,
            aim,
            upload: None,
        };
        if let Command::LoadBuild { build, .. } = &mut request.command {
            request.upload = Some(Box::new(crate::wire::Upload::take(build)));
        }
        request
    }
    /// Put an uploaded build's bricks back into its command.
    pub fn restore(&mut self) -> anyhow::Result<()> {
        match (self.upload.take(), &mut self.command) {
            (Some(upload), Command::LoadBuild { build, .. }) => upload.restore(build),
            (Some(_), _) => anyhow::bail!("Uploaded bricks without a build"),
            (None, _) => Ok(()),
        }
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Movement {
    pub version: u32,
    /// Sequence of the last input; earlier entries count down from it.
    pub newest: u64,
    /// Consecutive prediction-tick inputs, oldest first.
    pub inputs: Vec<MoveInput>,
    /// The camera the client flies or orbits while one has control.
    pub camera: Option<CameraView>,
    /// The seat these moves are made for, and from which move on; `None` on
    /// foot. The host reads each move by the seat it was made for.
    pub seat: Option<SeatSince>,
}
impl Movement {
    /// Callers validate first; the arithmetic cannot overflow for any
    /// `newest`, including a hostile `u64::MAX`.
    pub fn sequenced(&self) -> impl Iterator<Item = (u64, MoveInput)> + '_ {
        let first = self
            .newest
            .saturating_sub((self.inputs.len() as u64).saturating_sub(1));
        self.inputs
            .iter()
            .enumerate()
            .map(move |(i, input)| (first + i as u64, *input))
    }
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.version == VERSION
                && !self.inputs.is_empty()
                && self.inputs.len() <= MOVEMENT_REDUNDANCY
                && self.newest >= self.inputs.len() as u64,
            "Invalid movement datagram"
        );
        if let Some(camera) = &self.camera {
            camera.validate()?;
        }
        Ok(())
    }
}
/// Items of the unreliable state datagrams from the host. A datagram is an
/// array of them ([`crate::codec::pack_datagrams`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
/// Variants travel as one-letter names: every item carries its tag.
pub enum Datagram {
    /// This client's own pose: what its prediction reconciles with.
    #[serde(rename = "p")]
    Pose(Pose),
    /// Another player's pose.
    #[serde(rename = "r")]
    Remote(RemotePose),
    #[serde(rename = "v")]
    Vehicle(bri_sim::session::VehiclePose),
    #[serde(rename = "o")]
    Orb(Orb),
}
/// Where an admin's free camera is: its `cameraImage` orb, seen by others.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Orb {
    pub tick: u64,
    pub owner: OwnerId,
    pub eye: [f32; 3],
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pose {
    pub tick: u64,
    pub acknowledged_input: u64,
    pub player: PlayerState,
    /// The body this pose moves (`Vitals::spawn_tick`). Poses and vitals
    /// arrive on separate streams, so a client's own respawned body can be
    /// drawn before its vitals say it lives. Remote poses leave it 0: they
    /// are drawn behind the vitals and read the body from the tick timeline.
    pub spawn_tick: u64,
}
/// Another player's pose: what drawing them needs, without the state only
/// their own prediction uses (jump timers, jet energy, the input they were
/// acknowledged up to). Quantized well below what anyone can see: position
/// to a centimetre, velocity to a centimetre a second, look angles to a
/// ten-thousandth of a radian. MessagePack writes small integers short, so
/// a position within 327 units of the origin costs 3 bytes an axis instead
/// of a float's 5; the flags share a byte and the usual scale is left out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemotePose {
    pub tick: u64,
    pub owner: OwnerId,
    /// Centimetres.
    pub feet: [i32; 3],
    /// Centimetres per second.
    pub velocity: [i16; 3],
    /// Yaw, pitch and head turn in ten-thousandths of a radian.
    pub look: [i16; 3],
    /// [`RemotePose::GROUNDED`], [`RemotePose::CROUCHED`], [`RemotePose::JETTING`].
    pub flags: u8,
    pub archetype: bri_sim::archetype::ArchetypeId,
    /// None for the normal size.
    pub scale: Option<f32>,
}
const CENTIMETRES: f32 = 100.0;
const LOOK_UNITS: f32 = 10_000.0;
fn quantize(value: f32, scale: f32) -> i16 {
    (value * scale)
        .round()
        .clamp(i16::MIN as f32, i16::MAX as f32) as i16
}
impl RemotePose {
    pub const GROUNDED: u8 = 1;
    pub const CROUCHED: u8 = 2;
    pub const JETTING: u8 = 4;
    pub fn of(tick: u64, p: &PlayerState) -> Self {
        let flag = |on: bool, bit: u8| if on { bit } else { 0 };
        Self {
            tick,
            owner: p.owner,
            // Saturates; positions are bounded by the world far inside an
            // i32 of centimetres.
            feet: p.feet.map(|x| (x * CENTIMETRES).round() as i32),
            velocity: p.velocity.map(|v| quantize(v, CENTIMETRES)),
            look: [p.yaw, p.pitch, p.head_yaw].map(|a| quantize(a, LOOK_UNITS)),
            flags: flag(p.grounded, Self::GROUNDED)
                | flag(p.crouched, Self::CROUCHED)
                | flag(p.jetting, Self::JETTING),
            archetype: p.archetype,
            scale: (p.scale != 1.0).then_some(p.scale),
        }
    }
    /// As a pose, with the owner-only state at its defaults.
    pub fn into_pose(self) -> Pose {
        let [yaw, pitch, head_yaw] = self.look.map(|a| f32::from(a) / LOOK_UNITS);
        Pose {
            tick: self.tick,
            acknowledged_input: 0,
            spawn_tick: 0,
            player: PlayerState {
                owner: self.owner,
                feet: self.feet.map(|x| x as f32 / CENTIMETRES),
                velocity: self.velocity.map(|v| f32::from(v) / CENTIMETRES),
                yaw,
                pitch,
                head_yaw,
                grounded: self.flags & Self::GROUNDED != 0,
                crouched: self.flags & Self::CROUCHED != 0,
                jetting: self.flags & Self::JETTING != 0,
                jump: Default::default(),
                archetype: self.archetype,
                scale: self.scale.unwrap_or(1.0),
                energy: bri_sim::player::PlayerTuning::default().max_energy,
                tick: Default::default(),
                tether: None,
            },
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PublicWorld {
    pub name: String,
    pub map_id: String,
    pub palette: Vec<[f32; 4]>,
    pub bricks: bri_world::Bricks,
}
pub fn public_brick(brick: &Brick) -> Brick {
    let mut brick = brick.clone();
    brick.source_records.clear();
    brick
}
/// [`Brick::stored_bound`] of [`public_brick`], without the copy.
fn public_size(brick: &Brick) -> u64 {
    if brick.source_records.is_empty() {
        brick.stored_bound()
    } else {
        public_brick(brick).stored_bound()
    }
}
/// The replicated view of a world's bricks: an O(1) snapshot of the
/// persistent map, copying only the bricks that carry private source records.
pub fn public_bricks(bricks: &bri_world::Bricks) -> bri_world::Bricks {
    let mut public = bricks.clone();
    for (id, brick) in bricks {
        if !brick.source_records.is_empty()
            && let Some(brick) = public.get_mut(id)
        {
            brick.source_records.clear();
        }
    }
    public
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub weapons: bri_sim::session::WeaponView,
    pub tools: BTreeMap<OwnerId, bri_sim::session::ToolInventory>,
    pub cue_cursor: u64,
    pub dropped_cues: u64,
    pub cursor: u64,
    pub tick: u64,
    pub world: PublicWorld,
    pub names: BTreeMap<OwnerId, String>,
    pub avatars: BTreeMap<OwnerId, bri_content::avatar::Appearance>,
    pub chat: Vec<ChatLine>,
    pub poses: Vec<Pose>,
    pub vitals: BTreeMap<OwnerId, bri_sim::session::Vitals>,
    pub minigames: Vec<bri_sim::session::MiniGameView>,
    pub vehicles: Vec<bri_sim::session::VehicleInfo>,
    pub vehicle_poses: Vec<bri_sim::session::VehiclePose>,
    /// Admin `/timeScale`.
    pub time_scale: f32,
    /// Bricks that stream after this checkpoint as `WorldChunk` frames;
    /// `world.bricks` itself travels empty.
    pub world_bricks: u64,
    /// How many `WorldChunk` frames carry them, so a client can read them
    /// all and decode them in parallel.
    #[serde(default)]
    pub world_chunks: u64,
    /// How many of those a joiner waits for before playing: the bricks
    /// around them. The rest stream in afterwards.
    #[serde(default)]
    pub world_near_chunks: u64,
    /// Scene nodes of map shapes players have smashed.
    pub broken_shapes: BTreeSet<u32>,
    /// The Tutorial's targets on the range.
    #[serde(default)]
    pub targets: Vec<bri_sim::tutorial::TargetView>,
    /// Add-On map light rules, oldest first.
    #[serde(default)]
    pub map_lights: Vec<bri_sim::session::MapLightRule>,
    /// The live environment over the map's own.
    #[serde(default)]
    pub environment: bri_content::atmosphere::Settings,
    /// v20's player datablocks, then the enabled packages' archetypes.
    /// Poses name a player's archetype by its index here.
    pub archetypes: bri_sim::archetype::Archetypes,
    /// Entities of enabled packages.
    pub entities: Vec<bri_sim::session::EntityInfo>,
    /// Enabled packages' state as this client sees it: keys visible to
    /// everyone, plus its own owner-visible keys in a welcome.
    pub package_state: bri_sim::session::PackageStateView,
    /// How fast each falling projectile drops per tick, by definition, for
    /// coasting projectiles between updates.
    #[serde(default)]
    pub projectile_falls: BTreeMap<String, f32>,
    /// Add-On world shapes (`show_shapes`), by key.
    #[serde(default)]
    pub world_shapes: BTreeMap<String, Vec<bri_package_runtime::ops::WorldShape>>,
}
impl Checkpoint {
    /// Everything but the bricks, plus an O(1) snapshot of the authoritative
    /// bricks to stream after it (see [`WorldTransfer`]). Cheap enough for the
    /// authority loop at any world size.
    pub fn from_session(session: &Session, cursor: u64) -> (Self, bri_world::Bricks) {
        let world = session.simulation().state();
        let checkpoint = Self {
            weapons: session.weapon_view(),
            tools: session.tool_inventories(),
            cue_cursor: session.cue_cursor(),
            dropped_cues: session.dropped_cues(),
            cursor,
            tick: world.tick,
            world: PublicWorld {
                name: world.name.clone(),
                map_id: world.map_id.clone(),
                palette: world.palette.clone(),
                bricks: bri_world::Bricks::new(),
            },
            names: session.names(),
            avatars: session.avatars(),
            chat: session.chat(),
            poses: poses(session),
            vitals: session.vitals(),
            minigames: session.minigame_views(),
            vehicles: session.vehicle_infos(),
            vehicle_poses: session.vehicle_poses(),
            time_scale: session.time_scale(),
            broken_shapes: session.broken_shapes(),
            targets: session.tutorial_targets(),
            map_lights: session.map_light_rules(),
            environment: session.environment(),
            archetypes: session.archetypes().clone(),
            world_bricks: world.bricks.len() as u64,
            world_chunks: 0,
            world_near_chunks: 0,
            entities: session.package_entities(),
            package_state: session.package_state(),
            projectile_falls: session.projectile_falls(),
            world_shapes: session
                .world_shapes()
                .into_iter()
                .map(|(k, s)| (k, s.to_vec()))
                .collect(),
        };
        (checkpoint, world.bricks.clone())
    }
}
/// Most bricks per `WorldChunk` frame. A world of any size streams as bounded
/// frames after its checkpoint instead of one monolithic message.
pub const WORLD_CHUNK: usize = 4096;
/// Most [`bri_world::Brick::stored_bound`] bytes per `WorldChunk`: well inside
/// a frame however heavy each brick is, since a count alone does not bound
/// bytes (a few thousand event-laden bricks are gigabytes).
pub const WORLD_CHUNK_BYTES: u64 = 8 * 1024 * 1024;
/// Bricks within this distance of a joiner arrive before they can play;
/// the rest stream in while they do.
pub const NEAR_RADIUS: f32 = 64.0;
/// Edge of the neighbourhoods a world transfer orders by distance.
pub const NEAR_CELL: f32 = 16.0;
/// Most bricks a joiner waits for before playing, however dense the build
/// around them.
pub const NEAR_MOST: usize = 50_000;
/// A checkpoint message (Welcome or MapChanged) and the bricks that follow it.
pub struct WorldTransfer {
    pub head: Message,
    pub bricks: bri_world::Bricks,
    /// Where the receiver stands: bricks go nearest first, and the head says
    /// how many chunks hold the ones within [`NEAR_RADIUS`]. Without a focus
    /// the whole world arrives before play.
    pub focus: Option<[f32; 3]>,
}
impl WorldTransfer {
    /// Encode the head and its chunks, dropping private source records.
    /// Linear in the world: run it off the authority loop. Chunks encode on
    /// several threads, a few at a time so a huge world is never copied
    /// whole.
    pub fn encode(self) -> anyhow::Result<Vec<Vec<u8>>> {
        let mut frames = Vec::new();
        self.encode_each(|frame| frames.push(frame))?;
        Ok(frames)
    }
    /// [`Self::encode`], handing each frame on as soon as it is encoded:
    /// the head and the bricks around the receiver go out while the rest
    /// of a large world is still encoding.
    pub fn encode_each(mut self, mut emit: impl FnMut(Vec<u8>)) -> anyhow::Result<()> {
        let mut order: Vec<(BrickId, &Brick)> = self.bricks.iter().map(|(id, b)| (*id, b)).collect();
        let mut near = order.len();
        if let Some(focus) = self.focus {
            // Nearest neighbourhood first, each neighbourhood's bricks in id
            // order: builds stay together, so chunks compress as well as in
            // plain id order.
            let cell = |b: &Brick| -> (u32, [i32; 3]) {
                let key = std::array::from_fn(|a| (b.position[a] / NEAR_CELL).floor() as i32);
                let centre = |a: usize| (key[a] as f32 + 0.5) * NEAR_CELL - focus[a];
                let distance = (0..3).map(|a| centre(a).powi(2)).sum::<f32>().sqrt();
                ((distance / NEAR_CELL) as u32, key)
            };
            // (distance ring, neighbourhood key)
            type Cell = (u32, [i32; 3]);
            let mut keyed: Vec<(Cell, BrickId, &Brick)> =
                order.iter().map(|(id, b)| (cell(b), *id, *b)).collect();
            keyed.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
            // Every neighbourhood reaching within NEAR_RADIUS.
            let rings = (NEAR_RADIUS / NEAR_CELL) as u32 + 1;
            near = keyed
                .partition_point(|(key, _, _)| key.0 <= rings)
                .min(NEAR_MOST);
            order = keyed.into_iter().map(|(_, id, b)| (id, b)).collect();
        }
        // Chunk sizes first, so the head can say how many chunks follow.
        let mut sizes = Vec::new();
        let (mut count, mut bytes) = (0, 0);
        let mut near_chunks = 0;
        for (i, (_, brick)) in order.iter().enumerate() {
            let size = public_size(brick);
            if count > 0 && (count == WORLD_CHUNK || bytes + size > WORLD_CHUNK_BYTES) {
                sizes.push(count);
                (count, bytes) = (0, 0);
            }
            if i < near {
                near_chunks = sizes.len() + 1;
            }
            count += 1;
            bytes += size;
        }
        if count > 0 {
            sizes.push(count);
        }
        match &mut self.head {
            Message::Welcome { checkpoint, .. } | Message::MapChanged(checkpoint) => {
                checkpoint.world_chunks = sizes.len() as u64;
                checkpoint.world_near_chunks = near_chunks as u64;
            }
            _ => {}
        }
        emit(crate::codec::encode(&self.head)?);
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get().min(8));
        let mut bricks = order.into_iter();
        for wave in sizes.chunks(threads) {
            let chunks = wave
                .iter()
                .map(|n| {
                    bricks
                        .by_ref()
                        .take(*n)
                        .map(|(id, brick)| (id, public_brick(brick)))
                        .collect()
                })
                .collect();
            for frame in encode_chunks(chunks)? {
                emit(frame);
            }
        }
        Ok(())
    }
}
/// `WorldChunk` frames for `chunks`, in order, one thread each.
fn encode_chunks(chunks: Vec<Vec<(BrickId, Brick)>>) -> anyhow::Result<Vec<Vec<u8>>> {
    let encode = |chunk: Vec<(BrickId, Brick)>| crate::codec::encode(&Message::WorldChunk(chunk));
    if chunks.len() <= 1 {
        return chunks.into_iter().map(encode).collect();
    }
    std::thread::scope(|scope| {
        let workers: Vec<_> = chunks
            .into_iter()
            .map(|chunk| scope.spawn(move || encode(chunk)))
            .collect();
        workers
            .into_iter()
            .map(|w| {
                w.join()
                    .map_err(|_| anyhow::anyhow!("World chunk encoder panicked"))?
            })
            .collect()
    })
}
/// Client side of a [`WorldTransfer`]: fills a checkpoint's world from the
/// chunks that follow it, exactly as many bricks as it announced.
pub struct WorldAssembly {
    checkpoint: Checkpoint,
}
impl WorldAssembly {
    pub fn new(checkpoint: Checkpoint) -> anyhow::Result<Self> {
        anyhow::ensure!(
            checkpoint.world.bricks.is_empty()
                && checkpoint.world_bricks <= bri_world::MAX_BRICKS as u64
                && checkpoint.world_chunks <= checkpoint.world_bricks
                && checkpoint.world_near_chunks <= checkpoint.world_chunks
                && (checkpoint.world_chunks > 0) == (checkpoint.world_bricks > 0),
            "Invalid world transfer"
        );
        Ok(Self { checkpoint })
    }
    pub fn complete(&self) -> bool {
        self.checkpoint.world.bricks.len() as u64 == self.checkpoint.world_bricks
    }
    pub fn add(&mut self, chunk: Vec<(BrickId, Brick)>) -> anyhow::Result<()> {
        let remaining = self.checkpoint.world_bricks - self.checkpoint.world.bricks.len() as u64;
        anyhow::ensure!(
            !chunk.is_empty() && chunk.len() <= WORLD_CHUNK && chunk.len() as u64 <= remaining,
            "Invalid world chunk"
        );
        for (id, brick) in chunk {
            anyhow::ensure!(
                self.checkpoint.world.bricks.insert(id, brick).is_none(),
                "Duplicate brick in world transfer"
            );
        }
        Ok(())
    }
    pub fn finish(self) -> anyhow::Result<Checkpoint> {
        anyhow::ensure!(self.complete(), "Incomplete world transfer");
        Ok(self.checkpoint)
    }
    /// The checkpoint with the bricks so far, after `chunks` chunks, and
    /// what is still to come.
    pub fn split(self, chunks: u64) -> anyhow::Result<(Checkpoint, WorldRest)> {
        let checkpoint = self.checkpoint;
        let rest = WorldRest {
            bricks: checkpoint.world_bricks - checkpoint.world.bricks.len() as u64,
            chunks: checkpoint
                .world_chunks
                .checked_sub(chunks)
                .ok_or_else(|| anyhow::anyhow!("Invalid world transfer"))?,
        };
        anyhow::ensure!(
            (rest.bricks > 0) == (rest.chunks > 0) && rest.chunks <= rest.bricks,
            "Invalid world transfer"
        );
        Ok((checkpoint, rest))
    }
}
/// The chunks of a world transfer still to come after a joiner started
/// playing.
#[derive(Debug)]
pub struct WorldRest {
    bricks: u64,
    chunks: u64,
}
impl WorldRest {
    pub fn done(&self) -> bool {
        self.chunks == 0
    }
    /// Add the next chunk to `world`, returning its brick ids.
    pub fn add(
        &mut self,
        world: &mut PublicWorld,
        chunk: Vec<(BrickId, Brick)>,
    ) -> anyhow::Result<Vec<BrickId>> {
        let last = self.chunks == 1;
        anyhow::ensure!(
            self.chunks > 0
                && !chunk.is_empty()
                && chunk.len() <= WORLD_CHUNK
                && chunk.len() as u64 <= self.bricks
                && (!last || chunk.len() as u64 == self.bricks),
            "Invalid world chunk"
        );
        let mut ids = Vec::with_capacity(chunk.len());
        for (id, brick) in chunk {
            anyhow::ensure!(id > 0, "Invalid brick identity");
            brick.validate(world.palette.len())?;
            anyhow::ensure!(
                world.bricks.insert(id, brick).is_none(),
                "Duplicate brick in world transfer"
            );
            ids.push(id);
        }
        self.bricks -= ids.len() as u64;
        self.chunks -= 1;
        Ok(ids)
    }
}
pub fn poses(session: &Session) -> Vec<Pose> {
    session
        .motion_states()
        .into_iter()
        .map(|(player, acknowledged_input)| Pose {
            tick: session.simulation().state().tick,
            acknowledged_input,
            spawn_tick: session.spawn_tick(player.owner).unwrap_or_default(),
            player,
        })
        .collect()
}
#[derive(Debug, Clone, Serialize, Deserialize)]
/// Fields with nothing to say are left out of the encoding.
pub struct Delta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weapons: Option<WeaponDelta>,
    /// Inventories that changed, by owner; players who left drop out with
    /// `names`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tools: BTreeMap<OwnerId, bri_sim::session::ToolInventory>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cues: Vec<bri_sim::presentation::Cue>,
    pub dropped_cues: u64,
    pub base: u64,
    pub cursor: u64,
    pub tick: u64,
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        with = "crate::wire::changes"
    )]
    pub bricks: BTreeMap<BrickId, Option<Brick>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub names: Option<BTreeMap<OwnerId, String>>,
    /// Appearances that changed, by owner.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub avatars: BTreeMap<OwnerId, bri_content::avatar::Appearance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette: Option<Vec<[f32; 4]>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chat: Vec<ChatLine>,
    /// Vitals that changed, by owner.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub vitals: BTreeMap<OwnerId, bri_sim::session::Vitals>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minigames: Option<Vec<bri_sim::session::MiniGameView>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vehicles: Option<Vec<bri_sim::session::VehicleInfo>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_scale: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub broken_shapes: Option<BTreeSet<u32>>,
    /// The Tutorial's targets, whole, whenever one launched, fell or left.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub targets: Option<Vec<bri_sim::tutorial::TargetView>>,
    /// Add-On map light rules, whole, whenever one changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map_lights: Option<Vec<bri_sim::session::MapLightRule>>,
    /// The live environment, whole, whenever it changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<bri_content::atmosphere::Settings>,
    /// Package entities that appeared, changed, moved or left.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entities: Option<EntityDelta>,
    /// World shape sets that changed, by key; an empty one is gone.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub world_shapes: BTreeMap<String, Vec<bri_package_runtime::ops::WorldShape>>,
}
impl Delta {
    /// Nothing changed but the tick (and the cursor).
    pub fn is_empty(&self) -> bool {
        let Self {
            weapons,
            tools,
            cues,
            dropped_cues: _,
            base: _,
            cursor: _,
            tick: _,
            bricks,
            names,
            avatars,
            palette,
            chat,
            vitals,
            minigames,
            vehicles,
            time_scale,
            broken_shapes,
            targets,
            map_lights,
            environment,
            entities,
            world_shapes,
        } = self;
        weapons.is_none()
            && tools.is_empty()
            && cues.is_empty()
            && bricks.is_empty()
            && names.is_none()
            && avatars.is_empty()
            && palette.is_none()
            && chat.is_empty()
            && vitals.is_empty()
            && minigames.is_none()
            && vehicles.is_none()
            && time_scale.is_none()
            && broken_shapes.is_none()
            && targets.is_none()
            && map_lights.is_none()
            && environment.is_none()
            && entities.is_none()
            && world_shapes.is_empty()
    }
}
/// The world shape sets that changed between `sent` and `now` (an empty
/// set for one taken away); `sent` becomes `now`. Sets are shared, so an
/// unchanged one compares by pointer.
pub fn changed_world_shapes(
    sent: &mut BTreeMap<String, std::sync::Arc<Vec<bri_package_runtime::ops::WorldShape>>>,
    now: BTreeMap<String, std::sync::Arc<Vec<bri_package_runtime::ops::WorldShape>>>,
) -> BTreeMap<String, Vec<bri_package_runtime::ops::WorldShape>> {
    let mut changed: BTreeMap<_, _> = sent
        .keys()
        .filter(|k| !now.contains_key(*k))
        .map(|k| (k.clone(), Vec::new()))
        .collect();
    for (key, set) in &now {
        if !sent.get(key).is_some_and(|s| std::sync::Arc::ptr_eq(s, set)) {
            changed.insert(key.clone(), set.to_vec());
        }
    }
    *sent = now;
    changed
}
/// What changed in the weapons view. Projectiles fly on every client by
/// [`bri_weapons::coast`]; the host only sends the ones that appeared or
/// left their coasted flight (a bounce, a stick, a hit).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WeaponDelta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub static_items: Option<Vec<bri_sim::item_spawners::StaticItem>>,
    /// Held images that changed, by owner; an empty list unmounts them.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub images: BTreeMap<OwnerId, Vec<bri_sim::session::MountedImage>>,
    /// Projectiles as the host has them now: new, or corrected.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub projectiles: Vec<bri_weapons::Projectile>,
    /// Projectiles that are gone.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drops: Option<Vec<bri_weapons::Drop>>,
}
/// What changed among package entities.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EntityDelta {
    /// New entities, and ones whose kind, model or label changed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changed: Vec<bri_sim::session::EntityInfo>,
    /// Entities that only moved or turned: id, position and yaw.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub moved: Vec<(u64, [f32; 3], f32)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<u64>,
}
impl EntityDelta {
    /// The changes from `last` to `current`; `last` becomes `current`.
    pub fn between(
        last: &mut BTreeMap<u64, bri_sim::session::EntityInfo>,
        current: Vec<bri_sim::session::EntityInfo>,
    ) -> Option<Self> {
        Self::between_joined(last, &[], current)
    }
    /// [`Self::between`] when players joined since `last` holding `joined`
    /// (their checkpoints' entities): the update brings them in line too.
    pub fn between_joined(
        last: &mut BTreeMap<u64, bri_sim::session::EntityInfo>,
        joined: &[Vec<bri_sim::session::EntityInfo>],
        current: Vec<bri_sim::session::EntityInfo>,
    ) -> Option<Self> {
        let mut delta = Self::default();
        let current: BTreeMap<u64, _> = current.into_iter().map(|e| (e.id, e)).collect();
        // What each client holds: the last update's entities, or a joiner's.
        let held: Vec<BTreeMap<u64, &bri_sim::session::EntityInfo>> =
            std::iter::once(last.iter().map(|(id, e)| (*id, e)).collect())
                .chain(
                    joined
                        .iter()
                        .map(|view| view.iter().map(|e| (e.id, e)).collect()),
                )
                .collect();
        for (id, e) in &current {
            let olds = || held.iter().map(|h| h.get(id).copied());
            if olds().all(|old| old == Some(e)) {
                continue;
            }
            if olds().all(|old| {
                old.is_some_and(|old| {
                    old.kind == e.kind && old.model == e.model && old.label == e.label
                })
            }) {
                delta.moved.push((*id, e.position, e.yaw));
            } else {
                delta.changed.push(e.clone());
            }
        }
        let ids: BTreeSet<u64> = held.iter().flat_map(|h| h.keys().copied()).collect();
        drop(held);
        delta.removed = ids
            .into_iter()
            .filter(|id| !current.contains_key(id))
            .collect();
        *last = current;
        (delta != Self::default()).then_some(delta)
    }
    pub fn apply(
        &self,
        entities: &mut BTreeMap<u64, bri_sim::session::EntityInfo>,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.changed.len() <= 1024 && self.moved.len() <= 1024 && self.removed.len() <= 1024,
            "Too many package entity changes"
        );
        for (id, position, yaw) in &self.moved {
            let e = entities
                .get_mut(id)
                .ok_or_else(|| anyhow::anyhow!("Unknown package entity moved"))?;
            e.position = *position;
            e.yaw = *yaw;
            e.validate()?;
        }
        for e in &self.changed {
            e.validate()?;
            entities.insert(e.id, e.clone());
        }
        for id in &self.removed {
            entities.remove(id);
        }
        anyhow::ensure!(entities.len() <= 1024, "Too many package entities");
        Ok(())
    }
}
/// Most ticks one update coasts projectiles: a host that stalls longer has
/// removed or corrected them by the time it sends again.
pub const MAX_COAST_TICKS: u64 = 1200;
/// Coast every projectile of `view` over `ticks`, as clients and the host's
/// record of them both do between updates.
pub fn coast_projectiles(
    view: &mut bri_sim::session::WeaponView,
    falls: &BTreeMap<String, f32>,
    ticks: u64,
) {
    for p in &mut view.projectiles {
        let fall = falls.get(&p.definition).copied().unwrap_or(0.0);
        for _ in 0..ticks.min(MAX_COAST_TICKS) {
            bri_weapons::coast(p, fall);
        }
    }
}
impl WeaponDelta {
    /// Most players one update may change the held images of.
    pub const MAX_IMAGES: usize = 64;
    /// Cut this update down to what [`WeaponDelta::apply`] accepts. Returns
    /// whether anything was left out (the caller sends it later).
    pub fn clamp_to_wire_limits(&mut self) -> bool {
        let mut deferred = false;
        while self.images.len() > Self::MAX_IMAGES {
            self.images.pop_last();
            deferred = true;
        }
        for list_len in [self.projectiles.len(), self.removed.len()] {
            deferred |= list_len > bri_weapons::MAX_PROJECTILES;
        }
        self.projectiles.truncate(bri_weapons::MAX_PROJECTILES);
        self.removed.truncate(bri_weapons::MAX_PROJECTILES);
        deferred
    }
    /// Apply to a view already coasted to this update's tick.
    pub fn apply(&self, view: &mut bri_sim::session::WeaponView) -> anyhow::Result<()> {
        let mut ids = BTreeSet::new();
        anyhow::ensure!(
            self.projectiles.len() <= bri_weapons::MAX_PROJECTILES
                && self.removed.len() <= bri_weapons::MAX_PROJECTILES
                && self.images.len() <= Self::MAX_IMAGES
                && self.projectiles.iter().all(|p| ids.insert(p.id)),
            "Invalid weapons update"
        );
        if let Some(items) = &self.static_items {
            view.static_items = items.clone();
        }
        for (owner, images) in &self.images {
            if images.is_empty() {
                view.images.remove(owner);
            } else {
                view.images.insert(*owner, images.clone());
            }
        }
        let removed: BTreeSet<_> = self.removed.iter().collect();
        view.projectiles
            .retain(|p| !ids.contains(&p.id) && !removed.contains(&p.id));
        view.projectiles.extend(self.projectiles.iter().cloned());
        view.projectiles.sort_by_key(|p| p.id);
        if let Some(drops) = &self.drops {
            view.drops = drops.clone();
        }
        Ok(())
    }
}
#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
    /// First answer to a `JoinBegin`. The listing lets a probe (the join
    /// list, the host's reachability check) learn about the server over the
    /// game port and stop here.
    Challenge {
        nonce: [u8; 32],
        listing: Listing,
    },
    Welcome {
        owner: OwnerId,
        administrator: bool,
        resume: ResumeToken,
        checkpoint: Checkpoint,
    },
    Update(Delta),
    /// The host changed maps: the full state of the new mission. Its bricks
    /// follow as `WorldChunk` frames, like a Welcome's.
    MapChanged(Checkpoint),
    /// Up to `WORLD_CHUNK` bricks of the checkpoint sent just before.
    WorldChunk(#[serde(with = "crate::wire::bricks")] Vec<(BrickId, Brick)>),
    AdminSnapshot(bri_sim::session::AdminSnapshot),
    /// Package state as this client sees it (`Session::package_state_for`):
    /// keys visible to everyone plus its own owner-visible keys. Sent to
    /// each client when its view changes, so one player's private keys
    /// never reach another.
    PackageState(bri_sim::session::PackageStateView),
    /// Addressed to this client only (minigame chat, prints, invitations).
    Notice(bri_sim::session::Notice),
    Reply {
        sequence: u64,
        result: Result<Reply, bri_sim::session::Rejection>,
    },
    Rejected(String),
    /// The join was refused because these shared packages differ. A client
    /// can fetch what it lacks from the server and join again
    /// ([`crate::client::Client::connect_fetching`]).
    PackagesDiffer(Vec<bri_package::environment::Mismatch>),
}
