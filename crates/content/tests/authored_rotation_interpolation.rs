//! Invented native controls, independent of any original game's conversion.
use bri_console::Clamp;
use bri_content::{
    animation::{Layer, sample, sample_layers, sample_layers_with_transition},
    shape::{Animation, ClipSet, Shape},
};
use glam::{Mat4, Quat, Vec3};
use serde_json::{Value, json};

fn clip(policy: Option<&str>) -> Value {
    let mut value = json!({
        "name":"invented", "frames":2, "duration":1.0,
        "looping":false, "additive":false, "priority":0,
        "nodes":[{"node":"root", "translations":[], "scales":[],
            "scale_rotations":[],
            "rotations":[[0.0,0.0,0.0,1.0],
                [0.0,0.0,0.8660254_f32,0.5]]}],
        "objects":[], "ground_translations":[], "ground_rotations":[], "triggers":[]
    });
    if let Some(policy) = policy {
        value["rotation_interpolation"] = json!(policy);
    }
    value
}

fn shape() -> Shape {
    serde_json::from_value(json!({
        "schema_version":1, "id":"test:invented-rotation",
        "nodes":[{"name":"root", "parent":null,
            "translation":[1.0,2.0,3.0], "rotation":[0.0,0.0,0.0,1.0]},
            {"name":"socket", "parent":0,
            "translation":[2.0,0.0,0.0], "rotation":[0.0,0.0,0.0,1.0]}],
        "objects":[], "details":[], "meshes":[], "materials":[], "animations":[]
    }))
    .unwrap()
}

// Component-space oracle: no native/glam interpolation helper.
fn normalized_linear(a: [f32; 4], mut b: [f32; 4], t: f32) -> Quat {
    if a.iter().zip(b).map(|(a, b)| a * b).sum::<f32>() < 0.0 {
        b = b.map(|v| -v);
    }
    let mixed: [f32; 4] = std::array::from_fn(|i| a[i] * (1.0 - t) + b[i] * t);
    let length = mixed.iter().map(|v| v * v).sum::<f32>().sqrt();
    Quat::from_array(mixed.map(|v| v / length))
}

fn rotation(animation: &Animation, t: f32) -> Mat4 {
    sample(&shape(), Some(animation), t).unwrap().nodes[0]
}

#[test]
fn authored_normalized_linear_keys_retain_policy_and_differ_from_spherical() {
    let value = clip(Some("normalized_linear"));
    let animation: Animation = serde_json::from_value(value).unwrap();
    let a = animation.nodes[0].rotations[0];
    let b = animation.nodes[0].rotations[1];
    let expected =
        Mat4::from_rotation_translation(normalized_linear(a, b, 0.25), Vec3::new(1.0, 2.0, 3.0));
    let spherical = Mat4::from_rotation_translation(
        Quat::from_rotation_z(std::f32::consts::FRAC_PI_6),
        Vec3::new(1.0, 2.0, 3.0),
    );
    assert!(!expected.abs_diff_eq(spherical, 0.01));
    assert!(rotation(&animation, 0.25).abs_diff_eq(expected, 1e-6));
    assert_eq!(
        serde_json::to_value(&animation).unwrap()["rotation_interpolation"],
        "normalized_linear"
    );
    let mut shape = shape();
    shape.animations.push(animation);
    shape.validate().unwrap();
    let roundtrip: Shape = serde_json::from_slice(&serde_json::to_vec(&shape).unwrap()).unwrap();
    assert!(rotation(&roundtrip.animations[0], 0.25).abs_diff_eq(expected, 1e-6));
    let socket = sample(&roundtrip, Some(&roundtrip.animations[0]), 0.25).unwrap();
    assert!(socket.nodes[1].abs_diff_eq(expected * Mat4::from_translation(Vec3::X * 2.0), 1e-6));
}

