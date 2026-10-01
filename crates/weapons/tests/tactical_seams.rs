//! Weapon seams a tactical gun pack needs, built as general data: hitscan
//! shots, spread that changes when moving and after a pause, a gun held in
//! each hand, projectiles that pop on a bounce, throw out children and burn
//! what stands near them. The packs are written here; they are our own.
use bri_weapons::*;
use glam::Vec3;

const A: ActorId = ActorId(1);
const B: ActorId = ActorId(2);

/// A world with a player standing 10 units ahead of the shooter (down -z)
/// and a floor at y = -1. Rays that pass within 1 unit of the player's
/// centre hit them; anything else that crosses the floor lands on it.
struct Range {
    player: Vec3,
    near: Vec<Nearby>,
}
impl Default for Range {
    fn default() -> Self {
        Self {
            player: Vec3::new(0.0, 0.0, -10.0),
            near: Vec::new(),
        }
    }
}
impl Query for Range {
    fn sweep(&mut self, start: Vec3, end: Vec3, filter: Filter) -> Option<Hit> {
        let d = end - start;
        let length = d.length();
        if length <= 0.0 {
            return None;
        }
        if filter.players && filter.source != B {
            let along = (self.player - start).dot(d) / (length * length);
            if (0.0..=1.0).contains(&along) && (start + d * along).distance(self.player) < 1.0 {
                return Some(Hit {
                    target: TargetId::Actor(B),
                    position: start + d * along,
                    normal: -d / length,
                    fraction: along,
                    color: None,
                });
            }
        }
        if start.y >= -1.0 && end.y < -1.0 {
            let t = (start.y + 1.0) / (start.y - end.y);
            return Some(Hit {
                target: TargetId::Map(0),
                position: start + d * t,
                normal: Vec3::Y,
                fraction: t,
                color: None,
            });
        }
        None
    }
    fn radius(&mut self, _: Vec3, _: f32, _: usize) -> Vec<Nearby> {
        self.near.clone()
    }
    fn can_affect(&self, _: ActorId, _: TargetId) -> bool {
        true
    }
    fn can_catch(&self, _: ActorId, _: ActorId) -> bool {
        false
    }
}

/// A pistol whose bullet arrives at once, and a dual pair of them.
const PISTOLS: &str = r#"{
    "schema_version": 3,
    "id": "kit",
    "items": {
        "kit:weapon/pistol": { "ui_name": "Pistol", "image": "kit:image/pistol" },
        "kit:weapon/dual": { "ui_name": "Dual Pistols", "image": "kit:image/right" }
    },
    "images": {
        "kit:image/pistol": {
            "projectile": "kit:projectile/bullet",
            "shot": { "projectiles": 1, "spread": 0.01, "moving_spread": 0.05,
                      "rested": { "after_ticks": 60, "spread": 0.0 },
                      "hitscan": { "range": 200, "tracer": { "color": [1, 0.9, 0.5, 1] } } },
            "states": [
                { "name": "Activate", "ticks": 4, "timeout": 1 },
                { "name": "Ready", "down": 2 },
                { "name": "Fire", "ticks": 6, "timeout": 3, "script": "onFire", "allow_change": false },
                { "name": "Wait", "ticks": 2, "timeout": 1 }
            ]
        },
        "kit:image/right": {
            "projectile": "kit:projectile/bullet",
            "left_image": "kit:image/left",
            "shot": { "projectiles": 1, "hitscan": { "range": 200 } },
            "states": [
                { "name": "Activate", "ticks": 4, "timeout": 1 },
                { "name": "Ready", "down": 2 },
                { "name": "Fire", "ticks": 6, "timeout": 3, "script": "onFire", "allow_change": false },
                { "name": "Akimbo", "ticks": 6, "timeout": 1, "script": "onFireAkimbo" }
            ]
        },
        "kit:image/left": {
            "projectile": "kit:projectile/bullet",
            "mount_point": 1,
            "shot": { "projectiles": 1, "hitscan": { "range": 200 } },
            "states": [
                { "name": "Ready", "down": 1 },
                { "name": "Fire", "ticks": 2, "timeout": 0, "script": "onFire" }
            ]
        }
    },
    "projectiles": {
        "kit:projectile/bullet": { "speed": 200, "damage": 12, "damage_type": "$DamageType::Pistol",
                                   "lifetime_ticks": 120, "impulse": 50 }
    },
    "damage_types": { "pistol": { "name": "Pistol", "suicide_message": "%1 shot themselves",
                                   "murder_message": "%2 shot %1", "vehicle_scale": 1.0, "direct": true } }
}"#;

