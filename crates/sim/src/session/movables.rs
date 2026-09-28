//! Moving things by hand, for Add-Ons with the `physics` capability:
//! pushing, holding and throwing players, vehicles (every loose physics
//! body) and package entities, spawning an Add-On's own vehicles, and the
//! credit a thrown object carries for what it hits.
//!
//! The engine owns the mechanism and the rules; a package owns the policy
//! (a gravity gun, a tractor beam, a wrecking ball). Who may move what
//! follows the minigame rules, and the trust list outside minigames, the
//! same way whoever throws a rocket or rides a vehicle is judged:
//!
//! - a player may move another player when their minigame lets them hurt
//!   that player, or, outside minigames, when that player trusts them to
//!   build;
//! - a vehicle when its minigame lets them damage it, or, outside
//!   minigames, when they could ride it (its owner trusts them);
//! - a package entity always.
//!
//! What a moved object then hits is credited to whoever moved it for a few
//! seconds, so a thrown tank that lands on someone is the thrower's kill.
use super::*;
use bri_package_runtime::ops::{MAX_HOLD_DISTANCE, ObjectRef, PLAYER_MASS};
use bri_package_runtime::script::{HoldView, ObjectView};
use bri_vehicles::{self as veh, VehicleId};
use rapier3d::parry::query::ShapeCastOptions;
use rapier3d::prelude::*;

/// How long a push or throw keeps crediting the mover.
const CREDIT_TICKS: u64 = 5 * 120;
/// Vehicles one package may have spawned at once.
pub(super) const MAX_PACKAGE_VEHICLES: usize = 64;
/// A held object dragged this far from where it should float is let go.
const HOLD_BREAK: f32 = MAX_HOLD_DISTANCE + 8.0;
/// Fastest a hold drags anything, units per second.
const HOLD_SPEED: f32 = 45.0;
/// How often a hold is checked against the rules again.
const HOLD_RECHECK: u64 = 12;
/// A player's body centre above their feet.
const PLAYER_CENTRE: f32 = 1.3;

#[derive(Debug, Clone, Copy)]
struct Hold {
    target: ObjectRef,
    distance: f32,
}

#[derive(Default)]
pub(super) struct Movables {
    holds: BTreeMap<OwnerId, Hold>,
    /// Who moved an object last, and until which tick it counts.
    credits: BTreeMap<ObjectRef, (OwnerId, u64)>,
    /// Vehicles packages spawned, by package.
    spawned: BTreeMap<u64, String>,
}

impl Session {
    /// Vehicles as package scripts see them.
    pub(super) fn movable_views(&self) -> Vec<ObjectView> {
        let Some(world) = &self.vehicles.world else {
            return Vec::new();
        };
        world
            .snapshot(&self.simulation.physics)
            .vehicles
            .into_iter()
            .filter(|v| !v.destroyed)
            .filter_map(|v| {
                let d = world.definition(&v.definition)?;
                let size = (Vec3::from(d.bounds_max) - Vec3::from(d.bounds_min)) * v.scale;
                let centre = self
                    .object_centre(ObjectRef::Vehicle(v.id.0))
                    .unwrap_or(Vec3::from(v.transform.position));
                Some(ObjectView {
                    object: ObjectRef::Vehicle(v.id.0),
                    definition: v.definition.clone(),
                    position: centre.to_array(),
                    velocity: v.velocity,
                    mass: d.mass,
                    radius: size.length() * 0.5,
                    owner: (v.owner.0 != 0).then_some(v.owner.0),
                    package: self
                        .movables
                        .spawned
                        .get(&v.id.0)
                        .cloned()
                        .unwrap_or_default(),
                })
            })
            .collect()
    }
    pub(super) fn hold_views(&self) -> Vec<HoldView> {
        self.movables
            .holds
            .iter()
            .map(|(player, h)| HoldView {
                player: *player,
                object: h.target,
                distance: h.distance,
            })
            .collect()
    }
    /// What `player` holds.
    pub fn held_by(&self, player: OwnerId) -> Option<ObjectRef> {
        self.movables.holds.get(&player).map(|h| h.target)
    }
    /// Who a push or throw credits for what `target` hits now.
    pub(super) fn mover_credit(&self, target: ObjectRef) -> Option<OwnerId> {
        let tick = self.simulation.state().tick;
        self.movables
            .credits
            .get(&target)
            .filter(|(owner, until)| tick <= *until && self.peers.contains_key(owner))
            .map(|(owner, _)| *owner)
    }
    fn credit(&mut self, target: ObjectRef, by: OwnerId) {
        let until = self.simulation.state().tick + CREDIT_TICKS;
        self.movables.credits.insert(target, (by, until));
    }

