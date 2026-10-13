//! The Wire Lite sample (`packages/samples/sample-wire-lite`), a small
//! Wiremod port onto the Add-On building blocks: bricks become switches,
//! gates and a lamp by aiming (`aim_reach`), are wired by aiming and by
//! dragging one gate onto another in the circuit panel, and an
//! empty-handed click (`on_activate`) on a switch sends the signal
//! through to the lamp's colour.
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::{
    Catalog,
    panel::{PanelDrop, PanelEvent},
};
use bri_sim::{
    definitions::{Definition, Definitions},
    player::MoveInput,
    session::{Command, PackageArg, PackageCommand, PanelCommand, Reply, Session},
    simulation::Simulation,
};
use bri_world::{OwnerId, World};
use glam::Vec3;
use rapier3d::prelude::*;
use std::{path::PathBuf, sync::Arc};

const WIRE: &str = "sample-wire-lite";
/// The palette: white, the lit colour and the dark one.
const LIT: u8 = 1;
const DARK: u8 = 2;
/// Height of a standing player's eye over their feet.
const EYE: f32 = 2.156;

fn add_ons() -> Arc<Catalog> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/samples");
    let set = PackageSet {
        schema_version: 1,
        packages: vec![PackageEntry {
            id: WIRE.into(),
            version: "1.0.0".into(),
            side: Side::Server,
            dir: WIRE.into(),
            role: None,
        }],
    };
    Arc::new(Catalog::load(&root, &set, true).unwrap_or_else(|e| panic!("{e:#?}")))
}

fn session() -> Session {
    let mesh = Mesh {
        schema_version: 1,
        id: "plate".into(),
        footprint_studs: [2, 2],
        height_plates: 3,
        attachment_rows: vec!["bb".into(); 6],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![],
    };
    let collision = CollisionBody {
        id: "plate".into(),
        parts: vec![Part::Box {
            center: [0.0; 3],
            size: [1.0, 0.6, 1.0],
        }],
    };
    let shape = bri_physics::content::collider(&collision)
        .unwrap()
        .build()
        .shared_shape()
        .clone();
    let definitions = Definitions {
        entries: [(
            "plate".into(),
            Definition {
                mesh,
                collision,
                shape,
                indestructible: false,
                special: Default::default(),
                reflection: None,
                link: None,
                glass: [0.0; 4],
                bot: None,
            },
        )]
        .into(),
    };
    let palette = vec![[1.0; 4], [1.0, 0.85, 0.1, 1.0], [0.25, 0.25, 0.25, 1.0]];
    let mut s = Session::new(
        Simulation::new(
            World::new("Lab".into(), "lab".into(), palette),
            definitions,
            vec![
                ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0)),
            ],
        )
        .unwrap(),
    );
    s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 0.0)]).unwrap();
    s.install_packages(add_ons(), None).unwrap();
    s
}

