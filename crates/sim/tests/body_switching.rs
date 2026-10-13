//! v0.3.0 W4: becoming another body while playing, without a mini-game.
//! Bodies from separate Add-Ons (the Playground's Runner, the Trooper and
//! the Wall Kicker with their own movement controllers, the Commando, and
//! this test's Giant and Statue) are chosen from a brick's
//! `onActivate -> Player -> ChangeDatablock`, from `/body` and from a mini-game's
//! rules, and every way goes through the one transition: the held Carbine,
//! a seat in the Dune Buggy, momentum in the air, health and the room to
//! stand all come through it, with nothing special-cased per body.
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_package::packages::{PackageEntry, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::{Definition, Definitions},
    player::{MoveInput, PlayerState},
    session::{Command, MiniGameRequest, Notice, PackageArg, PackageCommand, Session, ToolCatalog},
    simulation::Simulation,
};
use bri_world::{EventRow, EventTarget, EventValue, OwnerId, World, authority::Edit};
use glam::Vec3;
use rapier3d::prelude::*;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

const STANDARD: &str = "v20.player.playerstandardarmor";
const RUNNER: &str = "playground-runner:archetype/runner";
const TROOPER: &str = "sample-trooper:archetype/trooper";
const KICKER: &str = "sample-wall-kick:archetype/kicker";
const COMMANDO: &str = "sample-commando:archetype/commando";
const STOMPER: &str = "playground-stomper:archetype/stomper";
const GIANT: &str = "test-bodies:archetype/giant";
const STATUE: &str = "test-bodies:archetype/statue";
const CARBINE: &str = "playground-rifle:weapon/carbine";
const BUGGY: &str = "playground-buggy:vehicle/buggy";
const SPAWN: Vec3 = Vec3::new(0.0, 0.05, 8.0);

fn packages() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages")
}

/// Every Add-On a test runs, by id: where it is and its side.
fn add_on(id: &str) -> (PathBuf, Side) {
    match id {
        "playground-runner" => (
            packages().join("playground/playground-runner"),
            Side::Server,
        ),
        "playground-runner-look" => (
            packages().join("playground/playground-runner-look"),
            Side::Client,
        ),
        "playground-rifle" => (packages().join("playground/playground-rifle"), Side::Shared),
        "playground-buggy" => (packages().join("playground/playground-buggy"), Side::Shared),
        "playground-mode" => (packages().join("playground/playground-mode"), Side::Server),
        "playground-runner-hud" => (
            packages().join("playground/playground-runner-hud"),
            Side::Client,
        ),
        "playground-stomper" => (
            packages().join("playground/playground-stomper"),
            Side::Shared,
        ),
        "playground-stomper-look" => (
            packages().join("playground/playground-stomper-look"),
            Side::Client,
        ),
        "playground-stomper-rules" => (
            packages().join("playground/playground-stomper-rules"),
            Side::Server,
        ),
        "playground-fizzer" => (
            packages().join("playground/playground-fizzer"),
            Side::Shared,
        ),
        "playground-fizzer-look" => (
            packages().join("playground/playground-fizzer-look"),
            Side::Client,
        ),
        "playground-fizzer-rules" => (
            packages().join("playground/playground-fizzer-rules"),
            Side::Server,
        ),
        "blockhead_bot" => (packages().join("blockhead_bot"), Side::Shared),
        "sample-commando" | "sample-trooper" | "sample-wall-kick" => {
            (packages().join("samples").join(id), Side::Server)
        }
        "sample-commando-rifle" => (packages().join("samples").join(id), Side::Shared),
        "sample-commando-look" => (packages().join("samples").join(id), Side::Client),
        "test-bodies" => (
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bodies"),
            Side::Server,
        ),
        _ => panic!("no Add-On {id}"),
    }
}

/// The bodies on a plain building server, no mode.
const BODIES: [&str; 7] = [
    "playground-runner",
    "playground-runner-look",
    "playground-rifle",
    "playground-buggy",
    "sample-trooper",
    "sample-wall-kick",
    "test-bodies",
];
/// The Playground mode, with the Add-Ons it names.
const MODE: [&str; 14] = [
    "playground-runner",
    "playground-runner-look",
    "playground-runner-hud",
    "sample-trooper",
    "playground-rifle",
    "playground-buggy",
    "blockhead_bot",
    "playground-stomper",
    "playground-stomper-look",
    "playground-stomper-rules",
    "playground-fizzer",
    "playground-fizzer-look",
    "playground-fizzer-rules",
    "playground-mode",
];

fn catalog(ids: &[&str], mode: bool) -> Arc<Catalog> {
    let dirs: Vec<_> = ids
        .iter()
        .map(|id| {
            let (dir, side) = add_on(id);
            (
                dir,
                PackageEntry {
                    id: (*id).into(),
                    version: "1.0.0".into(),
                    side,
                    dir: (*id).into(),
                    role: None,
                },
            )
        })
        .collect();
    let mut catalog = Catalog::load_dirs(&dirs, true).unwrap_or_else(|e| panic!("{e:#?}"));
    if mode {
        let mode = catalog
            .for_mode("playground-mode:mode/playground")
            .unwrap_or_else(|e| panic!("{e:#?}"));
        catalog.mode = mode.mode;
    }
    Arc::new(catalog)
}

/// The Carbine, the buggy's gun and (with `commando`) the Commando Rifle,
/// merged as a host merges every package's.
fn weapons(commando: bool) -> bri_weapons::Pack {
    let load = |dir: PathBuf| {
        bri_weapons::Pack::from_json(&std::fs::read(dir.join("weapons.json")).unwrap()).unwrap()
    };
    let mut others = vec![(
        "buggy".into(),
        load(packages().join("playground/playground-buggy/assets")),
    )];
    if commando {
        others.push((
            "commando".into(),
            load(packages().join("samples/sample-commando-rifle/assets")),
        ));
    }
    let (pack, problems) =
        load(packages().join("playground/playground-rifle/assets")).merge(others);
    assert!(problems.is_empty(), "{problems:?}");
    pack
}