    /// The vehicle a player's body is part of: their seat, or their tumble.
    fn ridden(&self, player: OwnerId) -> Option<VehicleId> {
        self.vehicles
            .world
            .as_ref()?
            .occupant(veh::OccupantId(player))
            .map(|(v, _)| v)
    }
    /// Where an object's middle is now.
    pub(super) fn object_centre(&self, target: ObjectRef) -> Option<Vec3> {
        match target {
            ObjectRef::Player(p) => {
                if let Some(v) = self.ridden(p) {
                    return self.object_centre(ObjectRef::Vehicle(v.0));
                }
                let peer = self.peers.get(&p)?;
                Some(Vec3::from(peer.player.state().feet) + Vec3::Y * PLAYER_CENTRE)
            }
            ObjectRef::Vehicle(v) => {
                let world = self.vehicles.world.as_ref()?;
                if !world.is_alive(VehicleId(v)) {
                    return None;
                }
                match world.body_of(VehicleId(v)) {
                    Some(body) => Some(Vec3::from(
                        self.simulation.physics.bodies.get(body)?.center_of_mass(),
                    )),
                    None => world
                        .snapshot(&self.simulation.physics)
                        .vehicles
                        .into_iter()
                        .find(|s| s.id.0 == v)
                        .map(|s| Vec3::from(s.transform.position) + Vec3::Y),
                }
            }
            ObjectRef::Entity(e) => {
                let entity = self.packages.as_ref()?.entities.get(&e)?;
                Some(Vec3::from(entity.body.state().feet) + Vec3::Y * PLAYER_CENTRE)
            }
        }
    }
    fn object_velocity(&self, target: ObjectRef) -> Option<Vec3> {
        match target {
            ObjectRef::Player(p) => {
                if let Some(v) = self.ridden(p) {
                    return self.object_velocity(ObjectRef::Vehicle(v.0));
                }
                Some(Vec3::from(self.peers.get(&p)?.player.state().velocity))
            }
            ObjectRef::Vehicle(v) => {
                let world = self.vehicles.world.as_ref()?;
                match world.body_of(VehicleId(v)) {
                    Some(body) => Some(Vec3::from(
                        self.simulation.physics.bodies.get(body)?.linvel(),
                    )),
                    None => world
                        .snapshot(&self.simulation.physics)
                        .vehicles
                        .into_iter()
                        .find(|s| s.id.0 == v)
                        .map(|s| Vec3::from(s.velocity)),
                }
            }
            ObjectRef::Entity(e) => Some(Vec3::from(
                self.packages
                    .as_ref()?
                    .entities
                    .get(&e)?
                    .body
                    .state()
                    .velocity,
            )),
        }
    }
    fn object_mass(&self, target: ObjectRef) -> f32 {
        match target {
            ObjectRef::Vehicle(v) => self
                .vehicles
                .world
                .as_ref()
                .and_then(|w| w.definition_of(VehicleId(v)))
                .map_or(PLAYER_MASS, |d| d.mass),
            ObjectRef::Player(p) => match self.ridden(p) {
                Some(v) => self.object_mass(ObjectRef::Vehicle(v.0)),
                None => PLAYER_MASS,
            },
            ObjectRef::Entity(_) => PLAYER_MASS,
        }
    }

