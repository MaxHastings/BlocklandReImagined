//! Raycasting guns with Torque's loaded states, as Tier+Tactical builds
//! them: a magazine that keeps `loaded` and `ammo` for the image's states,
//! rounds that arrive as the image's own reload state runs, a ray that
//! lands its projectile with another's explosion, a sound by what it hit
//! and a tracer round flown to it, a range
//! and spread that shrink on the move, and a bullet that slows whoever it
//! hits.
//! The pack is written here; it is our own.
use bri_weapons::*;
use glam::Vec3;

const A: ActorId = ActorId(1);
const B: ActorId = ActorId(2);

/// Open air, or a body (`B`) 20 units ahead down -Z.
struct World {
    body: bool,
}
impl Query for World {
    fn sweep(&mut self, from: Vec3, to: Vec3, _: Filter) -> Option<Hit> {
        let at = Vec3::new(0.0, from.y, -20.0);
        (self.body && to.z < -20.0).then(|| Hit {
            target: TargetId::Actor(B),
            position: at,
            normal: Vec3::Z,
            fraction: (from - at).length() / (from - to).length(),
            color: None,
        })
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

/// A 3-round pistol whose states follow `loaded` (fire, empty, the light
/// key's reload) the way Tier+Tactical's do, and whose ray lands a
/// projectile carrying its damage. Its reload is three states, 30 ticks, the rounds arriving as
/// `Reloaded` (`onReloaded`) is entered.
const GUNS: &str = r#"{
    "schema_version": 3,
    "id": "ray",
    "items": {
        "ray:weapon/pistol": { "ui_name": "Pistol", "image": "ray:image/pistol" },
        "ray:weapon/smg": { "ui_name": "SMG", "image": "ray:image/smg" }
    },
    "images": {
        "ray:image/pistol": {
            "projectile": "ray:projectile/pistolray",
            "shot": {
                "projectiles": 1,
                "spread": 0.0,
                "moving_spread": 0.0,
                "moving_speed": 0.1,
                "hitscan": {
                    "range": 200,
                    "moving_range": 85,
                    "explosion": "SparkProjectile",
                    "flown": "ray:projectile/tracer",
                    "player_sound": "ray:sound/flesh",
                    "other_sound": "ray:sound/ricochet"
                }
            },
            "magazine": { "size": 3, "ammo": "nine", "reload_ticks": 600, "reserve": 4,
                          "reload_state": "onReloaded" },
            "states": [
                { "name": "Activate", "ticks": 4, "timeout": 7 },
                { "name": "Ready", "not_loaded": 9, "down": 2 },
                { "name": "FireCheck", "loaded": 3, "not_loaded": 11 },
                { "name": "Fire", "ticks": 6, "timeout": 4, "script": "onFire", "allow_change": false },
                { "name": "Smoke", "up": 5 },
                { "name": "Wait", "ticks": 1, "timeout": 7 },
                { "name": "Spare" },
                { "name": "LoadCheck", "loaded": 1, "not_loaded": 10 },
                { "name": "ReloadWait", "ticks": 10, "timeout": 12 },
                { "name": "ManualReload", "ammo": 8, "no_ammo": 12 },
                { "name": "Empty", "loaded": 1, "ammo": 8, "down": 2 },
                { "name": "EmptyFire", "ammo": 8, "up": 10 },
                { "name": "ReloadStart", "ticks": 10, "timeout": 13 },
                { "name": "Reloaded", "ticks": 10, "timeout": 1, "script": "onReloaded" }
            ]
        },
        "ray:image/smg": {
            "projectile": "ray:projectile/bullet",
            "shot": { "projectiles": 1, "hitscan": { "range": 200 } },
            "states": [
                { "name": "Activate", "ticks": 4, "timeout": 1 },
                { "name": "Ready", "down": 2 },
                { "name": "Fire", "ticks": 6, "timeout": 3, "script": "onFire" },
                { "name": "Wait", "up": 1 }
            ]
        }
    },
    "projectiles": {
        "ray:projectile/tracer": { "speed": 200, "lifetime_ticks": 120 },
        "ray:projectile/pistolray": { "speed": 200, "lifetime_ticks": 120, "damage": 12,
                                      "damage_type": "$DamageType::Pistol", "impulse": 100,
                                      "vertical": 50, "collide_players": true },
        "ray:projectile/bullet": { "speed": 200, "lifetime_ticks": 120, "damage": 5,
                                   "collide_players": true, "slow": { "divisor": 2 } },
        "ray:projectile/spark": { "name": "SparkProjectile", "speed": 1, "lifetime_ticks": 1,
                                  "explosion": { "effect": "sparkexplosion" } }
    },
    "damage_types": { "pistol": { "name": "Pistol", "suicide_message": "%1 shot themselves",
                                  "murder_message": "%2 shot %1", "vehicle_scale": 1.0,
                                  "direct": true } }
}"#;

fn world() -> WeaponsWorld {
    let mut w = WeaponsWorld::new(Pack::from_json(GUNS.as_bytes()).unwrap()).unwrap();
    w.add_actor(A, 5).unwrap();
    w.add_actor(B, 5).unwrap();
    let slot = w.give(A, "ray:weapon/pistol").unwrap();
    w.equip(A, Some(slot)).unwrap();
    step(&mut w, 8, false);
    w
}

fn step(w: &mut WeaponsWorld, ticks: usize, body: bool) -> Vec<Event> {
    (0..ticks)
        .flat_map(|_| w.step(&mut World { body }))
        .collect()
}

/// Press, hold two ticks, let go and play the shot out.
fn click(w: &mut WeaponsWorld, body: bool) -> Vec<Event> {
    w.trigger(A, true).unwrap();
    let mut events = step(w, 2, body);
    w.trigger(A, false).unwrap();
    events.extend(step(w, 14, body));
    events
}

fn shots(events: &[Event]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, Event::Tracer { .. }))
        .count()
}

