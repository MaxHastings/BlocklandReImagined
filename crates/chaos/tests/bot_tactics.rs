//! Actual bot controls select unfamiliar authored weapons and damage an enemy.
//! No brain target, objective or bot movement is injected by these fixtures.
use bri_chaos::fixture;
use bri_minigames::Settings;
use bri_sim::player::MoveInput;
use bri_sim::session::{Command, MiniGameRequest, Session, ToolCatalog};
use bri_weapons::{Pack, testing};
use bri_world::{Brick, ContentRef, VehicleSpawn, World, build::SavedBuild};
use glam::Vec3;
use std::collections::BTreeSet;

const A: &str = "tactics:weapon/orbit";
const B: &str = "tactics:weapon/meridian";
const C: &str = "tactics:weapon/zenith";
const D: &str = "tactics:weapon/tributary";

fn alias(pack: &mut Pack, item: &str, image: &str, projectile: &str, new: &str) {
    let mut i = pack.items[item].clone();
    let mut image = pack.images[image].clone();
    let mut p = pack.projectiles[projectile].clone();
    let suffix = new.rsplit('/').next().unwrap();
    let image_id = format!("tactics:image/{suffix}");
    let projectile_id = format!("tactics:projectile/{suffix}");
    i.id = new.into();
    i.name = suffix.into();
    i.image = image_id.clone();
    image.id = image_id.clone();
    image.name = suffix.into();
    image.projectile = Some(projectile_id.clone());
    p.id = projectile_id.clone();
    p.name = suffix.into();
    pack.items.insert(new.into(), i);
    pack.images.insert(image_id, image);
    pack.projectiles.insert(projectile_id, p);
}
fn pack() -> Pack {
    let mut p = testing::pack();
    alias(
        &mut p,
        testing::GUN_ITEM,
        testing::GUN_IMAGE,
        testing::GUN_PROJECTILE,
        A,
    );
    alias(
        &mut p,
        testing::ROCKET_ITEM,
        testing::ROCKET_IMAGE,
        testing::ROCKET_PROJECTILE,
        B,
    );
    alias(
        &mut p,
        testing::SPEAR_ITEM,
        testing::SPEAR_IMAGE,
        testing::SPEAR_PROJECTILE,
        C,
    );
    alias(
        &mut p,
        testing::SWORD_ITEM,
        testing::SWORD_IMAGE,
        testing::SWORD_PROJECTILE,
        D,
    );
    // Authored test values give multiple observable rounds before a kill.
    p.projectiles
        .get_mut("tactics:projectile/orbit")
        .unwrap()
        .damage = 10.0;
    p
}
fn game(mut pack: Pack, loadout: &[&str], distance: f32) -> (Session, u64, u64, u64) {
    let mut s = fixture::synthetic().unwrap().session;
    // A generous stationary test bot can acquire, charge and fire while the
    // actual aim model still retains its reaction and normal turn limits.
    for image in pack.images.values_mut() {
        if image.id.starts_with("tactics:") {
            image.casing.clear();
        }
    }
    let items = pack.items.keys().cloned().collect();
    s.set_weapon_pack(pack).unwrap();
    s.set_tool_catalog(ToolCatalog {
        items,
        vehicles: [fixture::BOT.into()].into(),
        vehicle_bricks: [fixture::PLATE.into()].into(),
        ..Default::default()
    })
    .unwrap();
    s.set_spawn_points(vec![Vec3::new(-25.0, 0.05, 35.0)])
        .unwrap();
    let human = s
        .join("Observer".into(), Vec3::new(-25.0, 0.05, 35.0), true)
        .unwrap();
    let mut world = World::new(
        "Unfamiliar equipment".into(),
        "chaos/map".into(),
        vec![[1.0; 4]],
    );
    let mut brick = Brick::new(
        ContentRef::Resolved(fixture::PLATE.into()),
        [-24.75, 0.1, 35.25 - distance],
        human,
    );
    brick.vehicle = Some(Box::new(VehicleSpawn {
        vehicle: ContentRef::Resolved(fixture::BOT.into()),
        recolor: false,
    }));
    world.bricks.insert(1, brick);
    world.next_brick_id = 2;
    s.command(
        human,
        100,
        Command::LoadBuild {
            build: Box::new(SavedBuild::new(world)),
            ownership: false,
        },
    )
    .unwrap();
    let mut seq = 1 << 40;
    ticks(&mut s, human, &mut seq, 10);
    let bot = *s.names().keys().find(|o| s.is_bot(**o)).unwrap();
    let mut slots: [Option<String>; 5] = [None, None, None, None, None];
    for (slot, item) in loadout.iter().enumerate() {
        slots[slot] = Some((*item).into());
    }
    s.command(
        human,
        101,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: Settings {
                loadout: slots,
                ..Default::default()
            },
        }),
    )
    .unwrap();
    (s, human, bot, seq)
}
fn ticks(s: &mut Session, human: u64, seq: &mut u64, n: usize) {
    for _ in 0..n {
        *seq += 1;
        s.movement(human, *seq, MoveInput::default()).unwrap();
        s.step().unwrap();
    }
}
fn feet(s: &Session, owner: u64) -> Vec3 {
    s.snapshot()
        .players
        .iter()
        .find(|p| p.owner == owner)
        .unwrap()
        .feet
        .into()
}
fn fight(
    s: &mut Session,
    human: u64,
    bot: u64,
    seq: &mut u64,
    ticks_to_run: usize,
) -> (BTreeSet<String>, BTreeSet<u64>, usize) {
    let mut kinds = BTreeSet::new();
    let mut ids = BTreeSet::new();
    let mut taps = 0;
    for _ in 0..ticks_to_run {
        ticks(s, human, seq, 1);
        for p in s.weapon_view().fired().filter(|p| p.source.0 == bot) {
            kinds.insert(p.definition.clone());
            ids.insert(p.id);
        }
        taps += 1;
        if s.vitals()[&human].health < 100.0 {
            break;
        }
    }
    (kinds, ids, taps)
}

