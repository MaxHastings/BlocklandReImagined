//! The Grapple Rope Add-On (`packages/showcase/grapple-rope`) played
//! through the authoritative session, and the engine's rope under it
//! (`tether`): the hook flies and bites bricks and the map, the rope
//! holds the player, the wheel climbs it, letting go drops off it, and it
//! breaks when its brick goes.
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::{Definition, Definitions},
    player::MoveInput,
    session::{
        ActionAim, Command, PackageArg, PackageCommand, Reply, Session, ToolAction,
    },
    simulation::Simulation,
};
use bri_world::{OwnerId, World};
use glam::Vec3;
use rapier3d::prelude::*;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

const ROPE: &str = "grapple-rope-tool:weapon/grapplerope";
/// The map's ceiling: a slab whose underside is 20 units up.
const CEILING: f32 = 20.0;

fn showcase() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/showcase")
}

fn definitions() -> Definitions {
    let mesh = Mesh {
        schema_version: 1,
        id: "brick".into(),
        footprint_studs: [2, 2],
        height_plates: 3,
        attachment_rows: vec!["bb".into(); 6],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![],
    };
    let collision = CollisionBody {
        id: "brick".into(),
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
    Definitions {
        entries: [(
            "brick".into(),
            Definition {
                mesh,
                collision,
                shape,
                indestructible: false,
                special: Default::default(),
                reflection: None,
                link: None,
                glass: [0.0; 4],
            },
        )]
        .into(),
    }
}

fn add_ons() -> Arc<Catalog> {
    let packages = [
        ("grapple-rope", Side::Server),
        ("grapple-rope-tool", Side::Shared),
    ]
    .into_iter()
    .map(|(id, side)| PackageEntry {
        id: id.into(),
        version: "1.0.0".into(),
        side,
        dir: id.into(),
        role: None,
    })
    .collect();
    Arc::new(
        Catalog::load(
            &showcase(),
            &PackageSet {
                schema_version: 1,
                packages,
            },
            true,
        )
        .unwrap_or_else(|e| panic!("{e:#?}")),
    )
}

struct Game {
    s: Session,
    seq: BTreeMap<OwnerId, u64>,
    moves: BTreeMap<OwnerId, u64>,
    looks: BTreeMap<OwnerId, MoveInput>,
}
impl Game {
    fn new() -> Self {
        let mut s = Session::new(
            Simulation::new(
                World::new("Jungle".into(), "jungle".into(), vec![[1.0; 4], [0.6; 4]]),
                definitions(),
                vec![
                    ColliderBuilder::cuboid(200.0, 0.5, 200.0)
                        .translation(Vector::new(0.0, -0.5, 0.0)),
                    ColliderBuilder::cuboid(12.0, 0.5, 12.0)
                        .translation(Vector::new(0.0, CEILING + 0.5, -12.0)),
                ],
            )
            .unwrap(),
        );
        s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 0.0)]).unwrap();
        let pack = bri_weapons::Pack::from_json(
            &std::fs::read(showcase().join("grapple-rope-tool/assets/weapons.json")).unwrap(),
        )
        .unwrap();
        s.set_weapon_pack(pack).unwrap();
        s.install_packages(add_ons(), None).unwrap();
        Self {
            s,
            seq: BTreeMap::new(),
            moves: BTreeMap::new(),
            looks: BTreeMap::new(),
        }
    }
    fn join(&mut self, name: &str, at: Vec3) -> OwnerId {
        let owner = self.s.join(name.into(), at, true).unwrap();
        self.looks.insert(owner, MoveInput::default());
        owner
    }
    fn cmd(&mut self, owner: OwnerId, command: Command) -> anyhow::Result<Reply> {
        let n = self.seq.entry(owner).or_default();
        *n += 1;
        self.s.command(owner, *n, command)
    }
    fn package(&mut self, owner: OwnerId, command: &str, args: Vec<PackageArg>) {
        let look = self.looks[&owner];
        let n = self.seq.entry(owner).or_default();
        *n += 1;
        self.s
            .command_with_aim(
                owner,
                *n,
                Command::Package(PackageCommand {
                    package: "grapple-rope".into(),
                    command: command.into(),
                    args,
                }),
                Some(ActionAim {
                    yaw: look.yaw,
                    pitch: look.pitch,
                }),
            )
            .unwrap();
    }
    fn look(&mut self, owner: OwnerId, yaw: f32, pitch: f32) {
        let input = self.looks.get_mut(&owner).unwrap();
        input.yaw = yaw;
        input.pitch = pitch;
    }
    fn walk(&mut self, owner: OwnerId, forward: f32) {
        self.looks.get_mut(&owner).unwrap().forward = forward;
    }
    fn steps(&mut self, n: usize) {
        for _ in 0..n {
            for (owner, input) in &self.looks {
                let m = self.moves.entry(*owner).or_default();
                *m += 1;
                let _ = self.s.movement(*owner, *m, *input);
            }
            self.s.step().unwrap();
        }
    }
    fn feet(&self, owner: OwnerId) -> Vec3 {
        self.s
            .motion_states()
            .into_iter()
            .find(|(p, _)| p.owner == owner)
            .map(|(p, _)| Vec3::from(p.feet))
            .unwrap()
    }
    fn rope(&self, owner: OwnerId) -> Vec<f64> {
        self.s
            .package_state()
            .packages
            .get("grapple-rope")
            .and_then(|ns| ns.players.get(&owner))
            .and_then(|m| m.get("rope"))
            .and_then(|v| v.as_array())
            .map(|a| a.iter().map(|x| x.as_f64().unwrap()).collect())
            .unwrap_or_default()
    }
    /// Left click: hold it down to throw and hang on, let go to drop off.
    fn trigger(&mut self, owner: OwnerId, down: bool) {
        self.cmd(owner, Command::WeaponTrigger { down }).unwrap();
        self.steps(2);
    }
    /// Join with the rope in hand, ready to throw.
    fn roper(&mut self, name: &str, at: Vec3) -> OwnerId {
        let owner = self.join(name, at);
        self.steps(30);
        self.package(owner, "grapplerope", vec![]);
        self.steps(30);
        owner
    }
}

