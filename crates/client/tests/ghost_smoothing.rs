//! Replicated bodies move smoothly at the render frame rate between the
//! host's 20 Hz updates, on loopback and with latency and jitter.
//!
//! A host simulates a bouncing ball (and a package entity sliding along) at
//! 120 Hz and sends its state every 6 ticks over an ordered stream with a
//! seeded delay. The client renders at 144 Hz through `ghosts::Tracks` and
//! the test samples every frame's pose.
use bri_client::ghosts::{Bounce, Hit, Kind, Mode, Tracks, Update};
use glam::Vec3;
use std::collections::VecDeque;

const TICK: f32 = 1.0 / 120.0;
const INTERVAL: u64 = 6;
const FRAME: f32 = 1.0 / 144.0;
const GRAVITY: Vec3 = Vec3::new(0.0, -9.81, 0.0);

/// The host's ball: v20-style tick integration and Torque's projectile
/// bounce (elasticity 0.5, friction 0.2) off the floor.
#[derive(Clone, Copy)]
struct Ball {
    position: Vec3,
    velocity: Vec3,
    bounces: u32,
}
impl Ball {
    fn resting(&self) -> bool {
        self.position.y <= 0.0 && self.velocity.y == 0.0
    }
    fn step(&mut self) {
        if !self.resting() {
            self.velocity += GRAVITY * TICK;
        }
        self.position += self.velocity * TICK;
        if self.position.y < 0.0 {
            self.position.y = 0.0;
            self.velocity.y = -self.velocity.y * 0.5;
            self.velocity.x *= 0.8 * 0.5;
            self.bounces += 1;
            if self.velocity.y < 1.0 {
                self.velocity.y = 0.0;
            }
        }
    }
    fn update(&self) -> Update {
        Update {
            position: self.position,
            velocity: self.velocity,
            acceleration: if self.resting() { Vec3::ZERO } else { GRAVITY },
            rotation: glam::Quat::IDENTITY,
            bounce: Some(Bounce {
                elasticity: 0.5,
                friction: 0.2,
                rest_speed: 1.0,
            }),
            horizon: 480.0,
        }
    }
}

