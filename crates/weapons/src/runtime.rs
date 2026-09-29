//! Host-authoritative fixed-tick gameplay. Coordinates: X-right, Y-up, -Z-forward.
use crate::*;
use anyhow::{Result, ensure};
use glam::{Quat, Vec3};
use std::{collections::BTreeMap, sync::Arc};
pub const MAX_ACTORS: usize = 128;
pub const MAX_PROJECTILES: usize = 1024;
pub const MAX_DROPS: usize = 1024;
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
/// Images whose v20 `onFire` is a script that raycasts and acts on the hit
/// object (`hammerImage::onFire`, `wrenchImage::onFire`, ...) instead of
/// calling `Parent::onFire`. The runtime reports [`Event::ToolFire`] and the
/// host performs the hit; no projectile is launched.
pub const HOST_TOOL_IMAGES: [&str; 5] = [
    "hammerimage",
    "wrenchimage",
    "printgunimage",
    "wandimage",
    "adminwandimage",
];
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ActorId(pub u64);
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
    #[serde(default)]
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
                self.muzzle[1]
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
            "Invalid actor frame"
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
    #[serde(default)]
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
/// `Item::mGravity`.
/// Dropped items' mass: v20's item datablocks set `mass = 1` (inferred from
/// the stock weapon items; the PC's v20 audit can confirm).
pub const ITEM_MASS: f32 = 1.0;
const ITEM_GRAVITY: f32 = 20.0;
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
    /// The liquid covering most of the axis-aligned box standing on `bottom`
    /// and `height` tall, if any.
    fn liquid(&mut self, _bottom: Vec3, _height: f32) -> Option<Liquid> {
        None
    }
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
    /// A [`HOST_TOOL_IMAGES`] image, or an Add-On tool's image with a
    /// `command`, entered its `onFire` state.
    ToolFire {
        actor: ActorId,
        image: String,
        hand: u8,
        command: Option<String>,
    },
    /// A shot's recoil: add `velocity` to the shooter's own velocity.
    Recoil {
        actor: ActorId,
        velocity: Vec3,
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
    Dropped {
        drop: u64,
        item: String,
        position: Vec3,
        velocity: Vec3,
    },
    DropRemoved {
        drop: u64,
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
    #[serde(default)]
    pub paint: Option<u8>,
    /// The direction a stuck projectile flew in: its velocity is zero, but
    /// its model keeps pointing that way (v20 keeps the last transform).
    #[serde(default)]
    pub heading: Option<Vec3>,
}
/// Vertical speed a projectile loses each tick of flight (`gravityMod`);
/// none unless it is ballistic.
pub fn fall_per_tick(d: &crate::ProjectileDef) -> f32 {
    if d.ballistic {
        9.81 * d.gravity / 120.0
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
    #[serde(default)]
    pub rotation: Quat,
    #[serde(default = "unit_scale")]
    pub scale: f32,
    pub id: u64,
    pub item: String,
    pub position: Vec3,
    pub velocity: Vec3,
    pub source: ActorId,
    pub pickup_after: u64,
    pub expires: u64,
}
fn unit_scale() -> f32 {
    1.
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Equipped {
    image: String,
    state: usize,
    remaining: u32,
    entered: bool,
    trigger: bool,
    hand: u8,
    /// Palette index for the derived colour spray can image.
    #[serde(default)]
    paint: Option<u8>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Actor {
    pub inventory: Vec<Option<String>>,
    pub selected: Option<usize>,
    pub frame: Frame,
    pub ammo: bool,
    pub skiing: bool,
    images: [Option<Equipped>; 2],
    last_shot: Option<u64>,
    ball_ready: u64,
    spawn_tick: u64,
    tackle_until: u64,
}
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
}
impl WeaponsWorld {
    pub fn new(pack: Pack) -> Result<Self> {
        pack.validate()?;
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
        })
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
                skiing: false,
                images: [None, None],
                last_shot: None,
                ball_ready: 0,
                spawn_tick: self.tick,
                tackle_until: 0,
            },
        );
        Ok(())
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
        a.spawn_tick = self.tick;
        self.actors.insert(id, a);
        Ok(())
    }
    /// Authored item boxes that dropped items fall and rest on.
    pub fn set_item_bounds(&mut self, bounds: BTreeMap<String, ItemBounds>) {
        self.item_bounds = bounds;
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
    pub fn equip(&mut self, id: ActorId, slot: Option<usize>) -> Result<()> {
        ensure!(
            self.events.len() < 8192,
            "Command event budget; advance/drain before retry"
        );
        let a = self.actors.get(&id).context("Unknown actor")?;
        for e in a.images.iter().flatten() {
            ensure!(
                self.pack.images[&e.image]
                    .states
                    .get(e.state)
                    .is_none_or(|s| s.allow_change),
                "Image state prevents equip"
            );
        }
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
        self.unmount(id, &mut a);
        a.selected = slot;
        if let Some(image) = image {
            self.mount(id, &mut a, &image, 0);
            if image == native_id("image", "AkimboGunImage") {
                self.mount(id, &mut a, &native_id("image", "LeftHandedGunImage"), 1);
            }
        }
        self.actors.insert(id, a);
        Ok(())
    }
    /// `Player::mountImage` for an image that is not an inventory item: spray
    /// cans (`serverCmdUseSprayCan`/`UseFXCan`) and the admin wand. Like
    /// those commands it deselects the tool slot. `paint` binds the colour
    /// can's palette index, the native form of `color<N>SprayCanImage`.
    pub fn mount_image(&mut self, id: ActorId, image: &str, paint: Option<u8>) -> Result<()> {
        ensure!(
            self.events.len() < 8192,
            "Command event budget; advance/drain before retry"
        );
        ensure!(self.pack.images.contains_key(image), "Unknown image");
        let a = self.actors.get(&id).context("Unknown actor")?;
        for e in a.images.iter().flatten() {
            ensure!(
                self.pack.images[&e.image]
                    .states
                    .get(e.state)
                    .is_none_or(|s| s.allow_change),
                "Image state prevents equip"
            );
        }
        let mut a = self.actors.remove(&id).unwrap();
        self.unmount(id, &mut a);
        self.mount(id, &mut a, image, 0);
        if let Some(e) = &mut a.images[0] {
            e.paint = paint;
        }
        self.actors.insert(id, a);
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
        if a.images[0].take().is_some() {
            self.events.push(Event::Unmounted { actor: id, hand: 0 });
        }
        if let Some(image) = image {
            self.mount(id, &mut a, &image, 0);
        }
        self.actors.insert(id, a);
        Ok(())
    }
    fn mount(&mut self, id: ActorId, a: &mut Actor, image: &str, hand: u8) {
        if self.pack.images.contains_key(image) {
            a.images[hand as usize] = Some(Equipped {
                image: image.into(),
                state: 0,
                remaining: 0,
                entered: false,
                trigger: false,
                hand,
                paint: None,
            });
            if image.to_ascii_lowercase().contains("basketballshoot") {
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
        }
    }
    fn unmount(&mut self, id: ActorId, a: &mut Actor) {
        if a.images.iter().any(Option::is_some) {
            self.events.push(Event::SportMovement {
                actor: id,
                locked: false,
            });
        }
        for (hand, image) in a.images.iter_mut().enumerate() {
            if image.take().is_some() {
                self.events.push(Event::Unmounted {
                    actor: id,
                    hand: hand as u8,
                });
            }
        }
        a.selected = None;
    }
    pub fn trigger(&mut self, id: ActorId, down: bool) -> Result<()> {
        let a = self.actors.get_mut(&id).context("Unknown actor")?;
        if let Some(e) = &mut a.images[0] {
            e.trigger = down;
        }
        Ok(())
    }
    /// Sports movement trigger switches dribble/standing presentation to shoot mode.
    pub fn sport_trigger(&mut self, id: ActorId, trigger: u8, down: bool) -> Result<()> {
        ensure!(self.events.len() < 8192, "Command event budget");
        ensure!((2..=4).contains(&trigger), "Invalid sport trigger");
        let a = self.actors.get(&id).context("Unknown actor")?;
        let name = a.images[0].as_ref().map(|e| e.image.as_str()).unwrap_or("");
        let action = if name.contains("football") && trigger == 4 && down && !a.frame.can_jet {
            Some(SportAction::FootballLateral)
        } else if name.contains("soccer") {
            Some(if trigger == 4 && down {
                SportAction::SoccerPop
            } else {
                SportAction::SoccerDrop
            })
        } else if name.contains("basketballshoot") && trigger == 4 && !down && !a.frame.can_jet {
            Some(SportAction::BasketballPass)
        } else {
            None
        };
        if let Some(action) = action {
            self.sport_action(id, action)?;
            return Ok(());
        }
        if down && name == native_id("image", "basketballImage") {
            let mut a = self.actors.remove(&id).unwrap();
            self.mount(id, &mut a, &native_id("image", "basketballShootImage"), 0);
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
        if a.selected == Some(slot) {
            self.equip(id, None)?;
        }
        self.actors.get_mut(&id).unwrap().inventory[slot] = None;
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
                // Engine-family evidence:15 engine ticks at32ms. Round UP at120Hz.
                pickup_after: self.tick + 58,
                expires: self.tick + 1200,
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
                source: ActorId(0),
                pickup_after: self.tick,
                expires: self.tick + 1200,
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
    /// Host validates contact and minigame permission. Thrower exclusion applies only to its source.
    pub fn pickup(&mut self, id: ActorId, drop: u64) -> Result<usize> {
        ensure!(self.events.len() < 8192, "Command event budget");
        let d = self.drops.get(&drop).context("Unknown drop")?;
        ensure!(
            id != d.source || self.tick >= d.pickup_after,
            "Pickup cooldown"
        );
        let item = d.item.clone();
        let slot = self.give(id, &item)?;
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
            // v20 `Player::updateMove` sets image slot 1's trigger from move
            // trigger 1, which Blockland never sends, before the images run.
            // `AkimboGunImage::onFireAkimbo`'s setImageTrigger(1, 1) is
            // therefore a pulse the left gun sees only in the tick it is set.
            if let Some(left) = &mut a.images[1] {
                left.trigger = false;
            }
            for hand in 0..2 {
                if let Some(mut e) = a.images[hand].take() {
                    let keep = self.advance(id, &mut a, &mut e, q);
                    if keep && a.images[hand].is_none() {
                        a.images[hand] = Some(e);
                    }
                }
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
    ) -> bool {
        let image = self.pack.images[&e.image].clone();
        if image.states.is_empty() {
            return true;
        }
        if e.entered && e.remaining > 0 {
            e.remaining -= 1;
        }
        for _ in 0..16 {
            let state = &image.states[e.state];
            // A zero-timeout state that times out into itself (the wands'
            // sparkling Ready) restarts its emitter once the emission ends
            // rather than spinning; trigger transitions stay immediate.
            let self_loop = state.ticks == 0 && state.timeout == Some(e.state);
            if !e.entered {
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
                if !state.sequence.is_empty() {
                    self.events.push(Event::Animation {
                        actor: id,
                        thread: 0,
                        sequence: state.sequence.clone(),
                        image_hand: Some(e.hand),
                    });
                }
                if !state.sound.is_empty() {
                    self.events.push(Event::Sound {
                        source: TargetId::Actor(id),
                        profile: state.sound.clone(),
                        position: a.frame.muzzle[e.hand as usize],
                    });
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
                if !self.callback(id, a, e, &image, &state.script, q) {
                    if a.images[e.hand as usize].is_none() {
                        self.events.push(Event::Unmounted {
                            actor: id,
                            hand: e.hand,
                        });
                    }
                    return false;
                }
            }
            if e.remaining > 0 && state.wait && !self_loop {
                return true;
            }
            let next = if !a.ammo { state.no_ammo } else { state.ammo }
            .or(if e.trigger { state.down } else { state.up })
            .or(if e.remaining == 0 {
                state.timeout
            } else {
                None
            });
            let Some(next) = next else {
                return true;
            };
            if self_loop && next == e.state {
                e.entered = false;
                return true;
            }
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
        false
    }
    fn animation(&mut self, id: ActorId, sequence: &str) {
        self.events.push(Event::Animation {
            actor: id,
            thread: 2,
            sequence: sequence.into(),
            image_hand: None,
        });
    }
    fn callback(
        &mut self,
        id: ActorId,
        a: &mut Actor,
        e: &Equipped,
        image: &Image,
        script: &str,
        q: &mut impl Query,
    ) -> bool {
        let name = image.name.to_ascii_lowercase();
        // An Add-On tool's own moments run its commands, then carry on.
        if let Some(command) = image.commands.for_script(script)
            && !(script.eq_ignore_ascii_case("onfire") && image.command.is_some())
        {
            self.events.push(Event::ToolFire {
                actor: id,
                image: image.id.clone(),
                hand: e.hand,
                command: Some(command.clone()),
            });
            if script.eq_ignore_ascii_case("onfire") {
                return true;
            }
        }
        match script.to_ascii_lowercase().as_str() {
            "oncharge" => {
                if name.contains("spear") || name.contains("football") {
                    self.animation(id, "spearReady");
                }
            }
            "onabortcharge" | "onstopfire" => self.animation(id, "root"),
            "onprefire" => {
                if name.contains("key") {
                    self.animation(id, "shiftLeft");
                } else if name == "wrenchimage" {
                    self.animation(id, "wrench");
                } else if name.contains("sword")
                    || matches!(
                        name.as_str(),
                        "hammerimage" | "wandimage" | "adminwandimage"
                    )
                {
                    self.animation(id, "armattack");
                }
            }
            "onfireakimbo" => {
                if let Some(left) = &mut a.images[1] {
                    left.trigger = true;
                }
            }
            "onfire" => {
                if HOST_TOOL_IMAGES.contains(&name.as_str()) || image.command.is_some() {
                    self.events.push(Event::ToolFire {
                        actor: id,
                        image: image.id.clone(),
                        hand: e.hand,
                        command: image.command.clone(),
                    });
                    return true;
                }
                if name == "skiweaponimage" {
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
                    return true;
                }
                if name.contains("keyimage") {
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
                            matched: key_matches(
                                [image.color[0], image.color[1], image.color[2]],
                                color,
                            ),
                        });
                    }
                    return true;
                }
                if name == "basketballimage" {
                    self.mount(id, a, &native_id("image", "basketballShootImage"), 0);
                    if let Some(new) = &mut a.images[0] {
                        new.trigger = e.trigger;
                    }
                    return false;
                }
                let Some(projectile) = &image.projectile else {
                    return true;
                };
                let p = self.pack.projectiles[projectile].clone();
                let sport = p.sport_image.is_some();
                if sport && self.tick < a.ball_ready {
                    return true;
                }
                if name.contains("dodgeball") && self.tick < a.spawn_tick + 120 {
                    return true;
                }
                if a.last_shot
                    .is_some_and(|t| self.tick < t + image.min_shot_ticks as u64)
                {
                    return true;
                }
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
                    let (power, up) = if name.contains("dodgeball") {
                        (30.0, 4.0)
                    } else if name.contains("football") {
                        (40.0, 0.0)
                    } else if name.contains("soccer") {
                        (20.0, 3.0)
                    } else {
                        (7.0, 7.5)
                    };
                    velocity = direction * power + Vec3::Y * up + a.frame.velocity;
                    if name.contains("basketball") {
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
                                let zs = (dist / 22.0).clamp(0.01, 1.0);
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
                let shot = image.shot.unwrap_or(Shot {
                    projectiles: 1,
                    spread: 0.0,
                    recoil: 0.0,
                });
                if shot.recoil > 0.0 {
                    // Recoil lands before the projectiles, which inherit it.
                    let kick = -direction * shot.recoil;
                    velocity += kick * p.inherit;
                    self.events.push(Event::Recoil {
                        actor: id,
                        velocity: kick,
                    });
                }
                for n in 0..shot.projectiles {
                    let turn = if shot.spread > 0.0 {
                        let angle = |axis: u64| {
                            let r = unit_random(self.tick, id.0, u64::from(n) * 3 + axis);
                            (r - 0.5) * 10.0 * std::f32::consts::PI * shot.spread
                        };
                        Quat::from_euler(glam::EulerRot::XYZ, angle(0), angle(1), angle(2))
                    } else {
                        Quat::IDENTITY
                    };
                    if let Err(error) = self.spawn(
                        projectile,
                        id,
                        origin,
                        turn * velocity * a.frame.scale,
                        a.frame.scale,
                    ) {
                        self.events.push(Event::Diagnostic {
                            actor: Some(id),
                            message: error.to_string(),
                        });
                        return true;
                    }
                    if let Some(p) = self.projectiles.get_mut(&(self.next_id - 1)) {
                        p.was_thrown = name.contains("football");
                        p.paint = e.paint;
                    }
                }
                if name.contains("spear") || name.contains("football") {
                    self.animation(id, "spearThrow");
                } else if name.contains("pushbroom") {
                    self.animation(id, "rotCW");
                } else if e.hand == 1 {
                    self.animation(id, "leftrecoil");
                } else if name.contains("gun") || name.contains("horseray") {
                    self.animation(id, "shiftAway");
                }
                if sport {
                    a.ball_ready = self.tick + 36;
                    if let Some(slot) = a.selected {
                        a.inventory[slot] = None;
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
    /// One tick of a projectile's flight. [`coast`] is the same motion when
    /// it hits nothing.
    fn projectile_step(&mut self, p: &mut Projectile, q: &mut impl Query) -> bool {
        let d = self.pack.projectiles[&p.definition].clone();
        p.age += 1;
        if p.age >= d.lifetime_ticks {
            if d.explode_death {
                self.explode(p, &d, q, None);
            }
            return false;
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
            match q.on_contact(&contact) {
                ContactResponse::Continue => {}
                ContactResponse::Delete => return false,
                ContactResponse::Explode => {
                    self.explode(p, &d, q, Some(normal));
                    return false;
                }
                response => match redirected_velocity(&contact, response) {
                    Ok(velocity) => {
                        p.velocity = velocity;
                        p.position += velocity.normalize_or_zero() * 0.002;
                        p.age = 0;
                        p.stuck = false;
                        self.events.push(Event::Bounced {
                            projectile: p.id,
                            position: p.position,
                            velocity,
                        });
                        return true;
                    }
                    Err(error) => self.events.push(Event::Diagnostic {
                        actor: None,
                        message: error.to_string(),
                    }),
                },
            }
            let allowed = q.can_affect(p.source, hit.target);
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
                    let dodge = d.name.eq_ignore_ascii_case("dodgeballProjectile");
                    if dodge && !p.bounced && allowed {
                        self.events.push(Event::Damage {
                            source: p.source,
                            target: hit.target,
                            amount: 50000.0,
                            kind: "$DamageType::CannonBallDirect".into(),
                            position: hit.position,
                        });
                    } else if q.can_catch(p.source, target)
                        && let Some(image) = self.mount_ball(target, image)
                    {
                        if d.name.eq_ignore_ascii_case("footballProjectile") && !p.bounced {
                            let catcher = self.actors[&target].frame.position;
                            let delta = catcher - p.origin;
                            self.events.push(Event::FootballCatch {
                                source: p.source,
                                catcher: target,
                                distance_feet: (Vec3::new(delta.x, 0.0, delta.z).length() * 1.875)
                                    .round() as u32,
                                was_thrown: p.was_thrown,
                            });
                        }
                        self.events.push(Event::BallCaught {
                            actor: target,
                            projectile: p.id,
                            image,
                        });
                        return false;
                    }
                }
            } else if allowed {
                if d.name.eq_ignore_ascii_case("horseRayProjectile") {
                    if let TargetId::Actor(target) = hit.target {
                        self.events.push(Event::HorseTransform {
                            source: p.source,
                            target,
                            player_type: "v20.player.horsearmor".into(),
                            dismount: true,
                            reapply_colors: true,
                        });
                    }
                } else if d.damage > 0.0
                    && matches!(
                        hit.target,
                        TargetId::Actor(_) | TargetId::Vehicle(_) | TargetId::Entity(_)
                    )
                {
                    self.events.push(Event::Damage {
                        source: p.source,
                        target: hit.target,
                        amount: d.damage.clamp(0.0, 100.0) * p.scale,
                        kind: d.damage_type.clone(),
                        position: hit.position,
                    });
                }
                if matches!(
                    hit.target,
                    TargetId::Actor(_) | TargetId::Vehicle(_) | TargetId::Entity(_)
                ) && (d.impulse > 0.0 || d.vertical > 0.0)
                {
                    self.events.push(Event::Impulse {
                        source: p.source,
                        target: hit.target,
                        impulse: (p.velocity.normalize_or_zero() * d.impulse
                            + Vec3::Y * d.vertical)
                            * p.scale,
                        position: hit.position,
                    });
                }
                if d.brick.direct && matches!(hit.target, TargetId::Brick(_)) {
                    self.events.push(Event::BrickImpact {
                        source: p.source,
                        target: Some(hit.target),
                        position: hit.position,
                        parameters: d.brick.clone(),
                    });
                }
            }
            if p.age >= d.arm_ticks
                || (d.explode_player && matches!(hit.target, TargetId::Actor(_)))
                || !d.ballistic
            {
                self.explode(p, &d, q, Some(normal));
                return false;
            }
            if d.min_stick_speed > 0.0 && p.velocity.length() >= d.min_stick_speed {
                let incidence = (-p.velocity.normalize_or_zero())
                    .dot(normal)
                    .clamp(-1.0, 1.0)
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
            self.events.push(Event::Bounced {
                projectile: p.id,
                position: p.position,
                velocity: p.velocity,
            });
            self.effect(p, &d.bounce_effect, Some(normal));
            if d.sport_image.is_some() && d.rest_speed > 0.0 && p.velocity.length() < d.rest_speed {
                let item = if d.name.eq_ignore_ascii_case("footballProjectile") {
                    "footballItem"
                } else {
                    "soccerBallItem"
                };
                let item = native_id("weapon", item);
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
                            expires: self.tick + 1200,
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
    fn explode(
        &mut self,
        p: &Projectile,
        d: &ProjectileDef,
        q: &mut impl Query,
        direction: Option<Vec3>,
    ) {
        self.effect(p, &d.explosion.effect, direction);
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
                (1.0 - (distance / radius).powi(2)).clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        for target in targets.into_iter().take(MAX_QUERY_TARGETS) {
            if !target.center.is_finite() || !q.can_affect_radius(p.source, target.target) {
                continue;
            }
            let distance = target.center.distance(p.position);
            let damage_factor = falloff(distance, d.explosion.radius * p.scale);
            if damage_factor > 0.0 && d.explosion.damage > 0.0 {
                self.events.push(Event::Damage {
                    source: p.source,
                    target: target.target,
                    amount: d.explosion.damage * p.scale * damage_factor,
                    kind: d.radius_damage_type.clone(),
                    position: p.position,
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
                        * impulse_factor,
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
pub use persistence::WeaponsSave;

/// Source Bounce/Redirect preserve incident speed when normalized and cap new speed at 200.
pub fn redirected_velocity(impact: &ProjectileContact, response: ContactResponse) -> Result<Vec3> {
    ensure!(
        impact.velocity.is_finite()
            && impact.normal.is_finite()
            && impact.normal.length_squared() > 0.1,
        "Invalid impact"
    );
    let velocity = match response {
        ContactResponse::Bounce(factor) => {
            ensure!(
                factor.is_finite() && factor.abs() <= 1000.0,
                "Invalid bounce factor"
            );
            let normal = impact.normal.normalize();
            (impact.velocity - normal * impact.velocity.dot(normal) * 2.0) * factor
        }
        ContactResponse::Redirect { vector, normalized } => {
            ensure!(
                vector.is_finite() && vector.abs().max_element() <= 1e7,
                "Invalid redirect vector"
            );
            if normalized {
                vector.normalize_or_zero() * impact.velocity.length()
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