#[test]
fn the_hook_bites_the_map_and_the_rope_holds_and_climbs() {
    let mut g = Game::new();
    let a = g.roper("Tarzan", Vec3::new(0.0, 0.05, 0.0));
    assert_eq!(g.rope(a)[0], 0.0);
    // Aim up at the ceiling ahead and throw.
    g.look(a, 0.0, 1.0);
    g.trigger(a, true);
    let rope = g.rope(a);
    assert_eq!(rope[0], 1.0, "the hook is flying: {rope:?}");
    assert!((rope[2] - f64::from(CEILING)).abs() < 0.05, "at the ceiling: {rope:?}");
    assert!(g.s.tether_of(a).is_none(), "not until it lands");
    g.steps(30);
    assert_eq!(g.rope(a)[0], 2.0);
    let tether = g.s.tether_of(a).expect("hooked");
    assert!((tether.anchor[1] - CEILING).abs() < 0.05);
    let start = tether.length;
    assert!((start - g.rope(a)[4] as f32).abs() < 0.5, "{start} vs {:?}", g.rope(a));
    // Walking away, the rope stops them a rope's length from the anchor.
    g.look(a, std::f32::consts::PI, 0.0);
    g.walk(a, 1.0);
    g.steps(360);
    g.walk(a, 0.0);
    let anchor = Vec3::from(tether.anchor);
    let grip = |g: &Game| g.feet(a) + Vec3::Y * 2.65 * 0.85;
    assert!(grip(&g).distance(anchor) < start + 0.6, "leashed");
    // Rolling the wheel forward climbs: up off the floor.
    g.package(a, "climb", vec![PackageArg::Int(5)]);
    g.package(a, "climb", vec![PackageArg::Int(5)]);
    g.steps(360);
    let climbed = g.s.tether_of(a).unwrap();
    assert!((climbed.target - (start - 25.0).max(2.0)).abs() < 0.01, "{climbed:?}");
    assert!(g.feet(a).y > 3.0, "lifted off the floor: {}", g.feet(a));
    // Letting go drops off the rope.
    g.trigger(a, false);
    g.steps(6);
    assert!(g.s.tether_of(a).is_none());
    assert_eq!(g.rope(a)[0], 0.0);
    g.steps(240);
    assert!(g.feet(a).y < 0.1, "fell to the floor");
}

#[test]
fn a_miss_flies_out_and_back_and_holds_nothing() {
    let mut g = Game::new();
    let a = g.roper("Tarzan", Vec3::new(0.0, 0.05, 30.0));
    // Level, away from the ceiling: nothing within reach.
    g.look(a, std::f32::consts::PI, 0.0);
    g.trigger(a, true);
    assert_eq!(g.rope(a)[0], 3.0);
    g.steps(120);
    assert_eq!(g.rope(a)[0], 0.0, "back in");
    assert!(g.s.tether_of(a).is_none());
    g.trigger(a, false);
}

#[test]
fn the_rope_breaks_when_its_brick_goes_and_when_the_rope_is_put_away() {
    let mut g = Game::new();
    let a = g.roper("Tarzan", Vec3::new(0.0, 0.05, 30.0));
    // A column of bricks 6 ahead; the top one is the target.
    for layer in 0..5 {
        let y = 0.3 + 0.6 * layer as f32;
        let planted = g.cmd(
            a,
            Command::Plant {
                definition: "brick".into(),
                position: [0.0, y, 24.0],
                quarter_turns: 0,
                color: 1,
            },
        );
        assert!(matches!(planted, Ok(Reply::Planted(_))), "{planted:?}");
    }
    g.look(a, 0.0, 0.05);
    g.trigger(a, true);
    g.steps(30);
    assert_eq!(g.rope(a)[0], 2.0, "{:?}", g.rope(a));
    assert!(g.s.tether_of(a).is_some());
    // The brick it bit is removed: the rope breaks, and the rule sees it.
    let undone = g.cmd(a, Command::Tool(ToolAction::UndoBrick));
    assert!(matches!(undone, Ok(Reply::Undone(Some(_)))), "{undone:?}");
    g.steps(12);
    assert!(g.s.tether_of(a).is_none());
    assert_eq!(g.rope(a)[0], 0.0);
    g.trigger(a, false);
    // Hooked again, lower down the column, then the rope put away: off
    // the rope.
    g.look(a, 0.0, -0.05);
    g.trigger(a, true);
    g.steps(30);
    assert!(g.s.tether_of(a).is_some());
    g.cmd(a, Command::EquipTool { slot: None }).unwrap();
    g.steps(12);
    assert!(g.s.tether_of(a).is_none());
    assert_eq!(g.rope(a)[0], 0.0);
}

#[test]
fn everyone_gets_the_rope_outside_minigames() {
    let mut g = Game::new();
    let a = g.join("Jane", Vec3::new(0.0, 0.05, 0.0));
    g.steps(30);
    assert!(
        g.s.tool_inventories()[&a]
            .slots
            .iter()
            .any(|s| s.as_deref() == Some(ROPE))
    );
}
