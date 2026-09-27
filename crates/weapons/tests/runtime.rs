use bri_weapons::*;
use glam::Vec3;
use std::collections::BTreeMap;
#[derive(Default)]
struct Scene {
    hit: Option<Hit>,
    near: Vec<Nearby>,
    deny: bool,
    response: Option<ContactResponse>,
}
impl Query for Scene {
    fn on_contact(&mut self, _: &ProjectileContact) -> ContactResponse {
        self.response.unwrap_or(ContactResponse::Continue)
    }
    fn sweep(&mut self, start: Vec3, end: Vec3, filter: Filter) -> Option<Hit> {
        self.hit
            .as_ref()
            .filter(|h| {
                !(matches!(h.target, TargetId::Actor(_)) && (!filter.players || filter.world_only))
            })
            .and_then(|h| {
                let segment = end - start;
                if segment.length_squared() < 0.0001 {
                    return None;
                }
                let t = (h.position - start).dot(segment) / segment.length_squared();
                if (0.0..=1.0).contains(&t) {
                    Some(Hit {
                        fraction: t,
                        ..h.clone()
                    })
                } else {
                    None
                }
            })
    }
    fn radius(&mut self, _: Vec3, _: f32, limit: usize) -> Vec<Nearby> {
        self.near.iter().take(limit).cloned().collect()
    }
    fn can_affect(&self, _: ActorId, _: TargetId) -> bool {
        !self.deny
    }
    fn can_catch(&self, _: ActorId, _: ActorId) -> bool {
        !self.deny
    }
}
fn load() -> Pack {
    let path = std::env::var_os("BRI_WEAPONS_PACK")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../content/weapons-pack-008/weapons.json")
        });
    Pack::from_json(&std::fs::read(path).expect("Run documented importer first")).unwrap()
}
fn world(item: &str) -> WeaponsWorld {
    let mut w = WeaponsWorld::new(load()).unwrap();
    w.add_actor(ActorId(1), 5).unwrap();
    let slot = w.give(ActorId(1), &native_id("weapon", item)).unwrap();
    w.equip(ActorId(1), Some(slot)).unwrap();
    w
}
fn run(w: &mut WeaponsWorld, n: usize, q: &mut impl Query) -> Vec<Event> {
    (0..n).flat_map(|_| w.step(q)).collect()
}
fn shots(e: &[Event]) -> usize {
    e.iter()
        .filter(|e| matches!(e, Event::Spawned { .. }))
        .count()
}
fn hit(target: TargetId, z: f32) -> Hit {
    Hit {
        target,
        position: Vec3::new(0.0, 0.0, z),
        normal: Vec3::Z,
        fraction: 0.0,
        color: Some([1.0, 0.0, 0.0]),
    }
}
fn empty() -> Pack {
    Pack {
        schema_version: SCHEMA,
        id: "test".into(),
        items: BTreeMap::new(),
        images: BTreeMap::new(),
        projectiles: BTreeMap::new(),
        damage_types: BTreeMap::new(),
        explosions: BTreeMap::new(),
        definitions: vec![],
        resources: vec![],
        diagnostics: vec![],
    }
}
#[test]
fn actor_inputs_are_bounded() {
    let mut w = WeaponsWorld::new(empty()).unwrap();
    assert!(w.add_actor(ActorId(1), 17).is_err());
    w.add_actor(ActorId(1), 5).unwrap();
    assert!(w.add_actor(ActorId(1), 5).is_err());
    let frame = Frame {
        eye: Vec3::splat(f32::NAN),
        ..Default::default()
    };
    assert!(w.set_frame(ActorId(1), frame).is_err());
    assert!(w.equip(ActorId(1), Some(99)).is_err());
    assert!(
        w.spawn("unknown", ActorId(1), Vec3::ZERO, Vec3::ZERO, 1.0)
            .is_err()
    );
}

