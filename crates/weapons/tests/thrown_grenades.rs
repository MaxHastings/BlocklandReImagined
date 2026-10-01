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
        "nade:weapon/molotov": { "ui_name": "Molotov", "image": "nade:image/molotov" }
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

fn step(w: &mut WeaponsWorld, q: &mut Air, ticks: usize) -> Vec<Event> {
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
