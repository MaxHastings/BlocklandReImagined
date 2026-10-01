//! Which state datagrams each player gets at a pose interval.
//!
//! Every player, vehicle and admin camera used to go to every peer 40 times
//! a second, standing still or not: eight idle players cost the host about
//! 650 KB/s. Now an item goes out while it changes, for a few intervals
//! after it settles (so one lost datagram never leaves a stale resting
//! pose), then once a second. A player's own pose, which acknowledges their
//! inputs, goes to them at 10 Hz while it is still.
//!
//! Other players' poses also go out less often the further they are from
//! each viewer (Torque's ghost priority by distance; Unreal's net update
//! frequency): 40 Hz within [`NEAR`], halving with each doubling of the
//! distance, down to 5 Hz. Without it every moving player went to every
//! other at 40 Hz, and the host's upload grew with the square of the player
//! count: 2.1 MB/s (17 Mbit/s) for 32 players running. Clients render each
//! remote player far enough in the past for its own rate.
use crate::protocol::{Datagram, Orb, POSE_INTERVAL, Pose, RemotePose, WeaponDelta};
use bri_sim::session::WeaponView;
use bri_sim::{player::PlayerState, session::VehiclePose};
use bri_weapons::Projectile;
use bri_world::OwnerId;
use std::cmp::Ordering;
use std::collections::BTreeMap;

/// Unchanged intervals still sent after the last change.
const SETTLE: u32 = 3;
/// Most ticks between two sends of an unchanged item (one second).
const KEEPALIVE: u64 = 120;
/// Ticks between a still player's own poses (10 Hz): acknowledgements well
/// inside the predictor's two seconds of input history.
const OWN_IDLE: u64 = POSE_INTERVAL * 4;
/// Differences no one can see: a millimetre, a ten-thousandth of a radian.
const DISTANCE: f32 = 1e-3;
const ANGLE: f32 = 1e-4;
/// Jet energy others never see; the owner's own stream still carries it.
const ENERGY: f32 = 0.5;

/// Distance within which other players' poses go out at the full rate.
pub const NEAR: f32 = 32.0;
/// Slowest rate a moving player is sent at, far away: 5 Hz.
const FARTHEST_INTERVAL: u64 = POSE_INTERVAL * 8;
/// Ticks between two poses of a moving player `distance` from the viewer:
/// 40 Hz within NEAR, 20 Hz within twice that, 10 Hz within four times,
/// then 5 Hz.
pub fn pose_interval(distance: f32) -> u64 {
    // NaN (an unknown distance) is as far as it gets.
    let (mut interval, mut reach) = (POSE_INTERVAL, NEAR);
    let within = |reach: f32| {
        matches!(
            distance.partial_cmp(&reach),
            Some(Ordering::Less | Ordering::Equal)
        )
    };
    while !within(reach) && interval < FARTHEST_INTERVAL {
        interval *= 2;
        reach *= 2.0;
    }
    interval
}

/// Moving players a viewer gets at the rate their distance allows; the next
/// as many at no more than half that, the rest at no more than a quarter.
/// A crowd of runners around one player is where the host's upload peaks:
/// ranking by distance keeps the closest at full rate and bounds each
/// viewer's pose rate (Fortnite-style significance, Torque's priority).
pub const FULL_RATE_PLAYERS: usize = 12;
/// The pose interval a viewer gets a moving player at: by its distance
/// and by how many moving players are closer.
fn ranked_interval(distance: f32, rank: usize) -> u64 {
    let by_rank = POSE_INTERVAL << (rank / FULL_RATE_PLAYERS).min(2);
    pose_interval(distance).max(by_rank)
}

/// Who receives one item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Audience {
    All,
    Only(OwnerId),
    AllBut(OwnerId),
    /// These viewers, in ascending order.
    Listed(Vec<OwnerId>),
}
impl Audience {
    pub fn includes(&self, peer: OwnerId) -> bool {
        match self {
            Audience::All => true,
            Audience::Only(owner) => *owner == peer,
            Audience::AllBut(owner) => *owner != peer,
            Audience::Listed(viewers) => viewers.binary_search(&peer).is_ok(),
        }
    }
}

