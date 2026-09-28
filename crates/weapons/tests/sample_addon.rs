//! The Bubble Blaster sample Add-On's weapons file loads into the weapons
//! runtime and fires a harmless, floaty, shoving bubble.
use bri_weapons::*;
use glam::Vec3;
use std::path::PathBuf;

struct Empty;
impl Query for Empty {
    fn sweep(&mut self, _: Vec3, _: Vec3, _: Filter) -> Option<Hit> {
        None
    }
    fn radius(&mut self, _: Vec3, _: f32, _: usize) -> Vec<Nearby> {
        vec![]
    }
    fn can_affect(&self, _: ActorId, _: TargetId) -> bool {
        true
    }
    fn can_catch(&self, _: ActorId, _: ActorId) -> bool {
        false
    }
}

#[test]
fn bubble_blaster_sample_fires_a_harmless_bubble() {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/samples/sample-bubble-blaster/assets/weapons.json");
    let pack = Pack::from_json(&std::fs::read(file).unwrap()).unwrap();
    let bubble = &pack.projectiles["sample-bubble-blaster:projectile/bubble"];
    assert_eq!(bubble.damage, 0.0);
    assert!(bubble.impulse > 0.0);
    let mut world = WeaponsWorld::new(pack).unwrap();
    world.add_actor(ActorId(1), 5).unwrap();
    let slot = world
        .give(ActorId(1), "sample-bubble-blaster:weapon/bubble_blaster")
        .unwrap();
    world.equip(ActorId(1), Some(slot)).unwrap();
    let mut spawned = vec![];
    for tick in 0..240 {
        // One click: press, then release on the next tick.
        if tick == 60 || tick == 61 {
            world.trigger(ActorId(1), tick == 60).unwrap();
        }
        for e in world.step(&mut Empty) {
            if let Event::Spawned { definition, .. } = e {
                spawned.push(definition);
            }
        }
    }
    assert_eq!(spawned, ["sample-bubble-blaster:projectile/bubble"]);
}
