//! A gun whose spread builds while it is fired and settles when the holder
//! stops (Halo's trigger error): the first shot of a burst lands at the
//! smallest spread, held fire reaches the widest, and a rest brings it back.
//! The pack is written here; it is our own.
use bri_weapons::*;
use glam::Vec3;

const A: ActorId = ActorId(1);

struct Air;
impl Query for Air {
    fn sweep(&mut self, _: Vec3, _: Vec3, _: Filter) -> Option<Hit> {
        None
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

/// A pistol firing every 34 ticks, spread 0 at error 0 to 0.2 at error 1,
/// rising in 60 ticks and settling in 24, recovering for one round's 34.
const PACK: &str = r#"{
    "schema_version": 5,
    "id": "err",
    "items": { "err:weapon/gun": { "ui_name": "Gun", "image": "err:image/gun" } },
    "images": {
        "err:image/gun": {
            "projectile": "err:projectile/ray",
            "shot": {
                "projectiles": 1,
                "error": { "min": 0.0, "max": 0.2, "rise_ticks": 60, "fall_ticks": 24, "recover_ticks": 34 },
                "hitscan": { "range": 200 }
            },
            "states": [
                { "name": "Activate", "ticks": 4, "timeout": 1 },
                { "name": "Ready", "down": 2 },
                { "name": "Fire", "ticks": 34, "timeout": 3, "script": "onFire" },
                { "name": "Hold", "up": 1, "timeout": 2 }
            ]
        }
    },
    "projectiles": { "err:projectile/ray": { "speed": 200, "lifetime_ticks": 120, "damage": 5 } }
}"#;

fn world() -> WeaponsWorld {
    let mut w = WeaponsWorld::new(Pack::from_json(PACK.as_bytes()).unwrap()).unwrap();
    w.add_actor(A, 5).unwrap();
    let slot = w.give(A, "err:weapon/gun").unwrap();
    w.equip(A, Some(slot)).unwrap();
    run(&mut w, 8);
    w
}
fn run(w: &mut WeaponsWorld, ticks: usize) -> Vec<Event> {
    (0..ticks).flat_map(|_| w.step(&mut Air)).collect()
}
fn ends(events: &[Event]) -> Vec<Vec3> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Tracer { to, .. } => Some(*to),
            _ => None,
        })
        .collect()
}

#[test]
fn held_fire_widens_the_spread_and_a_rest_settles_it() {
    let mut w = world();
    // The aim with no spread: a shot at error 0 lands exactly on it.
    w.trigger(A, true).unwrap();
    let held = ends(&run(&mut w, 34 * 6));
    assert!(held.len() >= 5, "{}", held.len());
    let straight = held[0];
    let off = |to: Vec3| (to - straight).length();
    // The first lands on the aim; the error is 0.57 by the second, the full
    // 1 from the third on: later shots stray.
    assert!(held[1..].iter().all(|to| off(*to) > 0.5), "{held:?}");
    // The third on share one error, so the cone, not the first shot, bounds
    // them: none strays past the widest spread's reach.
    let widest = 5.0 * std::f32::consts::PI * 0.2 / 2.0 * 200.0;
    assert!(held.iter().all(|to| off(*to) < widest * 2.0));
    // Let go: the error settles in 24 ticks, so a shot after a rest is the
    // first of a new burst, straight again.
    w.trigger(A, false).unwrap();
    run(&mut w, 120);
    w.trigger(A, true).unwrap();
    let after = ends(&run(&mut w, 8));
    w.trigger(A, false).unwrap();
    assert_eq!(after.len(), 1);
    assert!(off(after[0]) < 1e-3, "{:?} against {straight:?}", after[0]);
}

#[test]
fn a_quick_tap_leaves_the_error_part_risen() {
    let mut w = world();
    // One tap, then another as soon as the round allows: the second shot's
    // error is what the first's recovery built, not zero.
    w.trigger(A, true).unwrap();
    let mut shots = ends(&run(&mut w, 2));
    w.trigger(A, false).unwrap();
    run(&mut w, 40);
    w.trigger(A, true).unwrap();
    shots.extend(ends(&run(&mut w, 2)));
    w.trigger(A, false).unwrap();
    assert_eq!(shots.len(), 2);
    // After 34 ticks recovering and a few settling: well under full, above 0.
    assert!((shots[1] - shots[0]).length() > 1e-3);
}

#[test]
fn error_must_be_ordered_and_in_range() {
    let bad = PACK.replace("\"min\": 0.0, \"max\": 0.2", "\"min\": 0.5, \"max\": 0.2");
    assert!(Pack::from_json(bad.as_bytes()).is_err());
    let bad = PACK.replace("\"rise_ticks\": 60", "\"rise_ticks\": 0");
    assert!(Pack::from_json(bad.as_bytes()).is_err());
}

#[test]
fn a_hitscan_may_reach_a_long_way_but_not_without_bound() {
    let long = PACK.replace(
        "\"hitscan\": { \"range\": 200 }",
        "\"hitscan\": { \"range\": 4120 }",
    );
    assert!(Pack::from_json(long.as_bytes()).is_ok());
    let far = PACK.replace(
        "\"hitscan\": { \"range\": 200 }",
        "\"hitscan\": { \"range\": 9000 }",
    );
    assert!(Pack::from_json(far.as_bytes()).is_err());
}
