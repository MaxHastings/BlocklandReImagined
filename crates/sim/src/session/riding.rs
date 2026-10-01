//! Players riding rideable players: `Armor::onCollision`'s player branch.
//!
//! A player whose archetype `can_ride` lands on top of a living player whose
//! archetype is `rideable` with mount points (a Horse-Ray horse, or any
//! package body that declares seats) and takes its first free seat. The
//! mount keeps its own controls; riders sit on its mount points, look around
//! and use items, and leave with jet. A bot mount (no controlling client) is
//! steered by the rider in its first seat, as v20's `setControlObject` did.
//!
//! Rules also seat players and bots on any body's mount points
//! (`mountObject`), rideable or not: a Blockhead carrying another in its
//! arms. Landing on a body that is not rideable never boards it.
use super::*;
use rapier3d::parry::query::ShapeCastOptions;
use rapier3d::prelude::*;

/// `Armor::onCollision`: the rider's feet must be this far over the mount's.
const MOUNT_ABOVE: f32 = 0.2;
/// Players never quite touch: each closes half its gap to another per tick
/// (`bri_motor` player pushing), so a rider landing on a moving mount stops
/// a little above it. Boxes this close count as a collision.
const CONTACT: f32 = 0.1;

/// Replicated seat of a player riding another player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ride {
    pub mount: OwnerId,
    /// Index into the mount archetype's mount points.
    pub seat: u8,
    /// The rider steers the mount (first seat of a bot mount).
    pub steers: bool,
}

#[derive(Default)]
pub(super) struct Riding {
    /// Rider to mount and seat.
    seats: BTreeMap<OwnerId, (OwnerId, u8)>,
    /// Jet held last input: a new press gets off (`doDismount`).
    jet_held: BTreeMap<OwnerId, bool>,
    /// A passenger's body turn on the seat (`mRot.z`): their move's yaw.
    turn: BTreeMap<OwnerId, f32>,
    /// The world yaw a rider's moves carried when they mounted, which is
    /// not a turn (their client did not yet know it was seated).
    mount_yaw: BTreeMap<OwnerId, f32>,
    /// Riders a rule seated (`mountObject`): they need only their mount
    /// point, not a rideable body.
    scripted: BTreeSet<OwnerId>,
    /// Riders who may not jump off (`canDismount = 0`).
    locked: BTreeSet<OwnerId>,
}
impl Riding {
    pub(super) fn is_riding(&self, owner: OwnerId) -> bool {
        self.seats.contains_key(&owner)
    }
    pub(super) fn riders_of(&self, mount: OwnerId) -> Vec<(OwnerId, u8)> {
        self.seats
            .iter()
            .filter(|(_, (m, _))| *m == mount)
            .map(|(rider, (_, seat))| (*rider, *seat))
            .collect()
    }
    fn taken(&self, mount: OwnerId, seat: u8) -> bool {
        self.seats.values().any(|s| *s == (mount, seat))
    }
    /// The rider in a mount's first seat.
    pub(super) fn driver_of(&self, mount: OwnerId) -> Option<OwnerId> {
        self.seats
            .iter()
            .find(|(_, s)| **s == (mount, 0))
            .map(|(rider, _)| *rider)
    }
}

