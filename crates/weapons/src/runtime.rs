//! Host-authoritative fixed-tick gameplay. Coordinates: X-right, Y-up, -Z-forward.
use crate::*;
use anyhow::{Result, ensure};
use bri_console::Clamp;
use bri_content::passage::{MAX_CARRIES, PAST};
use glam::{Quat, Vec3};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
pub const MAX_ACTORS: usize = 128;
pub const MAX_PROJECTILES: usize = 1024;
pub const MAX_DROPS: usize = 1024;
/// Ticks a dropped item stays in the world (10 s at 120 Hz).
pub const DROP_LIFETIME_TICKS: u64 = 1200;
/// Ticks before the dropper may pick an item back up: v20's 15 engine ticks
/// of 32 ms, rounded up at 120 Hz.
pub const DROP_PICKUP_DELAY_TICKS: u64 = 58;
pub const MAX_QUERY_TARGETS: usize = 128;
/// Explosions one tick may queue with [`WeaponsWorld::spawn_explosion`].
/// Each one queries and damages everything in its radius, so an event loop
/// that spawns them without pause cannot stall the host; the excess is
/// refused and the caller notes it.
pub const MAX_EXPLOSIONS_PER_TICK: usize = 8;
/// Core tool actions are implemented by the host's building authority. They
/// share inventory/drop rules with weapons but have no weapon state machine.
pub const HAMMER: &str = "v20.weapon.hammeritem";
pub const WRENCH: &str = "v20.weapon.wrenchitem";
pub const PRINTER: &str = "v20.weapon.printgun";
pub const WAND: &str = "v20.weapon.wanditem";
/// The core tools in their inventory order; name one by its constant.
pub const CORE_TOOLS: [&str; 4] = [HAMMER, WRENCH, PRINTER, WAND];
/// The building mechanism an image's `onFire` runs, if it is one of the
/// host's ([`HostTool`], [`OnFire::Tool`]).
pub fn host_tool(image: &Image) -> Option<HostTool> {
    match image.on_fire {
        Some(OnFire::Tool(tool)) => Some(tool),
        _ => None,
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ActorId(pub u64);
impl ActorId {
    /// No actor: the thrower of an item the world itself put down (the
    /// `spawnItem` event, an Add-On rule's `drop_item`). Owners start at 1.
    pub const NOBODY: Self = Self(0);
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TargetId {
    Actor(ActorId),
    Vehicle(u64),
    Brick(u64),
    Map(u64),
    /// A `StaticShape` a map's script spawned and moves (the Tutorial's
    /// targets), by the host's id for it.
    Shape(u64),
    /// A creature or object an Add-On spawned: shots and blasts hurt and
    /// push it like a player, and its package decides what that means.
    Entity(u64),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mount {
    None,
    Skis,
    Other,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frame {
    pub body_yaw: f32,
    pub position: Vec3,
    pub eye: Vec3,
    pub muzzle: [Vec3; 2],
    pub direction: Vec3,
    pub velocity: Vec3,
    pub scale: f32,
    pub grounded: bool,
    pub mount: Mount,
    pub horse: bool,
    pub first_person: bool,
    pub can_jet: bool,
    /// The body's middle, the point an opening of a linked brick carries
    /// it by. A shot from an eye or muzzle already through an opening the
    /// body is not yet through comes out of the far side, as it is seen.
    #[serde(default)]
    pub middle: Option<Vec3>,
}
impl Default for Frame {
    fn default() -> Self {
        Self {
            body_yaw: 0.,
            position: Vec3::ZERO,
            eye: Vec3::ZERO,
            muzzle: [Vec3::ZERO; 2],
            direction: Vec3::NEG_Z,
            velocity: Vec3::ZERO,
            scale: 1.0,
            grounded: true,
            mount: Mount::None,
            horse: false,
            first_person: true,
            can_jet: true,
            middle: None,
        }
    }
}
impl Frame {
    fn validate(&self) -> Result<()> {
        ensure!(
            [
                self.position,
                self.eye,
                self.direction,
                self.velocity,
                self.muzzle[0],
                self.muzzle[1],
                self.middle.unwrap_or(self.eye)
            ]
            .iter()
            .all(|v| v.is_finite() && v.abs().max_element() < 1e7),
            "Nonfinite/out of bounds actor frame"
        );
        ensure!(
            self.body_yaw.is_finite()
                && self.body_yaw.abs() <= std::f32::consts::PI
                && (0.01..=100.0).contains(&self.scale)
                && self.direction.length_squared() > 0.1
                && self.velocity.length() < 10000.0,
            "Invalid actor frame: yaw={}, scale={}, direction length squared={}, speed={}",
            self.body_yaw,
            self.scale,
            self.direction.length_squared(),
            self.velocity.length()
        );
        Ok(())
    }
}
#[derive(Debug, Clone)]
pub struct Hit {
    pub target: TargetId,
    pub position: Vec3,
    pub normal: Vec3,
    pub fraction: f32,
    pub color: Option<[f32; 3]>,
}
#[derive(Debug, Clone)]
pub struct Nearby {
    pub target: TargetId,
    pub center: Vec3,
    pub distance: f32,
}
#[derive(Debug, Clone, Copy)]
pub struct Filter {
    /// None for aim/tool queries; host uses age for source-collider grace on projectiles.
    pub projectile_age_ticks: Option<u32>,
    pub source: ActorId,
    pub players: bool,
    pub world_only: bool,
}
/// Collision context for synchronous native brick outputs (core Projectile::Bounce/Redirect).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectileContact {
    pub projectile: u64,
    pub definition: String,
    pub source: ActorId,
    pub target: TargetId,
    pub position: Vec3,
    pub velocity: Vec3,
    pub normal: Vec3,
    pub scale: f32,
    /// Palette index of a colour spray can's paint projectile.
    pub paint: Option<u8>,
}
#[derive(Debug, Clone, Copy)]
pub enum ContactResponse {
    Continue,
    Delete,
    /// `Projectile::Explode`: explode at the contact, armed or not.
    Explode,
    Bounce(f32),
    Redirect {
        vector: Vec3,
        normalized: bool,
    },
}
/// The liquid holding a box: how much of the box it covers and the
/// liquid's density and viscosity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Liquid {
    pub coverage: f32,
    pub density: f32,
    pub viscosity: f32,
}
/// `density` of every stock v20 `ItemData` (tools, weapons, keys, skis and
/// balls). `drag` is never set, so items feel no liquid drag.
pub const ITEM_DENSITY: f32 = 0.2;
/// Dropped items' mass: v20's item datablocks set `mass = 1` (inferred from
/// the stock weapon items; the PC's v20 audit can confirm).
pub const ITEM_MASS: f32 = 1.0;
/// `Item::mGravity`: how fast a dropped item falls, on the host and in the
/// clients' smoothing between its updates.
pub const ITEM_GRAVITY: f32 = 20.0;
/// Adapter must sweep the entire segment, including thin native map and brick colliders.
/// Radius results use closest bounds distance, deterministic target order, and the given cap.
/// Permissions and visibility are authoritative host decisions; no numeric ID grants access.
pub trait Query {
    /// Execute authorized zero-delay projectile brick outputs before default collision.
    fn on_contact(&mut self, _: &ProjectileContact) -> ContactResponse {
        ContactResponse::Continue
    }

    fn sweep(&mut self, start: Vec3, end: Vec3, filter: Filter) -> Option<Hit>;
    /// Sweep a box of `half` extents and `rotation` whose centre moves from
    /// `start` to `end`; the hit position is the box centre at contact.
    /// Adapters without shape casts sweep the box's lowest point instead.
    fn sweep_box(
        &mut self,
        start: Vec3,
        end: Vec3,
        half: Vec3,
        rotation: Quat,
        filter: Filter,
    ) -> Option<Hit> {
        let bottom = Vec3::Y * -ItemBounds::lowest(half, rotation);
        self.sweep(start - bottom, end - bottom, filter)
            .map(|hit| Hit {
                position: hit.position + bottom,
                ..hit
            })
    }
    fn radius(&mut self, center: Vec3, radius: f32, limit: usize) -> Vec<Nearby>;
    fn can_affect(&self, source: ActorId, target: TargetId) -> bool;
    /// Explosion splash; unlike a direct hit it also honours `selfDamage`.
    fn can_affect_radius(&self, source: ActorId, target: TargetId) -> bool {
        self.can_affect(source, target)
    }
    fn can_catch(&self, source: ActorId, target: ActorId) -> bool;
    /// Whether `target` is `source`'s teammate or ally in a mini-game with
    /// weapon damage on, whatever its friendly fire ([`crate::Aura::ally_damage`]).
    fn is_ally(&self, _source: ActorId, _target: TargetId) -> bool {
        false
    }
    /// The liquid covering most of the axis-aligned box standing on `bottom`
    /// and `height` tall, if any.
    fn liquid(&mut self, _bottom: Vec3, _height: f32) -> Option<Liquid> {
        None
    }
    /// The first opening of a linked brick (a portal) the move from `start`
    /// to `end` goes in through: how far along, and the rigid move that
    /// carries what went in to where it comes out.
    fn passage(&mut self, _start: Vec3, _end: Vec3) -> Option<(f32, glam::Affine3A)> {
        None
    }
}
/// Follow a path of points through the openings of linked bricks it goes
/// in through: where its last point ends up and the carries applied,
/// composed (None when it went through none).
fn follow(q: &mut impl Query, path: &[Vec3]) -> (Vec3, Option<glam::Affine3A>) {
    let Some(&start) = path.first() else {
        return (Vec3::ZERO, None);
    };
    let (mut at, mut total) = (start, None::<glam::Affine3A>);
    for &next in &path[1..] {
        let mut to = total.map_or(next, |c| c.transform_point3(next));
        for _ in 0..MAX_CARRIES {
            let Some((t, carry)) = q.passage(at, to) else {
                break;
            };
            at = carry.transform_point3(at.lerp(to, t));
            to = carry.transform_point3(to);
            total = Some(carry * total.unwrap_or(glam::Affine3A::IDENTITY));
            let rest = to - at;
            if rest.length_squared() < 1e-12 {
                break;
            }
            at += rest.normalize() * PAST.min(rest.length());
        }
        at = to;
    }
    (at, total)
}
/// Carry `position` and `velocity` (and a `rotation`) by a linked brick's
/// rigid move.
fn carried(carry: &glam::Affine3A, position: Vec3, velocity: Vec3) -> (Vec3, Vec3, Quat) {
    let (_, turn, _) = carry.to_scale_rotation_translation();
    (carry.transform_point3(position), turn * velocity, turn)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    Contact {
        impact: ProjectileContact,
    },
    Mounted {
        actor: ActorId,
        image: String,
        hand: u8,
    },
    Unmounted {
        actor: ActorId,
        hand: u8,
    },
    /// A [`HostTool`] image, or an Add-On tool's image with a `command`,
    /// entered its `onFire` state, or an Add-On image's other moment ran a
    /// command.
    ToolFire {
        actor: ActorId,
        image: String,
        hand: u8,
        command: Option<String>,
        /// The host's building mechanism to perform, for a [`HostTool`]
        /// image's `onFire`.
        tool: Option<HostTool>,
    },
    /// A shot's recoil: add `velocity` to the shooter's own velocity.
    Recoil {
        actor: ActorId,
        velocity: Vec3,
    },
    /// A hit slows the player it hit ([`crate::ProjectileDef::slow`]).
    Slow {
        actor: ActorId,
        slow: crate::Slow,
    },
    ImageState {
        actor: ActorId,
        image: String,
        state: String,
        hand: u8,
    },
    Animation {
        actor: ActorId,
        thread: u8,
        sequence: String,
        image_hand: Option<u8>,
    },
    Sound {
        source: TargetId,
        profile: String,
        position: Vec3,
    },
    Effect {
        source: TargetId,
        definition: String,
        position: Vec3,
        node: String,
        seconds: f32,
        image: Option<String>,
        hand: Option<u8>,
        /// Actual image aim or collision normal; absent for lifetime expiry.
        direction: Option<Vec3>,
        scale: f32,
    },
    Shell {
        actor: ActorId,
        image: String,
        hand: u8,
    },
    /// A [`crate::Hitscan`] ray of `image` in `hand` ended at `to`: each
    /// player draws its tracer from their own copy of the image.
    Tracer {
        actor: ActorId,
        hand: u8,
        image: String,
        to: Vec3,
    },
    /// A ricocheting hitscan ray went on from `from` to `to`
    /// ([`crate::Ricochet`]): drawn in the image's tracer style.
    Ricochet {
        actor: ActorId,
        image: String,
        from: Vec3,
        to: Vec3,
    },
    Spawned {
        projectile: u64,
        definition: String,
        source: ActorId,
        position: Vec3,
        velocity: Vec3,
    },
    Removed {
        projectile: u64,
    },
    Bounced {
        projectile: u64,
        position: Vec3,
        velocity: Vec3,
    },
    Damage {
        source: ActorId,
        target: TargetId,
        amount: f32,
        kind: String,
        position: Vec3,
        /// Which way the hurt travelled, a unit vector or zero: the shot's
        /// flight for a direct hit, from the blast centre outward for splash.
        direction: Vec3,
        /// The projectile that did it (its definition id), as v20's
        /// `ProjectileData::damage` knew its own datablock.
        #[serde(default)]
        projectile: String,
        /// A special kill this hurt makes ([`crate::DamageType::special`]):
        /// a shot a guard sent back ([`crate::Guard::reflect_kill`]).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        special: Option<String>,
        /// The landings of a ricocheting ray before this one
        /// ([`crate::Ricochet`]); 0 for anything else.
        #[serde(default)]
        bounces: u32,
    },
    Impulse {
        source: ActorId,
        target: TargetId,
        impulse: Vec3,
        position: Vec3,
    },
    Burn {
        source: ActorId,
        target: TargetId,
        seconds: f32,
    },
    BrickImpact {
        source: ActorId,
        target: Option<TargetId>,
        position: Vec3,
        parameters: BrickImpact,
    },
    HorseTransform {
        source: ActorId,
        target: ActorId,
        player_type: String,
        dismount: bool,
        reapply_colors: bool,
    },
    Key {
        actor: ActorId,
        brick: u64,
        matched: bool,
    },
    /// Host creates native ski vehicle at position, preserves velocity, mounts after 30 ticks.
    StartSkis {
        actor: ActorId,
        position: Vec3,
        velocity: Vec3,
        mount_after_ticks: u32,
    },
    StopSkis {
        actor: ActorId,
    },
    /// `SkiWeaponImage::onFire` while riding another vehicle: the host
    /// center-prints "Can't use skis right now." for two seconds.
    SkisUnavailable {
        actor: ActorId,
    },
    SkiNodes {
        actor: ActorId,
        visible: bool,
    },
    SportMovement {
        actor: ActorId,
        locked: bool,
    },
    Tumble {
        actor: ActorId,
        ticks: u32,
        velocity: Vec3,
    },
    FootballCatch {
        source: ActorId,
        catcher: ActorId,
        distance_feet: u32,
        was_thrown: bool,
    },
    Touchdown {
        actor: ActorId,
        brick: u64,
    },
    BallHit {
        source: ActorId,
        brick: u64,
        projectile: u64,
    },
    BallCaught {
        actor: ActorId,
        projectile: u64,
        image: String,
    },
    BallRest {
        projectile: u64,
        item: String,
        position: Vec3,
    },
    /// The holder's magazine or reserve changed (a shot, a reload, a gun
    /// drawn): the ammo display shows [`WeaponsWorld::ammo`].
    Ammo {
        actor: ActorId,
    },
    Dropped {
        drop: u64,
        item: String,
        position: Vec3,
        velocity: Vec3,
    },
    DropRemoved {
        drop: u64,
    },
    /// A sound `actor` alone hears, at their ears (`play2D`): a burning
    /// player's sizzle.
    Heard {
        actor: ActorId,
        profile: String,
    },
    /// Text in the middle of the holder's screen for `seconds`: a cooked
    /// grenade's countdown.
    Print {
        actor: ActorId,
        text: String,
        seconds: f32,
    },
    /// A projectile exploded with a blast reaching `radius`: what a
    /// bystander notices, whoever it hurts.
    Blast {
        source: ActorId,
        position: Vec3,
        radius: f32,
    },
    Diagnostic {
        actor: Option<ActorId>,
        message: String,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Projectile {
    pub id: u64,
    pub definition: String,
    pub source: ActorId,
    pub position: Vec3,
    pub velocity: Vec3,
    pub scale: f32,
    pub age: u32,
    pub bounced: bool,
    pub stuck: bool,
    pub origin: Vec3,
    pub was_thrown: bool,
    /// Palette index of a colour spray can's paint (`colorID`).
    pub paint: Option<u8>,
    /// The direction a stuck projectile flew in: its velocity is zero, but
    /// its model keeps pointing that way (v20 keeps the last transform).
    pub heading: Option<Vec3>,
    /// Bounces so far, for `max_bounces`. Host bookkeeping, not replicated.
    #[serde(skip)]
    pub bounces: u32,
    /// The runtime tick it was made on; `children` and `aura` count from it.
    /// Host bookkeeping, not replicated.
    #[serde(skip)]
    pub spawned: u64,
}
/// Vertical speed a projectile loses each tick of flight (`gravityMod`);
/// none unless it is ballistic.
pub fn fall_per_tick(d: &crate::ProjectileDef) -> f32 {
    if d.ballistic {
        9.81 * d.gravity / crate::TICK_HZ as f32
    } else {
        0.0
    }
}
/// One tick of free flight, exactly as the host moves a projectile that hits
/// nothing. Clients coast replicated projectiles with it between the host's
/// corrections, so a projectile's flight costs no bandwidth.
pub fn coast(p: &mut Projectile, fall: f32) {
    p.age = p.age.saturating_add(1);
    if p.stuck {
        return;
    }
    p.velocity.y -= fall;
    p.position += p.velocity * (1.0 / 120.0);
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Drop {
    pub rotation: Quat,
    pub scale: f32,
    pub id: u64,
    pub item: String,
    pub position: Vec3,
    pub velocity: Vec3,
    /// Who threw it, or [`ActorId::NOBODY`] for an item the world put down.
    pub source: ActorId,
    pub pickup_after: u64,
    pub expires: u64,
    /// The rounds in a thrown gun's magazine, for whoever picks it up.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rounds: Option<u32>,
    /// A palette colour tinting it instead of its image's own colour: the
    /// colour a `paint_tint` item was held in when dropped (its holder's
    /// spray colour), or one an Add-On gave it (a team's flag).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paint: Option<u8>,
    /// Text floating over it (`setShapeName`), in a palette colour
    /// (`setShapeNameColor`): a dropped flag's countdown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<DropName>,
}
/// A dropped item's floating name ([`Drop::name`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DropName {
    pub text: String,
    /// Palette colour index.
    pub color: u8,
}
/// Longest a dropped item's name may be, in characters.
pub const MAX_DROP_NAME: usize = 32;
/// Longest an Add-On's rules may keep a dropped item lying, ticks (ten
/// minutes).
pub const MAX_DROP_TICKS: u64 = 120 * 600;
/// How a world drop not thrown by an actor looks and lasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DropLook {
    /// A palette colour tinting it.
    pub paint: Option<u8>,
    /// Ticks until it pops: 1 to [`MAX_DROP_TICKS`]; v20's drops pop after
    /// ten seconds (1200).
    pub lifetime: u64,
}
impl Default for DropLook {
    fn default() -> Self {
        Self {
            paint: None,
            lifetime: DROP_LIFETIME_TICKS,
        }
    }
}
fn loaded() -> bool {
    true
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Equipped {
    image: String,
    state: usize,
    remaining: u32,
    entered: bool,
    /// The trigger this image's state machine sees. The right hand copies
    /// the actor's held trigger every tick; the left hand only ever gets
    /// `onFireAkimbo`'s one-tick pulse.
    trigger: bool,
    hand: u8,
    /// Palette index for the derived colour spray can image.
    paint: Option<u8>,
    /// The key its magazine's rounds are kept under, fixed as it mounts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    magazine: Option<String>,
}
/// Torque's `nextImage`: a right-hand image asked for while the held one's
/// state forbids image changes. It mounts on the next state that allows one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct NextImage {
    image: String,
    paint: Option<u8>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Actor {
    pub inventory: Vec<Option<String>>,
    pub selected: Option<usize>,
    pub frame: Frame,
    pub ammo: bool,
    /// Whether the image in hand is loaded (`setImageLoaded`), which its
    /// states' `loaded` and `not_loaded` transitions read.
    #[serde(default = "loaded")]
    pub loaded: bool,
    pub skiing: bool,
    /// Torque's four image slots: 0 the right hand, 1 the left (an akimbo
    /// gun's), 2 and 3 worn on the body (a carried flag on the back). Tool
    /// changes touch only the hands.
    images: [Option<Equipped>; IMAGE_SLOTS],
    /// The held fire button (`move->trigger[0]`). It belongs to the player,
    /// not the image: `Player::updateMove` hands it to image slot 0 every
    /// tick, so an image mounted while it is held sees it at once.
    #[serde(default)]
    trigger: bool,
    #[serde(default)]
    next: Option<NextImage>,
    last_shot: Option<u64>,
    /// The palette colour the holder last picked for their spray can,
    /// which `paint_tint` images take.
    #[serde(default)]
    spray: u8,
    ball_ready: u64,
    spawn_tick: u64,
    tackle_until: u64,
    /// Rounds in each carried gun's magazine ([`crate::Magazine`]), by
    /// [`slot_key`]: two of one gun each keep their own, as the tactical
    /// packs' `%obj.toolAmmo[%slot]` does. A gun a rule mounted with no tool
    /// selected keeps its rounds under its image id.
    #[serde(default)]
    rounds: BTreeMap<String, u32>,
    /// Reserve ammo by type.
    #[serde(default)]
    reserve: BTreeMap<String, Reserve>,
    #[serde(default)]
    reload: Option<Reload>,
    /// The grenade whose fuse is burning in the hand ([`crate::Cook`]).
    #[serde(default)]
    cook: Option<Cooking>,
    /// Image states' timed cues still to play ([`crate::State::cues`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    cues: Vec<PendingCue>,
    /// The image in slot [`EMOTE_SLOT`] (`Player::emote`): an emote, pain,
    /// flames or an Add-On's effect on the body ([`WeaponsWorld::emote`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    emote: Option<Equipped>,
    /// Projectiles the guard in hand ([`crate::Guard::durability`]) stops
    /// before it breaks, counted from its first stop: Kai's `shieldHP`,
    /// which lasts until the holder dies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    guard_left: Option<u32>,
    /// A bot, not a player ([`WeaponsWorld::set_bot`]).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    bot: bool,
}
/// Torque's image slot 3, which `Player::emote` and `Player::burn` mount
/// into: a new emote replaces the one there. Its image runs its states on
/// their timeouts, and a state script with a command runs it for the
/// wearer, as `medigunHealImage::onHeal` healed whoever wore it.
pub const EMOTE_SLOT: u8 = 3;
/// A [`crate::Cue`] waiting for its tick, with where the holder was as its
/// state began (v20 scheduled `serverPlay3D` with the position then).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct PendingCue {
    due: u64,
    cue: crate::Cue,
    position: Vec3,
}
/// The most cues one holder has waiting.
const MAX_PENDING_CUES: usize = 64;
/// [`Actor::cook`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Cooking {
    image: String,
    /// The tick the fuse was lit.
    lit: u64,
}
/// A holder's reserve of one ammo type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reserve {
    Rounds(u32),
    /// Reloads never run out (an Add-On's endless-ammo rule).
    Endless,
}
impl Reserve {
    fn take(&mut self, wanted: u32) -> u32 {
        match self {
            Self::Endless => wanted,
            Self::Rounds(n) => {
                let taken = wanted.min(*n);
                *n -= taken;
                taken
            }
        }
    }
    fn any(self) -> bool {
        self != Self::Rounds(0)
    }
}
/// A reload under way: `item`'s magazine fills at tick `done`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Reload {
    item: String,
    done: u64,
}
/// What a holder's held gun has to shoot, for the ammo display and rules.
#[derive(Debug, Clone, PartialEq)]
pub struct AmmoView {
    pub item: String,
    pub rounds: u32,
    pub size: u32,
    pub ammo: String,
    pub name: String,
    pub reserve: Reserve,
    pub reloading: bool,
    /// How long the display stays up ([`crate::Magazine::display_ticks`]).
    pub display_ticks: u32,
    /// Counted from the reserve ([`crate::Magazine::from_reserve`]):
    /// `rounds` is the reserve, and there is no magazine to show.
    pub counted: bool,
    /// Where its rounds come from ([`crate::Magazine::supply`]).
    pub supply: crate::Supply,
    /// Whether it has a display at all ([`crate::Magazine::displayed`]).
    pub shown: bool,
}
/// The key the rounds of the gun `item` in tool `slot` are kept under.
fn slot_key(item: &str, slot: usize) -> String {
    format!("{item}#{slot}")
}
/// The item (or image) a magazine key belongs to.
fn key_item(key: &str) -> &str {
    key.rsplit_once('#').map_or(key, |(item, _)| item)
}
/// The rounds `key`'s magazine holds: one counted from its reserve
/// ([`crate::Magazine::from_reserve`]) holds what the reserve does.
fn rounds_in(a: &Actor, key: &str, magazine: &crate::Magazine) -> u32 {
    magazine_rounds(
        magazine,
        a.rounds.get(key).copied().unwrap_or(0),
        a.reserve
            .get(&magazine.ammo)
            .copied()
            .unwrap_or(Reserve::Rounds(0)),
    )
}
fn magazine_rounds(magazine: &crate::Magazine, stored: u32, reserve: Reserve) -> u32 {
    if magazine.counts_reserve() {
        match reserve {
            Reserve::Endless => u32::MAX,
            Reserve::Rounds(n) => n,
        }
    } else if magazine.supply == crate::Supply::Unlimited {
        // Never used: always full.
        magazine.size
    } else {
        stored
    }
}
fn ammo_view(
    key: &str,
    magazine: crate::Magazine,
    rounds: u32,
    reserve: Reserve,
    reloading: bool,
) -> AmmoView {
    AmmoView {
        rounds: rounds.min(100_000),
        counted: magazine.counts_reserve(),
        supply: magazine.supply,
        shown: magazine.displayed(),
        item: key_item(key).to_string(),
        size: magazine.size,
        name: magazine.name().to_string(),
        reserve,
        ammo: magazine.ammo,
        reloading,
        display_ticks: magazine.display_ticks,
    }
}
/// Whether a reload of `magazine` has something to fill it from: reserve,
/// or nothing at all ([`crate::Supply::Endless`]).
fn can_fill(a: &Actor, magazine: &crate::Magazine) -> bool {
    match magazine.supply {
        crate::Supply::Endless | crate::Supply::Unlimited => true,
        _ => a.reserve.get(&magazine.ammo).is_some_and(|r| r.any()),
    }
}
/// Whether `key`'s magazine has a shot: its rounds, and under
/// [`crate::Supply::Both`] as many in the reserve too.
fn has_shot(a: &Actor, key: &str, magazine: &crate::Magazine) -> bool {
    magazine.fires(rounds_in(a, key, magazine))
        && (magazine.supply != crate::Supply::Both
            || a.reserve.get(&magazine.ammo).is_some_and(|r| match r {
                Reserve::Endless => true,
                Reserve::Rounds(n) => *n >= magazine.per_shot,
            }))
}
/// A shot's rounds out of `key`'s magazine and, by its supply, the
/// reserve. Whether it was the magazine's last shot.
fn take_shot(a: &mut Actor, key: &str, magazine: &crate::Magazine) -> bool {
    use crate::Supply;
    let rounds = rounds_in(a, key, magazine);
    let last = magazine.last(rounds);
    if matches!(
        magazine.supply,
        Supply::Reserve | Supply::Endless | Supply::Both
    ) && !magazine.from_reserve
    {
        let left = if last { 0 } else { rounds - magazine.per_shot };
        a.rounds.insert(key.to_owned(), left);
    }
    if (magazine.counts_reserve() || magazine.supply == Supply::Both)
        && let Some(reserve) = a.reserve.get_mut(&magazine.ammo)
    {
        reserve.take(magazine.per_shot);
    }
    last
}
impl Actor {
    /// Whether the fire button is held, whatever is (or is not) in hand.
    pub fn trigger_held(&self) -> bool {
        self.trigger
    }
}
/// One ray of a hitscan shot ([`WeaponsWorld::hitscan`]).
struct Ray<'a> {
    hand: u8,
    image: &'a str,
    /// The image's projectile.
    definition: &'a str,
    from: Vec3,
    /// Where the image's projectile leaves the barrel.
    muzzle: Vec3,
    direction: Vec3,
    range: f32,
    hitscan: &'a crate::Hitscan,
}
/// What one tick of an image's state machine asks of its holder.
enum Advance {
    Keep,
    Drop,
    /// Entered a state that allows image changes with a `nextImage` waiting.
    Switch,
}
/// Torque's `MaxMountedImages`: image slots per player.
pub const IMAGE_SLOTS: usize = 4;
/// Slots 0 and 1 are the hands.
pub const HAND_SLOTS: usize = 2;
/// Slots worn on the body, which tool changes leave alone.
pub const WORN_SLOTS: std::ops::Range<usize> = HAND_SLOTS..IMAGE_SLOTS;
pub struct WeaponsWorld {
    pub pack: Arc<Pack>,
    pub tick: u64,
    actors: BTreeMap<ActorId, Actor>,
    projectiles: BTreeMap<u64, Projectile>,
    drops: BTreeMap<u64, Drop>,
    /// Authored item boxes; a drop without one falls as a point.
    item_bounds: BTreeMap<String, ItemBounds>,
    next_id: u64,
    events: Vec<Event>,
    /// Explosions queued for the next tick; they never fly.
    explosions: Vec<Projectile>,
    /// Projectiles that go off at this age, before their lifetime: a cooked
    /// grenade's fuse, a cluster's bomblets ([`crate::Children::fuse_ticks`]).
    fuses: BTreeMap<u64, u32>,
    /// The tick each projectile last played its collision sound
    /// ([`crate::CollisionSound::gap_ticks`]).
    sounded: BTreeMap<u64, u64>,
    /// Projectiles a guard sent back ([`crate::Guard::reflect`]): the tick
    /// it did, and the special kill they make.
    reflected: BTreeMap<u64, (u64, Option<String>)>,
    /// This tick's projectiles a guard stopped: who stopped each, and the
    /// share of push left. Their blasts spare that holder.
    stopped: Vec<(u64, ActorId, f32)>,
}
impl WeaponsWorld {
    pub fn new(pack: Pack) -> Result<Self> {
        pack.validate()?;
        anyhow::ensure!(
            pack.external_projectiles.is_empty(),
            "The pack fires projectiles of packages it depends on ({}); merge them first",
            pack.external_projectiles
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        );
        Ok(Self {
            pack: Arc::new(pack),
            tick: 0,
            actors: BTreeMap::new(),
            projectiles: BTreeMap::new(),
            drops: BTreeMap::new(),
            item_bounds: BTreeMap::new(),
            next_id: 1,
            events: vec![],
            explosions: vec![],
            fuses: BTreeMap::new(),
            sounded: BTreeMap::new(),
            reflected: BTreeMap::new(),
            stopped: vec![],
        })
    }
    /// Plays `pack` from now on in place of the pack it plays: the same
    /// definitions with some fields changed ([`Pack::with_settings`]), so
    /// everything held, flying and lying keeps going. Each field takes
    /// effect where the game next reads it: the next shot, reload or spawn.
    pub fn retune(&mut self, pack: Pack) -> Result<()> {
        pack.validate()?;
        anyhow::ensure!(
            self.pack.items.keys().eq(pack.items.keys())
                && self.pack.projectiles.keys().eq(pack.projectiles.keys())
                && self.pack.images.len() == pack.images.len()
                && self
                    .pack
                    .images
                    .iter()
                    .zip(&pack.images)
                    .all(|((a, x), (b, y))| a == b && x.states.len() == y.states.len()),
            "A retuned pack has the same definitions"
        );
        self.pack = Arc::new(pack);
        Ok(())
    }
    /// The id of the image `id` holds in `hand`.
    pub fn image_id(&self, id: ActorId, hand: u8) -> Option<&str> {
        let equipped = self.actors.get(&id)?.images.get(hand as usize)?.as_ref()?;
        Some(&equipped.image)
    }
    pub fn image_state(&self, id: ActorId, hand: u8) -> Option<(&Image, &State)> {
        let equipped = self.actors.get(&id)?.images.get(hand as usize)?.as_ref()?;
        let image = self.pack.images.get(&equipped.image)?;
        Some((image, image.states.get(equipped.state)?))
    }
    /// Palette index carried by a mounted colour spray can.
    pub fn image_paint(&self, id: ActorId, hand: u8) -> Option<u8> {
        self.actors
            .get(&id)?
            .images
            .get(hand as usize)?
            .as_ref()?
            .paint
    }
    pub fn actor(&self, id: ActorId) -> Option<&Actor> {
        self.actors.get(&id)
    }
    pub fn projectiles(&self) -> impl Iterator<Item = &Projectile> {
        self.projectiles.values()
    }
    pub fn projectile(&self, id: u64) -> Option<&Projectile> {
        self.projectiles.get(&id)
    }
    /// A scheduled output on the still-live original projectile. Bounce uses
    /// the activation's normal and current velocity; normalized redirect uses
    /// its current speed. Retired IDs are never substituted.
    pub fn respond_projectile(
        &mut self,
        id: u64,
        normal: Vec3,
        response: ContactResponse,
    ) -> Result<bool> {
        if !self.projectiles.contains_key(&id) {
            return Ok(false);
        }
        match response {
            ContactResponse::Delete => Ok(self.remove_projectile(id)),
            ContactResponse::Explode => {
                ensure!(
                    self.explosions.len() < MAX_EXPLOSIONS_PER_TICK && self.events.len() < 8192,
                    "Explosion budget for this tick"
                );
                // Explicit explosions already run at the next weapon phase.
                let p = self.projectiles.remove(&id).unwrap();
                self.explosions.push(p);
                self.events.push(Event::Removed { projectile: id });
                Ok(true)
            }
            ContactResponse::Bounce(_) | ContactResponse::Redirect { .. } => {
                let p = self.projectiles.get_mut(&id).unwrap();
                self.events.push(redirect_projectile(p, normal, response)?);
                Ok(true)
            }
            ContactResponse::Continue => anyhow::bail!("No projectile output"),
        }
    }
    /// [`fall_per_tick`] of every projectile that falls, by definition.
    pub fn projectile_falls(&self) -> BTreeMap<String, f32> {
        self.pack
            .projectiles
            .iter()
            .map(|(id, d)| (id.clone(), fall_per_tick(d)))
            .filter(|(_, fall)| *fall != 0.0)
            .collect()
    }
    pub fn drops(&self) -> impl Iterator<Item = &Drop> {
        self.drops.values()
    }
    /// Push a dropped item: its velocity changes by `impulse / mass`
    /// (`Item::applyImpulse`), and a resting item starts moving again.
    pub fn push_drop(&mut self, id: u64, impulse: Vec3) {
        if let Some(d) = self.drops.get_mut(&id)
            && impulse.is_finite()
        {
            d.velocity = (d.velocity + impulse / ITEM_MASS).clamp_length_max(200.0);
        }
    }
    pub fn add_actor(&mut self, id: ActorId, slots: usize) -> Result<()> {
        ensure!(
            !self.actors.contains_key(&id)
                && self.actors.len() < MAX_ACTORS
                && (1..=16).contains(&slots),
            "Actor/slot admission"
        );
        self.actors.insert(
            id,
            Actor {
                inventory: vec![None; slots],
                selected: None,
                frame: Frame::default(),
                ammo: true,
                loaded: true,
                skiing: false,
                images: Default::default(),
                trigger: false,
                next: None,
                last_shot: None,
                spray: 0,
                ball_ready: 0,
                spawn_tick: self.tick,
                tackle_until: 0,
                rounds: BTreeMap::new(),
                reserve: BTreeMap::new(),
                reload: None,
                cook: None,
                cues: Vec::new(),
                emote: None,
                guard_left: None,
                bot: false,
            },
        );
        Ok(())
    }
    /// Whether `id` is a bot, which some rules treat apart
    /// ([`crate::Guard::bots_keep`]).
    pub fn set_bot(&mut self, id: ActorId, bot: bool) -> Result<()> {
        self.actors.get_mut(&id).context("Unknown actor")?.bot = bot;
        Ok(())
    }
    /// The share of a fall or crash's hurt to `id`, moving along `toward`
    /// as it struck, that a guard they hold and face it with lets through
    /// ([`crate::Guard::fall_damage`]); None when no guard takes any.
    pub fn guard_fall_share(&self, id: ActorId, toward: Vec3) -> Option<f32> {
        let (guard, _, a) = guard_held(&self.actors, &self.pack, id)?;
        let share = guard.fall_damage?;
        (a.frame.direction.dot(toward) > 0.0).then_some(share)
    }
    /// The clang of a guard taking a fall or crash along `toward`
    /// ([`Self::guard_fall_share`]).
    pub fn guard_fall_clang(&mut self, id: ActorId, toward: Vec3) {
        if self.guard_fall_share(id, toward).is_none() {
            return;
        }
        let Some((guard, _, a)) = guard_held(&self.actors, &self.pack, id) else {
            return;
        };
        let (explosion, middle, scale) = (guard.hit_explosion.clone(), body(a), a.frame.scale);
        self.burst(&explosion, id, middle, scale * 2.0);
    }
    /// Remove one live projectile without exploding it (`killObjects`).
    pub fn remove_projectile(&mut self, projectile: u64) -> bool {
        let removed = self.projectiles.remove(&projectile).is_some();
        if removed {
            self.events.push(Event::Removed { projectile });
        }
        removed
    }
    pub fn remove_actor(&mut self, id: ActorId) {
        if let Some(mut a) = self.actors.remove(&id) {
            self.unmount(id, &mut a);
            Self::take_worn(&mut self.events, id, &mut a);
        }
        let removed: Vec<_> = self
            .projectiles
            .values()
            .filter(|p| p.source == id)
            .map(|p| p.id)
            .collect();
        for projectile in removed {
            self.projectiles.remove(&projectile);
            self.events.push(Event::Removed { projectile });
        }
    }
    pub fn set_frame(&mut self, id: ActorId, frame: Frame) -> Result<()> {
        frame.validate()?;
        self.actors.get_mut(&id).context("Unknown actor")?.frame = frame;
        Ok(())
    }
    pub fn set_ammo(&mut self, id: ActorId, ammo: bool) -> Result<()> {
        self.actors.get_mut(&id).context("Unknown actor")?.ammo = ammo;
        Ok(())
    }
    pub fn set_loaded(&mut self, id: ActorId, loaded: bool) -> Result<()> {
        self.actors.get_mut(&id).context("Unknown actor")?.loaded = loaded;
        Ok(())
    }
    pub fn give(&mut self, id: ActorId, item: &str) -> Result<usize> {
        let a = self.actors.get(&id).context("Unknown actor")?;
        let slot = a
            .inventory
            .iter()
            .position(Option::is_none)
            .context("Inventory full")?;
        self.give_at(id, slot, item)?;
        Ok(slot)
    }
    /// Trusted host loadouts preserve authored empty slots. Normal pickups use
    /// `give` so they fill the first available slot instead.
    pub fn give_at(&mut self, id: ActorId, slot: usize, item: &str) -> Result<()> {
        ensure!(self.contains_item(item), "Unknown item");
        // `ItemData::onPickup` takes the first free slot; v20 has no
        // duplicate check, so a player may carry two of one item.
        let a = self.actors.get_mut(&id).context("Unknown actor")?;
        let place = a.inventory.get_mut(slot).context("Invalid item slot")?;
        ensure!(place.is_none(), "Occupied item slot");
        *place = Some(item.into());
        // A gun new to the slot comes with a full magazine.
        a.rounds.remove(&slot_key(item, slot));
        Ok(())
    }
    /// Trusted respawn/loadout replacement: unmount held images, clear the
    /// selection and install `items` slot for slot. In-flight projectiles keep
    /// flying; they belong to the world, not the inventory.
    pub fn set_inventory(&mut self, id: ActorId, items: &[Option<String>]) -> Result<()> {
        for item in items.iter().flatten() {
            ensure!(self.contains_item(item), "Unknown loadout item");
        }
        let slots = self
            .actors
            .get(&id)
            .context("Unknown actor")?
            .inventory
            .len();
        ensure!(items.len() == slots, "Loadout slot count mismatch");
        let mut a = self.actors.remove(&id).context("Unknown actor")?;
        self.unmount(id, &mut a);
        a.selected = None;
        a.inventory = items.to_vec();
        // Each slot's magazine stays with the gun still in it.
        let kept: BTreeSet<String> = a
            .inventory
            .iter()
            .enumerate()
            .filter_map(|(slot, item)| Some(slot_key(item.as_ref()?, slot)))
            .collect();
        a.rounds.retain(|key, _| kept.contains(key));
        a.spawn_tick = self.tick;
        self.actors.insert(id, a);
        Ok(())
    }
    /// Authored item boxes that dropped items fall and rest on.
    pub fn set_item_bounds(&mut self, bounds: BTreeMap<String, ItemBounds>) {
        self.item_bounds = bounds;
    }
    /// An item for building, not fighting: its image runs a [`HostTool`],
    /// or it is one of the [`CORE_TOOLS`] the engine supplies when the pack
    /// has none of its own.
    pub fn building_tool(&self, item: &str) -> bool {
        match self.pack.items.get(item) {
            Some(i) => self.pack.images.get(&i.image).and_then(host_tool).is_some(),
            None => CORE_TOOLS.contains(&item),
        }
    }
    pub fn contains_item(&self, item: &str) -> bool {
        self.pack.items.contains_key(item) || CORE_TOOLS.contains(&item)
    }
    /// Every item [`Self::contains_item`] accepts.
    pub fn item_ids(&self) -> impl Iterator<Item = &str> {
        let pack = self.pack.items.keys().map(String::as_str);
        let core = CORE_TOOLS
            .into_iter()
            .filter(|id| !self.pack.items.contains_key(*id));
        pack.chain(core)
    }
    /// `ServerCmdUseTool` / `ServerCmdUnUseTool`. The selection changes at
    /// once. Putting tools away unmounts at once (`unmountImage`); a new
    /// image waits, as Torque's `setImage` does, while the held image's
    /// state forbids image changes (a gun mid-shot), then mounts on the next
    /// state that allows one. The trigger stays held throughout.
    pub fn equip(&mut self, id: ActorId, slot: Option<usize>) -> Result<()> {
        ensure!(
            self.events.len() < 8192,
            "Command event budget; advance/drain before retry"
        );
        let a = self.actors.get(&id).context("Unknown actor")?;
        let image = if let Some(slot) = slot {
            let item = a
                .inventory
                .get(slot)
                .and_then(Option::as_ref)
                .context("Empty slot")?;
            self.pack.items.get(item).map(|item| item.image.clone())
        } else {
            None
        };
        let mut a = self.actors.remove(&id).unwrap();
        // A reload its image's states run stops as a tool is drawn or put
        // away, as the states do (Tier+Tactical's `onUse` clearing
        // `TT_forceToolReload`); drawn again, the gun checks its rounds
        // afresh.
        if self.magazine_of(&a).is_some_and(|(_, m)| m.scripted()) {
            a.reload = None;
        }
        // Another copy of the gun in hand drawn from another slot comes out
        // afresh when its magazine says so (Tier's Remount Duplicate
        // Items: `serverCmdUnUseTool`, then `serverCmdUseTool`); else the
        // image stays mid-state with that slot's rounds.
        let remount = slot != a.selected
            && image.as_ref().is_some_and(|image| {
                a.images[0].as_ref().is_some_and(|h| &h.image == image)
                    && self.pack.images[image]
                        .magazine
                        .as_ref()
                        .is_some_and(|m| m.remount)
            });
        if remount {
            self.unmount(id, &mut a);
        }
        // Selected first, so the image mounting knows whose magazine it is.
        a.selected = slot;
        match image {
            Some(image) => {
                let paint = self
                    .pack
                    .images
                    .get(&image)
                    .filter(|i| i.paint_tint)
                    .map(|_| a.spray);
                self.change_image(id, &mut a, &image, paint)
            }
            None => self.unmount(id, &mut a),
        }
        a.selected = slot;
        self.switch_magazine(id, &mut a);
        self.actors.insert(id, a);
        Ok(())
    }
    /// The palette colour `id` last picked for their spray can
    /// (`%client.currentColor`), which `paint_tint` images they take out
    /// show.
    pub fn set_spray_color(&mut self, id: ActorId, color: u8) -> Result<()> {
        self.actors.get_mut(&id).context("Unknown actor")?.spray = color;
        Ok(())
    }
    /// `Player::mountImage` for an image that is not an inventory item: spray
    /// cans (`serverCmdUseSprayCan`/`UseFXCan`) and the admin wand. Like
    /// those commands it deselects the tool slot. `paint` binds the colour
    /// can's palette index, the native form of `color<N>SprayCanImage`.
    /// It mounts, or waits, exactly as [`Self::equip`] does.
    pub fn mount_image(&mut self, id: ActorId, image: &str, paint: Option<u8>) -> Result<()> {
        ensure!(
            self.events.len() < 8192,
            "Command event budget; advance/drain before retry"
        );
        ensure!(self.pack.images.contains_key(image), "Unknown image");
        let mut a = self.actors.remove(&id).context("Unknown actor")?;
        self.change_image(id, &mut a, image, paint);
        a.selected = None;
        self.actors.insert(id, a);
        Ok(())
    }
    /// Torque's `ShapeBase::setImage` for the right hand. The image already
    /// held (the same colour can) stays as it is, mid-state; each palette
    /// colour is its own v20 datablock, so another colour mounts afresh.
    fn change_image(&mut self, id: ActorId, a: &mut Actor, image: &str, paint: Option<u8>) {
        let wanted = NextImage {
            image: image.into(),
            paint,
        };
        match &a.images[0] {
            Some(e) if e.image == wanted.image && e.paint == wanted.paint => a.next = None,
            Some(e)
                if !self.pack.images[&e.image]
                    .states
                    .get(e.state)
                    .is_none_or(|s| s.allow_change) =>
            {
                a.next = Some(wanted)
            }
            _ => self.swap_images(id, a, wanted),
        }
    }
    /// Replace whatever is held with `next`; the akimbo gun brings its left
    /// hand. The selection is the caller's.
    fn swap_images(&mut self, id: ActorId, a: &mut Actor, next: NextImage) {
        let selected = a.selected;
        self.unmount(id, a);
        a.selected = selected;
        self.mount(id, a, &next.image, 0);
        if let Some(e) = &mut a.images[0] {
            e.paint = next.paint;
        }
        if let Some(left) = self.left_image(&next.image) {
            self.mount(id, a, &left, 1);
        }
    }
    /// The image an image brings into the left hand ([`Image::left_image`]).
    fn left_image(&self, image: &str) -> Option<String> {
        self.pack.images.get(image)?.left_image.clone()
    }
    /// The magazine of the gun in the right hand and the key its rounds
    /// are kept under: the tool slot it was drawn from ([`slot_key`]), or
    /// the image itself when a rule mounted it with no tool selected.
    fn magazine_of(&self, a: &Actor) -> Option<(String, crate::Magazine)> {
        self.magazine_in(a.images[0].as_ref()?)
    }
    /// With nothing in the right hand, the selected tool's magazine if it
    /// is counted from the reserve: a grenade put away when the last was
    /// thrown, which comes back as reserve arrives.
    fn stowed(&self, a: &Actor) -> Option<(String, crate::Magazine)> {
        if a.images[0].is_some() {
            return None;
        }
        let slot = a.selected?;
        let item = a.inventory.get(slot)?.as_ref()?;
        let image = &self.pack.items.get(item)?.image;
        let magazine = self.pack.images.get(image)?.magazine.clone()?;
        magazine
            .from_reserve
            .then(|| (slot_key(item, slot), magazine))
    }
    /// The magazine of `held`, which a running state has out of the hand.
    fn magazine_in(&self, held: &Equipped) -> Option<(String, crate::Magazine)> {
        let magazine = self.pack.images.get(&held.image)?.magazine.clone()?;
        Some((held.magazine.clone()?, magazine))
    }
    /// A gun with a magazine comes into the right hand: a new one is full,
    /// a first gun of its ammo brings the starting reserve, a reload of
    /// another gun stops, and the hand has ammo while there is a shot.
    fn load_magazine(&mut self, a: &mut Actor, image: &str) -> Option<String> {
        let Some(magazine) = self.pack.images.get(image).and_then(|i| i.magazine.clone()) else {
            a.reload = None;
            return None;
        };
        let key = a
            .selected
            .and_then(|slot| Some(slot_key(a.inventory.get(slot)?.as_ref()?, slot)))
            .unwrap_or_else(|| image.to_string());
        if a.reload.as_ref().is_some_and(|r| r.item != key) {
            a.reload = None;
        }
        if !magazine.from_reserve {
            a.rounds.entry(key.clone()).or_insert(magazine.size);
        }
        a.reserve
            .entry(magazine.ammo.clone())
            .or_insert(Reserve::Rounds(magazine.reserve.min(magazine.max_reserve)));
        self.magazine_flags(a, image, &key, &magazine);
        Some(key)
    }
    /// Another copy of the gun already in hand was selected: the image
    /// stays mounted, mid-state, but its rounds are now that slot's (the
    /// tactical packs' `Weapon::onUse` for duplicates, with its own
    /// `TT_toolAmmo[%toolNum]`).
    fn switch_magazine(&mut self, id: ActorId, a: &mut Actor) {
        let Some(slot) = a.selected else { return };
        let Some(item) = a.inventory.get(slot).cloned().flatten() else {
            return;
        };
        let key = slot_key(&item, slot);
        let Some(held) = a.images[0].as_mut() else {
            return;
        };
        if held.magazine.is_none() || held.magazine.as_ref() == Some(&key) {
            return;
        }
        held.magazine = Some(key.clone());
        let image = held.image.clone();
        let Some(magazine) = self.pack.images[&image].magazine.clone() else {
            return;
        };
        if a.reload.as_ref().is_some_and(|r| r.item != key) {
            a.reload = None;
        }
        if !magazine.from_reserve {
            a.rounds.entry(key.clone()).or_insert(magazine.size);
        }
        self.magazine_flags(a, &image, &key, &magazine);
        self.events.push(Event::Ammo { actor: id });
    }
    /// A shot from the right hand: with a magazine it takes its rounds, or
    /// is refused (the gun clicks, and an empty one reloads). `Some(true)`
    /// for the magazine's last shot ([`crate::Magazine::last_rounds`]).
    fn spend_rounds(&mut self, id: ActorId, a: &mut Actor, held: &Equipped) -> Option<bool> {
        let Some((key, magazine)) = self.magazine_in(held) else {
            return Some(false);
        };
        let shot = has_shot(a, &key, &magazine);
        if !magazine.reloads() {
            // Counted from the reserve (a grenade's throw, an Arena gun's
            // shot takes it there), or never used at all.
            if !shot {
                self.empty_click(id, a, &magazine);
                return None;
            }
            take_shot(a, &key, &magazine);
            self.magazine_flags(a, &held.image, &key, &magazine);
            self.events.push(Event::Ammo { actor: id });
            return Some(false);
        }
        if magazine.scripted() {
            // The image's states decide when it fires and reloads; a shot
            // ends a reload under way, as a pump's trigger stops its shells.
            if !shot {
                self.empty_click(id, a, &magazine);
                return None;
            }
            a.reload = None;
            let last = take_shot(a, &key, &magazine);
            self.events.push(Event::Ammo { actor: id });
            return Some(last);
        }
        let reloading = a.reload.is_some();
        if reloading && magazine.one_by_one && shot {
            // A pull of the trigger stops loading shells one by one.
            a.reload = None;
        } else if reloading || !shot {
            self.empty_click(id, a, &magazine);
            if !self.begin_reload(id, a, key.clone(), &magazine) {
                self.magazine_flags(a, &held.image, &key, &magazine);
            }
            return None;
        }
        let last = take_shot(a, &key, &magazine);
        self.magazine_flags(a, &held.image, &key, &magazine);
        self.events.push(Event::Ammo { actor: id });
        if !magazine.fires(rounds_in(a, &key, &magazine)) {
            self.begin_reload(id, a, key, &magazine);
        }
        Some(last)
    }
    fn empty_click(&mut self, id: ActorId, a: &Actor, magazine: &crate::Magazine) {
        if !magazine.empty_sound.is_empty() {
            self.events.push(Event::Sound {
                source: TargetId::Actor(id),
                profile: magazine.empty_sound.clone(),
                position: a.frame.position,
            });
        }
    }
    /// Start reloading the held gun's magazine, if it is not full, there is
    /// reserve to load and no reload is under way. Whether one started.
    fn start_reload(&mut self, id: ActorId, a: &mut Actor) -> bool {
        let Some((key, magazine)) = self.magazine_of(a) else {
            return false;
        };
        self.begin_reload(id, a, key, &magazine)
    }
    /// Start reloading `key`'s magazine if it is not full, there is reserve
    /// to load and no reload is under way.
    fn begin_reload(
        &mut self,
        id: ActorId,
        a: &mut Actor,
        key: String,
        magazine: &crate::Magazine,
    ) -> bool {
        let rounds = a.rounds.get(&key).copied().unwrap_or(0);
        if a.reload.is_some()
            || !magazine.reloads()
            || rounds >= magazine.size
            || !can_fill(a, magazine)
        {
            return false;
        }
        let scripted = magazine.scripted();
        if scripted
            && !magazine.light_states.is_empty()
            && !a.images[0]
                .as_ref()
                .and_then(|e| self.pack.images.get(&e.image)?.states.get(e.state))
                .is_some_and(|s| {
                    magazine
                        .light_states
                        .iter()
                        .any(|n| n.eq_ignore_ascii_case(&s.name))
                })
        {
            return false;
        }
        a.reload = Some(Reload {
            item: key.clone(),
            // A scripted magazine's rounds wait for its reload state.
            done: if scripted {
                u64::MAX
            } else {
                self.tick + u64::from(magazine.reload_ticks)
            },
        });
        if let Some(image) = a.images[0].as_ref().map(|e| e.image.clone()) {
            self.magazine_flags(a, &image, &key, magazine);
        }
        if let Some(check) = &magazine.on_reload {
            Self::apply_check(a, &key, magazine, check);
        }
        if !magazine.reload_sequence.is_empty() {
            self.animation(id, &magazine.reload_sequence);
        }
        if !magazine.reload_sound.is_empty() {
            self.events.push(Event::Sound {
                source: TargetId::Actor(id),
                profile: magazine.reload_sound.clone(),
                position: a.frame.position,
            });
        }
        self.events.push(Event::Ammo { actor: id });
        true
    }
    /// A reload whose time is up moves its rounds: all it can, or one and
    /// the next one's wait.
    fn finish_reload(&mut self, id: ActorId, a: &mut Actor) {
        let Some(reload) = a.reload.clone().filter(|r| self.tick >= r.done) else {
            return;
        };
        a.reload = None;
        let Some((key, magazine)) = self.magazine_of(a).filter(|(k, _)| *k == reload.item) else {
            return;
        };
        let rounds = a.rounds.get(&key).copied().unwrap_or(0);
        // A load of several rounds into a magazine with less room loses the
        // rest, as the Paired Shotgun's chamber check threw its second
        // shell away.
        let wanted = if magazine.one_by_one {
            magazine.per_load
        } else {
            magazine.size.saturating_sub(rounds)
        };
        let taken = match magazine.supply {
            // Filled from nothing: the reserve only says it may reload.
            crate::Supply::Endless | crate::Supply::Both => wanted,
            _ => a
                .reserve
                .get_mut(&magazine.ammo)
                .map_or(0, |r| r.take(wanted)),
        };
        let rounds = (rounds + taken).min(magazine.size);
        a.rounds.insert(key.clone(), rounds);
        self.events.push(Event::Ammo { actor: id });
        if magazine.one_by_one && rounds < magazine.size && !magazine.scripted() {
            let more = can_fill(a, &magazine);
            if more {
                a.reload = Some(Reload {
                    item: key.clone(),
                    done: self.tick + u64::from(magazine.reload_ticks),
                });
                if !magazine.reload_sound.is_empty() {
                    self.events.push(Event::Sound {
                        source: TargetId::Actor(id),
                        profile: magazine.reload_sound.clone(),
                        position: a.frame.position,
                    });
                }
            }
        }
        if let Some(image) = a.images[0].as_ref().map(|e| e.image.clone()) {
            self.magazine_flags(a, &image, &key, &magazine);
        }
        if let Some(check) = &magazine.on_loaded {
            Self::apply_check(a, &key, &magazine, check);
        }
    }
    /// The right hand's flags from its magazine ([`crate::Magazine`]): for
    /// an image whose states use `loaded`, loaded while there is a shot and
    /// no reload, with ammo while there is reserve to reload from (as
    /// Tier+Tactical's `TT_onLoadCheck` set them); for any other, ammo
    /// while there is a shot and no reload.
    fn magazine_flags(&self, a: &mut Actor, image: &str, key: &str, magazine: &crate::Magazine) {
        if !magazine.checks.is_empty() {
            // Its state scripts set the flags (`apply_check`).
            return;
        }
        let shot = has_shot(a, key, magazine) && a.reload.is_none();
        let uses_loaded = self.pack.images.get(image).is_some_and(|i| {
            i.states
                .iter()
                .any(|s| s.loaded.is_some() || s.not_loaded.is_some())
        });
        if uses_loaded {
            a.loaded = shot;
            a.ammo = can_fill(a, magazine);
        } else {
            a.ammo = shot;
        }
    }
    /// A [`crate::Check`] on the right hand's flags, from `key`'s magazine.
    /// Whether it took a shot's rounds ([`crate::Check::spend`]).
    fn apply_check(
        a: &mut Actor,
        key: &str,
        magazine: &crate::Magazine,
        check: &crate::Check,
    ) -> bool {
        // Under Both a shot needs its rounds in the reserve as well: short
        // of them, the magazine reads as empty.
        let mut rounds = rounds_in(a, key, magazine);
        if magazine.fires(rounds) && !has_shot(a, key, magazine) {
            rounds = 0;
        }
        let reserve = can_fill(a, magazine);
        if let Some(c) = &check.loaded {
            a.loaded = c.holds(magazine, rounds, reserve);
        }
        if let Some(c) = &check.ammo {
            a.ammo = c.holds(magazine, rounds, reserve);
        }
        if check.keeps_reload && a.reload.as_ref().is_some_and(|r| r.item == key) {
            a.loaded = false;
        }
        if check.spend && a.loaded && magazine.fires(rounds) {
            take_shot(a, key, magazine);
            return true;
        }
        false
    }
    /// The light key: reload the held gun if it has a magazine that is
    /// not full and reserve to fill it. Whether a reload started.
    pub fn reload(&mut self, id: ActorId) -> Result<bool> {
        ensure!(self.events.len() < 8192, "Command event budget");
        let mut a = self.actors.remove(&id).context("Unknown actor")?;
        let started = self.start_reload(id, &mut a);
        self.actors.insert(id, a);
        Ok(started)
    }
    /// A fuse burning in the hand ([`crate::Cook`]): its countdown, and
    /// going off when it runs out. Put out when the grenade is put away.
    fn burn_fuse(&mut self, id: ActorId, a: &mut Actor) {
        let Some(cooking) = a.cook.clone() else {
            return;
        };
        let held = a.images[0].as_ref().map(|e| e.image.as_str());
        let Some((image, cook)) = self
            .pack
            .images
            .get(&cooking.image)
            .and_then(|i| Some((i.clone(), i.cook.clone()?)))
            .filter(|_| held == Some(cooking.image.as_str()))
        else {
            a.cook = None;
            return;
        };
        let burned = self
            .tick
            .saturating_sub(cooking.lit)
            .min(u64::from(u32::MAX)) as u32;
        if burned >= cook.fuse_ticks {
            a.cook = None;
            if let Some(projectile) = &image.projectile {
                let at = a.frame.position + Vec3::Y * cook.burst_height;
                if let Err(error) = self.spawn_explosion(projectile, id, at, 1.0) {
                    self.events.push(Event::Diagnostic {
                        actor: Some(id),
                        message: format!("Cooked {}: {error}", image.id),
                    });
                }
            }
            self.unmount(id, a);
            return;
        }
        if let Some(text) = cook.print_at(burned) {
            self.events.push(Event::Print {
                actor: id,
                text,
                seconds: cook.print_seconds,
            });
        }
    }
    /// The light key with a gun in hand: whether the gun took it. A gun
    /// with a magazine reloads; one whose magazine names `light_states`
    /// leaves the key to the light outside those states or when no reload
    /// starts ([`crate::Magazine::light_states`]).
    pub fn light_key(&mut self, id: ActorId) -> Result<bool> {
        let Some(magazine) = self
            .actors
            .get(&id)
            .and_then(|a| self.magazine_of(a))
            .map(|(_, m)| m)
        else {
            return Ok(false);
        };
        // A grenade, or a gun that never reloads, has nothing to reload:
        // the key works the light.
        if !magazine.reloads() {
            return Ok(false);
        }
        if magazine.light_states.is_empty() {
            self.reload(id)?;
            return Ok(true);
        }
        let in_state = self.image_state(id, 0).is_some_and(|(_, state)| {
            magazine
                .light_states
                .iter()
                .any(|s| s.eq_ignore_ascii_case(&state.name))
        });
        let reloads = in_state && self.reload(id)?;
        // A gun short of rounds with nothing to load shows what it has
        // (`TT_onUseLight` with no reserve), and the key works the light.
        // (Tier's display is up for a time, or until put away when that
        // time is 0; its dry pulls show it again.)
        if !reloads
            && magazine.displayed()
            && (magazine.display_ticks > 0 || !magazine.display_scripts.is_empty())
            && let Some(view) = self.ammo(id)
            && view.rounds < view.size
            && !self.actors.get(&id).is_some_and(|a| can_fill(a, &magazine))
        {
            self.events.push(Event::Ammo { actor: id });
        }
        Ok(reloads)
    }
    /// The held gun's magazine and reserve, when it has one, or the
    /// selected grenade's that left the hand for want of reserve.
    pub fn ammo(&self, id: ActorId) -> Option<AmmoView> {
        let a = self.actors.get(&id)?;
        let (item, magazine) = self.magazine_of(a).or_else(|| self.stowed(a))?;
        let rounds = rounds_in(a, &item, &magazine);
        let reserve = a
            .reserve
            .get(&magazine.ammo)
            .copied()
            .unwrap_or(Reserve::Rounds(0));
        Some(ammo_view(
            &item,
            magazine,
            rounds,
            reserve,
            a.reload.is_some(),
        ))
    }
    /// Read the magazine that equipping this inventory slot would provide.
    /// A never-drawn slot starts full, and the first magazine of an ammo
    /// kind supplies its authored starting reserve. Existing empty magazines
    /// and shared reserves stay empty. This projection changes no state and
    /// does not promise that a busy image can switch immediately.
    pub fn ammo_on_equip(&self, id: ActorId, slot: usize) -> Option<AmmoView> {
        let a = self.actors.get(&id)?;
        let item = a.inventory.get(slot)?.as_ref()?;
        let image = &self.pack.items.get(item)?.image;
        let magazine = self.pack.images.get(image)?.magazine.clone()?;
        let key = slot_key(item, slot);
        let reserve = a
            .reserve
            .get(&magazine.ammo)
            .copied()
            .unwrap_or(Reserve::Rounds(magazine.reserve.min(magazine.max_reserve)));
        let stored = a.rounds.get(&key).copied().unwrap_or(magazine.size);
        let rounds = magazine_rounds(&magazine, stored, reserve);
        let reloading = a.reload.as_ref().is_some_and(|r| r.item == key);
        Some(ammo_view(&key, magazine, rounds, reserve, reloading))
    }
    /// Every reserve a holder has, by ammo name.
    pub fn reserves(&self, id: ActorId) -> Option<&BTreeMap<String, Reserve>> {
        Some(&self.actors.get(&id)?.reserve)
    }
    /// A holder's reserve of `ammo`.
    pub fn reserve(&self, id: ActorId, ammo: &str) -> Option<Reserve> {
        self.actors.get(&id)?.reserve.get(ammo).copied()
    }
    /// Set a holder's reserve of `ammo` (an ammo box, an endless-ammo
    /// rule), capped at 100000 rounds.
    pub fn set_reserve(&mut self, id: ActorId, ammo: &str, reserve: Reserve) -> Result<()> {
        ensure!(
            (1..=32).contains(&ammo.len())
                && ammo
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
            "Invalid ammo name"
        );
        let a = self.actors.get_mut(&id).context("Unknown actor")?;
        ensure!(
            a.reserve.len() < 64 || a.reserve.contains_key(ammo),
            "Too many ammo types"
        );
        let reserve = match reserve {
            Reserve::Rounds(n) => Reserve::Rounds(n.min(100_000)),
            Reserve::Endless => Reserve::Endless,
        };
        a.reserve.insert(ammo.to_string(), reserve);
        let mut a = self.actors.remove(&id).expect("checked");
        // A grenade put away for want of reserve comes back into the hand.
        if let Some((key, magazine)) = self.stowed(&a)
            && magazine.ammo == ammo
            && magazine.fires(rounds_in(&a, &key, &magazine))
            && let Some(image) = self.pack.items.get(key_item(&key)).map(|i| i.image.clone())
        {
            self.swap_images(id, &mut a, NextImage { image, paint: None });
        }
        // An empty gun waiting on reserve reloads as soon as it has some.
        if let Some((key, magazine)) = self.magazine_of(&a) {
            if a.rounds.get(&key).copied().unwrap_or(0) < magazine.per_shot {
                self.start_reload(id, &mut a);
            }
            if let Some(image) = a.images[0].as_ref().map(|e| e.image.clone()) {
                self.magazine_flags(&mut a, &image, &key, &magazine);
            }
        }
        self.actors.insert(id, a);
        self.events.push(Event::Ammo { actor: id });
        Ok(())
    }
    /// Add `rounds` to a holder's reserve of `ammo` (an ammo pickup), up to
    /// the most any magazine of that ammo carries. An endless reserve stays
    /// endless.
    pub fn give_ammo(&mut self, id: ActorId, ammo: &str, rounds: u32) -> Result<()> {
        let cap = self
            .pack
            .images
            .values()
            .filter_map(|i| i.magazine.as_ref())
            .filter(|m| m.ammo == ammo)
            .map(|m| m.max_reserve)
            .max()
            .unwrap_or(100_000);
        let reserve = match self.reserve(id, ammo).unwrap_or(Reserve::Rounds(0)) {
            Reserve::Rounds(n) => Reserve::Rounds(n.saturating_add(rounds).min(cap)),
            Reserve::Endless => Reserve::Endless,
        };
        self.set_reserve(id, ammo, reserve)
    }
    /// Set the rounds in a holder's magazine of `item`, up to its size: the
    /// one in hand if they hold it, else the first they carry.
    pub fn set_rounds(&mut self, id: ActorId, item: &str, rounds: u32) -> Result<()> {
        let a = self.actors.get(&id).context("Unknown actor")?;
        let holds = |slot: usize| a.inventory.get(slot).and_then(Option::as_deref) == Some(item);
        let slot = a
            .selected
            .filter(|s| holds(*s))
            .or_else(|| (0..a.inventory.len()).find(|s| holds(*s)))
            .context("They carry no such item")?;
        self.set_slot_rounds(id, slot, rounds)
    }
    /// Set the rounds in the magazine of the gun in tool `slot`.
    fn set_slot_rounds(&mut self, id: ActorId, slot: usize, rounds: u32) -> Result<()> {
        let a = self.actors.get(&id).context("Unknown actor")?;
        let item = a
            .inventory
            .get(slot)
            .and_then(Option::as_ref)
            .context("Empty slot")?;
        let size = self
            .pack
            .items
            .get(item)
            .and_then(|i| self.pack.images.get(&i.image)?.magazine.as_ref())
            .filter(|m| !m.from_reserve)
            .map(|m| m.size)
            .context("That item has no magazine")?;
        let item = slot_key(item, slot);
        let mut a = self.actors.remove(&id).context("Unknown actor")?;
        a.rounds.insert(item.clone(), rounds.min(size));
        if let Some((key, magazine)) = self.magazine_of(&a)
            && key == item
            && let Some(image) = a.images[0].as_ref().map(|e| e.image.clone())
        {
            self.magazine_flags(&mut a, &image, &key, &magazine);
        }
        self.actors.insert(id, a);
        self.events.push(Event::Ammo { actor: id });
        Ok(())
    }
    /// Set the rounds in a dropped gun's magazine, for whoever picks it up
    /// (v20 Add-Ons' `%item.mag`), up to its magazine's size.
    pub fn set_drop_rounds(&mut self, drop: u64, rounds: u32) -> Result<()> {
        let d = self.drops.get_mut(&drop).context("Unknown drop")?;
        let size = self
            .pack
            .items
            .get(&d.item)
            .and_then(|i| self.pack.images.get(&i.image)?.magazine.as_ref())
            .filter(|m| !m.from_reserve)
            .map(|m| m.size)
            .context("That item has no magazine")?;
        d.rounds = Some(rounds.min(size));
        Ok(())
    }
    /// A fresh life: magazines full again and reserves back to each gun's
    /// starting amount when next drawn.
    /// A new body for the holder: animations its states scheduled on the
    /// old one are gone with it (their sounds still play, as v20's global
    /// `schedule` did).
    pub fn respawned(&mut self, id: ActorId) -> Result<()> {
        let a = self.actors.get_mut(&id).context("Unknown actor")?;
        // The new body wears nothing in its emote slot, and a guard it
        // takes up starts whole.
        a.emote = None;
        a.guard_left = None;
        a.cues.retain(|c| !c.cue.sound.is_empty());
        for c in &mut a.cues {
            c.cue.thread = None;
            c.cue.sequence.clear();
        }
        Ok(())
    }
    /// `Player::emote(%image)`: mount `image` in [`EMOTE_SLOT`], replacing
    /// the one there, whose `unmount` command runs; `None`, or an image this
    /// pack lacks, empties the slot (`unMountImage(3)`). The image's `mount`
    /// command runs as it goes on.
    pub fn emote(&mut self, id: ActorId, image: Option<&str>) -> Result<()> {
        ensure!(
            self.events.len() < 8192,
            "Command event budget; advance/drain before retry"
        );
        let a = self.actors.get_mut(&id).context("Unknown actor")?;
        let old = a.emote.take();
        if let Some(old) = old {
            self.put_away(id, &old);
        }
        let Some(image) = image.filter(|i| self.pack.images.contains_key(*i)) else {
            return Ok(());
        };
        let a = self.actors.get_mut(&id).expect("checked");
        a.emote = Some(Equipped {
            image: image.into(),
            state: 0,
            remaining: 0,
            entered: false,
            trigger: false,
            hand: EMOTE_SLOT,
            paint: None,
            magazine: None,
        });
        if let Some(command) = self.pack.images[image].commands.mount.clone() {
            self.events.push(Event::ToolFire {
                actor: id,
                image: image.into(),
                hand: EMOTE_SLOT,
                command: Some(command),
                tool: None,
            });
        }
        Ok(())
    }
    /// The image in a holder's [`EMOTE_SLOT`] and the name of its state.
    pub fn emote_state(&self, id: ActorId) -> Option<(&str, &str)> {
        let e = self.actors.get(&id)?.emote.as_ref()?;
        let image = self.pack.images.get(&e.image)?;
        Some((&image.id, &image.states.get(e.state)?.name))
    }
    /// The [`EMOTE_SLOT`] image's states, one tick: they follow their
    /// timeouts, as slot 3 has no trigger and an image mounts loaded and
    /// without ammo. A state script with a command runs it for the wearer.
    /// The slot shows nothing itself: clients play the image from the cue
    /// that mounted it. False when the image is gone.
    fn advance_emote(&mut self, id: ActorId, e: &mut Equipped) -> bool {
        let pack = self.pack.clone();
        let Some(image) = pack.images.get(&e.image) else {
            return false;
        };
        if image.states.is_empty() {
            return true;
        }
        if e.entered && e.remaining > 0 {
            e.remaining -= 1;
        }
        for _ in 0..16 {
            let state = &image.states[e.state];
            if !e.entered {
                e.entered = true;
                e.remaining = state.ticks;
                if let Some(command) = image.commands.for_script(&state.script) {
                    self.events.push(Event::ToolFire {
                        actor: id,
                        image: image.id.clone(),
                        hand: EMOTE_SLOT,
                        command: Some(command.clone()),
                        tool: None,
                    });
                }
            }
            if e.remaining > 0 && state.wait {
                return true;
            }
            let next = state
                .loaded
                .or(state.no_ammo)
                .or(state.up)
                .or(if e.remaining == 0 {
                    state.timeout
                } else {
                    None
                });
            match next {
                // A zero-time state that leads to itself waits a tick.
                Some(next) if next != e.state || state.ticks > 0 => {
                    e.state = next;
                    e.entered = false;
                }
                _ => return true,
            }
        }
        self.events.push(Event::Diagnostic {
            actor: Some(id),
            message: format!(
                "Image instantaneous transition budget exceeded: {}",
                e.image
            ),
        });
        false
    }
    pub fn reset_ammo(&mut self, id: ActorId) -> Result<()> {
        let a = self.actors.get_mut(&id).context("Unknown actor")?;
        a.rounds.clear();
        a.reserve.clear();
        a.reload = None;
        Ok(())
    }
    /// `Player::mountImage(%image, 0)` from an Add-On's rules: put `image`
    /// in the right hand (a scope, a second fire mode) and keep the selected
    /// tool slot, as v20's `mountImage` did. `None` puts back the selected
    /// tool's own image, or empties the hand. Like v20 it ignores the held
    /// image's `allow_change`: the rules decide.
    pub fn swap_image(&mut self, id: ActorId, image: Option<&str>) -> Result<()> {
        ensure!(
            self.events.len() < 8192,
            "Command event budget; advance/drain before retry"
        );
        let a = self.actors.get(&id).context("Unknown actor")?;
        let image = match image {
            Some(image) => {
                ensure!(self.pack.images.contains_key(image), "Unknown image");
                Some(image.to_string())
            }
            None => a
                .selected
                .and_then(|slot| a.inventory.get(slot)?.as_ref())
                .and_then(|item| self.pack.items.get(item))
                .map(|item| item.image.clone()),
        };
        let mut a = self.actors.remove(&id).expect("checked");
        // The rules' image wins over a switch still waiting on the old one.
        a.next = None;
        for hand in 0..2u8 {
            if a.images[hand as usize].take().is_some() {
                self.events.push(Event::Unmounted { actor: id, hand });
            }
        }
        if let Some(image) = image {
            self.mount(id, &mut a, &image, 0);
            if let Some(left) = self.left_image(&image) {
                self.mount(id, &mut a, &left, 1);
            }
        }
        self.actors.insert(id, a);
        Ok(())
    }
    /// An image leaves the emote slot: its `unmount` command runs. The
    /// hands' `mount` and `unmount` commands run from the host, which sees
    /// the right hand's image change from tick to tick.
    fn put_away(&mut self, id: ActorId, old: &Equipped) {
        if let Some(command) = self
            .pack
            .images
            .get(&old.image)
            .and_then(|i| i.commands.unmount.clone())
        {
            self.events.push(Event::ToolFire {
                actor: id,
                image: old.image.clone(),
                hand: old.hand,
                command: Some(command),
                tool: None,
            });
        }
    }
    fn mount(&mut self, id: ActorId, a: &mut Actor, image: &str, hand: u8) {
        if self.pack.images.contains_key(image) {
            // `ShapeBase::mountImage(%image, %slot, %loaded = true)` and
            // `WeaponImage::onMount`'s `setImageAmmo(%slot, 1)`: every image
            // put in the hand starts loaded and with ammo. The flags are the
            // hand's, so an emptied gun must not leave the next one empty.
            if hand == 0 {
                a.ammo = true;
                a.loaded = true;
            }
            let magazine = if hand == 0 {
                self.load_magazine(a, image)
            } else {
                None
            };
            let shown = magazine.is_some();
            a.images[hand as usize] = Some(Equipped {
                image: image.into(),
                state: 0,
                remaining: 0,
                entered: false,
                trigger: hand == 0 && a.trigger,
                hand,
                paint: None,
                magazine,
            });
            if self
                .pack
                .images
                .get(image)
                .is_some_and(|i| i.sport.and_then(|s| s.keys) == Some(SportKeys::Pass))
            {
                self.events.push(Event::SportMovement {
                    actor: id,
                    locked: !a.frame.can_jet,
                });
            }
            self.events.push(Event::Mounted {
                actor: id,
                image: image.into(),
                hand,
            });
            if shown {
                self.events.push(Event::Ammo { actor: id });
            }
        }
    }
    /// `Player::mountImage(%image, %slot)` for a worn slot (2 or 3) from an
    /// Add-On's rules: a carried flag on the back. `paint` tints it with a
    /// palette colour (a team's). `None` takes it off. Worn images stay
    /// through tool changes; death and respawn take them off.
    pub fn wear(
        &mut self,
        id: ActorId,
        slot: u8,
        image: Option<&str>,
        paint: Option<u8>,
    ) -> Result<()> {
        ensure!(
            self.events.len() < 8192,
            "Command event budget; advance/drain before retry"
        );
        ensure!(
            WORN_SLOTS.contains(&usize::from(slot)),
            "Worn image slots are 2 and 3"
        );
        if let Some(image) = image {
            ensure!(self.pack.images.contains_key(image), "Unknown image");
        }
        let mut a = self.actors.remove(&id).context("Unknown actor")?;
        if a.images[usize::from(slot)].take().is_some() {
            self.events.push(Event::Unmounted {
                actor: id,
                hand: slot,
            });
        }
        if let Some(image) = image {
            self.mount(id, &mut a, image, slot);
            if let Some(e) = &mut a.images[usize::from(slot)] {
                e.paint = paint;
            }
        }
        self.actors.insert(id, a);
        Ok(())
    }
    /// Take off every worn image (death, a new body).
    pub fn clear_worn(&mut self, id: ActorId) {
        if let Some(a) = self.actors.get_mut(&id) {
            Self::take_worn(&mut self.events, id, a);
        }
    }
    fn take_worn(events: &mut Vec<Event>, id: ActorId, a: &mut Actor) {
        for slot in WORN_SLOTS {
            if a.images[slot].take().is_some() {
                events.push(Event::Unmounted {
                    actor: id,
                    hand: slot as u8,
                });
            }
        }
    }
    /// Empty both hands (tools away, a new tool, death).
    fn unmount(&mut self, id: ActorId, a: &mut Actor) {
        if a.images[..HAND_SLOTS].iter().any(Option::is_some) {
            self.events.push(Event::SportMovement {
                actor: id,
                locked: false,
            });
        }
        for (hand, image) in a.images[..HAND_SLOTS].iter_mut().enumerate() {
            if image.take().is_some() {
                self.events.push(Event::Unmounted {
                    actor: id,
                    hand: hand as u8,
                });
            }
        }
        a.next = None;
        a.selected = None;
        a.cook = None;
    }
    /// The held fire button. It is the player's, so it holds across image
    /// changes, colour cans, empty hands and mid-fire switches, as v20's
    /// move trigger does; only a release (or the host: death, a lapsed
    /// input lease) lets it go.
    pub fn trigger(&mut self, id: ActorId, down: bool) -> Result<()> {
        let a = self.actors.get_mut(&id).context("Unknown actor")?;
        a.trigger = down;
        if let Some(e) = &mut a.images[0] {
            e.trigger = down;
        }
        Ok(())
    }
    /// Abandon a held charge without taking its release-to-fire transition.
    /// Restart the same hand images through the normal unmount/mount events,
    /// preserving selection and paint. Worn images and inventory are untouched.
    /// Hosts must clear any queued presses before using this cancellation.
    pub fn cancel_charge(&mut self, id: ActorId) -> Result<bool> {
        let a = self.actors.get(&id).context("Unknown actor")?;
        if !a.images[0]
            .as_ref()
            .is_some_and(|e| self.pack.images[&e.image].charges())
        {
            return Ok(false);
        }
        ensure!(
            self.events.len() < 8192,
            "Command event budget; advance/drain before retry"
        );
        let image = a.images[0].as_ref().expect("checked");
        let restart = NextImage {
            image: image.image.clone(),
            paint: image.paint,
        };
        let mut a = self.actors.remove(&id).expect("checked");
        let flags = (a.ammo, a.loaded);
        let pending = a.next.take();
        a.trigger = false;
        self.swap_images(id, &mut a, restart);
        (a.ammo, a.loaded) = flags;
        a.next = pending;
        self.actors.insert(id, a);
        Ok(true)
    }

