//! Linked bricks: pairing by name and walking through their openings, with
//! a made-up doorway brick (no converted content needed).
use bri_content::{
    brick::{Brick as Mesh, Face, Link},
    collision::{CollisionBody, Part},
};
use bri_sim::{
    definitions::{Definition, Definitions},
    player::{MoveInput, Player, PlayerTuning},
    simulation::Simulation,
};
use bri_world::{Brick, ContentRef, World};
use glam::Vec3;
use rapier3d::prelude::*;

const PORTAL: &str = "portal";
const WALL: &str = "wall";

/// A 1x4x5 doorway (2 wide, 3 tall, half a unit deep) opening north and
/// south through its middle, and a 1x4x5 solid wall.
fn definitions() -> Definitions {
    let mesh = |id: &str| Mesh {
        schema_version: 1,
        id: id.into(),
        footprint_studs: [4, 1],
        height_plates: 15,
        attachment_rows: vec!["bbbb".into(); 15],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![],
    };
    let link = Link {
        faces: vec![Face::North, Face::South],
        depth: 0.5,
        inset: 0.0,
        tint: [1.0; 3],
        idle: [0.5; 3],
        pass: true,
        frame: 0.1,
        name: "Portal".into(),
    };
    let body = |id: &str, parts: Vec<Part>| {
        let collision = CollisionBody {
            id: id.into(),
            parts,
        };
        let shape = bri_physics::content::collider(&collision)
            .unwrap()
            .build()
            .shared_shape()
            .clone();
        (collision, shape)
    };
    let door = mesh("door");
    let (portal_collision, portal_shape) = body(
        PORTAL,
        link.frame_boxes(&door)
            .into_iter()
            .map(|b| Part::Box {
                center: b.center,
                size: b.size,
            })
            .collect(),
    );
    let (wall_collision, wall_shape) = body(
        WALL,
        vec![Part::Box {
            center: [0.0; 3],
            size: [2.0, 3.0, 0.5],
        }],
    );
    let definition = |mesh, collision, shape, link| Definition {
        mesh,
        collision,
        shape,
        indestructible: false,
        special: Default::default(),
        reflection: None,
        link,
        glass: [0.0; 4],
    };
    Definitions {
        entries: [
            (
                PORTAL.to_string(),
                definition(door, portal_collision, portal_shape, Some(link)),
            ),
            (
                WALL.to_string(),
                definition(mesh("wall"), wall_collision, wall_shape, None),
            ),
        ]
        .into(),
    }
}

fn brick(definition: &str, position: [f32; 3], turns: u8, name: Option<&str>) -> Brick {
    let mut brick = Brick::new(ContentRef::Resolved(definition.into()), position, 1);
    brick.quarter_turns = turns;
    brick.name = name.map(Into::into);
    brick
}

fn simulation(bricks: Vec<Brick>) -> Simulation {
    let mut world = World::new("Portals".into(), "test".into(), vec![[1.0; 4]]);
    for (i, brick) in bricks.into_iter().enumerate() {
        world.bricks.insert(i as u64 + 1, brick);
        world.next_brick_id = i as u64 + 2;
    }
    Simulation::new(
        world,
        definitions(),
        vec![ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0))],
    )
    .unwrap()
}

/// Walk `ticks` 120 Hz steps along `yaw`, turning the look with every
/// opening passed as a player's game does. Returns the body's middle at
/// each Torque tick, carried back to where the walk began so a straight
/// walk stays straight.
fn walk(sim: &mut Simulation, player: &mut Player, mut yaw: f32, ticks: usize) -> Vec<Vec3> {
    let mut back = glam::Affine3A::IDENTITY;
    let mut path = Vec::new();
    for _ in 0..ticks {
        let input = MoveInput {
            forward: 1.0,
            yaw,
            ..Default::default()
        };
        let events = sim.step_body(player, input, &[]).unwrap();
        if let Some(carry) = events.passed {
            yaw = bri_content::passage::carried_yaw(&carry, yaw);
            back = back * carry.inverse();
        }
        sim.step().unwrap();
        if events.ticked {
            let middle = Vec3::from(player.state().feet) + Vec3::Y * player.middle();
            path.push(back.transform_point3(middle));
        }
    }
    path
}

fn spawn(sim: &mut Simulation, feet: Vec3) -> Player {
    let mut player =
        Player::spawn(&mut sim.physics, 1, feet, PlayerTuning::default()).unwrap();
    for _ in 0..60 {
        sim.step_body(&mut player, MoveInput::default(), &[]).unwrap();
        sim.step().unwrap();
    }
    player
}