fn world(json: &str) -> WeaponsWorld {
    let mut w = WeaponsWorld::new(Pack::from_json(json.as_bytes()).unwrap()).unwrap();
    w.add_actor(A, 5).unwrap();
    w
}

fn step(w: &mut WeaponsWorld, q: &mut Range, ticks: usize, events: &mut Vec<Event>) {
    for _ in 0..ticks {
        events.extend(w.step(q));
    }
}

fn holding(item: &str) -> (WeaponsWorld, Range) {
    let mut w = world(PISTOLS);
    let slot = w.give(A, item).unwrap();
    w.equip(A, Some(slot)).unwrap();
    let mut q = Range::default();
    let mut events = Vec::new();
    step(&mut w, &mut q, 8, &mut events);
    (w, q)
}

/// Press, hold a tick, let go and play out the shot.
fn click(w: &mut WeaponsWorld, q: &mut Range) -> Vec<Event> {
    let mut events = Vec::new();
    w.trigger(A, true).unwrap();
    step(w, q, 2, &mut events);
    w.trigger(A, false).unwrap();
    step(w, q, 14, &mut events);
    events
}

fn tracers(events: &[Event]) -> Vec<(u8, Vec3)> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Tracer { hand, to, .. } => Some((*hand, *to)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_hitscan_shot_lands_at_once_and_does_what_its_projectile_would() {
    let (mut w, mut q) = holding("kit:weapon/pistol");
    let events = click(&mut w, &mut q);
    assert!(
        !events.iter().any(|e| matches!(e, Event::Spawned { .. })),
        "nothing flies"
    );
    assert_eq!(w.projectiles().count(), 0);
    let lines = tracers(&events);
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0].1.distance(Vec3::new(0.0, 0.0, -10.0)) < 1.0,
        "{lines:?}"
    );
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
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Impulse { target: TargetId::Actor(t), .. } if *t == B
    )));
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Contact { impact } if impact.target == TargetId::Actor(B)
        )),
        "brick and Add-On hooks hear it land, as a projectile's contact"
    );
}

#[test]
fn a_hitscan_shot_that_meets_nothing_draws_its_full_reach() {
    let (mut w, mut q) = holding("kit:weapon/pistol");
    q.player = Vec3::new(50.0, 0.0, 0.0);
    let events = click(&mut w, &mut q);
    let lines = tracers(&events);
    assert_eq!(lines.len(), 1);
    assert!((lines[0].1.length() - 200.0).abs() < 0.01, "{lines:?}");
    assert!(!events.iter().any(|e| matches!(e, Event::Damage { .. })));
    assert!(!events.iter().any(|e| matches!(e, Event::Contact { .. })));
}

#[test]
fn a_hitscan_reach_scales_with_the_shooter() {
    let (mut w, mut q) = holding("kit:weapon/pistol");
    q.player = Vec3::new(50.0, 0.0, 0.0);
    w.set_frame(
        A,
        Frame {
            scale: 2.0,
            ..Frame::default()
        },
    )
    .unwrap();
    let lines = tracers(&click(&mut w, &mut q));
    assert!((lines[0].1.length() - 400.0).abs() < 0.01, "{lines:?}");
}

#[test]
fn the_spread_is_chosen_by_movement_and_rest() {
    let shot = Shot {
        spread: 0.01,
        moving_spread: Some(0.05),
        rested: Some(Rested {
            after_ticks: 60,
            spread: 0.0,
        }),
        ..Shot::SINGLE
    };
    assert_eq!(shot.spread_for(0.0, None), 0.0, "a first shot is rested");
    assert_eq!(shot.spread_for(0.0, Some(60)), 0.0);
    assert_eq!(shot.spread_for(0.0, Some(59)), 0.01, "a follow-up");
    assert_eq!(shot.spread_for(5.0, None), 0.05, "moving beats rest");
    assert_eq!(Shot::SINGLE.spread_for(5.0, Some(1)), 0.0);
    let still_only = Shot {
        moving_spread: None,
        ..shot
    };
    assert_eq!(
        still_only.spread_for(5.0, None),
        0.01,
        "moving is never rested"
    );
}