    /// Whether `mover` may push, hold or throw `target` (see the module
    /// documentation for the rules).
    pub fn may_move(&self, mover: OwnerId, target: ObjectRef) -> bool {
        let Some(peer) = self.peers.get(&mover) else {
            return false;
        };
        if !peer.combat.alive || self.teleport_lockout(mover, super::admin_players::TELEPORT_WEAPON_LOCK_MS, false) {
            return false;
        }
        match target {
            ObjectRef::Player(p) => {
                let Some(victim) = self.peers.get(&p) else {
                    return false;
                };
                if p == mover || !victim.combat.alive {
                    return false;
                }
                let (Ok(source), Ok(t)) = (
                    self.minigames.projectile_source(peer.combat.player),
                    self.minigames.target_for_player(victim.combat.player),
                ) else {
                    return false;
                };
                match self.minigames.can_damage(source, t) {
                    bri_minigames::Decision::Allow => true,
                    bri_minigames::Decision::OutsideMinigames => {
                        peer.actor.trusted(p, bri_world::authority::trust::BUILD)
                    }
                    _ => false,
                }
            }
            ObjectRef::Vehicle(v) => {
                let Some(world) = &self.vehicles.world else {
                    return false;
                };
                if !world.is_alive(VehicleId(v)) || self.ridden(mover) == Some(VehicleId(v)) {
                    return false;
                }
                let Some((owner, _)) = self.vehicle_owner_and_mass(v) else {
                    return false;
                };
                match self.vehicle_damage_decision(mover, v) {
                    Some(allowed) => allowed,
                    None => {
                        owner == 0
                            || owner == mover
                            || peer.actor.trusted(owner, bri_world::authority::trust::BUILD)
                    }
                }
            }
            ObjectRef::Entity(e) => self
                .packages
                .as_ref()
                .is_some_and(|h| h.entities.contains_key(&e)),
        }
    }

    /// The nearest movable object along `direction` from `eye` within
    /// `reach`, and how far: other players, vehicles (not the one the
    /// aimer rides) and package entities. A thin sphere is swept so a
    /// player-sized target is easy to point at.
    pub(super) fn aim_object(
        &self,
        aimer: OwnerId,
        eye: Vec3,
        direction: Vec3,
        reach: f32,
    ) -> Option<(ObjectRef, Vec3, f32)> {
        let direction = direction.try_normalize()?;
        if !(eye.is_finite() && reach.is_finite() && reach > 0.0) {
            return None;
        }
        let own_body = (1_u128 << 64) | u128::from(aimer);
        let ridden = self
            .ridden(aimer)
            .map(|v| super::vehicles::VEHICLE_TAG | u128::from(v.0));
        let predicate = |_: ColliderHandle, c: &Collider| {
            let tag = c.user_data >> 64;
            (1..=3).contains(&tag)
                && c.user_data != own_body
                && Some(c.user_data) != ridden
        };
        let shape = Ball::new(0.35);
        let (handle, hit) = self
            .simulation
            .physics
            .query_pipeline_with_filter(QueryFilter::default().predicate(&predicate))
            .cast_shape(
                &Pose::from_translation(eye),
                direction * reach,
                &shape,
                ShapeCastOptions {
                    max_time_of_impact: 1.0,
                    stop_at_penetration: true,
                    ..Default::default()
                },
            )?;
        let tag = self.simulation.physics.colliders[handle].user_data;
        let id = tag as u64;
        let object = match tag >> 64 {
            1 => match self.ridden(id) {
                Some(v) => ObjectRef::Vehicle(v.0),
                None => ObjectRef::Player(id),
            },
            2 => {
                // A tumbling player's body is their tumble vehicle; aiming
                // at it aims at the player.
                let rider = self.vehicles.world.as_ref().and_then(|w| {
                    let v = w
                        .snapshot(&self.simulation.physics)
                        .vehicles
                        .into_iter()
                        .find(|s| s.id.0 == id)?;
                    let d = w.definition(&v.definition)?;
                    (d.family == veh::Family::Tumble)
                        .then(|| v.seats.first()?.occupant.map(|o| o.owner.0))
                        .flatten()
                });
                match rider {
                    Some(player) => ObjectRef::Player(player),
                    None => ObjectRef::Vehicle(id),
                }
            }
            _ => ObjectRef::Entity(id),
        };
        let distance = hit.time_of_impact * reach;
        Some((object, eye + direction * distance, distance))
    }

