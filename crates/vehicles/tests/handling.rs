//! How an Add-On vehicle drives, measured headlessly: the Playground's
//! Dune Buggy as it ships, and the same buggy retuned with Warthog-like
//! numbers through `vehicles.json` alone. Each number is evidence for the
//! handling audit (`docs/audits/vehicle-handling.md`), not a feel verdict;
//! Max's playtest judges the feel.
#[allow(unused_macros)]
mod common;
use bri_vehicles::*;
use glam::{Quat, Vec3};
use rapier3d::prelude::PhysicsWorld;

const BUGGY: &str = "playground-buggy:vehicle/buggy";

fn buggy_pack() -> Pack {
    Pack::load(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packages/playground/playground-buggy/assets/vehicles.json"),
    )
    .unwrap()
}

fn spawn(pack: Pack, at: [f32; 3]) -> (VehiclesWorld, PhysicsWorld) {
    let mut v = VehiclesWorld::new(pack).unwrap();
    let mut w = common::floor(4000.);
    v.spawn(
        &mut w,
        Spawn {
            scale: 1.,
            id: VehicleId(1),
            owner: OwnerId(10),
            definition: BUGGY.into(),
            transform: Transform {
                position: at,
                ..Default::default()
            },
            spawn_id: None,
            respawn_ticks: None,
        },
    )
    .unwrap();
    w.detect_collisions(&(), &());
    (v, w)
}

