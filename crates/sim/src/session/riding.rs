//! Players riding rideable players: `Armor::onCollision`'s player branch.
//!
//! A player whose archetype `can_ride` lands on top of a living player whose
//! archetype is `rideable` with mount points (a Horse-Ray horse, or any
//! package body that declares seats) and takes its first free seat. The
//! mount keeps its own controls; riders sit on its mount points, look around
//! and use items, and leave with jet. A bot mount (no controlling client) is
//! steered by the rider in its first seat, as v20's `setControlObject` did.
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
}
impl Riding {
    pub(super) fn is_riding(&self, owner: OwnerId) -> bool {
        self.seats.contains_key(&owner)
    }
    fn riders_of(&self, mount: OwnerId) -> Vec<(OwnerId, u8)> {
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

impl Session {
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
            self.dismount_player(rider, false);
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
            if let Some(peer) = self.peers.get_mut(&rider) {
                peer.player.place(
                    &mut self.simulation.physics,
                    position,
                    state.yaw,
                    Vec3::from(state.velocity),
                );
            }
        }
    }
    /// `Armor::doDismount`: the rider leaves 2.2 up (along their own up),
    /// else 3 up, 3 down or 3 to either side (times the mount's scale),
    /// wherever their box fits, moving at the mount's velocity plus that
    /// offset. Blocked everywhere, a voluntary dismount stays seated and a
    /// forced one leaves in place.
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
            self.riding.seats.remove(&rider);
            self.riding.jet_held.remove(&rider);
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
        if exit.is_none() && !forced {
            return;
        }
        let offset = exit.unwrap_or(Vec3::ZERO);
        self.riding.seats.remove(&rider);
        self.riding.jet_held.remove(&rider);
        self.vehicles
            .note_dismount(rider, self.simulation.state().tick);
        if let Some(peer) = self.peers.get_mut(&rider) {
            let yaw = peer.player.state().yaw;
            if peer
                .player
                .teleport(&mut self.simulation.physics, start + offset, yaw)
                .is_ok()
            {
                // `setVelocity(%vehicleVelocity)` then an impulse of the
                // offset times the rider's mass.
                peer.player.push(velocity + offset);
            }
            peer.player
                .set_solid(&mut self.simulation.physics, peer.combat.alive);
            peer.inputs.clear();
        }
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
        let seats = if kind.rideable {
            kind.mount_points.len()
        } else {
            0
        };
        for (rider, seat) in self.riding.riders_of(mount) {
            if usize::from(seat) >= seats {
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