#[test]
fn a_rested_shot_goes_true_and_a_quick_follow_up_spreads() {
    let (mut w, mut q) = holding("kit:weapon/pistol");
    q.player = Vec3::new(500.0, 0.0, 0.0);
    let first = tracers(&click(&mut w, &mut q))[0].1;
    assert!(
        first.distance(Vec3::new(0.0, 0.0, -200.0)) < 1e-3,
        "rested: dead on, {first}"
    );
    let second = tracers(&click(&mut w, &mut q))[0].1;
    assert!(
        second.distance(Vec3::new(0.0, 0.0, -200.0)) > 0.01,
        "16 ticks later: spread, {second}"
    );
    let mut events = Vec::new();
    step(&mut w, &mut q, 60, &mut events);
    let third = tracers(&click(&mut w, &mut q))[0].1;
    assert!(
        third.distance(Vec3::new(0.0, 0.0, -200.0)) < 1e-3,
        "rested again"
    );
}

#[test]
fn moving_spreads_the_shot_wider_and_everyone_computes_the_same_one() {
    let shoot = |velocity: Vec3| {
        let (mut w, mut q) = holding("kit:weapon/pistol");
        q.player = Vec3::new(500.0, 0.0, 0.0);
        w.set_frame(
            A,
            Frame {
                velocity,
                ..Frame::default()
            },
        )
        .unwrap();
        tracers(&click(&mut w, &mut q))[0].1
    };
    let running = shoot(Vec3::new(5.0, 0.0, 0.0));
    assert_eq!(running, shoot(Vec3::new(5.0, 0.0, 0.0)), "no random state");
    let off = running.distance(Vec3::new(0.0, 0.0, -200.0));
    assert!(off > 1.0, "a moving shot strays: {off}");
}

#[test]
fn a_dual_gun_brings_its_left_hand_and_takes_it_away() {
    let mut w = world(PISTOLS);
    let dual = w.give(A, "kit:weapon/dual").unwrap();
    let single = w.give(A, "kit:weapon/pistol").unwrap();
    w.equip(A, Some(dual)).unwrap();
    let mut q = Range::default();
    let mut events = Vec::new();
    step(&mut w, &mut q, 8, &mut events);
    assert_eq!(w.image_state(A, 0).unwrap().0.id, "kit:image/right");
    assert_eq!(w.image_state(A, 1).unwrap().0.id, "kit:image/left");
    w.equip(A, Some(single)).unwrap();
    step(&mut w, &mut q, 8, &mut events);
    assert_eq!(w.image_state(A, 0).unwrap().0.id, "kit:image/pistol");
    assert!(w.image_state(A, 1).is_none(), "the left hand is emptied");
}

#[test]
fn the_left_gun_fires_after_the_right_one() {
    let (mut w, mut q) = holding("kit:weapon/dual");
    q.player = Vec3::new(500.0, 0.0, 0.0);
    let events = click(&mut w, &mut q);
    let hands: Vec<u8> = tracers(&events).iter().map(|(h, _)| *h).collect();
    assert_eq!(hands, [0, 1], "right, then the pulsed left");
}

#[test]
fn a_rule_swapping_the_image_keeps_the_hands_consistent() {
    let (mut w, _) = holding("kit:weapon/dual");
    w.swap_image(A, Some("kit:image/pistol")).unwrap();
    assert!(
        w.image_state(A, 1).is_none(),
        "a one-handed image clears the left"
    );
    w.swap_image(A, None).unwrap();
    assert_eq!(
        w.image_state(A, 1).unwrap().0.id,
        "kit:image/left",
        "the tool's own pair"
    );
}

