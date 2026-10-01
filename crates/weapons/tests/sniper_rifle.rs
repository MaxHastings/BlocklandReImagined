//! The Sniper Rifle Add-On (`packages/showcase/sniper-rifle`), our take on
//! Kaje's: one heavy straight round a pull, an arm kick, muzzle smoke, then
//! the bolt worked (throwing the case) before the trigger is let go for the
//! next. Also the limits of the aiming fields it uses (`Zoom`).
use bri_weapons::*;
use glam::Vec3;
use std::path::Path;

struct Open;
impl Query for Open {
    fn sweep(&mut self, _: Vec3, _: Vec3, _: Filter) -> Option<Hit> {
        None
    }
    fn radius(&mut self, _: Vec3, _: f32, _: usize) -> Vec<Nearby> {
        Vec::new()
    }
    fn can_affect(&self, _: ActorId, _: TargetId) -> bool {
        true
    }
    fn can_catch(&self, _: ActorId, _: ActorId) -> bool {
        true
    }
}

const RIFLE: &str = "sniper-rifle:weapon/sniperrifle";
const IMAGE: &str = "sniper-rifle:image/sniperrifle";
const ROUND: &str = "sniper-rifle:projectile/round";
const A: ActorId = ActorId(1);

fn pack() -> Pack {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/showcase/sniper-rifle/assets/weapons.json");
    Pack::from_json(&std::fs::read(path).unwrap()).unwrap()
}

fn armed() -> WeaponsWorld {
    let mut w = WeaponsWorld::new(pack()).unwrap();
    w.add_actor(A, 5).unwrap();
    let slot = w.give(A, RIFLE).unwrap();
    w.equip(A, Some(slot)).unwrap();
    step(&mut w, 40);
    w
}

fn step(w: &mut WeaponsWorld, ticks: usize) -> Vec<Event> {
    (0..ticks).flat_map(|_| w.step(&mut Open)).collect()
}

fn state(w: &WeaponsWorld) -> String {
    w.image_state(A, 0).unwrap().1.name.clone()
}

#[test]
fn it_is_kajes_rifle_with_a_scope() {
    let pack = pack();
    let round = &pack.projectiles[ROUND];
    // Kaje's round: 2000 units a second, dead straight, 150 damage (a
    // Blockhead has 100) and a hard knock.
    assert_eq!(
        (round.speed, round.gravity, round.damage),
        (2000.0, 0.0, 150.0)
    );
    assert_eq!((round.impulse, round.vertical), (1200.0, 1400.0));
    assert_eq!(round.trail, "sniper-rifle:emitter/trail");
    let image = &pack.images[IMAGE];
    assert_eq!(image.fire_animation.as_deref(), Some("shiftAway"));
    let zoom = image.zoom.as_ref().unwrap();
    assert!(zoom.on_jet && !zoom.jets && !zoom.crosshair && zoom.first_person);
    assert_eq!((zoom.fov, zoom.levels.as_slice()), (22.0, &[10.0][..]));
    assert_eq!(zoom.overlay.as_deref(), Some("scope/scope"));
    assert!(zoom.sway.is_some());
    assert_eq!(
        (zoom.level_fov(0), zoom.level_fov(1), zoom.level_fov(7)),
        (22.0, 10.0, 10.0)
    );
}

#[test]
fn one_round_a_pull_then_the_bolt() {
    let mut w = armed();
    assert_eq!(state(&w), "Ready");
    w.trigger(A, true).unwrap();
    let fired = step(&mut w, 2);
    let rounds: Vec<_> = fired
        .iter()
        .filter_map(|e| match e {
            Event::Spawned {
                definition,
                velocity,
                ..
            } => Some((definition.clone(), velocity.length())),
            _ => None,
        })
        .collect();
    assert_eq!(rounds.len(), 1, "{fired:?}");
    assert_eq!(rounds[0].0, ROUND);
    assert!((rounds[0].1 - 2000.0).abs() < 1.0, "{}", rounds[0].1);
    // The shot kicks the arm, as Kaje's onFire played shiftAway.
    assert!(fired.iter().any(|e| matches!(e,
        Event::Animation { thread: 2, sequence, .. } if sequence == "shiftAway")));
    // Holding the trigger: smoke, the bolt (its animation and the case
    // thrown out), then nothing more until it is let go.
    let held = step(&mut w, 240);
    assert!(!held.iter().any(|e| matches!(e, Event::Spawned { .. })));
    let states: Vec<_> = held
        .iter()
        .filter_map(|e| match e {
            Event::ImageState { state, .. } => Some(state.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(states, ["Smoke", "Bolt", "Reload"]);
    assert!(
        held.iter().any(|e| matches!(e, Event::Shell { .. })),
        "the bolt throws the case"
    );
    assert_eq!(state(&w), "Reload");
    w.trigger(A, false).unwrap();
    step(&mut w, 2);
    assert_eq!(state(&w), "Ready");
    w.trigger(A, true).unwrap();
    assert!(
        step(&mut w, 2)
            .iter()
            .any(|e| matches!(e, Event::Spawned { .. }))
    );
}

#[test]
fn a_shot_takes_about_a_second_and_a_half() {
    // Conan's update fired "slightly faster" than Kaje's 2.14 s: fire,
    // smoke and bolt take 1.5 s before the rifle is ready again.
    let mut w = armed();
    w.trigger(A, true).unwrap();
    step(&mut w, 1);
    w.trigger(A, false).unwrap();
    let mut ticks = 0;
    while state(&w) != "Ready" {
        step(&mut w, 1);
        ticks += 1;
        assert!(ticks < 1000);
    }
    assert!((170..=190).contains(&ticks), "{ticks} ticks");
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
    bad.images
        .get_mut(IMAGE)
        .unwrap()
        .zoom
        .as_mut()
        .unwrap()
        .levels = vec![40.0];
    let error = bad.validate().unwrap_err().to_string();
    assert!(error.contains(IMAGE) && error.contains("levels"), "{error}");
    let mut bad = pack();
    bad.images.get_mut(IMAGE).unwrap().fire_animation = Some("shift away".into());
    assert!(bad.validate().is_err());
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

/// A shot's arm animation is the image's to choose; left out, the engine
/// keeps v20's (a gun kicks), and `""` plays none.
#[test]
fn fire_animation_overrides_the_engines_pick() {
    let fired = |animation: Option<&str>| {
        let mut pack = pack();
        pack.images.get_mut(IMAGE).unwrap().fire_animation = animation.map(str::to_string);
        let mut w = WeaponsWorld::new(pack).unwrap();
        w.add_actor(A, 5).unwrap();
        let slot = w.give(A, RIFLE).unwrap();
        w.equip(A, Some(slot)).unwrap();
        step(&mut w, 40);
        w.trigger(A, true).unwrap();
        step(&mut w, 2)
            .into_iter()
            .filter_map(|e| match e {
                Event::Animation {
                    thread: 2,
                    sequence,
                    ..
                } => Some(sequence),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(fired(Some("spearThrow")), ["spearThrow"]);
    assert!(fired(Some("")).is_empty());
    // "SniperRifleImage" has no "gun" in its name, so v20's pick is none.
    assert!(fired(None).is_empty());
}
