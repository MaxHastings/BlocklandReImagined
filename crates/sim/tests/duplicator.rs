//! The Duplicator (`packages/duplicator`) and the engine seams under it:
//! copying a build (`copy_build`), placing it all or none
//! (`PlaceBlueprint`), one undo for a placed copy, and an Add-On tool whose
//! swing runs an Add-On command.
use bri_admin::{Action, Request, ServerSettings};
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    blueprint::Blueprint,
    definitions::{Definition, Definitions},
    player::MoveInput,
    session::{ActionAim, Command, Notice, PackageCommand, Reply, Session, ToolAction},
    simulation::{PlantFailure, Simulation},
};
use bri_world::{Brick, BrickId, OwnerId, World};
use glam::Vec3;
use rapier3d::prelude::*;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

const TOOL: &str = "duplicator-tool:weapon/duplicator";

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
        quads: vec![],
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
            },
        )]
        .into(),
    }
}

fn add_ons() -> Arc<Catalog> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/duplicator");
    let packages = [
        ("duplicator", Side::Server),
        ("duplicator-tool", Side::Shared),
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
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/duplicator/duplicator-tool/assets/weapons.json");
    bri_weapons::Pack::from_json(&std::fs::read(path).unwrap()).unwrap()
}

/// A flat floor, one plate definition, the Duplicator running.
struct Game {
    s: Session,
    seq: BTreeMap<OwnerId, u64>,
}
impl Game {
    fn new() -> Self {
        Self::with_pack(tool_pack(), None)
    }
    fn with_pack(
        pack: bri_weapons::Pack,
        bounds: Option<BTreeMap<String, bri_weapons::ItemBounds>>,
    ) -> Self {
        let mut s = Session::new(
            Simulation::new(
                World::new(
                    "Dup".into(),
                    "dup".into(),
                    vec![[1.0; 4], [0.2, 0.4, 1.0, 1.0]],
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
        s.set_weapon_pack(pack).unwrap();
        if let Some(bounds) = bounds {
            s.set_item_bounds(bounds).unwrap();
        }
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
    fn steps(&mut self, n: usize) {
        for _ in 0..n {
            self.s.step().unwrap();
        }
    }
    fn plant(&mut self, owner: OwnerId, position: [f32; 3], turns: u8) -> BrickId {
        match self.cmd(
            owner,
            Command::Plant {
                definition: "plate".into(),
                position,
                quarter_turns: turns,
                color: 1,
            },
        ) {
            Ok(Reply::Planted(id)) => id,
            other => panic!("plant at {position:?}: {other:?}"),
        }
    }
    fn bricks(&self) -> usize {
        self.s.snapshot().world.bricks.len()
    }
    fn place(&mut self, owner: OwnerId, position: [f32; 3], turns: u8) -> anyhow::Result<Reply> {
        self.cmd(
            owner,
            Command::PlaceBlueprint {
                position,
                quarter_turns: turns,
                mirrored: false,
            },
        )
    }
    /// The Duplicator's click, sent as its command with the player's aim.
    fn select(&mut self, owner: OwnerId) -> anyhow::Result<Reply> {
        let n = self.seq.entry(owner).or_default();
        *n += 1;
        self.s.command_with_aim(
            owner,
            *n,
            Command::Package(PackageCommand {
                package: "duplicator".into(),
                command: "select".into(),
                args: vec![],
            }),
            Some(ActionAim {
                yaw: 0.0,
                pitch: -1.55,
            }),
        )
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

fn look_down(g: &mut Game, owner: OwnerId) {
    for n in 0..30u64 {
        g.s.movement(
            owner,
            1000 + n,
            MoveInput {
                pitch: -1.55,
                ..Default::default()
            },
        )
        .unwrap();
        g.s.step().unwrap();
    }
}

fn failure(result: anyhow::Result<Reply>) -> Option<PlantFailure> {
    result
        .err()
        .and_then(|e| e.downcast_ref::<PlantFailure>().copied())
}

/// A three-plate stair (A on the floor, B on A, C half on B) and a lone
/// plate D, all the host's.
fn stair(g: &mut Game, owner: OwnerId) -> [BrickId; 4] {
    let a = g.plant(owner, [0.5, 0.1, 0.25], 0);
    let b = g.plant(owner, [0.5, 0.3, 0.25], 0);
    let c = g.plant(owner, [1.0, 0.5, 0.25], 0);
    let d = g.plant(owner, [4.5, 0.1, 0.25], 0);
    [a, b, c, d]
}

#[test]
fn a_copy_takes_the_clicked_brick_and_the_build_on_it() {
    let mut g = Game::new();
    let host =
        g.s.join("Host".into(), Vec3::new(0.0, 0.05, 3.0), true)
            .unwrap();
    let [a, b, c, _] = stair(&mut g, host);
    // From the bottom: the whole stair, nearest first, not the lone plate.
    assert_eq!(g.s.copy_build(host, a, 100, true, TOOL).unwrap(), 3);
    let copy = g.s.blueprint(host).unwrap().clone();
    assert_eq!(copy.bricks.len(), 3);
    assert_eq!(copy.tool, TOOL);
    assert!(
        copy.bricks
            .iter()
            .all(|brick| brick.owner == 0 && brick.color == 1)
    );
    // It was sent to the player to show.
    assert!(
        g.s.take_private_notices().iter().any(
            |(o, n)| *o == host && matches!(n, Notice::Blueprint(Some(sent)) if **sent == copy)
        )
    );
    // From the middle: never below the clicked brick...
    assert_eq!(g.s.copy_build(host, b, 100, true, TOOL).unwrap(), 2);
    // ...unless the Add-On asks for everything joined to it.
    assert_eq!(g.s.copy_build(host, c, 100, false, TOOL).unwrap(), 3);
    // Too big a build is refused, not cut short, and the old copy stays.
    let error = g.s.copy_build(host, a, 2, true, TOOL).unwrap_err();
    assert!(
        format!("{error:#}").contains("more than 2 bricks"),
        "{error:#}"
    );
    assert_eq!(g.s.blueprint(host).unwrap().bricks.len(), 3);
    // Only a tool this server has may place it.
    assert!(
        g.s.copy_build(host, a, 100, true, "other:weapon/none")
            .is_err()
    );
}

#[test]
fn a_copy_turns_on_the_grid_and_plants_all_or_nothing() {
    let mut g = Game::new();
    let host =
        g.s.join("Host".into(), Vec3::new(0.0, 0.05, 3.0), true)
            .unwrap();
    let [a, ..] = stair(&mut g, host);
    g.s.copy_build(host, a, 100, true, TOOL).unwrap();
    let copy: Blueprint = g.s.blueprint(host).unwrap().clone();
    let before = g.bricks();
    // Over the original: every brick overlaps, nothing is planted.
    assert_eq!(
        failure(g.place(host, copy.origin, 0)),
        Some(PlantFailure::Overlap)
    );
    // In the air: nothing holds it up.
    assert_eq!(
        failure(g.place(host, [-3.0, 2.0, 0.0], 0)),
        Some(PlantFailure::Float)
    );
    // One brick of it would overlap the lone plate: none of it plants.
    assert_eq!(
        failure(g.place(host, [4.5, 0.0, 0.0], 0)),
        Some(PlantFailure::Overlap)
    );
    assert_eq!(g.bricks(), before);
    // Turned a quarter, off to the side, it all plants on the grid.
    let Ok(Reply::Planted(_)) = g.place(host, [-3.0, 0.0, -2.0], 1) else {
        panic!("the copy plants")
    };
    assert_eq!(g.bricks(), before + 3);
    let world = g.s.snapshot().world;
    let placed: Vec<&Brick> = world
        .bricks
        .values()
        .filter(|brick| brick.position[0] < -1.0)
        .collect();
    assert_eq!(placed.len(), 3);
    for brick in &placed {
        assert_eq!(
            (brick.owner, brick.quarter_turns, brick.color),
            (host, 1, 1)
        );
        bri_sim::grid::Bounds::new(brick, &definitions().entries["plate"].mesh).unwrap();
    }
    // The pivot's point is snapped: a request between grid points lands on one.
    g.steps(121);
    let Ok(Reply::Planted(_)) = g.place(host, [6.13, 0.07, 3.24], 2) else {
        panic!("a snapped copy plants")
    };
    assert_eq!(g.bricks(), before + 6);
}

#[test]
fn one_undo_takes_a_placed_copy_back() {
    let mut g = Game::new();
    let host =
        g.s.join("Host".into(), Vec3::new(0.0, 0.05, 3.0), true)
            .unwrap();
    let [a, ..] = stair(&mut g, host);
    let single = g.plant(host, [-6.5, 0.1, 0.25], 0);
    g.s.copy_build(host, a, 100, true, TOOL).unwrap();
    let before = g.bricks();
    g.place(host, [-3.0, 0.0, -2.0], 0).unwrap();
    assert_eq!(g.bricks(), before + 3);
    let Ok(Reply::Undone(Some(_))) = g.cmd(host, Command::Tool(ToolAction::UndoBrick)) else {
        panic!("the copy is undone")
    };
    assert_eq!(g.bricks(), before, "all of the copy went in one undo");
    // The next undo is the plant before it.
    let Ok(Reply::Undone(Some(id))) = g.cmd(host, Command::Tool(ToolAction::UndoBrick)) else {
        panic!("the plate is undone")
    };
    assert_eq!(id, single);
}

#[test]
fn copies_respect_trust() {
    let mut g = Game::new();
    let host =
        g.s.join("Host".into(), Vec3::new(0.0, 0.05, 3.0), true)
            .unwrap();
    let [a, _, _, d] = stair(&mut g, host);
    // The guest stands on the host's lone plate.
    let guest =
        g.s.join("Guest".into(), Vec3::new(4.5, 0.25, 0.25), false)
            .unwrap();
    // Nobody copies a build its owner does not trust them with.
    assert!(g.s.copy_build(guest, a, 100, true, TOOL).is_err());
    assert!(g.s.blueprint(guest).is_none());
    // Through the Add-On (a click on the plate underfoot) the player is
    // told why.
    look_down(&mut g, guest);
    g.s.take_private_notices();
    g.select(guest).unwrap();
    assert!(g.s.blueprint(guest).is_none());
    let told = g.prints(guest);
    assert!(
        told.iter().any(|t| t.contains("does not trust you")),
        "{told:?}"
    );
    // A guest's own plate copies, and the host's bricks never join it: the
    // copy may not be planted onto the host's plate either.
    let own = g.plant(guest, [2.5, 0.1, -2.25], 0);
    assert_eq!(g.s.copy_build(guest, own, 100, false, TOOL).unwrap(), 1);
    let on_d = [4.5, 0.2, 0.0];
    assert_eq!(
        failure(g.place(guest, on_d, 0)),
        Some(PlantFailure::Forbidden)
    );
    let _ = d;
}

#[test]
fn the_brick_limit_and_plant_rate_hold_for_copies() {
    let mut g = Game::new();
    let host =
        g.s.join("Host".into(), Vec3::new(0.0, 0.05, 3.0), true)
            .unwrap();
    let guest =
        g.s.join("Guest".into(), Vec3::new(2.0, 0.05, 3.0), false)
            .unwrap();
    let settings = ServerSettings {
        brick_limit: 8,
        bricks_per_second: 3,
        ..ServerSettings::default()
    };
    let Ok(Reply::Admin(_)) = g.cmd(
        host,
        Command::Admin(Request::new(Action::HostConfigure { settings })),
    ) else {
        panic!("the host configures")
    };
    let a = g.plant(guest, [0.5, 0.1, 0.25], 0);
    g.plant(guest, [0.5, 0.3, 0.25], 0);
    g.s.copy_build(guest, a, 100, true, TOOL).unwrap();
    g.steps(121);
    // Too far from the builder is refused like a far plant.
    assert_eq!(
        failure(g.place(guest, [90.0, 0.0, 0.0], 0)),
        Some(PlantFailure::TooFar)
    );
    // A copy uses the rest of its plant window: the next must wait.
    g.place(guest, [-2.0, 0.0, 0.0], 0).unwrap();
    assert_eq!(
        failure(g.place(guest, [-4.0, 0.0, 0.0], 0)),
        Some(PlantFailure::Limit)
    );
    assert_eq!(
        failure(g.cmd(
            guest,
            Command::Plant {
                definition: "plate".into(),
                position: [6.5, 0.1, 0.25],
                quarter_turns: 0,
                color: 0,
            }
        )),
        Some(PlantFailure::Limit)
    );
    g.steps(121);
    g.place(guest, [-4.0, 0.0, 0.0], 0).unwrap();
    assert_eq!(g.bricks(), 6);
    // Two more would pass the server's limit of eight: none of it plants.
    g.steps(121);
    g.plant(guest, [8.5, 0.1, 0.25], 0);
    assert_eq!(g.bricks(), 7);
    g.steps(121);
    assert_eq!(
        failure(g.place(guest, [-6.0, 0.0, 0.0], 0)),
        Some(PlantFailure::Limit)
    );
    assert_eq!(g.bricks(), 7);
}

#[test]
fn slash_dup_and_a_swing_of_the_duplicator_copy_the_build_underfoot() {
    let mut g = Game::new();
    let builder =
        g.s.join("Builder".into(), Vec3::new(0.0, 0.05, 3.0), true)
            .unwrap();
    // A tower to stand on.
    g.plant(builder, [0.5, 0.1, 0.25], 0);
    g.plant(builder, [0.5, 0.3, 0.25], 0);
    let host =
        g.s.join("Host".into(), Vec3::new(0.5, 0.45, 0.25), true)
            .unwrap();
    // `/dup` in chat: the Duplicator goes in the tool list and in hand.
    let dup = Command::Package(PackageCommand {
        package: String::new(),
        command: "dup".into(),
        args: vec![],
    });
    g.cmd(host, dup.clone()).unwrap();
    let tools = g.s.tool_inventories()[&host].clone();
    let slot = tools
        .slots
        .iter()
        .position(|s| s.as_deref() == Some(TOOL))
        .unwrap();
    assert_eq!(tools.selected, Some(slot));
    // Asking again does not hand out a second one.
    g.cmd(host, dup).unwrap();
    let tools = g.s.tool_inventories()[&host].clone();
    assert_eq!(
        tools
            .slots
            .iter()
            .filter(|s| s.as_deref() == Some(TOOL))
            .count(),
        1
    );
    // Look down and swing.
    look_down(&mut g, host);
    g.s.take_private_notices();
    g.cmd(host, Command::WeaponTrigger { down: true }).unwrap();
    g.steps(60);
    g.cmd(host, Command::WeaponTrigger { down: false }).unwrap();
    g.steps(30);
    let copy =
        g.s.blueprint(host)
            .expect("the swing copied the brick underfoot");
    assert_eq!(copy.bricks.len(), 1, "the top plate, nothing below it");
    let told = g.prints(host);
    assert!(told.iter().any(|t| t == "Copied 1 brick"), "{told:?}");
}

#[test]
fn slash_duplorcator_pulls_the_duplicator_out_with_every_slot_full() {
    // Five other tools to fill the slots with.
    let mut pack = tool_pack();
    let fillers: Vec<String> = (0..5)
        .map(|n| format!("duplicator-tool:weapon/filler{n}"))
        .collect();
    for id in &fillers {
        let mut item = pack.items[TOOL].clone();
        item.id = id.clone();
        pack.items.insert(id.clone(), item);
    }
    let bounds = pack
        .items
        .keys()
        .map(|id| (id.clone(), bri_weapons::ItemBounds::FALLBACK))
        .collect();
    let mut g = Game::with_pack(pack, Some(bounds));
    let host =
        g.s.join("Host".into(), Vec3::new(0.0, 0.05, 3.0), true)
            .unwrap();
    for id in &fillers {
        if g.s.give_item(host, id).is_err() {
            break;
        }
    }
    let tools = g.s.tool_inventories()[&host].clone();
    assert!(tools.slots.iter().all(Option::is_some), "{tools:?}");
    g.cmd(host, Command::EquipTool { slot: Some(1) }).unwrap();
    let in_hand = tools.slots[1].clone().unwrap();
    // v20's Add-On named it `/duplorcator`; it came out whatever you carried.
    g.cmd(
        host,
        Command::Package(PackageCommand {
            package: String::new(),
            command: "duplorcator".into(),
            args: vec![],
        }),
    )
    .unwrap();
    let tools = g.s.tool_inventories()[&host].clone();
    assert_eq!(tools.slots[1].as_deref(), Some(TOOL));
    assert_eq!(tools.selected, Some(1));
    // The tool it replaced went down on the ground, not away.
    assert!(g.s.weapon_view().drops.iter().any(|d| d.item == in_hand));
}

#[test]
fn custom_games_on_base_maps_run_the_duplicator() {
    // No game mode claims it and it needs no Add-On world, so a Custom
    // game on Slate or Bedroom runs it, as v20 ran enabled Add-Ons.
    let base = add_ons().for_base_map().unwrap();
    assert!(base.packages.contains_key("duplicator"));
    assert!(base.packages.contains_key("duplicator-tool"));
}
