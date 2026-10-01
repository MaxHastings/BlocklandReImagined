//! Grenades as a tactical pack throws them: counted from the holder's
//! reserve with no magazine of their own, put away when the last is thrown
//! and back in hand as more arrive; a firebomb whose embers scatter as its
//! script threw them and burn only the players near them. The pack is
//! written here; it is our own.
use bri_weapons::*;
use glam::Vec3;

const A: ActorId = ActorId(1);
const B: ActorId = ActorId(2);

/// Open air, with whatever stands near any point.
#[derive(Default)]
struct Air {
    near: Vec<Nearby>,
}
impl Query for Air {
    fn sweep(&mut self, _: Vec3, _: Vec3, _: Filter) -> Option<Hit> {
        None
    }
    fn radius(&mut self, _: Vec3, _: f32, _: usize) -> Vec<Nearby> {
        self.near.clone()
    }
    fn can_affect(&self, _: ActorId, _: TargetId) -> bool {
        true
    }
    fn can_catch(&self, _: ActorId, _: ActorId) -> bool {
        false
    }
}

const KIT: &str = r#"{
    "schema_version": 3,
    "id": "nade",
    "items": {
        "nade:weapon/grenade": { "ui_name": "Grenade", "image": "nade:image/grenade" },
        "nade:weapon/pistol": { "ui_name": "Pistol", "image": "nade:image/pistol" },
        "nade:weapon/molotov": { "ui_name": "Molotov", "image": "nade:image/molotov" },
        "nade:weapon/mortar": { "ui_name": "Mortar", "image": "nade:image/mortar" }
    },
    "images": {
        "nade:image/grenade": {
            "projectile": "nade:projectile/grenade",
            "magazine": { "size": 1, "ammo": "nades", "reload_ticks": 1, "reserve": 2,
                          "max_reserve": 4, "from_reserve": true, "display": "Grenades" },
            "states": [
                { "name": "Activate", "ticks": 1, "timeout": 1 },
                { "name": "Ready", "down": 2, "script": "onReady" },
                { "name": "Tick", "ticks": 6, "timeout": 3, "allow_change": false },
                { "name": "Fire", "ticks": 60, "timeout": 0, "script": "onFire", "allow_change": false }
            ]
        },
        "nade:image/molotov": {
            "projectile": "nade:projectile/grenade",
            "states": [
                { "name": "Activate", "ticks": 1, "timeout": 1 },
                { "name": "Armed", "ticks": 48, "wait": false, "timeout": 1, "up": 2,
                  "arm": "spearReady", "arm_once": true },
                { "name": "Fire", "ticks": 60, "timeout": 0, "arm": "spearThrow" }
            ]
        },
        "nade:image/mortar": {
            "projectile": "nade:projectile/grenade",
            "shot": { "lob": { "speed": 15, "range": 200, "otherwise": 80, "distance_divisor": 4,
                               "jitter_steps": [2, 2], "jitter_divisor": [4, 4] } },
            "states": [
                { "name": "Ready", "down": 1 },
                { "name": "Fire", "ticks": 2, "timeout": 0, "script": "onFire" }
            ]
        },
        "nade:image/pistol": {
            "projectile": "nade:projectile/grenade",
            "states": [ { "name": "Ready" } ]
        }
    },
    "projectiles": {
        "nade:projectile/grenade": { "speed": 20, "ballistic": true, "lifetime_ticks": 240 },
        "nade:projectile/firebomb": { "speed": 20, "lifetime_ticks": 600, "explode_death": true,
            "children": { "projectile": "nade:projectile/ember", "count": 3, "max_count": 4,
                          "on_explode": true,
                          "steps": { "low": [-3, -3, -2], "high": [3, 3, 4],
                                     "offset": [-0.5, -0.5, -0.5], "step": [2.5, 2.5, -2.5] } } },
        "nade:projectile/flak": { "speed": 20, "ballistic": true, "lifetime_ticks": 300,
            "arm_ticks": 300, "elasticity": 0.9,
            "children": [
                { "projectile": "nade:projectile/spark", "count": 3, "speed": 10, "angles": true,
                  "every_ticks": 12, "max_times": 2 },
                { "projectile": "nade:projectile/spark", "count": 2, "max_count": 4, "redraw": true,
                  "speed": 100, "angles": true, "on_hit": true } ] },
        "nade:projectile/spark": { "speed": 5, "lifetime_ticks": 4 },
        "nade:projectile/ember": { "speed": 5, "ballistic": true, "lifetime_ticks": 1800,
            "aura": { "radius": 4, "damage": 4, "every_ticks": 36, "players_only": true,
                      "effect": "emberFlames", "target_sound": "nade:sound/burn",
                      "max_pulses": 2 } }
    }
}"#;

