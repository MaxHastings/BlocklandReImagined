use bri_weapons::{Pack, Zoom};
use serde_json::{Value, json};

fn zoom(allowed: Option<Value>) -> Zoom {
    let mut value = json!({"fov":40});
    if let Some(allowed) = allowed {
        value["allowed_states"] = allowed;
    }
    serde_json::from_value(value).unwrap()
}
fn pack(allowed: Value) -> Value {
    let mut value = serde_json::to_value(bri_weapons::testing::pack()).unwrap();
    value["images"][bri_weapons::testing::GUN_IMAGE]["zoom"] =
        json!({"fov":40,"allowed_states":allowed});
    value
}
fn load(value: &Value) -> anyhow::Result<Pack> {
    Pack::from_json(&serde_json::to_vec(value)?)
}

#[test]
fn omitted_is_legacy_and_empty_is_barred_even_without_state() {
    let legacy = zoom(None);
    assert!(legacy.allows_state(None) && legacy.allows_state(Some("anything")));
    assert!(
        serde_json::to_value(&legacy)
            .unwrap()
            .get("allowed_states")
            .is_none()
    );
    let barred = zoom(Some(json!([])));
    assert!(barred.validate().is_ok());
    assert!(!barred.allows_state(None) && !barred.allows_state(Some("Ready")));
    assert_eq!(
        serde_json::to_value(&barred).unwrap()["allowed_states"],
        json!([])
    );
    assert!(load(&pack(json!([]))).is_ok());
}

#[test]
fn known_state_names_match_case_insensitively_and_roundtrip() {
    let original = pack(json!(["rEaDy", "FIRE", "waitForRelease"]));
    let loaded = load(&original).unwrap();
    let scope = loaded.images[bri_weapons::testing::GUN_IMAGE]
        .zoom
        .as_ref()
        .unwrap();
    assert!(scope.allows_state(Some("ready")) && scope.allows_state(Some("Fire")));
    assert!(
        !scope.allows_state(None)
            && !scope.allows_state(Some("Reload"))
            && !scope.allows_state(Some("unknown"))
    );
    let again = load(&serde_json::to_value(&loaded).unwrap()).unwrap();
    assert_eq!(
        again.images[bri_weapons::testing::GUN_IMAGE].zoom.as_ref(),
        Some(scope)
    );
}

#[test]
fn unknown_duplicate_or_ambiguous_image_state_references_are_rejected() {
    for names in [json!(["absent"]), json!(["Ready", "ready"])] {
        assert!(load(&pack(names)).is_err());
    }
    let mut ambiguous = pack(json!(["Ready"]));
    let states = ambiguous["images"][bri_weapons::testing::GUN_IMAGE]["states"]
        .as_array_mut()
        .unwrap();
    let ready = states
        .iter()
        .find(|s| s["name"] == "Ready")
        .unwrap()
        .clone();
    states.push(ready);
    assert!(load(&ambiguous).is_err());
}

#[test]
fn state_list_and_name_bounds_are_enforced() {
    for names in [
        json!([""]),
        json!(["   "]),
        json!(["Ready\n"]),
        json!(["r\u{e9}ady"]),
        json!(["x".repeat(129)]),
        json!((0..65).map(|n| format!("state{n}")).collect::<Vec<_>>()),
    ] {
        assert!(zoom(Some(names)).validate().is_err());
    }
    let mut boundary = pack(json!(
        (0..64).map(|n| format!("state{n}")).collect::<Vec<_>>()
    ));
    boundary["images"][bri_weapons::testing::GUN_IMAGE]["states"] = json!(
        (0..64)
            .map(|n| json!({"name":format!("state{n}")}))
            .collect::<Vec<_>>()
    );
    assert!(load(&boundary).is_ok());
}

#[test]
fn overlay_fit_defaults_to_the_legacy_black_fit_and_rejects_unknown_words() {
    let legacy: Zoom = serde_json::from_value(json!({"fov":40})).unwrap();
    assert_eq!(legacy.overlay_fit, bri_weapons::OverlayFit::HeightBlack);
    assert!(
        serde_json::to_value(&legacy)
            .unwrap()
            .get("overlay_fit")
            .is_none()
    );
    for (word, fit) in [
        ("height", bri_weapons::OverlayFit::Height),
        ("stretch", bri_weapons::OverlayFit::Stretch),
    ] {
        let z: Zoom = serde_json::from_value(json!({"fov":40,"overlay_fit":word})).unwrap();
        assert_eq!(z.overlay_fit, fit);
        assert_eq!(serde_json::to_value(&z).unwrap()["overlay_fit"], word);
    }
    assert!(serde_json::from_value::<Zoom>(json!({"fov":40,"overlay_fit":"cover"})).is_err());
}
