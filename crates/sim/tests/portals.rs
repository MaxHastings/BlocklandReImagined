//! Linked bricks: pairing by name and walking through their openings, with
//! a made-up doorway brick (no converted content needed).
use bri_content::collision::{CollisionBody, Part};
use bri_sim::{
    definitions::Definitions,
    player::{MoveInput, Player, PlayerTuning},
    simulation::Simulation,
};
use bri_world::{Brick, ContentRef, World};
use glam::Vec3;
use rapier3d::prelude::*;

const PORTAL: &str = "portal";
const BIG: &str = "big_portal";
const HUGE: &str = "huge_portal";
const WALL: &str = "wall";

/// The synthetic portal doorway ([`bri_sim::testing::portal`]), the same
/// stretched to 1x14x10 (7 wide, 6 tall) and 1x20x12 (10 wide, 7.2 tall) as
/// the Portal Add-On's bigger ones are, and a 1x4x5 solid wall.
fn definitions() -> Definitions {
    use bri_sim::testing::portal;
    let mut wall = portal(WALL, None);
    wall.link = None;
    wall.collision = CollisionBody {
        id: WALL.into(),
        parts: vec![Part::Box {
            center: [0.0; 3],
            size: [2.0, 3.0, 0.5],
        }],
    };
    wall.shape = bri_physics::content::collider(&wall.collision)
        .unwrap()
        .build()
        .shared_shape()
        .clone();
    Definitions {
        entries: [
            (BIG.to_string(), portal(BIG, Some([14, 1, 30]))),
            (HUGE.to_string(), portal(HUGE, Some([20, 1, 36]))),
            (PORTAL.to_string(), portal(PORTAL, None)),
            (WALL.to_string(), wall),
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
    simulation_with(bricks, vec![])
}

/// [`simulation`] with more fixed colliders on the ground.
fn simulation_with(bricks: Vec<Brick>, more: Vec<ColliderBuilder>) -> Simulation {
    let mut world = World::new("Portals".into(), "test".into(), vec![[1.0; 4]]);
    for (i, brick) in bricks.into_iter().enumerate() {
        world.bricks.insert(i as u64 + 1, brick);
        world.next_brick_id = i as u64 + 2;
    }
    Simulation::new(
        world,
        definitions(),
        std::iter::once(
            ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0)),
        )
        .chain(more)
        .collect(),
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

/// Through an opening a body jumps to the partner; it does not travel the
/// distance between them in one physics step. Driven there as its next
/// kinematic pose, it swept from the doorway at twenty units a step and
/// the contact solver threw what it touched (here a crate against its
/// side, toward the partner) at about 400 u/s.
#[test]
fn walking_through_does_not_fling_what_the_walker_touches() {
    let mut sim = simulation(vec![
        brick(BIG, [0.0, 3.0, -4.25], 0, Some("Portal_a")),
        brick(BIG, [20.25, 3.0, -4.0], 1, Some("Portal_a")),
    ]);
    // Standing on the sill just short of the doorway, a crate against its
    // side toward the partner, the way the carry goes.
    let mut player = spawn(&mut sim, Vec3::new(0.0, 0.3, -3.6));
    let body = sim.physics.colliders[player.collider()].compute_aabb();
    let (crate_, _) = sim.physics.insert(
        RigidBodyBuilder::dynamic().translation(Vector::new(
            body.maxs.x + 0.2,
            body.mins.y + 0.25,
            -3.8,
        )),
        ColliderBuilder::cuboid(0.2, 0.2, 0.4)
            .density(10.0)
            .friction(0.0),
    );
    bri_physics::detect_collisions(&mut sim.physics);
    for _ in 0..60 {
        sim.step_body(&mut player, MoveInput::default(), &[])
            .unwrap();
        sim.step().unwrap();
    }
    let (mut passed, mut fastest) = (false, 0.0f32);
    for _ in 0..240 {
        let input = MoveInput {
            forward: 0.4,
            ..Default::default()
        };
        let events = sim.step_body(&mut player, input, &[]).unwrap();
        passed |= events.passed.is_some();
        sim.step().unwrap();
        fastest = fastest.max(sim.physics.bodies[crate_].linvel().length());
    }
    assert!(passed, "the walker went through");
    assert!(fastest < 15.0, "the crate was thrown at {fastest} u/s");
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
        ActorId, Filter, Frame as WeaponFrame, Hit, Nearby, Pack, Query, TargetId, WeaponsWorld,
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
            r#"{{ "schema_version": 5, "id": "t",
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
            ally: &|_, _| false,
            catch: &catch,
            responses: &responses,
            truncated_targets: 0,
            shapes: &[],
            rewound: None,
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
        free.spawn(definition, SHOOTER, start, velocity, 1.0)
            .unwrap();
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
            Some(c) => (
                c.transform_point3(p.position),
                c.transform_vector3(p.velocity),
            ),
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

/// Bodies bigger than a player: a Steel Ball and jeep- and tank-sized
/// boxes go through the big portal whole, turned with their speed and spin, and the
/// small one's frame stops them.
mod vehicles {
    use super::*;
    use bri_sim::session::carry_through_openings;
    use bri_vehicles::{OwnerId, Pack, Spawn, Transform, VehicleId, VehiclesWorld};
    use std::collections::BTreeMap;

    const BALL: &str = "steel-ball-kit:vehicle/steelball";
    const PLANE: &str = "test_plane:vehicle/standinplane";
    const JEEP: &str = "steel-ball-kit:vehicle/jeepbox";
    const TANK: &str = "steel-ball-kit:vehicle/tankbox";

    /// The Steel Ball as its Add-On ships it, and boxes built from it the
    /// size of the stock Jeep's hull (3.21 wide, 2.63 tall, 5.36 long) and
    /// of the stock Tank's hull with its turret on (4.7 wide, 4.4 tall,
    /// 6.44 long), as the converted vehicle pack measures them.
    fn pack() -> Pack {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/showcase/steel-ball-kit/assets/vehicles.json"
        );
        let mut pack: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        for (id, half) in [(JEEP, [1.605f32, 1.315, 2.68]), (TANK, [2.35, 2.2, 3.22])] {
            let mut body = pack["definitions"][0].clone();
            let corners: Vec<[f32; 3]> = (0..8)
                .map(|i| std::array::from_fn(|a| if i >> a & 1 == 1 { half[a] } else { -half[a] }))
                .collect();
            body["id"] = id.into();
            // A plain body of its hulls (a ball is a sphere round its bounds).
            body["family"] = "Tumble".into();
            body["datablock"] = "BoxVehicle".into();
            body["collision_hulls"] = serde_json::json!([corners]);
            body["bounds_min"] = serde_json::json!(half.map(|h| -h));
            body["bounds_max"] = serde_json::json!(half);
            body["mass"] = 300.0.into();
            body["friction"] = 0.0.into();
            body["smash"] = serde_json::Value::Null;
            pack["definitions"].as_array_mut().unwrap().push(body);
        }
        let pack: Pack = serde_json::from_value(pack).unwrap();
        pack.validate().unwrap();
        pack
    }

    /// The stand-in plane: its wings are wider than the 1x14x10 opening
    /// (6.9) but fit the 1x20x12's (9.9), though only its narrow body
    /// collides, as a v20 plane's does.
    fn planes() -> Pack {
        Pack::load(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../vehicles/tests/fixtures/stand-in-plane/assets/vehicles.json"
        ))
        .unwrap()
    }

    /// Two big portals and two small ones, all of one name: each size
    /// pairs only with its own. Two of the biggest stand in the air at
    /// x = 60 and 100, the second turned.
    fn portals() -> Simulation {
        simulation(vec![
            brick(BIG, [0.0, 3.0, -4.25], 0, Some("Portal_a")),
            brick(BIG, [20.25, 3.0, -4.0], 1, Some("Portal_a")),
            brick(PORTAL, [-20.0, 1.5, -4.25], 0, Some("Portal_a")),
            brick(PORTAL, [-40.25, 1.5, -4.0], 1, Some("Portal_a")),
            brick(HUGE, [60.0, 13.6, -4.25], 0, Some("Portal_a")),
            brick(HUGE, [100.25, 13.6, -4.0], 1, Some("Portal_a")),
        ])
    }

    type Motion = (Vec3, Vec3, glam::Quat);
    struct Run {
        /// The carry made, with the motion (velocity, spin and turn) just
        /// before and just after it.
        carried: Option<(glam::Affine3A, Motion, Motion)>,
        centre: Vec3,
        velocity: Vec3,
        /// The middle after every tick, and whether it had been carried.
        path: Vec<(Vec3, bool)>,
        /// The most its nose (-z) pointed up before it was carried.
        nose_up: f32,
        /// The most openings it was part way into at once, and how many it
        /// still is at the end.
        most_straddles: usize,
        straddles_left: usize,
    }

    /// `definition` sent at `velocity` from `at` for `ticks`, the host's
    /// order each tick: vehicles before, the world, vehicles after, then
    /// through any opening.
    fn run(pack: Pack, definition: &str, at: Vec3, velocity: Vec3, ticks: usize) -> Run {
        run_in(portals(), pack, definition, at, velocity, ticks)
    }

    /// [`run`] in `sim`. Vehicles step against its openings as the host's
    /// do, and the shared step keeps the contact rules of bodies part way
    /// through them.
    fn run_in(
        mut sim: Simulation,
        pack: Pack,
        definition: &str,
        at: Vec3,
        velocity: Vec3,
        ticks: usize,
    ) -> Run {
        let mut world = VehiclesWorld::new(pack).unwrap();
        let id = VehicleId(1);
        world
            .spawn(
                &mut sim.physics,
                Spawn {
                    id,
                    owner: OwnerId(1),
                    definition: definition.into(),
                    transform: Transform {
                        position: at.to_array(),
                        ..Default::default()
                    },
                    spawn_id: None,
                    respawn_ticks: None,
                    scale: 1.0,
                },
            )
            .unwrap();
        world
            .set_velocity(&mut sim.physics, id, velocity.to_array())
            .unwrap();
        let motion = |world: &VehiclesWorld, sim: &Simulation| {
            let s = world.vehicle_snapshot(&sim.physics, id).unwrap();
            (
                Vec3::from(s.velocity),
                Vec3::from(s.angular_velocity),
                glam::Quat::from_array(s.transform.rotation),
            )
        };
        let mut carried = None;
        let (mut path, mut most_straddles, mut nose_up) = (Vec::new(), 0, f32::MIN);
        for _ in 0..ticks {
            sim.step_vehicle_bodies(&mut world, &[]).unwrap();
            most_straddles = most_straddles.max(world.straddles());
            let before = BTreeMap::from([(id, world.centre(&sim.physics, id).unwrap())]);
            let hooks = world.portal_contacts().clone();
            sim.step_with(&hooks).unwrap();
            world.post_step(&mut sim.physics).unwrap();
            let going = motion(&world, &sim);
            let passages = sim.passages().clone();
            if let Some(&(_, carry)) =
                carry_through_openings(&mut world, &mut sim.physics, &passages, &before)
                    .unwrap()
                    .first()
            {
                assert!(carried.is_none(), "{definition}: carried twice");
                carried = Some((carry, going, motion(&world, &sim)));
            }
            path.push((world.centre(&sim.physics, id).unwrap(), carried.is_some()));
            if carried.is_none() {
                nose_up = nose_up.max((motion(&world, &sim).2 * Vec3::NEG_Z).y);
            }
            world.drain_intents();
        }
        let (velocity, _, _) = motion(&world, &sim);
        Run {
            carried,
            centre: world.centre(&sim.physics, id).unwrap(),
            velocity,
            path,
            nose_up,
            most_straddles,
            straddles_left: world.straddles(),
        }
    }

    /// The run was carried, keeping its speed, spin and turn as the
    /// portal turns them.
    fn kept(run: &Run, definition: &str) {
        let Some((carry, before, after)) = run.carried else {
            panic!(
                "{definition}: not carried, at {} going {}",
                run.centre, run.velocity
            );
        };
        let (_, turn, _) = carry.to_scale_rotation_translation();
        assert!(
            after.0.distance(turn * before.0) < 1e-3,
            "{definition}: {before:?} {after:?}"
        );
        assert!(
            after.1.distance(turn * before.1) < 1e-3,
            "{definition}: {before:?} {after:?}"
        );
        assert!(
            after.2.dot(turn * before.2).abs() > 1.0 - 1e-5,
            "{definition}: turned wrong"
        );
    }

    #[test]
    fn a_plane_flies_through_the_biggest_portal_wings_and_all() {
        let span = {
            let p = planes();
            let d = p.definitions.iter().find(|d| d.id == PLANE).unwrap();
            d.bounds_max[0] - d.bounds_min[0]
        };
        // Too wide for the 1x14x10 (6.9 inside), within the 1x20x12's 9.9.
        assert!(span > 6.9 && span < 9.9, "{span}");
        for across in [-0.3f32, 0.0, 0.4] {
            for speed in [40.0f32, 80.0] {
                // Flying level at the opening's middle height, nose first.
                let at = Vec3::new(60.0 + across, 13.0, -4.25 + 6.0);
                let run = run(planes(), PLANE, at, Vec3::new(0.0, 0.0, -speed), 60);
                kept(&run, PLANE);
                // Out of the partner at x = 100.25, flying +x.
                assert!(
                    run.centre.x > 103.0 && run.velocity.x > speed * 0.8,
                    "{speed} at {across}: at {} going {}",
                    run.centre,
                    run.velocity
                );
            }
        }
    }

    #[test]
    fn a_steel_ball_a_jeep_and_a_tank_go_through_the_big_portal_whole() {
        // The ball rolls over the sill; the boxes (no wheels) are sent in
        // fast enough to clear it before they settle. Off centre with room
        // to spare: the opening is 6.9 wide and 5.75 tall inside.
        let cases = [
            (BALL, 1.26f32, 15.0f32, [-1.5f32, 0.0, 1.5]),
            (JEEP, 1.7, 30.0, [-1.4, 0.0, 1.4]),
            (TANK, 2.8, 30.0, [-0.8, 0.0, 0.8]),
        ];
        for (definition, rest, speed, offsets) in cases {
            for across in offsets {
                let at = Vec3::new(across, rest, -4.25 + 3.5);
                let run = run(pack(), definition, at, Vec3::new(0.0, 0.0, -speed), 120);
                kept(&run, definition);
                // Out of the north side of the partner at x = 20.25,
                // going +x.
                assert!(
                    run.centre.x > 22.0 && run.velocity.x > 5.0,
                    "{definition} at {across}: at {} going {}",
                    run.centre,
                    run.velocity
                );
            }
        }
    }

    /// The two big portals of [`portals`] alone, with `more` fixed colliders.
    fn big_pair_with(more: Vec<ColliderBuilder>) -> Simulation {
        simulation_with(
            vec![
                brick(BIG, [0.0, 3.0, -4.25], 0, Some("Portal_a")),
                brick(BIG, [20.25, 3.0, -4.0], 1, Some("Portal_a")),
            ],
            more,
        )
    }

    #[test]
    fn a_jeep_drives_through_a_portal_set_against_a_wall() {
        // A wall flush behind the first portal: the part of the jeep past
        // its plane is beyond the partner, so the wall never stops it.
        let wall = ColliderBuilder::cuboid(10.0, 5.0, 0.5).translation(Vector::new(0.0, 5.0, -5.0));
        let at = Vec3::new(0.0, 1.7, -4.25 + 3.5);
        let run = run_in(
            big_pair_with(vec![wall]),
            pack(),
            JEEP,
            at,
            Vec3::new(0.0, 0.0, -30.0),
            120,
        );
        kept(&run, JEEP);
        assert!(
            run.centre.x > 22.0 && run.velocity.x > 5.0,
            "at {} going {}",
            run.centre,
            run.velocity
        );
        assert!(run.most_straddles >= 1);
        assert_eq!(run.straddles_left, 0, "nothing straddled once through");
    }

    #[test]
    fn a_wall_just_past_the_partner_stops_the_nose_before_the_middle_crosses() {
        // The jeep is 5.36 long. The partner's opening is its brick's face
        // at x = 20, and a wall 2 past it meets the jeep's nose while its
        // middle is still 0.68 short of the first opening (z = -4), so it
        // stops there, never carried, and never thrown back.
        let face = 20.0 + 2.0;
        let wall =
            ColliderBuilder::cuboid(0.5, 5.0, 10.0).translation(Vector::new(face + 0.5, 5.0, -4.0));
        let at = Vec3::new(0.0, 1.7, -4.25 + 3.5);
        let run = run_in(
            big_pair_with(vec![wall]),
            pack(),
            JEEP,
            at,
            Vec3::new(0.0, 0.0, -10.0),
            200,
        );
        assert!(
            run.carried.is_none(),
            "carried into the wall: {}",
            run.centre
        );
        // Stopped against it: the nose at the wall, the middle 2.68 back.
        let short = run.centre.z + 4.0;
        assert!(
            (0.6..1.0).contains(&short),
            "middle {short} short of the opening"
        );
        assert!(run.velocity.length() < 1.0, "still going {}", run.velocity);
        // It came to rest against it after a small rebound, as off a wall
        // standing there (it meets it while riding the sill, so it tips a
        // little), not thrown back through the portal.
        let nearest = run.path.iter().map(|(p, _)| p.z).fold(f32::MAX, f32::min);
        assert!(
            run.centre.z - nearest < 0.4,
            "bounced back {}",
            run.centre.z - nearest
        );
    }

    #[test]
    fn a_car_s_front_wheels_ride_onto_a_plank_past_the_partner_before_it_crosses() {
        // A plank 0.3 high just past the partner. The front wheels go past
        // the first opening's plane well before the car's middle does, and
        // they ride up onto it there, lifting the car's nose early.
        let face = 20.0;
        let plank = ColliderBuilder::cuboid(1.0, 0.15, 3.0).translation(Vector::new(
            face + 0.3 + 1.0,
            0.15,
            -4.0,
        ));
        let car = bri_vehicles::testing::CAR;
        let at = Vec3::new(0.0, 0.6, -4.25 + 5.0);
        let run = run_in(
            big_pair_with(vec![plank]),
            // Coasting, with nothing braking it.
            bri_vehicles::testing::pack_with(|d| d.engine_brake = 0.0),
            car,
            at,
            Vec3::new(0.0, 0.0, -15.0),
            240,
        );
        kept(&run, car);
        // 0.3 up over the 3.2 between its axles.
        assert!(
            run.nose_up > 0.06,
            "nose up {} before crossing",
            run.nose_up
        );
        // And the crossing itself is smooth: no tick moves the middle much
        // further than the car travels.
        for pair in run.path.windows(2) {
            let ((a, ca), (b, cb)) = (pair[0], pair[1]);
            if ca == cb {
                assert!((b - a).y.abs() < 0.08, "jolted {} at {a}", (b - a).y);
            }
        }
        assert_eq!(run.straddles_left, 0);
    }

    #[test]
    fn the_small_portal_stops_what_does_not_fit() {
        let cases = [
            (BALL, 1.26f32, 15.0f32),
            (JEEP, 1.7, 30.0),
            (TANK, 2.8, 30.0),
        ];
        for (definition, rest, speed) in cases {
            let at = Vec3::new(-20.0, rest, -4.25 + 3.5);
            let run = run(pack(), definition, at, Vec3::new(0.0, 0.0, -speed), 120);
            assert!(run.carried.is_none(), "{definition} went through");
            assert!(run.centre.z > -4.25, "{definition}: at {}", run.centre);
        }
    }

    /// The ticks of `count` jeeps parked, each in front of its own pair of
    /// big portals: part way into the first (`into`) or well back from it.
    /// Returns the time of each tick's host order and every jeep's final
    /// pose, bit for bit.
    fn parked(
        count: usize,
        into: bool,
        ticks: usize,
    ) -> (Vec<std::time::Duration>, Vec<[u32; 7]>, usize, f32) {
        let mut bricks = Vec::new();
        for i in 0..count {
            let x = i as f32 * 60.0;
            let name = format!("Portal_{i}");
            bricks.push(brick(BIG, [x, 3.0, -4.25], 0, Some(&name)));
            bricks.push(brick(BIG, [x + 20.25, 3.0, -4.0], 1, Some(&name)));
        }
        let ground =
            ColliderBuilder::cuboid(2000.0, 0.5, 200.0).translation(Vector::new(0.0, -0.5, 0.0));
        let mut sim = simulation_with(bricks, vec![ground]);
        let mut world = VehiclesWorld::new(pack()).unwrap();
        for i in 0..count {
            // The jeep is 5.36 long: its middle 0.7 short of the plane puts
            // two units of its nose past it.
            let z = if into { -4.0 + 0.7 } else { 20.0 };
            world
                .spawn(
                    &mut sim.physics,
                    Spawn {
                        id: VehicleId(i as u64 + 1),
                        owner: OwnerId(1),
                        definition: JEEP.into(),
                        transform: Transform {
                            position: [i as f32 * 60.0, 1.7, z],
                            ..Default::default()
                        },
                        spawn_id: None,
                        respawn_ticks: None,
                        scale: 1.0,
                    },
                )
                .unwrap();
        }
        // Kept awake, so both layouts pay for the same simulated bodies and
        // the difference is what the split costs.
        for (_, body) in sim
            .physics
            .bodies
            .iter_mut()
            .filter(|(_, b)| b.is_dynamic())
        {
            body.activation_mut().normalized_linear_threshold = -1.0;
            body.activation_mut().angular_threshold = -1.0;
        }
        // The top of the first opening's sill, which the jeep rests on.
        let sill = 5.0
            - sim
                .physics
                .query_pipeline_with_filter(QueryFilter::only_fixed())
                .cast_ray(
                    &Ray::new(Vec3::new(0.0, 5.0, -4.25), Vec3::NEG_Y),
                    10.0,
                    true,
                )
                .unwrap()
                .1;
        let mut times = Vec::new();
        let mut most = 0;
        for _ in 0..ticks {
            let started = std::time::Instant::now();
            sim.step_vehicle_bodies(&mut world, &[]).unwrap();
            most = most.max(world.straddles());
            let before: BTreeMap<_, _> = (0..count)
                .map(|i| VehicleId(i as u64 + 1))
                .map(|id| (id, world.centre(&sim.physics, id).unwrap()))
                .collect();
            let hooks = world.portal_contacts().clone();
            sim.step_with(&hooks).unwrap();
            world.post_step(&mut sim.physics).unwrap();
            let passages = sim.passages().clone();
            carry_through_openings(&mut world, &mut sim.physics, &passages, &before).unwrap();
            times.push(started.elapsed());
        }
        let poses = (0..count)
            .map(|i| {
                let t = world
                    .vehicle_snapshot(&sim.physics, VehicleId(i as u64 + 1))
                    .unwrap()
                    .transform;
                let [a, b, c] = t.position;
                let [d, e, f, g] = t.rotation;
                [a, b, c, d, e, f, g].map(f32::to_bits)
            })
            .collect();
        (times, poses, most, sill)
    }

    #[test]
    fn stress_vehicles_part_way_through_openings_cost_and_repeat() {
        // Diagnostic timings are printed, never thresholded; the repeat is
        // checked bit for bit. `BRI_STRESS_VEHICLES` sets the count.
        let count = std::env::var("BRI_STRESS_VEHICLES")
            .ok()
            .and_then(|n| n.parse().ok())
            .unwrap_or(8);
        // The box jeep is a Tumble, cleared at 240 ticks when slow.
        let ticks = 230;
        let median = |mut t: Vec<std::time::Duration>| {
            let t = t.split_off(60);
            let mut micros: Vec<_> = t.iter().map(|d| d.as_secs_f64() * 1e6).collect();
            micros.sort_by(f64::total_cmp);
            (micros[micros.len() / 2], micros[micros.len() * 99 / 100])
        };
        let (mut back, back_poses, none, sill) = parked(count, false, ticks);
        let (mut into, into_poses, most, _) = parked(count, true, ticks);
        let (more, again, _, _) = parked(count, true, ticks);
        into.extend(more.into_iter().skip(60));
        back.extend(parked(count, false, ticks).0.into_iter().skip(60));
        assert_eq!(none, 0, "parked back, nothing is part way in");
        assert_eq!(most, count, "each is part way into its opening");
        assert_eq!(into_poses, again, "the same run comes out the same");
        // Parked, they stay put: no creep, hop or carry from the split.
        // Part way in, each rests on the opening's sill.
        for (a, b) in into_poses.iter().zip(&back_poses) {
            let y = |p: &[u32; 7]| f32::from_bits(p[1]);
            assert!(
                (y(a) - y(b) - sill).abs() < 0.05,
                "rests on the {sill} sill: {} vs {}",
                y(a),
                y(b)
            );
            assert!(
                (f32::from_bits(a[2]) + 3.3).abs() < 0.1,
                "and stays where it was parked: {}",
                f32::from_bits(a[2])
            );
        }
        let (back, back99) = median(back);
        let (into, into99) = median(into);
        eprintln!(
            "{count} jeeps, host order per tick (median / p99 us): back {back:.0} / {back99:.0}, part way in {into:.0} / {into99:.0}"
        );
    }
}

#[test]
fn aiming_into_a_portal_reaches_what_stands_past_its_partner() {
    // Looking north into a doorway whose partner faces east: tools, clicks
    // and the brick in hand reach the wall standing east of the partner,
    // not the wall right behind the first doorway.
    let sim = simulation(vec![
        brick(PORTAL, [0.0, 1.5, -4.25], 0, Some("Portal_a")),
        brick(WALL, [0.0, 1.5, -4.75], 0, None),
        brick(PORTAL, [10.25, 1.5, -4.0], 1, Some("Portal_a")),
        brick(WALL, [14.25, 1.5, -4.0], 1, None),
    ]);
    let eye = Vec3::new(0.0, 1.5, 0.0);
    let (hit, leg) = sim.target_through(eye, Vec3::NEG_Z, 20.0).unwrap().unwrap();
    assert_eq!(hit.brick, Some(4), "{hit:?}");
    assert!(leg.direction.abs_diff_eq(Vec3::X, 1e-4), "{leg:?}");
    assert!(hit.normal.abs_diff_eq(Vec3::NEG_X, 1e-3), "{hit:?}");
    // Its distance runs along the sight: four units in, four past.
    assert!((hit.distance - 8.0).abs() < 0.01, "{hit:?}");
    // Where it stands is as near as it looks: along the way it is seen by.
    let (near, carry) = sim.passages().shortest(eye, hit.position);
    assert!(carry.is_some() && near < 10.0, "{near}");
    // A straight ray never gets there.
    let straight = sim.target(eye, Vec3::NEG_Z, 20.0).unwrap();
    assert_ne!(straight.and_then(|h| h.brick), Some(4));
    // A click's five units end before that wall.
    assert_eq!(sim.activate(eye, Vec3::NEG_Z).unwrap(), None);
}
