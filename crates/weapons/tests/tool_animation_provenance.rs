use anyhow::{Context, Result};
use bri_weapons::*;
use glam::Vec3;

const A: ActorId = ActorId(7);
const ITEM: &str = "origin:weapon/rifle";
struct Air;
impl Query for Air {
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
fn world() -> Result<WeaponsWorld> {
    let mut world=WeaponsWorld::new(Pack::from_json(br#"{
        "schema_version":5,"id":"origin",
        "items":{"origin:weapon/rifle":{"ui_name":"Rifle","image":"origin:image/rifle"}},
        "images":{"origin:image/rifle":{
            "commands":{"states":{"onready":"rules:ready"}},
            "states":[{"name":"Activate","script":"onReady","ticks":1,"timeout":1},{"name":"Ready"}]}},"projectiles":{}
    }"#)?)?;
    world.add_actor(A, 3)?;
    world.set_animation_owner(A, AnimationOwner { life: 11, body: 12 })?;
    world.give(A, ITEM)?;
    world.give(A, ITEM)?;
    world.equip(A, Some(0))?;
    Ok(world)
}
fn command(events: Vec<Event>) -> Result<AnimationBinding> {
    events
        .into_iter()
        .find_map(|event| match event {
            Event::ToolFire {
                command: Some(command),
                animation_binding,
                ..
            } if command == "rules:ready" => animation_binding,
            _ => None,
        })
        .context("No original tool command binding")
}

#[test]
fn detached_actor_and_equipped_capture_original_instance_before_queue_delivery() -> Result<()> {
    let mut world = world()?;
    let item = world.animation_item(A, 0);
    let image = world.animation_image(A, 0, false);
    let queued = world.step(&mut Air);
    world.equip(A, Some(1))?;
    world.set_animation_owner(A, AnimationOwner { life: 21, body: 22 })?;
    let captured = command(queued)?;
    assert_eq!(captured.owner, Some(AnimationOwner { life: 11, body: 12 }));
    assert_eq!(captured.item, item);
    assert_eq!(captured.image, image);
    assert_ne!(captured.item, world.animation_item(A, 1));
    assert_eq!(captured.image, world.animation_image(A, 0, false));
    Ok(())
}

#[test]
fn forced_same_id_remount_does_not_repair_a_queued_origin() -> Result<()> {
    let mut world = world()?;
    let queued = world.step(&mut Air);
    let original = world.animation_image(A, 0, false);
    world.equip(A, None)?;
    world.equip(A, Some(0))?;
    let captured = command(queued)?;
    assert_eq!(captured.image, original);
    assert_ne!(captured.image, world.animation_image(A, 0, false));
    assert_eq!(captured.item, world.animation_item(A, 0));
    Ok(())
}