/// A plate: 1 wide, 0.2 tall, 0.5 deep.
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
                bot: None,
            },
        )]
        .into(),
    }
}

/// A low roof 3.2 above the floor over x -20..-10, a room a Blockhead
/// stands in and a Giant does not.
const UNDER_ROOF: Vec3 = Vec3::new(-15.0, 0.05, 0.0);

/// The floor, and a roof 3.2 above it around [`UNDER_ROOF`].
fn map() -> Vec<ColliderBuilder> {
    vec![
        ColliderBuilder::cuboid(200.0, 0.5, 200.0).translation(Vector::new(0.0, -0.5, 0.0)),
        ColliderBuilder::cuboid(5.0, 0.25, 5.0).translation(Vector::new(
            UNDER_ROOF.x,
            3.45,
            UNDER_ROOF.z,
        )),
    ]
}

struct Game {
    s: Session,
    seq: BTreeMap<OwnerId, u64>,
    moves: BTreeMap<OwnerId, u64>,
    looks: BTreeMap<OwnerId, MoveInput>,
}
impl Game {
    /// A building server with no mini-game running [`BODIES`].
    fn new() -> Self {
        Self::server(&BODIES, false, false)
    }
    fn server(ids: &[&str], mode: bool, commando: bool) -> Self {
        let mut s = Session::new(
            Simulation::new(
                World::new("Bodies".into(), "bodies".into(), vec![[1.0; 4]]),
                definitions(),
                map(),
            )
            .unwrap(),
        );
        s.set_weapon_pack(weapons(commando)).unwrap();
        s.set_item_bounds(
            [(
                CARBINE.to_string(),
                bri_weapons::ItemBounds {
                    min: [-0.1, -0.15, -0.9],
                    max: [0.1, 0.15, 0.4],
                },
            )]
            .into(),
        )
        .unwrap();
        let buggy = bri_vehicles::Pack::load(
            packages().join("playground/playground-buggy/assets/vehicles.json"),
        )
        .unwrap();
        s.set_vehicle_pack(buggy, Vec::new()).unwrap();
        s.set_tool_catalog(ToolCatalog {
            items: [CARBINE.to_string()].into(),
            vehicles: [BUGGY.to_string()].into(),
            vehicle_bricks: ["plate".to_string()].into(),
            ..Default::default()
        })
        .unwrap();
        // The test catalog, with v20's `Player addHealth`.
        let mut events = bri_events::testing::catalog();
        events.outputs.push(bri_events::OutputDef {
            id: "out/Player/addHealth".into(),
            class_name: "Player".into(),
            name: "addHealth".into(),
            params: vec![bri_events::Param::Int {
                min: -1000,
                max: 1000,
                default: 25,
            }],
            append_client: false,
            source: "fixture".into(),
            source_line: 1,
            package: None,
        });
        s.set_event_catalog(events, Vec::new()).unwrap();
        s.set_body_grips(
            "v20.shape.m",
            bri_sim::session::shape_grips(&bri_content::testing::avatar::rig(), &[]),
        )
        .unwrap();
        let mut ids = ids.to_vec();
        if commando {
            ids.extend([
                "sample-commando-rifle",
                "sample-commando-look",
                "sample-commando",
            ]);
        }
        s.install_packages(catalog(&ids, mode), None).unwrap();
        s.set_spawn_points(vec![SPAWN]).unwrap();
        Self {
            s,
            seq: BTreeMap::new(),
            moves: BTreeMap::new(),
            looks: BTreeMap::new(),
        }
    }
    fn join(&mut self, name: &str) -> OwnerId {
        let owner = self.s.join(name.into(), SPAWN, false).unwrap();
        self.looks.insert(owner, MoveInput::default());
        self.steps(40);
        owner
    }
    fn cmd(&mut self, owner: OwnerId, command: Command) -> anyhow::Result<()> {
        let n = self.seq.entry(owner).or_default();
        *n += 1;
        self.s.command(owner, *n, command).map(drop)
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
    fn drive(&mut self, owner: OwnerId, input: MoveInput, ticks: usize) {
        let before = self.looks.insert(owner, input).unwrap();
        self.steps(ticks);
        self.looks.insert(
            owner,
            MoveInput {
                yaw: input.yaw,
                pitch: input.pitch,
                ..before
            },
        );
    }
    fn state(&self, owner: OwnerId) -> PlayerState {
        self.s
            .motion_states()
            .into_iter()
            .find(|(p, _)| p.owner == owner)
            .unwrap()
            .0
    }
    fn body(&self, owner: OwnerId) -> String {
        self.s
            .archetypes()
            .resolve(self.state(owner).archetype)
            .id
            .clone()
    }
    fn eye(&self, owner: OwnerId) -> f32 {
        let state = self.state(owner);
        self.s.archetypes().eye(&state).y - state.feet[1]
    }
    fn health(&self, owner: OwnerId) -> f32 {
        self.s.vitals()[&owner].health
    }
    fn held(&self, owner: OwnerId) -> Option<String> {
        let inventory = self.s.tool_inventories().remove(&owner)?;
        inventory
            .selected
            .and_then(|slot| inventory.slots[slot].clone())
    }
    fn carries(&self, owner: OwnerId, item: &str) -> bool {
        self.s.tool_inventories()[&owner]
            .slots
            .iter()
            .any(|s| s.as_deref() == Some(item))
    }
    fn equip(&mut self, owner: OwnerId, item: &str) {
        let slot = self.s.tool_inventories()[&owner]
            .slots
            .iter()
            .position(|s| s.as_deref() == Some(item))
            .unwrap_or_else(|| panic!("{owner} carries no {item}"));
        self.cmd(owner, Command::EquipTool { slot: Some(slot) })
            .unwrap();
        self.steps(1);
    }
    /// A brick that turns whoever clicks it into `body` (the wrench's
    /// `onActivate -> Player -> ChangeDatablock`), planted by `builder`.
    fn class_brick(&mut self, builder: OwnerId, at: [f32; 3], body: &str) -> u64 {
        self.cmd(
            builder,
            Command::Plant {
                definition: "plate".into(),
                position: at,
                quarter_turns: 0,
                color: 0,
            },
        )
        .unwrap();
        let id = *self.s.simulation().state().bricks.keys().max().unwrap();
        self.s
            .edit_brick(
                builder,
                id,
                Edit::Events(vec![EventRow {
                    conditions: vec![],
                    preserved: None,
                    enabled: true,
                    input: "onActivate".into(),
                    delay_ms: 0,
                    target: EventTarget::Slot(bri_events::Slot::Player),
                    output: "ChangeDatablock".into(),
                    params: vec![EventValue::Datablock(Some(body.into()))],
                }]),
            )
            .unwrap_or_else(|e| panic!("{e:#}"));
        id
    }
    /// `owner` clicks the brick.
    fn click(&mut self, brick: u64, owner: OwnerId) {
        self.s.fire_brick_input(brick, "onActivate", Some(owner));
        self.steps(1);
        assert!(
            self.s.take_event_diagnostics().is_empty(),
            "the event ran cleanly"
        );
    }
    /// `owner` types `/body <named>`.
    fn slash_body(&mut self, owner: OwnerId, named: &str) {
        self.cmd(
            owner,
            Command::Package(PackageCommand {
                package: String::new(),
                command: "body".into(),
                args: named
                    .split_whitespace()
                    .map(|w| PackageArg::String(w.into()))
                    .collect(),
            }),
        )
        .unwrap();
        self.steps(BODY_COOLDOWN);
    }
    /// `/body <named>` with no steps after it.
    fn slash_body_now(&mut self, owner: OwnerId, named: &str) {
        self.cmd(
            owner,
            Command::Package(PackageCommand {
                package: String::new(),
                command: "body".into(),
                args: vec![PackageArg::String(named.into())],
            }),
        )
        .unwrap();
    }
    fn told(&mut self, owner: OwnerId) -> Vec<String> {
        self.s
            .take_private_notices()
            .into_iter()
            .filter(|(o, _)| *o == owner)
            .filter_map(|(_, n)| match n {
                Notice::Chat(text) | Notice::Center { text, .. } => Some(text),
                _ => None,
            })
            .collect()
    }
    /// Run at the buggy, hopping, until mounted (v20 mounts from above).
    fn board(&mut self, owner: OwnerId) -> (u64, u8) {
        for _ in 0..30 {
            self.drive(
                owner,
                MoveInput {
                    forward: -1.0,
                    ..Default::default()
                },
                20,
            );
            for i in 0..30 {
                self.drive(
                    owner,
                    MoveInput {
                        forward: 1.0,
                        jump: i < 3,
                        ..Default::default()
                    },
                    1,
                );
                if let Some(seat) = self.s.mounted(owner) {
                    return seat;
                }
            }
        }
        panic!("never boarded the buggy");
    }
}
/// Ticks `/body` waits between switches, and a little more.
const BODY_COOLDOWN: usize = 65;

/// Max's ask: a room of class bricks, no mini-game. Each brick's
/// ChangeDatablock lists every loaded Add-On body by id beside v20's
/// datablocks, and clicking one makes the clicker that body: its own
/// height and eyes, its own movement controller, the Carbine still in
/// hand, health kept as a share. A builder can click back to a Blockhead.
#[test]
fn class_bricks_turn_a_builder_into_any_add_on_body_and_back() {
    let mut g = Game::new();
    let ann = g.join("Ann");
    let listed = bri_sim::session::event_bodies(g.s.archetypes());
    for body in [RUNNER, TROOPER, KICKER, GIANT, STATUE] {
        assert!(listed.iter().any(|b| b == body), "{body} in {listed:?}");
    }
    assert!(listed.iter().any(|b| b == "PlayerStandardArmor"));
    assert!(listed.iter().any(|b| b == "HorseArmor"));
    let bricks: Vec<_> = [STANDARD, RUNNER, TROOPER, KICKER]
        .iter()
        .enumerate()
        .map(|(i, body)| {
            let datablock = if *body == STANDARD {
                "PlayerStandardArmor"
            } else {
                body
            };
            g.class_brick(ann, [3.5 + i as f32 * 2.0, 0.1, 0.25], datablock)
        })
        .collect();
    g.s.give_item(ann, CARBINE).unwrap();
    g.equip(ann, CARBINE);
    assert_eq!(g.body(ann), STANDARD);
    let blockhead_eye = g.eye(ann);

    g.click(bricks[1], ann);
    assert_eq!(g.body(ann), RUNNER);
    assert!(g.eye(ann) < blockhead_eye - 0.4, "the Runner's own eyes");
    assert_eq!(g.held(ann).as_deref(), Some(CARBINE), "the Carbine in hand");
    let tuning = g.s.archetypes().tuning(g.state(ann).archetype, 1.0);
    assert!(
        tuning.controller.is_none(),
        "the Runner moves as a Blockhead does"
    );

    g.click(bricks[2], ann);
    assert_eq!(g.body(ann), TROOPER);
    let trooper = g.s.archetypes().tuning(g.state(ann).archetype, 1.0);
    assert!(trooper.controller.is_some(), "the Trooper's own controller");
    g.click(bricks[3], ann);
    assert_eq!(g.body(ann), KICKER);
    let kicker = g.s.archetypes().tuning(g.state(ann).archetype, 1.0);
    assert!(kicker.controller.is_some() && kicker.controller != trooper.controller);
    assert_eq!(g.held(ann).as_deref(), Some(CARBINE));

    // It walks, jumps and lands as itself, with the Carbine.
    g.drive(
        ann,
        MoveInput {
            forward: 1.0,
            jump: true,
            ..Default::default()
        },
        60,
    );
    g.steps(120);
    assert!(g.state(ann).grounded && g.s.vitals()[&ann].alive);

    g.click(bricks[0], ann);
    assert_eq!(g.body(ann), STANDARD);
    assert!((g.eye(ann) - blockhead_eye).abs() < 1e-3);
    assert_eq!(g.held(ann).as_deref(), Some(CARBINE));
    assert!(
        g.s.package_diagnostics().is_empty(),
        "{:?}",
        g.s.package_diagnostics()
    );
}

/// The Carbine fires from each body's own hand: the shot leaves lower for
/// the short Runner than for the Blockhead, and from the Blockhead's hand
/// again once it is a Blockhead again.
#[test]
fn the_carbine_fires_from_the_hand_of_whichever_body_holds_it() {
    let mut g = Game::new();
    let ann = g.join("Ann");
    let bricks = [STANDARD, RUNNER, STANDARD].map(|b| {
        let datablock = if b == STANDARD {
            "PlayerStandardArmor"
        } else {
            b
        };
        let x = 3.5 + 2.0 * g.s.simulation().state().bricks.len() as f32;
        g.class_brick(ann, [x, 0.1, 0.25], datablock)
    });
    g.s.give_item(ann, CARBINE).unwrap();
    g.equip(ann, CARBINE);
    g.steps(300);
    let mut heights = Vec::new();
    for brick in bricks {
        g.click(brick, ann);
        g.steps(30);
        let feet = g.state(ann).feet[1];
        let before: Vec<u64> = g.s.weapon_view().fired().map(|p| p.id).collect();
        g.cmd(ann, Command::WeaponTrigger { down: true }).unwrap();
        let mut shot = None;
        for _ in 0..4 {
            g.steps(1);
            shot = shot.or_else(|| {
                g.s.weapon_view()
                    .fired()
                    .find(|p| p.source.0 == ann && !before.contains(&p.id))
                    .cloned()
            });
        }
        g.cmd(ann, Command::WeaponTrigger { down: false }).unwrap();
        g.steps(30);
        let shot = shot.unwrap_or_else(|| panic!("{} fires", g.body(ann)));
        heights.push(shot.origin[1] - feet);
    }
    let [blockhead, runner, again] = heights[..] else {
        unreachable!()
    };
    assert!(runner < blockhead - 0.2, "{runner} below {blockhead}");
    assert!(
        (again - blockhead).abs() < 0.05,
        "{again} back at {blockhead}"
    );
}

/// Switching mid-jump keeps the body's momentum and starts the new body's
/// controller afresh: nothing the Wall Kicker's controller remembered
/// reaches the Trooper's, and the switch lands where the jump would have.
#[test]
fn switching_mid_air_keeps_momentum_and_starts_the_new_controller_afresh() {
    let mut g = Game::new();
    let ann = g.join("Ann");
    let [kicker, trooper] = [KICKER, TROOPER].map(|b| {
        let x = 3.5 + 2.0 * g.s.simulation().state().bricks.len() as f32;
        g.class_brick(ann, [x, 0.1, 0.25], b)
    });
    g.click(kicker, ann);
    g.steps(30);
    g.drive(
        ann,
        MoveInput {
            forward: 1.0,
            jump: true,
            ..Default::default()
        },
        12,
    );
    let before = g.state(ann);
    assert!(!before.grounded, "in the air");
    assert!(before.controller.is_some(), "the kicker's controller runs");
    g.s.fire_brick_input(trooper, "onActivate", Some(ann));
    // The transition happens inside the step; read it before the motor
    // moves the new body.
    g.s.step().unwrap();
    let after = g.state(ann);
    assert_eq!(g.body(ann), TROOPER);
    let horizontal = |s: &PlayerState| Vec3::new(s.velocity[0], 0.0, s.velocity[2]).length();
    assert!(
        horizontal(&after) > horizontal(&before) * 0.5,
        "momentum carries: {:?} -> {:?}",
        before.velocity,
        after.velocity
    );
    assert!(!after.grounded, "still in the air");
    assert!(
        after.controller != before.controller,
        "the Trooper's controller starts from nothing it was given"
    );
    g.looks.insert(ann, MoveInput::default());
    g.steps(240);
    assert!(g.state(ann).grounded, "lands");
    assert_eq!(g.body(ann), TROOPER);
}

/// Seated in the Dune Buggy, a body that can ride stays in its seat and
/// gets out later with room for the body it is now; one that cannot ride
/// (the Statue) is set down beside the buggy, standing, alive and out of it.
#[test]
fn switching_while_seated_in_the_buggy_stays_seated_or_steps_out_safely() {
    let mut g = Game::new();
    let ann = g.join("Ann");
    let buggy =
        g.s.spawn_vehicle_at(ann, BUGGY, Vec3::new(0.0, 1.0, 0.0), 0.0, Vec3::ZERO)
            .unwrap();
    g.steps(60);
    assert_eq!(g.board(ann).0, buggy);
    let seat = g.s.mounted(ann);
    g.slash_body(ann, "Runner");
    assert_eq!(g.body(ann), RUNNER);
    assert_eq!(g.s.mounted(ann), seat, "the Runner keeps the seat");
    g.slash_body(ann, "giant");
    assert_eq!(g.body(ann), GIANT, "a seat needs no room to stand");
    assert_eq!(g.s.mounted(ann), seat);
    // Out with the jet key: the exit tests the Giant's box, not the
    // Blockhead's it boarded with, so it is not set down inside the cage.
    g.drive(
        ann,
        MoveInput {
            jet: true,
            ..Default::default()
        },
        2,
    );
    g.steps(30);
    assert_eq!(g.s.mounted(ann), None);
    let giant = g.state(ann);
    let (min, max) = (
        Vec3::from(giant.feet) - Vec3::new(0.62, 0.0, 0.62),
        Vec3::from(giant.feet) + Vec3::new(0.62, 5.4, 0.62),
    );
    let buggy_at =
        g.s.vehicle_poses()
            .into_iter()
            .find(|v| v.id == buggy)
            .unwrap();
    let centre = Vec3::from(buggy_at.position);
    assert!(
        !(centre.cmpge(min - Vec3::splat(0.5)).all() && centre.cmple(max + Vec3::splat(0.5)).all()),
        "the Giant stands clear of the buggy: {:?} vs {centre}",
        giant.feet
    );

    // Back in as a Blockhead, then become a body that cannot ride.
    g.slash_body(ann, "Standard Player");
    g.steps(120);
    assert_eq!(g.board(ann).0, buggy);
    g.slash_body(ann, "Statue");
    assert_eq!(g.body(ann), STATUE);
    assert_eq!(g.s.mounted(ann), None, "the Statue cannot ride");
    assert!(g.s.vitals()[&ann].alive);
    g.steps(120);
    let statue = g.state(ann);
    assert!(statue.grounded, "set down on its feet: {statue:?}");
    let occupants =
        g.s.vehicle_infos()
            .into_iter()
            .find(|v| v.id == buggy)
            .unwrap()
            .occupants;
    assert!(occupants.iter().all(|o| *o != Some(ann)));
}

/// A body that uses no items puts the Carbine away and keeps it; back in a
/// body that does, the Carbine comes out again with a click of its slot.
#[test]
fn a_body_without_hands_puts_the_carbine_away_and_keeps_it() {
    let mut g = Game::new();
    let ann = g.join("Ann");
    g.s.give_item(ann, CARBINE).unwrap();
    g.equip(ann, CARBINE);
    g.slash_body(ann, "Statue");
    assert_eq!(g.body(ann), STATUE);
    assert_eq!(g.held(ann), None, "put away");
    assert!(g.carries(ann, CARBINE), "still carried");
    let slot = g.s.tool_inventories()[&ann]
        .slots
        .iter()
        .position(|s| s.as_deref() == Some(CARBINE))
        .unwrap();
    assert!(
        g.cmd(ann, Command::EquipTool { slot: Some(slot) }).is_err(),
        "a Statue takes nothing out"
    );
    g.slash_body(ann, "Runner");
    g.equip(ann, CARBINE);
    assert_eq!(g.held(ann).as_deref(), Some(CARBINE));
}

/// Under a low roof the Giant does not fit: the click is refused and the
/// clicker told why, nothing changes, and out from under it the same brick
/// works. A smaller body always fits.
#[test]
fn a_body_too_tall_for_the_room_is_refused_and_said() {
    let mut g = Game::new();
    let ann = g.join("Ann");
    let giant = g.class_brick(ann, [3.5, 0.1, 0.25], GIANT);
    let runner = g.class_brick(ann, [5.5, 0.1, 0.25], RUNNER);
    g.s.set_spawn_points(vec![UNDER_ROOF]).unwrap();
    g.cmd(ann, Command::Suicide).unwrap();
    g.steps(200);
    g.cmd(ann, Command::Respawn).unwrap();
    g.steps(20);
    assert!(g.s.vitals()[&ann].alive);
    assert!(Vec3::from(g.state(ann).feet).distance(UNDER_ROOF) < 0.5);
    g.told(ann);
    g.click(giant, ann);
    assert_eq!(g.body(ann), STANDARD, "no room for the Giant");
    assert!(
        g.told(ann)
            .iter()
            .any(|t| t.contains("No room to become Giant")),
        "told why"
    );
    g.click(runner, ann);
    assert_eq!(g.body(ann), RUNNER, "the Runner fits under the roof");
    // Out in the open.
    g.drive(
        ann,
        MoveInput {
            forward: 1.0,
            yaw: std::f32::consts::FRAC_PI_2,
            ..Default::default()
        },
        360,
    );
    assert!(g.state(ann).feet[0] > -9.0, "{:?}", g.state(ann).feet);
    g.click(giant, ann);
    assert_eq!(g.body(ann), GIANT);
}

/// Health is kept as a share of the body's maximum, so a switch neither
/// heals nor kills: half a Blockhead's 100 is half a Giant's 200.
#[test]
fn switching_keeps_the_share_of_health() {
    let mut g = Game::new();
    let ann = g.join("Ann");
    let giant = g.class_brick(ann, [3.5, 0.1, 0.25], GIANT);
    let blockhead = g.class_brick(ann, [5.5, 0.1, 0.25], "PlayerStandardArmor");
    let hurt = g.class_brick(ann, [7.5, 0.1, 0.25], "PlayerStandardArmor");
    g.s.edit_brick(
        ann,
        hurt,
        Edit::Events(vec![EventRow {
            conditions: vec![],
            preserved: None,
            enabled: true,
            input: "onActivate".into(),
            delay_ms: 0,
            target: EventTarget::Slot(bri_events::Slot::Player),
            output: "addHealth".into(),
            params: vec![EventValue::Int(-50)],
        }]),
    )
    .unwrap();
    // Past spawn protection, which spares a new body from events' harm.
    g.steps(400);
    g.click(hurt, ann);
    assert!((g.health(ann) - 50.0).abs() < 0.5, "{}", g.health(ann));
    g.click(giant, ann);
    assert!((g.health(ann) - 100.0).abs() < 0.5, "{}", g.health(ann));
    g.click(blockhead, ann);
    assert!((g.health(ann) - 50.0).abs() < 0.5, "{}", g.health(ann));
}

/// Outside a mini-game a body the player picked is theirs until they pick
/// another: they respawn as it. An Add-On's own choice (the Commando
/// total conversion makes everyone a Commando as they join) is a choice
/// like any other: the latest one wins.
#[test]
fn a_chosen_body_is_kept_through_respawns_and_the_latest_choice_wins() {
    let mut g = Game::server(&BODIES, false, true);
    let ann = g.join("Ann");
    assert_eq!(g.body(ann), COMMANDO, "the Commando Add-On chose");
    let runner = g.class_brick(ann, [3.5, 0.1, 0.25], RUNNER);
    g.click(runner, ann);
    assert_eq!(g.body(ann), RUNNER);
    g.cmd(ann, Command::Suicide).unwrap();
    g.steps(200);
    g.cmd(ann, Command::Respawn).unwrap();
    g.steps(20);
    assert!(g.s.vitals()[&ann].alive);
    assert_eq!(g.body(ann), RUNNER, "respawned as the body they picked");
    // The Trooper Add-On's own command is the Add-On choosing, through
    // the same transition.
    g.cmd(
        ann,
        Command::Package(PackageCommand {
            package: "sample-trooper".into(),
            command: "trooper".into(),
            args: vec![],
        }),
    )
    .unwrap();
    g.steps(2);
    assert_eq!(g.body(ann), TROOPER);
    g.cmd(ann, Command::Suicide).unwrap();
    g.steps(200);
    g.cmd(ann, Command::Respawn).unwrap();
    g.steps(20);
    assert_eq!(g.body(ann), TROOPER);
}

/// `/body` lists what one may be and becomes it by name or id, ignoring
/// case; an unknown name says so and changes nothing.
#[test]
fn slash_body_lists_and_switches_bodies() {
    let mut g = Game::new();
    let ann = g.join("Ann");
    g.told(ann);
    g.slash_body(ann, "");
    let list = g.told(ann).join("\n");
    for name in [
        "Standard Player",
        "Runner",
        "Trooper",
        "Wall Kicker",
        "Giant",
    ] {
        assert!(list.contains(name), "{name} in {list}");
    }
    g.slash_body(ann, "wall kicker");
    assert_eq!(g.body(ann), KICKER);
    g.slash_body(ann, RUNNER);
    assert_eq!(g.body(ann), RUNNER);
    g.slash_body(ann, "Mario");
    assert_eq!(g.body(ann), RUNNER);
    assert!(
        g.told(ann)
            .iter()
            .any(|t| t.contains("No body is called Mario"))
    );
    // A body the host has but this game left out says why, by name or id.
    g.s.set_absent_bodies(vec![
        bri_sim::session::AbsentBody {
            id: "halo:archetype/chief".into(),
            name: "Master Chief".into(),
            why: "Plays only in Halo Slayer games.".into(),
        },
        // One this game has after all is never listed as absent.
        bri_sim::session::AbsentBody {
            id: RUNNER.into(),
            name: "Runner".into(),
            why: "never shown".into(),
        },
    ]);
    g.slash_body(ann, "");
    let list = g.told(ann).join("\n");
    assert!(list.contains("Not in this game: Master Chief"), "{list}");
    assert!(!list.contains("never shown"), "{list}");
    for named in ["master chief", "halo:archetype/chief"] {
        g.slash_body(ann, named);
        assert_eq!(g.body(ann), RUNNER);
        assert_eq!(
            g.told(ann),
            ["Master Chief is not in this game. Plays only in Halo Slayer games."]
        );
    }
}

/// In the Playground mode's mini-game (Runner body, Carbine loadout) the
/// mode lists the Blockhead as a body players may pick and keeps a pick:
/// mid-match `/body` turns a Runner holding the Carbine into a Blockhead
/// still holding it, they respawn as one, a body the mode does not list is
/// refused, and a class brick still works as in v20.
#[test]
fn mid_match_a_runner_becomes_a_blockhead_and_respawns_as_one() {
    let mut g = Game::server(&MODE, true, false);
    let ann = g.join("Ann");
    assert!(g.s.vitals()[&ann].minigame.is_some());
    assert_eq!(g.body(ann), RUNNER);
    g.steps(300);
    g.equip(ann, CARBINE);
    g.told(ann);
    g.slash_body(ann, "");
    let list = g.told(ann).join("\n");
    assert!(
        list.contains("Runner") && list.contains("Standard Player"),
        "{list}"
    );
    assert!(!list.contains("Horse"), "{list}");
    g.slash_body(ann, "Standard Player");
    assert_eq!(g.body(ann), STANDARD);
    assert_eq!(g.held(ann).as_deref(), Some(CARBINE), "still holding it");
    g.slash_body(ann, "Horse");
    assert_eq!(g.body(ann), STANDARD);
    assert!(
        g.told(ann)
            .iter()
            .any(|t| t.contains("does not let you be Horse"))
    );
    g.cmd(ann, Command::Suicide).unwrap();
    g.steps(3 * 120 + 60);
    g.cmd(ann, Command::Respawn).unwrap();
    g.steps(20);
    assert!(g.s.vitals()[&ann].alive);
    assert_eq!(g.body(ann), STANDARD, "the mode keeps a picked body");
    assert!(g.carries(ann, CARBINE));
    // The builder of a brick decides its event, as in v20: any body.
    let horse = g.class_brick(ann, [3.5, 0.1, 0.25], "HorseArmor");
    g.click(horse, ann);
    assert_eq!(g.body(ann), "v20.player.horsearmor");
}

/// Max's evening: Ann picks a body from a class brick while building,
/// opens a mini-game from the dialog and keeps it; Bo connects later,
/// joins, and picks another body from a brick inside the game. Both
/// respawn as what they picked, so two bodies (Mario and Master Chief, in
/// his words; the Runner and the Trooper here) fight in one game. A game
/// that turns `keep_chosen_body` off spawns everyone as its player type.
#[test]
fn bodies_picked_from_bricks_coexist_in_a_dialog_mini_game() {
    let mut g = Game::new();
    let ann = g.join("Ann");
    let runner = g.class_brick(ann, [3.5, 0.1, 0.25], RUNNER);
    let trooper = g.class_brick(ann, [-3.5, 0.1, 0.25], TROOPER);
    g.click(runner, ann);
    assert_eq!(g.body(ann), RUNNER);
    let dialog = bri_minigames::Settings {
        loadout: Default::default(),
        ..Default::default()
    };
    assert!(dialog.keep_chosen_body, "on unless a game turns it off");
    g.cmd(
        ann,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: dialog.clone(),
        }),
    )
    .unwrap();
    g.steps(20);
    let game = g.s.minigame_views()[0].id;
    assert_eq!(g.s.vitals()[&ann].minigame, Some(game));
    assert_eq!(g.body(ann), RUNNER, "kept the body picked before the game");
    let bo = g.join("Bo");
    g.cmd(bo, Command::MiniGame(MiniGameRequest::Join { game }))
        .unwrap();
    g.steps(20);
    assert_eq!(g.s.vitals()[&bo].minigame, Some(game));
    assert_eq!(g.body(bo), STANDARD, "the game's player type");
    g.click(trooper, bo);
    assert_eq!(g.body(bo), TROOPER);
    let respawn = |g: &mut Game, owner| {
        g.cmd(owner, Command::Suicide).unwrap();
        g.steps(200);
        g.cmd(owner, Command::Respawn).unwrap();
        g.steps(20);
        assert!(g.s.vitals()[&owner].alive);
    };
    for _ in 0..2 {
        respawn(&mut g, ann);
        respawn(&mut g, bo);
        assert_eq!(g.body(ann), RUNNER);
        assert_eq!(g.body(bo), TROOPER);
    }
    // The dialog's edits keep the rule.
    g.cmd(
        ann,
        Command::MiniGame(MiniGameRequest::Configure {
            settings: bri_minigames::Settings {
                keep_chosen_body: false,
                ..dialog.clone()
            },
        }),
    )
    .unwrap();
    g.steps(20);
    respawn(&mut g, bo);
    assert_eq!(g.body(bo), TROOPER);
    // A game that sets its own bodies: everyone spawns as its player type,
    // and leaving it does not bring the pick back.
    g.cmd(bo, Command::MiniGame(MiniGameRequest::Leave))
        .unwrap();
    g.steps(20);
    g.cmd(
        bo,
        Command::MiniGame(MiniGameRequest::Create {
            color: 1,
            settings: bri_minigames::Settings {
                keep_chosen_body: false,
                ..dialog
            },
        }),
    )
    .unwrap();
    g.steps(20);
    respawn(&mut g, bo);
    assert_eq!(g.body(bo), STANDARD);
    g.cmd(bo, Command::MiniGame(MiniGameRequest::Leave))
        .unwrap();
    g.steps(20);
    respawn(&mut g, bo);
    assert_eq!(g.body(bo), STANDARD);
}

