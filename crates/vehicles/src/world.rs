use crate::{FIXED_DT, schema::*};
use anyhow::{Context, Result, ensure};
use glam::{Quat, Vec3};
use rapier3d::parry::query::ShapeCastOptions;
use rapier3d::{
    control::{
        CharacterAutostep, CharacterLength, DynamicRayCastVehicleController,
        KinematicCharacterController, WheelTuning,
    },
    prelude::*,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
mod checkpoint;
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
    #[serde(default)]
    pub look_delta: [f32; 2],
}
/// Velocity change on contact that wrecks skis (`hardImpactSpeed` is 10;
/// ordinary bumps only raise ski impact puffs).
const SKI_WRECK_SPEED: f32 = 20.;
/// Torque's player step height (`maxStepHeight`) for player-type mounts.
const ACTOR_STEP_HEIGHT: f32 = 1.0;
/// Steering Auto-Return (on by default in v20): released mouse steering
/// halves every quarter second.
const STEERING_RETURN_PER_TICK: f32 = 0.977_15;
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
    pub steering: f32,
    pub animation: String,
    pub charge: u8,
    pub energy: f32,
    pub jetting: bool,
    pub turret_aim: [f32; 2],
    pub turret_damage: Option<f32>,
    pub turret_transform: Option<Transform>,
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
    controller: Option<DynamicRayCastVehicleController>,
    seats: Vec<Option<Occupant>>,
    controls: Vec<Controls>,
    damage: f32,
    born: u64,
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
    restored_suspension: Option<Vec<f32>>,
    restored_contacts: Option<Vec<bool>>,
    /// Torque `mSteering`: accumulated mouse steering (yaw, pitch), radians.
    mouse_steering: [f32; 2],
    /// Player-type mounts move kinematically with their own velocity.
    motion: Vec3,
    grounded: bool,
}
impl Instance {
    fn velocity(&self, d: &Definition, b: &RigidBody) -> Vec3 {
        if d.is_actor() {
            self.motion
        } else {
            b.linvel()
        }
    }
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
fn effective_seat_pose(b: &RigidBody, d: &Definition, v: &Instance, index: usize) -> Transform {
    if index == 2
        && v.turret_damage.is_some_and(|damage| damage >= 250.)
        && let Some(t) = &d.attachment_fallback_seat
    {
        return transform(&(b.position() * local_pose(t, v.spawn.scale)));
    }
    let base = &d.seats[index];
    if index == 2 && d.attachment_mount.is_some() {
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
        })
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
    /// Where an occupant sits, if mounted.
    pub fn occupant(&self, occupant: OccupantId) -> Option<(VehicleId, usize)> {
        self.occupied.get(&occupant).copied()
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
        let controller = if d.wheels.is_empty() {
            None
        } else {
            let c = build_controller(body, d, s.scale);
            Some(c)
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
                controller,
                seats: vec![None; count],
                controls: vec![Controls::default(); count],
                damage: 0.,
                born: self.tick,
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
                restored_suspension: None,
                restored_contacts: None,
                mouse_steering: [0.; 2],
                motion: Vec3::ZERO,
                grounded: false,
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
        v.controls[seat] = Controls::default();
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
    /// Safely rejects obstructed exits; returns original mounted state on failure.
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
        if matches!(d.family, Family::Skis | Family::Tumble) {
            v.seats[seat] = None;
            v.controls[seat] = Controls::default();
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
        let capsule = Capsule::new_y(0.55, 0.3);
        let queries = world.query_pipeline_with_filter(
            QueryFilter::default()
                .exclude_rigid_body(v.body)
                .exclude_sensors(),
        );
        let offsets = [
            Vec3::Y * 2.2,
            Vec3::Y * 3.,
            -Vec3::Y * 3.,
            Vec3::X * 3.,
            -Vec3::X * 3.,
        ];
        let exit = offsets.into_iter().find_map(|offset| {
            let offset = offset * v.spawn.scale;
            let dst = start + offset;
            let p = Pose::translation(dst.x, dst.y, dst.z);
            let clear = queries.intersect_shape(p, &capsule).next().is_none();
            let sweep = queries.cast_shape(
                &Pose::translation(start.x, start.y, start.z),
                offset,
                &capsule,
                ShapeCastOptions {
                    max_time_of_impact: 1.,
                    stop_at_penetration: false,
                    ..Default::default()
                },
            );
            (clear && sweep.is_none()).then_some((dst, offset))
        });
        ensure!(exit.is_some() || forced, "all dismount positions blocked");
        let (p, impulse) = exit.unwrap_or((start, Vec3::ZERO));
        let velocity = body_velocity + b.angvel().cross(p - b.translation()) + impulse;
        v.seats[seat] = None;
        v.controls[seat] = Controls::default();
        v.charge = 0;
        v.charge_started = None;
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
        if d.is_actor() {
            // Player::applyImpulse: velocity changes by impulse / mass.
            v.motion += impulse / d.mass;
            v.grounded &= impulse.y <= 0.;
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
    pub fn set_velocity(
        &mut self,
        world: &mut PhysicsWorld,
        id: VehicleId,
        velocity: [f32; 3],
    ) -> Result<()> {
        let velocity = Vec3::from_array(velocity);
        ensure!(velocity.is_finite(), "invalid velocity");
        let v = self.instances.get_mut(&id).context("unknown vehicle")?;
        if self.catalog[&v.spawn.definition].is_actor() {
            v.motion = velocity;
            v.grounded &= velocity.y <= 0.;
        } else {
            world.bodies[v.body].set_linvel(velocity, true);
        }
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
            let ticks = ((((speed - 10.) / 50.) * 7. + 1.).clamp(1., 7.) * 120.).round() as u64;
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
            Family::Wheeled
                | Family::FlyingWheeled
                | Family::Flying
                | Family::Ball
                | Family::Skis
                | Family::Tumble
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
            v.controller = None;
            self.intents.push(Intent::Destroyed { vehicle: id, by });
            let initial = if v.turret_damage.is_some_and(|damage| damage < 250.) {
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
                *damage = 250.;
            }
            self.intents.push(Intent::Effect {
                vehicle: id,
                id: "VehicleBurnEmitter".into(),
                active: true,
            });
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
        if *damage >= 250. || v.dead_at.is_some() {
            return Ok(());
        }
        *damage = (*damage + amount).min(250.);
        if *damage >= 250. {
            if let Some(collider) = v.turret_collider.take() {
                world.remove_collider(collider);
            }
            v.last_damage = by;
            v.charge = 0;
            v.charge_started = None;
            v.controls[2] = Controls::default();
            let d = &self.catalog[&v.spawn.definition];
            self.intents.push(Intent::Fire(explosion(
                v,
                d,
                "v20.projectile.tankturretexplosionprojectile",
                world,
                d.initial_explosion_offset,
            )));
            if let Some(o) = v.seats[2] {
                let fallback = d.attachment_fallback_seat.as_ref().unwrap();
                self.intents.push(Intent::Mounted {
                    vehicle: id,
                    occupant: o,
                    seat: 2,
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
        let driver = v.seats.first().copied().flatten();
        let authored = |value: f32, default: f32| {
            if value > 0. && value < 1e30 {
                value
            } else {
                default
            }
        };
        let minimum =
            authored(d.runover_speed, 2.).clamp(2., 999.) + if driver.is_none() { 2. } else { 0. };
        let speed = velocity.length();
        self.intents.push(Intent::RunOver {
            vehicle: id,
            owner: v
                .seats
                .iter()
                .flatten()
                .next()
                .map_or(v.spawn.owner, |o| o.owner),
            target,
            damage: if speed > minimum {
                speed * authored(d.runover_damage, 5.)
            } else {
                0.
            },
            velocity: (velocity * authored(d.runover_push, 1.2)).to_array(),
        });
        Ok(())
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
    pub fn pre_step(
        &mut self,
        world: &mut PhysicsWorld,
        water_height: impl Fn([f32; 3]) -> Option<f32>,
    ) -> Result<()> {
        ensure!(
            (world.integration_parameters.dt - FIXED_DT).abs() < 1e-6,
            "vehicles require shared 120Hz timestep"
        );
        ensure!(!self.step_pending, "pre_step already called");
        self.step_pending = true;
        for (id, v) in &mut self.instances {
            let d = &self.catalog[&v.spawn.definition];
            let control = v.controls.first().copied().unwrap_or_default();
            let alive = v.dead_at.is_none();
            let driven = alive && v.seats.first().is_some_and(Option::is_some);
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
            // Vehicle::updateMove: mouse steering accumulates, clamped to the
            // steering angle. FlyingVehicle damps it below maxAutoSpeed.
            if d.seat_role(0) == SeatRole::MouseDriver {
                let limit = d.max_steering.max(0.01);
                let mut steering = v.mouse_steering;
                if driven {
                    for (axis, turn) in steering.iter_mut().zip(c.look_delta) {
                        *axis = (*axis + turn).clamp(-limit, limit);
                    }
                } else {
                    steering = [0.; 2];
                }
                if let Some(f) = &d.flight
                    && velocity.length() < f.max_auto_speed
                {
                    let damping = f.auto_input_damping.powf(25. / 96.);
                    steering.iter_mut().for_each(|x| *x *= damping);
                }
                for (axis, turn) in steering.iter_mut().zip(c.look_delta) {
                    if turn == 0. {
                        *axis *= STEERING_RETURN_PER_TICK;
                    }
                }
                v.mouse_steering = steering;
            }
            let (steer, pitch, roll) = if d.seat_role(0) == SeatRole::MouseDriver {
                let limit = d.max_steering.max(0.01);
                (
                    v.mouse_steering[0] / limit,
                    v.mouse_steering[1] / limit,
                    c.strafe,
                )
            } else {
                (c.steer, c.pitch, c.roll)
            };
            let water = water_height(p.to_array());
            let aabb = world.colliders[v.collider].compute_aabb();
            let submerged = water.filter(|h| h.is_finite()).map_or(0., |h| {
                ((h - aabb.mins.y) / (aabb.maxs.y - aabb.mins.y).max(0.01)).clamp(0., 1.)
            });
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
                actor_step(*id, v, d, c, driven, submerged, world, &mut self.intents);
                v.jump_held = c.jump;
                if alive {
                    weapon_step(self.tick, *id, v, d, world, &mut self.intents);
                }
                continue;
            }
            let ground = world
                .query_pipeline_with_filter(
                    QueryFilter::default()
                        .exclude_rigid_body(v.body)
                        .exclude_sensors(),
                )
                .cast_ray_and_get_normal(&Ray::new(p + Vec3::Y * 0.1, -Vec3::Y), 0.3, true)
                .is_some_and(|(_, hit)| {
                    hit.normal.y
                        >= if d.run_surface_angle > 0. {
                            d.run_surface_angle.to_radians().cos()
                        } else {
                            0.2
                        }
                })
                || v.controller
                    .as_ref()
                    .is_some_and(|c| c.wheels().iter().any(|w| w.raycast_info().is_in_contact));
            let ground = ground
                || v.restored_contacts
                    .as_ref()
                    .is_some_and(|contacts| contacts.iter().any(|x| *x));
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
            let gravity = VEHICLE_GRAVITY;
            let b = &mut world.bodies[v.body];
            b.reset_forces(false);
            b.reset_torques(false);
            if submerged > 0. {
                b.add_force(
                    Vec3::Y * (d.mass * gravity * submerged / d.density.max(0.05))
                        - velocity * (d.mass * submerged * 1.5),
                    true,
                );
            }
            if alive {
                match d.family {
                    Family::Wheeled | Family::FlyingWheeled => {
                        let target = steer * d.max_steering;
                        if d.strafe_steering {
                            // steeringStrafeSteeringRate: 0.1 radian per 32 ms
                            // tick, returning to center when released.
                            let rate = if steer == 0. { 0.9 } else { 3.125 };
                            v.steering +=
                                (target - v.steering).clamp(-rate * FIXED_DT, rate * FIXED_DT);
                        } else {
                            v.steering = target;
                        }
                        if d.family == Family::FlyingWheeled {
                            let force = if c.throttle >= 0. {
                                d.thrust
                            } else {
                                d.reverse_thrust
                            };
                            b.add_force(
                                forward * (force * c.throttle * (1. - speed.abs() / 40.).max(0.))
                                    + up * (d.lift * speed.abs())
                                    + Vec3::Y * (if v.jetting { d.energy.jet_force } else { 0. }),
                                true,
                            );
                            b.add_torque(
                                -up * (steer * d.yaw_force) - right * (pitch * d.pitch_force)
                                    + forward * (roll * d.roll_force),
                                true,
                            );
                        }
                    }
                    Family::Flying => {
                        let f = d.flight.as_ref().expect("validated flight settings");
                        let speed = velocity.length();
                        let desired = f.hover_height * v.spawn.scale;
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
                        force -= right * (speed * right.dot(velocity) * f.horizontal_surface_force)
                            + up * (speed * up.dot(velocity) * f.vertical_surface_force);
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
                    Family::Skis => {
                        v.steering = steer * d.max_steering;
                        if ground || in_water {
                            b.add_force(
                                forward * (d.thrust * c.throttle)
                                    - right * velocity.dot(right) * 50.,
                                true,
                            );
                            if c.brake {
                                let horizontal = Vec3::new(velocity.x, 0., velocity.z);
                                b.apply_impulse(
                                    -horizontal.clamp_length_max(d.brake_force / d.mass * FIXED_DT)
                                        * d.mass,
                                    true,
                                );
                            }
                        }
                        b.add_torque(
                            -up * (steer * d.yaw_force) - right * (pitch * d.pitch_force)
                                + forward * (roll * d.roll_force),
                            true,
                        );
                    }
                    Family::Tumble | Family::Ball => {}
                    Family::Horse | Family::Rowboat | Family::Cannon | Family::Turret => {
                        unreachable!("player-type mounts step above")
                    }
                }
            }
            v.jump_held = c.jump;
            if let Some(controller) = &mut v.controller {
                let wheel_count = d.wheels.iter().filter(|w| w.powered).count().max(1) as f32;
                for (w, def) in controller.wheels_mut().iter_mut().zip(&d.wheels) {
                    // Positive steering turns right (clockwise from above); Rapier
                    // turns the wheel counterclockwise about the chassis up axis.
                    w.steering = -v.steering * def.steering;
                    w.engine_force = if def.powered {
                        c.throttle * d.engine_force / wheel_count
                            * (1. - speed.abs() / d.max_speed).max(0.)
                    } else {
                        0.
                    };
                    w.brake = if c.brake {
                        d.brake_force / wheel_count * FIXED_DT
                    } else if c.throttle.abs() < 0.001 {
                        d.engine_brake / wheel_count * FIXED_DT
                    } else {
                        0.
                    };
                }
                let q = world.broad_phase.as_query_pipeline_mut(
                    world.narrow_phase.query_dispatcher(),
                    &mut world.bodies,
                    &mut world.colliders,
                    QueryFilter::default()
                        .exclude_rigid_body(v.body)
                        .exclude_sensors(),
                );
                controller.update_vehicle(FIXED_DT, q);
                v.restored_suspension = None;
                v.restored_contacts = None;
            }
            if v.turret_damage.is_some_and(|damage| damage >= 250.)
                && let Some(collider) = v.turret_collider.take()
            {
                world.remove_collider(collider);
            }
            if let (Some(collider), Some(mount)) = (v.turret_collider, &d.attachment_mount) {
                let aim = v.controls.get(2).copied().unwrap_or_default();
                let mut p = local_pose(mount, v.spawn.scale);
                p.rotation *= Quat::from_rotation_y(aim.aim_yaw);
                world.colliders[collider].set_position_wrt_parent(p);
            }
            if alive {
                weapon_step(self.tick, *id, v, d, world, &mut self.intents);
            }
        }
        Ok(())
    }
    pub fn post_step(&mut self, world: &mut PhysicsWorld) -> Result<()> {
        ensure!(self.step_pending, "post_step needs pre_step");
        self.step_pending = false;
        self.tick = self.tick.checked_add(1).context("vehicle tick overflow")?;
        let mut impacts = vec![];
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
            if d.family == Family::Tumble {
                let age = self.tick - v.born;
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
                let delta = (v.previous_velocity - v.velocity(d, b)).length();
                let contacting = b.colliders().iter().any(|c| {
                    world
                        .contact_pairs_with(*c)
                        .any(|p| p.has_any_active_contact())
                });
                if contacting
                    && delta > d.impact_threshold
                    && matches!(d.family, Family::Skis | Family::Tumble)
                {
                    let projectile = if d.family == Family::Skis {
                        "v20.projectile.skiimpactaprojectile"
                    } else {
                        "v20.projectile.tumbleimpactaprojectile"
                    };
                    let mut fire = explosion(v, d, projectile, world, 0.);
                    fire.velocity = [0.; 3];
                    self.intents.push(Intent::Fire(fire));
                }
                // skiVehicle::onWreck: a hard landing or ending up on its
                // side throws the skier into a tumble.
                if d.family == Family::Skis
                    && v.seats[0].is_some()
                    && contacting
                    && (delta >= SKI_WRECK_SPEED || (*b.rotation() * Vec3::Y).y < 0.25)
                {
                    wrecks.push(*id);
                    continue;
                }
                if contacting && delta > d.impact_threshold {
                    impacts.push((
                        *id,
                        (delta - d.impact_threshold) * d.impact_damage,
                        v.last_damage,
                    ));
                    if delta > 15. {
                        self.intents.push(Intent::Audio {
                            vehicle: *id,
                            id: "fastImpactSound".into(),
                        });
                    }
                }
            }
        }
        for (id, damage, owner) in impacts {
            self.damage(world, id, damage, owner)?;
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
                .map(|(id, v)| {
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
                        id: *id,
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
                        wheel_suspension: v.restored_suspension.clone().unwrap_or_else(|| {
                            v.controller.as_ref().map_or_else(Vec::new, |c| {
                                c.wheels()
                                    .iter()
                                    .map(|w| w.raycast_info().suspension_length)
                                    .collect()
                            })
                        }),
                        wheel_rotation: v.controller.as_ref().map_or_else(Vec::new, |c| {
                            c.wheels().iter().map(|w| w.rotation).collect()
                        }),
                        steering: v.steering,
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
                                .clamp(d.look_pitch[0], d.look_pitch[1]),
                        ],
                        turret_damage: v.turret_damage,
                        turret_transform: d
                            .attachment_mount
                            .as_ref()
                            .filter(|_| v.turret_damage.is_none_or(|damage| damage < 250.))
                            .map(|t| transform(&(b.position() * local_pose(t, v.spawn.scale)))),
                    }
                })
                .collect(),
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
#[allow(clippy::too_many_arguments)]
fn actor_step(
    id: VehicleId,
    v: &mut Instance,
    d: &Definition,
    c: Controls,
    driven: bool,
    submerged: f32,
    world: &mut PhysicsWorld,
    intents: &mut Vec<Intent>,
) {
    let dt = FIXED_DT;
    let scale = v.spawn.scale;
    let b = &world.bodies[v.body];
    let position = b.translation();
    let mut rotation = *b.rotation();
    if driven {
        // mRot.z follows the rider's accumulated mouse turn.
        rotation = Quat::from_rotation_y(-c.aim_yaw);
    }
    let forward = rotation * -Vec3::Z;
    let right = rotation * Vec3::X;
    let swimming = if d.family == Family::Rowboat {
        submerged > 0.05
    } else {
        submerged >= 0.9
    };
    let [fs, bs, ss] = if swimming {
        d.underwater_speeds
    } else {
        [d.max_speed, d.reverse_speed, d.max_side_speed]
    };
    let (throttle, strafe) = if driven {
        (c.throttle, c.strafe)
    } else {
        (0., 0.)
    };
    let move_vec = forward * throttle + right * strafe;
    let move_speed = if throttle > 0. {
        fs * throttle
    } else {
        bs * -throttle
    }
    .max(ss * strafe.abs());
    let desired = move_vec.normalize_or_zero() * move_speed * scale;
    let mut velocity = v.motion;
    let horizontal = Vec3::new(velocity.x, 0., velocity.z);
    let grounded = v.grounded;
    let horizontal = if grounded || swimming {
        horizontal + (desired - horizontal).clamp_length_max(d.engine_force / d.mass * dt)
    } else {
        horizontal
    };
    velocity.x = horizontal.x;
    velocity.z = horizontal.z;
    let jumped = driven && grounded && c.jump && !v.jump_held && d.jump_speed > 0.;
    if jumped {
        velocity.y = d.jump_speed;
        intents.push(Intent::Audio {
            vehicle: id,
            id: "HorseJumpSound".into(),
        });
    }
    if !grounded || jumped {
        velocity.y -= VEHICLE_GRAVITY * dt;
    } else {
        velocity.y = velocity.y.min(0.);
    }
    if submerged > 0. {
        // Buoyancy against water of density 1, with the body's density.
        velocity.y += submerged / d.density.max(0.05) * VEHICLE_GRAVITY * dt;
        velocity *= (1. - submerged * 1.5 * dt).max(0.);
    }
    velocity *= (1. - d.drag * dt).max(0.);
    velocity.y = velocity.y.max(-80.);
    let collider = &world.colliders[v.collider];
    let shape = collider.shared_shape().clone();
    let local = *collider.position_wrt_parent().unwrap_or(&Pose::IDENTITY);
    let pose = Pose::from_parts(position, rotation) * local;
    let walkable = d.run_surface_angle.clamp(1., 89.).to_radians();
    let rising = velocity.y > 0.;
    let controller = KinematicCharacterController {
        offset: CharacterLength::Absolute(0.01),
        autostep: grounded.then_some(CharacterAutostep {
            max_height: CharacterLength::Absolute(ACTOR_STEP_HEIGHT * scale),
            min_width: CharacterLength::Absolute(0.1),
            include_dynamic_bodies: false,
        }),
        max_slope_climb_angle: walkable,
        min_slope_slide_angle: walkable,
        snap_to_ground: (!rising).then_some(CharacterLength::Absolute(0.2)),
        ..Default::default()
    };
    let query = world.query_pipeline_with_filter(
        QueryFilter::default()
            .exclude_rigid_body(v.body)
            .exclude_sensors(),
    );
    let mut normals = Vec::new();
    let motion = controller.move_shape(dt, &query, shape.as_ref(), &pose, velocity * dt, |hit| {
        normals.push(hit.hit.normal1)
    });
    let intended = velocity * dt;
    let moved = motion.translation;
    let climbed = moved.y > intended.y.max(0.) + 0.01;
    for normal in normals {
        let into = velocity.dot(normal);
        if into >= 0. {
            continue;
        }
        let across = -Vec3::new(normal.x, 0., normal.z).normalize_or_zero();
        let expected = intended.dot(across);
        if normal.y < walkable.cos()
            && climbed
            && expected > 0.
            && moved.dot(across) >= expected * 0.5
        {
            continue;
        }
        velocity -= normal * into;
    }
    v.grounded = motion.grounded && !rising;
    if v.grounded {
        velocity.y = 0.;
    }
    v.motion = velocity;
    world.bodies[v.body].set_next_kinematic_position(Pose::from_parts(position + moved, rotation));
    if d.family == Family::Horse && v.dead_at.is_none() {
        let animation = if !v.grounded && !swimming {
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
}
fn weapon_step(
    tick: u64,
    id: VehicleId,
    v: &mut Instance,
    d: &Definition,
    world: &mut PhysicsWorld,
    intents: &mut Vec<Intent>,
) {
    if v.turret_damage.is_some_and(|damage| damage >= 250.) {
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
    c.aim_pitch = c.aim_pitch.clamp(d.look_pitch[0], d.look_pitch[1]);
    let ready = v
        .last_shot
        .is_none_or(|last| tick - last >= weapon.cooldown_ticks);
    let pressed = c.fire && !v.fire_held;
    v.fire_held = c.fire;
    let mut fire = false;
    if weapon.charge_ticks > 0 {
        if c.fire && ready && (pressed || v.charge_started.is_some()) {
            let start = *v.charge_started.get_or_insert(tick);
            v.charge = (1 + (tick - start) / weapon.charge_ticks)
                .min(u64::from(weapon.charge_steps)) as u8;
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
        let aiming = Quat::from_rotation_y(c.aim_yaw) * Quat::from_rotation_x(c.aim_pitch);
        let direction = *b.rotation() * aiming * (-Vec3::Z);
        let pivot = Vec3::from_array(weapon.pivot);
        let origin = b.position().transform_point(
            (pivot + aiming * (Vec3::from_array(weapon.muzzle.position) - pivot)) * v.spawn.scale,
        );
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
        intents.push(Intent::Audio {
            vehicle: id,
            id: weapon.sound.clone(),
        });
        intents.push(Intent::Effect {
            vehicle: id,
            id: weapon.effect.clone(),
            active: true,
        });
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
    let mut parts = vec![];
    for hull in &d.collision_hulls {
        let points: Vec<_> = hull
            .iter()
            .map(|p| Vec3::from_array(*p) * s.scale)
            .collect();
        parts.push((
            Pose::IDENTITY,
            SharedShape::convex_hull(&points).context("degenerate vehicle hull")?,
        ));
    }
    let mut offset = Pose::IDENTITY;
    let shape = if d.family == Family::Ball {
        let min = Vec3::from_array(d.bounds_min);
        let max = Vec3::from_array(d.bounds_max);
        SharedShape::ball((max - min).max_element() * 0.5 * s.scale)
    } else if d.is_actor() {
        // A player's box, as the character controller sweeps it.
        let (min, max) = d
            .collision_hulls
            .iter()
            .flatten()
            .fold((Vec3::MAX, Vec3::MIN), |(lo, hi), p| {
                (lo.min(Vec3::from_array(*p)), hi.max(Vec3::from_array(*p)))
            });
        offset = Pose::from_translation((min + max) * 0.5 * s.scale);
        let half = (max - min) * 0.5 * s.scale;
        SharedShape::cuboid(half.x, half.y, half.z)
    } else {
        SharedShape::compound(parts)
    };
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
    .linear_damping(if d.family == Family::Flying {
        0.
    } else {
        d.drag * 0.05
    })
    .angular_damping(d.angular_drag)
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

    Ok((
        builder,
        ColliderBuilder::new(shape)
            .position(offset)
            .mass_properties(mass_properties(d, s.scale))
            .friction(d.friction)
            .restitution(d.restitution),
        prepared_turret,
    ))
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
fn build_controller(
    body: RigidBodyHandle,
    d: &Definition,
    scale: f32,
) -> DynamicRayCastVehicleController {
    let mut c = DynamicRayCastVehicleController::new(body);
    c.index_up_axis = 1;
    c.index_forward_axis = 2;
    for w in &d.wheels {
        // Torque springs push `force * (1 - extension)` and damp
        // `damping * velocity / length`; Rapier takes both per unit chassis mass.
        let length = w.rest_length * scale;
        let tuning = WheelTuning {
            suspension_stiffness: w.spring / length / d.mass,
            suspension_compression: w.damping / length / d.mass,
            suspension_damping: w.damping / length / d.mass,
            max_suspension_travel: length,
            side_friction_stiffness: 1.,
            friction_slip: w.friction,
            max_suspension_force: f32::MAX,
        };
        c.add_wheel(
            Vec3::from_array(w.position) * scale,
            -Vec3::Y,
            Vec3::X,
            w.rest_length * scale,
            w.radius * scale,
            &tuning,
        );
    }

    c
}
#[cfg(test)]
mod scale_tests {
    use super::*;
    #[test]
    fn native_wheel_geometry_scales_with_collision_and_mass_stays_authored() {
        let pack = Pack::load(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../content/vehicles-pack-010/vehicles.json"
        ))
        .unwrap();
        let mut vehicles = VehiclesWorld::new(pack).unwrap();
        let mut world = bri_physics::new_world();
        for (id, scale) in [(1, 1.), (2, 2.)] {
            vehicles
                .spawn(
                    &mut world,
                    Spawn {
                        id: VehicleId(id),
                        owner: OwnerId(1),
                        definition: "v20.vehicle.jeepvehicle".into(),
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
        let a = &vehicles.instances[&VehicleId(1)];
        let b = &vehicles.instances[&VehicleId(2)];
        for (a, b) in a
            .controller
            .as_ref()
            .unwrap()
            .wheels()
            .iter()
            .zip(b.controller.as_ref().unwrap().wheels())
        {
            assert_eq!(b.radius, a.radius * 2.);
            assert_eq!(
                b.chassis_connection_point_cs,
                a.chassis_connection_point_cs * 2.
            );
            assert_eq!(b.suspension_rest_length, a.suspension_rest_length * 2.);
        }
        assert_eq!(world.bodies[a.body].mass(), world.bodies[b.body].mass());
    }
}
