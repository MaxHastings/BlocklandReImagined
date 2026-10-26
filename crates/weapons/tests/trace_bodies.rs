//! A traced ray (`hitscan.trace`) may go on through a player's body
//! (`trace.players`), keeping a share of its strength, and loses more of it
//! the thicker the surface it crosses (`depth`). The pack is written here;
//! it is our own.
use bri_weapons::*;
use glam::Vec3;

const A: ActorId = ActorId(1);
const B: ActorId = ActorId(2);
const C: ActorId = ActorId(3);

/// Bodies as slabs 0.6 thick across the way down -Z: `B` in front at
/// z = -10, `C` behind it at z = -14.
struct World;
const SLABS: [(ActorId, f32); 2] = [(B, -10.0), (C, -14.0)];
impl Query for World {
    fn sweep(&mut self, from: Vec3, to: Vec3, _: Filter) -> Option<Hit> {
        let dz = to.z - from.z;
        SLABS
            .iter()
            .filter_map(|&(id, front)| {
                // The face met first: the front going -Z, the back going +Z.
                let z = if dz < 0.0 { front } else { front - 0.6 };
                let t = (z - from.z) / dz;
                (0.0..=1.0).contains(&t).then(|| Hit {
                    target: TargetId::Actor(id),
                    position: from + (to - from) * t,
                    normal: if dz < 0.0 { Vec3::Z } else { Vec3::NEG_Z },
                    fraction: t,
                    color: None,
                })
            })
            .min_by(|a, b| a.fraction.total_cmp(&b.fraction))
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

fn pack(players: &str) -> String {
    r#"{
    "schema_version": 5,
    "id": "thru",
    "items": { "thru:weapon/rifle": { "ui_name": "Rifle", "image": "thru:image/rifle" } },
    "images": {
        "thru:image/rifle": {
            "projectile": "thru:projectile/bullet",
            "shot": { "projectiles": 1, "spread": 0.0, "hitscan": { "range": 200,
                "trace": { "max_landings": 4, "floor": 0.1, "scatter": 0.0, "players": PLAYERS } } },
            "states": [
                { "name": "Activate", "ticks": 4, "timeout": 1 },
                { "name": "Ready", "down": 2 },
                { "name": "Fire", "ticks": 6, "timeout": 3, "script": "onFire" },
                { "name": "Wait", "up": 1 }
            ]
        }
    },
    "projectiles": { "thru:projectile/bullet": { "speed": 200, "lifetime_ticks": 120, "damage": 40,
                                                 "damage_type": "$DamageType::Rifle", "collide_players": true } },
    "damage_types": { "rifle": { "name": "Rifle", "suicide_message": "%1 shot themselves",
                                 "murder_message": "%2 shot %1", "vehicle_scale": 1.0, "direct": true } }
}"#
    .replace("PLAYERS", players)
}

/// Each body's damage from one shot, front to back.
fn shot(players: &str) -> Vec<(ActorId, f32)> {
    let mut w = WeaponsWorld::new(Pack::from_json(pack(players).as_bytes()).unwrap()).unwrap();
    for id in [A, B, C] {
        w.add_actor(id, 5).unwrap();
    }
    let eye = Vec3::new(0.0, 1.0, 0.0);
    w.set_frame(
        A,
        Frame {
            position: eye,
            eye,
            middle: Some(eye),
            direction: Vec3::NEG_Z,
            muzzle: [eye; 2],
            ..Frame::default()
        },
    )
    .unwrap();
    let slot = w.give(A, "thru:weapon/rifle").unwrap();
    w.equip(A, Some(slot)).unwrap();
    for _ in 0..8 {
        w.step(&mut World);
    }
    w.trigger(A, true).unwrap();
    let mut events = w.step(&mut World);
    w.trigger(A, false).unwrap();
    events.extend((0..40).flat_map(|_| w.step(&mut World)));
    events
        .into_iter()
        .filter_map(|e| match e {
            Event::Damage {
                target: TargetId::Actor(id),
                amount,
                ..
            } => Some((id, amount)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_body_stops_the_ray_unless_the_trace_lets_it_through_at_a_share() {
    let stopped = shot("{}");
    assert_eq!(stopped.len(), 1, "{stopped:?}");
    assert_eq!(stopped[0].0, B);
    assert!((stopped[0].1 - 40.0).abs() < 1e-3);
    let through = shot(r#"{ "penetration": 0.5 }"#);
    assert_eq!(through.len(), 2, "{through:?}");
    assert_eq!((through[0].0, through[1].0), (B, C));
    assert!((through[0].1 - 40.0).abs() < 1e-3, "{through:?}");
    assert!((through[1].1 - 20.0).abs() < 1e-3, "{through:?}");
}

#[test]
fn a_thick_surface_costs_more_strength_than_a_thin_one() {
    // The body is 0.6 thick: 0.5 share, and 1 - 0.6 * 0.5 / 1.2 = 0.75 more.
    let through = shot(r#"{ "penetration": 0.5, "depth": 1.2 }"#);
    assert_eq!(through.len(), 2, "{through:?}");
    assert!(
        (through[1].1 - 40.0 * 0.5 * 0.75).abs() < 1e-3,
        "{through:?}"
    );
}
