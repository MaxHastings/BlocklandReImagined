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

/// The held image frames the same at every screen aspect once it has a reference aspect: the vertical field is
/// the one its horizontal field has there, so a point in front of the eye lands at the same height on 4:3, 16:9 and 21:9.
#[test]
fn a_reference_aspect_keeps_the_guns_framing_at_every_screen_shape() {
    let view = |reference_aspect: Option<f32>| FirstPersonView {
        horizontal_fov: 85.,
        near: 0.1,
        far: 100.,
        scale: 1.,
        eye_offset_reference_fov: None,
        reference_aspect,
    };
    assert!(view(Some(4. / 3.)).validate().is_ok() && view(Some(9.)).validate().is_err());
    // Height on screen (tan of the angle over the half-height tangent) of a point 0.3 below and 0.6 ahead.
    let height = |view: FirstPersonView, aspect: f32| {
        (0.3_f32 / 0.6) / (view.vertical_fov(aspect) * 0.5).tan()
    };
    let fixed = view(Some(4. / 3.));
    let at_native = height(fixed, 4. / 3.);
    for aspect in [4. / 3., 16. / 9., 21. / 9., 32. / 9.] {
        assert!(
            (height(fixed, aspect) - at_native).abs() < 1e-5,
            "{aspect}: the gun keeps its height"
        );
    }
    // Without it (Torque's rule) a wider screen crops: the same point sits lower, and past the screen's edge at 21:9.
    let torque = view(None);
    assert!(
        height(torque, 21. / 9.) > height(torque, 16. / 9.)
            && height(torque, 16. / 9.) > height(torque, 4. / 3.)
    );
    assert!(
        height(torque, 21. / 9.) > 1.0,
        "the point leaves the screen: {}",
        height(torque, 21. / 9.)
    );
}
