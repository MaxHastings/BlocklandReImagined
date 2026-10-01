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
    session::{
        ActionAim, Command, MiniGameRequest, PackageCommand, Reply, Session, ToolCatalog,
        WrenchProperties,
    },
    simulation::Simulation,
};
use bri_vehicles::schema::{Family, Seat, Transform};
use bri_world::{BrickId, OwnerId, World, authority::Edit};
use glam::Vec3;
use rapier3d::prelude::*;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

const GUN: &str = "gravity-gun-tool:weapon/gravitygun";
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
                link: None,
                glass: [0.0; 4],
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
    let path = showcase().join("gravity-gun-tool/assets/weapons.json");
    bri_weapons::Pack::from_json(&std::fs::read(path).unwrap()).unwrap()
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
    crate_.harms_only_in_minigames = false;
    crate_.max_damage = 200.0;
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
    tumble.harms_only_in_minigames = false;
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
        Self::with(World::new(
            "Showcase".into(),
            "showcase".into(),
            vec![[1.0; 4], [0.6; 4]],
        ))
    }
    fn with(world: World) -> Self {
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
        s.set_weapon_pack(weapons()).unwrap();
        s.set_vehicle_pack(vehicles(), Vec::new()).unwrap();
        s.install_packages(add_ons(), None).unwrap();
        // "brick" doubles as a vehicle spawn brick that may hold the ball.
        s.set_tool_catalog(ToolCatalog {
            vehicles: [BALL.to_string()].into(),
            vehicle_bricks: ["brick".to_string()].into(),
            ..Default::default()
        })
        .unwrap();
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

/// Max, v0.1.10: with left click held at something out of reach, walking
/// up to it never grabbed it; he had to let go and click again. A trigger
/// held with nothing caught keeps reaching, and catches it once in range.
#[test]
fn holding_the_trigger_catches_what_comes_in_range_without_a_second_click() {
    let mut g = Game::new();
    let host = g.join("Host", Vec3::new(0.0, 0.05, 0.0));
    g.s.give_tool(host, GUN, true).unwrap();
    let crate_ =
        g.s.spawn_vehicle_at(0, CRATE, Vec3::new(0.0, 1.0, -75.0), 0.0, Vec3::ZERO)
            .unwrap();
    g.steps(60);
    trigger(&mut g, host, true);
    g.steps(30);
    assert_eq!(g.s.held_by(host), None, "out of reach");
    assert_eq!(g.beam(host)[..3], [0.0, 0.0, 1.0], "the beam reaches for it");
    // Walk up to it with the trigger still down: caught on the way.
    g.looks.get_mut(&host).unwrap().forward = 1.0;
    let mut caught = None;
    for tick in 0..1200 {
        g.steps(1);
        if g.s.held_by(host).is_some() {
            caught = Some(tick);
            break;
        }
    }
    g.looks.get_mut(&host).unwrap().forward = 0.0;
    assert!(caught.is_some(), "never caught it");
    assert_eq!(g.s.held_by(host), Some(ObjectRef::Vehicle(crate_)));
    let (at, _) = g.vehicle(crate_).unwrap();
    let gap = at.distance(g.feet(host));
    assert!(gap > 50.0 && gap < 62.0, "caught as it came in range, {gap} off");
    g.steps(12);
    assert_eq!(g.beam(host)[..3], [1.0, crate_ as f64, 1.0], "and the beam shows it");
    // Let go: nothing is reached for any more.
    trigger(&mut g, host, false);
    assert_eq!(g.s.held_by(host), None);
    g.steps(30);
    assert_eq!(g.s.held_by(host), None, "let go stays let go");
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
fn an_administrator_can_grab_anyone_outside_minigames_but_not_inside() {
    let mut g = Game::new();
    // Administrators (see `join`), and neither trusts the other.
    let a = g.join("Admin", Vec3::new(0.0, 0.05, 0.0));
    let b = g.join_verified("Bob", Vec3::new(0.0, 0.05, -5.0), 2);
    g.s.give_tool(a, GUN, true).unwrap();
    g.steps(60);
    g.package(a, "gravity-gun", "grab").unwrap();
    assert_eq!(g.s.held_by(a), Some(ObjectRef::Player(b)));
    g.look(a, 0.0, 0.3);
    g.steps(120);
    assert!(g.feet(b).y > 1.0, "Bob hangs in the air: {}", g.feet(b));
    g.package(a, "gravity-gun", "release").unwrap();
    g.steps(600);
    // In a minigame, its rules decide for administrators too: Bob, not in
    // it, is out of reach.
    g.minigame(a, &[]);
    g.s.give_tool(a, GUN, true).unwrap();
    g.look(a, 0.0, 0.0);
    g.steps(60);
    let to_bob = g.feet(b) - g.feet(a);
    g.look(a, (-to_bob.x).atan2(-to_bob.z), 0.0);
    g.steps(15);
    g.package(a, "gravity-gun", "grab").unwrap();
    assert_eq!(g.s.held_by(a), None);
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

/// A vehicle spawn brick of `owner`'s at `x` along z = 8, set to the
/// Steel Ball.
fn ball_brick(g: &mut Game, owner: OwnerId, x: f32) -> BrickId {
    let Ok(Reply::Planted(brick)) = g.cmd(
        owner,
        Command::Plant {
            definition: "brick".into(),
            position: [x, 0.3, 8.0],
            quarter_turns: 0,
            color: 1,
        },
    ) else {
        panic!("spawn brick at {x}")
    };
    g.s.edit_brick(
        owner,
        brick,
        Edit::Properties(WrenchProperties {
            vehicle: Some(BALL.into()),
            raycast: true,
            colliding: true,
            visible: true,
            ..Default::default()
        }),
    )
    .unwrap();
    brick
}

/// Balls sitting over the spawn bricks at x from `from` to `to`.
fn balls_over(g: &Game, from: f32, to: f32) -> usize {
    g.vehicles_of(BALL)
        .into_iter()
        .filter(|id| {
            g.vehicle(*id)
                .is_some_and(|(at, _)| (from - 2.0..=to + 2.0).contains(&at.x))
        })
        .count()
}

fn clearballs(g: &mut Game, owner: OwnerId) {
    g.cmd(
        owner,
        Command::Package(PackageCommand {
            package: String::new(),
            command: "clearballs".into(),
            args: vec![],
        }),
    )
    .unwrap();
    g.steps(2);
}

/// Max, v0.1.10: Steel Balls come only from a vehicle spawn brick. Each
/// player keeps three; a fourth brick tells its builder so, and
/// /clearballs puts away only the caller's own.
#[test]
fn steel_balls_come_from_spawn_bricks_three_per_player() {
    let mut g = Game::new();
    let host = g.join("Host", Vec3::new(0.0, 0.05, 0.0));
    let guest = g.join("Guest", Vec3::new(0.0, 0.05, -8.0));
    g.steps(30);
    let tools = &g.s.tool_inventories()[&host].slots;
    assert!(
        !tools.iter().flatten().any(|t| t.starts_with("steel-ball")),
        "no ball in hand: {tools:?}"
    );
    for i in 0..3 {
        ball_brick(&mut g, host, i as f32 * 8.0);
        g.steps(10);
    }
    let first = g.vehicles_of(BALL);
    assert_eq!(first.len(), 3, "one ball on each brick");
    let (at, _) = g.vehicle(first[0]).unwrap();
    g.steps(120);
    let (rested, _) = g.vehicle(first[0]).unwrap();
    assert!(
        (rested.y - at.y).abs() < 2.0 && rested.y > 0.5,
        "it sits on its brick: {at} -> {rested}"
    );
    g.s.take_private_notices();
    ball_brick(&mut g, host, 24.0);
    g.steps(10);
    assert_eq!(
        balls_over(&g, 0.0, 24.0),
        3,
        "the fourth brick is held back"
    );
    assert!(g.s.take_private_notices().iter().any(|(o, n)| *o == host
        && matches!(n, bri_sim::session::Notice::Center { text, .. }
            if text.ends_with("You already have 3 Steel Balls"))));
    // Another player's brick is theirs to count.
    ball_brick(&mut g, guest, -8.0);
    g.steps(10);
    assert_eq!(balls_over(&g, -8.0, -8.0), 1);
    clearballs(&mut g, host);
    assert_eq!(
        balls_over(&g, 0.0, 24.0),
        0,
        "/clearballs puts the caller's away"
    );
    assert_eq!(balls_over(&g, -8.0, -8.0), 1, "and only the caller's");
    // They stay away until a brick's wrench asks again.
    g.steps(120);
    assert_eq!(balls_over(&g, 0.0, 24.0), 0);
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

/// Rolls a ball owned by `owner` down lane `x` toward -z at `speed`,
/// 10 units short of the walls at z = -14.
fn fire(g: &mut Game, owner: OwnerId, lane: f32, speed: f32) -> u64 {
    g.s.spawn_vehicle_at(
        owner,
        BALL,
        Vec3::new(lane, 1.3, -4.0),
        0.0,
        Vec3::new(0.0, 0.0, -speed),
    )
    .unwrap()
}

#[test]
fn a_hurled_steel_ball_punches_through_walls_only_in_minigames() {
    let mut g = Game::new();
    let a = g.join("Alpha", Vec3::new(40.0, 0.05, 6.0));
    let b = g.join("Bravo", Vec3::new(-6.0, 0.05, 6.0));
    g.steps(30);
    // Outside minigames it breaks nothing, not even its owner's bricks.
    let bobs = wall(&mut g, b, 0.0, -14.0);
    let alphas = wall(&mut g, a, 24.0, -14.0);
    fire(&mut g, a, 0.0, 25.0);
    fire(&mut g, a, 24.0, 25.0);
    g.steps(120);
    assert_eq!(standing(&g, &bobs), bobs.len(), "Bravo's wall stands");
    assert_eq!(standing(&g, &alphas), alphas.len(), "so does Alpha's own");
    // In a minigame with brick damage a hurl punches a hole straight
    // through the minigame's bricks and rolls on out the other side...
    g.minigame(a, &[b]);
    let fast = wall(&mut g, a, 8.0, -14.0);
    let slow = wall(&mut g, a, 16.0, -14.0);
    let ball = fire(&mut g, b, 8.0, 25.0);
    // ...while a gentle roll only bumps into them.
    fire(&mut g, b, 16.0, 11.0);
    g.steps(150);
    let knocked = fast.len() - standing(&g, &fast);
    assert!(knocked >= 3, "a 25 u/s ball broke {knocked} bricks");
    let (at, v) = g.vehicle(ball).unwrap();
    assert!(
        at.z < -18.0 && v.z < -8.0,
        "it rolled on through the wall: at {at}, moving {v}"
    );
    assert_eq!(standing(&g, &slow), slow.len(), "a roll breaks nothing");
    // A bunker four walls thick uses up its momentum: it breaks in, then
    // stops inside.
    let mut bunker = Vec::new();
    for layer in 0..4 {
        bunker.extend(wall(&mut g, a, 32.0, -14.0 - layer as f32));
        g.steps(40);
    }
    let ball = fire(&mut g, b, 32.0, 25.0);
    g.steps(120);
    let broken = bunker.len() - standing(&g, &bunker);
    let (at, _) = g.vehicle(ball).unwrap();
    assert!(broken >= 3, "it broke into the bunker: {broken}");
    assert!(at.z > -20.5, "but not out of it: at {at}");
}

/// A ball owned by Alpha rolled at `speed` into Bravo (or into Alpha,
/// `at_owner`), in a minigame or not: whether the target was bowled over,
/// their health after, and whether they were pushed.
fn bowl(in_minigame: bool, speed: f32, at_owner: bool) -> (bool, f32, bool) {
    let mut g = Game::new();
    let a = g.join("Alpha", Vec3::new(0.0, 0.05, 0.0));
    let b = g.join("Bravo", Vec3::new(0.0, 0.05, -12.0));

    g.steps(30);
    if in_minigame {
        g.minigame(a, &[b]);
    }
    let target = if at_owner { a } else { b };
    let before = g.feet(target);
    g.s.spawn_vehicle_at(
        a,
        BALL,
        before + Vec3::new(9.0, 1.3, 0.0),
        0.0,
        Vec3::new(-speed, 0.0, 0.0),
    )
    .unwrap();
    let mut bowled = false;
    for _ in 0..120 {
        g.steps(1);
        bowled |= g.s.mounted(target).is_some();
    }
    let health = if g.s.is_alive(target) {
        g.s.vitals()[&target].health
    } else {
        0.0
    };
    (bowled, health, g.feet(target).distance(before) > 0.5)
}

#[test]
fn a_steel_ball_only_bumps_players_outside_minigames_and_kills_when_hurled_inside() {
    let (bowled, health, pushed) = bowl(false, 24.0, false);
    assert!(!bowled, "never bowled over outside minigames");
    assert_eq!(health, 100.0, "no harm outside minigames");
    assert!(pushed, "but bumped aside");
    // In a minigame a gentle roll only bumps...
    let (bowled, health, _) = bowl(true, 11.0, false);
    assert!(!bowled && health == 100.0, "a roll only bumps");
    // ...it never harms the player it belongs to...
    let (bowled, health, _) = bowl(true, 24.0, true);
    assert!(!bowled && health == 100.0, "its owner is safe");
    // ...and a hurl kills whoever it hits.
    let (_, health, _) = bowl(true, 24.0, false);
    assert_eq!(health, 0.0, "a hurled ball kills");
}

#[test]
fn a_hard_hit_wrecks_a_vehicle_only_in_minigames() {
    let mut g = Game::new();
    let a = g.join("Alpha", Vec3::new(20.0, 0.05, 6.0));
    let b = g.join("Bravo", Vec3::new(-20.0, 0.05, 6.0));
    g.steps(30);
    // Alpha's crates: v20 minigames let members damage the vehicles of the
    // minigame's owner.
    let crate_at = |g: &mut Game, lane: f32| {
        g.s.spawn_vehicle_at(a, CRATE, Vec3::new(lane, 1.0, -14.0), 0.0, Vec3::ZERO)
            .unwrap()
    };
    // Outside minigames it only shoves the crate aside.
    let shoved = crate_at(&mut g, 0.0);
    g.steps(30);
    fire(&mut g, b, 0.0, 30.0);
    g.steps(90);
    let (at, _) = g.vehicle(shoved).expect("still there");
    assert!(at.z < -15.0, "shoved on: {at}");
    assert!(g.vehicles_of(CRATE).contains(&shoved));
    // In a minigame a roll only nudges one, and a hard hit wrecks one.
    g.minigame(a, &[b]);
    let nudged = crate_at(&mut g, 8.0);
    let wrecked = crate_at(&mut g, 16.0);
    g.steps(30);
    fire(&mut g, b, 8.0, 11.0);
    fire(&mut g, b, 16.0, 30.0);
    g.steps(90);
    let alive = g.vehicles_of(CRATE);
    assert!(alive.contains(&nudged), "a roll only nudges");
    assert!(!alive.contains(&wrecked), "a hard hit wrecks it");
}

#[test]
fn everyone_gets_the_gun_outside_minigames_and_the_loadout_decides_inside() {
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
        assert!(holds(&g, owner, GUN));
    }
    // A respawn sets the items afresh, and they come back.
    g.cmd(b, Command::Suicide).unwrap();
    g.steps(130);
    g.cmd(b, Command::Respawn).unwrap();
    g.steps(2);
    assert!(holds(&g, b, GUN));
    // In a minigame its loadout decides; asking by name is refused.
    g.minigame(a, &[b]);
    assert!(!holds(&g, b, GUN));
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
    ball_brick(&mut g, host, 0.0);
    g.steps(10);
    assert_eq!(g.vehicles_of(BALL).len(), 1);
    g.s.take_private_notices();
    ball_brick(&mut g, host, 8.0);
    g.steps(10);
    assert_eq!(
        g.vehicles_of(BALL).len(),
        1,
        "the quota holds the second back"
    );
    assert!(g.s.take_private_notices().iter().any(|(o, n)| *o == host
        && matches!(n, bri_sim::session::Notice::Center { text, .. }
            if text.ends_with("You already have a physics-vehicle"))));
}

/// Max, v0.1.9: swinging a held jeep and letting go gave it an extra
/// kick, like the old blast. Letting go adds nothing: the thing flies on
/// with the speed the swing gave it, and only gravity changes that.
#[test]
fn letting_go_carries_only_the_swing() {
    let mut g = Game::new();
    let host = g.join("Host", Vec3::new(0.0, 0.05, 0.0));
    g.s.give_tool(host, GUN, true).unwrap();
    let crate_ =
        g.s.spawn_vehicle_at(0, CRATE, Vec3::new(0.0, 1.0, -6.0), 0.0, Vec3::ZERO)
            .unwrap();
    g.steps(60);
    trigger(&mut g, host, true);
    g.look(host, 0.0, 0.3);
    g.steps(60);
    let mut yaw = 0.0;
    for _ in 0..18 {
        yaw += 0.06;
        g.look(host, yaw, 0.3);
        g.steps(1);
    }
    // The tick the trigger comes up still carries the hold's last pull.
    g.cmd(host, Command::WeaponTrigger { down: false }).unwrap();
    g.steps(1);
    let mut speeds = vec![g.vehicle(crate_).unwrap().1];
    for _ in 0..12 {
        g.steps(1);
        speeds.push(g.vehicle(crate_).unwrap().1);
    }
    assert!(g.s.held_by(host).is_none(), "let go");
    for w in speeds.windows(2) {
        let flat = |v: Vec3| Vec3::new(v.x, 0.0, v.z).length();
        assert!(
            flat(w[1]) <= flat(w[0]) + 0.05 && w[1].y <= w[0].y + 0.05,
            "sped up after letting go: {speeds:?}"
        );
    }
    assert!(speeds[0].length() > 15.0, "the swing flung it: {}", speeds[0]);
}

/// A world with one Blockhead Bot spawn brick, owned by owner 1 (the
/// principal `[1; 32]`).
fn bot_world() -> World {
    let mut world = World::new("Showcase".into(), "showcase".into(), vec![[1.0; 4], [0.6; 4]]);
    world
        .owners
        .insert(1, bri_world::OwnerRecord::new([1; 32], "Builder".into()));
    let mut brick = bri_world::Brick::new(
        bri_world::ContentRef::Resolved("brick".into()),
        [0.0, 0.3, -6.0],
        1,
    );
    brick.vehicle = Some(bri_world::VehicleSpawn {
        vehicle: bri_world::ContentRef::Resolved("bot.blockhead".into()),
        recolor: false,
    });
    world.bricks.insert(1, brick);
    world.next_brick_id = 2;
    world
}

/// Max, v0.1.9: the gun would not grab a Blockhead Bot, because Add-Ons
/// never saw bots at all. A bot is a player without a connection; it is
/// grabbed, carried and let go like one. Outside minigames its spawn
/// brick's owner decides who may move it, as for the vehicles such a brick
/// spawns.
#[test]
fn a_bot_is_grabbed_like_a_player() {
    let mut g = Game::with(bot_world());
    // The Blockhead Bot Add-On provides the kind the brick spawns.
    g.s.set_bot_kinds(
        bri_sim::bot_kind::BotPack::from_json(include_bytes!(
            "../../../packages/blockhead_bot/assets/bots.json"
        ))
        .unwrap()
        .bots,
    )
    .unwrap();
    // The brick's owner, not an administrator.
    let builder = g.join_verified("Builder", Vec3::new(0.0, 0.05, 0.0), 1);
    assert_eq!(builder, 1);
    let stranger = g.join_verified("Stranger", Vec3::new(4.0, 0.05, 0.0), 9);
    g.s.give_tool(builder, GUN, true).unwrap();
    g.steps(30);
    let bot = *g.s.names().keys().find(|o| g.s.is_bot(**o)).expect("a bot");
    assert!(!g.s.may_move(stranger, ObjectRef::Player(bot)), "not the stranger's");
    // Aim at it wherever it has wandered.
    let at = g.feet(bot) + Vec3::Y * 1.3 - (g.feet(builder) + Vec3::Y * 2.1);
    let flat = Vec3::new(at.x, 0.0, at.z).length();
    g.look(builder, at.x.atan2(-at.z), at.y.atan2(flat));
    g.steps(2);
    g.package(builder, "gravity-gun", "grab").unwrap();
    assert_eq!(g.s.held_by(builder), Some(ObjectRef::Player(bot)), "the bot is caught");
    assert_eq!(g.beam(builder)[..3], [2.0, bot as f64, 1.0]);
    let before = g.feet(bot);
    g.look(builder, 0.0, 0.5);
    g.steps(120);
    assert!(g.s.held_by(builder).is_some(), "still held");
    assert!(g.feet(bot).y > before.y + 1.0, "lifted: {before} -> {}", g.feet(bot));
    g.package(builder, "gravity-gun", "release").unwrap();
    g.steps(2);
    assert!(g.s.held_by(builder).is_none());
}

/// Max, v0.1.9: a held player spun round in the beam on the holder's
/// screen while on their own they hung still. A tumbling player watches
/// through the corpse camera, so their mouse turns nothing; their client
/// still sends the seat's world heading as its yaw, which the host took
/// for a passenger's turn on the seat, doubling every turn of the swing.
/// The body rides its tumble however the holder swings it.
#[test]
fn a_held_player_turns_only_with_their_tumble() {
    let mut g = Game::new();
    let a = g.join("Admin", Vec3::new(0.0, 0.05, 0.0));
    let b = g.join_verified("Bob", Vec3::new(0.0, 0.05, -5.0), 2);
    g.s.give_tool(a, GUN, true).unwrap();
    g.steps(60);
    g.package(a, "gravity-gun", "grab").unwrap();
    g.look(a, 0.0, 0.3);
    let heading = |g: &Game| {
        let (id, _) = g.s.mounted(b)?;
        let v = g.s.vehicle_poses().into_iter().find(|v| v.id == id)?;
        let forward = glam::Quat::from_array(v.rotation) * Vec3::NEG_Z;
        Some(forward.x.atan2(-forward.z))
    };
    let yaw = |g: &Game| {
        g.s.motion_states()
            .into_iter()
            .find(|(p, _)| p.owner == b)
            .map(|(p, _)| p.yaw)
            .unwrap()
    };
    let wrap = |a: f32| {
        (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
    };
    g.steps(40);
    let seat = wrap(yaw(&g) - heading(&g).expect("held players tumble"));
    // Swing them half way round; their client sends what it sends while
    // tumbling: the seat's heading as it sees it.
    let mut turned = 0.0;
    for tick in 0..120 {
        turned += 0.025;
        g.look(a, turned, 0.3);
        let h = heading(&g).expect("still held");
        g.look(b, wrap(h + seat), 0.0);
        g.steps(1);
        if tick > 5 {
            let h = heading(&g).unwrap();
            let off = wrap(yaw(&g) - h - seat);
            assert!(off.abs() < 0.05, "tick {tick}: body {off} off its tumble");
        }
    }
    assert!(wrap(heading(&g).unwrap()).abs() > 1.5, "the swing turned the tumble");
}

/// Max, v0.1.9: "they shouldn't always tumble if i move them gently and
/// carefully somewhere", "maybe if i toss them and they fly and hit a
/// wall". Let go, a held player has their body back at once: set down
/// gently they stand; thrown, they fly and slide to a stop on their feet,
/// unless they hit something hard, which tumbles them.
#[test]
fn a_thrown_player_tumbles_only_when_they_hit_something_hard() {
    let hold_and_let_go = |swing: f32, walled: bool| {
        let mut g = Game::new();
        let a = g.join("Admin", Vec3::new(0.0, 0.05, 0.0));
        let b = g.join_verified("Bob", Vec3::new(0.0, 0.05, -5.0), 2);
        if walled {
            // Across the line a hard swing throws them along.
            wall(&mut g, a, 10.0, 4.0);
        }
        g.s.give_tool(a, GUN, true).unwrap();
        g.steps(60);
        g.look(a, 0.0, 0.1);
        trigger(&mut g, a, true);
        g.steps(90);
        assert!(g.s.mounted(b).is_some(), "held players tumble");
        let mut yaw = 0.0;
        for _ in 0..18 {
            yaw += swing;
            g.look(a, yaw, 0.1);
            g.steps(1);
        }
        g.cmd(a, Command::WeaponTrigger { down: false }).unwrap();
        g.steps(1);
        assert!(g.s.held_by(a).is_none(), "let go");
        assert!(g.s.mounted(b).is_none(), "their own body again");
        (g, b)
    };
    let tumbled = |g: &mut Game, b: OwnerId, ticks: usize| {
        (0..ticks).any(|_| {
            g.steps(1);
            g.s.mounted(b).is_some()
        })
    };
    // Carried a little way round, slowly, and let go: they stand there.
    let (mut g, b) = hold_and_let_go(0.005, false);
    let put = g.feet(b);
    assert!(!tumbled(&mut g, b, 120), "set down gently");
    let feet = g.feet(b);
    assert!(feet.y < put.y + 0.05, "fell or stood, never rose: {put} -> {feet}");
    // Swung hard and let go in the open: they fly, land and slide.
    let (mut g, b) = hold_and_let_go(0.08, false);
    let from = g.feet(b);
    assert!(!tumbled(&mut g, b, 240), "nothing hard to hit");
    assert!(g.feet(b).distance(from) > 15.0, "thrown far: {from} -> {}", g.feet(b));
    // The same throw into a wall.
    let (mut g, b) = hold_and_let_go(0.08, true);
    assert!(tumbled(&mut g, b, 120), "hit the wall hard");
}

/// A `definition` of Alpha's rolled at `speed` into Bravo, in a minigame or
/// not: its speed along the roll just before and just after the hit (the
/// biggest one-tick drop), and a second and a half after it was rolled.
fn run_into_player(definition: &str, in_minigame: bool, speed: f32) -> (f32, f32, f32) {
    let mut g = Game::new();
    let a = g.join("Alpha", Vec3::new(0.0, 0.05, 0.0));
    let b = g.join("Bravo", Vec3::new(0.0, 0.05, -12.0));
    g.steps(30);
    if in_minigame {
        g.minigame(a, &[b]);
    }
    let at = g.feet(b);
    let id =
        g.s.spawn_vehicle_at(
            a,
            definition,
            at + Vec3::new(9.0, 1.3, 0.0),
            0.0,
            Vec3::new(-speed, 0.0, 0.0),
        )
        .unwrap();
    let mut speeds = vec![speed];
    for _ in 0..180 {
        g.steps(1);
        speeds.push(-g.vehicle(id).unwrap().1.x);
    }
    let hit = (1..speeds.len())
        .max_by(|i, j| (speeds[i - 1] - speeds[*i]).total_cmp(&(speeds[j - 1] - speeds[*j])))
        .unwrap();
    (speeds[hit - 1], speeds[hit], speeds[180])
}

/// Max, v0.1.10: a Steel Ball that hit a player stopped dead. The solver
/// took a walking player for an immovable wall; now the two share the hit
/// by weight, so a 900 kg ball barely slows for a 90 kg player and rolls
/// on, and a 300 kg crate slows more but never bounces back off one.
#[test]
fn a_vehicle_rolls_on_through_a_player_it_hits() {
    // Outside minigames it only bumps; in one (Max: "IN A MINIGAME") a
    // roll bumps, a harder hit bowls them over and a hurl kills.
    for (in_minigame, speed) in [(false, 24.0), (true, 13.0), (true, 16.0), (true, 24.0)] {
        let (before, after, later) = run_into_player(BALL, in_minigame, speed);
        assert!(
            after > before * 0.75,
            "the ball barely slows (minigame {in_minigame}, {speed} u/s): {before} -> {after}"
        );
        assert!(
            later > 3.0,
            "and rolls on (minigame {in_minigame}, {speed} u/s): {later}"
        );
    }
    let (before, after, _) = run_into_player(CRATE, false, 24.0);
    assert!(
        after > before * 0.5 && after < before * 0.85,
        "the crate slows more, but never bounces back: {before} -> {after}"
    );
}

/// A content-free launcher (`kind`: "rocket" or "tankshell") whose shell
/// explodes with v20's blast push: `impulse` away from the blast within
/// `radius`, plus `vertical` up, both fading with distance squared.
fn add_launcher(
    pack: &mut bri_weapons::Pack,
    kind: &str,
    impulse: f32,
    vertical: f32,
    radius: f32,
) {
    let item_id = format!("test:item/{kind}");
    let image_id = format!("test:image/{kind}");
    let projectile_id = format!("test:projectile/{kind}");
    let state = |name: &str, ticks, script: &str| bri_weapons::State {
        name: name.into(),
        ticks,
        wait: true,
        allow_change: true,
        script: script.into(),
        ..Default::default()
    };
    let states = vec![
        bri_weapons::State {
            timeout: Some(1),
            ..state("Activate", 0, "")
        },
        bri_weapons::State {
            down: Some(2),
            ..state("Ready", 0, "")
        },
        bri_weapons::State {
            timeout: Some(3),
            ..state("Fire", 60, "onFire")
        },
        bri_weapons::State {
            timeout: Some(1),
            ..state("Reload", 0, "")
        },
    ];
    let image = bri_weapons::Image {
        id: image_id.clone(),
        name: "rocketLauncherImage".into(),
        model: String::new(),
        projectile: Some(projectile_id.clone()),
        mount_point: 0,
        offset: [0.; 3],
        eye_offset: [0.; 3],
        source_rotation_degrees: [0.; 3],
        correct_muzzle: false,
        melee: false,
        color: [1.; 4],
        color_shift: false,
        arm_ready: true,
        casing: String::new(),
        min_shot_ticks: 0,
        command: Default::default(),
        commands: Default::default(),
        shot: None,
        eye_rotation: [0.0; 3],
        zoom: None,
        crosshair: true,
        follow_arm: false,
        paint_tint: false,
        left_image: None,
        magazine: None,
        volleys: vec![],
        last_shot: None,
        state_shots: Default::default(),
        cook: None,
        paint_picker: false,
        scripts: Default::default(),
        hide_nodes: Vec::new(),
        both_arms: false,
        states,
    };
    let item = bri_weapons::Item {
        id: item_id.clone(),
        name: "rocketLauncherItem".into(),
        ui_name: "Rocket L.".into(),
        image: image_id.clone(),
        ..Default::default()
    };
    let projectile = bri_weapons::ProjectileDef {
        id: projectile_id.clone(),
        name: "rocketLauncherProjectile".into(),
        model: String::new(),
        speed: 40.,
        inherit: 0.,
        gravity: 0.,
        lifetime_ticks: 480,
        fade_ticks: 0,
        arm_ticks: 0,
        ballistic: false,
        elasticity: 0.,
        friction: 0.,
        damage: 0.,
        damage_type: String::new(),
        radius_damage_type: String::new(),
        impulse: 0.,
        vertical: 0.,
        explode_player: true,
        explode_death: true,
        collide_players: true,
        explosion: bri_weapons::Explosion {
            effect: String::new(),
            damage: 0.,
            radius: 0.,
            impulse,
            impulse_radius: radius,
            impulse_vertical: vertical,
            burn_seconds: 0.,
        },
        brick: bri_weapons::BrickImpact {
            radius: 0.,
            direct: true,
            force: 20.,
            max_volume: 1000.,
            max_floating_volume: 1000.,
        },
        bounce_effect: String::new(),
        stick_effect: String::new(),
        blood_effect: String::new(),
        bounce_angle: 0.,
        min_stick_speed: 0.,
        trail: String::new(),
        sound: String::new(),
        light_radius: 0.,
        light_color: [0.; 3],
        sport_image: None,
        rest_speed: 0.,
        max_bounces: 0,
        children: Vec::new(),
        aura: None,
        fixed_damage: false,
        slow: None,
    };
    pack.items.insert(item_id, item);
    pack.images.insert(image_id, image);
    pack.projectiles.insert(projectile_id, projectile);
}

/// Alpha's blast from a `kind` launcher pushing `impulse` out within
/// `radius` on the ground beside a resting Steel Ball, in a minigame or
/// not: how far it rolled away in two seconds.
fn blast_beside_ball(kind: &str, impulse: f32, radius: f32, in_minigame: bool) -> f32 {
    let mut g = Game::new();
    let mut pack = weapons();
    add_launcher(&mut pack, kind, impulse, 0.0, radius);
    pack.validate().unwrap();
    g.s.set_weapon_pack(pack).unwrap();
    let a = g.join("Alpha", Vec3::new(0.0, 0.05, 0.0));
    g.steps(30);
    let ball =
        g.s.spawn_vehicle_at(a, BALL, Vec3::new(0.0, 1.3, -10.0), 0.0, Vec3::ZERO)
            .unwrap();
    g.steps(120);
    if in_minigame {
        g.minigame(a, &[]);
    }
    let (start, _) = g.vehicle(ball).unwrap();
    g.s.give_tool(a, &format!("test:item/{kind}"), true)
        .unwrap();
    g.steps(60);
    // The ground 2.5 to the ball's left, from the eye.
    let d = start + Vec3::new(2.5, -1.25, 0.0) - Vec3::new(0.0, 2.2, 0.0);
    g.look(
        a,
        d.x.atan2(-d.z),
        d.y.atan2(Vec3::new(d.x, 0.0, d.z).length()),
    );
    g.steps(5);
    g.cmd(a, Command::WeaponTrigger { down: true }).unwrap();
    g.steps(2);
    g.cmd(a, Command::WeaponTrigger { down: false }).unwrap();
    g.steps(240);
    let (end, _) = g.vehicle(ball).unwrap();
    start.x - end.x
}

/// Max, v0.1.10: "explosions from rockets or tank shells don't move steel
/// ball". By v20's rule a blast moves a vehicle by its impulse over its
/// mass, which barely stirs 900 kg of steel; the ball's `blast_scale`
/// knocks it about as a third of that weight would be.
#[test]
fn rockets_and_tank_shells_knock_the_steel_ball_away() {
    // The stock explosions' pushes: the rocket's 4000 within 6, the tank
    // shell's 5000 within 15.
    for in_minigame in [false, true] {
        let rocket = blast_beside_ball("rocket", 4000.0, 6.0, in_minigame);
        assert!(
            rocket > 6.0,
            "a rocket rolls it away (minigame {in_minigame}): {rocket}"
        );
        let shell = blast_beside_ball("tankshell", 5000.0, 15.0, in_minigame);
        assert!(
            shell > 6.0,
            "so does a tank shell (minigame {in_minigame}): {shell}"
        );
    }
}
