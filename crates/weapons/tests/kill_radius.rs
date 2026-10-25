//! A projectile's explosion may kill outright within a radius of its
//! centre (`explosion.kill_radius`), whatever its falloff would give. The
//! pack is written here; it is our own.
use bri_weapons::*;
use glam::Vec3;

const A: ActorId = ActorId(1);
const B: ActorId = ActorId(2);

/// Open air with one body, player `B`, which every blast reaches.
struct World {
    body: Vec3,
}
impl Query for World {
    /// A wall across the way at z = -10.
    fn sweep(&mut self, start: Vec3, end: Vec3, _: Filter) -> Option<Hit> {
        let fraction = (start.z + 10.0) / (start.z - end.z);
        (0.0..=1.0).contains(&fraction).then(|| Hit {
            target: TargetId::Map(0),
            position: start + (end - start) * fraction,
            normal: Vec3::Z,
            fraction,
            color: None,
        })
    }
    fn radius(&mut self, at: Vec3, _: f32, _: usize) -> Vec<Nearby> {
        vec![Nearby {
            target: TargetId::Actor(B),
            center: self.body,
            distance: self.body.distance(at),
        }]
    }
    fn can_affect(&self, _: ActorId, _: TargetId) -> bool {
        true
    }
    fn can_catch(&self, _: ActorId, _: ActorId) -> bool {
        false
    }
}

const PACK: &str = r#"{
    "schema_version": 5,
    "id": "boom",
    "items": { "boom:weapon/tube": { "ui_name": "Tube", "image": "boom:image/tube" } },
    "images": {
        "boom:image/tube": {
            "projectile": "boom:projectile/rocket",
            "states": [
                { "name": "Activate", "ticks": 4, "timeout": 1 },
                { "name": "Ready", "down": 2 },
                { "name": "Fire", "ticks": 6, "timeout": 3, "script": "onFire" },
                { "name": "Wait", "up": 1 }
            ]
        }
    },
    "projectiles": { "boom:projectile/rocket": {
        "speed": 200, "lifetime_ticks": 300, "gravity": 0, "damage_type": "$DamageType::Boom", "radius_damage_type": "$DamageType::Boom",
        "explosion": { "damage": 50, "radius": 10, "kill_radius": 2 } } },
    "damage_types": { "boom": { "name": "Boom", "suicide_message": "%1 blew up",
                                "murder_message": "%2 blew up %1", "vehicle_scale": 1.0, "direct": false } }
}"#;

fn frame(at: Vec3) -> Frame {
    Frame {
        position: at,
        eye: at,
        middle: Some(at),
        ..Frame::default()
    }
}

/// Fire the rocket with the body at `body`; what the blast did.
fn blast(body: Vec3) -> Vec<Event> {
    let mut world = World { body };
    let mut w = WeaponsWorld::new(Pack::from_json(PACK.as_bytes()).unwrap()).unwrap();
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
    w.set_frame(B, frame(body)).unwrap();
    let slot = w.give(A, "boom:weapon/tube").unwrap();
    w.equip(A, Some(slot)).unwrap();
    for _ in 0..8 {
        w.step(&mut world);
    }
    w.trigger(A, true).unwrap();
    w.step(&mut world);
    w.trigger(A, false).unwrap();
    (0..200).flat_map(|_| w.step(&mut world)).collect()
}

fn centre(events: &[Event]) -> Vec3 {
    events
        .iter()
        .find_map(|e| match e {
            Event::Blast { position, .. } => Some(*position),
            _ => None,
        })
        .unwrap_or_else(|| panic!("the rocket goes off: {events:?}"))
}

fn dealt(events: &[Event]) -> Option<f32> {
    events.iter().find_map(|e| match e {
        Event::Damage { amount, .. } => Some(*amount),
        _ => None,
    })
}

#[test]
fn a_blast_kills_within_its_kill_radius_and_falls_off_beyond_it() {
    // Learn where it goes off with the body out of the way.
    let at = centre(&blast(Vec3::new(500.0, 1.0, 500.0)));
    let inside = dealt(&blast(at + Vec3::X)).expect("hurt");
    assert!(inside >= KILL_RADIUS_DAMAGE, "{inside}");
    let beyond = dealt(&blast(at + Vec3::X * 4.0)).expect("hurt");
    assert!(beyond < 50.0, "{beyond}");
}
