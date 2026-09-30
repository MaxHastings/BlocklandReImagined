//! Item_Sports balls through the host session with the generated v20 pack.
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_sim::{
    definitions::{Definition, Definitions},
    session::{Command, Session},
    simulation::Simulation,
};
use bri_weapons::{ItemBounds, native_id};
use bri_world::{Brick, ContentRef, World};
use glam::Vec3;
use rapier3d::prelude::*;
use std::collections::BTreeMap;
mod common;
use common::*;

fn session(item: &str) -> Session {
    let mesh = Mesh {
        schema_version: 1,
        id: "plate".into(),
        footprint_studs: [2, 2],
        height_plates: 1,
        attachment_rows: vec!["bb".into(), "bb".into()],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![],
    };
    let collision = CollisionBody {
        id: "plate".into(),
        parts: vec![Part::Box {
            center: [0.; 3],
            size: [1., 0.2, 1.],
        }],
    };
    let shape = bri_physics::content::collider(&collision)
        .unwrap()
        .build()
        .shared_shape()
        .clone();
    let definitions = Definitions {
        entries: BTreeMap::from([(
            "plate".into(),
            Definition {
                mesh,
                collision,
                shape,
                indestructible: false,
                special: Default::default(),
                reflection: None,
                link: None,
                glass: [0.0; 4],
            },
        )]),
    };
    let mut world = World::new("Sports".into(), "test".into(), vec![[1.; 4]]);
    let mut b = Brick::new(ContentRef::Resolved("plate".into()), [0., 0.1, 0.], 77);
    b.item_spawn.item = Some(ContentRef::Resolved(native_id("weapon", item)));
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
    let pack = weapon_pack();
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

#[test]
#[ignore = "requires generated native weapons-pack-007"]
fn walking_into_a_ball_mounts_it_and_fire_throws_it() {
    for item in [
        "basketballItem",
        "dodgeballItem",
        "footballItem",
        "soccerBallItem",
    ] {
        let mut s = session(item);
        let p = s
            .join("Player".into(), Vec3::new(0., 0.35, 0.), false)
            .unwrap();
        step(&mut s, p, 130);
        let image = held(&s, p).unwrap_or_else(|| panic!("{item} not picked up"));
        assert!(image.contains("ball"), "{item}: {image}");
        s.command(p, 1, Command::WeaponTrigger { down: true })
            .unwrap();
        step(&mut s, p, 100);
        s.command(p, 2, Command::WeaponTrigger { down: false })
            .unwrap();
        step(&mut s, p, 4);
        assert!(
            !s.weapon_view().projectiles.is_empty() || held(&s, p).is_none(),
            "{item} was not thrown: {:?}",
            held(&s, p)
        );
        assert!(held(&s, p).is_none(), "{item} still held");
    }
}

fn throw(s: &mut Session, p: u64, seq: u64) {
    s.command(p, seq, Command::WeaponTrigger { down: true })
        .unwrap();
    step(s, p, 100);
    s.command(p, seq + 1, Command::WeaponTrigger { down: false })
        .unwrap();
}

#[test]
#[ignore = "requires generated native weapons-pack-007"]
fn a_pass_is_caught_by_a_player_in_the_same_game() {
    let mut s = session("basketballItem");
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
        Some("v20.image.basketballimage"),
        "the pass was not caught"
    );
}

#[test]
#[ignore = "requires generated native weapons-pack-007"]
fn a_resting_football_becomes_an_item_that_mounts_on_touch() {
    let mut s = session("footballItem");
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
    assert_eq!(drop.item, "v20.weapon.footballitem");
    let b = s
        .join("Receiver".into(), drop.position + Vec3::Y * 0.2, false)
        .unwrap();
    step(&mut s, b, 5);
    assert_eq!(held(&s, b).as_deref(), Some("v20.image.footballimage"));
    assert!(s.weapon_view().drops.is_empty());
}

#[test]
#[ignore = "requires generated native weapons-pack-007"]
fn dying_drops_the_ball() {
    let mut s = session("dodgeballItem");
    let a = s
        .join("Holder".into(), Vec3::new(0., 0.35, 0.), false)
        .unwrap();
    step(&mut s, a, 130);
    assert!(held(&s, a).is_some());
    s.command(a, 1, Command::Suicide).unwrap();
    s.step().unwrap();
    assert!(held(&s, a).is_none());
    assert!(
        s.weapon_view()
            .projectiles
            .iter()
            .any(|p| p.definition == "v20.projectile.dodgeballprojectile")
    );
}

#[test]
#[ignore = "requires generated native weapons-pack-007"]
fn a_ball_in_the_first_loadout_slot_is_the_start_ball() {
    use bri_minigames::Settings;
    use bri_sim::session::MiniGameRequest;
    let mut s = session("basketballItem");
    let a = s
        .join("Owner".into(), Vec3::new(5., 0.35, 5.), false)
        .unwrap();
    step(&mut s, a, 5);
    let mut settings = Settings::default();
    settings.loadout[0] = Some(native_id("weapon", "dodgeballItem"));
    s.command(
        a,
        1,
        Command::MiniGame(MiniGameRequest::Create { color: 0, settings }),
    )
    .unwrap();
    step(&mut s, a, 2);
    assert_eq!(held(&s, a).as_deref(), Some("v20.image.dodgeballimage"));
    // `serverCmdSetMiniGameData` strips balls from the tool slots.
    assert!(
        s.tool_inventories()[&a]
            .slots
            .iter()
            .flatten()
            .all(|i| !i.contains("ball"))
    );
}
