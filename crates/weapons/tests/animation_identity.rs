//! Native producer stamps, independent of names and presentation cue timing.
use anyhow::{Context, Result};
use bri_weapons::*;
use glam::Vec3;

const A: ActorId = ActorId(7);
const ITEM: &str = "identity:weapon/item";
const IMAGE: &str = "identity:image/item";
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
fn world() -> Result<WeaponsWorld> {
    let pack = Pack::from_json(
        br#"{
        "schema_version":5,"id":"identity",
        "items":{"identity:weapon/item":{"ui_name":"Item","image":"identity:image/item"}},
        "images":{"identity:image/item":{"states":[
            {"name":"Activate","sequence":"native_clip","ticks":1,"timeout":1,
                "cues":[{"after_ms":100,"thread":2,"sequence":"deferred_clip"}]},
            {"name":"Ready"}]}},"projectiles":{}
    }"#,
    )?;
    let mut world = WeaponsWorld::new(pack)?;
    world.add_actor(A, 3)?;
    world.set_animation_owner(A, AnimationOwner { life: 11, body: 12 })?;
    world.give(A, ITEM)?;
    world.give(A, ITEM)?;
    world.equip(A, Some(0))?;
    Ok(world)
}
fn start(events: Vec<Event>, sequence_name: &str) -> Result<AnimationProvenance> {
    events
        .into_iter()
        .find_map(|event| match event {
            Event::Animation {
                sequence,
                provenance,
                ..
            } if sequence == sequence_name => provenance,
            _ => None,
        })
        .context("No actual native producer stamp")
}

#[test]
fn equal_items_and_temporary_state_take_reinsert_preserve_the_right_incarnations() -> Result<()> {
    let mut world = world()?;
    let first = world.animation_item(A, 0).unwrap();
    let second = world.animation_item(A, 1).unwrap();
    assert_eq!(first.item, second.item);
    assert_ne!(first.generation, second.generation);
    let image = world.animation_image(A, 0, false).unwrap();
    let event = start(world.step(&mut Empty), "native_clip")?;
    assert_eq!(event.binding.item.as_ref(), Some(&first));
    assert_eq!(event.binding.image.as_ref(), Some(&image));
    assert_eq!(
        event.binding.owner,
        Some(AnimationOwner { life: 11, body: 12 })
    );
    for _ in 0..4 {
        world.step(&mut Empty);
    }
    assert_eq!(world.animation_image(A, 0, false).as_ref(), Some(&image));
    world.equip(A, Some(1))?;
    assert_eq!(world.animation_image(A, 0, false).as_ref(), Some(&image));
    world.set_state(A, "Activate")?;
    let next = start(world.step(&mut Empty), "native_clip")?;
    assert_eq!(next.binding.item.as_ref(), Some(&second));
    assert_eq!(next.binding.image.as_ref(), Some(&image));
    assert!(next.serial > event.serial);
    assert!(world.give_at(A, 1, ITEM).is_err());
    assert_eq!(world.animation_item(A, 1).as_ref(), Some(&second));
    Ok(())
}

#[test]
fn deferred_cue_keeps_queue_time_owner_item_and_mount_instead_of_current_selection() -> Result<()> {
    let mut world = world()?;
    let first = world.animation_item(A, 0).unwrap();
    let image = world.animation_image(A, 0, false).unwrap();
    world.step(&mut Empty);
    world.equip(A, Some(1))?;
    world.set_animation_owner(A, AnimationOwner { life: 21, body: 22 })?;
    let mut captured = None;
    for _ in 0..13 {
        for event in world.step(&mut Empty) {
            if let Event::Animation {
                sequence,
                provenance,
                ..
            } = event
                && sequence == "deferred_clip"
            {
                captured = provenance;
            }
        }
    }
    let captured = captured.context("Deferred native cue never fired")?;
    assert_eq!(
        captured.binding.owner,
        Some(AnimationOwner { life: 11, body: 12 })
    );
    assert_eq!(captured.binding.item.as_ref(), Some(&first));
    assert_eq!(captured.binding.image.as_ref(), Some(&image));
    assert_ne!(captured.binding.item, world.animation_item(A, 1));
    Ok(())
}

#[test]
fn drop_pickup_and_explicit_equal_id_loadout_replacement_get_fresh_custody() -> Result<()> {
    let mut world = world()?;
    let first = world.animation_item(A, 0).unwrap();
    let second = world.animation_item(A, 1).unwrap();
    let drop = world.drop_item(A, 0)?;
    assert!(world.animation_item(A, 0).is_none());
    assert!(world.animation_image(A, 0, false).is_none());
    let pickup_after = world.drops().find(|d| d.id == drop).unwrap().pickup_after;
    while world.tick < pickup_after {
        world.step(&mut Empty);
    }
    assert_eq!(world.pickup(A, drop)?, 0);
    let fresh = world.animation_item(A, 0).unwrap();
    assert_eq!(fresh.item, first.item);
    assert_ne!(fresh.generation, first.generation);
    assert_eq!(world.animation_item(A, 1).as_ref(), Some(&second));
    world.set_inventory(A, &[Some(ITEM.into()), Some(ITEM.into()), None])?;
    assert_ne!(
        world.animation_item(A, 0).unwrap().generation,
        fresh.generation
    );
    assert_ne!(
        world.animation_item(A, 1).unwrap().generation,
        second.generation
    );
    Ok(())
}