#[test]
fn unfamiliar_direct_splash_charge_and_melee_families_fire_and_hit_through_real_controls() {
    for (item, distance) in [(A, 20.0), (B, 20.0), (C, 20.0), (D, 7.0)] {
        let (mut s, human, bot, mut seq) = game(pack(), &[item], distance);
        let (kinds, ids, ticks) = fight(&mut s, human, bot, &mut seq, 120 * 25);
        assert!(
            !ids.is_empty(),
            "{item} never launched, thoughts={:?}",
            s.bot_thoughts()
        );
        assert!(
            s.vitals()[&human].health < 100.0,
            "{item} never hit; kinds={kinds:?} thoughts={:?}",
            s.bot_thoughts()
        );
        assert!(kinds.iter().all(|k| k.starts_with("tactics:projectile/")));
        eprintln!(
            "{item}: first damage after {ticks} ticks; {} distinct launches",
            ids.len()
        );
    }
}

#[test]
fn nearby_blast_risk_selects_the_safe_second_inventory_slot() {
    let (mut s, human, bot, mut seq) = game(pack(), &[B, A], 4.0);
    let mut safe_slot_while_close = false;
    let mut seen_ids = BTreeSet::new();
    for _ in 0..120 * 15 {
        ticks(&mut s, human, &mut seq, 1);
        let view = s.weapon_view();
        if feet(&s, bot).distance(feet(&s, human)) < 7.0 {
            safe_slot_while_close |= view.images.get(&bot).is_some_and(|images| {
                images
                    .iter()
                    .any(|i| i.hand == 0 && i.image == "tactics:image/orbit")
            });
        }
        for p in view.fired().filter(|p| p.source.0 == bot) {
            if !seen_ids.insert(p.id) {
                continue;
            }
            if p.definition == "tactics:projectile/meridian" {
                let clearance = p.origin.distance(feet(&s, human));
                assert!(
                    clearance > 7.0,
                    "rocket launched before backing out of its blast envelope: {clearance}"
                );
            }
        }
        if s.vitals()[&human].health < 100.0 {
            break;
        }
    }
    assert!(
        safe_slot_while_close,
        "did not equip the useful second slot while the first weapon's blast was unsafe"
    );
    eprintln!(
        "close blast: human={:?} bot={:?} health={} observed_live_launches={} images={:?}",
        feet(&s, human),
        feet(&s, bot),
        s.vitals()[&human].health,
        seen_ids.len(),
        s.weapon_view().images.get(&bot)
    );
    assert_eq!(
        s.vitals()[&human].health,
        90.0,
        "the first real damage must be the useful direct weapon's authored10, even if its projectile expires within its spawn tick"
    );
    assert!(s.vitals()[&human].health < 100.0);
}

