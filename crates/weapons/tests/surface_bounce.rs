//! A projectile's bounce by surface kind: what it keeps and the effect it makes.
use bri_weapons::Pack;
use serde_json::json;

fn pack(surfaces: serde_json::Value, effects: serde_json::Value) -> anyhow::Result<Pack> {
    let pack = json!({
        "schema_version": 5,
        "id": "kit",
        "items": {},
        "images": {},
        "projectiles": {
            "kit:projectile/ball": {
                "speed": 10.0, "lifetime_ticks": 240, "ballistic": true, "elasticity": 0.4,
                "elasticity_surfaces": surfaces, "bounce_surfaces": effects
            }
        }
    });
    Pack::from_json(&serde_json::to_vec(&pack)?)
}

#[test]
fn a_surface_factor_and_a_bounce_effect_by_kind_are_read() {
    let pack = pack(
        json!({ "metal": 1.25, "sand": 0.375 }),
        json!({ "wood": "knock" }),
    )
    .unwrap();
    let ball = &pack.projectiles["kit:projectile/ball"];
    assert_eq!(ball.elasticity_surfaces["metal"], 1.25);
    assert_eq!(ball.elasticity_surfaces["sand"], 0.375);
    assert_eq!(ball.bounce_surfaces["wood"], "knock");
}

#[test]
fn a_factor_outside_zero_to_two_is_refused() {
    assert!(pack(json!({ "metal": 3.0 }), json!({})).is_err());
    assert!(pack(json!({ "metal": -0.1 }), json!({})).is_err());
}
