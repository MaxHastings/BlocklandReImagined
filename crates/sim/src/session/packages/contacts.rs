//! Bodies meeting (`on_contact`): one player's body landing on another's, or
//! bumping it, a vehicle running into a player, and two vehicles crashing. The engine says who met
//! whom, from which side and how fast; what that means (a stomp flattens, a
//! run-over kills, a bump does nothing) is the rules' own, so a Mario
//! Add-On and a Warthog Add-On need no code here
//! (`docs/architecture/bots.md`, Attacks).
use super::game_hooks::declaring;
use super::*;
use bri_package_runtime::rhai::Map;
use bri_vehicles::VehicleId;

/// How far apart two bodies may be and still touch: the motor keeps them
/// from overlapping, so they meet a hair apart.
const TOUCH: f32 = 0.1;
/// The most contacts told a tick, so a crowd cannot take the tick.
const MAX_A_TICK: usize = 64;

/// Which bodies touched at the end of the last tick, so a contact is told
/// once, as it begins.
#[derive(Default)]
pub(super) struct Contacts {
    /// Pairs of players, lower id first.
    bodies: BTreeSet<(OwnerId, OwnerId)>,
    /// Pairs of vehicles, lower id first.
    vehicles: BTreeSet<(u64, u64)>,
}

/// A contact as the rules hear it.
pub(in crate::session) struct Contact {
    /// "body", "vehicle" or "vehicles".
    kind: &'static str,
    /// The one that came down on, ran into or drove into the other: the
    /// upper body of a stomp, the quicker of a bump, a vehicle's driver.
    by: Option<OwnerId>,
    /// The player met; a struck vehicle's driver.
    target: Option<OwnerId>,
    vehicle: Option<u64>,
    /// The vehicle struck, when two met.
    target_vehicle: Option<u64>,
    /// Where `by` (or the vehicle) met `target` from: "above", "below" or
    /// "side".
    from: &'static str,
    /// `by`'s velocity (the vehicle's) less `target`'s, before they met.
    velocity: Vec3,
}

impl Session {
    /// Tells `on_contact` hooks of every pair of living, unseated bodies
    /// that began to touch this tick. `before` is each player's velocity
    /// before this tick's movement (meeting stops them).
    pub(in crate::session) fn package_body_contacts(&mut self, before: &BTreeMap<OwnerId, Vec3>) {
        let Some(host) = self.packages.as_ref() else {
            return;
        };
        if declaring(host, |b| b.on_contact).is_empty() {
            return;
        }
        let boxes: Vec<(OwnerId, Vec3, Vec3, Vec3)> = self
            .peers
            .iter()
            .filter(|(o, p)| p.combat.alive && !self.seated(**o) && !self.riding.is_riding(**o))
            .map(|(o, p)| {
                let state = p.player.state();
                let feet = Vec3::from(state.feet);
                let half = p.player.tuning().width * state.scale * 0.5;
                let height = crate::water::body_height(state, p.player.tuning()) * state.scale;
                let velocity = before
                    .get(o)
                    .copied()
                    .unwrap_or_else(|| Vec3::from(state.velocity));
                (
                    *o,
                    feet - Vec3::new(half, 0.0, half),
                    feet + Vec3::new(half, height, half),
                    velocity,
                )
            })
            .collect();
        let mut touching = BTreeSet::new();
        let mut begun = Vec::new();
        let was = &self.packages.as_ref().unwrap().contacts.bodies;
        for (i, a) in boxes.iter().enumerate() {
            for b in &boxes[i + 1..] {
                let lo = a.1.max(b.1);
                let hi = a.2.min(b.2);
                let overlap = hi - lo + Vec3::splat(TOUCH);
                if overlap.min_element() < 0.0 {
                    continue;
                }
                let pair = (a.0.min(b.0), a.0.max(b.0));
                touching.insert(pair);
                if !was.contains(&pair) {
                    begun.push((a, b, overlap));
                }
            }
        }
        let mut contacts = Vec::new();
        for (a, b, overlap) in begun.into_iter().take(MAX_A_TICK) {
            // They met along the axis they overlap least on.
            let (by, target) = if overlap.y < overlap.x.min(overlap.z) {
                // The upper one came down on the lower.
                if a.1.y >= b.1.y { (a, b) } else { (b, a) }
            } else {
                // The one closing faster ran into the other.
                let toward = |from: &(OwnerId, Vec3, Vec3, Vec3),
                              to: &(OwnerId, Vec3, Vec3, Vec3)| {
                    let d = (to.1 + to.2 - from.1 - from.2) * 0.5;
                    from.3.dot(Vec3::new(d.x, 0.0, d.z).normalize_or_zero())
                };
                if toward(a, b) >= toward(b, a) {
                    (a, b)
                } else {
                    (b, a)
                }
            };
            let from = if overlap.y < overlap.x.min(overlap.z) {
                if by.1.y >= target.1.y {
                    "above"
                } else {
                    "below"
                }
            } else {
                "side"
            };
            contacts.push(Contact {
                kind: "body",
                by: Some(by.0),
                target: Some(target.0),
                vehicle: None,
                target_vehicle: None,
                from,
                velocity: by.3 - target.3,
            });
        }
        self.packages.as_mut().unwrap().contacts.bodies = touching;
        for contact in contacts {
            self.package_contact(contact);
        }
    }

