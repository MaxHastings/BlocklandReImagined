//! Image fields Add-Ons and ports set (`Image`, `Zoom`): the aiming
//! fields' limits and the scope's sway, and the body nodes a held image
//! hides. Content-free: the base is the sample Bubble Blaster.
use bri_weapons::*;
use std::path::Path;

const IMAGE: &str = "sample-bubble-blaster:image/bubble_blaster";

fn pack() -> Pack {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/samples/sample-bubble-blaster/assets/weapons.json");
    Pack::from_json(&std::fs::read(path).unwrap()).unwrap()
}

fn zoom(json: &str) -> Result<(), String> {
    serde_json::from_str::<Zoom>(json)
        .map_err(|e| e.to_string())?
        .validate()
}

#[test]
fn aiming_fields_have_limits() {
    assert!(zoom(r#"{"fov": 22}"#).is_ok());
    assert!(zoom(r#"{"fov": 90}"#).is_err());
    // Each step narrower than the last, within 5 to 85, at most eight.
    assert!(zoom(r#"{"fov": 22, "levels": [10, 5]}"#).is_ok());
    assert!(zoom(r#"{"fov": 22, "levels": [30]}"#).is_err());
    assert!(zoom(r#"{"fov": 22, "levels": [10, 10]}"#).is_err());
    assert!(zoom(r#"{"fov": 22, "levels": [4]}"#).is_err());
    assert!(zoom(r#"{"fov": 85, "levels": [80, 70, 60, 50, 40, 30, 20, 10, 5]}"#).is_err());
    assert!(zoom(r#"{"fov": 22, "sensitivity": 0.5}"#).is_ok());
    assert!(zoom(r#"{"fov": 22, "sensitivity": 0}"#).is_err());
    assert!(zoom(r#"{"fov": 22, "sensitivity": 5}"#).is_err());
    // The picture stays inside the Add-On.
    for bad in ["../scope", "/scope", "c:scope", "a//b", "a\\\\b", ""] {
        assert!(
            zoom(&format!(r#"{{"fov": 22, "overlay": "{bad}"}}"#)).is_err(),
            "{bad}"
        );
    }
    assert!(zoom(r#"{"fov": 22, "overlay": "scope/scope"}"#).is_ok());
    assert!(zoom(r#"{"fov": 22, "sway": {"degrees": 0.5, "seconds": 4}}"#).is_ok());
    assert!(zoom(r#"{"fov": 22, "sway": {"degrees": 6, "seconds": 4}}"#).is_err());
    assert!(zoom(r#"{"fov": 22, "sway": {"degrees": 1, "seconds": 0.1}}"#).is_err());
    assert!(zoom(r#"{"fov": 22, "sway": {"degrees": 1, "seconds": 4, "crouched": 2}}"#).is_err());
    assert!(zoom(r#"{"fov": 22, "sway": {"degrees": 1, "seconds": 4, "moving": 0.5}}"#).is_err());
    // A pack whose image breaks them is refused, naming the image.
    let mut bad = pack();
    bad.images.get_mut(IMAGE).unwrap().zoom =
        Some(serde_json::from_str(r#"{"fov": 22, "levels": [40]}"#).unwrap());
    let error = bad.validate().unwrap_err().to_string();
    assert!(error.contains(IMAGE) && error.contains("levels"), "{error}");
}

/// A held image hides up to 16 body nodes by name (`hide_nodes`).
#[test]
fn hidden_nodes_have_limits() {
    let with = |nodes: Vec<String>| {
        let mut pack = pack();
        let image = pack.images.get_mut(IMAGE).unwrap();
        image.hide_nodes = nodes;
        image.both_arms = true;
        pack.validate()
    };
    assert!(with(vec!["lhand".into(), "rhook".into()]).is_ok());
    assert!(with(vec!["l hand".into()]).is_err());
    assert!(with(vec![String::new()]).is_err());
    assert!(with(vec!["n".repeat(33)]).is_err());
    assert!(with((0..17).map(|i| format!("node{i}")).collect()).is_err());
    // Both read back from a pack's JSON, and are left out when unset.
    let image: Image =
        serde_json::from_str(r#"{"hide_nodes": ["lhand"], "both_arms": true}"#).unwrap();
    assert_eq!(
        (image.hide_nodes.as_slice(), image.both_arms),
        (&["lhand".to_string()][..], true)
    );
    let plain = serde_json::to_value(Image::default()).unwrap();
    assert!(plain.get("hide_nodes").is_none() && plain.get("both_arms").is_none());
}

#[test]
fn sway_is_a_figure_of_eight() {
    let sway = Sway {
        degrees: 1.0,
        seconds: 4.0,
        crouched: 0.3,
        moving: 2.0,
    };
    let a = 1f32.to_radians();
    let (yaw, pitch) = sway.offset(0.0);
    assert!(yaw.abs() < 1e-6 && pitch.abs() < 1e-6);
    // A quarter round: fully to one side, back level.
    let (yaw, pitch) = sway.offset(0.25);
    assert!(
        (yaw - a).abs() < 1e-6 && pitch.abs() < 1e-6,
        "{yaw} {pitch}"
    );
    // An eighth: half as far up as it goes aside, at its highest.
    let (_, pitch) = sway.offset(0.125);
    assert!((pitch - a / 2.0).abs() < 1e-6, "{pitch}");
    assert_eq!(sway.offset(0.3), sway.offset(1.3));
}

/// How bots use an image (`bot`): its fire style and reach, from data.
#[test]
fn bot_use_is_read_and_limited() {
    let with = |bot: serde_json::Value| {
        let mut json: serde_json::Value = serde_json::to_value(pack()).unwrap();
        json["images"][IMAGE]["bot"] = bot;
        serde_json::from_value::<Pack>(json)
            .map_err(|e| e.to_string())
            .and_then(|p| p.validate().map(|_| p).map_err(|e| e.to_string()))
    };
    let pack = with(serde_json::json!({"fire": "hold", "reach": 8.0})).unwrap();
    let bot = pack.images[IMAGE].bot.unwrap();
    assert_eq!(bot.fire, BotFire::Hold);
    assert_eq!(bot.reach, Some(8.0));
    assert_eq!(
        with(serde_json::json!({})).unwrap().images[IMAGE]
            .bot
            .unwrap()
            .fire,
        BotFire::Tap
    );
    assert!(with(serde_json::json!({"fire": "spray"})).is_err());
    assert!(with(serde_json::json!({"reach": 0.0})).is_err());
    assert!(with(serde_json::json!({"reach": 5000.0})).is_err());
    assert!(with(serde_json::json!({"aim": 1})).is_err());
    let near = with(serde_json::json!({"reach": 30.0, "near": 2.5})).unwrap();
    assert_eq!(near.images[IMAGE].bot.unwrap().near, Some(2.5));
    assert!(with(serde_json::json!({"reach": 3.0, "near": 3.0})).is_err());
    assert!(with(serde_json::json!({"near": -1.0})).is_err());
}

#[test]
fn manipulation_requires_explicit_grounded_hold_semantics() {
    let mut p = pack();
    let image = p.images.get_mut(IMAGE).unwrap();
    image.projectile = None;
    image.melee = false;
    image.command = Some("unfamiliar:acquire".into());
    image.bot = Some(BotUse {
        fire: BotFire::Hold,
        manipulation: Some(BotManipulation::Hold {
            near: 2.5,
            reach: 60.0,
            force: 90000.0,
            turn: true,
        }),
        ..Default::default()
    });
    p.validate().unwrap();
    let encoded = serde_json::to_value(&p).unwrap();
    let restored: Pack = serde_json::from_value(encoded.clone()).unwrap();
    assert_eq!(restored.images[IMAGE].bot, p.images[IMAGE].bot);
    for (key, value) in [
        ("near", serde_json::json!(0)),
        ("reach", serde_json::json!(2.0)),
        ("force", serde_json::json!(-1)),
        ("reach", serde_json::json!(2001)),
        ("unknown", serde_json::json!(true)),
    ] {
        let mut bad = encoded.clone();
        bad["images"][IMAGE]["bot"]["manipulation"][key] = value;
        assert!(
            serde_json::from_value::<Pack>(bad)
                .map(|v| v.validate().is_err())
                .unwrap_or(true),
            "{key}"
        );
    }
    p.images.get_mut(IMAGE).unwrap().bot.as_mut().unwrap().fire = BotFire::Tap;
    assert!(p.validate().is_err());
    p.images.get_mut(IMAGE).unwrap().bot.as_mut().unwrap().fire = BotFire::Hold;
    p.images.get_mut(IMAGE).unwrap().command = None;
    p.images.get_mut(IMAGE).unwrap().commands = Default::default();
    assert!(
        p.validate().is_err(),
        "an opaque image is not a native hold executor"
    );
}

/// A charged image (the Spear: held back, thrown on letting go) says so
/// from its states, so a bot holds it and lets go once letting go fires.
#[test]
fn a_charged_image_fires_on_release_from_its_armed_state() {
    let pack = bri_weapons::testing::pack();
    let spear = &pack.images[bri_weapons::testing::SPEAR_IMAGE];
    assert!(spear.charges());
    let armed: Vec<&str> = spear
        .states
        .iter()
        .filter(|s| spear.fires_on_release(s))
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(armed.len(), 1, "{armed:?}");
    for tapped in [
        bri_weapons::testing::GUN_IMAGE,
        bri_weapons::testing::ROCKET_IMAGE,
    ] {
        assert!(!pack.images[tapped].charges(), "{tapped}");
    }
}
