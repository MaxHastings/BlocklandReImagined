//! A held guard ([`Guard`]): a shield that, raised, takes most of what
//! strikes its holder from in front, sends shots back the way they look,
//! spares them the blast of a shot it stopped, and breaks after enough
//! stops, as Kai's riot shield did. The pack is written here; it is our own.
use bri_weapons::*;
use glam::Vec3;

const A: ActorId = ActorId(1);
const B: ActorId = ActorId(2);

/// `B` stands 20 units ahead of `A` down -Z: what flies at -Z strikes
/// `B`'s front at -19.6, and what flies back at +Z past 0 strikes `A`. A
/// blast reaches `B` at its middle.
struct Field;
impl Query for Field {
    fn sweep(&mut self, from: Vec3, to: Vec3, _: Filter) -> Option<Hit> {
        let (target, z) = if to.z < -19.6 && from.z >= -19.6 {
            (B, -19.6)
        } else if to.z > 0.0 && from.z <= 0.0 && from.z > -19.5 {
            (A, 0.0)
        } else {
            return None;
        };
        let fraction = (z - from.z) / (to.z - from.z);
        Some(Hit {
            target: TargetId::Actor(target),
            position: from.lerp(to, fraction),
            normal: Vec3::Z * (to.z - from.z).signum() * -1.0,
            fraction,
            color: None,
        })
    }
    fn radius(&mut self, center: Vec3, radius: f32, _: usize) -> Vec<Nearby> {
        let middle = Vec3::new(0.0, 1.0, -20.0);
        let distance = center.distance(middle);
        if distance <= radius {
            vec![Nearby {
                target: TargetId::Actor(B),
                center: middle,
                distance,
            }]
        } else {
            vec![]
        }
    }
    fn can_affect(&self, _: ActorId, _: TargetId) -> bool {
        true
    }
    fn can_catch(&self, _: ActorId, _: ActorId) -> bool {
        false
    }
}

/// A gun firing a slow bullet that bursts on what it hits, a ray gun, and
/// a shield that guards while `Ready`, sends shots back and breaks after
/// two stops.
const KIT: &str = r#"{
    "schema_version": 4,
    "id": "kit",
    "items": {
        "kit:weapon/gun": { "ui_name": "Gun", "image": "kit:image/gun" },
        "kit:weapon/raygun": { "ui_name": "Ray Gun", "image": "kit:image/raygun" },
        "kit:weapon/shield": { "ui_name": "Shield", "image": "kit:image/shield" }
    },
    "images": {
        "kit:image/gun": {
            "projectile": "kit:projectile/bullet",
            "states": [
                { "name": "Activate", "ticks": 4, "timeout": 1 },
                { "name": "Ready", "down": 2 },
                { "name": "Fire", "ticks": 6, "timeout": 3, "script": "onFire" },
                { "name": "Wait", "up": 1 }
            ]
        },
        "kit:image/raygun": {
            "projectile": "kit:projectile/ray",
            "shot": { "projectiles": 1, "hitscan": { "range": 200 } },
            "states": [
                { "name": "Activate", "ticks": 4, "timeout": 1 },
                { "name": "Ready", "down": 2 },
                { "name": "Fire", "ticks": 6, "timeout": 3, "script": "onFire" },
                { "name": "Wait", "up": 1 }
            ]
        },
        "kit:image/shield": {
            "projectile": "kit:projectile/bash",
            "states": [
                { "name": "Activate", "ticks": 4, "timeout": 1 },
                { "name": "Ready" }
            ],
            "guard": {
                "states": ["ready"],
                "front": { "up": 0.75, "above": 3.3, "down": 0.75, "below": 4.4 },
                "projectile_damage": 0.05,
                "damage": 0.2,
                "push": 0.3,
                "reflect": true,
                "reflect_kill": "Reflected",
                "hit_explosion": "ClangProjectile",
                "sounds": ["kit:sound/bing", "kit:sound/twang", "kit:sound/twang"],
                "durability": 2,
                "break_explosion": "kit:projectile/pieces"
            }
        }
    },
    "projectiles": {
        "kit:projectile/bullet": { "speed": 60, "lifetime_ticks": 600, "damage": 40,
                                   "damage_type": "$DamageType::Gun", "impulse": 100,
                                   "collide_players": true,
                                   "explosion": { "radius": 4, "damage": 20,
                                                  "impulse_radius": 4, "impulse": 50 } },
        "kit:projectile/ray": { "speed": 200, "lifetime_ticks": 120, "damage": 30,
                                "damage_type": "$DamageType::Gun", "collide_players": true },
        "kit:projectile/bash": { "speed": 100, "lifetime_ticks": 8 },
        "kit:projectile/clang": { "name": "ClangProjectile", "speed": 1, "lifetime_ticks": 1,
                                  "explosion": { "effect": "clangexplosion" } },
        "kit:projectile/pieces": { "speed": 1, "lifetime_ticks": 1,
                                   "explosion": { "effect": "piecesexplosion" } }
    },
    "damage_types": {
        "gun": { "name": "Gun", "suicide_message": "<bitmap:ci/gun> %1",
                 "murder_message": "%2 <bitmap:ci/gun> %1", "vehicle_scale": 1.0,
                 "direct": true },
        "reflected": { "name": "Reflected", "suicide_message": "<bitmap:ci/reflect> %3%1",
                       "murder_message": "%2 <bitmap:ci/reflect>%3%1", "vehicle_scale": 1.0,
                       "direct": false, "special": true }
    },
    "bindings": [
        { "setting": "$Pref::Server::Test::Durability",
          "field": ["images", "kit:image/shield", "guard", "durability"],
          "values": { "-1": null, "0": 1 }, "scale": 1 },
        { "setting": "$Pref::Server::Test::BreakBot",
          "field": ["images", "kit:image/shield", "guard", "bots_keep"],
          "values": { "false": true } },
        { "setting": "$Pref::Server::Test::StopFalls",
          "field": ["images", "kit:image/shield", "guard", "fall_damage"],
          "values": { "true": 0.125 } }
    ]
}"#;

