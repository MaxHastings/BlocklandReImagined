//! The engine seams a classic throw Add-On needs (Electrk's Player Throwing,
//! Nobot's Throwmod), played through the authoritative session with a test
//! rule (`tests/fixtures/carry`; never shipped, players import the original
//! Add-On): an empty-hand click's `on_activate`, a rule seating one player on another's
//! mount point (`mount_object`) that they cannot jump off, `unmount_object`
//! carrying the mount's swing, `set_scale`, `unmount_image`,
//! `set_look_limits`, and bots as players an Add-On can read.
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
    shape::{Node, Shape},
};
use bri_minigames::Settings;
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::{Catalog, ops::ObjectRef};
use bri_sim::{
    definitions::{Definition, Definitions},
    player::MoveInput,
    session::{Command, MiniGameRequest, Ride, Session, shape_mount_points},
    simulation::Simulation,
};
use bri_world::{OwnerId, World};
use glam::Vec3;
use rapier3d::prelude::*;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

const NO_JET: &str = "v20.player.playernojet";

fn showcase() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/showcase")
}

fn catalog(root: &std::path::Path, id: &str) -> Arc<Catalog> {
    Arc::new(
        Catalog::load(
            root,
            &PackageSet {
                schema_version: 1,
                packages: vec![PackageEntry {
                    id: id.into(),
                    version: "1.0.0".into(),
                    side: Side::Server,
                    dir: id.into(),
                    role: None,
                }],
            },
            true,
        )
        .unwrap_or_else(|e| panic!("{e:#?}")),
    )
}

fn add_ons() -> Arc<Catalog> {
    catalog(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures"),
        "carry",
    )
}

/// One 2x2 brick, for the bot's spawn brick.
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

fn node(name: &str, parent: Option<usize>, translation: [f32; 3]) -> Node {
    Node {
        name: name.into(),
        parent,
        translation,
        rotation: [0.0, 0.0, 0.0, 1.0],
    }
}

/// A Blockhead skeleton with two mount points, the hands, under a chest
/// node, and a stray `mount3` past a gap Torque would not count.
fn blockhead() -> Shape {
    Shape {
        schema_version: 1,
        id: "v20.shape.m".into(),
        nodes: vec![
            node("root", None, [0.0; 3]),
            node("chest", Some(0), [0.0, 1.5, 0.0]),
            node("Mount0", Some(1), [0.5, 0.2, -0.3]),
            node("mount1", Some(1), [-0.5, 0.2, -0.3]),
            node("mount3", Some(1), [0.0, 9.0, 0.0]),
        ],
        objects: vec![],
        details: vec![],
        meshes: vec![],
        materials: vec![],
        animations: vec![],
    }
}

