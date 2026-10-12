//! The CPU cost of a many-room build's brick emitters, content-free: 488
//! emitters (the Bedroom stack of the v0.2.10 waste audit) in 61 rooms of
//! eight, about 100 live particles each, stepped and snapshotted as the
//! client does every frame. Views: "spawn" looks at one room, "overview"
//! sees every room from above.
//!
//!   cargo test --release -p bri-fx-runtime --test many_rooms_perf -- --ignored --nocapture
use bri_fx_runtime::*;
use glam::Vec3;
use std::time::Instant;

const ROOMS: usize = 61;
const PER_ROOM: usize = 8;
const ROOM_SPACING: f32 = 48.;
const FRAMES: usize = 400;
const DT: f32 = 1. / 165.;

fn room(i: usize) -> Vec3 {
    Vec3::new((i % 8) as f32, 0., (i / 8) as f32) * ROOM_SPACING
}

fn view(eye: Vec3, at: Vec3, fov_degrees: f32) -> Camera {
    let forward = (at - eye).normalize();
    let right = forward.cross(Vec3::Y).normalize();
    Camera {
        view_projection: bri_render::scene::perspective(
            fov_degrees.to_radians(),
            16. / 9.,
            0.1,
            1000.,
        ) * glam::camera::rh::view::look_at_mat4(eye, at, Vec3::Y),
        position: eye,
        right,
        up: right.cross(forward),
    }
}

fn percentile(times: &mut [f64], p: f64) -> f64 {
    times.sort_by(f64::total_cmp);
    times[((times.len() - 1) as f64 * p).round() as usize]
}

#[test]
#[ignore = "timing benchmark; run in release with --nocapture"]
fn many_rooms_of_emitters() {
    let pack = testing::pack(|l| {
        let e = &mut l.emitters[0];
        e.period = 0.03;
        e.period_variance = 0.01;
        e.speed = 1.5;
        e.speed_variance = 0.5;
        e.theta_degrees = [0., 60.];
        let p = &mut l.particles[0];
        p.lifetime = 3.;
        p.lifetime_variance = 0.5;
        p.gravity = -0.1;
        p.drag = 0.5;
    });
    let mut world = EffectsWorld::new(pack, EffectsLimits::default(), 7).unwrap();
    for r in 0..ROOMS {
        for k in 0..PER_ROOM {
            let at = room(r) + Vec3::new((k % 4) as f32 * 4., 0.5, (k / 4) as f32 * 6.);
            world
                .start_emitter(
                    "emitter",
                    SourceTransform {
                        position: at,
                        ..Default::default()
                    },
                    SourceOptions::default(),
                )
                .unwrap();
        }
    }
    let spawn = view(Vec3::new(6., 2., 14.), Vec3::new(6., 1., -20.), 60.);
    let overview = view(
        Vec3::new(4. * ROOM_SPACING, 400., -150.),
        Vec3::new(4. * ROOM_SPACING, 0., 3.5 * ROOM_SPACING),
        70.,
    );
    // Settle to a full cloud.
    for _ in 0..(4. / DT) as usize {
        world.advance(DT, Vec3::ZERO).unwrap();
        world.cull_sources(&[spawn.view_projection]);
    }
    for (name, camera) in [("spawn", spawn), ("overview", overview)] {
        let (mut advance, mut snapshot, mut frame) = (vec![], vec![], vec![]);
        let mut drawn = 0;
        for _ in 0..FRAMES {
            let start = Instant::now();
            world.advance(DT, Vec3::ZERO).unwrap();
            let advanced = Instant::now();
            world.cull_sources(&[camera.view_projection]);
            let f = world.snapshot_in_view(&camera);
            let done = Instant::now();
            drawn = f.particles.len();
            std::hint::black_box(f);
            advance.push((advanced - start).as_secs_f64() * 1e3);
            snapshot.push((done - advanced).as_secs_f64() * 1e3);
            frame.push((done - start).as_secs_f64() * 1e3);
        }
        println!(
            "{name}: simulated {} drawn {drawn} | advance p50 {:.3} ms | cull+snapshot p50 {:.3} ms | effects p50 {:.3} p95 {:.3} ms",
            world.particle_count(),
            percentile(&mut advance, 0.5),
            percentile(&mut snapshot, 0.5),
            percentile(&mut frame, 0.5),
            percentile(&mut frame, 0.95),
        );
    }
}