    /// Apply one of the `physics` operations. `caller` is the player whose
    /// command asked, who must be allowed to move what they move.
    pub(super) fn apply_physics_op(
        &mut self,
        package: &str,
        op: bri_package_runtime::Op,
        caller: Option<OwnerId>,
    ) -> Result<()> {
        use bri_package_runtime::Op;
        let allowed = |s: &Self, target: ObjectRef, by: Option<OwnerId>| -> Result<()> {
            if let Some(mover) = caller.or(by) {
                ensure!(
                    s.may_move(mover, target),
                    "Player {mover} may not move {target} under the minigame and trust rules"
                );
            }
            Ok(())
        };
        match op {
            Op::Push {
                target,
                velocity,
                by,
            } => {
                let by = by.filter(|b| self.peers.contains_key(b));
                allowed(self, target, by)?;
                let velocity = Vec3::from(velocity);
                self.push_object(target, velocity)?;
                if let Some(by) = by.or(caller) {
                    self.credit(target, by);
                }
                Ok(())
            }
            Op::Tumble {
                player,
                velocity,
                by,
            } => {
                let by = by.filter(|b| self.peers.contains_key(b));
                let target = ObjectRef::Player(player);
                allowed(self, target, by)?;
                let peer = self.peers.get(&player).context("No such player")?;
                ensure!(peer.combat.alive, "Only living players tumble");
                let velocity = Vec3::from(velocity);
                match self.ridden(player) {
                    // Already tumbling: fling the tumble.
                    Some(v) if self.vehicles.mounted_family(player) == Some(veh::Family::Tumble) => {
                        self.push_object(ObjectRef::Vehicle(v.0), velocity - self.object_velocity(target).unwrap_or_default())?;
                    }
                    Some(_) => anyhow::bail!("A seated player cannot tumble"),
                    None => {
                        self.tumble_player(player, velocity)?;
                    }
                }
                if let Some(by) = by.or(caller) {
                    self.credit(target, by);
                    if let Some(v) = self.ridden(player) {
                        self.credit(ObjectRef::Vehicle(v.0), by);
                    }
                }
                Ok(())
            }
            Op::Hold {
                player,
                target,
                distance,
            } => {
                ensure!(
                    caller.is_none_or(|c| c == player),
                    "A player holds things only by their own command"
                );
                let peer = self.peers.get(&player).context("No such player")?;
                ensure!(peer.combat.alive, "Only living players hold things");
                ensure!(
                    target != ObjectRef::Player(player),
                    "A player cannot hold themselves"
                );
                ensure!(
                    self.may_move(player, target),
                    "Player {player} may not move {target} under the minigame and trust rules"
                );
                // One holder at a time: taking it from someone else ends
                // their hold.
                self.movables.holds.retain(|_, h| h.target != target);
                self.movables
                    .holds
                    .insert(player, Hold { target, distance });
                self.credit(target, player);
                Ok(())
            }
            Op::LetGo { player } => {
                self.movables.holds.remove(&player);
                Ok(())
            }
            Op::SpawnVehicle {
                definition,
                position,
                yaw,
                velocity,
                owner,
            } => {
                let host = self.packages.as_ref().context("No packages are enabled")?;
                let namespace = definition.split(':').next().unwrap_or_default();
                let depends = host
                    .catalog
                    .packages
                    .get(package)
                    .is_some_and(|p| p.manifest.dependencies.contains_key(namespace));
                ensure!(
                    namespace == package || depends,
                    "`{definition}` is not a vehicle of `{package}` or an Add-On it depends on"
                );
                let count = self
                    .movables
                    .spawned
                    .values()
                    .filter(|p| p.as_str() == package)
                    .count();
                ensure!(
                    count < MAX_PACKAGE_VEHICLES,
                    "`{package}` already has {MAX_PACKAGE_VEHICLES} vehicles out"
                );
                let owner = owner.filter(|o| self.peers.contains_key(o));
                // Add-On vehicles count toward the server's vehicle limits.
                if let Err(text) = self.vehicle_room(owner.unwrap_or(0), &definition) {
                    if let Some(owner) = owner {
                        self.notify(owner, Notice::Center { text: text.clone(), seconds: 2.0 });
                    }
                    anyhow::bail!("{}", text.trim_start_matches('\u{E000}'));
                }
                let transform = veh::Transform {
                    position,
                    rotation: glam::Quat::from_rotation_y(-yaw).to_array(),
                };
                let id = self
                    .spawn_transient(owner.unwrap_or(0), &definition, transform, Vec3::from(velocity), 1.0)
                    .with_context(|| format!("`{definition}` could not spawn there"))?;
                self.movables.spawned.insert(id.0, package.to_string());
                if let Some(owner) = owner {
                    self.credit(ObjectRef::Vehicle(id.0), owner);
                }
                Ok(())
            }
            Op::RemoveVehicle { vehicle } => {
                ensure!(
                    self.movables.spawned.get(&vehicle).map(String::as_str) == Some(package),
                    "Vehicle {vehicle} was not spawned by `{package}`"
                );
                self.movables.spawned.remove(&vehicle);
                self.remove_vehicle(VehicleId(vehicle))
            }
            other => anyhow::bail!("{other:?} is not a physics operation"),
        }
    }