/// `A` holds `gun`; `B` holds the shield up, looking along `look`.
fn field(gun: &str, look: Vec3) -> WeaponsWorld {
    field_set(gun, look, &[])
}

/// [`field`] with the kit's settings at these values.
fn field_set(gun: &str, look: Vec3, settings: &[(&str, &str)]) -> WeaponsWorld {
    let pack = Pack::from_json(KIT.as_bytes())
        .unwrap()
        .with_settings(|name| {
            settings
                .iter()
                .find(|(n, _)| format!("$Pref::Server::Test::{n}") == name)
                .map(|(_, v)| (*v).to_owned())
        })
        .unwrap();
    let mut w = WeaponsWorld::new(pack).unwrap();
    w.add_actor(A, 5).unwrap();
    w.add_actor(B, 5).unwrap();
    let slot = w.give(A, gun).unwrap();
    w.equip(A, Some(slot)).unwrap();
    w.give(B, "kit:weapon/gun").unwrap();
    let slot = w.give(B, "kit:weapon/shield").unwrap();
    w.equip(B, Some(slot)).unwrap();
    w.set_frame(
        A,
        Frame {
            eye: Vec3::new(0.0, 1.0, 0.0),
            muzzle: [Vec3::new(0.0, 1.0, 0.0); 2],
            middle: Some(Vec3::new(0.0, 1.0, 0.0)),
            ..Frame::default()
        },
    )
    .unwrap();
    w.set_frame(
        B,
        Frame {
            position: Vec3::new(0.0, 0.0, -20.0),
            eye: Vec3::new(0.0, 2.0, -20.0),
            middle: Some(Vec3::new(0.0, 1.0, -20.0)),
            direction: look,
            ..Frame::default()
        },
    )
    .unwrap();
    for _ in 0..8 {
        w.step(&mut Field);
    }
    w
}

/// `A` fires once, and the shot plays out.
fn fire(w: &mut WeaponsWorld) -> Vec<Event> {
    w.trigger(A, true).unwrap();
    let mut events: Vec<Event> = (0..2).flat_map(|_| w.step(&mut Field)).collect();
    w.trigger(A, false).unwrap();
    events.extend((0..90).flat_map(|_| w.step(&mut Field)));
    events
}

