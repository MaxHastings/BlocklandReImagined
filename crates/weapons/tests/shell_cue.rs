//! A casing is thrown on the frame the clip ejects it (a state cue), not as
//! the state starts. Content-free authored pack.
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

#[test]
fn a_cue_ejects_the_shell_after_its_delay() {
    let pack = Pack::from_json(
        br#"{
  "schema_version": 5, "id": "t",
  "items": { "t:weapon/gun": { "ui_name": "Gun", "image": "t:image/gun" } },
  "images": { "t:image/gun": { "casing": "t:casing", "states": [
    { "name": "Ready", "down": 1 },
    { "name": "Fire", "ticks": 60, "timeout": 0,
      "cues": [ { "after_ms": 250, "eject_shell": true } ] }
  ] } }
}"#,
    )
    .unwrap();
    let a = ActorId(1);
    let mut w = WeaponsWorld::new(pack).unwrap();
    w.add_actor(a, 5).unwrap();
    w.give(a, "t:weapon/gun").unwrap();
    w.equip(a, Some(0)).unwrap();
    w.step(&mut Open);
    w.trigger(a, true).unwrap();
    let mut shells = Vec::new();
    for tick in 0..60 {
        let events = w.step(&mut Open);
        if events.iter().any(|e| matches!(e, Event::Shell { .. })) {
            shells.push(tick);
        }
    }
    assert_eq!(shells.len(), 1, "one shell: {shells:?}");
    assert!(
        (28..=32).contains(&shells[0]),
        "250 ms is 30 ticks after the state began: {shells:?}"
    );
}
