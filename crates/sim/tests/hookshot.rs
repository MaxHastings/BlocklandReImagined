//! The HookShot Add-On (`packages/showcase/hookshot`) played through the
//! authoritative session: a click shoots the spearhead, and if it bites
//! the chain hauls the player straight to the spot. Tapped, it lets go
//! there; held, they hang there, winching with jump and crouch, until the
//! next click. A click mid-flight lets go early, and so does putting it
//! away.
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::{Definition, Definitions},
    player::MoveInput,
    session::{ActionAim, Command, PackageArg, PackageCommand, Reply, Session},
    simulation::Simulation,
};
use bri_world::{OwnerId, World};
use glam::Vec3;
use rapier3d::prelude::*;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

const HOOK: &str = "hookshot-tool:weapon/hookshot";
/// The map's wall: its face is 30 units ahead (-z), 30 high.
const WALL: f32 = -30.0;

fn showcase() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/showcase")
}

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

fn add_ons() -> Arc<Catalog> {
    let packages = [("hookshot", Side::Server), ("hookshot-tool", Side::Shared)]
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
                World::new("Jungle".into(), "jungle".into(), vec![[1.0; 4], [0.6; 4]]),
                definitions(),
                vec![
                    ColliderBuilder::cuboid(200.0, 0.5, 200.0)
                        .translation(Vector::new(0.0, -0.5, 0.0)),
                    ColliderBuilder::cuboid(12.0, 15.0, 0.5).translation(Vector::new(
                        0.0,
                        15.0,
                        WALL - 0.5,
                    )),
                ],
            )
            .unwrap(),
        );
        s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 0.0)]).unwrap();
        let pack = bri_weapons::Pack::from_json(
            &std::fs::read(showcase().join("hookshot-tool/assets/weapons.json")).unwrap(),
        )
        .unwrap();
        s.set_weapon_pack(pack).unwrap();
        s.install_packages(add_ons(), None).unwrap();
        Self {
            s,
            seq: BTreeMap::new(),
            moves: BTreeMap::new(),
            looks: BTreeMap::new(),
        }
    }
    fn join(&mut self, name: &str, at: Vec3) -> OwnerId {
        let owner = self.s.join(name.into(), at, true).unwrap();
        self.looks.insert(owner, MoveInput::default());
        owner
    }
    fn cmd(&mut self, owner: OwnerId, command: Command) -> anyhow::Result<Reply> {
        let n = self.seq.entry(owner).or_default();
        *n += 1;
        self.s.command(owner, *n, command)
    }
    fn package(&mut self, owner: OwnerId, command: &str, args: Vec<PackageArg>) {
        let look = self.looks[&owner];
        let n = self.seq.entry(owner).or_default();
        *n += 1;
        self.s
            .command_with_aim(
                owner,
                *n,
                Command::Package(PackageCommand {
                    package: "hookshot".into(),
                    command: command.into(),
                    args,
                }),
                Some(ActionAim {
                    yaw: look.yaw,
                    pitch: look.pitch,
                }),
            )
            .unwrap();
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
    fn hook(&self, owner: OwnerId) -> Vec<f64> {
        self.s
            .package_state()
            .packages
            .get("hookshot")
            .and_then(|ns| ns.players.get(&owner))
            .and_then(|m| m.get("hook"))
            .and_then(|v| v.as_array())
            .map(|a| a.iter().map(|x| x.as_f64().unwrap()).collect())
            .unwrap_or_default()
    }
    /// One left click: press and let go.
    fn click(&mut self, owner: OwnerId) {
        self.cmd(owner, Command::WeaponTrigger { down: true })
            .unwrap();
        self.steps(2);
        self.cmd(owner, Command::WeaponTrigger { down: false })
            .unwrap();
        self.steps(8);
    }
    /// Join with the hook in hand, ready to fire.
    /// Press and hold for a third of a second (a hold, not a tap), then
    /// let go.
    fn hold(&mut self, owner: OwnerId) {
        self.cmd(owner, Command::WeaponTrigger { down: true })
            .unwrap();
        self.steps(40);
        self.cmd(owner, Command::WeaponTrigger { down: false })
            .unwrap();
        self.steps(2);
    }
    fn walk(&mut self, owner: OwnerId, forward: f32) {
        self.looks.get_mut(&owner).unwrap().forward = forward;
    }
    fn hooker(&mut self, name: &str, at: Vec3) -> OwnerId {
        let owner = self.join(name, at);
        self.steps(30);
        self.package(owner, "hookshot", vec![]);
        self.steps(30);
        owner
    }
    fn velocity(&self, owner: OwnerId) -> Vec3 {
        self.s
            .motion_states()
            .into_iter()
            .find(|(p, _)| p.owner == owner)
            .map(|(p, _)| Vec3::from(p.velocity))
            .unwrap()
    }
}

