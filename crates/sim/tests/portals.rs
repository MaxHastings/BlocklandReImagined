//! Linked bricks: pairing by name and walking through their openings, with
//! a made-up doorway brick (no converted content needed).
use bri_content::{
    brick::{Brick as Mesh, Face, Frame, Link},
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
        // The stock window's: thin sides and top, a sill to step over.
        frame: Frame {
            sides: 0.05,
            top: 0.05,
            bottom: 0.2,
        },
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
            back *= carry.inverse();
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
    let mut player = Player::spawn(&mut sim.physics, 1, feet, PlayerTuning::default()).unwrap();
    for _ in 0..60 {
        sim.step_body(&mut player, MoveInput::default(), &[])
            .unwrap();
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
    // A standing player steps up onto the sill and off it again.
    let rise = path.iter().map(|p| p.y).fold(f32::MIN, f32::max) - path[0].y;
    assert!((0.15..0.3).contains(&rise), "rose {rise}");
    assert!((path.last().unwrap().y - path[0].y).abs() < 0.05);
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

/// Shots through a portal: every projectile whose flight crosses the
/// opening comes out of the partner with its speed, turned, and keeps its
/// owner; a shot fired with the muzzle already past the opening goes
/// through too. A small authored pack keeps it content-free.
mod shots {
    use super::*;
    use bri_sim::weapon_query::WeaponQuery;
    use bri_weapons::{
        ActorId, Filter, Hit, Nearby, Pack, Query, TargetId, WeaponsWorld,
        Frame as WeaponFrame,
    };
    use std::collections::BTreeMap;

    const SHOOTER: ActorId = ActorId(7);

    /// A gun (fast, straight), a rocket launcher (slow, straight), a bow
    /// (arcing) and a sword (melee: a short-lived projectile from the eye).
    fn pack() -> Pack {
        let states = r#"[
          { "name": "Activate", "ticks": 1, "timeout": 1 },
          { "name": "Ready", "down": 2 },
          { "name": "Fire", "ticks": 600, "script": "onFire", "timeout": 3 },
          { "name": "Reload", "up": 1 }
        ]"#;
        let image = |name: &str, projectile: &str, melee: bool| {
            format!(
                r#""t:image/{name}": {{ "name": "{name}Image", "projectile": "t:projectile/{projectile}", "melee": {melee}, "states": {states} }}"#
            )
        };
        let item = |name: &str| {
            format!(r#""t:weapon/{name}": {{ "ui_name": "{name}", "image": "t:image/{name}" }}"#)
        };
        let projectile = |name: &str, speed: f32, ballistic: bool, lifetime: u32| {
            format!(
                r#""t:projectile/{name}": {{ "id": "t:projectile/{name}", "name": "{name}Projectile", "speed": {speed}, "inherit": 0, "gravity": 1, "ballistic": {ballistic}, "lifetime_ticks": {lifetime}, "fade_ticks": 0, "elasticity": 0 }}"#
            )
        };
        let names = ["gun", "rocket", "bow", "sword"];
        let json = format!(
            r#"{{ "schema_version": 3, "id": "t",
              "items": {{ {} }},
              "images": {{ {}, {}, {}, {} }},
              "projectiles": {{ {}, {}, {}, {} }} }}"#,
            names.map(item).join(","),
            image("gun", "bullet", false),
            image("rocket", "rocket", false),
            image("bow", "arrow", false),
            image("sword", "slash", true),
            projectile("bullet", 200.0, false, 240),
            projectile("rocket", 40.0, false, 480),
            projectile("arrow", 100.0, true, 240),
            projectile("slash", 50.0, false, 60),
        );
        Pack::from_json(json.as_bytes()).unwrap()
    }

    fn step(sim: &Simulation, w: &mut WeaponsWorld) -> Vec<bri_weapons::Event> {
        let yes = |_: ActorId, _: TargetId| true;
        let catch = |_: ActorId, _: ActorId| false;
        let responses = BTreeMap::new();
        let mut q = WeaponQuery {
            simulation: sim,
            affect: &yes,
            affect_radius: &yes,
            catch: &catch,
            responses: &responses,
            truncated_targets: 0,
            shapes: &[],
        };
        w.step(&mut q)
    }

    /// Nothing to hit and no openings: where a shot flies on its own.
    struct Open;
    impl Query for Open {
        fn sweep(&mut self, _: Vec3, _: Vec3, _: Filter) -> Option<Hit> {
            None
        }
        fn radius(&mut self, _: Vec3, _: f32, _: usize) -> Vec<Nearby> {
            Vec::new()
        }
        fn can_affect(&self, _: ActorId, _: TargetId) -> bool {
            true
        }
        fn can_catch(&self, _: ActorId, _: ActorId) -> bool {
            true
        }
    }

    /// The doorway pair of `walking_through...`: in through the south side
    /// of the one at z = -4.25, out of the north side of the one at
    /// x = 10.25 turned a quarter.
    fn doorways() -> Simulation {
        simulation(vec![
            brick(PORTAL, [0.0, 1.5, -4.25], 0, Some("Portal_a")),
            brick(PORTAL, [10.25, 1.5, -4.0], 1, Some("Portal_a")),
        ])
    }

    /// Where a shot from `start` at `velocity` is after `ticks`: its free
    /// flight, carried by the first opening that flight goes in through.
    fn expected(
        sim: &Simulation,
        definition: &str,
        start: Vec3,
        velocity: Vec3,
        ticks: usize,
    ) -> (Vec3, Vec3) {
        let mut free = WeaponsWorld::new(pack()).unwrap();
        free.spawn(definition, SHOOTER, start, velocity, 1.0).unwrap();
        let mut carry = None;
        for _ in 0..ticks {
            let before = free.projectiles().next().unwrap().position;
            free.step(&mut Open);
            let after = free.projectiles().next().unwrap().position;
            if carry.is_none() {
                carry = sim.passages().first(before, after).map(|(p, _)| p.carry);
            }
        }
        let p = free.projectiles().next().unwrap();
        match carry {
            Some(c) => (c.transform_point3(p.position), c.transform_vector3(p.velocity)),
            None => (p.position, p.velocity),
        }
    }

    #[test]
    fn every_shot_into_the_opening_comes_out_of_its_partner() {
        let sim = doorways();
        let passage = sim.passages().list[1];
        assert_eq!(passage.normal, Vec3::Z, "the south side");
        let mut failures = Vec::new();
        let mut cases = 0;
        for speed in [3.0, 20.0, 60.0, 200.0, 900.0] {
            for yaw in [-0.6f32, -0.25, 0.0, 0.1, 0.45] {
                for pitch in [-0.2f32, 0.0, 0.15] {
                    for across in [-0.9f32, -0.45, 0.0, 0.33, 0.9] {
                        for up in [0.5f32, 1.4, 2.7] {
                            for before in [0.0005f32, 0.05, 1.7] {
                                let direction = Vec3::new(
                                    yaw.sin() * pitch.cos(),
                                    pitch.sin(),
                                    -yaw.cos() * pitch.cos(),
                                );
                                // In through this point of the opening, from
                                // `before` in front of it.
                                let through = Vec3::new(across, up, -4.25);
                                // Clear of the frame, which runs a quarter
                                // unit either side of the opening (in the
                                // partner too, turned the same way).
                                let drift = direction * (0.27 / -direction.z);
                                let clear = |p: Vec3| p.x.abs() < 0.93 && p.y > 0.22 && p.y < 2.93;
                                if !(clear(through + drift) && clear(through - drift)) {
                                    continue;
                                }
                                let start = through - direction * before / -direction.z;
                                let velocity = direction * speed;
                                // On through to over half a unit past.
                                let ticks = (((before + 0.6) / -direction.z) / speed * 120.0)
                                    .ceil()
                                    .max(1.0) as usize;
                                // Arrows fall: only quick ones reach the far side unhurt.
                                let arrows = speed >= 60.0;
                                for definition in ["t:projectile/bullet", "t:projectile/arrow"]
                                    .into_iter()
                                    .filter(|d| arrows || !d.ends_with("arrow"))
                                {
                                    let (position, moving) =
                                        expected(&sim, definition, start, velocity, ticks);
                                    // The quickest go a long way in a tick:
                                    // some reach the ground past the partner.
                                    if position.y < 0.05 {
                                        continue;
                                    }
                                    cases += 1;
                                    let mut w = WeaponsWorld::new(pack()).unwrap();
                                    w.spawn(definition, SHOOTER, start, velocity, 1.0).unwrap();
                                    for _ in 0..ticks {
                                        step(&sim, &mut w);
                                    }
                                    let Some(p) = w.projectiles().next() else {
                                        failures.push(format!(
                                            "{definition} {speed} yaw {yaw} pitch {pitch} at \
                                             ({across}, {up}) from {before}: stopped"
                                        ));
                                        continue;
                                    };
                                    if p.position.distance(position) > 2e-3
                                        || p.velocity.distance(moving) > 2e-3
                                        || p.source != SHOOTER
                                    {
                                        failures.push(format!(
                                            "{definition} {speed} yaw {yaw} pitch {pitch} at \
                                             ({across}, {up}) from {before}: at {} going {}, \
                                             not {position} going {moving}",
                                            p.position, p.velocity
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(
            failures.is_empty(),
            "{} of {cases} shots went wrong:\n{}",
            failures.len(),
            failures[..failures.len().min(12)].join("\n")
        );
    }

    /// Each weapon fired up close, its muzzle reaching past the opening,
    /// and from further back: the shot always comes out of the partner.
    #[test]
    fn each_weapon_fires_through_from_any_distance() {
        let sim = doorways();
        for weapon in ["gun", "rocket", "bow", "sword"] {
            // Below zero the eye leads the body's middle through.
            for distance in [3.0f32, 0.9, 0.35, 0.1, 0.01, -0.05] {
                for (yaw, across) in [(0.0f32, 0.0f32), (0.3, -0.4), (-0.2, 0.5)] {
                    let mut w = WeaponsWorld::new(pack()).unwrap();
                    w.add_actor(SHOOTER, 5).unwrap();
                    w.give(SHOOTER, &format!("t:weapon/{weapon}")).unwrap();
                    w.equip(SHOOTER, Some(0)).unwrap();
                    let direction = Vec3::new(yaw.sin(), 0.0, -yaw.cos());
                    let eye = Vec3::new(across, 1.9, -4.25 + distance);
                    // The gun is held to the right and ahead of the eye.
                    let right = Vec3::new(yaw.cos(), 0.0, yaw.sin());
                    let muzzle = eye + direction * 0.7 + right * 0.25 - Vec3::Y * 0.2;
                    // The eye is a little ahead of the middle.
                    let middle = eye - Vec3::Y * 0.6 - direction * 0.15;
                    w.set_frame(
                        SHOOTER,
                        WeaponFrame {
                            position: eye - Vec3::Y * 1.9,
                            eye,
                            muzzle: [muzzle; 2],
                            direction,
                            middle: Some(middle),
                            ..Default::default()
                        },
                    )
                    .unwrap();
                    for _ in 0..4 {
                        step(&sim, &mut w);
                    }
                    w.trigger(SHOOTER, true).unwrap();
                    let mut fired = false;
                    for _ in 0..30 {
                        let events = step(&sim, &mut w);
                        fired |= events
                            .iter()
                            .any(|e| matches!(e, bri_weapons::Event::Spawned { .. }));
                    }
                    assert!(fired, "{weapon} never fired");
                    let p = w
                        .projectiles()
                        .next()
                        .unwrap_or_else(|| panic!("{weapon} from {distance}: shot stopped"));
                    // Out of the north side of the partner at x = 10.25,
                    // going +x (north turned a quarter).
                    assert!(
                        p.position.x > 10.3 && p.velocity.x > 0.0 && p.source == SHOOTER,
                        "{weapon} from {distance}, yaw {yaw}: at {} going {}",
                        p.position,
                        p.velocity
                    );
                }
            }
        }
    }
}
