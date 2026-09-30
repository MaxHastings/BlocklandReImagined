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
    /// Press and release jet (the right mouse button).
    fn jet(&mut self, owner: OwnerId) {
        self.looks.get_mut(&owner).unwrap().jet = true;
        self.steps(2);
        self.looks.get_mut(&owner).unwrap().jet = false;
        self.steps(2);
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
    fn beam(&self, owner: OwnerId) -> Vec<i64> {
        self.s
            .package_state()
            .packages
            .get("gravity-gun")
            .and_then(|ns| ns.players.get(&owner))
            .and_then(|m| m.get("beam"))
            .and_then(|v| v.as_array())
            .map(|a| a.iter().map(|x| x.as_i64().unwrap()).collect())
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

#[test]
fn the_gravity_gun_grabs_holds_swings_and_drops_a_vehicle() {
    let mut g = Game::new();
    let host = g.join("Host", Vec3::new(0.0, 0.05, 0.0));
    g.s.give_tool(host, GUN, true).unwrap();
    let crate_ =
        g.s.spawn_vehicle_at(0, CRATE, Vec3::new(0.0, 1.0, -9.0), 0.0, Vec3::ZERO)
            .unwrap();
    g.steps(60);
    // Right click, the tool's jet command, grabs what the player looks at.
    g.jet(host);
    assert_eq!(g.s.held_by(host), Some(ObjectRef::Vehicle(crate_)));
    assert_eq!(g.beam(host)[..3], [1, crate_ as i64, 0]);
    g.steps(180);
    let eye = g.feet(host) + Vec3::Y * 2.0;
    let (at, _) = g.vehicle(crate_).unwrap();
    assert!(
        at.z < -3.0 && at.z > -6.5,
        "dragged in and held ahead: {at}"
    );
    assert!(at.y > 0.8, "held off the ground: {at}");
    assert!(
        (at.x).abs() < 0.8 && (at.y - eye.y).abs() < 2.0,
        "{at} vs eye {eye}"
    );
    // Turning a quarter to the right swings it round with the view.
    g.look(host, std::f32::consts::FRAC_PI_2, 0.0);
    g.steps(240);
    let (at, _) = g.vehicle(crate_).unwrap();
    assert!(at.x > 3.0 && at.z.abs() < 1.5, "swung to the right: {at}");
    // Right click again lets go; it falls and rests on the floor.
    g.jet(host);
    assert_eq!(g.s.held_by(host), None);
    g.steps(240);
    assert_eq!(g.beam(host)[..2], [0, 0]);
    let (at, v) = g.vehicle(crate_).unwrap();
    assert!(
        at.y < 1.3 && v.length() < 0.5,
        "dropped and resting: {at} {v}"
    );
}

#[test]
fn a_charged_throw_flies_farther_than_a_tap() {
    let throw = |charge_ticks: usize| {
        let mut g = Game::new();
        let host = g.join("Host", Vec3::new(0.0, 0.05, 0.0));
        g.s.give_tool(host, GUN, true).unwrap();
        let crate_ =
            g.s.spawn_vehicle_at(0, CRATE, Vec3::new(0.0, 1.0, -6.0), 0.0, Vec3::ZERO)
                .unwrap();
        g.steps(30);
        g.jet(host);
        assert_eq!(g.s.held_by(host), Some(ObjectRef::Vehicle(crate_)));
        g.steps(120);
        // Left click: held down charges, released throws.
        g.cmd(host, Command::WeaponTrigger { down: true }).unwrap();
        g.steps(charge_ticks);
        g.cmd(host, Command::WeaponTrigger { down: false }).unwrap();
        g.steps(4);
        assert_eq!(g.s.held_by(host), None, "thrown, not held");
        assert_eq!(g.beam(host)[3], 1, "one shot fired");
        let (_, v) = g.vehicle(crate_).unwrap();
        g.steps(120);
        (v.length(), g.vehicle(crate_).unwrap().0.z)
    };
    let (tap, tap_z) = throw(2);
    let (full, full_z) = throw(100);
    assert!(tap > 15.0, "even a tap throws: {tap}");
    assert!(
        full > tap * 2.0,
        "a full charge throws much harder: {full} vs {tap}"
    );
    assert!(
        full_z < tap_z - 10.0,
        "and much farther: {full_z} vs {tap_z}"
    );
}

#[test]
fn a_thrown_heavy_vehicle_kills_in_a_minigame_and_credits_the_thrower() {
    let mut g = Game::new();
    let a = g.join("Alpha", Vec3::new(0.0, 0.05, 0.0));
    let b = g.join("Bravo", Vec3::new(0.0, 0.05, -26.0));
    g.minigame(a, &[b]);
    // A minigame hands out its own loadout; the gun is given again.
    g.s.give_tool(a, GUN, true).unwrap();
    let crate_ =
        g.s.spawn_vehicle_at(a, CRATE, Vec3::new(0.0, 1.0, -6.0), 0.0, Vec3::ZERO)
            .unwrap();
    g.steps(30);
    g.package(a, "gravity-gun", "grab").unwrap();
    assert_eq!(g.s.held_by(a), Some(ObjectRef::Vehicle(crate_)));
    g.steps(120);
    g.package(a, "gravity-gun", "charge").unwrap();
    g.steps(90);
    g.package(a, "gravity-gun", "fire").unwrap();
    let mut dead = false;
    for _ in 0..240 {
        g.steps(1);
        if !g.s.is_alive(b) {
            dead = true;
            break;
        }
    }
    assert!(dead, "the thrown crate crushed Bravo");
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
    g.steps(15);
    g.package(a, "gravity-gun", "grab").unwrap();
    assert_eq!(g.s.held_by(a), None, "right click again drops it");
    g.look(a, 0.0, 0.0);
    g.steps(15);
    g.package(a, "gravity-gun", "grab").unwrap();
    assert_eq!(g.s.held_by(a), Some(ObjectRef::Player(b)));
    g.steps(120);
    assert!(g.feet(b).y > 0.5, "Bob is lifted: {}", g.feet(b));
    // A punt throws him into a tumble, but outside a minigame it cannot hurt.
    g.package(a, "gravity-gun", "fire").unwrap();
    g.steps(2);
    assert!(g.s.mounted(b).is_some(), "Bob tumbles");
    g.steps(600);
    assert!(g.s.is_alive(b));
    assert_eq!(g.s.vitals()[&b].health, 100.0);
}

#[test]
fn a_punt_in_a_minigame_throws_a_player_into_a_tumble() {
    let mut g = Game::new();
    let a = g.join("Alpha", Vec3::new(0.0, 0.05, 0.0));
    let b = g.join("Bravo", Vec3::new(0.0, 0.05, -5.0));
    g.minigame(a, &[b]);
    g.s.give_tool(a, GUN, true).unwrap();
    g.steps(10);
    let before = g.feet(b);
    g.package(a, "gravity-gun", "fire").unwrap();
    g.steps(2);
    assert!(g.s.mounted(b).is_some(), "Bravo is knocked into a tumble");
    assert_eq!(g.beam(a)[3..], [1, 2, b as i64], "the shot names Bravo");
    g.steps(60);
    let flown = g.feet(b).distance(before);
    assert!(flown > 5.0, "punted {flown} units");
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
fn right_click_with_the_gun_grabs_without_jetting() {
    let rise = |gun: bool| {
        let mut g = Game::new();
        let host = g.join("Host", Vec3::new(0.0, 0.05, 0.0));
        g.steps(2);
        if gun {
            g.s.give_tool(host, GUN, true).unwrap();
        } else {
            g.s.give_tool(host, GUN, false).unwrap();
            g.cmd(host, Command::EquipTool { slot: None }).unwrap();
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
        armed.abs() < 0.05,
        "the gun takes right click: rose {armed}"
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
