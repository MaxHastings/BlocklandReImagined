//! The fire button is the player's, as v20's move trigger is: it holds
//! across image, tool and colour changes, and image changes follow
//! Torque's `setImage` (a mount waits for a state that allows image
//! changes; putting tools away does not wait). A small authored pack with
//! the spray can's and the gun's state shapes keeps these content-free.
use bri_weapons::*;
use glam::Vec3;

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

const A: ActorId = ActorId(1);
const CAN: &str = "t:image/can";
const GUN: &str = "t:image/gun";

/// `blueSprayCanImage` and `gunImage` at 120 ticks a second.
fn pack() -> Pack {
    Pack::from_json(
        br#"{
  "schema_version": 3,
  "id": "t",
  "items": {
    "t:weapon/gun": { "image": "t:image/gun" },
    "t:weapon/sword": { "image": "t:image/sword" }
  },
  "images": {
    "t:image/can": { "states": [
      { "name": "Activate", "ticks": 60, "wait": false, "timeout": 4, "down": 2 },
      { "name": "Ready", "down": 2 },
      { "name": "Fire", "ticks": 5, "script": "onFire", "timeout": 2, "up": 3 },
      { "name": "StopFire", "timeout": 1 },
      { "name": "CapOff", "ticks": 24, "wait": false, "down": 2, "timeout": 1 }
    ] },
    "t:image/gun": { "states": [
      { "name": "Activate", "ticks": 18, "timeout": 1 },
      { "name": "Ready", "down": 2 },
      { "name": "Fire", "ticks": 17, "allow_change": false, "script": "onFire", "timeout": 3 },
      { "name": "Smoke", "ticks": 1, "timeout": 4 },
      { "name": "Reload", "up": 1 }
    ] },
    "t:image/sword": { "states": [
      { "name": "Activate", "ticks": 60, "timeout": 1 },
      { "name": "Ready", "down": 2 },
      { "name": "Fire", "ticks": 24, "script": "onFire", "timeout": 3 },
      { "name": "CheckFire", "up": 1, "down": 2 }
    ] }
  }
}"#,
    )
    .unwrap()
}

fn world() -> WeaponsWorld {
    let mut w = WeaponsWorld::new(pack()).unwrap();
    w.add_actor(A, 5).unwrap();
    w.give(A, "t:weapon/gun").unwrap();
    w.give(A, "t:weapon/sword").unwrap();
    w
}

fn step(w: &mut WeaponsWorld, ticks: usize) -> Vec<Event> {
    (0..ticks).flat_map(|_| w.step(&mut Open)).collect()
}

fn state(w: &WeaponsWorld) -> Option<(String, String)> {
    w.image_state(A, 0)
        .map(|(image, state)| (image.id.clone(), state.name.clone()))
}

