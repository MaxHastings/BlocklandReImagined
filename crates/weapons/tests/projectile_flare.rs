//! A projectile may carry a lens flare on its light. The pack is written
//! here; it is our own.
use bri_weapons::*;

const GUN: &str = r#"{
    "schema_version": 5,
    "id": "glow",
    "items": { "glow:weapon/gun": { "ui_name": "Gun", "image": "glow:image/gun" } },
    "images": {
        "glow:image/gun": {
            "projectile": "glow:projectile/bolt",
            "states": [
                { "name": "Activate", "ticks": 4, "timeout": 1 },
                { "name": "Ready", "down": 2 },
                { "name": "Fire", "ticks": 6, "timeout": 3, "script": "onFire" },
                { "name": "Wait", "up": 1 }
            ]
        }
    },
    "projectiles": { "glow:projectile/bolt": {
        "speed": 100, "lifetime_ticks": 600, "gravity": 0, "collide_players": true,
        "damage": 10, "damage_type": "$DamageType::Gun",
        "light_radius": 6.0, "light_color": [0.5, 1.0, 1.0],
        "flare": { "texture": "glow/flare.png", "color": [1.0, 1.0, 1.0], "third_person": true,
                   "constant_size": null, "near_size": 0.0, "far_size": 0.5,
                   "near_distance": 12.0, "far_distance": 27.0, "fade_seconds": 0.1,
                   "blend_mode": 0, "link_color": false, "link_size": false } } },
    "damage_types": { "gun": { "name": "Gun", "suicide_message": "%1 shot themselves",
                               "murder_message": "%2 shot %1", "vehicle_scale": 1.0, "direct": true } }
}"#;

#[test]
fn a_flare_on_a_projectile_light_loads() {
    let pack = Pack::from_json(GUN.as_bytes()).unwrap();
    let flare = pack.projectiles["glow:projectile/bolt"]
        .flare
        .as_ref()
        .unwrap();
    assert_eq!((flare.near_distance, flare.far_distance), (12.0, 27.0));
}

#[test]
fn a_flare_needs_a_light_a_texture_and_ordered_distances() {
    let bad = |from: &str, to: &str| {
        let json = GUN.replace(from, to);
        assert!(
            Pack::from_json(json.as_bytes()).is_err(),
            "{to} should be refused"
        );
    };
    bad("\"light_radius\": 6.0", "\"light_radius\": 0.0");
    bad("\"far_distance\": 27.0", "\"far_distance\": 12.0");
    bad("\"blend_mode\": 0", "\"blend_mode\": 3");
    bad("\"glow/flare.png\"", "\"../flare.png\"");
    bad("\"near_size\": 0.0", "\"near_size\": -1.0");
}