struct Sent<T> {
    state: T,
    tick: u64,
    quiet: u32,
}
/// What to send for one item this interval.
enum Decision<T> {
    Skip,
    Send,
    /// Send the last state again at the previous interval first: the item
    /// was held still and receivers interpolate from where it rested.
    HoldThenSend(T),
}
/// What to send of one item to one audience at `tick`, when it is due every
/// `interval` ticks while it moves.
fn decide<K: Ord, T: Clone>(
    sent: &mut BTreeMap<K, Sent<T>>,
    id: K,
    state: &T,
    tick: u64,
    same: impl Fn(&T, &T) -> bool,
    interval: u64,
) -> Decision<T> {
    let Some(last) = sent.get_mut(&id) else {
        sent.insert(
            id,
            Sent {
                state: state.clone(),
                tick,
                quiet: 0,
            },
        );
        return Decision::Send;
    };
    if tick < last.tick + interval {
        return Decision::Skip;
    }
    if same(&last.state, state) {
        last.quiet = last.quiet.saturating_add(1);
        if last.quiet > SETTLE && tick < last.tick + KEEPALIVE {
            return Decision::Skip;
        }
        last.state = state.clone();
        last.tick = tick;
        return Decision::Send;
    }
    // Still until now: the receiver's newest pose is older than one interval.
    let held = last.quiet > 0 && last.tick + interval < tick;
    let previous = std::mem::replace(&mut last.state, state.clone());
    last.tick = tick;
    last.quiet = 0;
    if held {
        Decision::HoldThenSend(previous)
    } else {
        Decision::Send
    }
}
fn near(a: &[f32], b: &[f32], tolerance: f32) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| (a - b).abs() <= tolerance)
}
/// Whether anyone could tell the two poses apart.
fn player_same(a: &PlayerState, b: &PlayerState) -> bool {
    a.owner == b.owner
        && near(&a.feet, &b.feet, DISTANCE)
        && near(&a.velocity, &b.velocity, DISTANCE)
        && near(
            &[a.yaw, a.pitch, a.head_yaw],
            &[b.yaw, b.pitch, b.head_yaw],
            ANGLE,
        )
        && (a.energy - b.energy).abs() <= ENERGY
        && a.grounded == b.grounded
        && a.crouched == b.crouched
        && a.jetting == b.jetting
        && a.archetype == b.archetype
        && a.scale == b.scale
        && a.tether == b.tether
}
fn vehicle_same(a: &VehiclePose, b: &VehiclePose) -> bool {
    near(&a.position, &b.position, DISTANCE)
        && near(&a.rotation, &b.rotation, ANGLE)
        && near(&a.velocity, &b.velocity, DISTANCE)
        && near(&[a.steering], &[b.steering], ANGLE)
        && near(&a.wheel_suspension, &b.wheel_suspension, DISTANCE)
        && near(&a.wheel_rotation, &b.wheel_rotation, ANGLE)
        && near(&a.turret_aim, &b.turret_aim, ANGLE)
        && a.wheel_contact == b.wheel_contact
        && a.jetting == b.jetting
        && a.driver_steering == b.driver_steering
}