/// What each hurt dealt: to whom, how much, and the special kill it makes.
fn hurts(events: &[Event]) -> Vec<(TargetId, f32, Option<String>)> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Damage {
                target,
                amount,
                special,
                ..
            } => Some((*target, *amount, special.clone())),
            _ => None,
        })
        .collect()
}

fn pushes(events: &[Event], target: ActorId) -> Vec<f32> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Impulse {
                target: TargetId::Actor(t),
                impulse,
                ..
            } if *t == target => Some(impulse.length()),
            _ => None,
        })
        .collect()
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

#[test]
fn a_raised_shield_stops_a_shot_from_in_front_and_sends_it_back() {
    let mut w = field("kit:weapon/gun", Vec3::Z);
    let events = fire(&mut w);
    let hurt = hurts(&events);
    // The hit keeps 5%, then 20% of that again as any hurt from in front
    // does (Kai's two packages both scale it); the blast spares B.
    let on_b: Vec<_> = hurt
        .iter()
        .filter(|(t, ..)| *t == TargetId::Actor(B))
        .collect();
    assert_eq!(on_b.len(), 1, "{hurt:?}");
    assert!(near(on_b[0].1, 40.0 * 0.05 * 0.2), "{hurt:?}");
    assert!(on_b[0].2.is_none());
    // Its push keeps 30%, its blast's push too.
    let pushed = pushes(&events, B);
    assert_eq!(pushed.len(), 2, "{pushed:?}");
    assert!(near(pushed[0], 100.0 * 0.3), "{pushed:?}");
    assert!(pushed[1] < 50.0 * 0.3 + 0.01, "{pushed:?}");
    // One of its sounds where the shot struck.
    assert!(events.iter().any(|e| matches!(e,
        Event::Sound { profile, position, .. }
            if profile.starts_with("kit:sound/") && near(position.z, -19.6))));
    // Sent back as B's, the way B looks, as fast as it came: it strikes A
    // and the kill would read as reflected.
    assert!(events.iter().any(|e| matches!(e,
        Event::Spawned { source, velocity, .. } if *source == B && near(velocity.z, 60.0))));
    let on_a: Vec<_> = hurt
        .iter()
        .filter(|(t, ..)| *t == TargetId::Actor(A))
        .collect();
    assert!(!on_a.is_empty(), "{hurt:?}");
    assert!(near(on_a[0].1, 40.0), "{hurt:?}");
    assert_eq!(on_a[0].2.as_deref(), Some("Reflected"));
    // The clang goes off at B.
    assert!(events.iter().any(|e| matches!(e,
        Event::Effect { definition, position, .. }
            if definition == "clangexplosion" && near(position.z, -20.0))));
}

#[test]
fn a_shield_turned_away_stops_nothing() {
    let mut w = field("kit:weapon/gun", Vec3::NEG_Z);
    let events = fire(&mut w);
    let on_b: Vec<_> = hurts(&events)
        .into_iter()
        .filter(|(t, ..)| *t == TargetId::Actor(B))
        .collect();
    // The hit, then the blast.
    assert_eq!(on_b.len(), 2, "{on_b:?}");
    assert!(near(on_b[0].1, 40.0), "{on_b:?}");
    assert!(on_b[1].1 > 0.0);
    assert!(!events.iter().any(|e| matches!(e,
        Event::Spawned { source, .. } if *source == B)));
}

#[test]
fn a_ray_from_in_front_keeps_a_fifth_and_is_not_sent_back() {
    let mut w = field("kit:weapon/raygun", Vec3::Z);
    let events = fire(&mut w);
    let on_b: Vec<_> = hurts(&events)
        .into_iter()
        .filter(|(t, ..)| *t == TargetId::Actor(B))
        .collect();
    assert_eq!(on_b.len(), 1, "{on_b:?}");
    assert!(near(on_b[0].1, 30.0 * 0.2), "{on_b:?}");
    assert!(!events.iter().any(|e| matches!(e,
        Event::Spawned { source, .. } if *source == B)));
}

