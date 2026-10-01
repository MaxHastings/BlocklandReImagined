//! The Tank's turret through seat changes on the host, with no v20 content:
//! the content-free chaos Tank, and moves shaped as a real client sends
//! them in each seat (a mouse driver's raw mouse turn, a passenger's turn on
//! the seat, a gunner's look). Max, v0.1.10: "when i switch seat to the tank
//! turrent it resets its position rather than keeping whatever rotation it
//! had".
use bri_chaos::fixture;
use bri_sim::{
    player::MoveInput,
    session::{SeatSince, Session},
};
use bri_world::{Brick, ContentRef, VehicleSpawn};
use glam::{Quat, Vec3};
use std::collections::VecDeque;
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
    s.set_vehicle_pack(pack, Vec::new()).unwrap();
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
/// A client `lag` ticks away each way: it learns its seat, the hull and the
/// turret `lag` ticks late, its moves reach the host `lag` ticks after it
/// makes them, and each move is shaped by the seat it believes it is in, as
/// the real client's are: a mouse driver's raw mouse turn, a passenger's
/// turn on the seat (0 with the mouse still), a gunner's look, which it
/// turns onto the barrel when it learns it has the gun. Each move carries
/// the seat it was made for (`SeatSince`).
struct Client {
    lag: usize,
    seen: VecDeque<(Option<u8>, f32, [f32; 2])>,
    /// Moves on their way: sequence, move, the seat it was made for.
    moves: VecDeque<(u64, MoveInput, Option<SeatSince>)>,
    made: u64,
    seat: Option<u8>,
    report: Option<SeatSince>,
    look: (f32, f32),
}
impl Client {
    fn new(lag: usize, s: &Session, rider: &Rider) -> Self {
        Self {
            lag,
            seen: VecDeque::new(),
            moves: VecDeque::new(),
            seat: rider.seat(s),
            report: None,
            made: rider.sequence,
            look: (1.3, 0.0),
        }
    }
    /// One tick: the client acts on what it has heard and the host runs
    /// the move that has arrived. Returns the seat the client believes in.
    fn tick(&mut self, s: &mut Session, rider: &mut Rider) -> Option<u8> {
        self.seen.push_back((rider.seat(s), hull(s), aim(s)));
        let (seat, hull, aim) = if self.seen.len() > self.lag {
            self.seen.pop_front().unwrap()
        } else {
            *self.seen.front().unwrap()
        };
        if seat != self.seat {
            self.seat = seat;
            if seat == Some(2) {
                self.look = (wrap(hull - aim[0]), aim[1]);
            }
        }
        let made = match self.seat {
            Some(1) => MoveInput::default(),
            _ => MoveInput {
                yaw: self.look.0,
                pitch: self.look.1,
                ..Default::default()
            },
        };
        self.made += 1;
        let vehicle = s.vehicle_poses()[0].id;
        self.report = SeatSince::follow(
            self.report,
            self.seat.map(|seat| (vehicle, seat)),
            self.made,
        );
        self.moves.push_back((self.made, made, self.report));
        // A move reaches the host `lag` ticks after it is made, with the
        // seat report it was sent with.
        if self.moves.len() > self.lag {
            let (sequence, arrived, seat) = self.moves.pop_front().unwrap();
            s.seat_report(rider.owner, sequence, seat).unwrap();
            s.movement(rider.owner, sequence, arrived).unwrap();
            rider.sequence = sequence;
        }
        s.step().unwrap();
        self.seat
    }
    /// The gunner turns their look to `yaw`, `pitch`.
    fn look(&mut self, yaw: f32, pitch: f32) {
        self.look = (yaw, pitch);
    }
}
fn assert_kept(s: &Session, owner: u64, kept: [f32; 2], what: &str) {
    let now = aim(s);
    assert!(
        wrap(now[0] - kept[0]).abs() < 1e-3 && (now[1] - kept[1]).abs() < 1e-3,
        "{what}: turret at {now:?}, left at {kept:?}, rider in {:?}",
        s.mounted(owner)
    );
}
/// Round the seats to `seat` as the app test does: the client asks for the
/// next seat as soon as it hears it has a new one, so it can pass through a
/// seat before any of its moves for that seat arrive. Every tick the turret
/// must stay at `kept`, if given; then the client settles for `settle` ticks.
fn seat_to(
    s: &mut Session,
    rider: &mut Rider,
    client: &mut Client,
    seat: u8,
    kept: Option<[f32; 2]>,
) {
    let owner = rider.owner;
    let mut asked = None;
    for _ in 0..600 {
        let believed = client.tick(s, rider);
        if let Some(kept) = kept {
            assert_kept(s, owner, kept, &format!("lag {}", client.lag));
        }
        if believed == Some(seat) && rider.seat(s) == Some(seat) {
            break;
        }
        if believed.is_some() && believed != Some(seat) && asked != believed {
            asked = believed;
            rider.next_seat(s);
        }
    }
    assert_eq!(rider.seat(s), Some(seat));
    for _ in 0..(4 * client.lag + 30) {
        client.tick(s, rider);
        if let Some(kept) = kept {
            assert_kept(s, owner, kept, &format!("lag {}, settling", client.lag));
        }
    }
}