fn state(w: &WeaponsWorld) -> String {
    w.image_state(A, 0).unwrap().1.name.clone()
}

fn rounds(w: &WeaponsWorld) -> u32 {
    w.ammo(A).unwrap().rounds
}

#[test]
fn loaded_states_fire_every_round_then_reload_with_the_images_own_states() {
    let mut w = world();
    assert_eq!(state(&w), "Ready");
    // Every round fires: FireCheck goes on `loaded`, which a plain ammo
    // flag would have left waiting there for ever.
    for left in [2, 1] {
        assert_eq!(shots(&click(&mut w, false)), 1);
        assert_eq!(rounds(&w), left);
        assert_eq!(state(&w), "Ready");
    }
    // The last round empties it; the reload starts and the image goes
    // through its reload states, not loaded until the rounds arrive.
    assert_eq!(shots(&click(&mut w, false)), 1);
    assert_eq!(rounds(&w), 0);
    assert!(w.ammo(A).unwrap().reloading);
    let mut seen = vec![];
    let mut arrived_in = None;
    for _ in 0..40 {
        step(&mut w, 1, false);
        let now = state(&w);
        if seen.last() != Some(&now) {
            seen.push(now.clone());
        }
        if arrived_in.is_none() && rounds(&w) == 3 {
            arrived_in = Some(now);
        }
    }
    assert!(
        seen.starts_with(&["Empty".into(), "ReloadWait".into()])
            || seen.starts_with(&["ReloadWait".into()]),
        "{seen:?}"
    );
    // Long before `reload_ticks` (600) were up: when Reloaded ran.
    assert_eq!(arrived_in.as_deref(), Some("Reloaded"), "{seen:?}");
    assert_eq!(w.ammo(A).unwrap().reserve, Reserve::Rounds(1));
    assert_eq!(state(&w), "Ready");
    assert_eq!(shots(&click(&mut w, false)), 1);
}

#[test]
fn the_light_key_reloads_through_not_loaded_and_an_empty_gun_without_reserve_clicks() {
    let mut w = world();
    click(&mut w, false);
    assert!(w.reload(A).unwrap());
    // Ready goes on `not_loaded` to the manual reload, then on `ammo`.
    step(&mut w, 1, false);
    assert!(
        ["ManualReload", "ReloadWait"].contains(&state(&w).as_str()),
        "{}",
        state(&w)
    );
    step(&mut w, 40, false);
    assert_eq!(
        (rounds(&w), w.ammo(A).unwrap().reserve),
        (3, Reserve::Rounds(3))
    );
    // Spend everything: three magazines' worth leaves it empty with none
    // to reload, and the trigger only clicks.
    for _ in 0..3 {
        for _ in 0..3 {
            click(&mut w, false);
        }
        step(&mut w, 40, false);
    }
    assert_eq!(w.ammo(A).unwrap().reserve, Reserve::Rounds(0));
    let left = rounds(&w);
    for _ in 0..left {
        click(&mut w, false);
    }
    assert_eq!(rounds(&w), 0);
    assert_eq!(shots(&click(&mut w, false)), 0);
    assert_eq!(state(&w), "Empty");
    // Ammo arrives: the empty gun reloads on `ammo` and fires again.
    w.give_ammo(A, "nine", 3).unwrap();
    step(&mut w, 40, false);
    assert_eq!(rounds(&w), 3);
    assert_eq!(shots(&click(&mut w, false)), 1);
}