    /// Sports movement trigger switches dribble/standing presentation to shoot mode.
    pub fn sport_trigger(&mut self, id: ActorId, trigger: u8, down: bool) -> Result<()> {
        ensure!(self.events.len() < 8192, "Command event budget");
        ensure!((2..=4).contains(&trigger), "Invalid sport trigger");
        let a = self.actors.get(&id).context("Unknown actor")?;
        let held = a.images[0]
            .as_ref()
            .and_then(|e| self.pack.images.get(&e.image));
        let jet = trigger == 4 && !a.frame.can_jet;
        let action = match held.and_then(|i| i.sport).and_then(|s| s.keys) {
            Some(SportKeys::Lateral) if jet && down => Some(SportAction::FootballLateral),
            Some(SportKeys::Pop) => Some(if trigger == 4 && down {
                SportAction::SoccerPop
            } else {
                SportAction::SoccerDrop
            }),
            Some(SportKeys::Pass) if jet && !down => Some(SportAction::BasketballPass),
            _ => None,
        };
        if let Some(action) = action {
            self.sport_action(id, action)?;
            return Ok(());
        }
        // A ball's other keys swap it as its `onFire` does
        // (`basketballImage::onBallTrigger`).
        if down
            && let Some(OnFire::Mount(next)) = held
                .filter(|i| i.sport.is_some())
                .and_then(|i| i.on_fire.clone())
        {
            let mut a = self.actors.remove(&id).unwrap();
            self.mount(id, &mut a, &next, 0);
            self.actors.insert(id, a);
        }
        Ok(())
    }
    pub fn drop_item(&mut self, id: ActorId, slot: usize) -> Result<u64> {
        ensure!(self.events.len() < 8192, "Command event budget");
        ensure!(self.drops.len() < MAX_DROPS, "Drop budget");
        let a = self.actors.get(&id).context("Unknown actor")?;
        let item = a
            .inventory
            .get(slot)
            .and_then(Option::as_ref)
            .context("Empty slot")?
            .clone();
        ensure!(
            self.pack
                .items
                .get(&item)
                .map_or_else(|| CORE_TOOLS.contains(&item.as_str()), |item| item.can_drop,),
            "Item cannot drop"
        );
        // v20 ServerCmdDropTool: feet + 1.5*zScale + eyeVector; no
        // inherited player velocity, throw speed20*zScale, pop after10s.
        let pos =
            a.frame.position + Vec3::Y * (1.5 * a.frame.scale) + a.frame.direction.normalize();
        let vel = a.frame.direction.normalize() * (20.0 * a.frame.scale);
        let scale = a.frame.scale;
        let rotation = Quat::from_rotation_y(-a.frame.body_yaw);
        let paint = self
            .pack
            .items
            .get(&item)
            .and_then(|i| self.pack.images.get(&i.image))
            .filter(|i| i.paint_tint)
            .map(|_| a.spray);
        if a.selected == Some(slot) {
            self.equip(id, None)?;
        }
        let a = self.actors.get_mut(&id).unwrap();
        a.inventory[slot] = None;
        // The magazine goes with the gun (`servercmdDropTool` handing
        // `TT_toolAmmo[%slot]` to the item); another of it keeps its own.
        let rounds = a.rounds.remove(&slot_key(&item, slot));
        let drop = self.next_id;
        self.next_id += 1;
        self.drops.insert(
            drop,
            Drop {
                rotation,
                scale,
                id: drop,
                item: item.clone(),
                position: pos,
                velocity: vel,
                source: id,
                pickup_after: self.tick + DROP_PICKUP_DELAY_TICKS,
                expires: self.tick + DROP_LIFETIME_TICKS,
                rounds,
                paint,
                name: None,
            },
        );
        self.events.push(Event::Dropped {
            drop,
            item,
            position: pos,
            velocity: vel,
        });
        Ok(drop)
    }
    /// A world drop not thrown by an actor (the `spawnItem` brick event).
    /// Anyone may pick it up at once; it pops after ten seconds.
    pub fn spawn_drop(&mut self, item: &str, position: Vec3, velocity: Vec3) -> Result<u64> {
        self.spawn_drop_with(item, position, velocity, DropLook::default())
    }
    /// [`Self::spawn_drop`] tinted and lasting as `look` says (a dropped
    /// flag waiting to go home).
    pub fn spawn_drop_with(
        &mut self,
        item: &str,
        position: Vec3,
        velocity: Vec3,
        look: DropLook,
    ) -> Result<u64> {
        ensure!(
            (1..=MAX_DROP_TICKS).contains(&look.lifetime),
            "Drop lifetime out of range"
        );
        ensure!(self.events.len() < 8192, "Command event budget");
        ensure!(self.drops.len() < MAX_DROPS, "Drop budget");
        ensure!(self.contains_item(item), "Unknown item");
        ensure!(
            position.is_finite()
                && velocity.is_finite()
                && position.abs().max_element() < 1e7
                && velocity.length() <= 200.0,
            "Invalid drop input"
        );
        let drop = self.next_id;
        self.next_id += 1;
        self.drops.insert(
            drop,
            Drop {
                rotation: Quat::IDENTITY,
                scale: 1.0,
                id: drop,
                item: item.into(),
                position,
                velocity,
                source: ActorId::NOBODY,
                pickup_after: self.tick,
                expires: self.tick + look.lifetime,
                rounds: None,
                paint: look.paint,
                name: None,
            },
        );
        self.events.push(Event::Dropped {
            drop,
            item: item.into(),
            position,
            velocity,
        });
        Ok(drop)
    }
    /// Whether `id` may pick up `drop` now: it exists and, if they threw
    /// it, its throw cooldown is over.
    pub fn pickup_ready(&self, id: ActorId, drop: u64) -> bool {
        self.drops
            .get(&drop)
            .is_some_and(|d| id != d.source || self.tick >= d.pickup_after)
    }
    /// Name a world drop, or take its name away (`setShapeName`).
    pub fn set_drop_name(&mut self, drop: u64, name: Option<DropName>) -> Result<()> {
        if let Some(n) = &name {
            ensure!(
                n.text.chars().count() <= MAX_DROP_NAME && !n.text.chars().any(char::is_control),
                "A dropped item's name is at most {MAX_DROP_NAME} characters"
            );
        }
        self.drops
            .get_mut(&drop)
            .context("No such dropped item")?
            .name = name;
        Ok(())
    }
    /// Delete a world drop without giving it to anyone (an Add-On used it
    /// up where it lay).
    pub fn remove_drop(&mut self, drop: u64) -> bool {
        let removed = self.drops.remove(&drop).is_some();
        if removed {
            self.events.push(Event::DropRemoved { drop });
        }
        removed
    }
    /// Take one `item` out of an actor's tools (`%obj.tool[%slot] = 0`):
    /// the selected slot if it holds one, else the first that does. A held
    /// item is put away first. The slot it came from, or `None` when they
    /// carry none.
    pub fn take_item(&mut self, id: ActorId, item: &str) -> Result<Option<usize>> {
        ensure!(self.events.len() < 8192, "Command event budget");
        let a = self.actors.get(&id).context("Unknown actor")?;
        let holds = |slot: usize| a.inventory.get(slot).and_then(Option::as_deref) == Some(item);
        let Some(slot) = a
            .selected
            .filter(|s| holds(*s))
            .or_else(|| (0..a.inventory.len()).find(|s| holds(*s)))
        else {
            return Ok(None);
        };
        if a.selected == Some(slot) {
            self.equip(id, None)?;
        }
        let a = self.actors.get_mut(&id).expect("checked");
        a.inventory[slot] = None;
        a.rounds.remove(&slot_key(item, slot));
        Ok(Some(slot))
    }
    /// Host validates contact and minigame permission. Thrower exclusion applies only to its source.
    pub fn pickup(&mut self, id: ActorId, drop: u64) -> Result<usize> {
        ensure!(self.events.len() < 8192, "Command event budget");
        let d = self.drops.get(&drop).context("Unknown drop")?;
        ensure!(
            id != d.source || self.tick >= d.pickup_after,
            "Pickup cooldown"
        );
        let item = d.item.clone();
        let rounds = d.rounds;
        let slot = self.give(id, &item)?;
        // Its own magazine comes with it, into the slot it lands in; one
        // thrown with none comes full (`Player::pickup`).
        if let Some(rounds) = rounds {
            self.set_slot_rounds(id, slot, rounds)?;
        }
        self.drops.remove(&drop);
        self.events.push(Event::DropRemoved { drop });
        Ok(slot)
    }
    /// Vehicle/event adapters pass explicit already-authorized source identity and native velocity.
    pub fn spawn(
        &mut self,
        definition: &str,
        source: ActorId,
        position: Vec3,
        velocity: Vec3,
        scale: f32,
    ) -> Result<u64> {
        ensure!(
            self.events.len() < 8192 && self.projectiles.len() < MAX_PROJECTILES,
            "Projectile/event budget"
        );
        ensure!(
            self.pack.projectiles.contains_key(definition),
            "Unknown projectile"
        );
        ensure!(
            position.is_finite()
                && velocity.is_finite()
                && position.abs().max_element() < 1e7
                && velocity.length() <= 10000.0
                && (0.01..=100.0).contains(&scale),
            "Invalid projectile input"
        );
        let id = self.next_id;
        self.next_id += 1;
        self.projectiles.insert(
            id,
            Projectile {
                id,
                definition: definition.into(),
                source,
                position,
                velocity,
                scale,
                age: 0,
                bounced: false,
                stuck: false,
                origin: position,
                was_thrown: false,
                paint: None,
                heading: None,
                bounces: 0,
                spawned: self.tick,
            },
        );
        self.events.push(Event::Spawned {
            projectile: id,
            definition: definition.into(),
            source,
            position,
            velocity,
        });
        Ok(id)
    }
    /// `fxDTSBrick::spawnExplosion`: a projectile made where it is and
    /// exploded at once (`%p.explode()`), without flying or becoming a live
    /// projectile. It explodes at the start of the next tick, where the host's
    /// world can be queried; at most [`MAX_EXPLOSIONS_PER_TICK`] wait.
    pub fn spawn_explosion(
        &mut self,
        definition: &str,
        source: ActorId,
        position: Vec3,
        scale: f32,
    ) -> Result<()> {
        ensure!(
            self.explosions.len() < MAX_EXPLOSIONS_PER_TICK && self.events.len() < 8192,
            "Explosion budget for this tick"
        );
        ensure!(
            self.pack.projectiles.contains_key(definition),
            "Unknown projectile"
        );
        ensure!(
            position.is_finite()
                && position.abs().max_element() < 1e7
                && (0.01..=100.0).contains(&scale),
            "Invalid explosion input"
        );
        let id = self.next_id;
        self.next_id += 1;
        self.explosions.push(Projectile {
            id,
            definition: definition.into(),
            source,
            position,
            // `initialVelocity = "0 0 1"`: Torque up, native +Y.
            velocity: Vec3::Y,
            scale,
            age: 0,
            bounced: false,
            stuck: false,
            origin: position,
            was_thrown: false,
            paint: None,
            heading: None,
            bounces: 0,
            spawned: self.tick,
        });
        Ok(())
    }
    /// Advances exactly one 1/120s tick. Drain every returned event before the next tick.
    pub fn step(&mut self, q: &mut impl Query) -> Vec<Event> {
        self.tick += 1;
        for p in std::mem::take(&mut self.explosions) {
            if let Some(d) = self.pack.projectiles.get(&p.definition).cloned() {
                self.explode(&p, &d, q, None);
            }
        }
        let ids: Vec<_> = self.actors.keys().copied().collect();
        for id in ids {
            let mut a = self.actors.remove(&id).unwrap();
            self.finish_reload(id, &mut a);
            self.burn_fuse(id, &mut a);
            self.play_cues(id, &mut a);
            // v20 `Player::updateMove` sets image slot 1's trigger from move
            // trigger 1, which Blockland never sends, before the images run.
            // `AkimboGunImage::onFireAkimbo`'s setImageTrigger(1, 1) is
            // therefore a pulse the left gun sees only in the tick it is set.
            if let Some(left) = &mut a.images[1] {
                left.trigger = false;
            }
            // Slot 0 gets the held move trigger every tick, whichever image
            // is mounted (`setImageTriggerState(0, move->trigger[0])`).
            if let Some(right) = &mut a.images[0] {
                right.trigger = a.trigger;
            }
            for hand in 0..IMAGE_SLOTS {
                // A waiting image mounts and runs in the same tick, once.
                for _ in 0..2 {
                    let Some(mut e) = a.images[hand].take() else {
                        break;
                    };
                    match self.advance(id, &mut a, &mut e, q) {
                        Advance::Keep if a.images[hand].is_none() => a.images[hand] = Some(e),
                        Advance::Keep | Advance::Drop => {}
                        Advance::Switch => {
                            a.images[hand] = Some(e);
                            let next = a.next.take().expect("a switch has a next image");
                            self.swap_images(id, &mut a, next);
                            continue;
                        }
                    }
                    break;
                }
            }
            if let Some(mut e) = a.emote.take()
                && self.advance_emote(id, &mut e)
            {
                a.emote = Some(e);
            }
            self.actors.insert(id, a);
        }
        let ids: Vec<_> = self.projectiles.keys().copied().collect();
        for id in ids {
            let mut p = self.projectiles.remove(&id).unwrap();
            if self.projectile_step(&mut p, q) {
                self.projectiles.insert(id, p);
            } else {
                self.events.push(Event::Removed { projectile: id });
            }
        }
        if !self.fuses.is_empty() {
            let live = &self.projectiles;
            self.fuses.retain(|id, _| live.contains_key(id));
        }
        if !self.reflected.is_empty() {
            let live = &self.projectiles;
            self.reflected.retain(|id, _| live.contains_key(id));
        }
        if !self.sounded.is_empty() {
            let live = &self.projectiles;
            self.sounded.retain(|id, _| live.contains_key(id));
        }
        self.stopped.clear();
        self.guard_hurt();
        // v20 `Item::updatePos`: the item's box falls under gravity 20 and
        // rests on its lowest face, bouncing with elasticity 0.2, friction 0.6.
        for d in self.drops.values_mut() {
            if d.velocity.length_squared() < 0.000001 {
                continue;
            }
            let shape = self.item_bounds.get(&d.item).map(|b| {
                let min = Vec3::from(b.min) * d.scale;
                let max = Vec3::from(b.max) * d.scale;
                (d.rotation * ((min + max) * 0.5), (max - min) * 0.5)
            });
            let (offset, half) = shape.unwrap_or((Vec3::ZERO, Vec3::ZERO));
            // `Item::updateVelocity` with `ShapeBase::updateContainer`: from
            // 10% coverage, buoyancy is density ratio times coverage against
            // gravity, so a density-0.2 item floats a fifth under.
            let rise = shape
                .and_then(|_| {
                    let extent = ItemBounds::lowest(half, d.rotation);
                    let bottom = d.position + offset - Vec3::Y * extent;
                    q.liquid(bottom, extent * 2.0)
                })
                .filter(|l| l.coverage >= 0.1 && l.density.is_finite())
                .map_or(0.0, |l| l.density / ITEM_DENSITY * l.coverage.min(1.0));
            d.velocity.y -= ITEM_GRAVITY * (1.0 - rise) / 120.0;
            let start = d.position + offset;
            let end = start + d.velocity / 120.0;
            let filter = Filter {
                projectile_age_ticks: None,
                source: d.source,
                players: false,
                world_only: true,
            };
            // Through a portal: out of its partner, turned, with the rest of
            // the move, unless something stops it first.
            if let Some((t, carry)) = q.passage(start, end) {
                let at = start.lerp(end, t);
                let blocked = if shape.is_some() {
                    q.sweep_box(start, at, half, d.rotation, filter)
                } else {
                    q.sweep(start, at, filter)
                };
                if blocked.is_none() {
                    let (moved, velocity, turn) = carried(&carry, end, d.velocity);
                    d.rotation = (turn * d.rotation).normalize();
                    d.velocity = velocity;
                    d.position = moved - turn * offset;
                    continue;
                }
            }
            let hit = if shape.is_some() {
                q.sweep_box(start, end, half, d.rotation, filter)
            } else {
                q.sweep(start, end, filter)
            };
            if let Some(hit) = hit {
                if hit.position.is_finite()
                    && hit.normal.is_finite()
                    && hit.normal.length_squared() > 0.1
                {
                    let normal = hit.normal.normalize();
                    let vn = normal * d.velocity.dot(normal);
                    d.position = hit.position - offset + normal * 0.002;
                    d.velocity = ((d.velocity - vn) * 0.4 - vn) * 0.2;
                    if d.velocity.length() < 0.15 {
                        d.velocity = Vec3::ZERO;
                    }
                }
            } else {
                d.position = end - offset;
            }
        }
        let expired: Vec<_> = self
            .drops
            .values()
            .filter(|d| self.tick >= d.expires)
            .map(|d| d.id)
            .collect();
        for drop in expired {
            self.drops.remove(&drop);
            self.events.push(Event::DropRemoved { drop });
        }
        std::mem::take(&mut self.events)
    }
    fn advance(
        &mut self,
        id: ActorId,
        a: &mut Actor,
        e: &mut Equipped,
        q: &mut impl Query,
    ) -> Advance {
        let image = self.pack.images[&e.image].clone();
        if image.states.is_empty() {
            return Advance::Keep;
        }
        if e.entered && e.remaining > 0 {
            e.remaining -= 1;
        }
        // The state the image just left, for `arm_once`.
        let mut left = None;
        for _ in 0..16 {
            let state = &image.states[e.state];
            // A zero-timeout state that times out into itself (the wands'
            // sparkling Ready) restarts its emitter once the emission ends
            // rather than spinning; trigger transitions stay immediate.
            let self_loop = state.ticks == 0 && state.timeout == Some(e.state);
            if !e.entered {
                // `setImageState`: a state that allows image changes mounts
                // the waiting `nextImage` instead of being entered.
                if e.hand == 0 && a.next.is_some() && state.allow_change {
                    return Advance::Switch;
                }
                e.entered = true;
                e.remaining = if self_loop {
                    ((state.emitter_seconds * TICK_HZ as f32).ceil() as u32).max(1)
                } else {
                    state.ticks
                };
                self.events.push(Event::ImageState {
                    actor: id,
                    image: image.id.clone(),
                    state: state.name.clone(),
                    hand: e.hand,
                });
                // A state script that checks the magazine sets the flags.
                if e.hand == 0
                    && let Some(magazine) = &image.magazine
                    && let Some(key) = e.magazine.as_deref()
                    && let Some((_, check)) = magazine
                        .checks
                        .iter()
                        .find(|(s, _)| s.eq_ignore_ascii_case(&state.script))
                    // It spent a round: any reload is off.
                    && Self::apply_check(a, key, magazine, check)
                {
                    a.reload = None;
                    self.events.push(Event::Ammo { actor: id });
                }
                // A state that shows the ammo display again.
                if e.hand == 0
                    && let Some(magazine) = &image.magazine
                    && magazine
                        .display_scripts
                        .iter()
                        .any(|s| s.eq_ignore_ascii_case(&state.script))
                {
                    self.events.push(Event::Ammo { actor: id });
                }
                // The magazine's own reload state: its rounds are due now.
                if e.hand == 0
                    && let Some(magazine) = &image.magazine
                    && !magazine.reload_state.is_empty()
                    && state.script.eq_ignore_ascii_case(&magazine.reload_state)
                    && let Some(key) = e.magazine.as_deref()
                {
                    match a.reload.as_mut() {
                        Some(reload) if reload.item == key => {
                            reload.done = reload.done.min(self.tick);
                        }
                        // A scripted magazine reloads whenever its states
                        // get here, as `onReloaded` ran `TT_reload`.
                        None if magazine.scripted() => {
                            a.reload = Some(Reload {
                                item: key.to_string(),
                                done: self.tick,
                            });
                        }
                        _ => {}
                    }
                }
                if !state.sequence.is_empty() {
                    self.events.push(Event::Animation {
                        actor: id,
                        thread: 0,
                        sequence: state.sequence.clone(),
                        image_hand: Some(e.hand),
                    });
                }
                if !state.arm.is_empty() && (!state.arm_once || left != Some(e.state)) {
                    self.animation(id, &state.arm);
                }
                if !state.gesture.is_empty() {
                    self.events.push(Event::Animation {
                        actor: id,
                        thread: 3,
                        sequence: state.gesture.clone(),
                        image_hand: None,
                    });
                }
                if !state.sound.is_empty() {
                    self.events.push(Event::Sound {
                        source: TargetId::Actor(id),
                        profile: state.sound.clone(),
                        position: a.frame.muzzle[e.hand as usize],
                    });
                }
                for cue in &state.cues {
                    let pending = PendingCue {
                        due: self.tick + cue.ticks(),
                        cue: cue.clone(),
                        position: a.frame.position,
                    };
                    if cue.ticks() == 0 {
                        self.play_cue(id, pending);
                    } else if a.cues.len() < MAX_PENDING_CUES {
                        a.cues.push(pending);
                    }
                }
                if !state.emitter.is_empty() {
                    self.events.push(Event::Effect {
                        source: TargetId::Actor(id),
                        definition: crate::paint_effect(&state.emitter, e.paint),
                        position: a.frame.muzzle[e.hand as usize],
                        node: state.emitter_node.clone(),
                        seconds: state.emitter_seconds,
                        image: Some(image.id.clone()),
                        hand: Some(e.hand),
                        direction: Some(a.frame.direction.normalize()),
                        scale: a.frame.scale,
                    });
                }
                if state.eject_shell && !image.casing.is_empty() {
                    self.events.push(Event::Shell {
                        actor: id,
                        image: image.id.clone(),
                        hand: e.hand,
                    });
                }
                let use_up = image
                    .scripts
                    .get(&state.script.to_ascii_lowercase())
                    .is_some_and(|s| s.use_up);
                if !self.callback(id, a, e, &image, &state.script, q) || use_up {
                    if use_up
                        && let Some(slot) = a.selected
                        && let Some(tool) = a.inventory.get_mut(slot)
                    {
                        *tool = None;
                        self.unmount(id, a);
                    }
                    if a.images[e.hand as usize].is_none() {
                        self.events.push(Event::Unmounted {
                            actor: id,
                            hand: e.hand,
                        });
                    }
                    return Advance::Drop;
                }
                // A grenade counted from the reserve, with none left, leaves
                // the hand; its tool stays selected for the reserve to come.
                if e.hand == 0
                    && let Some(magazine) = &image.magazine
                    && magazine.from_reserve
                    && !magazine.fires(rounds_in(a, "", magazine))
                {
                    let selected = a.selected;
                    self.unmount(id, a);
                    a.selected = selected;
                    // Or its tool goes too (Tier's Clear Unusable Grenades).
                    if magazine.clear_when_out
                        && let Some(slot) = selected
                        && let Some(tool) = a.inventory.get_mut(slot)
                    {
                        *tool = None;
                        a.selected = None;
                    }
                    self.events.push(Event::Unmounted { actor: id, hand: 0 });
                    self.events.push(Event::Ammo { actor: id });
                    return Advance::Drop;
                }
            }
            if e.remaining > 0 && state.wait && !self_loop {
                return Advance::Keep;
            }
            // Torque checks loaded, then ammo, then the trigger, then the
            // timeout. Only the right hand's image keeps these flags.
            let loaded = e.hand != 0 || a.loaded;
            let next = if loaded {
                state.loaded
            } else {
                state.not_loaded
            }
            .or(if !a.ammo { state.no_ammo } else { state.ammo })
            .or(if e.trigger { state.down } else { state.up })
            .or(if e.remaining == 0 {
                state.timeout
            } else {
                None
            });
            let Some(next) = next else {
                return Advance::Keep;
            };
            if self_loop && next == e.state {
                e.entered = false;
                return Advance::Keep;
            }
            left = Some(e.state);
            e.state = next;
            e.entered = false;
        }
        self.events.push(Event::Diagnostic {
            actor: Some(id),
            message: format!(
                "Image instantaneous transition budget exceeded: {}",
                e.image
            ),
        });
        Advance::Drop
    }
    /// Play each of the holder's state cues whose time has come.
    fn play_cues(&mut self, id: ActorId, a: &mut Actor) {
        if a.cues.is_empty() {
            return;
        }
        let (due, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut a.cues)
            .into_iter()
            .partition(|c| c.due <= self.tick);
        a.cues = waiting;
        for c in due {
            self.play_cue(id, c);
        }
    }
    fn play_cue(&mut self, id: ActorId, c: PendingCue) {
        if let Some(thread) = c.cue.thread
            && !c.cue.sequence.is_empty()
        {
            self.events.push(Event::Animation {
                actor: id,
                thread,
                sequence: c.cue.sequence,
                image_hand: None,
            });
        }
        if !c.cue.sound.is_empty() {
            self.events.push(Event::Sound {
                source: TargetId::Actor(id),
                profile: c.cue.sound,
                position: c.position,
            });
        }
    }
    fn animation(&mut self, id: ActorId, sequence: &str) {
        self.events.push(Event::Animation {
            actor: id,
            thread: 2,
            sequence: sequence.into(),
            image_hand: None,
        });
    }
    /// The image state scripts [`Self::callback`] runs itself, as v20's
    /// stock `WeaponImage` functions did, for an image whose Add-On does not
    /// define its own.
    pub const NATIVE_STATE_SCRIPTS: &[&str] = &[
        "oncharge",
        "onabortcharge",
        "onstopfire",
        "onprefire",
        "onfireakimbo",
        "onfire",
    ];
    fn callback(
        &mut self,
        id: ActorId,
        a: &mut Actor,
        e: &Equipped,
        image: &Image,
        script: &str,
        q: &mut impl Query,
    ) -> bool {
        // An Add-On tool's own moments run its commands, then carry on. A
        // gun's `onFire` command runs and its projectile still flies, as a
        // v20 `Image::onFire` package calling `Parent::onFire` did (a
        // magazine counting its rounds); a tool with no projectile only
        // runs the command.
        if let Some(command) = image.commands.for_script(script)
            && !(script.eq_ignore_ascii_case("onfire") && image.command.is_some())
        {
            self.events.push(Event::ToolFire {
                actor: id,
                image: image.id.clone(),
                hand: e.hand,
                command: Some(command.clone()),
                tool: None,
            });
            if script.eq_ignore_ascii_case("onfire") && image.projectile.is_none() {
                return true;
            }
        }
        let script = script.to_ascii_lowercase();
        if let Some(cook) = &image.cook
            && cook.script == script
            && e.hand == 0
            && a.cook.is_none()
        {
            a.cook = Some(Cooking {
                image: image.id.clone(),
                lit: self.tick,
            });
        }
        // A script the image describes as data replaces the built-in one.
        let ported = image.scripts.get(&script);
        if let Some(s) = ported {
            if !s.arm.is_empty() {
                self.animation(id, &s.arm);
            }
            if !s.fire {
                return true;
            }
        }
        // A fire state of its own (`onFire2`): its shot fires as onFire's.
        let state_shot = image.state_shots.get(&script).cloned();
        let script = if ported.is_some() || state_shot.is_some() {
            "onfire"
        } else {
            script.as_str()
        };
        // The arm move a state's script played (`playThread(2, ...)`) is the
        // state's own `arm`, played as it was entered.
        let state_arm = image.states.get(e.state).is_some_and(|s| !s.arm.is_empty());
        match script {
            "onabortcharge" | "onstopfire" if !state_arm => self.animation(id, "root"),
            "onfireakimbo" => {
                if let Some(left) = &mut a.images[1] {
                    left.trigger = true;
                }
            }
            "onfire" => {
                if ported.is_none() {
                    if image.command.is_some() {
                        self.events.push(Event::ToolFire {
                            actor: id,
                            image: image.id.clone(),
                            hand: e.hand,
                            command: image.command.clone(),
                            tool: None,
                        });
                        return true;
                    }
                    match &image.on_fire {
                        None => {}
                        Some(OnFire::Tool(tool)) => {
                            self.events.push(Event::ToolFire {
                                actor: id,
                                image: image.id.clone(),
                                hand: e.hand,
                                command: None,
                                tool: Some(*tool),
                            });
                            return true;
                        }
                        Some(OnFire::Skis) => return self.fire_skis(id, a),
                        Some(OnFire::Key) => {
                            self.fire_key(id, a, image, q);
                            return true;
                        }
                        Some(OnFire::Mount(next)) => {
                            self.mount(id, a, next, 0);
                            if let Some(new) = &mut a.images[0] {
                                new.trigger = e.trigger;
                            }
                            return false;
                        }
                    }
                }
                // A ported script's own projectile before the image's.
                let Some(projectile) = ported
                    .and_then(|s| s.projectile.as_ref())
                    .or(image.projectile.as_ref())
                else {
                    return true;
                };
                // A shot on the move may fly another projectile (a weaker
                // round), as its spread and range change, and a rested one
                // its own (a steadier round).
                let idle_ticks = a.last_shot.map(|t| self.tick.saturating_sub(t));
                let projectile = state_shot
                    .as_ref()
                    .or(image.shot.as_ref())
                    .and_then(|s| s.projectile_for(a.frame.velocity.length(), idle_ticks))
                    .unwrap_or(projectile);
                let p = self.pack.projectiles[projectile].clone();
                let sport = p.sport_image.is_some();
                if sport && self.tick < a.ball_ready {
                    return true;
                }
                let grace = image.sport.map_or(0, |s| u64::from(s.spawn_grace_ticks));
                if grace > 0 && self.tick < a.spawn_tick + grace {
                    return true;
                }
                if a.last_shot
                    .is_some_and(|t| self.tick < t + image.min_shot_ticks as u64)
                {
                    return true;
                }
                // The right gun's magazine pays for each shot, the left
                // gun's too when it has none of its own (both pistols of a
                // pair load from one count, as Tier+Tactical's do).
                let free = state_shot
                    .as_ref()
                    .or(image.shot.as_ref())
                    .is_some_and(|s| s.free);
                let pays = if free {
                    None
                } else if e.hand == 0 {
                    Some(e.clone())
                } else if image.magazine.is_none() {
                    a.images[0].clone()
                } else {
                    None
                };
                let last = match pays {
                    Some(pays) => match self.spend_rounds(id, a, &pays) {
                        Some(last) => last,
                        None => return true,
                    },
                    None => false,
                };
                // The magazine's last rounds fire the image's last shot.
                let (shot, volleys) = match image.last_shot.as_ref().filter(|_| last) {
                    _ if state_shot.is_some() => (state_shot.clone(), image.volleys.as_slice()),
                    Some(l) => (Some(l.shot.clone()), l.volleys.as_slice()),
                    None => (image.shot.clone(), image.volleys.as_slice()),
                };
                a.last_shot = Some(self.tick);
                let mut origin = if image.melee {
                    a.frame.eye
                } else {
                    a.frame.muzzle[e.hand as usize]
                };
                let direction = a.frame.direction.normalize();
                let mut speed = p.speed;
                if image.melee {
                    if let Some(hit) = q.sweep(
                        a.frame.eye,
                        a.frame.eye + direction * 20.0,
                        Filter {
                            projectile_age_ticks: None,
                            source: id,
                            players: true,
                            world_only: false,
                        },
                    ) {
                        let muzzle_distance =
                            a.frame.muzzle[e.hand as usize].distance(hit.position);
                        if muzzle_distance > 0.01 {
                            speed *= a.frame.eye.distance(hit.position) / muzzle_distance;
                        }
                    }
                } else if a.frame.first_person
                    && let Some(hit) = q.sweep(
                        a.frame.eye,
                        a.frame.eye + direction * 5.0,
                        Filter {
                            projectile_age_ticks: None,
                            source: id,
                            players: true,
                            world_only: false,
                        },
                    )
                    && a.frame.eye.distance(hit.position) < 3.1
                {
                    origin = a.frame.eye;
                }
                let mut velocity = direction * speed + a.frame.velocity * p.inherit;
                if sport {
                    let sport = image.sport.unwrap_or_default();
                    let [power, up] = sport.throw;
                    velocity = direction * power + Vec3::Y * up + a.frame.velocity;
                    if sport.aimed_throw {
                        let target = q.sweep(
                            a.frame.eye,
                            a.frame.eye + direction * 20.0,
                            Filter {
                                projectile_age_ticks: None,
                                source: id,
                                players: true,
                                world_only: false,
                            },
                        );
                        if let Some(hit) = target {
                            let dist = a.frame.eye.distance(hit.position).min(11.0);
                            if matches!(hit.target, TargetId::Actor(_)) && a.frame.grounded {
                                velocity = direction * 15.0 + Vec3::Y * 2.0 + a.frame.velocity;
                            } else {
                                let scale = (dist / 11.0 + 0.1).min(1.0);
                                let zs = (dist / 22.0).clamped(0.01, 1.0);
                                let inherited = if a.frame.grounded {
                                    a.frame.velocity
                                } else {
                                    Vec3::new(
                                        a.frame.velocity.x,
                                        a.frame.velocity.y / 2.0,
                                        a.frame.velocity.z,
                                    ) * zs
                                };
                                let up =
                                    if !a.frame.grounded && inherited.length() > 0.0 && dist < 10.0
                                    {
                                        5.0
                                    } else {
                                        7.5
                                    };
                                velocity = direction * (7.0 * scale) + Vec3::Y * up + inherited;
                            }
                        }
                    }
                }
                let shot = shot.unwrap_or(Shot::SINGLE);
                if let Some(lob) = shot.lob {
                    // Up by how far off the look lands, from the feet.
                    let reach = lob.range * a.frame.scale;
                    let distance = q
                        .sweep(
                            a.frame.eye,
                            a.frame.eye + direction * reach,
                            Filter {
                                projectile_age_ticks: None,
                                source: id,
                                players: true,
                                world_only: false,
                            },
                        )
                        .map_or(lob.otherwise, |hit| a.frame.position.distance(hit.position));
                    let jitter = |axis: usize| {
                        let r = unit_random(self.tick, id.0, 5000 + axis as u64);
                        let steps = lob.jitter_steps[axis];
                        let whole = ((steps + 1) as f32 * r) as u32;
                        whole.min(steps) as f32 / lob.jitter_divisor[axis]
                    };
                    // v20's x and y are the world's x and -z; the spawn
                    // below scales by the holder, which the script did not.
                    let up = distance / lob.distance_divisor;
                    velocity = (direction * lob.speed + Vec3::new(jitter(0), up, -jitter(1)))
                        / a.frame.scale.max(0.01);
                }
                let pace = a.frame.velocity.length();
                let spread = shot.spread_for(pace, idle_ticks);
                let kick = shot.recoil_velocity(direction);
                if kick != Vec3::ZERO {
                    // Recoil lands before the projectiles, which inherit it.
                    velocity += kick * p.inherit;
                    self.events.push(Event::Recoil {
                        actor: id,
                        velocity: kick,
                    });
                }
                // Out of the far side of any opening between the body and
                // where the shot starts.
                let body = a.frame.middle.unwrap_or(a.frame.eye);
                let (origin, through) = follow(q, &[body, a.frame.eye, origin]);
                let velocity = through.map_or(velocity, |c| carried(&c, Vec3::ZERO, velocity).1);
                // A muzzle shot with something right before the eye starts
                // at the eye, so it cannot leave through a wall the gun is
                // pressed into.
                let eye_first = shot.hitscan.as_ref().is_some_and(|h| {
                    !h.from_eye
                        && h.eye_within.is_some_and(|within| {
                            q.sweep(
                                a.frame.eye,
                                a.frame.eye + direction * within,
                                Filter {
                                    projectile_age_ticks: None,
                                    source: id,
                                    players: p.collide_players,
                                    world_only: false,
                                },
                            )
                            .is_some()
                        })
                });
                // Where the look meets something, for a muzzle shot that
                // converges on it.
                let look_point = shot
                    .hitscan
                    .as_ref()
                    .filter(|h| h.converge && !h.from_eye && !eye_first)
                    .map(|h| {
                        let end = a.frame.eye + direction * h.range;
                        q.sweep(
                            a.frame.eye,
                            end,
                            Filter {
                                projectile_age_ticks: None,
                                source: id,
                                players: p.collide_players,
                                world_only: false,
                            },
                        )
                        .map_or(end, |hit| hit.position)
                    });
                for n in 0..shot.projectiles {
                    let turn = if spread > 0.0 {
                        let angle = |axis: u64| {
                            let r = unit_random(self.tick, id.0, u64::from(n) * 3 + axis);
                            (r - 0.5) * 10.0 * std::f32::consts::PI * spread
                        };
                        Quat::from_euler(glam::EulerRot::XYZ, angle(0), angle(1), angle(2))
                    } else {
                        Quat::IDENTITY
                    };
                    if let Some(hitscan) = &shot.hitscan {
                        let range = match hitscan.moving_range {
                            Some(moving) if pace > shot.moving_speed => moving,
                            _ => hitscan.range,
                        };
                        let from_eye = hitscan.from_eye || eye_first;
                        let from = if from_eye { a.frame.eye } else { origin };
                        let aim = match look_point {
                            _ if from_eye => direction,
                            Some(point) => (point - origin).normalize_or(direction),
                            None => velocity.normalize_or(direction),
                        };
                        self.hitscan(
                            id,
                            a,
                            Ray {
                                hand: e.hand,
                                image: &image.id,
                                definition: projectile,
                                from,
                                muzzle: origin,
                                direction: turn * aim,
                                range,
                                hitscan,
                            },
                            q,
                        );
                        continue;
                    }
                    if let Err(error) = self.spawn(
                        projectile,
                        id,
                        origin,
                        turn * velocity * a.frame.scale,
                        a.frame.scale * shot.scale,
                    ) {
                        self.events.push(Event::Diagnostic {
                            actor: Some(id),
                            message: error.to_string(),
                        });
                        return true;
                    }
                    if let Some(p) = self.projectiles.get_mut(&(self.next_id - 1)) {
                        p.was_thrown = image.sport.is_some_and(|s| s.thrown);
                        p.paint = e.paint;
                    }
                    // A cooked grenade flies with what is left of its fuse.
                    if let (Some(cook), Some(lit)) = (
                        &image.cook,
                        a.cook
                            .as_ref()
                            .filter(|c| c.image == image.id)
                            .map(|c| c.lit),
                    ) {
                        let burned = self.tick.saturating_sub(lit).min(u64::from(u32::MAX)) as u32;
                        self.fuses
                            .insert(self.next_id - 1, cook.fuse_ticks.saturating_sub(burned));
                    }
                }
                if a.cook.as_ref().is_some_and(|c| c.image == image.id) {
                    a.cook = None;
                }
                // Further volleys (a shotgun's slug after its pellets): each
                // its own projectile and spread, along the same aim, with the
                // recoil the shot already took.
                for (v, volley) in volleys.iter().enumerate() {
                    let Some(d) = self.pack.projectiles.get(&volley.projectile) else {
                        continue;
                    };
                    let base = direction * d.speed + (a.frame.velocity + kick) * d.inherit;
                    for n in 0..volley.projectiles {
                        let turn = if volley.spread > 0.0 {
                            let angle = |axis: u64| {
                                let salt = (v as u64 + 1) * 1000 + u64::from(n) * 3 + axis;
                                let r = unit_random(self.tick, id.0, salt);
                                (r - 0.5) * 10.0 * std::f32::consts::PI * volley.spread
                            };
                            Quat::from_euler(glam::EulerRot::XYZ, angle(0), angle(1), angle(2))
                        } else {
                            Quat::IDENTITY
                        };
                        if let Err(error) = self.spawn(
                            &volley.projectile,
                            id,
                            origin,
                            turn * base * a.frame.scale,
                            a.frame.scale,
                        ) {
                            self.events.push(Event::Diagnostic {
                                actor: Some(id),
                                message: error.to_string(),
                            });
                            return true;
                        }
                    }
                }
                if ported.is_some() || state_arm {
                    // The port or the state played the arm's own animation.
                } else if e.hand == 1 {
                    self.animation(id, "leftrecoil");
                }
                if sport {
                    self.ball_released(id, a.frame.eye);
                    a.ball_ready = self.tick + 36;
                    if let Some(slot) = a.selected
                        && let Some(item) = a.inventory[slot].take()
                    {
                        a.rounds.remove(&slot_key(&item, slot));
                    }
                    self.unmount(id, a);
                    self.animation(id, "root");
                    return false;
                }
            }
            _ => {}
        }
        true
    }
    /// [`crate::CollisionSound`]: at the projectile, when it is moving
    /// fast enough and has not played it within the gap.
    fn collision_sound(&mut self, p: &Projectile, sound: &crate::CollisionSound) {
        if p.velocity.length() <= sound.min_speed
            || self
                .sounded
                .get(&p.id)
                .is_some_and(|last| self.tick < last + u64::from(sound.gap_ticks))
        {
            return;
        }
        self.sounded.insert(p.id, self.tick);
        self.events.push(Event::Sound {
            source: TargetId::Actor(p.source),
            profile: sound.profile.clone(),
            position: p.position,
        });
    }
    /// `skiWeaponImage::onFire`: on foot, put the skis on (and lower the
    /// item, `false`); on skis, take them off. Whether the item stays held.
    fn fire_skis(&mut self, id: ActorId, a: &mut Actor) -> bool {
        match a.frame.mount {
            Mount::Other => self.events.push(Event::SkisUnavailable { actor: id }),
            Mount::Skis => {
                a.skiing = false;
                self.events.push(Event::StopSkis { actor: id });
                self.events.push(Event::SkiNodes {
                    actor: id,
                    visible: false,
                });
            }
            Mount::None => {
                if !a.skiing {
                    a.skiing = true;
                    self.events.push(Event::StartSkis {
                        actor: id,
                        position: a.frame.position + Vec3::Y * 0.3,
                        velocity: a.frame.velocity,
                        mount_after_ticks: 30,
                    });
                    self.events.push(Event::SkiNodes {
                        actor: id,
                        visible: true,
                    });
                    self.unmount(id, a);
                    return false;
                }
            }
        }
        true
    }
    /// `keyImage::onFire`: try the brick the holder points at within ten
    /// units against the key's colour.
    fn fire_key(&mut self, id: ActorId, a: &Actor, image: &Image, q: &mut impl Query) {
        let end = a.frame.eye + a.frame.direction.normalize() * 10.0 * a.frame.scale;
        if let Some(hit) = q.sweep(
            a.frame.eye,
            end,
            Filter {
                projectile_age_ticks: None,
                source: id,
                players: false,
                world_only: true,
            },
        ) && let (TargetId::Brick(brick), Some(color)) = (hit.target, hit.color)
            && q.can_affect(id, hit.target)
        {
            self.events.push(Event::Key {
                actor: id,
                brick,
                matched: key_matches([image.color[0], image.color[1], image.color[2]], color),
            });
        }
    }
    /// One tick of a projectile's flight. [`coast`] is the same motion when
    /// it hits nothing.
    fn projectile_step(&mut self, p: &mut Projectile, q: &mut impl Query) -> bool {
        let d = self.pack.projectiles[&p.definition].clone();
        p.age += 1;
        if let Some(fuse) = self.fuses.get(&p.id).copied()
            && p.age >= fuse
        {
            self.explode(p, &d, q, None);
            return false;
        }
        if p.age >= d.lifetime_ticks {
            if d.explode_death {
                self.explode(p, &d, q, None);
            }
            return false;
        }
        // Age restarts on a redirect, so pulses count from the spawn.
        let flown = self.tick.saturating_sub(p.spawned);
        if let Some(aura) = &d.aura
            && flown > 0
            && flown.is_multiple_of(u64::from(aura.every_ticks))
            && (aura.max_pulses == 0
                || flown / u64::from(aura.every_ticks) <= u64::from(aura.max_pulses))
        {
            self.aura(p, &d, aura, q);
        }
        for (set, c) in d.children.iter().enumerate() {
            if c.every_ticks > 0
                && flown > 0
                && flown.is_multiple_of(u64::from(c.every_ticks))
                && (c.max_times == 0 || flown / u64::from(c.every_ticks) <= u64::from(c.max_times))
            {
                self.children(p, c, set);
            }
        }
        if p.stuck {
            return true;
        }
        p.velocity.y -= fall_per_tick(&d);
        let mut remaining = 1.0 / 120.0;
        for _ in 0..4 {
            let end = p.position + p.velocity * remaining;
            let filter = Filter {
                projectile_age_ticks: Some(p.age),
                source: p.source,
                players: d.collide_players,
                world_only: false,
            };
            // Through a portal: the rest of the tick's flight continues out
            // of its partner, turned, unless something is hit first.
            if let Some((t, carry)) = q.passage(p.position, end) {
                let at = p.position.lerp(end, t);
                if q.sweep(p.position, at, filter).is_none() {
                    let (moved, velocity, _) = carried(&carry, at, p.velocity);
                    // Past the partner's plane by a hair, so the rest of the
                    // tick does not go back in through it.
                    p.position = moved + velocity.normalize_or_zero() * PAST;
                    p.velocity = velocity;
                    p.heading = p.heading.map(|h| carried(&carry, Vec3::ZERO, h).1);
                    remaining *= 1.0 - t;
                    continue;
                }
            }
            let Some(hit) = q.sweep(p.position, end, filter) else {
                p.position = end;
                return true;
            };
            if !hit.position.is_finite()
                || !hit.normal.is_finite()
                || hit.normal.length_squared() < 0.1
                || !(0.0..=1.0).contains(&hit.fraction)
            {
                self.events.push(Event::Diagnostic {
                    actor: None,
                    message: "Rejected invalid collision adapter result".into(),
                });
                return false;
            }
            p.position = hit.position;
            let normal = hit.normal.normalize();
            let contact = ProjectileContact {
                projectile: p.id,
                definition: p.definition.clone(),
                source: p.source,
                target: hit.target,
                position: hit.position,
                velocity: p.velocity,
                normal,
                scale: p.scale,
                paint: p.paint,
            };
            self.events.push(Event::Contact {
                impact: contact.clone(),
            });
            if let Some(sound) = &d.collision_sound {
                self.collision_sound(p, sound);
            }
            match q.on_contact(&contact) {
                ContactResponse::Continue => {}
                ContactResponse::Delete => return false,
                ContactResponse::Explode => {
                    self.explode(p, &d, q, Some(normal));
                    return false;
                }
                response => match redirect_projectile(p, contact.normal, response) {
                    Ok(event) => {
                        self.events.push(event);
                        return true;
                    }
                    Err(error) => self.events.push(Event::Diagnostic {
                        actor: None,
                        message: error.to_string(),
                    }),
                },
            }
            let allowed = q.can_affect(p.source, hit.target);
            let mut stopped = false;
            if let Some(image) = &d.sport_image {
                if let TargetId::Brick(brick) = hit.target
                    && allowed
                {
                    self.events.push(Event::BallHit {
                        source: p.source,
                        brick,
                        projectile: p.id,
                    });
                }
                if let TargetId::Actor(target) = hit.target {
                    let dodge = d.sport_hit == Some(SportHit::KnockOut);
                    if dodge && !p.bounced && allowed {
                        self.events.push(Event::Damage {
                            source: p.source,
                            target: hit.target,
                            amount: 50000.0,
                            kind: "$DamageType::CannonBallDirect".into(),
                            position: hit.position,
                            direction: p.velocity.normalize_or_zero(),
                            projectile: p.definition.clone(),
                            special: self.special_of(p.id),
                            bounces: 0,
                        });
                    } else if q.can_catch(p.source, target)
                        && let Some(image) = self.mount_ball(target, image)
                    {
                        self.football_catch(p, &d, target);
                        self.events.push(Event::BallCaught {
                            actor: target,
                            projectile: p.id,
                            image,
                        });
                        return false;
                    }
                }
            } else if allowed {
                stopped = self.direct_hit(p, &d, hit.target, hit.position, false);
            }
            for (set, c) in d.children.iter().enumerate().filter(|(_, c)| c.on_hit) {
                self.children(p, c, set);
            }
            if p.age >= d.arm_ticks
                || (d.explode_player && matches!(hit.target, TargetId::Actor(_)))
                || !d.ballistic
            {
                self.explode(p, &d, q, Some(normal));
                return false;
            }
            // Kai's shield deleted a projectile it stopped (`%obj.schedule(10,
            // delete)`): one that would bounce or stick is gone unexploded.
            if stopped {
                return false;
            }
            if d.min_stick_speed > 0.0 && p.velocity.length() >= d.min_stick_speed {
                let incidence = (-p.velocity.normalize_or_zero())
                    .dot(normal)
                    .clamped(-1.0, 1.0)
                    .acos()
                    .to_degrees();
                if incidence < d.bounce_angle / 2.0 {
                    p.stuck = true;
                    p.heading = p.velocity.try_normalize();
                    p.velocity = Vec3::ZERO;
                    self.effect(p, &d.stick_effect, Some(normal));
                    return true;
                }
            }
            let normal_velocity = normal * p.velocity.dot(normal);
            p.velocity = ((p.velocity - normal_velocity) * (1.0 - d.friction) - normal_velocity)
                * d.elasticity;
            p.position += normal * 0.001;
            p.bounced = true;
            p.bounces += 1;
            if d.max_bounces > 0 && p.bounces >= d.max_bounces {
                self.explode(p, &d, q, Some(normal));
                return false;
            }
            self.events.push(Event::Bounced {
                projectile: p.id,
                position: p.position,
                velocity: p.velocity,
            });
            self.effect(p, &d.bounce_effect, Some(normal));
            for (set, c) in d.children.iter().enumerate().filter(|(_, c)| c.on_bounce) {
                self.children(p, c, set);
            }
            // `onRest`: at rest it is the item that holds its ball's image.
            if let Some(item) = d
                .sport_image
                .as_ref()
                .filter(|_| d.rest_speed > 0.0 && p.velocity.length() < d.rest_speed)
                .and_then(|image| self.pack.items.iter().find(|(_, it)| &it.image == image))
                .map(|(id, _)| id.clone())
            {
                self.events.push(Event::BallRest {
                    projectile: p.id,
                    item: item.clone(),
                    position: p.position,
                });
                // `onRest`: a popping item facing the ball's travel.
                if self.drops.len() < MAX_DROPS {
                    let drop = self.next_id;
                    self.next_id += 1;
                    let travel = Vec3::new(p.velocity.x, 0.0, p.velocity.z);
                    let rotation = if travel.length_squared() > 1e-6 {
                        Quat::from_rotation_arc(Vec3::NEG_Z, travel.normalize())
                    } else {
                        Quat::IDENTITY
                    };
                    self.drops.insert(
                        drop,
                        Drop {
                            rotation,
                            scale: p.scale,
                            id: drop,
                            item: item.clone(),
                            position: p.position,
                            velocity: Vec3::ZERO,
                            source: p.source,
                            pickup_after: self.tick,
                            expires: self.tick + DROP_LIFETIME_TICKS,
                            rounds: None,
                            paint: None,
                            name: None,
                        },
                    );
                    self.events.push(Event::Dropped {
                        drop,
                        item,
                        position: p.position,
                        velocity: Vec3::ZERO,
                    });
                }
                return false;
            }
            remaining *= 1.0 - hit.fraction;
            if remaining < 0.00001 {
                return true;
            }
        }
        self.events.push(Event::Diagnostic {
            actor: None,
            message: format!(
                "Projectile {} collision iteration cap; remaining substep not simulated",
                p.id
            ),
        });
        true
    }
    /// An explosion's composite plus its `soundProfile`, both at the projectile.
    fn effect(&mut self, p: &Projectile, definition: &str, direction: Option<Vec3>) {
        if let Some(sound) = self
            .pack
            .explosions
            .get(&definition.to_ascii_lowercase())
            .map(|e| e.sound.clone())
            .filter(|s| !s.is_empty())
        {
            self.events.push(Event::Sound {
                source: TargetId::Actor(p.source),
                profile: sound,
                position: p.position,
            });
        }
        if !definition.is_empty() {
            self.events.push(Event::Effect {
                source: TargetId::Actor(p.source),
                definition: crate::paint_effect(definition, p.paint),
                position: p.position,
                node: String::new(),
                seconds: 0.0,
                image: None,
                hand: None,
                direction,
                scale: p.scale,
            });
        }
    }
    /// What a projectile does to what it hits, when the rules allow it:
    /// damage, a shove, a brick knocked loose. A hitscan shot (`ray`) does
    /// the same where its ray lands. Returns whether a guard stopped it.
    fn direct_hit(
        &mut self,
        p: &Projectile,
        d: &ProjectileDef,
        target: TargetId,
        position: Vec3,
        ray: bool,
    ) -> bool {
        // `ProjectileData::damage` under Kai's `Shield` package: a flying
        // projectile (not a ray, which hurts through `ShapeBase::damage`)
        // that strikes a guard from in front.
        let stop = match target {
            TargetId::Actor(holder) if !ray => guard_held(&self.actors, &self.pack, holder)
                .and_then(|(guard, _, a)| {
                    let (look, middle) = (a.frame.direction.normalize_or_zero(), body(a));
                    guard
                        .covers(look, middle, a.frame.scale, position, p.velocity)
                        .then(|| (holder, guard.clone()))
                }),
            _ => None,
        };
        let (hurt, push) = stop
            .as_ref()
            .map_or((1.0, 1.0), |(_, g)| (g.projectile_damage, g.push));
        if let Some(player_type) = &d.turns_into {
            if let TargetId::Actor(actor) = target {
                self.events.push(Event::HorseTransform {
                    source: p.source,
                    target: actor,
                    player_type: player_type.clone(),
                    dismount: true,
                    reapply_colors: true,
                });
            }
        } else if d.damage > 0.0
            && hurt > 0.0
            && matches!(
                target,
                TargetId::Actor(_) | TargetId::Vehicle(_) | TargetId::Entity(_)
            )
        {
            self.events.push(Event::Damage {
                source: p.source,
                target,
                amount: d.damage.clamped(0.0, 100.0)
                    * if d.fixed_damage { 1.0 } else { p.scale }
                    * hurt,
                kind: d.damage_type.clone(),
                position,
                direction: p.velocity.normalize_or_zero(),
                projectile: p.definition.clone(),
                special: self.special_of(p.id),
                bounces: p.bounces,
            });
        }
        if matches!(
            target,
            TargetId::Actor(_) | TargetId::Vehicle(_) | TargetId::Entity(_)
        ) && (d.impulse > 0.0 || d.vertical > 0.0)
        {
            self.events.push(Event::Impulse {
                source: p.source,
                target,
                impulse: (p.velocity.normalize_or_zero() * d.impulse + Vec3::Y * d.vertical)
                    * p.scale
                    * push,
                position,
            });
        }
        if let (Some(slow), TargetId::Actor(actor)) = (d.slow, target) {
            self.events.push(Event::Slow { actor, slow });
        }
        if d.brick.direct && matches!(target, TargetId::Brick(_)) {
            self.events.push(Event::BrickImpact {
                source: p.source,
                target: Some(target),
                position,
                parameters: d.brick.clone(),
            });
        }
        match stop {
            Some((holder, guard)) => {
                self.stop(p, d, holder, position, &guard);
                true
            }
            None => false,
        }
    }
    /// A guard stopped `p` where it struck, `at`: the clang at the holder,
    /// one of its sounds, a stop off what it has left (breaking it at the
    /// last), its blast told to spare the holder, and the shot sent back.
    fn stop(
        &mut self,
        p: &Projectile,
        d: &ProjectileDef,
        holder: ActorId,
        at: Vec3,
        guard: &crate::Guard,
    ) {
        let Some(a) = self.actors.get(&holder) else {
            return;
        };
        let (look, middle, scale, velocity, bot) = (
            a.frame.direction.normalize_or_zero(),
            body(a),
            a.frame.scale,
            a.frame.velocity,
            a.bot,
        );
        self.burst(&guard.hit_explosion, holder, middle, scale);
        if let Some(durability) = guard.durability
            && !(guard.bots_keep && bot)
        {
            let a = self.actors.get_mut(&holder).expect("checked");
            let left = a.guard_left.unwrap_or(durability).saturating_sub(1);
            a.guard_left = Some(left);
            if left == 0 {
                a.guard_left = None;
                self.break_guard(holder, guard, middle, scale);
            }
        }
        self.stopped.push((p.id, holder, guard.push));
        if !guard.sounds.is_empty() {
            let n = guard.sounds.len();
            let pick = (unit_random(self.tick, p.id, GUARD_SOUND_DRAW) * n as f32) as usize;
            self.events.push(Event::Sound {
                source: TargetId::Actor(holder),
                profile: guard.sounds[pick.min(n - 1)].clone(),
                position: at,
            });
        }
        let lately = self
            .reflected
            .get(&p.id)
            .is_some_and(|(tick, _)| self.tick.saturating_sub(*tick) < REFLECT_TICKS);
        if guard.reflect && !lately && look != Vec3::ZERO {
            // From in front of the holder, the way they look, as fast as it
            // came, with their own motion as the projectile inherits it.
            let from = middle + look * (velocity.length() / 5.0 + 1.0);
            let speed = p.velocity.length();
            match self.spawn(
                &p.definition,
                holder,
                from,
                look * speed + velocity * d.inherit,
                p.scale,
            ) {
                Ok(id) => {
                    self.reflected
                        .insert(id, (self.tick, guard.reflect_kill.clone()));
                }
                Err(error) => self.events.push(Event::Diagnostic {
                    actor: Some(holder),
                    message: format!("A guard could not send a shot back: {error}"),
                }),
            }
        }
    }
    /// The guard in `holder`'s hand breaks: the first of their tools whose
    /// image it is leaves them, and if that is the one in hand, or none
    /// is, their hand empties and `break_explosion` goes off, as Kai's
    /// shield did when its `shieldHP` ran out.
    fn break_guard(&mut self, holder: ActorId, guard: &crate::Guard, middle: Vec3, scale: f32) {
        let Some(a) = self.actors.get(&holder) else {
            return;
        };
        let Some(held) = a.images[0].as_ref().map(|e| e.image.clone()) else {
            return;
        };
        let slot = a.inventory.iter().position(|item| {
            item.as_ref()
                .and_then(|i| self.pack.items.get(i))
                .is_some_and(|i| i.image == held)
        });
        let in_hand = slot.is_none_or(|s| a.selected == Some(s));
        if let Some(slot) = slot {
            let a = self.actors.get_mut(&holder).expect("checked");
            if let Some(item) = a.inventory[slot].take() {
                a.rounds.remove(&slot_key(&item, slot));
            }
        }
        if in_hand {
            let emptied = if slot.is_some() {
                self.equip(holder, None)
            } else {
                self.swap_image(holder, None)
            };
            if let Err(error) = emptied {
                self.events.push(Event::Diagnostic {
                    actor: Some(holder),
                    message: format!("A broken guard stayed in hand: {error}"),
                });
            }
            self.burst(&guard.break_explosion, holder, middle, scale);
        }
    }
    /// `ShapeBase::spawnExplosion`: the projectile `name` (an id, or a
    /// datablock name in any loaded pack) goes off at `at`.
    fn burst(&mut self, name: &str, source: ActorId, at: Vec3, scale: f32) {
        let Some(definition) = self.projectile_named(name) else {
            return;
        };
        if let Err(error) = self.spawn_explosion(&definition, source, at, scale) {
            self.events.push(Event::Diagnostic {
                actor: Some(source),
                message: error.to_string(),
            });
        }
    }
    /// The special kill a projectile makes: one a guard sent back.
    fn special_of(&self, projectile: u64) -> Option<String> {
        self.reflected
            .get(&projectile)
            .and_then(|(_, kill)| kill.clone())
    }
    /// `ShapeBase::damage` under Kai's `Shield` package: hurt that strikes
    /// a held guard from in front keeps only the guard's share, with its
    /// clang at the holder. Hurt above 5000 (an instant kill) or struck at
    /// the holder's feet is not stopped.
    fn guard_hurt(&mut self) {
        let mut clangs = vec![];
        for e in &mut self.events {
            let Event::Damage {
                target: TargetId::Actor(holder),
                amount,
                position,
                ..
            } = e
            else {
                continue;
            };
            let Some((guard, _, a)) = guard_held(&self.actors, &self.pack, *holder) else {
                continue;
            };
            if *amount > 5000.0 || position.distance(a.frame.position) < 0.1 {
                continue;
            }
            let middle = body(a);
            let look = a.frame.direction.normalize_or_zero();
            if guard.covers(look, middle, a.frame.scale, *position, middle - *position) {
                *amount *= guard.damage;
                clangs.push((guard.hit_explosion.clone(), *holder, middle, a.frame.scale));
            }
        }
        for (explosion, holder, middle, scale) in clangs {
            self.burst(&explosion, holder, middle, scale);
        }
    }
    /// One ray of a [`crate::Hitscan`] shot: the projectile `definition`
    /// lands where the ray first meets something, as if it had flown there,
    /// with the hitscan's landing sound; every player draws the tracer to
    /// that point, and its `flown` projectile flies there. A ricocheting
    /// ray ([`crate::Ricochet`]) then turns off what it met and lands again.
    fn hitscan(&mut self, id: ActorId, a: &Actor, ray: Ray, q: &mut impl Query) {
        let definition = ray.definition;
        let mut d = self.pack.projectiles[definition].clone();
        if let Some(damage) = ray.hitscan.damage {
            d.damage = damage;
        }
        let base = d.damage;
        let ricochet = ray.hitscan.ricochet;
        let turns = ricochet.map_or(0, |r| r.times);
        let mut from = ray.from;
        let mut direction = ray.direction.normalize_or(Vec3::NEG_Z);
        let mut reach = ray.range * a.frame.scale;
        // One draw a shot: its every ray and landing plays the same pair.
        let draw = unit_random(self.tick, id.0, HIT_SOUND_DRAW);
        for landing in 0..=turns {
            let hit = q.sweep(
                from,
                from + direction * reach,
                Filter {
                    // Once it has turned it is a shot coming back, which
                    // can meet its shooter (`%ignore` is then only what it
                    // last hit, which it leaves from just off the face).
                    projectile_age_ticks: (landing > 0).then_some(u32::MAX),
                    source: id,
                    players: d.collide_players,
                    world_only: false,
                },
            );
            let to = hit
                .as_ref()
                .map_or(from + direction * reach, |h| h.position);
            if landing == 0 {
                self.events.push(Event::Tracer {
                    actor: id,
                    hand: ray.hand,
                    image: ray.image.into(),
                    to,
                });
                if let Some(flown) = self.projectile_named(&ray.hitscan.flown)
                    && let Some(speed) = self.pack.projectiles.get(&flown).map(|f| f.speed)
                {
                    // Flown from the muzzle to the end, to be seen.
                    let along = (to - ray.muzzle).normalize_or(direction);
                    if let Err(error) = self.spawn(
                        &flown,
                        id,
                        ray.muzzle,
                        along * speed * a.frame.scale,
                        a.frame.scale,
                    ) {
                        self.events.push(Event::Diagnostic {
                            actor: Some(id),
                            message: error.to_string(),
                        });
                    }
                }
            } else {
                self.events.push(Event::Ricochet {
                    actor: id,
                    image: ray.image.into(),
                    from,
                    to,
                });
            }
            let Some(hit) = hit.filter(|h| h.position.is_finite() && h.normal.is_finite()) else {
                return;
            };
            // ShortRifleKai's `onRaycastDamage`: more for each landing
            // before, its own share on the shooter.
            if let Some(r) = ricochet {
                d.damage = if hit.target == TargetId::Actor(id) {
                    base * r.shooter
                } else {
                    (base + landing as f32 * r.damage).clamped(-100.0, 100.0)
                };
            }
            let normal = hit.normal.normalize_or(-direction);
            if !self.hitscan_land(
                id,
                a,
                definition,
                &d,
                ray.hitscan,
                landing,
                from,
                direction,
                &hit,
                normal,
                draw,
                q,
            ) {
                return;
            }
            reach -= hit.position.distance(from);
            if reach <= 0.0 {
                return;
            }
            // Mirrored about the face it met, from just off it.
            direction = (direction - normal * 2.0 * direction.dot(normal)).normalize_or(normal);
            from = hit.position + normal * 0.01;
        }
    }
    /// A hitscan ray's `landing` (0 the first) at `hit`: its contact,
    /// damage, explosion and sound. False when the contact deleted it.
    #[allow(clippy::too_many_arguments)]
    fn hitscan_land(
        &mut self,
        id: ActorId,
        a: &Actor,
        definition: &str,
        d: &ProjectileDef,
        hitscan: &crate::Hitscan,
        landing: u32,
        from: Vec3,
        direction: Vec3,
        hit: &Hit,
        normal: Vec3,
        draw: f32,
        q: &mut impl Query,
    ) -> bool {
        let projectile = self.next_id;
        self.next_id += 1;
        let p = Projectile {
            id: projectile,
            definition: definition.into(),
            source: id,
            position: hit.position,
            // Never zero, so a still projectile still knows which way it hit.
            velocity: direction * d.speed.max(1.0),
            scale: a.frame.scale,
            age: d.arm_ticks,
            bounced: landing > 0,
            stuck: false,
            origin: from,
            was_thrown: false,
            paint: None,
            heading: None,
            bounces: landing,
            spawned: self.tick,
        };
        let contact = ProjectileContact {
            projectile,
            definition: definition.into(),
            source: id,
            target: hit.target,
            position: hit.position,
            velocity: p.velocity,
            normal,
            scale: p.scale,
            paint: None,
        };
        self.events.push(Event::Contact {
            impact: contact.clone(),
        });
        if matches!(q.on_contact(&contact), ContactResponse::Delete) {
            return false;
        }
        if q.can_affect(id, hit.target) {
            self.direct_hit(&p, d, hit.target, hit.position, true);
        }
        match self
            .projectile_named(&hitscan.explosion)
            .and_then(|e| Some((self.pack.projectiles.get(&e)?.clone(), e)))
        {
            // `%p.explode()` where the ray landed, facing out of the surface.
            Some((blast, definition)) => {
                let at = Projectile {
                    definition,
                    velocity: normal,
                    ..p.clone()
                };
                self.explode(&at, &blast, q, Some(normal));
            }
            None => self.explode(&p, d, q, Some(normal)),
        }
        let sound = hitscan.sound(matches!(hit.target, TargetId::Actor(_)), draw);
        if !sound.is_empty() {
            self.events.push(Event::Sound {
                source: TargetId::Actor(id),
                profile: sound.to_owned(),
                position: hit.position,
            });
        }
        true
    }
    /// A projectile by id, or by its datablock name in any loaded pack.
    fn projectile_named(&self, name: &str) -> Option<String> {
        if name.is_empty() {
            return None;
        }
        if self.pack.projectiles.contains_key(name) {
            return Some(name.to_owned());
        }
        self.pack
            .projectiles
            .iter()
            .find(|(_, d)| d.name.eq_ignore_ascii_case(name))
            .map(|(id, _)| id.clone())
    }
    /// `children`: throw them out in directions from the tick and the
    /// parent, so every player computes the same ones.
    /// Each `set` of a projectile's children draws its own directions.
    fn children(&mut self, p: &Projectile, c: &crate::Children, set: usize) {
        let draw = |salt: u64| {
            let r = unit_random(self.tick, p.id, salt);
            (c.count + ((c.max_count - c.count + 1) as f32 * r) as u32).min(c.max_count)
        };
        let count = if c.max_count <= c.count {
            c.count
        } else if c.redraw {
            // Past `count`, each further child only while a fresh draw
            // is above the number thrown so far.
            (c.count..c.max_count)
                .find(|&n| draw(2100 + set as u64 * 16 + u64::from(n)) <= n)
                .unwrap_or(c.max_count)
        } else {
            draw(2000 + set as u64)
        };
        for n in (0..u64::from(count)).map(|n| n + set as u64 * 16) {
            let z = unit_random(self.tick, p.id, n * 2) * 2.0 - 1.0;
            let phi = unit_random(self.tick, p.id, n * 2 + 1) * std::f32::consts::TAU;
            let r = (1.0 - z * z).max(0.0).sqrt();
            let mut direction = Vec3::new(r * phi.cos(), z, r * phi.sin());
            let mut own = direction * c.speed;
            if c.angles {
                let degrees = |salt: u64| {
                    let r = unit_random(self.tick, p.id, salt);
                    ((361.0 * r) as u32).min(360) as f32 / 360.0 * std::f32::consts::TAU
                };
                let (a, b) = (degrees(4000 + n * 2), degrees(4000 + n * 2 + 1));
                own = Vec3::new(a.cos(), b.cos(), a.sin()) * c.speed;
                direction = own.normalize_or_zero();
            }
            if let Some(steps) = &c.steps {
                own = Vec3::from_array(std::array::from_fn(|axis| {
                    let (low, high) = (steps.low[axis], steps.high[axis]);
                    let r = unit_random(self.tick, p.id, 3000 + n * 3 + axis as u64);
                    let whole = (low + ((high - low + 1) as f32 * r) as i32).min(high);
                    (whole as f32 + steps.offset[axis]) * steps.step[axis]
                }));
                direction = own.normalize_or_zero();
            }
            let velocity = (own + p.velocity * c.inherit) * p.scale;
            let at = p.position + direction * 0.05 * p.scale;
            match self.spawn(&c.projectile, p.source, at, velocity, p.scale) {
                Ok(child) => {
                    if let Some([low, high]) = c.fuse_ticks {
                        let r = unit_random(self.tick, p.id, 1000 + n);
                        let fuse = low + ((high - low + 1) as f32 * r) as u32;
                        self.fuses.insert(child, fuse.min(high));
                    }
                }
                Err(error) => {
                    self.events.push(Event::Diagnostic {
                        actor: Some(p.source),
                        message: format!("Children of projectile {}: {error}", p.id),
                    });
                    return;
                }
            }
        }
    }
    /// `aura`: one pulse of damage to everything in reach, by the splash
    /// rules, at full strength throughout the radius.
    fn aura(&mut self, p: &Projectile, d: &ProjectileDef, aura: &crate::Aura, q: &mut impl Query) {
        let kind = if aura.damage_type.is_empty() {
            d.radius_damage_type.clone()
        } else {
            aura.damage_type.clone()
        };
        let mut hurt = 0;
        for target in q
            .radius(p.position, aura.radius * p.scale, MAX_QUERY_TARGETS)
            .into_iter()
            .take(MAX_QUERY_TARGETS)
        {
            if aura.max_targets > 0 && hurt >= aura.max_targets {
                break;
            }
            // An ally takes the aura's ally damage instead, friendly fire
            // or not.
            let ally = aura.ally_damage.filter(|_| {
                target.target != TargetId::Actor(p.source) && q.is_ally(p.source, target.target)
            });
            if !target.center.is_finite()
                || aura.players_only && matches!(target.target, TargetId::Vehicle(_))
                || ally.is_none() && !q.can_affect_radius(p.source, target.target)
            {
                continue;
            }
            if !aura.effect.is_empty() {
                let scale = match target.target {
                    TargetId::Actor(t) => self.actors.get(&t).map_or(1.0, |a| a.frame.scale),
                    _ => 1.0,
                };
                self.events.push(Event::Effect {
                    source: target.target,
                    definition: aura.effect.clone(),
                    position: target.center,
                    node: String::new(),
                    seconds: 0.0,
                    image: None,
                    hand: None,
                    direction: None,
                    scale,
                });
            }
            if !aura.target_sound.is_empty()
                && let TargetId::Actor(actor) = target.target
            {
                self.events.push(Event::Heard {
                    actor,
                    profile: aura.target_sound.clone(),
                });
            }
            let damage = ally.unwrap_or(aura.damage);
            if damage > 0.0 {
                self.events.push(Event::Damage {
                    source: p.source,
                    target: target.target,
                    amount: damage * p.scale,
                    kind: kind.clone(),
                    position: p.position,
                    direction: (target.center - p.position).normalize_or_zero(),
                    projectile: p.definition.clone(),
                    special: self.special_of(p.id),
                    bounces: 0,
                });
            }
            if aura.burn_seconds > 0.0 {
                self.events.push(Event::Burn {
                    source: p.source,
                    target: target.target,
                    seconds: aura.burn_seconds,
                });
            }
            hurt += 1;
        }
    }
    fn explode(
        &mut self,
        p: &Projectile,
        d: &ProjectileDef,
        q: &mut impl Query,
        direction: Option<Vec3>,
    ) {
        self.effect(p, &d.explosion.effect, direction);
        for (set, c) in d.children.iter().enumerate().filter(|(_, c)| c.on_explode) {
            self.children(p, c, set);
        }
        if d.brick.radius > 0.0 {
            self.events.push(Event::BrickImpact {
                source: p.source,
                target: None,
                position: p.position,
                parameters: d.brick.clone(),
            });
        }
        let radius = d.explosion.radius.max(d.explosion.impulse_radius) * p.scale;
        if radius <= 0.0 {
            return;
        }
        self.events.push(Event::Blast {
            source: p.source,
            position: p.position,
            radius,
        });
        let targets = q.radius(p.position, radius, MAX_QUERY_TARGETS);
        if targets.len() > MAX_QUERY_TARGETS {
            self.events.push(Event::Diagnostic {
                actor: None,
                message: "Radius adapter exceeded target budget".into(),
            });
        }
        // `ProjectileData::onExplode`: no line-of-sight test; distance is
        // taken to the target's centre and both falloffs are quadratic.
        let falloff = |distance: f32, radius: f32| {
            if radius > 0.0 {
                (1.0 - (distance / radius).powi(2)).clamped(0.0, 1.0)
            } else {
                0.0
            }
        };
        for target in targets.into_iter().take(MAX_QUERY_TARGETS) {
            if !target.center.is_finite() || !q.can_affect_radius(p.source, target.target) {
                continue;
            }
            let distance = target.center.distance(p.position);
            // A guard that stopped this projectile spares its holder the
            // blast and keeps its share of the push (`damageCancel`).
            let spared = self
                .stopped
                .iter()
                .find(|(id, holder, _)| *id == p.id && target.target == TargetId::Actor(*holder))
                .map(|(.., push)| *push);
            let damage_factor = falloff(distance, d.explosion.radius * p.scale);
            if damage_factor > 0.0 && d.explosion.damage > 0.0 && spared.is_none() {
                self.events.push(Event::Damage {
                    source: p.source,
                    target: target.target,
                    amount: d.explosion.damage * p.scale * damage_factor,
                    kind: d.radius_damage_type.clone(),
                    position: p.position,
                    direction: (target.center - p.position).normalize_or_zero(),
                    projectile: p.definition.clone(),
                    special: self.special_of(p.id),
                    bounces: 0,
                });
                if d.explosion.burn_seconds > 0.0 {
                    self.events.push(Event::Burn {
                        source: p.source,
                        target: target.target,
                        seconds: d.explosion.burn_seconds * damage_factor,
                    });
                }
            }
            let impulse_factor = falloff(distance, d.explosion.impulse_radius * p.scale);
            if impulse_factor > 0.0
                && (d.explosion.impulse > 0.0 || d.explosion.impulse_vertical > 0.0)
            {
                // `radiusImpulse` flattens a downward push on anything
                // standing within three units of the ground.
                let mut push = target.center - p.position;
                if push.y < 0.0
                    && q.sweep(
                        target.center,
                        target.center - Vec3::Y * 3.0,
                        Filter {
                            projectile_age_ticks: None,
                            source: p.source,
                            players: false,
                            world_only: true,
                        },
                    )
                    .is_some_and(|hit| matches!(hit.target, TargetId::Map(_) | TargetId::Brick(_)))
                {
                    push.y = 0.0;
                }
                self.events.push(Event::Impulse {
                    source: p.source,
                    target: target.target,
                    impulse: (push.normalize_or_zero() * d.explosion.impulse
                        + Vec3::Y * d.explosion.impulse_vertical)
                        * p.scale
                        * impulse_factor
                        * spared.unwrap_or(1.0),
                    position: p.position,
                });
            }
        }
    }
}
use anyhow::Context;
/// Original HSV hue-distance rule; grey keys/bricks use hue -1.
pub fn key_matches(key: [f32; 3], brick: [f32; 3]) -> bool {
    fn hue(c: [f32; 3]) -> f32 {
        let min = c[0].min(c[1]).min(c[2]);
        let max = c[0].max(c[1]).max(c[2]);
        let delta = max - min;
        if delta <= 0.0 {
            return -1.0;
        }
        let h = if max == c[0] {
            (c[1] - c[2]) / delta
        } else if max == c[1] {
            2.0 + (c[2] - c[0]) / delta
        } else {
            4.0 + (c[0] - c[1]) / delta
        };
        (h / 6.0).rem_euclid(1.0)
    }
    if !key.into_iter().chain(brick).all(|v| v.is_finite()) {
        return false;
    }
    let a = hue(key);
    let b = hue(brick);
    if (a < 0.0) != (b < 0.0) {
        return false;
    }
    let mut diff = (a - b).abs();
    if diff > 0.5 {
        diff = 1.0 - diff;
    }
    diff <= 0.1
}
mod sports;

