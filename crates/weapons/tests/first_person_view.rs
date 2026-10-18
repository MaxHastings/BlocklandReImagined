use bri_weapons::{FirstPersonView, Pack};
use serde_json::json;
fn policy() -> serde_json::Value {
    json!({"horizontal_fov":62.,"near":0.03,"far":400.,"scale":0.7,"eye_offset_reference_fov":100.})
}
#[test]
fn omitted_stays_legacy_and_authored_policy_roundtrips() {
    let legacy = bri_weapons::testing::pack();
    let value = serde_json::to_value(&legacy).unwrap();
    assert!(
        value["images"][bri_weapons::testing::GUN_IMAGE]
            .get("first_person_view")
            .is_none()
    );
    let mut enabled = value;
    enabled["images"][bri_weapons::testing::GUN_IMAGE]["first_person_view"] = policy();
    let pack = Pack::from_json(&serde_json::to_vec(&enabled).unwrap()).unwrap();
    let view = pack.images[bri_weapons::testing::GUN_IMAGE]
        .first_person_view
        .unwrap();
    assert_eq!(view.scale, 0.7);
    assert_eq!(
        serde_json::from_value::<FirstPersonView>(serde_json::to_value(view).unwrap()).unwrap(),
        view
    );
    let mut defaults = policy();
    defaults.as_object_mut().unwrap().remove("scale");
    let defaults: FirstPersonView = serde_json::from_value(defaults).unwrap();
    assert_eq!(defaults.scale, 1.);
}
#[test]
fn invalid_projection_scale_and_unknown_policy_are_rejected() {
    for (field, value) in [
        ("horizontal_fov", 0.),
        ("horizontal_fov", 180.),
        ("near", 0.),
        ("far", 0.02),
        ("scale", 0.),
        ("scale", -1.),
        ("eye_offset_reference_fov", 180.),
    ] {
        let mut p = policy();
        p[field] = json!(value);
        assert!(
            serde_json::from_value::<FirstPersonView>(p)
                .unwrap()
                .validate()
                .is_err(),
            "{field} {value}"
        );
    }
    let mut p: FirstPersonView = serde_json::from_value(policy()).unwrap();
    p.scale = f32::INFINITY;
    assert!(p.validate().is_err());
    p.scale = 1.;
    p.near = f32::NAN;
    assert!(p.validate().is_err());
    let mut bad = policy();
    bad["guess"] = json!(true);
    assert!(serde_json::from_value::<FirstPersonView>(bad).is_err());
}

#[test]
fn numerically_degenerate_finite_values_fail_at_admission() {
    let good: FirstPersonView = serde_json::from_value(policy()).unwrap();
    for bad in [
        FirstPersonView {
            horizontal_fov: f32::from_bits(1),
            ..good
        },
        FirstPersonView {
            scale: f32::from_bits(1),
            ..good
        },
        FirstPersonView {
            scale: f32::MAX,
            ..good
        },
        FirstPersonView {
            near: f32::from_bits(1),
            ..good
        },
        FirstPersonView {
            near: f32::MAX / 2.,
            far: f32::MAX,
            ..good
        },
    ] {
        assert!(bad.validate().is_err(), "{bad:?}");
    }
}
