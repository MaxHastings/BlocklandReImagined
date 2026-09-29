//! Which state datagrams each player gets at a pose interval.
//!
//! Every player, vehicle and admin camera used to go to every peer 40 times
//! a second, standing still or not: eight idle players cost the host about
//! 650 KB/s. Now an item goes out while it changes, for a few intervals
//! after it settles (so one lost datagram never leaves a stale resting
//! pose), then once a second. A player's own pose, which acknowledges their
//! inputs, goes to them at 10 Hz while it is still.
use crate::protocol::{Datagram, Orb, POSE_INTERVAL, Pose, RemotePose, WeaponDelta};
use bri_sim::session::WeaponView;
use bri_sim::{player::PlayerState, session::VehiclePose};
use bri_weapons::Projectile;
use bri_world::OwnerId;
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

/// Who receives one item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Audience {
    All,
    Only(OwnerId),
    AllBut(OwnerId),
}
impl Audience {
    pub fn includes(self, peer: OwnerId) -> bool {
        match self {
            Audience::All => true,
            Audience::Only(owner) => owner == peer,
            Audience::AllBut(owner) => owner != peer,
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
fn decide<T: Clone>(
    sent: &mut BTreeMap<u64, Sent<T>>,
    id: u64,
    state: &T,
    tick: u64,
    same: impl Fn(&T, &T) -> bool,
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
    if same(&last.state, state) {
        last.quiet = last.quiet.saturating_add(1);
        if last.quiet > SETTLE && tick < last.tick + KEEPALIVE {
            return Decision::Skip;
        }
        last.state = state.clone();
        last.tick = tick;
        return Decision::Send;
    }
    let held = last.tick + POSE_INTERVAL < tick;
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
}

/// The host's record of what it last sent of each item.
#[derive(Default)]
pub struct StateStream {
    remote: BTreeMap<u64, Sent<PlayerState>>,
    own: BTreeMap<u64, Sent<PlayerState>>,
    vehicles: BTreeMap<u64, Sent<VehiclePose>>,
    orbs: BTreeMap<u64, Sent<[f32; 3]>>,
}
impl StateStream {
    /// The datagram items of the interval at `tick`, in send order, with
    /// who gets each.
    pub fn interval(
        &mut self,
        tick: u64,
        poses: Vec<Pose>,
        vehicles: Vec<VehiclePose>,
        orbs: Vec<(OwnerId, [f32; 3])>,
    ) -> Vec<(Datagram, Audience)> {
        let mut out = Vec::new();
        self.remote
            .retain(|id, _| poses.iter().any(|p| p.player.owner == *id));
        self.own
            .retain(|id, _| poses.iter().any(|p| p.player.owner == *id));
        self.vehicles
            .retain(|id, _| vehicles.iter().any(|v| v.id == *id));
        self.orbs.retain(|id, _| orbs.iter().any(|(o, _)| o == id));
        for pose in poses {
            let owner = pose.player.owner;
            let to_others = decide(&mut self.remote, owner, &pose.player, tick, player_same);
            // The owner's own stream: every change, and 10 Hz while still.
            let to_owner = match self.own.get_mut(&owner) {
                Some(last)
                    if player_same(&last.state, &pose.player) && tick < last.tick + OWN_IDLE =>
                {
                    false
                }
                Some(last) => {
                    last.state = pose.player.clone();
                    last.tick = tick;
                    true
                }
                None => {
                    self.own.insert(
                        owner,
                        Sent {
                            state: pose.player.clone(),
                            tick,
                            quiet: 0,
                        },
                    );
                    true
                }
            };
            if let Decision::HoldThenSend(state) = &to_others {
                out.push((
                    Datagram::Remote(RemotePose::of(tick - POSE_INTERVAL, state)),
                    Audience::AllBut(owner),
                ));
            }
            if !matches!(to_others, Decision::Skip) {
                out.push((
                    Datagram::Remote(RemotePose::of(tick, &pose.player)),
                    Audience::AllBut(owner),
                ));
            }
            if to_owner {
                out.push((Datagram::Pose(pose), Audience::Only(owner)));
            }
        }
        for vehicle in vehicles {
            match decide(&mut self.vehicles, vehicle.id, &vehicle, tick, vehicle_same) {
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
            match decide(&mut self.orbs, owner, &eye, tick, |a, b| {
                near(a, b, DISTANCE)
            }) {
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
                tick: Default::default(),
            },
        }
    }
    fn audiences(items: &[(Datagram, Audience)], owner: OwnerId) -> Vec<(u64, Audience)> {
        items
            .iter()
            .filter_map(|(d, a)| match d {
                Datagram::Pose(p) if p.player.owner == owner => Some((p.tick, *a)),
                Datagram::Remote(p) if p.owner == owner => Some((p.tick, *a)),
                _ => None,
            })
            .collect()
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
            let items = stream.interval(tick, vec![pose(1, tick, x)], vec![], vec![]);
            sent.push(audiences(&items, 1));
        }
        // First sight goes to everyone, then SETTLE unchanged intervals to
        // the others while the owner's own stream is already at 10 Hz.
        assert_eq!(sent[0], [(0, Audience::AllBut(1)), (0, Audience::Only(1))]);
        for (interval, items) in sent.iter().enumerate().take(SETTLE as usize + 1).skip(1) {
            assert_eq!(
                items,
                &[(interval as u64 * POSE_INTERVAL, Audience::AllBut(1))]
            );
        }
        // Then only the owner's 10 Hz stream, plus the one-second keepalive.
        let others: Vec<u64> = sent[..60]
            .iter()
            .flatten()
            .filter(|(_, a)| a.includes(2))
            .map(|(t, _)| *t)
            .collect();
        assert_eq!(others, [0, 3, 6, 9, 129]);
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
                (hold, Audience::AllBut(1)),
                (now, Audience::AllBut(1)),
                (now, Audience::Only(1))
            ]
        );
        assert!(sent[61..].iter().all(|s| s.len() == 2));
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
                turret_aim: [0.0; 2],
                jetting: false,
            };
            let items = stream.interval(
                tick,
                vec![pose(1, tick, noise)],
                vec![vehicle],
                vec![(1, [noise; 3])],
            );
            if interval > SETTLE as u64 {
                assert!(
                    items.iter().all(|(_, a)| *a == Audience::Only(1)),
                    "{interval}"
                );
            }
        }
        assert!(stream.interval(60, vec![], vec![], vec![]).is_empty());
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
