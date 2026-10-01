//! The Tank's turret through seat changes on the host, with no v20 content:
//! the content-free chaos Tank, and moves shaped as a real client sends
//! them in each seat (a mouse driver's raw mouse turn, a passenger's turn on
//! the seat, a gunner's look). Max, v0.1.10: "when i switch seat to the tank
//! turrent it resets its position rather than keeping whatever rotation it
//! had".
use bri_chaos::fixture;
use bri_sim::{player::MoveInput, session::Session};
use bri_world::{Brick, ContentRef, VehicleSpawn};
use glam::{Quat, Vec3};
use std::f32::consts::{PI, TAU};

const TANK: &str = "v20.vehicle.chaostank";

fn wrap(a: f32) -> f32 {
    (a + PI).rem_euclid(TAU) - PI
}
struct Rider {
    owner: u64,
    sequence: u64,
}
impl Rider {
    fn feed(&mut self, s: &mut Session, input: MoveInput, ticks: usize) {
        for _ in 0..ticks {
            self.sequence += 1;
            s.movement(self.owner, self.sequence, input).unwrap();
            s.step().unwrap();
        }
    }
    fn seat(&self, s: &Session) -> Option<u8> {
        s.mounted(self.owner).map(|(_, seat)| seat)
    }
    /// Next seat round, as `/nextSeat` does.
    fn next_seat(&mut self, s: &mut Session) {
        s.switch_seat(self.owner, 1).unwrap();
    }
}
fn hull(s: &Session) -> f32 {
    let forward = Quat::from_array(s.vehicle_poses()[0].rotation) * Vec3::NEG_Z;
    forward.x.atan2(-forward.z)
}
fn aim(s: &Session) -> [f32; 2] {
    s.vehicle_poses()[0].turret_aim
}
/// A session with the chaos Tank on a spawn brick 6 ahead of a player who
/// has jumped on it.
fn boarded() -> (Session, Rider) {
    let mut brick = Brick::new(
        ContentRef::Resolved(fixture::PLATE.into()),
        [0.25, 0.1, -6.25],
        0,
    );
    brick.vehicle = Some(VehicleSpawn {
        vehicle: ContentRef::Resolved(TANK.into()),
        recolor: false,
    });
    let mut s = Session::new(fixture::synthetic_simulation(&[brick]).unwrap());
    let (pack, _) = fixture::synthetic_vehicles_with_tank().unwrap();
    s.set_vehicle_pack(pack).unwrap();
    s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 0.0)]).unwrap();
    let owner = s
        .join("Gunner".into(), Vec3::new(0.0, 0.05, 0.0), true)
        .unwrap();
    let mut rider = Rider { owner, sequence: 0 };
    rider.feed(&mut s, MoveInput::default(), 60);
    assert_eq!(s.vehicle_poses().len(), 1, "the Tank spawned");
    for i in 0..600 {
        if rider.seat(&s).is_some() {
            break;
        }
        let to = Vec3::from(s.vehicle_poses()[0].position);
        let feet = Vec3::from(
            s.snapshot()
                .players
                .into_iter()
                .find(|p| p.owner == owner)
                .unwrap()
                .feet,
        );
        let d = to - feet;
        rider.feed(
            &mut s,
            MoveInput {
                forward: 1.0,
                jump: i % 3 == 0,
                yaw: d.x.atan2(-d.z),
                ..Default::default()
            },
            1,
        );
    }
    assert!(rider.seat(&s).is_some(), "never boarded the Tank");
    (s, rider)
}
/// Round the seats to `seat`, sending each seat's moves as a client does:
/// for `lag` ticks after each change the moves still in flight carry the
/// previous seat's look; then the new seat's. A gunner's client looks
/// along the barrel. Every tick the turret must stay at `kept`, if given.
fn seat_to(s: &mut Session, rider: &mut Rider, seat: u8, kept: Option<[f32; 2]>, lag: usize) {
    // What the client sends in each seat, with the mouse still: a mouse
    // driver's raw turn, a passenger's turn on the seat, a gunner's look.
    let mut last = MoveInput {
        yaw: 0.7,
        ..Default::default()
    };
    for _ in 0..3 {
        if rider.seat(s) == Some(seat) {
            return;
        }
        rider.next_seat(s);
        let owner = rider.owner;
        let check = |s: &Session| {
            if let Some(kept) = kept {
                let now = aim(s);
                assert!(
                    wrap(now[0] - kept[0]).abs() < 1e-3 && (now[1] - kept[1]).abs() < 1e-3,
                    "seat {:?}: turret at {now:?}, left at {kept:?}",
                    s.mounted(owner)
                );
            }
        };
        for _ in 0..lag {
            rider.feed(s, last, 1);
            check(s);
        }
        last = match rider.seat(s) {
            Some(0) => MoveInput {
                yaw: 1.3,
                ..Default::default()
            },
            Some(1) => MoveInput::default(),
            Some(2) => {
                let [yaw, pitch] = aim(s);
                MoveInput {
                    yaw: wrap(hull(s) - yaw),
                    pitch,
                    ..Default::default()
                }
            }
            other => panic!("seat {other:?}"),
        };
        for _ in 0..10 {
            rider.feed(s, last, 1);
            check(s);
        }
    }
    assert_eq!(rider.seat(s), Some(seat));
}

