//! Magazines as general image data: rounds spent per shot, a reload that
//! fills from a reserve shared by every gun of the same ammo, shells loaded
//! one at a time, and a thrown gun that keeps what it had. The pack is
//! written here; it is our own.
use bri_weapons::*;
use glam::Vec3;

const A: ActorId = ActorId(1);
const B: ActorId = ActorId(2);

/// Open air: every shot flies to the end of its reach.
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

/// A 3-round pistol and a carbine sharing its ammo, and a 4-shell shotgun
/// loaded shell by shell.
const GUNS: &str = r#"{
    "schema_version": 3,
    "id": "mag",
    "items": {
        "mag:weapon/pistol": { "ui_name": "Pistol", "image": "mag:image/pistol" },
        "mag:weapon/carbine": { "ui_name": "Carbine", "image": "mag:image/carbine" },
        "mag:weapon/shotgun": { "ui_name": "Shotgun", "image": "mag:image/shotgun" }
    },
    "images": {
        "mag:image/pistol": {
            "projectile": "mag:projectile/bullet",
            "shot": { "projectiles": 1, "hitscan": { "range": 100 } },
            "magazine": { "size": 3, "ammo": "light", "reload_ticks": 24, "reserve": 7,
                          "max_reserve": 20, "reload_sound": "mag:sound/reload",
                          "empty_sound": "mag:sound/click", "display": "Light Rounds" },
            "states": [
                { "name": "Activate", "ticks": 4, "timeout": 1 },
                { "name": "Ready", "down": 2 },
                { "name": "Fire", "ticks": 6, "timeout": 3, "script": "onFire", "allow_change": false },
                { "name": "Wait", "ticks": 2, "timeout": 1 }
            ]
        },
        "mag:image/carbine": {
            "projectile": "mag:projectile/bullet",
            "shot": { "projectiles": 1, "hitscan": { "range": 100 } },
            "magazine": { "size": 10, "ammo": "light", "reload_ticks": 36, "reserve": 30,
                          "max_reserve": 30 },
            "states": [
                { "name": "Activate", "ticks": 4, "timeout": 1 },
                { "name": "Ready", "down": 2 },
                { "name": "Fire", "ticks": 6, "timeout": 3, "script": "onFire", "allow_change": false },
                { "name": "Wait", "ticks": 2, "timeout": 1 }
            ]
        },
        "mag:image/shotgun": {
            "projectile": "mag:projectile/bullet",
            "shot": { "projectiles": 1, "hitscan": { "range": 100 } },
            "magazine": { "size": 4, "ammo": "shells", "reload_ticks": 12, "one_by_one": true,
                          "reserve": 8 },
            "states": [
                { "name": "Activate", "ticks": 4, "timeout": 1 },
                { "name": "Ready", "down": 2 },
                { "name": "Fire", "ticks": 6, "timeout": 3, "script": "onFire", "allow_change": false },
                { "name": "Wait", "ticks": 2, "timeout": 1 }
            ]
        }
    },
    "projectiles": {
        "mag:projectile/bullet": { "speed": 200, "damage": 10, "damage_type": "$DamageType::Gun",
                                   "lifetime_ticks": 120 }
    },
    "damage_types": { "gun": { "name": "Gun", "suicide_message": "%1 shot themselves",
                               "murder_message": "%2 shot %1", "vehicle_scale": 1.0, "direct": true } }
}"#;

fn world() -> WeaponsWorld {
    let mut w = WeaponsWorld::new(Pack::from_json(GUNS.as_bytes()).unwrap()).unwrap();
    w.add_actor(A, 5).unwrap();
    w.add_actor(B, 5).unwrap();
    w
}

fn step(w: &mut WeaponsWorld, ticks: usize) -> Vec<Event> {
    (0..ticks).flat_map(|_| w.step(&mut Air)).collect()
}

fn holding(w: &mut WeaponsWorld, id: ActorId, item: &str) -> usize {
    let slot = w.give(id, item).unwrap();
    w.equip(id, Some(slot)).unwrap();
    step(w, 8);
    slot
}

