use bri_sim::{
    player::{MoveInput, PlayerState},
    session::{CameraView, ChatLine, Command, Reply, Session},
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
pub const VERSION: u32 = 45;
/// Inputs repeated in every movement datagram so isolated losses cost nothing.
pub const MOVEMENT_REDUNDANCY: usize = 6;
/// Most inputs one frame may hand the transport (split across datagrams).
pub const MAX_MOVEMENT_BATCH: usize = 48;
/// Unreliable datagram payload bound. QUIC's minimum 1200-byte path MTU less
/// packet and frame overhead still carries it, so an encodable datagram is
/// always sendable.
pub const MAX_DATAGRAM: usize = 1100;
/// Server ticks between unreliable pose broadcasts (40 Hz at 120 Hz).
pub const POSE_INTERVAL: u64 = 3;
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
impl Hello {
    pub fn validate_bounds(&self) -> anyhow::Result<()> {
        anyhow::ensure!(self.version == VERSION, "Incompatible protocol version");
        anyhow::ensure!(
            !self.name.trim().is_empty()
                && self.name.len() <= 48
                && !self.name.chars().any(char::is_control),
            "Invalid player name"
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
    // Binds the claimed package set into the signed join context.
    transcript.extend_from_slice(&<sha2::Sha256 as sha2::Digest>::digest(
        rmp_serde::to_vec(&hello.packages)?,
    ));
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
/// Unreliable state datagrams from the host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Datagram {
    Pose(Pose),
    Vehicle(bri_sim::session::VehiclePose),
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
    /// Scene nodes of map shapes players have smashed.
    pub broken_shapes: BTreeSet<u32>,
    /// v20's player datablocks, then the enabled packages' archetypes.
    /// Poses name a player's archetype by its index here.
    pub archetypes: bri_sim::archetype::Archetypes,
    /// Entities of enabled packages.
    pub entities: Vec<bri_sim::session::EntityInfo>,
    /// Enabled packages' state as this client sees it: keys visible to
    /// everyone, plus its own owner-visible keys in a welcome.
    pub package_state: bri_sim::session::PackageStateView,
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
            archetypes: session.archetypes().clone(),
            world_bricks: world.bricks.len() as u64,
            entities: session.package_entities(),
            package_state: session.package_state(),
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
/// A checkpoint message (Welcome or MapChanged) and the bricks that follow it.
pub struct WorldTransfer {
    pub head: Message,
    pub bricks: bri_world::Bricks,
}
impl WorldTransfer {
    /// Encode the head and its chunks, dropping private source records.
    /// Linear in the world: run it off the authority loop.
    pub fn encode(self) -> anyhow::Result<Vec<Vec<u8>>> {
        let mut frames = vec![crate::codec::encode(&self.head)?];
        let mut chunk = Vec::with_capacity(WORLD_CHUNK);
        let mut bytes = 0;
        for (id, brick) in &self.bricks {
            let brick = public_brick(brick);
            let size = brick.stored_bound();
            if !chunk.is_empty() && (chunk.len() == WORLD_CHUNK || bytes + size > WORLD_CHUNK_BYTES)
            {
                frames.push(crate::codec::encode(&Message::WorldChunk(std::mem::take(
                    &mut chunk,
                )))?);
                bytes = 0;
            }
            bytes += size;
            chunk.push((*id, brick));
        }
        if !chunk.is_empty() {
            frames.push(crate::codec::encode(&Message::WorldChunk(chunk))?);
        }
        Ok(frames)
    }
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
                && checkpoint.world_bricks <= bri_world::MAX_BRICKS as u64,
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
}
pub fn poses(session: &Session) -> Vec<Pose> {
    session
        .motion_states()
        .into_iter()
        .map(|(player, acknowledged_input)| Pose {
            tick: session.simulation().state().tick,
            acknowledged_input,
            player,
        })
        .collect()
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delta {
    pub weapons: Option<bri_sim::session::WeaponView>,
    pub tools: Option<BTreeMap<OwnerId, bri_sim::session::ToolInventory>>,
    pub cues: Vec<bri_sim::presentation::Cue>,
    pub dropped_cues: u64,
    pub base: u64,
    pub cursor: u64,
    pub tick: u64,
    pub bricks: BTreeMap<BrickId, Option<Brick>>,
    pub names: Option<BTreeMap<OwnerId, String>>,
    pub avatars: Option<BTreeMap<OwnerId, bri_content::avatar::Appearance>>,
    pub palette: Option<Vec<[f32; 4]>>,
    pub chat: Vec<ChatLine>,
    pub vitals: Option<BTreeMap<OwnerId, bri_sim::session::Vitals>>,
    pub minigames: Option<Vec<bri_sim::session::MiniGameView>>,
    pub vehicles: Option<Vec<bri_sim::session::VehicleInfo>>,
    pub time_scale: Option<f32>,
    pub broken_shapes: Option<BTreeSet<u32>>,
    /// Package entities, when any moved or changed.
    pub entities: Option<Vec<bri_sim::session::EntityInfo>>,
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
    WorldChunk(Vec<(BrickId, Brick)>),
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
