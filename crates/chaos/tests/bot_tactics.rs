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
fn game(pack: Pack, loadout: &[&str], distance: f32) -> (Session, u64, u64, u64) {
    game_scene(
        pack,
        loadout,
        Vec3::new(-25.0, 0.05, 35.0),
        [-24.75, 0.1, 35.25 - distance],
        vec![],
    )
}
fn game_scene(
    pack: Pack,
    loadout: &[&str],
    spawn: Vec3,
    bot_spawn: [f32; 3],
    geometry: Vec<Brick>,
) -> (Session, u64, u64, u64) {
    game_scene_kind(pack, loadout, spawn, bot_spawn, geometry, None)
}
fn game_scene_kind(
    pack: Pack,
    loadout: &[&str],
    spawn: Vec3,
    bot_spawn: [f32; 3],
    geometry: Vec<Brick>,
    kind: Option<bri_sim::bot_kind::BotKind>,
) -> (Session, u64, u64, u64) {
    game_scene_kind_with_packages(pack, loadout, spawn, bot_spawn, geometry, kind, None)
}

fn game_scene_kind_with_packages(
    mut pack: Pack,
    loadout: &[&str],
    spawn: Vec3,
    bot_spawn: [f32; 3],
    geometry: Vec<Brick>,
    kind: Option<bri_sim::bot_kind::BotKind>,
    catalog: Option<std::sync::Arc<bri_package_runtime::Catalog>>,
) -> (Session, u64, u64, u64) {
    let mut s = fixture::synthetic().unwrap().session;
    if let Some(kind) = kind {
        s.set_bot_kinds(vec![kind]).unwrap();
    }
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
    if let Some(catalog) = catalog {
        s.install_packages(catalog, None).unwrap();
    }
    s.set_spawn_points(vec![spawn]).unwrap();
    let human = s.join("Observer".into(), spawn, true).unwrap();
    let mut world = World::new(
        "Unfamiliar equipment".into(),
        "chaos/map".into(),
        vec![[1.0; 4]],
    );
    let mut brick = Brick::new(
        ContentRef::Resolved(fixture::PLATE.into()),
        bot_spawn,
        human,
    );
    brick.vehicle = Some(Box::new(VehicleSpawn {
        vehicle: ContentRef::Resolved(fixture::BOT.into()),
        recolor: false,
        team: None,
    }));
    world.bricks.insert(1, brick);
    world.next_brick_id = 2;
    for mut brick in geometry {
        brick.owner = human;
        world.bricks.insert(world.next_brick_id, brick);
        world.next_brick_id += 1;
    }
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

fn charged_scene() -> (Session, u64, u64, u64) {
    charged_scene_with_release_chain(false)
}

fn charged_scene_with_release_chain(indirect: bool) -> (Session, u64, u64, u64) {
    let mut p = pack();
    let image = p.images.get_mut("tactics:image/zenith").unwrap();
    for state in &mut image.states {
        if state.script.eq_ignore_ascii_case("oncharge") {
            state.ticks = 84;
        } else if state.script.eq_ignore_ascii_case("onfire") {
            state.ticks = 60;
        }
    }
    if indirect {
        let armed = image
            .states
            .iter()
            .position(|s| image.fires_on_release(s))
            .unwrap();
        let relay = image.states.len();
        let mut state = image.states[armed].clone();
        state.name = "ReleaseRelay".into();
        state.ticks = 0;
        state.wait = false;
        image.states.push(state);
        image.states[armed].up = Some(relay);
    }
    p.projectiles
        .get_mut("tactics:projectile/zenith")
        .unwrap()
        .damage = 10.0;
    game_scene_kind(
        p,
        &[C],
        Vec3::new(-25.0, 0.05, 35.0),
        [-24.75, 0.1, 47.25],
        vec![],
        Some(bri_sim::bot_kind::BotKind {
            id: fixture::BOT.into(),
            name: "Patient unfamiliar thrower".into(),
            turn_degrees: 10.0,
            aim_error_degrees: 0.0,
            reaction_seconds: 0.05,
            ..Default::default()
        }),
    )
}

fn mounted_hand_state(s: &Session, bot: u64) -> Option<(String, String)> {
    s.weapon_view()
        .images
        .get(&bot)?
        .iter()
        .find(|i| i.hand == 0)
        .map(|i| (i.image.clone(), i.state.clone()))
}

fn wait_for_charge(s: &mut Session, human: u64, bot: u64, seq: &mut u64) {
    for _ in 0..120 * 25 {
        ticks(s, human, seq, 1);
        if mounted_hand_state(s, bot).is_some_and(|(_, state)| state == "Charge") {
            return;
        }
    }
    panic!("no real wind-up: {:?}", s.bot_thoughts());
}

#[test]
fn a_moving_target_keeps_native_windup_until_a_valid_throw() {
    let (mut s, human, bot, mut seq) = charged_scene();
    wait_for_charge(&mut s, human, bot, &mut seq);
    let start = feet(&s, human);
    let mut armed_ticks = 0;
    let mut fired = BTreeSet::new();
    let mut saw_fire = false;
    let mut trace = Vec::new();
    for tick in 0..120 * 20 {
        seq += 1;
        s.movement(
            human,
            seq,
            MoveInput {
                right: if tick < 120 { 1.0 } else { 0.0 },
                ..Default::default()
            },
        )
        .unwrap();
        s.step().unwrap();
        let (_, state) = mounted_hand_state(&s, bot).expect("charged image remains mounted");
        if trace.last().is_none_or(|(_, before)| before != &state) && trace.len() < 16 {
            trace.push((tick, state.clone()));
        }
        for p in s.weapon_view().fired().filter(|p| p.source.0 == bot) {
            fired.insert(p.id);
        }
        if state == "Fire" {
            saw_fire = true;
        }
        if !saw_fire {
            assert!(
                matches!(state.as_str(), "Charge" | "Armed"),
                "moving-target tracking restarted the native wind-up: {trace:?} thoughts={:?}",
                s.bot_thoughts()
            );
            armed_ticks += usize::from(state == "Armed");
        }
        if s.vitals()[&human].health < 100.0 {
            break;
        }
    }
    assert!(feet(&s, human).distance(start) > 2.0, "target really moved");
    assert!(
        armed_ticks >= 12,
        "no meaningful native held wait: {trace:?}"
    );
    assert!(saw_fire && !fired.is_empty(), "no actual throw: {trace:?}");
    assert!(
        s.vitals()[&human].health < 100.0,
        "no real delivery: {trace:?}"
    );
    eprintln!(
        "moving native throw: trace={trace:?} held_armed_ticks={armed_ticks} actual_observed_projectile_ids={fired:?}"
    );
}

#[test]
fn replacing_the_charged_equipment_cancels_without_throwing() {
    let (mut s, human, bot, mut seq) = charged_scene();
    wait_for_charge(&mut s, human, bot, &mut seq);
    s.command(
        human,
        102,
        Command::MiniGame(MiniGameRequest::Configure {
            settings: Settings {
                loadout: [Some(A.into()), None, None, None, None],
                ..Default::default()
            },
        }),
    )
    .unwrap();
    let mut saw_replacement = false;
    for _ in 0..120 * 15 {
        ticks(&mut s, human, &mut seq, 1);
        saw_replacement |=
            mounted_hand_state(&s, bot).is_some_and(|(image, _)| image == "tactics:image/orbit");
        assert!(
            s.weapon_view()
                .fired()
                .all(|p| { p.source.0 != bot || p.definition != "tactics:projectile/zenith" }),
            "replacing a real charged tool released its old attack"
        );
        if s.vitals()[&human].health < 100.0 {
            break;
        }
    }
    assert!(saw_replacement, "replacement was actually equipped");
    assert!(
        s.vitals()[&human].health < 100.0,
        "normal attack control did not recover"
    );
}

#[test]
fn disconnecting_the_charge_target_cancels_without_a_stale_throw() {
    let (mut s, human, bot, mut seq) = charged_scene();
    wait_for_charge(&mut s, human, bot, &mut seq);
    s.disconnect(human).unwrap();
    for _ in 0..120 * 4 {
        s.step().unwrap();
        assert!(
            s.weapon_view()
                .fired()
                .all(|p| { p.source.0 != bot || p.definition != "tactics:projectile/zenith" }),
            "disconnected participant's held attack launched"
        );
        assert!(s.bot_thoughts().iter().all(|t| t.visible != Some(human)));
    }
    assert!(
        mounted_hand_state(&s, bot).is_none_or(|(_, state)| state != "Charge" && state != "Armed")
    );
}

#[test]
fn a_queued_release_cannot_fire_while_native_charge_admission_waits() {
    let (mut s, human, bot, mut seq) = charged_scene();
    wait_for_charge(&mut s, human, bot, &mut seq);
    let mut armed = false;
    for _ in 0..120 * 3 {
        ticks(&mut s, human, &mut seq, 1);
        if mounted_hand_state(&s, bot).is_some_and(|(_, state)| state == "Armed") {
            armed = true;
            break;
        }
    }
    assert!(armed, "actual native charge never reached Armed");
    assert!(
        s.simulation().state().tick - s.spawn_tick(human).unwrap() < 300,
        "this admission wait requires the real never-fired target's initial protection"
    );
    // Ordinary queued trigger-up can arrive independently of the brain's
    // current hold. The actual post-movement admission must override it.
    s.command(bot, 1 << 48, Command::WeaponTrigger { down: false })
        .unwrap();
    ticks(&mut s, human, &mut seq, 1);
    assert!(mounted_hand_state(&s, bot).is_some_and(|(_, state)| state == "Armed"));
    assert!(s.weapon_view().fired().all(|p| p.source.0 != bot));
}

#[test]
fn an_indirect_queued_release_still_checks_the_actual_shot_direction() {
    let (mut s, human, bot, mut seq) = charged_scene_with_release_chain(true);
    wait_for_charge(&mut s, human, bot, &mut seq);
    // The relay graph keeps the brain's direct-release cadence held, so the
    // ordinary external release below tests the actual-frame safety boundary.
    while s.simulation().state().tick - s.spawn_tick(human).unwrap() < 360 {
        ticks(&mut s, human, &mut seq, 1);
    }
    assert!(mounted_hand_state(&s, bot).is_some_and(|(_, state)| state == "Armed"));
    let before = feet(&s, human);
    for _ in 0..60 {
        seq += 1;
        s.movement(
            human,
            seq,
            MoveInput {
                right: 1.0,
                ..Default::default()
            },
        )
        .unwrap();
        s.step().unwrap();
    }
    assert!(feet(&s, human).distance(before) > 2.0);
    assert!(
        s.bot_thoughts()
            .iter()
            .any(|t| t.bot == bot && t.visible == Some(human))
    );
    s.command(bot, 1 << 48, Command::WeaponTrigger { down: false })
        .unwrap();
    s.step().unwrap();
    assert!(mounted_hand_state(&s, bot).is_some_and(|(_, state)| state == "Armed"));
    assert!(s.weapon_view().fired().all(|p| p.source.0 != bot));
    assert_eq!(s.vitals()[&human].health, 100.0);
}

#[test]
fn an_already_released_charge_cancels_when_its_target_disconnects() {
    let (mut s, human, bot, mut seq) = charged_scene();
    wait_for_charge(&mut s, human, bot, &mut seq);
    for _ in 0..120 * 3 {
        if mounted_hand_state(&s, bot).is_some_and(|(_, state)| state == "Armed") {
            break;
        }
        ticks(&mut s, human, &mut seq, 1);
    }
    assert!(mounted_hand_state(&s, bot).is_some_and(|(_, state)| state == "Armed"));
    // Trusted host release changes the native button immediately, before
    // weapon advancement. Participant removal must cancel that pending shot.
    s.release_trigger(bot).unwrap();
    s.disconnect(human).unwrap();
    for _ in 0..120 * 4 {
        s.step().unwrap();
        assert!(s.weapon_view().fired().all(|p| p.source.0 != bot));
    }
    assert!(
        mounted_hand_state(&s, bot).is_none_or(|(_, state)| state != "Charge" && state != "Armed")
    );
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
fn a_close_ranged_flyer_escapes_a_low_roof_and_delivers_a_safe_blast() {
    let geometry = vec![
        Brick::new(
            ContentRef::Resolved(fixture::TALL.into()),
            [-24.75, 1.5, 35.25],
            1,
        ),
        Brick::new(
            ContentRef::Resolved(fixture::BASEPLATE.into()),
            [-24.75, 6.3, 31.25],
            1,
        ),
    ];
    let (mut s, human, bot, mut seq) = game_scene(
        pack(),
        &[B],
        Vec3::new(-24.75, 3.05, 35.25),
        [-24.75, 0.1, 31.25],
        geometry,
    );
    let mut saw_close_fly = false;
    let mut saw_released_lift = false;
    let mut escaped = false;
    let mut observed = BTreeSet::new();
    for _ in 0..120 * 25 {
        ticks(&mut s, human, &mut seq, 1);
        let distance = feet(&s, bot).distance(feet(&s, human));
        let flying = s
            .bot_thoughts()
            .iter()
            .any(|t| t.bot == bot && t.behaviour == "fly");
        if flying && distance < 7.0 {
            saw_close_fly = true;
            saw_released_lift |= s
                .snapshot()
                .players
                .iter()
                .any(|p| p.owner == bot && !p.jetting);
        }
        escaped |= saw_close_fly && distance >= 7.0;
        for p in s.weapon_view().fired().filter(|p| p.source.0 == bot) {
            if observed.insert(p.id) {
                assert!(
                    p.origin.distance(feet(&s, human)) > 7.0,
                    "unsafe blast during flight recovery"
                );
            }
        }
        if s.vitals()[&human].health < 100.0 {
            break;
        }
    }
    assert!(saw_close_fly, "authored scene never exercised close Fly");
    assert!(saw_released_lift, "kept jetting in close blast range");
    assert!(escaped, "never regained blast clearance");
    assert!(
        s.vitals()[&human].health < 100.0,
        "no actual delivery after recovery: {:?}",
        s.bot_thoughts()
    );
    eprintln!(
        "ranged flight recovery: human={:?} bot={:?} health={} observed_live_projectile_ids={}",
        feet(&s, human),
        feet(&s, bot),
        s.vitals()[&human].health,
        observed.len(),
    );
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

#[test]
fn gaining_an_alternative_weapon_preserves_the_actual_native_hold_until_its_throw() {
    use bri_package::packages::{PackageEntry, PackageSet, Side};
    let mut weapons = pack();
    let hold = Pack::from_json(include_bytes!(
        "../../../packages/showcase/gravity-gun-tool/assets/weapons.json"
    ))
    .unwrap();
    weapons.items.extend(hold.items);
    weapons.images.extend(hold.images);
    let tool = "gravity-gun-tool:weapon/gravitygun";
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/showcase");
    let packages = [
        ("gravity-gun", Side::Server),
        ("gravity-gun-tool", Side::Shared),
    ]
    .into_iter()
    .map(|(id, side)| PackageEntry {
        id: id.into(),
        version: "1.0.0".into(),
        side,
        dir: id.into(),
        role: None,
    })
    .collect();
    let catalog = bri_package_runtime::Catalog::load(
        &root,
        &PackageSet {
            schema_version: 1,
            packages,
        },
        true,
    )
    .unwrap();
    let (mut s, human, bot, mut seq) = game_scene_kind_with_packages(
        weapons,
        &[tool],
        Vec3::new(-25.0, 0.05, 35.0),
        [-24.75, 0.1, 23.25],
        vec![],
        None,
        Some(std::sync::Arc::new(catalog)),
    );
    let target = bri_package_runtime::ops::ObjectRef::Player(human);
    for _ in 0..120 * 20 {
        ticks(&mut s, human, &mut seq, 1);
        if s.held_by(bot) == Some(target) {
            break;
        }
    }
    assert_eq!(
        s.held_by(bot),
        Some(target),
        "the real trigger must catch the actual enemy: {:?}",
        s.bot_thoughts()
    );
    let caught = s.simulation().state().tick;
    // A real inventory mutation makes a supported alternative available while
    // the normal package holds the human. No brain/hold state is injected.
    s.give_tool(bot, A, false).unwrap();
    let mut carried = false;
    let mut released = false;
    let mut fastest = 0.0_f32;
    for _ in 0..120 * 9 {
        ticks(&mut s, human, &mut seq, 1);
        let thought = s.bot_thoughts().into_iter().find(|b| b.bot == bot).unwrap();
        carried |= thought.behaviour == "carry";
        let inventory = &s.tool_inventories()[&bot];
        let selected = inventory
            .selected
            .and_then(|slot| inventory.slots[slot].as_deref());
        if s.held_by(bot) == Some(target) {
            assert_eq!(
                selected,
                Some(tool),
                "new equipment interrupted an observed live grip at tick{}: {thought:?}",
                s.simulation().state().tick
            );
        }
        fastest = fastest.max(
            Vec3::from(
                s.snapshot()
                    .players
                    .iter()
                    .find(|p| p.owner == human)
                    .unwrap()
                    .velocity,
            )
            .length(),
        );
        if mounted_hand_state(&s, bot).is_some_and(|(_, state)| state == "Release") {
            assert!(
                s.simulation().state().tick >= caught + 90,
                "a throw must preserve its real lift interval"
            );
            assert_eq!(s.held_by(bot), None, "release must end the canonical grip");
            released = true;
            break;
        }
    }
    assert!(
        carried && released && fastest > 3.0,
        "normal carry/swing/release failed: carried={carried}, release={released}, fastest={fastest}, thoughts={:?}",
        s.bot_thoughts()
    );
}

#[test]
fn an_alternative_weapon_cannot_replace_a_live_charge_during_a_real_range_excursion() {
    let mut weapons = pack();
    let image = weapons.images.get_mut("tactics:image/zenith").unwrap();
    image.bot.get_or_insert_with(Default::default).reach = Some(13.0);
    for state in &mut image.states {
        if state.script.eq_ignore_ascii_case("oncharge") {
            state.ticks = 84;
        }
    }
    let (mut s, human, bot, mut seq) = game_scene_kind(
        weapons,
        &[C],
        Vec3::new(-25.0, 0.05, 35.0),
        [-24.75, 0.1, 47.25],
        vec![],
        Some(bri_sim::bot_kind::BotKind {
            id: fixture::BOT.into(),
            name: "Committed unfamiliar thrower".into(),
            aim_error_degrees: 0.0,
            reaction_seconds: 0.05,
            behaviours: [
                ("chase".into(), 0.0),
                ("wander".into(), 0.0),
                ("fly".into(), 0.0),
            ]
            .into(),
            ..Default::default()
        }),
    );
    wait_for_charge(&mut s, human, bot, &mut seq);
    s.give_tool(bot, A, false).unwrap();
    let mut outside = false;
    let mut returned = false;
    let mut fired = false;
    for tick in 0..120 * 5 {
        seq += 1;
        let toward = feet(&s, bot) - feet(&s, human);
        s.movement(
            human,
            seq,
            MoveInput {
                yaw: if tick < 90 {
                    0.0
                } else {
                    toward.x.atan2(-toward.z)
                },
                forward: if tick < 90 {
                    1.0
                } else if feet(&s, human).distance(feet(&s, bot)) > 12.0 {
                    // The bot still strafes through its ordinary fight
                    // controls. Steer toward its observed live position,
                    // rather than assuming the return lies on the old axis.
                    1.0
                } else {
                    0.0
                },
                ..Default::default()
            },
        )
        .unwrap();
        s.step().unwrap();
        outside |= feet(&s, human).distance(feet(&s, bot)) > 13.5;
        returned |= outside && feet(&s, human).distance(feet(&s, bot)) <= 12.5;
        let inventory = &s.tool_inventories()[&bot];
        let selected = inventory
            .selected
            .and_then(|slot| inventory.slots[slot].as_deref());
        assert_eq!(
            selected,
            Some(C),
            "an alternative replaced the ordinary windup during target motion: {:?}",
            s.bot_thoughts()
        );
        if s.weapon_view()
            .fired()
            .any(|p| p.source.0 == bot && p.definition == "tactics:projectile/zenith")
        {
            fired = true;
            break;
        }
        let (_, state) = mounted_hand_state(&s, bot).unwrap();
        assert!(
            matches!(state.as_str(), "Charge" | "Armed"),
            "windup restarted instead of tracking: {state}"
        );
    }
    assert!(
        outside,
        "human controls must actually cross the declared attack band"
    );
    assert!(
        returned,
        "ordinary controls must actually return the human inside the band: human={:?}, bot={:?}",
        feet(&s, human),
        feet(&s, bot)
    );
    assert!(
        fired,
        "returning into the band must release the retained native charge: human={:?}, bot={:?}, state={:?}, thoughts={:?}",
        feet(&s, human),
        feet(&s, bot),
        mounted_hand_state(&s, bot),
        s.bot_thoughts()
    );
}

/// The image a bot holds in its hand, if any.
fn held_image(s: &Session, bot: u64) -> Option<String> {
    s.weapon_view()
        .images
        .get(&bot)
        .and_then(|images| images.iter().find(|i| i.hand == 0).map(|i| i.image.clone()))
}

/// A pack with a weak and a strong copy of the gun; `scripted` gives both
/// a two-projectile shot, which the native profile does not describe, so
/// only their data ranks them.
fn weak_and_strong(scripted: bool) -> Pack {
    let mut p = pack();
    for (name, damage) in [("weak", 2.0), ("strong", 40.0)] {
        alias(
            &mut p,
            testing::GUN_ITEM,
            testing::GUN_IMAGE,
            testing::GUN_PROJECTILE,
            &format!("tactics:weapon/{name}"),
        );
        p.projectiles
            .get_mut(&format!("tactics:projectile/{name}"))
            .unwrap()
            .damage = damage;
        if scripted {
            p.images
                .get_mut(&format!("tactics:image/{name}"))
                .unwrap()
                .shot = Some(
                serde_json::from_value(serde_json::json!({ "projectiles": 2, "spread": 0.0 }))
                    .unwrap(),
            );
        }
    }
    p
}

#[test]
fn the_strongest_weapon_is_used_not_the_first() {
    for scripted in [false, true] {
        let (mut s, human, bot, mut seq) = game(
            weak_and_strong(scripted),
            &[
                "tactics:weapon/weak",
                "tactics:weapon/weak",
                "tactics:weapon/strong",
            ],
            14.0,
        );
        let mut strong_ticks = 0;
        let mut weak_ticks = 0;
        for _ in 0..120 * 12 {
            ticks(&mut s, human, &mut seq, 1);
            match held_image(&s, bot).as_deref() {
                Some("tactics:image/strong") => strong_ticks += 1,
                Some("tactics:image/weak") => weak_ticks += 1,
                _ => {}
            }
            if s.vitals()[&human].health <= 20.0 {
                break;
            }
        }
        assert!(
            strong_ticks > weak_ticks * 4,
            "scripted={scripted}: strong {strong_ticks} weak {weak_ticks} thoughts={:?}",
            s.bot_thoughts()
        );
        assert!(
            s.vitals()[&human].health < 100.0,
            "scripted={scripted}: never hit"
        );
    }
}

#[test]
fn a_splash_weapon_aims_low_more_often_than_not() {
    // A rocket kills in a couple of hits, so several fresh duels at a few
    // ranges give the count.
    let (mut low, mut chest) = (0, 0);
    for distance in [14.0, 18.0, 22.0, 26.0, 30.0, 16.0] {
        let (mut s, human, bot, mut seq) = game(pack(), &[B], distance);
        let mut seen = BTreeSet::new();
        for _ in 0..120 * 20 {
            ticks(&mut s, human, &mut seq, 1);
            let fired: Vec<u64> = s
                .weapon_view()
                .fired()
                .filter(|p| p.source.0 == bot)
                .map(|p| p.id)
                .collect();
            for id in fired {
                if !seen.insert(id) {
                    continue;
                }
                let thought = s.bot_thoughts().into_iter().find(|t| t.bot == bot).unwrap();
                match thought
                    .surprise
                    .decisions
                    .iter()
                    .find(|d| d.domain == "aim")
                    .map(|d| d.chosen.as_str())
                {
                    Some("feet" | "surface") => low += 1,
                    _ => chest += 1,
                }
            }
            if s.vitals()[&human].health <= 0.0 || seen.len() >= 6 {
                break;
            }
        }
    }
    assert!(low + chest >= 6, "only {} rockets", low + chest);
    assert!(low > chest, "aimed low {low} times, at the chest {chest}");
}
