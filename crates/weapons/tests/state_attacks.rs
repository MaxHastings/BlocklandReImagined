//! Image state scripts as data (`Image::scripts`), what a port writes for a
//! v20 Add-On's `Image::on...` functions: the holder's arm animation, a
//! second attack launching its own projectile and a thrown item used up on
//! the throw. The packs are written here; they are our own.
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

/// A knife: a click jabs, holding for 12 ticks then letting go stabs.
const KNIFE: &str = r#"{
    "schema_version": 4,
    "id": "kit",
    "items": { "kit:weapon/knife": { "ui_name": "Knife", "image": "kit:image/knife" } },
    "images": {
        "kit:image/knife": {
            "projectile": "kit:projectile/stab",
            "states": [
                { "name": "Activate", "ticks": 4, "timeout": 1 },
                { "name": "Ready", "down": 2 },
                { "name": "Charge", "ticks": 12, "wait": false, "timeout": 5, "up": 3,
                  "allow_change": false, "script": "onCharge" },
                { "name": "Jab", "ticks": 4, "timeout": 4, "script": "onFiretwo" },
                { "name": "StopFire", "ticks": 4, "timeout": 1, "script": "onStopFire" },
                { "name": "Armed", "up": 6, "allow_change": false },
                { "name": "Stab", "ticks": 4, "timeout": 1, "script": "onFire" }
            ],
            "scripts": {
                "oncharge": { "arm": "spearReady" },
                "onfiretwo": { "arm": "armattack", "fire": true, "projectile": "kit:projectile/jab" },
                "onfire": { "arm": "spearThrow", "fire": true }
            }
        }
    },
    "projectiles": {
        "kit:projectile/jab": { "damage": 30, "lifetime_ticks": 12 },
        "kit:projectile/stab": { "damage": 100, "lifetime_ticks": 12 }
    }
}"#;

/// A grenade: the first press pulls the pin (a casing), the second throws
/// it and the grenade is gone from the holder's tools.
const GRENADE: &str = r#"{
    "schema_version": 4,
    "id": "kit",
    "items": { "kit:weapon/grenade": { "ui_name": "Grenade", "image": "kit:image/grenade" } },
    "images": {
        "kit:image/grenade": {
            "projectile": "kit:projectile/grenade",
            "casing": "kitPinDebris",
            "states": [
                { "name": "Activate", "ticks": 4, "timeout": 1 },
                { "name": "Ready", "down": 2 },
                { "name": "Pin", "ticks": 4, "timeout": 3, "eject_shell": true, "allow_change": false },
                { "name": "Pinned", "down": 4, "allow_change": false },
                { "name": "Fire", "ticks": 4, "timeout": 5, "script": "onFire",
                  "allow_change": false },
                { "name": "Done" }
            ],
            "scripts": { "onfire": { "arm": "spearThrow", "fire": true, "use_up": true } }
        }
    },
    "projectiles": {
        "kit:projectile/grenade": { "speed": 30, "lifetime_ticks": 300, "ballistic": true,
                                    "explode_death": true }
    },
    "definitions": [
        { "name": "kitPinDebris", "class": "DebrisData", "parent": null,
          "source": { "path": "kit/weapons.json", "sha256": "", "line": 0 },
          "fields": { "shapefile": "kit/pin.dts" } }
    ]
}"#;

fn world(json: &str) -> WeaponsWorld {
    let mut w = WeaponsWorld::new(Pack::from_json(json.as_bytes()).unwrap()).unwrap();
    w.add_actor(A, 5).unwrap();
    w
}

fn step(w: &mut WeaponsWorld, ticks: usize, events: &mut Vec<Event>) {
    for _ in 0..ticks {
        events.extend(w.step(&mut Open));
    }
}

fn arm(events: &[Event]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Animation {
                thread: 2,
                sequence,
                ..
            } => Some(sequence.clone()),
            _ => None,
        })
        .collect()
}

fn launched(events: &[Event]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Spawned { definition, .. } => Some(definition.clone()),
            _ => None,
        })
        .collect()
}

fn state(w: &WeaponsWorld) -> String {
    w.image_state(A, 0).unwrap().1.name.clone()
}

