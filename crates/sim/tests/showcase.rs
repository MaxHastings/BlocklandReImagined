//! The showcase Add-Ons (`packages/showcase`): the Gravity Gun and the
//! Steel Ball, played through the authoritative session, and the engine
//! seams under them: the `physics` operations (hold, push, tumble, spawn
//! and remove vehicles), an image's charge, fire and jet commands, the
//! credit a thrown object carries, and vehicles that smash bricks and
//! shove players.
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_minigames::Settings;
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::{Catalog, ops::ObjectRef};
use bri_sim::{
    definitions::{Definition, Definitions},
    player::MoveInput,
    session::{ActionAim, Command, MiniGameRequest, PackageCommand, Reply, Session},
    simulation::Simulation,
};
use bri_vehicles::schema::{Family, Seat, Transform};
use bri_world::{BrickId, OwnerId, World};
use glam::Vec3;
use rapier3d::prelude::*;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

const GUN: &str = "gravity-gun-tool:weapon/gravitygun";
const BALL_TOOL: &str = "steel-ball-kit:weapon/steelball";
const BALL: &str = "steel-ball-kit:vehicle/steelball";
/// A plain heavy box standing in for a tank: the tank's mass and run-over
/// damage, no wheels.
const CRATE: &str = "test-kit:vehicle/crate";

fn showcase() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/showcase")
}

fn definitions() -> Definitions {
    // A 2x2 brick (1 x 0.6 x 1 units): volume 12, well under the ball's 30.
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
            },
        )]
        .into(),
    }
}

