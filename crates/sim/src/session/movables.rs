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
//! - a package entity always;
//! - outside minigames, an administrator anything (they may already fetch
//!   and teleport players).
//!
//! What a moved object then hits is credited to whoever moved it for a few
//! seconds, so a thrown tank that lands on someone is the thrower's kill.
use super::*;
use bri_package_runtime::ops::{MAX_HOLD_DISTANCE, ObjectRef, PLAYER_MASS};
use bri_package_runtime::script::{HoldView, ObjectView};
use bri_vehicles::{self as veh, VehicleId};
use glam::Quat;
use rapier3d::parry::query::ShapeCastOptions;
use rapier3d::prelude::*;

/// How long a push or throw keeps crediting the mover.
const CREDIT_TICKS: u64 = 5 * 120;
/// Vehicles one package may have spawned at once.
pub(super) const MAX_PACKAGE_VEHICLES: usize = 64;
/// A held object this far from where it should be is let go at once.
const HOLD_BREAK: f32 = MAX_HOLD_DISTANCE + 8.0;
/// Fastest a hold closes on its point, units per second, on top of the
/// point's own motion.
const HOLD_SPEED: f32 = 60.0;
/// Fastest a held object is carried at all (a flick of the view flings
/// it about this fast).
const HOLD_CARRY: f32 = 90.0;
/// How quickly a hold closes the gap, per second: the gap shrinks by this
/// fraction of itself every second, so it settles without overshooting.
const HOLD_GAIN: f32 = 18.0;
/// Most acceleration a hold gives anything, units per second squared.
const MAX_HOLD_ACCEL: f32 = 450.0;
/// The pull `hold` uses unless told otherwise, mass x acceleration: a
/// player-weight body answers at the ceiling above, a jeep (300) at 120,
/// a steel ball (900) at 40, and anything past about 1,800 cannot lift
/// against gravity and is only dragged.
const HOLD_FORCE: f32 = 36_000.0;
/// How quickly a held object turns back to its grip, per second.
const TURN_GAIN: f32 = 14.0;
/// Fastest a hold spins anything, radians per second.
const TURN_SPEED: f32 = 20.0;
/// Gravity on players, vehicles and entities alike, units per second
/// squared; a hold carries the object's weight.
const GRAVITY: f32 = 20.0;
/// A pull that has not brought its object closer for this long gives up.
const PULL_STALL_TICKS: u32 = 120;
/// A caught object kept this far off its point (units past its own
/// size) for `SNAG_TICKS` has snagged on something and is let go.
const SNAG_DISTANCE: f32 = 3.0;
const SNAG_TICKS: u32 = 60;
/// How often a hold is checked against the rules again.
const HOLD_RECHECK: u64 = 12;
/// A player's body centre above their feet.
const PLAYER_CENTRE: f32 = 1.3;
/// Half the width of a player's body, for keeping held things out of it.
const PLAYER_HALF_WIDTH: f32 = 0.7;
const DT: f32 = 1.0 / 120.0;

