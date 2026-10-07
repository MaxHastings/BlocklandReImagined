//! Pack fields server settings decide ([`Binding`]) and the magazine
//! supplies one of them picks ([`Supply`]): Tier+Tactical's ammo systems,
//! display and recoil preferences, as general data. The pack is written
//! here; it is our own.
use bri_weapons::*;
use glam::Vec3;
use std::collections::BTreeMap;

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

/// A 3-round pistol with a kick, its supply, display time and kick bound
/// to settings.
const GUN: &str = r#"{
    "schema_version": 4,
    "id": "set",
    "items": {
        "set:weapon/pistol": { "ui_name": "Pistol", "image": "set:image/pistol" }
    },
    "images": {
        "set:image/pistol": {
            "projectile": "set:projectile/bullet",
            "shot": { "projectiles": 1, "hitscan": { "range": 100 },
                      "kick": { "amplitude": 0.2, "frequency": 4, "seconds": 0.2 } },
            "magazine": { "size": 3, "ammo": "light", "reload_ticks": 24, "reserve": 4,
                          "display_ticks": 480 },
            "states": [
                { "name": "Activate", "ticks": 4, "timeout": 1 },
                { "name": "Ready", "down": 2 },
                { "name": "Fire", "ticks": 6, "timeout": 3, "script": "onFire", "allow_change": false },
                { "name": "Wait", "ticks": 2, "timeout": 1 }
            ]
        }
    },
    "projectiles": {
        "set:projectile/bullet": { "speed": 200, "damage": 10, "damage_type": "$DamageType::Gun",
                                   "lifetime_ticks": 120 }
    },
    "damage_types": { "gun": { "name": "Gun", "suicide_message": "%1 shot themselves",
                               "murder_message": "%2 shot %1", "vehicle_scale": 1.0, "direct": true } },
    "bindings": [
        { "setting": "$Pref::Server::Test::Ammo",
          "field": ["images", "set:image/pistol", "magazine", "supply"],
          "values": { "0": "reserve", "1": "endless", "2": "unlimited", "3": "counted" } },
        { "setting": "$Pref::Server::Test::Ammo",
          "field": ["images", "set:image/pistol", "magazine", "supply"],
          "values": { "2": "endless", "3": "both" },
          "when": { "$Pref::Server::Test::AlwaysReload": "true" } },
        { "setting": "test-rules:display_time",
          "field": ["images", "set:image/pistol", "magazine", "display_ticks"],
          "scale": 120 },
        { "setting": "$Pref::Server::Test::Recoil",
          "field": ["images", "set:image/pistol", "shot", "kick"],
          "values": { "false": null } }
    ]
}"#;

fn pack() -> Pack {
    Pack::from_json(GUN.as_bytes()).unwrap()
}

fn with(values: &[(&str, &str)]) -> Result<Pack, anyhow::Error> {
    let values: BTreeMap<String, String> = values
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    pack().with_settings(|name| values.get(name).cloned())
}

fn pistol(pack: &Pack) -> &Image {
    &pack.images["set:image/pistol"]
}

#[test]
fn settings_set_their_bound_fields_and_leave_the_rest_as_authored() {
    let authored = pack();
    assert_eq!(
        authored.bound_settings().into_iter().collect::<Vec<_>>(),
        [
            "$Pref::Server::Test::AlwaysReload",
            "$Pref::Server::Test::Ammo",
            "$Pref::Server::Test::Recoil",
            "test-rules:display_time"
        ]
    );
    // No running Add-On declares them: as authored.
    let none = with(&[]).unwrap();
    assert_eq!(
        serde_json::to_value(&none).unwrap(),
        serde_json::to_value(&authored).unwrap()
    );

    let set = with(&[
        ("$Pref::Server::Test::Ammo", "1"),
        ("test-rules:display_time", "2"),
        ("$Pref::Server::Test::Recoil", "false"),
    ])
    .unwrap();
    let magazine = pistol(&set).magazine.clone().unwrap();
    assert_eq!(magazine.supply, Supply::Endless);
    assert_eq!(magazine.display_ticks, 240, "2 seconds");
    assert!(
        pistol(&set).shot.as_ref().unwrap().kick.is_none(),
        "kick off"
    );
    // Recoil on is not listed: the authored kick stays.
    let on = with(&[("$Pref::Server::Test::Recoil", "true")]).unwrap();
    assert!(pistol(&on).shot.as_ref().unwrap().kick.is_some());

    // A later binding of the field wins while its `when` holds.
    let arena = |always: &str| {
        with(&[
            ("$Pref::Server::Test::Ammo", "3"),
            ("$Pref::Server::Test::AlwaysReload", always),
        ])
        .unwrap()
    };
    assert_eq!(
        pistol(&arena("false")).magazine.as_ref().unwrap().supply,
        Supply::Counted
    );
    assert_eq!(
        pistol(&arena("true")).magazine.as_ref().unwrap().supply,
        Supply::Both
    );
}

#[test]
fn a_value_out_of_a_fields_range_is_refused() {
    // 61 seconds is past the display's 7200 ticks.
    let error = with(&[("test-rules:display_time", "61")]).unwrap_err();
    assert!(
        format!("{error:#}").contains("magazine display"),
        "{error:#}"
    );
    let error = with(&[("test-rules:display_time", "soon")]).unwrap_err();
    assert!(format!("{error:#}").contains("not a number"), "{error:#}");
}