fn world() -> WeaponsWorld {
    let mut w = WeaponsWorld::new(Pack::from_json(KIT.as_bytes()).unwrap()).unwrap();
    w.add_actor(A, 5).unwrap();
    w
}

fn step(w: &mut WeaponsWorld, q: &mut impl Query, ticks: usize) -> Vec<Event> {
    (0..ticks).flat_map(|_| w.step(q)).collect()
}

/// Press, hold, let go and play the throw out.
fn throw(w: &mut WeaponsWorld, q: &mut Air) -> usize {
    w.trigger(A, true).unwrap();
    let mut events = step(w, q, 2);
    w.trigger(A, false).unwrap();
    events.extend(step(w, q, 80));
    events
        .iter()
        .filter(|e| matches!(e, Event::Spawned { definition, .. } if definition == "nade:projectile/grenade"))
        .count()
}

fn in_hand(w: &WeaponsWorld) -> bool {
    w.image_state(A, 0).is_some()
}

#[test]
fn a_grenade_counted_from_the_reserve_is_put_away_with_the_last_and_back_as_more_arrive() {
    let mut w = world();
    let mut q = Air::default();
    let slot = w.give(A, "nade:weapon/grenade").unwrap();
    w.equip(A, Some(slot)).unwrap();
    step(&mut w, &mut q, 4);
    let view = w.ammo(A).unwrap();
    assert!(view.counted && view.rounds == 2, "{view:?}");
    assert_eq!(view.reserve, Reserve::Rounds(2));

    assert_eq!(throw(&mut w, &mut q), 1);
    assert_eq!(w.reserve(A, "nades"), Some(Reserve::Rounds(1)));
    assert!(in_hand(&w), "one left: still in hand");
    assert!(
        !w.light_key(A).unwrap(),
        "nothing to reload: the key works the light"
    );

    assert_eq!(throw(&mut w, &mut q), 1);
    assert_eq!(w.reserve(A, "nades"), Some(Reserve::Rounds(0)));
    assert!(!in_hand(&w), "the last thrown, the hand is empty");
    assert_eq!(
        w.actor(A).unwrap().selected,
        Some(slot),
        "its tool stays selected"
    );
    let view = w.ammo(A).unwrap();
    assert!(
        view.counted && view.rounds == 0,
        "the display shows none left"
    );
    assert_eq!(throw(&mut w, &mut q), 0, "nothing to throw");

    // Drawn again with none, it goes straight back.
    w.equip(A, Some(slot)).unwrap();
    step(&mut w, &mut q, 4);
    assert!(!in_hand(&w));

    // A grenade bag: back in hand, and it throws.
    w.give_ammo(A, "nades", 1).unwrap();
    assert!(in_hand(&w), "more arrived: back in hand");
    step(&mut w, &mut q, 4);
    assert_eq!(throw(&mut w, &mut q), 1);
    assert!(!in_hand(&w));

    // Another tool drawn: ammo for the grenade does not take it out.
    let pistol = w.give(A, "nade:weapon/pistol").unwrap();
    w.equip(A, Some(pistol)).unwrap();
    step(&mut w, &mut q, 2);
    w.give_ammo(A, "nades", 1).unwrap();
    assert_eq!(
        w.image_state(A, 0).map(|(i, _)| i.id.clone()),
        Some("nade:image/pistol".to_string())
    );
    assert!(w.ammo(A).is_none());
}