#[test]
fn a_long_flight_weapon_closes_distance_instead_of_waiting_forever_on_a_budget() {
    let mut p = pack();
    let slow = p.projectiles.get_mut("tactics:projectile/orbit").unwrap();
    slow.speed = 5.0;
    slow.lifetime_ticks = 2400;
    let (mut s, human, bot, mut seq) = game(p, &[A], 30.0);
    let initial = feet(&s, bot).distance(feet(&s, human));
    let mut closest = initial;
    let mut fired = BTreeSet::new();
    for _ in 0..120 * 20 {
        ticks(&mut s, human, &mut seq, 1);
        closest = closest.min(feet(&s, bot).distance(feet(&s, human)));
        fired.extend(
            s.weapon_view()
                .fired()
                .filter(|p| p.source.0 == bot)
                .map(|p| p.id),
        );
        if s.vitals()[&human].health < 100.0 {
            break;
        }
    }
    eprintln!(
        "long flight: initial={initial} closest={closest} launches={} health={}",
        fired.len(),
        s.vitals()[&human].health
    );
    assert!(
        closest < initial - 3.0,
        "out-of-envelope bot did not close: initial={initial} closest={closest} thoughts={:?}",
        s.bot_thoughts()
    );
    assert!(
        !fired.is_empty(),
        "never fired after approaching the supported flight envelope"
    );
    assert!(
        s.vitals()[&human].health < 100.0,
        "approached and fired but did not hit the ordinary target"
    );
}

#[test]
fn a_depleted_stored_magazine_switches_to_the_usable_undrawn_slot() {
    const NEXT: &str = "tactics:weapon/borealis";
    let mut p = pack();
    alias(
        &mut p,
        testing::GUN_ITEM,
        testing::GUN_IMAGE,
        testing::GUN_PROJECTILE,
        NEXT,
    );
    p.projectiles
        .get_mut("tactics:projectile/borealis")
        .unwrap()
        .damage = 10.0;
    for image in ["tactics:image/orbit", "tactics:image/borealis"] {
        // These authored ray weapons isolate ammunition/control recovery from
        // a one-round projectile miss while preserving ordinary turn/error.
        p.images.get_mut(image).unwrap().shot = Some(
            serde_json::from_str(r#"{"projectiles":1,"hitscan":{"range":100,"from_eye":true}}"#)
                .unwrap(),
        );
        p.images.get_mut(image).unwrap().magazine = Some(
            serde_json::from_str(
                r#"{"size":1,"ammo":"one_shared_reserve","reload_ticks":24,"reserve":0}"#,
            )
            .unwrap(),
        );
    }
    let (mut s, human, bot, mut seq) = game(p, &[A, NEXT], 20.0);
    let protected_until = s.vitals()[&human].spawn_tick + 300;
    let mut held = BTreeSet::new();
    let mut traces = Vec::new();
    let mut states = BTreeSet::new();
    for _ in 0..120 * 25 {
        ticks(&mut s, human, &mut seq, 1);
        if let Some(images) = s.weapon_view().images.get(&bot) {
            states.extend(
                images
                    .iter()
                    .filter(|i| i.hand == 0)
                    .map(|i| (i.image.clone(), i.state.clone())),
            );
            held.extend(
                images
                    .iter()
                    .filter(|i| i.hand == 0)
                    .map(|i| i.image.clone()),
            );
        }
        for cue in s.take_cues() {
            if matches!(cue.kind, bri_sim::presentation::CueKind::Tracer { actor, .. } if actor == bot)
            {
                traces.push((cue.tick, cue.position, feet(&s, bot), feet(&s, human)));
            }
        }
        if s.vitals()[&human].health <= 80.0 {
            break;
        }
    }
    assert!(
        held.contains("tactics:image/orbit"),
        "first available slot was never equipped"
    );
    assert!(
        held.contains("tactics:image/borealis"),
        "depleted first slot blocked usable new equipment: {:?}",
        s.bot_thoughts()
    );
    assert_eq!(
        s.vitals()[&human].health,
        80.0,
        "both actual one-round magazines must deliver damage without a fabricated refill: traces={traces:?} states={states:?} thoughts={:?}",
        s.bot_thoughts()
    );
    assert_eq!(traces.len(), 2, "one shot from each finite magazine");
    assert!(
        traces.iter().all(|(tick, ..)| *tick >= protected_until),
        "finite rounds were spent against canonical spawn immunity: {traces:?}"
    );
    // Remaining inventory must not be repeatedly redrawn to refill its empty
    // stored magazine or the shared exhausted reserve.
    ticks(&mut s, human, &mut seq, 120 * 3);
    assert_eq!(s.vitals()[&human].health, 80.0);
    assert!(!s.take_cues().iter().any(|cue| {
        matches!(cue.kind, bri_sim::presentation::CueKind::Tracer { actor, .. } if actor == bot)
    }));
}