#[test]
fn bindings_reach_only_the_packs_own_fields() {
    let bad = |field: &str| {
        let json = GUN.replace(
            r#""field": ["images", "set:image/pistol", "shot", "kick"]"#,
            field,
        );
        Pack::from_json(json.as_bytes()).unwrap_err().to_string()
    };
    assert!(
        bad(r#""field": ["images", "set:image/rifle", "shot", "kick"]"#)
            .contains("does not declare")
    );
    assert!(bad(r#""field": ["images", "set:image/pistol", "states"]"#).contains("states"));
    assert!(bad(r#""field": ["sounds", "x", "file"]"#).contains("kind"));
}

#[test]
fn a_merge_keeps_each_parts_bindings_and_drops_those_of_what_it_dropped() {
    let mut other = pack();
    other.id = "other".into();
    other.items.clear();
    other.images.clear();
    other.projectiles.clear();
    other.bindings = vec![Binding {
        setting: "x:y".into(),
        field: vec!["images".into(), "other:image/gone".into(), "shot".into()],
        values: BTreeMap::from([("1".into(), serde_json::Value::Null)]),
        scale: None,
        when: BTreeMap::new(),
    }];
    let (merged, _) = pack().merge(vec![("other/assets".into(), other)]);
    merged.validate().unwrap();
    assert_eq!(merged.bindings.len(), pack().bindings.len());
}

fn world(pack: Pack) -> WeaponsWorld {
    let mut w = WeaponsWorld::new(pack).unwrap();
    w.add_actor(A, 5).unwrap();
    let slot = w.give(A, "set:weapon/pistol").unwrap();
    w.equip(A, Some(slot)).unwrap();
    step(&mut w, 8);
    w
}

fn step(w: &mut WeaponsWorld, ticks: usize) -> Vec<Event> {
    (0..ticks).flat_map(|_| w.step(&mut Air)).collect()
}

/// Press and play out one shot; whether it fired.
fn shoot(w: &mut WeaponsWorld) -> bool {
    w.trigger(A, true).unwrap();
    let mut events = step(w, 2);
    w.trigger(A, false).unwrap();
    events.extend(step(w, 14));
    events.iter().any(|e| matches!(e, Event::Tracer { .. }))
}

fn ammo(w: &WeaponsWorld) -> (u32, Reserve) {
    let v = w.ammo(A).unwrap();
    (v.rounds, v.reserve)
}

fn supplied(supply: &str) -> WeaponsWorld {
    world(with(&[("$Pref::Server::Test::Ammo", supply)]).unwrap())
}

#[test]
fn an_endless_magazine_reloads_from_nothing() {
    let mut w = supplied("1");
    w.set_reserve(A, "light", Reserve::Rounds(0)).unwrap();
    for _ in 0..3 {
        assert!(shoot(&mut w));
    }
    assert!(!shoot(&mut w), "empty: it clicks and reloads");
    step(&mut w, 30);
    assert_eq!(
        ammo(&w),
        (3, Reserve::Rounds(0)),
        "full, the reserve untouched"
    );
}

#[test]
fn an_unlimited_magazine_never_runs_out_or_shows() {
    let mut w = supplied("2");
    for _ in 0..10 {
        assert!(shoot(&mut w));
    }
    assert_eq!(ammo(&w), (3, Reserve::Rounds(4)));
    assert!(!w.ammo(A).unwrap().shown);
}

#[test]
fn a_counted_gun_shoots_its_reserve_and_never_reloads() {
    let mut w = supplied("3");
    assert!(w.ammo(A).unwrap().counted);
    for left in (0..4).rev() {
        assert!(shoot(&mut w));
        assert_eq!(ammo(&w).1, Reserve::Rounds(left));
    }
    assert!(!shoot(&mut w), "out of reserve, it clicks");
    assert!(!w.reload(A).unwrap());
}

#[test]
fn a_gun_that_must_reload_under_arena_spends_both_and_reloads_free() {
    let mut w = world(
        with(&[
            ("$Pref::Server::Test::Ammo", "3"),
            ("$Pref::Server::Test::AlwaysReload", "true"),
        ])
        .unwrap(),
    );
    for _ in 0..3 {
        assert!(shoot(&mut w));
    }
    assert_eq!(ammo(&w), (0, Reserve::Rounds(1)));
    // Empty: the click reloads, filling all 3 from nothing.
    assert!(!shoot(&mut w));
    step(&mut w, 30);
    assert_eq!(ammo(&w), (3, Reserve::Rounds(1)));
    assert!(shoot(&mut w));
    assert_eq!(ammo(&w), (2, Reserve::Rounds(0)));
    // No reserve left: no shot, though the magazine holds two.
    assert!(!shoot(&mut w));
}

#[test]
fn a_retuned_world_plays_the_new_fields_and_keeps_what_is_held() {
    let mut w = world(pack());
    assert!(shoot(&mut w));
    assert_eq!(ammo(&w), (2, Reserve::Rounds(4)));
    w.retune(with(&[("$Pref::Server::Test::Ammo", "2")]).unwrap())
        .unwrap();
    assert_eq!(ammo(&w), (3, Reserve::Rounds(4)), "unlimited reads full");
    assert!(shoot(&mut w));
    w.retune(pack()).unwrap();
    assert_eq!(ammo(&w), (2, Reserve::Rounds(4)), "its rounds were kept");
    // Not another set of weapons.
    let mut fewer = pack();
    fewer.bindings.clear();
    fewer.items.clear();
    fewer.images.clear();
    assert!(w.retune(fewer).is_err());
}