fn board(v: &mut VehiclesWorld, w: &PhysicsWorld) {
    let seat = v.snapshot(w).vehicles[0].seats[0].transform.position;
    v.mount(
        w,
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
}

fn step(v: &mut VehiclesWorld, w: &mut PhysicsWorld, c: Controls) -> VehicleSnapshot {
    if v.occupant(OccupantId(20)).is_some() {
        v.set_controls(OwnerId(10), OccupantId(20), c).unwrap();
    }
    v.pre_step(w, &[]).unwrap();
    w.step();
    v.post_step(w).unwrap();
    v.snapshot(w).vehicles.remove(0)
}

fn seconds(s: f32) -> usize {
    (s * 120.).round() as usize
}

fn settle(v: &mut VehiclesWorld, w: &mut PhysicsWorld) {
    for _ in 0..seconds(2.) {
        step(v, w, Controls::default());
    }
}

fn throttle() -> Controls {
    Controls {
        throttle: 1.,
        strafe_steering_off: true,
        auto_return_off: true,
        ..Default::default()
    }
}

#[derive(Debug)]
struct Run {
    /// Speed after each whole second at full throttle.
    speeds: Vec<f32>,
    /// Sideways distance from the starting line after the run.
    drift: f32,
    /// Heading change over the run, radians.
    heading: f32,
}

fn heading(t: &Transform) -> f32 {
    let f = Quat::from_array(t.rotation) * Vec3::NEG_Z;
    f.x.atan2(-f.z)
}

/// Full throttle, no steering, for `secs`.
fn straight(pack: Pack, secs: usize) -> Run {
    let (mut v, mut w) = spawn(pack, [0., 1.5, 0.]);
    settle(&mut v, &mut w);
    board(&mut v, &w);
    let start = v.snapshot(&w).vehicles[0].transform.clone();
    let mut speeds = vec![];
    let mut s = None;
    for i in 1..=seconds(secs as f32) {
        let snap = step(&mut v, &mut w, throttle());
        if i % 120 == 0 {
            speeds.push(Vec3::from(snap.velocity).length());
        }
        s = Some(snap);
    }
    let s = s.unwrap();
    let travelled = Vec3::from(s.transform.position) - Vec3::from(start.transform_position());
    let forward = Quat::from_array(start.rotation) * Vec3::NEG_Z;
    let side = forward.cross(Vec3::Y);
    Run {
        speeds,
        drift: travelled.dot(side),
        heading: heading(&s.transform) - heading(&start),
    }
}

trait Pos {
    fn transform_position(&self) -> [f32; 3];
}
impl Pos for Transform {
    fn transform_position(&self) -> [f32; 3] {
        self.position
    }
}

/// Speed over yaw rate after `secs` at full lock and `throttle_share`.
fn circle_at(pack: Pack, secs: f32, throttle_share: f32) -> f32 {
    let (mut v, mut w) = spawn(pack, [0., 1.5, 0.]);
    settle(&mut v, &mut w);
    board(&mut v, &w);
    let lock = v.definition(BUGGY).unwrap().max_steering;
    let mut s = None;
    for i in 0..seconds(secs) {
        let mut c = throttle();
        c.throttle = throttle_share;
        c.look_delta = [if i == 0 { lock } else { 0. }, 0.];
        s = Some(step(&mut v, &mut w, c));
    }
    let s = s.unwrap();
    Vec3::from(s.velocity).length() / s.angular_velocity[1].abs()
}

/// Dropped from `height` above its ride height: the deepest the suspension
/// went (lowest wheel extension) and how many bounces (the chassis turning
/// from rising to falling) before it settled.
fn drop_test(pack: Pack, height: f32) -> (f32, usize) {
    let (mut v, mut w) = spawn(pack.clone(), [0., 1.5, 0.]);
    settle(&mut v, &mut w);
    let rest = v.snapshot(&w).vehicles[0].transform.position[1];
    let (mut v, mut w) = spawn(pack, [0., rest + height, 0.]);
    let mut deepest = 1f32;
    let mut bounces = 0;
    let mut rising = false;
    for _ in 0..seconds(4.) {
        let s = step(&mut v, &mut w, Controls::default());
        if s.wheel_contact.iter().any(|c| *c) {
            deepest = s.wheel_suspension.iter().copied().fold(deepest, f32::min);
        }
        let up = s.velocity[1] > 0.05;
        if rising && s.velocity[1] < -0.05 {
            bounces += 1;
        }
        if up {
            rising = true;
        } else if s.velocity[1] < -0.05 {
            rising = false;
        }
    }
    (deepest, bounces)
}

/// The buggy as it ships (`tools/make_playground_assets.py`), the
/// Playground's stand-in for a Warthog-style off-roader, with `edit` made
/// to it: every handling trait below comes from `vehicles.json` fields.
fn warthog(edit: impl Fn(&mut Definition)) -> Pack {
    let mut pack = buggy_pack();
    let d = pack.definitions.iter_mut().find(|d| d.id == BUGGY).unwrap();
    edit(d);
    pack.validate().unwrap();
    pack
}

#[test]
fn authored_wheel_angles_do_not_lose_their_lock_to_the_v20_curve() {
    let make = |linear| {
        warthog(|d| {
            d.max_steering = 30.0_f32.to_radians();
            d.steering.toward_look = true;
            for wheel in &mut d.wheels {
                wheel.linear_steering = linear;
            }
        })
    };
    let (old, _) = drive_toward_look(make(false), std::f32::consts::FRAC_PI_2, 1.0);
    let (corrected, _) = drive_toward_look(make(true), std::f32::consts::FRAC_PI_2, 1.0);
    eprintln!("squared angle heading {old}, actual wheel angle heading {corrected}");
    assert!(corrected > old + 0.15, "old {old}, corrected {corrected}");
}

#[test]
fn look_steering_keys_can_override_an_opposing_camera() {
    let pack = warthog(|d| {
        d.steering.toward_look = true;
        d.steering.keyboard_override = true;
    });
    let (mut vehicles, mut physics) = spawn(pack, [0., 1.5, 0.]);
    settle(&mut vehicles, &mut physics);
    board(&mut vehicles, &physics);
    let mut controls = throttle();
    controls.strafe = 1.0;
    controls.look_yaw = -1.5;
    let mut snapshot = step(&mut vehicles, &mut physics, controls);
    for _ in 0..30 {
        snapshot = step(&mut vehicles, &mut physics, controls);
    }
    assert!(
        snapshot.steering > 0.1,
        "right key owns the turn: {}",
        snapshot.steering
    );
    controls.strafe = 0.0;
    for _ in 0..60 {
        snapshot = step(&mut vehicles, &mut physics, controls);
    }
    assert!(
        snapshot.steering < -0.1,
        "release resumes the camera: {}",
        snapshot.steering
    );
}

/// Full throttle at full lock for `secs`: the most it rolled over, radians.
fn worst_roll(pack: Pack, secs: f32) -> f32 {
    let (mut v, mut w) = spawn(pack, [0., 1.5, 0.]);
    settle(&mut v, &mut w);
    board(&mut v, &w);
    let mut worst = 0f32;
    for _ in 0..seconds(secs) {
        let mut c = throttle();
        c.look_yaw = 1.5;
        c.look_delta = [0.55, 0.];
        let s = step(&mut v, &mut w, c);
        let up = Quat::from_array(s.transform.rotation) * Vec3::Y;
        worst = worst.max(up.angle_between(Vec3::Y));
    }
    worst
}

/// Up to speed, then the driver looks hard right for `secs` (full lock)
/// and back along a new heading 90 degrees right of the start, which they
/// hold in the world: whether a rear tyre slid in the turn, and the yaw
/// rate and heading error two seconds later.
fn drift_and_recover(pack: Pack, secs: f32) -> (bool, f32, f32) {
    let (mut v, mut w) = spawn(pack, [0., 1.5, 0.]);
    settle(&mut v, &mut w);
    board(&mut v, &w);
    let d = v.definition(BUGGY).unwrap().clone();
    let start = heading(&v.snapshot(&w).vehicles[0].transform);
    let target = start + std::f32::consts::FRAC_PI_2;
    let mut rear_slid = false;
    let run_up = seconds(3.);
    let turn = run_up + seconds(secs);
    let mut s = v.snapshot(&w).vehicles.remove(0);
    for i in 0..turn + seconds(3.) {
        let mut c = throttle();
        let now = heading(&s.transform);
        c.look_yaw = if i < run_up {
            0.
        } else if i < turn {
            1.5
        } else {
            (target - now + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI
        };
        s = step(&mut v, &mut w, c);
        if (run_up..turn).contains(&i) {
            rear_slid |= d
                .wheels
                .iter()
                .zip(&s.wheel_tire)
                .any(|(w, t)| w.position[2] > 0. && t.slipping);
        }
    }
    let error = (target - heading(&s.transform) + std::f32::consts::PI)
        .rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    (rear_slid, s.angular_velocity[1], error)
}

/// The driver looks `turn` radians right of the way the vehicle faced and
/// holds that direction in the world while driving at full throttle, as
/// the client holds a `toward_look` driver's view: the heading after
/// `secs`, and the yaw rate then.
fn drive_toward_look(pack: Pack, turn: f32, secs: f32) -> (f32, f32) {
    let (mut v, mut w) = spawn(pack, [0., 1.5, 0.]);
    settle(&mut v, &mut w);
    board(&mut v, &w);
    let start = heading(&v.snapshot(&w).vehicles[0].transform);
    let target = start + turn;
    let mut s = v.snapshot(&w).vehicles.remove(0);
    for _ in 0..seconds(secs) {
        let mut c = throttle();
        let now = heading(&s.transform);
        c.look_yaw = (target - now + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        s = step(&mut v, &mut w, c);
    }
    (heading(&s.transform) - start, s.angular_velocity[1])
}

/// Seconds from a drop of `height` above ride height until a wheel touches.
fn hang_time(pack: Pack, height: f32) -> f32 {
    let (mut v, mut w) = spawn(pack.clone(), [0., 1.5, 0.]);
    settle(&mut v, &mut w);
    let rest = v.snapshot(&w).vehicles[0].transform.position[1];
    let (mut v, mut w) = spawn(pack, [0., rest + height, 0.]);
    for i in 0..seconds(5.) {
        if step(&mut v, &mut w, Controls::default())
            .wheel_contact
            .iter()
            .any(|c| *c)
        {
            return i as f32 / 120.;
        }
    }
    f32::INFINITY
}

#[test]
fn an_empty_engine_curve_is_torques_straight_fall_exactly() {
    let mut d = buggy_pack().definitions.remove(0);
    d.engine_curve.clear();
    for i in 0..=40 {
        let share = i as f32 / 32.;
        let torque = if share > 1. { 0. } else { 1. - share };
        assert_eq!(
            d.engine_torque(share).to_bits(),
            torque.to_bits(),
            "{share}"
        );
    }
    let mut curved = d.clone();
    curved.engine_curve = vec![[0., 1.], [0.7, 0.95], [1., 0.]];
    assert_eq!(curved.engine_torque(0.35), 0.975);
    assert!((curved.engine_torque(0.85) - 0.475).abs() < 1e-6);
    assert_eq!(curved.engine_torque(1.01), 0.);
}

/// Halo's Warthog tags (`docs/audits/vehicle-handling.md`): 0.275 Halo
/// units a tick (34 here) top speed, gained at a steady 0.0028 a tick per
/// tick (10.4 a second per second here, about 3.3 s to the top).
#[test]
fn the_warthog_pulls_steadily_to_its_tag_top_speed_and_tracks_straight() {
    let run = straight(warthog(|_| {}), 6);
    let linear = straight(warthog(|d| d.engine_curve.clear()), 6);
    eprintln!("warthog {run:?}\nlinear torque {linear:?}");
    let s = &run.speeds;
    assert!((9.0..12.0).contains(&s[0]), "about 10.4 a second: {s:?}");
    assert!(
        ((s[1] - s[0]) - s[0]).abs() < s[0] * 0.15,
        "a steady pull, as Halo's constant acceleration: {s:?}"
    );
    assert!(s[2] > 28. && (33.0..34.5).contains(&s[5]), "{s:?}");
    assert!(
        linear.speeds[2] < s[2] - 5.,
        "the flat curve keeps pulling where v20's straight fall fades: {:?}",
        linear.speeds
    );
    assert!(
        run.drift.abs() < 0.05 && run.heading.abs() < 0.005,
        "{run:?}"
    );
}

/// Halo's Warthog sheds its top speed at 0.011 a tick per tick when the
/// driver pulls back: under a second. Here pulling back brakes it as hard
/// as the tyres hold, where v20's reversed engine all but does nothing at
/// top speed.
#[test]
fn pulling_back_stops_it_hard() {
    let back = Controls {
        throttle: -1.,
        ..throttle()
    };
    let (_, top, stop) = launch_and_stop(warthog(|_| {}), 5., back, 6.);
    let (_, _, v20) = launch_and_stop(warthog(|d| d.throttle_brakes = false), 5., back, 6.);
    let coast = Controls {
        throttle: 0.,
        ..throttle()
    };
    let (_, _, coasting) = launch_and_stop(warthog(|_| {}), 5., coast, 12.);
    eprintln!(
        "from {top}: pulled back stops in {stop} s, v20's reverse {v20} s, coasting {coasting} s"
    );
    assert!(stop < 1.3, "{stop}");
    assert!(v20 > stop * 2., "{v20}");
    assert!(coasting > stop * 2., "{coasting}");
}

#[test]
fn rebound_damping_soaks_a_landing_on_long_springs() {
    let (deep, bounces) = drop_test(warthog(|_| {}), 2.);
    let (_, pogo) = drop_test(
        warthog(|d| d.wheels.iter_mut().for_each(|w| w.rebound_damping = 0.)),
        2.,
    );
    eprintln!("warthog drop: deepest {deep}, {bounces} bounces; no rebound damping {pogo}");
    assert!(deep < 0.35, "the landing uses the long travel: {deep}");
    assert!(bounces <= 1, "it settles: {bounces} bounces");
    assert!(
        pogo > bounces,
        "Torque's compression-only damper pogos: {pogo}"
    );
}

#[test]
fn four_wheel_steering_turns_tighter() {
    // At a walking pace, where the rear's steering shows, not the tyres'
    // slide.
    let four = circle_at(
        warthog(|d| {
            d.steering.toward_look = false;
            d.max_speed = 8.;
            d.steering.speed_lock = 1.;
        }),
        6.,
        1.,
    );
    let front = circle_at(
        warthog(|d| {
            d.steering.toward_look = false;
            d.max_speed = 8.;
            d.steering.speed_lock = 1.;
            d.wheels
                .iter_mut()
                .filter(|w| w.position[2] > 0.)
                .for_each(|w| w.steering = 0.)
        }),
        6.,
        1.,
    );
    eprintln!("circle: four-wheel {four}, front only {front}");
    assert!(four < front * 0.8, "{four} vs {front}");
}

#[test]
fn it_drifts_through_a_hard_turn_and_comes_out_straight() {
    let (slid, yaw_rate, error) = drift_and_recover(warthog(|_| {}), 0.6);
    eprintln!("drift: rear slid {slid}, then yaw rate {yaw_rate}, heading off by {error}");
    assert!(slid, "the rear steps out");
    assert!(
        yaw_rate.abs() < 0.2 && error.abs() < 0.15,
        "{yaw_rate} {error}"
    );
}

#[test]
fn it_steers_toward_where_the_driver_looks_and_straightens_on_arrival() {
    let (turned, yaw_rate) = drive_toward_look(warthog(|_| {}), 1.2, 5.);
    eprintln!("toward look: turned {turned}, yaw rate {yaw_rate}");
    assert!((turned - 1.2).abs() < 0.12, "{turned}");
    assert!(yaw_rate.abs() < 0.15, "{yaw_rate}");
    // Looking the other way turns the other way.
    let (turned, _) = drive_toward_look(warthog(|_| {}), -1.2, 5.);
    assert!((turned + 1.2).abs() < 0.12, "{turned}");
}

#[test]
fn a_lighter_gravity_hangs_longer_off_a_jump() {
    let floaty = hang_time(warthog(|_| {}), 6.);
    let full = hang_time(warthog(|d| d.gravity_scale = None), 6.);
    eprintln!("hang time: {floaty} at 0.85 g, {full} at 1 g");
    let ratio = floaty / full;
    assert!((ratio - (1. / 0.85f32).sqrt()).abs() < 0.05, "{ratio}");
}

#[test]
fn it_stays_on_its_wheels_at_full_lock_and_full_speed() {
    let roll = worst_roll(warthog(|_| {}), 6.);
    eprintln!("worst roll at full lock: {roll}");
    assert!(roll < 0.5, "{roll}");
}

/// Full throttle `secs`, then `then` until stopped (or `limit` seconds):
/// seconds to 90% of the top speed, the top speed, and seconds to stop.
fn launch_and_stop(pack: Pack, secs: f32, then: Controls, limit: f32) -> (f32, f32, f32) {
    let (mut v, mut w) = spawn(pack, [0., 1.5, 0.]);
    settle(&mut v, &mut w);
    board(&mut v, &w);
    let top = v.definition(BUGGY).unwrap().max_speed;
    let mut ninety = f32::INFINITY;
    let mut fastest = 0f32;
    for i in 0..seconds(secs) {
        let s = step(&mut v, &mut w, throttle());
        let speed = Vec3::from(s.velocity).length();
        fastest = fastest.max(speed);
        if speed >= top * 0.9 && ninety.is_infinite() {
            ninety = i as f32 / 120.;
        }
    }
    let mut stopped = f32::INFINITY;
    for i in 0..seconds(limit) {
        let s = step(&mut v, &mut w, then);
        if Vec3::from(s.velocity).length() < 0.5 {
            stopped = i as f32 / 120.;
            break;
        }
    }
    (ninety, fastest, stopped)
}

/// The numbers the buggy takes straight from Halo's Warthog tags, at 4.12
/// units to a Halo world unit, and its reverse top speed as driven.
#[test]
fn the_buggy_carries_the_warthogs_tag_numbers() {
    let pack = buggy_pack();
    let d = pack.definitions.iter().find(|d| d.id == BUGGY).unwrap();
    assert_eq!((d.max_speed, d.reverse_speed), (34., 14.8));
    assert!((d.max_steering - 30f32.to_radians()).abs() < 1e-4);
    assert!(d.wheels.iter().all(|w| w.powered && w.rest_length == 0.62));
    for w in &d.wheels {
        let (along, across) = if w.position[2] > 0. {
            (0.45, 0.65)
        } else {
            (0.75, 0.45)
        };
        assert_eq!(w.tire.static_friction, along * 4.);
        assert!((w.tire.lateral_grip - across / along).abs() < 1e-3);
    }
    let (mut v, mut w) = spawn(pack, [0., 1.5, 0.]);
    settle(&mut v, &mut w);
    board(&mut v, &w);
    let mut s = None;
    for _ in 0..seconds(5.) {
        s = Some(step(
            &mut v,
            &mut w,
            Controls {
                throttle: -1.,
                ..throttle()
            },
        ));
    }
    let speed = Vec3::from(s.unwrap().velocity).length();
    eprintln!("reverse top speed {speed}");
    assert!((13.5..15.5).contains(&speed), "{speed}");
}

/// A buggy shoved into someone who holds their ground (here a fixed
/// body, as the solver sees a walking player) stops against them. Sharing
/// the hit once handed it back into them faster each step, until it flew
/// off at hundreds of units a second (390 and 470 for these two).
#[test]
fn a_buggy_pinned_against_someone_who_holds_their_ground_never_speeds_up() {
    use rapier3d::prelude::{ColliderBuilder, RigidBodyBuilder};
    for (half, gap) in [
        (Vec3::new(0.6, 1.3, 0.6), 4.3),
        (Vec3::new(1.0, 1.3, 0.5), 3.0),
    ] {
        let (mut v, mut w) = spawn(buggy_pack(), [0., 1.0, 0.]);
        let (_, someone) = w.insert(
            RigidBodyBuilder::fixed().translation(Vec3::new(0., half.y + 0.01, -gap)),
            ColliderBuilder::cuboid(half.x, half.y, half.z),
        );
        w.detect_collisions(&(), &());
        v.set_velocity(&mut w, VehicleId(1), [0., 0., -16.])
            .unwrap();
        let mut fastest = 0f32;
        for _ in 0..seconds(2.) {
            v.pre_step(&mut w, &[]).unwrap();
            w.step();
            v.share_contacts(&mut w, someone, 90.);
            v.post_step(&mut w).unwrap();
            let s = v.snapshot(&w).vehicles.remove(0);
            fastest = fastest.max(Vec3::from(s.velocity).length());
        }
        assert!(fastest <= 16.5, "it never sped up ({half}): {fastest}");
    }
}

/// The gunner's post fires as the Warthog's chaingun (`warthog gun` weap):
/// for as long as the trigger is held, 8 rounds a second rising to 15 over
/// the first second, straying 1 rising to 2 degrees over 3 seconds, and
/// without kicking the buggy. Letting go and pulling again starts slow.
#[test]
fn the_gunners_chaingun_spins_up_spreads_and_does_not_kick() {
    let pack = buggy_pack();
    let gun = pack
        .definitions
        .iter()
        .find(|d| d.id == BUGGY)
        .and_then(|d| d.weapon.clone())
        .unwrap();
    let auto = gun
        .automatic
        .clone()
        .expect("the chaingun fires while held");
    assert_eq!(auto.rate, [8.0, 15.0]);
    assert_eq!(gun.recoil, Some(0.0));
    let (mut v, mut w) = spawn(pack, [0.0, 1.0, 0.0]);
    settle(&mut v, &mut w);
    let seat = v.snapshot(&w).vehicles[0].seats[2].transform.position;
    let gunner = OccupantId(22);
    v.mount(
        &w,
        VehicleId(1),
        2,
        Occupant {
            id: gunner,
            owner: OwnerId(10),
            body: [1.25, 2.65],
        },
        seat,
    )
    .unwrap();
    let rest = v.snapshot(&w).vehicles[0].transform.position;
    v.drain_intents();
    let mut shots = Vec::new();
    let fire = |v: &mut VehiclesWorld,
                w: &mut PhysicsWorld,
                on: bool,
                ticks: usize,
                shots: &mut Vec<(usize, Vec3)>,
                from: usize| {
        v.set_controls(
            OwnerId(10),
            gunner,
            Controls {
                fire: on,
                ..Default::default()
            },
        )
        .unwrap();
        for t in 0..ticks {
            v.pre_step(w, &[]).unwrap();
            w.step();
            v.post_step(w).unwrap();
            for i in v.drain_intents() {
                if let Intent::Fire(f) = i {
                    shots.push((from + t, Vec3::from(f.direction)));
                }
            }
        }
    };
    fire(&mut v, &mut w, true, seconds(4.0), &mut shots, 0);
    let in_second = |a: f32, b: f32| {
        shots
            .iter()
            .filter(|(t, _)| (seconds(a)..seconds(b)).contains(t))
            .count()
    };
    let first = in_second(0.0, 1.0);
    let spun = in_second(2.0, 3.0);
    assert!(
        (9..=12).contains(&first),
        "{first} rounds in the first second"
    );
    assert_eq!(spun, 15, "15 a second once spun up");
    // The aim is still: the rounds stray around it, up to the error angle.
    let aim = shots
        .iter()
        .fold(Vec3::ZERO, |sum, (_, d)| sum + *d)
        .normalize();
    let off = |a: f32, b: f32| {
        shots
            .iter()
            .filter(|(t, _)| (seconds(a)..seconds(b)).contains(t))
            .map(|(_, d)| d.angle_between(aim))
            .fold(0.0f32, f32::max)
    };
    assert!(off(0.0, 0.5) < 0.0175 + 0.004, "{}", off(0.0, 0.5));
    assert!(
        off(3.0, 4.0) > 0.02 && off(3.0, 4.0) < 0.0349 + 0.004,
        "{}",
        off(3.0, 4.0)
    );
    assert!(
        shots.windows(2).all(|p| p[0].1 != p[1].1),
        "every round strays its own way"
    );
    let moved = Vec3::from(v.snapshot(&w).vehicles[0].transform.position) - Vec3::from(rest);
    assert!(moved.length() < 0.05, "60 rounds moved it {moved}");
    // Let go, pull again: back to 8 a second.
    fire(
        &mut v,
        &mut w,
        false,
        seconds(0.5),
        &mut shots,
        seconds(4.0),
    );
    let before = shots.len();
    fire(&mut v, &mut w, true, seconds(0.5), &mut shots, seconds(4.5));
    let again = shots.len() - before;
    assert!(
        (4..=5).contains(&again),
        "{again} rounds in the first half second again"
    );
}
