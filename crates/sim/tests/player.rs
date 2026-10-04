use bri_sim::player::{MotionEvents, MoveInput, Player, PlayerTuning};
use glam::Vec3;
use rapier3d::prelude::*;
fn scene() -> PhysicsWorld {
    let mut world = bri_physics::new_world();
    world.insert_collider(
        ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0)),
        None,
    );
    world.detect_collisions(&(), &());
    world
}
#[test]
fn native_water_buoyancy_drag_and_exit_share_the_player_motor() {
    use bri_content::{environment::Image, water::Water};
    let image = Image {
        file: "fixture.png".into(),
        source: "fixture".into(),
        sha256: "0".repeat(64),
        width: 1,
        height: 1,
    };
    let water = Water {
        schema_version: 1,
        node: 0,
        id: "fixture".into(),
        min: [-100., -10., -100.],
        max: [100., 10., 100.],
        repeat_period: None,
        liquid_type: "OceanWater".into(),
        density: 1.,
        viscosity: 40.,
        surface: image.clone(),
        shore: image,
        reflection: None,
        opacity: 0.25,
        wave_amplitude: 0.,
        flow: [0.; 2],
        distortion: [0., 0., 1.],
        tiles: [1.; 2],
        depth_mask: false,
        depth_alpha: [0., 1., 20., 1.],
        reflection_intensity: 0.,
        parallax: 0.,
        warnings: vec![],
        current: [0.0; 3],
    };
    water.validate().unwrap();
    let mut world = scene();
    let mut player = spawn(&mut world);
    let waters = [water];
    for _ in 0..120 {
        player
            .step_in_water(
                &mut world,
                MoveInput {
                    forward: 1.,
                    ..Default::default()
                },
                &waters,
            )
            .unwrap();
        world.step();
    }
    assert!(
        player.state().feet[1] > 1.0,
        "Submerged player did not rise"
    );
    assert!(
        player.state().crouched,
        "v20 crouches fully submerged players"
    );
    // v20 swim push 0.5 per 32 ms tick, then drag 0.1 * viscosity 40 takes
    // 12.8% of the speed: 0.5 * 0.872 / 0.128 = 3.41.
    assert!(
        (-3.5..-3.3).contains(&player.state().velocity[2]),
        "Swim speed/drag not applied: {:?}",
        player.state()
    );
    for _ in 0..2400 {
        player
            .step_in_water(&mut world, MoveInput::default(), &waters)
            .unwrap();
        world.step();
    }
    assert!(
        (player.state().feet[1] - (10.0 - 2.65 * 0.7)).abs() < 0.03,
        "Floating equilibrium diverged: {:?}",
        player.state()
    );
    assert!(
        player.state().velocity[2].abs() < 0.001,
        "Liquid drag did not stop idle momentum"
    );
    assert!(!player.state().crouched, "a floating player stands");
    // Holding crouch dives to the bottom and holds the player there.
    for _ in 0..360 {
        player
            .step_in_water(
                &mut world,
                MoveInput {
                    crouch: true,
                    ..Default::default()
                },
                &waters,
            )
            .unwrap();
        world.step();
    }
    assert!(
        player.state().grounded,
        "Crouch did not dive: {:?}",
        player.state()
    );
    // v20 rests a player 0.01 (the post-hit back-off) above the floor.
    assert!(player.state().feet[1] < 0.011, "{:?}", player.state());
    // Holding jump swims back up faster than floating does.
    let mut surfaced = None;
    for tick in 0..240 {
        player
            .step_in_water(
                &mut world,
                MoveInput {
                    jump: true,
                    ..Default::default()
                },
                &waters,
            )
            .unwrap();
        world.step();
        if surfaced.is_none() && player.state().feet[1] > 10.0 - 2.65 {
            surfaced = Some(tick);
        }
    }
    assert!(surfaced.is_some_and(|tick| tick < 120), "{surfaced:?}");
    for _ in 0..600 {
        player
            .step_in_water(&mut world, MoveInput::default(), &waters)
            .unwrap();
        world.step();
    }
    let height = player.state().feet[1];
    for _ in 0..60 {
        player
            .step_in_water(&mut world, MoveInput::default(), &[])
            .unwrap();
        world.step();
    }
    assert!(
        player.state().feet[1] < height - 1.0,
        "Water force leaked after leaving liquid"
    );
}
fn step(p: &mut Player, w: &mut PhysicsWorld, input: MoveInput, n: usize) {
    for _ in 0..n {
        p.step(w, input).unwrap();
        w.step();
    }
}
/// Step until the motor runs its next 32 ms Torque tick: that tick's events.
fn tick(p: &mut Player, w: &mut PhysicsWorld, input: MoveInput) -> MotionEvents {
    loop {
        let events = p.step(w, input).unwrap();
        w.step();
        if events.ticked {
            return events;
        }
    }
}
fn spawn(w: &mut PhysicsWorld) -> Player {
    let mut p = Player::spawn(w, 1, Vec3::new(0.0, 0.05, 0.0), PlayerTuning::default()).unwrap();
    step(&mut p, w, MoveInput::default(), 60);
    assert!(p.state().grounded);
    assert!(p.state().feet[1] > 0.0);
    p
}
#[test]
fn walking_speed_jump_edge_and_landing() {
    let mut world = scene();
    let mut p = spawn(&mut world);
    step(
        &mut p,
        &mut world,
        MoveInput {
            forward: 1.0,
            ..Default::default()
        },
        120,
    );
    // v20 applies drag after the run force: 7 * (1 - 0.1 * 0.032) per tick.
    assert!(
        (p.state().velocity[2] + 6.9776).abs() < 0.001,
        "{:?}",
        p.state()
    );
    assert!(p.state().feet[2] < -6.0);
    step(&mut p, &mut world, MoveInput::default(), 30);
    assert!(Vec3::from(p.state().velocity).length() < 0.01);
    let jump = MoveInput {
        jump: true,
        ..Default::default()
    };
    assert!(tick(&mut p, &mut world, jump).jumped);
    step(&mut p, &mut world, jump, 45);
    assert!(p.state().feet[1] > 3.0, "{:?}", p.state());
    step(&mut p, &mut world, MoveInput::default(), 220);
    assert!(p.state().grounded);
    assert!(p.state().feet[1] < 0.02);
    // v20 jumps whenever jump is held and jumpDelay has run out. The delay
    // runs out in the air and the landing reopens the jump, so holding it
    // hops again on the tick after the landing tick.
    let mut landed_at = None;
    let mut rehop_after = None;
    assert!(tick(&mut p, &mut world, jump).jumped);
    for tick in 0..100 {
        let events = self::tick(&mut p, &mut world, jump);
        if events.landed {
            landed_at = Some(tick);
        }
        if events.jumped {
            rehop_after = landed_at.map(|landed| tick - landed);
            break;
        }
    }
    assert_eq!(rehop_after, Some(1), "{landed_at:?}");
}
#[test]
fn a_jump_stays_available_briefly_after_walking_off_a_ledge() {
    let mut world = bri_physics::new_world();
    world.insert_collider(
        ColliderBuilder::cuboid(2.0, 0.5, 2.0).translation(Vector::new(0.0, -0.5, 0.0)),
        None,
    );
    world.detect_collisions(&(), &());
    let mut p = spawn(&mut world);
    let walk = MoveInput {
        forward: 1.0,
        ..Default::default()
    };
    let mut airborne = 0;
    for _ in 0..400 {
        p.step(&mut world, walk).unwrap();
        world.step();
        if !p.state().grounded {
            airborne += 1;
            if airborne == 20 {
                break;
            }
        }
    }
    assert_eq!(airborne, 20, "{:?}", p.state());
    let late = MoveInput { jump: true, ..walk };
    assert!(tick(&mut p, &mut world, late).jumped, "{:?}", p.state());
    assert!(p.state().velocity[1] > 8.0, "{:?}", p.state());
}
#[test]
fn standard_crouch_collision_matches_v20_clearance_at_each_player_scale() {
    // Recovered PlayerStandardArmor authors VectorScale("1.25 1.25 1.00", 4).
    // The v20 collision step quarters that box, then applies object scale.
    // Test real controls against just-wide/high-enough and just-too-small gaps.
    for scale in [0.75, 1.0, 1.5] {
        for (gap_width, gap_height, fits) in
            [(1.29, 1.04, true), (1.21, 1.04, false), (1.29, 0.96, false)]
        {
            let mut world = scene();
            let mut player = Player::spawn(
                &mut world,
                1,
                Vec3::new(0.0, 0.05, 1.0),
                PlayerTuning::default(),
            )
            .unwrap();
            player
                .set_archetype(
                    &mut world,
                    player.state().archetype,
                    PlayerTuning::default(),
                    scale,
                )
                .unwrap();
            step(&mut player, &mut world, MoveInput::default(), 60);
            let half_gap = gap_width * scale * 0.5;
            for side in [-1.0, 1.0] {
                world.insert_collider(
                    ColliderBuilder::cuboid(0.5, 2.0 * scale, 1.0).translation(Vector::new(
                        side * (half_gap + 0.5),
                        2.0 * scale,
                        -2.0,
                    )),
                    None,
                );
            }
            world.insert_collider(
                ColliderBuilder::cuboid(half_gap + 1.0, 0.5 * scale, 1.0).translation(Vector::new(
                    0.0,
                    (gap_height + 0.5) * scale,
                    -2.0,
                )),
                None,
            );
            world.detect_collisions(&(), &());
            let walk = MoveInput {
                forward: 1.0,
                ..Default::default()
            };
            step(&mut player, &mut world, walk, 120);
            assert!(
                player.state().feet[2] > -1.0,
                "standing entered gap: {scale} {gap_width} {gap_height} {:?}",
                player.state()
            );
            let crouch = MoveInput {
                crouch: true,
                ..walk
            };
            if !fits {
                step(&mut player, &mut world, crouch, 240);
                assert!(player.state().crouched);
                assert!(
                    player.state().feet[2] > -1.0,
                    "undersized gap passed: {scale} {gap_width} {gap_height} {:?}",
                    player.state()
                );
                continue;
            }
            for _ in 0..240 {
                step(&mut player, &mut world, crouch, 1);
                if player.state().feet[2] < -1.8 {
                    break;
                }
            }
            assert!(
                player.state().feet[2] < -1.8,
                "crouched body did not enter: {scale} {:?}",
                player.state()
            );
            step(&mut player, &mut world, MoveInput::default(), 60);
            assert!(
                player.state().crouched,
                "stood inside low gap: {scale} {:?}",
                player.state()
            );
            let (min, max) = player.world_bounds();
            let min = Vec3::from(min);
            let max = Vec3::from(max);
            assert!(
                ((max - min) - Vec3::new(1.25, 1.0, 1.25) * scale)
                    .abs()
                    .max_element()
                    < 1e-5
            );
            assert!((min.y - player.state().feet[1]).abs() < 1e-5);
            assert!((player.middle() - 0.5 * scale).abs() < 1e-5);
            let collider = &world.colliders[player.collider()];
            let halves = collider.shape().as_cuboid().unwrap().half_extents;
            assert!((halves.y - 0.5 * scale).abs() < 1e-5);
            step(&mut player, &mut world, walk, 300);
            assert!(
                player.state().feet[2] < -4.0,
                "did not leave gap: {scale} {:?}",
                player.state()
            );
            assert!(
                !player.state().crouched,
                "did not stand after clearing gap: {scale} {:?}",
                player.state()
            );
            let (min, max) = player.world_bounds();
            assert!((max[1] - min[1] - 2.65 * scale).abs() < 1e-5);
        }
    }
}
#[test]
fn crouch_clearance_wall_slide_and_camera_occlusion() {
    let mut world = scene();
    let mut p = spawn(&mut world);
    tick(
        &mut p,
        &mut world,
        MoveInput {
            crouch: true,
            ..Default::default()
        },
    );
    assert!(p.state().crouched);
    let ceiling = world.insert_collider(
        ColliderBuilder::cuboid(3.0, 0.2, 3.0).translation(Vector::new(0.0, 1.6, 0.0)),
        None,
    );
    world.detect_collisions(&(), &());
    step(&mut p, &mut world, MoveInput::default(), 30);
    assert!(p.state().crouched);
    world.remove_collider(ceiling);
    world.detect_collisions(&(), &());
    step(&mut p, &mut world, MoveInput::default(), 1);
    assert!(!p.state().crouched, "{:?}", p.state());
    world.insert_collider(
        ColliderBuilder::cuboid(0.1, 4.0, 10.0).translation(Vector::new(2.0, 4.0, 0.0)),
        None,
    );
    world.insert_collider(
        ColliderBuilder::cuboid(10.0, 4.0, 0.1).translation(Vector::new(0.0, 4.0, 2.0)),
        None,
    );
    world.detect_collisions(&(), &());
    let camera = p.camera(&world, true);
    assert!(camera.z < 1.8 && camera.z > 1.5);
    step(
        &mut p,
        &mut world,
        MoveInput {
            right: 1.0,
            forward: 1.0,
            ..Default::default()
        },
        120,
    );
    assert!(p.state().feet[0] < 1.28);
    assert!(p.state().feet[2] < -3.0);
}
#[test]
fn jets_are_unlimited_and_rejected_inputs_do_not_mutate_state() {
    let mut world = scene();
    let mut p = spawn(&mut world);
    // v20: straight-up thrust barely beats gravity, so a standing jet rises slowly.
    step(
        &mut p,
        &mut world,
        MoveInput {
            jet: true,
            ..Default::default()
        },
        240,
    );
    assert!(p.state().jetting);
    assert!((2.0..6.0).contains(&p.state().feet[1]), "{:?}", p.state());
    assert!(p.state().velocity[1] > 1.0);
    // Jetting forward builds momentum well past running speed.
    step(
        &mut p,
        &mut world,
        MoveInput {
            forward: 1.0,
            jet: true,
            ..Default::default()
        },
        120,
    );
    assert!(p.state().velocity[2] < -15.0, "{:?}", p.state());
    assert!(!p.state().grounded);
    // Releasing the jet keeps that momentum: holding forward does not brake it.
    let carried = p.state().velocity[2];
    step(
        &mut p,
        &mut world,
        MoveInput {
            forward: 1.0,
            ..Default::default()
        },
        30,
    );
    assert!(p.state().velocity[2] < carried * 0.95);
    let before = p.state().clone();
    assert!(
        p.step(
            &mut world,
            MoveInput {
                forward: 2.0,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert_eq!(*p.state(), before);
    assert!(
        p.step(
            &mut world,
            MoveInput {
                yaw: f32::NAN,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert_eq!(*p.state(), before);
    p.despawn(&mut world);
    assert!(world.bodies.is_empty());
    assert_eq!(world.colliders.len(), 1);
}
#[test]
fn plate_steps_are_traversed_without_jump() {
    let mut world = scene();
    let mut p = spawn(&mut world);
    world.insert_collider(
        ColliderBuilder::cuboid(3.0, 0.2, 2.0).translation(Vector::new(0.0, 0.2, -3.0)),
        None,
    );
    world.detect_collisions(&(), &());
    step(
        &mut p,
        &mut world,
        MoveInput {
            forward: 1.0,
            ..Default::default()
        },
        90,
    );
    assert!(p.state().feet[2] < -3.0, "{:?}", p.state());
    assert!((p.state().feet[1] - 0.405).abs() < 0.02);
}
#[test]
fn idle_ground_contact_does_not_sink_or_retrigger_touch() {
    let mut w = scene();
    let floor = w.colliders.iter().next().unwrap().0;
    w.colliders[floor].user_data = 17;
    let mut p = Player::spawn(
        &mut w,
        1,
        Vec3::new(0.0, 0.05, 0.0),
        PlayerTuning::default(),
    )
    .unwrap();
    let mut entered = 0;
    for _ in 0..2400 {
        entered += p
            .step(&mut w, MoveInput::default())
            .unwrap()
            .touched
            .iter()
            .filter(|id| **id == 17)
            .count();
        w.step();
    }
    assert_eq!(entered, 1);
    assert!(p.state().feet[1] > 0.0 && p.state().feet[1] < 0.015);
}

fn walk_forward(p: &mut Player, w: &mut PhysicsWorld, ticks: usize) {
    step(
        p,
        w,
        MoveInput {
            forward: 1.0,
            ..Default::default()
        },
        ticks,
    );
}
/// A solid wedge rising along -Z from `start` over `run` to `rise`, then a landing.
fn ramp(w: &mut PhysicsWorld, start: f32, run: f32, rise: f32) {
    let points = [
        Vector::new(-3.0, 0.0, -start),
        Vector::new(3.0, 0.0, -start),
        Vector::new(-3.0, 0.0, -start - run),
        Vector::new(3.0, 0.0, -start - run),
        Vector::new(-3.0, rise, -start - run),
        Vector::new(3.0, rise, -start - run),
    ];
    w.insert_collider(ColliderBuilder::convex_hull(&points).unwrap(), None);
    w.insert_collider(
        ColliderBuilder::cuboid(3.0, rise * 0.5, 5.0).translation(Vector::new(
            0.0,
            rise * 0.5,
            -start - run - 5.0,
        )),
        None,
    );
    w.detect_collisions(&(), &());
}
#[test]
fn ramps_are_walked_at_running_speed() {
    for (degrees, run) in [(45.0_f32, 3.0_f32), (25.0, 6.0)] {
        let mut w = scene();
        let mut p = spawn(&mut w);
        let rise = run * degrees.to_radians().tan();
        ramp(&mut w, 1.0, run, rise);
        // v20 runs at full speed along the slope: 7 u/s over the ramp's length.
        let length = run / degrees.to_radians().cos();
        // Running off the top of a steep ramp launches the player, as in v20:
        // allow half a second more to land.
        let ticks = ((1.0 + length + 1.0) / 7.0 * 120.0 * 1.25) as usize + 60;
        walk_forward(&mut p, &mut w, ticks);
        let s = p.state();
        assert!(
            s.feet[2] < -1.0 - run,
            "{degrees} degree ramp stalled: {s:?}"
        );
        assert!(
            (s.feet[1] - rise).abs() < 0.05,
            "{degrees} degree ramp: {s:?}"
        );
        assert!(s.grounded);
    }
}
#[test]
fn brick_staircases_and_ledges_are_climbed_without_jumping() {
    // 1x brick stairs: 0.6 risers on 0.5 treads, up to a landing.
    let mut w = scene();
    let mut p = spawn(&mut w);
    for i in 0..6 {
        let height = 0.6 * (i + 1) as f32;
        let depth = if i == 5 { 5.0 } else { 0.25 };
        w.insert_collider(
            ColliderBuilder::cuboid(3.0, height * 0.5, depth).translation(Vector::new(
                0.0,
                height * 0.5,
                -1.25 - 0.5 * i as f32 - depth + 0.25,
            )),
            None,
        );
    }
    w.detect_collisions(&(), &());
    walk_forward(&mut p, &mut w, 150);
    let s = p.state();
    assert!(s.feet[2] < -5.0, "staircase stalled: {s:?}");
    assert!((s.feet[1] - 3.6).abs() < 0.02, "{s:?}");
    // A step above v20's 1.0 maxStepHeight still needs a jump.
    let mut w = scene();
    let mut p = spawn(&mut w);
    w.insert_collider(
        ColliderBuilder::cuboid(3.0, 0.6, 3.0).translation(Vector::new(0.0, 0.6, -4.0)),
        None,
    );
    w.detect_collisions(&(), &());
    walk_forward(&mut p, &mut w, 120);
    assert!(p.state().feet[1] < 0.1 && p.state().feet[2] > -1.0);
    // A plate ledge keeps nearly full running speed.
    let mut w = scene();
    let mut p = spawn(&mut w);
    w.insert_collider(
        ColliderBuilder::cuboid(3.0, 0.1, 5.0).translation(Vector::new(0.0, 0.1, -6.0)),
        None,
    );
    w.detect_collisions(&(), &());
    walk_forward(&mut p, &mut w, 90);
    let s = p.state();
    assert!((s.feet[1] - 0.2).abs() < 0.02, "{s:?}");
    assert!(s.velocity[2] < -6.5, "ledge braked the player: {s:?}");
}

/// A raised floor (`riser` tall) under a ceiling whose underside is `ceiling`
/// above the ground, from z=-1 onward.
fn low_room(w: &mut PhysicsWorld, riser: f32, ceiling: f32) {
    if riser > 0.0 {
        w.insert_collider(
            ColliderBuilder::cuboid(3.0, riser * 0.5, 5.0).translation(Vector::new(
                0.0,
                riser * 0.5,
                -6.0,
            )),
            None,
        );
    }
    w.insert_collider(
        ColliderBuilder::cuboid(3.0, 0.3, 5.0).translation(Vector::new(0.0, ceiling + 0.3, -6.0)),
        None,
    );
    w.detect_collisions(&(), &());
}
#[test]
fn v20_steps_need_only_player_height_beneath_ceilings() {
    // v20 Player::step accepts a step when the player's own height (2.65, or
    // 1.0 crouched) fits above it; it does not need maxStepHeight of headroom.
    // A plate floor inside a five-brick room, a brick step under 4 bricks + 1
    // plate + 1 brick, and a crouched plate under a 1.4 lintel all clear.
    for (riser, ceiling, crouch) in [(0.2, 3.0, false), (0.6, 3.4, false), (0.2, 1.4, true)] {
        let mut w = scene();
        let mut p = spawn(&mut w);
        low_room(&mut w, riser, ceiling);
        let input = MoveInput {
            forward: 1.0,
            crouch,
            ..Default::default()
        };
        step(&mut p, &mut w, input, 150);
        let s = p.state();
        assert!(
            s.feet[2] < -3.0,
            "{riser} step under {ceiling} stalled: {s:?}"
        );
        assert!((s.feet[1] - riser).abs() < 0.02 && s.grounded, "{s:?}");
    }
    // One plate less headroom than the player's height still blocks.
    let mut w = scene();
    let mut p = spawn(&mut w);
    low_room(&mut w, 0.2, 2.8);
    walk_forward(&mut p, &mut w, 150);
    assert!(
        p.state().feet[2] > -1.0 && p.state().feet[1] < 0.011,
        "{:?}",
        p.state()
    );
}
#[test]
fn jumping_under_a_v20_lintel_bumps_the_head_and_keeps_walking() {
    // A four-brick-and-two-plate opening (2.8) clears a standing 2.65 player.
    let mut w = scene();
    let mut p = spawn(&mut w);
    low_room(&mut w, 0.0, 2.8);
    walk_forward(&mut p, &mut w, 60);
    assert!(p.state().feet[2] < -3.0, "{:?}", p.state());
    let jump = MoveInput {
        forward: 1.0,
        jump: true,
        ..Default::default()
    };
    assert!(tick(&mut p, &mut w, jump).jumped);
    let mut peak = 0.0_f32;
    let walk = MoveInput {
        jump: false,
        ..jump
    };
    for _ in 0..60 {
        p.step(&mut w, walk).unwrap();
        w.step();
        peak = peak.max(p.state().feet[1]);
    }
    assert!(peak > 0.1 && peak < 0.16, "head passed the ceiling: {peak}");
    assert!(
        p.state().grounded && p.state().feet[1] < 0.011,
        "{:?}",
        p.state()
    );
    assert!(p.state().velocity[2] < -6.5, "{:?}", p.state());
}
#[test]
fn jumps_add_to_slope_velocity_along_the_surface_normal() {
    // v20 adds jumpForce along the surface normal to the current velocity, so
    // a jump while running up a ramp carries the run's climb with it.
    let mut w = scene();
    let mut p = spawn(&mut w);
    let degrees = 25.0_f32;
    ramp(&mut w, 1.0, 6.0, 6.0 * degrees.to_radians().tan());
    let walk = MoveInput {
        forward: 1.0,
        ..Default::default()
    };
    for _ in 0..240 {
        if p.state().feet[1] > 1.0 && p.state().grounded {
            break;
        }
        p.step(&mut w, walk).unwrap();
        w.step();
    }
    assert!(
        p.state().grounded && p.state().feet[1] > 1.0,
        "{:?}",
        p.state()
    );
    let jump = MoveInput { jump: true, ..walk };
    assert!(tick(&mut p, &mut w, jump).jumped);
    let expected = 7.0 * degrees.to_radians().sin() + 12.0 * degrees.to_radians().cos();
    let rise = p.state().velocity[1];
    assert!((rise - expected).abs() < 0.5, "{rise} vs {expected}");
}
#[test]
fn crouched_jets_push_flat_along_the_facing_without_lift() {
    let mut world = scene();
    let mut p = spawn(&mut world);
    // Jet up first, then crouch-jet with no move input: v20 thrusts along the
    // body's forward axis only, so the player dashes forward and sinks.
    step(
        &mut p,
        &mut world,
        MoveInput {
            jet: true,
            ..Default::default()
        },
        240,
    );
    let start = p.state().clone();
    step(
        &mut p,
        &mut world,
        MoveInput {
            jet: true,
            crouch: true,
            ..Default::default()
        },
        60,
    );
    let s = p.state();
    assert!(s.crouched && s.jetting);
    // Half a second of 2000/90 thrust forward (-Z at yaw 0), less drag.
    assert!(s.velocity[2] < start.velocity[2] - 10.0, "{s:?}");
    assert!(s.velocity[0].abs() < 0.01, "{s:?}");
    assert!(s.velocity[1] < start.velocity[1] - 5.0, "{s:?}");
}
#[test]
fn motion_events_name_the_colliders_the_sweep_hit() {
    let mut w = scene();
    let mut p = spawn(&mut w);
    let wall = w.insert_collider(
        ColliderBuilder::cuboid(3.0, 2.0, 0.25).translation(Vector::new(0.0, 2.0, -2.0)),
        None,
    );
    w.detect_collisions(&(), &());
    let forward = MoveInput {
        forward: 1.0,
        ..Default::default()
    };
    let mut hit = vec![];
    for _ in 0..60 {
        hit.extend(
            p.step(&mut w, forward)
                .unwrap()
                .hits
                .into_iter()
                .map(|(c, _)| c),
        );
        w.step();
    }
    assert!(hit.contains(&wall), "{hit:?}");
}

/// A level slide lane like "Mr.Block's Slides": two 72 degree ramp faces
/// (74.5 degrees: rise 1.8 over run 0.5) facing each other along z, their
/// feet 0.5 apart, so the 1.25 wide box wedges between them.
fn v_lane() -> PhysicsWorld {
    let mut w = bri_physics::new_world();
    for side in [-1.0_f32, 1.0] {
        let mut points = vec![];
        for z in [-200.0, 200.0] {
            points.push(Vector::new(side * 0.25, 0.0, z));
            points.push(Vector::new(side * 0.75, 1.8, z));
            points.push(Vector::new(side * 0.75, 0.0, z));
        }
        w.insert_collider(ColliderBuilder::convex_hull(&points).unwrap(), None);
    }
    w.detect_collisions(&(), &());
    w
}

/// Wedged in a level lane, a v20 rider keeps speeding up with no input: each
/// 32 ms tick gravity and drag apply, `updatePos` removes the speed into one
/// face and then the other (each leaving 0.01 of elasticity), and Torque's
/// crease rule re-aims the remaining speed along the lane. Iterating exactly
/// those per-tick equations from 2 u/s settles at 6.517 u/s after 30 s; the
/// same rules at 120 Hz settle at 3.248, which is why the motor runs
/// Torque's ticks.
#[test]
fn a_wedged_rider_gains_lane_speed_on_v20_ticks() {
    let lane_speed = |torque_tick: Option<f32>| {
        let mut w = v_lane();
        let mut p = Player::spawn(
            &mut w,
            1,
            Vec3::new(0.0, 1.5, -150.0),
            PlayerTuning::default(),
        )
        .unwrap();
        p.set_motion(Vec3::new(0.0, 0.0, 2.0), false);
        let mut speeds = vec![];
        for second in 0..30 {
            match torque_tick {
                Some(dt) => {
                    for _ in 0..(1.0 / dt).round() as usize {
                        p.torque_tick(&mut w, MoveInput::default(), &[], dt)
                            .unwrap();
                    }
                }
                None => step(&mut p, &mut w, MoveInput::default(), 120),
            }
            let v = Vec3::from(p.state().velocity);
            let feet = Vec3::from(p.state().feet);
            // Still wedged in the lane, moving along it.
            assert!(
                feet.x.abs() < 0.01 && (1.3..1.45).contains(&feet.y),
                "{second}: {feet}"
            );
            speeds.push(v.z);
        }
        speeds
    };
    let v20 = lane_speed(None);
    eprintln!("120 Hz steps on 32 ms ticks: {v20:?}");
    let settled = v20[29];
    assert!((settled - 6.517).abs() < 0.01, "settled at {settled}");
    assert!(v20.windows(2).all(|w| w[1] >= w[0] - 0.01), "{v20:?}");
    // Stepping the server at 120 Hz runs exactly Torque's ticks.
    let ticks = lane_speed(Some(0.032));
    assert!((ticks[29] - settled).abs() < 0.2, "{ticks:?}");
    let fast = lane_speed(Some(1.0 / 120.0));
    assert!(
        (fast[29] - 3.248).abs() < 0.05,
        "120 Hz ticks settle at {}",
        fast[29]
    );
}

#[test]
fn a_swimmer_rising_from_the_bottom_stays_crouched_until_it_surfaces() {
    // Slate Sea: the slate lies 9 under the surface. A player spawned on it
    // floats up fully submerged; v20 holds the crouch pose all the way, and
    // standing only once the crouched box breaks the surface.
    // Each 32 ms tick adds buoyancy (1 / 0.7 - 1) x gravity 20 and then
    // removes drag 0.1 x viscosity 40 of the speed, so the rise settles at
    // 8.571 x (1 - 4 x 0.032) / 4 = 1.869 units a second.
    let mut world = scene();
    let waters = [bri_content::water::Water::volume(
        [-100., -91., -100.],
        [100., 9., 100.],
    )];
    let mut player = spawn(&mut world);
    // The crouch state each time it changes; the motor may not tick on the
    // very first step.
    let mut poses = vec![];
    let mut fastest: f32 = 0.0;
    for _ in 0..900 {
        player
            .step_in_water(&mut world, MoveInput::default(), &waters)
            .unwrap();
        world.step();
        fastest = fastest.max(player.state().velocity[1]);
        let crouched = player.state().crouched;
        if poses.last() != Some(&crouched) {
            poses.push(crouched);
        }
    }
    assert!((fastest - 1.869).abs() < 0.005, "rose at {fastest}");
    assert!(
        poses.ends_with(&[true, false]) && poses.iter().filter(|&&c| c).count() == 1,
        "crouch should end once, at the surface: {poses:?}"
    );
    assert!(!player.state().crouched);
    assert!(player.state().feet[1] > 6.0, "{:?}", player.state());
}
/// An eight-sided cone whose faces are 69.7 degrees steep: just inside the
/// 70 degree run angle, like the stock Pine Tree's collision cone.
fn steep_cone(w: &mut PhysicsWorld) {
    let (base, height, radius) = (1.3, 3.0, 1.2_f32);
    let mut points = vec![Vector::new(0.0, base + height, 0.0)];
    for i in 0..8 {
        // A face, not an edge, looks along +x.
        let a = (i as f32 + 0.5) * std::f32::consts::FRAC_PI_4;
        points.push(Vector::new(radius * a.cos(), base, radius * a.sin()));
    }
    w.insert_collider(ColliderBuilder::convex_hull(&points).unwrap(), None);
    w.detect_collisions(&(), &());
}
/// Stand on the cone's +x face, then hold forward and jump facing `yaw`:
/// the first jump's velocity and the surface it jumped off.
fn jump_off_steep_cone(yaw: f32) -> (Vec3, Vec3) {
    let mut w = scene();
    steep_cone(&mut w);
    let face = 1.2 * std::f32::consts::FRAC_PI_8.cos();
    let (x, rise) = (0.9_f32, 4.3 - 0.9 / face * 3.0);
    let mut p = Player::spawn_overlapping(
        &mut w,
        1,
        Vec3::new(x + 0.625, rise + 0.3, 0.0),
        PlayerTuning::default(),
    )
    .unwrap();
    let stand = MoveInput {
        yaw,
        ..Default::default()
    };
    step(&mut p, &mut w, stand, 60);
    assert!(p.state().grounded, "{:?}", p.state());
    let jump = MoveInput {
        forward: 1.0,
        jump: true,
        ..stand
    };
    for _ in 0..8 {
        if tick(&mut p, &mut w, jump).jumped {
            let state = p.state();
            return (Vec3::from(state.velocity), Vec3::from(state.jump.normal));
        }
    }
    panic!("no jump off the cone: {:?}", p.state())
}
#[test]
fn jumping_away_from_a_steep_face_launches_along_the_move() {
    // v20 updateMove (0x5AF94F): a 69.7 degree face is still a run surface,
    // and the jump adds 12 x normal.y up plus, facing away from the face,
    // 12 x (move . normal) along the move. Off a pine tree that launches the
    // player well past run speed; facing into the face it is only a weak hop.
    let slope = (3.0 / (1.2 * std::f32::consts::FRAC_PI_8.cos())).atan();
    let (away, normal) = jump_off_steep_cone(std::f32::consts::FRAC_PI_2);
    assert!((normal.y - slope.cos()).abs() < 0.01, "{normal}");
    // The box's corner may rest on a neighbouring face; the push follows
    // whichever face it jumped off, on top of the tick's run force.
    let push = 12.0 * normal.x;
    assert!(
        push > 7.0 && away.x > push && away.x < push + 2.0,
        "{away} {normal}"
    );
    assert!(away.x > 9.0, "{away}");
    let (into, normal) = jump_off_steep_cone(-std::f32::consts::FRAC_PI_2);
    // Only the tick's run force moves it toward the face; no push.
    assert!(into.x < 0.0 && into.x > -2.0, "{into} {normal}");
    assert!(
        into.y > 0.0 && into.y < 12.0 * normal.y + 0.5,
        "{into} {normal}"
    );
}
#[test]
fn a_held_bunny_hop_carries_speed_from_hop_to_hop() {
    // v20 runs jumpDelay down in the air (0x5AFAC3) and reopens the jump the
    // moment updatePos meets a floor (0x5B175B), so holding jump and forward
    // hops again after one or two ground ticks: each landing costs only that
    // much run-force braking, and speed from a ramp launch carries on.
    let mut w = scene();
    let mut p = spawn(&mut w);
    p.set_motion(Vec3::new(0.0, 0.0, -15.0), true);
    let hop = MoveInput {
        forward: 1.0,
        jump: true,
        ..Default::default()
    };
    let mut speeds = vec![];
    for _ in 0..600 {
        if tick(&mut p, &mut w, hop).jumped {
            let v = p.state().velocity;
            speeds.push(Vec3::new(v[0], 0.0, v[2]).length());
        }
    }
    // Two ground ticks of runForce/mass x 32 ms = 1.536 each per landing.
    assert!(speeds.len() >= 4, "{speeds:?}");
    for pair in speeds[..3].windows(2) {
        let lost = pair[0] - pair[1];
        assert!(lost > 2.5 && lost < 3.3, "{speeds:?}");
    }
    assert!((speeds.last().unwrap() - 6.978).abs() < 0.01, "{speeds:?}");
}

/// Walking into a wall of stacked bricks grazes the upper brick's underside
/// at the seam edge-on. That is no ceiling hit: afterwards, back on level
/// ground with no further blocking hit, the jump still works. (A joiner
/// reported jump dying until they jetted or crouched.) A head bump under a
/// lintel is still a ceiling hit.
#[test]
fn walking_into_a_stacked_brick_wall_keeps_the_jump() {
    let jump = MoveInput {
        jump: true,
        ..Default::default()
    };
    for seam in [0.6_f32, 1.2, 1.8, 2.4] {
        let mut w = scene();
        for (bottom, top) in [(0.0, seam), (seam, seam + 3.0)] {
            w.insert_collider(
                ColliderBuilder::cuboid(3.0, (top - bottom) * 0.5, 0.5).translation(Vector::new(
                    0.0,
                    (top + bottom) * 0.5,
                    -3.0,
                )),
                None,
            );
        }
        w.detect_collisions(&(), &());
        let mut p = spawn(&mut w);
        walk_forward(&mut p, &mut w, 120);
        assert!(
            p.state().feet[2] > -2.6,
            "went through the wall: {:?}",
            p.state()
        );
        assert!(!p.state().jump.ceiling, "seam {seam}: {:?}", p.state().jump);
        let back = MoveInput {
            forward: -1.0,
            ..Default::default()
        };
        step(&mut p, &mut w, back, 60);
        step(&mut p, &mut w, MoveInput::default(), 60);
        assert!(p.state().grounded, "{:?}", p.state());
        assert!(
            tick(&mut p, &mut w, jump).jumped,
            "seam {seam}: {:?}",
            p.state().jump
        );
    }

    let mut w = scene();
    let mut p = spawn(&mut w);
    low_room(&mut w, 0.0, 2.8);
    walk_forward(&mut p, &mut w, 60);
    let hop = MoveInput {
        forward: 1.0,
        ..jump
    };
    assert!(tick(&mut p, &mut w, hop).jumped);
    let mut bumped = false;
    for _ in 0..20 {
        p.step(&mut w, MoveInput { jump: false, ..hop }).unwrap();
        w.step();
        bumped |= p.state().jump.ceiling;
    }
    assert!(bumped, "the lintel no longer counts as a ceiling");
}

/// A pitched roof of 45 degree ramp bricks, one unit square each, rising
/// along -Z in rows from z=-2, each row on a column of bricks.
fn ramp_roof(w: &mut PhysicsWorld) {
    let rise = 1.0;
    for row in 0..8 {
        let (y, z) = (row as f32 * rise, -2.0 - row as f32);
        for col in -6..6 {
            let x = col as f32;
            let p = |a: f32, b: f32, c: f32| Vector::new(a, b, c);
            let points = [
                p(x, y, z),
                p(x + 1.0, y, z),
                p(x, y, z - 1.0),
                p(x + 1.0, y, z - 1.0),
                p(x, y + rise, z - 1.0),
                p(x + 1.0, y + rise, z - 1.0),
            ];
            w.insert_collider(ColliderBuilder::convex_hull(&points).unwrap(), None);
            if y > 0.0 {
                w.insert_collider(
                    ColliderBuilder::cuboid(0.5, y * 0.5, 0.5).translation(Vector::new(
                        x + 0.5,
                        y * 0.5,
                        z - 0.5,
                    )),
                    None,
                );
            }
        }
    }
    w.detect_collisions(&(), &());
}

/// Wandering over a roof of ramp bricks meets their seams at every angle
/// (the joiner who lost the jump was on a roof, and crouching did not bring
/// it back). No seam may leave the ceiling flag set on the roof.
#[test]
fn wandering_a_ramp_brick_roof_never_latches_a_ceiling() {
    use std::f32::consts::{PI, TAU};
    for start in [1_u32, 2, 3, 12345] {
        let mut w = scene();
        ramp_roof(&mut w);
        let mut p = spawn(&mut w);
        let mut seed = start;
        let mut yaw = 0.0;
        for i in 0..6000 {
            if i % 40 == 0 {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                yaw = (seed >> 8) as f32 / 16_777_216.0 * TAU - PI;
            }
            // Steer back onto the roof when near its edges.
            let f = p.state().feet;
            let yaw = if f[0].abs() > 5.0 || f[2] > -1.0 || f[2] < -9.0 {
                (-f[0]).atan2(5.0 + f[2])
            } else {
                yaw
            };
            let input = MoveInput {
                forward: 1.0,
                yaw,
                jump: i % 97 == 0,
                ..Default::default()
            };
            p.step(&mut w, input).unwrap();
            w.step();
            assert!(
                !(p.state().grounded && p.state().jump.ceiling),
                "seed {start} step {i}: {:?}",
                p.state()
            );
        }
    }
}

/// Walking or crouching into a corner cluttered with bricks, then standing:
/// the jump must still lift the player (a host reproduced the lost jump by
/// crouching into a corner). Seeded brick piles against one or two walls.
#[test]
fn crouching_into_a_brick_corner_keeps_the_jump() {
    let cube = |w: &mut PhysicsWorld, min: Vec3, max: Vec3| {
        let half = (max - min) * 0.5;
        let at = min + half;
        w.insert_collider(
            ColliderBuilder::cuboid(half.x, half.y, half.z)
                .translation(Vector::new(at.x, at.y, at.z)),
            None,
        );
    };
    let mut seed = 7_u32;
    let mut next = move || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 8) as f32 / 16_777_216.0
    };
    let mut pick = |k: u32| ((next() * k as f32) as u32).min(k - 1);
    let mut tried = 0;
    for trial in 0..400 {
        let mut w = scene();
        cube(
            &mut w,
            Vec3::new(-6.0, 0.0, -3.0),
            Vec3::new(6.0, 4.8, -2.0),
        );
        if pick(2) == 0 {
            cube(
                &mut w,
                Vec3::new(-3.0, 0.0, -2.0),
                Vec3::new(-2.0, 4.8, 6.0),
            );
        }
        for _ in 0..1 + pick(4) {
            let at = Vec3::new(
                -2.0 + pick(6) as f32 * 0.5,
                0.0,
                -2.0 + pick(6) as f32 * 0.5,
            );
            let size = Vec3::new(0.5 * (1 + pick(4)) as f32, 0.0, 0.5 * (1 + pick(4)) as f32);
            let y = [0.0, 0.0, 0.4, 1.2, 0.8][pick(5) as usize];
            let h = [0.4, 1.2, 1.2, 2.4][pick(4) as usize];
            cube(&mut w, at + Vec3::Y * y, at + size + Vec3::Y * (y + h));
        }
        w.detect_collisions(&(), &());
        let mut p = Player::spawn(
            &mut w,
            1,
            Vec3::new(4.5, 0.05, 4.5),
            PlayerTuning::default(),
        )
        .unwrap();
        step(&mut p, &mut w, MoveInput::default(), 30);
        let yaw = -std::f32::consts::FRAC_PI_4 + (pick(1000) as f32 / 1000.0 - 0.5) * 1.2;
        let crouch = pick(3) != 0;
        let walk = MoveInput {
            forward: 1.0,
            crouch,
            yaw,
            ..Default::default()
        };
        step(&mut p, &mut w, walk, 250);
        step(
            &mut p,
            &mut w,
            MoveInput {
                crouch,
                ..Default::default()
            },
            30,
        );
        step(&mut p, &mut w, MoveInput::default(), 60);
        let state = p.state().clone();
        // Left on top of a pile or still crouched under one: not this case.
        if !state.grounded || state.crouched {
            continue;
        }
        tried += 1;
        let mut peak = state.feet[1];
        for _ in 0..20 {
            p.step(
                &mut w,
                MoveInput {
                    jump: true,
                    ..Default::default()
                },
            )
            .unwrap();
            w.step();
            peak = peak.max(p.state().feet[1]);
        }
        assert!(peak > state.feet[1] + 0.3, "trial {trial}: {:?}", state);
    }
    assert!(tried > 300, "{tried}");
}
