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
    surface: Option<&'static str>,
}
impl Query for World {
    fn surface_kind(&mut self, _: TargetId, _: Vec3, _: Vec3) -> Option<&str> {
        self.surface
    }
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
    surface_effects(item, target, None)
}
fn surface_effects(item: &str, target: TargetId, surface: Option<&'static str>) -> Vec<String> {
    let guns = GUNS
        .replace(
            r#""map": "dust""#,
            r#""map": "dust", "surfaces": {"stone":"stone-chips"}"#,
        )
        .replace(
            r#""brick": "chips""#,
            r#""brick": "chips", "surfaces": {"stone":"stone-chips"}"#,
        );
    let mut w = WeaponsWorld::new(Pack::from_json(guns.as_bytes()).unwrap()).unwrap();
    w.add_actor(A, 5).unwrap();
    w.add_actor(B, 5).unwrap();
    let slot = w.give(A, item).unwrap();
    w.equip(A, Some(slot)).unwrap();
    let mut world = World { target, surface };
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

#[test]
fn authored_material_overrides_target_category_and_unknown_material_falls_back() {
    for gun in ["imp:weapon/rifle", "imp:weapon/cannon"] {
        assert_eq!(
            surface_effects(gun, TargetId::Map(0), Some("stone")),
            ["stone-chips"]
        );
        assert_eq!(
            surface_effects(gun, TargetId::Vehicle(7), Some("unmapped")),
            ["sparks"]
        );
    }
}

#[test]
fn surface_marks_select_the_complete_style_and_keep_seeded_variation() {
    let decal: Decal = serde_json::from_value(serde_json::json!({
        "texture":"default.png", "size":0.2, "seconds":30.,
        "surfaces":{"legacy":"legacy.png"}, "profiles":{
            "wood":null,
            "stone":{"texture":"stone.png","blend":"double_multiply",
                "size":[0.1,0.3],"color":[1.,0.8,0.6,1.],"seconds":[80.,100.],
                "fade_seconds":20.,"rotation_degrees":[0.,360.]}
        }
    }))
    .unwrap();
    assert!(decal.valid());
    assert!(decal.for_surface(Some("wood"), 10).is_none());
    let a = decal.for_surface(Some("stone"), 10).unwrap();
    assert_eq!(a, decal.for_surface(Some("stone"), 10).unwrap());
    assert_ne!(a, decal.for_surface(Some("stone"), 11).unwrap());
    assert_eq!(a.texture.as_deref(), Some("stone.png"));
    assert_eq!(a.blend, DecalBlend::DoubleMultiply);
    assert_eq!(a.fade_seconds, Some(20.));
    assert!((0.1..=0.3).contains(&a.size) && (80. ..=100.).contains(&a.seconds));
    assert!(a.valid());
    assert_eq!(
        decal
            .for_surface(Some("legacy"), 10)
            .unwrap()
            .texture
            .as_deref(),
        Some("legacy.png")
    );
    let mut required = decal;
    required.require_profile = true;
    assert!(required.for_surface(Some("sand"), 10).is_none());
    assert!(required.for_surface(None, 10).is_none());
    required
        .profiles
        .get_mut("stone")
        .unwrap()
        .as_mut()
        .unwrap()
        .size = [0.5, 0.1];
    assert!(!required.valid());
}