#[test]
fn absent_and_explicit_spherical_preserve_native_sampling() {
    let absent: Animation = serde_json::from_value(clip(None)).unwrap();
    let explicit: Animation = serde_json::from_value(clip(Some("spherical"))).unwrap();
    for time in [-1.0_f32, 0.0, 0.25, 0.5, 1.0, 2.0] {
        let t = time.clamped(0.0, 1.0);
        let key_rotation = Quat::from_array(absent.nodes[0].rotations[0])
            .slerp(Quat::from_array(absent.nodes[0].rotations[1]), t)
            .normalize();
        // The legacy sampler also composes this absolute layer over the rest
        // rotation, even at full weight. Preserve that normalization step in
        // the exact oracle rather than comparing two different calculations.
        let expected = Mat4::from_rotation_translation(
            Quat::IDENTITY.slerp(key_rotation, 1.0).normalize(),
            Vec3::new(1.0, 2.0, 3.0),
        );
        assert_eq!(rotation(&absent, time), rotation(&explicit, time));
        assert_eq!(rotation(&absent, time), expected);
    }
    assert_eq!(
        serde_json::to_vec(&absent).unwrap(),
        serde_json::to_vec(&explicit).unwrap()
    );
    assert!(
        serde_json::to_value(absent)
            .unwrap()
            .get("rotation_interpolation")
            .is_none()
    );
    assert_eq!(serde_json::to_value(explicit).unwrap(), clip(None));
}

#[test]
fn invalid_rotation_policy_is_rejected_instead_of_silently_ignored() {
    for policy in [json!("unverified"), json!(3), json!(null), json!({})] {
        let mut value = clip(None);
        value["rotation_interpolation"] = policy;
        assert!(serde_json::from_value::<Animation>(value).is_err());
    }
}

#[test]
fn layer_weights_and_frozen_transition_remain_spherical() {
    let animation: Animation = serde_json::from_value(clip(Some("normalized_linear"))).unwrap();
    let sampled = normalized_linear(
        animation.nodes[0].rotations[0],
        animation.nodes[0].rotations[1],
        0.25,
    );
    let layer = Layer {
        animation: &animation,
        time: 0.25,
        weight: 0.25,
    };
    let expected = Mat4::from_rotation_translation(
        Quat::IDENTITY.slerp(sampled, 0.25).normalize(),
        Vec3::new(1.0, 2.0, 3.0),
    );
    assert!(sample_layers(&shape(), &[layer]).unwrap().nodes[0].abs_diff_eq(expected, 1e-6));
    let (_, frozen) = sample_layers_with_transition(&shape(), &[], 0, None).unwrap();
    let full = Layer {
        weight: 1.0,
        ..layer
    };
    let (pose, _) =
        sample_layers_with_transition(&shape(), &[full], 1, Some((&frozen, 0.75))).unwrap();
    assert!(pose.nodes[0].abs_diff_eq(expected, 1e-6));

    let mut additive = animation.clone();
    additive.additive = true;
    let additive_layer = Layer {
        animation: &additive,
        ..layer
    };
    assert!(
        sample_layers(&shape(), &[additive_layer]).unwrap().nodes[0].abs_diff_eq(expected, 1e-6)
    );
}

#[test]
fn non_collinear_keys_match_independent_component_math() {
    // Invented endpoints also used by the private unchanged-SDK counterexample.
    let a = Quat::from_rotation_x(0.7).to_array();
    let b = Quat::from_rotation_y(2.0).to_array();
    let mut value = clip(Some("normalized_linear"));
    value["nodes"][0]["rotations"] = json!([a, b]);
    let animation: Animation = serde_json::from_value(value).unwrap();
    let expected = normalized_linear(a, b, 0.25);
    let sdk = Quat::from_array([0.28481275, 0.23297675, 0.0, 0.92984056]);
    assert!(expected.abs_diff_eq(sdk, 1e-7));
    assert!(!expected.abs_diff_eq(Quat::from_array(a).slerp(Quat::from_array(b), 0.25), 0.001));
    let matrix = Mat4::from_rotation_translation(expected, Vec3::new(1.0, 2.0, 3.0));
    assert!(rotation(&animation, 0.25).abs_diff_eq(matrix, 1e-6));
}