/// Press, hold a tick, let go and play out the shot. How many shots left
/// the gun.
fn click(w: &mut WeaponsWorld) -> (usize, Vec<Event>) {
    w.trigger(A, true).unwrap();
    let mut events = step(w, 2);
    w.trigger(A, false).unwrap();
    events.extend(step(w, 14));
    let shots = events
        .iter()
        .filter(|e| matches!(e, Event::Tracer { .. }))
        .count();
    (shots, events)
}

fn sounds(events: &[Event]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Sound { profile, .. } => Some(profile.clone()),
            _ => None,
        })
        .collect()
}

fn rounds(w: &WeaponsWorld) -> (u32, Reserve, bool) {
    let view = w.ammo(A).unwrap();
    (view.rounds, view.reserve, view.reloading)
}

#[test]
fn a_magazine_empties_reloads_from_the_reserve_and_clicks_while_it_does() {
    let mut w = world();
    let events = {
        let slot = w.give(A, "mag:weapon/pistol").unwrap();
        w.equip(A, Some(slot)).unwrap();
        step(&mut w, 8)
    };
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Ammo { actor } if *actor == A)),
        "drawing it shows the display"
    );
    let view = w.ammo(A).unwrap();
    assert_eq!(
        (
            view.item.as_str(),
            view.size,
            view.ammo.as_str(),
            view.name.as_str()
        ),
        ("mag:weapon/pistol", 3, "light", "Light Rounds")
    );
    assert_eq!(
        rounds(&w),
        (3, Reserve::Rounds(7), false),
        "a new gun is full"
    );
    assert_eq!(click(&mut w).0, 1);
    assert_eq!(click(&mut w).0, 1);
    let (shots, events) = click(&mut w);
    assert_eq!(shots, 1);
    assert!(
        sounds(&events).contains(&"mag:sound/reload".to_string()),
        "the last round starts a reload"
    );
    assert!(rounds(&w).2);
    let (shots, events) = click(&mut w);
    assert_eq!(shots, 0, "nothing to shoot while it reloads");
    assert!(sounds(&events).contains(&"mag:sound/click".to_string()));
    step(&mut w, 24);
    assert_eq!(rounds(&w), (3, Reserve::Rounds(4), false));
    // The light key: not while full, then a partial reload takes only
    // what the magazine lacks.
    assert!(!w.reload(A).unwrap());
    click(&mut w);
    assert!(w.reload(A).unwrap());
    step(&mut w, 25);
    assert_eq!(rounds(&w), (3, Reserve::Rounds(3), false));
}

#[test]
fn an_empty_reserve_leaves_the_gun_empty_until_ammo_arrives() {
    let mut w = world();
    holding(&mut w, A, "mag:weapon/pistol");
    w.set_reserve(A, "light", Reserve::Rounds(0)).unwrap();
    for _ in 0..3 {
        click(&mut w);
    }
    assert_eq!(rounds(&w), (0, Reserve::Rounds(0), false));
    assert_eq!(click(&mut w).0, 0);
    // An ammo box: capped at the most a magazine of that ammo carries, and
    // an empty gun waiting on it reloads at once.
    w.give_ammo(A, "light", 500).unwrap();
    assert_eq!(rounds(&w), (0, Reserve::Rounds(30), true));
    step(&mut w, 30);
    assert_eq!(rounds(&w), (3, Reserve::Rounds(27), false));
}

#[test]
fn guns_of_one_ammo_share_a_reserve_and_keep_their_own_rounds() {
    let mut w = world();
    let pistol = holding(&mut w, A, "mag:weapon/pistol");
    click(&mut w);
    let carbine = holding(&mut w, A, "mag:weapon/carbine");
    assert_eq!(
        rounds(&w),
        (10, Reserve::Rounds(7), false),
        "the pistol's reserve already counts"
    );
    // A reload stops when the gun is put away.
    for _ in 0..10 {
        click(&mut w);
    }
    assert!(rounds(&w).2);
    w.equip(A, Some(pistol)).unwrap();
    step(&mut w, 60);
    assert_eq!(rounds(&w), (2, Reserve::Rounds(7), false));
    w.equip(A, Some(carbine)).unwrap();
    step(&mut w, 8);
    assert_eq!(rounds(&w).0, 0);
}

