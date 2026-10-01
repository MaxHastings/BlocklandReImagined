//! The Grapple Rope and Hookshot ports (`ports/tool_grapplerope`,
//! `ports/weapon_loz_hookshot`), imported from our own stand-ins in
//! `tests/fixtures/ports` (CC0: the community Add-Ons' folder names and
//! function shapes with their own numbers) and played through the
//! authoritative session with their host rules. Each asserts what the v20
//! script does, with the stand-in's numbers.
use bri_addon_import::{Options, import};
use bri_package::{library::Library, packages::PackageSet};
use bri_package_runtime::Catalog;
use bri_sim::{
    player::MoveInput,
    session::{ActionAim, Command, PackageCommand, Session},
};
use bri_world::OwnerId;
use glam::Vec3;
use rapier3d::prelude::*;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

fn fresh(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bri-grapples-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Imports the stand-in `addon` with the built-in ports into a content
/// root, and loads the import and its rules as a host does.
fn imported(root: &Path, addon: &str, namespace: &str) -> (bri_weapons::Pack, Arc<Catalog>) {
    let out = root.join("addons").join(namespace);
    let report = import(&Options {
        input: Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/ports")
            .join(addon),
        out: out.clone(),
        reference: None,
        core: vec![],
        version: "1.0.0".into(),
    })
    .unwrap();
    let port = &report.ports[0];
    assert!(port.applied, "{:?}", port.reason);
    assert!(
        report.needs_behaviour.iter().all(|b| b.port.is_some()),
        "every function is ported: {:?}",
        report.needs_behaviour
    );
    let library = Library::scan(root).unwrap();
    let rules = format!("{namespace}-rules");
    let set = PackageSet {
        schema_version: 1,
        packages: [namespace, rules.as_str()]
            .iter()
            .map(|id| library.get(id).unwrap().package.clone())
            .collect(),
    };
    let catalog = Catalog::load(root, &set, true).unwrap_or_else(|e| panic!("{e:#?}"));
    let pack =
        bri_weapons::Pack::from_json(&std::fs::read(out.join("assets/weapons.json")).unwrap())
            .unwrap();
    (pack, Arc::new(catalog))
}

struct Game {
    s: Session,
    rules: String,
    seq: u64,
    moves: u64,
    input: MoveInput,
    player: OwnerId,
    /// Other players, walking as their input says.
    others: Vec<(OwnerId, MoveInput, u64)>,
}

impl Game {
    /// A flat floor, with `extra` map pieces, and one player at the origin
    /// holding `item`.
    fn new(
        root: &Path,
        addon: &str,
        namespace: &str,
        item: &str,
        extra: Vec<ColliderBuilder>,
    ) -> Self {
        let (pack, catalog) = imported(root, addon, namespace);
        let mut map = vec![
            ColliderBuilder::cuboid(200.0, 0.5, 200.0).translation(Vector::new(0.0, -0.5, 0.0)),
        ];
        map.extend(extra);
        let mut s = Session::new(
            bri_sim::simulation::Simulation::new(
                bri_world::World::new("Grapples".into(), "grapples".into(), vec![[1.0; 4]]),
                bri_sim::definitions::Definitions {
                    entries: Default::default(),
                },
                map,
            )
            .unwrap(),
        );
        let spawn = Vec3::new(0.0, 0.05, 0.0);
        s.set_spawn_points(vec![spawn]).unwrap();
        s.set_weapon_pack(pack).unwrap();
        s.install_packages(catalog, None).unwrap();
        let player = s.join("Tester".into(), spawn, false).unwrap();
        let mut g = Self {
            s,
            rules: format!("{namespace}-rules"),
            seq: 0,
            moves: 0,
            input: MoveInput::default(),
            player,
            others: vec![],
        };
        g.steps(30);
        g.s.give_item(player, item).unwrap();
        let slot = g.s.tool_inventories()[&player]
            .slots
            .iter()
            .position(|s| s.as_deref() == Some(item))
            .unwrap();
        g.cmd(Command::EquipTool { slot: Some(slot) });
        g.steps(30);
        g
    }
    fn cmd(&mut self, command: Command) {
        self.seq += 1;
        self.s.command(self.player, self.seq, command).unwrap();
    }
    fn rule(&mut self, command: &str) {
        self.seq += 1;
        self.s
            .command_with_aim(
                self.player,
                self.seq,
                Command::Package(PackageCommand {
                    package: self.rules.clone(),
                    command: command.into(),
                    args: vec![],
                }),
                Some(ActionAim {
                    yaw: self.input.yaw,
                    pitch: self.input.pitch,
                }),
            )
            .unwrap();
    }
    fn look(&mut self, yaw: f32, pitch: f32) {
        self.input.yaw = yaw;
        self.input.pitch = pitch;
        self.steps(4);
    }
    fn trigger(&mut self, down: bool) {
        self.cmd(Command::WeaponTrigger { down });
    }
    fn steps(&mut self, n: usize) {
        for _ in 0..n {
            self.moves += 1;
            let _ = self.s.movement(self.player, self.moves, self.input);
            for (owner, input, n) in &mut self.others {
                *n += 1;
                let _ = self.s.movement(*owner, *n, *input);
            }
            self.s.step().unwrap();
        }
    }
    fn me(&self) -> bri_sim::player::PlayerState {
        self.s
            .motion_states()
            .into_iter()
            .find(|(p, _)| p.owner == self.player)
            .map(|(p, _)| p)
            .unwrap()
    }
    fn feet(&self) -> Vec3 {
        self.feet_of(self.player)
    }
    fn feet_of(&self, owner: OwnerId) -> Vec3 {
        self.s
            .motion_states()
            .into_iter()
            .find(|(p, _)| p.owner == owner)
            .map(|(p, _)| Vec3::from(p.feet))
            .unwrap()
    }
    fn velocity(&self) -> Vec3 {
        Vec3::from(self.me().velocity)
    }
}

/// The map's ceiling: a slab whose underside is this high, ahead of the
/// player (toward -Z).
const CEILING: f32 = 20.0;

#[test]
fn the_grapple_rope_hangs_its_holder_where_the_hook_strikes() {
    let dir = fresh("rope");
    let ceiling = ColliderBuilder::cuboid(12.0, 0.5, 12.0).translation(Vector::new(
        0.0,
        CEILING + 0.5,
        -12.0,
    ));
    let mut g = Game::new(
        &dir,
        "Tool_GrappleRope",
        "tool_grapplerope",
        "tool_grapplerope:weapon/grapplerope",
        vec![ceiling],
    );
    // The stand-in checks its line of sight 1.5 above the feet.
    let rules =
        std::fs::read_to_string(dir.join("addons/tool_grapplerope-rules/grapple.rhai")).unwrap();
    assert!(rules.contains("fn lift() { num(1.5) }"), "{rules}");
    // Its rope is drawn by the trail of the projectile it fires along the
    // rope, at the speed it fires it.
    let pack = bri_weapons::Pack::from_json(
        &std::fs::read(dir.join("addons/tool_grapplerope/assets/weapons.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        pack.images["tool_grapplerope:image/grappleropeimage"].rope,
        Some(bri_weapons::Rope {
            projectile: "tool_grapplerope:projectile/chainprojectile".into(),
            speed: 120.0,
        })
    );
    assert!(
        !pack.projectiles["tool_grapplerope:projectile/chainprojectile"]
            .trail
            .is_empty()
    );

    // Aim up at the ceiling ahead and hold the click: the hook flies at
    // the stand-in's 150 units a second and the rope takes hold where it
    // strikes, as long as the distance then.
    g.look(0.0, 1.0);
    g.trigger(true);
    g.steps(2);
    assert!(
        g.s.tether_of(g.player).is_none(),
        "not until the hook lands"
    );
    g.steps(60);
    let rope =
        g.s.tether_of(g.player)
            .expect("roped where the hook struck");
    assert!((rope.anchor[1] - CEILING).abs() < 0.1, "{rope:?}");
    assert!(rope.anchor[2] < -1.0, "ahead: {rope:?}");
    assert_eq!(rope.swing, 0.0, "the movement keys do not pump it");
    let length = rope.length;
    // Still holding, it is a rope, not a winch: walking away, they are
    // stopped a rope's length from the spot and no farther.
    g.look(std::f32::consts::PI, 0.0);
    g.input.forward = 1.0;
    g.steps(240);
    g.input.forward = 0.0;
    let held = g.s.tether_of(g.player).expect("still roped while held");
    assert_eq!(held.length, length, "it neither reels in nor out");
    assert!(
        (g.feet() + Vec3::Y * 2.65 * 0.85).distance(Vec3::from(held.anchor)) < length + 0.6,
        "leashed"
    );

    // Swinging, then letting go: off the rope with all their speed.
    g.input.forward = 1.0;
    g.steps(6);
    let before = g.velocity();
    g.trigger(false);
    g.steps(2);
    assert!(g.s.tether_of(g.player).is_none(), "let go");
    let after = g.velocity();
    assert!(
        (after - before).length() < 3.0,
        "keeps its speed: {before} then {after}"
    );

    // A shot that strikes after the click is let go holds nothing.
    g.input.forward = 0.0;
    g.steps(120);
    g.look(0.0, 1.0);
    g.trigger(true);
    g.trigger(false);
    g.steps(90);
    assert!(g.s.tether_of(g.player).is_none());

    // Putting it away lets go too.
    g.trigger(true);
    g.steps(60);
    assert!(g.s.tether_of(g.player).is_some());
    g.cmd(Command::EquipTool { slot: None });
    g.steps(8);
    assert!(g.s.tether_of(g.player).is_none());
    assert!(
        g.s.package_diagnostics().is_empty(),
        "{:?}",
        g.s.package_diagnostics()
    );
    std::fs::remove_dir_all(dir).unwrap();
}

/// The wall the Hookshot is fired at: its face is this far ahead (-Z).
const WALL: f32 = 40.0;

#[test]
fn the_hookshot_pulls_its_shooter_to_where_it_strikes() {
    let dir = fresh("hookshot");
    let wall =
        ColliderBuilder::cuboid(20.0, 20.0, 0.5).translation(Vector::new(0.0, 20.0, -WALL - 0.5));
    let mut g = Game::new(
        &dir,
        "Weapon_Loz_Hookshot",
        "weapon_loz_hookshot",
        "weapon_loz_hookshot:weapon/hookshotitem",
        vec![wall],
    );
    let rules = std::fs::read_to_string(dir.join("addons/weapon_loz_hookshot-rules/hookshot.rhai"))
        .unwrap();
    for (name, value) in [
        ("stop", "4"),
        ("near", "12"),
        ("slow", "25"),
        ("far", "13"),
        ("fast", "40"),
        ("every", "125"),
    ] {
        assert!(
            rules.contains(&format!("fn {name}() {{ num({value}) }}"))
                || rules.contains(&format!("num({value}) * 0.12")),
            "{name} is the stand-in's {value}"
        );
    }

    // Level at the wall: the shot strikes it and the pull sets their speed
    // straight at the spot, 40 while farther than 13.
    g.look(0.0, 0.0);
    g.trigger(true);
    g.trigger(false);
    let mut fast = 0.0f32;
    let mut slow_seen = false;
    for _ in 0..600 {
        g.steps(1);
        let to = WALL + g.feet().z;
        let v = g.velocity();
        if to > 16.0 {
            fast = fast.max(-v.z);
        }
        if to < 11.0 && to > 6.0 && (-v.z - 25.0).abs() < 3.0 {
            slow_seen = true;
        }
    }
    assert!((fast - 40.0).abs() < 2.0, "pulled at 40: {fast}");
    assert!(slow_seen, "slowed to 25 within 12");
    // The pull ends within 4 of the spot (measured from the feet to where
    // it struck, at eye height on the wall).
    let end = g.feet();
    assert!(WALL + end.z < 4.5, "at the wall: {end}");
    // It has ended: they stay where they stopped.
    g.steps(240);
    assert!(g.feet().distance(end) < 1.0, "the pull is over");

    // Walk back, shoot again and `/degrapple`: the pull stops and they
    // stop with it.
    g.look(std::f32::consts::PI, 0.0);
    g.input.forward = 1.0;
    g.steps(400);
    g.input.forward = 0.0;
    g.steps(60);
    assert!(WALL + g.feet().z > 20.0, "walked back: {}", g.feet());
    g.look(0.0, 0.0);
    g.trigger(true);
    g.trigger(false);
    g.steps(40);
    assert!(-g.velocity().z > 20.0, "pulled again");
    g.rule("degrapple");
    // No pulse sets their speed again: it only falls, by friction or the
    // wall, from here on.
    let mut last = g.velocity().length();
    for _ in 0..240 {
        g.steps(1);
        let now = g.velocity().length();
        assert!(
            now <= last + 0.5,
            "pushed again after /degrapple: {last} then {now}"
        );
        last = now;
    }
    assert!(
        g.s.package_diagnostics().is_empty(),
        "{:?}",
        g.s.package_diagnostics()
    );
    std::fs::remove_dir_all(dir).unwrap();
}

/// A shot that strikes a player pulls the shooter after them as they move.
#[test]
fn the_hookshot_follows_a_player_it_strikes() {
    let dir = fresh("follow");
    let mut g = Game::new(
        &dir,
        "Weapon_Loz_Hookshot",
        "weapon_loz_hookshot",
        "weapon_loz_hookshot:weapon/hookshotitem",
        vec![],
    );
    let target =
        g.s.join("Target".into(), Vec3::new(0.0, 0.05, -30.0), false)
            .unwrap();
    g.others.push((target, MoveInput::default(), 0));
    g.steps(30);
    g.look(0.0, 0.0);
    g.trigger(true);
    g.trigger(false);
    g.steps(30);
    assert!(
        -g.velocity().z > 20.0,
        "pulled toward them: {}",
        g.velocity()
    );
    // They walk off sideways: the pull turns after them, until the shooter
    // is within 4 of them, where it ends.
    g.others[0].1.yaw = -std::f32::consts::FRAC_PI_2;
    g.others[0].1.forward = 1.0;
    let mut closest = f32::MAX;
    for _ in 0..240 {
        g.steps(1);
        closest = closest.min(g.feet().distance(g.feet_of(target)));
    }
    let them = g.feet_of(target);
    assert!(them.x.abs() > 8.0, "they walked off: {them}");
    let me = g.feet();
    assert!(
        me.x * them.x > 0.0 && me.x.abs() > 3.0,
        "turned after them: {me} and {them}"
    );
    assert!(closest < 4.5, "caught up within 4: {closest}");
    assert!(
        g.s.package_diagnostics().is_empty(),
        "{:?}",
        g.s.package_diagnostics()
    );
    std::fs::remove_dir_all(dir).unwrap();
}
