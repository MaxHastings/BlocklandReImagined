//! Clearing bricks: the Admin menu's Clear All Bricks and Clear Brick Group,
//! and v20's `/clearBricks` (a player's own bricks). A big build must clear
//! in one quick request, not one physics refresh per brick.
use bri_admin::{Action, Request};
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_sim::{
    definitions::{Definition, Definitions},
    session::{Command, Session},
    simulation::Simulation,
};
use bri_world::{Brick, ContentRef, OwnerId, World};
use glam::Vec3;
use rapier3d::prelude::*;
use std::time::{Duration, Instant};

fn definitions() -> Definitions {
    let mesh = Mesh {
        schema_version: 1,
        id: "plate".into(),
        footprint_studs: [2, 1],
        height_plates: 1,
        attachment_rows: vec!["bb".into()],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![],
    };
    let collision = CollisionBody {
        id: "plate".into(),
        parts: vec![Part::Box {
            center: [0.0; 3],
            size: [1.0, 0.2, 0.5],
        }],
    };
    let shape = bri_physics::content::collider(&collision)
        .unwrap()
        .build()
        .shared_shape()
        .clone();
    Definitions {
        entries: [(
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
                bot: None,
            },
        )]
        .into(),
    }
}

/// A world holding `per_owner` plates for each of `owners`, laid out on a
/// grid well away from the spawn point.
fn session(owners: &[OwnerId], per_owner: usize) -> Session {
    let mut world = World::new("Clear".into(), "clear".into(), vec![[1.0; 4], [0.0; 4]]);
    let mut id = 1;
    for (row, &owner) in owners.iter().enumerate() {
        for i in 0..per_owner {
            let position = [
                20.0 + (i % 100) as f32 * 1.0,
                0.1 + (i / 100) as f32 * 0.2,
                20.25 + row as f32 * 60.0,
            ];
            world.bricks.insert(
                id,
                Brick::new(ContentRef::Resolved("plate".into()), position, owner),
            );
            id += 1;
        }
    }
    world.next_brick_id = id;
    let mut s = Session::new(
        Simulation::new(
            world,
            definitions(),
            vec![
                ColliderBuilder::cuboid(500.0, 0.5, 500.0).translation(Vector::new(0.0, -0.5, 0.0)),
            ],
        )
        .unwrap(),
    );
    s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 0.0)]).unwrap();
    s
}

/// Server chat lines so far, as players see them.
fn chat(s: &mut Session) -> Vec<String> {
    s.chat().into_iter().map(|line| line.text).collect()
}

#[test]
fn clear_all_bricks_on_a_big_build_is_one_quick_request() {
    let mut s = session(&[1, 2], 20_000);
    let host = s
        .join("Host".into(), Vec3::new(0.0, 0.05, 0.0), true)
        .unwrap();
    s.step().unwrap();
    assert_eq!(s.simulation().state().bricks.len(), 40_000);
    let started = Instant::now();
    s.command_with_aim(
        host,
        1,
        Command::Admin(Request::new(Action::ClearAllBricks)),
        None,
    )
    .unwrap();
    let took = started.elapsed();
    assert!(s.simulation().state().bricks.is_empty());
    assert!(
        took < Duration::from_secs(5),
        "clearing 40,000 bricks took {took:?}"
    );
    s.step().unwrap();
}

fn plant(s: &mut Session, owner: OwnerId, seq: u64, position: [f32; 3]) {
    s.command_with_aim(
        owner,
        seq,
        Command::Plant {
            definition: "plate".into(),
            position,
            quarter_turns: 0,
            color: 0,
        },
        None,
    )
    .unwrap();
    for _ in 0..121 {
        s.step().unwrap();
    }
}

fn typed(command: &str) -> Command {
    Command::Package(bri_sim::session::PackageCommand {
        package: String::new(),
        command: command.into(),
        args: vec![],
    })
}

#[test]
fn clear_bricks_takes_only_your_own_and_waits_five_seconds() {
    let mut s = session(&[], 0);
    let ann = s
        .join("Ann".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    let bob = s
        .join("Bob".into(), Vec3::new(4.0, 0.05, 0.0), false)
        .unwrap();
    s.step().unwrap();
    plant(&mut s, ann, 1, [-1.5, 0.1, -2.25]);
    plant(&mut s, ann, 2, [-1.5, 0.1, -4.25]);
    plant(&mut s, bob, 1, [5.5, 0.1, -2.25]);
    assert_eq!(s.simulation().state().bricks.len(), 3);
    chat(&mut s);
    s.command_with_aim(ann, 3, typed("clearBricks"), None)
        .unwrap();
    let owners: Vec<_> = s
        .simulation()
        .state()
        .bricks
        .values()
        .map(|b| b.owner)
        .collect();
    assert_eq!(owners, vec![bob], "only Ann's bricks go");
    // v20's `MsgClearBricks` line, to everyone.
    let lines = chat(&mut s);
    assert!(
        lines
            .iter()
            .any(|l| l.contains("Ann") && l.contains("'s bricks")),
        "{lines:?}"
    );
    // Within five seconds a second clear does nothing.
    s.step().unwrap();
    plant(&mut s, ann, 4, [-1.5, 0.1, -2.25]);
    s.command_with_aim(ann, 5, typed("clearbricks"), None)
        .unwrap();
    assert_eq!(s.simulation().state().bricks.len(), 2);
    for _ in 0..(5 * 120) {
        s.step().unwrap();
    }
    s.command_with_aim(ann, 6, typed("clearbricks"), None)
        .unwrap();
    assert_eq!(s.simulation().state().bricks.len(), 1);
    // `/clearAllBricks` is not for players.
    s.command_with_aim(bob, 2, typed("clearAllBricks"), None)
        .unwrap_err();
    assert_eq!(s.simulation().state().bricks.len(), 1);
}

#[test]
fn clear_all_bricks_is_for_administrators() {
    let mut s = session(&[1], 50);
    let host = s
        .join("Host".into(), Vec3::new(0.0, 0.05, 0.0), true)
        .unwrap();
    let guest = s
        .join("Guest".into(), Vec3::new(4.0, 0.05, 0.0), false)
        .unwrap();
    s.step().unwrap();
    assert!(
        s.command_with_aim(
            guest,
            1,
            Command::Admin(Request::new(Action::ClearAllBricks)),
            None
        )
        .is_err()
    );
    assert_eq!(s.simulation().state().bricks.len(), 50);
    chat(&mut s);
    s.command_with_aim(
        host,
        1,
        Command::Admin(Request::new(Action::ClearAllBricks)),
        None,
    )
    .unwrap();
    assert!(s.simulation().state().bricks.is_empty());
    assert!(
        chat(&mut s)
            .iter()
            .any(|l| l.contains("cleared all bricks"))
    );
}