#[test]
fn shortest_arc_normalization_loop_endpoints_and_degenerate_timing_are_preserved() {
    let a = Quat::from_rotation_x(0.7).to_array();
    for b in [
        Quat::from_rotation_y(2.0).to_array(),
        Quat::from_rotation_y(2.0).to_array().map(|v| -v),
        a,
        a.map(|v| -v),
        Quat::from_rotation_x(0.70001).to_array(),
    ] {
        let mut value = clip(Some("normalized_linear"));
        value["nodes"][0]["rotations"] = json!([a, b]);
        let mut animation: Animation = serde_json::from_value(value).unwrap();
        for t in [0.0, 0.001, 0.25, 0.5, 0.9, 1.0] {
            let expected = Mat4::from_rotation_translation(
                normalized_linear(a, b, t),
                Vec3::new(1.0, 2.0, 3.0),
            );
            let actual = rotation(&animation, t);
            assert!(actual.abs_diff_eq(expected, 1e-6));
            assert!((actual.x_axis.truncate().length() - 1.0).abs() < 1e-6);
        }
        assert_eq!(rotation(&animation, -1.0), rotation(&animation, 0.0));
        assert_eq!(rotation(&animation, 2.0), rotation(&animation, 1.0));
        animation.looping = true;
        // Two keys occupy [0,.5); closing segment is b -> a over [.5,1).
        let closing =
            Mat4::from_rotation_translation(normalized_linear(b, a, 0.5), Vec3::new(1.0, 2.0, 3.0));
        assert!(rotation(&animation, 0.75).abs_diff_eq(closing, 1e-6));
        assert_eq!(rotation(&animation, 1.0), rotation(&animation, 0.0));
        assert_eq!(rotation(&animation, -0.25), rotation(&animation, 0.75));
        animation.duration = 0.0;
        assert_eq!(rotation(&animation, 0.75), rotation(&animation, 0.0));
        animation.frames = 1;
        animation.nodes[0].rotations.truncate(1);
        assert_eq!(rotation(&animation, 100.0), rotation(&animation, 0.0));
    }
}

#[test]
fn sparse_rest_scale_axes_and_clipset_roundtrip_keep_their_separate_semantics() {
    let mut value = clip(Some("normalized_linear"));
    value["nodes"][0]["rotations"] = json!([]);
    let mut animation: Animation = serde_json::from_value(value).unwrap();
    let mut rest = shape();
    rest.nodes[0].rotation = Quat::from_rotation_x(0.4).to_array();
    assert_eq!(
        sample(&rest, Some(&animation), 0.25).unwrap().nodes,
        sample(&rest, None, 0.25).unwrap().nodes
    );
    animation.nodes[0].scales = vec![[2.0, 1.0, 1.0]; 2];
    animation.nodes[0].scale_rotations = vec![
        Quat::IDENTITY.to_array(),
        Quat::from_rotation_z(2.0).to_array(),
    ];
    let mut spherical: Animation = serde_json::from_value(clip(None)).unwrap();
    spherical.nodes = animation.nodes.clone();
    assert_eq!(rotation(&animation, 0.25), rotation(&spherical, 0.25));
    let clips = ClipSet {
        schema_version: 1,
        id: "test:rotations".into(),
        animations: vec![animation],
    };
    let roundtrip: ClipSet = serde_json::from_slice(&serde_json::to_vec(&clips).unwrap()).unwrap();
    assert_eq!(
        serde_json::to_value(&clips).unwrap(),
        serde_json::to_value(&roundtrip).unwrap()
    );
    assert_eq!(
        rotation(&clips.animations[0], 0.25),
        rotation(&roundtrip.animations[0], 0.25)
    );
}

#[test]
fn invalid_keys_times_and_weights_remain_rejected() {
    let valid: Animation = serde_json::from_value(clip(Some("normalized_linear"))).unwrap();
    for key in [
        [0.0; 4],
        [0.0, 0.0, 0.0, 2.0],
        [f32::NAN, 0.0, 0.0, 1.0],
        [f32::INFINITY, 0.0, 0.0, 1.0],
    ] {
        let mut invalid = valid.clone();
        invalid.nodes[0].rotations[1] = key;
        assert!(sample(&shape(), Some(&invalid), 0.25).is_err());
    }
    let mut invalid = valid.clone();
    invalid.nodes[0].rotations.pop();
    assert!(sample(&shape(), Some(&invalid), 0.25).is_err());
    for time in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(sample(&shape(), Some(&valid), time).is_err());
    }
    for weight in [-0.1, 1.1, f32::NAN] {
        assert!(
            sample_layers(
                &shape(),
                &[Layer {
                    animation: &valid,
                    time: 0.25,
                    weight
                }]
            )
            .is_err()
        );
    }
}