#[test]
fn shells_load_one_at_a_time_and_a_pull_of_the_trigger_fires_instead() {
    let mut w = world();
    holding(&mut w, A, "mag:weapon/shotgun");
    for _ in 0..3 {
        click(&mut w);
    }
    assert_eq!(rounds(&w), (1, Reserve::Rounds(8), false));
    assert!(w.reload(A).unwrap());
    step(&mut w, 12);
    assert_eq!(
        rounds(&w),
        (2, Reserve::Rounds(7), true),
        "one shell, then the next"
    );
    assert_eq!(click(&mut w).0, 1, "the pull stops the loading and fires");
    assert_eq!(rounds(&w), (1, Reserve::Rounds(7), false));
}

/// Two shells a pass (the Paired Shotgun's script loaded two): into a
/// magazine with room for one, the second is lost, as its chamber check
/// threw it away.
#[test]
fn a_load_of_two_shells_fills_and_loses_what_does_not_fit() {
    let json = GUNS.replace(
        r#""one_by_one": true,"#,
        r#""one_by_one": true, "per_load": 2,"#,
    );
    let mut w = WeaponsWorld::new(Pack::from_json(json.as_bytes()).unwrap()).unwrap();
    w.add_actor(A, 5).unwrap();
    holding(&mut w, A, "mag:weapon/shotgun");
    for _ in 0..3 {
        click(&mut w);
    }
    assert_eq!(rounds(&w), (1, Reserve::Rounds(8), false));
    assert!(w.reload(A).unwrap());
    step(&mut w, 12);
    assert_eq!(rounds(&w), (3, Reserve::Rounds(6), true));
    step(&mut w, 12);
    assert_eq!(rounds(&w), (4, Reserve::Rounds(4), false), "one shell lost");
}

#[test]
fn endless_reserve_never_runs_out_and_a_new_life_refills() {
    let mut w = world();
    holding(&mut w, A, "mag:weapon/pistol");
    w.set_reserve(A, "light", Reserve::Endless).unwrap();
    w.give_ammo(A, "light", 5).unwrap();
    for _ in 0..3 {
        click(&mut w);
    }
    step(&mut w, 24);
    assert_eq!(rounds(&w), (3, Reserve::Endless, false));
    click(&mut w);
    w.reset_ammo(A).unwrap();
    let slot = w.give(A, "mag:weapon/carbine").unwrap();
    w.equip(A, Some(slot)).unwrap();
    step(&mut w, 8);
    assert_eq!(rounds(&w), (10, Reserve::Rounds(30), false));
}

#[test]
fn a_thrown_gun_keeps_its_rounds_for_whoever_picks_it_up() {
    let mut w = world();
    let slot = holding(&mut w, A, "mag:weapon/pistol");
    click(&mut w);
    click(&mut w);
    let drop = w.drop_item(A, slot).unwrap();
    assert_eq!(w.drops().find(|d| d.id == drop).unwrap().rounds, Some(1));
    let slot = w.pickup(B, drop).unwrap();
    w.equip(B, Some(slot)).unwrap();
    step(&mut w, 8);
    assert_eq!(w.ammo(B).unwrap().rounds, 1);
    assert_eq!(
        w.ammo(B).unwrap().reserve,
        Reserve::Rounds(7),
        "their own reserve"
    );
}

#[test]
fn two_of_one_gun_each_keep_their_own_magazine() {
    // Tier+Tactical's `%obj.toolAmmo[%slot]`: one per slot, not per gun.
    let mut w = world();
    let first = holding(&mut w, A, "mag:weapon/pistol");
    let second = w.give(A, "mag:weapon/pistol").unwrap();
    click(&mut w);
    click(&mut w);
    assert_eq!(rounds(&w).0, 1);
    w.equip(A, Some(second)).unwrap();
    step(&mut w, 8);
    assert_eq!(rounds(&w).0, 3, "the other copy is still full");
    click(&mut w);
    assert_eq!(rounds(&w).0, 2);
    w.equip(A, Some(first)).unwrap();
    step(&mut w, 8);
    assert_eq!(rounds(&w).0, 1, "the first copy kept what it had");
    // Each thrown copy takes its own magazine with it; a gun picked into
    // a slot it never filled before comes with what it had.
    let drop = w.drop_item(A, second).unwrap();
    assert_eq!(w.drops().find(|d| d.id == drop).unwrap().rounds, Some(2));
    assert_eq!(rounds(&w).0, 1, "the one in hand is untouched");
    let drop = w.drop_item(A, first).unwrap();
    assert_eq!(w.drops().find(|d| d.id == drop).unwrap().rounds, Some(1));
    // A new gun in an emptied slot is full, not the old one's leftovers.
    let slot = holding(&mut w, A, "mag:weapon/pistol");
    assert_eq!(slot, first);
    assert_eq!(rounds(&w).0, 3);
}