struct Game {
    s: Session,
    seq: BTreeMap<OwnerId, u64>,
    moves: BTreeMap<OwnerId, u64>,
    inputs: BTreeMap<OwnerId, MoveInput>,
}
impl Game {
    fn new() -> Self {
        Self::with(World::new("Carry".into(), "carry".into(), vec![[1.0; 4]]))
    }
    fn with(world: World) -> Self {
        Self::with_add_ons(world, add_ons())
    }
    fn with_add_ons(world: World, add_ons: Arc<Catalog>) -> Self {
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
        s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 0.0)]).unwrap();
        s.set_body_mount_points("v20.shape.m", shape_mount_points(&blockhead()))
            .unwrap();
        s.install_packages(add_ons, None).unwrap();
        Self {
            s,
            seq: BTreeMap::new(),
            moves: BTreeMap::new(),
            inputs: BTreeMap::new(),
        }
    }
    fn join(&mut self, name: &str, at: Vec3) -> OwnerId {
        let owner = self.s.join(name.into(), at, false).unwrap();
        self.inputs.insert(owner, MoveInput::default());
        owner
    }
    fn join_verified(&mut self, name: &str, at: Vec3, key: u8) -> OwnerId {
        let owner = self
            .s
            .join_verified(
                name.into(),
                at,
                false,
                Some(bri_admin::Principal([key; 32])),
            )
            .unwrap();
        self.inputs.insert(owner, MoveInput::default());
        owner
    }
    fn cmd(&mut self, owner: OwnerId, command: Command) -> anyhow::Result<bri_sim::session::Reply> {
        let n = self.seq.entry(owner).or_default();
        *n += 1;
        self.s.command(owner, *n, command)
    }
    fn click(&mut self, owner: OwnerId) {
        self.cmd(owner, Command::Activate).unwrap();
    }
    fn steps(&mut self, n: usize) {
        for _ in 0..n {
            for (owner, input) in &self.inputs {
                let m = self.moves.entry(*owner).or_default();
                *m += 1;
                let _ = self.s.movement(*owner, *m, *input);
            }
            self.s.step().unwrap();
        }
    }
    fn state(&self, owner: OwnerId) -> bri_sim::player::PlayerState {
        self.s
            .motion_states()
            .into_iter()
            .find(|(p, _)| p.owner == owner)
            .map(|(p, _)| p)
            .unwrap()
    }
    fn feet(&self, owner: OwnerId) -> Vec3 {
        Vec3::from(self.state(owner).feet)
    }
    fn velocity(&self, owner: OwnerId) -> Vec3 {
        Vec3::from(self.state(owner).velocity)
    }
    /// Turn `owner` to look at the middle of `target`.
    fn aim_at(&mut self, owner: OwnerId, target: OwnerId) {
        let at = self.feet(target) + Vec3::Y * 1.3 - (self.feet(owner) + Vec3::Y * 2.1);
        let flat = Vec3::new(at.x, 0.0, at.z).length();
        let input = self.inputs.get_mut(&owner).unwrap();
        input.yaw = at.x.atan2(-at.z);
        input.pitch = at.y.atan2(flat);
        self.steps(2);
    }
    /// Respawn `owner` a step from `other`.
    fn respawn_beside(&mut self, owner: OwnerId, other: OwnerId) {
        self.cmd(owner, Command::Suicide).unwrap();
        for _ in 0..600 {
            self.steps(1);
            if self.s.vitals()[&owner].alive {
                break;
            }
            let at = self.feet(other) + Vec3::new(0.0, 0.05, 1.5);
            self.s.set_spawn_points(vec![at]).unwrap();
            let _ = self.cmd(owner, Command::Respawn);
        }
        assert!(self.s.vitals()[&owner].alive, "respawned");
    }
    fn ride(&self, owner: OwnerId) -> Option<Ride> {
        self.s.vitals().get(&owner).and_then(|v| v.ride)
    }
    fn scale(&self, owner: OwnerId) -> f32 {
        self.state(owner).scale
    }
    fn archetype(&self, owner: OwnerId) -> String {
        let id = self.state(owner).archetype;
        self.s.archetypes().resolve(id).id.clone()
    }
    /// `owner` starts a minigame (weapons hurt by default) and `others`
    /// join, each respawning where they stood.
    fn minigame(&mut self, owner: OwnerId, others: &[OwnerId]) {
        let here = self.feet(owner);
        self.s.set_spawn_points(vec![here]).unwrap();
        self.cmd(
            owner,
            Command::MiniGame(MiniGameRequest::Create {
                color: 0,
                settings: Settings::default(),
            }),
        )
        .unwrap();
        let game = self.s.minigame_views()[0].id;
        for other in others {
            let there = self.feet(*other);
            self.s.set_spawn_points(vec![there]).unwrap();
            self.cmd(*other, Command::MiniGame(MiniGameRequest::Join { game }))
                .unwrap();
        }
        // Past spawn protection.
        self.steps(330);
    }
}

/// Torque's `numMountPoints`: `mount0`, `mount1`, ... in order, at the
/// rest pose through the node chain, stopping at the first gap.
#[test]
fn a_body_has_the_mount_points_its_model_names() {
    let points = shape_mount_points(&blockhead());
    assert_eq!(points.len(), 2, "{points:?}");
    assert_eq!(points[0].node, "Mount0");
    assert_eq!(points[0].position, [0.5, 1.7, -0.3]);
    assert_eq!(points[1].position, [-0.5, 1.7, -0.3]);
    let g = Game::new();
    let standard =
        g.s.archetypes()
            .find("v20.player.playerstandardarmor")
            .unwrap();
    let body = g.s.archetypes().resolve(standard);
    assert_eq!(body.mount_points, points, "the Blockhead gets its model's");
    assert!(!body.rideable, "nobody boards a Blockhead by landing on it");
}