    /// Tells `on_contact` hooks that `vehicle` ran into `player`, as the
    /// contact began.
    pub(in crate::session) fn package_vehicle_contact(
        &mut self,
        vehicle: u64,
        player: OwnerId,
        player_velocity: Vec3,
    ) {
        let Some(host) = self.packages.as_ref() else {
            return;
        };
        if declaring(host, |b| b.on_contact).is_empty() {
            return;
        }
        let Some(world) = self.vehicles.world.as_ref() else {
            return;
        };
        let Some(v) = world.vehicle_snapshot(&self.simulation.physics, VehicleId(vehicle)) else {
            return;
        };
        let Some(peer) = self.peers.get(&player) else {
            return;
        };
        let state = peer.player.state();
        let top =
            state.feet[1] + crate::water::body_height(state, peer.player.tuning()) * state.scale;
        let centre = v.transform.position[1];
        let from = if centre > top {
            "above"
        } else if centre < state.feet[1] {
            "below"
        } else {
            "side"
        };
        let contact = Contact {
            kind: "vehicle",
            by: self.vehicle_driver(VehicleId(vehicle)),
            target: Some(player),
            vehicle: Some(vehicle),
            target_vehicle: None,
            from,
            velocity: Vec3::from(v.velocity) - player_velocity,
        };
        self.package_contact(contact);
    }

    /// Tells `on_contact` hooks of every two vehicles whose hulls began to
    /// touch this tick: `vehicle` is the one closing faster, which ran into
    /// `target_vehicle`; `by` and `target` are their drivers.
    pub(in crate::session) fn package_vehicle_crashes(&mut self) {
        let Some(host) = self.packages.as_ref() else {
            return;
        };
        if declaring(host, |b| b.on_contact).is_empty() {
            return;
        }
        let Some(world) = self.vehicles.world.as_ref() else {
            return;
        };
        let physics = &self.simulation.physics;
        let bodies: std::collections::HashMap<rapier3d::prelude::RigidBodyHandle, VehicleId> =
            world
                .ids()
                .filter_map(|id| Some((world.body_of(id)?, id)))
                .collect();
        let mut touching = BTreeSet::new();
        for (body, id) in &bodies {
            let Some(b) = physics.bodies.get(*body) else {
                continue;
            };
            for c in b.colliders() {
                for pair in physics.contact_pairs_with(*c) {
                    if !pair.has_any_active_contact() {
                        continue;
                    }
                    let other = if pair.collider1 == *c {
                        pair.collider2
                    } else {
                        pair.collider1
                    };
                    let Some(other) = physics
                        .colliders
                        .get(other)
                        .and_then(|c| c.parent())
                        .and_then(|b| bodies.get(&b))
                    else {
                        continue;
                    };
                    if other != id {
                        touching.insert((id.0.min(other.0), id.0.max(other.0)));
                    }
                }
            }
        }
        let host = self.packages.as_mut().expect("checked");
        let before = std::mem::replace(&mut host.contacts.vehicles, touching.clone());
        let mut contacts = Vec::new();
        for (a, b) in touching.difference(&before).take(MAX_A_TICK) {
            let world = self.vehicles.world.as_ref().expect("checked");
            let (Some(va), Some(vb)) = (
                world.vehicle_snapshot(physics, VehicleId(*a)),
                world.vehicle_snapshot(physics, VehicleId(*b)),
            ) else {
                continue;
            };
            let (pa, pb) = (
                Vec3::from(va.transform.position),
                Vec3::from(vb.transform.position),
            );
            let (wa, wb) = (Vec3::from(va.velocity), Vec3::from(vb.velocity));
            let toward = |from: Vec3, to: Vec3, v: Vec3| v.dot((to - from).normalize_or_zero());
            let (by, target, by_at, target_at, velocity) =
                if toward(pa, pb, wa) >= toward(pb, pa, wb) {
                    (*a, *b, pa, pb, wa - wb)
                } else {
                    (*b, *a, pb, pa, wb - wa)
                };
            let d = by_at - target_at;
            let from = if d.y.abs() > Vec3::new(d.x, 0.0, d.z).length() {
                if d.y > 0.0 { "above" } else { "below" }
            } else {
                "side"
            };
            contacts.push(Contact {
                kind: "vehicles",
                by: self.vehicle_driver(VehicleId(by)),
                target: self.vehicle_driver(VehicleId(target)),
                vehicle: Some(by),
                target_vehicle: Some(target),
                from,
                velocity,
            });
        }
        for contact in contacts {
            self.package_contact(contact);
        }
    }

    fn package_contact(&mut self, contact: Contact) {
        let Some(host) = self.packages.as_ref() else {
            return;
        };
        let hooks = declaring(host, |b| b.on_contact);
        let id = |v: Option<u64>| v.map_or(Dynamic::UNIT, |v| Dynamic::from_int(v as i64));
        let float = |v: f32| Dynamic::from_float(v.into());
        let mut info = Map::new();
        info.insert("kind".into(), Dynamic::from(contact.kind.to_owned()));
        info.insert("by".into(), id(contact.by));
        info.insert("target".into(), id(contact.target));
        info.insert("vehicle".into(), id(contact.vehicle));
        info.insert("target_vehicle".into(), id(contact.target_vehicle));
        info.insert("from".into(), Dynamic::from(contact.from.to_owned()));
        info.insert("speed".into(), float(contact.velocity.length()));
        info.insert(
            "velocity".into(),
            Dynamic::from_array(contact.velocity.to_array().map(float).into()),
        );
        for package in hooks {
            let _ = self.run_package(
                &package,
                "on_contact",
                vec![Dynamic::from_map(info.clone())],
                Budget::Command,
                None,
                None,
                None,
            );
            self.charge_work(&package);
        }
    }
}