#[test]
fn a_ray_lands_its_damage_and_push_with_a_named_explosion_sound_and_tracer() {
    let mut w = world();
    let events = click(&mut w, true);
    let damage: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Damage {
                target,
                amount,
                kind,
                ..
            } => Some((*target, *amount, kind.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        damage,
        [(TargetId::Actor(B), 12.0, "$DamageType::Pistol".to_string())]
    );
    let push = events.iter().find_map(|e| match e {
        Event::Impulse { impulse, .. } => Some(*impulse),
        _ => None,
    });
    assert_eq!(push, Some(Vec3::new(0.0, 50.0, -100.0)));
    // The explosion projectile named by its datablock name explodes there.
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Effect { definition, position, .. }
            if definition == "sparkexplosion" && position.z == -20.0
    )));
    let sounds: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            Event::Sound { profile, .. } => Some(profile.as_str()),
            _ => None,
        })
        .collect();
    assert!(sounds.contains(&"ray:sound/flesh"), "{sounds:?}");
    assert!(!sounds.contains(&"ray:sound/ricochet"));
    // The tracer round flies from the muzzle to the hit.
    let tracer = events.iter().find_map(|e| match e {
        Event::Spawned {
            definition,
            velocity,
            ..
        } if definition == "ray:projectile/tracer" => Some(*velocity),
        _ => None,
    });
    let tracer = tracer.expect("the tracer projectile flies");
    assert!((tracer.length() - 200.0).abs() < 0.01);
    assert!(tracer.z < 0.0);
}

#[test]
fn a_moving_shooter_reaches_less() {
    let mut w = world();
    let still = click(&mut w, false);
    let reach = |events: &[Event]| {
        events.iter().find_map(|e| match e {
            Event::Tracer { to, .. } => Some(to.length()),
            _ => None,
        })
    };
    assert_eq!(reach(&still), Some(200.0));
    w.set_frame(
        A,
        Frame {
            velocity: Vec3::new(5.0, 0.0, 0.0),
            ..Frame::default()
        },
    )
    .unwrap();
    let moving = click(&mut w, false);
    assert_eq!(reach(&moving), Some(85.0));
}

#[test]
fn a_bullet_slows_the_player_it_hits_not_its_shooter() {
    let mut w = world();
    let slot = w.give(A, "ray:weapon/smg").unwrap();
    w.equip(A, Some(slot)).unwrap();
    step(&mut w, 8, false);
    let slows = |events: &[Event]| -> Vec<(ActorId, Slow)> {
        events
            .iter()
            .filter_map(|e| match e {
                Event::Slow { actor, slow } => Some((*actor, *slow)),
                _ => None,
            })
            .collect()
    };
    assert_eq!(slows(&click(&mut w, false)), []);
    assert_eq!(slows(&click(&mut w, true)), [(B, Slow { divisor: 2.0 })]);
}

#[test]
fn tier_slowdown_falls_from_halfway_to_its_floor() {
    let slow = Slow { divisor: 2.0 };
    // Floor 1 / (3 * 2) = 1/6; the first shot goes halfway to it.
    let first = slow.after_hit(None);
    assert!((first - (1.0 + 1.0 / 6.0) / 2.0).abs() < 1e-6);
    let second = slow.after_hit(Some(first));
    assert!((second - first / 2.0).abs() < 1e-6);
    assert!((slow.after_hit(Some(0.2)) - 1.0 / 6.0).abs() < 1e-6);
    assert_eq!(slow.after_hit(Some(0.1)), 0.1);
}

