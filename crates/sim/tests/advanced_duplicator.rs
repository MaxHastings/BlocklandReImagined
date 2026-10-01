//! A stand-in Advanced Duplicator (`tests/fixtures/duplicators`, our own
//! rule and tool, never shipped) and the engine
//! seams under it: copying a box (`copy_box`), mirroring a copy as it is
//! placed (`mirror_copy`, `PlaceBlueprint::mirrored`), cutting a copy's
//! originals away with one undo that puts them back (`cut_copy`), painting
//! them (`paint_copy`), and a player's selection box (`show_box`).
use bri_content::{
    brick::{Brick as Mesh, Face, Quad, Surface, Vertex},
    collision::{CollisionBody, Part},
};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::{
    Catalog,
    ops::{CopyRule, MirrorAxis, StackReach},
};
use bri_sim::{
    definitions::{Definition, Definitions},
    player::MoveInput,
    session::{ActionAim, Command, Notice, PackageCommand, Reply, Session, ToolAction},
    simulation::Simulation,
};
use bri_world::{Brick, BrickId, ContentRef, OwnerId, World};
use glam::Vec3;
use rapier3d::prelude::*;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

const TOOL: &str = "advanced-duplicator-tool:weapon/advanced-duplicator";

/// A box-shaped brick `w` by `d` studs and `h` plates, drawn as one quad
/// through `top` (its own frame).
fn definition(w: u32, d: u32, h: u32, top: [[f32; 3]; 4]) -> Definition {
    let mesh = Mesh {
        schema_version: 1,
        id: format!("{w}x{d}x{h}"),
        footprint_studs: [w, d],
        height_plates: h,
        attachment_rows: vec!["b".repeat(w as usize); (d * h) as usize],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![Quad {
            face: Face::Top,
            surface: Surface::Ramp,
            vertices: top.map(|position| Vertex {
                position,
                normal: [0.0, 1.0, 0.0],
                uv: [0.0; 2],
            }),
            colors: None,
        }],
    };
    let size = [w as f32 * 0.5, h as f32 * 0.2, d as f32 * 0.5];
    let collision = CollisionBody {
        id: format!("{w}x{d}x{h}"),
        parts: vec![Part::Box {
            center: [0.0; 3],
            size,
        }],
    };
    let shape = bri_physics::content::collider(&collision)
        .unwrap()
        .build()
        .shared_shape()
        .clone();
    Definition {
        mesh,
        collision,
        shape,
        indestructible: false,
        special: Default::default(),
        reflection: None,
        link: None,
        glass: [0.0; 4],
    }
}

/// A plain `w` by `d` plate, as v20's `BRICK` geometry is: what a
/// supercut puts back over what stuck out of its box.
fn plain(w: u32, d: u32) -> Definition {
    let (x, z) = (w as f32 * 0.25, d as f32 * 0.25);
    let mut plain = definition(w, d, 1, [[-x, 0.1, -z], [x, 0.1, -z], [x, 0.1, z], [-x, 0.1, z]]);
    plain.mesh.quads[0].surface = Surface::Top;
    plain.mesh.collision_boxes = vec![bri_content::brick::CollisionBox {
        center: [0.0; 3],
        size: [w as f32 * 0.5, 0.2, d as f32 * 0.5],
    }];
    plain
}

/// A 2x1 plate, a 2x1 wedge with its mirror twin (its top slopes three
/// ways, so no turn of it is its own reflection), and plain 2x1 and 4x1
/// plates.
fn definitions() -> Definitions {
    let plate = definition(
        2,
        1,
        1,
        [
            [-0.5, 0.1, -0.25],
            [0.5, 0.1, -0.25],
            [0.5, 0.1, 0.25],
            [-0.5, 0.1, 0.25],
        ],
    );
    let right = definition(
        2,
        1,
        3,
        [
            [-0.5, -0.3, -0.25],
            [0.5, 0.1, -0.25],
            [0.5, 0.3, 0.25],
            [-0.5, 0.1, 0.25],
        ],
    );
    let mut left = right.clone();
    for vertex in &mut left.mesh.quads[0].vertices {
        vertex.position[0] = -vertex.position[0];
    }
    Definitions {
        entries: [
            ("plate".to_string(), plate),
            ("wedge-left".to_string(), left),
            ("wedge-right".to_string(), right),
            ("plain-2x1".to_string(), plain(2, 1)),
            ("plain-4x1".to_string(), plain(4, 1)),
        ]
        .into(),
    }
}