#[test]
fn core_tools_share_slots_drops_and_validated_checkpoints() {
    let mut w = WeaponsWorld::new(empty()).unwrap();
    let a = ActorId(1);
    let b = ActorId(2);
    w.add_actor(a, 5).unwrap();
    w.add_actor(b, 5).unwrap();
    for (slot, item) in CORE_TOOLS[..3].iter().enumerate() {
        assert_eq!(w.give(a, item).unwrap(), slot);
    }
    // v20 allows a second copy of an item.
    assert_eq!(w.give(a, CORE_TOOLS[0]).unwrap(), 3);
    w.drop_item(a, 3).unwrap();
    let before = w.actor(a).unwrap().inventory.clone();
    assert!(w.give(a, "v20.weapon.unknown").is_err());
    assert_eq!(w.actor(a).unwrap().inventory, before);
    w.equip(a, Some(0)).unwrap();
    assert!(w.image_state(a, 0).is_none()); // building authority owns the hammer action
    let drop = w.drop_item(a, 0).unwrap();
    assert_eq!(w.actor(a).unwrap().selected, None);
    assert!(w.pickup(a, drop).is_err());
    let bytes = serde_json::to_vec(&w.save()).unwrap();
    let mut restored = WeaponsWorld::restore(empty(), &bytes).unwrap();
    assert_eq!(restored.pickup(b, drop).unwrap(), 0);
    assert_eq!(
        restored.actor(b).unwrap().inventory[0].as_deref(),
        Some(CORE_TOOLS[0])
    );
    assert!(restored.pickup(a, drop).is_err());
    assert_eq!(restored.give(a, CORE_TOOLS[3]).unwrap(), 0);
    let mut corrupt = restored.save();
    corrupt.actors[0].1.inventory[4] = Some("v20.weapon.unknown".into());
    assert!(WeaponsWorld::restore(empty(), &serde_json::to_vec(&corrupt).unwrap()).is_err());
}

#[test]
fn dropped_tool_preserves_body_orientation_scale_timeout_and_checked_legacy_migration() {
    let mut w = WeaponsWorld::new(empty()).unwrap();
    let actor = ActorId(7);
    w.add_actor(actor, 5).unwrap();
    w.give(actor, CORE_TOOLS[0]).unwrap();
    w.set_frame(
        actor,
        Frame {
            body_yaw: std::f32::consts::FRAC_PI_2,
            scale: 2.,
            position: Vec3::new(8., 2., 4.),
            direction: Vec3::Y,
            velocity: Vec3::X * 99.,
            ..Default::default()
        },
    )
    .unwrap();
    let id = w.drop_item(actor, 0).unwrap();
    let drop = w.drops().next().unwrap();
    assert_eq!(drop.position, Vec3::new(8., 6., 4.));
    assert_eq!(drop.velocity, Vec3::Y * 40.);
    assert!((drop.rotation * Vec3::NEG_Z - Vec3::X).length() < 0.00001);
    assert_eq!(drop.scale, 2.);
    w.tick = 57;
    assert!(w.pickup(actor, id).is_err());
    let save = w.save();
    assert_eq!(save.schema_version, 3);
    let mut malformed = save.clone();
    malformed.drops[0].rotation = glam::Quat::from_xyzw(0., 0., 0., 0.);
    assert!(WeaponsWorld::restore(empty(), &serde_json::to_vec(&malformed).unwrap()).is_err());
    let mut legacy = serde_json::to_value(&save).unwrap();
    legacy["schema_version"] = serde_json::json!(2);
    legacy["drops"][0].as_object_mut().unwrap().remove("scale");
    legacy["drops"][0]
        .as_object_mut()
        .unwrap()
        .remove("rotation");
    legacy["actors"][0][1]["frame"]
        .as_object_mut()
        .unwrap()
        .remove("body_yaw");
    let restored = WeaponsWorld::restore(empty(), &serde_json::to_vec(&legacy).unwrap()).unwrap();
    assert_eq!(restored.drops().next().unwrap().scale, 1.);
    assert_eq!(
        restored.drops().next().unwrap().rotation,
        glam::Quat::IDENTITY
    );
    w.tick = 58;
    assert_eq!(w.pickup(actor, id).unwrap(), 0);
}