/// A model's mount points (`numMountPoints`): its `mount0`, `mount1`, ...
/// nodes in order, as Torque counts them, at the model's rest pose, from
/// the feet facing -Z. They stop at the first number the model lacks, or
/// at [`crate::archetype::MAX_MOUNT_POINTS`].
pub fn shape_mount_points(shape: &bri_content::shape::Shape) -> Vec<crate::archetype::MountPoint> {
    let rest = |mut i: usize| {
        let mut transform = glam::Mat4::IDENTITY;
        for _ in 0..shape.nodes.len() {
            let n = &shape.nodes[i];
            transform = glam::Mat4::from_rotation_translation(
                glam::Quat::from_array(n.rotation).normalize(),
                Vec3::from(n.translation),
            ) * transform;
            match n.parent {
                Some(p) if p < shape.nodes.len() => i = p,
                _ => break,
            }
        }
        transform.w_axis.truncate()
    };
    let mut points = Vec::new();
    for number in 0..crate::archetype::MAX_MOUNT_POINTS {
        let name = format!("mount{number}");
        let Some(index) = shape
            .nodes
            .iter()
            .position(|n| n.name.eq_ignore_ascii_case(&name))
        else {
            break;
        };
        let position = rest(index);
        if !position.is_finite() {
            break;
        }
        points.push(crate::archetype::MountPoint {
            node: shape.nodes[index].name.clone(),
            position: position.to_array(),
            pose: "root".into(),
        });
    }
    points
}

