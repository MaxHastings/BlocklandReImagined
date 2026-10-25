//! A tool is put down before the next comes up, a tool asked for again while
//! it goes down comes back, and a later draw starts in the quick raise.
//! Content-free authored pack.
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

/// Rifle: Draw (long) → Ready; asked to give way it plays Lower (not
/// interruptible) into Holstered, which allows the change; a later draw
/// starts in Raise (short). Grenade: plain.
fn world() -> WeaponsWorld {
    let pack = Pack::from_json(
        br#"{
  "schema_version": 5,
  "id": "t",
  "items": {
    "t:weapon/rifle": { "ui_name": "Rifle", "image": "t:image/rifle" },
    "t:weapon/grenade": { "ui_name": "Grenade", "image": "t:image/grenade" }
  },
  "images": {
    "t:image/rifle": { "lower": 2, "redraw": 4, "states": [
      { "name": "Draw", "ticks": 30, "allow_change": false, "timeout": 1 },
      { "name": "Ready" },
      { "name": "Lower", "ticks": 20, "allow_change": false, "timeout": 3 },
      { "name": "Holstered" },
      { "name": "Raise", "ticks": 5, "allow_change": false, "timeout": 1 }
    ] },
    "t:image/grenade": { "states": [ { "name": "Ready" } ] }
  }
}"#,
    )
    .unwrap();
    let mut w = WeaponsWorld::new(pack).unwrap();
    w.add_actor(A, 5).unwrap();
    w.give(A, "t:weapon/rifle").unwrap();
    w.give(A, "t:weapon/grenade").unwrap();
    w
}
fn run(w: &mut WeaponsWorld, ticks: u32) {
    for _ in 0..ticks {
        w.step(&mut Open);
    }
}
fn held(w: &WeaponsWorld) -> (String, String) {
    let (image, state) = w.image_state(A, 0).unwrap();
    (image.id.clone(), state.name.clone())
}

#[test]
fn a_tool_is_lowered_before_the_next_one_comes_up() {
    let mut w = world();
    w.equip(A, Some(0)).unwrap();
    run(&mut w, 2);
    assert_eq!(held(&w).1, "Draw");
    run(&mut w, 40);
    assert_eq!(held(&w).1, "Ready");
    w.equip(A, Some(1)).unwrap();
    run(&mut w, 2);
    assert_eq!(held(&w), ("t:image/rifle".into(), "Lower".into()));
    run(&mut w, 10);
    assert_eq!(held(&w).1, "Lower", "still going down");
    run(&mut w, 20);
    assert_eq!(held(&w).0, "t:image/grenade", "changed once it was down");
}

#[test]
fn a_tool_asked_for_again_while_it_goes_down_comes_back_up_quickly() {
    let mut w = world();
    w.equip(A, Some(0)).unwrap();
    run(&mut w, 40);
    w.equip(A, Some(1)).unwrap();
    run(&mut w, 5);
    w.equip(A, Some(0)).unwrap();
    run(&mut w, 2);
    assert_eq!(held(&w), ("t:image/rifle".into(), "Raise".into()));
    run(&mut w, 10);
    assert_eq!(held(&w), ("t:image/rifle".into(), "Ready".into()));
}

#[test]
fn a_later_draw_starts_in_the_redraw_state() {
    let mut w = world();
    w.equip(A, Some(0)).unwrap();
    run(&mut w, 40);
    w.equip(A, Some(1)).unwrap();
    run(&mut w, 40);
    assert_eq!(held(&w).0, "t:image/grenade");
    w.equip(A, Some(0)).unwrap();
    run(&mut w, 2);
    assert_eq!(held(&w), ("t:image/rifle".into(), "Raise".into()));
    w.respawned(A).unwrap();
    w.equip(A, None).unwrap();
    w.equip(A, Some(0)).unwrap();
    run(&mut w, 2);
    assert_eq!(held(&w).1, "Draw", "a new life draws in full");
}