/// The host's record of what it last sent of each item.
#[derive(Default)]
pub struct StateStream {
    /// Other players' poses as each viewer last got them, by (viewer, player).
    remote: BTreeMap<(OwnerId, OwnerId), Sent<PlayerState>>,
    own: BTreeMap<u64, Sent<Pose>>,
    /// Every player's pose at the previous interval, to tell who is moving.
    previous: BTreeMap<OwnerId, PlayerState>,
    vehicles: BTreeMap<u64, Sent<VehiclePose>>,
    orbs: BTreeMap<u64, Sent<[f32; 3]>>,
}
impl StateStream {
    /// The datagram items of the interval at `tick`, in send order, with
    /// who gets each. `viewers` are the connected players with where each
    /// sees from (None: anywhere, so full rate).
    pub fn interval(
        &mut self,
        tick: u64,
        poses: Vec<Pose>,
        vehicles: Vec<VehiclePose>,
        orbs: Vec<(OwnerId, [f32; 3])>,
        viewers: &[(OwnerId, Option<[f32; 3]>)],
    ) -> Vec<(Datagram, Audience)> {
        let mut out = Vec::new();
        let mut viewers = viewers.to_vec();
        viewers.sort_unstable_by_key(|(viewer, _)| *viewer);
        self.remote.retain(|(viewer, id), _| {
            poses.iter().any(|p| p.player.owner == *id)
                && viewers.binary_search_by_key(viewer, |(v, _)| *v).is_ok()
        });
        self.own
            .retain(|id, _| poses.iter().any(|p| p.player.owner == *id));
        self.vehicles
            .retain(|id, _| vehicles.iter().any(|v| v.id == *id));
        self.orbs.retain(|id, _| orbs.iter().any(|(o, _)| o == id));
        // Each viewer's pose interval for every other moving player.
        let moving: Vec<&Pose> = poses
            .iter()
            .filter(|p| {
                self.previous
                    .get(&p.player.owner)
                    .is_none_or(|last| !player_same(last, &p.player))
            })
            .collect();
        let mut intervals = BTreeMap::new();
        for (viewer, eye) in &viewers {
            let Some(eye) = eye else { continue };
            let mut near: Vec<(f32, OwnerId)> = moving
                .iter()
                .filter(|p| p.player.owner != *viewer)
                .map(|p| {
                    let feet = glam::Vec3::from(p.player.feet);
                    (glam::Vec3::from(*eye).distance(feet), p.player.owner)
                })
                .collect();
            near.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            for (rank, (distance, owner)) in near.into_iter().enumerate() {
                intervals.insert((*viewer, owner), ranked_interval(distance, rank));
            }
        }
        self.previous = poses
            .iter()
            .map(|p| (p.player.owner, p.player.clone()))
            .collect();
        for pose in poses {
            let owner = pose.player.owner;
            let mut listeners = Vec::new();
            for (viewer, eye) in &viewers {
                if *viewer == owner {
                    continue;
                }
                // Still players settle and keep alive at their distance's rate.
                let interval = match (intervals.get(&(*viewer, owner)), eye) {
                    (Some(interval), _) => *interval,
                    (None, Some(eye)) => pose_interval(
                        glam::Vec3::from(*eye).distance(glam::Vec3::from(pose.player.feet)),
                    ),
                    (None, None) => POSE_INTERVAL,
                };
                match decide(
                    &mut self.remote,
                    (*viewer, owner),
                    &pose.player,
                    tick,
                    player_same,
                    interval,
                ) {
                    Decision::Skip => continue,
                    Decision::Send => {}
                    Decision::HoldThenSend(state) => out.push((
                        Datagram::Remote(RemotePose::of(tick - interval, &state)),
                        Audience::Only(*viewer),
                    )),
                }
                listeners.push(*viewer);
            }
            // The owner's own stream: every change (a new body included),
            // and 10 Hz while still.
            let to_owner = match self.own.get_mut(&owner) {
                Some(last)
                    if player_same(&last.state.player, &pose.player)
                        && last.state.spawn_tick == pose.spawn_tick
                        && tick < last.tick + OWN_IDLE =>
                {
                    false
                }
                Some(last) => {
                    last.state = pose.clone();
                    last.tick = tick;
                    true
                }
                None => {
                    self.own.insert(
                        owner,
                        Sent {
                            state: pose.clone(),
                            tick,
                            quiet: 0,
                        },
                    );
                    true
                }
            };
            if !listeners.is_empty() {
                out.push((
                    Datagram::Remote(RemotePose::of(tick, &pose.player)),
                    Audience::Listed(listeners),
                ));
            }
            if to_owner {
                out.push((Datagram::Pose(pose), Audience::Only(owner)));
            }
        }
        for vehicle in vehicles {
            match decide(
                &mut self.vehicles,
                vehicle.id,
                &vehicle,
                tick,
                vehicle_same,
                POSE_INTERVAL,
            ) {
                Decision::Skip => continue,
                Decision::Send => {}
                Decision::HoldThenSend(held) => out.push((
                    Datagram::Vehicle(VehiclePose {
                        tick: tick - POSE_INTERVAL,
                        ..held
                    }),
                    Audience::All,
                )),
            }
            out.push((Datagram::Vehicle(vehicle), Audience::All));
        }
        for (owner, eye) in orbs {
            match decide(
                &mut self.orbs,
                owner,
                &eye,
                tick,
                |a, b| near(a, b, DISTANCE),
                POSE_INTERVAL,
            ) {
                Decision::Skip => continue,
                Decision::Send => {}
                Decision::HoldThenSend(held) => out.push((
                    Datagram::Orb(Orb {
                        tick: tick - POSE_INTERVAL,
                        owner,
                        eye: held,
                    }),
                    Audience::All,
                )),
            }
            out.push((Datagram::Orb(Orb { tick, owner, eye }), Audience::All));
        }
        out
    }
}

/// The entries of `current` that differ from `last`, which becomes
/// `current`: per-player state sent only for the players it changed for.
pub fn changed_entries<V: Clone + PartialEq>(
    last: &mut BTreeMap<OwnerId, V>,
    current: BTreeMap<OwnerId, V>,
) -> BTreeMap<OwnerId, V> {
    let changed = current
        .iter()
        .filter(|(id, v)| last.get(id) != Some(v))
        .map(|(id, v)| (*id, v.clone()))
        .collect();
    *last = current;
    changed
}