#[test]
fn looking_steeply_up_the_shield_covers_what_strikes_above_its_reach() {
    // Kai's test by height: looking up, anything above 3.3 units below the
    // body's middle is covered, even a shot from behind.
    let mut w = field("kit:weapon/raygun", Vec3::new(0.0, 0.9, -0.43).normalize());
    let on_b: Vec<_> = hurts(&fire(&mut w))
        .into_iter()
        .filter(|(t, ..)| *t == TargetId::Actor(B))
        .collect();
    assert!(near(on_b[0].1, 30.0 * 0.2), "{on_b:?}");
}

#[test]
fn a_shield_breaks_on_its_last_stop_and_leaves_the_holder() {
    let mut w = field("kit:weapon/gun", Vec3::Z);
    fire(&mut w);
    assert!(w.image_state(B, 0).is_some());
    let events = fire(&mut w);
    let b = w.actor(B).unwrap();
    assert!(b.selected.is_none() && w.image_state(B, 0).is_none());
    assert_eq!(
        b.inventory.iter().flatten().collect::<Vec<_>>(),
        ["kit:weapon/gun"]
    );
    assert!(events.iter().any(|e| matches!(e,
        Event::Effect { definition, .. } if definition == "piecesexplosion")));
    // A new one starts whole: B dies, and two more stops break it again.
    w.respawned(B).unwrap();
    let slot = w.give(B, "kit:weapon/shield").unwrap();
    w.equip(B, Some(slot)).unwrap();
    for _ in 0..8 {
        w.step(&mut Field);
    }
    fire(&mut w);
    assert!(w.image_state(B, 0).is_some());
}

#[test]
fn the_durability_setting_counts_stops_and_minus_one_never_breaks() {
    // 1 stop, as 0 is.
    let mut w = field_set("kit:weapon/gun", Vec3::Z, &[("Durability", "0")]);
    fire(&mut w);
    assert!(w.image_state(B, 0).is_none(), "broken at the first stop");
    let mut w = field_set("kit:weapon/gun", Vec3::Z, &[("Durability", "-1")]);
    for _ in 0..4 {
        fire(&mut w);
    }
    assert!(w.image_state(B, 0).is_some(), "never breaks");
}

#[test]
fn with_bots_keeping_their_shields_only_players_wear_theirs_out() {
    let mut w = field_set("kit:weapon/gun", Vec3::Z, &[("BreakBot", "false")]);
    w.set_bot(B, true).unwrap();
    for _ in 0..4 {
        fire(&mut w);
    }
    assert!(w.image_state(B, 0).is_some(), "a bot's never wears out");
    w.set_bot(B, false).unwrap();
    fire(&mut w);
    fire(&mut w);
    assert!(w.image_state(B, 0).is_none(), "a player's does");
    // Breaking for bots (the default), a bot's breaks too.
    let mut w = field_set("kit:weapon/gun", Vec3::Z, &[("BreakBot", "true")]);
    w.set_bot(B, true).unwrap();
    fire(&mut w);
    fire(&mut w);
    assert!(w.image_state(B, 0).is_none());
}

#[test]
fn a_shield_raised_the_way_its_holder_falls_takes_most_of_the_fall() {
    // Off (the default), a fall hurts in full.
    let mut w = field("kit:weapon/gun", Vec3::NEG_Y);
    assert_eq!(w.guard_fall(B, 40.0, Vec3::NEG_Y), 40.0);
    let mut w = field_set("kit:weapon/gun", Vec3::NEG_Y, &[("StopFalls", "true")]);
    assert!(near(w.guard_fall(B, 40.0, Vec3::NEG_Y), 5.0), "an eighth");
    let clang = w.step(&mut Field);
    assert!(
        clang.iter().any(|e| matches!(e,
        Event::Effect { definition, .. } if definition == "clangexplosion")),
        "{clang:?}"
    );
    // Looking up, the fall is not met.
    let mut w = field_set("kit:weapon/gun", Vec3::Y, &[("StopFalls", "true")]);
    assert_eq!(w.guard_fall(B, 40.0, Vec3::NEG_Y), 40.0);
    // Nor is it with the shield put away.
    let mut w = field_set("kit:weapon/gun", Vec3::NEG_Y, &[("StopFalls", "true")]);
    w.equip(B, None).unwrap();
    assert_eq!(w.guard_fall(B, 40.0, Vec3::NEG_Y), 40.0);
}