#[test]
#[ignore = "requires converted vanilla weapons pack"]
fn native_weapons_fill_remaining_stock_slots_and_reuse_dropped_tool_slot() {
    let mut w = WeaponsWorld::new(load()).unwrap();
    let actor = ActorId(1);
    w.add_actor(actor, 5).unwrap();
    for item in &CORE_TOOLS[..3] {
        w.give(actor, item).unwrap();
    }
    let items: Vec<_> = w.pack.items.keys().cloned().collect();
    assert_eq!(w.give(actor, &items[0]).unwrap(), 3);
    assert_eq!(w.give(actor, &items[1]).unwrap(), 4);
    assert!(w.give(actor, &items[2]).is_err());
    w.drop_item(actor, 1).unwrap();
    assert_eq!(w.give(actor, &items[2]).unwrap(), 1);
    w.equip(actor, Some(1)).unwrap();
    assert!(w.image_state(actor, 0).is_some());
    let restored = WeaponsWorld::restore(load(), &serde_json::to_vec(&w.save()).unwrap()).unwrap();
    assert_eq!(
        restored.actor(actor).unwrap().inventory,
        w.actor(actor).unwrap().inventory
    );
}
#[test]
fn key_hue_rule_wraps_and_rejects_grey() {
    assert!(key_matches([1.0, 0.0, 0.0], [0.5, 0.0, 0.02]));
    assert!(key_matches([1.0, 1.0, 0.0], [0.5, 0.5, 0.0]));
    assert!(!key_matches([1.0, 0.0, 0.0], [0.5, 0.5, 0.5]));
    assert!(!key_matches([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]));
    assert!(!key_matches([f32::NAN, 0.0, 0.0], [1.0, 0.0, 0.0]));
}
#[test]
fn schema_rejects_future_version() {
    let mut p = empty();
    p.schema_version = 99;
    assert!(WeaponsWorld::new(p).is_err());
}
#[test]
#[ignore = "requires converted vanilla weapons pack"]
fn inventory_drop_pickup_and_disconnect() {
    let mut w = world("GunItem");
    let mut q = Scene::default();
    run(&mut w, 30, &mut q);
    assert!(w.give(ActorId(1), &native_id("weapon", "GunItem")).is_err());
    let d = w.drop_item(ActorId(1), 0).unwrap();
    assert!(w.pickup(ActorId(1), d).is_err());
    w.add_actor(ActorId(2), 5).unwrap();
    assert_eq!(w.pickup(ActorId(2), d).unwrap(), 0);
    assert!(w.pickup(ActorId(1), d).is_err());
    w.spawn(
        &native_id("projectile", "gunProjectile"),
        ActorId(2),
        Vec3::ZERO,
        Vec3::NEG_Z,
        1.0,
    )
    .unwrap();
    w.remove_actor(ActorId(2));
    assert_eq!(w.projectiles().count(), 0);
    assert!(
        w.step(&mut q)
            .iter()
            .any(|e| matches!(e, Event::Removed { .. }))
    );
}
#[test]
#[ignore = "requires converted vanilla weapons pack"]
fn gun_is_semi_auto_and_akimbo_fires_on_release() {
    let mut w = world("GunItem");
    let mut q = Scene::default();
    w.trigger(ActorId(1), true).unwrap();
    let e = run(&mut w, 240, &mut q);
    assert_eq!(shots(&e), 1);
    w.trigger(ActorId(1), false).unwrap();
    run(&mut w, 1, &mut q);
    w.trigger(ActorId(1), true).unwrap();
    assert_eq!(shots(&run(&mut w, 60, &mut q)), 1);
    let mut w = world("AkimboGunItem");
    w.trigger(ActorId(1), true).unwrap();
    assert_eq!(shots(&run(&mut w, 120, &mut q)), 1);
    w.trigger(ActorId(1), false).unwrap();
    let e = run(&mut w, 30, &mut q);
    assert_eq!(shots(&e), 1);
    assert!(
        e.iter()
            .any(|e| matches!(e,Event::Animation{sequence,..} if sequence=="leftrecoil"))
    );
}
#[test]
#[ignore = "requires converted vanilla weapons pack"]
fn bow_auto_and_rocket_cooldown() {
    let mut q = Scene::default();
    let mut w = world("BowItem");
    w.trigger(ActorId(1), true).unwrap();
    assert_eq!(shots(&run(&mut w, 240, &mut q)), 3);
    let mut w = world("rocketLauncherItem");
    w.trigger(ActorId(1), true).unwrap();
    assert_eq!(shots(&run(&mut w, 15, &mut q)), 1);
    assert!(w.equip(ActorId(1), None).is_err());
    w.trigger(ActorId(1), false).unwrap();
    run(&mut w, 90, &mut q);
    w.trigger(ActorId(1), true).unwrap();
    assert_eq!(shots(&run(&mut w, 1, &mut q)), 1);
}
#[test]
#[ignore = "requires converted vanilla weapons pack"]
fn spear_short_charge_cancels_long_charge_releases() {
    let mut q = Scene::default();
    let mut w = world("spearItem");
    run(&mut w, 20, &mut q);
    w.trigger(ActorId(1), true).unwrap();
    run(&mut w, 30, &mut q);
    w.trigger(ActorId(1), false).unwrap();
    assert_eq!(shots(&run(&mut w, 100, &mut q)), 0);
    w.trigger(ActorId(1), true).unwrap();
    assert_eq!(shots(&run(&mut w, 90, &mut q)), 0);
    w.trigger(ActorId(1), false).unwrap();
    let e = run(&mut w, 1, &mut q);
    assert_eq!(shots(&e), 1);
    assert!(
        e.iter()
            .any(|e| matches!(e,Event::Animation{sequence,..} if sequence=="spearThrow"))
    );
}
#[test]
#[ignore = "requires converted vanilla weapons pack"]
fn sword_and_broom_short_reach_with_distinct_damage() {
    for (item, damage, impulse) in [("swordItem", 35.0, 0.0), ("pushBroomItem", 0.0, 1300.0)] {
        let mut w = world(item);
        let mut q = Scene {
            hit: Some(hit(TargetId::Actor(ActorId(2)), -2.0)),
            ..Default::default()
        };
        w.trigger(ActorId(1), true).unwrap();
        let e = run(&mut w, 120, &mut q);
        assert_eq!(
            e.iter()
                .filter(|e| matches!(e,Event::Damage{amount,..} if *amount==damage))
                .count()
                > 0,
            damage > 0.0
        );
        assert_eq!(
            e.iter()
                .filter(|e| matches!(e,Event::Impulse{impulse:v,..} if v.y==impulse))
                .count()
                > 0,
            impulse > 0.0
        );
        assert!(w.projectiles().all(|p| p.position.length() < 6.0));
    }
}
#[test]
#[ignore = "requires converted vanilla weapons pack"]
fn key_and_skis_emit_executable_host_commands() {
    let mut w = world("redKeyItem");
    let mut q = Scene {
        hit: Some(hit(TargetId::Brick(10), -3.0)),
        ..Default::default()
    };
    w.trigger(ActorId(1), true).unwrap();
    assert!(run(&mut w, 10, &mut q).iter().any(|e| matches!(
        e,
        Event::Key {
            brick: 10,
            matched: true,
            ..
        }
    )));
    let mut w = world("SkiItem");
    w.trigger(ActorId(1), true).unwrap();
    let e = run(&mut w, 65, &mut Scene::default());
    assert!(e.iter().any(|e| matches!(
        e,
        Event::StartSkis {
            mount_after_ticks: 30,
            ..
        }
    )));
    assert!(w.actor(ActorId(1)).unwrap().skiing);
    assert_eq!(w.actor(ActorId(1)).unwrap().selected, None);
}
#[test]
#[ignore = "requires converted vanilla weapons pack"]
fn horse_ray_transforms_without_nominal_damage() {
    let mut w = world("horseRayItem");
    let mut q = Scene {
        hit: Some(hit(TargetId::Actor(ActorId(2)), -2.0)),
        ..Default::default()
    };
    w.trigger(ActorId(1), true).unwrap();
    let e = run(&mut w, 60, &mut q);
    assert!(e.iter().any(|e| matches!(
        e,
        Event::HorseTransform {
            target: ActorId(2),
            dismount: true,
            ..
        }
    )));
    assert!(!e.iter().any(|e| matches!(e, Event::Damage { .. })));
}
#[test]
#[ignore = "requires converted vanilla weapons pack"]
fn explosion_radius_falloff_ignores_cover_and_intends_bricks() {
    // v20 `onExplode`: no line-of-sight test and a quadratic falloff,
    // 100 * (1 - (1.5 / 3)^2) = 75 at half the rocket's damage radius.
    let mut w = WeaponsWorld::new(load()).unwrap();
    w.spawn(
        &native_id("projectile", "rocketLauncherProjectile"),
        ActorId(1),
        Vec3::ZERO,
        Vec3::NEG_Z * 65.0,
        1.0,
    )
    .unwrap();
    let mut q = Scene {
        hit: Some(hit(TargetId::Map(1), -0.25)),
        near: vec![Nearby {
            target: TargetId::Actor(ActorId(2)),
            center: Vec3::new(1.5, 0.0, -0.25),
            distance: 1.0,
        }],
        ..Default::default()
    };
    let e = w.step(&mut q);
    assert!(
        e.iter()
            .any(|e| matches!(e,Event::Damage{amount,..}if (*amount-75.0).abs()<0.001))
    );
    assert!(
        e.iter()
            .any(|e| matches!(e, Event::BrickImpact { target: None, .. }))
    );
}
#[test]
#[ignore = "requires converted vanilla weapons pack"]
fn special_projectiles_gravity_bounce_and_cleanup() {
    for name in [
        "gravityRocketProjectile",
        "pinballProjectile",
        "pongProjectile",
        "radioWaveProjectile",
        "tankShellProjectile",
        "CannonBallProjectile",
        "clockProjectile",
    ] {
        let mut w = WeaponsWorld::new(load()).unwrap();
        let id = native_id("projectile", name);
        w.spawn(&id, ActorId(1), Vec3::ZERO, Vec3::NEG_Z * 65.0, 1.0)
            .unwrap();
        let mut q = Scene::default();
        run(&mut w, 2, &mut q);
        let p = w.projectiles().next().unwrap();
        if w.pack.projectiles[&id].gravity > 0.0 && w.pack.projectiles[&id].ballistic {
            assert!(p.velocity.y < 0.0);
        } else {
            assert_eq!(p.velocity.y, 0.0);
        }
        run(&mut w, 3700, &mut q);
        assert_eq!(w.projectiles().count(), 0);
    }
    let mut w = WeaponsWorld::new(load()).unwrap();
    w.spawn(
        &native_id("projectile", "pongProjectile"),
        ActorId(1),
        Vec3::ZERO,
        Vec3::NEG_Z * 65.0,
        1.0,
    )
    .unwrap();
    let mut q = Scene {
        hit: Some(hit(TargetId::Map(1), -0.25)),
        ..Default::default()
    };
    assert!(
        w.step(&mut q)
            .iter()
            .any(|e| matches!(e,Event::Bounced{velocity,..}if velocity.z>64.0))
    );
}
#[test]
#[ignore = "requires converted vanilla weapons pack"]
fn sports_charge_throw_consume_catch_and_dodgeball_damage() {
    for item in [
        "basketballItem",
        "dodgeballItem",
        "FootballItem",
        "soccerBallItem",
    ] {
        let mut w = world(item);
        let mut q = Scene::default();
        run(&mut w, 130, &mut q);
        w.trigger(ActorId(1), true).unwrap();
        run(&mut w, 1, &mut q);
        w.trigger(ActorId(1), true).unwrap();
        run(&mut w, 100, &mut q);
        w.trigger(ActorId(1), false).unwrap();
        let e = run(&mut w, 1, &mut q);
        assert_eq!(shots(&e), 1, "{item}");
        assert!(w.actor(ActorId(1)).unwrap().inventory[0].is_none());
    }
    let mut w = WeaponsWorld::new(load()).unwrap();
    w.add_actor(ActorId(2), 5).unwrap();
    w.spawn(
        &native_id("projectile", "footballProjectile"),
        ActorId(1),
        Vec3::ZERO,
        Vec3::NEG_Z * 40.0,
        1.0,
    )
    .unwrap();
    let mut q = Scene {
        hit: Some(hit(TargetId::Actor(ActorId(2)), -0.1)),
        ..Default::default()
    };
    assert!(w.step(&mut q).iter().any(|e| matches!(
        e,
        Event::BallCaught {
            actor: ActorId(2),
            ..
        }
    )));
    let mut w = WeaponsWorld::new(load()).unwrap();
    w.spawn(
        &native_id("projectile", "dodgeballProjectile"),
        ActorId(1),
        Vec3::ZERO,
        Vec3::NEG_Z * 30.0,
        1.0,
    )
    .unwrap();
    assert!(w.step(&mut q).iter().any(|e| matches!(
        e,
        Event::Damage {
            amount: 50000.0,
            ..
        }
    )));
}
#[test]
#[ignore = "requires converted vanilla weapons pack"]
fn malicious_projectile_and_state_inputs_are_bounded() {
    let mut p = load();
    // A zero-tick cycle between two states. A state timing out into itself
    // is the wands' timed sparkle loop, so the cycle must span two states.
    let states = &mut p
        .images
        .get_mut(&native_id("image", "gunImage"))
        .unwrap()
        .states;
    for (state, next) in [(0, 1), (1, 0)] {
        states[state].ticks = 0;
        states[state].timeout = Some(next);
    }
    let mut w = WeaponsWorld::new(p).unwrap();
    w.add_actor(ActorId(1), 5).unwrap();
    w.give(ActorId(1), &native_id("weapon", "GunItem")).unwrap();
    w.equip(ActorId(1), Some(0)).unwrap();
    assert!(w.step(&mut Scene::default()).iter().any(
        |e| matches!(e,Event::Diagnostic{message,..}if message.contains("transition budget"))
    ));
    let p = native_id("projectile", "gunProjectile");
    assert!(
        w.spawn(&p, ActorId(1), Vec3::ZERO, Vec3::splat(f32::NAN), 1.0)
            .is_err()
    );
    for _ in 0..MAX_PROJECTILES {
        w.spawn(&p, ActorId(1), Vec3::ZERO, Vec3::NEG_Z, 1.0)
            .unwrap();
    }
    assert!(
        w.spawn(&p, ActorId(1), Vec3::ZERO, Vec3::NEG_Z, 1.0)
            .is_err()
    );
}
#[test]
#[ignore = "requires converted vanilla weapons pack"]
fn pack_complete_native_models_and_hidden_variants() {
    let p = load();
    // 17 weapons plus the hammer, wrench, printer and wand images.
    assert_eq!(p.items.len(), 21);
    for n in [
        "LeftHandedGunImage",
        "basketballShootImage",
        "soccerBallStandImage",
        "horseFootballImage",
        "horseDodgeballImage",
    ] {
        assert!(p.images.contains_key(&native_id("image", n)));
    }
    for r in &p.resources {
        assert!(r.native_file.is_some(), "{} {:?}", r.path, r.diagnostics);
    }
    assert_eq!(
        p.projectiles[&native_id("projectile", "gunProjectile")].speed,
        90.0
    );
    assert_eq!(
        p.projectiles[&native_id("projectile", "spearProjectile")].lifetime_ticks,
        2400
    );
}
struct PhysicsScene {
    world: rapier3d::prelude::PhysicsWorld,
    target: TargetId,
}
impl Query for PhysicsScene {
    fn sweep(&mut self, start: Vec3, end: Vec3, _: Filter) -> Option<Hit> {
        use rapier3d::prelude::*;
        let delta = end - start;
        let len = delta.length();
        if len < 0.000001 {
            return None;
        }
        let ray = Ray::new(
            Vector::from_array(start.to_array()),
            Vector::from_array((delta / len).to_array()),
        );
        self.world
            .query_pipeline()
            .cast_ray_and_get_normal(&ray, len, true)
            .map(|(_, hit)| Hit {
                target: self.target,
                position: start + delta / len * hit.time_of_impact,
                normal: Vec3::from_array(hit.normal.to_array()),
                fraction: hit.time_of_impact / len,
                color: None,
            })
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
#[test]
#[ignore = "requires converted vanilla weapons pack"]
fn swept_projectile_hits_thin_native_brick_recipe() {
    use rapier3d::prelude::*;
    let mut physics = bri_physics::new_world();
    let recipe = bri_content::collision::CollisionBody {
        id: "test.authored.thin_brick".into(),
        parts: vec![bri_content::collision::Part::Box {
            center: [0.0, 0.0, -0.25],
            size: [1.0, 1.0, 0.01],
        }],
    };
    physics.insert(
        RigidBodyBuilder::fixed(),
        bri_physics::content::collider(&recipe).unwrap(),
    );
    physics.detect_collisions(&(), &());
    let mut q = PhysicsScene {
        world: physics,
        target: TargetId::Brick(7),
    };
    let mut w = WeaponsWorld::new(load()).unwrap();
    w.spawn(
        &native_id("projectile", "gunProjectile"),
        ActorId(1),
        Vec3::ZERO,
        Vec3::NEG_Z * 90.0,
        1.0,
    )
    .unwrap();
    let e = w.step(&mut q);
    assert_eq!(w.projectiles().count(), 0);
    assert!(e.iter().any(|e| matches!(
        e,
        Event::BrickImpact {
            target: Some(TargetId::Brick(7)),
            ..
        }
    )));
}
#[test]
#[ignore = "requires converted vanilla weapons and map-bundle-006"]
fn projectile_sweep_collides_with_authored_bedroom_interior() {
    use bri_content::scene::{Kind, Scene as NativeScene};
    use rapier3d::prelude::*;
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/map-bundle-006");
    let bundle: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("bundle.json")).unwrap()).unwrap();
    let map = &bundle["maps"][0];
    let scene: NativeScene =
        serde_json::from_slice(&std::fs::read(root.join(map["file"].as_str().unwrap())).unwrap())
            .unwrap();
    let mut physics = bri_physics::new_world();
    for node in scene
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, Kind::Interior))
    {
        let id = node.asset.as_ref().unwrap();
        let interior: bri_content::interior::Interior = serde_json::from_slice(
            &std::fs::read(root.join(bundle["assets"][id].as_str().unwrap())).unwrap(),
        )
        .unwrap();
        physics.insert(
            RigidBodyBuilder::fixed(),
            bri_physics::content::interior_collider(
                &interior.details[0],
                glam::Mat4::from_cols_array(&node.transform),
            )
            .unwrap(),
        );
    }
    physics.detect_collisions(&(), &());
    let spawn = scene
        .nodes
        .iter()
        .find(|n| matches!(n.kind, Kind::Spawn))
        .unwrap();
    let start = Vec3::new(
        spawn.transform[12],
        spawn.transform[13] + 5.0,
        spawn.transform[14],
    );
    let mut q = PhysicsScene {
        world: physics,
        target: TargetId::Map(1),
    };
    let floor = q
        .sweep(
            start,
            start - Vec3::Y * 50.0,
            Filter {
                projectile_age_ticks: None,
                source: ActorId(1),
                players: false,
                world_only: true,
            },
        )
        .unwrap();
    let mut w = WeaponsWorld::new(load()).unwrap();
    w.spawn(
        &native_id("projectile", "gunProjectile"),
        ActorId(1),
        floor.position + Vec3::Y * 0.3,
        -Vec3::Y * 90.0,
        1.0,
    )
    .unwrap();
    assert!(
        w.step(&mut q)
            .iter()
            .any(|e| matches!(e, Event::Removed { .. }))
    );
}
#[test]
#[ignore = "requires converted vanilla weapons pack"]
fn sports_actions_tackle_steal_and_touchdown() {
    let mut q = Scene::default();
    let mut w = WeaponsWorld::new(load()).unwrap();
    w.add_actor(ActorId(1), 5).unwrap();
    w.add_actor(ActorId(2), 5).unwrap();
    w.use_sport_item(ActorId(1), &native_id("weapon", "FootballItem"))
        .unwrap();
    assert!(w.touchdown(ActorId(1), 9).unwrap());
    run(&mut w, 40, &mut q);
    assert!(w.tackle(ActorId(1), ActorId(2), 1234, &q).unwrap());
    assert!(!w.tackle(ActorId(1), ActorId(2), 1234, &q).unwrap());
    let e = w.step(&mut q);
    assert!(
        e.iter()
            .any(|e| matches!(e, Event::Tumble { ticks: 360, .. }))
    );
    assert_eq!(w.projectiles().count(), 1);
    let mut w = WeaponsWorld::new(load()).unwrap();
    w.add_actor(ActorId(1), 5).unwrap();
    w.use_sport_item(ActorId(1), &native_id("weapon", "soccerBallItem"))
        .unwrap();
    run(&mut w, 40, &mut q);
    w.sport_action(ActorId(1), SportAction::SoccerPop).unwrap();
    assert!(w.projectiles().next().unwrap().velocity.y >= 9.0);
    let mut w = WeaponsWorld::new(load()).unwrap();
    w.add_actor(ActorId(1), 5).unwrap();
    w.add_actor(ActorId(2), 5).unwrap();
    w.use_sport_item(ActorId(2), &native_id("weapon", "basketballItem"))
        .unwrap();
    q.hit = Some(hit(TargetId::Actor(ActorId(2)), -2.0));
    assert!(!w.steal_basketball(ActorId(1), 1, &mut q).unwrap());
    assert!(w.steal_basketball(ActorId(1), 0, &mut q).unwrap());
    assert_eq!(w.projectiles().count(), 1);
}
#[test]
#[ignore = "requires converted vanilla weapons pack"]
fn save_restore_preserves_authoritative_trajectory_and_image_clock() {
    let mut w = world("BowItem");
    w.trigger(ActorId(1), true).unwrap();
    let mut q = Scene::default();
    run(&mut w, 150, &mut q);
    let bytes = serde_json::to_vec(&w.save()).unwrap();
    let mut restored = WeaponsWorld::restore(load(), &bytes).unwrap();
    let a = run(&mut w, 240, &mut q);
    let b = run(&mut restored, 240, &mut q);
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&b).unwrap()
    );
    assert_eq!(
        serde_json::to_string(&w.save()).unwrap(),
        serde_json::to_string(&restored.save()).unwrap()
    );
    let mut save = w.save();
    save.next_id = 0;
    assert!(WeaponsWorld::restore(load(), &serde_json::to_vec(&save).unwrap()).is_err());
}
#[test]
fn projectile_event_bounce_and_redirect_preserve_source_limits() {
    let impact = ProjectileContact {
        projectile: 1,
        definition: "test".into(),
        source: ActorId(1),
        target: TargetId::Brick(1),
        position: Vec3::ZERO,
        velocity: Vec3::NEG_Z * 90.0,
        normal: Vec3::Z,
        scale: 1.0,
        paint: None,
    };
    assert_eq!(
        redirected_velocity(&impact, ContactResponse::Bounce(2.0)).unwrap(),
        Vec3::Z * 180.0
    );
    assert_eq!(
        redirected_velocity(
            &impact,
            ContactResponse::Redirect {
                vector: Vec3::Y * 1000.0,
                normalized: false
            }
        )
        .unwrap(),
        Vec3::Y * 200.0
    );
    assert_eq!(
        redirected_velocity(
            &impact,
            ContactResponse::Redirect {
                vector: Vec3::X * 8.0,
                normalized: true
            }
        )
        .unwrap(),
        Vec3::X * 90.0
    );
    assert!(redirected_velocity(&impact, ContactResponse::Bounce(f32::NAN)).is_err());
}

