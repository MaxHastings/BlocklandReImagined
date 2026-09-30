//! Vehicle spawn bricks, mounting, driving, weapons and destruction.
//!
//! `bri-vehicles` simulates the bodies in the shared physics world; this
//! adapter owns which brick spawned which vehicle, who sits where, how player
//! inputs become vehicle controls, and what its intents do to the world
//! (projectiles, player placement, damage, respawns and presentation cues).
use super::*;
use bri_minigames::{self as mg, Decision};
use bri_vehicles::{self as veh, Intent, OccupantId, SpawnId, VehicleId, schema::SeatRole};
use bri_weapons::ActorId;
use rapier3d::prelude::*;

/// `$Game::MinMountTime`: a player cannot remount right after leaving.
const MIN_MOUNT_TICKS: u64 = 120;
/// Families that are not placed on spawn bricks (item/state vehicles).
const INTERNAL_FAMILIES: [veh::Family; 2] = [veh::Family::Skis, veh::Family::Tumble];
/// `WheeledVehicleData::onCollision`/`Armor::onCollision`: a player mounts
/// only from above, feet this far over the mount's origin.
const MOUNT_ABOVE: f32 = 0.2;
/// Collider tag namespace for vehicles (players use 1 << 64).
pub(super) const VEHICLE_TAG: u128 = 2 << 64;

#[derive(Default)]
pub(super) struct Vehicles {
    pub(super) world: Option<veh::VehiclesWorld>,
    by_brick: BTreeMap<BrickId, VehicleId>,
    brick_of: BTreeMap<VehicleId, BrickId>,
    colors: BTreeMap<VehicleId, Option<u8>>,
    next_id: u64,
    mounted: BTreeMap<OwnerId, Mount>,
    last_dismount: BTreeMap<OwnerId, u64>,
    /// Jet held last input: a new press leaves the vehicle.
    jet_held: BTreeMap<OwnerId, bool>,
    fire_held: BTreeMap<OwnerId, bool>,
    /// Look angles last fed to the vehicle, for mouse steering deltas.
    last_look: BTreeMap<OwnerId, (f32, f32)>,
    /// Players whose `$pref::Input::UseStrafeSteering` and
    /// `UseAutoReturnSteering` (`SteeringPrefsEvent`) differ from the
    /// client's shipped [`DEFAULT_STEERING`]; everyone else has those.
    pub(super) steering: BTreeMap<OwnerId, (bool, bool)>,
    /// Each passenger's body turn on their seat (`mRot.z`, which a mounted
    /// player's transform turns the mount node by): their move's yaw.
    passenger_turn: BTreeMap<OwnerId, f32>,
    /// The world yaw a rider's moves carried when they mounted: until their
    /// client knows it is seated and sends the turn instead, that yaw is not
    /// a turn.
    mount_yaw: BTreeMap<OwnerId, f32>,
    /// Skis spawned by the ski item wait to be boarded (`schedule(250, mountObject)`).
    pending_skis: Vec<(OwnerId, VehicleId, u64)>,
    /// Players riding a tumble vehicle, watched through the corpse camera.
    tumbling: BTreeSet<OwnerId>,
    /// Player/vehicle pairs in contact last tick, so run-overs fire on contact.
    touching: BTreeSet<(OwnerId, VehicleId)>,
    scanned: bool,
}
/// Queue length past which a seated player's backlog drains fast (a stall).
const SEATED_FLOOD: usize = 30;
/// Ticks a seated player's queue is watched before a lasting excess drains.
const SEATED_WINDOW: u64 = 240;
/// Moves a seated player's queue keeps beyond the worst jitter it has shown.
const SEATED_SPARE: usize = 2;

/// How many of a seated player's queued moves the host runs this tick.
///
/// Their vehicle simulates every tick with or without a move, and their
/// client predicts it one step per move (`Predictor::drive_pose` replays
/// the moves after `driver_input` one a tick), so the host runs one a tick:
/// a late move repeats the last and leaves the queue one longer afterwards,
/// which then absorbs jitter of that size. Draining a backlog in bursts
/// (three moves in one step) and starving again kept the client correcting
/// its view. Only a queue that stayed longer than a small spare for two
/// whole seconds (a burst, or the client's clock running a little fast)
/// drains, one extra move at a time; a stall's backlog drains fast.
#[derive(Clone, Debug, Default)]
pub struct SeatedPace {
    low: Option<usize>,
    ticks: u64,
}
impl SeatedPace {
    /// The moves to run this tick with `queued` waiting.
    pub fn runs(&mut self, queued: usize) -> usize {
        if queued > SEATED_FLOOD {
            *self = Self::default();
            return 3;
        }
        let low = self.low.map_or(queued, |low| low.min(queued));
        self.ticks += 1;
        if self.ticks < SEATED_WINDOW {
            self.low = Some(low);
            return 1;
        }
        *self = Self::default();
        if low > SEATED_SPARE { 2 } else { 1 }
    }
}
#[derive(Debug, Clone)]
struct Mount {
    vehicle: VehicleId,
    seat: usize,
}

/// `$pref::Input::UseStrafeSteering` and `UseAutoReturnSteering` as the
/// client ships them (the reference install's, both off; stock v20 had
/// both on). The host assumes them until a player's `SteeringPrefsEvent`
/// says otherwise, so a driver whose prefs have not arrived is still
/// steered as their client predicts.
pub const DEFAULT_STEERING: (bool, bool) = (false, false);

/// Replicated vehicle identity and occupancy (reliable, on change).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VehicleInfo {
    pub id: u64,
    pub definition: String,
    /// Palette index when the spawn brick recolors the vehicle.
    pub color: Option<u8>,
    pub occupants: Vec<Option<OwnerId>>,
    pub destroyed: bool,
    /// The spawn's uniform scale: a driving client predicts the vehicle at it.
    pub scale: f32,
}
/// Replicated vehicle motion (unreliable datagrams, 40 Hz).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VehiclePose {
    pub id: u64,
    pub tick: u64,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub velocity: [f32; 3],
    pub steering: f32,
    pub wheel_suspension: Vec<f32>,
    pub wheel_rotation: Vec<f32>,
    /// Wheels on the ground, for the client's tire emitters.
    pub wheel_contact: Vec<bool>,
    /// Each wheel's spin and tyre stretch, which a driving client predicts from.
    pub wheel_tire: Vec<veh::TireState>,
    pub turret_aim: [f32; 2],
    pub jetting: bool,
    /// The body's spin (rad/s) and the driver's accumulated mouse steering:
    /// with the rest, all a driving client needs to predict its vehicle.
    pub angular_velocity: [f32; 3],
    pub mouse_steering: [f32; 2],
    /// The newest move of the driver's this pose includes (their
    /// `acknowledged_input`), 0 with no driver: the driver's client replays
    /// its later moves from here.
    pub driver_input: u64,
    /// The driver's `UseStrafeSteering` and `UseAutoReturnSteering` as the
    /// host steers their moves ([`DEFAULT_STEERING`] with no driver): their
    /// client predicts with these, never its own copy, so the two agree.
    pub driver_steering: (bool, bool),
    /// Ticks since the driver's move last turned, for auto-return.
    pub steering_quiet: u8,
    /// A player-type mount's motor state (horse, rowboat, cannon, turret),
    /// which its rider's client predicts from; `None` for other vehicles.
    pub actor: Option<crate::player::PlayerState>,
}
impl VehiclePose {
    /// The motion a predicting client restores its vehicle to.
    pub fn motion(&self) -> veh::Motion {
        veh::Motion {
            transform: bri_vehicles::Transform {
                position: self.position,
                rotation: self.rotation,
            },
            velocity: self.velocity,
            angular_velocity: self.angular_velocity,
            mouse_steering: self.mouse_steering,
            steering: self.steering,
            steering_quiet: self.steering_quiet,
            wheel_suspension: self.wheel_suspension.clone(),
            wheel_rotation: self.wheel_rotation.clone(),
            wheel_contact: self.wheel_contact.clone(),
            wheel_tire: self.wheel_tire.clone(),
            actor: self.actor.clone(),
        }
    }
}
/// A driver's move as their vehicle's controls: the keys, and the mouse turn
/// since their previous move (`last`: its yaw and pitch), which a
/// mouse-steered vehicle accumulates. The host and a client predicting its
/// own vehicle both use it, so they steer alike.
pub fn driver_controls(
    input: &MoveInput,
    last: (f32, f32),
    fire: bool,
    (strafe_off, auto_return_off): (bool, bool),
) -> veh::Controls {
    veh::Controls {
        throttle: input.forward,
        brake: input.jump,
        fire,
        strafe: input.right,
        look_delta: [
            wrap(input.yaw - last.0),
            wrap_half(input.pitch - last.1),
        ],
        strafe_steering_off: strafe_off,
        auto_return_off,
        ..Default::default()
    }
}