#[test]
fn bad_hitscans_and_slowdowns_are_refused() {
    let long = format!(r#""explosion": "{}","#, "x".repeat(129));
    for (good, bad) in [
        (r#""explosion": "SparkProjectile","#, long.as_str()),
        (r#""divisor": 2"#, r#""divisor": 0.5"#),
        (r#""moving_range": 85"#, r#""moving_range": 5000"#),
        (r#""not_loaded": 9"#, r#""not_loaded": 99"#),
    ] {
        assert!(GUNS.contains(good), "{good}");
        let pack = GUNS.replacen(good, bad, 1);
        assert!(
            Pack::from_json(pack.as_bytes()).is_err(),
            "{bad} was accepted"
        );
    }
}

/// A knife as Kai's melee packs script it: each swing draws one of its
/// pairs of hit sounds (`getRandom(0, 1)` before `Parent::onFire`), and its
/// stab, a fire state of its own, deals less than its slash (each set the
/// damage field first).
const KNIFE: &str = r#"{
    "schema_version": 3,
    "id": "k",
    "items": { "k:weapon/knife": { "ui_name": "Knife", "image": "k:image/knife" } },
    "images": {
        "k:image/knife": {
            "projectile": "k:projectile/ray",
            "shot": { "projectiles": 1, "hitscan": {
                "range": 30,
                "player_sound": "k:sound/flesh",
                "other_sound": "k:sound/clang",
                "sounds": [ { "player": "k:sound/cut" }, { "other": "k:sound/scrape" } ],
                "damage": 100
            } },
            "state_shots": { "onstab": { "projectiles": 1, "hitscan": {
                "range": 30, "player_sound": "k:sound/flesh", "damage": 55
            } } },
            "states": [
                { "name": "Activate", "ticks": 4, "timeout": 1 },
                { "name": "Ready", "down": 2 },
                { "name": "Charge", "ticks": 20, "wait": false, "timeout": 4, "up": 3 },
                { "name": "Stab", "ticks": 6, "timeout": 1, "script": "onStab" },
                { "name": "Armed", "up": 5 },
                { "name": "Slash", "ticks": 6, "timeout": 1, "script": "onFire" }
            ]
        }
    },
    "projectiles": {
        "k:projectile/ray": { "speed": 200, "lifetime_ticks": 120, "damage": 10,
                              "collide_players": true }
    }
}"#;

/// Each hit of a press held `hold` ticks: its damage and landing sound.
fn swing(w: &mut WeaponsWorld, hold: usize) -> Vec<(f32, String)> {
    w.trigger(A, true).unwrap();
    let mut events = step(w, hold, true);
    w.trigger(A, false).unwrap();
    events.extend(step(w, 12, true));
    let damage = events.iter().filter_map(|e| match e {
        Event::Damage { amount, .. } => Some(*amount),
        _ => None,
    });
    let sound = events.iter().filter_map(|e| match e {
        Event::Sound { profile, .. } => Some(profile.clone()),
        _ => None,
    });
    damage.zip(sound).collect()
}

#[test]
fn a_swing_draws_one_pair_of_hit_sounds_and_each_fire_state_its_damage() {
    let mut w = WeaponsWorld::new(Pack::from_json(KNIFE.as_bytes()).unwrap()).unwrap();
    w.add_actor(A, 5).unwrap();
    w.add_actor(B, 5).unwrap();
    let slot = w.give(A, "k:weapon/knife").unwrap();
    w.equip(A, Some(slot)).unwrap();
    step(&mut w, 8, false);
    let stab = swing(&mut w, 2);
    assert_eq!(stab, [(55.0, "k:sound/flesh".to_string())]);
    let mut slashes = vec![];
    for _ in 0..24 {
        slashes.extend(swing(&mut w, 26));
    }
    assert_eq!(slashes.len(), 24);
    assert!(slashes.iter().all(|(d, _)| *d == 100.0));
    // A pair that names only the other side keeps the player sound.
    let heard = |s: &str| slashes.iter().filter(|(_, p)| p == s).count();
    assert!(heard("k:sound/cut") > 0 && heard("k:sound/flesh") > 0);
    assert_eq!(heard("k:sound/cut") + heard("k:sound/flesh"), 24);
}

#[test]
fn hit_sound_pairs_pick_by_the_draw_and_keep_what_they_leave_out() {
    let h: Hitscan = serde_json::from_str(
        r#"{ "range": 4, "player_sound": "p", "other_sound": "o",
             "sounds": [ { "other": "a" }, { "player": "b", "other": "" } ] }"#,
    )
    .unwrap();
    assert_eq!((h.sound(true, 0.0), h.sound(false, 0.0)), ("p", "a"));
    assert_eq!((h.sound(true, 0.99), h.sound(false, 0.99)), ("b", ""));
    assert_eq!(h.sound(false, 1.0), "");
    // At most 8 pairs, damage within ±100.
    for bad in [
        KNIFE.replacen(r#""damage": 100"#, r#""damage": 105"#, 1),
        KNIFE.replacen(
            r#"{ "other": "k:sound/scrape" }"#,
            &[r#"{ "other": "k:sound/scrape" }"#; 9].join(", "),
            1,
        ),
    ] {
        assert!(Pack::from_json(bad.as_bytes()).is_err());
    }
}
