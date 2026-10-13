//! The Mixed Add-On Playground (`packages/playground`), played headless
//! through the authoritative session. Five Add-Ons written as if by
//! different people (the Runner body, the Pulse Carbine, the Dune Buggy,
//! the Blockhead Bot and the Stomper) meet in one game mode, the only
//! package that names them. Nothing in the engine knows any of them: each test is ordinary
//! commands and movement, and a failure here is a cross-Add-On defect or
//! a missing seam, recorded in `docs/audits/interop-matrix.md`.
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::{Definition, Definitions},
    player::MoveInput,
    session::{Command, Session, ToolCatalog},
    simulation::Simulation,
};
use bri_world::{Brick, ContentRef, OwnerId, VehicleSpawn, World};
use glam::Vec3;
use rapier3d::prelude::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Arc,
};

const MODE: &str = "playground-mode:mode/playground";
const RUNNER: &str = "playground-runner:archetype/runner";
const CARBINE: &str = "playground-rifle:weapon/carbine";
const BUGGY: &str = "playground-buggy:vehicle/buggy";
const BOT: &str = "bot.blockhead";
const STOMPER: &str = "playground-stomper:bot/stomper";
const STOMPER_BODY: &str = "playground-stomper:archetype/stomper";

/// The Add-Ons, by id: their folder under `packages/` and side.
const ADD_ONS: [(&str, &str, Side); 10] = [
    (
        "playground-runner",
        "playground/playground-runner",
        Side::Server,
    ),
    (
        "playground-runner-look",
        "playground/playground-runner-look",
        Side::Client,
    ),
    (
        "playground-runner-hud",
        "playground/playground-runner-hud",
        Side::Client,
    ),
    ("sample-trooper", "samples/sample-trooper", Side::Server),
    (
        "playground-rifle",
        "playground/playground-rifle",
        Side::Shared,
    ),
    (
        "playground-buggy",
        "playground/playground-buggy",
        Side::Shared,
    ),
    ("blockhead_bot", "blockhead_bot", Side::Shared),
    (
        "playground-stomper",
        "playground/playground-stomper",
        Side::Shared,
    ),
    (
        "playground-stomper-look",
        "playground/playground-stomper-look",
        Side::Client,
    ),
    (
        "playground-mode",
        "playground/playground-mode",
        Side::Server,
    ),
];

fn packages() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages")
}

fn set(ids: &[&str]) -> PackageSet {
    PackageSet {
        schema_version: 1,
        packages: ADD_ONS
            .iter()
            .filter(|(id, _, _)| ids.contains(id))
            .map(|(id, dir, side)| PackageEntry {
                id: (*id).into(),
                version: "1.0.0".into(),
                side: *side,
                dir: (*dir).into(),
                role: None,
            })
            .collect(),
    }
}

/// Every Add-On, narrowed to the mode as a host starting it would be, or
/// all of them on a plain building server.
fn catalog(mode: bool) -> Arc<Catalog> {
    let ids: Vec<&str> = ADD_ONS.iter().map(|(id, _, _)| *id).collect();
    let mut catalog =
        Catalog::load(&packages(), &set(&ids), true).unwrap_or_else(|e| panic!("{e:#?}"));
    if mode {
        let mode = catalog.for_mode(MODE).unwrap_or_else(|e| panic!("{e:#?}"));
        catalog.mode = mode.mode;
    }
    Arc::new(catalog)
}

/// The carbine's weapons and the buggy's gun, merged as a host merges
/// every package's.
fn weapons() -> bri_weapons::Pack {
    let load = |dir: &str| {
        let path = packages().join(dir).join("weapons.json");
        bri_weapons::Pack::from_json(&std::fs::read(path).unwrap()).unwrap()
    };
    let (pack, problems) = load("playground/playground-rifle/assets").merge(vec![(
        "playground/playground-buggy/assets".into(),
        load("playground/playground-buggy/assets"),
    )]);
    assert!(problems.is_empty(), "{problems:?}");
    pack
}

/// Each Carbine image's muzzle in the hand's frame, from its own model,
/// as a host computes it.
fn barrels() -> BTreeMap<String, [f32; 3]> {
    let dir = packages().join("playground/playground-rifle/assets");
    weapons()
        .images
        .iter()
        .map(|(id, image)| {
            let shape: bri_content::shape::Shape =
                serde_json::from_slice(&std::fs::read(dir.join(&image.model)).unwrap()).unwrap();
            let barrel = bri_weapons::muzzle::barrel(
                image.offset,
                image.source_rotation_degrees,
                Some(&shape),
            );
            (id.clone(), barrel.to_array())
        })
        .collect()
}

fn vehicles() -> bri_vehicles::Pack {
    bri_vehicles::Pack::load(packages().join("playground/playground-buggy/assets/vehicles.json"))
        .unwrap()
}