#[test]
fn a_click_flies_them_to_the_spot_and_lets_go_there() {
    let mut g = Game::new();
    let a = g.hooker("Link", Vec3::new(0.0, 0.05, 0.0));
    assert_eq!(g.hook(a)[0], 0.0);
    g.look(a, 0.0, 0.35);
    g.click(a);
    let hook = g.hook(a);
    assert_eq!(hook[0], 1.0, "flying: {hook:?}");
    g.steps(40);
    let hook = g.hook(a);
    assert_eq!(hook[0], 2.0, "{hook:?}");
    assert!(
        (hook[3] - f64::from(WALL)).abs() < 0.05,
        "in the wall: {hook:?}"
    );
    let anchor = Vec3::new(hook[1] as f32, hook[2] as f32, hook[3] as f32);
    assert!(g.s.tether_of(a).unwrap().straight);
    // Hauled straight there, fast, then let go on arrival.
    let grip = |g: &Game| g.feet(a) + Vec3::Y * 2.65 * 0.85;
    let (mut fastest, mut nearest) = (0.0_f32, f32::MAX);
    for _ in 0..240 {
        g.steps(1);
        fastest = fastest.max(g.velocity(a).length());
        nearest = nearest.min(grip(&g).distance(anchor));
        if g.hook(a)[0] == 0.0 {
            break;
        }
    }
    assert!(fastest > 40.0, "fast: {fastest}");
    assert!(nearest < 2.7, "got there: {nearest}");
    assert_eq!(g.hook(a)[0], 0.0, "let go on arrival");
    assert!(g.s.tether_of(a).is_none());
    // And dropped to the floor below, unhurt.
    g.steps(480);
    assert!(g.feet(a).y < 0.1, "landed: {}", g.feet(a));
    assert!(
        g.s.motion_states().into_iter().any(|(p, _)| p.owner == a),
        "still in the game"
    );
}

#[test]
fn a_second_click_or_putting_it_away_lets_go_mid_flight() {
    let mut g = Game::new();
    let a = g.hooker("Link", Vec3::new(0.0, 0.05, 0.0));
    g.look(a, 0.0, 0.35);
    g.click(a);
    g.steps(30);
    assert_eq!(g.hook(a)[0], 2.0);
    let hauling = g.velocity(a).length();
    assert!(hauling > 40.0, "hauling: {hauling}");
    g.click(a);
    assert!(g.s.tether_of(a).is_none());
    assert_eq!(g.hook(a)[0], 0.0);
    // The grip slowed them as it let go: a tumble, not a splat.
    let after = g.velocity(a).length();
    assert!(after < hauling * 0.45, "slowed: {hauling} -> {after}");
    g.steps(480);
    assert!(g.feet(a).y < 0.1, "landed: {}", g.feet(a));
    // Again, then put away.
    g.click(a);
    g.steps(30);
    assert!(g.s.tether_of(a).is_some());
    g.cmd(a, Command::EquipTool { slot: None }).unwrap();
    g.steps(12);
    assert!(g.s.tether_of(a).is_none());
    assert_eq!(g.hook(a)[0], 0.0);
}