#[test]
fn the_tank_turret_keeps_its_aim_through_every_seat_change() {
    for lag in [0, 1, 4, 12] {
        let (mut s, mut rider) = boarded();
        let mut client = Client::new(lag, &s, &rider);
        seat_to(&mut s, &mut rider, &mut client, 2, None);
        // The gunner looks well round to the right of the hull.
        client.look(wrap(hull(&s) + 2.5), 0.2);
        for _ in 0..(2 * lag + 10) {
            client.tick(&mut s, &mut rider);
        }
        let aimed = aim(&s);
        assert!(
            wrap(aimed[0] + 2.5).abs() < 1e-3,
            "lag {lag}: turret at {aimed:?}"
        );
        // Gun to driver, through the passenger's seat back to the gun, twice.
        for _ in 0..2 {
            seat_to(&mut s, &mut rider, &mut client, 0, Some(aimed));
            seat_to(&mut s, &mut rider, &mut client, 2, Some(aimed));
        }
        // Then the gunner turns it again.
        client.look(wrap(hull(&s) - 1.0), 0.0);
        for _ in 0..(2 * lag + 10) {
            client.tick(&mut s, &mut rider);
        }
        let now = aim(&s);
        assert!((now[0] - 1.0).abs() < 1e-3, "lag {lag}: turret at {now:?}");
    }
}

/// A gunner turns the turret from their first move after looking along it.
#[test]
fn a_new_gunner_aims_the_turret_once_they_look_along_it() {
    let (mut s, mut rider) = boarded();
    let mut client = Client::new(3, &s, &rider);
    seat_to(&mut s, &mut rider, &mut client, 2, None);
    client.look(wrap(hull(&s) + 0.5), 0.2);
    for _ in 0..10 {
        client.tick(&mut s, &mut rider);
    }
    let now = aim(&s);
    assert!(
        (now[0] + 0.5).abs() < 0.01 && (now[1] - 0.2).abs() < 0.01,
        "turret at {now:?}"
    );
}

/// A seat report from an older datagram, arriving late, changes nothing: the
/// gunner keeps turning the turret.
#[test]
fn a_late_report_from_the_old_seat_is_ignored() {
    let (mut s, mut rider) = boarded();
    let mut client = Client::new(2, &s, &rider);
    seat_to(&mut s, &mut rider, &mut client, 2, None);
    let vehicle = s.vehicle_poses()[0].id;
    let stale = SeatSince {
        vehicle,
        seat: 1,
        since: 1,
    };
    s.seat_report(rider.owner, 1, Some(stale)).unwrap();
    client.look(wrap(hull(&s) + 0.7), 0.1);
    for _ in 0..10 {
        client.tick(&mut s, &mut rider);
    }
    let now = aim(&s);
    assert!(
        (now[0] + 0.7).abs() < 0.01 && (now[1] - 0.1).abs() < 0.01,
        "turret at {now:?}"
    );
}