/// The whole round: lift a player in a minigame into your arms, carry them
/// (they cannot jump off, and survive the holder's change of body), then
/// throw them where you look.
#[test]
fn a_click_lifts_a_player_and_the_next_throws_them() {
    let mut g = Game::new();
    let holder = g.join("Holder", Vec3::new(0.0, 0.05, 0.0));
    let held = g.join("Held", Vec3::new(0.0, 0.05, -1.5));
    g.minigame(holder, &[held]);
    g.aim_at(holder, held);
    let before = g.archetype(holder);
    g.click(holder);
    g.steps(1);
    assert_eq!(
        g.ride(held),
        Some(Ride {
            mount: holder,
            seat: 1,
            steers: false
        }),
        "in the left hand"
    );
    assert!(
        (g.scale(held) - 0.6).abs() < 1e-4,
        "shrunk: {}",
        g.scale(held)
    );
    assert_eq!(g.archetype(holder), NO_JET, "no jets while carrying");
    assert_eq!(g.s.vitals()[&held].look_limits, Some([0.5, 0.5]));

    // Jump to get off: refused (`canDismount` 0).
    g.inputs.get_mut(&held).unwrap().jet = true;
    g.steps(30);
    g.inputs.get_mut(&held).unwrap().jet = false;
    assert_eq!(g.ride(held).map(|r| r.mount), Some(holder), "still held");

    // Carried along as the holder walks.
    g.inputs.get_mut(&holder).unwrap().forward = 1.0;
    g.steps(60);
    g.inputs.get_mut(&holder).unwrap().forward = 0.0;
    let gap = g.feet(held) - g.feet(holder);
    assert!(gap.length() < 3.0, "carried: {gap}");

    // Throw: level, ahead.
    g.inputs.get_mut(&holder).unwrap().pitch = 0.0;
    g.steps(2);
    let yaw = g.inputs[&holder].yaw;
    let ahead = Vec3::new(yaw.sin(), 0.0, -yaw.cos());
    g.click(holder);
    assert_eq!(g.ride(held), None, "let go");
    let flung = g.velocity(held);
    assert!(flung.dot(ahead) > 20.0, "thrown ahead: {flung}");
    g.steps(1);
    assert!((g.scale(held) - 1.0).abs() < 1e-4, "full size again");
    assert_eq!(g.s.vitals()[&held].look_limits, None);
    assert_eq!(g.archetype(holder), before, "the holder's own body back");
}

/// Outside minigames, nobody lifts another player, and the click falls
/// through to the stock activate.
#[test]
fn outside_minigames_players_are_not_lifted() {
    let mut g = Game::new();
    let holder = g.join("Holder", Vec3::new(0.0, 0.05, 0.0));
    let other = g.join("Other", Vec3::new(0.0, 0.05, -1.5));
    g.steps(330);
    g.aim_at(holder, other);
    g.click(holder);
    g.steps(1);
    assert_eq!(g.ride(other), None);
    assert!((g.scale(other) - 1.0).abs() < 1e-4);
}

/// A dead holder drops what they carried, who gets their size back.
#[test]
fn a_holder_who_dies_lets_go() {
    let mut g = Game::new();
    let holder = g.join("Holder", Vec3::new(0.0, 0.05, 0.0));
    let held = g.join("Held", Vec3::new(0.0, 0.05, -1.5));
    g.minigame(holder, &[held]);
    g.aim_at(holder, held);
    g.click(holder);
    g.steps(1);
    assert!(g.ride(held).is_some());
    g.cmd(holder, Command::Suicide).unwrap();
    g.steps(20);
    assert_eq!(g.ride(held), None, "dropped");
    assert!((g.scale(held) - 1.0).abs() < 1e-4, "full size again");
}