#[derive(Debug, Clone, Copy)]
struct Hold {
    target: ObjectRef,
    distance: f32,
    /// Most the hold may pull, mass x acceleration.
    force: f32,
    /// The point held by, in the object's own frame (from its body's
    /// centre of mass), for a physics body held off-centre.
    anchor: Vec3,
    /// The object's turn relative to the holder's heading, kept while it
    /// is held (`turn`).
    grip: Option<Quat>,
    /// Where the hold point was, and how fast it moves.
    last_point: Option<Vec3>,
    lead: Vec3,
    /// The holder's heading last tick, and how fast it turns.
    last_yaw: Option<f32>,
    turning: f32,
    /// Reached its point at least once.
    caught: bool,
    /// Nearest it has come while being pulled in.
    closest: f32,
    /// Ticks without headway (pulling) or kept off its point (caught).
    stuck: u32,
    /// For a player: whether they were alive when caught. Dying or
    /// respawning ends the hold (a corpse held is a corpse let go).
    alive: bool,
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
                    Some(body) => Some(self.simulation.physics.bodies.get(body)?.center_of_mass()),
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
                    Some(body) => Some(self.simulation.physics.bodies.get(body)?.linvel()),
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
        if !peer.combat.alive
            || self.teleport_lockout(mover, super::admin_players::TELEPORT_WEAPON_LOCK_MS, false)
        {
            return false;
        }
        // Outside minigames an administrator may move anyone and anything,
        // as they may already fetch and teleport players; inside one, its
        // rules decide for them as for everyone.
        let trusted = |owner: OwnerId| {
            peer.actor.administrator
                || peer.actor.trusted(owner, bri_world::authority::trust::BUILD)
        };
        match target {
            ObjectRef::Player(p) => {
                let Some(victim) = self.peers.get(&p) else {
                    return false;
                };
                if p == mover {
                    return false;
                }
                // A corpse can no longer be hurt: it may be moved by
                // players of its minigame, or outside minigames by those
                // it trusted, as it could have been alive.
                if !victim.combat.alive {
                    return match (self.game_of(mover), self.game_of(p)) {
                        (Some(a), Some(b)) => a == b,
                        (None, None) => trusted(p),
                        _ => false,
                    };
                }
                let (Ok(source), Ok(t)) = (
                    self.minigames.projectile_source(peer.combat.player),
                    self.minigames.target_for_player(victim.combat.player),
                ) else {
                    return false;
                };
                match self.minigames.can_damage(source, t) {
                    bri_minigames::Decision::Allow => true,
                    bri_minigames::Decision::OutsideMinigames => trusted(p),
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
                        owner == 0 || owner == mover || trusted(owner)
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
            (1..=3).contains(&tag) && c.user_data != own_body && Some(c.user_data) != ridden
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
                    Some(v)
                        if self.vehicles.mounted_family(player) == Some(veh::Family::Tumble) =>
                    {
                        self.push_object(
                            ObjectRef::Vehicle(v.0),
                            velocity - self.object_velocity(target).unwrap_or_default(),
                        )?;
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
                at,
                force,
                turn,
            } => {
                ensure!(
                    caller.is_none_or(|c| c == player),
                    "A player holds things only by their own command"
                );
                let peer = self.peers.get(&player).context("No such player")?;
                ensure!(peer.combat.alive, "Only living players hold things");
                let (eye, look, yaw) = (
                    peer.player.eye(),
                    peer.player.state().forward(),
                    peer.player.state().yaw,
                );
                ensure!(
                    target != ObjectRef::Player(player),
                    "A player cannot hold themselves"
                );
                ensure!(
                    self.may_move(player, target),
                    "Player {player} may not move {target} under the minigame and trust rules"
                );
                // A living player is held as their tumble: a body the
                // server moves alone, which nobody's prediction fights.
                if let ObjectRef::Player(p) = target
                    && self.peers.get(&p).is_some_and(|v| v.combat.alive)
                    && self.ridden(p).is_none()
                {
                    let velocity = self.object_velocity(target).unwrap_or_default();
                    self.tumble_player(p, velocity)?;
                }
                // One holder at a time: taking it from someone else ends
                // their hold.
                self.movables.holds.retain(|_, h| h.target != target);
                let (anchor, grip) = match self.held_body(target) {
                    Some(body) => {
                        let b = &self.simulation.physics.bodies[body];
                        let pose = *b.position();
                        let anchor = at.map_or(Vec3::ZERO, |at| {
                            pose.rotation.inverse() * (Vec3::from(at) - b.center_of_mass())
                        });
                        let heading = Quat::from_rotation_y(-yaw);
                        (
                            anchor,
                            turn.then(|| (heading.inverse() * pose.rotation).normalize()),
                        )
                    }
                    None => (Vec3::ZERO, None),
                };
                let centre = self.object_centre(target).unwrap_or_default();
                let closest = self
                    .hold_point_of(target, anchor)
                    .unwrap_or(centre)
                    .distance(eye + look * distance);
                self.movables.holds.insert(
                    player,
                    Hold {
                        target,
                        distance,
                        force: force.unwrap_or(HOLD_FORCE),
                        anchor,
                        grip,
                        last_point: None,
                        lead: Vec3::ZERO,
                        last_yaw: None,
                        turning: 0.0,
                        caught: false,
                        closest,
                        stuck: 0,
                        alive: self.target_alive(target),
                    },
                );
                self.credit(target, player);
                Ok(())
            }
            Op::HoldDistance { player, distance } => {
                ensure!(
                    caller.is_none_or(|c| c == player),
                    "A player reels in only by their own command"
                );
                if let Some(hold) = self.movables.holds.get_mut(&player) {
                    hold.distance = distance;
                }
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
                        self.notify(
                            owner,
                            Notice::Center {
                                text: text.clone(),
                                seconds: 2.0,
                            },
                        );
                    }
                    anyhow::bail!("{}", text.trim_start_matches('\u{E000}'));
                }
                let transform = veh::Transform {
                    position,
                    rotation: glam::Quat::from_rotation_y(-yaw).to_array(),
                };
                let id = self
                    .spawn_transient(
                        owner.unwrap_or(0),
                        &definition,
                        transform,
                        Vec3::from(velocity),
                        1.0,
                    )
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
                let current = self.object_velocity(target).context("No such vehicle")?;
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

    /// The rigid body a held object moves as: a vehicle's, or the tumble
    /// of a knocked-down player. Players on their feet, corpses and
    /// entities are character bodies moved by velocity alone.
    fn held_body(&self, target: ObjectRef) -> Option<RigidBodyHandle> {
        let world = self.vehicles.world.as_ref()?;
        let vehicle = match target {
            ObjectRef::Vehicle(v) => VehicleId(v),
            ObjectRef::Player(p)
                if self.vehicles.mounted_family(p) == Some(veh::Family::Tumble) =>
            {
                self.ridden(p)?
            }
            _ => return None,
        };
        world.body_of(vehicle)
    }
    /// Whether a player target is alive (anything else counts as alive).
    fn target_alive(&self, target: ObjectRef) -> bool {
        match target {
            ObjectRef::Player(p) => self.peers.get(&p).is_some_and(|v| v.combat.alive),
            _ => true,
        }
    }
    /// The vehicle carrying a held object, whose tumble must not settle.
    fn held_vehicle(&self, target: ObjectRef) -> Option<VehicleId> {
        match target {
            ObjectRef::Vehicle(v) => Some(VehicleId(v)),
            ObjectRef::Player(p) => self.ridden(p),
            ObjectRef::Entity(_) => None,
        }
    }
    /// Where the point a hold grips is now.
    fn hold_point_of(&self, target: ObjectRef, anchor: Vec3) -> Option<Vec3> {
        match self.held_body(target) {
            Some(body) => {
                let b = self.simulation.physics.bodies.get(body)?;
                Some(b.center_of_mass() + b.position().rotation * anchor)
            }
            None => self.object_centre(target),
        }
    }
    /// The collider tag a held object's body carries.
    fn object_tag(&self, target: ObjectRef) -> Option<u128> {
        match target {
            ObjectRef::Entity(e) => Some((3_u128 << 64) | u128::from(e)),
            _ => self
                .held_vehicle(target)
                .map(|v| super::vehicles::VEHICLE_TAG | u128::from(v.0))
                .or(match target {
                    ObjectRef::Player(p) => Some((1_u128 << 64) | u128::from(p)),
                    _ => None,
                }),
        }
    }
    /// Whether `player` stands on `target`: then holding it would lift
    /// them with it, so the hold lets go.
    fn standing_on(&self, player: OwnerId, target: ObjectRef) -> bool {
        let (Some(peer), Some(tag)) = (self.peers.get(&player), self.object_tag(target)) else {
            return false;
        };
        let feet = Vec3::from(peer.player.state().feet);
        let predicate = |_: ColliderHandle, c: &Collider| c.user_data == tag;
        let shape = Ball::new(0.3);
        self.simulation
            .physics
            .query_pipeline_with_filter(QueryFilter::default().predicate(&predicate))
            .cast_shape(
                &Pose::from_translation(feet + Vec3::Y * 0.4),
                -Vec3::Y * 0.35,
                &shape,
                ShapeCastOptions {
                    max_time_of_impact: 1.0,
                    stop_at_penetration: true,
                    ..Default::default()
                },
            )
            .is_some()
    }

    /// Carry every held object to where its holder looks. Runs after the
    /// players move and before the physics step.
    ///
    /// Each tick the hold sets the velocity that takes its point to the
    /// hold point: the point's own motion (so it keeps up with turning and
    /// running) plus a closing speed that shrinks the gap by a fixed
    /// fraction a second, never faster than it could stop from, so it
    /// settles without wobbling. It carries the object's weight, and the
    /// change is limited by its force over the object's mass: light things
    /// snap into place and heavy ones swing in slowly, but none overshoot.
    /// A held body keeps its turn relative to the holder's heading.
    pub(super) fn step_holds(&mut self) {
        let tick = self.simulation.state().tick;
        self.movables.credits.retain(|_, (_, until)| *until >= tick);
        let recheck = tick.is_multiple_of(HOLD_RECHECK);
        let holds: Vec<(OwnerId, Hold)> =
            self.movables.holds.iter().map(|(p, h)| (*p, *h)).collect();
        for (player, mut hold) in holds {
            let holder = self.peers.get(&player).filter(|p| p.combat.alive).map(|p| {
                let state = p.player.state();
                (
                    p.player.eye(),
                    state.forward(),
                    Vec3::from(state.feet),
                    state.yaw,
                )
            });
            // A player the hold carries whose tumble ended goes limp again.
            if let ObjectRef::Player(p) = hold.target
                && hold.alive
                && self.peers.get(&p).is_some_and(|v| v.combat.alive)
                && self.ridden(p).is_none()
            {
                let velocity = self.object_velocity(hold.target).unwrap_or_default();
                let _ = self.tumble_player(p, velocity);
            }
            let keep = holder.is_some()
                && self.target_alive(hold.target) == hold.alive
                && !self.seated(player)
                && (!recheck || self.may_move(player, hold.target))
                && !self.standing_on(player, hold.target);
            let at = self.hold_point_of(hold.target, hold.anchor);
            let (Some((eye, look, feet, yaw)), Some(at), true) = (holder, at, keep) else {
                self.movables.holds.remove(&player);
                continue;
            };
            let radius = self.object_radius(hold.target);
            let point = hold_point(eye, look, feet, hold.distance, radius);
            // How fast the point moves: turning, walking, looking about.
            if let Some(last) = hold.last_point {
                let moved = (point - last) / DT;
                let moved = if moved.length() > HOLD_CARRY * 2.0 {
                    Vec3::ZERO
                } else {
                    moved
                };
                hold.lead = hold.lead.lerp(moved, 0.5);
            }
            hold.last_point = Some(point);
            if let Some(last) = hold.last_yaw {
                let turned = (yaw - last + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                    - std::f32::consts::PI;
                hold.turning = hold.turning * 0.5 + turned / DT * 0.5;
            }
            hold.last_yaw = Some(yaw);
            // Aim for where the point will be when this tick's step ends.
            let offset = point + hold.lead * DT - at;
            let gap = offset.length();
            if gap > HOLD_BREAK {
                self.movables.holds.remove(&player);
                continue;
            }
            // Pulled in, it must keep coming; caught, it must stay close.
            if !hold.caught {
                if gap < hold.closest - 0.05 {
                    hold.closest = gap;
                    hold.stuck = 0;
                } else {
                    hold.stuck += 1;
                }
                if gap < (0.25 * radius).max(0.75) {
                    hold.caught = true;
                    hold.stuck = 0;
                }
                if hold.stuck > PULL_STALL_TICKS {
                    self.movables.holds.remove(&player);
                    continue;
                }
            } else {
                hold.stuck = if gap > SNAG_DISTANCE + 0.5 * radius {
                    hold.stuck + 1
                } else {
                    0
                };
                if hold.stuck > SNAG_TICKS {
                    self.movables.holds.remove(&player);
                    continue;
                }
            }
            let mass = self.object_mass(hold.target).max(1.0);
            let accel = (hold.force / mass).min(MAX_HOLD_ACCEL);
            let closing = (gap * HOLD_GAIN)
                .min((1.6 * accel * gap).sqrt())
                .min(HOLD_SPEED);
            let wanted =
                (hold.lead + offset.normalize_or_zero() * closing).clamp_length_max(HOLD_CARRY);
            let current = self.object_velocity(hold.target).unwrap_or_default();
            let change = (wanted - current + Vec3::Y * GRAVITY * DT).clamp_length_max(accel * DT);
            let _ = self.push_object(hold.target, change);
            if let Some(body) = self.held_body(hold.target) {
                let heading = Quat::from_rotation_y(-yaw);
                let spin_accel = accel / radius.max(0.5);
                if let Some(b) = self.simulation.physics.bodies.get_mut(body) {
                    let spin = b.angvel();
                    let wanted = match hold.grip {
                        Some(grip) => {
                            let mut error = (heading * grip) * b.position().rotation.inverse();
                            if error.w < 0.0 {
                                error = -error;
                            }
                            let (axis, angle) = error.normalize().to_axis_angle();
                            // Turning with the holder, and back to its grip.
                            (Vec3::NEG_Y * hold.turning + axis * angle * TURN_GAIN)
                                .clamp_length_max(TURN_SPEED)
                        }
                        // Held things steady instead of spinning.
                        None => spin * 0.9,
                    };
                    b.set_angvel(
                        spin + (wanted - spin).clamp_length_max(spin_accel * DT),
                        true,
                    );
                }
            }
            self.credit(hold.target, player);
            if let Some(h) = self.movables.holds.get_mut(&player) {
                *h = hold;
            }
        }
        let held: Vec<VehicleId> = self
            .movables
            .holds
            .values()
            .filter_map(|h| self.held_vehicle(h.target))
            .collect();
        if let Some(world) = &mut self.vehicles.world {
            world.set_held(held);
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
    /// How far an object reaches from its middle, for keeping it clear of
    /// its holder: a vehicle's box, a body's width.
    fn object_radius(&self, target: ObjectRef) -> f32 {
        let vehicle = match target {
            ObjectRef::Vehicle(v) => Some(VehicleId(v)),
            ObjectRef::Player(p) => self.ridden(p),
            ObjectRef::Entity(_) => None,
        };
        vehicle
            .and_then(|v| {
                let world = self.vehicles.world.as_ref()?;
                let d = world.definition_of(v)?;
                let size = Vec3::from(d.bounds_max) - Vec3::from(d.bounds_min);
                Some(size.length() * 0.5)
            })
            .unwrap_or(PLAYER_CENTRE)
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
        if !self.peers.contains_key(&source) || !self.simulation.state().bricks.contains_key(&brick)
        {
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
        ensure!(
            position.is_finite() && yaw.is_finite() && velocity.is_finite(),
            "Invalid vehicle spawn"
        );
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

/// Where a hold floats its object: `distance` along the holder's look,
/// but never inside the holder, so looking down sets it before their feet
/// instead of pulling it into them.
fn hold_point(eye: Vec3, look: Vec3, feet: Vec3, distance: f32, radius: f32) -> Vec3 {
    let mut point = eye + look * distance;
    let clear = PLAYER_HALF_WIDTH + 0.7 * radius;
    let flat = Vec3::new(point.x - feet.x, 0.0, point.z - feet.z);
    let below_head = point.y < eye.y + 0.5 + 0.7 * radius;
    if below_head && flat.length() < clear {
        let ahead = Vec3::new(look.x, 0.0, look.z)
            .try_normalize()
            .or_else(|| flat.try_normalize())
            .unwrap_or(Vec3::NEG_Z);
        point.x = feet.x + ahead.x * clear;
        point.z = feet.z + ahead.z * clear;
    }
    point
}
