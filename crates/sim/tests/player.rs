use bri_sim::player::{MoveInput, Player, PlayerTuning};
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
    // v20 swim push 0.5 per 32 ms against drag 0.1 * viscosity 40.
    assert!(
        (-3.9..-3.6).contains(&player.state().velocity[2]),
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
    assert!(player.state().feet[1] < 0.01);
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
    assert!((p.state().velocity[2] + 7.0).abs() < 0.01);
    assert!(p.state().feet[2] < -6.0);
    step(&mut p, &mut world, MoveInput::default(), 30);
    assert!(Vec3::from(p.state().velocity).length() < 0.01);
    let jump = MoveInput {
        jump: true,
        ..Default::default()
    };
    assert!(p.step(&mut world, jump).unwrap().jumped);
    world.step();
    step(&mut p, &mut world, jump, 45);
    assert!(p.state().feet[1] > 3.0, "{:?}", p.state());
    step(&mut p, &mut world, MoveInput::default(), 220);
    assert!(p.state().grounded);
    assert!(p.state().feet[1] < 0.02);
    // v20 jumps whenever jump is held and jumpDelay has run out: holding it
    // hops again after the landing tick plus 3 Torque ticks of contact.
    let mut landed_at = None;
    let mut rehop_after = None;
    assert!(p.step(&mut world, jump).unwrap().jumped);
    world.step();
    for tick in 0..400 {
        let events = p.step(&mut world, jump).unwrap();
        world.step();
        if events.landed {
            landed_at = Some(tick);
        }
        if events.jumped {
            rehop_after = landed_at.map(|landed| tick - landed);
            break;
        }
    }
    assert_eq!(rehop_after, Some(13), "{landed_at:?}");
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
    assert!(p.step(&mut world, late).unwrap().jumped, "{:?}", p.state());
    assert!(p.state().velocity[1] > 8.0, "{:?}", p.state());
}
#[test]
fn crouch_clearance_wall_slide_and_camera_occlusion() {
    let mut world = scene();
    let mut p = spawn(&mut world);
    step(
        &mut p,
        &mut world,
        MoveInput {
            crouch: true,
            ..Default::default()
        },
        1,
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
        let ticks = ((1.0 + length + 1.0) / 7.0 * 120.0 * 1.25) as usize;
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
    assert!(p.state().feet[2] > -1.0 && p.state().feet[1] < 0.01);
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
    assert!(p.step(&mut w, jump).unwrap().jumped);
    w.step();
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
        p.state().grounded && p.state().feet[1] < 0.01,
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
        if p.state().feet[1] > 1.0 {
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
    assert!(p.step(&mut w, jump).unwrap().jumped);
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
