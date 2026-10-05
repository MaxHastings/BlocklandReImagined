use crate::{FIXED_DT, schema::*};
use anyhow::{Context, Result, ensure};
use bri_console::Clamp;
use bri_motor::player::{MoveInput, Player, PlayerState, PlayerTuning, TORQUE_TICK};
use glam::{Quat, Vec3};
use rapier3d::parry::query::ShapeCastOptions;
use rapier3d::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
mod checkpoint;
mod tires;
pub use tires::{TireState, WheelState};
/// Torque vehicles and players fall at 20 m/s^2 whatever the shared world uses.
pub const VEHICLE_GRAVITY: f32 = 20.;
pub use checkpoint::*;
macro_rules! id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
        pub struct $name(pub u64);
    };
}
id!(VehicleId);
id!(OwnerId);
id!(OccupantId);
id!(SpawnId);
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Occupant {
    pub id: OccupantId,
    pub owner: OwnerId,
    /// The rider's standing box, [width, height]: dismount clearance tests
    /// exactly the body the rider gets back.
    pub body: [f32; 2],
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Controls {
    pub throttle: f32,
    pub steer: f32,
    pub pitch: f32,
    pub roll: f32,
    pub vertical: f32,
    pub strafe: f32,
    pub brake: bool,
    pub jump: bool,
    pub jet: bool,
    pub fire: bool,
    /// Attached turret: aim relative to the hull. Player-type mounts: the
    /// world heading the rider looks along (the mount turns to face it).
    pub aim_yaw: f32,
    pub aim_pitch: f32,
    /// This tick's mouse turn (yaw right, pitch up), radians: Torque's
    /// `move->yaw`/`move->pitch`, which mouse-steered vehicles accumulate.
    pub look_delta: [f32; 2],
    /// The driver turned `$pref::Input::UseStrafeSteering` off: a vehicle
    /// with `steeringUseStrafeSteering` is then mouse-steered too.
    pub strafe_steering_off: bool,
    /// The driver turned `$pref::Input::UseAutoReturnSteering` off.
    pub auto_return_off: bool,
}
/// `WheeledVehicle::updateCollision` (0x572303) wrecks a vehicle whose body
/// collides while none of its first three wheels touches the ground.
const WRECK_WHEELS: usize = 3;
/// v20 caps the flying lift at 4000 whatever the datablock says (0x575382).
const WHEELED_LIFT_CAP: f32 = 4000.;
/// v20 rescales a wheeled vehicle faster than 200 to 199 (0x575bd5).
const WHEELED_SPEED_CAP: f32 = 200.;
/// How much the control surfaces bite: none below `stallSpeed`, full at
/// `stallSpeed + maxForwardVel`.
/// 120 Hz ticks in one of v20's 32 ms moves, rounded up: a driver's move
/// steers without auto-return until this long has passed without a turn.
const AUTO_RETURN_QUIET: u8 = 4;
/// Contacts whose normal is closer to level than this (|y| of the unit
/// normal, about 45 degrees) are hits from the side.
const SIDE_HIT: f32 = 0.7;
fn bite(f: &WheeledFlightSettings, speed: f32) -> f32 {
    if f.max_forward_vel > 0. {
        ((speed - f.stall_speed) / f.max_forward_vel).clamped(0., 1.)
    } else {
        0.
    }
}
impl Controls {
    fn validate(self) -> Result<Self> {
        ensure!(
            [
                self.throttle,
                self.steer,
                self.pitch,
                self.roll,
                self.vertical,
                self.strafe,
                self.aim_yaw,
                self.aim_pitch,
                self.look_delta[0],
                self.look_delta[1]
            ]
            .iter()
            .all(|x| x.is_finite()),
            "nonfinite controls"
        );
        ensure!(
            self.look_delta
                .iter()
                .all(|x| x.abs() <= std::f32::consts::PI),
            "look turn outside bounds"
        );
        ensure!(
            [
                self.throttle,
                self.steer,
                self.pitch,
                self.roll,
                self.vertical,
                self.strafe
            ]
            .iter()
            .all(|x| x.abs() <= 1.),
            "control outside [-1,1]"
        );
        ensure!(
            self.aim_pitch.abs() <= std::f32::consts::FRAC_PI_2
                && self.aim_yaw.abs() <= std::f32::consts::TAU,
            "aim outside bounds"
        );
        Ok(self)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Spawn {
    pub id: VehicleId,
    pub owner: OwnerId,
    pub definition: String,
    pub transform: Transform,
    pub spawn_id: Option<SpawnId>,
    pub respawn_ticks: Option<u64>,
    /// Uniform native scale, 0.2 through 5.0.
    pub scale: f32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SeatSnapshot {
    pub index: usize,
    pub occupant: Option<Occupant>,
    pub transform: Transform,
    pub pose: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VehicleSnapshot {
    pub scale: f32,
    pub id: VehicleId,
    pub owner: OwnerId,
    pub definition: String,
    pub transform: Transform,
    pub velocity: [f32; 3],
    pub angular_velocity: [f32; 3],
    pub damage: f32,
    pub destroyed: bool,
    pub seats: Vec<SeatSnapshot>,
    pub wheel_suspension: Vec<f32>,
    pub wheel_rotation: Vec<f32>,
    /// Each wheel touching the ground (`mWheel[i].surface.contact`): the
    /// client sprays the tire emitter from those.
    pub wheel_contact: Vec<bool>,
    /// Each wheel's spin and tyre stretch, which a predicting client
    /// restores with the rest of the motion.
    #[serde(default)]
    pub wheel_tire: Vec<TireState>,
    pub steering: f32,
    /// Torque `mSteering`: the driver's accumulated mouse steering (yaw,
    /// pitch), which a predicting client restores to replay its moves.
    #[serde(default)]
    pub mouse_steering: [f32; 2],
    /// Ticks since the driver's move last turned, up to `AUTO_RETURN_QUIET`.
    #[serde(default)]
    pub steering_quiet: u8,
    pub animation: String,
    pub charge: u8,
    pub energy: f32,
    pub jetting: bool,
    pub turret_aim: [f32; 2],
    pub turret_damage: Option<f32>,
    pub turret_transform: Option<Transform>,
    /// A player-type mount's motor state.
    #[serde(default)]
    pub actor: Option<PlayerState>,
}
impl VehicleSnapshot {
    /// Where to draw the vehicle. A player-type mount's body moves on the
    /// motor's 32 ms Torque ticks; it is drawn between the last two
    /// (`PlayerState::shown_feet`), as v20 renders a Player, instead of
    /// stepping every fourth 120 Hz tick.
    pub fn shown_transform(&self) -> Transform {
        match &self.actor {
            Some(actor) => Transform {
                position: actor.shown_feet(),
                ..self.transform.clone()
            },
            None => self.transform.clone(),
        }
    }
}
/// A vehicle's replicated motion: what a client predicting the vehicle it
/// drives resets it to before replaying its unacknowledged moves.
#[derive(Clone, Debug)]
pub struct Motion {
    /// Authoritative travel evidence consumed by the client predictor.
    pub passage_frame: bri_content::passage::PassageFrame,
    pub transform: Transform,
    pub velocity: [f32; 3],
    pub angular_velocity: [f32; 3],
    pub mouse_steering: [f32; 2],
    pub steering: f32,
    /// Ticks since the driver's move last turned (see `AUTO_RETURN_QUIET`).
    pub steering_quiet: u8,
    pub wheel_suspension: Vec<f32>,
    pub wheel_rotation: Vec<f32>,
    pub wheel_contact: Vec<bool>,
    pub wheel_tire: Vec<TireState>,
    /// A player-type mount's motor state, which it replays from exactly.
    pub actor: Option<PlayerState>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub schema_version: u32,
    pub tick: u64,
    pub vehicles: Vec<VehicleSnapshot>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FireIntent {
    pub scale: f32,
    pub projectile: String,
    pub vehicle: VehicleId,
    pub occupant: Option<OccupantId>,
    pub owner: OwnerId,
    pub origin: [f32; 3],
    pub direction: [f32; 3],
    pub velocity: [f32; 3],
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Intent {
    Fire(FireIntent),
    Mounted {
        vehicle: VehicleId,
        occupant: Occupant,
        seat: usize,
        transform: Transform,
        pose: String,
    },
    Dismounted {
        vehicle: VehicleId,
        occupant: Occupant,
        transform: Transform,
        velocity: [f32; 3],
        forced: bool,
    },
    Effect {
        vehicle: VehicleId,
        id: String,
        active: bool,
    },
    Audio {
        vehicle: VehicleId,
        id: String,
    },
    Animation {
        vehicle: VehicleId,
        id: String,
    },
    /// A charging gun (the pirate cannon's `CannonStrengthLoop`) reached
    /// `charge` of `steps`; the gunner sees it as a bottom print.
    Charged {
        vehicle: VehicleId,
        owner: OwnerId,
        charge: u8,
        steps: u8,
    },
    Destroyed {
        vehicle: VehicleId,
        by: OwnerId,
    },
    TumbleRequested {
        vehicle: VehicleId,
        occupant: Occupant,
        transform: Transform,
        velocity: [f32; 3],
        requested_ticks: u64,
        scale: f32,
    },
    Removed {
        vehicle: VehicleId,
    },
    RespawnDue {
        scale: f32,
        spawn_id: SpawnId,
        definition: String,
        owner: OwnerId,
        transform: Transform,
    },
    /// A vehicle whose definition smashes struck something (`other` is
    /// that collider's user data) moving `speed` into it at `point`.
    Struck {
        vehicle: VehicleId,
        owner: OwnerId,
        other: u128,
        /// The part of `other` struck, when it is made of parts (a chunk of
        /// bricks sharing a collider).
        other_part: u32,
        point: [f32; 3],
        speed: f32,
        /// The vehicle's velocity before the step that struck.
        velocity: [f32; 3],
    },
    RunOver {
        vehicle: VehicleId,
        owner: OwnerId,
        target: OccupantId,
        damage: f32,
        velocity: [f32; 3],
    },
}
#[derive(Clone, Copy, Debug)]
pub enum VehiclePart {
    Chassis,
    Turret,
}
/// An attached turret's own damage pool, v20's `TankTurretVehicle`'s.
pub const TURRET_MAX_DAMAGE: f32 = 250.;
#[derive(Clone, Copy, Debug)]
pub enum DamageKind {
    Direct,
    Radius,
    Burn,
    Impact,
}
struct Instance {
    spawn: Spawn,
    body: RigidBodyHandle,
    collider: ColliderHandle,
    turret_collider: Option<ColliderHandle>,
    /// Torque's wheels; none once destroyed.
    wheels: Vec<WheelState>,
    seats: Vec<Option<Occupant>>,
    controls: Vec<Controls>,
    damage: f32,
    born: u64,
    /// A tumble body given a length (`tumble(%obj, %time)`): it ends at
    /// this tick, however it lies, instead of when it settles.
    ends: Option<u64>,
    dead_at: Option<u64>,
    last_damage: OwnerId,
    last_shot: Option<u64>,
    charge_started: Option<u64>,
    turret_damage: Option<f32>,
    charge: u8,
    steering: f32,
    animation: String,
    water: bool,
    previous_velocity: Vec3,
    jump_held: bool,
    fire_held: bool,
    mounted_once: bool,
    water_coverage: f32,
    energy: f32,
    jetting: bool,
    energy_phase: u8,
    /// Torque `mSteering`: accumulated mouse steering (yaw, pitch), radians.
    mouse_steering: [f32; 2],
    /// 120 Hz ticks since the driver's move last turned, up to
    /// [`AUTO_RETURN_QUIET`].
    steering_quiet: u8,
    /// Player-type mounts run on the player motor with their datablock.
    actor: Option<Player>,
    /// Jumps instead of travel: host teleports and openings that carried
    /// it ([`VehiclesWorld::relocations`]).
    relocations: u64,
}
impl Instance {
    fn weapon_available(&self, d: &Definition) -> bool {
        self.dead_at.is_none()
            && d.weapon.is_some()
            && self
                .turret_damage
                .is_none_or(|damage| damage < TURRET_MAX_DAMAGE)
    }
    fn cancel_charge(&mut self, id: VehicleId, d: &Definition, intents: &mut Vec<Intent>) {
        if self.charge > 0 {
            intents.push(Intent::Effect {
                vehicle: id,
                id: "CannonFuseImage".into(),
                active: false,
            });
            if let Some((o, gun)) = d
                .weapon_seat()
                .and_then(|seat| self.seats[seat])
                .zip(d.weapon.as_ref())
            {
                intents.push(Intent::Charged {
                    vehicle: id,
                    owner: o.owner,
                    charge: 0,
                    steps: gun.charge_steps,
                });
            }
        }
        self.charge = 0;
        self.charge_started = None;
        self.fire_held = false;
    }
    fn velocity(&self, _: &Definition, b: &RigidBody) -> Vec3 {
        match &self.actor {
            Some(actor) => Vec3::from(actor.state().velocity),
            None => b.linvel(),
        }
    }
}
/// A player-type mount's `PlayerData` as motor constants: its box, speeds,
/// `runForce`/mass, `jumpForce`/mass, surfaces, energy and density.
/// Call for a player-type mount with its validated canonical scale. The
/// renderer reads this same motor box to split the body at its travel middle.
pub fn actor_tuning(d: &Definition, scale: f32) -> PlayerTuning {
    let (min, max) = d
        .collision_hulls
        .iter()
        .flatten()
        .fold((Vec3::MAX, Vec3::MIN), |(lo, hi), p| {
            (lo.min(Vec3::from_array(*p)), hi.max(Vec3::from_array(*p)))
        });
    let size = (max - min).max(Vec3::splat(0.1));
    let authored = |key: &str| d.authored.get(key).and_then(|v| v.parse::<f32>().ok());
    let slope = d.run_surface_angle.clamped(1., 89.);
    let [uf, ub, us] = d.underwater_speeds;
    PlayerTuning {
        width: size.x.max(size.z),
        stand_height: size.y,
        crouch_height: size.y,
        stand_eye: size.y * 0.9,
        crouch_eye: size.y * 0.9,
        eye_forward: 0.0,
        forward: d.max_speed,
        backward: d.reverse_speed,
        sideways: d.max_side_speed,
        crouch_forward: d.max_speed,
        crouch_backward: d.reverse_speed,
        crouch_sideways: d.max_side_speed,
        underwater_forward: uf,
        underwater_backward: ub,
        underwater_sideways: us,
        acceleration: (d.engine_force / d.mass.max(0.001)).max(0.001),
        jump_speed: d.jump_speed.max(0.),
        density: d.density.max(0.05),
        drag: d.drag.max(0.001),
        slope_degrees: slope,
        jump_surface_degrees: authored("jumpsurfaceangle")
            .unwrap_or(slope)
            .clamped(1., 89.),
        // `jumpDelay` in 32 ms ticks, at 120 Hz.
        jump_delay_ticks: (authored("jumpdelay").unwrap_or(0.) * 3.75).clamped(0., 255.) as u8,
        can_jet: false,
        max_energy: d.energy.maximum.max(0.),
        recharge: d.energy.recharge_per_32ms.max(0.) / TORQUE_TICK,
        min_jet_energy: 0.,
        jet_drain: 0.,
        // A floating rowboat rows at its underwater speeds.
        swim_coverage: if d.family == Family::Rowboat {
            0.05
        } else {
            0.9
        },
        ..PlayerTuning::default()
    }
    .scaled(scale)
}
/// Feet and heading of a spawn transform.
fn feet_and_yaw(t: &Transform) -> (Vec3, f32) {
    let forward = Quat::from_array(t.rotation) * Vec3::NEG_Z;
    (Vec3::from_array(t.position), forward.x.atan2(-forward.z))
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct PendingRespawn {
    tick: u64,
    spawn: Spawn,
}
/// Server authority component. Call pre_step, shared PhysicsWorld::step, then post_step once per tick.
/// Never attach an instance to another PhysicsWorld; physics handles are private and world-local.
pub struct VehiclesWorld {
    catalog: BTreeMap<String, Definition>,
    instances: BTreeMap<VehicleId, Instance>,
    occupied: BTreeMap<OccupantId, (VehicleId, usize)>,
    tick: u64,
    intents: Vec<Intent>,
    respawns: Vec<PendingRespawn>,
    catalog_fingerprint: String,
    step_pending: bool,
    /// A client's copy predicting the vehicle it drives: only the host
    /// removes, wrecks or respawns vehicles.
    prediction: bool,
    /// Vehicles the host is holding up (a held object): a held tumble
    /// body never counts as settled.
    held: BTreeSet<VehicleId>,
}
fn pose(t: &Transform) -> Pose {
    Pose::from_parts(Vec3::from_array(t.position), Quat::from_array(t.rotation))
}
fn transform(p: &Pose) -> Transform {
    Transform {
        position: p.translation.to_array(),
        rotation: p.rotation.to_array(),
    }
}
fn local_pose(t: &Transform, scale: f32) -> Pose {
    let mut p = pose(t);
    p.translation *= scale;
    p
}
fn seat_pose(body: &RigidBody, seat: &Seat, scale: f32) -> Transform {
    transform(&(body.position() * local_pose(&seat.transform, scale)))
}
/// A seat's controls once its rider leaves or a new one sits down: at rest,
/// except that an attached turret (the Tank's `TankTurretPlayer`) keeps
/// pointing where its last gunner left it. In v20 the turret is its own
/// Player object, so its rotation stays put between gunners and the next
/// gunner takes control looking where it points.
fn idle_controls(d: &Definition, v: &Instance, seat: usize) -> Controls {
    let turret = d.attachment_mount.is_some()
        && !d.is_actor()
        && d.seats.get(seat).is_some_and(|s| s.weapon);
    match v.controls.get(seat) {
        Some(kept) if turret => Controls {
            aim_yaw: kept.aim_yaw,
            aim_pitch: kept.aim_pitch,
            ..Controls::default()
        },
        _ => Controls::default(),
    }
}
fn effective_seat_pose(b: &RigidBody, d: &Definition, v: &Instance, index: usize) -> Transform {
    if d.weapon_seat() == Some(index)
        && v.turret_damage
            .is_some_and(|damage| damage >= TURRET_MAX_DAMAGE)
        && let Some(t) = &d.attachment_fallback_seat
    {
        return transform(&(b.position() * local_pose(t, v.spawn.scale)));
    }
    let base = &d.seats[index];
    if d.weapon_seat() == Some(index) && d.attachment_mount.is_some() {
        let pivot = d
            .attachment_mount
            .as_ref()
            .map_or(Vec3::ZERO, |t| Vec3::from_array(t.position));
        let yaw = Quat::from_rotation_y(v.controls[index].aim_yaw);
        let mut local = pose(&base.transform);
        local.translation = pivot + yaw * (local.translation - pivot);
        local.rotation = yaw * local.rotation;
        local.translation *= v.spawn.scale;
        return transform(&(b.position() * local));
    }
    seat_pose(b, base, v.spawn.scale)
}
impl VehiclesWorld {
    pub fn new(pack: Pack) -> Result<Self> {
        pack.validate()?;
        let catalog_fingerprint = format!("{:x}", Sha256::digest(serde_json::to_vec(&pack)?));
        Ok(Self {
            catalog_fingerprint,
            step_pending: false,
            catalog: pack
                .definitions
                .into_iter()
                .map(|d| (d.id.clone(), d))
                .collect(),
            instances: BTreeMap::new(),
            occupied: BTreeMap::new(),
            tick: 0,
            intents: vec![],
            respawns: vec![],
            prediction: false,
            held: BTreeSet::new(),
        })
    }
    /// The vehicles the host holds this tick (see `held`), replacing the
    /// last set.
    pub fn set_held(&mut self, held: impl IntoIterator<Item = VehicleId>) {
        self.held = held.into_iter().collect();
    }
    /// Make this a client's prediction copy: its vehicles are never removed,
    /// wrecked or respawned here; the host's poses and listings decide that.
    pub fn set_prediction(&mut self, prediction: bool) {
        self.prediction = prediction;
    }
    /// Maps shared-world physics hits to gameplay identity without using collider user_data.
    pub fn classify_collider(&self, collider: ColliderHandle) -> Option<(VehicleId, VehiclePart)> {
        self.instances.iter().find_map(|(id, v)| {
            if v.collider == collider {
                Some((*id, VehiclePart::Chassis))
            } else if v.turret_collider == Some(collider) {
                Some((*id, VehiclePart::Turret))
            } else {
                None
            }
        })
    }
    pub fn definition(&self, id: &str) -> Option<&Definition> {
        self.catalog.get(id)
    }
    /// The definition a live vehicle was spawned from.
    pub fn definition_of(&self, id: VehicleId) -> Option<&Definition> {
        self.catalog.get(&self.instances.get(&id)?.spawn.definition)
    }
    /// An authored gun still usable on the current body. Occupancy and
    /// trigger state remain separate from this physical capability.
    pub fn weapon_available(&self, id: VehicleId) -> bool {
        self.instances
            .get(&id)
            .is_some_and(|v| v.weapon_available(&self.catalog[&v.spawn.definition]))
    }
    pub fn definitions(&self) -> impl Iterator<Item = &Definition> {
        self.catalog.values()
    }
    /// Colliders attached to a vehicle body (chassis and turret), so the host
    /// can tag them for weapon hit attribution.
    pub fn colliders_of(&self, world: &PhysicsWorld, id: VehicleId) -> Vec<ColliderHandle> {
        self.instances
            .get(&id)
            .map(|v| world.bodies[v.body].colliders().to_vec())
            .unwrap_or_default()
    }
    /// The contact solver treats a kinematic body (a walking player) as
    /// immovable, so a vehicle that ran into one stopped dead or bounced
    /// back as off a wall. This shares last step's contact impulses between
    /// each vehicle touching `collider` and that body as between two free
    /// bodies, with the body weighing `mass`: of the impulse J the solver
    /// gave the vehicle (masses m and M) it keeps J * M / (m + M), and the
    /// body takes the rest, -J / (m + M) of velocity. A heavy ball barely
    /// slows for a player; a player can barely move it. Returns the body's
    /// change of velocity.
    pub fn share_contacts(
        &self,
        world: &mut PhysicsWorld,
        collider: ColliderHandle,
        mass: f32,
    ) -> Vec3 {
        if mass.is_nan() || mass <= 0. {
            return Vec3::ZERO;
        }
        let mut kicks = Vec::new();
        for pair in world.contact_pairs_with(collider) {
            let (other, sign) = if pair.collider1 == collider {
                (pair.collider2, 1.)
            } else {
                (pair.collider1, -1.)
            };
            let Some(body) = world.colliders.get(other).and_then(|c| c.parent()) else {
                continue;
            };
            let Some(v) = self.instances.values().find(|v| v.body == body) else {
                continue;
            };
            if v.actor.is_some() || !world.bodies[body].is_dynamic() {
                continue;
            }
            // Each manifold's normal points from collider1 to collider2:
            // what collider2's body was pushed along. Only hits from the
            // side: a body standing on a vehicle, or under one, keeps
            // holding it up as the ground does.
            let impulse: Vec3 = pair
                .manifolds()
                .iter()
                .filter(|m| m.data.normal.y.abs() < SIDE_HIT)
                .map(|m| m.data.normal * m.points.iter().map(|p| p.data.impulse).sum::<f32>())
                .sum::<Vec3>()
                * sign;
            if impulse.is_finite() && impulse.length_squared() > 0. {
                kicks.push((body, impulse));
            }
        }
        let mut change = Vec3::ZERO;
        for (body, impulse) in kicks {
            let b = &mut world.bodies[body];
            let total = b.mass() + mass;
            b.apply_impulse(-impulse * (b.mass() / total), true);
            change -= impulse / total;
        }
        change
    }
    /// Transfer actual motion removed by a character sweep to this body's
    /// side. The host checks permissions and ownership of a physics grip.
    /// Returns the momentum transferred, zero for an ineligible contact.
    pub fn push_contact(
        &mut self,
        world: &mut PhysicsWorld,
        id: VehicleId,
        contact: &bri_motor::torque::SweepContact,
        mover_mass: f32,
    ) -> f32 {
        let Some(v) = self.instances.get(&id) else {
            return 0.0;
        };
        if v.dead_at.is_some()
            || v.actor.is_some()
            || contact.normal.y.abs() >= SIDE_HIT
            || world
                .colliders
                .get(contact.collider)
                .and_then(|c| c.parent())
                != Some(v.body)
        {
            return 0.0;
        }
        crate::contact_push::transfer(
            world,
            v.body,
            contact.point,
            contact.normal,
            contact.velocity,
            contact.removed_speed,
            mover_mass,
        )
    }
    /// Where an occupant sits, if mounted.
    pub fn occupant(&self, occupant: OccupantId) -> Option<(VehicleId, usize)> {
        self.occupied.get(&occupant).copied()
    }
    /// Abandon a held charge without firing on trigger release. Used when
    /// an action is cancelled; ordinary release remains a firing action.
    pub fn cancel_weapon_charge(&mut self, occupant: OccupantId) {
        let Some((id, seat)) = self.occupant(occupant) else {
            return;
        };
        if self.definition_of(id).and_then(Definition::weapon_seat) != Some(seat) {
            return;
        }
        let Some(v) = self.instances.get_mut(&id) else {
            return;
        };
        v.cancel_charge(id, &self.catalog[&v.spawn.definition], &mut self.intents);
        v.controls[seat].fire = false;
    }

    /// Current authoritative occupancy. Geometry observations may be shared
    /// for a tick, but admission and coordination use live seat state.
    pub fn seat_occupant(&self, id: VehicleId, seat: usize) -> Option<Occupant> {
        self.instances.get(&id)?.seats.get(seat).copied().flatten()
    }

    /// The rigid body for hosts that move a held vehicle. Player mounts have none.
    pub fn body_of(&self, id: VehicleId) -> Option<RigidBodyHandle> {
        let v = self.instances.get(&id)?;
        v.actor.is_none().then_some(v.body)
    }
    pub fn is_alive(&self, id: VehicleId) -> bool {
        self.instances.get(&id).is_some_and(|v| v.dead_at.is_none())
    }
    pub fn tick(&self) -> u64 {
        self.tick
    }
    pub fn drain_intents(&mut self) -> Vec<Intent> {
        std::mem::take(&mut self.intents)
    }
    pub fn spawn(&mut self, world: &mut PhysicsWorld, s: Spawn) -> Result<()> {
        ensure!(
            !self.instances.contains_key(&s.id)
                && !self.respawns.iter().any(|p| p.spawn.id == s.id),
            "duplicate vehicle identity"
        );
        ensure!(
            s.spawn_id.is_none_or(|id| !self
                .instances
                .values()
                .any(|v| v.spawn.spawn_id == Some(id))
                && !self.respawns.iter().any(|p| p.spawn.spawn_id == Some(id))),
            "duplicate spawn brick"
        );
        let d = self.catalog.get(&s.definition).context("unknown vehicle")?;
        let (builder, collider, prepared_turret) = prepare_spawn(&s, d, world.gravity.length())?;
        let (body, collider) = world.insert(builder, collider);
        let turret_collider =
            prepared_turret.map(|collider| world.insert_collider(collider, Some(body)));
        let actor = if d.is_actor() {
            let (feet, yaw) = feet_and_yaw(&s.transform);
            Some(Player::adopt(
                body,
                collider,
                feet,
                yaw,
                actor_tuning(d, s.scale),
            )?)
        } else {
            None
        };
        let count = d.seats.len();
        self.instances.insert(
            s.id,
            Instance {
                last_damage: s.owner,
                spawn: s,
                body,
                collider,
                turret_collider,
                wheels: vec![WheelState::default(); d.wheels.len()],
                seats: vec![None; count],
                controls: vec![Controls::default(); count],
                damage: 0.,
                born: self.tick,
                ends: None,
                dead_at: None,
                last_shot: None,
                charge_started: None,
                turret_damage: d.attachment_model.as_ref().map(|_| 0.),
                charge: 0,
                steering: 0.,
                animation: "root".into(),
                water: false,
                previous_velocity: Vec3::ZERO,
                jump_held: false,
                fire_held: false,
                mounted_once: false,
                water_coverage: 0.,
                energy: 0.,
                jetting: false,
                energy_phase: 0,
                mouse_steering: [0.; 2],
                steering_quiet: AUTO_RETURN_QUIET,
                actor,
                relocations: 0,
            },
        );
        Ok(())
    }
    /// Host must resolve authenticated owner/entity and minigame/trust permission before mounting.
    /// position is authoritative player position, never an unchecked client-provided location.
    pub fn mount(
        &mut self,
        world: &PhysicsWorld,
        id: VehicleId,
        seat: usize,
        occupant: Occupant,
        position: [f32; 3],
    ) -> Result<()> {
        ensure!(
            !self.occupied.contains_key(&occupant.id),
            "occupant already mounted"
        );
        let v = self.instances.get_mut(&id).context("unknown vehicle")?;
        ensure!(v.dead_at.is_none(), "destroyed vehicle");
        let d = &self.catalog[&v.spawn.definition];
        let sd = d.seats.get(seat).context("invalid seat")?;
        ensure!(v.seats[seat].is_none(), "seat occupied");
        let t = effective_seat_pose(&world.bodies[v.body], d, v, seat);
        let p = Vec3::from_array(position);
        ensure!(
            p.is_finite()
                && p.distance(Vec3::from_array(t.position)) <= d.mount_distance * v.spawn.scale,
            "mount out of reach"
        );
        v.seats[seat] = Some(occupant);
        v.mounted_once = true;
        v.controls[seat] = idle_controls(d, v, seat);
        self.occupied.insert(occupant.id, (id, seat));
        self.intents.push(Intent::Mounted {
            vehicle: id,
            occupant,
            seat,
            transform: t,
            pose: sd.pose.clone(),
        });
        Ok(())
    }
    /// Reset a live vehicle's motion to a replicated one (a client's own
    /// driven vehicle, before it replays its moves). A player-type mount's
    /// motor takes the feet, facing and velocity.
    pub fn restore_motion(
        &mut self,
        world: &mut PhysicsWorld,
        id: VehicleId,
        motion: &Motion,
    ) -> Result<()> {
        let rotation = Quat::from_array(motion.transform.rotation);
        let position = Vec3::from_array(motion.transform.position);
        ensure!(
            position.is_finite()
                && rotation.is_finite()
                && rotation.length_squared() > 0.5
                && motion.velocity.iter().all(|x| x.is_finite())
                && motion.angular_velocity.iter().all(|x| x.is_finite())
                && motion.mouse_steering.iter().all(|x| x.is_finite())
                && motion.steering.is_finite(),
            "invalid vehicle motion"
        );
        let v = self.instances.get_mut(&id).context("unknown vehicle")?;
        if let Some(actor) = &mut v.actor {
            let (feet, yaw) = feet_and_yaw(&motion.transform);
            // The motor's whole state when the host sent it (its Torque
            // tick phase and the last tick's feet included), else the pose.
            let state = match &motion.actor {
                Some(sent) => PlayerState {
                    owner: actor.state().owner,
                    ..sent.clone()
                },
                None => {
                    let mut state = actor.state().clone();
                    state.feet = feet.to_array();
                    state.yaw = yaw;
                    state.velocity = motion.velocity;
                    state
                }
            };
            let tuning = actor.tuning().clone();
            actor.restore(world, state, tuning)?;
            v.previous_velocity = Vec3::from_array(motion.velocity);
            bri_physics::detect_collisions(world);
            return Ok(());
        }
        let b = world
            .bodies
            .get_mut(v.body)
            .context("vehicle body missing")?;
        b.set_position(Pose::from_parts(position, rotation.normalize()), true);
        b.set_linvel(Vec3::from_array(motion.velocity), true);
        b.set_angvel(Vec3::from_array(motion.angular_velocity), true);
        v.previous_velocity = Vec3::from_array(motion.velocity);
        v.mouse_steering = motion.mouse_steering;
        v.steering = motion.steering;
        v.steering_quiet = motion.steering_quiet.min(AUTO_RETURN_QUIET);
        let d = &self.catalog[&v.spawn.definition];
        let count = v.wheels.len();
        if motion.wheel_suspension.len() == count
            && motion.wheel_contact.len() == count
            && motion.wheel_rotation.len() == count
            && motion.wheel_tire.len() == count
        {
            ensure!(
                motion.wheel_tire.iter().all(TireState::is_finite)
                    && motion.wheel_suspension.iter().all(|x| x.is_finite())
                    && motion.wheel_rotation.iter().all(|x| x.is_finite()),
                "invalid vehicle motion"
            );
            for (i, (w, def)) in v.wheels.iter_mut().zip(&d.wheels).enumerate() {
                *w = WheelState {
                    extension: (motion.wheel_suspension[i] / (def.rest_length * v.spawn.scale))
                        .clamped(0., 1.),
                    contact: motion.wheel_contact[i],
                    rotation: motion.wheel_rotation[i],
                    tire: motion.wheel_tire[i],
                };
            }
        }
        bri_physics::detect_collisions(world);
        Ok(())
    }
    /// Where a seat is now, for mounts a script forces regardless of reach.
    pub fn seat_position(
        &self,
        world: &PhysicsWorld,
        id: VehicleId,
        seat: usize,
    ) -> Option<[f32; 3]> {
        let v = self.instances.get(&id)?;
        let d = &self.catalog[&v.spawn.definition];
        (seat < d.seats.len())
            .then(|| effective_seat_pose(&world.bodies[v.body], d, v, seat).position)
    }
    pub fn set_controls(
        &mut self,
        owner: OwnerId,
        occupant: OccupantId,
        input: Controls,
    ) -> Result<()> {
        let input = input.validate()?;
        let (id, seat) = *self.occupied.get(&occupant).context("not mounted")?;
        let v = self.instances.get_mut(&id).unwrap();
        ensure!(
            v.seats[seat].is_some_and(|x| x.owner == owner),
            "owner mismatch"
        );
        ensure!(v.dead_at.is_none(), "destroyed vehicle");
        let sd = &self.catalog[&v.spawn.definition].seats[seat];
        ensure!(
            sd.controls || sd.weapon,
            "passenger has no vehicle controls"
        );
        v.controls[seat] = input;
        Ok(())
    }
    /// Uses the authored exit search and v20's last-point fallback.
    pub fn dismount(
        &mut self,
        world: &PhysicsWorld,
        owner: OwnerId,
        occupant: OccupantId,
        forced: bool,
    ) -> Result<()> {
        let (id, seat) = *self.occupied.get(&occupant).context("not mounted")?;
        let v = self.instances.get_mut(&id).unwrap();
        let passenger = v.seats[seat].unwrap();
        ensure!(passenger.owner == owner, "owner mismatch");
        let d = &self.catalog[&v.spawn.definition];
        let b = &world.bodies[v.body];
        let mounted = effective_seat_pose(b, d, v, seat);
        let start = Vec3::from_array(mounted.position);
        ensure!(
            d.family != Family::Tumble || forced,
            "tumbling occupant cannot manually dismount"
        );
        let body_velocity = v.velocity(d, b);
        // `doSimpleDismount` (the skis and the tumble body, and any Add-On
        // that sets it): out in place with the vehicle's velocity.
        let simple = matches!(d.family, Family::Skis | Family::Tumble)
            || d.authored.get("dosimpledismount").is_some_and(|v| {
                !matches!(v.trim().to_ascii_lowercase().as_str(), "0" | "false" | "")
            });
        if simple {
            if d.weapon_seat() == Some(seat) {
                v.cancel_charge(id, d, &mut self.intents);
            }
            v.seats[seat] = None;
            v.controls[seat] = idle_controls(d, v, seat);
            self.occupied.remove(&occupant);
            self.intents.push(Intent::Dismounted {
                vehicle: id,
                occupant: passenger,
                transform: mounted,
                velocity: body_velocity.to_array(),
                forced,
            });
            return Ok(());
        }
        let queries = world.query_pipeline_with_filter(
            QueryFilter::default()
                .exclude_rigid_body(v.body)
                .exclude_sensors(),
        );
        // `Armor::doDismount`: 2.2 up the seat (the rider's transform turns
        // it), then 3 up, 3 down, 3 along world +X and -X, all times the
        // vehicle's scale. The first clear point is taken, with its offset
        // as an impulse (per unit of mass, so a change of velocity).
        let offsets = [
            Quat::from_array(mounted.rotation).normalize() * Vec3::Y * 2.2,
            Vec3::Y * 3.,
            -Vec3::Y * 3.,
            Vec3::X * 3.,
            -Vec3::X * 3.,
        ]
        .map(|offset| offset * v.spawn.scale);
        let exit = offsets.into_iter().find_map(|offset| {
            exit_clear(&queries, start, offset, passenger.body).then_some((start + offset, offset))
        });
        // With every point blocked the rider still gets out, at the last
        // point tried and without the push; a forced dismount (death,
        // removal) stays on the seat instead.
        let (p, impulse) = exit.unwrap_or(if forced {
            (start, Vec3::ZERO)
        } else {
            (start + offsets[4], Vec3::ZERO)
        });
        // `setVelocity(%vehicle.getVelocity())`: the body's own velocity,
        // with no share of its spin.
        let velocity = body_velocity + impulse;
        if d.weapon_seat() == Some(seat) {
            v.cancel_charge(id, d, &mut self.intents);
        }
        v.seats[seat] = None;
        v.controls[seat] = idle_controls(d, v, seat);
        self.occupied.remove(&occupant);
        self.intents.push(Intent::Dismounted {
            vehicle: id,
            occupant: passenger,
            transform: Transform {
                position: p.to_array(),
                rotation: mounted.rotation,
            },
            velocity: velocity.to_array(),
            forced,
        });
        Ok(())
    }
    pub fn disconnect(&mut self, world: &PhysicsWorld, owner: OwnerId) {
        let ids: Vec<_> = self
            .instances
            .values()
            .flat_map(|v| v.seats.iter().flatten())
            .filter(|o| o.owner == owner)
            .map(|o| o.id)
            .collect();
        for id in ids {
            let _ = self.dismount(world, owner, id, true);
        }
    }
    pub fn remove(&mut self, world: &mut PhysicsWorld, id: VehicleId) -> Result<()> {
        let seats = self
            .instances
            .get(&id)
            .context("unknown vehicle")?
            .seats
            .clone();
        for o in seats.into_iter().flatten() {
            self.dismount(world, o.owner, o.id, true)?;
        }
        let v = self.instances.remove(&id).unwrap();
        world.remove_body(v.body);
        self.intents.push(Intent::Removed { vehicle: id });
        Ok(())
    }
    /// Server-authorized projectile/event impulse, resolved to the shared body.
    pub fn apply_impulse(
        &mut self,
        world: &mut PhysicsWorld,
        id: VehicleId,
        point: [f32; 3],
        impulse: [f32; 3],
    ) -> Result<()> {
        let point = Vec3::from_array(point);
        let impulse = Vec3::from_array(impulse);
        ensure!(point.is_finite() && impulse.is_finite(), "invalid impulse");
        let v = self.instances.get_mut(&id).context("unknown vehicle")?;
        let d = &self.catalog[&v.spawn.definition];
        if let Some(actor) = &mut v.actor {
            // Player::applyImpulse: velocity changes by impulse / mass.
            actor.push(impulse / d.mass);
        } else {
            world.bodies[v.body].apply_impulse_at_point(impulse, point, true);
        }
        Ok(())
    }
    /// Host-owned transition preserves world velocity when starting skis or a tumble.
    /// Host-authorized energy restoration/event. Bounds come from the native datablock.
    pub fn set_energy(&mut self, id: VehicleId, energy: f32) -> Result<()> {
        let v = self.instances.get_mut(&id).context("unknown vehicle")?;
        let maximum = self.catalog[&v.spawn.definition].energy.maximum;
        ensure!(
            energy.is_finite() && (0. ..=maximum).contains(&energy),
            "energy outside datablock bounds"
        );
        v.energy = energy;
        Ok(())
    }
    /// A tumble body lasts `ticks` (120 a second) from now, then lets its
    /// rider up, rather than ending when it settles.
    pub fn set_tumble_ticks(&mut self, id: VehicleId, ticks: u64) -> Result<()> {
        let v = self.instances.get_mut(&id).context("unknown vehicle")?;
        ensure!(
            self.catalog[&v.spawn.definition].family == Family::Tumble,
            "only a tumble has a length"
        );
        v.ends = Some(self.tick + ticks.max(1));
        Ok(())
    }
    pub fn set_velocity(
        &mut self,
        world: &mut PhysicsWorld,
        id: VehicleId,
        velocity: [f32; 3],
    ) -> Result<()> {
        let velocity = Vec3::from_array(velocity);
        ensure!(velocity.is_finite(), "invalid velocity");
        let v = self.instances.get_mut(&id).context("unknown vehicle")?;
        if let Some(actor) = &mut v.actor {
            let grounded = actor.state().grounded && velocity.y <= 0.;
            actor.set_motion(velocity, grounded);
        } else {
            world.bodies[v.body].set_linvel(velocity, true);
        }
        Ok(())
    }
    /// Host-authorized `setTransform` followed by `setVelocity("0 0 0")` and
    /// `setAngularVelocity("0 0 0")`, as admin teleports move a ridden
    /// vehicle. Actor mounts keep only the heading, like `Player::setTransform`.
    pub fn set_transform(
        &mut self,
        world: &mut PhysicsWorld,
        id: VehicleId,
        transform: &Transform,
    ) -> Result<()> {
        ensure!(
            transform
                .position
                .iter()
                .chain(transform.rotation.iter())
                .all(|x| x.is_finite())
                && transform.position.iter().all(|x| x.abs() <= 1_000_000.)
                && (Quat::from_array(transform.rotation).length_squared() - 1.).abs() < 0.001,
            "invalid transform"
        );
        let v = self.instances.get_mut(&id).context("unknown vehicle")?;
        if let Some(actor) = &mut v.actor {
            let (feet, yaw) = feet_and_yaw(transform);
            actor.teleport(world, feet, yaw)?;
        } else {
            let body = &mut world.bodies[v.body];
            body.set_position(pose(transform), true);
            body.set_linvel(Vec3::ZERO, true);
            body.set_angvel(Vec3::ZERO, true);
        }
        v.previous_velocity = Vec3::ZERO;
        v.relocations += 1;
        Ok(())
    }
    /// How many times vehicle `id` jumped instead of travelling: each
    /// [`VehiclesWorld::set_transform`] and each opening that carried it.
    /// Whoever follows its path between two looks treats a change as a
    /// jump to the new place, never as the line between.
    pub fn relocations(&self, id: VehicleId) -> Option<u64> {
        self.instances.get(&id).map(|v| v.relocations)
    }
    /// Where a vehicle is, for what it passes through: its middle.
    pub fn centre(&self, world: &PhysicsWorld, id: VehicleId) -> Option<Vec3> {
        let v = self.instances.get(&id)?;
        Some(match &v.actor {
            Some(actor) => {
                let state = actor.state();
                Vec3::from(state.feet) + Vec3::Y * actor.middle()
            }
            None => world.bodies.get(v.body)?.center_of_mass(),
        })
    }
    /// The same travel middle at the between-tick pose the host draws.
    pub fn shown_centre(&self, world: &PhysicsWorld, id: VehicleId) -> Option<Vec3> {
        let v = self.instances.get(&id)?;
        match &v.actor {
            Some(actor) => Some(Vec3::from(actor.state().shown_feet()) + Vec3::Y * actor.middle()),
            None => self.centre(world, id),
        }
    }
    /// Every vehicle there is.
    pub fn ids(&self) -> impl Iterator<Item = VehicleId> + '_ {
        self.instances.keys().copied()
    }
    /// Carry a vehicle rigidly by `carry` (through a linked brick's
    /// opening): its pose moves and turns, and its motion turns with it, so
    /// it comes out moving as it went in.
    pub fn carry(
        &mut self,
        world: &mut PhysicsWorld,
        id: VehicleId,
        carry: &glam::Affine3A,
    ) -> Result<()> {
        let (scale, turn, _) = carry.to_scale_rotation_translation();
        ensure!(
            scale.abs_diff_eq(Vec3::ONE, 1e-3) && carry.translation.is_finite(),
            "invalid carry"
        );
        let v = self.instances.get_mut(&id).context("unknown vehicle")?;
        if let Some(actor) = &mut v.actor {
            let state = actor.state().clone();
            let feet = carry.transform_point3(Vec3::from(state.feet));
            let velocity = turn * Vec3::from(state.velocity);
            let yaw = bri_content::passage::carried_yaw(carry, state.yaw);
            actor.teleport(world, feet, yaw)?;
            actor.set_motion(velocity, state.grounded);
        } else {
            let body = &mut world.bodies[v.body];
            let position = *body.position();
            let moved = Pose::from_parts(
                carry.transform_point3(position.translation),
                (turn * position.rotation).normalize(),
            );
            let (linear, angular) = (turn * body.linvel(), turn * body.angvel());
            body.set_position(moved, true);
            body.set_linvel(linear, true);
            body.set_angvel(angular, true);
        }
        v.previous_velocity = turn * v.previous_velocity;
        v.relocations += 1;
        Ok(())
    }
    /// Script onWreck equivalent; root starts deathVehicle and clears weapon ski state.
    pub fn wreck_skis(&mut self, world: &mut PhysicsWorld, id: VehicleId) -> Result<()> {
        let v = self.instances.get(&id).context("unknown vehicle")?;
        ensure!(
            self.catalog[&v.spawn.definition].family == Family::Skis,
            "not skis"
        );
        let intent = if let Some(occupant) = v.seats[0] {
            let b = &world.bodies[v.body];
            let speed = b.linvel().length();
            let ticks = ((((speed - 10.) / 50.) * 7. + 1.).clamped(1., 7.) * 120.).round() as u64;
            Some(Intent::TumbleRequested {
                vehicle: id,
                occupant,
                transform: transform(b.position()),
                velocity: b.linvel().to_array(),
                requested_ticks: ticks,
                scale: v.spawn.scale,
            })
        } else {
            None
        };
        self.remove(world, id)?;
        if let Some(intent) = intent {
            self.intents.push(intent);
        }
        Ok(())
    }
    pub fn cancel_spawn(&mut self, world: &mut PhysicsWorld, spawn: SpawnId) -> Result<()> {
        self.respawns.retain(|p| p.spawn.spawn_id != Some(spawn));
        let ids: Vec<_> = self
            .instances
            .iter()
            .filter(|(_, v)| v.spawn.spawn_id == Some(spawn))
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            self.remove(world, id)?;
        }
        Ok(())
    }
    /// Damage is a server-only action after the host's minigame and damage-type scaling policy.
    pub fn damage(
        &mut self,
        world: &PhysicsWorld,
        id: VehicleId,
        amount: f32,
        by: OwnerId,
    ) -> Result<()> {
        ensure!(amount.is_finite() && amount >= 0., "invalid damage");
        let v = self.instances.get_mut(&id).context("unknown vehicle")?;
        let d = &self.catalog[&v.spawn.definition];
        if v.dead_at.is_some() || self.tick - v.born < d.invulnerable_ticks {
            return Ok(());
        }
        let amount = if matches!(
            d.family,
            Family::Wheeled | Family::Flying | Family::Ball | Family::Skis | Family::Tumble
        ) {
            amount / v.spawn.scale
        } else {
            amount
        };
        v.damage = (v.damage + amount).min(d.max_damage);
        v.last_damage = by;
        if v.damage >= d.max_damage {
            v.dead_at = Some(self.tick);
            v.controls.fill(Controls::default());
            v.charge = 0;
            v.charge_started = None;
            v.wheels.clear();
            self.intents.push(Intent::Destroyed { vehicle: id, by });
            let initial = if v
                .turret_damage
                .is_some_and(|damage| damage < TURRET_MAX_DAMAGE)
            {
                Some("v20.projectile.tankturretexplosionprojectile")
            } else {
                d.initial_explosion.as_deref()
            };
            if let Some(projectile) = initial {
                self.intents.push(Intent::Fire(explosion(
                    v,
                    d,
                    projectile,
                    world,
                    d.initial_explosion_offset,
                )));
            }
            if let Some(damage) = &mut v.turret_damage {
                *damage = TURRET_MAX_DAMAGE;
            }
            // The wreck's fire is drawn from the replicated destroyed state
            // (`Definition::wreck_emitters`); no cue is sent for it.
            self.intents.push(Intent::Animation {
                vehicle: id,
                id: "death1".into(),
            });
        }
        Ok(())
    }
    /// Attached Tank turret has its own 250 damage pool. Host routes a turret hit here.
    /// Disabling it returns the gunner to the hull's mount2 and removes its weapon.
    pub fn damage_turret(
        &mut self,
        world: &mut PhysicsWorld,
        id: VehicleId,
        amount: f32,
        by: OwnerId,
    ) -> Result<()> {
        ensure!(amount.is_finite() && amount >= 0., "invalid turret damage");
        let v = self.instances.get_mut(&id).context("unknown vehicle")?;
        let damage = v
            .turret_damage
            .as_mut()
            .context("vehicle has no attached turret")?;
        if *damage >= TURRET_MAX_DAMAGE || v.dead_at.is_some() {
            return Ok(());
        }
        *damage = (*damage + amount).min(TURRET_MAX_DAMAGE);
        if *damage >= TURRET_MAX_DAMAGE {
            if let Some(collider) = v.turret_collider.take() {
                world.remove_collider(collider);
            }
            v.last_damage = by;
            v.charge = 0;
            v.charge_started = None;
            let d = &self.catalog[&v.spawn.definition];
            let gunner = d.weapon_seat();
            if let Some(index) = gunner {
                v.controls[index] = Controls::default();
            }
            self.intents.push(Intent::Fire(explosion(
                v,
                d,
                "v20.projectile.tankturretexplosionprojectile",
                world,
                d.initial_explosion_offset,
            )));
            if let Some((index, o)) = gunner.and_then(|index| v.seats[index].map(|o| (index, o))) {
                let fallback = d.attachment_fallback_seat.as_ref().unwrap();
                self.intents.push(Intent::Mounted {
                    vehicle: id,
                    occupant: o,
                    seat: index,
                    transform: transform(
                        &(world.bodies[v.body].position() * local_pose(fallback, v.spawn.scale)),
                    ),
                    pose: "root".into(),
                });
            }
        }
        Ok(())
    }
    pub fn passenger_protected(&self, id: VehicleId, kind: DamageKind) -> Result<bool> {
        let v = self.instances.get(&id).context("unknown vehicle")?;
        let d = &self.catalog[&v.spawn.definition];
        Ok(match kind {
            DamageKind::Direct => d.protect_direct,
            DamageKind::Radius => d.protect_radius,
            DamageKind::Burn => d.protect_burn,
            DamageKind::Impact => false,
        })
    }
    /// A player touched this vehicle without boarding it
    /// (`WheeledVehicleData::onCollision`). Faster than `minRunOverSpeed`
    /// (at least 2, plus 2 with no driver) it does speed times
    /// `runOverDamageScale` damage; either way it sets the player's velocity to
    /// its own times `runOverPushScale`. Host applies both only where the
    /// minigame lets the vehicle damage the player. Player-type mounts do not.
    pub fn player_contact(
        &mut self,
        world: &PhysicsWorld,
        id: VehicleId,
        target: OccupantId,
        target_velocity: [f32; 3],
    ) -> Result<()> {
        let v = self.instances.get(&id).context("unknown vehicle")?;
        let d = &self.catalog[&v.spawn.definition];
        if self.occupied.contains_key(&target) || d.is_actor() || v.dead_at.is_some() {
            return Ok(());
        }
        ensure!(
            Vec3::from_array(target_velocity).is_finite(),
            "invalid target velocity"
        );
        let velocity = v.velocity(d, &world.bodies[v.body]);
        ensure!(velocity.is_finite(), "invalid contact velocity");
        let driver = d.control_seat().and_then(|seat| v.seats[seat]);
        let authored = |value: f32, default: f32| {
            if value > 0. && value < 1e30 {
                value
            } else {
                default
            }
        };
        let minimum = authored(d.runover_speed, 2.).clamped(2., 999.)
            + if driver.is_none() { 2. } else { 0. };
        let speed = velocity.length();
        self.intents.push(Intent::RunOver {
            vehicle: id,
            owner: driver.map_or(v.spawn.owner, |o| o.owner),
            target,
            // An authored scale of 0 is harmless; only an unset one (out of
            // range) takes Torque's default.
            damage: if speed > minimum {
                speed
                    * if (0. ..1e30).contains(&d.runover_damage) {
                        d.runover_damage
                    } else {
                        5.
                    }
            } else {
                0.
            },
            velocity: (velocity * authored(d.runover_push, 1.2)).to_array(),
        });
        Ok(())
    }
    /// The damage a part of a live vehicle takes to be destroyed: its
    /// definition's `max_damage`, or an attached turret's own pool.
    pub fn max_damage(&self, id: VehicleId, part: VehiclePart) -> Option<f32> {
        let v = self.instances.get(&id).filter(|v| v.dead_at.is_none())?;
        Some(match part {
            VehiclePart::Chassis => self.catalog[&v.spawn.definition].max_damage,
            VehiclePart::Turret => TURRET_MAX_DAMAGE,
        })
    }
    /// Which part of a vehicle a hit at `point` struck: its attached turret
    /// when that is the nearer collider.
    pub fn hit_part(&self, world: &PhysicsWorld, id: VehicleId, point: [f32; 3]) -> VehiclePart {
        let Some(v) = self.instances.get(&id) else {
            return VehiclePart::Chassis;
        };
        let point = Vec3::from_array(point);
        let distance = |c: ColliderHandle| {
            let collider = &world.colliders[c];
            collider
                .shape()
                .distance_to_point(collider.position(), point, true)
        };
        match v.turret_collider {
            Some(turret) if distance(turret) < distance(v.collider) => VehiclePart::Turret,
            _ => VehiclePart::Chassis,
        }
    }
    /// Host samples water height at each body position (None outside authored water volumes).
    /// `waters` are the map's authored liquids: buoyancy and splashes for
    /// vehicles, and swimming for player-type mounts.
    pub fn pre_step(
        &mut self,
        world: &mut PhysicsWorld,
        waters: &[bri_content::water::Water],
    ) -> Result<()> {
        self.pre_step_through(
            world,
            waters,
            &(),
            &bri_content::passage::Passages::default(),
        )?;
        Ok(())
    }
    /// Step player-type mounts through the host's openings with the same
    /// motor and collision-part identities as walking players. Physical
    /// vehicles still cross after the shared physics step.
    pub fn pre_step_through(
        &mut self,
        world: &mut PhysicsWorld,
        waters: &[bri_content::water::Water],
        parts: &dyn bri_motor::torque::PartTags,
        passages: &bri_content::passage::Passages,
    ) -> Result<Vec<(VehicleId, glam::Affine3A)>> {
        let mut carried = Vec::new();
        ensure!(
            (world.integration_parameters.dt - FIXED_DT).abs() < 1e-6,
            "vehicles require shared 120Hz timestep"
        );
        ensure!(!self.step_pending, "pre_step already called");
        self.step_pending = true;
        for (id, v) in &mut self.instances {
            let d = &self.catalog[&v.spawn.definition];
            let driver_seat = d.control_seat();
            let control = driver_seat.map_or_else(Controls::default, |seat| v.controls[seat]);
            let alive = v.dead_at.is_none();
            let driven = alive && driver_seat.is_some_and(|seat| v.seats[seat].is_some());
            let c = if alive { control } else { Controls::default() };
            let wants_jet = alive && (c.jet || c.vertical > 0.);
            if !wants_jet {
                v.jetting = false;
            } else if !v.jetting && v.energy >= d.energy.minimum_jet && d.energy.maximum > 0. {
                v.jetting = true;
            }
            // Exact rational cadence: 25/96 of a 32ms source tick per 120Hz step.
            v.energy_phase += 25;
            if v.energy_phase >= 96 {
                v.energy_phase -= 96;
                if alive {
                    v.energy = (v.energy + d.energy.recharge_per_32ms).min(d.energy.maximum);
                    if v.jetting {
                        let next = v.energy - d.energy.drain_per_32ms;
                        if next < 0. {
                            v.jetting = false;
                            v.energy = 0.;
                        } else {
                            v.energy = next;
                        }
                    }
                }
            }
            let b = &world.bodies[v.body];
            let p = b.translation();
            let rot = *b.rotation();
            let velocity = v.velocity(d, b);
            v.previous_velocity = velocity;
            let forward = rot * (-Vec3::Z);
            let right = rot * Vec3::X;
            let up = rot * Vec3::Y;
            let speed = velocity.dot(forward);
            // Vehicle::updateMove (0x56b590): the move's yaw and pitch add to
            // the steering, clamped to the steering angle. The driver's move
            // carries the mouse turn, or, with strafe steering (the vehicle's
            // `steeringUseStrafeSteering` and the driver's
            // `$pref::Input::UseStrafeSteering`), +-steeringStrafeSteeringRate
            // per 32 ms tick for a held strafe key while the mouse only looks
            // around (Player::updateMove 0x5b2e89). FlyingVehicle damps the
            // steering below maxAutoSpeed.
            let strafe_mode = d.strafe_steering && !c.strafe_steering_off;
            let wheeled = matches!(d.family, Family::Wheeled | Family::Skis);
            let driver = matches!(
                driver_seat.map_or(SeatRole::Passenger, |seat| d.seat_role(seat)),
                SeatRole::MouseDriver | SeatRole::StrafeDriver
            );
            if driver {
                let limit = d.max_steering.max(0.01);
                let mut steering = v.mouse_steering;
                let turn = if strafe_mode {
                    let key = if c.strafe > 0. {
                        1.
                    } else if c.strafe < 0. {
                        -1.
                    } else {
                        0.
                    };
                    [key * d.steering.strafe_rate * 25. / 96., 0.]
                } else {
                    c.look_delta
                };
                if driven {
                    for (axis, turn) in steering.iter_mut().zip(turn) {
                        *axis = (*axis + turn).clamped(-limit, limit);
                    }
                } else {
                    steering = [0.; 2];
                }
                v.steering_quiet = if turn[0] != 0. {
                    0
                } else {
                    v.steering_quiet.saturating_add(1).min(AUTO_RETURN_QUIET)
                };
                if let Some(f) = &d.flight
                    && velocity.length() < f.max_auto_speed
                {
                    let damping = f.auto_input_damping.powf(25. / 96.);
                    steering.iter_mut().for_each(|x| *x *= damping);
                }
                // WheeledVehicle::updateMove (0x570c4a): with the driver's
                // `$pref::Input::UseAutoReturnSteering` and the vehicle's
                // `steeringUseAutoReturn`, a move with no yaw returns both
                // axes by rate x throttle share (`move->y`, so steering holds
                // with the throttle released). A v20 move spans 32 ms, so for
                // the mouse "no yaw" means none for that long: a mouse moving
                // at the frame rate leaves most 120 Hz moves without yaw, and
                // returning on each of those fought it. A held strafe key
                // turns every tick.
                let (rate, max) = (
                    d.steering.auto_return_rate,
                    d.steering.auto_return_max_speed,
                );
                if wheeled
                    && d.steering.auto_return
                    && !c.auto_return_off
                    && if strafe_mode {
                        turn[0] == 0.
                    } else {
                        v.steering_quiet >= AUTO_RETURN_QUIET
                    }
                    && max > 0.
                {
                    let share = c.throttle.abs().min(max) / max;
                    let keep = (1. - rate * share).max(0.).powf(25. / 96.);
                    steering.iter_mut().for_each(|x| *x *= keep);
                }
                v.mouse_steering = steering;
            }
            let (steer, pitch, roll) = if driver {
                let limit = d.max_steering.max(0.01);
                (
                    v.mouse_steering[0] / limit,
                    v.mouse_steering[1] / limit,
                    if strafe_mode { 0. } else { c.strafe },
                )
            } else {
                (c.steer, c.pitch, c.roll)
            };
            let aabb = world.colliders[v.collider].compute_aabb();
            let liquid = bri_content::water::submersion(
                waters,
                [p.x, aabb.mins.y, p.z],
                (aabb.maxs.y - aabb.mins.y).max(0.01),
            );
            let submerged = liquid.map_or(0., |(_, coverage)| coverage);
            // `ShapeBase::updateContainer`: from 10% coverage, buoyancy is the
            // density ratio and drag is `drag` x viscosity, both x coverage.
            let (buoyancy, drag) = match liquid {
                Some((water, coverage)) if coverage >= 0.1 => (
                    water.density / d.density.max(0.05) * coverage,
                    d.drag.max(0.) * water.viscosity * coverage,
                ),
                _ => (0., 0.),
            };
            v.water_coverage = submerged;
            let in_water = submerged > 0.05;
            if v.water != in_water {
                self.intents.push(Intent::Effect {
                    vehicle: *id,
                    id: if in_water {
                        "vehicleSplash"
                    } else {
                        "vehicleFoamEmitter"
                    }
                    .into(),
                    active: true,
                });
                v.water = in_water;
            }
            if d.is_actor() {
                if let Some(carry) = actor_step(
                    *id,
                    v,
                    d,
                    c,
                    driven,
                    waters,
                    world,
                    parts,
                    passages,
                    &mut self.intents,
                )? {
                    v.previous_velocity = carry.transform_vector3(v.previous_velocity);
                    v.relocations += 1;
                    carried.push((*id, carry));
                }
                v.jump_held = c.jump;
                if alive {
                    weapon_step(self.tick, *id, v, d, world, &mut self.intents);
                }
                continue;
            }
            let hover_distance = d.flight.as_ref().map(|f| {
                let desired = f.hover_height * v.spawn.scale;
                let range = 10. * v.spawn.scale + desired;
                world
                    .query_pipeline_with_filter(
                        QueryFilter::default()
                            .exclude_rigid_body(v.body)
                            .exclude_sensors(),
                    )
                    .cast_ray(&Ray::new(p, -Vec3::Y), range, true)
                    .map_or(range, |(_, distance)| distance)
            });
            // Wheel 0 on the ground at the last wheel update: a sled's
            // surfaces bite only then.
            let wheel0_contact = v.wheels.first().is_some_and(|w| w.contact);
            let hull_friction = if d.family == Family::Skis {
                hull_friction(world, v.body, velocity, d.friction, d.mass)
            } else {
                Vec3::ZERO
            };
            let gravity = VEHICLE_GRAVITY;
            let b = &mut world.bodies[v.body];
            b.reset_forces(false);
            b.reset_torques(false);
            b.add_force(hull_friction, true);
            if buoyancy > 0. || drag > 0. {
                // `WheeledVehicle::updateForces` and FlyingVehicle's: lift of
                // buoyancy x gravity x mass, and drag straight on the velocity,
                // not scaled by mass.
                b.add_force(
                    Vec3::Y * (buoyancy * gravity * d.mass) - velocity * drag,
                    true,
                );
                // Wheeled vehicles also take `torque -= angMomentum * mDrag`,
                // which decays spin at `drag` per second whatever the inertia.
                if matches!(d.family, Family::Wheeled | Family::Skis | Family::Ball) {
                    let spin = b.angvel() * (1. - drag * FIXED_DT).max(0.);
                    b.set_angvel(spin, true);
                }
            }
            if alive {
                match d.family {
                    // Skis are a WheeledVehicle with frictionless NothingTires:
                    // only Blockland's flying forces move and turn them.
                    Family::Wheeled | Family::Skis => {
                        v.steering = steer * d.max_steering;
                        if let Some(f) = &d.wheeled_flight {
                            // Speed along the nose, either way (0x575208).
                            let speed = speed.abs();
                            let mut force = Vec3::ZERO;
                            // Stock Torque jets push along the nose; v20 never
                            // passes a rider's jet or crouch to a vehicle.
                            if v.jetting {
                                force += forward * d.energy.jet_force;
                            }
                            // Thrust only below the speed limit for its way.
                            if c.throttle > 0. && speed < f.max_forward_vel {
                                force += forward * (c.throttle * d.thrust);
                            } else if c.throttle < 0. && speed < f.max_reverse_vel {
                                force += forward * (c.throttle * d.reverse_thrust);
                            }
                            // Lift along the roof, truncated to a whole number
                            // and capped whatever the pitch or stall.
                            force += up * (d.lift * speed).trunc().clamped(0., WHEELED_LIFT_CAP);
                            let bite = bite(f, speed);
                            // Squared mouse steering over maxSteeringAngle;
                            // a positive pitch (mouse up with v20's default
                            // vehicle mouse invert) dips the nose.
                            let (yaw, pitch) = (steer * steer.abs(), pitch * pitch.abs());
                            b.add_torque(
                                (-right * (pitch * d.pitch_force) - up * (yaw * d.yaw_force)
                                    + forward * (roll * d.roll_force))
                                    * bite,
                                true,
                            );
                            // Wings: sideways and roof-wise air is resisted
                            // with the square of speed once above stall. A
                            // sled's (skis) grip only with wheel 0 down.
                            if !f.sled || wheel0_contact {
                                let air = velocity.length() * bite;
                                force -= right
                                    * (right.dot(velocity) * air * f.horizontal_surface_force)
                                    + up * (up.dot(velocity) * air * f.vertical_surface_force);
                            }
                            b.add_force(force, true);
                            if velocity.length() > WHEELED_SPEED_CAP {
                                let capped = velocity.normalize() * (WHEELED_SPEED_CAP - 1.);
                                b.set_linvel(capped, true);
                            }
                        }
                    }
                    // FlyingVehicle::updateForces (blocklandv20.exe 0x568770,
                    // stock Torque): every force and torque is along the
                    // craft's own axes, so it climbs where its nose points.
                    Family::Flying => {
                        let f = d.flight.as_ref().expect("validated flight settings");
                        let speed = velocity.length();
                        let desired = f.hover_height * v.spawn.scale;
                        // getHeight (0x568420): height above hover height in
                        // tenths of the 10-unit ray, at most 1 with nothing
                        // below, so the roof jet holds 90% of the weight up high.
                        let normalized_height =
                            (hover_distance.unwrap() - desired) / (10. * v.spawn.scale);
                        let support = if normalized_height > 0. {
                            d.mass * gravity * (1. - normalized_height.min(1.) * 0.1)
                        } else {
                            d.mass * gravity - d.energy.jet_force * normalized_height
                        };
                        let mut force = up * support - velocity * f.min_drag;
                        if speed < f.max_auto_speed {
                            let auto = 1. - speed / f.max_auto_speed;
                            force -= forward * (f.auto_linear_force * auto * forward.dot(velocity))
                                + right * (f.auto_linear_force * auto * right.dot(velocity));
                            b.add_torque(
                                -right * (f.auto_angular_force * auto * forward.dot(Vec3::Y)),
                                true,
                            );
                        }
                        // Damping surfaces: straight on the sideways and roof
                        // velocity, not scaled by speed as the wheeled ones are.
                        force -= right * (right.dot(velocity) * f.horizontal_surface_force)
                            + up * (up.dot(velocity) * f.vertical_surface_force);
                        force += forward * (c.throttle * d.thrust) + right * (c.strafe * d.thrust);
                        if v.jetting {
                            force += if c.throttle > 0. {
                                forward * d.energy.jet_force
                            } else if c.throttle < 0. {
                                -forward * d.energy.jet_force
                            } else {
                                up * (d.energy.jet_force * f.vertical_thrust_multiple)
                            };
                        }
                        // FlyingVehicle::updateForces: squared steering; a
                        // positive pitch (mouse up with the default vehicle
                        // mouse invert) dips the nose.
                        let torque = -right * (pitch * pitch.abs() * f.steering_force)
                            - up * (steer * steer.abs() * f.steering_force)
                            + forward
                                * (steer * steer.abs() * f.steering_roll_force
                                    + f.auto_angular_force * right.dot(Vec3::Y)
                                    - d.roll_force * right.dot(velocity));
                        b.add_force(force, true);
                        b.add_torque(torque, true);
                    }
                    Family::Tumble | Family::Ball => {}
                    Family::Horse | Family::Rowboat | Family::Cannon | Family::Turret => {
                        unreachable!("player-type mounts step above")
                    }
                }
            }
            v.jump_held = c.jump;
            if !v.wheels.is_empty() {
                tires::update(
                    world,
                    v.body,
                    d,
                    v.spawn.scale,
                    &mut v.wheels,
                    tires::Drive {
                        steering: v.steering,
                        throttle: c.throttle,
                        braking: c.brake,
                        jetting: v.jetting,
                    },
                    FIXED_DT,
                );
            }
            if v.turret_damage
                .is_some_and(|damage| damage >= TURRET_MAX_DAMAGE)
                && let Some(collider) = v.turret_collider.take()
            {
                world.remove_collider(collider);
            }
            if let (Some(collider), Some(mount)) = (v.turret_collider, &d.attachment_mount) {
                let aim = d
                    .weapon_seat()
                    .map_or_else(Controls::default, |seat| v.controls[seat]);
                let mut p = local_pose(mount, v.spawn.scale);
                p.rotation *= Quat::from_rotation_y(aim.aim_yaw);
                world.colliders[collider].set_position_wrt_parent(p);
            }
            if alive {
                weapon_step(self.tick, *id, v, d, world, &mut self.intents);
            }
        }
        Ok(carried)
    }
    pub fn post_step(&mut self, world: &mut PhysicsWorld) -> Result<()> {
        ensure!(self.step_pending, "post_step needs pre_step");
        self.step_pending = false;
        self.tick = self.tick.checked_add(1).context("vehicle tick overflow")?;
        let mut removed = vec![];
        let mut wrecks = vec![];
        for (id, v) in &self.instances {
            let d = &self.catalog[&v.spawn.definition];
            if matches!(d.family, Family::Skis | Family::Tumble)
                && v.mounted_once
                && v.seats.iter().all(Option::is_none)
            {
                removed.push(*id);
                continue;
            }
            if d.family == Family::Tumble && !self.held.contains(id) {
                let age = self.tick - v.born;
                if let Some(ends) = v.ends {
                    if self.tick >= ends {
                        removed.push(*id);
                    }
                    continue;
                }
                if age >= 5400
                    || (age > 0
                        && age.is_multiple_of(240)
                        && (world.bodies[v.body].linvel().length() < 1. || v.water_coverage > 0.3))
                {
                    removed.push(*id);
                    continue;
                }
            }
            if let Some(dead) = v.dead_at {
                if self.tick >= dead + d.burn_ticks {
                    if let Some(projectile) = &d.final_explosion {
                        self.intents.push(Intent::Fire(explosion(
                            v,
                            d,
                            projectile,
                            world,
                            d.final_explosion_offset,
                        )));
                    }
                    if let (Some(_), Some(delay)) = (v.spawn.spawn_id, v.spawn.respawn_ticks) {
                        self.respawns.push(PendingRespawn {
                            tick: (dead + delay).max(self.tick + 12),
                            spawn: v.spawn.clone(),
                        });
                    }
                    removed.push(*id);
                }
            } else {
                let b = &world.bodies[v.body];
                if let Some(smash) = &d.smash {
                    for c in b.colliders() {
                        for p in world.contact_pairs_with(*c) {
                            let (other, outward) = if p.collider1 == *c {
                                (p.collider2, 1.)
                            } else {
                                (p.collider1, -1.)
                            };
                            let other_side = outward > 0.;
                            // How fast they closed: another vehicle's own
                            // motion before the step counts against it, so
                            // pushing one along is not striking it again.
                            let other_velocity = world
                                .colliders
                                .get(other)
                                .and_then(|c| c.parent())
                                .and_then(|body| {
                                    self.instances
                                        .values()
                                        .find(|o| o.body == body)
                                        .map(|o| o.previous_velocity)
                                })
                                .unwrap_or(Vec3::ZERO);
                            let closing = v.previous_velocity - other_velocity;
                            let hit = p.solver_manifolds().iter().find_map(|m| {
                                let speed = closing.dot(m.data.normal) * outward;
                                (m.data.num_active_contacts() > 0 && speed >= smash.speed).then(
                                    || {
                                        // The surface under the body's centre,
                                        // along the contact normal.
                                        let reach = (Vec3::from_array(d.bounds_max)
                                            - Vec3::from_array(d.bounds_min))
                                        .min_element()
                                            * 0.5
                                            * v.spawn.scale;
                                        let point =
                                            b.translation() + m.data.normal * outward * reach;
                                        let part =
                                            if other_side { m.subshape2 } else { m.subshape1 };
                                        (point, speed, part)
                                    },
                                )
                            });
                            if let Some((point, speed, part)) = hit
                                && let Some(collider) = world.colliders.get(other)
                            {
                                self.intents.push(Intent::Struck {
                                    vehicle: *id,
                                    owner: v.spawn.owner,
                                    other: collider.user_data,
                                    other_part: part,
                                    point: point.to_array(),
                                    speed,
                                    velocity: v.previous_velocity.to_array(),
                                });
                            }
                        }
                    }
                }
                let delta = (v.previous_velocity - v.velocity(d, b)).length();
                // Vehicle::updatePos (0x56ecb1): a body collision raises
                // `onImpact` and the impact sounds. v20 applies no damage
                // for it: `collDamageThresholdVel`/`collDamageMultiplier`
                // are only packed for the network (0x56a346), never read.
                // Player-type mounts are Armor and never get here.
                if !d.is_actor() {
                    let number = |key: &str, default: f32| {
                        d.authored
                            .get(key)
                            .and_then(|v| v.trim().parse::<f32>().ok())
                            .unwrap_or(default)
                    };
                    // Vehicle::resolveCollision: the body struck something,
                    // moving into it faster than `contactTol`. Resting
                    // contacts and the wheels never count.
                    let tolerance = number("contacttol", 0.1);
                    let collided = b.colliders().iter().any(|c| {
                        world.contact_pairs_with(*c).any(|p| {
                            let outward = if p.collider1 == *c { 1. } else { -1. };
                            p.solver_manifolds().iter().any(|m| {
                                m.data.num_active_contacts() > 0
                                    && v.previous_velocity.dot(m.data.normal) * outward > tolerance
                            })
                        })
                    });
                    if !collided {
                        continue;
                    }
                    // `onImpact` past `minImpactSpeed` (default 25, 0x569aac):
                    // only skiVehicle and deathVehicle script it, with a puff.
                    let projectile = match d.family {
                        Family::Skis => Some("v20.projectile.skiimpactaprojectile"),
                        Family::Tumble => Some("v20.projectile.tumbleimpactaprojectile"),
                        _ => None,
                    };
                    if let Some(projectile) = projectile
                        && delta > number("minimpactspeed", 25.)
                    {
                        let mut fire = explosion(v, d, projectile, world, 0.);
                        fire.velocity = [0.; 3];
                        self.intents.push(Intent::Fire(fire));
                    }
                    // The datablock's hard or soft impact sound by speed
                    // (defaults 50 and 25).
                    let sound = if delta >= number("hardimpactspeed", 50.) {
                        d.authored.get("hardimpactsound")
                    } else if delta >= number("softimpactspeed", 25.) {
                        d.authored.get("softimpactsound")
                    } else {
                        None
                    };
                    if let Some(sound) = sound.map(|s| s.trim()).filter(|s| !s.is_empty()) {
                        self.intents.push(Intent::Audio {
                            vehicle: *id,
                            id: sound.into(),
                        });
                    }
                    // `onWreck` (0x572348): the collision came with none of
                    // the first three wheels on the ground. skiVehicle's
                    // script throws its skier into a tumble.
                    let airborne = !v.wheels.is_empty()
                        && v.wheels.iter().take(WRECK_WHEELS).all(|w| !w.contact);
                    if d.family == Family::Skis && v.seats[0].is_some() && airborne {
                        wrecks.push(*id);
                    }
                }
            }
        }
        if self.prediction {
            removed.clear();
            wrecks.clear();
            self.respawns.clear();
        }
        for id in removed {
            self.remove(world, id)?;
        }
        for id in wrecks {
            self.wreck_skis(world, id)?;
        }
        let mut ready = vec![];
        self.respawns.retain(|p| {
            if p.tick <= self.tick {
                ready.push(p.spawn.clone());
                false
            } else {
                true
            }
        });
        for s in ready {
            self.intents.push(Intent::RespawnDue {
                scale: s.scale,
                spawn_id: s.spawn_id.unwrap(),
                definition: s.definition,
                owner: s.owner,
                transform: s.transform,
            });
        }
        Ok(())
    }
    pub fn snapshot(&self, world: &PhysicsWorld) -> Snapshot {
        Snapshot {
            schema_version: 2,
            tick: self.tick,
            vehicles: self
                .instances
                .iter()
                .map(|(id, v)| self.snapshot_of(world, *id, v))
                .collect(),
        }
    }
    /// One vehicle's snapshot: what [`VehiclesWorld::snapshot`] holds for
    /// it, without building every other vehicle's.
    pub fn vehicle_snapshot(&self, world: &PhysicsWorld, id: VehicleId) -> Option<VehicleSnapshot> {
        let v = self.instances.get(&id)?;
        Some(self.snapshot_of(world, id, v))
    }
    /// A vehicle's owner and whether it is destroyed, without a snapshot.
    pub fn owner_of(&self, id: VehicleId) -> Option<(OwnerId, bool)> {
        let v = self.instances.get(&id)?;
        Some((v.spawn.owner, v.dead_at.is_some()))
    }
    /// Every vehicle's id, owner and whether it is destroyed.
    pub fn owners(&self) -> impl Iterator<Item = (VehicleId, OwnerId, bool)> + '_ {
        self.instances
            .iter()
            .map(|(id, v)| (*id, v.spawn.owner, v.dead_at.is_some()))
    }
    fn snapshot_of(&self, world: &PhysicsWorld, id: VehicleId, v: &Instance) -> VehicleSnapshot {
        let d = &self.catalog[&v.spawn.definition];
        let b = &world.bodies[v.body];
        let weapon_control = d
            .seats
            .iter()
            .position(|s| s.weapon)
            .map(|i| v.controls[i])
            .unwrap_or_default();
        VehicleSnapshot {
            scale: v.spawn.scale,
            id,
            owner: v.spawn.owner,
            definition: d.id.clone(),
            transform: transform(b.position()),
            velocity: v.velocity(d, b).to_array(),
            angular_velocity: b.angvel().to_array(),
            damage: v.damage,
            destroyed: v.dead_at.is_some(),
            seats: d
                .seats
                .iter()
                .enumerate()
                .map(|(i, s)| SeatSnapshot {
                    index: i,
                    occupant: v.seats[i],
                    transform: effective_seat_pose(b, d, v, i),
                    pose: s.pose.clone(),
                })
                .collect(),
            wheel_suspension: v
                .wheels
                .iter()
                .zip(&d.wheels)
                .map(|(w, def)| w.extension * def.rest_length * v.spawn.scale)
                .collect(),
            wheel_rotation: v.wheels.iter().map(|w| w.rotation).collect(),
            wheel_contact: v.wheels.iter().map(|w| w.contact).collect(),
            wheel_tire: v.wheels.iter().map(|w| w.tire).collect(),
            actor: v.actor.as_ref().map(|a| a.state().clone()),
            steering: v.steering,
            mouse_steering: v.mouse_steering,
            steering_quiet: v.steering_quiet,
            animation: v.animation.clone(),
            charge: v.charge,
            energy: v.energy,
            jetting: v.jetting,
            turret_aim: [
                if d.is_actor() {
                    0.
                } else {
                    weapon_control.aim_yaw
                },
                weapon_control
                    .aim_pitch
                    .clamped(d.look_pitch[0], d.look_pitch[1]),
            ],
            turret_damage: v.turret_damage,
            turret_transform: d
                .attachment_mount
                .as_ref()
                .filter(|_| {
                    v.turret_damage
                        .is_none_or(|damage| damage < TURRET_MAX_DAMAGE)
                })
                .map(|t| transform(&(b.position() * local_pose(t, v.spawn.scale)))),
        }
    }
}
fn explosion(
    v: &Instance,
    _d: &Definition,
    projectile: &str,
    world: &PhysicsWorld,
    offset: f32,
) -> FireIntent {
    FireIntent {
        scale: v.spawn.scale,
        projectile: projectile.into(),
        vehicle: v.spawn.id,
        occupant: None,
        owner: v.last_damage,
        origin: (world.bodies[v.body].translation() + Vec3::Y * offset * v.spawn.scale).to_array(),
        direction: Vec3::Y.to_array(),
        velocity: Vec3::Y.to_array(),
    }
}
/// Player-type mounts (PlayerData: Horse, Rowboat, Pirate Cannon, Tank
/// Turret) move like players, per `Player::updateMove`: the rider's look turns
/// the mount, the move keys run it at its authored speeds and `runForce`, jump
/// uses `jumpForce`, and it steps up ledges and climbs its run surface angle.
/// Player-type mounts run the player motor: the rider's look turns the
/// mount, the move keys run it at its authored speeds and `runForce`, jump
/// uses `jumpForce`, and it steps, climbs, swims and floats like a player.
#[allow(clippy::too_many_arguments)]
fn actor_step(
    id: VehicleId,
    v: &mut Instance,
    d: &Definition,
    c: Controls,
    driven: bool,
    waters: &[bri_content::water::Water],
    world: &mut PhysicsWorld,
    parts: &dyn bri_motor::torque::PartTags,
    passages: &bri_content::passage::Passages,
    intents: &mut Vec<Intent>,
) -> Result<Option<glam::Affine3A>> {
    let actor = v.actor.as_mut().context("actor mount without a motor")?;
    let yaw = if driven {
        // mRot.z follows the rider's accumulated mouse turn.
        (c.aim_yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
    } else {
        actor.state().yaw
    };
    let (throttle, strafe) = if driven {
        (c.throttle.clamped(-1., 1.), c.strafe.clamped(-1., 1.))
    } else {
        (0., 0.)
    };
    let input = MoveInput {
        forward: throttle,
        right: strafe,
        yaw,
        pitch: 0.,
        head_yaw: 0.,
        jump: driven && c.jump,
        crouch: false,
        jet: false,
    };
    let motion = actor.step_through(world, input, waters, parts, passages)?;
    if motion.jumped && d.family == Family::Horse {
        intents.push(Intent::Audio {
            vehicle: id,
            id: "HorseJumpSound".into(),
        });
    }
    let state = actor.state();
    if d.family == Family::Horse && v.dead_at.is_none() {
        let animation = if !state.grounded && v.water_coverage < 0.9 {
            "jump"
        } else if throttle > 0.01 {
            "run"
        } else if throttle < -0.01 {
            "back"
        } else if strafe.abs() > 0.01 {
            "side"
        } else {
            "root"
        };
        if v.animation != animation {
            v.animation = animation.into();
            intents.push(Intent::Animation {
                vehicle: id,
                id: animation.into(),
            });
        }
    }
    Ok(motion.passed)
}
fn weapon_step(
    tick: u64,
    id: VehicleId,
    v: &mut Instance,
    d: &Definition,
    world: &mut PhysicsWorld,
    intents: &mut Vec<Intent>,
) {
    if !v.weapon_available(d) {
        return;
    }
    let Some(weapon) = &d.weapon else { return };
    let Some(index) = d.seats.iter().position(|s| s.weapon) else {
        return;
    };
    let Some(occupant) = v.seats[index] else {
        v.charge = 0;
        v.charge_started = None;
        return;
    };
    let mut c = v.controls[index];
    if d.is_actor() {
        c.aim_yaw = 0.;
    }
    c.aim_pitch = c.aim_pitch.clamped(d.look_pitch[0], d.look_pitch[1]);
    let ready = v
        .last_shot
        .is_none_or(|last| tick - last >= weapon.cooldown_ticks);
    let pressed = c.fire && !v.fire_held;
    v.fire_held = c.fire;
    let mut fire = false;
    if weapon.charge_ticks > 0 {
        if c.fire && ready && (pressed || v.charge_started.is_some()) {
            let start = *v.charge_started.get_or_insert(tick);
            let charge = (1 + (tick - start) / weapon.charge_ticks)
                .min(u64::from(weapon.charge_steps)) as u8;
            if charge != v.charge {
                intents.push(Intent::Charged {
                    vehicle: id,
                    owner: occupant.owner,
                    charge,
                    steps: weapon.charge_steps,
                });
            }
            v.charge = charge;
            if tick == start {
                intents.push(Intent::Effect {
                    vehicle: id,
                    id: "CannonFuseImage".into(),
                    active: true,
                });
            }
        } else if !c.fire && v.charge > 0 && ready {
            fire = true;
        }
    } else {
        fire = pressed && ready;
    }
    if fire {
        let b = &mut world.bodies[v.body];
        let Some((muzzle, aimed)) = d.muzzle([c.aim_yaw, c.aim_pitch]) else {
            return;
        };
        let direction = *b.rotation() * aimed;
        let origin = b.position().transform_point(muzzle * v.spawn.scale);
        let speed = weapon.speed * f32::from(v.charge.max(1)) * v.spawn.scale;
        intents.push(Intent::Fire(FireIntent {
            scale: v.spawn.scale,
            projectile: weapon.projectile.clone(),
            vehicle: id,
            occupant: Some(occupant.id),
            owner: occupant.owner,
            origin: origin.to_array(),
            direction: direction.to_array(),
            velocity: (direction * speed).to_array(),
        }));
        // A pack may leave a weapon's sound or effect out; clients refuse a
        // cue that names none.
        if !weapon.sound.is_empty() {
            intents.push(Intent::Audio {
                vehicle: id,
                id: weapon.sound.clone(),
            });
        }
        if !weapon.effect.is_empty() {
            intents.push(Intent::Effect {
                vehicle: id,
                id: weapon.effect.clone(),
                active: true,
            });
        }
        intents.push(Intent::Animation {
            vehicle: id,
            id: "activate".into(),
        });
        if d.family != Family::Cannon {
            b.apply_impulse(-(direction + Vec3::Y) * d.mass * 5., true);
        }
        v.last_shot = Some(tick);
        v.charge = 0;
        v.charge_started = None;
        if weapon.charge_ticks > 0 {
            intents.push(Intent::Effect {
                vehicle: id,
                id: "CannonFuseImage".into(),
                active: false,
            });
        }
    }
}

/// A vehicle's body as it collides, at `scale`: its shape and where that
/// sits in the vehicle's frame. Everything that stands for the body in a
/// physics world (the host's, a client's cosmetic debris) uses this one.
pub fn body_shape(d: &Definition, scale: f32) -> Result<(SharedShape, Pose)> {
    let mut parts = vec![];
    for hull in &d.collision_hulls {
        let points: Vec<_> = hull.iter().map(|p| Vec3::from_array(*p) * scale).collect();
        parts.push((
            Pose::IDENTITY,
            SharedShape::convex_hull(&points).context("degenerate vehicle hull")?,
        ));
    }
    let mut offset = Pose::IDENTITY;
    let shape = if d.family == Family::Skis {
        // The box around skivehicle.dts's collision hulls (the main hull is
        // a box with a slightly tapered, not quite flat base). Rapier gave
        // those near-degenerate faces sideways contact normals that kicked
        // sliding skis into spins; the box slides true.
        let (min, max) = d
            .collision_hulls
            .iter()
            .flatten()
            .fold((Vec3::MAX, Vec3::MIN), |(lo, hi), p| {
                (lo.min(Vec3::from_array(*p)), hi.max(Vec3::from_array(*p)))
            });
        offset = Pose::from_translation((min + max) * 0.5 * scale);
        let half = (max - min) * 0.5 * scale;
        SharedShape::cuboid(half.x, half.y, half.z)
    } else if d.family == Family::Ball {
        let min = Vec3::from_array(d.bounds_min);
        let max = Vec3::from_array(d.bounds_max);
        SharedShape::ball((max - min).max_element() * 0.5 * scale)
    } else if d.is_actor() {
        // A player's box, as the character controller sweeps it.
        let (min, max) = d
            .collision_hulls
            .iter()
            .flatten()
            .fold((Vec3::MAX, Vec3::MIN), |(lo, hi), p| {
                (lo.min(Vec3::from_array(*p)), hi.max(Vec3::from_array(*p)))
            });
        offset = Pose::from_translation((min + max) * 0.5 * scale);
        let half = (max - min) * 0.5 * scale;
        SharedShape::cuboid(half.x, half.y, half.z)
    } else {
        SharedShape::compound(parts)
    };
    Ok((shape, offset))
}

fn prepare_spawn(
    s: &Spawn,
    d: &Definition,
    world_gravity: f32,
) -> Result<(RigidBodyBuilder, ColliderBuilder, Option<ColliderBuilder>)> {
    ensure!(
        s.scale.is_finite() && (0.2..=5.).contains(&s.scale),
        "vehicle scale outside [0.2,5]"
    );
    ensure!(
        s.respawn_ticks.is_none_or(|t| t <= 120 * 86400),
        "respawn duration too large"
    );
    ensure!(
        s.transform
            .position
            .iter()
            .chain(s.transform.rotation.iter())
            .all(|x| x.is_finite()),
        "invalid spawn transform"
    );
    ensure!(
        (Quat::from_array(s.transform.rotation).length_squared() - 1.).abs() < 0.001,
        "spawn rotation must be normalized"
    );
    ensure!(
        s.transform.position.iter().all(|x| x.abs() < 1e7),
        "spawn position outside native bounds"
    );
    let (shape, offset) = body_shape(d, s.scale)?;
    let mut builder = if d.is_actor() {
        RigidBodyBuilder::kinematic_position_based()
    } else {
        RigidBodyBuilder::dynamic()
    }
    .pose(pose(&s.transform))
    .gravity_scale(if world_gravity > 0. {
        VEHICLE_GRAVITY / world_gravity
    } else {
        0.
    })
    .linear_damping(match d.family {
        Family::Flying => 0.,
        // WheeledVehicle::updateForces, every wheeled vehicle: the
        // datablock's `drag` on velocity (not scaled by mass), and
        // `rotationalDrag` plus that drag on angular momentum.
        Family::Wheeled | Family::Skis => d.drag / d.mass.max(0.01),
        _ => d.drag * 0.05,
    })
    .angular_damping(if matches!(d.family, Family::Wheeled | Family::Skis) {
        d.angular_drag + d.drag
    } else {
        d.angular_drag
    })
    .ccd_enabled(true);
    if d.is_actor() {
        builder = builder.can_sleep(false);
    }
    let prepared_turret = if let Some(mount) = &d.attachment_mount {
        let mut parts = vec![];
        for hull in &d.attachment_collision_hulls {
            let points: Vec<_> = hull
                .iter()
                .map(|p| Vec3::from_array(*p) * s.scale)
                .collect();
            parts.push((
                Pose::IDENTITY,
                SharedShape::convex_hull(&points).context("degenerate turret hull")?,
            ));
        }
        Some(
            ColliderBuilder::new(SharedShape::compound(parts))
                .position(local_pose(mount, s.scale))
                .mass(0.)
                .friction(d.friction),
        )
    } else {
        None
    };

    let mut collider = ColliderBuilder::new(shape)
        .position(offset)
        .mass_properties(mass_properties(d, s.scale))
        .friction(d.friction)
        .restitution(d.restitution);
    if d.family == Family::Skis {
        // The skis' body friction is applied by `hull_friction` at the
        // centre of mass, so the solver's own contact friction is off.
        collider = collider
            .friction(0.)
            .friction_combine_rule(CoefficientCombineRule::Min);
    }
    Ok((builder, collider, prepared_turret))
}

/// The skis ride on their body: four `skiSpring`s (195 each) cannot hold up
/// 90 kg at 20 m/s², so the hull slides on the ground with `bodyFriction`
/// (Torque's `Vehicle::resolveContacts`). Rapier's friction at the hull's
/// off-centre contact points spun sliding skis round at speed, so the
/// friction is applied here instead: Coulomb friction from last step's
/// normal impulses, at the centre of mass, never reversing the slide.
fn hull_friction(
    world: &PhysicsWorld,
    body: RigidBodyHandle,
    velocity: Vec3,
    friction: f32,
    mass: f32,
) -> Vec3 {
    let mut force = Vec3::ZERO;
    let mut slide = Vec3::ZERO;
    for c in world.bodies[body].colliders() {
        for p in world.contact_pairs_with(*c) {
            for m in p.solver_manifolds() {
                let n = m.data.normal;
                let tangent = velocity - n * velocity.dot(n);
                if tangent.length_squared() > 1e-8 {
                    let impulse: f32 = m.points.iter().map(|pt| pt.data.impulse).sum();
                    force -= tangent.normalize() * (friction * impulse / FIXED_DT);
                    slide = tangent;
                }
            }
        }
    }
    // Friction can stop the slide within a step but not push it backwards.
    force.clamp_length_max(slide.length() * mass / FIXED_DT)
}
/// Torque rigid bodies: authored mass at `massCenter` with the inertia of a
/// solid box, rather than a uniform-density collision hull.
fn mass_properties(d: &Definition, scale: f32) -> MassProperties {
    let size = Vec3::from_array(d.inertia_box) * scale;
    let squared = size * size;
    MassProperties::new(
        Vec3::from_array(d.mass_center) * scale,
        d.mass,
        Vec3::new(
            squared.y + squared.z,
            squared.x + squared.z,
            squared.x + squared.y,
        ) * (d.mass / 12.),
    )
}
/// Torque `Player::checkDismountPoint`: the rider's own box, feet at the
/// exit point, must be empty, and the way there from the seat unblocked.
fn exit_clear(queries: &QueryPipeline, start: Vec3, offset: Vec3, body: [f32; 2]) -> bool {
    let [width, height] = body;
    let shape = Cuboid::new(Vec3::new(width * 0.5, height * 0.5, width * 0.5));
    let centre = start + Vec3::Y * (height * 0.5);
    let dst = centre + offset;
    let clear = queries
        .intersect_shape(Pose::translation(dst.x, dst.y, dst.z), &shape)
        .next()
        .is_none();
    clear
        && queries
            .cast_shape(
                &Pose::translation(centre.x, centre.y, centre.z),
                offset,
                &shape,
                ShapeCastOptions {
                    max_time_of_impact: 1.,
                    stop_at_penetration: false,
                    ..Default::default()
                },
            )
            .is_none()
}
#[cfg(test)]
mod scale_tests {
    use super::*;
    #[test]
    fn wheel_geometry_scales_with_collision_and_mass_stays_authored() {
        wheel_geometry_scales(crate::testing::pack(), crate::testing::CAR);
    }
    #[test]
    #[ignore = "requires generated v20 content"]
    fn native_wheel_geometry_scales_with_collision_and_mass_stays_authored() {
        let pack = Pack::load(
            bri_package::testing::pack_dir(
                &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"),
                "vehicles",
            )
            .join("vehicles.json"),
        )
        .unwrap();
        wheel_geometry_scales(pack, "v20.vehicle.jeepvehicle");
    }
    fn wheel_geometry_scales(pack: Pack, definition: &str) {
        let mut vehicles = VehiclesWorld::new(pack).unwrap();
        let mut world = bri_physics::new_world();
        for (id, scale) in [(1, 1.), (2, 2.)] {
            vehicles
                .spawn(
                    &mut world,
                    Spawn {
                        id: VehicleId(id),
                        owner: OwnerId(1),
                        definition: definition.into(),
                        transform: Transform {
                            position: [id as f32 * 20., 30., 0.],
                            ..Default::default()
                        },
                        spawn_id: None,
                        respawn_ticks: None,
                        scale,
                    },
                )
                .unwrap();
        }
        let shown = vehicles.snapshot(&world).vehicles;
        for (a, b) in shown[0]
            .wheel_suspension
            .iter()
            .zip(&shown[1].wheel_suspension)
        {
            assert_eq!(*b, a * 2.);
        }
        let a = &vehicles.instances[&VehicleId(1)];
        let b = &vehicles.instances[&VehicleId(2)];
        assert_eq!(world.bodies[a.body].mass(), world.bodies[b.body].mass());
    }
}
#[cfg(test)]
mod dismount_tests {
    use super::*;
    #[test]
    fn exit_clearance_tests_the_riders_own_box_at_their_feet() {
        let mut world = bri_physics::new_world();
        // A low roof over the exit point: its underside 2 units above the seat.
        world.insert(
            RigidBodyBuilder::fixed().translation(Vec3::new(3., 2.5, 0.)),
            ColliderBuilder::cuboid(1., 0.5, 1.),
        );
        world.step();
        let queries = world.query_pipeline_with_filter(QueryFilter::default());
        let seat = Vec3::new(0., 0.1, 0.);
        let exit = Vec3::X * 3.;
        // A standing player (2.65 tall) would end up inside the roof.
        assert!(!exit_clear(&queries, seat, exit, [1.25, 2.65]));
        // A rider short enough to fit under it gets out.
        assert!(exit_clear(&queries, seat, exit, [1.25, 1.5]));
        // Open ground elsewhere is clear for the full body.
        assert!(exit_clear(&queries, seat, -exit, [1.25, 2.65]));
    }
}
