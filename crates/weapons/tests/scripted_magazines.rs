//! Magazines whose image states run them, as a v20 script ammo system did
//! (Tier+Tactical's): state scripts set `loaded` and `ammo`, the rounds
//! move only as the image enters its reload state, and an empty gun goes
//! to its reload states by its own checks. The pack is written here; it is
//! our own.
use bri_weapons::*;
use glam::Vec3;

const A: ActorId = ActorId(1);

struct Air;
impl Query for Air {
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
        false
    }
}

/// A 4-shell pump loaded a shell at a time and a 3-round pistol, with
/// state graphs shaped like Tier+Tactical's: the pump checks the magazine
/// after every shot (`CheckLoad`), between shells (`CheckReload`) and
/// before ejecting (`CheckAlive`, which only says the holder lives); the
/// pistol's light key goes through `ManualReload`, straight to the
/// magazine change when it is not empty.
const GUNS: &str = r#"{
    "schema_version": 3,
    "id": "mag",
    "items": {
        "mag:weapon/pump": { "ui_name": "Pump", "image": "mag:image/pump" },
        "mag:weapon/pistol": { "ui_name": "Pistol", "image": "mag:image/pistol" }
    },
    "images": {
        "mag:image/pump": {
            "projectile": "mag:projectile/pellet",
            "magazine": {
                "size": 4, "ammo": "shells", "reload_ticks": 15, "one_by_one": true,
                "reserve": 2, "reload_state": "onReloaded",
                "checks": {
                    "CheckLoad": { "loaded": ["shot"], "ammo": ["reserve"] },
                    "CheckReload": { "loaded": ["full", "no_reserve"], "ammo": ["reserve"] },
                    "CheckAlive": { "ammo": true }
                },
                "on_reload": { "loaded": false, "ammo": ["empty"] },
                "light_states": ["Ready", "Empty"]
            },
            "states": [
                { "name": "Activate", "ticks": 4, "timeout": 5 },
                { "name": "Ready", "down": 2, "not_loaded": 6 },
                { "name": "Fire", "ticks": 6, "timeout": 3, "script": "onFire", "allow_change": false },
                { "name": "AliveA", "ticks": 1, "timeout": 4, "script": "CheckAlive" },
                { "name": "AliveB", "ammo": 13 },
                { "name": "LoadA", "ticks": 1, "timeout": 7, "script": "CheckLoad" },
                { "name": "ReloadA", "ticks": 1, "timeout": 8, "script": "CheckReload" },
                { "name": "LoadB", "loaded": 1, "ammo": 10, "no_ammo": 9 },
                { "name": "ReloadB", "loaded": 11, "not_loaded": 7 },
                { "name": "Empty", "ammo": 10, "loaded": 1, "down": 2 },
                { "name": "Reload", "ticks": 10, "timeout": 12, "down": 2, "script": "onReloadStart" },
                { "name": "Pump", "ticks": 6, "timeout": 1 },
                { "name": "Reloaded", "ticks": 5, "timeout": 6, "script": "onReloaded" },
                { "name": "Eject", "ticks": 6, "timeout": 5 }
            ]
        },
        "mag:image/pistol": {
            "projectile": "mag:projectile/pellet",
            "magazine": {
                "size": 3, "ammo": "nine", "reload_ticks": 40, "reserve": 9,
                "reload_state": "onReloaded",
                "checks": {
                    "CheckLoad": { "loaded": ["shot"], "ammo": ["reserve"] },
                    "CheckAlive": { "ammo": true }
                },
                "on_reload": { "loaded": false, "ammo": ["empty"] },
                "on_loaded": { "loaded": true },
                "light_states": ["Ready", "Empty"]
            },
            "states": [
                { "name": "Activate", "ticks": 4, "timeout": 7 },
                { "name": "Ready", "down": 2, "not_loaded": 13 },
                { "name": "FireCheckA", "ticks": 1, "timeout": 3, "script": "CheckLoad" },
                { "name": "FireCheckB", "loaded": 4, "not_loaded": 15 },
                { "name": "Fire", "ticks": 6, "timeout": 5, "script": "onFire", "allow_change": false },
                { "name": "Smoke", "up": 6 },
                { "name": "Wait", "ticks": 1, "timeout": 7 },
                { "name": "LoadCheckA", "ticks": 1, "timeout": 8, "script": "CheckLoad" },
                { "name": "LoadCheckB", "loaded": 1, "not_loaded": 14 },
                { "name": "MagOut", "ticks": 10, "timeout": 10 },
                { "name": "MagIn", "ticks": 10, "timeout": 12 },
                { "name": "Reloaded", "ticks": 10, "timeout": 1, "no_ammo": 14, "script": "onReloaded" },
                { "name": "Alive", "ticks": 1, "timeout": 11, "script": "CheckAlive" },
                { "name": "ManualReload", "ammo": 9, "no_ammo": 10 },
                { "name": "Empty", "ammo": 9, "loaded": 1, "down": 2 },
                { "name": "EmptyFire", "ammo": 9, "loaded": 1, "up": 14 }
            ]
        }
    },
    "projectiles": {
        "mag:projectile/pellet": { "speed": 200, "lifetime_ticks": 2 }
    }
}"#;

fn world(item: &str) -> WeaponsWorld {
    let mut w = WeaponsWorld::new(Pack::from_json(GUNS.as_bytes()).unwrap()).unwrap();
    w.add_actor(A, 5).unwrap();
    let slot = w.give(A, item).unwrap();
    w.equip(A, Some(slot)).unwrap();
    step(&mut w, 10);
    w
}