#[test]
fn left_images_and_hitscans_are_checked() {
    let missing = PISTOLS.replace(
        "\"left_image\": \"kit:image/left\"",
        "\"left_image\": \"kit:image/none\"",
    );
    assert!(Pack::from_json(missing.as_bytes()).is_err());
    let nested = PISTOLS.replace(
        "\"mount_point\": 1,",
        "\"mount_point\": 1, \"left_image\": \"kit:image/pistol\",",
    );
    assert!(Pack::from_json(nested.as_bytes()).is_err());
    let far = PISTOLS.replace("\"range\": 200, \"tracer\"", "\"range\": 5000, \"tracer\"");
    assert!(Pack::from_json(far.as_bytes()).is_err());
    let rest = PISTOLS.replace("\"after_ticks\": 60", "\"after_ticks\": 0");
    assert!(Pack::from_json(rest.as_bytes()).is_err());
    assert!(Pack::from_json(PISTOLS.as_bytes()).is_ok());
}

/// Thrown things: a grenade that pops on its third bounce, a flak shell
/// that sprays sparks in flight, a firebomb that bursts into embers that
/// burn whatever stands near them.
const THROWN: &str = r#"{
    "schema_version": 3,
    "id": "kit",
    "items": {},
    "images": {},
    "projectiles": {
        "kit:projectile/conc": { "speed": 20, "ballistic": true, "gravity": 1, "elasticity": 0.6,
                                 "friction": 0, "arm_ticks": 600, "lifetime_ticks": 600,
                                 "max_bounces": 3, "explosion": { "effect": "concExplosion" } },
        "kit:projectile/flak": { "speed": 60, "lifetime_ticks": 120,
                                 "children": { "projectile": "kit:projectile/spark", "count": 4,
                                               "speed": 20, "every_ticks": 24 } },
        "kit:projectile/spark": { "speed": 20, "lifetime_ticks": 12 },
        "kit:projectile/firebomb": { "speed": 20, "ballistic": true, "lifetime_ticks": 600,
                                     "children": { "projectile": "kit:projectile/ember", "count": 3,
                                                   "speed": 5, "on_explode": true } },
        "kit:projectile/ember": { "speed": 5, "ballistic": true, "elasticity": 0.3, "arm_ticks": 1800,
                                  "lifetime_ticks": 1800,
                                  "aura": { "radius": 4, "damage": 4, "every_ticks": 36,
                                            "damage_type": "$DamageType::Fire", "burn_seconds": 1 } }
    },
    "damage_types": { "fire": { "name": "Fire", "suicide_message": "%1 burned",
                                 "murder_message": "%2 burned %1", "vehicle_scale": 1.0, "direct": false } }
}"#;

fn removed(events: &[Event]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, Event::Removed { .. }))
        .count()
}

#[test]
fn a_grenade_pops_on_its_third_bounce() {
    let mut w = world(THROWN);
    let mut q = Range {
        player: Vec3::new(500.0, 0.0, 0.0),
        ..Range::default()
    };
    w.spawn(
        "kit:projectile/conc",
        A,
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(4.0, -6.0, 0.0),
        1.0,
    )
    .unwrap();
    let mut events = Vec::new();
    let mut bounces = 0;
    for _ in 0..600 {
        let tick = w.step(&mut q);
        bounces += tick
            .iter()
            .filter(|e| matches!(e, Event::Bounced { .. }))
            .count();
        let gone = removed(&tick) > 0;
        events.extend(tick);
        if gone {
            break;
        }
    }
    assert_eq!(bounces, 2, "two bounces, then the third sets it off");
    assert_eq!(
        w.projectiles().count(),
        0,
        "it went off long before its fuse"
    );
    assert!(
        events.iter().any(
            |e| matches!(e, Event::Effect { definition, .. } if definition == "concExplosion")
        )
    );
}

