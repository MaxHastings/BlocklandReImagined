//! The Grappling Hook Add-On (`packages/showcase/grappling-hook`) played
//! through the authoritative session: one click fires the hook and the
//! winch pulls the player straight to it, they hang there (switching items
//! too) until the next click, the jump and crouch keys reel, and a hook in
//! another player carries them along.
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::{Definition, Definitions},
    player::MoveInput,
    session::{
        ActionAim, Command, PackageArg, PackageCommand, Reply, Session,
    },
    simulation::Simulation,
};
use bri_world::{OwnerId, World};
use glam::Vec3;
use rapier3d::prelude::*;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

const HOOK: &str = "grappling-hook-tool:weapon/grapplinghook";
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
    let packages = [
        ("grappling-hook", Side::Server),
        ("grappling-hook-tool", Side::Shared),
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
                    ColliderBuilder::cuboid(12.0, 15.0, 0.5)
                        .translation(Vector::new(0.0, 15.0, WALL - 0.5)),
                ],
            )
            .unwrap(),
        );
        s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 0.0)]).unwrap();
        let pack = bri_weapons::Pack::from_json(
            &std::fs::read(showcase().join("grappling-hook-tool/assets/weapons.json")).unwrap(),
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
                    package: "grappling-hook".into(),
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
    fn walk(&mut self, owner: OwnerId, forward: f32) {
        self.looks.get_mut(&owner).unwrap().forward = forward;
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
            .get("grappling-hook")
            .and_then(|ns| ns.players.get(&owner))
            .and_then(|m| m.get("hook"))
            .and_then(|v| v.as_array())
            .map(|a| a.iter().map(|x| x.as_f64().unwrap()).collect())
            .unwrap_or_default()
    }
    /// One left click: press and let go.
    fn click(&mut self, owner: OwnerId) {
        self.cmd(owner, Command::WeaponTrigger { down: true }).unwrap();
        self.steps(2);
        self.cmd(owner, Command::WeaponTrigger { down: false }).unwrap();
        self.steps(8);
    }
    /// Join with the hook in hand, ready to fire.
    fn hooker(&mut self, name: &str, at: Vec3) -> OwnerId {
        let owner = self.join(name, at);
        self.steps(30);
        self.package(owner, "grapplinghook", vec![]);
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
fn a_click_pulls_them_straight_to_the_wall_and_the_next_lets_go() {
    let mut g = Game::new();
    let a = g.hooker("Batman", Vec3::new(0.0, 0.05, 0.0));
    assert_eq!(g.hook(a)[0], 0.0);
    // Aim up the wall ahead and fire.
    g.look(a, 0.0, 0.5);
    g.click(a);
    g.steps(30);
    let hook = g.hook(a);
    assert_eq!(hook[0], 2.0, "{hook:?}");
    assert!((hook[3] - f64::from(WALL)).abs() < 0.05, "in the wall: {hook:?}");
    let anchor = Vec3::new(hook[1] as f32, hook[2] as f32, hook[3] as f32);
    assert!(anchor.y > 10.0, "{anchor}");
    // The winch pulls them all the way in, and they arrive gently: no
    // faster than a hard landing, never hurt.
    let mut fastest = 0.0_f32;
    for _ in 0..240 {
        g.steps(1);
        fastest = fastest.max(g.velocity(a).length());
    }
    let grip = |g: &Game| g.feet(a) + Vec3::Y * 2.65 * 0.85;
    let t = g.s.tether_of(a).expect("still hooked");
    assert!((t.length - 2.5).abs() < 0.01, "{t:?}");
    assert!(grip(&g).distance(anchor) < 3.2, "at the wall: {}", grip(&g).distance(anchor));
    assert!(g.feet(a).y > 8.0, "hanging: {}", g.feet(a));
    assert!(fastest > 20.0 && fastest < 50.0, "fastest {fastest}");
    // It hangs there: a second later it is still at the wall.
    g.steps(120);
    assert!(grip(&g).distance(anchor) < 3.2);
    assert!(g.velocity(a).length() < 1.0);
    // Switching items keeps them hanging.
    g.cmd(a, Command::EquipTool { slot: None }).unwrap();
    g.steps(12);
    assert!(g.s.tether_of(a).is_some());
    assert_eq!(g.hook(a)[0], 2.0);
    // With the hook back in hand, a click lets go and they fall.
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
fn jump_reels_in_and_crouch_lets_out() {
    let mut g = Game::new();
    let a = g.hooker("Batman", Vec3::new(0.0, 0.05, 0.0));
    g.look(a, 0.0, 0.5);
    g.click(a);
    g.steps(360);
    let at_wall = g.s.tether_of(a).unwrap().length;
    // Crouch held: rope pays out, and stops (braking) once let go.
    g.looks.get_mut(&a).unwrap().crouch = true;
    g.steps(120);
    g.looks.get_mut(&a).unwrap().crouch = false;
    g.steps(60);
    let out = g.s.tether_of(a).unwrap();
    assert!(out.length > at_wall + 10.0, "{at_wall} -> {out:?}");
    g.steps(120);
    let held = g.s.tether_of(a).unwrap();
    assert!((held.length - out.length).abs() < 0.5, "stopped: {out:?} {held:?}");
    // Jump held: back in.
    g.looks.get_mut(&a).unwrap().jump = true;
    g.steps(360);
    g.looks.get_mut(&a).unwrap().jump = false;
    g.steps(4);
    let back = g.s.tether_of(a).unwrap();
    assert!((back.length - 2.5).abs() < 0.5, "{back:?}");
    // The wheel reels too.
    g.package(a, "reel", vec![PackageArg::Int(-3)]);
    g.steps(240);
    assert!(g.s.tether_of(a).unwrap().length > 10.0);
}

#[test]
fn a_hook_in_a_player_carries_them_along_unless_an_admin_says_no() {
    let mut g = Game::new();
    let a = g.hooker("Batman", Vec3::new(0.0, 0.05, 0.0));
    let b = g.join("Robin", Vec3::new(0.0, 0.05, -10.0));
    g.steps(30);
    // Level at Robin's chest.
    g.look(a, 0.0, -0.05);
    g.click(a);
    g.steps(30);
    let hook = g.hook(a);
    assert_eq!(hook[0], 2.0, "{hook:?}");
    assert_eq!(hook[5], 1.0, "bit a player: {hook:?}");
    assert_eq!(hook[6] as u64, b);
    g.steps(240);
    let near = g.feet(a).distance(g.feet(b));
    assert!(near < 4.0, "pulled to Robin: {near}");
    // Robin runs off (away from the wall): Batman comes too.
    g.look(b, 0.0, 0.0);
    g.look(b, std::f32::consts::FRAC_PI_2, 0.0);
    g.walk(b, 1.0);
    g.steps(480);
    g.walk(b, 0.0);
    let moved = g.feet(b);
    assert!(moved.distance(Vec3::new(0.0, 0.05, -10.0)) > 10.0, "{moved}");
    assert!(g.s.tether_of(a).is_some());
    assert!(g.feet(a).distance(moved) < 6.0, "carried: {} vs {moved}", g.feet(a));
    g.click(a);
    assert!(g.s.tether_of(a).is_none());
    // Batman is an admin: players and vehicles are off, and a hook at
    // Robin now misses.
    g.package(a, "hookobjects", vec![]);
    g.steps(4);
    let (from, to) = (g.feet(a), g.feet(b));
    let d = to - from;
    g.look(a, d.x.atan2(-d.z), -0.1);
    g.click(a);
    assert_eq!(g.hook(a)[0], 3.0, "{:?}", g.hook(a));
    g.steps(120);
    assert!(g.s.tether_of(a).is_none());
}

#[test]
fn everyone_gets_the_hook_outside_minigames() {
    let mut g = Game::new();
    let a = g.join("Jane", Vec3::new(0.0, 0.05, 0.0));
    g.steps(30);
    assert!(
        g.s.tool_inventories()[&a]
            .slots
            .iter()
            .any(|s| s.as_deref() == Some(HOOK))
    );
}
