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
    world: Option<veh::VehiclesWorld>,
    by_brick: BTreeMap<BrickId, VehicleId>,
    brick_of: BTreeMap<VehicleId, BrickId>,
    colors: BTreeMap<VehicleId, Option<u8>>,
    next_id: u64,
    mounted: BTreeMap<OwnerId, Mount>,
    last_dismount: BTreeMap<OwnerId, u64>,
    jump_held: BTreeMap<OwnerId, bool>,
    fire_held: BTreeMap<OwnerId, bool>,
    /// Look angles last fed to the vehicle, for mouse steering deltas.
    last_look: BTreeMap<OwnerId, (f32, f32)>,
    /// Skis spawned by the ski item wait to be boarded (`schedule(250, mountObject)`).
    pending_skis: Vec<(OwnerId, VehicleId, u64)>,
    /// Players riding a tumble vehicle, watched through the corpse camera.
    tumbling: BTreeSet<OwnerId>,
    scanned: bool,
}
#[derive(Debug, Clone)]
struct Mount {
    vehicle: VehicleId,
    seat: usize,
}

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
    pub turret_aim: [f32; 2],
    pub jetting: bool,
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
fn occupant(owner: OwnerId) -> veh::Occupant {
    veh::Occupant {
        id: OccupantId(owner),
        owner: veh::OwnerId(owner),
    }
}