#[test]
fn a_flak_shell_sprays_sparks_as_it_flies() {
    let mut w = world(THROWN);
    let mut q = Range {
        player: Vec3::new(500.0, 0.0, 0.0),
        ..Range::default()
    };
    w.spawn(
        "kit:projectile/flak",
        A,
        Vec3::new(0.0, 5.0, 0.0),
        Vec3::new(0.0, 0.0, -60.0),
        1.0,
    )
    .unwrap();
    let mut events = Vec::new();
    for _ in 0..50 {
        events.extend(w.step(&mut q));
    }
    let sparks: Vec<Vec3> = events
        .iter()
        .filter_map(|e| match e {
            Event::Spawned {
                definition,
                velocity,
                ..
            } if definition == "kit:projectile/spark" => Some(*velocity),
            _ => None,
        })
        .collect();
    assert_eq!(sparks.len(), 8, "four at 24 ticks and four at 48");
    assert!(sparks.iter().all(|v| (v.length() - 20.0).abs() < 1e-3));
    assert!(
        sparks.windows(2).all(|p| p[0] != p[1]),
        "each its own direction"
    );
    // The same flight throws the same sparks.
    let mut again = world(THROWN);
    again
        .spawn(
            "kit:projectile/flak",
            A,
            Vec3::new(0.0, 5.0, 0.0),
            Vec3::new(0.0, 0.0, -60.0),
            1.0,
        )
        .unwrap();
    let mut events2 = Vec::new();
    for _ in 0..50 {
        events2.extend(again.step(&mut q));
    }
    let sparks2: Vec<Vec3> = events2
        .iter()
        .filter_map(|e| match e {
            Event::Spawned {
                definition,
                velocity,
                ..
            } if definition == "kit:projectile/spark" => Some(*velocity),
            _ => None,
        })
        .collect();
    assert_eq!(sparks, sparks2);
}

#[test]
fn a_firebomb_bursts_into_embers_that_burn_whoever_stands_near() {
    let mut w = world(THROWN);
    let mut q = Range {
        player: Vec3::new(500.0, 0.0, 0.0),
        near: vec![Nearby {
            target: TargetId::Actor(B),
            center: Vec3::ZERO,
            distance: 1.0,
        }],
    };
    w.spawn(
        "kit:projectile/firebomb",
        A,
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(0.0, -10.0, 0.0),
        1.0,
    )
    .unwrap();
    let mut events = Vec::new();
    for _ in 0..20 {
        events.extend(w.step(&mut q));
    }
    let embers = events
        .iter()
        .filter(|e| matches!(e, Event::Spawned { definition, .. } if definition == "kit:projectile/ember"))
        .count();
    assert_eq!(embers, 3, "it bursts on landing");
    events.clear();
    for _ in 0..72 {
        events.extend(w.step(&mut q));
    }
    let burns: Vec<f32> = events
        .iter()
        .filter_map(|e| match e {
            Event::Damage {
                kind,
                amount,
                target: TargetId::Actor(t),
                ..
            } if kind == "$DamageType::Fire" && *t == B => Some(*amount),
            _ => None,
        })
        .collect();
    assert_eq!(
        burns, [4.0; 6],
        "three embers, each every 36 ticks, at full strength"
    );
    assert!(events.iter().any(|e| matches!(e, Event::Burn { .. })));
}

#[test]
fn projectile_extras_are_checked() {
    let chain = THROWN.replace(
        "\"kit:projectile/spark\": { \"speed\": 20, \"lifetime_ticks\": 12 }",
        "\"kit:projectile/spark\": { \"speed\": 20, \"lifetime_ticks\": 12, \"children\": { \"projectile\": \"kit:projectile/ember\", \"on_explode\": true } }",
    );
    assert!(
        Pack::from_json(chain.as_bytes()).is_err(),
        "children may not have children"
    );
    let many = THROWN.replace("\"count\": 4", "\"count\": 17");
    assert!(Pack::from_json(many.as_bytes()).is_err());
    let never = THROWN.replace(", \"every_ticks\": 24", "");
    assert!(
        Pack::from_json(never.as_bytes()).is_err(),
        "no moment to throw them"
    );
    let hot = THROWN.replace("\"damage\": 4", "\"damage\": 400");
    assert!(Pack::from_json(hot.as_bytes()).is_err());
    let quick = THROWN.replace("\"every_ticks\": 36", "\"every_ticks\": 1");
    assert!(Pack::from_json(quick.as_bytes()).is_err());
    let bouncy = THROWN.replace("\"max_bounces\": 3", "\"max_bounces\": 65");
    assert!(Pack::from_json(bouncy.as_bytes()).is_err());
    assert!(Pack::from_json(THROWN.as_bytes()).is_ok());
}