#[test]
fn bricks_of_one_name_link_and_others_stay_shut() {
    let mut sim = simulation(vec![
        brick(PORTAL, [0.0, 1.5, -4.25], 0, Some("Portal_a")),
        brick(PORTAL, [10.25, 1.5, -4.0], 1, Some("portal_A")),
        brick(PORTAL, [20.0, 1.5, -4.25], 0, None),
        brick(PORTAL, [30.0, 1.5, -4.25], 0, Some("other")),
    ]);
    let links = sim.links();
    assert_eq!(links.partner(1), Some(2));
    assert_eq!(links.partner(2), Some(1));
    assert_eq!(links.partner(3), None);
    assert_eq!(links.partner(4), None);
    // Two linked bricks with two open sides each; the others shut.
    assert_eq!(links.passages().list.len(), 4);
    assert_eq!(links.passages().closed.len(), 4);
    // Renaming one breaks the pair; a third of the name makes a ring.
    let _ = links;
    sim.mutate(2, |b| b.name = Some("x".into())).unwrap();
    assert_eq!(sim.links().partner(1), None);
    sim.mutate(2, |b| b.name = Some("Portal_A".into())).unwrap();
    sim.mutate(4, |b| b.name = Some("Portal_A".into())).unwrap();
    let links = sim.links();
    assert_eq!(
        (links.partner(1), links.partner(2), links.partner(4)),
        (Some(2), Some(4), Some(1))
    );
}

#[test]
fn walking_through_comes_out_of_the_partner_turned_without_a_hitch() {
    // In through the south side of a doorway at z = -4 walking north;
    // out of the north side of its partner, turned a quarter, walking east.
    let mut sim = simulation(vec![
        brick(PORTAL, [0.0, 1.5, -4.25], 0, Some("Portal_a")),
        brick(PORTAL, [10.25, 1.5, -4.0], 1, Some("Portal_a")),
    ]);
    let mut player = spawn(&mut sim, Vec3::new(0.0, 0.05, 0.0));
    let path = walk(&mut sim, &mut player, 0.0, 240);
    let feet = Vec3::from(player.state().feet);
    let velocity = Vec3::from(player.state().velocity);
    assert!(
        feet.x > 11.0 && (feet.z + 4.0).abs() < 0.2,
        "came out at {feet}"
    );
    assert!(velocity.x > 3.0 && velocity.z.abs() < 0.2, "{velocity}");
    let yaw = player.state().yaw;
    assert!((yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-3, "{yaw}");
    // Seen from where it went in, the walk never jumps or stalls.
    let steps: Vec<f32> = path.windows(2).map(|w| w[0].distance(w[1])).collect();
    let most = steps.iter().copied().fold(0.0, f32::max);
    assert!(most < 0.3, "{steps:?}");
    assert!(path.iter().all(|p| p.x.abs() < 0.05), "{path:?}");
    assert!(path.last().unwrap().z < -6.0);
}

#[test]
fn a_wall_right_behind_the_doorway_does_not_stop_the_walk() {
    // A doorway set against a wall is a way through it; the far side is
    // what the walker meets instead, a wall there stops them.
    let mut sim = simulation(vec![
        brick(PORTAL, [0.0, 1.5, -4.25], 0, Some("Portal_a")),
        brick(WALL, [0.0, 1.5, -4.75], 0, None),
        brick(PORTAL, [10.0, 1.5, -4.25], 0, Some("Portal_a")),
        brick(WALL, [10.0, 1.5, -5.75], 0, None),
    ]);
    let mut player = spawn(&mut sim, Vec3::new(0.0, 0.05, 0.0));
    walk(&mut sim, &mut player, 0.0, 360);
    let feet = Vec3::from(player.state().feet);
    // Through the first wall, stopped by the second one past the partner.
    assert!(feet.x > 9.5 && feet.x < 10.5, "{feet}");
    assert!(feet.z < -4.2 && feet.z > -5.3, "{feet}");
}

#[test]
fn an_unlinked_doorway_is_shut() {
    let mut sim = simulation(vec![brick(PORTAL, [0.0, 1.5, -4.25], 0, None)]);
    let mut player = spawn(&mut sim, Vec3::new(0.0, 0.05, 0.0));
    walk(&mut sim, &mut player, 0.0, 240);
    let feet = Vec3::from(player.state().feet);
    assert!(feet.z > -4.0 && feet.z < -3.0, "{feet}");
}