struct Lab {
    s: Session,
    p: OwnerId,
    seq: u64,
    moves: u64,
    look: MoveInput,
}
impl Lab {
    fn new() -> Self {
        let mut s = session();
        let p = s
            .join("Builder".into(), Vec3::new(0.0, 0.05, 0.0), false)
            .unwrap();
        let mut lab = Self {
            s,
            p,
            seq: 0,
            moves: 0,
            look: MoveInput::default(),
        };
        lab.steps(10);
        lab
    }
    fn steps(&mut self, n: usize) {
        for _ in 0..n {
            self.moves += 1;
            self.s.movement(self.p, self.moves, self.look).unwrap();
            self.s.step().unwrap();
        }
    }
    fn send(&mut self, command: Command) -> anyhow::Result<Reply> {
        self.seq += 1;
        let reply = self.s.command(self.p, self.seq, command);
        self.steps(1);
        let problems = self.s.take_package_problems();
        assert!(problems.is_empty(), "{problems:#?}");
        reply
    }
    fn plant(&mut self, x: f32) -> u64 {
        let Reply::Planted(brick) = self
            .send(Command::Plant {
                definition: "plate".into(),
                position: [x, 0.3, -3.0],
                quarter_turns: 0,
                color: 0,
            })
            .unwrap()
        else {
            panic!("expected a plant")
        };
        brick
    }
    fn look_at(&mut self, brick: u64) {
        let b = &self.s.simulation().state().bricks[&brick];
        let target = Vec3::from(b.position) + Vec3::Y * 0.2;
        let feet = Vec3::from(
            self.s
                .snapshot()
                .players
                .into_iter()
                .find(|p| p.owner == self.p)
                .unwrap()
                .feet,
        );
        let d = target - (feet + Vec3::Y * EYE);
        self.look.yaw = d.x.atan2(-d.z);
        self.look.pitch = d.y.atan2(Vec3::new(d.x, 0.0, d.z).length());
        self.steps(3);
    }
    fn run(&mut self, command: &str, args: Vec<PackageArg>) {
        self.send(Command::Package(PackageCommand {
            package: WIRE.into(),
            command: command.into(),
            args,
        }))
        .unwrap();
    }
    fn gate(&mut self, brick: u64, kind: &str) {
        self.look_at(brick);
        self.run("gate", vec![PackageArg::String(kind.into())]);
    }
    fn click(&mut self, brick: u64) {
        self.look_at(brick);
        self.send(Command::Activate).unwrap();
        self.send(Command::ActivateRelease).unwrap();
    }
    fn color(&self, brick: u64) -> u8 {
        self.s.simulation().state().bricks[&brick].color
    }
    /// The circuit window's rows.
    fn rows(&self) -> Vec<String> {
        self.s
            .open_panel(self.p)
            .expect("the circuit is open")
            .panel
            .items[1]
            .rows
            .clone()
    }
    fn row_of(&self, brick: u64) -> u16 {
        self.rows()
            .iter()
            .position(|r| r.starts_with(&format!("#{brick} ")))
            .unwrap() as u16
    }
    /// Drag one gate's row onto another's in the circuit window.
    fn drag(&mut self, from: u64, to: u64) {
        let drop = PanelDrop {
            from: "gates".into(),
            row: self.row_of(from),
            at: Some(self.row_of(to)),
        };
        self.send(Command::Panel(PanelCommand {
            package: WIRE.into(),
            event: PanelEvent {
                panel: "circuit".into(),
                control: "gates".into(),
                drop: Some(drop),
                ..PanelEvent::default()
            },
        }))
        .unwrap();
    }
}

#[test]
fn two_switches_through_an_and_gate_light_the_lamp() {
    let mut lab = Lab::new();
    let a = lab.plant(-3.0);
    let b = lab.plant(-1.0);
    let and = lab.plant(1.0);
    let lamp = lab.plant(3.0);
    lab.gate(a, "switch");
    lab.gate(b, "Switch");
    lab.gate(and, "and");
    lab.gate(lamp, "lamp");
    for brick in [a, b, and, lamp] {
        assert_eq!(lab.color(brick), DARK, "a new gate shows it is off");
    }
    // A into the gate by aiming at each end in turn.
    lab.look_at(a);
    lab.run("wire", vec![]);
    lab.look_at(and);
    lab.run("wire", vec![]);
    // B and the lamp by dragging in the circuit window.
    lab.run("circuit", vec![]);
    assert_eq!(lab.rows().len(), 4);
    lab.drag(b, and);
    lab.drag(and, lamp);
    let and_row = lab.rows()[usize::from(lab.row_of(and))].clone();
    assert!(
        and_row.contains(&a.to_string()) && and_row.contains(&b.to_string()),
        "{and_row}"
    );

    lab.click(a);
    assert_eq!(lab.color(a), LIT);
    assert_eq!(lab.color(lamp), DARK, "one switch is not enough for and");
    lab.click(b);
    assert_eq!(lab.color(and), LIT);
    assert_eq!(lab.color(lamp), LIT);
    lab.click(a);
    assert_eq!(lab.color(lamp), DARK);
}
