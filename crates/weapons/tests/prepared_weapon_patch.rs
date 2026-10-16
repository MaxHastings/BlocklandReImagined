//! Prepared exact-instance writes; no Session/package animation bridge.
use anyhow::Result;
use bri_weapons::*;
use glam::Vec3;
use serde_json::Value;

const A: ActorId = ActorId(7);
const B: ActorId = ActorId(8);
const ITEM: &str = "patch:weapon/rifle";
const IMAGE: &str = "patch:image/rifle";

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
    let pack = Pack::from_json(
        br#"{
        "schema_version":5,"id":"patch",
        "items":{"patch:weapon/rifle":{"ui_name":"Rifle","image":"patch:image/rifle"}},
        "images":{"patch:image/rifle":{
            "magazine":{"size":31,"ammo":"rifle","reload_ticks":1,"reserve":0},
            "commands":{"states":{"onbusy":"patch:busy"}},
            "states":[{"name":"Ready"},
                {"name":"Busy","script":"onBusy","sequence":"busy_clip"}]}},
        "projectiles":{}
        }"#,
    )?;
    let mut world = WeaponsWorld::new(pack)?;
    for id in [A, B] {
        world.add_actor(id, 3)?;
        world.set_animation_owner(id, AnimationOwner { life: 11, body: 12 })?;
        world.give(id, ITEM)?;
        world.give(id, ITEM)?;
        world.equip(id, Some(1))?;
        world.set_rounds(id, ITEM, 7)?;
        world.equip(id, Some(0))?;
        world.set_rounds(id, ITEM, 12)?;
    }
    world.step(&mut Air);
    Ok(world)
}

fn binding(world: &WeaponsWorld, id: ActorId) -> AnimationBinding {
    AnimationBinding {
        owner: world.animation_owner(id),
        item: world.animation_item(id, world.actor(id).unwrap().selected.unwrap()),
        image: world.animation_image(id, 0, false),
    }
}

fn snapshot(world: &WeaponsWorld) -> Result<Value> {
    Ok(serde_json::to_value(world.save())?)
}

fn unchanged(world: &mut WeaponsWorld, control: &mut WeaponsWorld) -> Result<()> {
    assert_eq!(snapshot(world)?, snapshot(control)?);
    // Includes pending events which the persistence snapshot intentionally omits.
    assert_eq!(
        serde_json::to_value(world.step(&mut Air))?,
        serde_json::to_value(control.step(&mut Air))?
    );
    assert_eq!(snapshot(world)?, snapshot(control)?);
    Ok(())
}

#[test]
fn bad_expected_range_and_state_leave_actor_and_pending_events_unchanged() -> Result<()> {
    for (expected, rounds, state) in [
        (Some(11), Some(31), Some("Busy")),
        (Some(12), Some(32), Some("Busy")),
        (Some(12), Some(u32::MAX), None),
        (Some(12), Some(31), Some("missing")),
        (None, None, Some("")),
    ] {
        let mut world = world()?;
        let mut control = self::world()?;
        // A preexisting notice must be retained too.
        world.set_rounds(B, ITEM, 10)?;
        control.set_rounds(B, ITEM, 10)?;
        let origin = binding(&world, A);
        assert!(
            world
                .prepare_weapon_patch(A, &origin, expected, rounds, state)
                .is_err()
        );
        unchanged(&mut world, &mut control)?;
    }
    Ok(())
}