fn entered<'a>(events: &'a [Event], image: &str) -> Vec<&'a str> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::ImageState {
                image: i, state, ..
            } if i == image => Some(state.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_held_trigger_keeps_spraying_through_colour_changes() {
    let mut w = world();
    w.mount_image(A, CAN, Some(3)).unwrap();
    w.trigger(A, true).unwrap();
    let first = step(&mut w, 30);
    assert!(entered(&first, CAN).contains(&"Fire"));
    // Scrolling to the next colour mounts `color4SprayCanImage`; its
    // Activate goes straight to Fire because the trigger is still down.
    w.mount_image(A, CAN, Some(4)).unwrap();
    let next = step(&mut w, 1);
    assert_eq!(entered(&next, CAN), ["Activate", "Fire"]);
    assert_eq!(w.image_paint(A, 0), Some(4));
    let held = step(&mut w, 60);
    assert!(!entered(&held, CAN).contains(&"StopFire"));
    assert!(entered(&held, CAN).iter().filter(|s| **s == "Fire").count() >= 10);
    w.trigger(A, false).unwrap();
    assert!(entered(&step(&mut w, 6), CAN).contains(&"StopFire"));
}

#[test]
fn the_same_can_again_is_not_remounted() {
    let mut w = world();
    w.mount_image(A, CAN, Some(3)).unwrap();
    w.trigger(A, true).unwrap();
    step(&mut w, 10);
    w.mount_image(A, CAN, Some(3)).unwrap();
    let events = step(&mut w, 1);
    assert!(!events.iter().any(|e| matches!(e, Event::Mounted { .. })));
    assert_eq!(state(&w).unwrap().1, "Fire");
}

#[test]
fn switching_mid_shot_waits_for_a_state_that_allows_it() {
    let mut w = world();
    w.equip(A, Some(0)).unwrap();
    step(&mut w, 20);
    w.trigger(A, true).unwrap();
    step(&mut w, 1);
    assert_eq!(state(&w).unwrap(), (GUN.into(), "Fire".into()));
    // The gun's Fire forbids image changes: the choice is taken now and
    // the sword mounts when the gun leaves Fire, not refused.
    w.equip(A, Some(1)).unwrap();
    assert_eq!(w.actor(A).unwrap().selected, Some(1));
    assert_eq!(state(&w).unwrap().0, GUN);
    let events = step(&mut w, 16);
    assert!(entered(&events, GUN).is_empty());
    assert_eq!(state(&w).unwrap(), (GUN.into(), "Fire".into()));
    let events = step(&mut w, 1);
    assert!(!entered(&events, GUN).contains(&"Smoke"));
    assert_eq!(
        state(&w).unwrap().0,
        "t:image/sword",
        "Smoke allows the change, so it is never entered"
    );
    assert_eq!(entered(&events, "t:image/sword"), ["Activate"]);
    assert_eq!(w.actor(A).unwrap().selected, Some(1));
}

#[test]
fn a_new_image_fires_at_once_if_the_trigger_is_still_held() {
    let mut w = world();
    w.equip(A, Some(0)).unwrap();
    step(&mut w, 20);
    w.trigger(A, true).unwrap();
    step(&mut w, 1);
    w.mount_image(A, CAN, Some(0)).unwrap();
    let events = step(&mut w, 17);
    assert_eq!(entered(&events, CAN), ["Activate", "Fire"]);
}

#[test]
fn a_release_before_the_switch_is_kept() {
    let mut w = world();
    w.equip(A, Some(0)).unwrap();
    step(&mut w, 20);
    w.trigger(A, true).unwrap();
    step(&mut w, 1);
    w.mount_image(A, CAN, Some(0)).unwrap();
    w.trigger(A, false).unwrap();
    let events = step(&mut w, 120);
    assert_eq!(entered(&events, CAN), ["Activate", "CapOff", "Ready"]);
}

#[test]
fn putting_tools_away_mid_shot_is_immediate() {
    let mut w = world();
    w.equip(A, Some(0)).unwrap();
    step(&mut w, 20);
    w.trigger(A, true).unwrap();
    step(&mut w, 1);
    w.equip(A, None).unwrap();
    assert!(state(&w).is_none());
    assert_eq!(w.actor(A).unwrap().selected, None);
    assert!(w.actor(A).unwrap().trigger_held());
}

#[test]
fn a_trigger_held_with_empty_hands_fires_the_tool_taken_out() {
    let mut w = world();
    w.trigger(A, true).unwrap();
    step(&mut w, 5);
    w.equip(A, Some(1)).unwrap();
    let events = step(&mut w, 61);
    assert_eq!(entered(&events, "t:image/sword"), ["Activate", "Ready", "Fire"]);
}

#[test]
fn putting_away_forgets_a_waiting_image() {
    let mut w = world();
    w.equip(A, Some(0)).unwrap();
    step(&mut w, 20);
    w.trigger(A, true).unwrap();
    step(&mut w, 1);
    w.equip(A, Some(1)).unwrap();
    w.equip(A, Some(0)).unwrap();
    // Choosing the held gun again cancels the waiting sword.
    let events = step(&mut w, 40);
    assert!(entered(&events, "t:image/sword").is_empty());
    assert_eq!(state(&w).unwrap().0, GUN);
}
