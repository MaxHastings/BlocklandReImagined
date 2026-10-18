//! An image that rests in a state for a random while fidgets: it enters
//! another state, which times out back into the rest, and any trigger
//! transition comes first. Content-free authored pack.
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

fn world(skip: f32) -> WeaponsWorld {
    let pack = Pack::from_json(
        format!(
            r#"{{
  "schema_version": 5,
  "id": "t",
  "items": {{ "t:weapon/gun": {{ "ui_name": "Gun", "image": "t:image/gun" }} }},
  "images": {{
    "t:image/gun": {{ "states": [
      {{ "name": "Activate", "ticks": 18, "timeout": 1 }},
      {{ "name": "Ready", "down": 3, "fidget": {{ "seconds": [1.0, 2.0], "to": 2, "skip": {skip} }} }},
      {{ "name": "Posing", "ticks": 60, "wait": false, "down": 3, "timeout": 1 }},
      {{ "name": "Fire", "ticks": 8, "script": "onFire", "timeout": 1 }}
    ] }}
  }}
}}"#
        )
        .as_bytes(),
    )
    .unwrap();
    let mut w = WeaponsWorld::new(pack).unwrap();
    w.add_actor(A, 5).unwrap();
    w.give(A, "t:weapon/gun").unwrap();
    w.equip(A, Some(0)).unwrap();
    w
}

fn name(w: &WeaponsWorld) -> String {
    w.image_state(A, 0).unwrap().1.name.clone()
}

fn run(w: &mut WeaponsWorld, ticks: usize) -> Vec<String> {
    (0..ticks)
        .map(|_| {
            w.step(&mut Open);
            name(w)
        })
        .collect()
}

#[test]
fn a_rested_image_fidgets_within_its_range_and_returns() {
    let mut w = world(0.0);
    let states = run(&mut w, 18 + 120 * 2 + 60 + 4);
    let first = states.iter().position(|s| s == "Posing").expect("it fidgets");
    // Ready from tick 18 on; a wait of 1 to 2 seconds.
    assert!((18 + 119..=18 + 241).contains(&first), "{first}");
    let back = states[first..].iter().position(|s| s == "Ready").expect("it returns");
    assert_eq!(back, 60, "the posing state times out back into the rest");
}

#[test]
fn the_trigger_interrupts_a_fidget() {
    let mut w = world(0.0);
    run(&mut w, 18 + 120 * 2 + 5);
    assert_eq!(name(&w), "Posing");
    w.trigger(A, true).unwrap();
    run(&mut w, 2);
    assert_eq!(name(&w), "Fire");
}

#[test]
fn skipped_draws_wait_again_and_the_draws_are_deterministic() {
    let seen = |skip: f32| {
        let mut w = world(skip);
        run(&mut w, 120 * 30)
            .windows(2)
            .filter(|p| p[0] == "Ready" && p[1] == "Posing")
            .count()
    };
    assert_eq!(seen(0.0), seen(0.0));
    assert!(seen(0.0) >= 8, "no skips: a fidget every 1 to 2 s plus the 0.5 s clip");
    assert!(seen(0.9) < seen(0.0), "skips space them out");
}

#[test]
fn a_fidget_must_stay_inside_the_image() {
    let bad = |fidget: &str| {
        Pack::from_json(
            format!(
                r#"{{ "schema_version": 5, "id": "t",
  "items": {{ "t:weapon/gun": {{ "ui_name": "Gun", "image": "t:image/gun" }} }},
  "images": {{ "t:image/gun": {{ "states": [ {{ "name": "Ready", "fidget": {fidget} }} ] }} }} }}"#
            )
            .as_bytes(),
        )
        .is_err()
    };
    assert!(bad(r#"{ "seconds": [1.0, 2.0], "to": 5 }"#));
    assert!(bad(r#"{ "seconds": [2.0, 1.0], "to": 0 }"#));
    assert!(bad(r#"{ "seconds": [1.0, 2.0], "to": 0, "skip": 1.0 }"#));
    assert!(!bad(r#"{ "seconds": [1.0, 2.0], "to": 0 }"#));
}