    /// Change an object's velocity by `delta`.
    fn push_object(&mut self, target: ObjectRef, delta: Vec3) -> Result<()> {
        ensure!(delta.is_finite(), "Invalid push");
        match target {
            ObjectRef::Player(p) => {
                if let Some(v) = self.ridden(p) {
                    // A seated player moves with their seat; a tumbling one
                    // is their tumble.
                    if self.vehicles.mounted_family(p) == Some(veh::Family::Tumble) {
                        return self.push_object(ObjectRef::Vehicle(v.0), delta);
                    }
                    anyhow::bail!("A seated player cannot be pushed");
                }
                let peer = self.peers.get_mut(&p).context("No such player")?;
                peer.player.push(delta);
                Ok(())
            }
            ObjectRef::Vehicle(v) => {
                let current = self
                    .object_velocity(target)
                    .context("No such vehicle")?;
                let world = self.vehicles.world.as_mut().context("No vehicles")?;
                world.set_velocity(
                    &mut self.simulation.physics,
                    VehicleId(v),
                    (current + delta).clamp_length_max(200.0).to_array(),
                )
            }
            ObjectRef::Entity(e) => {
                let entity = self
                    .packages
                    .as_mut()
                    .and_then(|h| h.entities.get_mut(&e))
                    .context("No such entity")?;
                entity.body.push(delta);
                Ok(())
            }
        }
    }

    /// Pull every held object toward where its holder looks. Runs before
    /// the physics step. Heavy things answer slowly, so they swing and lag.
    pub(super) fn step_holds(&mut self) {
        let tick = self.simulation.state().tick;
        self.movables.credits.retain(|_, (_, until)| *until >= tick);
        let recheck = tick.is_multiple_of(HOLD_RECHECK);
        let holds: Vec<(OwnerId, Hold)> =
            self.movables.holds.iter().map(|(p, h)| (*p, *h)).collect();
        for (player, hold) in holds {
            let aim = self.peers.get(&player).filter(|p| p.combat.alive).map(|p| {
                (p.player.eye(), p.player.state().forward())
            });
            let keep = aim.is_some()
                && !self.vehicles.is_mounted(player)
                && (!recheck || self.may_move(player, hold.target));
            let centre = self.object_centre(hold.target);
            let (Some((eye, look)), Some(centre), true) = (aim, centre, keep) else {
                self.movables.holds.remove(&player);
                continue;
            };
            let point = eye + look * hold.distance;
            let offset = point - centre;
            if offset.length() > HOLD_BREAK {
                self.movables.holds.remove(&player);
                continue;
            }
            let mass = self.object_mass(hold.target).max(1.0);
            // Response per second: nimble for a player, sluggish for a tank.
            let rate = 25.0 / (1.0 + mass / 300.0);
            let blend = 1.0 - (-rate / 120.0_f32).exp();
            let wanted = (offset * 12.0).clamp_length_max(HOLD_SPEED);
            let current = self.object_velocity(hold.target).unwrap_or_default();
            let _ = self.push_object(hold.target, (wanted - current) * blend);
            if let ObjectRef::Vehicle(v) = hold.target
                && let Some(body) = self
                    .vehicles
                    .world
                    .as_ref()
                    .and_then(|w| w.body_of(VehicleId(v)))
                && let Some(b) = self.simulation.physics.bodies.get_mut(body)
            {
                // Held things steady instead of spinning.
                let spin = b.angvel() * 0.92;
                b.set_angvel(spin, true);
            }
            self.credit(hold.target, player);
        }
        let alive: Vec<u64> = self.movables.spawned.keys().copied().collect();
        for id in alive {
            if !self
                .vehicles
                .world
                .as_ref()
                .is_some_and(|w| w.is_alive(VehicleId(id)))
            {
                self.movables.spawned.remove(&id);
            }
        }
    }

