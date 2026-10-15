//! An image's `aim_assist` bends a shot onto a body the holder could hurt
//! near their aim, and leaves it alone for a body too far off the aim, out
//! of range or one it may not hurt. The pack is written here; it is our own.
use bri_weapons::*;
use glam::Vec3;

const A: ActorId = ActorId(1);
const B: ActorId = ActorId(2);

/// Open air; `B` may be hurt or not.
struct World {
    hurtable: bool,
}
impl Query for World {
    fn sweep(&mut self, _: Vec3, _: Vec3, _: Filter) -> Option<Hit> {
        None
    }
    fn radius(&mut self, _: Vec3, _: f32, _: usize) -> Vec<Nearby> {
        Vec::new()
    }
    fn can_affect(&self, _: ActorId, target: TargetId) -> bool {
        self.hurtable || target != TargetId::Actor(B)
    }
    fn can_catch(&self, _: ActorId, _: ActorId) -> bool {
        false
    }
}

const GUN: &str = r#"{
    "schema_version": 5,
    "id": "assist",
    "items": { "assist:weapon/rifle": { "ui_name": "Rifle", "image": "assist:image/rifle" } },
    "images": {
        "assist:image/rifle": {
            "projectile": "assist:projectile/round",
            "aim_assist": { "angle": 0.1, "range": 50, "slowdown": 0.5 },
            "shot": { "projectiles": 1, "hitscan": { "range": 100, "from_eye": true } },
            "states": [
                { "name": "Activate", "ticks": 4, "timeout": 1 },
                { "name": "Ready", "down": 2 },
                { "name": "Fire", "ticks": 6, "timeout": 3, "script": "onFire" },
                { "name": "Wait", "up": 1 }
            ]
        }
    },
    "projectiles": { "assist:projectile/round": { "speed": 200, "lifetime_ticks": 120,
                                                   "damage": 10, "collide_players": true } }
}"#;

/// Where the shot's ray ended, with `B`'s middle at `target`.
fn shoot(target: Vec3, hurtable: bool) -> Vec3 {
    let mut w = WeaponsWorld::new(Pack::from_json(GUN.as_bytes()).unwrap()).unwrap();
    w.add_actor(A, 5).unwrap();
    w.add_actor(B, 5).unwrap();
    let eye = Vec3::new(0.0, 2.0, 0.0);
    w.set_frame(
        A,
        Frame {
            eye,
            middle: Some(eye),
            ..Frame::default()
        },
    )
    .unwrap();
    w.set_frame(
        B,
        Frame {
            position: target,
            eye: target,
            middle: Some(target),
            ..Frame::default()
        },
    )
    .unwrap();
    let slot = w.give(A, "assist:weapon/rifle").unwrap();
    w.equip(A, Some(slot)).unwrap();
    let mut world = World { hurtable };
    for _ in 0..8 {
        w.step(&mut world);
    }
    w.trigger(A, true).unwrap();
    let events: Vec<_> = (0..4).flat_map(|_| w.step(&mut world)).collect();
    events
        .iter()
        .find_map(|e| match e {
            Event::Tracer { to, .. } => Some(*to),
            _ => None,
        })
        .expect("a shot")
}

#[test]
fn a_shot_bends_onto_a_body_near_the_aim_and_no_other() {
    let eye = Vec3::new(0.0, 2.0, 0.0);
    // 1.5 units beside the aim at 20 away: 0.075 radians, inside 0.1.
    let near = Vec3::new(1.5, 2.0, -20.0);
    let to = shoot(near, true);
    let bent = (to - eye).normalize();
    assert!(bent.angle_between((near - eye).normalize()) < 1e-3, "{to}");
    // 4 units beside: 0.2 radians, outside the cone.
    let straight = |to: Vec3| (to - eye).normalize().angle_between(Vec3::NEG_Z) < 1e-3;
    assert!(straight(shoot(Vec3::new(4.0, 2.0, -20.0), true)));
    // Beyond its 50-unit range.
    assert!(straight(shoot(Vec3::new(3.0, 2.0, -60.0), true)));
    // A body the holder may not hurt (a teammate, outside the game).
    assert!(straight(shoot(near, false)));
}
