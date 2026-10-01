//! The engine seams a fill tool is built on, through a small Fill Can of
//! the test's own (`tests/fixtures/fill-can`): flood-fill painting
//! (`paint_fill`, `Simulation::fill_region`) through shared faces or v20's
//! box search, colours and FX, cut at a limit or refused, one undo step,
//! and a held tool in its holder's spray colour (an image's `paint_tint`).
//! The ported Tool_Fill_Can is tested in `bri-addon-import`'s `ports.rs`.
use bri_content::{
    brick::{Brick as Mesh, Face, Quad, Surface, Vertex},
    collision::{CollisionBody, Part},
};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::{
    Catalog,
    ops::{FillPaint, VehiclePaint},
};
use bri_sim::{
    definitions::{Definition, Definitions},
    session::{
        ActionAim, Command, Fill, FillRules, Notice, PackageCommand, Reply, Session, ToolAction,
    },
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
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
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
        .join("tests/fixtures/fill-can/fill-can-tool/assets/weapons.json");
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
        Self::with(|_| {})
    }
    /// A game whose world starts as `build` leaves it.
    fn with(build: impl FnOnce(&mut World)) -> Self {
        let mut world = World::new(
            "Fill".into(),
            "fill".into(),
            vec![[1.0; 4], [0.2, 0.4, 1.0, 1.0], [0.9, 0.1, 0.1, 1.0]],
        );
        build(&mut world);
        let mut s = Session::new(
            Simulation::new(
                world,
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

/// Shared faces only, refusing more than `limit`.
fn faces(limit: usize) -> FillRules {
    FillRules {
        limit,
        reach: None,
        stop_at_limit: false,
    }
}
fn color(c: u8) -> FillPaint {
    FillPaint::Color(c)
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
    let fill = g.s.paint_fill(host, a, color(BLUE), faces(5000)).unwrap();
    assert_eq!(
        fill,
        Fill {
            painted: 3,
            refused: 0,
            stopped: false
        }
    );
    let colors = g.colors();
    // Beside and on top are reached; another colour stops it, and so
    // does a brick meeting it only along an edge.
    assert_eq!(
        [a, b, c, d, e, f].map(|id| colors[&id]),
        [BLUE, BLUE, BLUE, WHITE, RED, RED]
    );
    // The same colour again changes nothing.
    let again = g.s.paint_fill(host, b, color(BLUE), faces(5000)).unwrap();
    assert_eq!(again, Fill::default());
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
    let over = g.s.paint_fill(host, a, color(BLUE), faces(2)).unwrap_err();
    assert!(format!("{over:#}").contains("More than 2"), "{over:#}");
    assert!(g.s.paint_fill(host, a, color(9), faces(5000)).is_err());
    assert!(g.s.paint_fill(host, a, color(BLUE), faces(0)).is_err());
    assert!(
        g.s.paint_fill(host, a, FillPaint::ColorEffect(7), faces(9))
            .is_err()
    );
    assert_eq!(g.colors(), before);
    // Exactly the limit is fine.
    assert_eq!(
        g.s.paint_fill(host, a, color(BLUE), faces(3))
            .unwrap()
            .painted,
        3
    );
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
    let fill =
        g.s.paint_fill(alice, mine, color(BLUE), faces(5000))
            .unwrap();
    assert_eq!(
        fill,
        Fill {
            painted: 1,
            refused: 1,
            stopped: false
        }
    );
    let colors = g.colors();
    assert_eq!(
        [mine, his, far, beyond].map(|id| colors[&id]),
        [BLUE, RED, RED, RED]
    );
    // Starting on Bob's brick is refused outright, as the spray can is.
    let refused =
        g.s.paint_fill(alice, his, color(BLUE), faces(5000))
            .unwrap_err();
    assert!(
        format!("{refused:#}").contains("Bob does not trust you"),
        "{refused:#}"
    );
    assert_eq!(g.colors()[&his], RED);
}

/// v20's box search: each brick's box grown by 0.3 sideways and 0.15 up
/// and down reaches `f`, which meets `b` only along an edge, through it
/// `e` past the white `d`, but not a brick a stud away; a limit stops the fill after that many, in the
/// order it reached them.
#[test]
fn with_reach_a_fill_finds_what_v20s_box_search_found_and_can_stop_at_its_limit() {
    let mut g = Game::new();
    let host = player(&mut g, "Host", true);
    let [a, b, c, d, e, f] = scene(&mut g, host);
    let gap = g.plant(host, [-1.0, 0.1, 0.25], RED);
    let rules = |limit| FillRules {
        limit,
        reach: Some([0.3, 0.15]),
        stop_at_limit: true,
    };
    // A stud's gap (0.5) is out of reach.
    let fill = g.s.paint_fill(host, a, color(BLUE), rules(5000)).unwrap();
    assert_eq!(fill.painted, 5);
    assert!(!fill.stopped);
    let colors = g.colors();
    assert_eq!(
        [a, b, c, d, e, f, gap].map(|id| colors[&id]),
        [BLUE, BLUE, BLUE, WHITE, BLUE, BLUE, RED]
    );
    assert!(g.undo(host).is_some());
    // At its limit the fill stops: what it reached first is painted.
    let fill = g.s.paint_fill(host, a, color(BLUE), rules(2)).unwrap();
    assert_eq!(
        fill,
        Fill {
            painted: 2,
            refused: 0,
            stopped: true
        }
    );
    assert_eq!(g.colors().values().filter(|c| **c == BLUE).count(), 2);
}

/// An FX fill gives the colour's bricks the effect and leaves their colour;
/// undo puts back the effect each had, but not on a brick changed since.
#[test]
fn effect_fills_undo_only_what_is_still_as_the_fill_left_it() {
    let mut g = Game::new();
    let host = player(&mut g, "Host", true);
    let [a, b, c, ..] = scene(&mut g, host);
    let fill =
        g.s.paint_fill(host, a, FillPaint::ColorEffect(3), faces(50))
            .unwrap();
    assert_eq!(fill.painted, 3);
    let fx = |g: &Game, id: BrickId| g.s.snapshot().world.bricks[&id].color_effect;
    assert_eq!([a, b, c].map(|id| fx(&g, id)), [3, 3, 3]);
    assert!(g.colors().values().all(|c| *c != BLUE));
    // `c` is given another effect before the undo.
    g.s.edit_brick(host, c, bri_world::authority::Edit::ColorEffect(5))
        .unwrap();
    assert!(g.undo(host).is_some());
    assert_eq!([a, b, c].map(|id| fx(&g, id)), [0, 0, 5]);
    let fill =
        g.s.paint_fill(host, a, FillPaint::ShapeEffect(1), faces(50))
            .unwrap();
    assert_eq!(fill.painted, 3);
    assert_eq!(g.s.snapshot().world.bricks[&b].shape_effect, 1);
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

/// The stand-in plane (crates/vehicles/tests/fixtures), as the vehicle a
/// spawn brick makes.
const PLANE: &str = "test_plane:vehicle/standinplane";

/// `paint_vehicle`: a vehicle its spawn brick recolours is painted through
/// the brick, any other on its own; one undo step puts it back, and the
/// spawn brick's build must trust the painter fully.
#[test]
fn a_vehicle_is_painted_through_its_recolouring_brick_or_alone_and_undone() {
    // Two plane spawns, red, of a build no one here owns: one recolours
    // its plane.
    let spawn = |x: f32, recolor: bool| {
        let mut brick = Brick::new(
            bri_world::ContentRef::Resolved("plate".into()),
            [x, 0.1, -30.25],
            4242,
        );
        brick.color = RED;
        brick.vehicle = Some(Box::new(bri_world::VehicleSpawn {
            vehicle: bri_world::ContentRef::Resolved(PLANE.into()),
            recolor,
        }));
        brick
    };
    let mut g = Game::with(|w| {
        w.bricks.insert(1, spawn(-20.5, true));
        w.bricks.insert(2, spawn(20.5, false));
        w.next_brick_id = 3;
    });
    let pack = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../vehicles/tests/fixtures/stand-in-plane/assets/vehicles.json");
    g.s.set_vehicle_pack(bri_vehicles::Pack::load(pack).unwrap(), Vec::new())
        .unwrap();
    let admin = player(&mut g, "Admin", true);
    let guest = player(&mut g, "Guest", false);
    g.steps(121);
    let infos = g.s.vehicle_infos();
    assert_eq!(infos.len(), 2, "{infos:?}");
    let palette = g.s.simulation().state().palette.clone();
    let opaque = |c: u8| {
        let [r, g, b, _] = palette[usize::from(c)];
        [r, g, b, 1.0]
    };
    let color_of = |g: &Game, id: u64| {
        g.s.vehicle_infos()
            .into_iter()
            .find(|v| v.id == id)
            .unwrap()
            .color
    };
    let recolored = infos.iter().find(|v| v.color.is_some()).unwrap().id;
    let plain = infos.iter().find(|v| v.color.is_none()).unwrap().id;
    assert_eq!(color_of(&g, recolored), Some(opaque(RED)));

    // A guest the build does not trust paints nothing.
    let refused = g.s.paint_vehicle(guest, plain, VehiclePaint::Color(BLUE));
    assert_eq!(
        refused.unwrap_err().to_string(),
        "BL_ID: 4242 does not trust you enough to do that."
    );
    assert_eq!(color_of(&g, plain), None);

    // The recoloured plane is painted through its brick.
    let blue =
        g.s.paint_vehicle(admin, recolored, VehiclePaint::Color(BLUE))
            .unwrap();
    assert_eq!(blue, opaque(BLUE));
    assert_eq!(color_of(&g, recolored), Some(opaque(BLUE)));
    assert_eq!(g.colors()[&1], BLUE);
    // The other alone, in any colour; its brick stays red.
    let rgb = [0.25, 0.5, 0.75];
    g.s.paint_vehicle(admin, plain, VehiclePaint::Rgb(rgb))
        .unwrap();
    assert_eq!(color_of(&g, plain), Some([0.25, 0.5, 0.75, 1.0]));
    assert_eq!(g.colors()[&2], RED);
    // An FX can's colour on the recoloured plane leaves its brick alone.
    g.s.paint_vehicle(admin, recolored, VehiclePaint::Rgb(rgb))
        .unwrap();
    assert_eq!(color_of(&g, recolored), Some([0.25, 0.5, 0.75, 1.0]));
    assert_eq!(g.colors()[&1], BLUE);

    // Each Ctrl+Z takes one paint back, latest first.
    g.undo(admin);
    assert_eq!(color_of(&g, recolored), Some(opaque(BLUE)));
    g.undo(admin);
    assert_eq!(color_of(&g, plain), None);
    assert_eq!(g.undo(admin), Some(1));
    assert_eq!(color_of(&g, recolored), Some(opaque(RED)));
    assert_eq!(g.colors()[&1], RED);
}