#[test]
fn it_flies_them_to_another_player_and_misses_into_open_sky() {
    let mut g = Game::new();
    let a = g.hooker("Link", Vec3::new(0.0, 0.05, 0.0));
    let b = g.join("Zelda", Vec3::new(0.0, 0.05, -14.0));
    g.steps(30);
    g.look(a, 0.0, -0.05);
    g.click(a);
    g.steps(20);
    let hook = g.hook(a);
    assert_eq!(hook[0], 2.0, "{hook:?}");
    assert_eq!(hook[5], 1.0, "bit a player: {hook:?}");
    assert_eq!(hook[6] as u64, b);
    g.steps(240);
    assert_eq!(g.hook(a)[0], 0.0, "arrived and let go");
    assert!(
        g.feet(a).distance(g.feet(b)) < 3.5,
        "at Zelda: {} {}",
        g.feet(a),
        g.feet(b)
    );
    // Away from the wall, level: open sky. Out and back, holding nothing.
    g.look(a, std::f32::consts::PI, 0.3);
    g.click(a);
    assert_eq!(g.hook(a)[0], 3.0);
    g.steps(240);
    assert_eq!(g.hook(a)[0], 0.0, "back in");
    assert!(g.s.tether_of(a).is_none());
}

#[test]
fn everyone_gets_the_hookshot_outside_minigames() {
    let mut g = Game::new();
    let a = g.join("Link", Vec3::new(0.0, 0.05, 0.0));
    g.steps(30);
    assert!(
        g.s.tool_inventories()[&a]
            .slots
            .iter()
            .any(|s| s.as_deref() == Some(HOOK))
    );
}

#[test]
fn a_held_click_hangs_them_at_the_wall_until_the_next_click() {
    let mut g = Game::new();
    let a = g.hooker("Batman", Vec3::new(0.0, 0.05, 0.0));
    // Aim up the wall ahead and hold.
    g.look(a, 0.0, 0.5);
    g.hold(a);
    g.steps(20);
    let hook = g.hook(a);
    assert_eq!(hook[0], 2.0, "{hook:?}");
    assert!(
        (hook[3] - f64::from(WALL)).abs() < 0.05,
        "in the wall: {hook:?}"
    );
    let anchor = Vec3::new(hook[1] as f32, hook[2] as f32, hook[3] as f32);
    assert!(anchor.y > 10.0, "{anchor}");
    // Hauled all the way in, arriving gently, and it holds them there.
    let mut fastest = 0.0_f32;
    for _ in 0..240 {
        g.steps(1);
        fastest = fastest.max(g.velocity(a).length());
    }
    let grip = |g: &Game| g.feet(a) + Vec3::Y * 2.65 * 0.85;
    let t = g.s.tether_of(a).expect("still hooked");
    assert!((t.length - 2.0).abs() < 0.01, "{t:?}");
    assert!(
        grip(&g).distance(anchor) < 3.0,
        "at the wall: {}",
        grip(&g).distance(anchor)
    );
    assert!(g.feet(a).y > 8.0, "hanging: {}", g.feet(a));
    assert!(fastest > 20.0, "fastest {fastest}");
    g.steps(120);
    assert!(grip(&g).distance(anchor) < 3.0);
    assert!(g.velocity(a).length() < 1.0);
    // Switching items keeps them hanging.
    g.cmd(a, Command::EquipTool { slot: None }).unwrap();
    g.steps(12);
    assert!(g.s.tether_of(a).is_some());
    assert_eq!(g.hook(a)[0], 2.0);
    // With it back in hand, a click lets go and they fall.
    let slot = g.s.tool_inventories()[&a]
        .slots
        .iter()
        .position(|s| s.as_deref() == Some(HOOK))
        .unwrap();
    g.cmd(a, Command::EquipTool { slot: Some(slot) }).unwrap();
    g.steps(30);
    g.click(a);
    assert!(g.s.tether_of(a).is_none());
    assert_eq!(g.hook(a)[0], 0.0);
    g.steps(360);
    assert!(g.feet(a).y < 0.1, "fell to the floor: {}", g.feet(a));
}

