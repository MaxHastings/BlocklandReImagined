//! A projectile's lens flare whose texture is not available (a startup check
//! builds the effects without the Add-On's pictures) leaves the flare out and
//! keeps the light; with the picture it keeps the flare. The pack is written
//! here; it is our own.
use bri_client::weapon_effects::WeaponEffects;
use bri_render::scene::SceneImage;
use bri_weapons::Pack;
use std::sync::Arc;

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

fn effects(textures: bool) -> WeaponEffects {
    let image = SceneImage {
        label: "flare".into(),
        width: 4,
        height: 4,
        rgba: vec![255; 4 * 4 * 4],
        srgb: true,
    };
    let base = bri_fx_runtime::testing::pack_from(
        bri_fx_runtime::testing::library(),
        bri_fx_runtime::testing::manifest(),
    );
    let weapons = Arc::new(Pack::from_json(GUN.as_bytes()).unwrap());
    WeaponEffects::with_textures(base, weapons, Default::default(), |key| {
        (textures && key == "glow/flare.png").then_some(&image)
    })
    .expect("the effects build either way")
}

#[test]
fn a_flare_without_its_picture_is_left_out_and_with_it_kept() {
    let without = effects(false);
    assert!(
        without
            .diagnostics
            .messages
            .iter()
            .any(|m| m.contains("left its lens flare out")),
        "{:?}",
        without.diagnostics.messages
    );
    let with = effects(true);
    assert!(
        !with
            .diagnostics
            .messages
            .iter()
            .any(|m| m.contains("lens flare")),
        "{:?}",
        with.diagnostics.messages
    );
}
