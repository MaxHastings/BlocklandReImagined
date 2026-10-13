//! Three players in the Playground buggy (its driver, its passenger and its
//! gunner) over a bad Internet connection: 150 ms each way with jitter on
//! top, and one datagram in twenty lost, each way. The host is the real
//! session running the buggy; each player is this client's own code for
//! what they see: the driver predicts the buggy (`Motion::drive`), the
//! passenger and the gunner draw it from the host's poses
//! (`ClientVehicles::update`) on their own server clock, and the gunner
//! aims its gun from their own look (`ClientVehicles::aim_locally`). What
//! each sees is measured against the host frame by frame: the jolts a
//! rubber-banding vehicle shows, how far it is drawn from where the host
//! had it, and where the gun points on the host against where the gunner
//! looked.
use super::*;
use bri_net::protocol::MOVEMENT_REDUNDANCY;
use bri_sim::{
    session::{SeatSince, Session, VehicleInfo, VehiclePose},
    simulation::Simulation,
};
use bri_vehicles::{OwnerId as VehicleOwner, Spawn, Transform, VehicleId};
use bri_world::World;
use rapier3d::prelude::{ColliderBuilder, Vector};
use std::{collections::BTreeMap, path::PathBuf};

const BUGGY: &str = "playground-buggy:vehicle/buggy";
/// One way, before jitter.
const ONE_WAY: f64 = 0.150;
/// Extra one-way delay, uniform in 0..=JITTER: it reorders datagrams.
const JITTER: f64 = 0.040;
/// The share of datagrams lost, each way.
const LOSS: f64 = 0.05;
/// How long the measured drive lasts, and when measuring starts (the
/// clocks and queues have settled by then).
const SECONDS: f64 = 14.0;
const SETTLED: f64 = 3.0;

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/playground/playground-buggy/assets")
}
fn ground() -> ColliderBuilder {
    ColliderBuilder::cuboid(1000., 0.5, 1000.).translation(Vector::new(0., -0.5, 0.))
}

