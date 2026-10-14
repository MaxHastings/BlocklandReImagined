//! `on_object`: the rules hear every object come into the world, leave it
//! or change hands, whoever made it: a vehicle a brick spawned, another
//! Add-On's creature, a gun a player dropped, a bot. An Add-On that logs,
//! taxes, tags or cleans up the world's objects (a prop protection, a
//! cleanup timer, an economy charging for spawned cars) needs nothing else
//! from the Add-On or engine system that made them.
//!
//! The engine compares what stands in the world with the last tick, so
//! every way an object can appear or go is seen, and a game without the
//! hook never pays for it.
use super::game_hooks::declaring;
use super::*;
use bri_package_runtime::rhai::Map;

/// Most changes told in one tick. More are told as a single `overflow`
/// event, and what the engine remembers stays right, so the next tick's
/// changes are exact again.
const MAX_A_TICK: usize = 512;

/// One object as the hook compares it between ticks.
#[derive(Debug, Clone, PartialEq)]
struct Seen {
    definition: String,
    package: String,
    owner: Option<u64>,
    driver: Option<u64>,
    position: [f32; 3],
}

/// The objects standing at the end of the last tick the hook looked.
#[derive(Default)]
pub(super) struct ObjectWatch {
    seen: BTreeMap<String, Seen>,
}

impl Session {
    /// What stands in the world now, by the reference rules name it with.
    fn objects_now(&self) -> BTreeMap<String, Seen> {
        let mut now = BTreeMap::new();
        if let Some(world) = &self.vehicles.world {
            let snapshot = world.snapshot(&self.simulation.physics);
            for v in snapshot.vehicles.into_iter().filter(|v| !v.destroyed) {
                let driver = world
                    .definition(&v.definition)
                    .and_then(|d| d.control_seat())
                    .and_then(|seat| v.seats.get(seat))
                    .and_then(|s| s.occupant.as_ref())
                    .map(|o| o.owner.0)
                    .filter(|o| self.peers.contains_key(o));
                now.insert(
                    ObjectRef::Vehicle(v.id.0).to_string(),
                    Seen {
                        definition: v.definition.clone(),
                        package: self
                            .movables
                            .spawned
                            .get(&v.id.0)
                            .cloned()
                            .unwrap_or_default(),
                        owner: (v.owner.0 != 0).then_some(v.owner.0),
                        driver,
                        position: v.transform.position,
                    },
                );
            }
        }
        for (owner, peer) in self.peers.iter().filter(|(o, _)| self.bots.is_bot(**o)) {
            now.insert(
                ObjectRef::Player(*owner).to_string(),
                Seen {
                    definition: self.bot_kind_id(*owner).unwrap_or_default().to_owned(),
                    package: String::new(),
                    owner: self.bot_brick_owner(*owner),
                    driver: None,
                    position: peer.player.state().feet,
                },
            );
        }
        if let Some(host) = self.packages.as_ref() {
            for (id, e) in &host.entities {
                now.insert(
                    ObjectRef::Entity(*id).to_string(),
                    Seen {
                        definition: e.kind.clone(),
                        package: e.package.clone(),
                        owner: None,
                        driver: None,
                        position: e.body.state().feet,
                    },
                );
            }
            for d in self.weapons.drops() {
                now.insert(
                    format!("drop:{}", d.id),
                    Seen {
                        definition: d.item.clone(),
                        package: host
                            .item_hooks
                            .package_drops
                            .get(&d.id)
                            .cloned()
                            .unwrap_or_default(),
                        owner: None,
                        driver: None,
                        position: d.position.to_array(),
                    },
                );
            }
        }
        now
    }

    /// Tell `on_object` hooks what came, went or changed hands since the
    /// last tick. The first look after the rules start tells every object
    /// already standing as `created`.
    pub(in crate::session) fn deliver_object_changes(&mut self) {
        let Some(host) = self.packages.as_ref() else {
            return;
        };
        let hooks = declaring(host, |b| b.on_object);
        if hooks.is_empty() {
            return;
        }
        let now = self.objects_now();
        let host = self.packages.as_mut().expect("checked");
        let before = std::mem::replace(&mut host.object_watch.seen, now.clone());
        let mut events: Vec<Map> = Vec::new();
        let id = |v: Option<u64>| v.map_or(Dynamic::UNIT, |v| Dynamic::from_int(v as i64));
        let event = |kind: &str, object: &str, seen: &Seen| {
            let (sort, _) = object.split_once(':').unwrap_or((object, ""));
            let mut map = Map::new();
            map.insert("kind".into(), kind.into());
            map.insert("ref".into(), object.into());
            map.insert("object".into(), sort.into());
            map.insert("definition".into(), seen.definition.clone().into());
            map.insert("addon".into(), seen.package.clone().into());
            map.insert("owner".into(), id(seen.owner));
            map.insert("driver".into(), id(seen.driver));
            for (axis, v) in ["x", "y", "z"].iter().zip(seen.position) {
                map.insert((*axis).into(), Dynamic::from_float(v.into()));
            }
            map
        };
        for (object, seen) in &now {
            match before.get(object) {
                None => events.push(event("created", object, seen)),
                Some(was) if was.owner != seen.owner || was.driver != seen.driver => {
                    let mut map = event("changed", object, seen);
                    let mut old = Map::new();
                    old.insert("owner".into(), id(was.owner));
                    old.insert("driver".into(), id(was.driver));
                    map.insert("was".into(), Dynamic::from_map(old));
                    events.push(map);
                }
                Some(_) => {}
            }
        }
        for (object, was) in &before {
            if !now.contains_key(object) {
                events.push(event("removed", object, was));
            }
        }
        if events.len() > MAX_A_TICK {
            let dropped = events.len() - MAX_A_TICK;
            events.truncate(MAX_A_TICK);
            let mut map = Map::new();
            map.insert("kind".into(), "overflow".into());
            map.insert("dropped".into(), Dynamic::from_int(dropped as i64));
            events.push(map);
        }
        for map in events {
            for package in &hooks {
                let _ = self.run_package(
                    package,
                    "on_object",
                    vec![Dynamic::from_map(map.clone())],
                    Budget::Command,
                    None,
                    None,
                    None,
                );
                self.charge_work(package);
            }
        }
    }
}