/// How far a client's coasted projectile may drift from the host's before
/// the host corrects it.
const PROJECTILE_DRIFT: f32 = 1e-3;
/// The host's copy of the weapons view as clients have it: projectiles
/// coasted exactly as each client coasts them.
#[derive(Default)]
pub struct WeaponStream {
    sent: WeaponView,
    tick: u64,
    falls: BTreeMap<String, f32>,
    /// Views handed to players who joined since the last update. A joiner
    /// holds what the host had then, which may differ from `sent`: a
    /// projectile that came and went between two updates, say.
    joined: Vec<WeaponView>,
}
impl WeaponStream {
    /// Every client starts again from `view` (a new map's checkpoint).
    pub fn reset(&mut self, view: WeaponView, tick: u64, falls: BTreeMap<String, f32>) {
        *self = Self {
            sent: view,
            tick,
            falls,
            joined: Vec::new(),
        };
    }
    /// A player joined holding `view`; the next update brings it in line.
    pub fn joined(&mut self, view: &WeaponView) {
        self.joined.push(view.clone());
    }
    /// Projectiles clients are flying.
    pub fn in_flight(&self) -> bool {
        !self.sent.projectiles.is_empty()
    }
    /// What an update sent at `tick` must say for clients to match `current`.
    /// Call once per update actually sent.
    pub fn delta(&mut self, current: &WeaponView, tick: u64) -> Option<WeaponDelta> {
        crate::protocol::coast_projectiles(
            &mut self.sent,
            &self.falls,
            tick.saturating_sub(self.tick),
        );
        self.tick = tick;
        let joined = std::mem::take(&mut self.joined);
        let held = || std::iter::once(&self.sent).chain(&joined);
        let mut delta = WeaponDelta::default();
        if held().any(|v| v.static_items != current.static_items) {
            delta.static_items = Some(current.static_items.clone());
        }
        for (owner, images) in &current.images {
            if held().any(|v| v.images.get(owner) != Some(images)) {
                delta.images.insert(*owner, images.clone());
            }
        }
        for owner in held().flat_map(|v| v.images.keys()) {
            if !current.images.contains_key(owner) {
                delta.images.insert(*owner, Vec::new());
            }
        }
        let coasted: BTreeMap<u64, &Projectile> =
            self.sent.projectiles.iter().map(|p| (p.id, p)).collect();
        for p in &current.projectiles {
            if coasted
                .get(&p.id)
                .is_none_or(|sent| !projectile_same(sent, p))
            {
                delta.projectiles.push(p.clone());
            }
        }
        let live: std::collections::BTreeSet<u64> =
            current.projectiles.iter().map(|p| p.id).collect();
        let held_ids: std::collections::BTreeSet<u64> = held()
            .flat_map(|v| v.projectiles.iter().map(|p| p.id))
            .collect();
        delta.removed = held_ids.difference(&live).copied().collect();
        if held().any(|v| v.drops != current.drops) {
            delta.drops = Some(current.drops.clone());
        }
        if delta == WeaponDelta::default() {
            return None;
        }
        // One update carries at most what a client accepts (players joining
        // and leaving inside one update can touch more owners than a server
        // holds at once). The rest stays different from `sent`, so the next
        // update carries it; a joiner's view is compared again until then.
        let deferred = delta.clamp_to_wire_limits();
        if deferred {
            self.joined = joined;
        }
        // Keep the coasted copies clients have; take the host's for the rest.
        if let Err(error) = delta.apply(&mut self.sent) {
            // Unreachable after clamping; resending everything is the safe
            // answer if it ever is reached.
            eprintln!("Weapons update did not apply to the host's copy ({error:#}); resending all");
            self.sent = WeaponView::default();
        }
        Some(delta)
    }
}
fn projectile_same(a: &Projectile, b: &Projectile) -> bool {
    a.age == b.age
        && a.stuck == b.stuck
        && a.bounced == b.bounced
        && a.was_thrown == b.was_thrown
        && a.scale == b.scale
        && a.paint == b.paint
        && a.definition == b.definition
        && a.source == b.source
        && a.origin == b.origin
        && a.heading == b.heading
        && a.position.distance(b.position) <= PROJECTILE_DRIFT
        && a.velocity.distance(b.velocity) <= PROJECTILE_DRIFT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weapon_updates_past_the_wire_limits_are_carried_over_the_next_updates() {
        let mut stream = WeaponStream::default();
        // More players than one update may name dropped their weapons at
        // once (a full server churning inside one update).
        let owners = WeaponDelta::MAX_IMAGES as u64 + 6;
        for owner in 1..=owners {
            stream.sent.images.insert(
                owner,
                vec![bri_sim::session::MountedImage {
                    image: "gun".into(),
                    state: "Ready".into(),
                    hand: 0,
                    paint: None,
                }],
            );
        }
        let current = WeaponView::default();
        let first = stream.delta(&current, 1).expect("an update");
        assert_eq!(first.images.len(), WeaponDelta::MAX_IMAGES);
        first
            .apply(&mut WeaponView::default())
            .expect("a client accepts it");
        let second = stream.delta(&current, 2).expect("the rest follows");
        assert_eq!(second.images.len(), 6);
        assert!(stream.delta(&current, 3).is_none());
    }
    fn pose(owner: OwnerId, tick: u64, x: f32) -> Pose {
        Pose {
            tick,
            acknowledged_input: tick,
            spawn_tick: 0,
            player: PlayerState {
                owner,
                feet: [x, 0.0, 0.0],
                velocity: [0.0; 3],
                yaw: 0.0,
                pitch: 0.0,
                head_yaw: 0.0,
                grounded: true,
                crouched: false,
                jetting: false,
                jump: Default::default(),
                archetype: Default::default(),
                scale: 1.0,
                energy: 100.0,
                speed_scale: 1.0,
                tick: Default::default(),
                tether: None,
            },
        }
    }
    /// Player 1 and a viewer beside them.
    const VIEWERS: &[(OwnerId, Option<[f32; 3]>)] =
        &[(1, Some([0.0; 3])), (2, Some([1.0, 0.0, 0.0]))];
    fn others() -> Audience {
        Audience::Listed(vec![2])
    }
    fn audiences(items: &[(Datagram, Audience)], owner: OwnerId) -> Vec<(u64, Audience)> {
        items
            .iter()
            .filter_map(|(d, a)| match d {
                Datagram::Pose(p) if p.player.owner == owner => Some((p.tick, a.clone())),
                Datagram::Remote(p) if p.owner == owner => Some((p.tick, a.clone())),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_new_body_reaches_its_owner_at_once_even_where_the_old_one_stood() {
        let mut stream = StateStream::default();
        stream.interval(0, vec![pose(1, 0, 0.0)], vec![], vec![], VIEWERS);
        let still = stream.interval(
            POSE_INTERVAL,
            vec![pose(1, POSE_INTERVAL, 0.0)],
            vec![],
            vec![],
            VIEWERS,
        );
        assert!(!still.iter().any(|(_, a)| *a == Audience::Only(1)));
        let respawned = Pose {
            spawn_tick: 2 * POSE_INTERVAL,
            ..pose(1, 2 * POSE_INTERVAL, 0.0)
        };
        let items = stream.interval(2 * POSE_INTERVAL, vec![respawned], vec![], vec![], VIEWERS);
        assert!(items.iter().any(|(d, a)| *a == Audience::Only(1)
            && matches!(d, Datagram::Pose(p) if p.spawn_tick == 2 * POSE_INTERVAL)));
    }

    #[test]
    fn a_still_player_settles_then_keeps_alive_and_resumes_from_where_it_rested() {
        let mut stream = StateStream::default();
        let mut sent = Vec::new();
        for interval in 0..100_u64 {
            let tick = interval * POSE_INTERVAL;
            // Still until interval 60, then walking.
            let x = if interval < 60 {
                0.0
            } else {
                (interval - 59) as f32
            };
            let items = stream.interval(tick, vec![pose(1, tick, x)], vec![], vec![], VIEWERS);
            sent.push(audiences(&items, 1));
        }
        // First sight goes to everyone, then SETTLE unchanged intervals to
        // the others while the owner's own stream is already at 10 Hz.
        assert_eq!(sent[0], [(0, others()), (0, Audience::Only(1))]);
        for (interval, items) in sent.iter().enumerate().take(SETTLE as usize + 1).skip(1) {
            assert_eq!(items, &[(interval as u64 * POSE_INTERVAL, others())]);
        }
        // Then only the owner's 10 Hz stream, plus the one-second keepalive.
        let to_others: Vec<u64> = sent[..60]
            .iter()
            .flatten()
            .filter(|(_, a)| a.includes(2))
            .map(|(t, _)| *t)
            .collect();
        assert_eq!(to_others, [0, 3, 6, 9, 129]);
        let owner = sent[..60]
            .iter()
            .flatten()
            .filter(|(_, a)| a.includes(1))
            .count();
        assert_eq!(owner, 60 / 4);
        // Moving again: the resting pose at the previous interval first.
        let (hold, now) = (59 * POSE_INTERVAL, 60 * POSE_INTERVAL);
        assert_eq!(
            sent[60],
            [
                (hold, Audience::Only(2)),
                (now, others()),
                (now, Audience::Only(1))
            ]
        );
        // Still within NEAR of the viewer: every interval.
        assert!(sent[61..90].iter().all(|s| s.len() == 2));
    }

    #[test]
    fn far_players_are_sent_less_often_and_resume_from_where_they_rested() {
        assert_eq!(pose_interval(0.0), POSE_INTERVAL);
        assert_eq!(pose_interval(NEAR), POSE_INTERVAL);
        assert_eq!(pose_interval(NEAR + 1.0), POSE_INTERVAL * 2);
        assert_eq!(pose_interval(NEAR * 3.0), POSE_INTERVAL * 4);
        assert_eq!(pose_interval(NEAR * 100.0), POSE_INTERVAL * 8);
        assert_eq!(pose_interval(f32::NAN), POSE_INTERVAL * 8);
        // Player 1 runs away from viewer 2 at the origin, 5 units a second,
        // then stops 300 units out; viewer 3 has no viewpoint (full rate).
        let viewers = [(2, Some([0.0; 3])), (1, Some([0.0; 3])), (3, None)];
        let mut stream = StateStream::default();
        let (mut to_2, mut to_3) = (Vec::new(), Vec::new());
        for interval in 0..3000_u64 {
            let tick = interval * POSE_INTERVAL;
            let x = (tick as f32 * 5.0 / 120.0).min(300.0);
            for (datagram, audience) in
                stream.interval(tick, vec![pose(1, tick, x)], vec![], vec![], &viewers)
            {
                if let Datagram::Remote(p) = datagram {
                    if audience.includes(2) {
                        to_2.push((p.tick, p.clone().into_pose().player.feet[0]));
                    }
                    if audience.includes(3) {
                        to_3.push(p.tick);
                    }
                }
            }
        }
        // Each pose to viewer 2 follows the last by the interval for where
        // the player was, until it rests and keeps alive once a second.
        let moving = to_2
            .iter()
            .take_while(|(_, x)| *x < 300.0)
            .collect::<Vec<_>>();
        for pair in moving.windows(2) {
            let ((a, _), (b, x)) = (pair[0], pair[1]);
            assert!(b - a <= pose_interval(*x), "{a} -> {b} at {x}");
        }
        assert_eq!(moving[1].0 - moving[0].0, POSE_INTERVAL);
        let far = moving.iter().rev().take(10).collect::<Vec<_>>();
        assert!(far.windows(2).all(|p| p[0].0 - p[1].0 == POSE_INTERVAL * 8));
        let rest = to_2.iter().position(|(_, x)| *x >= 300.0).unwrap();
        assert!(
            to_2[rest..]
                .windows(2)
                .skip(SETTLE as usize)
                .all(|p| p[1].0 - p[0].0 >= KEEPALIVE)
        );
        // A viewer with no viewpoint gets every interval while it moves.
        assert!(
            to_3.windows(2)
                .take(1000)
                .all(|p| p[1] - p[0] == POSE_INTERVAL)
        );
        // Over the run out to 300 units, under a third of the full rate.
        let sent_2 = to_2.iter().filter(|(t, _)| *t < 60 * 120).count();
        let sent_3 = to_3.iter().filter(|t| **t < 60 * 120).count();
        assert!(sent_2 * 3 < sent_3, "{sent_2} vs {sent_3}");

        // Walking back from rest: the resting pose one interval earlier,
        // then the move, both to the far viewer.
        let tick = 3000 * POSE_INTERVAL;
        let items = stream.interval(tick, vec![pose(1, tick, 299.0)], vec![], vec![], &viewers);
        let to_2: Vec<_> = items
            .iter()
            .filter(|(_, a)| a.includes(2))
            .map(|(d, _)| match d {
                Datagram::Remote(p) => (p.tick, p.clone().into_pose().player.feet[0]),
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(to_2, [(tick - POSE_INTERVAL * 8, 300.0), (tick, 299.0)]);
    }

    #[test]
    fn a_running_remote_pose_is_compact_and_round_trips() {
        let mut running = pose(4, 1_000_000, 123.456);
        running.player.feet = [123.456, 2.5, -287.25];
        running.player.velocity = [6.1, -0.4, 3.3];
        running.player.yaw = 2.5;
        running.player.pitch = -0.3;
        running.player.grounded = true;
        let remote = RemotePose::of(running.tick, &running.player);
        let bytes = crate::codec::encode_datagram_item(&Datagram::Remote(remote.clone())).unwrap();
        // The protocol 53 layout: float position, a byte per flag, scale.
        #[derive(serde::Serialize)]
        enum Old {
            #[serde(rename = "r")]
            Remote(
                u64,
                u64,
                [f32; 3],
                [i16; 3],
                [i16; 3],
                bool,
                bool,
                bool,
                u8,
                f32,
            ),
        }
        let old = crate::codec::encode_datagram_item(&Old::Remote(
            remote.tick,
            4,
            running.player.feet,
            remote.velocity,
            remote.look,
            true,
            false,
            false,
            0,
            1.0,
        ))
        .unwrap();
        assert_eq!(old.len(), 52);
        assert!(bytes.len() <= 39, "{} bytes", bytes.len());
        let back = remote.into_pose().player;
        for (a, b) in back.feet.iter().zip(running.player.feet) {
            assert!((a - b).abs() <= 0.005, "{a} vs {b}");
        }
        assert!(back.grounded && !back.crouched && !back.jetting);
        assert_eq!(back.scale, 1.0);
        let mut big = running.player.clone();
        big.scale = 2.0;
        big.crouched = true;
        let back = RemotePose::of(0, &big).into_pose().player;
        assert_eq!(
            (back.scale, back.crouched, back.grounded),
            (2.0, true, true)
        );
    }

    #[test]
    fn a_crowd_keeps_the_closest_moving_players_at_full_rate() {
        // Viewer 100 at the origin; 30 players 1..=30 units away, all within
        // NEAR, all running, plus one standing still right beside it.
        let viewers = [(100, Some([0.0; 3]))];
        let mut stream = StateStream::default();
        let mut sent = BTreeMap::<OwnerId, usize>::new();
        for interval in 0..=40_u64 {
            let tick = interval * POSE_INTERVAL;
            let mut poses: Vec<Pose> = (1..=30)
                .map(|owner| {
                    let mut p = pose(owner, tick, owner as f32);
                    p.player.feet[2] = tick as f32 * 0.01;
                    p
                })
                .collect();
            poses.push(pose(31, tick, 0.5));
            for (datagram, audience) in stream.interval(tick, poses, vec![], vec![], &viewers) {
                if let Datagram::Remote(p) = datagram
                    && audience.includes(100)
                    && interval > 0
                {
                    *sent.entry(p.owner).or_default() += 1;
                }
            }
        }
        for owner in 1..=30 {
            let expected = match owner as usize {
                1..=FULL_RATE_PLAYERS => 40,
                n if n <= FULL_RATE_PLAYERS * 2 => 20,
                _ => 10,
            };
            assert_eq!(sent[&owner], expected, "player {owner}");
        }
        // The still one settled: its first pose and SETTLE more.
        assert_eq!(sent.get(&31), Some(&(SETTLE as usize)));
    }

    #[test]
    fn float_noise_is_still_and_departed_items_are_forgotten() {
        let mut stream = StateStream::default();
        for interval in 0..20_u64 {
            let tick = interval * POSE_INTERVAL;
            let noise = if interval % 2 == 0 { 0.0 } else { 1e-5 };
            let vehicle = VehiclePose {
                id: 7,
                tick,
                position: [noise; 3],
                rotation: [0.0, 0.0, 0.0, 1.0],
                velocity: [0.0; 3],
                steering: 0.0,
                wheel_suspension: vec![0.1; 4],
                wheel_rotation: vec![0.0; 4],
                wheel_contact: vec![true; 4],
                wheel_tire: vec![Default::default(); 4],
                turret_aim: [0.0; 2],
                jetting: false,
                angular_velocity: [0.0; 3],
                mouse_steering: [0.0; 2],
                driver_input: 0,
                driver_steering: (false, false),
                steering_quiet: 0,
                actor: None,
            };
            let items = stream.interval(
                tick,
                vec![pose(1, tick, noise)],
                vec![vehicle],
                vec![(1, [noise; 3])],
                VIEWERS,
            );
            if interval > SETTLE as u64 {
                assert!(
                    items.iter().all(|(_, a)| *a == Audience::Only(1)),
                    "{interval}"
                );
            }
        }
        assert!(
            stream
                .interval(60, vec![], vec![], vec![], VIEWERS)
                .is_empty()
        );
        assert!(stream.remote.is_empty() && stream.vehicles.is_empty() && stream.orbs.is_empty());
    }

    fn rocket(id: u64, velocity: glam::Vec3) -> Projectile {
        Projectile {
            id,
            definition: "arrow".into(),
            source: bri_weapons::ActorId(1),
            position: glam::Vec3::ZERO,
            velocity,
            scale: 1.0,
            age: 0,
            bounced: false,
            stuck: false,
            origin: glam::Vec3::ZERO,
            was_thrown: false,
            paint: None,
            heading: None,
            bounces: 0,
            spawned: 0,
        }
    }

    #[test]
    fn projectiles_are_sent_once_and_clients_coast_them_to_the_hosts_flight() {
        let falls: BTreeMap<String, f32> = [("arrow".to_string(), 9.81 * 0.5 / 120.0)].into();
        let mut host = WeaponView {
            projectiles: vec![rocket(1, glam::Vec3::new(0.0, 10.0, -30.0))],
            ..Default::default()
        };
        let mut stream = WeaponStream::default();
        stream.reset(WeaponView::default(), 0, falls.clone());
        let mut client = WeaponView::default();
        let mut client_tick = 0;
        let mut sent = Vec::new();
        for tick in 1..=240_u64 {
            // The host flies it; at tick 150 it bounces off something.
            for p in &mut host.projectiles {
                bri_weapons::coast(p, falls["arrow"]);
                if tick == 150 {
                    p.velocity = glam::Vec3::new(3.0, 2.0, 1.0);
                    p.bounced = true;
                    p.age = 0;
                }
            }
            if tick == 200 {
                host.projectiles.clear();
            }
            if !tick.is_multiple_of(6) {
                continue;
            }
            let delta = stream.delta(&host, tick);
            crate::protocol::coast_projectiles(&mut client, &falls, tick - client_tick);
            client_tick = tick;
            if let Some(delta) = &delta {
                delta.apply(&mut client).unwrap();
                sent.push((tick, delta.projectiles.len(), delta.removed.len()));
            }
            assert_eq!(client.projectiles.len(), host.projectiles.len());
            for (c, h) in client.projectiles.iter().zip(&host.projectiles) {
                assert!(projectile_same(c, h), "tick {tick}: {c:?} vs {h:?}");
            }
        }
        // Its launch, its bounce and its end; nothing while it flies.
        assert_eq!(sent, [(6, 1, 0), (150, 1, 0), (204, 0, 1)]);
    }

    #[test]
    fn held_movement_keeps_every_input_once() {
        use crate::protocol::{MAX_MOVEMENT_BATCH, merge_movement};
        let input = |forward: f32| bri_sim::player::MoveInput {
            forward,
            ..Default::default()
        };
        let batch = |newest: u64, first: u64| {
            (
                newest,
                (first..=newest)
                    .map(|s| input(s as f32 / 1000.0))
                    .collect::<Vec<_>>(),
            )
        };
        // Overlapping redundancy: 5..=10 then 7..=12 is 5..=12.
        let (newest, inputs) = merge_movement(batch(10, 5), batch(12, 7));
        assert_eq!(newest, 12);
        assert_eq!(inputs, batch(12, 5).1);
        // Adjacent: 1..=3 then 4..=6.
        assert_eq!(merge_movement(batch(3, 1), batch(6, 4)).1, batch(6, 1).1);
        // A gap or an older batch: only the newer one.
        assert_eq!(merge_movement(batch(3, 1), batch(9, 6)).1, batch(9, 6).1);
        assert_eq!(merge_movement(batch(9, 6), batch(8, 5)).1, batch(8, 5).1);
        // Bounded.
        let (_, long) = merge_movement(batch(40, 1), batch(80, 41));
        assert_eq!(long.len(), MAX_MOVEMENT_BATCH);
        assert_eq!(long.last(), Some(&input(0.08)));
    }

    #[test]
    fn moving_entities_send_only_where_they_are() {
        use crate::protocol::EntityDelta;
        use bri_sim::session::EntityInfo;
        let zombie = |id, x: f32| EntityInfo {
            id,
            kind: "zombies:zombie".into(),
            model: "zombies:model/zombie".into(),
            position: [x, 0.0, 0.0],
            yaw: 0.5,
            scale: 1.0,
            label: "Zombie".into(),
        };
        let mut host = BTreeMap::new();
        let mut client = BTreeMap::new();
        let first = EntityDelta::between(&mut host, vec![zombie(1, 0.0), zombie(2, 5.0)]).unwrap();
        assert_eq!(first.changed.len(), 2);
        first.apply(&mut client).unwrap();
        assert_eq!(
            EntityDelta::between(&mut host, vec![zombie(1, 0.0), zombie(2, 5.0)]),
            None
        );
        let mut renamed = zombie(2, 6.0);
        renamed.label = "Boss".into();
        let next = EntityDelta::between(
            &mut host,
            vec![zombie(1, 0.5), renamed.clone(), zombie(3, 9.0)],
        )
        .unwrap();
        assert_eq!(next.moved, [(1, [0.5, 0.0, 0.0], 0.5)]);
        assert_eq!(next.changed, [renamed, zombie(3, 9.0)]);
        next.apply(&mut client).unwrap();
        let gone = EntityDelta::between(&mut host, vec![zombie(3, 9.0)]).unwrap();
        assert_eq!(gone.removed, [1, 2]);
        gone.apply(&mut client).unwrap();
        assert_eq!(client, host);
        // A move of an entity the client never had is refused.
        let bad = EntityDelta {
            moved: vec![(9, [0.0; 3], 0.0)],
            ..Default::default()
        };
        assert!(bad.apply(&mut client).is_err());
        // A joiner handed an entity that left before the next update, and
        // one that was only just born, ends up like everyone else.
        let mut joiner: BTreeMap<u64, EntityInfo> =
            [(3, zombie(3, 9.0)), (4, zombie(4, 1.0))].into();
        let joined = vec![joiner.values().cloned().collect::<Vec<_>>()];
        let next = EntityDelta::between_joined(&mut host, &joined, vec![zombie(3, 9.5)]).unwrap();
        assert_eq!(next.moved, [(3, [9.5, 0.0, 0.0], 0.5)]);
        assert_eq!(next.removed, [4]);
        next.apply(&mut client).unwrap();
        next.apply(&mut joiner).unwrap();
        assert_eq!(client, host);
        assert_eq!(joiner, host);
    }

    #[test]
    fn a_joiner_loses_projectiles_that_ended_between_updates() {
        use bri_sim::session::WeaponView;
        let mut stream = WeaponStream::default();
        stream.reset(WeaponView::default(), 0, BTreeMap::new());
        let spark = Projectile {
            id: 7,
            ..rocket(1, glam::Vec3::ZERO)
        };
        let checkpoint = WeaponView {
            projectiles: vec![spark],
            ..Default::default()
        };
        stream.joined(&checkpoint);
        let delta = stream.delta(&WeaponView::default(), 6).unwrap();
        assert_eq!(delta.removed, [7]);
        // Only once.
        assert_eq!(stream.delta(&WeaponView::default(), 12), None);
    }
}
