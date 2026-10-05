//! Item_Sports balls through the host session, on the made-up weapons and
//! on the generated v20 pack.
use bri_sim::{
    definitions::{Definitions, Special},
    session::{Command, Session},
    simulation::Simulation,
};
use bri_weapons::ItemBounds;
use bri_world::{Brick, ContentRef, World};
use glam::Vec3;
use rapier3d::prelude::*;
use std::collections::BTreeMap;
mod common;
use common::*;

/// A session whose one plate spawns `item`, with the fixture's weapons.
fn session(f: &Fixture, item: Item) -> Session {
    let definitions = Definitions {
        entries: BTreeMap::from([(
            "plate".into(),
            bri_sim::testing::definition("plate", [2, 2], 1, Special::None, false),
        )]),
    };
    let mut world = World::new("Sports".into(), "test".into(), vec![[1.; 4]]);
    let mut b = Brick::new(ContentRef::Resolved("plate".into()), [0., 0.1, 0.], 77);
    b.item_spawn.item = Some(ContentRef::Resolved(f.item(item).into()));
    b.item_spawn.respawn_ms = 1000;
    world.bricks.insert(1, b);
    world.next_brick_id = 2;
    let simulation = Simulation::new(
        world,
        definitions,
        vec![ColliderBuilder::cuboid(100., 0.5, 100.).translation(Vector::new(0., -0.5, 0.))],
    )
    .unwrap();
    let mut s = Session::new(simulation);
    let pack = f.weapons.clone();
    let bounds = pack
        .items
        .keys()
        .map(|id| {
            (
                id.clone(),
                ItemBounds {
                    min: [-0.3; 3],
                    max: [0.3; 3],
                },
            )
        })
        .collect();
    s.set_weapon_pack(pack).unwrap();
    s.set_item_bounds(bounds).unwrap();
    s
}

/// The image an item mounts.
fn image_of(f: &Fixture, item: Item) -> String {
    f.weapons.items[f.item(item)].image.clone()
}

fn held(s: &Session, owner: u64) -> Option<String> {
    s.weapon_view()
        .images
        .get(&owner)
        .and_then(|i| i.iter().find(|i| i.hand == 0))
        .map(|i| i.image.clone())
}

fn step(s: &mut Session, owner: u64, n: usize) {
    for _ in 0..n {
        hold_still(s, owner);
        s.step().unwrap();
    }
}

on_both! {
fn walking_into_a_ball_mounts_it_and_fire_throws_it(f: &Fixture) {
    for item in [
        Item::Basketball,
        Item::Dodgeball,
        Item::Football,
        Item::SoccerBall,
    ] {
        let mut s = session(f, item);
        let p = s
            .join("Player".into(), Vec3::new(0., 0.35, 0.), false)
            .unwrap();
        step(&mut s, p, 130);
        let image = held(&s, p).unwrap_or_else(|| panic!("{item:?} not picked up"));
        assert!(image.contains("ball"), "{item:?}: {image}");
        s.command(p, 1, Command::WeaponTrigger { down: true })
            .unwrap();
        step(&mut s, p, 100);
        s.command(p, 2, Command::WeaponTrigger { down: false })
            .unwrap();
        step(&mut s, p, 4);
        assert!(
            !s.weapon_view().projectiles.is_empty() || held(&s, p).is_none(),
            "{item:?} was not thrown: {:?}",
            held(&s, p)
        );
        assert!(held(&s, p).is_none(), "{item:?} still held");
    }
}
}

fn throw(s: &mut Session, p: u64, seq: u64) {
    s.command(p, seq, Command::WeaponTrigger { down: true })
        .unwrap();
    step(s, p, 100);
    s.command(p, seq + 1, Command::WeaponTrigger { down: false })
        .unwrap();
}

on_both! {
fn a_pass_is_caught_by_a_player_in_the_same_game(f: &Fixture) {
    let mut s = session(f, Item::Basketball);
    let a = s
        .join("Passer".into(), Vec3::new(0., 0.35, 0.), false)
        .unwrap();
    step(&mut s, a, 130);
    assert!(held(&s, a).is_some());
    let b = s
        .join("Catcher".into(), Vec3::new(0., 0.35, -4.), false)
        .unwrap();
    step(&mut s, a, 30);
    throw(&mut s, a, 1);
    for _ in 0..240 {
        hold_still(&mut s, b);
        step(&mut s, a, 1);
        if held(&s, b).is_some() {
            break;
        }
    }
    assert!(held(&s, a).is_none());
    assert_eq!(
        held(&s, b).as_deref(),
        Some(image_of(f, Item::Basketball).as_str()),
        "the pass was not caught"
    );
}
}