/// Max's Goomba as a player type: in the Playground's match a Runner
/// holding the Carbine becomes the Stomper (the enemy's own body, from its
/// own Add-On) from a class brick, keeps the Carbine and a full share of
/// its smaller health, becomes a Stomper again in the buggy's driving seat
/// with `/body`, and respawns as the body it picked last.
#[test]
fn a_runner_becomes_a_stomper_drives_the_buggy_and_respawns_as_one() {
    let mut g = Game::server(&MODE, true, false);
    let ann = g.join("Ann");
    assert_eq!(g.body(ann), RUNNER);
    g.steps(300);
    g.equip(ann, CARBINE);
    let stomper = g.class_brick(ann, [6.5, 0.1, 8.25], STOMPER);
    g.click(stomper, ann);
    assert_eq!(g.body(ann), STOMPER);
    assert_eq!(g.held(ann).as_deref(), Some(CARBINE), "still holding it");
    assert!((g.health(ann) - 40.0).abs() < 1e-3, "{}", g.health(ann));
    g.slash_body(ann, "Runner");
    assert_eq!(g.body(ann), RUNNER);
    let buggy =
        g.s.spawn_vehicle_at(ann, BUGGY, Vec3::new(0.0, 1.0, 0.0), 0.0, Vec3::ZERO)
            .unwrap();
    g.steps(60);
    assert_eq!(g.board(ann).0, buggy);
    let seat = g.s.mounted(ann);
    g.slash_body(ann, "Stomper");
    assert_eq!(g.body(ann), STOMPER, "the mode lets players be a Stomper");
    assert_eq!(g.s.mounted(ann), seat, "a Stomper drives the buggy");
    g.cmd(ann, Command::Suicide).unwrap();
    g.steps(3 * 120 + 60);
    g.cmd(ann, Command::Respawn).unwrap();
    g.steps(20);
    assert!(g.s.vitals()[&ann].alive);
    assert_eq!(g.body(ann), STOMPER, "respawned as the body picked last");
    assert!(g.carries(ann, CARBINE));
}