#[test]
fn a_remounting_gun_drawn_from_another_slot_comes_out_afresh() {
    // Tier's Remount Duplicate Items: drawing the other copy puts the gun
    // away and draws it again; without it the gun stays up mid-state.
    let state = |w: &WeaponsWorld| w.image_state(A, 0).unwrap().1.name.clone();
    let mut w = world();
    holding(&mut w, A, "mag:weapon/pistol");
    let second = w.give(A, "mag:weapon/pistol").unwrap();
    click(&mut w);
    w.equip(A, Some(second)).unwrap();
    assert_eq!(state(&w), "Ready", "stays up");
    assert_eq!(rounds(&w).0, 3, "with the other copy's rounds");

    let guns = GUNS.replace(
        "\"display\": \"Light Rounds\"",
        "\"display\": \"Light Rounds\", \"remount\": true",
    );
    let mut w = WeaponsWorld::new(Pack::from_json(guns.as_bytes()).unwrap()).unwrap();
    w.add_actor(A, 5).unwrap();
    let first = holding(&mut w, A, "mag:weapon/pistol");
    let second = w.give(A, "mag:weapon/pistol").unwrap();
    click(&mut w);
    w.equip(A, Some(second)).unwrap();
    assert_eq!(state(&w), "Activate", "drawn afresh");
    step(&mut w, 8);
    assert_eq!(rounds(&w).0, 3);
    // Drawing the same slot again does not.
    w.equip(A, Some(second)).unwrap();
    assert_eq!(state(&w), "Ready");
    w.equip(A, Some(first)).unwrap();
    step(&mut w, 8);
    assert_eq!(rounds(&w).0, 2, "each copy keeps its own");
}

#[test]
fn magazines_and_ammo_commands_are_checked() {
    for (from, to) in [
        ("\"size\": 3,", "\"size\": 0,"),
        ("\"size\": 3,", "\"size\": 1001,"),
        (
            "\"ammo\": \"light\", \"reload_ticks\": 24",
            "\"ammo\": \"\", \"reload_ticks\": 24",
        ),
        (
            "\"ammo\": \"light\", \"reload_ticks\": 24",
            "\"ammo\": \"a b\", \"reload_ticks\": 24",
        ),
        ("\"reload_ticks\": 24", "\"reload_ticks\": 0"),
        (
            "\"reload_ticks\": 24",
            "\"reload_ticks\": 24, \"per_shot\": 4",
        ),
        ("\"reserve\": 7,", "\"reserve\": 100001,"),
    ] {
        let json = GUNS.replacen(from, to, 1);
        assert_ne!(json, GUNS, "{from}");
        assert!(Pack::from_json(json.as_bytes()).is_err(), "{to}");
    }
    let mut w = world();
    holding(&mut w, A, "mag:weapon/pistol");
    assert!(w.set_reserve(A, "a b", Reserve::Rounds(1)).is_err());
    assert!(w.set_rounds(A, "mag:weapon/none", 1).is_err());
    w.set_rounds(A, "mag:weapon/pistol", 99).unwrap();
    assert_eq!(rounds(&w).0, 3, "up to the magazine's size");
}

