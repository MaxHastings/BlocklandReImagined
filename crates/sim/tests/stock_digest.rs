//! A fixed tour of every stock item on the generated v20 content, printing
//! a digest of the match state after every tick. Two builds whose tours
//! print the same lines played the same: loop 3's before/after proof that
//! moving the stock script behaviour into pack data changed no gameplay
//! (`docs/progress/2026-10-07-capabilities-loop-3.md`). A recording cannot
//! show this across a content regeneration, since a replay refuses other
//! content.
//!
//! Run it on each build against that build's content and compare:
//! `cargo test -p bri-sim --test stock_digest -- --ignored --nocapture`.
use bri_sim::{
    definitions::{Definitions, Special},
    player::MoveInput,
    replay::Digester,
    session::{Command, Session},
    simulation::Simulation,
};
use bri_weapons::ItemBounds;
use bri_world::{Brick, ContentRef, World};
use glam::Vec3;
use rapier3d::prelude::*;
use std::collections::BTreeMap;
use std::hash::{DefaultHasher, Hash, Hasher};
mod common;
use common::*;

/// Every item of the stock weapons pack.
const ITEMS: [&str; 21] = [
    "v20.weapon.akimbogunitem",
    "v20.weapon.basketballitem",
    "v20.weapon.bluekeyitem",
    "v20.weapon.bowitem",
    "v20.weapon.dodgeballitem",
    "v20.weapon.footballitem",
    "v20.weapon.greenkeyitem",
    "v20.weapon.gunitem",
    "v20.weapon.hammeritem",
    "v20.weapon.horserayitem",
    "v20.weapon.printgun",
    "v20.weapon.pushbroomitem",
    "v20.weapon.redkeyitem",
    "v20.weapon.rocketlauncheritem",
    "v20.weapon.skiitem",
    "v20.weapon.soccerballitem",
    "v20.weapon.spearitem",
    "v20.weapon.sworditem",
    "v20.weapon.wanditem",
    "v20.weapon.wrenchitem",
    "v20.weapon.yellowkeyitem",
];

/// A plate under the thrower spawning `item`, on a wide floor.
fn session(f: &Fixture, item: &str) -> Session {
    let definitions = Definitions {
        entries: BTreeMap::from([(
            "plate".into(),
            bri_sim::testing::definition("plate", [2, 2], 1, Special::None, false),
        )]),
    };
    let mut world = World::new("Tour".into(), "test".into(), vec![[1.; 4]]);
    let mut b = Brick::new(ContentRef::Resolved("plate".into()), [0., 0.1, 0.], 77);
    b.item_spawn.item = Some(ContentRef::Resolved(item.into()));
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

/// What the thrower does on each tick of the tour: (trigger, jet, crouch),
/// the trigger as a change (Some) or left as it is (None).
fn plan(tick: usize) -> (Option<bool>, bool, bool) {
    let trigger = match tick {
        140 => Some(true),  // a tap
        148 => Some(false),
        300 => Some(true), // a long hold, a charge
        420 => Some(false),
        640 => Some(true), // held through jet and crouch
        900 => Some(false),
        _ => None,
    };
    (trigger, (700..760).contains(&tick), (800..840).contains(&tick))
}

#[test]
#[ignore = "requires generated v20 content; prints digests to compare between builds"]
fn every_stock_item_plays_the_same() {
    let f = Fixture::content();
    let mut whole = DefaultHasher::new();
    for item in ITEMS {
        let mut s = session(&f, item);
        let thrower = s
            .join("Thrower".into(), Vec3::new(0., 0.35, 0.), false)
            .unwrap();
        let target = s
            .join("Target".into(), Vec3::new(0., 0.35, -4.), false)
            .unwrap();
        let mut digester = Digester::new(&s);
        let mut digest = DefaultHasher::new();
        let mut seq = 0;
        let mut held = None;
        for tick in 0..1200 {
            if tick == 130
                && let Some(slot) = s.tool_inventories()[&thrower]
                    .slots
                    .iter()
                    .position(|i| i.as_deref() == Some(item))
            {
                // A ball mounts as it is picked up; a tool is drawn.
                s.equip_tool(thrower, Some(slot)).unwrap();
            }
            let (trigger, jet, crouch) = plan(tick);
            for (owner, yaw) in [(thrower, 0.0), (target, std::f32::consts::PI)] {
                let input = MoveInput {
                    yaw,
                    jet: owner == thrower && jet,
                    crouch: owner == thrower && crouch,
                    ..Default::default()
                };
                s.movement(owner, move_sequence(&s), input).unwrap();
            }
            if let Some(down) = trigger {
                seq += 1;
                s.command(thrower, seq, Command::WeaponTrigger { down })
                    .unwrap();
            }
            s.step().unwrap();
            if tick == 139 {
                held = s
                    .weapon_view()
                    .images
                    .get(&thrower)
                    .and_then(|i| i.iter().find(|i| i.hand == 0))
                    .map(|i| i.image.clone());
            }
            let p = digester.parts(&s);
            (p.world, p.players, p.weapons, p.vehicles, p.minigames, p.packages, p.chat)
                .hash(&mut digest);
        }
        let digest = digest.finish();
        digest.hash(&mut whole);
        // The tour holds the item it is about, so its digest is that
        // item's play.
        let image = &f.weapons.items[item].image;
        assert_eq!(held.as_ref(), Some(image), "{item} was not in hand");
        println!("{item} {digest:016x}");
    }
    println!("whole {:016x}", whole.finish());
}