    /// A smashing vehicle struck something: bricks break under the same
    /// rules a rocket's hit follows, credited to whoever threw the vehicle,
    /// else its owner.
    pub(super) fn vehicle_struck(
        &mut self,
        vehicle: u64,
        owner: OwnerId,
        other: u128,
        point: Vec3,
    ) -> Result<()> {
        if other >> 64 != 0 {
            return Ok(());
        }
        let brick = other as BrickId;
        let Some(smash) = self
            .vehicles
            .world
            .as_ref()
            .and_then(|w| w.definition_of(VehicleId(vehicle)))
            .and_then(|d| d.smash)
        else {
            return Ok(());
        };
        let source = self
            .mover_credit(ObjectRef::Vehicle(vehicle))
            .unwrap_or(owner);
        if !self.peers.contains_key(&source) || !self.simulation.state().bricks.contains_key(&brick) {
            return Ok(());
        }
        let direct = bri_weapons::BrickImpact {
            radius: 0.0,
            direct: true,
            force: smash.force,
            max_volume: smash.max_volume,
            max_floating_volume: smash.max_volume,
        };
        self.blow_up_bricks(source, Some(brick), point, &direct)?;
        if smash.radius > 0.0 {
            let around = bri_weapons::BrickImpact {
                radius: smash.radius,
                direct: false,
                ..direct
            };
            self.blow_up_bricks(source, None, point, &around)?;
        }
        Ok(())
    }

    /// A player left: they hold nothing, and nothing they threw counts as
    /// theirs any more.
    pub(super) fn forget_mover(&mut self, owner: OwnerId) {
        self.movables.holds.remove(&owner);
        self.movables
            .holds
            .retain(|_, h| h.target != ObjectRef::Player(owner));
        self.movables.credits.retain(|_, (by, _)| *by != owner);
    }
    /// Put a loose vehicle in the world with no spawn brick (host tooling,
    /// tests): `owner`'s, or the world's with 0. Returns its id.
    pub fn spawn_vehicle_at(
        &mut self,
        owner: OwnerId,
        definition: &str,
        position: Vec3,
        yaw: f32,
        velocity: Vec3,
    ) -> Result<u64> {
        ensure!(position.is_finite() && yaw.is_finite() && velocity.is_finite(), "Invalid vehicle spawn");
        let transform = veh::Transform {
            position: position.to_array(),
            rotation: glam::Quat::from_rotation_y(-yaw).to_array(),
        };
        let id = self
            .spawn_transient(owner, definition, transform, velocity, 1.0)
            .with_context(|| format!("`{definition}` could not spawn there"))?;
        Ok(id.0)
    }
    /// Vehicles `package` spawned that are still out, oldest first.
    pub fn package_vehicles(&self, package: &str) -> Vec<u64> {
        self.movables
            .spawned
            .iter()
            .filter(|(_, p)| p.as_str() == package)
            .map(|(id, _)| *id)
            .collect()
    }
}