#[test]
fn a_click_jabs_with_the_scripts_own_projectile() {
    let mut w = world(KNIFE);
    let slot = w.give(A, "kit:weapon/knife").unwrap();
    w.equip(A, Some(slot)).unwrap();
    let mut events = Vec::new();
    step(&mut w, 8, &mut events);
    assert_eq!(state(&w), "Ready");
    events.clear();
    w.trigger(A, true).unwrap();
    step(&mut w, 3, &mut events);
    assert_eq!(
        state(&w),
        "Charge",
        "a charge does not wait for its timeout"
    );
    w.trigger(A, false).unwrap();
    step(&mut w, 12, &mut events);
    assert_eq!(launched(&events), ["kit:projectile/jab"]);
    assert_eq!(arm(&events), ["spearReady", "armattack", "root"]);
    assert_eq!(state(&w), "Ready");
}

#[test]
fn a_held_charge_stabs_with_the_images_projectile() {
    let mut w = world(KNIFE);
    let slot = w.give(A, "kit:weapon/knife").unwrap();
    w.equip(A, Some(slot)).unwrap();
    let mut events = Vec::new();
    step(&mut w, 8, &mut events);
    events.clear();
    w.trigger(A, true).unwrap();
    step(&mut w, 20, &mut events);
    assert_eq!(state(&w), "Armed");
    assert!(
        launched(&events).is_empty(),
        "nothing is launched while charged"
    );
    w.trigger(A, false).unwrap();
    step(&mut w, 8, &mut events);
    assert_eq!(launched(&events), ["kit:projectile/stab"]);
    assert_eq!(arm(&events), ["spearReady", "spearThrow"]);
}

#[test]
fn a_thrown_grenade_is_used_up_and_the_next_one_stays() {
    let mut w = world(GRENADE);
    w.give_at(A, 0, "kit:weapon/grenade").unwrap();
    w.give_at(A, 1, "kit:weapon/grenade").unwrap();
    w.equip(A, Some(0)).unwrap();
    let mut events = Vec::new();
    step(&mut w, 8, &mut events);
    events.clear();
    // The first press pulls the pin.
    w.trigger(A, true).unwrap();
    step(&mut w, 2, &mut events);
    w.trigger(A, false).unwrap();
    step(&mut w, 8, &mut events);
    assert_eq!(state(&w), "Pinned");
    assert!(
        events.iter().any(|e| matches!(e, Event::Shell { .. })),
        "the pin flies off"
    );
    assert!(launched(&events).is_empty());
    // The second throws it.
    w.trigger(A, true).unwrap();
    step(&mut w, 8, &mut events);
    assert_eq!(launched(&events), ["kit:projectile/grenade"]);
    let a = w.actor(A).unwrap();
    assert_eq!(a.inventory[0], None, "the thrown grenade left the tools");
    assert_eq!(a.inventory[1].as_deref(), Some("kit:weapon/grenade"));
    assert_eq!(a.selected, None);
    assert!(w.image_state(A, 0).is_none(), "the hand is empty");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Unmounted { hand: 0, .. }))
    );
    assert_eq!(w.projectiles().count(), 1, "the grenade flies on");
}

#[test]
fn script_projectiles_and_arm_animations_are_checked() {
    let missing = KNIFE.replace(
        "\"projectile\": \"kit:projectile/jab\"",
        "\"projectile\": \"kit:projectile/none\"",
    );
    assert!(Pack::from_json(missing.as_bytes()).is_err());
    let unfired = KNIFE.replace("\"fire\": true, \"projectile\"", "\"projectile\"");
    assert!(
        Pack::from_json(unfired.as_bytes()).is_err(),
        "a projectile needs fire"
    );
    let upper = KNIFE.replace("\"onfiretwo\":", "\"onFiretwo\":");
    assert!(
        Pack::from_json(upper.as_bytes()).is_err(),
        "script names are lower case"
    );
    let bad = KNIFE.replace("\"spearThrow\"", "\"spear throw\"");
    assert!(Pack::from_json(bad.as_bytes()).is_err());
    let long = KNIFE.replace("\"spearThrow\"", &format!("\"{}\"", "a".repeat(65)));
    assert!(Pack::from_json(long.as_bytes()).is_err());
    assert!(Pack::from_json(KNIFE.as_bytes()).is_ok());
}