fn step(w: &mut WeaponsWorld, ticks: usize) -> Vec<Event> {
    (0..ticks).flat_map(|_| w.step(&mut Air)).collect()
}

fn click(w: &mut WeaponsWorld) -> usize {
    w.trigger(A, true).unwrap();
    let mut events = step(w, 2);
    w.trigger(A, false).unwrap();
    events.extend(step(w, 30));
    events
        .iter()
        .filter(|e| matches!(e, Event::Spawned { .. }))
        .count()
}

fn state(w: &WeaponsWorld) -> String {
    w.image_state(A, 0).unwrap().1.name.clone()
}

fn rounds(w: &WeaponsWorld) -> (u32, Reserve) {
    let v = w.ammo(A).unwrap();
    (v.rounds, v.reserve)
}

/// A shot and the states visited over the `ticks` after it.
fn shot_path(w: &mut WeaponsWorld, ticks: usize) -> Vec<String> {
    w.trigger(A, true).unwrap();
    let mut seen = path(w, 2);
    w.trigger(A, false).unwrap();
    for s in path(w, ticks) {
        if seen.last() != Some(&s) {
            seen.push(s);
        }
    }
    seen
}

/// The states visited over `ticks`, each once in a row.
fn path(w: &mut WeaponsWorld, ticks: usize) -> Vec<String> {
    let mut seen: Vec<String> = vec![];
    for _ in 0..ticks {
        step(w, 1);
        let now = state(w);
        if seen.last() != Some(&now) {
            seen.push(now);
        }
    }
    seen
}

#[test]
fn an_empty_pump_loads_a_shell_per_pass_through_its_reload_states() {
    let mut w = world("mag:weapon/pump");
    assert_eq!(state(&w), "Ready");
    for _ in 0..3 {
        assert_eq!(click(&mut w), 1);
    }
    // Empty with two shells in reserve: its own check sends it to the
    // reload states, and each pass through Reloaded moves one shell.
    let seen = shot_path(&mut w, 120);
    assert_eq!(rounds(&w), (2, Reserve::Rounds(0)), "{seen:?}");
    let passes = seen.iter().filter(|s| *s == "Reloaded").count();
    assert_eq!(passes, 2, "{seen:?}");
    // Out of reserve, the reload check counts it as loaded: it pumps and
    // is ready, not stuck waiting for shells.
    assert_eq!(state(&w), "Ready", "{seen:?}");
    assert!(seen.contains(&"Pump".to_string()), "{seen:?}");
    assert_eq!(click(&mut w), 1);
}

#[test]
fn a_pump_with_no_reserve_still_ejects_after_its_last_shot() {
    let mut w = world("mag:weapon/pump");
    w.set_reserve(A, "shells", Reserve::Rounds(0)).unwrap();
    for _ in 0..4 {
        assert_eq!(click(&mut w), 1);
    }
    // The alive check, not the reserve, lets it through to the eject.
    assert_eq!(state(&w), "Empty");
    assert_eq!(click(&mut w), 0);
    assert_eq!(rounds(&w), (0, Reserve::Rounds(0)));
    // Shells picked up while empty start the reload from Empty.
    w.give_ammo(A, "shells", 3).unwrap();
    path(&mut w, 120);
    assert_eq!(rounds(&w), (3, Reserve::Rounds(0)));
}

#[test]
fn the_pistols_light_key_skips_the_empty_magazine_states_when_rounds_are_left() {
    let mut w = world("mag:weapon/pistol");
    assert_eq!(click(&mut w), 1);
    assert_eq!(state(&w), "Ready");
    assert!(w.reload(A).unwrap());
    let mut seen = vec![state(&w)];
    seen.extend(path(&mut w, 60));
    assert_eq!(rounds(&w), (3, Reserve::Rounds(8)), "{seen:?}");
    assert!(!seen.contains(&"MagOut".to_string()), "{seen:?}");
    assert!(seen.contains(&"MagIn".to_string()), "{seen:?}");
    assert_eq!(state(&w), "Ready");
    // Emptied, it goes through every reload state by itself.
    for _ in 0..2 {
        assert_eq!(click(&mut w), 1);
    }
    let seen = shot_path(&mut w, 80);
    assert!(
        seen.contains(&"MagOut".to_string()) && seen.contains(&"Reloaded".to_string()),
        "{seen:?}"
    );
    assert_eq!(rounds(&w), (3, Reserve::Rounds(5)));
}

#[test]
fn the_light_key_does_nothing_outside_its_reload_states() {
    let mut w = world("mag:weapon/pistol");
    click(&mut w);
    w.trigger(A, true).unwrap();
    step(&mut w, 12);
    // Held in Smoke: no reload starts.
    assert_eq!(state(&w), "Smoke");
    assert!(!w.reload(A).unwrap());
    w.trigger(A, false).unwrap();
    step(&mut w, 20);
    assert_eq!(state(&w), "Ready");
    assert_eq!(rounds(&w).0, 1);
}

#[test]
fn checks_need_a_reload_state_and_facts() {
    let bad = GUNS.replacen(r#""reload_state": "onReloaded","#, "", 1);
    assert!(Pack::from_json(bad.as_bytes()).is_err());
    let bad = GUNS.replacen(r#""loaded": ["shot"]"#, r#""loaded": []"#, 1);
    assert!(Pack::from_json(bad.as_bytes()).is_err());
}