#[test]
fn incomplete_forged_foreign_and_nonordinary_bindings_are_refused() -> Result<()> {
    let mut world = world()?;
    let mut control = self::world()?;
    let actual = binding(&world, A);
    let mut invalid = vec![AnimationBinding::default(), binding(&world, B)];
    let mut change = |edit: fn(&mut AnimationBinding)| {
        let mut altered = actual.clone();
        edit(&mut altered);
        invalid.push(altered);
    };
    change(|b| b.owner = None);
    change(|b| b.item = None);
    change(|b| b.image = None);
    change(|b| b.owner.as_mut().unwrap().life += 1);
    change(|b| b.owner.as_mut().unwrap().body += 1);
    change(|b| b.item.as_mut().unwrap().generation += 1);
    change(|b| b.item.as_mut().unwrap().slot = 1);
    change(|b| b.item.as_mut().unwrap().item = "patch:weapon/other".into());
    change(|b| b.image.as_mut().unwrap().generation += 1);
    change(|b| b.image.as_mut().unwrap().hand = 3);
    change(|b| b.image.as_mut().unwrap().emote = true);
    change(|b| b.image.as_mut().unwrap().image = "patch:image/other".into());
    for origin in invalid {
        assert!(
            world
                .prepare_weapon_patch(A, &origin, Some(12), Some(31), Some("Busy"))
                .is_err()
        );
    }
    assert!(
        world
            .prepare_weapon_patch(ActorId(99), &actual, None, None, None)
            .is_err()
    );
    unchanged(&mut world, &mut control)
}

#[test]
fn stale_life_body_custody_selection_and_mount_cannot_patch_a_replacement() -> Result<()> {
    for case in 0..6 {
        let mut world = world()?;
        let mut control = self::world()?;
        let origin = binding(&world, A);
        for target in [&mut world, &mut control] {
            match case {
                0 => target.set_animation_owner(A, AnimationOwner { life: 21, body: 12 })?,
                1 => target.set_animation_owner(A, AnimationOwner { life: 11, body: 22 })?,
                2 => target.equip(A, Some(1))?,
                3 => {
                    target.equip(A, None)?;
                    target.equip(A, Some(0))?;
                }
                4 => {
                    target.set_inventory(A, &[Some(ITEM.into()), Some(ITEM.into()), None])?;
                    target.equip(A, Some(0))?;
                }
                5 => target.remove_actor(A),
                _ => unreachable!(),
            }
        }
        assert!(
            world
                .prepare_weapon_patch(A, &origin, None, Some(31), Some("Busy"))
                .is_err()
        );
        unchanged(&mut world, &mut control)?;
    }
    Ok(())
}

#[test]
fn patch_targets_selected_duplicate_and_preserves_other_actor_reserve_and_identity() -> Result<()> {
    let mut world = world()?;
    world.equip(A, Some(1))?;
    let origin = binding(&world, A);
    let first = world.animation_item(A, 0);
    let serial = world.save().animation_serial;
    let other = serde_json::to_value(world.actor(B))?;
    let reserve = world.reserve(A, "rifle");
    world
        .prepare_weapon_patch(A, &origin, Some(7), Some(31), Some("bUsY"))?
        .commit();
    assert_eq!(world.ammo(A).unwrap().rounds, 31);
    assert_eq!(world.ammo_on_equip(A, 0).unwrap().rounds, 12);
    assert_eq!(world.ammo_on_equip(A, 1).unwrap().rounds, 31);
    assert_eq!(world.image_state(A, 0).unwrap().1.name, "Busy");
    assert_eq!(world.animation_item(A, 0), first);
    assert_eq!(binding(&world, A), origin);
    assert_eq!(world.save().animation_serial, serial);
    assert_eq!(world.reserve(A, "rifle"), reserve);
    assert_eq!(serde_json::to_value(world.actor(B))?, other);
    Ok(())
}

#[test]
fn dropping_guard_aborts_all_prepared_writes() -> Result<()> {
    let mut world = world()?;
    let mut control = self::world()?;
    let origin = binding(&world, A);
    let patch = world.prepare_weapon_patch(A, &origin, Some(12), Some(31), Some("Busy"))?;
    drop(patch);
    unchanged(&mut world, &mut control)
}