on_both! {
/// `CatchFootballMessage`: the first thrown catch sets the record, so the
/// reward sound plays at both players and the win star over the passer, as
/// well as the one every clean catch puts over the catcher.
fn a_record_football_catch_rewards_both_and_stars_the_passer(f: &Fixture) {
    use bri_sim::presentation::CueKind;
    let mut s = session(f, Item::Football);
    let a = s
        .join("Passer".into(), Vec3::new(0., 0.35, 0.), false)
        .unwrap();
    step(&mut s, a, 130);
    let b = s
        .join("Catcher".into(), Vec3::new(0., 0.35, -4.), false)
        .unwrap();
    step(&mut s, a, 30);
    s.take_cues();
    throw(&mut s, a, 1);
    let mut cues = s.take_cues();
    for _ in 0..240 {
        hold_still(&mut s, b);
        step(&mut s, a, 1);
        cues.extend(s.take_cues());
        if held(&s, b).is_some() {
            break;
        }
    }
    assert!(held(&s, b).is_some(), "the pass was not caught");
    // The catch's cues follow on the next steps.
    for _ in 0..3 {
        step(&mut s, a, 1);
        cues.extend(s.take_cues());
    }
    let rewards = cues
        .iter()
        .filter(|c| matches!(&c.kind, CueKind::WeaponSound { profile } if profile == "rewardSound"))
        .count();
    assert_eq!(rewards, 2, "receiver and passer");
    let stars: Vec<_> = cues
        .iter()
        .filter(|c| {
            matches!(&c.kind, CueKind::WeaponEffect { definition, .. } if definition == "WinStarExplosion")
        })
        .collect();
    assert_eq!(stars.len(), 2, "catcher and passer");
    let near = |at: Vec3| stars.iter().any(|c| Vec3::from(c.position).distance(at) < 2.5);
    assert!(near(Vec3::new(0., 0.35, 0.)), "over the passer");
    assert!(near(Vec3::new(0., 0.35, -4.)), "over the catcher");
}
}

on_both! {
fn a_resting_football_becomes_an_item_that_mounts_on_touch(f: &Fixture) {
    let mut s = session(f, Item::Football);
    let a = s
        .join("Kicker".into(), Vec3::new(0., 0.35, 0.), false)
        .unwrap();
    step(&mut s, a, 130);
    throw(&mut s, a, 1);
    let mut rested = None;
    for _ in 0..1200 {
        step(&mut s, a, 1);
        if let Some(d) = s.weapon_view().drops.first() {
            rested = Some(d.clone());
            break;
        }
    }
    let drop = rested.expect("the football never came to rest as an item");
    assert_eq!(drop.item, f.item(Item::Football));
    let b = s
        .join("Receiver".into(), drop.position + Vec3::Y * 0.2, false)
        .unwrap();
    step(&mut s, b, 5);
    assert_eq!(held(&s, b), Some(image_of(f, Item::Football)));
    assert!(s.weapon_view().drops.is_empty());
}
}

on_both! {
fn dying_drops_the_ball(f: &Fixture) {
    let mut s = session(f, Item::Dodgeball);
    let a = s
        .join("Holder".into(), Vec3::new(0., 0.35, 0.), false)
        .unwrap();
    step(&mut s, a, 130);
    assert!(held(&s, a).is_some());
    s.command(a, 1, Command::Suicide).unwrap();
    s.step().unwrap();
    assert!(held(&s, a).is_none());
    let ball = f.weapons.images[&image_of(f, Item::Dodgeball)]
        .projectile
        .clone()
        .unwrap();
    assert!(
        s.weapon_view()
            .projectiles
            .iter()
            .any(|p| p.definition == ball)
    );
}
}

on_both! {
fn a_ball_in_the_first_loadout_slot_is_the_start_ball(f: &Fixture) {
    use bri_sim::session::MiniGameRequest;
    let mut s = session(f, Item::Basketball);
    let a = s
        .join("Owner".into(), Vec3::new(5., 0.35, 5.), false)
        .unwrap();
    step(&mut s, a, 5);
    let mut settings = f.minigame_settings();
    settings.loadout[0] = Some(f.item(Item::Dodgeball).into());
    s.command(
        a,
        1,
        Command::MiniGame(MiniGameRequest::Create { color: 0, settings }),
    )
    .unwrap();
    step(&mut s, a, 2);
    assert_eq!(held(&s, a), Some(image_of(f, Item::Dodgeball)));
    // `serverCmdSetMiniGameData` strips balls from the tool slots.
    assert!(
        s.tool_inventories()[&a]
            .slots
            .iter()
            .flatten()
            .all(|i| !i.contains("ball"))
    );
}
}