#[test]
#[ignore = "requires converted vanilla weapons pack"]
fn synchronous_brick_output_redirects_before_explosion() {
    let mut w = WeaponsWorld::new(load()).unwrap();
    w.spawn(
        &native_id("projectile", "rocketLauncherProjectile"),
        ActorId(1),
        Vec3::ZERO,
        Vec3::NEG_Z * 65.0,
        1.0,
    )
    .unwrap();
    let mut q = Scene {
        hit: Some(hit(TargetId::Brick(3), -0.25)),
        response: Some(ContactResponse::Redirect {
            vector: Vec3::X,
            normalized: true,
        }),
        ..Default::default()
    };
    let events = w.step(&mut q);
    assert_eq!(w.projectiles().count(), 1);
    assert_eq!(w.projectiles().next().unwrap().velocity, Vec3::X * 65.0);
    assert!(!events.iter().any(|e| matches!(
        e,
        Event::Damage { .. } | Event::Removed { .. } | Event::BrickImpact { .. }
    )));
    assert!(events.iter().any(|e| matches!(e, Event::Bounced { .. })));
}
#[test]
fn spray_paint_effects_carry_the_palette_index() {
    assert_eq!(
        paint_effect("bluePaintEmitter", Some(12)),
        "color12PaintEmitter"
    );
    assert_eq!(
        paint_effect("bluePaintExplosion", None),
        "bluePaintExplosion"
    );
    assert_eq!(paint_effect("gunExplosion", Some(3)), "gunExplosion");
    assert_eq!(
        paint_effect_base("color12PaintEmitter"),
        Some((12, "bluePaintEmitter".into()))
    );
    assert_eq!(
        paint_effect_base("color0PaintExplosion"),
        Some((0, "bluePaintExplosion".into()))
    );
    for name in [
        "colorPaintEmitter",
        "color999PaintEmitter",
        "color3Spray",
        "gunExplosion",
    ] {
        assert_eq!(paint_effect_base(name), None, "{name}");
    }
}