/// Both Duplicators, as a server with both turned on runs them.
fn add_ons() -> Arc<Catalog> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/duplicators");
    let packages = [
        ("duplicator", "duplicator", Side::Server),
        (
            "duplicator-tool",
            "duplicator-tool",
            Side::Shared,
        ),
        (
            "advanced-duplicator-tool",
            "advanced-duplicator-tool",
            Side::Shared,
        ),
        (
            "advanced-duplicator",
            "advanced-duplicator",
            Side::Server,
        ),
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

fn tool_pack() -> bri_weapons::Pack {
    let read = |path: &str| {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path);
        bri_weapons::Pack::from_json(&std::fs::read(path).unwrap()).unwrap()
    };
    let mut pack = read("tests/fixtures/duplicators/duplicator-tool/assets/weapons.json");
    let advanced =
        read("tests/fixtures/duplicators/advanced-duplicator-tool/assets/weapons.json");
    pack.items.extend(advanced.items);
    pack.images.extend(advanced.images);
    pack
}

/// A flat floor, three brick definitions, both Duplicators running.
struct Game {
    s: Session,
    seq: BTreeMap<OwnerId, u64>,
}
impl Game {
    fn new() -> Self {
        let mut s = Session::new(
            Simulation::new(
                World::new(
                    "Dup".into(),
                    "dup".into(),
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
        s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 0.0)]).unwrap();
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
    fn plant_as(
        &mut self,
        owner: OwnerId,
        definition: &str,
        position: [f32; 3],
        color: u8,
    ) -> BrickId {
        self.steps(121);
        match self.cmd(
            owner,
            Command::Plant {
                definition: definition.into(),
                position,
                quarter_turns: 0,
                color,
            },
        ) {
            Ok(Reply::Planted(id)) => id,
            other => panic!("plant {definition} at {position:?}: {other:?}"),
        }
    }
    fn plant(&mut self, owner: OwnerId, position: [f32; 3]) -> BrickId {
        self.plant_as(owner, "plate", position, 0)
    }
    fn steps(&mut self, n: usize) {
        for _ in 0..n {
            self.s.step().unwrap();
        }
    }
    fn bricks(&self) -> BTreeMap<BrickId, Brick> {
        self.s.snapshot().world.bricks.into_iter().collect()
    }
    fn place(
        &mut self,
        owner: OwnerId,
        position: [f32; 3],
        turns: u8,
        mirrored: bool,
    ) -> anyhow::Result<Reply> {
        self.steps(121);
        self.cmd(
            owner,
            Command::PlaceBlueprint {
                position,
                quarter_turns: turns,
                mirrored,
                flipped: false,
            },
        )
    }
    fn undo(&mut self, owner: OwnerId) -> Option<BrickId> {
        match self.cmd(owner, Command::Tool(ToolAction::UndoBrick)) {
            Ok(Reply::Undone(id)) => id,
            other => panic!("undo: {other:?}"),
        }
    }
    /// A chat command, typed.
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
    /// A swing of the Advanced Duplicator, aimed along `yaw` and `pitch`.
    fn click(&mut self, owner: OwnerId, yaw: f32, pitch: f32) {
        self.steps(13);
        let n = self.seq.entry(owner).or_default();
        *n += 1;
        self.s
            .command_with_aim(
                owner,
                *n,
                Command::Package(PackageCommand {
                    package: "advanced-duplicator".into(),
                    command: "click".into(),
                    args: vec![],
                }),
                Some(ActionAim { yaw, pitch }),
            )
            .unwrap();
    }
    fn notices(&mut self, owner: OwnerId) -> Vec<Notice> {
        self.s
            .take_private_notices()
            .into_iter()
            .filter(|(o, _)| *o == owner)
            .map(|(_, n)| n)
            .collect()
    }
}

fn prints(notices: &[Notice]) -> Vec<String> {
    notices
        .iter()
        .filter_map(|n| match n {
            Notice::Center { text, .. } | Notice::Bottom { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn host(g: &mut Game) -> OwnerId {
    g.s.join("Host".into(), Vec3::new(0.0, 0.05, 3.0), true)
        .unwrap()
}

/// A two-plate tower (A, B on A), a plate C beside it and a lone plate D
/// further off.
fn scene(g: &mut Game, owner: OwnerId) -> [BrickId; 4] {
    let a = g.plant(owner, [0.5, 0.1, 0.25]);
    let b = g.plant(owner, [0.5, 0.3, 0.25]);
    let c = g.plant(owner, [-1.0, 0.1, -0.25]);
    let d = g.plant(owner, [4.5, 0.1, 0.25]);
    [a, b, c, d]
}

/// `copy_box` as the Advanced Duplicator asks: the bricks copied, or why
/// none were.
fn copy_box(
    g: &mut Game,
    owner: OwnerId,
    min: [f32; 3],
    max: [f32; 3],
    limit: usize,
) -> Result<usize, String> {
    let copied = g.s.copy_box(
        owner,
        min,
        max,
        true,
        limit,
        CopyRule::default(),
        TOOL,
        "advanced-duplicator",
    );
    match copied.error {
        Some((_, message)) => Err(message),
        None => Ok(copied.selection.bricks.len()),
    }
}

#[test]
fn a_box_copies_what_lies_wholly_inside_it() {
    let mut g = Game::new();
    let host = host(&mut g);
    scene(&mut g, host);
    // The tower and its neighbour, not the lone plate.
    assert_eq!(
        copy_box(&mut g, host, [-1.5, 0.0, -0.5], [1.0, 0.4, 0.5], 100).unwrap(),
        3
    );
    let copy = g.s.blueprint(host).unwrap().clone();
    assert_eq!(copy.bricks.len(), 3);
    assert_eq!(copy.size, [5, 2, 2]);
    // A box grows out to the grid: 0.3 high still takes the upper plate...
    assert_eq!(
        copy_box(&mut g, host, [-1.5, 0.0, -0.5], [1.0, 0.3, 0.5], 100).unwrap(),
        3
    );
    // ...and one a plate high only the bottom layer.
    assert_eq!(
        copy_box(&mut g, host, [-1.5, 0.0, -0.5], [1.0, 0.2, 0.5], 100).unwrap(),
        2
    );
    // A brick half in the box stays out.
    assert_eq!(
        copy_box(&mut g, host, [0.0, 0.0, -0.5], [1.0, 0.4, 0.5], 100).unwrap(),
        2
    );
    // ...unless the box is not limited to what lies wholly inside it: the
    // neighbour reaching into this one comes too.
    assert_eq!(
        copy_box(&mut g, host, [-0.75, 0.0, -0.5], [1.0, 0.4, 0.5], 100).unwrap(),
        2
    );
    let copied = g.s.copy_box(
        host,
        [-0.75, 0.0, -0.5],
        [1.0, 0.4, 0.5],
        false,
        100,
        CopyRule::default(),
        TOOL,
        "advanced-duplicator",
    );
    assert_eq!(copied.selection.bricks.len(), 3);
    // Too many is cut short, lowest first, and says so.
    let copied = g.s.copy_box(
        host,
        [-1.5, 0.0, -0.5],
        [1.0, 0.4, 0.5],
        true,
        2,
        CopyRule::default(),
        TOOL,
        "advanced-duplicator",
    );
    assert_eq!(copied.selection.bricks.len(), 2);
    assert!(copied.selection.limit_reached);
    assert_eq!(g.s.blueprint(host).unwrap().bricks.len(), 2);
    // An empty box, and one too big.
    let error = copy_box(&mut g, host, [10.0, 0.0, 10.0], [12.0, 1.0, 12.0], 100).unwrap_err();
    assert!(format!("{error:#}").contains("no bricks"), "{error:#}");
    assert!(copy_box(&mut g, host, [-600.0, 0.0, 0.0], [600.0, 1.0, 1.0], 100).is_err());
    // Nobody copies bricks whose owner does not trust them.
    let guest =
        g.s.join("Guest".into(), Vec3::new(2.0, 0.05, 3.0), false)
            .unwrap();
    let error = copy_box(&mut g, guest, [-1.5, 0.0, -0.5], [1.0, 0.4, 0.5], 100).unwrap_err();
    assert!(format!("{error:#}").contains("trust"), "{error:#}");
    assert!(g.s.blueprint(guest).is_none());
}

#[test]
fn a_mirrored_copy_plants_the_reflection_with_twins_swapped() {
    let mut g = Game::new();
    let host = host(&mut g);
    // A right wedge with a plate on its high end.
    g.plant_as(host, "wedge-right", [0.5, 0.3, 0.25], 1);
    let wedge = g
        .bricks()
        .into_iter()
        .find(|(_, b)| b.definition == ContentRef::Resolved("wedge-right".into()))
        .unwrap()
        .0;
    g.plant_as(host, "plate", [1.0, 0.7, 0.25], 2);
    let copied = g.s.copy_build(
        host,
        wedge,
        100,
        StackReach {
            up: true,
            limited: true,
        },
        CopyRule::default(),
        "advanced-duplicator-tool:weapon/advanced-duplicator",
        "advanced-duplicator",
    );
    assert!(copied.error.is_none());
    // Mirroring is part of the placement: the host tells the player.
    g.notices(host);
    g.s.mirror_copy(host, MirrorAxis::X).unwrap();
    assert!(
        g.notices(host)
            .iter()
            .any(|n| matches!(n, Notice::MirrorCopy { across_z: false }))
    );
    let before = g.bricks().len();
    let Ok(Reply::Planted(_)) = g.place(host, [-3.0, 0.0, 0.0], 0, true) else {
        panic!("the mirrored copy plants")
    };
    let placed: Vec<Brick> = g
        .bricks()
        .into_values()
        .filter(|b| b.position[0] < -1.0)
        .collect();
    assert_eq!(placed.len(), 2);
    assert_eq!(g.bricks().len(), before + 2);
    let find = |id: &str| {
        placed
            .iter()
            .find(|b| b.definition == ContentRef::Resolved(id.into()))
            .unwrap_or_else(|| panic!("no {id}"))
            .clone()
    };
    // The wedge became its twin, and the plate on its high end moved to
    // the other side: west of the wedge's middle now.
    let left = find("wedge-left");
    let plate = find("plate");
    assert_eq!(left.color, 1);
    assert!(plate.position[0] < left.position[0], "{plate:?} {left:?}");
    // Unmirrored, the same copy plants as it was.
    let Ok(Reply::Planted(_)) = g.place(host, [-3.0, 0.0, -3.0], 0, false) else {
        panic!("the copy plants")
    };
    assert!(g.bricks().values().any(|b| {
        b.definition == ContentRef::Resolved("wedge-right".into()) && b.position[2] < -2.0
    }));
    // Mirroring with nothing copied tells the player.
    let guest =
        g.s.join("Guest".into(), Vec3::new(2.0, 0.05, 3.0), false)
            .unwrap();
    assert!(g.s.mirror_copy(guest, MirrorAxis::View).is_err());
}

#[test]
fn a_cut_moves_a_build_and_undo_puts_it_back_as_it_was() {
    let mut g = Game::new();
    let host = host(&mut g);
    let [a, b, c, d] = scene(&mut g, host);
    let world = g.bricks();
    let original: Vec<Brick> = [a, b, c].iter().map(|id| world[id].clone()).collect();
    copy_box(&mut g, host, [-1.5, 0.0, -0.5], [1.0, 0.4, 0.5], 100).unwrap();
    assert_eq!(g.s.cut_copy(host).unwrap(), 3);
    let world = g.bricks();
    assert_eq!(world.len(), 1);
    assert!(world.contains_key(&d));
    // The copy is still in hand: planting it elsewhere moved the build.
    assert!(g.s.blueprint(host).is_some());
    let Ok(Reply::Planted(_)) = g.place(host, [-4.0, 0.0, -3.0], 0, false) else {
        panic!("the moved build plants")
    };
    assert_eq!(g.bricks().len(), 4);
    // Undo takes the planted copy back, then the cut: every brick where it
    // was, as it was.
    g.undo(host);
    assert_eq!(g.bricks().len(), 1);
    g.notices(host);
    let restored = g.undo(host);
    assert!(restored.is_some(), "{:?}", prints(&g.notices(host)));
    let mut back: Vec<Brick> = g
        .bricks()
        .into_iter()
        .filter(|(id, _)| *id != d)
        .map(|(_, b)| b)
        .collect();
    back.sort_by(|x, y| x.position.partial_cmp(&y.position).unwrap());
    let mut expected = original.clone();
    expected.sort_by(|x, y| x.position.partial_cmp(&y.position).unwrap());
    assert_eq!(back, expected);
    // A cut whose undo finds something in the way waits until it is clear.
    copy_box(&mut g, host, [-1.5, 0.0, -0.5], [1.0, 0.4, 0.5], 100).unwrap();
    g.s.cut_copy(host).unwrap();
    g.s.take_private_notices();
    let guest =
        g.s.join("Guest".into(), Vec3::new(2.0, 0.05, 3.0), false)
            .unwrap();
    g.plant(guest, [-1.0, 0.1, -0.25]);
    assert!(g.undo(host).is_none());
    assert!(
        prints(&g.notices(host))
            .iter()
            .any(|t| t.contains("in the way")),
    );
    assert_eq!(g.bricks().len(), 2);
    g.undo(guest);
    assert!(g.undo(host).is_some());
    assert_eq!(g.bricks().len(), 4);
}

#[test]
fn cuts_and_fills_need_full_trust_and_undo_as_one_step() {
    let mut g = Game::new();
    let host = host(&mut g);
    let [a, b, c, _] = scene(&mut g, host);
    let guest =
        g.s.join("Guest".into(), Vec3::new(2.0, 0.05, 3.0), false)
            .unwrap();
    // The guest's own plate copies; they may not cut or paint anything
    // they only built on.
    let own = g.plant(guest, [2.5, 0.1, -2.25]);
    let up = StackReach {
        up: true,
        limited: true,
    };
    let copied = g.s.copy_build(
        guest,
        own,
        100,
        up,
        CopyRule::default(),
        TOOL,
        "advanced-duplicator",
    );
    assert!(copied.error.is_none());
    assert!(g.s.cut_copy(host).is_err(), "the host holds no copy");
    assert_eq!(g.s.paint_copy(guest, 2).unwrap(), 1);
    assert_eq!(g.bricks()[&own].color, 2);
    // The host's copy: a fill paints all three at once...
    copy_box(&mut g, host, [-1.5, 0.0, -0.5], [1.0, 0.4, 0.5], 100).unwrap();
    assert_eq!(g.s.paint_copy(host, 1).unwrap(), 3);
    let world = g.bricks();
    assert!([a, b, c].iter().all(|id| world[id].color == 1));
    // ...a colour off the palette is refused...
    assert!(g.s.paint_copy(host, 9).is_err());
    // ...and one undo takes it all back.
    assert!(g.undo(host).is_some());
    let world = g.bricks();
    assert!([a, b, c].iter().all(|id| world[id].color == 0));
    // Bricks the guest has no full trust on: nothing is cut.
    let guest_copy_of_host = copy_box(&mut g, guest, [-1.5, 0.0, -0.5], [1.0, 0.4, 0.5], 100);
    assert!(guest_copy_of_host.is_err());
    assert_eq!(g.bricks().len(), 5);
}

#[test]
fn a_selection_box_is_outlined_on_the_grid_for_its_player() {
    let mut g = Game::new();
    let host = host(&mut g);
    g.notices(host);
    g.s.show_box(host, Some(([0.1, 0.05, -0.3], [1.2, 0.3, 0.4])), TOOL)
        .unwrap();
    let outline = g.notices(host).into_iter().find_map(|n| match n {
        Notice::SelectionBox(Some(outline)) => Some(outline),
        _ => None,
    });
    let outline = outline.expect("the box is outlined");
    assert_eq!(outline.tool, TOOL);
    let near = |a: [f32; 3], b: [f32; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-5);
    assert!(near(outline.min, [0.0, 0.0, -0.5]), "{outline:?}");
    assert!(near(outline.max, [1.5, 0.4, 0.5]), "{outline:?}");
    outline.validate().unwrap();
    g.s.show_box(host, None, "").unwrap();
    assert!(
        g.notices(host)
            .iter()
            .any(|n| matches!(n, Notice::SelectionBox(None)))
    );
}

/// The yaw and pitch that look from `eye` at `target`.
fn aim_at(eye: Vec3, target: Vec3) -> (f32, f32) {
    let d = (target - eye).normalize();
    (d.x.atan2(-d.z), d.y.asin())
}

#[test]
fn the_advanced_duplicator_copies_a_box_between_two_clicks() {
    let mut g = Game::new();
    let builder = host(&mut g);
    // The scene in red, for the fill to change.
    let a = g.plant_as(builder, "plate", [0.5, 0.1, 0.25], 2);
    let b = g.plant_as(builder, "plate", [0.5, 0.3, 0.25], 2);
    let c = g.plant_as(builder, "plate", [-1.0, 0.1, -0.25], 2);
    let d = g.plant_as(builder, "plate", [4.5, 0.1, 0.25], 2);
    // Standing on the tower.
    let player =
        g.s.join("Player".into(), Vec3::new(0.5, 0.45, 0.25), true)
            .unwrap();
    for n in 0..30u64 {
        g.s.movement(
            player,
            1000 + n,
            MoveInput {
                pitch: -1.55,
                ..Default::default()
            },
        )
        .unwrap();
        g.s.step().unwrap();
    }
    // /adup: the gold wand in hand; the classic /dup is still its own.
    g.typed(player, "adup");
    let tools = g.s.tool_inventories()[&player].clone();
    let slot = tools
        .slots
        .iter()
        .position(|s| s.as_deref() == Some(TOOL))
        .unwrap();
    assert_eq!(tools.selected, Some(slot));
    g.typed(player, "dup");
    assert!(
        g.s.tool_inventories()[&player]
            .slots
            .iter()
            .any(|s| s.as_deref() == Some("duplicator-tool:weapon/duplicator"))
    );
    g.typed(player, "adup");
    // Stack mode: a click on the tower's top copies it alone (nothing on it).
    g.notices(player);
    g.click(player, 0.0, -1.55);
    assert_eq!(g.s.blueprint(player).unwrap().bricks.len(), 1);
    // Box mode: a floor cell past the neighbour plate, then the tower top.
    g.typed(player, "box");
    let feet = Vec3::new(0.5, 0.4, 0.25);
    let eye = feet + Vec3::Y * 2.156;
    let (yaw, pitch) = aim_at(eye, Vec3::new(-1.75, 0.0, -1.25));
    g.notices(player);
    g.click(player, yaw, pitch);
    let told = g.notices(player);
    assert!(
        told.iter()
            .any(|n| matches!(n, Notice::SelectionBox(Some(o)) if o.tool == TOOL)),
        "{told:?}"
    );
    assert!(prints(&told).iter().any(|t| t.contains("opposite corner")));
    g.click(player, 0.0, -1.55);
    let told = g.notices(player);
    assert!(
        prints(&told).iter().any(|t| t == "Copied 3 bricks"),
        "{told:?}"
    );
    let copy = g.s.blueprint(player).unwrap().clone();
    assert_eq!(copy.bricks.len(), 3);
    // /mirror turns the copy over left to right as the player faces.
    g.typed(player, "mirror");
    assert!(
        g.notices(player)
            .iter()
            .any(|n| matches!(n, Notice::MirrorCopy { .. }))
    );
    // /fillcolor paints the originals in the spray colour (white, the
    // first, until a can is picked); /cut takes them away (the player
    // stands on the tower, and falls).
    g.typed(player, "fillcolor");
    let world = g.bricks();
    assert!([a, b, c].iter().all(|id| world[id].color == 0));
    assert_eq!(world[&d].color, 2);
    g.typed(player, "cut");
    let world = g.bricks();
    assert_eq!(world.len(), 1);
    assert!(world.contains_key(&d));
    // Ctrl+Z puts them back, then unpaints them.
    assert!(g.undo(player).is_some());
    assert_eq!(g.bricks().len(), 4);
    assert!(g.undo(player).is_some());
    assert!(g.bricks().values().all(|b| b.color == 2));
    // /box again goes back to stack mode and takes the outline away.
    g.notices(player);
    g.typed(player, "box");
    assert!(
        g.notices(player)
            .iter()
            .any(|n| matches!(n, Notice::SelectionBox(None)))
    );
}

/// A field of `w` by `d` 2x1 plates on the floor, from (0, -6).
fn field(g: &mut Game, owner: OwnerId, w: usize, d: usize) -> Vec<BrickId> {
    let mut ids = Vec::new();
    for i in 0..w {
        for j in 0..d {
            ids.push(g.plant(owner, [0.5 + i as f32, 0.1, -5.75 + j as f32 * 0.5]));
        }
    }
    ids
}

/// Step until `owner`'s copy work is done; how many ticks it took.
fn finish_work(g: &mut Game, owner: OwnerId) -> usize {
    let mut ticks = 0;
    while g.s.copy_working(owner) {
        g.steps(1);
        ticks += 1;
        assert!(ticks < 10_000, "the copy work never finished");
    }
    ticks
}

#[test]
fn a_big_copy_plants_and_undoes_a_slice_each_tick() {
    let mut g = Game::new();
    let host = host(&mut g);
    field(&mut g, host, 4, 6);
    assert_eq!(
        copy_box(&mut g, host, [0.0, 0.0, -6.0], [4.0, 0.2, -3.0], 100).unwrap(),
        24
    );
    // About a brick's planting a tick.
    g.s.set_copy_work(30);
    g.steps(121);
    let reply = g.cmd(
        host,
        Command::PlaceBlueprint {
            position: [-3.0, 0.0, 0.0],
            quarter_turns: 0,
            mirrored: false,
            flipped: false,
        },
    );
    assert!(matches!(reply, Ok(Reply::Accepted)), "{reply:?}");
    assert!(g.s.copy_working(host));
    // One job at a time: another plant or an undo waits.
    let busy = g
        .cmd(
            host,
            Command::PlaceBlueprint {
                position: [-3.0, 0.0, -4.0],
                quarter_turns: 0,
                mirrored: false,
                flipped: false,
            },
        )
        .unwrap_err();
    assert!(format!("{busy:#}").contains("still working"), "{busy:#}");
    assert!(g.undo(host).is_none());
    assert!(finish_work(&mut g, host) > 10);
    assert_eq!(g.bricks().len(), 48);
    // One undo takes it all back, a slice each tick too.
    g.undo(host);
    assert!(g.s.copy_working(host));
    finish_work(&mut g, host);
    assert_eq!(g.bricks().len(), 24);
}

#[test]
fn a_cancelled_plant_keeps_what_went_in_as_one_undo() {
    let mut g = Game::new();
    let host = host(&mut g);
    field(&mut g, host, 4, 6);
    copy_box(&mut g, host, [0.0, 0.0, -6.0], [4.0, 0.2, -3.0], 100).unwrap();
    g.s.set_copy_work(30);
    g.steps(121);
    g.cmd(
        host,
        Command::PlaceBlueprint {
            position: [-3.0, 0.0, 0.0],
            quarter_turns: 0,
            mirrored: false,
            flipped: false,
        },
    )
    .unwrap();
    // All or none: every brick is checked before the first goes in.
    g.steps(20);
    assert_eq!(g.bricks().len(), 24);
    g.steps(15);
    assert!(g.s.cancel_copy(host));
    assert!(!g.s.copy_working(host));
    assert!(!g.s.cancel_copy(host), "nothing left to cancel");
    let planted = g.bricks().len() - 24;
    assert!((1..24).contains(&planted), "{planted} planted");
    // Undo takes back just those.
    g.s.set_copy_work(bri_sim::session::DEFAULT_COPY_WORK);
    g.undo(host);
    finish_work(&mut g, host);
    assert_eq!(g.bricks().len(), 24);
}

#[test]
fn a_big_cut_goes_over_ticks_and_its_undo_puts_every_brick_back() {
    let mut g = Game::new();
    let host = host(&mut g);
    field(&mut g, host, 4, 6);
    let before: Vec<Brick> = g.bricks().into_values().collect();
    copy_box(&mut g, host, [0.0, 0.0, -6.0], [4.0, 0.2, -3.0], 100).unwrap();
    g.s.set_copy_work(40);
    g.typed(host, "cut");
    assert!(g.s.copy_working(host));
    assert!(finish_work(&mut g, host) > 3);
    assert!(g.bricks().is_empty());
    g.undo(host);
    finish_work(&mut g, host);
    let mut back: Vec<Brick> = g.bricks().into_values().collect();
    back.sort_by(|x, y| x.position.partial_cmp(&y.position).unwrap());
    let mut expected = before;
    expected.sort_by(|x, y| x.position.partial_cmp(&y.position).unwrap());
    assert_eq!(back, expected);
}

#[test]
fn a_supercut_puts_plain_bricks_over_what_stuck_out_and_its_undo_goes_over_ticks() {
    let mut g = Game::new();
    let host = host(&mut g);
    let guest =
        g.s.join("Guest".into(), Vec3::new(2.0, 0.05, 3.0), false)
            .unwrap();
    field(&mut g, host, 2, 4);
    // Half of it reaches into the box.
    g.plant_as(host, "plain-4x1", [0.0, 0.1, 0.25], 0);
    let before: Vec<Brick> = g.bricks().into_values().collect();
    let cut = g
        .s
        .super_cut(host, [0.0, 0.0, -6.0], [2.0, 0.2, 0.5], Some("advanced-duplicator"))
        .unwrap();
    assert_eq!((cut.bricks, cut.placed, cut.refused), (9, 1, 0));
    let world = g.bricks();
    let piece = world.values().next().expect("the half outside the box");
    assert_eq!(world.len(), 1);
    assert_eq!(piece.definition, ContentRef::Resolved("plain-2x1".into()));
    assert_eq!(piece.position, [-0.5, 0.1, 0.25]);
    // About a brick a tick from here on.
    g.s.set_copy_work(32);
    // The guest's plate where the long one's other half stood: the undo
    // takes the plain brick out, finds the way blocked and puts it back.
    let blocker = g.plant(guest, [0.5, 0.1, 0.25]);
    g.undo(host);
    finish_work(&mut g, host);
    let prints = prints(&g.notices(host));
    assert!(prints.iter().any(|p| p.contains("Something is in the way")), "{prints:?}");
    let world = g.bricks();
    assert_eq!(world.len(), 2);
    assert!(world.values().any(|b| b.position == [-0.5, 0.1, 0.25]));
    // With the way clear, the next undo puts every cut brick back as it
    // was, a slice each tick, and the plain brick goes.
    g.undo(guest);
    assert!(!g.bricks().contains_key(&blocker));
    g.undo(host);
    assert!(g.s.copy_working(host));
    assert!(finish_work(&mut g, host) > 3);
    let mut back: Vec<Brick> = g.bricks().into_values().collect();
    back.sort_by(|x, y| x.position.partial_cmp(&y.position).unwrap());
    let mut expected = before;
    expected.sort_by(|x, y| x.position.partial_cmp(&y.position).unwrap());
    assert_eq!(back, expected);
}

/// A copy planted while its player runs a mini-game saves with the build
/// and that mini-game, and loads back brick for brick, mini-game and all,
/// where the duplicator copies and plants it again.
#[test]
fn a_planted_copy_saves_and_loads_back_with_its_mini_game() {
    use bri_sim::session::MiniGameRequest;
    let mut g = Game::new();
    let host = host(&mut g);
    let settings = bri_minigames::Settings {
        title: "Copies".into(),
        loadout: Default::default(),
        ..bri_minigames::Settings::default()
    };
    scene(&mut g, host);
    copy_box(&mut g, host, [-1.5, 0.0, -0.5], [1.0, 0.4, 0.5], 100).unwrap();
    g.cmd(host, Command::MiniGame(MiniGameRequest::Create { color: 3, settings: settings.clone() }))
        .unwrap();
    g.steps(130);
    let Ok(Reply::Planted(_)) = g.place(host, [-4.0, 0.0, -3.0], 0, false) else {
        panic!("the copy plants")
    };
    assert_eq!(g.bricks().len(), 7);
    let build = match g.cmd(host, Command::SaveBuild { events: true, ownership: true }) {
        Ok(Reply::Saved(build)) => build,
        other => panic!("{other:?}"),
    };
    assert!(build.minigame.is_some());
    let shape = |bricks: BTreeMap<BrickId, Brick>| {
        let mut v: Vec<_> = bricks
            .into_values()
            .map(|b| (b.definition, b.position.map(f32::to_bits), b.quarter_turns, b.color))
            .collect();
        v.sort_by(|x, y| x.1.cmp(&y.1));
        v
    };
    let saved = shape(g.bricks());

    let mut h = Game::new();
    let loader = self::host(&mut h);
    let bytes = bri_world::build::encode(&build).unwrap();
    let build = bri_world::build::decode(&bytes).unwrap();
    h.cmd(loader, Command::LoadBuild { build: Box::new(build), ownership: true })
        .unwrap();
    while h.s.build_loading() {
        h.steps(1);
    }
    h.steps(2);
    assert_eq!(shape(h.bricks()), saved);
    let view = h.s.minigame_views();
    assert_eq!(view.len(), 1);
    assert_eq!(view[0].settings, settings);
    // The loaded build is the loader's to copy and plant again.
    assert_eq!(copy_box(&mut h, loader, [-1.5, 0.0, -0.5], [1.0, 0.4, 0.5], 100), Ok(3));
    h.steps(130);
    let Ok(Reply::Planted(_)) = h.place(loader, [4.0, 0.0, 4.0], 0, false) else {
        panic!("the loaded build copies")
    };
    assert_eq!(h.bricks().len(), 10);
}