#[test]
fn next_equipment_ammo_projects_initialization_without_mutating_any_actor() {
    let mut w = world();
    let pistol = w.give(A, "mag:weapon/pistol").unwrap();
    let carbine = w.give(A, "mag:weapon/carbine").unwrap();
    let before = format!("{:?}", w.actor(A).unwrap());
    let p = w.ammo_on_equip(A, pistol).unwrap();
    let c = w.ammo_on_equip(A, carbine).unwrap();
    assert_eq!((p.rounds, p.reserve), (3, Reserve::Rounds(7)));
    assert_eq!((c.rounds, c.reserve), (10, Reserve::Rounds(30)));
    assert_eq!(format!("{:?}", w.actor(A).unwrap()), before);
    assert!(w.reserve(A, "light").is_none());
    assert!(w.ammo_on_equip(ActorId(999), 0).is_none());
    assert!(w.ammo_on_equip(A, 999).is_none());
    assert!(w.ammo_on_equip(A, 4).is_none());
    w.equip(A, Some(pistol)).unwrap();
    step(&mut w, 8);
    assert_eq!(w.ammo(A), w.ammo_on_equip(A, pistol));
    w.set_reserve(A, "light", Reserve::Rounds(0)).unwrap();
    for _ in 0..3 {
        assert_eq!(click(&mut w).0, 1);
    }
    assert_eq!(w.ammo_on_equip(A, pistol).unwrap().rounds, 0);
    let c = w.ammo_on_equip(A, carbine).unwrap();
    assert_eq!((c.rounds, c.reserve), (10, Reserve::Rounds(0)));
    w.equip(A, Some(carbine)).unwrap();
    step(&mut w, 8);
    assert_eq!(w.ammo(A), w.ammo_on_equip(A, carbine));
    assert_eq!(w.ammo_on_equip(A, pistol).unwrap().rounds, 0);
    assert_eq!(w.reserve(A, "light"), Some(Reserve::Rounds(0)));
}

#[test]
fn next_equipment_ammo_preserves_shared_counted_and_unlimited_supply() {
    for (supply, from_reserve) in [
        (Supply::Counted, false),
        (Supply::Reserve, true),
        (Supply::Unlimited, false),
    ] {
        let mut pack = Pack::from_json(GUNS.as_bytes()).unwrap();
        let m = pack
            .images
            .get_mut("mag:image/pistol")
            .unwrap()
            .magazine
            .as_mut()
            .unwrap();
        m.supply = supply;
        m.from_reserve = from_reserve;
        let mut w = WeaponsWorld::new(pack).unwrap();
        w.add_actor(A, 5).unwrap();
        let slot = w.give(A, "mag:weapon/pistol").unwrap();
        assert_eq!(
            w.ammo_on_equip(A, slot).unwrap().rounds,
            if supply == Supply::Unlimited { 3 } else { 7 }
        );
        w.set_reserve(A, "light", Reserve::Rounds(0)).unwrap();
        let before = format!("{:?}", w.actor(A).unwrap());
        assert_eq!(
            w.ammo_on_equip(A, slot).unwrap().rounds,
            if supply == Supply::Unlimited { 3 } else { 0 }
        );
        assert_eq!(format!("{:?}", w.actor(A).unwrap()), before);
        w.equip(A, Some(slot)).unwrap();
        step(&mut w, 8);
        assert_eq!(w.ammo(A), w.ammo_on_equip(A, slot));
        if from_reserve {
            assert!(
                w.image_state(A, 0).is_none(),
                "empty counted throw is stowed"
            );
            assert_eq!(w.actor(A).unwrap().selected, Some(slot));
        }
        w.set_reserve(A, "light", Reserve::Endless).unwrap();
        assert_eq!(
            w.ammo_on_equip(A, slot).unwrap().rounds,
            if supply == Supply::Unlimited {
                3
            } else {
                100_000
            }
        );
    }
}

#[test]
fn held_ammo_keeps_a_custom_mount_while_slot_projection_uses_the_inventory() {
    let mut w = world();
    let slot = holding(&mut w, A, "mag:weapon/pistol");
    w.swap_image(A, Some("mag:image/shotgun")).unwrap();
    step(&mut w, 8);
    assert_eq!(w.ammo(A).unwrap().ammo, "shells");
    assert_eq!(w.ammo_on_equip(A, slot).unwrap().ammo, "light");
}