/// The Blockhead Bot's kinds and the Stomper's, merged as a host merges
/// every package's.
fn bot_kinds() -> Vec<bri_sim::bot_kind::BotKind> {
    let load = |file: &str| {
        bri_sim::bot_kind::BotPack::from_json(&std::fs::read(packages().join(file)).unwrap())
            .unwrap()
    };
    bri_sim::bot_kind::BotPack::merge([
        load("blockhead_bot/assets/bots.json"),
        load("playground/playground-stomper/bots.json"),
    ])
    .unwrap()
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

/// The fort's front wall: plates 12 wide and 8 high, across the buggy's
/// path 16 ahead of the spawn, facing it.
const WALL_Z: f32 = -16.25;
const SPAWN: Vec3 = Vec3::new(0.0, 0.05, 8.0);
fn fort(world: &mut World) {
    for row in 0..8 {
        for column in 0..12 {
            let id = world.next_brick_id;
            world.bricks.insert(
                id,
                Brick::new(
                    ContentRef::Resolved("plate".into()),
                    [column as f32 - 5.5, 0.1 + 0.2 * row as f32, WALL_Z],
                    0,
                ),
            );
            world.next_brick_id += 1;
        }
    }
}

/// A Vehicle Spawn brick holding a Blockhead Bot, off to the side.
fn bot_spawn(world: &mut World, at: [f32; 3]) {
    kind_spawn(world, at, BOT);
}

/// A Vehicle Spawn brick holding a bot of `kind`.
fn kind_spawn(world: &mut World, at: [f32; 3], kind: &str) {
    let id = world.next_brick_id;
    let mut brick = Brick::new(ContentRef::Resolved("plate".into()), at, 0);
    brick.vehicle = Some(Box::new(VehicleSpawn {
        vehicle: ContentRef::Resolved(kind.into()),
        recolor: false,
        team: None,
    }));
    world.bricks.insert(id, brick);
    world.next_brick_id += 1;
}

struct Game {
    s: Session,
    seq: BTreeMap<OwnerId, u64>,
    moves: BTreeMap<OwnerId, u64>,
    looks: BTreeMap<OwnerId, MoveInput>,
}
impl Game {
    fn with(world: World) -> Self {
        Self::server(world, true)
    }
    /// A server running every Add-On, in the mode or (`mode` false) as a
    /// plain building server with no mini-game.
    fn server(world: World, mode: bool) -> Self {
        let mut s = Session::new(
            Simulation::new(
                world,
                definitions(),
                vec![
                    ColliderBuilder::cuboid(200.0, 0.5, 200.0)
                        .translation(Vector::new(0.0, -0.5, 0.0)),
                ],
            )
            .unwrap(),
        );
        s.set_weapon_pack(weapons()).unwrap();
        // A host measures these from the item's model as it loads content.
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
        s.set_vehicle_pack(vehicles(), bot_kinds()).unwrap();
        s.set_tool_catalog(ToolCatalog {
            items: [CARBINE.to_string()].into(),
            vehicles: [BUGGY.to_string(), BOT.to_string(), STOMPER.to_string()].into(),
            vehicle_bricks: ["plate".to_string()].into(),
            ..Default::default()
        })
        .unwrap();
        // What a host learns at setup (`HostSetup`): each image's barrel
        // and the Blockhead's hands (here the made-up test rig's), so
        // shots leave the muzzle in each body's hand.
        s.set_image_barrels(barrels()).unwrap();
        s.set_body_grips(
            "v20.shape.m",
            bri_sim::session::shape_grips(&bri_content::testing::avatar::rig(), &[]),
        )
        .unwrap();
        s.install_packages(catalog(mode), None).unwrap();
        // The mode's mini-game spawns everyone here, behind where the
        // tests park the buggy.
        s.set_spawn_points(vec![SPAWN]).unwrap();
        Self {
            s,
            seq: BTreeMap::new(),
            moves: BTreeMap::new(),
            looks: BTreeMap::new(),
        }
    }
    fn new() -> Self {
        let mut world = World::new("Playground".into(), "playground".into(), vec![[1.0; 4]]);
        fort(&mut world);
        Self::with(world)
    }
    /// Joins `name`; the mode's mini-game spawns them at a spawn point.
    fn join(&mut self, name: &str) -> OwnerId {
        let owner = self.s.join(name.into(), SPAWN, false).unwrap();
        self.looks.insert(owner, MoveInput::default());
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
    /// Feeds `input` to `owner` for `ticks`, the others standing still.
    fn drive(&mut self, owner: OwnerId, input: MoveInput, ticks: usize) {
        let before = self.looks.insert(owner, input).unwrap();
        self.steps(ticks);
        // Keep the look, drop the keys.
        self.looks.insert(
            owner,
            MoveInput {
                yaw: input.yaw,
                pitch: input.pitch,
                ..before
            },
        );
    }
    fn state(&self, owner: OwnerId) -> bri_sim::player::PlayerState {
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
    /// Takes `item` out, as pressing its slot's number does.
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
    fn fire(&mut self, owner: OwnerId) -> usize {
        let before = self.s.weapon_view().fired().count();
        self.cmd(owner, Command::WeaponTrigger { down: true })
            .unwrap();
        self.steps(2);
        let fired = self.s.weapon_view().fired().count() - before;
        self.cmd(owner, Command::WeaponTrigger { down: false })
            .unwrap();
        self.steps(30);
        fired
    }
    /// Run at the vehicle, hopping, until mounted (v20 mounts from above).
    fn board(&mut self, owner: OwnerId, yaw: f32) -> (u64, u8) {
        for _ in 0..30 {
            self.drive(
                owner,
                MoveInput {
                    forward: -1.0,
                    yaw,
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
                        yaw,
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
    /// Bricks still standing: a knocked-out brick stays in the world,
    /// unseen and passable, until it comes back.
    fn bricks(&self) -> usize {
        self.s
            .simulation()
            .state()
            .bricks
            .values()
            .filter(|b| b.visible && b.colliding && b.raycast)
            .count()
    }
}

/// Each Add-On but the mode loads on its own, with only its declared
/// dependencies, and none of them names another: no file of the Runner,
/// its look, the carbine, the buggy or the bot mentions another's id.
#[test]
fn each_add_on_stands_alone_and_names_no_other() {
    let independent = [
        &["playground-runner", "playground-runner-look"][..],
        &["playground-runner-look"],
        &[
            "playground-runner",
            "playground-runner-look",
            "playground-runner-hud",
        ],
        &["playground-rifle"],
        &["playground-buggy"],
        &["blockhead_bot"],
    ];
    for ids in independent {
        Catalog::load(&packages(), &set(ids), true)
            .unwrap_or_else(|e| panic!("{ids:?} alone: {e:#?}"));
    }
    // The runner and its look are one author's pair.
    let families = [
        (
            "playground-runner",
            &["playground-runner", "playground-runner-look"][..],
        ),
        (
            "playground-runner-look",
            &["playground-runner", "playground-runner-look"],
        ),
        // The HUD is drawn for the Runner's body, which it depends on.
        (
            "playground-runner-hud",
            &[
                "playground-runner",
                "playground-runner-look",
                "playground-runner-hud",
            ],
        ),
        ("playground-rifle", &["playground-rifle"]),
        ("playground-buggy", &["playground-buggy"]),
        ("blockhead_bot", &["blockhead_bot"]),
    ];
    fn texts(dir: &Path, out: &mut Vec<(PathBuf, String)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                texts(&path, out);
            } else if let Ok(text) = std::fs::read_to_string(&path) {
                out.push((path, text));
            }
        }
    }
    for (id, own) in families {
        let dir = ADD_ONS.iter().find(|(i, _, _)| *i == id).unwrap().1;
        let mut files = Vec::new();
        texts(&packages().join(dir), &mut files);
        assert!(!files.is_empty(), "{id} has files");
        for (other, _, _) in ADD_ONS.iter().filter(|(o, _, _)| !own.contains(o)) {
            for (path, text) in &files {
                assert!(!text.contains(other), "{} names {other}", path.display());
            }
        }
    }
}

/// The mode puts everyone in its mini-game as the Runner, carbine in hand:
/// the body is the Runner Add-On's (short, no jet, its eye under the
/// Blockhead's), and the carbine Add-On's trigger fires.
#[test]
fn a_player_joins_as_the_runner_holding_the_carbine_and_fires() {
    let mut g = Game::new();
    let p = g.join("Ann");
    g.steps(40);
    let games = g.s.minigame_views();
    assert_eq!(games.len(), 1);
    assert_eq!(g.s.vitals()[&p].minigame, Some(games[0].id));
    assert_eq!(g.body(p), RUNNER);
    let runner = g.s.archetypes().resolve(g.state(p).archetype);
    assert!(!runner.movement.can_jet);
    let eye = g.s.archetypes().eye(&g.state(p)).y;
    assert!(eye < 1.6, "the Runner sees from its own height: {eye}");
    // The mode's loadout puts it in the inventory; as in v20, nothing is
    // in hand until the player takes it out.
    assert!(g.carries(p, CARBINE));
    g.equip(p, CARBINE);
    assert_eq!(g.held(p).as_deref(), Some(CARBINE));
    // Past spawn protection, then a shot.
    g.steps(300);
    assert_eq!(g.fire(p), 1, "the carbine fires");
    assert!(
        g.s.package_diagnostics().is_empty(),
        "{:?}",
        g.s.package_diagnostics()
    );
}

/// The Runner boards the buggy into the driver's seat, keeps the carbine,
/// drives it into the fort's wall fast enough to knock bricks out (the
/// mode allows brick damage), and climbs out still the Runner with the
/// carbine.
#[test]
fn the_runner_drives_the_buggy_through_the_fort_and_climbs_out() {
    let mut g = Game::new();
    let p = g.join("Ann");
    g.steps(40);
    let buggy =
        g.s.spawn_vehicle_at(p, BUGGY, Vec3::new(0.0, 1.0, 0.0), 0.0, Vec3::ZERO)
            .unwrap();
    g.steps(60);
    // From behind, facing -z.
    let (vehicle, seat) = g.board(p, 0.0);
    assert_eq!((vehicle, seat), (buggy, 0), "the driver's seat");
    assert_eq!(g.body(p), RUNNER, "still the Runner while seated");
    assert!(g.carries(p, CARBINE), "the carbine stays in the inventory");
    let built = g.bricks();
    // Full throttle at the wall.
    g.drive(
        p,
        MoveInput {
            forward: 1.0,
            ..Default::default()
        },
        240,
    );
    let at =
        g.s.vehicle_infos()
            .into_iter()
            .find(|v| v.id == buggy)
            .expect("the buggy is still there");
    assert_eq!(at.occupants[0], Some(p), "still driving");
    let left = g.bricks();
    assert!(
        left < built,
        "the buggy knocked bricks out of the wall: {built} -> {left}"
    );
    // Jet climbs out.
    g.drive(
        p,
        MoveInput {
            jet: true,
            ..Default::default()
        },
        2,
    );
    g.steps(10);
    assert_eq!(g.s.mounted(p), None, "climbed out");
    assert!(g.s.vitals()[&p].alive);
    assert_eq!(g.body(p), RUNNER);
    assert!(g.carries(p, CARBINE));
}

/// The passenger's seat lets its rider use their weapon: a second Runner
/// takes it and fires the carbine from the moving buggy.
#[test]
fn a_passenger_fires_the_carbine_and_a_gunner_the_buggy_gun() {
    let mut g = Game::new();
    let driver = g.join("Ann");
    g.steps(40);
    let buggy =
        g.s.spawn_vehicle_at(driver, BUGGY, Vec3::new(0.0, 1.0, 0.0), 0.0, Vec3::ZERO)
            .unwrap();
    g.steps(60);
    assert_eq!(g.board(driver, 0.0), (buggy, 0));
    let passenger = g.join("Bo");
    g.steps(40);
    assert_eq!(g.board(passenger, 0.0), (buggy, 1), "the passenger's seat");
    g.steps(300);
    g.equip(passenger, CARBINE);
    let held = g.held(passenger);
    assert_eq!(held.as_deref(), Some(CARBINE), "held while seated");
    g.looks.get_mut(&driver).unwrap().forward = 1.0;
    g.steps(30);
    assert_eq!(g.fire(passenger), 1, "fires from the passenger's seat");
    assert_eq!(g.s.mounted(passenger), Some((buggy, 1)));
    // A third climbs onto the deck and works the buggy's own gun, which
    // fires the buggy Add-On's bolt, not anything in their hands.
    // The driver lets off and brakes to a stop (jump brakes).
    let look = g.looks.get_mut(&driver).unwrap();
    look.forward = 0.0;
    look.jump = true;
    g.steps(240);
    let gunner = g.join("Cy");
    g.steps(40);
    let (vehicle, seat) = g.board(gunner, 0.0);
    assert_eq!((vehicle, seat), (buggy, 2), "the gunner's post");
    g.steps(60);
    // One bolt a click, as v20's turrets fire, a click per cooldown.
    let before = g.s.weapon_view().fired().count();
    for _ in 0..3 {
        g.cmd(gunner, Command::WeaponTrigger { down: true })
            .unwrap();
        g.steps(2);
        g.cmd(gunner, Command::WeaponTrigger { down: false })
            .unwrap();
        g.steps(20);
    }
    let bolts = g.s.weapon_view().fired().count() - before;
    assert_eq!(bolts, 3, "a bolt for each click");
    assert_eq!(g.s.mounted(gunner), Some((buggy, 2)));
}

/// Two Runners in the mode's mini-game: carbine fire kills, the dead
/// player comes back as the Runner with the carbine, and nothing is lost
/// or doubled in either inventory.
#[test]
fn a_runner_killed_by_the_carbine_respawns_as_the_runner_with_it() {
    let mut g = Game::new();
    let a = g.join("Ann");
    g.s.set_spawn_points(vec![Vec3::new(0.0, 0.05, -10.0)])
        .unwrap();
    let b = g.join("Bo");
    g.steps(330);
    g.equip(a, CARBINE);
    let eye = g.s.archetypes().eye(&g.state(a));
    let target = Vec3::from(g.state(b).feet) + Vec3::Y * 1.0;
    let to = (target - eye).normalize();
    let look = g.looks.get_mut(&a).unwrap();
    look.pitch = to.y.asin();
    look.yaw = to.x.atan2(-to.z);
    g.steps(5);
    let mut shots = 0;
    while g.s.vitals()[&b].alive && shots < 10 {
        g.fire(a);
        shots += 1;
    }
    assert!(!g.s.vitals()[&b].alive, "four 25s drop a 100-health Runner");
    // After the mode's 3 s, a click respawns them, as in v20.
    g.steps(3 * 120 + 60);
    g.cmd(b, Command::Respawn).unwrap();
    g.steps(10);
    assert!(g.s.vitals()[&b].alive, "respawned");
    assert_eq!(g.body(b), RUNNER);
    let slots = &g.s.tool_inventories()[&b].slots;
    let carbines = slots
        .iter()
        .filter(|s| s.as_deref() == Some(CARBINE))
        .count();
    assert_eq!(carbines, 1, "one carbine after respawn: {slots:?}");
    assert_eq!(
        g.s.tool_inventories()[&a]
            .slots
            .iter()
            .filter(|s| s.as_deref() == Some(CARBINE))
            .count(),
        1
    );
}

/// A Blockhead Bot from its own Add-On, spawned in the Playground world,
/// fights the Runners: within a minute it hurts one of them, using the
/// carbine the mode hands out or its own.
#[test]
fn a_blockhead_bot_fights_the_runners() {
    let mut world = World::new("Playground".into(), "playground".into(), vec![[1.0; 4]]);
    fort(&mut world);
    bot_spawn(&mut world, [12.0, 0.1, -6.25]);
    let mut g = Game::with(world);
    // The mode's mini-game respawns the bot at a spawn point too. Sharing
    // the player's one, it would start inside them and never move (see
    // docs/audits/interop-matrix.md); a second keeps them apart.
    g.s.set_spawn_points(vec![SPAWN, Vec3::new(12.0, 0.05, -12.0)])
        .unwrap();
    let p = g.join("Ann");
    g.steps(60);
    let bots: BTreeSet<OwnerId> =
        g.s.names()
            .keys()
            .copied()
            .filter(|o| g.s.is_bot(*o))
            .collect();
    assert_eq!(bots.len(), 1, "the spawn brick made a bot");
    let bot = *bots.iter().next().unwrap();
    let game = g.s.vitals()[&p].minigame;
    assert_eq!(
        g.s.vitals()[&bot].minigame,
        game,
        "the bot is in the mode's mini-game"
    );
    let mut hurt = false;
    for _ in 0..60 {
        g.steps(120);
        let v = &g.s.vitals()[&p];
        if v.health < 100.0 || !v.alive {
            hurt = true;
            break;
        }
    }
    assert!(hurt, "the bot hurt the Runner within a minute");
}

/// Pulls `owner`'s trigger and returns the shot as it leaves, before it
/// can hit anything.
fn shoot(g: &mut Game, owner: OwnerId) -> bri_weapons::Projectile {
    let before: BTreeSet<u64> = g.s.weapon_view().fired().map(|s| s.id).collect();
    g.cmd(owner, Command::WeaponTrigger { down: true }).unwrap();
    let mut shot = None;
    for _ in 0..4 {
        g.steps(1);
        shot = shot.or_else(|| {
            g.s.weapon_view()
                .fired()
                .find(|p| p.source.0 == owner && !before.contains(&p.id))
                .cloned()
        });
    }
    g.cmd(owner, Command::WeaponTrigger { down: false })
        .unwrap();
    g.steps(30);
    shot.expect("a shot")
}

/// v0.3.0 W2: the host fired every shot from the eye. The Runner's shot
/// now leaves the Carbine's muzzle in its own right hand (its `sockets`,
/// the Carbine's `muzzlePoint`), well away from its eye, and still flies
/// at what the crosshair is on (`correct_muzzle`).
#[test]
fn the_runners_shot_leaves_the_carbines_muzzle_in_its_hand_and_meets_the_crosshair() {
    let mut g = Game::new();
    let p = g.join("Ann");
    g.steps(40);
    g.equip(p, CARBINE);
    g.steps(300);
    let state = g.state(p);
    let eye = g.s.archetypes().eye(&state);
    let shot = shoot(&mut g, p);
    // Facing -Z: the right hand (0.5, 0.74, -0.14) holding the barrel
    // (0, 0.14, -0.88).
    let muzzle = Vec3::from(state.feet) + Vec3::new(0.5, 0.88, -1.02);
    assert!(
        shot.origin.distance(muzzle) < 1e-3,
        "{} from {muzzle}",
        shot.origin
    );
    assert!(shot.origin.distance(eye) > 1.0, "not from the eye");
    // The crosshair is on the fort's wall straight ahead of the eye.
    let aim = Vec3::new(eye.x, eye.y, WALL_Z + 0.25);
    let along = shot.velocity.normalize();
    let to = aim - shot.origin;
    let miss = (to - along * to.dot(along)).length();
    assert!(miss < 0.05, "passes {miss} from where the crosshair is");
}

/// A Carbine pushed into a wall fires from the eye, so a shot never
/// starts on the far side of what the shooter stands against: pressed to
/// the fort, the Runner's muzzle is in or past the wall, and the shot
/// hits the wall's near face instead of flying on behind it.
#[test]
fn a_carbine_pressed_into_a_wall_fires_from_the_eye() {
    let mut g = Game::new();
    let p = g.join("Ann");
    g.steps(40);
    g.equip(p, CARBINE);
    g.steps(300);
    g.drive(
        p,
        MoveInput {
            forward: 1.0,
            ..Default::default()
        },
        600,
    );
    let state = g.state(p);
    let muzzle_z = state.feet[2] - 1.02;
    assert!(
        muzzle_z < WALL_Z + 0.25,
        "the muzzle is in the wall: {muzzle_z}"
    );
    g.cmd(p, Command::WeaponTrigger { down: true }).unwrap();
    let mut fired = false;
    for _ in 0..40 {
        g.steps(1);
        fired |= g.s.weapon_view().images[&p]
            .iter()
            .any(|i| i.state.eq_ignore_ascii_case("fire"));
        let behind =
            g.s.weapon_view()
                .fired()
                .filter(|s| s.source.0 == p)
                .any(|s| s.position.z < WALL_Z);
        assert!(!behind, "a shot came out behind the wall");
        if fired {
            g.cmd(p, Command::WeaponTrigger { down: false }).unwrap();
        }
    }
    assert!(fired, "it fired");
}

/// The same Carbine in a different body: the Blockhead Bot (the made-up
/// rig's hands here) fires it from its own hand, not the Runner's grip
/// and not its eye.
#[test]
fn a_blockhead_bot_fires_the_carbine_from_its_own_hand() {
    let mut world = World::new("Playground".into(), "playground".into(), vec![[1.0; 4]]);
    fort(&mut world);
    bot_spawn(&mut world, [12.0, 0.1, -6.25]);
    let mut g = Game::with(world);
    g.s.set_spawn_points(vec![SPAWN, Vec3::new(12.0, 0.05, -12.0)])
        .unwrap();
    let p = g.join("Ann");
    let grips = bri_sim::session::shape_grips(&bri_content::testing::avatar::rig(), &[]);
    let hand = glam::Mat4::from_cols_array(&grips["mount0"]);
    let barrel = Vec3::from(barrels()[&weapons().items[CARBINE].image]);
    let mut checked = 0;
    for _ in 0..7200 {
        let before: BTreeSet<u64> = g.s.weapon_view().fired().map(|s| s.id).collect();
        let bot = g.s.names().keys().copied().find(|o| g.s.is_bot(*o));
        let at = bot.map(|b| (b, g.state(b)));
        g.steps(1);
        let (Some((bot, state)), Some(shot)) = (
            at,
            g.s.weapon_view()
                .fired()
                .find(|s| !before.contains(&s.id) && bot.is_some_and(|b| s.source.0 == b))
                .cloned(),
        ) else {
            continue;
        };
        assert_eq!(g.held(bot).as_deref(), Some(CARBINE));
        let tuning = g.s.archetypes().resolve(state.archetype).movement.clone();
        let grip = bri_weapons::muzzle::Grip {
            hand,
            pivot: Vec3::new(0.0, tuning.stand_eye, -tuning.eye_forward),
        };
        let eye = g.s.archetypes().eye(&state);
        let feet = Vec3::from(state.feet);
        let shift = glam::Quat::from_rotation_y(state.yaw) * (eye - feet) / state.scale
            - Vec3::new(0.0, tuning.stand_eye, -tuning.eye_forward);
        let muzzle = grip.muzzle(barrel, feet, state.yaw, state.pitch, state.scale, shift);
        // A wall in the way fires from the eye; in the open, the muzzle.
        if shot.origin.distance(eye) > 1e-3 {
            assert!(
                shot.origin.distance(muzzle) < 0.2,
                "{} from {muzzle}",
                shot.origin
            );
            checked += 1;
        }
        if checked >= 3 || !g.s.vitals()[&p].alive {
            break;
        }
    }
    assert!(checked > 0, "the bot fired the carbine from its hand");
}

/// A Stomper, a simple enemy from its own Add-On, is the shared bot brain
/// with most of it turned off by data alone: no weapons, objectives,
/// vehicles or teamwork. In the Stomper's own body it walks to a Runner
/// and stomps them with its body, its hands empty, and never fights
/// another Stomper.
#[test]
fn a_stomper_walks_up_and_stomps_a_runner_with_its_body() {
    let mut world = World::new("Playground".into(), "playground".into(), vec![[1.0; 4]]);
    kind_spawn(&mut world, [6.5, 0.1, 2.25], STOMPER);
    kind_spawn(&mut world, [-6.5, 0.1, 2.25], STOMPER);
    let mut g = Game::with(world);
    g.s.set_spawn_points(vec![
        SPAWN,
        Vec3::new(8.0, 0.05, 0.0),
        Vec3::new(-8.0, 0.05, 0.0),
    ])
    .unwrap();
    let p = g.join("Ann");
    g.steps(60);
    let bots: Vec<OwnerId> =
        g.s.names()
            .keys()
            .copied()
            .filter(|o| g.s.is_bot(*o))
            .collect();
    assert_eq!(bots.len(), 2, "each spawn brick made a Stomper");
    for &bot in &bots {
        assert_eq!(g.body(bot), STOMPER_BODY, "a Stomper plays in its own body");
    }
    let mut stomped = false;
    for _ in 0..60 {
        g.steps(120);
        for &bot in &bots {
            assert_eq!(g.held(bot), None, "a Stomper never takes up a weapon");
            let v = &g.s.vitals()[&bot];
            assert!(
                v.health >= 40.0 || !v.alive,
                "Stompers never fight each other"
            );
        }
        let v = &g.s.vitals()[&p];
        if v.health < 100.0 || !v.alive {
            stomped = true;
            break;
        }
    }
    assert!(stomped, "a Stomper stomped the Runner within a minute");
}

/// With no mode and no mini-game, a builder puts the Add-Ons in the world
/// as v20 does: the Pulse Carbine on an item spawn brick and the Dune Buggy
/// on a vehicle spawn brick, each picked with the wrench. A Blockhead walks
/// over the carbine to take it and drives the buggy away.
#[test]
fn a_builder_spawns_the_carbine_and_the_buggy_from_bricks_without_a_mini_game() {
    use bri_world::{
        ItemSpawn,
        authority::{Edit, WrenchProperties},
    };
    let mut g = Game::server(
        World::new("Sandbox".into(), "playground".into(), vec![[1.0; 4]]),
        false,
    );
    let builder = g.join("Builder");
    assert_eq!(g.s.vitals()[&builder].minigame, None, "no mini-game");
    let plant = |g: &mut Game, position: [f32; 3], properties: WrenchProperties| {
        g.cmd(
            builder,
            Command::Plant {
                definition: "plate".into(),
                position,
                quarter_turns: 0,
                color: 0,
            },
        )
        .unwrap();
        let id = *g.s.simulation().state().bricks.keys().max().unwrap();
        g.s.edit_brick(builder, id, Edit::Properties(properties))
            .unwrap_or_else(|e| panic!("{e:#}"));
        id
    };
    let visible = WrenchProperties {
        raycast: true,
        colliding: true,
        visible: true,
        ..Default::default()
    };
    let rack = [6.5, 0.1, 0.25];
    let rack = plant(
        &mut g,
        rack,
        WrenchProperties {
            item_spawn: ItemSpawn {
                item: Some(ContentRef::Resolved(CARBINE.into())),
                position: 0,
                direction: 2,
                respawn_ms: 5000,
                restore: Default::default(),
            },
            ..visible.clone()
        },
    );
    let pad = plant(
        &mut g,
        [-7.5, 0.1, 0.25],
        WrenchProperties {
            vehicle: Some(BUGGY.into()),
            ..visible
        },
    );
    g.steps(30);
    assert!(
        g.s.weapon_view()
            .static_items
            .iter()
            .any(|i| i.brick == rack),
        "the item brick shows the carbine"
    );
    let buggies = g.s.vehicle_poses();
    assert_eq!(buggies.len(), 1, "the vehicle brick spawned the buggy");
    let at = Vec3::from(buggies[0].position);
    let pad = Vec3::from(g.s.simulation().state().bricks[&pad].position);
    assert!(
        Vec3::new(at.x - pad.x, 0.0, at.z - pad.z).length() < 1.0,
        "on its pad: {at} over {pad}"
    );
    // A second player walks onto the rack and takes the carbine.
    let taker =
        g.s.join("Taker".into(), Vec3::new(6.5, 0.35, 0.25), false)
            .unwrap();
    g.looks.insert(taker, MoveInput::default());
    g.steps(5);
    assert!(g.carries(taker, CARBINE), "the Blockhead took the carbine");
    assert_ne!(g.body(taker), RUNNER, "no mode: the standard body");
}

/// The buggy steers toward where its driver looks (`toward_look`), so a
/// bot at the wheel steers it by looking, not by the mouse turn the v20
/// jeep takes: a Blockhead Bot boards the parked buggy and drives it at a
/// Runner far across the field, turning to do so.
#[test]
fn a_bot_drives_the_look_steered_buggy_at_a_runner() {
    let mut world = World::new("Playground".into(), "playground".into(), vec![[1.0; 4]]);
    bot_spawn(&mut world, [40.0, 0.1, 40.25]);
    let mut g = Game::with(world);
    g.s.set_spawn_points(vec![
        Vec3::new(0.0, 0.05, 30.0),
        Vec3::new(40.0, 0.05, 44.0),
    ])
    .unwrap();
    let p = g.join("Ann");
    g.steps(60);
    let bot =
        g.s.names()
            .keys()
            .copied()
            .find(|o| g.s.is_bot(*o))
            .unwrap();
    let buggy =
        g.s.spawn_vehicle_at(bot, BUGGY, Vec3::new(44.0, 1.0, 40.0), 1.2, Vec3::ZERO)
            .unwrap();
    let start =
        g.s.vehicle_poses()
            .into_iter()
            .find(|v| v.id == buggy)
            .unwrap();
    let heading = |r: [f32; 4]| {
        let f = glam::Quat::from_array(r) * Vec3::NEG_Z;
        f.x.atan2(-f.z)
    };
    let (mut drove, mut turned, mut closest) = (false, 0f32, f32::INFINITY);
    for _ in 0..90 {
        g.steps(120);
        let v =
            g.s.vehicle_poses()
                .into_iter()
                .find(|v| v.id == buggy)
                .unwrap();
        if g.s.mounted(bot) == Some((buggy, 0)) {
            drove = true;
            let d = (heading(v.rotation) - heading(start.rotation) + std::f32::consts::PI)
                .rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            turned = turned.max(d.abs());
            let ann = Vec3::from(g.state(p).feet);
            closest = closest.min(Vec3::from(v.position).distance(ann));
        }
        if closest < 20.0 {
            break;
        }
    }
    assert!(drove, "the bot took the wheel");
    assert!(turned > 0.5, "it turned toward the Runner: {turned}");
    // From 45 away to within the buggy gun's reach, where it may well
    // climb back to the gun rather than keep driving.
    assert!(closest < 20.0, "it drove at the Runner: {closest}");
}

/// Inside the mode's mini-game a buggy running into a Stomper hurts it as
/// it hurts a player: a bot is a run-over target like anyone else. (The
/// driving is `the_runner_drives_the_buggy_through_the_fort_and_climbs_out`;
/// that a driven run-over is the driver's kill is
/// `a_runover_is_the_drivers_kill_even_when_the_victim_walks_into_it`.)
#[test]
fn a_buggy_runs_a_stomper_over() {
    let mut world = World::new("Playground".into(), "playground".into(), vec![[1.0; 4]]);
    kind_spawn(&mut world, [6.5, 0.1, 2.25], STOMPER);
    let mut g = Game::with(world);
    let p = g.join("Ann");
    g.steps(20);
    let bot =
        g.s.names()
            .keys()
            .copied()
            .find(|o| g.s.is_bot(*o))
            .expect("the spawn brick made a Stomper");
    assert_eq!(g.body(bot), STOMPER_BODY);
    assert_eq!(
        g.s.vitals()[&bot].minigame,
        g.s.vitals()[&p].minigame,
        "the Stomper plays in the Runner's mini-game"
    );
    // A fresh spawn takes no damage for its first 2.5 s, so wait that out:
    // a run-over before then pushes the Stomper but cannot hurt it.
    g.steps(300);
    let full = g.s.vitals()[&bot].health;
    let mut hit = false;
    for _ in 0..5 {
        // Launch the Runner's buggy at the Stomper from a few metres off,
        // faster than a run-over needs, from its side away from the Runner
        // it is after, so the Runner is not the one in the way.
        let at = Vec3::from(g.state(bot).feet);
        let mut away = at - Vec3::from(g.state(p).feet);
        away.y = 0.0;
        let away = away.try_normalize().unwrap_or(Vec3::Z);
        let from = at + away * 5.0 + Vec3::Y;
        let yaw = away.x.atan2(away.z);
        g.s.spawn_vehicle_at(p, BUGGY, from, yaw, -away * 16.0)
            .unwrap();
        for _ in 0..90 {
            g.steps(1);
            let v = &g.s.vitals()[&bot];
            if v.health < full || !v.alive {
                hit = true;
                break;
            }
        }
        if hit {
            break;
        }
    }
    assert!(hit, "the buggy ran the Stomper over");
}

/// The Carbine's pulse leaves a mark where it hits the fort (its
/// projectile's `decal`): the host tells every player which brick and
/// which face, and the pack says how the mark looks.
#[test]
fn a_carbine_shot_marks_the_brick_it_hits() {
    let mut g = Game::new();
    let p = g.join("Ann");
    g.steps(40);
    g.equip(p, CARBINE);
    g.steps(300);
    g.s.take_cues();
    assert_eq!(g.fire(p), 1);
    let decals: Vec<_> =
        g.s.take_cues()
            .into_iter()
            .filter_map(|c| match c.kind {
                bri_sim::presentation::CueKind::Decal {
                    brick,
                    normal,
                    projectile,
                    scale,
                } => Some((brick, Vec3::from(normal), projectile, scale, c.position)),
                _ => None,
            })
            .collect();
    assert_eq!(decals.len(), 1, "one mark for one shot");
    let (brick, normal, projectile, scale, at) = &decals[0];
    assert!(
        g.s.simulation().state().bricks.contains_key(brick),
        "on a brick of the fort"
    );
    assert!(
        normal.abs_diff_eq(Vec3::Z, 1e-3),
        "on the face toward the shooter: {normal}"
    );
    assert!(
        (at[2] - (WALL_Z + 0.25)).abs() < 0.05,
        "on the wall's near face: {at:?}"
    );
    assert_eq!(projectile, "playground-rifle:projectile/pulse");
    assert_eq!(*scale, 1.0);
    assert!(
        weapons().projectiles[projectile.as_str()].decal.is_some(),
        "the pack says how it looks"
    );
}
