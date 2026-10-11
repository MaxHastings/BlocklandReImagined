//! A vehicle nobody drives settles and sleeps: its wheel rays, suspension
//! and drag stop until a driver, a push or a hit wakes it. Before this a
//! parked vehicle re-woke itself every tick with its own spring forces.
#[macro_use]
mod common;
use bri_vehicles::*;
use common::Fixture;
use rapier3d::prelude::*;

fn spawn(v: &mut VehiclesWorld, w: &mut PhysicsWorld, name: &str, id: u64, x: f32) {
    v.spawn(
        w,
        Spawn {
            scale: 1.,
            id: VehicleId(id),
            owner: OwnerId(10),
            definition: name.into(),
            transform: Transform {
                position: [x, 1., 0.],
                ..Default::default()
            },
            spawn_id: None,
            respawn_ticks: None,
        },
    )
    .unwrap();
    w.detect_collisions(&(), &());
}
fn step(v: &mut VehiclesWorld, w: &mut PhysicsWorld, n: usize) {
    for _ in 0..n {
        v.pre_step(w, &[]).unwrap();
        w.step();
        v.post_step(w).unwrap();
        v.drain_intents();
    }
}
fn position(v: &VehiclesWorld, w: &PhysicsWorld) -> [f32; 3] {
    v.snapshot(w).vehicles[0].transform.position
}

on_both! {
fn a_parked_vehicle_sleeps_and_a_driver_wakes_it(f: &Fixture) {
    for name in [f.car, f.tank, f.flying_car, f.carpet] {
        let mut v = f.vehicles();
        let mut w = common::floor(500.);
        spawn(&mut v, &mut w, name, 1, 0.);
        // Settling: lands, bounces on its springs, comes to rest.
        step(&mut v, &mut w, 600);
        let rays = v.wheel_rays();
        let skips = v.resting_skips();
        let rest = position(&v, &w);
        step(&mut v, &mut w, 240);
        assert!(
            v.resting_skips() >= skips + 200,
            "{name}: parked for 2 s but stepped every tick ({} of 240 skipped)",
            v.resting_skips() - skips
        );
        let wheels = v.snapshot(&w).vehicles[0].wheel_contact.len() as u64;
        assert!(
            v.wheel_rays() - rays <= 40 * wheels,
            "{name}: a sleeping vehicle kept casting wheel rays ({} in 240 ticks)",
            v.wheel_rays() - rays
        );
        let still = position(&v, &w);
        for axis in 0..3 {
            assert!((still[axis] - rest[axis]).abs() < 1e-3, "{name} drifted while asleep");
        }
        // A driver mounting and pressing the throttle wakes it: it moves.
        let seat = v.snapshot(&w).vehicles[0].seats[0].transform.position;
        v.mount(
            &w,
            VehicleId(1),
            0,
            Occupant {
                id: OccupantId(20),
                owner: OwnerId(10),
                body: [1.25, 2.65],
            },
            seat,
        )
        .unwrap();
        v.set_controls(
            OwnerId(10),
            OccupantId(20),
            Controls {
                throttle: 1.,
                ..Default::default()
            },
        )
        .unwrap();
        let rays = v.wheel_rays();
        step(&mut v, &mut w, 240);
        let moved = position(&v, &w);
        let distance = ((moved[0] - still[0]).powi(2) + (moved[2] - still[2]).powi(2)).sqrt();
        assert!(distance > 1., "{name}: a driven vehicle stayed asleep ({distance} units)");
        if !v.snapshot(&w).vehicles[0].wheel_contact.is_empty() {
            assert!(v.wheel_rays() > rays, "{name}: no wheel rays while driven");
        }
    }
}
}

/// `cargo test --release -p bri-vehicles --test parked parked_timing -- --ignored --nocapture`
#[test]
#[ignore = "timing probe"]
fn parked_timing() {
    let f = Fixture::synthetic();
    let mut v = f.vehicles();
    let mut w = common::floor(500.);
    for i in 0..10u64 {
        spawn(&mut v, &mut w, f.car, i + 1, i as f32 * 6. - 30.);
    }
    step(&mut v, &mut w, 600);
    let (mut vehicles, mut physics) = (Vec::new(), Vec::new());
    for _ in 0..600 {
        let start = std::time::Instant::now();
        v.pre_step(&mut w, &[]).unwrap();
        let mid = std::time::Instant::now();
        w.step();
        v.post_step(&mut w).unwrap();
        v.drain_intents();
        vehicles.push((mid - start).as_secs_f64() * 1e6);
        physics.push(mid.elapsed().as_secs_f64() * 1e6);
    }
    let median = |s: &mut Vec<f64>| {
        s.sort_by(f64::total_cmp);
        s[s.len() / 2]
    };
    println!(
        "10 parked cars, per tick: vehicle pre_step {:.1} us, physics step + post {:.1} us (median of 600); wheel rays {}, resting skips {}",
        median(&mut vehicles),
        median(&mut physics),
        v.wheel_rays(),
        v.resting_skips()
    );
}
