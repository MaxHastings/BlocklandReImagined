//! A projectile with `stick_to_bodies` sticks to the player or vehicle it
//! hits and rides along as it moves. The pack is written here; it is our own.
use bri_weapons::*;
use glam::{Quat, Vec3};

const A: ActorId = ActorId(1);
const B: ActorId = ActorId(2);

/// Open air with one body to hit: a ball of radius 0.5 at `body`, which is
/// player `B`, or a vehicle when `vehicle` holds its pose.
struct World {
    body: Vec3,
    vehicle: Option<(Vec3, Quat)>,
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
            target: match self.vehicle {
                Some(_) => TargetId::Vehicle(7),
                None => TargetId::Actor(B),
            },
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
    fn vehicle_frame(&mut self, id: u64) -> Option<(Vec3, Quat)> {
        (id == 7).then_some(self.vehicle).flatten()
    }
}

const GRENADE: &str = r#"{
    "schema_version": 5,
    "id": "sticky",
    "items": { "sticky:weapon/grenade": { "ui_name": "Grenade", "image": "sticky:image/grenade" } },
    "images": {
        "sticky:image/grenade": {
            "projectile": "sticky:projectile/grenade",
            "states": [
                { "name": "Activate", "ticks": 4, "timeout": 1 },
                { "name": "Ready", "down": 2 },
                { "name": "Fire", "ticks": 6, "timeout": 3, "script": "onFire" },
                { "name": "Wait", "up": 1 }
            ]
        }
    },
    "projectiles": { "sticky:projectile/grenade": {
        "speed": 30, "lifetime_ticks": 600, "arm_ticks": 600, "ballistic": true,
        "gravity": 0, "collide_players": true, "stick_to_bodies": true } }
}"#;

fn frame(at: Vec3, yaw: f32) -> Frame {
    Frame {
        position: at,
        eye: at,
        middle: Some(at),
        body_yaw: yaw,
        ..Frame::default()
    }
}

/// Throw at a body 10 units ahead and let it land: the world and the
/// grenade's id.
fn throw(world: &mut World) -> (WeaponsWorld, u64) {
    let mut w = WeaponsWorld::new(Pack::from_json(GRENADE.as_bytes()).unwrap()).unwrap();
    w.add_actor(A, 5).unwrap();
    w.add_actor(B, 5).unwrap();
    let eye = Vec3::new(0.0, 1.0, 0.0);
    w.set_frame(
        A,
        Frame {
            direction: Vec3::NEG_Z,
            muzzle: [eye; 2],
            ..frame(eye, 0.0)
        },
    )
    .unwrap();
    w.set_frame(B, frame(world.body, 0.0)).unwrap();
    let slot = w.give(A, "sticky:weapon/grenade").unwrap();
    w.equip(A, Some(slot)).unwrap();
    for _ in 0..8 {
        w.step(world);
    }
    w.trigger(A, true).unwrap();
    for _ in 0..80 {
        w.step(world);
    }
    let p = w.projectiles().next().expect("the grenade lives");
    assert!(p.stuck && p.stuck_to.is_some(), "{p:?}");
    let id = p.id;
    (w, id)
}

fn position(w: &WeaponsWorld, id: u64) -> Vec3 {
    w.projectiles().find(|p| p.id == id).unwrap().position
}

#[test]
fn a_sticky_grenade_rides_on_the_player_it_hits() {
    let mut world = World {
        body: Vec3::new(0.0, 1.0, -10.0),
        vehicle: None,
    };
    let (mut w, id) = throw(&mut world);
    let stuck = position(&w, id);
    assert!(stuck.distance(world.body) < 0.6, "{stuck}");
    // The player walks 5 units right and turns half round: it goes along.
    let moved = Vec3::new(5.0, 1.0, -10.0);
    w.set_frame(B, frame(moved, std::f32::consts::PI)).unwrap();
    world.body = Vec3::new(100.0, 0.0, 0.0);
    w.step(&mut world);
    let offset = stuck - Vec3::new(0.0, 1.0, -10.0);
    let expected = moved + Quat::from_rotation_y(-std::f32::consts::PI) * offset;
    assert!(
        position(&w, id).distance(expected) < 1e-3,
        "{}",
        position(&w, id)
    );
    // Gone from the game: it stays where it was last.
    w.remove_actor(B);
    w.step(&mut world);
    assert!(position(&w, id).distance(expected) < 1e-3);
}

#[test]
fn a_sticky_grenade_rides_on_the_vehicle_it_hits() {
    let pose = (Vec3::new(0.0, 1.0, -10.0), Quat::IDENTITY);
    let mut world = World {
        body: pose.0,
        vehicle: Some(pose),
    };
    let (mut w, id) = throw(&mut world);
    let stuck = position(&w, id);
    let turned = (Vec3::new(0.0, 1.0, -30.0), Quat::from_rotation_y(0.5));
    world.vehicle = Some(turned);
    w.step(&mut world);
    let expected = turned.0 + turned.1 * (stuck - pose.0);
    assert!(
        position(&w, id).distance(expected) < 1e-3,
        "{}",
        position(&w, id)
    );
}