#[test]
fn hanging_jump_reels_in_and_crouch_lets_out() {
    let mut g = Game::new();
    let a = g.hooker("Batman", Vec3::new(0.0, 0.05, 0.0));
    g.look(a, 0.0, 0.5);
    g.hold(a);
    g.steps(360);
    let at_wall = g.s.tether_of(a).unwrap().length;
    // Crouch held: chain pays out, and stops (braking) once let go.
    g.looks.get_mut(&a).unwrap().crouch = true;
    g.steps(120);
    g.looks.get_mut(&a).unwrap().crouch = false;
    g.steps(60);
    let out = g.s.tether_of(a).unwrap();
    assert!(out.length > at_wall + 10.0, "{at_wall} -> {out:?}");
    g.steps(120);
    let held = g.s.tether_of(a).unwrap();
    assert!(
        (held.length - out.length).abs() < 0.5,
        "stopped: {out:?} {held:?}"
    );
    // Jump held: back in.
    g.looks.get_mut(&a).unwrap().jump = true;
    g.steps(360);
    g.looks.get_mut(&a).unwrap().jump = false;
    g.steps(4);
    let back = g.s.tether_of(a).unwrap();
    assert!((back.length - 2.0).abs() < 0.5, "{back:?}");
    // The wheel reels too.
    g.package(a, "reel", vec![PackageArg::Int(-3)]);
    g.steps(240);
    assert!(g.s.tether_of(a).unwrap().length > 8.0);
    // Tapped, the wheel does nothing: they are flying, not hanging.
    g.click(a);
    g.steps(360);
    g.click(a);
    g.package(a, "reel", vec![PackageArg::Int(-3)]);
    g.steps(4);
    assert!(g.s.tether_of(a).is_none_or(|t| t.target < 3.0));
}

#[test]
fn held_on_a_player_it_carries_them_along_unless_an_admin_says_no() {
    let mut g = Game::new();
    let a = g.hooker("Batman", Vec3::new(0.0, 0.05, 0.0));
    let b = g.join("Robin", Vec3::new(0.0, 0.05, -10.0));
    g.steps(30);
    // Level at Robin's chest.
    g.look(a, 0.0, -0.05);
    g.hold(a);
    g.steps(20);
    let hook = g.hook(a);
    assert_eq!(hook[0], 2.0, "{hook:?}");
    assert_eq!(hook[5], 1.0, "bit a player: {hook:?}");
    assert_eq!(hook[6] as u64, b);
    g.steps(240);
    let near = g.feet(a).distance(g.feet(b));
    assert!(near < 4.0, "hauled to Robin: {near}");
    // Robin runs off (away from the wall): Batman comes too.
    g.look(b, std::f32::consts::FRAC_PI_2, 0.0);
    g.walk(b, 1.0);
    g.steps(480);
    g.walk(b, 0.0);
    let moved = g.feet(b);
    assert!(
        moved.distance(Vec3::new(0.0, 0.05, -10.0)) > 10.0,
        "{moved}"
    );
    assert!(g.s.tether_of(a).is_some());
    assert!(
        g.feet(a).distance(moved) < 6.0,
        "carried: {} vs {moved}",
        g.feet(a)
    );
    g.click(a);
    assert!(g.s.tether_of(a).is_none());
    // Batman is an admin: players and vehicles are off, and a shot at
    // Robin now misses.
    g.package(a, "hookobjects", vec![]);
    g.steps(4);
    let d = g.feet(b) - g.feet(a);
    g.look(a, d.x.atan2(-d.z), -0.1);
    g.cmd(a, Command::WeaponTrigger { down: true }).unwrap();
    g.steps(2);
    assert_eq!(g.hook(a)[0], 3.0, "{:?}", g.hook(a));
    g.cmd(a, Command::WeaponTrigger { down: false }).unwrap();
    g.steps(240);
    assert!(g.s.tether_of(a).is_none());
}
