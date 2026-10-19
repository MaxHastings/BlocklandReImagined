//! A homing projectile picks a target as it is made and turns toward it at
//! its turn rate, keeping its speed. The pack is written here; it is our own.
use bri_weapons::*;
use glam::Vec3;

const A: ActorId = ActorId(1);
const B: ActorId = ActorId(2);

/// Open air with one body to hit: a ball of radius 0.5 at `body`, player `B`.
struct World {
    body: Vec3,
}
impl Query for World {
    fn sweep(&mut self, start: Vec3, end: Vec3, _: Filter) -> Option<Hit> {
        let along = end - start;
        let t = bri_console::Clamp::clamped(
            (self.body - start).dot(along) / along.length_squared(),
            0.0,
            1.0,
        );
        let at = start + along * t;
        (at.distance(self.body) < 0.5).then(|| Hit {
            target: TargetId::Actor(B),
            position: at,
            normal: (start - self.body).normalize(),
            fraction: t,
            color: None,
        })
    }
    fn radius(&mut self, from: Vec3, range: f32, _: usize) -> Vec<Nearby> {
        let distance = from.distance(self.body);
        (distance <= range)
            .then_some(Nearby {
                target: TargetId::Actor(B),
                center: self.body,
                distance,
            })
            .into_iter()
            .collect()
    }
    fn can_affect(&self, _: ActorId, _: TargetId) -> bool {
        true
    }
    fn can_catch(&self, _: ActorId, _: ActorId) -> bool {
        false
    }
}

const GUN: &str = r#"{
    "schema_version": 5,
    "id": "seek",
    "items": { "seek:weapon/seeker": { "ui_name": "Seeker", "image": "seek:image/seeker" } },
    "images": {
        "seek:image/seeker": {
            "projectile": "seek:projectile/bolt",
            "states": [
                { "name": "Activate", "ticks": 4, "timeout": 1 },
                { "name": "Ready", "down": 2 },
                { "name": "Fire", "ticks": 6, "timeout": 3, "script": "onFire" },
                { "name": "Wait", "up": 1 }
            ]
        }
    },
    "projectiles": { "seek:projectile/bolt": {
        "speed": 30, "lifetime_ticks": 1200, "gravity": 0, "collide_players": true,
        "damage": 20, "damage_type": "$DamageType::Gun",
        "homing": { "turn_rate": 2.0, "range": 100, "angle": 0.5 } } },
    "damage_types": { "gun": { "name": "Gun", "suicide_message": "%1 shot themselves",
                               "murder_message": "%2 shot %1", "vehicle_scale": 1.0, "direct": true } }
}"#;

fn frame(at: Vec3) -> Frame {
    Frame {
        position: at,
        eye: at,
        middle: Some(at),
        ..Frame::default()
    }
}

/// Fire down -Z at a body `side` units to the right and 30 ahead; the
/// damage events and the speeds the bolt flew at.
fn fire(pack: &str, side: f32) -> (usize, Vec<f32>) {
    let mut world = World {
        body: Vec3::new(side, 1.0, -30.0),
    };
    let mut w = WeaponsWorld::new(Pack::from_json(pack.as_bytes()).unwrap()).unwrap();
    w.add_actor(A, 5).unwrap();
    w.add_actor(B, 5).unwrap();
    let eye = Vec3::new(0.0, 1.0, 0.0);
    w.set_frame(
        A,
        Frame {
            direction: Vec3::NEG_Z,
            muzzle: [eye; 2],
            ..frame(eye)
        },
    )
    .unwrap();
    w.set_frame(B, frame(world.body)).unwrap();
    let slot = w.give(A, "seek:weapon/seeker").unwrap();
    w.equip(A, Some(slot)).unwrap();
    for _ in 0..8 {
        w.step(&mut world);
    }
    w.trigger(A, true).unwrap();
    w.step(&mut world);
    w.trigger(A, false).unwrap();
    let mut hits = 0;
    let mut speeds = Vec::new();
    for _ in 0..600 {
        let events = w.step(&mut world);
        hits += events
            .iter()
            .filter(|e| matches!(e, Event::Damage { .. }))
            .count();
        speeds.extend(w.projectiles().map(|p| p.velocity.length()));
    }
    (hits, speeds)
}

#[test]
fn a_homing_bolt_curves_onto_a_target_a_straight_one_misses() {
    // 6 units off a line 30 long: 11 degrees, inside the 0.5 radian cone.
    let (hits, speeds) = fire(GUN, 6.0);
    assert_eq!(hits, 1, "it turns onto the target");
    assert!(
        speeds.iter().all(|s| (s - 30.0).abs() < 0.01),
        "turning keeps its speed: {speeds:?}"
    );
    let straight = GUN.replace(
        "\"homing\": { \"turn_rate\": 2.0, \"range\": 100, \"angle\": 0.5 }",
        "\"fixed_damage\": false",
    );
    assert_eq!(fire(&straight, 6.0).0, 0, "without homing it passes by");
}

#[test]
fn a_target_outside_the_cone_is_not_followed() {
    // 25 units off a line 30 long: 40 degrees, beyond 0.5 radians (29).
    assert_eq!(fire(GUN, 25.0).0, 0);
}

#[test]
fn homing_has_limits() {
    for bad in [
        "\"turn_rate\": 0.0",
        "\"turn_rate\": 21.0",
        "\"range\": 0.5",
        "\"angle\": 4.0",
    ] {
        let key = bad.split(':').next().unwrap();
        let from = match key {
            "\"turn_rate\"" => "\"turn_rate\": 2.0",
            "\"range\"" => "\"range\": 100",
            _ => "\"angle\": 0.5",
        };
        let json = GUN.replace(from, bad);
        assert!(Pack::from_json(json.as_bytes()).is_err(), "{bad}");
    }
}