fn add_ons() -> Arc<Catalog> {
    let packages = [
        ("gravity-gun", Side::Server),
        ("gravity-gun-tool", Side::Shared),
        ("steel-ball", Side::Server),
        ("steel-ball-kit", Side::Shared),
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

fn weapons() -> bri_weapons::Pack {
    let load = |dir: &str| {
        let path = showcase().join(dir).join("assets/weapons.json");
        bri_weapons::Pack::from_json(&std::fs::read(path).unwrap()).unwrap()
    };
    let (pack, notes) =
        load("gravity-gun-tool").merge(vec![("steel-ball-kit".into(), load("steel-ball-kit"))]);
    assert!(notes.is_empty(), "{notes:?}");
    pack
}

/// The Steel Ball Kit's vehicles, plus a crate and v20's tumble body.
fn vehicles() -> bri_vehicles::Pack {
    let mut pack =
        bri_vehicles::Pack::load(showcase().join("steel-ball-kit/assets/vehicles.json")).unwrap();
    let ball = pack.definitions[0].clone();
    let corners = |x: f32, y: f32, z: f32| {
        let mut points = Vec::new();
        for sx in [-x, x] {
            for sy in [-y, y] {
                for sz in [-z, z] {
                    points.push([sx, sy, sz]);
                }
            }
        }
        points
    };
    let mut crate_ = ball.clone();
    crate_.id = CRATE.into();
    crate_.family = Family::Wheeled;
    crate_.collision_hulls = vec![corners(1.0, 1.0, 1.0)];
    crate_.bounds_min = [-1.0; 3];
    crate_.bounds_max = [1.0; 3];
    crate_.inertia_box = [2.0; 3];
    crate_.mass = 300.0;
    crate_.restitution = 0.1;
    crate_.runover_speed = 4.0;
    crate_.runover_damage = 25.0;
    crate_.runover_push = 1.2;
    crate_.smash = None;
    crate_.shove = false;
    let mut tumble = ball.clone();
    tumble.id = "v20.vehicle.deathvehicle".into();
    tumble.family = Family::Tumble;
    tumble.collision_hulls = vec![corners(0.6, 1.25, 0.4)];
    tumble.bounds_min = [-0.6, -1.25, -0.4];
    tumble.bounds_max = [0.6, 1.25, 0.4];
    tumble.inertia_box = [1.2, 2.5, 0.8];
    tumble.mass = 90.0;
    tumble.runover_speed = 3.4e38;
    tumble.runover_damage = 0.0;
    tumble.smash = None;
    tumble.shove = false;
    tumble.seats = vec![Seat {
        node: "mount0".into(),
        transform: Transform::default(),
        pose: "root".into(),
        controls: false,
        weapon: false,
    }];
    pack.definitions.push(crate_);
    pack.definitions.push(tumble);
    pack.validate().unwrap();
    pack
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
                World::new(
                    "Showcase".into(),
                    "showcase".into(),
                    vec![[1.0; 4], [0.6; 4]],
                ),
                definitions(),
                vec![
                    ColliderBuilder::cuboid(200.0, 0.5, 200.0)
                        .translation(Vector::new(0.0, -0.5, 0.0)),
                ],
            )
            .unwrap(),
        );
        s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 0.0)]).unwrap();
        s.set_weapon_pack(weapons()).unwrap();
        s.set_vehicle_pack(vehicles()).unwrap();
        s.install_packages(add_ons(), None).unwrap();
        Self {
            s,
            seq: BTreeMap::new(),
            moves: BTreeMap::new(),
            looks: BTreeMap::new(),
        }
    }
    fn join(&mut self, name: &str, at: Vec3) -> OwnerId {
        // Administrators, so tests plant walls without the plant rate.
        let owner = self.s.join(name.into(), at, true).unwrap();
        self.looks.insert(owner, MoveInput::default());
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
        self.looks.insert(owner, MoveInput::default());
        owner
    }
    fn cmd(&mut self, owner: OwnerId, command: Command) -> anyhow::Result<Reply> {
        let n = self.seq.entry(owner).or_default();
        *n += 1;
        self.s.command(owner, *n, command)
    }
    /// An Add-On command as the player's tool would send it, aimed along
    /// their look.
    fn package(&mut self, owner: OwnerId, package: &str, command: &str) -> anyhow::Result<Reply> {
        let look = self.looks[&owner];
        let n = self.seq.entry(owner).or_default();
        *n += 1;
        self.s.command_with_aim(
            owner,
            *n,
            Command::Package(PackageCommand {
                package: package.into(),
                command: command.into(),
                args: vec![],
            }),
            Some(ActionAim {
                yaw: look.yaw,
                pitch: look.pitch,
            }),
        )
    }
    fn look(&mut self, owner: OwnerId, yaw: f32, pitch: f32) {
        let input = self.looks.get_mut(&owner).unwrap();
        input.yaw = yaw;
        input.pitch = pitch;
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
    fn vehicle(&self, id: u64) -> Option<(Vec3, Vec3)> {
        self.s
            .vehicle_poses()
            .into_iter()
            .find(|v| v.id == id)
            .map(|v| (Vec3::from(v.position), Vec3::from(v.velocity)))
    }
    fn vehicles_of(&self, definition: &str) -> Vec<u64> {
        self.s
            .vehicle_infos()
            .into_iter()
            .filter(|v| v.definition == definition && !v.destroyed)
            .map(|v| v.id)
            .collect()
    }
    fn beam(&self, owner: OwnerId) -> Vec<f64> {
        self.s
            .package_state()
            .packages
            .get("gravity-gun")
            .and_then(|ns| ns.players.get(&owner))
            .and_then(|m| m.get("beam"))
            .and_then(|v| v.as_array())
            .map(|a| a.iter().map(|x| x.as_f64().unwrap()).collect())
            .unwrap_or_default()
    }
    /// `owner` starts a minigame and `others` join. Joining respawns a
    /// player, so each respawns where they stood.
    fn minigame(&mut self, owner: OwnerId, others: &[OwnerId]) {
        let here = self.feet(owner);
        self.s.set_spawn_points(vec![here]).unwrap();
        self.cmd(
            owner,
            Command::MiniGame(MiniGameRequest::Create {
                color: 0,
                settings: Settings {
                    loadout: Default::default(),
                    ..Settings::default()
                },
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

fn rotation(g: &Game, id: u64) -> glam::Quat {
    g.s.vehicle_poses()
        .into_iter()
        .find(|v| v.id == id)
        .map(|v| glam::Quat::from_array(v.rotation))
        .unwrap()
}
/// Left click: hold it down to grab, let go to drop.
fn trigger(g: &mut Game, owner: OwnerId, down: bool) {
    g.cmd(owner, Command::WeaponTrigger { down }).unwrap();
    g.steps(2);
}
fn wrap(yaw: f32) -> f32 {
    (yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

#[test]
fn the_gun_grabs_a_vehicle_where_it_points_and_carries_it_without_lag() {
    let mut g = Game::new();
    let host = g.join("Host", Vec3::new(0.0, 0.05, 0.0));
    g.s.give_tool(host, GUN, true).unwrap();
    let crate_ =
        g.s.spawn_vehicle_at(0, CRATE, Vec3::new(0.0, 1.0, -9.0), 0.0, Vec3::ZERO)
            .unwrap();
    g.steps(60);
    let (start, _) = g.vehicle(crate_).unwrap();
    // Hold left click: it is caught where it is, not dragged in.
    trigger(&mut g, host, true);
    assert_eq!(g.s.held_by(host), Some(ObjectRef::Vehicle(crate_)));
    assert_eq!(g.beam(host)[..3], [1.0, crate_ as f64, 1.0]);
    g.steps(120);
    let (at, v) = g.vehicle(crate_).unwrap();
    assert!(
        at.distance(start) < 0.6 && v.length() < 0.3,
        "held in place, steady: {start} -> {at}, {v}"
    );
    // Turn right at half a turn a second: it keeps up, it doesn't trail.
    let feet = g.feet(host);
    let reach = Vec3::new(at.x - feet.x, 0.0, at.z - feet.z).length();
    let turn = std::f32::consts::PI / 120.0;
    let mut yaw = 0.0_f32;
    let mut worst = 0.0_f32;
    let grip = rotation(&g, crate_);
    for tick in 0..120 {
        yaw += turn;
        g.look(host, wrap(yaw), 0.0);
        g.steps(1);
        if tick > 20 {
            let feet = g.feet(host);
            let (at, _) = g.vehicle(crate_).unwrap();
            // How far it lags sideways behind where the view points.
            let side = Vec3::new(yaw.cos(), 0.0, yaw.sin());
            worst = worst.max((at - feet).dot(side).abs());
        }
    }
    assert!(
        worst < 0.45,
        "it trails the view by {worst} units at {reach}"
    );
    g.steps(60);
    // It turned with the holder: half a turn round, the same face to them.
    let turned = rotation(&g, crate_) * grip.inverse();
    let expected = glam::Quat::from_rotation_y(-yaw);
    assert!(
        turned.angle_between(expected) < 0.15,
        "kept its angle to the holder: {turned} vs {expected}"
    );
    // Let go: it drops and rests.
    trigger(&mut g, host, false);
    assert_eq!(g.s.held_by(host), None);
    g.steps(240);
    assert_eq!(g.beam(host)[..3], [0.0, 0.0, 0.0]);
    let (at, v) = g.vehicle(crate_).unwrap();
    assert!(
        at.y < 1.3 && v.length() < 0.5,
        "dropped and resting: {at} {v}"
    );
}

#[test]
fn a_held_object_settles_without_wobbling_heavy_or_light() {
    for (definition, mass) in [(CRATE, 300.0), (BALL, 900.0)] {
        let mut g = Game::new();
        let host = g.join("Host", Vec3::new(0.0, 0.05, 0.0));
        g.s.give_tool(host, GUN, true).unwrap();
        let id =
            g.s.spawn_vehicle_at(0, definition, Vec3::new(0.0, 1.3, -8.0), 0.0, Vec3::ZERO)
                .unwrap();
        g.steps(60);
        trigger(&mut g, host, true);
        assert!(g.s.held_by(host).is_some());
        g.steps(60);
        // Look up a little: the object rises to the new point and stops
        // there, never shooting past it.
        g.look(host, 0.0, 0.35);
        let start = g.vehicle(id).unwrap().0.y;
        let mut ys = Vec::new();
        for _ in 0..240 {
            g.steps(1);
            ys.push(g.vehicle(id).unwrap().0.y);
        }
        let (at, v) = g.vehicle(id).unwrap();
        let highest = ys.iter().copied().fold(f32::MIN, f32::max);
        assert!(
            at.y > start + 2.0,
            "mass {mass}: rose from {start} to {}",
            at.y
        );
        assert!(v.length() < 0.3, "mass {mass}: settled, still moving {v}");
        assert!(
            highest < at.y + 0.2,
            "mass {mass}: overshot to {highest} past {}",
            at.y
        );
        // Rising, not bobbing: it never falls back on the way up.
        let dips = ys.windows(2).filter(|w| w[1] < w[0] - 0.02).count();
        assert_eq!(dips, 0, "mass {mass}: bobbed on the way up");
    }
}

#[test]
fn a_flick_of_the_view_flings_what_you_let_go_of() {
    let mut g = Game::new();
    let host = g.join("Host", Vec3::new(0.0, 0.05, 0.0));
    g.s.give_tool(host, GUN, true).unwrap();
    let crate_ =
        g.s.spawn_vehicle_at(0, CRATE, Vec3::new(0.0, 1.0, -6.0), 0.0, Vec3::ZERO)
            .unwrap();
    g.steps(60);
    trigger(&mut g, host, true);
    g.steps(60);
    // Swing right fast, then let go mid-swing.
    let mut yaw = 0.0;
    for _ in 0..18 {
        yaw += 0.06;
        g.look(host, yaw, 0.0);
        g.steps(1);
    }
    g.cmd(host, Command::WeaponTrigger { down: false }).unwrap();
    g.steps(1);
    let (_, v) = g.vehicle(crate_).unwrap();
    assert!(v.length() > 15.0 && v.x > 10.0, "flung sideways: {v}");
}

#[test]
fn the_wheel_reels_a_held_thing_out_and_in() {
    let mut g = Game::new();
    let host = g.join("Host", Vec3::new(0.0, 0.05, 0.0));
    g.s.give_tool(host, GUN, true).unwrap();
    let crate_ =
        g.s.spawn_vehicle_at(0, CRATE, Vec3::new(0.0, 1.0, -6.0), 0.0, Vec3::ZERO)
            .unwrap();
    g.steps(60);
    trigger(&mut g, host, true);
    g.steps(30);
    let reel = |g: &mut Game, notches: i64| {
        let n = g.seq.entry(host).or_default();
        *n += 1;
        g.s.command(
            host,
            *n,
            Command::Package(PackageCommand {
                package: "gravity-gun".into(),
                command: "reel".into(),
                args: vec![bri_sim::session::PackageArg::Int(notches)],
            }),
        )
        .unwrap();
        g.steps(90);
        g.vehicle(crate_).unwrap().0.z
    };
    let start = g.vehicle(crate_).unwrap().0.z;
    let out = reel(&mut g, 4);
    assert!(out < start - 4.0, "reeled out from {start} to {out}");
    let back = reel(&mut g, -4);
    assert!((back - start).abs() < 0.5, "and back in to {back}");
    assert!(g.s.held_by(host).is_some(), "still held");
}

#[test]
fn looking_down_sets_a_held_thing_before_you_instead_of_lifting_you() {
    let mut g = Game::new();
    let host = g.join("Host", Vec3::new(0.0, 0.05, 0.0));
    g.s.give_tool(host, GUN, true).unwrap();
    let ball =
        g.s.spawn_vehicle_at(0, BALL, Vec3::new(0.0, 1.3, -5.0), 0.0, Vec3::ZERO)
            .unwrap();
    g.steps(60);
    trigger(&mut g, host, true);
    assert!(g.s.held_by(host).is_some());
    g.steps(30);
    let before = g.feet(host);
    g.look(host, 0.0, -1.5);
    g.steps(240);
    let feet = g.feet(host);
    assert!(
        feet.distance(before) < 0.3,
        "the holder stayed put: {before} -> {feet}"
    );
    let (at, _) = g.vehicle(ball).unwrap();
    let flat = Vec3::new(at.x - feet.x, 0.0, at.z - feet.z).length();
    assert!(flat > 1.2, "the ball is in front, not under them: {at}");
}

#[test]
fn a_flung_heavy_vehicle_kills_in_a_minigame_and_credits_the_thrower() {
    let mut g = Game::new();
    let a = g.join("Alpha", Vec3::new(0.0, 0.05, 0.0));
    // Where a crate swung hard right and let go flies.
    let b = g.join("Bravo", Vec3::new(12.0, 0.05, -12.0));
    g.minigame(a, &[b]);
    // A minigame hands out its own loadout; the gun is given again.
    g.s.give_tool(a, GUN, true).unwrap();
    let crate_ = g
        .s
        .spawn_vehicle_at(a, CRATE, Vec3::new(-5.05, 1.0, -3.24), 0.0, Vec3::ZERO)
        .unwrap();
    g.look(a, -1.0, 0.0);
    g.steps(30);
    trigger(&mut g, a, true);
    assert_eq!(g.s.held_by(a), Some(ObjectRef::Vehicle(crate_)));
    g.steps(30);
    // Swing it round toward Bravo and let go.
    let mut yaw = -1.0_f32;
    while yaw < 0.0 {
        yaw += 0.06;
        g.look(a, yaw, 0.0);
        g.steps(1);
    }
    g.cmd(a, Command::WeaponTrigger { down: false }).unwrap();
    let mut dead = false;
    for _ in 0..240 {
        g.steps(1);
        if !g.s.is_alive(b) {
            dead = true;
            break;
        }
    }
    assert!(dead, "the flung crate crushed Bravo");
    assert_eq!(g.s.vitals()[&a].score, 1, "Alpha is credited with the kill");
    assert_eq!(g.beam(a)[3..6], [1.0, 1.0, crate_ as f64], "a throw is counted");
    assert!(g.s.is_alive(a));
}

#[test]
fn outside_minigames_trust_decides_what_the_gun_may_move() {
    let mut g = Game::new();
    let a = g.join_verified("Ann", Vec3::new(0.0, 0.05, 0.0), 1);
    let b = g.join_verified("Bob", Vec3::new(0.0, 0.05, -5.0), 2);
    g.s.give_tool(a, GUN, true).unwrap();
    g.steps(60);
    // Bob does not trust Ann: she cannot pick him up...
    g.package(a, "gravity-gun", "grab").unwrap();
    assert_eq!(g.s.held_by(a), None);
    // ...nor his crate.
    let bobs =
        g.s.spawn_vehicle_at(b, CRATE, Vec3::new(4.0, 1.0, -6.0), 0.0, Vec3::ZERO)
            .unwrap();
    g.steps(60);
    g.look(a, 0.58, 0.0);
    g.steps(15);
    g.package(a, "gravity-gun", "grab").unwrap();
    assert_eq!(g.s.held_by(a), None, "Bob's crate needs Bob's trust");
    // Once Bob trusts Ann, both are fair game.
    g.cmd(
        a,
        Command::TrustInvite {
            target: b,
            level: 1,
        },
    )
    .unwrap();
    g.cmd(b, Command::AcceptTrust { from: a }).unwrap();
    g.steps(15);
    g.package(a, "gravity-gun", "grab").unwrap();
    assert_eq!(g.s.held_by(a), Some(ObjectRef::Vehicle(bobs)));
    g.package(a, "gravity-gun", "release").unwrap();
    assert_eq!(g.s.held_by(a), None, "letting go drops it");
    g.look(a, 0.0, 0.0);
    g.steps(15);
    g.package(a, "gravity-gun", "grab").unwrap();
    assert_eq!(g.s.held_by(a), Some(ObjectRef::Player(b)));
    // A grabbed player goes limp, and stays limp however still they hang.
    g.steps(2);
    assert!(
        g.s.mounted(b).is_some(),
        "Bob hangs in the beam as a tumble"
    );
    g.look(a, 0.0, 0.3);
    g.steps(600);
    assert!(g.s.mounted(b).is_some(), "still held after five seconds");
    assert!(g.feet(b).y > 1.0, "Bob is lifted: {}", g.feet(b));
    // Let go, he drops and gets up once he lands.
    g.package(a, "gravity-gun", "release").unwrap();
    g.steps(600);
    assert!(g.s.mounted(b).is_none(), "Bob got up after landing");
    // Outside a minigame none of it hurts.
    assert!(g.s.is_alive(b));
    assert_eq!(g.s.vitals()[&b].health, 100.0);
}

#[test]
fn corpses_can_be_grabbed_carried_and_dropped() {
    let mut g = Game::new();
    let a = g.join("Alpha", Vec3::new(0.0, 0.05, 0.0));
    let b = g.join("Bravo", Vec3::new(0.0, 0.05, -5.0));
    g.minigame(a, &[b]);
    g.s.give_tool(a, GUN, true).unwrap();
    g.cmd(b, Command::Suicide).unwrap();
    g.steps(30);
    assert!(!g.s.is_alive(b));
    let lying = g.feet(b);
    g.look(a, 0.0, -0.25);
    g.steps(4);
    g.package(a, "gravity-gun", "grab").unwrap();
    assert_eq!(
        g.s.held_by(a),
        Some(ObjectRef::Player(b)),
        "the corpse is caught"
    );
    assert_eq!(g.beam(a)[..3], [2.0, b as f64, 1.0]);
    g.look(a, 0.0, 0.3);
    g.steps(120);
    assert!(
        g.feet(b).y > lying.y + 1.0,
        "lifted: {lying} -> {}",
        g.feet(b)
    );
    g.package(a, "gravity-gun", "release").unwrap();
    g.steps(120);
    assert!(g.feet(b).y < lying.y + 0.3, "dropped: {}", g.feet(b));
    assert!(!g.s.is_alive(b), "still a corpse");
}

#[test]
fn rolled_steel_balls_roll_on_and_each_player_keeps_three() {
    let mut g = Game::new();
    let host = g.join("Host", Vec3::new(0.0, 0.05, 0.0));
    g.s.give_tool(host, BALL_TOOL, true).unwrap();
    g.steps(30);
    g.cmd(host, Command::WeaponTrigger { down: true }).unwrap();
    g.steps(2);
    g.cmd(host, Command::WeaponTrigger { down: false }).unwrap();
    g.steps(2);
    let balls = g.vehicles_of(BALL);
    assert_eq!(balls.len(), 1, "left click rolled one out");
    let ball = balls[0];
    g.steps(360);
    let (at, v) = g.vehicle(ball).unwrap();
    assert!(at.z < -15.0, "it rolled on: {at}");
    assert!(
        (at.y - 1.25).abs() < 0.1,
        "a true sphere of radius 1.25 on the floor: {at}"
    );
    assert!(v.length() > 1.0, "still rolling after three seconds: {v}");
    // Three more: the oldest makes way.
    for _ in 0..3 {
        g.package(host, "steel-ball", "roll").unwrap();
        g.steps(40);
    }
    let now = g.vehicles_of(BALL);
    assert_eq!(now.len(), 3);
    assert!(!now.contains(&ball), "the first ball was put away");
    g.cmd(
        host,
        Command::Package(PackageCommand {
            package: String::new(),
            command: "clearballs".into(),
            args: vec![],
        }),
    )
    .unwrap();
    g.steps(2);
    assert!(g.vehicles_of(BALL).is_empty());
}

/// A wall of bricks across lane `x` of the floor, owned by `owner`.
fn wall(g: &mut Game, owner: OwnerId, x: f32, z: f32) -> Vec<BrickId> {
    let mut ids = Vec::new();
    for dx in [-1.0, 0.0, 1.0] {
        for layer in 0..4 {
            let y = 0.3 + 0.6 * layer as f32;
            match g.cmd(
                owner,
                Command::Plant {
                    definition: "brick".into(),
                    position: [x + dx, y, z],
                    quarter_turns: 0,
                    color: 1,
                },
            ) {
                Ok(Reply::Planted(id)) => ids.push(id),
                other => panic!("plant at {} {y} {z}: {other:?}", x + dx),
            }
        }
    }
    ids
}
fn standing(g: &Game, bricks: &[BrickId]) -> usize {
    let world = g.s.snapshot().world;
    bricks
        .iter()
        .filter(|id| world.bricks.get(*id).is_some_and(|b| b.colliding))
        .count()
}

#[test]
fn a_fast_steel_ball_breaks_bricks_under_rocket_rules() {
    let mut g = Game::new();
    let a = g.join("Alpha", Vec3::new(20.0, 0.05, -6.0));
    let b = g.join("Bravo", Vec3::new(-6.0, 0.05, -6.0));
    g.steps(30);
    // Every wall stands across its own lane, 10 units ahead of its ball.
    let fire = |g: &mut Game, owner: OwnerId, lane: f32, speed: f32| {
        g.s.spawn_vehicle_at(
            owner,
            BALL,
            Vec3::new(lane, 1.3, -4.0),
            0.0,
            Vec3::new(0.0, 0.0, -speed),
        )
        .unwrap()
    };
    // Outside minigames a ball breaks its owner's bricks only, as a
    // rocket does.
    let bobs = wall(&mut g, b, 0.0, -14.0);
    let alphas = wall(&mut g, a, 24.0, -14.0);
    fire(&mut g, a, 0.0, 25.0);
    fire(&mut g, a, 24.0, 25.0);
    g.steps(120);
    assert_eq!(standing(&g, &bobs), bobs.len(), "Bravo's wall is safe");
    assert!(
        standing(&g, &alphas) < alphas.len(),
        "Alpha's own wall breaks"
    );
    // In a minigame with brick damage, a member's hard hit knocks out the
    // minigame's bricks (its owner's, by v20's default)...
    g.minigame(a, &[b]);
    let fast = wall(&mut g, a, 8.0, -14.0);
    let slow = wall(&mut g, a, 16.0, -14.0);
    fire(&mut g, b, 8.0, 25.0);
    // ...but a gentle roll only bumps into them.
    fire(&mut g, b, 16.0, 5.0);
    g.steps(360);
    let knocked = fast.len() - standing(&g, &fast);
    assert!(
        knocked >= 2,
        "a 25 u/s ball broke {knocked} bricks in a minigame"
    );
    assert_eq!(
        standing(&g, &slow),
        slow.len(),
        "a slow ball breaks nothing"
    );
}

#[test]
fn a_steel_ball_shoves_players_aside_and_only_hurts_in_minigames() {
    let mut g = Game::new();
    let a = g.join("Alpha", Vec3::new(0.0, 0.05, 0.0));
    let b = g.join("Bravo", Vec3::new(0.0, 0.05, -12.0));
    g.steps(30);
    let before = g.feet(b);
    g.s.spawn_vehicle_at(
        a,
        BALL,
        Vec3::new(0.0, 1.3, -3.0),
        0.0,
        Vec3::new(0.0, 0.0, -18.0),
    )
    .unwrap();
    let mut bowled = false;
    for _ in 0..120 {
        g.steps(1);
        bowled |= g.s.mounted(b).is_some();
    }
    assert!(bowled, "bowled over into a tumble");
    g.steps(60);
    assert!(
        g.feet(b).distance(before) > 3.0,
        "shoved aside: {}",
        g.feet(b)
    );
    assert_eq!(g.s.vitals()[&b].health, 100.0, "no harm outside minigames");
    // In a minigame the same roll hurts, credited to the ball's owner.
    g.minigame(a, &[b]);
    let b_at = g.feet(b);
    g.s.spawn_vehicle_at(
        a,
        BALL,
        b_at + Vec3::new(0.0, 1.3, 9.0),
        0.0,
        Vec3::new(0.0, 0.0, -18.0),
    )
    .unwrap();
    g.steps(180);
    assert!(g.s.vitals()[&b].health < 100.0 || !g.s.is_alive(b));
}

#[test]
fn everyone_gets_both_items_outside_minigames_and_the_loadout_decides_inside() {
    let mut g = Game::new();
    let a = g.join("Alpha", Vec3::new(0.0, 0.05, 0.0));
    let b = g.join("Bravo", Vec3::new(6.0, 0.05, 0.0));
    let holds = |g: &Game, owner: OwnerId, item: &str| {
        g.s.tool_inventories()[&owner]
            .slots
            .iter()
            .any(|s| s.as_deref() == Some(item))
    };
    g.steps(2);
    for owner in [a, b] {
        assert!(holds(&g, owner, GUN) && holds(&g, owner, BALL_TOOL));
    }
    // A respawn sets the items afresh, and they come back.
    g.cmd(b, Command::Suicide).unwrap();
    g.steps(130);
    g.cmd(b, Command::Respawn).unwrap();
    g.steps(2);
    assert!(holds(&g, b, GUN) && holds(&g, b, BALL_TOOL));
    // In a minigame its loadout decides; asking by name is refused.
    g.minigame(a, &[b]);
    assert!(!holds(&g, b, GUN) && !holds(&g, b, BALL_TOOL));
    g.cmd(
        b,
        Command::Package(PackageCommand {
            package: String::new(),
            command: "gravitygun".into(),
            args: vec![],
        }),
    )
    .unwrap();
    g.steps(2);
    assert!(!holds(&g, b, GUN), "no gun from a command in a minigame");
}

#[test]
fn right_click_still_jets_with_the_gun_in_hand() {
    let rise = |gun: bool| {
        let mut g = Game::new();
        let host = g.join("Host", Vec3::new(0.0, 0.05, 0.0));
        g.steps(2);
        if gun {
            g.s.give_tool(host, GUN, true).unwrap();
        }
        g.steps(30);
        let before = g.feet(host).y;
        g.looks.get_mut(&host).unwrap().jet = true;
        g.steps(40);
        g.looks.get_mut(&host).unwrap().jet = false;
        g.feet(host).y - before
    };
    let bare = rise(false);
    assert!(bare > 0.15, "an empty-handed player jets: {bare}");
    let armed = rise(true);
    assert!(
        (armed - bare).abs() < 0.05,
        "the gun leaves right click alone: rose {armed} vs {bare}"
    );
}

#[test]
fn steel_balls_count_toward_the_per_builder_vehicle_quota() {
    let mut g = Game::new();
    let host = g.join("Host", Vec3::new(0.0, 0.05, 0.0));
    let mut settings = bri_admin::ServerSettings::default();
    settings.per_player.vehicles = 1;
    g.s.set_server_settings(settings).unwrap();
    g.steps(30);
    g.package(host, "steel-ball", "roll").unwrap();
    g.steps(40);
    assert_eq!(g.vehicles_of(BALL).len(), 1);
    g.s.take_private_notices();
    g.package(host, "steel-ball", "hurl").unwrap();
    g.steps(2);
    assert_eq!(
        g.vehicles_of(BALL).len(),
        1,
        "the quota holds the second back"
    );
    assert!(g.s.take_private_notices().iter().any(|(o, n)| *o == host
        && matches!(n, bri_sim::session::Notice::Center { text, .. }
            if text.ends_with("You already have a physics-vehicle"))));
}