/// The move of the rider controlling a player-type mount (horse, rowboat,
/// cannon, turret) as its controls: the mount walks by the keys and faces
/// where the rider looks; a horse jumps with jump, and nothing brakes. The
/// host and the rider's predicting client both use it.
pub fn actor_controls(input: &MoveInput, fire: bool, horse: bool) -> veh::Controls {
    veh::Controls {
        throttle: input.forward,
        fire,
        strafe: input.right,
        jump: horse && input.jump,
        aim_yaw: input.yaw,
        aim_pitch: input.pitch,
        ..Default::default()
    }
}

/// Heading (yaw, positive right) of a native rotation's forward axis.
pub(super) fn heading(rotation: [f32; 4]) -> f32 {
    let forward = glam::Quat::from_array(rotation) * Vec3::NEG_Z;
    forward.x.atan2(-forward.z)
}
fn wrap(angle: f32) -> f32 {
    (angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}
/// Mouse-steering pitch wraps with a half-turn period (see `Controls`).
fn wrap_half(angle: f32) -> f32 {
    (angle + std::f32::consts::FRAC_PI_2).rem_euclid(std::f32::consts::PI)
        - std::f32::consts::FRAC_PI_2
}
/// `bottomprintCannonStrength`: "Fire! :" and a bar of 20 `|`, yellow for
/// the power so far (two per step of ten) and black for the rest.
pub(super) fn cannon_strength(charge: u8, steps: u8) -> String {
    let lit = (usize::from(charge) * 20 / usize::from(steps.max(1))).min(20);
    format!(
        "<just:center><color:FF0000>Fire! <color:FFFFFF>:<color:FFFF00>{}<color:000000>{}",
        "|".repeat(lit),
        "|".repeat(20 - lit)
    )
}
fn occupant(peers: &BTreeMap<OwnerId, Peer>, owner: OwnerId) -> veh::Occupant {
    let tuning = peers
        .get(&owner)
        .map_or_else(bri_motor::player::PlayerTuning::default, |p| {
            p.player.tuning().clone()
        });
    veh::Occupant {
        id: OccupantId(owner),
        owner: veh::OwnerId(owner),
        body: [tuning.width, tuning.stand_height],
    }
}

impl Vehicles {
    pub(super) fn is_mounted(&self, owner: OwnerId) -> bool {
        self.mounted.contains_key(&owner)
    }
    /// The player drives a mouse-steered vehicle, so their move's yaw and
    /// pitch are its steering, not where their body looks.
    pub(super) fn mouse_steers(&self, owner: OwnerId) -> bool {
        let Some(mount) = self.mounted.get(&owner) else {
            return false;
        };
        let (strafe, _) = self.steering(owner);
        self.world
            .as_ref()
            .and_then(|w| w.definition_of(mount.vehicle))
            .is_some_and(|d| d.seat_role_for(mount.seat, strafe) == SeatRole::MouseDriver)
    }
    /// A player's `UseStrafeSteering` and `UseAutoReturnSteering`.
    pub(super) fn steering(&self, owner: OwnerId) -> (bool, bool) {
        self.steering.get(&owner).copied().unwrap_or(DEFAULT_STEERING)
    }
    /// `$Game::MinMountTime` has passed since this player last left a mount.
    pub(super) fn may_remount(&self, owner: OwnerId, tick: u64) -> bool {
        self.last_dismount
            .get(&owner)
            .is_none_or(|t| tick.saturating_sub(*t) >= MIN_MOUNT_TICKS)
    }
    /// `Armor::onUnMount` sets `lastMountTime`.
    pub(super) fn note_dismount(&mut self, owner: OwnerId, tick: u64) {
        self.last_dismount.insert(owner, tick);
    }
    /// The family of the vehicle a player rides.
    pub(super) fn mounted_family(&self, owner: OwnerId) -> Option<veh::Family> {
        let mount = self.mounted.get(&owner)?;
        Some(self.world.as_ref()?.definition_of(mount.vehicle)?.family)
    }
    /// Seated where fire shoots the mount's gun instead of tools
    /// (`armor::onTrigger` for TankTurretPlayer and CannonTurret).
    pub(super) fn weapon_seat(&self, owner: OwnerId) -> bool {
        let Some(mount) = self.mounted.get(&owner) else {
            return false;
        };
        self.world
            .as_ref()
            .and_then(|w| w.definition_of(mount.vehicle))
            .and_then(|d| d.seats.get(mount.seat))
            .is_some_and(|s| s.weapon)
    }
    /// A gunner's fire button drives the vehicle weapon, not items.
    pub(super) fn set_fire(&mut self, owner: OwnerId, down: bool) {
        self.fire_held.insert(owner, down);
    }
}
pub(super) fn combat_input_burst() -> f32 {
    48.0
}
impl Session {
    /// Install native vehicle definitions before clients connect.
    pub fn set_vehicle_pack(&mut self, pack: veh::Pack) -> Result<()> {
        ensure!(
            self.peers.is_empty() && self.departed.is_empty(),
            "Cannot replace live vehicle definitions"
        );
        self.vehicles = Vehicles {
            world: Some(veh::VehiclesWorld::new(pack)?),
            ..Default::default()
        };
        self.refresh_event_bindings()
    }
    /// Vehicles a spawn brick may hold (the wrench's Vehicle list).
    pub fn vehicle_choices(&self) -> Vec<(String, String)> {
        self.vehicles
            .world
            .as_ref()
            .map(|w| {
                w.definitions()
                    .filter(|d| !INTERNAL_FAMILIES.contains(&d.family))
                    .map(|d| (d.id.clone(), d.name.trim().to_string()))
                    .chain(Self::bot_choices())
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn vehicle_infos(&self) -> Vec<VehicleInfo> {
        let Some(world) = &self.vehicles.world else {
            return Vec::new();
        };
        world
            .snapshot(&self.simulation.physics)
            .vehicles
            .into_iter()
            .map(|v| VehicleInfo {
                id: v.id.0,
                definition: v.definition,
                color: self.vehicles.colors.get(&v.id).copied().flatten(),
                occupants: v
                    .seats
                    .iter()
                    .map(|s| s.occupant.map(|o| o.owner.0))
                    .collect(),
                destroyed: v.destroyed,
                scale: v.scale,
            })
            .collect()
    }
    pub fn vehicle_poses(&self) -> Vec<VehiclePose> {
        let Some(world) = &self.vehicles.world else {
            return Vec::new();
        };
        let tick = self.simulation.state().tick;
        world
            .snapshot(&self.simulation.physics)
            .vehicles
            .into_iter()
            .map(|v| VehiclePose {
                driver_input: v
                    .seats
                    .first()
                    .and_then(|s| s.occupant)
                    .and_then(|o| self.peers.get(&o.owner.0))
                    .map_or(0, |p| p.processed_move),
                driver_steering: v
                    .seats
                    .first()
                    .and_then(|s| s.occupant)
                    .map_or(DEFAULT_STEERING, |o| self.vehicles.steering(o.owner.0)),
                id: v.id.0,
                tick,
                // Where it is drawn: a player-type mount between its ticks.
                position: v.shown_transform().position,
                rotation: v.transform.rotation,
                velocity: v.velocity,
                steering: v.steering,
                wheel_suspension: v.wheel_suspension,
                wheel_rotation: v.wheel_rotation,
                wheel_contact: v.wheel_contact,
                wheel_tire: v.wheel_tire,
                turret_aim: v.turret_aim,
                jetting: v.jetting,
                angular_velocity: v.angular_velocity,
                mouse_steering: v.mouse_steering,
                steering_quiet: v.steering_quiet,
                actor: v.actor,
            })
            .collect()
    }
    /// The vehicle and seat a player occupies.
    pub fn mounted(&self, owner: OwnerId) -> Option<(u64, u8)> {
        self.vehicles
            .mounted
            .get(&owner)
            .map(|m| (m.vehicle.0, m.seat as u8))
    }

    /// Admin teleports of a rider (`dropPlayerAtCamera`, `/fetch`, `/find`)
    /// move the root mount instead and stop it. Returns where
    /// `Vehicle::teleportEffect` plays and its scale: the scale times the
    /// world box height over 2.65 (so the vehicle's scale counts twice).
    pub(super) fn teleport_mount(
        &mut self,
        owner: OwnerId,
        position: Vec3,
        rotation: glam::Quat,
    ) -> Result<Option<(Vec3, f32)>> {
        if let Some(ride) = self.ride(owner) {
            // A player mount is the root: it moves and its riders follow.
            if let Some((at, scale)) = self.teleport_mount(ride.mount, position, rotation)? {
                return Ok(Some((at, scale)));
            }
            let forward = rotation * Vec3::NEG_Z;
            let peer = self.peers.get_mut(&ride.mount).context("Unknown mount")?;
            let scale = peer.player.state().scale;
            peer.player.teleport(
                &mut self.simulation.physics,
                position,
                forward.x.atan2(-forward.z),
            )?;
            peer.inputs.clear();
            self.follow_player_mounts();
            return Ok(Some((position, scale)));
        }
        let Some(mount) = self.vehicles.mounted.get(&owner) else {
            return Ok(None);
        };
        let vehicle = mount.vehicle;
        let world = self.vehicles.world.as_mut().context("No vehicle world")?;
        let definition = world.definition_of(vehicle).context("Unknown vehicle")?;
        let height = definition.bounds_max[1] - definition.bounds_min[1];
        world.set_transform(
            &mut self.simulation.physics,
            vehicle,
            &veh::Transform {
                position: position.to_array(),
                rotation: rotation.normalize().to_array(),
            },
        )?;
        let scale = world
            .vehicle_snapshot(&self.simulation.physics, vehicle)
            .map_or(1.0, |v| v.scale);
        self.follow_seats()?;
        Ok(Some((position, scale * scale * height / 2.65)))
    }

    /// Spawn transform above a vehicle spawn brick, facing the brick's front.
    fn spawn_transform(&self, brick: &Brick, definition: &veh::Definition) -> veh::Transform {
        let yaw = -f32::from(brick.quarter_turns) * std::f32::consts::FRAC_PI_2;
        let top = brick.position[1] + 0.3;
        // Lift the body so its lowest wheel or hull point clears the brick.
        let clearance = definition
            .wheels
            .iter()
            .map(|w| w.radius + w.rest_length - w.position[1])
            .fold(-definition.bounds_min[1], f32::max)
            .max(0.0);
        veh::Transform {
            position: [brick.position[0], top + clearance + 0.1, brick.position[2]],
            rotation: glam::Quat::from_rotation_y(yaw).to_array(),
        }
    }
    fn spawn_vehicle_for(&mut self, brick_id: BrickId) -> Result<()> {
        if self
            .simulation
            .state()
            .bricks
            .get(&brick_id)
            .and_then(|b| b.vehicle.as_ref())
            .is_some_and(|v| {
                matches!(&v.vehicle, bri_world::ContentRef::Resolved(id) if super::bots::is_bot_kind(id))
            })
        {
            return Ok(());
        }
        let Some(brick) = self.simulation.state().bricks.get(&brick_id).cloned() else {
            return Ok(());
        };
        let Some(spawn) = &brick.vehicle else {
            return Ok(());
        };
        let bri_world::ContentRef::Resolved(definition) = &spawn.vehicle else {
            return Ok(());
        };
        if self
            .vehicles
            .world
            .as_ref()
            .is_none_or(|w| w.definition(definition).is_none())
        {
            return Ok(());
        }
        if let Err(text) = self.vehicle_room(brick.owner, definition) {
            self.notify(brick.owner, Notice::Center { text, seconds: 2.0 });
            return Ok(());
        }
        let def = self
            .vehicles
            .world
            .as_ref()
            .and_then(|w| w.definition(definition))
            .context("checked above")?;
        let transform = self.spawn_transform(&brick, def);
        let game = self.owner_game(brick.owner);
        let respawn_ms = game
            .and_then(|g| self.minigames.game(g).ok())
            .map_or(0, |g| g.settings.vehicle_respawn_ms);
        self.vehicles.next_id += 1;
        let id = VehicleId(self.vehicles.next_id);
        let spawn = veh::Spawn {
            id,
            owner: veh::OwnerId(brick.owner),
            definition: definition.clone(),
            transform,
            spawn_id: Some(SpawnId(brick_id)),
            respawn_ticks: Some(mg::ticks_for_ms(respawn_ms.max(1000))),
            scale: 1.0,
        };
        self.vehicles
            .world
            .as_mut()
            .unwrap()
            .spawn(&mut self.simulation.physics, spawn)?;
        self.tag_vehicle(id);
        self.vehicles.by_brick.insert(brick_id, id);
        self.vehicles.brick_of.insert(id, brick_id);
        self.vehicles.colors.insert(id, spawn_color(&brick));
        Ok(())
    }
    /// Whether `owner` may have one more vehicle of `definition`, or the
    /// line they are told: `fxDTSBrick::spawnVehicle`'s server totals,
    /// player-type mounts (horses, boats, cannons, turrets) apart from
    /// physics vehicles (`$Pref::Server::MaxPlayerVehicles_Total` and
    /// `MaxPhysVehicles_Total`), and before them an Internet server's
    /// per-builder quota (`$Pref::Server::Quota::Vehicle` and
    /// `Quota::Player`). Add-On vehicles count like any other.
    pub(super) fn vehicle_room(&self, owner: OwnerId, definition: &str) -> Result<(), String> {
        let Some(world) = &self.vehicles.world else {
            return Err("\u{E000}This server has no vehicles".into());
        };
        let Some(def) = world.definition(definition) else {
            return Err(format!("\u{E000}Unknown vehicle {definition}"));
        };
        let actor = def.is_actor();
        let settings = &self.admin.settings;
        let (limit, noun) = if actor {
            (settings.player_vehicles, "player-vehicle")
        } else {
            (settings.physics_vehicles, "physics-vehicle")
        };
        let snapshot = world.snapshot(&self.simulation.physics);
        let same_kind: Vec<_> = snapshot
            .vehicles
            .iter()
            .filter(|v| {
                world
                    .definition(&v.definition)
                    .is_some_and(|d| d.is_actor() == actor)
            })
            .collect();
        let quota = if actor {
            settings.per_player.players
        } else {
            settings.per_player.vehicles
        };
        let owned = same_kind
            .iter()
            .filter(|v| v.owner == veh::OwnerId(owner))
            .count();
        if !self.lan_host && owned >= quota as usize {
            return Err(if quota == 1 {
                format!("\u{E000}You already have a {noun}")
            } else {
                format!("\u{E000}You already have {quota} {noun}s")
            });
        }
        if same_kind.len() >= limit as usize {
            return Err(if limit == 1 {
                format!("\u{E000}Server is limited to 1 {noun}")
            } else {
                format!("\u{E000}Server is limited to {limit} {noun}s")
            });
        }
        Ok(())
    }
    fn tag_vehicle(&mut self, id: VehicleId) {
        if let Some(world) = &self.vehicles.world {
            for collider in world.colliders_of(&self.simulation.physics, id) {
                self.simulation.physics.colliders[collider].user_data =
                    VEHICLE_TAG | u128::from(id.0);
            }
        }
    }
    pub(super) fn remove_vehicle(&mut self, id: VehicleId) -> Result<()> {
        if let Some(world) = &mut self.vehicles.world {
            if let Some(spawn) = self.vehicles.brick_of.get(&id) {
                world.cancel_spawn(&mut self.simulation.physics, SpawnId(*spawn))?;
            } else {
                let _ = world.remove(&mut self.simulation.physics, id);
            }
        }
        self.forget_vehicle(id);
        Ok(())
    }
    fn forget_vehicle(&mut self, id: VehicleId) {
        if let Some(brick) = self.vehicles.brick_of.remove(&id)
            && self.vehicles.by_brick.get(&brick) == Some(&id)
        {
            self.vehicles.by_brick.remove(&brick);
        }
        self.vehicles.colors.remove(&id);
    }
    /// Keep spawned vehicles in step with their spawn bricks.
    fn reconcile_vehicle_bricks(&mut self) -> Result<()> {
        use super::dirty::Reader;
        if self.vehicles.world.is_none() {
            self.dirty.skip(Reader::Vehicles);
            return Ok(());
        }
        let candidates: Vec<BrickId> = if self.vehicles.scanned {
            self.dirty.read(Reader::Vehicles).into_iter().collect()
        } else {
            self.vehicles.scanned = true;
            self.dirty.skip(Reader::Vehicles);
            self.simulation.state().bricks.keys().copied().collect()
        };
        for brick_id in candidates {
            let wanted = self
                .simulation
                .state()
                .bricks
                .get(&brick_id)
                .and_then(|b| b.vehicle.as_ref())
                .and_then(|v| match &v.vehicle {
                    bri_world::ContentRef::Resolved(id) => Some(id.clone()),
                    _ => None,
                });
            self.reconcile_bot_brick(brick_id, wanted.as_deref())?;
            // Bot kinds share the spawn brick's list but are not vehicles.
            let wanted = wanted.filter(|id| !super::bots::is_bot_kind(id));
            let current = self.vehicles.by_brick.get(&brick_id).copied();
            let current_definition = current.and_then(|id| {
                self.vehicles
                    .world
                    .as_ref()?
                    .vehicle_snapshot(&self.simulation.physics, id)
                    .map(|v| v.definition)
            });
            if wanted == current_definition && (current.is_some() || wanted.is_none()) {
                continue;
            }
            if let Some(id) = current {
                self.remove_vehicle(id)?;
            } else if let Some(world) = &mut self.vehicles.world {
                world.cancel_spawn(&mut self.simulation.physics, SpawnId(brick_id))?;
            }
            if wanted.is_some() {
                self.spawn_vehicle_for(brick_id)?;
            }
        }
        Ok(())
    }
    /// Wrench `< Respawn >`: replace the brick's vehicle with a fresh one.
    /// `fxDTSBrick::recoverVehicle` (allGameScripts.cs:17839): respawn the
    /// brick's vehicle unless a player is riding it.
    pub(super) fn recover_vehicle_brick(&mut self, brick_id: BrickId) -> Result<()> {
        if let Some(vehicle) = self.vehicles.by_brick.get(&brick_id).copied()
            && self
                .vehicles
                .mounted
                .iter()
                .any(|(owner, m)| m.vehicle == vehicle && !self.is_bot(*owner))
        {
            return Ok(());
        }
        self.respawn_vehicle_brick(brick_id)
    }
    pub(super) fn respawn_vehicle_brick(&mut self, brick_id: BrickId) -> Result<()> {
        let brick = self
            .simulation
            .state()
            .bricks
            .get(&brick_id)
            .context("Unknown brick")?;
        ensure!(brick.vehicle.is_some(), "This brick has no vehicle");
        if let Some(kind) = brick.vehicle.as_ref().and_then(|v| match &v.vehicle {
            bri_world::ContentRef::Resolved(id) if super::bots::is_bot_kind(id) => Some(id.clone()),
            _ => None,
        }) {
            // Bots come back fresh at their brick.
            self.reconcile_bot_brick(brick_id, None)?;
            return self.reconcile_bot_brick(brick_id, Some(&kind));
        }
        if let Some(id) = self.vehicles.by_brick.get(&brick_id).copied() {
            self.remove_vehicle(id)?;
        } else if let Some(world) = &mut self.vehicles.world {
            world.cancel_spawn(&mut self.simulation.physics, SpawnId(brick_id))?;
        }
        self.spawn_vehicle_for(brick_id)
    }
    fn owner_game(&self, owner: OwnerId) -> Option<mg::GameId> {
        let player = self.peers.get(&owner)?.combat.player;
        self.minigames.player(player).ok()?.game
    }
    /// `miniGameCanUse` for riding: owners, trusted sandbox players and
    /// same-minigame players may ride; different minigames may not.
    pub(super) fn can_ride(&self, owner: OwnerId, vehicle_owner: OwnerId) -> bool {
        let Some(peer) = self.peers.get(&owner) else {
            return false;
        };
        let target = mg::Target::Object {
            kind: mg::ObjectKind::Vehicle,
            owner: Some(mg::AccountId(vehicle_owner)),
            membership: mg::Membership::Owner,
            spawn_brick: true,
        };
        match self.minigames.can_use(peer.combat.player, target) {
            Decision::Allow => true,
            // `$TrustLevel::RideVehicle` outside minigames.
            Decision::OutsideMinigames => peer
                .actor
                .trusted(vehicle_owner, bri_world::authority::trust::BUILD),
            _ => false,
        }
    }
    /// `miniGameCanDamage` for a vehicle: the minigame's answer, or `None`
    /// when neither side is in a minigame and trust decides instead.
    pub(super) fn vehicle_damage_decision(&self, source: OwnerId, vehicle: u64) -> Option<bool> {
        let (owner, _) = self.vehicle_owner_and_mass(vehicle)?;
        let peer = self.peers.get(&source)?;
        let source = self.minigames.projectile_source(peer.combat.player).ok()?;
        let target = mg::Target::Object {
            kind: mg::ObjectKind::Vehicle,
            owner: Some(mg::AccountId(owner)),
            membership: mg::Membership::Owner,
            spawn_brick: true,
        };
        match self.minigames.can_damage(source, target) {
            Decision::OutsideMinigames => None,
            decision => Some(decision == Decision::Allow),
        }
    }
    /// `miniGameCanDamage` for a dropped item, likewise.
    pub(super) fn item_damage_decision(
        &self,
        source: OwnerId,
        dropped_by: OwnerId,
    ) -> Option<bool> {
        let peer = self.peers.get(&source)?;
        let source = self.minigames.projectile_source(peer.combat.player).ok()?;
        let target = mg::Target::Object {
            kind: mg::ObjectKind::Item,
            owner: Some(mg::AccountId(dropped_by)),
            membership: mg::Membership::Owner,
            spawn_brick: false,
        };
        match self.minigames.can_damage(source, target) {
            Decision::OutsideMinigames => None,
            decision => Some(decision == Decision::Allow),
        }
    }
    /// Owner and datablock mass of a live vehicle.
    pub(super) fn vehicle_owner_and_mass(&self, vehicle: u64) -> Option<(OwnerId, f32)> {
        let world = self.vehicles.world.as_ref()?;
        let v = world
            .vehicle_snapshot(&self.simulation.physics, veh::VehicleId(vehicle))
            .filter(|v| !v.destroyed)?;
        Some((v.owner.0, world.definition(&v.definition)?.mass))
    }
    /// `WheeledVehicleData::damage`: scaled by the damage type's
    /// `$Damage::VehicleDamageScale`; a hit on the Tank's turret wears down
    /// the turret's own health instead.
    pub(super) fn damage_vehicle(
        &mut self,
        vehicle: u64,
        amount: f32,
        by: OwnerId,
        kind: &str,
        position: Vec3,
    ) -> Result<()> {
        let scale = self
            .weapons
            .pack
            .damage_type(kind)
            .map_or(1.0, |t| t.vehicle_scale);
        if let Some(world) = &mut self.vehicles.world {
            let id = VehicleId(vehicle);
            match world.hit_part(&self.simulation.physics, id, position.to_array()) {
                veh::VehiclePart::Turret => world.damage_turret(
                    &mut self.simulation.physics,
                    id,
                    amount * scale,
                    veh::OwnerId(by),
                )?,
                veh::VehiclePart::Chassis => world.damage(
                    &self.simulation.physics,
                    id,
                    amount * scale,
                    veh::OwnerId(by),
                )?,
            }
            let intents = world.drain_intents();
            self.apply_vehicle_intents(intents)?;
        }
        Ok(())
    }
    pub(super) fn push_vehicle(&mut self, vehicle: u64, position: Vec3, impulse: Vec3) {
        if let Some(world) = &mut self.vehicles.world {
            let _ = world.apply_impulse(
                &mut self.simulation.physics,
                VehicleId(vehicle),
                position.to_array(),
                impulse.to_array(),
            );
        }
    }
    /// Mounted players drive instead of walking, as their seat allows: the
    /// strafe keys or the mouse steer, a player-type mount faces where its
    /// rider looks, and a gunner aims relative to the hull. v20's
    /// `Player::processTick` hands the rider fire, jet and pitch and the
    /// vehicle everything else minus crouch: jet leaves (`doDismount`, "get
    /// out of the Jeep by pressing Jet"), and jump brakes a wheeled vehicle
    /// (`mBraking = trigger[2]`) or jumps the horse.
    pub(super) fn set_steering_prefs(&mut self, owner: OwnerId, strafe: bool, auto_return: bool) {
        if (strafe, auto_return) == DEFAULT_STEERING {
            self.vehicles.steering.remove(&owner);
        } else {
            self.vehicles.steering.insert(owner, (strafe, auto_return));
        }
    }
    pub(super) fn vehicle_input(&mut self, owner: OwnerId, input: MoveInput) -> Result<()> {
        let Some(mount) = self.vehicles.mounted.get(&owner).cloned() else {
            return Ok(());
        };
        let Some(world) = &mut self.vehicles.world else {
            return Ok(());
        };
        let was_held = self
            .vehicles
            .jet_held
            .insert(owner, input.jet)
            .unwrap_or(true);
        let (last_yaw, last_pitch) = self
            .vehicles
            .last_look
            .insert(owner, (input.yaw, input.pitch))
            .unwrap_or((input.yaw, input.pitch));
        let Some(v) = world.vehicle_snapshot(&self.simulation.physics, mount.vehicle) else {
            return Ok(());
        };
        let Some(d) = world.definition(&v.definition) else {
            return Ok(());
        };
        let horse = d.family == veh::Family::Horse;
        let skis = d.family == veh::Family::Skis;
        if input.jet && !was_held {
            let left = world
                .dismount(
                    &self.simulation.physics,
                    veh::OwnerId(owner),
                    OccupantId(owner),
                    false,
                )
                .is_ok();
            // The empty skis are deleted before this dismount is applied,
            // which then no longer knows they were skis: stop skiing now,
            // or the ski item stays "in use" and never fires again.
            if left && skis {
                let _ = self.weapons.cancel_skis(ActorId(owner));
            }
            return Ok(());
        }
        let fire = self
            .vehicles
            .fire_held
            .get(&owner)
            .copied()
            .unwrap_or(false);
        let (strafe, auto_return) = self
            .vehicles
            .steering
            .get(&owner)
            .copied()
            .unwrap_or(DEFAULT_STEERING);
        let (strafe_off, auto_return_off) = (!strafe, !auto_return);
        let controls = match d.seat_role_for(mount.seat, !strafe_off) {
            SeatRole::Passenger => {
                // `Player::updateMove` adds a passenger's turn to `mRot.z`
                // (0x5aeacd); the client sends it relative to the seat.
                let stale = self.vehicles.mount_yaw.get(&owner) == Some(&input.yaw);
                if !stale {
                    self.vehicles.mount_yaw.remove(&owner);
                    if input.yaw.is_finite() {
                        self.vehicles.passenger_turn.insert(owner, wrap(input.yaw));
                    }
                }
                return Ok(());
            }
            // The vehicle takes the strafe keys or the mouse turn by the
            // driver's steering prefs (`VehiclesWorld` steering).
            SeatRole::StrafeDriver | SeatRole::MouseDriver => driver_controls(
                &input,
                (last_yaw, last_pitch),
                fire,
                (strafe_off, auto_return_off),
            ),
            SeatRole::Actor => actor_controls(&input, fire, horse),
            SeatRole::Gunner => veh::Controls {
                fire,
                // Quaternion yaw turns left; look yaw turns right.
                aim_yaw: -wrap(input.yaw - heading(v.transform.rotation)),
                aim_pitch: input.pitch,
                ..Default::default()
            },
        };
        let _ = world.set_controls(veh::OwnerId(owner), OccupantId(owner), controls);
        Ok(())
    }
    /// Players touching vehicles (`WheeledVehicleData::onCollision`): one
    /// landing on top of a rideable vehicle they may use takes its first free
    /// mount node (`$Game::MinMountTime` after leaving one); anyone else it
    /// touches is run over and pushed, once per contact.
    fn vehicle_contacts(&mut self) -> Result<()> {
        let tick = self.simulation.state().tick;
        let Some(world) = &self.vehicles.world else {
            return Ok(());
        };
        let snapshot = world.snapshot(&self.simulation.physics);
        // Each live vehicle's colliders and their bounds, found once: a
        // player only runs the exact contact test against colliders whose
        // bounds its own box overlaps.
        let colliders: Vec<Vec<(ColliderHandle, Aabb)>> = snapshot
            .vehicles
            .iter()
            .map(|v| {
                if v.destroyed {
                    return Vec::new();
                }
                world
                    .colliders_of(&self.simulation.physics, v.id)
                    .into_iter()
                    .map(|c| (c, self.simulation.physics.colliders[c].compute_aabb()))
                    .collect()
            })
            .collect();
        let mut boarding = Vec::new();
        let mut touching = BTreeSet::new();
        for (owner, peer) in &self.peers {
            if !peer.combat.alive || self.seated(*owner) {
                continue;
            }
            let bounds = crate::player::item_bounds(&peer.player);
            let half = (Vec3::from(bounds.max) - Vec3::from(bounds.min)) * 0.5 + Vec3::splat(0.05);
            let body = Cuboid::new(half);
            let pose =
                Pose::from_translation((Vec3::from(bounds.min) + Vec3::from(bounds.max)) * 0.5);
            let feet = Vec3::from(peer.player.state().feet);
            let may_board = !self.bots.is_bot(*owner) && self.vehicles.may_remount(*owner, tick);
            let center = (Vec3::from(bounds.min) + Vec3::from(bounds.max)) * 0.5;
            let reach = Aabb::new(
                Vector::from_array((center - half).to_array()),
                Vector::from_array((center + half).to_array()),
            );
            for (v, colliders) in snapshot.vehicles.iter().zip(&colliders) {
                if v.destroyed {
                    continue;
                }
                let contact = colliders
                    .iter()
                    .filter(|(_, aabb)| aabb.intersects(&reach))
                    .any(|(c, _)| {
                        let collider = &self.simulation.physics.colliders[*c];
                        rapier3d::parry::query::contact(
                            &pose,
                            &body,
                            collider.position(),
                            collider.shape(),
                            0.0,
                        )
                        .is_ok_and(|c| c.is_some())
                    });
                if !contact {
                    continue;
                }
                let above = feet.y > v.transform.position[1] + MOUNT_ABOVE * v.scale;
                let free = v.seats.iter().any(|s| s.occupant.is_none());
                if above && !v.seats.is_empty() && may_board && self.can_ride(*owner, v.owner.0) {
                    if free {
                        boarding.push((*owner, v.id));
                    }
                } else {
                    touching.insert((*owner, v.id));
                }
            }
        }
        let begun: Vec<_> = touching
            .difference(&self.vehicles.touching)
            .copied()
            .collect();
        self.vehicles.touching = touching;
        let world = self.vehicles.world.as_mut().unwrap();
        for (owner, vehicle) in boarding {
            // The first free mount node takes the rider, wherever they touched.
            let free = world
                .vehicle_snapshot(&self.simulation.physics, vehicle)
                .and_then(|v| v.seats.into_iter().find(|s| s.occupant.is_none()));
            if let Some(seat) = free {
                let _ = world.mount(
                    &self.simulation.physics,
                    vehicle,
                    seat.index,
                    occupant(&self.peers, owner),
                    seat.transform.position,
                );
            }
        }
        for (owner, vehicle) in begun {
            let Some(velocity) = self.peers.get(&owner).map(|p| p.player.state().velocity) else {
                continue;
            };
            let _ = world.player_contact(
                &self.simulation.physics,
                vehicle,
                OccupantId(owner),
                velocity,
            );
        }
        Ok(())
    }
    /// `Vehicle::onActivate`: clicking a slow vehicle you may use flips it
    /// with an impulse of five times its mass, up and away from you.
    /// Returns whether the click went to a vehicle (not a nearer brick).
    pub(super) fn flip_vehicle(
        &mut self,
        owner: OwnerId,
        eye: Vec3,
        direction: Vec3,
        brick: Option<f32>,
    ) -> bool {
        let Some(world) = &self.vehicles.world else {
            return false;
        };
        let direction = direction.normalize_or_zero();
        let is_vehicle = |_: ColliderHandle, c: &Collider| c.user_data >> 64 == VEHICLE_TAG >> 64;
        let Some((collider, distance)) = self
            .simulation
            .physics
            .query_pipeline_with_filter(QueryFilter::default().predicate(&is_vehicle))
            .cast_ray(&Ray::new(eye, direction), 10.0, true)
        else {
            return false;
        };
        if brick.is_some_and(|brick| brick < distance) {
            return false;
        }
        let id = VehicleId(self.simulation.physics.colliders[collider].user_data as u64);
        let Some(v) = world
            .vehicle_snapshot(&self.simulation.physics, id)
            .filter(|v| !v.destroyed)
        else {
            return false;
        };
        let Some(mass) = world.definition(&v.definition).map(|d| d.mass) else {
            return false;
        };
        if Vec3::from(v.velocity).length() > 2.0 || !self.can_ride(owner, v.owner.0) {
            return true;
        }
        let impulse = (direction + Vec3::Y).normalize_or_zero() * mass * 5.0 / v.scale;
        self.push_vehicle(id.0, eye + direction * distance, impulse);
        true
    }
    /// `GameConnection::resetVehicles`: fresh vehicles on the owner's spawn bricks.
    pub(super) fn reset_owned_vehicles(&mut self, owner: OwnerId) {
        let bricks: Vec<BrickId> = self
            .simulation
            .state()
            .bricks
            .iter()
            .filter(|(_, b)| {
                b.owner == owner
                    && b.vehicle.as_ref().is_some_and(|v| {
                        !matches!(&v.vehicle, bri_world::ContentRef::Resolved(id) if super::bots::is_bot_kind(id))
                    })
            })
            .map(|(id, _)| *id)
            .collect();
        for brick in bricks {
            if let Err(error) = self.respawn_vehicle_brick(brick) {
                if self.notices.len() == 64 {
                    self.notices.pop_front();
                }
                self.notices
                    .push_back(format!("Reset vehicle {brick}: {error:#}"));
            }
        }
    }
    /// `fxDTSBrick::vehicleMinigameEject`: riders who may no longer use the
    /// owner's vehicles get off them.
    pub(super) fn eject_unwelcome_riders(&mut self, brick_owner: OwnerId) {
        let Some(world) = &self.vehicles.world else {
            return;
        };
        let riders: Vec<OwnerId> = world
            .snapshot(&self.simulation.physics)
            .vehicles
            .into_iter()
            .filter(|v| {
                self.vehicles.brick_of.get(&v.id).is_some_and(|b| {
                    self.simulation
                        .state()
                        .bricks
                        .get(b)
                        .is_some_and(|brick| brick.owner == brick_owner)
                })
            })
            .flat_map(|v| {
                v.seats
                    .into_iter()
                    .filter_map(|s| s.occupant.map(|o| o.owner.0))
                    .collect::<Vec<_>>()
            })
            .filter(|rider| !self.can_ride(*rider, brick_owner))
            .collect();
        for rider in riders {
            if let Some(world) = &mut self.vehicles.world {
                let _ = world.dismount(
                    &self.simulation.physics,
                    veh::OwnerId(rider),
                    OccupantId(rider),
                    false,
                );
            }
        }
        if let Some(world) = &mut self.vehicles.world {
            let intents = world.drain_intents();
            let _ = self.apply_vehicle_intents(intents);
        }
    }
    /// `Armor::damage` spares passengers the vehicle protects, and
    /// `radiusDamage` spares them its burn.
    pub(super) fn passenger_protected(&self, owner: OwnerId, kind: veh::DamageKind) -> bool {
        let (Some(mount), Some(world)) = (self.vehicles.mounted.get(&owner), &self.vehicles.world)
        else {
            return false;
        };
        world
            .passenger_protected(mount.vehicle, kind)
            .unwrap_or(false)
    }
    pub(super) fn vehicle_pre_step(&mut self) -> Result<()> {
        self.reconcile_vehicle_bricks()?;
        let waters = self.simulation.liquids();
        if let Some(world) = &mut self.vehicles.world {
            world.pre_step(&mut self.simulation.physics, &waters)?;
        }
        Ok(())
    }
    pub(super) fn vehicle_post_step(&mut self) -> Result<()> {
        let Some(world) = &mut self.vehicles.world else {
            return Ok(());
        };
        world.post_step(&mut self.simulation.physics)?;
        let intents = world.drain_intents();
        self.apply_vehicle_intents(intents)?;
        self.board_skis()?;
        self.follow_seats()?;
        self.vehicle_contacts()?;
        Ok(())
    }
    /// `Player::startSkiing`: an invisible ski vehicle at the skier's feet,
    /// moving as they were, boarded a quarter second later.
    pub(super) fn start_skis(
        &mut self,
        owner: OwnerId,
        position: Vec3,
        velocity: Vec3,
        after_ticks: u32,
    ) -> Result<()> {
        let Some(peer) = self.peers.get(&owner) else {
            return Ok(());
        };
        let yaw = peer.player.state().yaw;
        let paint = peer.current_color;
        let id = self.spawn_transient(
            owner,
            "v20.vehicle.skivehicle",
            veh::Transform {
                position: position.to_array(),
                rotation: glam::Quat::from_rotation_y(-yaw).to_array(),
            },
            velocity,
            1.0,
        );
        match id {
            Some(id) => {
                // `setNodeColor("LSki"/"RSki", getColorIDTable(%client.currentColor))`:
                // the skis take the skier's paint colour; the ski vehicle
                // itself is invisible, so its colour carries it.
                self.vehicles.colors.insert(id, Some(paint));
                let due = self.simulation.state().tick + u64::from(after_ticks);
                self.vehicles.pending_skis.push((owner, id, due));
            }
            None => {
                let _ = self.weapons.cancel_skis(ActorId(owner));
            }
        }
        Ok(())
    }
    /// Firing the skis again while skiing steps off them.
    pub(super) fn stop_skis(&mut self, owner: OwnerId) {
        if self.vehicles.mounted_family(owner) == Some(veh::Family::Skis)
            && let Some(world) = &mut self.vehicles.world
        {
            let _ = world.dismount(
                &self.simulation.physics,
                veh::OwnerId(owner),
                OccupantId(owner),
                true,
            );
        }
    }
    /// `tumble()`: ride an invisible tumbling body until it settles.
    pub(super) fn start_tumble(
        &mut self,
        owner: OwnerId,
        transform: veh::Transform,
        velocity: Vec3,
        scale: f32,
    ) -> Result<()> {
        let alive = self.peers.get(&owner).is_some_and(|p| p.combat.alive);
        if !alive || self.vehicles.mounted.contains_key(&owner) {
            return Ok(());
        }
        let Some(id) = self.spawn_transient(
            owner,
            "v20.vehicle.deathvehicle",
            transform,
            velocity,
            scale,
        ) else {
            return Ok(());
        };
        let world = self.vehicles.world.as_mut().unwrap();
        let seat = world
            .vehicle_snapshot(&self.simulation.physics, id)
            .and_then(|v| v.seats.first().map(|s| s.transform.position));
        let mounted = seat.is_some_and(|seat| {
            world
                .mount(
                    &self.simulation.physics,
                    id,
                    0,
                    occupant(&self.peers, owner),
                    seat,
                )
                .is_ok()
        });
        if !mounted {
            let _ = world.remove(&mut self.simulation.physics, id);
            return Ok(());
        }
        let intents = world.drain_intents();
        self.apply_vehicle_intents(intents)?;
        self.vehicles.tumbling.insert(owner);
        if let Some(peer) = self.peers.get_mut(&owner) {
            // The tumbling player watches through the corpse camera.
            peer.control = super::ControlObject::Corpse;
        }
        Ok(())
    }
    /// A tackle or crash throws the player into a tumble.
    pub(super) fn tumble_player(&mut self, owner: OwnerId, velocity: Vec3) -> Result<()> {
        if let Some(mount) = self.vehicles.mounted.get(&owner).cloned() {
            if self.vehicles.mounted_family(owner) == Some(veh::Family::Skis)
                && let Some(world) = &mut self.vehicles.world
            {
                world.wreck_skis(&mut self.simulation.physics, mount.vehicle)?;
                let intents = world.drain_intents();
                self.apply_vehicle_intents(intents)?;
            }
            return Ok(());
        }
        let Some(peer) = self.peers.get(&owner) else {
            return Ok(());
        };
        let state = peer.player.state();
        let transform = veh::Transform {
            position: state.feet,
            rotation: glam::Quat::from_rotation_y(-state.yaw).to_array(),
        };
        self.start_tumble(owner, transform, velocity, 1.0)
    }
    /// Spawn a helper vehicle no brick owns (skis, tumble) moving at `velocity`.
    pub(super) fn spawn_transient(
        &mut self,
        owner: OwnerId,
        definition: &str,
        transform: veh::Transform,
        velocity: Vec3,
        scale: f32,
    ) -> Option<VehicleId> {
        let world = self.vehicles.world.as_mut()?;
        self.vehicles.next_id += 1;
        let id = VehicleId(self.vehicles.next_id);
        world
            .spawn(
                &mut self.simulation.physics,
                veh::Spawn {
                    id,
                    owner: veh::OwnerId(owner),
                    definition: definition.into(),
                    transform,
                    spawn_id: None,
                    respawn_ticks: None,
                    scale,
                },
            )
            .ok()?;
        let _ = world.set_velocity(&mut self.simulation.physics, id, velocity.to_array());
        self.tag_vehicle(id);
        Some(id)
    }
    /// Board skis whose quarter second has passed; drop them otherwise.
    fn board_skis(&mut self) -> Result<()> {
        let tick = self.simulation.state().tick;
        let (due, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.vehicles.pending_skis)
            .into_iter()
            .partition(|(_, _, at)| *at <= tick);
        self.vehicles.pending_skis = waiting;
        for (owner, vehicle, _) in due {
            let eligible = self
                .peers
                .get(&owner)
                .is_some_and(|p| p.combat.alive && !self.vehicles.mounted.contains_key(&owner));
            let Some(world) = &mut self.vehicles.world else {
                continue;
            };
            // The script's `mountObject` has no reach check: however far
            // the skis slid in that quarter second, the skier is put on them.
            let seat = eligible
                .then(|| world.seat_position(&self.simulation.physics, vehicle, 0))
                .flatten();
            let boarded = seat.is_some_and(|seat| {
                world
                    .mount(
                        &self.simulation.physics,
                        vehicle,
                        0,
                        occupant(&self.peers, owner),
                        seat,
                    )
                    .is_ok()
            });
            if !boarded {
                let _ = world.remove(&mut self.simulation.physics, vehicle);
                let _ = self.weapons.cancel_skis(ActorId(owner));
            }
            let intents = world.drain_intents();
            self.apply_vehicle_intents(intents)?;
        }
        Ok(())
    }

    fn apply_vehicle_intents(&mut self, intents: Vec<Intent>) -> Result<()> {
        let tick = self.simulation.state().tick;
        for intent in intents {
            match intent {
                Intent::Fire(fire) => {
                    let _ = self.weapons.spawn(
                        &fire.projectile,
                        ActorId(fire.owner.0.max(1)),
                        Vec3::from(fire.origin),
                        Vec3::from(fire.velocity),
                        fire.scale,
                    );
                }
                Intent::Mounted {
                    vehicle,
                    occupant,
                    seat,
                    ..
                } => {
                    let owner = occupant.owner.0;
                    if let Some(peer) = self.peers.get_mut(&owner) {
                        peer.player.set_solid(&mut self.simulation.physics, false);
                        self.vehicles.mounted.insert(owner, Mount { vehicle, seat });
                        self.vehicles.jet_held.insert(owner, true);
                        // `Armor::onMount` resets the transform: facing the seat.
                        self.vehicles.passenger_turn.remove(&owner);
                        self.vehicles.mount_yaw.insert(owner, peer.input.yaw);
                        self.vehicles
                            .last_look
                            .insert(owner, (peer.input.yaw, peer.input.pitch));
                        let feet = peer.player.state().feet;
                        self.cues.emit(
                            tick,
                            crate::presentation::CueKind::VehicleSound {
                                vehicle: vehicle.0,
                                sound: "player.mount".into(),
                            },
                            feet,
                        );
                    }
                    // The Tank and cannon packages put tools away on boarding
                    // a gun seat (`ServerCmdUnUseTool`), whose fire button
                    // then works the gun. Other riders keep holding theirs.
                    if self.vehicles.weapon_seat(owner) {
                        let _ = self.weapons.trigger(ActorId(owner), false);
                        self.weapon_triggers.remove(&owner);
                        let _ = self.equip_tool(owner, None);
                    }
                }
                Intent::Dismounted {
                    vehicle,
                    occupant,
                    transform,
                    velocity,
                    ..
                } => {
                    let owner = occupant.owner.0;
                    let family = self
                        .vehicles
                        .world
                        .as_ref()
                        .and_then(|w| w.definition_of(vehicle))
                        .map(|d| d.family);
                    if family == Some(veh::Family::Skis) {
                        let _ = self.weapons.cancel_skis(ActorId(owner));
                    }
                    if self.vehicles.tumbling.remove(&owner)
                        && let Some(peer) = self.peers.get_mut(&owner)
                        && peer.combat.alive
                        && peer.control == super::ControlObject::Corpse
                    {
                        peer.control = super::ControlObject::Player;
                    }
                    self.vehicles.mounted.remove(&owner);
                    self.vehicles.last_dismount.insert(owner, tick);
                    if let Some(peer) = self.peers.get_mut(&owner) {
                        let yaw = peer.player.state().yaw;
                        peer.player.teleport(
                            &mut self.simulation.physics,
                            Vec3::from(transform.position),
                            yaw,
                        )?;
                        peer.player.push(Vec3::from(velocity));
                        peer.player
                            .set_solid(&mut self.simulation.physics, peer.combat.alive);
                        peer.inputs.clear();
                    }
                }
                Intent::Effect {
                    vehicle,
                    id,
                    active,
                } => {
                    if let Some(position) = self.vehicle_position(vehicle) {
                        self.cues.emit(
                            tick,
                            crate::presentation::CueKind::VehicleEffect {
                                vehicle: vehicle.0,
                                effect: id,
                                active,
                            },
                            position,
                        );
                    }
                }
                Intent::Audio { vehicle, id } => {
                    if let Some(position) = self.vehicle_position(vehicle) {
                        self.cues.emit(
                            tick,
                            crate::presentation::CueKind::VehicleSound {
                                vehicle: vehicle.0,
                                sound: id,
                            },
                            position,
                        );
                    }
                }
                Intent::Animation { .. } => {}
                Intent::Charged {
                    owner,
                    charge,
                    steps,
                    ..
                } => self.notify(
                    owner.0,
                    Notice::Bottom {
                        text: cannon_strength(charge, steps),
                        seconds: 1.0,
                        hide_bar: true,
                    },
                ),
                // The blast is heard from the `initialExplosionProjectile`'s
                // explosion (`vehicleExplosionSound`), as in v20; horses and
                // rowboats have no explosion.
                Intent::Destroyed { .. } => {}
                Intent::Removed { vehicle } => {
                    // Occupants were dismounted by the rule engine first.
                    let live_spawn = self.vehicles.brick_of.get(&vehicle).copied();
                    self.forget_vehicle(vehicle);
                    // A destroyed vehicle keeps its brick's respawn pending.
                    if let Some(brick) = live_spawn {
                        self.vehicles.by_brick.remove(&brick);
                    }
                }
                Intent::RespawnDue { spawn_id, .. } => {
                    let brick = spawn_id.0;
                    if self
                        .simulation
                        .state()
                        .bricks
                        .get(&brick)
                        .is_some_and(|b| b.vehicle.is_some())
                        && !self.vehicles.by_brick.contains_key(&brick)
                    {
                        self.spawn_vehicle_for(brick)?;
                    }
                }
                Intent::Struck {
                    vehicle,
                    owner,
                    other,
                    other_part,
                    point,
                    ..
                } => {
                    // A brick in a chunk collider, by the part struck.
                    let other = self
                        .simulation
                        .chunks()
                        .part_brick(other, other_part as usize)
                        .map_or(other, u128::from);
                    self.vehicle_struck(vehicle.0, owner.0, other, Vec3::from(point))?
                }
                Intent::RunOver {
                    vehicle,
                    owner,
                    target,
                    damage,
                    velocity,
                } => {
                    let victim = target.0;
                    // Whoever threw or holds the vehicle runs the victim
                    // over, not its driver or owner.
                    let owner = self
                        .mover_credit(bri_package_runtime::ops::ObjectRef::Vehicle(vehicle.0))
                        .unwrap_or(owner.0);
                    let hurts = self.can_damage_player(owner, victim, false);
                    let shoves = self
                        .vehicles
                        .world
                        .as_ref()
                        .and_then(|w| w.definition_of(vehicle))
                        .is_some_and(|d| d.shove);
                    if !hurts && !shoves {
                        continue;
                    }
                    if hurts && damage > 0.0 {
                        self.damage_player(
                            victim,
                            damage,
                            combat::DamageKind::Weapon {
                                name: "Vehicle".into(),
                                direct: false,
                            },
                            Some(owner),
                        )?;
                    }
                    if shoves {
                        // Bowled over: the victim tumbles away from it, so
                        // the vehicle rolls on through instead of stopping
                        // against a standing player.
                        if self.is_alive(victim) {
                            self.tumble_player(victim, Vec3::from(velocity) + Vec3::Y * 4.0)?;
                        }
                        continue;
                    }
                    // setVelocity: the push replaces the player's velocity.
                    if let Some(peer) = self.peers.get_mut(&victim) {
                        let current = Vec3::from(peer.player.state().velocity);
                        peer.player.push(Vec3::from(velocity) - current);
                    }
                }
                Intent::TumbleRequested {
                    occupant,
                    transform,
                    velocity,
                    scale,
                    ..
                } => {
                    // skiVehicle::onWreck stops skiing before the tumble.
                    let _ = self.weapons.cancel_skis(ActorId(occupant.owner.0));
                    self.start_tumble(occupant.owner.0, transform, velocity.into(), scale)?
                }
            }
        }
        Ok(())
    }
    fn vehicle_position(&self, vehicle: VehicleId) -> Option<[f32; 3]> {
        self.vehicles
            .world
            .as_ref()?
            .vehicle_snapshot(&self.simulation.physics, vehicle)
            .map(|v| v.transform.position)
    }
    /// Mounted players ride at their seat node.
    fn follow_seats(&mut self) -> Result<()> {
        let Some(world) = &self.vehicles.world else {
            return Ok(());
        };
        let snapshot = world.snapshot(&self.simulation.physics);
        for v in snapshot.vehicles {
            let passenger_seat = |index: usize| {
                world.definition(&v.definition).is_some_and(|d| {
                    d.seat_role(index) == SeatRole::Passenger
                })
            };
            for seat in &v.seats {
                let Some(o) = seat.occupant else { continue };
                let Some(peer) = self.peers.get_mut(&o.owner.0) else {
                    continue;
                };
                // A mounted player takes the mount transform turned by its
                // own `mRot.z` (`Player::setPosition` 0x5a6bc0). Drivers
                // and gunners never turn it; a passenger's mouse does.
                let turn = if passenger_seat(seat.index) {
                    self.vehicles
                        .passenger_turn
                        .get(&o.owner.0)
                        .copied()
                        .unwrap_or(0.0)
                } else {
                    0.0
                };
                let yaw = heading(seat.transform.rotation) + turn;
                peer.player.place(
                    &mut self.simulation.physics,
                    Vec3::from(seat.transform.position),
                    yaw,
                    Vec3::from(v.velocity),
                );
            }
        }
        Ok(())
    }
    /// Death or disconnect forces the occupant out of a vehicle or off a
    /// ridden player.
    pub(super) fn eject(&mut self, owner: OwnerId) {
        self.dismount_player(owner, true);
        if let Some(world) = &mut self.vehicles.world
            && self.vehicles.mounted.contains_key(&owner)
        {
            let _ = world.dismount(
                &self.simulation.physics,
                veh::OwnerId(owner),
                OccupantId(owner),
                true,
            );
            let intents = world.drain_intents();
            let _ = self.apply_vehicle_intents(intents);
        }
        self.vehicles.mounted.remove(&owner);
    }
    /// Spawn bricks that currently have a vehicle (`/resetVehicles`).
    pub(super) fn vehicle_spawn_bricks(&self) -> Vec<BrickId> {
        self.vehicles.by_brick.keys().copied().collect()
    }
    /// `/clearVehicles`: riders get off and the brick keeps its setting
    /// without spawning again until it changes.
    pub(super) fn clear_brick_vehicle(&mut self, brick: BrickId) -> Result<()> {
        let Some(id) = self.vehicles.by_brick.get(&brick).copied() else {
            return Ok(());
        };
        let riders: Vec<OwnerId> = self
            .vehicles
            .mounted
            .iter()
            .filter(|(_, m)| m.vehicle == id)
            .map(|(owner, _)| *owner)
            .collect();
        for owner in riders {
            self.eject(owner);
        }
        self.remove_vehicle(id)
    }
    /// `serverCmdNextSeat`/`PrevSeat`: the next free seat round the vehicle.
    /// On foot, alone on a one-seat mount or with every seat taken, nothing
    /// happens and nothing is said.
    pub fn switch_seat(&mut self, owner: OwnerId, step: i32) -> Result<()> {
        let Some(mount) = self.vehicles.mounted.get(&owner).cloned() else {
            return Ok(());
        };
        let Some(world) = self.vehicles.world.as_mut() else {
            return Ok(());
        };
        let Some(seats) = world
            .vehicle_snapshot(&self.simulation.physics, mount.vehicle)
            .map(|v| v.seats)
        else {
            return Ok(());
        };
        let count = seats.len() as i32;
        for offset in 1..count {
            let seat = (mount.seat as i32 + step * offset).rem_euclid(count) as usize;
            if seats[seat].occupant.is_some() {
                continue;
            }
            let position = seats[seat].transform.position;
            world.dismount(
                &self.simulation.physics,
                veh::OwnerId(owner),
                OccupantId(owner),
                true,
            )?;
            if world
                .mount(
                    &self.simulation.physics,
                    mount.vehicle,
                    seat,
                    occupant(&self.peers, owner),
                    position,
                )
                .is_ok()
            {
                // Drop the transient dismount so the player stays seated.
                let intents: Vec<_> = world
                    .drain_intents()
                    .into_iter()
                    .filter(|i| !matches!(i, Intent::Dismounted { .. }))
                    .collect();
                self.apply_vehicle_intents(intents)?;
                return Ok(());
            }
        }
        Ok(())
    }
}

fn spawn_color(brick: &Brick) -> Option<u8> {
    brick
        .vehicle
        .as_ref()
        .filter(|v| v.recolor)
        .map(|_| brick.color)
}
