//! A magazine whose rounds are heat gets them back as time passes
//! (`cool_ticks`), and a projectile's direct damage changes with the
//! distance it has flown (`falloff`). The pack is written here; it is our own.
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

const GUN: &str = r#"{
    "schema_version": 5,
    "id": "heat",
    "items": { "heat:weapon/bolter": { "ui_name": "Bolter", "image": "heat:image/bolter" } },
    "images": {
        "heat:image/bolter": {
            "projectile": "heat:projectile/bolt",
            "magazine": { "size": 4, "ammo": "battery", "reload_ticks": 60, "reserve": 100,
                          "max_reserve": 100, "supply": "both", "cool_ticks": 30 },
            "states": [
                { "name": "Activate", "ticks": 4, "timeout": 1 },
                { "name": "Ready", "down": 2 },
                { "name": "Fire", "ticks": 6, "timeout": 3, "script": "onFire" },
                { "name": "Wait", "up": 1 }
            ]
        }
    },
    "projectiles": { "heat:projectile/bolt": {
        "speed": 100, "lifetime_ticks": 600, "gravity": 0, "collide_players": true,
        "damage": 20, "damage_type": "$DamageType::Gun",
        "falloff": { "near": 10, "far": 50, "far_damage": 5 } } },
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

fn armed(world: &mut World) -> WeaponsWorld {
    let mut w = WeaponsWorld::new(Pack::from_json(GUN.as_bytes()).unwrap()).unwrap();
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
    let slot = w.give(A, "heat:weapon/bolter").unwrap();
    w.equip(A, Some(slot)).unwrap();
    for _ in 0..8 {
        w.step(world);
    }
    w
}

fn shoot(w: &mut WeaponsWorld, world: &mut World, ticks: usize) -> Vec<Event> {
    w.trigger(A, true).unwrap();
    w.step(world);
    w.trigger(A, false).unwrap();
    (0..ticks).flat_map(|_| w.step(world)).collect()
}

fn damage_at(distance: f32) -> f32 {
    let mut world = World {
        body: Vec3::new(0.0, 1.0, -distance),
    };
    let mut w = armed(&mut world);
    let events = shoot(&mut w, &mut world, 300);
    events
        .iter()
        .find_map(|e| match e {
            Event::Damage { amount, .. } => Some(*amount),
            _ => None,
        })
        .expect("the bolt hits")
}

#[test]
fn damage_falls_between_the_near_and_far_distances() {
    assert!((damage_at(5.0) - 20.0).abs() < 0.5, "{}", damage_at(5.0));
    // Halfway between 10 and 50 units: halfway between 20 and 5.
    let middle = damage_at(30.0);
    assert!((middle - 12.5).abs() < 0.6, "{middle}");
    let far = damage_at(80.0);
    assert!((far - 5.0).abs() < 0.5, "{far}");
}

#[test]
fn a_cooling_magazine_gets_rounds_back_while_held_and_stops_when_full() {
    let mut world = World {
        body: Vec3::new(0.0, 1.0, -300.0),
    };
    let mut w = armed(&mut world);
    let rounds = |w: &WeaponsWorld| w.ammo(A).unwrap().rounds;
    assert_eq!(rounds(&w), 4);
    shoot(&mut w, &mut world, 10);
    shoot(&mut w, &mut world, 10);
    assert_eq!(rounds(&w), 2);
    // One back every 30 ticks after the first shot: not yet at 27 ticks,
    // there at 42.
    for _ in 0..5 {
        w.step(&mut world);
    }
    assert_eq!(rounds(&w), 2);
    for _ in 0..15 {
        w.step(&mut world);
    }
    assert_eq!(rounds(&w), 3);
    for _ in 0..200 {
        w.step(&mut world);
    }
    assert_eq!(rounds(&w), 4);
    // Nothing is refunded from the battery: the two shots stay spent.
    assert_eq!(w.ammo(A).unwrap().reserve, Reserve::Rounds(98));
}

#[test]
fn falloff_and_cooling_have_limits() {
    let bad = |from: &str, to: &str| {
        let json = GUN.replace(from, to);
        assert!(
            Pack::from_json(json.as_bytes()).is_err(),
            "{to} should be refused"
        );
    };
    bad("\"cool_ticks\": 30", "\"cool_ticks\": 1201");
    bad("\"near\": 10", "\"near\": 60");
    bad("\"far_damage\": 5", "\"far_damage\": 101");
}