struct Rng(u64);
impl Rng {
    fn unit(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
}

fn floor(from: Vec3, to: Vec3) -> Option<Hit> {
    (to.y < 0.0 && from.y >= 0.0).then(|| {
        let fraction = from.y / (from.y - to.y).max(1e-6);
        Hit {
            position: from.lerp(to, fraction),
            normal: Vec3::Y,
            fraction,
            carry: None,
        }
    })
}

struct Run {
    /// Largest frame-to-frame move over the fastest the body really moved
    /// in that frame's time.
    worst_step: f32,
    /// Frames where the body stood still although it was moving.
    stalls: usize,
    frames: usize,
    /// Largest distance from where the host had the body at the tick the
    /// client presents.
    worst_error: f32,
}

/// Run the host and client together for `seconds`. `latency` and `jitter`
/// are one-way stream delays in seconds.
fn run(mode: Mode, latency: f32, jitter: f32, seed: u64) -> Run {
    run_with(mode, latency, jitter, seed, Send::Every)
}
#[derive(Clone, Copy, PartialEq)]
enum Send {
    /// The body's state in every update.
    Every,
    /// Its state once at spawn and again after each impact; later updates
    /// only advance the clock (Torque's ghosted projectiles).
    Impacts,
    /// Every update, drawn as it arrives (no smoothing).
    Raw,
}
fn run_with(mode: Mode, latency: f32, jitter: f32, seed: u64, send: Send) -> Run {
    let raw = send == Send::Raw;
    let mut host = Ball {
        position: Vec3::new(0.0, 1.0, 0.0),
        velocity: match mode {
            Mode::Simulated => Vec3::new(12.0, 11.0, 0.0),
            Mode::Interpolated => Vec3::new(5.0, 0.0, 0.0),
        },
        bounces: 0,
    };
    let mut sent_bounces = None;
    let slide = mode == Mode::Interpolated;
    // Host positions by tick, to measure against.
    let mut truth = vec![host.position];
    let mut rng = Rng(seed);
    let mut in_flight: VecDeque<(f32, u64, Option<Update>)> = VecDeque::new();
    let mut last_arrival = 0.0f32;
    let mut tracks = Tracks::default();
    let key = (Kind::Body, 1);
    let mut sweep = floor;
    let (mut time, mut host_tick) = (0.0f32, 0u64);
    let mut previous: Option<Vec3> = None;
    let mut latest = Vec3::ZERO;
    let mut out = Run {
        worst_step: 0.0,
        stalls: 0,
        frames: 0,
        worst_error: 0.0,
    };
    while time < 4.0 {
        time += FRAME;
        while (host_tick + 1) as f32 * TICK <= time {
            host_tick += 1;
            if slide {
                host.position += host.velocity * TICK;
            } else {
                host.step();
            }
            truth.push(host.position);
            if host_tick.is_multiple_of(INTERVAL) {
                // An ordered stream: a late datagram holds back later ones.
                let arrival =
                    (host_tick as f32 * TICK + latency + jitter * rng.unit()).max(last_arrival);
                last_arrival = arrival;
                let describe = send != Send::Impacts || sent_bounces != Some(host.bounces);
                sent_bounces = Some(host.bounces);
                in_flight.push_back((arrival, host_tick, describe.then(|| host.update())));
            }
        }
        tracks.advance(FRAME);
        while in_flight.front().is_some_and(|(at, _, _)| *at <= time) {
            let (_, tick, update) = in_flight.pop_front().unwrap();
            tracks.arrived(tick);
            if let Some(update) = update {
                tracks.observe(key, tick, mode, update, &mut sweep);
                latest = update.position;
            }
        }
        let Some(mut pose) = tracks.pose(key, &mut sweep) else {
            continue;
        };
        if raw {
            pose.position = latest;
        }
        // Skip the first update's settling.
        if time < 0.5 {
            previous = Some(pose.position);
            continue;
        }
        out.frames += 1;
        let now = tracks.now().unwrap();
        let shown = now;
        let at = |tick: f64| truth[(tick.max(0.0) as usize).min(truth.len() - 1)];
        // The host has not simulated past `shown` on loopback; measure speed
        // over the tick before it.
        let a = at(shown.floor());
        let b = at(shown.floor() + 1.0);
        let fastest = (a - at(shown.floor() - 1.0)).length().max((b - a).length()) / TICK;
        if let Some(previous) = previous {
            let moved = (pose.position - previous).length();
            let allowed = fastest.max(1.0) * FRAME;
            out.worst_step = out.worst_step.max(moved / allowed);
            if fastest > 1.0 && moved < 0.1 * fastest * FRAME {
                out.stalls += 1;
            }
        }
        if mode == Mode::Simulated {
            let expected = a.lerp(b, shown.fract() as f32);
            out.worst_error = out.worst_error.max((pose.position - expected).length());
        }
        previous = Some(pose.position);
    }
    out
}

fn check(name: &str, run: &Run, max_error: f32) {
    eprintln!(
        "{name}: worst step {:.2}x, stalls {}/{}, worst error {:.3}",
        run.worst_step, run.stalls, run.frames, run.worst_error
    );
    // Snapshot rendering moves six ticks' worth in one frame and then
    // nothing for several: an 8x step and most frames stalled.
    assert!(run.worst_step < 1.6, "{name}: stepped");
    assert!(
        run.stalls * 20 < run.frames,
        "{name}: stood still in {} of {} frames",
        run.stalls,
        run.frames
    );
    assert!(run.worst_error < max_error, "{name}: strayed from the host");
}

#[test]
fn simulated_bodies_move_smoothly_on_loopback() {
    check("ball loopback", &run(Mode::Simulated, 0.0, 0.0, 1), 0.3);
}

#[test]
fn simulated_bodies_move_smoothly_with_latency_and_jitter() {
    for seed in 1..=8 {
        check(
            &format!("ball 80 ms + 60 ms jitter #{seed}"),
            &run(Mode::Simulated, 0.08, 0.06, seed),
            0.6,
        );
    }
}

#[test]
fn simulated_bodies_fly_from_spawn_and_impacts_alone() {
    check(
        "ball impacts only, loopback",
        &run_with(Mode::Simulated, 0.0, 0.0, 1, Send::Impacts),
        0.3,
    );
    for seed in 1..=8 {
        check(
            &format!("ball impacts only, 80 ms + 60 ms jitter #{seed}"),
            &run_with(Mode::Simulated, 0.08, 0.06, seed, Send::Impacts),
            0.6,
        );
    }
}

#[test]
fn interpolated_bodies_move_smoothly_on_loopback_and_with_jitter() {
    check(
        "entity loopback",
        &run(Mode::Interpolated, 0.0, 0.0, 1),
        f32::MAX,
    );
    for seed in 1..=8 {
        check(
            &format!("entity 80 ms + 60 ms jitter #{seed}"),
            &run(Mode::Interpolated, 0.08, 0.06, seed),
            f32::MAX,
        );
    }
}

#[test]
fn the_measure_catches_snapshot_stepping() {
    let raw = run_with(Mode::Simulated, 0.0, 0.0, 1, Send::Raw);
    eprintln!(
        "raw: worst step {:.2}x, stalls {}/{}",
        raw.worst_step, raw.stalls, raw.frames
    );
    assert!(raw.worst_step > 3.0 && raw.stalls * 2 > raw.frames);
}
