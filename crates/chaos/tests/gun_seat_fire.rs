//! A gunner's held fire belongs to the gun seat it was pressed in. Pressing
//! fire in a gun seat, moving to a seat without a gun and letting go there
//! must not leave the gun "held": it would ignore the next press, or fire
//! by itself in another vehicle's gun seat.
use bri_chaos::fixture;
use bri_sim::{
    player::MoveInput,
    session::{Command, Session},
};
use bri_world::{Brick, ContentRef, VehicleSpawn};
use glam::Vec3;

const CANNON: &str = "v20.vehicle.chaoscannon";

struct Player {
    owner: u64,
    sequence: u64,
}
impl Player {
    fn feed(&mut self, s: &mut Session, input: MoveInput, ticks: usize) {
        for _ in 0..ticks {
            self.sequence += 1;
            s.movement(self.owner, self.sequence, input).unwrap();
            s.step().unwrap();
        }
    }
    fn command(&mut self, s: &mut Session, command: Command) {
        self.sequence += 1;
        s.command(self.owner, self.sequence, command).unwrap();
    }
    /// Run at the vehicle, hopping, until mounted (v20 mounts only from above).
    fn board(&mut self, s: &mut Session) {
        for i in 0..60 {
            let input = MoveInput {
                forward: 1.0,
                jump: i % 3 == 0,
                ..Default::default()
            };
            self.feed(s, input, 10);
            if s.mounted(self.owner).is_some() {
                return;
            }
        }
        panic!("never boarded");
    }
}

fn shots(s: &Session) -> usize {
    s.weapon_view().projectiles.len()
}

#[test]
fn a_release_in_another_seat_ends_the_gun_seat_hold() {
    let mut brick = Brick::new(
        ContentRef::Resolved(fixture::BASEPLATE.into()),
        [0.0, 0.1, -8.0],
        0,
    );
    brick.vehicle = Some(VehicleSpawn {
        vehicle: ContentRef::Resolved(CANNON.into()),
        recolor: false,
    });
    let mut s = Session::new(fixture::synthetic_simulation(&[brick]).unwrap());
    let (weapons, _) = fixture::synthetic_weapons().unwrap();
    let (vehicles, _) = fixture::synthetic_vehicles().unwrap();
    s.set_weapon_pack(weapons).unwrap();
    s.set_vehicle_pack(vehicles, Vec::new()).unwrap();
    let owner = s
        .join("Gunner".into(), Vec3::new(0.0, 0.05, 0.0), true)
        .unwrap();
    let mut p = Player { owner, sequence: 0 };
    p.feed(&mut s, MoveInput::default(), 60);
    p.board(&mut s);
    assert_eq!(s.mounted(owner).map(|m| m.1), Some(0), "the gun seat");
    // A press fires the gun.
    p.command(&mut s, Command::WeaponTrigger { down: true });
    p.feed(&mut s, MoveInput::default(), 2);
    assert!(shots(&s) > 0, "the gun seat fires when pressed");
    p.command(&mut s, Command::WeaponTrigger { down: false });
    p.feed(&mut s, MoveInput::default(), 240);
    assert_eq!(shots(&s), 0, "the shot has landed");
    // Press fire in the gun seat, move to the seat without a gun, let go.
    p.command(&mut s, Command::WeaponTrigger { down: true });
    p.feed(&mut s, MoveInput::default(), 2);
    s.switch_seat(owner, 1).unwrap();
    assert_eq!(s.mounted(owner).map(|m| m.1), Some(1), "the passenger seat");
    p.command(&mut s, Command::WeaponTrigger { down: false });
    p.feed(&mut s, MoveInput::default(), 240);
    let before = shots(&s);
    // Round the seats back into the gun seat: nothing is held there now,
    // so it waits for a press, and a press fires it.
    s.switch_seat(owner, 1).unwrap();
    s.switch_seat(owner, 1).unwrap();
    assert_eq!(s.mounted(owner).map(|m| m.1), Some(0), "the gun seat again");
    p.feed(&mut s, MoveInput::default(), 40);
    assert_eq!(shots(&s), before, "the gun fires on its own");
    p.command(&mut s, Command::WeaponTrigger { down: true });
    p.feed(&mut s, MoveInput::default(), 2);
    assert!(shots(&s) > before, "the gun answers the next press");
}