impl Vehicles {
    pub(super) fn is_mounted(&self, owner: OwnerId) -> bool {
        self.mounted.contains_key(&owner)
    }
    /// The family of the vehicle a player rides.
    pub(super) fn mounted_family(&self, owner: OwnerId) -> Option<veh::Family> {
        let mount = self.mounted.get(&owner)?;
        Some(self.world.as_ref()?.definition_of(mount.vehicle)?.family)
    }
    /// Seated players' fire button drives the vehicle weapon, not items.
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
                id: v.id.0,
                tick,
                position: v.transform.position,
                rotation: v.transform.rotation,
                velocity: v.velocity,
                steering: v.steering,
                wheel_suspension: v.wheel_suspension,
                wheel_rotation: v.wheel_rotation,
                turret_aim: v.turret_aim,
                jetting: v.jetting,
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
        let Some(world) = &self.vehicles.world else {
            return Ok(());
        };
        let Some(def) = world.definition(definition) else {
            return Ok(());
        };
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
    fn tag_vehicle(&mut self, id: VehicleId) {
        if let Some(world) = &self.vehicles.world {
            for collider in world.colliders_of(&self.simulation.physics, id) {
                self.simulation.physics.colliders[collider].user_data =
                    VEHICLE_TAG | u128::from(id.0);
            }
        }
    }
    fn remove_vehicle(&mut self, id: VehicleId) -> Result<()> {
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
        if self.vehicles.world.is_none() {
            return Ok(());
        }
        let candidates: Vec<BrickId> = if self.vehicles.scanned {
            self.dirty.iter().copied().collect()
        } else {
            self.vehicles.scanned = true;
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
                    .snapshot(&self.simulation.physics)
                    .vehicles
                    .into_iter()
                    .find(|v| v.id == id)
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
    pub(super) fn respawn_vehicle_brick(&mut self, brick_id: BrickId) -> Result<()> {
        let brick = self
            .simulation
            .state()
            .bricks
            .get(&brick_id)
            .context("Unknown brick")?;
        ensure!(brick.vehicle.is_some(), "This brick has no vehicle");
        if let Some(kind) = brick.vehicle.as_ref().and_then(|v| match &v.vehicle {
            bri_world::ContentRef::Resolved(id) if super::bots::is_bot_kind(id) => {
                Some(id.clone())
            }
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
    fn can_ride(&self, owner: OwnerId, vehicle_owner: OwnerId) -> bool {
        let Some(peer) = self.peers.get(&owner) else {
            return false;
        };
        let target = mg::Target::Object {
            kind: mg::ObjectKind::Vehicle,
            owner: Some(mg::AccountId(vehicle_owner)),
            membership: mg::Membership::Owner,
            spawn_brick: true,
        };
        matches!(
            self.minigames.can_use(peer.combat.player, target),
            Decision::Allow | Decision::OutsideMinigames
        )
    }
    /// `WheeledVehicle::damage`: vehicles outside minigames can be damaged;
    /// inside, the minigame's vehicle damage rule applies.
    pub(super) fn can_damage_vehicle(&self, source: OwnerId, vehicle: u64) -> bool {
        let Some(world) = &self.vehicles.world else {
            return false;
        };
        let owner = world
            .snapshot(&self.simulation.physics)
            .vehicles
            .into_iter()
            .find(|v| v.id.0 == vehicle)
            .map(|v| v.owner.0);
        let (Some(owner), Some(peer)) = (owner, self.peers.get(&source)) else {
            return false;
        };
        let Ok(source) = self.minigames.projectile_source(peer.combat.player) else {
            return false;
        };
        let target = mg::Target::Object {
            kind: mg::ObjectKind::Vehicle,
            owner: Some(mg::AccountId(owner)),
            membership: mg::Membership::Owner,
            spawn_brick: true,
        };
        matches!(
            self.minigames.can_damage(source, target),
            Decision::Allow | Decision::OutsideMinigames
        )
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
    /// Owner and datablock mass of a live vehicle.
    pub(super) fn vehicle_owner_and_mass(&self, vehicle: u64) -> Option<(OwnerId, f32)> {
        let world = self.vehicles.world.as_ref()?;
        let v = world
            .snapshot(&self.simulation.physics)
            .vehicles
            .into_iter()
            .find(|v| v.id.0 == vehicle && !v.destroyed)?;
        Some((v.owner.0, world.definition(&v.definition)?.mass))
    }
    pub(super) fn damage_vehicle(&mut self, vehicle: u64, amount: f32, by: OwnerId) -> Result<()> {
        if let Some(world) = &mut self.vehicles.world {
            world.damage(
                &self.simulation.physics,
                VehicleId(vehicle),
                amount,
                veh::OwnerId(by),
            )?;
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
    /// rider looks, and a gunner aims relative to the hull. Jump leaves the
    /// vehicle (`Armor::onTrigger` while mounted), except on the horse, where
    /// jump jumps and crouch dismounts.
    pub(super) fn vehicle_input(&mut self, owner: OwnerId, input: MoveInput) -> Result<()> {
        let Some(mount) = self.vehicles.mounted.get(&owner).cloned() else {
            return Ok(());
        };
        let Some(world) = &mut self.vehicles.world else {
            return Ok(());
        };
        let was_held = self
            .vehicles
            .jump_held
            .insert(owner, input.jump)
            .unwrap_or(true);
        let (last_yaw, last_pitch) = self
            .vehicles
            .last_look
            .insert(owner, (input.yaw, input.pitch))
            .unwrap_or((input.yaw, input.pitch));
        let snapshot = world.snapshot(&self.simulation.physics);
        let Some(v) = snapshot.vehicles.iter().find(|v| v.id == mount.vehicle) else {
            return Ok(());
        };
        let Some(d) = world.definition(&v.definition) else {
            return Ok(());
        };
        let horse = d.family == veh::Family::Horse;
        let leave = if horse {
            input.crouch
        } else {
            input.jump && !was_held
        };
        if leave {
            let _ = world.dismount(
                &self.simulation.physics,
                veh::OwnerId(owner),
                OccupantId(owner),
                false,
            );
            return Ok(());
        }
        let fire = self
            .vehicles
            .fire_held
            .get(&owner)
            .copied()
            .unwrap_or(false);
        let mut controls = veh::Controls {
            throttle: input.forward,
            brake: input.crouch && !horse,
            jet: input.jet,
            vertical: if input.jet { 1.0 } else { 0.0 },
            fire,
            ..Default::default()
        };
        match d.seat_role(mount.seat) {
            SeatRole::Passenger => return Ok(()),
            SeatRole::StrafeDriver => controls.steer = input.right,
            SeatRole::MouseDriver => {
                controls.strafe = input.right;
                controls.look_delta = [
                    wrap(input.yaw - last_yaw),
                    wrap_half(input.pitch - last_pitch),
                ];
            }
            SeatRole::Actor => {
                controls.strafe = input.right;
                controls.jump = horse && input.jump;
                controls.aim_yaw = input.yaw;
                controls.aim_pitch = input.pitch;
                controls.brake = false;
            }
            SeatRole::Gunner => {
                controls = veh::Controls {
                    fire,
                    // Quaternion yaw turns left; look yaw turns right.
                    aim_yaw: -wrap(input.yaw - heading(v.transform.rotation)),
                    aim_pitch: input.pitch,
                    ..Default::default()
                };
            }
        }
        let _ = world.set_controls(veh::OwnerId(owner), OccupantId(owner), controls);
        Ok(())
    }
    /// Walking into a rideable vehicle mounts its first free seat
    /// (`Armor::onCollision` with a vehicle, `$Game::MinMountTime`).
    fn mount_contacts(&mut self) -> Result<()> {
        let tick = self.simulation.state().tick;
        let Some(world) = &self.vehicles.world else {
            return Ok(());
        };
        let snapshot = world.snapshot(&self.simulation.physics);
        let mut attempts = Vec::new();
        for (owner, peer) in &self.peers {
            if !peer.combat.alive
                || self.bots.is_bot(*owner)
                || self.vehicles.mounted.contains_key(owner)
                || self
                    .vehicles
                    .last_dismount
                    .get(owner)
                    .is_some_and(|t| tick.saturating_sub(*t) < MIN_MOUNT_TICKS)
            {
                continue;
            }
            let bounds = peer.player.world_bounds();
            let aabb = Aabb::new(
                Vector::from_array(bounds.min) - Vector::splat(0.2),
                Vector::from_array(bounds.max) + Vector::splat(0.2),
            );
            let feet = Vec3::from(peer.player.state().feet);
            for v in &snapshot.vehicles {
                if v.destroyed || feet.y <= v.transform.position[1] + MOUNT_ABOVE * v.scale {
                    continue;
                }
                let touching = world
                    .colliders_of(&self.simulation.physics, v.id)
                    .into_iter()
                    .any(|c| {
                        let collider = &self.simulation.physics.colliders[c];
                        collider.compute_aabb().intersects(&aabb)
                    });
                if touching && self.can_ride(*owner, v.owner.0) {
                    attempts.push((*owner, v.id, v.seats.len(), feet));
                }
            }
        }
        let world = self.vehicles.world.as_mut().unwrap();
        for (owner, vehicle, _, _) in attempts {
            // The first free mount node takes the rider, wherever they touched.
            let free = world
                .snapshot(&self.simulation.physics)
                .vehicles
                .into_iter()
                .find(|v| v.id == vehicle)
                .and_then(|v| v.seats.into_iter().find(|s| s.occupant.is_none()));
            if let Some(seat) = free {
                let _ = world.mount(
                    &self.simulation.physics,
                    vehicle,
                    seat.index,
                    occupant(owner),
                    seat.transform.position,
                );
            }
        }
        Ok(())
    }
    pub(super) fn vehicle_pre_step(&mut self) -> Result<()> {
        self.reconcile_vehicle_bricks()?;
        let waters = self.simulation.liquids();
        if let Some(world) = &mut self.vehicles.world {
            world.pre_step(&mut self.simulation.physics, |p| {
                waters
                    .iter()
                    .find(|w| w.footprint(p[0], p[2]).is_some() && p[1] > w.min[1])
                    .map(|w| w.max[1])
            })?;
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
        self.mount_contacts()?;
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
        let Some(id) =
            self.spawn_transient(owner, "v20.vehicle.deathvehicle", transform, velocity, scale)
        else {
            return Ok(());
        };
        let world = self.vehicles.world.as_mut().unwrap();
        let seat = world
            .snapshot(&self.simulation.physics)
            .vehicles
            .into_iter()
            .find(|v| v.id == id)
            .and_then(|v| v.seats.first().map(|s| s.transform.position));
        let mounted = seat.is_some_and(|seat| {
            world
                .mount(&self.simulation.physics, id, 0, occupant(owner), seat)
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
    fn spawn_transient(
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
                .filter(|p| p.combat.alive && !self.vehicles.mounted.contains_key(&owner))
                .map(|p| Vec3::from(p.player.state().feet));
            let Some(world) = &mut self.vehicles.world else {
                continue;
            };
            let boarded = eligible.is_some_and(|feet| {
                world
                    .mount(&self.simulation.physics, vehicle, 0, occupant(owner), feet.to_array())
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
                        let _ = self.weapons.trigger(ActorId(owner), false);
                        self.weapon_triggers.remove(&owner);
                        self.vehicles.mounted.insert(owner, Mount { vehicle, seat });
                        self.vehicles.jump_held.insert(owner, true);
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
                Intent::Destroyed { vehicle, .. } => {
                    if let Some(position) = self.vehicle_position(vehicle) {
                        self.cues.emit(
                            tick,
                            crate::presentation::CueKind::VehicleSound {
                                vehicle: vehicle.0,
                                sound: "vehicle.explosion".into(),
                            },
                            position,
                        );
                    }
                }
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
                Intent::RunOver {
                    owner,
                    target,
                    damage,
                    velocity,
                    ..
                } => {
                    let victim = target.0;
                    if self.can_damage_player(owner.0, victim, false) {
                        self.damage_player(
                            victim,
                            damage,
                            combat::DamageKind::Weapon {
                                name: "Vehicle".into(),
                                direct: true,
                            },
                            Some(owner.0),
                        )?;
                    }
                    if let Some(peer) = self.peers.get_mut(&victim) {
                        peer.player.push(Vec3::from(velocity));
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
            .snapshot(&self.simulation.physics)
            .vehicles
            .into_iter()
            .find(|v| v.id == vehicle)
            .map(|v| v.transform.position)
    }
    /// Mounted players ride at their seat node.
    fn follow_seats(&mut self) -> Result<()> {
        let Some(world) = &self.vehicles.world else {
            return Ok(());
        };
        let snapshot = world.snapshot(&self.simulation.physics);
        for v in snapshot.vehicles {
            let Some(d) = world.definition(&v.definition) else {
                continue;
            };
            for seat in &v.seats {
                let Some(o) = seat.occupant else { continue };
                let Some(peer) = self.peers.get_mut(&o.owner.0) else {
                    continue;
                };
                // Whoever controls the vehicle sits fixed in the seat;
                // passengers turn freely (`mRot.z` relative to the mount).
                let yaw = if d.seat_role(seat.index) == SeatRole::Passenger {
                    peer.input.yaw
                } else {
                    heading(seat.transform.rotation)
                };
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
    /// Death or disconnect forces the occupant out.
    pub(super) fn eject(&mut self, owner: OwnerId) {
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
    pub fn switch_seat(&mut self, owner: OwnerId, step: i32) -> Result<()> {
        let mount = self
            .vehicles
            .mounted
            .get(&owner)
            .cloned()
            .context("You are not in a vehicle")?;
        let world = self.vehicles.world.as_mut().context("No vehicles")?;
        let seats = world
            .snapshot(&self.simulation.physics)
            .vehicles
            .into_iter()
            .find(|v| v.id == mount.vehicle)
            .map(|v| v.seats)
            .context("Vehicle is gone")?;
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
                    occupant(owner),
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
        anyhow::bail!("No free seat")
    }
}

fn spawn_color(brick: &Brick) -> Option<u8> {
    brick
        .vehicle
        .as_ref()
        .filter(|v| v.recolor)
        .map(|_| brick.color)
}
