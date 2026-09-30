//! The Fill Can (`packages/fill-can`) and the engine seams under it:
//! flood-fill painting through shared faces (`paint_fill`,
//! `Simulation::touching_region`) and a held tool in its holder's spray
//! colour (an image's `paint_tint`).
use bri_content::{
    brick::{Brick as Mesh, Face, Quad, Surface, Vertex},
    collision::{CollisionBody, Part},
};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::{Definition, Definitions},
    session::{ActionAim, Command, Fill, Notice, PackageCommand, Reply, Session, ToolAction},
    simulation::Simulation,
};
use bri_world::{Brick, BrickId, OwnerId, World};
use glam::Vec3;
use rapier3d::prelude::*;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

const TOOL: &str = "fill-can-tool:weapon/fill-can";
const IMAGE: &str = "fill-can-tool:image/fill-can";
const WHITE: u8 = 0;
const BLUE: u8 = 1;
const RED: u8 = 2;

/// A 2x1 plate.
fn definitions() -> Definitions {
    let mesh = Mesh {
        schema_version: 1,
        id: "plate".into(),
        footprint_studs: [2, 1],
        height_plates: 1,
        attachment_rows: vec!["bb".into()],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![Quad {
            face: Face::Top,
            surface: Surface::Ramp,
            vertices: [
                [-0.5, 0.1, -0.25],
                [0.5, 0.1, -0.25],
                [0.5, 0.1, 0.25],
                [-0.5, 0.1, 0.25],
            ]
            .map(|position| Vertex {
                position,
                normal: [0.0, 1.0, 0.0],
                uv: [0.0; 2],
            }),
            colors: None,
        }],
    };
    let collision = CollisionBody {
        id: "plate".into(),
        parts: vec![Part::Box {
            center: [0.0; 3],
            size: [1.0, 0.2, 0.5],
        }],
    };
    let shape = bri_physics::content::collider(&collision)
        .unwrap()
        .build()
        .shared_shape()
        .clone();
    Definitions {
        entries: [(
            "plate".to_string(),
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
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages");
    let packages = [
        ("fill-can-tool", "fill-can/fill-can-tool", Side::Shared),
        ("fill-can", "fill-can/fill-can", Side::Server),
    ]
    .into_iter()
    .map(|(id, dir, side)| PackageEntry {
        id: id.into(),
        version: "1.0.0".into(),
        side,
        dir: dir.into(),
        role: None,
    })
    .collect();
    Arc::new(
        Catalog::load(
            &root,
            &PackageSet {
                schema_version: 1,
                packages,
            },
            true,
        )
        .unwrap_or_else(|e| panic!("{e:#?}")),
    )
}

/// The Fill Can's weapons, and a stand-in for the stock colour spray can
/// (base game content) so picking a colour works.
fn tool_pack() -> bri_weapons::Pack {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/fill-can/fill-can-tool/assets/weapons.json");
    let mut pack = bri_weapons::Pack::from_json(&std::fs::read(path).unwrap()).unwrap();
    let mut can = pack.images[IMAGE].clone();
    can.id = "v20.image.bluespraycanimage".into();
    can.name = "BlueSprayCanImage".into();
    can.command = None;
    can.paint_tint = false;
    pack.images.insert(can.id.clone(), can);
    pack
}

struct Game {
    s: Session,
    seq: BTreeMap<OwnerId, u64>,
}
impl Game {
    fn new() -> Self {
        let mut s = Session::new(
            Simulation::new(
                World::new(
                    "Fill".into(),
                    "fill".into(),
                    vec![[1.0; 4], [0.2, 0.4, 1.0, 1.0], [0.9, 0.1, 0.1, 1.0]],
                ),
                definitions(),
                vec![
                    ColliderBuilder::cuboid(100.0, 0.5, 100.0)
                        .translation(Vector::new(0.0, -0.5, 0.0)),
                ],
            )
            .unwrap(),
        );
        s.set_spawn_points(vec![Vec3::new(0.0, 0.05, -6.0)])
            .unwrap();
        s.set_weapon_pack(tool_pack()).unwrap();
        s.install_packages(add_ons(), None).unwrap();
        Self {
            s,
            seq: BTreeMap::new(),
        }
    }
    fn cmd(&mut self, owner: OwnerId, command: Command) -> anyhow::Result<Reply> {
        let n = self.seq.entry(owner).or_default();
        *n += 1;
        self.s.command(owner, *n, command)
    }
    fn plant(&mut self, owner: OwnerId, position: [f32; 3], color: u8) -> BrickId {
        self.steps(121);
        match self.cmd(
            owner,
            Command::Plant {
                definition: "plate".into(),
                position,
                quarter_turns: 0,
                color,
            },
        ) {
            Ok(Reply::Planted(id)) => id,
            other => panic!("plant at {position:?}: {other:?}"),
        }
    }
    fn steps(&mut self, n: usize) {
        for _ in 0..n {
            self.s.step().unwrap();
        }
    }
    fn colors(&self) -> BTreeMap<BrickId, u8> {
        self.s
            .snapshot()
            .world
            .bricks
            .into_iter()
            .map(|(id, b): (BrickId, Brick)| (id, b.color))
            .collect()
    }
    fn undo(&mut self, owner: OwnerId) -> Option<BrickId> {
        match self.cmd(owner, Command::Tool(ToolAction::UndoBrick)) {
            Ok(Reply::Undone(id)) => id,
            other => panic!("undo: {other:?}"),
        }
    }
    fn typed(&mut self, owner: OwnerId, command: &str) {
        self.cmd(
            owner,
            Command::Package(PackageCommand {
                package: String::new(),
                command: command.into(),
                args: vec![],
            }),
        )
        .unwrap_or_else(|e| panic!("/{command}: {e:#}"));
    }
    /// A spray of the Fill Can, aimed along `yaw` and `pitch`.
    fn spray(&mut self, owner: OwnerId, yaw: f32, pitch: f32) {
        self.steps(31);
        let n = self.seq.entry(owner).or_default();
        *n += 1;
        self.s
            .command_with_aim(
                owner,
                *n,
                Command::Package(PackageCommand {
                    package: "fill-can".into(),
                    command: "fill".into(),
                    args: vec![],
                }),
                Some(ActionAim { yaw, pitch }),
            )
            .unwrap();
    }
    fn prints(&mut self, owner: OwnerId) -> Vec<String> {
        self.s
            .take_private_notices()
            .into_iter()
            .filter(|(o, _)| *o == owner)
            .filter_map(|(_, n)| match n {
                Notice::Center { text, .. } | Notice::Bottom { text, .. } => Some(text),
                _ => None,
            })
            .collect()
    }
}

/// On the ground, all red but `d`: `a`, `b` side by side, `c` on `b`, `d`
/// (white) beside `b`, `e` beside only `d`, and `f` meeting `b` only at a
/// corner edge.
fn scene(g: &mut Game, owner: OwnerId) -> [BrickId; 6] {
    let a = g.plant(owner, [0.5, 0.1, 0.25], RED);
    let b = g.plant(owner, [1.5, 0.1, 0.25], RED);
    let c = g.plant(owner, [1.5, 0.3, 0.25], RED);
    let d = g.plant(owner, [2.5, 0.1, 0.25], WHITE);
    let e = g.plant(owner, [3.5, 0.1, 0.25], RED);
    let f = g.plant(owner, [2.5, 0.1, 0.75], RED);
    [a, b, c, d, e, f]
}

/// A player standing clear of the bricks, each on their own spot.
fn player(g: &mut Game, name: &str, admin: bool) -> OwnerId {
    let spot = g.seq.len() as f32 * 1.5;
    let owner =
        g.s.join(name.into(), Vec3::new(0.5 - spot, 0.05, 3.0), admin)
            .unwrap();
    g.seq.insert(owner, 0);
    owner
}

#[test]
fn a_fill_spreads_through_touching_bricks_of_its_colour_only() {
    let mut g = Game::new();
    let host = player(&mut g, "Host", true);
    let [a, b, c, d, e, f] = scene(&mut g, host);
    let fill = g.s.paint_fill(host, a, BLUE, 5000).unwrap();
    assert_eq!(
        fill,
        Fill {
            painted: 3,
            refused: 0
        }
    );
    let colors = g.colors();
    // Beside and on top are reached; another colour stops it, and so
    // does a brick meeting it only along an edge.
    assert_eq!(
        [a, b, c, d, e, f].map(|id| colors[&id]),
        [BLUE, BLUE, BLUE, WHITE, RED, RED]
    );
    // The same colour again changes nothing and says so.
    let again = g.s.paint_fill(host, b, BLUE, 5000).unwrap_err();
    assert!(format!("{again:#}").contains("already"), "{again:#}");
    // One Ctrl+Z takes the whole fill back.
    assert!(g.undo(host).is_some());
    let colors = g.colors();
    assert!([a, b, c, e, f].iter().all(|id| colors[id] == RED));
    assert_eq!(colors[&d], WHITE);
}

#[test]
fn a_fill_over_its_limit_or_off_the_palette_paints_nothing() {
    let mut g = Game::new();
    let host = player(&mut g, "Host", true);
    let [a, ..] = scene(&mut g, host);
    let before = g.colors();
    let over = g.s.paint_fill(host, a, BLUE, 2).unwrap_err();
    assert!(format!("{over:#}").contains("More than 2"), "{over:#}");
    assert!(g.s.paint_fill(host, a, 9, 5000).is_err());
    assert!(g.s.paint_fill(host, a, BLUE, 0).is_err());
    assert_eq!(g.colors(), before);
    // Exactly the limit is fine.
    assert_eq!(g.s.paint_fill(host, a, BLUE, 3).unwrap().painted, 3);
}

#[test]
fn a_fill_flows_around_bricks_it_may_not_paint() {
    let mut g = Game::new();
    let alice = player(&mut g, "Alice", false);
    let bob = player(&mut g, "Bob", false);
    // Alice's two red plates with Bob's red plate between them, and
    // Bob's second red plate beside his first.
    let mine = g.plant(alice, [0.5, 0.1, 0.25], RED);
    let his = g.plant(bob, [1.5, 0.1, 0.25], RED);
    let far = g.plant(alice, [2.5, 0.1, 0.25], RED);
    let beyond = g.plant(bob, [1.5, 0.1, 0.75], RED);
    // Bob's bricks are not Alice's to paint: the fill stops at them, and
    // tells her one was left.
    let fill = g.s.paint_fill(alice, mine, BLUE, 5000).unwrap();
    assert_eq!(
        fill,
        Fill {
            painted: 1,
            refused: 1
        }
    );
    let colors = g.colors();
    assert_eq!(
        [mine, his, far, beyond].map(|id| colors[&id]),
        [BLUE, RED, RED, RED]
    );
    // Starting on Bob's brick is refused outright, as the spray can is.
    let refused = g.s.paint_fill(alice, his, BLUE, 5000).unwrap_err();
    assert!(
        format!("{refused:#}").contains("Bob does not trust you"),
        "{refused:#}"
    );
    assert_eq!(g.colors()[&his], RED);
}

/// The yaw and pitch that look from `eye` at `target`.
fn aim_at(eye: Vec3, target: Vec3) -> (f32, f32) {
    let d = (target - eye).normalize();
    (d.x.atan2(-d.z), d.y.asin())
}

#[test]
fn the_fill_can_shows_and_paints_the_colour_last_picked() {
    let mut g = Game::new();
    let builder = player(&mut g, "Builder", true);
    let [a, b, c, d, ..] = scene(&mut g, builder);
    let painter = player(&mut g, "Painter", true);
    g.steps(60);
    // Blue picked with the paint keys, then /fillcan: the can comes out
    // in blue.
    g.cmd(painter, Command::UseSprayCan { color: BLUE })
        .unwrap();
    g.typed(painter, "fillcan");
    let tools = g.s.tool_inventories()[&painter].clone();
    let slot = tools
        .slots
        .iter()
        .position(|s| s.as_deref() == Some(TOOL))
        .unwrap();
    assert_eq!(tools.selected, Some(slot));
    g.steps(30);
    let held = &g.s.weapon_view().images[&painter];
    assert!(
        held.iter()
            .any(|i| i.image == IMAGE && i.hand == 0 && i.paint == Some(BLUE)),
        "{held:?}"
    );
    // A spray at the red plate fills it and what touches it.
    let feet =
        g.s.snapshot()
            .players
            .iter()
            .find(|p| p.owner == painter)
            .unwrap()
            .feet;
    let eye = Vec3::from(feet) + Vec3::Y * 2.156;
    let (yaw, pitch) = aim_at(eye, Vec3::new(0.5, 0.2, 0.25));
    g.prints(painter);
    g.spray(painter, yaw, pitch);
    let told = g.prints(painter);
    assert!(told.iter().any(|t| t == "Filled 3 bricks"), "{told:?}");
    let colors = g.colors();
    assert_eq!(
        [a, b, c, d].map(|id| colors[&id]),
        [BLUE, BLUE, BLUE, WHITE]
    );
    // Another colour picked: the can comes back out in it.
    g.cmd(painter, Command::UseSprayCan { color: WHITE })
        .unwrap();
    g.cmd(painter, Command::EquipTool { slot: Some(slot) })
        .unwrap();
    g.steps(30);
    let held = &g.s.weapon_view().images[&painter];
    assert!(
        held.iter()
            .any(|i| i.image == IMAGE && i.paint == Some(WHITE)),
        "{held:?}"
    );
}