#[test]
fn a_grenade_cleared_when_out_leaves_the_inventory_with_the_last() {
    // Tier's Clear Unusable Grenades: the last thrown, its tool goes too.
    let kit = KIT.replace(
        "\"from_reserve\": true",
        "\"from_reserve\": true, \"clear_when_out\": true",
    );
    let mut w = WeaponsWorld::new(Pack::from_json(kit.as_bytes()).unwrap()).unwrap();
    w.add_actor(A, 5).unwrap();
    let mut q = Air::default();
    let slot = w.give(A, "nade:weapon/grenade").unwrap();
    w.equip(A, Some(slot)).unwrap();
    step(&mut w, &mut q, 4);
    assert_eq!(throw(&mut w, &mut q), 1);
    assert_eq!(
        w.actor(A).unwrap().inventory[slot].as_deref(),
        Some("nade:weapon/grenade")
    );
    assert_eq!(throw(&mut w, &mut q), 1);
    let a = w.actor(A).unwrap();
    assert!(a.inventory[slot].is_none() && a.selected.is_none());
    assert!(!in_hand(&w));
    // Only a grenade counted from its reserve can be.
    let bad = KIT.replace("\"from_reserve\": true", "\"clear_when_out\": true");
    assert!(Pack::from_json(bad.as_bytes()).is_err());
}

#[test]
fn endless_grenades_never_run_out() {
    let mut w = world();
    let mut q = Air::default();
    let slot = w.give(A, "nade:weapon/grenade").unwrap();
    w.equip(A, Some(slot)).unwrap();
    step(&mut w, &mut q, 4);
    w.set_reserve(A, "nades", Reserve::Endless).unwrap();
    for _ in 0..4 {
        assert_eq!(throw(&mut w, &mut q), 1);
    }
    assert!(in_hand(&w));
}

#[test]
fn embers_scatter_along_each_axis_and_burn_only_the_players_near() {
    let mut w = world();
    let mut q = Air {
        near: vec![
            Nearby {
                target: TargetId::Actor(B),
                center: Vec3::ZERO,
                distance: 1.0,
            },
            Nearby {
                target: TargetId::Vehicle(9),
                center: Vec3::ONE,
                distance: 1.0,
            },
        ],
    };
    let mut axes = Vec::new();
    // Burst each firebomb where it is.
    for n in 0..24 {
        w.spawn(
            "nade:projectile/firebomb",
            A,
            Vec3::new(n as f32 * 10.0, 0.0, 0.0),
            Vec3::ZERO,
            1.0,
        )
        .unwrap();
    }
    let events = step(&mut w, &mut Air::default(), 601);
    let mut total = 0;
    for e in &events {
        if let Event::Spawned {
            definition,
            velocity,
            ..
        } = e
            && definition == "nade:projectile/ember"
        {
            for (axis, v) in velocity.to_array().into_iter().enumerate() {
                let whole = v / [2.5, 2.5, -2.5][axis] + 0.5;
                let (low, high) = [(-3.0, 3.0), (-3.0, 3.0), (-2.0, 4.0)][axis];
                assert!(
                    (whole - whole.round()).abs() < 1e-3 && (low..=high).contains(&whole.round()),
                    "axis {axis} at {v}"
                );
                axes.push((axis, whole.round() as i32));
            }
            total += 1;
        }
    }
    assert!(
        (72..=96).contains(&total) && total != 72 && total != 96,
        "3 or 4 each, both seen: {total}"
    );
    for axis in 0..3 {
        let seen: std::collections::BTreeSet<_> = axes
            .iter()
            .filter(|(a, _)| *a == axis)
            .map(|(_, w)| *w)
            .collect();
        assert_eq!(seen.len(), 7, "axis {axis} takes every step: {seen:?}");
    }

    // One ember beside a player and a vehicle.
    let mut w = world();
    w.add_actor(B, 5).unwrap();
    w.spawn("nade:projectile/ember", A, Vec3::ZERO, Vec3::ZERO, 1.0)
        .unwrap();
    let events = step(&mut w, &mut q, 36);
    let hurt: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Damage { target, .. } => Some(*target),
            _ => None,
        })
        .collect();
    assert_eq!(hurt, [TargetId::Actor(B)], "players only");
    assert!(events.iter().any(|e| matches!(e,
        Event::Effect { source: TargetId::Actor(t), definition, .. } if *t == B && definition == "emberFlames")));
    assert!(events.iter().any(|e| matches!(e,
        Event::Heard { actor, profile } if *actor == B && profile == "nade:sound/burn")));
    assert!(!events.iter().any(|e| matches!(e, Event::Burn { .. })));
    let later = step(&mut w, &mut q, 36 * 4);
    let pulses = later
        .iter()
        .filter(|e| matches!(e, Event::Damage { .. }))
        .count();
    assert_eq!(pulses, 1, "two pulses in all, then it only glows");
}