/// A world with one Blockhead Bot spawn brick, owned by owner 1 (the
/// principal `[1; 32]`).
fn bot_world() -> World {
    let mut world = World::new("Carry".into(), "carry".into(), vec![[1.0; 4]]);
    world
        .owners
        .insert(1, bri_world::OwnerRecord::new([1; 32], "Builder".into()));
    let mut brick = bri_world::Brick::new(
        bri_world::ContentRef::Resolved("brick".into()),
        [0.0, 0.3, -6.0],
        1,
    );
    brick.vehicle = Some(Box::new(bri_world::VehicleSpawn {
        vehicle: bri_world::ContentRef::Resolved("bot.blockhead".into()),
        recolor: false,
    }));
    world.bricks.insert(1, brick);
    world.next_brick_id = 2;
    world
}

fn bot_game() -> Game {
    bot_game_with(add_ons())
}
fn bot_game_with(add_ons: Arc<Catalog>) -> Game {
    let mut g = Game::with_add_ons(bot_world(), add_ons);
    // Bricks spawn bots alongside vehicles, so the session needs a vehicle
    // pack; the Steel Ball Kit's is at hand.
    g.s.set_vehicle_pack(
        bri_vehicles::Pack::load(showcase().join("steel-ball-kit/assets/vehicles.json")).unwrap(),
        Vec::new(),
    )
    .unwrap();
    g.s.set_bot_kinds(
        bri_sim::bot_kind::BotPack::from_json(include_bytes!(
            "../../../packages/blockhead_bot/assets/bots.json"
        ))
        .unwrap()
        .bots,
    )
    .unwrap();
    g
}

/// v20 let you pick up the bot from your own bot brick outside minigames
/// (lpsroo's port of Throwmod); a stranger cannot. A rule sees a bot as a
/// player with `bot` and its brick's owner.
#[test]
fn a_builder_lifts_the_bot_from_their_own_brick() {
    let mut g = bot_game();
    let builder = g.join_verified("Builder", Vec3::new(0.0, 0.05, 0.0), 1);
    assert_eq!(builder, 1);
    let stranger = g.join_verified("Stranger", Vec3::new(4.0, 0.05, 0.0), 9);
    g.steps(30);
    let bot = *g.s.names().keys().find(|o| g.s.is_bot(**o)).expect("a bot");
    assert!(g.s.may_move(builder, ObjectRef::Player(bot)));
    assert!(!g.s.may_move(stranger, ObjectRef::Player(bot)));

    // Wherever it has wandered, the stranger beside it cannot lift it.
    g.respawn_beside(stranger, bot);
    g.aim_at(stranger, bot);
    g.click(stranger);
    g.steps(1);
    assert_eq!(g.ride(bot), None, "not the stranger's bot");

    // The builder can.
    g.respawn_beside(builder, bot);
    g.aim_at(builder, bot);
    g.click(builder);
    g.steps(1);
    assert_eq!(
        g.ride(bot).map(|r| r.mount),
        Some(builder),
        "the bot is lifted"
    );
    // A held bot does not walk off: its brain rests.
    g.steps(120);
    assert_eq!(g.ride(bot).map(|r| r.mount), Some(builder), "still held");
    g.click(builder);
    assert_eq!(g.ride(bot), None, "thrown");
}

/// A one-script test package `probe` in its own folder `name`.
fn probe(name: &str, capabilities: &str, behaviour: &str, script: &str) -> Arc<Catalog> {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let dir = root.join("probe");
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = format!(
        r#"{{ "schema_version": 1, "id": "probe", "version": "1.0.0", "api": 1,
  "name": "Probe", "description": "A test probe.",
  "authors": ["Blockland ReImagined"], "license": "CC0-1.0",
  "provenance": {{ "source": "original" }}, "capabilities": [{capabilities}],
  "provides": [
    {{ "kind": "behaviour", "id": "probe:behaviour/main", "file": "behaviour.json" }},
    {{ "kind": "script", "id": "probe:script/main", "file": "probe.rhai" }}
  ] }}"#
    );
    for (file, text) in [
        ("package.json", manifest.as_str()),
        ("behaviour.json", behaviour),
        ("probe.rhai", script),
    ] {
        std::fs::write(dir.join(file), text).unwrap();
    }
    catalog(&root, "probe")
}