#[test]
fn the_tank_turret_keeps_its_aim_through_every_seat_change() {
    let (mut s, mut rider) = boarded();
    seat_to(&mut s, &mut rider, 2, None, 4);
    // The gunner looks well round to the right of the hull.
    let look = MoveInput {
        yaw: wrap(hull(&s) + 2.5),
        pitch: 0.2,
        ..Default::default()
    };
    rider.feed(&mut s, look, 20);
    let aimed = aim(&s);
    assert!((wrap(aimed[0] + 2.5)).abs() < 1e-3, "turret at {aimed:?}");
    // Gun to driver to passenger to gun, with moves in flight each time.
    for lag in [0, 1, 6] {
        seat_to(&mut s, &mut rider, 0, Some(aimed), lag);
        rider.feed(
            &mut s,
            MoveInput {
                yaw: 1.3,
                ..Default::default()
            },
            30,
        );
        seat_to(&mut s, &mut rider, 2, Some(aimed), lag);
        let along = wrap(hull(&s) - aimed[0]);
        rider.feed(
            &mut s,
            MoveInput {
                yaw: along,
                pitch: aimed[1],
                ..Default::default()
            },
            30,
        );
        let now = aim(&s);
        assert!(
            wrap(now[0] - aimed[0]).abs() < 1e-3,
            "lag {lag}: back on the gun at {now:?}, left at {aimed:?}"
        );
    }
    // Then the gunner turns it again.
    let left = wrap(hull(&s) - 1.0);
    rider.feed(
        &mut s,
        MoveInput {
            yaw: left,
            ..Default::default()
        },
        10,
    );
    let now = aim(&s);
    assert!((now[0] - 1.0).abs() < 1e-3, "turret at {now:?}");
}

/// A gunner whose look turns on the first move after sitting down aims the
/// turret there at once: only moves still carrying the look they boarded
/// with leave it.
#[test]
fn a_new_gunner_who_looks_away_aims_the_turret_at_once() {
    let (mut s, mut rider) = boarded();
    seat_to(&mut s, &mut rider, 2, None, 0);
    let look = MoveInput {
        yaw: wrap(hull(&s) + 0.5),
        pitch: 0.2,
        ..Default::default()
    };
    rider.feed(&mut s, look, 10);
    let now = aim(&s);
    assert!(
        (now[0] + 0.5).abs() < 0.01 && (now[1] - 0.2).abs() < 0.01,
        "turret at {now:?}"
    );
}
