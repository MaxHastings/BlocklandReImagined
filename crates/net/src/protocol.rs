use bri_sim::{
    player::{MoveInput, PlayerState},
    session::{ChatLine, Command, Reply, Session},
};
use bri_world::{Brick, BrickId, OwnerId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub const VERSION: u32 = 24;
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
    pub content_id: String,
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
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JoinBegin {
    pub version: u32,
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
        anyhow::ensure!(
            !self.content_id.is_empty()
                && self.content_id.len() <= 128
                && !self.content_id.chars().any(char::is_control),
            "Invalid content identity"
        );
        if let Some(proof) = &self.identity {
            anyhow::ensure!(proof.signature.len() == 64, "Invalid identity signature length");
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
    append_text(&mut transcript, &hello.content_id)?;
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
        Ok(())
    }
}
/// Unreliable state datagrams from the host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Datagram {
    Pose(Pose),
    Vehicle(bri_sim::session::VehiclePose),
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
            world_bricks: world.bricks.len() as u64,
        };
        (checkpoint, world.bricks.clone())
    }
}
/// Bricks per `WorldChunk` frame. A world of any size streams as bounded
/// frames after its checkpoint instead of one monolithic message.
pub const WORLD_CHUNK: usize = 4096;
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
        for (id, brick) in &self.bricks {
            chunk.push((*id, public_brick(brick)));
            if chunk.len() == WORLD_CHUNK {
                frames.push(crate::codec::encode(&Message::WorldChunk(std::mem::take(
                    &mut chunk,
                )))?);
            }
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
        let remaining =
            self.checkpoint.world_bricks - self.checkpoint.world.bricks.len() as u64;
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
}
#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
    Challenge {
        nonce: [u8; 32],
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
    /// Addressed to this client only (minigame chat, prints, invitations).
    Notice(bri_sim::session::Notice),
    Reply {
        sequence: u64,
        result: Result<Reply, bri_sim::session::Rejection>,
    },
    Rejected(String),
}