/// Two players switch independently mid-match: one never sees the other's
/// choice, and the bot-free mode keeps both alive and armed.
#[test]
fn two_players_switch_independently_mid_match() {
    let mut g = Game::server(&MODE, true, false);
    let ann = g.join("Ann");
    let bo = g.join("Bo");
    g.steps(300);
    g.slash_body(ann, "Standard Player");
    assert_eq!(g.body(ann), STANDARD);
    assert_eq!(g.body(bo), RUNNER);
    g.slash_body(bo, "Standard Player");
    g.slash_body(ann, "Runner");
    assert_eq!(g.body(ann), RUNNER);
    assert_eq!(g.body(bo), STANDARD);
}

/// 150 ms each way, in 120 Hz steps.
const LATENCY: u64 = 18;

/// A predicting client while the host switches its body three times in the
/// air, between bodies with and without movement
/// controllers. The client learns of each switch from the host's pose: that
/// one pose corrects what it predicted with the old body, and every pose
/// after it agrees with the client's replay in the new body, so the switch
/// costs one blended correction, never a stream of them.
#[test]
fn a_predicting_client_follows_each_switch_with_one_correction() {
    let mut g = Game::new();
    let owner = g.s.join("Ann".into(), SPAWN, false).unwrap();
    let mut table: bri_sim::archetype::Archetypes =
        serde_json::from_slice(&serde_json::to_vec(g.s.archetypes()).unwrap()).unwrap();
    assert!(bri_sim::archetype::bind_controllers(&mut table).is_empty());
    let first =
        g.s.motion_states()
            .into_iter()
            .find(|(p, _)| p.owner == owner)
            .unwrap()
            .0;
    let mirror = bri_sim::prediction::CollisionMirror::new(definitions(), map(), vec![]);
    let start = first.archetype;
    let mut client = bri_sim::prediction::Predictor::new(mirror, first, table).unwrap();
    // Runs ahead, jumping each half second; the host switches body mid-air
    // (`/body` allows one switch a half second).
    let input = |tick: u64| MoveInput {
        forward: 1.0,
        yaw: 0.3,
        jump: tick % 60 < 4,
        ..Default::default()
    };
    let switches = [(30, KICKER), (100, TROOPER), (170, STANDARD)];
    let last_input = 200;
    let mut to_server = std::collections::VecDeque::new();
    let mut to_client = std::collections::VecDeque::new();
    let mut seen = start;
    let (mut switch_corrections, mut worst_between, mut airborne) = (0, 0.0_f32, 0);
    for tick in 1..=last_input + 3 * LATENCY {
        if tick <= last_input {
            client.step(input(tick)).unwrap();
            let recent: Vec<_> = client.recent(6).map(|(_, i)| *i).collect();
            to_server.push_back((tick + LATENCY, client.sequence(), recent));
        }
        while to_server.front().is_some_and(|(due, ..)| *due <= tick) {
            let (_, newest, inputs): (u64, u64, Vec<MoveInput>) = to_server.pop_front().unwrap();
            let first = newest + 1 - inputs.len() as u64;
            for (n, input) in inputs.into_iter().enumerate() {
                g.s.movement(owner, first + n as u64, input).unwrap();
            }
        }
        if let Some((_, body)) = switches.iter().find(|(at, _)| *at == tick) {
            let id = g.s.archetypes().find(body).unwrap();
            assert_ne!(id, client.state().archetype);
            let host = g.state(owner);
            airborne += usize::from(!host.grounded);
            g.slash_body_now(owner, body);
        }
        g.s.step().unwrap();
        if tick % 3 == 0 {
            let (p, ack) =
                g.s.motion_states()
                    .into_iter()
                    .find(|(p, _)| p.owner == owner)
                    .unwrap();
            to_client.push_back((tick + LATENCY, tick, ack, p));
        }
        while to_client.front().is_some_and(|(due, ..)| *due <= tick) {
            let (_, server_tick, ack, p) = to_client.pop_front().unwrap();
            let switched = p.archetype != seen;
            seen = p.archetype;
            if let Some(offset) = client
                .reconcile(server_tick, ack, p, Default::default())
                .unwrap()
            {
                if switched {
                    switch_corrections += 1;
                } else if ack > 15 {
                    worst_between = worst_between.max(offset.length());
                }
            }
        }
    }
    assert!(airborne >= 2, "switches mid-air: {airborne}");
    assert_eq!(switch_corrections, switches.len());
    assert!(
        worst_between < 1e-3,
        "between switches the client predicts its body exactly: {worst_between}"
    );
    let (host, ack) =
        g.s.motion_states()
            .into_iter()
            .find(|(p, _)| p.owner == owner)
            .unwrap();
    assert_eq!(ack, last_input);
    client
        .reconcile(u64::MAX, ack, host.clone(), Default::default())
        .unwrap();
    assert_eq!(client.state(), &host, "it ends exactly where the host is");
    assert_eq!(g.s.archetypes().resolve(host.archetype).id, STANDARD);
}

