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
    /// Who stands at `player`: B, or the shooter A for a shot coming back.
    who: ActorId,
    near: Vec<Nearby>,
}
impl Default for Range {
    fn default() -> Self {
        Self {
            player: Vec3::new(0.0, 0.0, -10.0),
            who: B,
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
        // A shot is never cast into its own shooter until it has turned.
        let mine = filter.source == self.who && filter.projectile_age_ticks.is_none();
        if filter.players && !mine {
            let along = (self.player - start).dot(d) / (length * length);
            if (0.0..=1.0).contains(&along) && (start + d * along).distance(self.player) < 1.0 {
                return Some(Hit {
                    target: TargetId::Actor(self.who),
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
    "schema_version": 4,
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
            still: true,
            projectile: None,
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
    // Tier+Tactical's Assault Rifle rests by time alone.
    let any_pace = Shot {
        rested: Some(Rested {
            still: false,
            projectile: Some("kit:projectile/true".into()),
            ..still_only.rested.clone().unwrap()
        }),
        ..still_only
    };
    assert_eq!(any_pace.spread_for(5.0, None), 0.0, "rested on the run");
    assert_eq!(
        any_pace.projectile_for(5.0, Some(60)).map(String::as_str),
        Some("kit:projectile/true")
    );
    assert_eq!(any_pace.projectile_for(5.0, Some(59)), None, "a follow-up");
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
    "schema_version": 4,
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
        ..Range::default()
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

#[test]
fn a_vertical_recoil_pushes_only_up_or_down() {
    // TT_knockback(%obj, 0, 0, -1): aiming up, the gunner is pushed down.
    let shot = Shot {
        recoil_vertical: Some(1.0),
        ..Shot::SINGLE
    };
    let aim = Vec3::new(0.0, 0.6, -0.8);
    assert_eq!(shot.recoil_velocity(aim), Vec3::new(0.0, -0.6, 0.0));
    // TT_knockback(%obj, -4, -4, -4): straight back along the aim.
    let shot = Shot {
        recoil: 4.0,
        ..Shot::SINGLE
    };
    assert_eq!(shot.recoil_velocity(aim), -aim * 4.0);
}

/// A rifle whose first round after a pause is a truer one, and a machine
/// gun whose every pull fires a second, free and tighter round.
const AUTOMATICS: &str = r#"{
    "schema_version": 4,
    "id": "kit",
    "items": {
        "kit:weapon/rifle": { "ui_name": "Rifle", "image": "kit:image/rifle" },
        "kit:weapon/mg": { "ui_name": "Machine Gun", "image": "kit:image/mg" }
    },
    "images": {
        "kit:image/rifle": {
            "projectile": "kit:projectile/round",
            "shot": { "spread": 0.01,
                      "rested": { "after_ticks": 60, "spread": 0.0, "projectile": "kit:projectile/true" } },
            "states": [
                { "name": "Activate", "ticks": 4, "timeout": 1 },
                { "name": "Ready", "down": 2 },
                { "name": "Fire", "ticks": 6, "timeout": 3, "script": "onFire", "allow_change": false },
                { "name": "Wait", "ticks": 2, "timeout": 1 }
            ]
        },
        "kit:image/mg": {
            "projectile": "kit:projectile/round",
            "shot": { "spread": 0.02 },
            "state_shots": { "onfire2": { "spread": 0.0, "free": true } },
            "magazine": { "size": 10, "ammo": "belt", "reload_ticks": 24, "reserve": 0, "max_reserve": 100 },
            "states": [
                { "name": "Activate", "ticks": 4, "timeout": 1 },
                { "name": "Ready", "down": 2 },
                { "name": "Fire", "ticks": 6, "timeout": 3, "script": "onFire", "allow_change": false },
                { "name": "Fire2", "ticks": 4, "timeout": 4, "script": "onFire2", "allow_change": false },
                { "name": "Wait", "ticks": 2, "timeout": 1 }
            ]
        }
    },
    "projectiles": {
        "kit:projectile/round": { "speed": 200, "damage": 12, "lifetime_ticks": 120 },
        "kit:projectile/true": { "speed": 300, "damage": 12, "lifetime_ticks": 120 }
    }
}"#;

fn spawned(events: &[Event]) -> Vec<(String, Vec3)> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Spawned {
                definition,
                velocity,
                ..
            } => Some((definition.clone(), *velocity)),
            _ => None,
        })
        .collect()
}

fn holding_automatic(item: &str) -> (WeaponsWorld, Range) {
    let mut w = world(AUTOMATICS);
    let slot = w.give(A, item).unwrap();
    w.equip(A, Some(slot)).unwrap();
    let mut q = Range::default();
    step(&mut w, &mut q, 8, &mut Vec::new());
    (w, q)
}

#[test]
fn a_rested_shot_flies_its_own_round() {
    let (mut w, mut q) = holding_automatic("kit:weapon/rifle");
    let first = spawned(&click(&mut w, &mut q));
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].0, "kit:projectile/true", "the first round is true");
    let second = spawned(&click(&mut w, &mut q));
    assert_eq!(second[0].0, "kit:projectile/round", "a quick follow-up");
    step(&mut w, &mut q, 60, &mut Vec::new());
    q.player = Vec3::new(500.0, 0.0, 0.0);
    let third = spawned(&click(&mut w, &mut q));
    assert_eq!(third[0].0, "kit:projectile/true", "rested again");
}

#[test]
fn a_firing_state_fires_a_free_second_round_straight() {
    let (mut w, mut q) = holding_automatic("kit:weapon/mg");
    // Every round of a few pulls: one from onFire, one from Fire2.
    let mut rounds = Vec::new();
    for _ in 0..3 {
        rounds.extend(spawned(&click(&mut w, &mut q)));
    }
    assert_eq!(rounds.len(), 6, "{rounds:?}");
    assert_eq!(
        w.ammo(A).unwrap().rounds,
        7,
        "only onFire takes from the magazine; onFire2 is free"
    );
    let straight = Vec3::new(0.0, 0.0, -200.0);
    for (n, (_, velocity)) in rounds.iter().enumerate() {
        if n % 2 == 1 {
            assert!(
                velocity.distance(straight) < 1e-3,
                "Fire2 is true: {velocity}"
            );
        }
    }
    assert!(
        rounds
            .iter()
            .step_by(2)
            .any(|(_, v)| v.distance(straight) > 0.01),
        "onFire keeps the shot's spread"
    );
}

#[test]
fn a_rested_round_is_checked() {
    let bad = AUTOMATICS.replace(
        r#""projectile": "kit:projectile/true""#,
        r#""projectile": "kit:projectile/none""#,
    );
    let error = Pack::from_json(bad.as_bytes())
        .expect_err("an unknown rested round is refused")
        .to_string();
    assert!(
        error.contains("Missing projectile kit:projectile/none"),
        "{error}"
    );
}

/// The pistol with its hitscan cast from a muzzle held 1.5 units to the
/// right of the eye, in third person, and `extra` in its hitscan.
fn muzzle_off_to_the_side(extra: &str) -> (WeaponsWorld, Range) {
    let json = PISTOLS
        .replace(r#""spread": 0.01, "moving_spread": 0.05,"#, "")
        .replace(
            r#""hitscan": { "range": 200, "tracer""#,
            &format!(r#""hitscan": {{ "range": 200, {extra} "tracer""#),
        );
    let mut w = world(&json);
    let slot = w.give(A, "kit:weapon/pistol").unwrap();
    w.equip(A, Some(slot)).unwrap();
    w.set_frame(
        A,
        Frame {
            muzzle: [Vec3::new(1.5, 0.0, 0.0); 2],
            first_person: false,
            ..Frame::default()
        },
    )
    .unwrap();
    let mut q = Range::default();
    step(&mut w, &mut q, 8, &mut Vec::new());
    (w, q)
}

fn hit_b(events: &[Event]) -> bool {
    events
        .iter()
        .any(|e| matches!(e, Event::Damage { target: TargetId::Actor(t), .. } if *t == B))
}

#[test]
fn a_muzzle_shot_starts_at_the_eye_when_something_stands_right_before_it() {
    // B stands 1.5 units before the eye: the muzzle's straight line passes
    // beside them.
    let (mut w, mut q) = muzzle_off_to_the_side("");
    q.player = Vec3::new(0.0, 0.0, -1.5);
    assert!(!hit_b(&click(&mut w, &mut q)), "from the muzzle it misses");
    let (mut w, mut q) = muzzle_off_to_the_side(r#""eye_within": 4.5,"#);
    q.player = Vec3::new(0.0, 0.0, -1.5);
    assert!(hit_b(&click(&mut w, &mut q)), "from the eye it hits");
    // Nothing that close: the shot still leaves the muzzle.
    let (mut w, mut q) = muzzle_off_to_the_side(r#""eye_within": 4.5,"#);
    let events = click(&mut w, &mut q);
    assert!(!hit_b(&events));
    let lines = tracers(&events);
    assert!((lines[0].1.x - 1.5).abs() < 0.01, "{lines:?}");
}

#[test]
fn a_converging_muzzle_shot_lands_where_the_eye_looks() {
    let (mut w, mut q) = muzzle_off_to_the_side("");
    assert!(
        !hit_b(&click(&mut w, &mut q)),
        "parallel to the look it misses"
    );
    let (mut w, mut q) = muzzle_off_to_the_side(r#""converge": true,"#);
    assert!(
        hit_b(&click(&mut w, &mut q)),
        "aimed at the look's point it hits"
    );
    // With nothing in the look, it aims at the far end of the range.
    let (mut w, mut q) = muzzle_off_to_the_side(r#""converge": true,"#);
    q.player = Vec3::new(50.0, 0.0, 0.0);
    let lines = tracers(&click(&mut w, &mut q));
    assert!(
        lines[0].1.distance(Vec3::new(0.0, 0.0, -200.0)) < 0.5,
        "{lines:?}"
    );
}

/// The pistol's round turning off what it meets up to `times` times, with
/// `damage` more for each landing before and `shooter` of its damage on
/// its own shooter, aimed along `direction`.
fn ricocheting(direction: Vec3) -> (WeaponsWorld, Range) {
    let json = PISTOLS
        .replace(r#""spread": 0.01, "moving_spread": 0.05,"#, "")
        .replace(
            r#""hitscan": { "range": 200, "tracer""#,
            r#""hitscan": { "range": 200,
                "ricochet": { "times": 2, "damage": 30, "shooter": 0.5 }, "tracer""#,
        );
    let mut w = world(&json);
    let slot = w.give(A, "kit:weapon/pistol").unwrap();
    w.equip(A, Some(slot)).unwrap();
    w.set_frame(
        A,
        Frame {
            direction,
            ..Frame::default()
        },
    )
    .unwrap();
    let mut q = Range::default();
    step(&mut w, &mut q, 8, &mut Vec::new());
    (w, q)
}

fn damage(events: &[Event]) -> Vec<(TargetId, f32, u32)> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Damage {
                target,
                amount,
                bounces,
                ..
            } => Some((*target, *amount, *bounces)),
            _ => None,
        })
        .collect()
}

/// ShortRifleKai's ray, as a pack field: aimed at the floor halfway to B,
/// it mirrors off the floor into B with 30 more damage for the landing
/// before, draws the turn as its own streak, and tells the hit it came
/// after one turn.
#[test]
fn a_ricocheting_ray_turns_off_the_floor_and_hits_harder_after() {
    let (mut w, mut q) = ricocheting(Vec3::new(0.0, -1.0, -5.0).normalize());
    let events = click(&mut w, &mut q);
    assert_eq!(damage(&events), [(TargetId::Actor(B), 42.0, 1)]);
    let lines = tracers(&events);
    assert!(
        lines[0].1.distance(Vec3::new(0.0, -1.0, -5.0)) < 0.01,
        "{lines:?}"
    );
    let turns: Vec<(Vec3, Vec3)> = events
        .iter()
        .filter_map(|e| match e {
            Event::Ricochet { from, to, .. } => Some((*from, *to)),
            _ => None,
        })
        .collect();
    assert_eq!(turns.len(), 2, "into B, then off B: {turns:?}");
    assert!(
        turns[0].0.distance(Vec3::new(0.0, -1.0, -5.0)) < 0.02,
        "{turns:?}"
    );
    assert!(
        turns[0].1.distance(Vec3::new(0.0, 0.0, -10.0)) < 0.05,
        "{turns:?}"
    );
    // Without a ricochet the same shot stops on the floor.
    let mut w = world(&PISTOLS.replace(r#""spread": 0.01, "moving_spread": 0.05,"#, ""));
    let slot = w.give(A, "kit:weapon/pistol").unwrap();
    w.equip(A, Some(slot)).unwrap();
    w.set_frame(
        A,
        Frame {
            direction: Vec3::new(0.0, -1.0, -5.0).normalize(),
            ..Frame::default()
        },
    )
    .unwrap();
    step(&mut w, &mut q, 8, &mut Vec::new());
    assert!(damage(&click(&mut w, &mut q)).is_empty());
}

/// Fired straight down, the turned ray comes back up into its shooter,
/// who takes the shooter's share of its damage.
#[test]
fn a_ricochet_back_into_its_shooter_does_the_shooters_share() {
    let (mut w, mut q) = ricocheting(Vec3::NEG_Y);
    q.who = A;
    q.player = Vec3::ZERO;
    let events = click(&mut w, &mut q);
    assert_eq!(damage(&events), [(TargetId::Actor(A), 6.0, 1)]);
}

#[test]
fn a_ricochet_out_of_range_is_refused() {
    let json = PISTOLS.replace(
        r#""hitscan": { "range": 200, "tracer""#,
        r#""hitscan": { "range": 200, "ricochet": { "times": 0 }, "tracer""#,
    );
    let error = Pack::from_json(json.as_bytes())
        .expect_err("a ricochet that never turns is refused")
        .to_string();
    assert!(error.contains("ricochet"), "{error}");
}