/// A package that notes who each player hook reached.
fn hook_probe() -> Arc<Catalog> {
    probe(
        "carry-hook-probe",
        "",
        r#"{ "schema_version": 1, "script": "probe.rhai",
  "state": { "global": { "seen": { "default": "", "visible": "everyone", "persist": false } } },
  "on_join": true, "on_loadout": true, "on_spawn": true }"#,
        r#"fn note(hook, who) { set("seen", get("seen") + hook + ":" + who + " "); }
fn on_join(who) { note("join", who); }
fn on_loadout(who) { note("loadout", who); }
fn on_spawn(who) { note("spawn", who); }"#,
    )
}

/// A new bot is not a joining player to Add-Ons: `on_join`, `on_loadout`
/// and `on_spawn` are for players, and used to reach a bot before it was
/// registered as one.
#[test]
fn a_new_bot_gets_no_player_hooks() {
    let mut g = bot_game_with(hook_probe());
    let builder = g.join_verified("Builder", Vec3::new(0.0, 0.05, 0.0), 1);
    g.steps(30);
    let bot = *g.s.names().keys().find(|o| g.s.is_bot(**o)).expect("a bot");
    let state = g.s.package_state();
    let seen = state.packages["probe"].global["seen"]
        .as_str()
        .unwrap()
        .to_owned();
    for hook in ["join", "loadout", "spawn"] {
        assert!(seen.contains(&format!("{hook}:{builder} ")), "{seen}");
        assert!(!seen.contains(&format!("{hook}:{bot} ")), "{seen}");
    }
}

/// The seams a rule needs to act on the fire button and tool switching:
/// `on_trigger` hears an empty hand's press (`Activate`) and its release
/// (`ActivateRelease`), and a fire press when no image is mounted; the
/// `equip` policy vetoes switching tools; `unmount_object` takes a rider
/// off a vehicle. The vehicles are the test's own (no game content).
#[test]
fn a_rule_hears_the_fire_button_vetoes_tools_and_takes_a_rider_off_a_vehicle() {
    let mut g = Game::with_add_ons(
        World::new("Carry".into(), "carry".into(), vec![[1.0; 4]]),
        probe(
            "carry-trigger-probe",
            r#""player", "physics""#,
            r#"{ "schema_version": 1, "script": "probe.rhai",
  "state": { "global": { "seen": { "default": "", "visible": "everyone", "persist": false } } },
  "on_trigger": true, "policies": ["equip"] }"#,
            r#"fn note(text) { set("seen", get("seen") + text + " "); }
fn on_trigger(who, trigger, down) {
    note(`${trigger}:${down}`);
    if !down { unmount_object(who); }
    true
}
fn allow_equip(who) { !player(who).mounted }"#,
        ),
    );
    g.s.set_vehicle_pack(bri_vehicles::testing::pack(), Vec::new())
        .unwrap();
    g.s.spawn_vehicle_at(
        0,
        bri_vehicles::testing::HORSE,
        Vec3::new(0.0, 0.5, 0.0),
        0.0,
        Vec3::ZERO,
    )
    .unwrap();
    // Dropped onto its back: v20 boards from above.
    let rider = g.join("Rider", Vec3::new(0.0, 4.0, 0.0));
    for _ in 0..240 {
        g.steps(1);
        if g.s.mounted(rider).is_some() {
            break;
        }
    }
    assert!(g.s.mounted(rider).is_some(), "riding the horse");
    let seen = |g: &Game| {
        g.s.package_state().packages["probe"].global["seen"]
            .as_str()
            .unwrap()
            .to_owned()
    };

    // No tools on horseback, by the rule's policy.
    assert!(g.cmd(rider, Command::EquipTool { slot: None }).is_err());
    // An empty hand's click is the press; letting go, the release, which
    // takes the rider off.
    g.click(rider);
    assert_eq!(seen(&g), "0:true ");
    assert!(g.s.mounted(rider).is_some());
    g.cmd(rider, Command::ActivateRelease).unwrap();
    assert_eq!(seen(&g), "0:true 0:false ");
    assert_eq!(g.s.mounted(rider), None, "off the horse");
    assert!(g.cmd(rider, Command::EquipTool { slot: None }).is_ok());
    // With no image mounted, a fire press reaches the rule too.
    g.cmd(rider, Command::WeaponTrigger { down: true }).unwrap();
    assert_eq!(seen(&g), "0:true 0:false 0:true ");
}