impl Session {
    /// Bodies drawn with `model` (`v20.shape.m`, the Blockhead) that
    /// declare no mount points get these, derived from the model
    /// ([`shape_mount_points`]): rules seat riders on them
    /// (`mount_object`), but nobody boards them by landing on them unless
    /// they are rideable. Set before players join.
    pub fn set_body_mount_points(
        &mut self,
        model: &str,
        points: Vec<crate::archetype::MountPoint>,
    ) -> Result<()> {
        ensure!(
            self.peers.is_empty(),
            "Mount points are set before players join"
        );
        self.archetypes.fill_mount_points(model, &points)?;
        self.body_mounts.insert(model.to_owned(), points);
        Ok(())
    }
    /// Reapply [`Self::set_body_mount_points`] to a rebuilt archetype table.
    pub(super) fn fill_body_mount_points(&mut self) -> Result<()> {
        for (model, points) in &self.body_mounts {
            self.archetypes.fill_mount_points(model, points)?;
        }
        Ok(())
    }
    /// The player this one rides and the mount point.
    pub(super) fn riding_seat(&self, owner: OwnerId) -> Option<(OwnerId, u8)> {
        self.riding.seats.get(&owner).copied()
    }
    /// The player this one rides, and where.
    pub fn ride(&self, owner: OwnerId) -> Option<Ride> {
        let (mount, seat) = *self.riding.seats.get(&owner)?;
        Some(Ride {
            mount,
            seat,
            steers: seat == 0 && self.bots.is_bot(mount),
        })
    }
    /// Seated on a vehicle or on another player: the walking motor is off.
    pub(super) fn seated(&self, owner: OwnerId) -> bool {
        self.vehicles.is_mounted(owner) || self.riding.is_riding(owner)
    }
    /// `Armor::onCollision`'s use check. A bot on a spawn brick asks the
    /// brick owner's trust and minigame, as its vehicles do. A player has no
    /// spawn brick, so only `miniGameCanUse` counts: anyone may ride outside
    /// minigames, and inside only players of the same minigame.
    fn may_ride_player(&self, rider: OwnerId, mount: OwnerId) -> bool {
        if let Some(owner) = self.bot_brick_owner(mount) {
            return self.can_ride(rider, owner);
        }
        self.game_of(rider) == self.game_of(mount)
    }
    /// Riders landing on top of a rideable player take its first free seat
    /// (`$Game::MinMountTime` after leaving any mount).
    pub(super) fn player_mount_contacts(&mut self) {
        let tick = self.simulation.state().tick;
        let mut boarding = Vec::new();
        for (&rider, peer) in &self.peers {
            if !peer.combat.alive
                || self.seated(rider)
                || self.bots.is_bot(rider)
                || !self
                    .archetypes
                    .resolve(peer.player.state().archetype)
                    .can_ride
                || !self.vehicles.may_remount(rider, tick)
            {
                continue;
            }
            let (min, max) = peer.player.world_bounds();
            let (min, max) = (
                Vec3::from(min) - Vec3::splat(CONTACT),
                Vec3::from(max) + Vec3::splat(CONTACT),
            );
            let feet = Vec3::from(peer.player.state().feet);
            for (&mount, other) in &self.peers {
                if mount == rider || !other.combat.alive || self.riding.is_riding(mount) {
                    continue;
                }
                let state = other.player.state();
                let kind = self.archetypes.resolve(state.archetype);
                if !kind.rideable || kind.mount_points.is_empty() {
                    continue;
                }
                let (omin, omax) = other.player.world_bounds();
                let touching = (0..3).all(|i| min[i] <= omax[i] && omin[i] <= max[i]);
                if !touching || feet.y <= state.feet[1] + MOUNT_ABOVE {
                    continue;
                }
                if !self.may_ride_player(rider, mount) {
                    continue;
                }
                boarding.push((rider, mount, kind.mount_points.len()));
                break;
            }
        }
        for (rider, mount, seats) in boarding {
            let Some(seat) = (0..seats as u8).find(|s| !self.riding.taken(mount, *s)) else {
                continue;
            };
            self.seat_rider(rider, mount, seat);
        }
    }
    fn seat_rider(&mut self, rider: OwnerId, mount: OwnerId, seat: u8) {
        let tick = self.simulation.state().tick;
        let Some(peer) = self.peers.get_mut(&rider) else {
            return;
        };
        peer.player.set_solid(&mut self.simulation.physics, false);
        peer.sitting = false;
        let feet = peer.player.state().feet;
        // `Armor::onMount` resets the transform: facing the seat.
        self.riding.turn.remove(&rider);
        self.riding.mount_yaw.insert(rider, peer.input.yaw);
        self.riding.seats.insert(rider, (mount, seat));
        // A jet held while landing does not throw the rider straight off.
        self.riding.jet_held.insert(rider, true);
        self.follow_player_mounts();
        // `Armor::onMount`: ServerPlay3D(playerMountSound, %obj.getPosition()).
        self.cues.emit(
            tick,
            crate::presentation::CueKind::VehicleSound {
                vehicle: 0,
                sound: "player.mount".into(),
            },
            feet,
        );
    }
    /// A rider's move: jet gets off, the first seat of a bot mount steers it,
    /// and everything else only turns the rider's head.
    pub(super) fn ride_input(&mut self, rider: OwnerId, input: MoveInput) -> Result<()> {
        let Some(&(mount, seat)) = self.riding.seats.get(&rider) else {
            return Ok(());
        };
        let was_held = self
            .riding
            .jet_held
            .insert(rider, input.jet)
            .unwrap_or(true);
        if input.jet && !was_held {
            // `canDismount = 0`: `Armor::onTrigger` ignores the jump.
            if !self.riding.locked.contains(&rider) {
                self.dismount_player(rider, false);
            }
            return Ok(());
        }
        if seat == 0 && self.bots.is_bot(mount) {
            self.drive_bot(
                mount,
                MoveInput {
                    jet: false,
                    ..input
                },
            )?;
        } else if self.riding.mount_yaw.get(&rider) != Some(&input.yaw) && input.yaw.is_finite() {
            // No control object: the turn reaches `mRot.z` (0x5aeacd), sent
            // relative to the seat, and `Player::setPosition` turns the body.
            self.riding.mount_yaw.remove(&rider);
            let turn = (input.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            self.riding.turn.insert(rider, turn);
        }
        Ok(())
    }
    /// Riders sit on their mount's seats, moving and facing with it.
    pub(super) fn follow_player_mounts(&mut self) {
        let seats: Vec<_> = self
            .riding
            .seats
            .iter()
            .map(|(r, (m, s))| (*r, *m, *s))
            .collect();
        for (rider, mount, seat) in seats {
            let Some(state) = self.peers.get(&mount).map(|p| p.player.state().clone()) else {
                continue;
            };
            let Some(point) = self
                .archetypes
                .resolve(state.archetype)
                .mount_points
                .get(usize::from(seat))
                .cloned()
            else {
                continue;
            };
            let position = point.seat(Vec3::from(state.feet), state.yaw, state.scale);
            let steers = seat == 0 && self.bots.is_bot(mount);
            let turn = if steers {
                0.0
            } else {
                self.riding.turn.get(&rider).copied().unwrap_or(0.0)
            };
            if let Some(peer) = self.peers.get_mut(&rider) {
                peer.player.place(
                    &mut self.simulation.physics,
                    position,
                    state.yaw + turn,
                    Vec3::from(state.velocity),
                );
            }
        }
    }
    /// `Armor::doDismount`: the rider leaves 2.2 up (along their own up),
    /// else 3 up, 3 down or 3 to either side (times the mount's scale),
    /// wherever their box fits, moving at the mount's velocity plus that
    /// offset. Blocked everywhere, a voluntary dismount still gets out, at
    /// the last point tried and without the push; a forced one leaves in
    /// place.
    pub(super) fn dismount_player(&mut self, rider: OwnerId, forced: bool) {
        let Some(&(mount, _)) = self.riding.seats.get(&rider) else {
            return;
        };
        let (scale, velocity, mount_collider) =
            self.peers.get(&mount).map_or((1.0, Vec3::ZERO, None), |p| {
                let s = p.player.state();
                (s.scale, Vec3::from(s.velocity), Some(p.player.collider()))
            });
        let Some(peer) = self.peers.get(&rider) else {
            self.forget_ride(rider);
            return;
        };
        let start = Vec3::from(peer.player.state().feet);
        let tuning = peer.player.tuning();
        let body = [tuning.width, tuning.stand_height];
        let own = peer.player.collider();
        let exempt =
            |handle: ColliderHandle, _: &Collider| handle != own && Some(handle) != mount_collider;
        let queries = self.simulation.physics.query_pipeline_with_filter(
            QueryFilter::default().exclude_sensors().predicate(&exempt),
        );
        let offsets = [
            Vec3::Y * 2.2,
            Vec3::Y * 3.0,
            Vec3::NEG_Y * 3.0,
            Vec3::X * 3.0,
            Vec3::NEG_X * 3.0,
        ];
        let exit = offsets
            .into_iter()
            .map(|offset| offset * scale)
            .find(|offset| exit_clear(&queries, start, *offset, body));
        let (place, push) = match exit {
            Some(offset) => (offset, offset),
            None if forced => (Vec3::ZERO, Vec3::ZERO),
            None => (offsets[4] * scale, Vec3::ZERO),
        };
        self.forget_ride(rider);
        self.vehicles
            .note_dismount(rider, self.simulation.state().tick);
        if let Some(peer) = self.peers.get_mut(&rider) {
            let yaw = peer.player.state().yaw;
            if peer
                .player
                .teleport(&mut self.simulation.physics, start + place, yaw)
                .is_ok()
            {
                // `setVelocity(%vehicleVelocity)` then an impulse of the
                // offset times the rider's mass.
                peer.player.push(velocity + push);
            }
            peer.player
                .set_solid(&mut self.simulation.physics, peer.combat.alive);
            peer.inputs.clear();
        }
    }
    fn forget_ride(&mut self, rider: OwnerId) {
        self.riding.seats.remove(&rider);
        self.riding.jet_held.remove(&rider);
        self.riding.turn.remove(&rider);
        self.riding.mount_yaw.remove(&rider);
        self.riding.scripted.remove(&rider);
        self.riding.locked.remove(&rider);
    }
    /// `%mount.mountObject(%rider, %node)` from a rule: `rider` (a player
    /// or bot) sits on `mount`'s mount point `node`, carried with it, not
    /// solid, and drawn on that node as it animates. The body need not be
    /// rideable. With `can_dismount` false the rider cannot jump off
    /// (`canDismount = 0`).
    pub(super) fn mount_player(
        &mut self,
        mount: OwnerId,
        rider: OwnerId,
        node: u8,
        can_dismount: bool,
    ) -> Result<()> {
        ensure!(mount != rider, "A player cannot mount themselves");
        let alive = |o: OwnerId| self.peers.get(&o).is_some_and(|p| p.combat.alive);
        ensure!(alive(mount) && alive(rider), "Only living players mount");
        ensure!(
            !self.seated(rider) && !self.seated(mount),
            "The rider or the mount is already seated"
        );
        ensure!(
            self.riding.riders_of(rider).is_empty(),
            "The rider carries riders of their own"
        );
        let kind = self
            .archetypes
            .resolve(self.peers[&mount].player.state().archetype);
        ensure!(
            usize::from(node) < kind.mount_points.len(),
            "The mount's body has no mount point {node}"
        );
        ensure!(
            !self.riding.taken(mount, node),
            "Mount point {node} is taken"
        );
        // A carried rider is no one's physics grip any more.
        self.release_holds_on(rider);
        self.riding.scripted.insert(rider);
        if !can_dismount {
            self.riding.locked.insert(rider);
        }
        self.seat_rider(rider, mount, node);
        Ok(())
    }
    /// `unMountObject` from a rule: the rider leaves the mount where they
    /// are, moving as it moved, solid again.
    pub(super) fn unmount_player(&mut self, rider: OwnerId) -> Result<()> {
        let &(mount, _) = self
            .riding
            .seats
            .get(&rider)
            .with_context(|| format!("Player {rider} rides no one"))?;
        let velocity = self
            .peers
            .get(&mount)
            .map_or(Vec3::ZERO, |p| Vec3::from(p.player.state().velocity));
        self.forget_ride(rider);
        self.vehicles
            .note_dismount(rider, self.simulation.state().tick);
        if let Some(peer) = self.peers.get_mut(&rider) {
            peer.player.push(velocity);
            peer.player
                .set_solid(&mut self.simulation.physics, peer.combat.alive);
            peer.inputs.clear();
        }
        Ok(())
    }
    /// `Armor::onDisabled` and disconnects: every rider is forced off.
    pub(super) fn release_riders(&mut self, mount: OwnerId) {
        for (rider, _) in self.riding.riders_of(mount) {
            self.dismount_player(rider, true);
        }
    }
    /// `Armor::onNewDataBlock`: riders get off a mount whose new body is not
    /// rideable or has fewer seats than theirs; the rest ride on.
    pub(super) fn reseat_riders(&mut self, mount: OwnerId) {
        let Some(state) = self.peers.get(&mount).map(|p| p.player.state()) else {
            return;
        };
        let kind = self.archetypes.resolve(state.archetype);
        let points = kind.mount_points.len();
        let seats = if kind.rideable { points } else { 0 };
        for (rider, seat) in self.riding.riders_of(mount) {
            // A rule's rider needs only its mount point.
            let limit = if self.riding.scripted.contains(&rider) {
                points
            } else {
                seats
            };
            if usize::from(seat) >= limit {
                self.dismount_player(rider, true);
            }
        }
        self.follow_player_mounts();
    }
}

/// `checkDismountPoint`: the rider's box fits at the exit and nothing lies
/// between the seat and it.
fn exit_clear(queries: &QueryPipeline, start: Vec3, offset: Vec3, body: [f32; 2]) -> bool {
    let [width, height] = body;
    let shape = Cuboid::new(Vec3::new(width * 0.5, height * 0.5, width * 0.5));
    let centre = start + Vec3::Y * (height * 0.5);
    let dst = centre + offset;
    queries
        .intersect_shape(Pose::translation(dst.x, dst.y, dst.z), &shape)
        .next()
        .is_none()
        && queries
            .cast_shape(
                &Pose::translation(centre.x, centre.y, centre.z),
                offset,
                &shape,
                ShapeCastOptions {
                    max_time_of_impact: 1.0,
                    stop_at_penetration: false,
                    ..Default::default()
                },
            )
            .is_none()
}
