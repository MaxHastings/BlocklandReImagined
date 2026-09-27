use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Id {
    pub index: u64,
    pub generation: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Class {
    Brick,
    Player,
    Client,
    Projectile,
    MiniGame,
    Vehicle,
}
impl Class {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "fxdtsbrick" => Some(Self::Brick),
            "player" => Some(Self::Player),
            "gameconnection" => Some(Self::Client),
            "projectile" => Some(Self::Projectile),
            "minigame" => Some(Self::MiniGame),
            "vehicle" => Some(Self::Vehicle),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Entity {
    pub class: Class,
    pub id: Id,
}
impl Entity {
    pub fn brick(id: Id) -> Self {
        Self {
            class: Class::Brick,
            id,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Slot {
    SelfBrick,
    Player,
    Client,
    Projectile,
    Bot,
    Driver,
    MiniGame,
    Ball,
}
impl Slot {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "self" => Some(Self::SelfBrick),
            "player" => Some(Self::Player),
            "client" => Some(Self::Client),
            "projectile" => Some(Self::Projectile),
            "bot" => Some(Self::Bot),
            "driver" => Some(Self::Driver),
            "minigame" => Some(Self::MiniGame),
            "ball" => Some(Self::Ball),
            _ => None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Target {
    Slot(Slot),
    Named(String),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Int(i64),
    Float(f32),
    Bool(bool),
    Text(String),
    Datablock(Option<String>),
    Vector(Vec3),
    Color(u8),
    Rows(RowSelection),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum RowSelection {
    All,
    Indices(Vec<u16>),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreservedRow {
    pub original: String,
    pub diagnostic: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Row {
    #[serde(default)]
    pub preserved: Option<PreservedRow>,
    pub enabled: bool,
    pub input: String,
    pub delay_ms: u32,
    pub target: Target,
    pub output: String,
    pub params: Vec<Value>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrickProgram {
    pub id: Id,
    pub owner_scope: u64,
    pub name: Option<String>,
    pub rows: Vec<Row>,
    pub print_count: u8,
    pub implicit_cancel_relays: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trigger {
    pub source: Id,
    pub input: String,
    pub origin: u64,
    pub client: Option<Entity>,
    pub targets: BTreeMap<Slot, Entity>,
}
impl Trigger {
    pub fn new(source: Id, input: impl Into<String>, origin: u64) -> Self {
        Self {
            source,
            input: input.into(),
            origin,
            client: None,
            targets: BTreeMap::new(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Up,
    Down,
    North,
    East,
    South,
    West,
}
impl Direction {
    pub fn from_index(i: i64) -> Self {
        match i {
            0 => Self::Up,
            1 => Self::Down,
            2 => Self::North,
            3 => Self::East,
            4 => Self::South,
            _ => Self::West,
        }
    }
    pub fn vector(self) -> Vec3 {
        match self {
            Self::Up => Vec3::Y,
            Self::Down => Vec3::NEG_Y,
            Self::North => Vec3::NEG_Z,
            Self::East => Vec3::X,
            Self::South => Vec3::Z,
            Self::West => Vec3::NEG_X,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum BrickOp {
    Color(u8),
    ColorFx(u8),
    ShapeFx(u8),
    Colliding(bool),
    Rendering(bool),
    RayCasting(bool),
    Presence {
        rendering: bool,
        ray_casting: bool,
        colliding: bool,
        revive_fake_dead: bool,
    },
    Disappear {
        seconds: i32,
    },
    FakeKill {
        velocity: Vec3,
        seconds: u32,
    },
    Respawn,
    Emitter(Option<String>),
    EmitterDirection(Direction),
    Light(Option<String>),
    Item(Option<String>),
    ItemDirection(Direction),
    ItemPosition(Direction),
    Music(Option<String>),
    PlaySound(Option<String>),
    SpawnItem {
        velocity: Vec3,
        item: Option<String>,
    },
    SpawnProjectile {
        velocity: Vec3,
        projectile: Option<String>,
        variance: Vec3,
        scale: f32,
    },
    SpawnExplosion {
        projectile: Option<String>,
        scale: f32,
    },
    Vehicle(Option<String>),
    RespawnVehicle,
    RecoverVehicle,
    RadiusImpulse {
        radius: f32,
        force: f32,
        vertical_force: f32,
    },
    PrintDigit(u8),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PlayerOp {
    Kill,
    Burn {
        seconds: u32,
    },
    ClearBurn,
    SetVelocity(Vec3),
    AddVelocity(Vec3),
    Scale(f32),
    AddHealth(i32),
    SetHealth(u32),
    DataBlock(Option<String>),
    Dismount,
    SpawnProjectile {
        speed: f32,
        projectile: Option<String>,
        variance: Vec3,
        scale: f32,
    },
    SpawnExplosion {
        projectile: Option<String>,
        scale: f32,
    },
    ClearTools,
    InstantRespawn,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageKind {
    Chat,
    Center,
    Bottom,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ClientOp {
    Message {
        kind: MessageKind,
        text: String,
        seconds: u32,
    },
    IncScore(i64),
    PlaySound(Option<String>),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MiniGameOp {
    Message {
        kind: MessageKind,
        text: String,
        seconds: u32,
    },
    Reset,
    RespawnAll,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ProjectileOp {
    Explode,
    Delete,
    Bounce(f32),
    Redirect { vector: Vec3, normalized: bool },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Intent {
    Brick(BrickOp),
    Player(PlayerOp),
    Client(ClientOp),
    MiniGame(MiniGameOp),
    Projectile(ProjectileOp),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) enum Action {
    Reappear(u64),
    Intent(Intent),
    Relay(Option<Direction>),
    Cancel,
    SetEnabled(RowSelection, bool),
    Toggle(RowSelection),
    Print { delta: i8, set: Option<u8> },
}
#[derive(Clone, Debug)]
pub struct Dispatch {
    pub source: Id,
    pub target: Entity,
    pub origin: u64,
    pub client: Option<Entity>,
    pub input: String,
    pub row: u16,
    pub output: String,
    pub scheduled_us: u64,
    pub now_us: u64,
    pub intent: Intent,
}
/// Deferred must have NO side effects. Applied mutations must be visible synchronously.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Apply {
    Applied,
    Deferred(String),
    Rejected(String),
}
/// Trusted server-internal adapter, not a public mod or network API.
pub trait Host {
    fn alive(&self, entity: Entity) -> bool;
    fn permitted(&self, context: &Trigger, target: Entity, output: &str) -> bool;
    fn relay_neighbors(
        &mut self,
        brick: Id,
        direction: Direction,
        limit: usize,
    ) -> Result<Vec<Id>, String>;
    fn apply(&mut self, dispatch: &Dispatch) -> Apply;
}