/// The host tunes an Add-On body while the game runs: the table changes in
/// place (players of the body carry on with the new numbers), a client's
/// copy takes the tuned body, a bad change changes nothing, and only the
/// host may tune.
#[test]
fn the_host_tunes_a_body_live_and_clients_get_it() {
    let mut g = Game::new();
    let ann = g.join("Ann");
    let host = g.s.join("Host".into(), SPAWN, true).unwrap();
    g.steps(5);
    g.told(ann);
    let before = g.s.archetypes().clone();
    let index = before.find(RUNNER).unwrap();
    // A player may not.
    let refused =
        g.s.tune_archetype(ann, RUNNER, &[("movement.forward".into(), 20.0)]);
    assert!(
        refused.unwrap_err().to_string().contains("Only the host"),
        "a player cannot tune"
    );
    // The host tunes speed and the right hand together.
    g.s.tune_archetype(
        host,
        RUNNER,
        &[
            ("movement.forward".into(), 20.0),
            ("sockets.right_hand.offset.1".into(), 0.125),
        ],
    )
    .unwrap();
    let tuned = g.s.archetypes().resolve(index).clone();
    assert_eq!(tuned.movement.forward, 20.0);
    assert_eq!(tuned.sockets.hand(0).unwrap().offset[1], 0.125);
    assert_eq!(g.s.tuned_revision(), 1);
    let file = &g.s.tuned_archetype_files()[RUNNER];
    assert_eq!(file["movement"]["forward"], serde_json::json!(20.0));
    // Everything else is as it was.
    assert_eq!(
        tuned.movement.jump_speed,
        before.resolve(index).movement.jump_speed
    );
    assert_eq!(tuned.look, before.resolve(index).look);
    // A client's table takes it.
    let mut client = before.clone();
    bri_sim::session::apply_tuned(&mut client, g.s.tuned_archetypes()).unwrap();
    assert_eq!(client.resolve(index), &tuned);
    // A change the body refuses changes nothing.
    for bad in [
        ("movement.forward", f32::NAN),
        ("controller", 1.0),
        ("max_time_scale", 100.0),
    ] {
        assert!(
            g.s.tune_archetype(host, RUNNER, &[(bad.0.into(), bad.1)])
                .is_err(),
            "{bad:?}"
        );
    }
    assert_eq!(g.s.archetypes().resolve(index), &tuned);
    assert_eq!(g.s.tuned_revision(), 1);
    // `/tune` from chat says why it refused.
    g.cmd(
        ann,
        Command::Package(PackageCommand {
            package: String::new(),
            command: "tune".into(),
            args: vec![
                PackageArg::String(RUNNER.into()),
                PackageArg::String("movement.forward".into()),
                PackageArg::Float(3.0),
            ],
        }),
    )
    .unwrap();
    assert!(
        g.told(ann).iter().any(|t| t.contains("Only the host")),
        "told why"
    );
    // A v20 body is not an Add-On's to tune.
    assert!(
        g.s.tune_archetype(
            host,
            "v20.player.playerstandardarmor",
            &[("movement.forward".into(), 9.0)]
        )
        .is_err()
    );
}
