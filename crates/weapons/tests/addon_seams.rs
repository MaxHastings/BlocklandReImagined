//! Seams Add-On weapons lean on: magazines (`set_ammo` per mounted image),
//! taking a tool back, pickup-only items and the cancel key's command.
//! The pack is written here; it is our own.
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

/// Two magazine guns whose `Ready` state falls to `Empty` without ammo,
/// and an ammo box nobody holds.
fn pack() -> Pack {
    let gun = |name: &str| {
        format!(
            r#""kit:image/{name}": {{
                "states": [
                    {{ "name": "Activate", "ticks": 2, "timeout": 1 }},
                    {{ "name": "Ready", "no_ammo": 2, "down": 3 }},
                    {{ "name": "Empty", "ammo": 1 }},
                    {{ "name": "Fire", "ticks": 2, "timeout": 1 }}
                ],
                "commands": {{ "cancel": "kit:switch_round" }}
            }}"#
        )
    };
    let json = format!(
        r#"{{
            "schema_version": {SCHEMA},
            "id": "kit",
            "items": {{
                "kit:weapon/a": {{ "ui_name": "Gun A", "image": "kit:image/a" }},
                "kit:weapon/b": {{ "ui_name": "Gun B", "image": "kit:image/b" }},
                "kit:weapon/ammo": {{ "ui_name": "Ammo Box" }}
            }},
            "images": {{ {}, {} }}
        }}"#,
        gun("a"),
        gun("b")
    );
    Pack::from_json(json.as_bytes()).unwrap()
}

fn step(w: &mut WeaponsWorld, ticks: usize) {
    for _ in 0..ticks {
        w.step(&mut Open);
    }
}

fn state(w: &WeaponsWorld) -> String {
    w.image_state(A, 0).unwrap().1.name.clone()
}

#[test]
fn the_next_gun_mounts_loaded_after_emptying_one() {
    let mut w = WeaponsWorld::new(pack()).unwrap();
    w.add_actor(A, 5).unwrap();
    let a = w.give(A, "kit:weapon/a").unwrap();
    let b = w.give(A, "kit:weapon/b").unwrap();
    w.equip(A, Some(a)).unwrap();
    step(&mut w, 10);
    assert_eq!(state(&w), "Ready");
    // The rules empty gun A's magazine.
    w.set_ammo(A, false).unwrap();
    step(&mut w, 2);
    assert_eq!(state(&w), "Empty");
    // Gun B comes out loaded, as `mountImage` and `WeaponImage::onMount`
    // left every freshly mounted image.
    w.equip(A, Some(b)).unwrap();
    step(&mut w, 10);
    assert_eq!(state(&w), "Ready");
    // And gun A, drawn again, is loaded too until the rules say otherwise.
    w.equip(A, Some(a)).unwrap();
    step(&mut w, 10);
    assert_eq!(state(&w), "Ready");
    w.set_ammo(A, false).unwrap();
    step(&mut w, 2);
    assert_eq!(state(&w), "Empty");
}

#[test]
fn take_item_prefers_the_held_slot_and_puts_it_away() {
    let mut w = WeaponsWorld::new(pack()).unwrap();
    w.add_actor(A, 5).unwrap();
    w.give_at(A, 0, "kit:weapon/a").unwrap();
    w.give_at(A, 3, "kit:weapon/a").unwrap();
    w.equip(A, Some(3)).unwrap();
    step(&mut w, 4);
    assert_eq!(w.take_item(A, "kit:weapon/a").unwrap(), Some(3));
    assert!(w.image_state(A, 0).is_none(), "the held gun was put away");
    assert_eq!(w.actor(A).unwrap().selected, None);
    assert_eq!(w.take_item(A, "kit:weapon/a").unwrap(), Some(0));
    assert_eq!(w.take_item(A, "kit:weapon/a").unwrap(), None);
    assert!(w.actor(A).unwrap().inventory.iter().all(Option::is_none));
}

#[test]
fn an_item_without_an_image_is_carried_but_holds_nothing() {
    let mut w = WeaponsWorld::new(pack()).unwrap();
    w.add_actor(A, 5).unwrap();
    let drop = w
        .spawn_drop("kit:weapon/ammo", Vec3::new(0.0, 1.0, 0.0), Vec3::ZERO)
        .unwrap();
    assert!(w.pickup_ready(A, drop));
    let slot = w.pickup(A, drop).unwrap();
    w.equip(A, Some(slot)).unwrap();
    step(&mut w, 4);
    assert!(w.image_state(A, 0).is_none());
    assert_eq!(w.actor(A).unwrap().selected, Some(slot));
    // A drop the rules use up where it lies goes without anyone taking it.
    let drop = w
        .spawn_drop("kit:weapon/ammo", Vec3::ZERO, Vec3::ZERO)
        .unwrap();
    w.step(&mut Open);
    assert!(w.remove_drop(drop));
    assert!(!w.remove_drop(drop));
    assert!(w.drops().all(|d| d.id != drop));
    assert!(!w.pickup_ready(A, drop));
}

#[test]
fn the_cancel_key_command_is_validated_and_found() {
    let pack = pack();
    let commands = &pack.images["kit:image/a"].commands;
    assert_eq!(commands.cancel.as_deref(), Some("kit:switch_round"));
    assert!(commands.runs("kit:switch_round"));
    assert!(!commands.runs("kit:fire"));
    let mut bad = pack.clone();
    bad.images.get_mut("kit:image/a").unwrap().commands.cancel = Some("no package".into());
    assert!(bad.validate().is_err());
}

#[test]
fn a_nameless_item_is_refused_with_its_id() {
    let mut bad = pack();
    bad.items.get_mut("kit:weapon/b").unwrap().ui_name = " ".into();
    let error = format!("{:#}", bad.validate().unwrap_err());
    assert!(error.contains("kit:weapon/b"), "{error}");
    assert!(error.contains("ui_name"), "{error}");
}