#[test]
fn an_arm_played_once_is_not_restarted_as_its_state_loops() {
    let mut w = world();
    let mut q = Air::default();
    let slot = w.give(A, "nade:weapon/molotov").unwrap();
    w.trigger(A, true).unwrap();
    w.equip(A, Some(slot)).unwrap();
    let arms = |events: &[Event]| {
        events
            .iter()
            .filter_map(|e| match e {
                Event::Animation {
                    thread: 2,
                    sequence,
                    ..
                } => Some(sequence.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let held = step(&mut w, &mut q, 200);
    assert_eq!(arms(&held), ["spearReady"], "raised once while held up");
    w.trigger(A, false).unwrap();
    let thrown = step(&mut w, &mut q, 2);
    assert_eq!(arms(&thrown), ["spearThrow"]);
    w.trigger(A, true).unwrap();
    let again = step(&mut w, &mut q, 120);
    assert_eq!(arms(&again), ["spearReady"], "raised again after the throw");
}

#[test]
fn counted_magazines_and_scattered_children_are_checked() {
    let reloads = KIT.replace(
        "\"from_reserve\": true,",
        "\"from_reserve\": true, \"one_by_one\": true,",
    );
    assert!(Pack::from_json(reloads.as_bytes()).is_err());
    let few = KIT.replace("\"max_count\": 4", "\"max_count\": 2");
    assert!(Pack::from_json(few.as_bytes()).is_err());
    let falling = KIT.replace("\"low\": [-3, -3, -2]", "\"low\": [-3, 4, -2]");
    assert!(Pack::from_json(falling.as_bytes()).is_err());
    let fast = KIT.replace("\"step\": [2.5", "\"step\": [300");
    assert!(Pack::from_json(fast.as_bytes()).is_err());
    assert!(Pack::from_json(KIT.as_bytes()).is_ok());
}

/// A floor at height 0 and nothing else.
struct Floor;
impl Query for Floor {
    fn sweep(&mut self, from: Vec3, to: Vec3, _: Filter) -> Option<Hit> {
        (from.y >= 0.0 && to.y < 0.0).then(|| {
            let fraction = from.y / (from.y - to.y);
            Hit {
                target: TargetId::Map(0),
                position: from.lerp(to, fraction),
                normal: Vec3::Y,
                fraction,
                color: None,
            }
        })
    }
    fn radius(&mut self, _: Vec3, _: f32, _: usize) -> Vec<Nearby> {
        Vec::new()
    }
    fn can_affect(&self, _: ActorId, _: TargetId) -> bool {
        true
    }
    fn can_catch(&self, _: ActorId, _: ActorId) -> bool {
        false
    }
}

fn sparks(events: &[Event]) -> Vec<Vec3> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Spawned {
                definition,
                velocity,
                ..
            } if definition == "nade:projectile/spark" => Some(*velocity),
            _ => None,
        })
        .collect()
}

/// `(cos a, cos b, sin a)` at `speed`, `a` a whole number of degrees.
fn at_kais_angles(v: Vec3, speed: f32) {
    let v = v / speed;
    assert!((v.x * v.x + v.z * v.z - 1.0).abs() < 1e-3, "{v}");
    assert!(v.y.abs() <= 1.0 + 1e-4, "{v}");
    let degrees = v.z.atan2(v.x).to_degrees();
    assert!((degrees - degrees.round()).abs() < 0.05, "{degrees}");
}

#[test]
fn flak_sparks_fly_at_kais_angles_and_burst_on_each_hit() {
    // In the air: three at each of its first two loops, then none, and
    // none as it dies without hitting anything.
    let mut w = world();
    w.spawn(
        "nade:projectile/flak",
        A,
        Vec3::new(0.0, 500.0, 0.0),
        Vec3::ZERO,
        1.0,
    )
    .unwrap();
    let mut by_tick = Vec::new();
    for tick in 1..=320 {
        let n = sparks(&w.step(&mut Floor));
        if n.is_empty() {
            continue;
        }
        n.iter().for_each(|v| at_kais_angles(*v, 10.0));
        by_tick.push((tick, n.len()));
    }
    assert_eq!(by_tick.len(), 2, "{by_tick:?}");
    assert!(by_tick.iter().all(|(_, n)| *n == 3), "{by_tick:?}");
    assert_eq!(by_tick[1].0 - by_tick[0].0, 12, "{by_tick:?}");

    // At each hit: two to four, the limit drawn again past two, so four
    // comes up least.
    let mut counts = [0; 5];
    let mut ups = 0;
    for n in 0..300 {
        let mut w = world();
        step(&mut w, &mut Air::default(), n);
        w.spawn(
            "nade:projectile/flak",
            A,
            Vec3::new(0.0, 0.05, 0.0),
            Vec3::new(0.0, -20.0, 0.0),
            1.0,
        )
        .unwrap();
        let burst = sparks(&w.step(&mut Floor));
        for v in &burst {
            at_kais_angles(*v, 100.0);
            ups += usize::from(v.y > 0.0);
        }
        counts[burst.len()] += 1;
    }
    assert_eq!(counts[0] + counts[1], 0, "{counts:?}");
    let [two, three, four] = [counts[2], counts[3], counts[4]];
    assert!(four < two && two < three, "1/3, 4/9, 2/9: {counts:?}");
    assert!(four < 84, "a fair draw would give four a third: {counts:?}");
    assert!(ups > 0, "flung every way");
}

/// The look meets something `at`, seen from `eye`; nothing else is hit.
struct Look {
    eye: Vec3,
    at: Option<Vec3>,
}
impl Query for Look {
    fn sweep(&mut self, from: Vec3, to: Vec3, _: Filter) -> Option<Hit> {
        let at = self.at?;
        let reaches = from.distance(at) <= from.distance(to);
        (from == self.eye && from.distance(to) > 100.0 && reaches).then(|| Hit {
            target: TargetId::Map(0),
            position: at,
            normal: Vec3::Y,
            fraction: from.distance(at) / from.distance(to),
            color: None,
        })
    }
    fn radius(&mut self, _: Vec3, _: f32, _: usize) -> Vec<Nearby> {
        Vec::new()
    }
    fn can_affect(&self, _: ActorId, _: TargetId) -> bool {
        true
    }
    fn can_catch(&self, _: ActorId, _: ActorId) -> bool {
        false
    }
}

#[test]
fn a_lob_comes_down_by_how_far_the_look_lands() {
    let eye = Vec3::new(0.0, 2.0, 0.0);
    let lob = |at: Option<Vec3>, scale: f32| {
        let mut w = world();
        let slot = w.give(A, "nade:weapon/mortar").unwrap();
        w.equip(A, Some(slot)).unwrap();
        w.set_frame(
            A,
            Frame {
                eye,
                muzzle: [Vec3::new(0.3, 1.5, -0.5); 2],
                scale,
                ..Frame::default()
            },
        )
        .unwrap();
        let mut q = Look { eye, at };
        step(&mut w, &mut q, 2);
        w.trigger(A, true).unwrap();
        let mut events = step(&mut w, &mut q, 1);
        w.trigger(A, false).unwrap();
        events.extend(step(&mut w, &mut q, 3));
        let shells: Vec<Vec3> = events
            .iter()
            .filter_map(|e| match e {
                Event::Spawned {
                    definition,
                    velocity,
                    ..
                } if definition == "nade:projectile/grenade" => Some(*velocity),
                _ => None,
            })
            .collect();
        assert_eq!(shells.len(), 1);
        shells[0]
    };
    let jittered = |v: f32| [0.0, 0.25, 0.5].iter().any(|j| (v - j).abs() < 1e-4);
    // 20 from the feet: up 20 / 4, ahead 15, and a quarter-step jitter
    // along the world's x and -z.
    let v = lob(Some(Vec3::new(0.0, 0.0, -20.0)), 1.0);
    assert!((v.y - 5.0).abs() < 1e-4, "{v}");
    assert!(jittered(v.x) && jittered(-v.z - 15.0), "{v}");
    // Nothing in reach: 80.
    let v = lob(None, 1.0);
    assert!((v.y - 20.0).abs() < 1e-4, "{v}");
    // A bigger holder reaches further, and throws no harder.
    let v = lob(Some(Vec3::new(0.0, 0.0, -300.0)), 2.0);
    assert!((v.y - 75.0).abs() < 1e-3, "{v}");
    let v = lob(Some(Vec3::new(0.0, 0.0, -300.0)), 1.0);
    assert!(
        (v.y - 20.0).abs() < 1e-4,
        "out of a smaller holder's reach: {v}"
    );
}

#[test]
fn an_aura_that_stops_at_its_first_target_hurts_one_a_pulse() {
    // Kai's molotov with its targeting fix turned off breaks out of its
    // search after the first player it burns.
    let kit = KIT.replace("\"max_pulses\": 2", "\"max_pulses\": 2, \"max_targets\": 1");
    let mut w = WeaponsWorld::new(Pack::from_json(kit.as_bytes()).unwrap()).unwrap();
    w.add_actor(A, 5).unwrap();
    let near = |target| Nearby {
        target,
        center: Vec3::ZERO,
        distance: 1.0,
    };
    let mut q = Air {
        near: vec![
            near(TargetId::Vehicle(9)),
            near(TargetId::Actor(B)),
            near(TargetId::Actor(ActorId(3))),
        ],
    };
    w.spawn("nade:projectile/ember", A, Vec3::ZERO, Vec3::ZERO, 1.0)
        .unwrap();
    let hurt: Vec<_> = step(&mut w, &mut q, 36)
        .into_iter()
        .filter_map(|e| match e {
            Event::Damage { target, .. } => Some(target),
            _ => None,
        })
        .collect();
    assert_eq!(
        hurt,
        [TargetId::Actor(B)],
        "the vehicle passed by, then one"
    );
    let bad = KIT.replace(
        "\"max_pulses\": 2",
        "\"max_pulses\": 2, \"max_targets\": 65",
    );
    assert!(Pack::from_json(bad.as_bytes()).is_err());
}

/// Teammates of the thrower that friendly fire spares, and an enemy.
struct Team {
    near: Vec<Nearby>,
}
impl Query for Team {
    fn sweep(&mut self, _: Vec3, _: Vec3, _: Filter) -> Option<Hit> {
        None
    }
    fn radius(&mut self, _: Vec3, _: f32, _: usize) -> Vec<Nearby> {
        self.near.clone()
    }
    fn can_affect(&self, _: ActorId, target: TargetId) -> bool {
        target == TargetId::Actor(ActorId(3))
    }
    fn can_catch(&self, _: ActorId, _: ActorId) -> bool {
        false
    }
    fn is_ally(&self, _: ActorId, target: TargetId) -> bool {
        matches!(target, TargetId::Actor(t) if t != ActorId(3))
    }
}

#[test]
fn an_aura_with_ally_damage_sears_teammates_friendly_fire_spares() {
    // Kai's Molotov Friendly Fire Override: allies take 1, not 4, the
    // thrower none; without it friendly fire spares them.
    let hurt = |kit: &str| {
        let mut w = WeaponsWorld::new(Pack::from_json(kit.as_bytes()).unwrap()).unwrap();
        w.add_actor(A, 5).unwrap();
        let near = |target| Nearby {
            target,
            center: Vec3::ZERO,
            distance: 1.0,
        };
        let mut q = Team {
            near: vec![
                near(TargetId::Actor(A)),
                near(TargetId::Actor(B)),
                near(TargetId::Actor(ActorId(3))),
            ],
        };
        w.spawn("nade:projectile/ember", A, Vec3::ZERO, Vec3::ZERO, 1.0)
            .unwrap();
        step(&mut w, &mut q, 36)
            .into_iter()
            .filter_map(|e| match e {
                Event::Damage { target, amount, .. } => Some((target, amount)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let enemy = (TargetId::Actor(ActorId(3)), 4.0);
    assert_eq!(hurt(KIT), [enemy]);
    let kit = KIT.replace("\"max_pulses\": 2", "\"max_pulses\": 2, \"ally_damage\": 1");
    assert_eq!(hurt(&kit), [(TargetId::Actor(B), 1.0), enemy]);
    let bad = KIT.replace(
        "\"max_pulses\": 2",
        "\"max_pulses\": 2, \"ally_damage\": 101",
    );
    assert!(Pack::from_json(bad.as_bytes()).is_err());
}