pub use sports::SportAction;

mod persistence;
pub use persistence::{SAVE_SCHEMA, WeaponsSave};

/// Source Bounce/Redirect preserve incident speed when normalized and cap new speed at 200.
fn redirect_projectile(
    p: &mut Projectile,
    normal: Vec3,
    response: ContactResponse,
) -> Result<Event> {
    let velocity = response_velocity(p.velocity, normal, response)?;
    p.velocity = velocity;
    p.position += velocity.normalize_or_zero() * 0.002;
    p.age = 0;
    p.stuck = false;
    Ok(Event::Bounced {
        projectile: p.id,
        position: p.position,
        velocity,
    })
}
pub fn redirected_velocity(impact: &ProjectileContact, response: ContactResponse) -> Result<Vec3> {
    response_velocity(impact.velocity, impact.normal, response)
}
fn response_velocity(current: Vec3, normal: Vec3, response: ContactResponse) -> Result<Vec3> {
    ensure!(
        current.is_finite() && normal.is_finite() && normal.length_squared() > 0.1,
        "Invalid impact"
    );
    let velocity = match response {
        ContactResponse::Bounce(factor) => {
            ensure!(
                factor.is_finite() && factor.abs() <= 1000.0,
                "Invalid bounce factor"
            );
            let normal = normal.normalize();
            (current - normal * current.dot(normal) * 2.0) * factor
        }
        ContactResponse::Redirect { vector, normalized } => {
            ensure!(
                vector.is_finite() && vector.abs().max_element() <= 1e7,
                "Invalid redirect vector"
            );
            if normalized {
                vector.normalize_or_zero() * current.length()
            } else {
                vector
            }
        }
        _ => return Err(anyhow::anyhow!("Response does not redirect")),
    };
    Ok(velocity.clamp_length_max(200.0))
}
/// A number in [0, 1) that host and players compute alike for one shot, so
/// spread needs no random state and nothing on the wire.
/// [`unit_random`]'s `n` for a hitscan shot's landing sounds, apart from
/// its rays' spreads (`3 * ray + axis`).
const HIT_SOUND_DRAW: u64 = u64::MAX;
/// [`unit_random`]'s draw of a stopped projectile's guard sound.
const GUARD_SOUND_DRAW: u64 = u64::MAX - 1;
/// How long a projectile a guard sent back cannot be sent back again: Kai's
/// 500 ms.
const REFLECT_TICKS: u64 = 60;
/// The guard `actor` holds up: their right hand's image's, while that image
/// is in one of its guarding states, with the image's id and the holder.
fn guard_held<'a>(
    actors: &'a BTreeMap<ActorId, Actor>,
    pack: &'a Pack,
    actor: ActorId,
) -> Option<(&'a crate::Guard, &'a str, &'a Actor)> {
    let a = actors.get(&actor)?;
    let held = a.images[0].as_ref()?;
    let image = pack.images.get(&held.image)?;
    let guard = image.guard.as_ref()?;
    guard
        .guards_in(&image.states.get(held.state)?.name)
        .then_some((guard, held.image.as_str(), a))
}
/// The middle of a holder's body (Torque's hack position).
fn body(a: &Actor) -> Vec3 {
    a.frame.middle.unwrap_or(a.frame.eye)
}
fn unit_random(tick: u64, actor: u64, n: u64) -> f32 {
    let mut z = tick
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(actor.wrapping_mul(0xBF58_476D_1CE4_E5B9))
        .wrapping_add(n.wrapping_mul(0x94D0_49BB_1331_11EB));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 40) as f32 / (1u64 << 24) as f32
}