#[test]
fn worn_image_does_not_borrow_the_selected_hand_item() -> Result<()> {
    let mut world = world()?;
    world.step(&mut Empty);
    world.wear(A, 2, Some(IMAGE), None)?;
    let event = world
        .step(&mut Empty)
        .into_iter()
        .find_map(|event| match event {
            Event::Animation {
                image_hand: Some(2),
                provenance,
                ..
            } => provenance,
            _ => None,
        })
        .context("Worn native image did not animate")?;
    assert!(event.binding.item.is_none());
    assert_eq!(event.binding.image.as_ref().unwrap().hand, 2);
    Ok(())
}

#[test]
fn checkpoint_preserves_deferred_provenance_and_allocator_after_removed_instances() -> Result<()> {
    let mut original = world()?;
    let first = start(original.step(&mut Empty), "native_clip")?;
    original.equip(A, Some(1))?;
    original.set_animation_owner(A, AnimationOwner { life: 21, body: 22 })?;
    let bytes = serde_json::to_vec(&original.save())?;
    let mut restored = WeaponsWorld::restore((*original.pack).clone(), &bytes)?;
    for _ in 0..14 {
        assert_eq!(
            serde_json::to_value(original.step(&mut Empty))?,
            serde_json::to_value(restored.step(&mut Empty))?
        );
    }
    // The high-water mark must survive even when no live item/mount can be
    // scanned to reconstruct the last emitted producer serial.
    original.remove_actor(A);
    // Checkpoints resume fixed-tick state after its transient events are read.
    original.step(&mut Empty);
    let save = original.save();
    let watermark = serde_json::to_value(&save)?["animation_serial"]
        .as_u64()
        .context("Checkpoint omitted animation allocator")?;
    assert!(watermark > first.serial);
    let mut restored =
        WeaponsWorld::restore((*original.pack).clone(), &serde_json::to_vec(&save)?)?;
    for world in [&mut original, &mut restored] {
        world.add_actor(A, 3)?;
        world.set_animation_owner(A, AnimationOwner { life: 31, body: 32 })?;
        world.give(A, ITEM)?;
        assert!(world.animation_item(A, 0).unwrap().generation > watermark);
        world.equip(A, Some(0))?;
    }
    assert_eq!(
        serde_json::to_value(original.step(&mut Empty))?,
        serde_json::to_value(restored.step(&mut Empty))?
    );
    assert_eq!(
        serde_json::to_value(original.save())?,
        serde_json::to_value(restored.save())?
    );
    Ok(())
}

#[test]
fn checkpoint_rejects_missing_or_rewound_animation_allocator_and_old_schema() -> Result<()> {
    let world = world()?;
    let original = serde_json::to_value(world.save())?;
    let mut missing = original.clone();
    missing.as_object_mut().unwrap().remove("animation_serial");
    assert!(WeaponsWorld::restore((*world.pack).clone(), &serde_json::to_vec(&missing)?).is_err());
    let mut rewound = original.clone();
    rewound["animation_serial"] = 1.into();
    assert!(WeaponsWorld::restore((*world.pack).clone(), &serde_json::to_vec(&rewound)?).is_err());
    let mut duplicated = original.clone();
    duplicated["actors"][0][1]["animation_items"][1] =
        duplicated["actors"][0][1]["animation_items"][0].clone();
    assert!(
        WeaponsWorld::restore((*world.pack).clone(), &serde_json::to_vec(&duplicated)?).is_err()
    );
    let mut old = original;
    old["schema_version"] = 3.into();
    assert!(WeaponsWorld::restore((*world.pack).clone(), &serde_json::to_vec(&old)?).is_err());
    Ok(())
}

#[test]
fn checkpoint_preserves_exhausted_allocator_without_reusing_identity() -> Result<()> {
    let world = world()?;
    let mut save = serde_json::to_value(world.save())?;
    save["animation_serial"] = u64::MAX.into();
    let mut restored = WeaponsWorld::restore((*world.pack).clone(), &serde_json::to_vec(&save)?)?;
    restored.give_at(A, 2, ITEM)?;
    assert!(restored.animation_item(A, 2).is_none());
    restored.equip(A, Some(0))?;
    assert!(restored.step(&mut Empty).into_iter().all(|e| !matches!(
        e,
        Event::Animation {
            provenance: Some(_),
            ..
        }
    )));
    assert_eq!(
        serde_json::to_value(restored.save())?["animation_serial"],
        u64::MAX
    );
    WeaponsWorld::restore(
        (*restored.pack).clone(),
        &serde_json::to_vec(&restored.save())?,
    )?;
    Ok(())
}