#[test]
fn commit_defers_entry_and_matches_existing_rounds_and_image_state_semantics() -> Result<()> {
    let mut world = world()?;
    let mut control = self::world()?;
    let origin = binding(&world, A);
    let patch = world.prepare_weapon_patch(A, &origin, Some(12), Some(31), Some("Busy"))?;
    patch.commit();
    control.set_rounds(A, ITEM, 31)?;
    control.set_state(A, "Busy")?;
    assert_eq!(snapshot(&world)?, snapshot(&control)?);
    let actor = serde_json::to_value(world.actor(A))?;
    assert_eq!(actor["images"][0]["entered"], false);
    let events = world.step(&mut Air);
    assert_eq!(
        serde_json::to_value(&events)?,
        serde_json::to_value(control.step(&mut Air))?
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, Event::Ammo { actor } if *actor == A))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, Event::ToolFire { command: Some(c), .. } if c == "patch:busy"))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, Event::Animation { sequence, .. } if sequence == "busy_clip"))
            .count(),
        1
    );
    assert!(
        !world
            .step(&mut Air)
            .iter()
            .any(|e| matches!(e, Event::ToolFire { command: Some(c), .. } if c == "patch:busy"))
    );
    Ok(())
}

#[test]
fn independent_optional_writes_and_expected_only_guard_preserve_existing_flags() -> Result<()> {
    for (expected, rounds, state) in [
        (None, None, None),
        (Some(12), None, None),
        (Some(12), None, Some("Busy")),
        (None, Some(0), None),
    ] {
        let mut world = world()?;
        let mut control = self::world()?;
        let origin = binding(&world, A);
        world
            .prepare_weapon_patch(A, &origin, expected, rounds, state)?
            .commit();
        if let Some(rounds) = rounds {
            control.set_rounds(A, ITEM, rounds)?;
        }
        if let Some(state) = state {
            control.set_state(A, state)?;
        }
        unchanged(&mut world, &mut control)?;
    }
    Ok(())
}

#[test]
fn bounded_event_capacity_refuses_before_any_patch_is_visible() -> Result<()> {
    let mut world = world()?;
    let mut control = self::world()?;
    for _ in 0..8192 {
        world.set_rounds(A, ITEM, 12)?;
        control.set_rounds(A, ITEM, 12)?;
    }
    let origin = binding(&world, A);
    assert!(
        world
            .prepare_weapon_patch(A, &origin, Some(12), Some(31), Some("Busy"))
            .is_err()
    );
    unchanged(&mut world, &mut control)
}

#[test]
fn no_owner_or_reserve_counted_supply_is_not_a_loaded_magazine_patch() -> Result<()> {
    let mut world = world()?;
    let mut control = self::world()?;
    world.add_actor(ActorId(9), 1)?;
    control.add_actor(ActorId(9), 1)?;
    for target in [&mut world, &mut control] {
        target.give(ActorId(9), ITEM)?;
        target.equip(ActorId(9), Some(0))?;
    }
    let no_owner = binding(&world, ActorId(9));
    assert!(
        world
            .prepare_weapon_patch(ActorId(9), &no_owner, None, None, Some("Busy"))
            .is_err()
    );
    unchanged(&mut world, &mut control)?;
    for supply in [Supply::Unlimited, Supply::Counted] {
        let mut world = self::world()?;
        let origin = binding(&world, A);
        let mut pack = (*world.pack).clone();
        pack.images
            .get_mut(IMAGE)
            .unwrap()
            .magazine
            .as_mut()
            .unwrap()
            .supply = supply;
        world.retune(pack)?;
        assert!(
            world
                .prepare_weapon_patch(A, &origin, None, Some(10), None)
                .is_err()
        );
    }
    Ok(())
}

#[test]
fn invalid_restored_owner_epochs_are_rejected_even_when_the_binding_matches() -> Result<()> {
    let original = world()?;
    for field in ["life", "body"] {
        let mut save = snapshot(&original)?;
        save["actors"][0][1]["animation_owner"][field] = 0.into();
        // Existing weapons restore does not own validation of host epochs.
        let bytes = serde_json::to_vec(&save)?;
        let mut world = WeaponsWorld::restore((*original.pack).clone(), &bytes)?;
        let mut control = WeaponsWorld::restore((*original.pack).clone(), &bytes)?;
        let origin = binding(&world, A);
        assert!(
            world
                .prepare_weapon_patch(A, &origin, Some(12), Some(31), Some("Busy"))
                .is_err()
        );
        unchanged(&mut world, &mut control)?;
    }
    Ok(())
}