/// Deterministic randomness (xorshift64), so a failing run replays.
struct Dice(u64);
impl Dice {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// The host with its three players, fed directly (no link) until seated.
struct Host {
    s: Session,
    sequence: BTreeMap<u64, u64>,
    looks: BTreeMap<u64, MoveInput>,
}
impl Host {
    fn steps(&mut self, n: usize) -> Result<()> {
        for _ in 0..n {
            for (owner, input) in &self.looks {
                let s = self.sequence.entry(*owner).or_default();
                *s += 1;
                self.s.movement(*owner, *s, *input)?;
            }
            self.s.step()?;
        }
        Ok(())
    }
    /// Backs off, then runs and hops on from behind (v20 boards from above).
    fn board(&mut self, owner: u64) -> Result<(u64, u8)> {
        for _ in 0..30 {
            self.looks.insert(
                owner,
                MoveInput {
                    forward: -1.,
                    ..Default::default()
                },
            );
            self.steps(20)?;
            for i in 0..30 {
                self.looks.insert(
                    owner,
                    MoveInput {
                        forward: 1.,
                        jump: i < 3,
                        ..Default::default()
                    },
                );
                self.steps(1)?;
                if let Some(seat) = self.s.mounted(owner) {
                    self.looks.insert(owner, MoveInput::default());
                    return Ok(seat);
                }
            }
        }
        anyhow::bail!("{owner} never boarded the buggy")
    }
    fn pose(&self, id: u64) -> VehiclePose {
        self.s
            .vehicle_poses()
            .into_iter()
            .find(|p| p.id == id)
            .unwrap()
    }
}

/// A player's client: its prediction and clock (`Motion`), what it draws of
/// the buggy (`ClientVehicles`), and its two ends of the link.
struct Player {
    owner: u64,
    seat: u8,
    motion: Motion,
    vehicles: crate::vehicles::ClientVehicles,
    /// The newest pose of the buggy heard, as the client's view keeps it.
    newest: BTreeMap<u64, VehiclePose>,
    /// Moves on their way to the host: (arrival, newest, moves).
    to_host: Vec<(f64, u64, Vec<MoveInput>)>,
    /// Poses on their way to the client: (arrival, pose).
    to_client: Vec<(f64, VehiclePose)>,
    /// Each move it sent, by sequence.
    sent: BTreeMap<u64, MoveInput>,
    /// What it drew last frame: (position, rotation, velocity).
    drawn: Option<(Vec3, glam::Quat, Vec3)>,
    seen: Seen,
}

/// What one player saw over the drive.
#[derive(Default, Debug)]
struct Seen {
    /// The largest change in the drawn buggy's velocity from one frame to
    /// the next (units/s): a jolt.
    jolt: f32,
    /// Frames whose velocity changed by more than [`ROUGH`] units/s.
    rough: usize,
    /// The farthest the drawn buggy was from the host's at the moment it
    /// stands for (units): for a driver, its prediction against the host's
    /// later pose for the same move; for a rider, against the host at the
    /// tick drawn.
    apart: f32,
    /// Frames the rider's view ran past the newest pose it had.
    starved: usize,
    frames: usize,
    /// For the driver: corrections that moved the drawn vehicle's warp by
    /// more than a centimetre or a hundredth of a radian, the worst one.
    corrections: usize,
    worst: (f32, f32),
}
/// The host's gun at a tick: (tick, the gunner's move it ran, the gun's
/// world look, the look on the gunner's screen then).
type Aim = (u64, u64, (f32, f32), Option<f32>);
/// A frame-to-frame change in the drawn velocity no driving does: 8
/// units/s within a 6-25 ms frame is over 30 g.
const ROUGH: f32 = 8.0;

impl Player {
    fn new(
        host: &Host,
        owner: u64,
        seat: u8,
        pack: &bri_vehicles::Pack,
        buggy: u64,
    ) -> Result<Self> {
        let rider = host
            .s
            .motion_states()
            .into_iter()
            .find(|(p, _)| p.owner == owner)
            .unwrap()
            .0;
        let mut mirror =
            CollisionMirror::new(bri_sim::testing::definitions(), vec![ground()], vec![]);
        mirror.sync(&host.s.simulation().state().bricks)?;
        let mut motion = Motion {
            predictor: Some(Predictor::new(mirror, rider, Default::default())?),
            mounted: true,
            ..Default::default()
        };
        motion
            .predictor
            .as_mut()
            .unwrap()
            .continue_after(host.sequence[&owner]);
        let start = host.pose(buggy);
        if seat == 0 {
            let info = host
                .s
                .vehicle_infos()
                .into_iter()
                .find(|v| v.id == buggy)
                .unwrap();
            motion.drive(Some((
                buggy,
                pack.clone(),
                bri_sim::prediction::DriveSpawn {
                    spawn: Spawn {
                        id: VehicleId(buggy),
                        owner: VehicleOwner(owner),
                        definition: info.definition.clone(),
                        scale: info.scale,
                        transform: Transform::default(),
                        spawn_id: None,
                        respawn_ticks: None,
                    },
                    seat: 0,
                    prefs: (!start.driver_steering.0, !start.driver_steering.1),
                },
                start.motion(),
            )))?;
        }
        motion.observe_clock(start.tick);
        Ok(Self {
            owner,
            seat,
            motion,
            vehicles: Default::default(),
            newest: [(buggy, start)].into(),
            to_host: vec![],
            to_client: vec![],
            sent: BTreeMap::new(),
            drawn: None,
            seen: Seen::default(),
        })
    }
}

#[test]
fn three_in_the_buggy_over_a_bad_connection_do_not_rubber_band() -> Result<()> {
    let pack = bri_vehicles::Pack::load(assets().join("vehicles.json"))?;
    let weapons = bri_weapons::Pack::from_json(&std::fs::read(assets().join("weapons.json"))?)?;
    let d = pack
        .definitions
        .iter()
        .find(|d| d.id == BUGGY)
        .unwrap()
        .clone();
    let world = World::new("Crew".into(), "test".into(), vec![[1.; 4]]);
    let mut s = Session::new(Simulation::new(
        world,
        bri_sim::testing::definitions(),
        vec![ground()],
    )?);
    s.set_weapon_pack(weapons)?;
    s.set_vehicle_pack(pack.clone(), Vec::new())?;
    let mut host = Host {
        s,
        sequence: BTreeMap::new(),
        looks: BTreeMap::new(),
    };
    // Each walks up from behind (the buggy faces -z) and climbs on: the
    // first takes the wheel, the next the passenger's seat, the last the gun.
    let mut crew = vec![];
    let buggy = host
        .s
        .spawn_vehicle_at(0, BUGGY, Vec3::new(0., 1., 0.), 0., Vec3::ZERO)?;
    for (i, name) in ["Driver", "Passenger", "Gunner"].into_iter().enumerate() {
        let owner = host.s.join(name.into(), Vec3::new(0., 0.05, 8.), false)?;
        host.looks.insert(owner, MoveInput::default());
        host.steps(60)?;
        let (vehicle, seat) = host.board(owner)?;
        assert_eq!((vehicle, usize::from(seat)), (buggy, i), "{name}'s seat");
        host.steps(60)?;
        crew.push((owner, seat));
    }
    let infos: BTreeMap<u64, VehicleInfo> = host
        .s
        .vehicle_infos()
        .into_iter()
        .map(|v| (v.id, v))
        .collect();
    let mut players = crew
        .iter()
        .map(|&(owner, seat)| Player::new(&host, owner, seat, &pack, buggy))
        .collect::<Result<Vec<_>>>()?;
    let passages = bri_content::passage::Passages::default();

    // What each player does: the driver floors it and weaves by looking
    // (the buggy steers toward the look), braking hard once; the passenger
    // looks about; the gunner keeps the gun on a point far off the road.
    let mark = Vec3::new(-150., 1., -150.);
    let driver_input = |t: f64| MoveInput {
        forward: if (7.0..7.8).contains(&t) { 0. } else { 1. },
        jump: (7.0..7.8).contains(&t),
        head_yaw: ((t * 0.9).sin() * 0.9 + if (t * 0.7).fract() < 0.1 { 0.6 } else { 0. }) as f32,
        ..Default::default()
    };
    let passenger_input = |t: f64| MoveInput {
        yaw: ((t * 1.1).sin() * 1.4) as f32,
        pitch: ((t * 0.8).sin() * 0.3) as f32,
        ..Default::default()
    };

    let mut dice = Dice(0x9e37_79b9_7f4a_7c15);
    let delay = |dice: &mut Dice| ONE_WAY + dice.next() * JITTER;
    // The host's truth: the buggy's place by tick, and its gun's world look
    // with the gunner's move it was taken from.
    let mut truth: BTreeMap<u64, (Vec3, glam::Quat)> = BTreeMap::new();
    let mut aims: Vec<Aim> = vec![];
    let (mut time, mut host_tick) = (0.0f64, host.pose(buggy).tick);
    let start_tick = host_tick;
    let mut lost = (0usize, 0usize);
    while time < SECONDS {
        let frame = 0.006 + dice.next() * 0.019;
        time += frame;
        // The host's ticks up to now.
        while (host_tick - start_tick + 1) as f64 / TICK_RATE <= time {
            for p in &mut players {
                let due: Vec<_> = p
                    .to_host
                    .iter()
                    .filter(|(at, ..)| *at <= time)
                    .cloned()
                    .collect();
                p.to_host.retain(|(at, ..)| *at > time);
                for (_, newest, moves) in due {
                    let seat = Some(SeatSince {
                        vehicle: buggy,
                        seat: p.seat,
                        since: 0,
                    });
                    let first = newest + 1 - moves.len() as u64;
                    host.s.seat_report(p.owner, newest, seat)?;
                    for (sequence, input) in (first..).zip(moves) {
                        let _ = host.s.movement(p.owner, sequence, input);
                    }
                }
            }
            host.s.step()?;
            let pose = host.pose(buggy);
            host_tick = pose.tick;
            truth.insert(
                pose.tick,
                (
                    Vec3::from(pose.position),
                    glam::Quat::from_array(pose.rotation),
                ),
            );
            let gunner = crew[2].0;
            let acknowledged = bri_net::protocol::poses(&host.s)
                .into_iter()
                .find(|p| p.player.owner == gunner)
                .map_or(0, |p| p.acknowledged_input);
            // And the gunner's look on their screen at that moment.
            let looking = players[2].sent.values().last().map(|m| m.yaw);
            aims.push((
                pose.tick,
                acknowledged,
                crate::vehicles::turret_look(&pose),
                looking,
            ));
            if host_tick % POSE_INTERVAL == 0 {
                for p in &mut players {
                    if dice.next() < LOSS {
                        lost.1 += 1;
                        continue;
                    }
                    p.to_client.push((time + delay(&mut dice), pose.clone()));
                }
            }
        }
        // Each client's frame, as `App::tick` runs it.
        for p in &mut players {
            let input = match p.seat {
                0 => driver_input(time),
                1 => passenger_input(time),
                _ => {
                    // The gunner looks at the mark from where they see the
                    // buggy now.
                    let at = p.drawn.map_or(Vec3::ZERO, |(at, ..)| at);
                    let to = mark - at;
                    MoveInput {
                        yaw: to.x.atan2(-to.z),
                        pitch: (to.y / to.length()).asin(),
                        ..Default::default()
                    }
                }
            };
            if let Some((sequence, moves)) =
                p.motion.advance(frame as f32, input, MOVEMENT_REDUNDANCY)?
            {
                let first = sequence + 1 - moves.len() as u64;
                for (s, m) in (first..).zip(&moves) {
                    p.sent.insert(s, *m);
                }
                if dice.next() < LOSS {
                    lost.0 += 1;
                } else {
                    p.to_host.push((time + delay(&mut dice), sequence, moves));
                }
            }
            let arrived: Vec<_> = p
                .to_client
                .iter()
                .filter(|(at, _)| *at <= time)
                .map(|(_, pose)| pose.clone())
                .collect();
            p.to_client.retain(|(at, _)| *at > time);
            for pose in arrived {
                p.motion.observe_clock(pose.tick);
                if p.newest.get(&pose.id).is_none_or(|n| n.tick < pose.tick) {
                    p.newest.insert(pose.id, pose);
                }
            }
            let measuring = time > SETTLED;
            let (position, rotation) = if p.seat == 0 {
                let (offset, turn) = (p.motion.drive_offset, p.motion.drive_turn);
                p.motion.observe_vehicle(&p.newest[&buggy])?;
                let moved = p.motion.drive_offset.distance(offset);
                let turned = p.motion.drive_turn.angle_between(turn);
                if measuring {
                    p.seen.worst = (p.seen.worst.0.max(moved), p.seen.worst.1.max(turned));
                    if moved > 0.01 || turned > 0.01 {
                        p.seen.corrections += 1;
                    }
                }
                let (_, position, rotation) = p.motion.driven_frame().unwrap();
                (position, rotation)
            } else {
                let now = p.motion.server_tick();
                p.vehicles.update(&infos, &p.newest, now, None, &passages);
                if p.seat == 2 {
                    p.vehicles.aim_locally(buggy, &d, input.yaw, input.pitch);
                }
                let f = p.vehicles.frame(buggy).unwrap();
                if let Some(now) = now
                    && measuring
                {
                    let shown = now - crate::vehicles::INTERPOLATION_TICKS;
                    if shown > p.newest[&buggy].tick as f64 {
                        p.seen.starved += 1;
                    }
                    let below = shown.floor() as u64;
                    if let (Some(a), Some(b)) = (truth.get(&below), truth.get(&(below + 1))) {
                        let t = (shown - below as f64) as f32;
                        p.seen.apart = p.seen.apart.max(f.position.distance(a.0.lerp(b.0, t)));
                    }
                }
                (f.position, f.rotation)
            };
            let velocity = p
                .drawn
                .map_or(Vec3::ZERO, |(at, ..)| (position - at) / frame as f32);
            if let Some((_, _, before)) = p.drawn
                && measuring
            {
                let jolt = velocity.distance(before);
                p.seen.jolt = p.seen.jolt.max(jolt);
                if jolt > ROUGH {
                    p.seen.rough += 1;
                }
                p.seen.frames += 1;
            }
            p.drawn = Some((position, rotation, velocity));
        }
    }
    for p in &players {
        println!(
            "seat {}: jolt {:.2} u/s, {} rough of {} frames, {} starved, apart {:.3}, {} corrections (worst {:.4} u {:.4} rad)",
            p.seat,
            p.seen.jolt,
            p.seen.rough,
            p.seen.frames,
            p.seen.starved,
            p.seen.apart,
            p.seen.corrections,
            p.seen.worst.0,
            p.seen.worst.1
        );
    }
    let wrap = |a: f32| {
        (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
    };
    // The gun on the host points where the gunner looked in the move the
    // host ran; behind the look on the gunner's screen by the trip there.
    let gunner = &players[2];
    let (mut aim_off, mut behind, mut sum, mut n) = (0f32, 0f32, 0f32, 0);
    for (tick, acknowledged, (yaw, _), looking) in &aims {
        if (*tick - start_tick) as f64 / TICK_RATE <= SETTLED {
            continue;
        }
        if let Some(look) = gunner.sent.get(acknowledged) {
            aim_off = aim_off.max(wrap(yaw - look.yaw).abs());
        }
        if let Some(looking) = looking {
            let off = wrap(yaw - looking).abs();
            behind = behind.max(off);
            sum += off;
            n += 1;
        }
    }
    let travelled: f32 = truth
        .values()
        .zip(truth.values().skip(1))
        .map(|(a, b)| a.0.distance(b.0))
        .sum();
    println!(
        "drove {travelled:.0} units; lost {} moves and {} poses; gun off the gunner's move {aim_off:.4} rad, behind their screen {behind:.4} rad (mean {:.4})",
        lost.0,
        lost.1,
        sum / n.max(1) as f32
    );
    assert!(lost.0 > 0 && lost.1 > 0, "the link lost datagrams");
    assert!(travelled > 100.0, "the buggy drove: {travelled}");
    let driver = &players[0].seen;
    // The driver's view never jolts, and the few corrections (a host tick
    // run without their move, after a late burst) ease in, small.
    assert_eq!(driver.rough, 0, "the driver's buggy jolted");
    assert!(
        driver.corrections <= 4 && driver.worst.0 < 0.3 && driver.worst.1 < 0.03,
        "the driver's buggy was corrected: {driver:?}"
    );
    for rider in &players[1..] {
        let seen = &rider.seen;
        // Riders see the host's buggy a few ticks late and smooth: rarely
        // run past the newest pose, never more than a few centimetres from
        // where the host had it.
        assert_eq!(seen.rough, 0, "seat {}: the buggy jolted", rider.seat);
        assert!(
            seen.starved * 100 <= seen.frames && seen.apart < 0.1,
            "seat {}: {seen:?}",
            rider.seat
        );
    }
    assert!(aim_off < 0.005, "the gun points off the gunner's look");
    Ok(())
}
