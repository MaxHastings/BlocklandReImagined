//! A projectile's `impacts`: the explosion effect it makes by what it lands
//! on (sparks off a vehicle, dust off the ground), for a ray and a flying
//! round alike, with `explosion.effect` wherever `impacts` names nothing.
//! The pack is written here; it is our own.
use bri_weapons::*;
use glam::Vec3;

const A: ActorId = ActorId(1);
const B: ActorId = ActorId(2);

/// `target` 20 units ahead down -Z, facing back.
struct World {
    target: TargetId,
}
impl Query for World {
    fn sweep(&mut self, from: Vec3, to: Vec3, _: Filter) -> Option<Hit> {
        let at = Vec3::new(0.0, from.y, -20.0);
        (to.z < -20.0 && from.z >= -20.0).then(|| Hit {
            target: self.target,
            position: at,
            normal: Vec3::Z,
            fraction: (from - at).length() / (from - to).length(),
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

const GUNS: &str = r#"{
    "schema_version": 5,
    "id": "imp",
    "items": {
        "imp:weapon/rifle": { "ui_name": "Rifle", "image": "imp:image/rifle" },
        "imp:weapon/cannon": { "ui_name": "Cannon", "image": "imp:image/cannon" }
    },
    "images": {
        "imp:image/rifle": {
            "projectile": "imp:projectile/bullet",
            "shot": { "projectiles": 1, "hitscan": { "range": 200 } },
            "states": [
                { "name": "Activate", "ticks": 4, "timeout": 1 },
                { "name": "Ready", "down": 2 },
                { "name": "Fire", "ticks": 6, "timeout": 3, "script": "onFire" },
                { "name": "Wait", "up": 1 }
            ]
        },
        "imp:image/cannon": {
            "projectile": "imp:projectile/shell",
            "states": [
                { "name": "Activate", "ticks": 4, "timeout": 1 },
                { "name": "Ready", "down": 2 },
                { "name": "Fire", "ticks": 6, "timeout": 3, "script": "onFire" },
                { "name": "Wait", "up": 1 }
            ]
        }
    },
    "projectiles": {
        "imp:projectile/bullet": {
            "speed": 200, "lifetime_ticks": 120, "damage": 5, "collide_players": true,
            "explosion": { "effect": "puff" },
            "impacts": { "vehicle": "sparks", "map": "dust" }
        },
        "imp:projectile/shell": {
            "speed": 100, "lifetime_ticks": 120, "collide_players": true, "explode_player": true,
            "explosion": { "effect": "puff" },
            "impacts": { "vehicle": "sparks", "brick": "chips" }
        }
    }
}"#;

/// Fire `item` once at `target` and return the explosion effects played.
fn effects(item: &str, target: TargetId) -> Vec<String> {
    let mut w = WeaponsWorld::new(Pack::from_json(GUNS.as_bytes()).unwrap()).unwrap();
    w.add_actor(A, 5).unwrap();
    w.add_actor(B, 5).unwrap();
    let slot = w.give(A, item).unwrap();
    w.equip(A, Some(slot)).unwrap();
    let mut world = World { target };
    for _ in 0..8 {
        w.step(&mut world);
    }
    w.trigger(A, true).unwrap();
    let mut events = Vec::new();
    for _ in 0..2 {
        events.extend(w.step(&mut world));
    }
    w.trigger(A, false).unwrap();
    for _ in 0..60 {
        events.extend(w.step(&mut world));
    }
    events
        .into_iter()
        .filter_map(|e| match e {
            Event::Effect {
                definition,
                position,
                ..
            } if (position.z + 20.0).abs() < 0.01 => Some(definition),
            _ => None,
        })
        .collect()
}

#[test]
fn a_ray_makes_the_effect_named_for_what_it_lands_on() {
    let rifle = "imp:weapon/rifle";
    assert_eq!(effects(rifle, TargetId::Vehicle(7)), ["sparks"]);
    assert_eq!(effects(rifle, TargetId::Map(0)), ["dust"]);
    // A surface it names nothing for keeps the explosion's own effect.
    assert_eq!(effects(rifle, TargetId::Brick(3)), ["puff"]);
    assert_eq!(effects(rifle, TargetId::Actor(B)), ["puff"]);
}

#[test]
fn a_flying_round_makes_the_effect_named_for_what_it_hits() {
    let cannon = "imp:weapon/cannon";
    assert_eq!(effects(cannon, TargetId::Vehicle(7)), ["sparks"]);
    assert_eq!(effects(cannon, TargetId::Brick(3)), ["chips"]);
    assert_eq!(effects(cannon, TargetId::Map(0)), ["puff"]);
}

#[test]
fn impacts_with_unknown_surfaces_are_refused() {
    let bad = GUNS.replace(r#""map": "dust""#, r#""water": "splash""#);
    assert!(Pack::from_json(bad.as_bytes()).is_err());
}
