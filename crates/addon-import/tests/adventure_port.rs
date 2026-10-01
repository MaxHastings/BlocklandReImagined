//! The Adventurer's Weapons port (`ports/weapon_modernwarbattles`) on our
//! stand-in (`tests/fixtures/ports/Weapon_ModernWarbattles`, CC0): the same
//! folder name and the hl2 ammo system's shape with our own guns and
//! numbers. Its guns get magazines from their item fields and reload in
//! their own states' time; its host rules give ammo boxes (a typed box
//! twice its amount, as the original) and headshots.
use bri_addon_import::{Options, import};
use bri_minigames::Settings;
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::Definitions,
    player::MoveInput,
    session::{ActionAim, Command, MiniGameRequest, PackageArg, PackageCommand, Session},
    simulation::Simulation,
};
use bri_weapons::*;
use bri_world::{OwnerId, World};
use glam::Vec3;
use rapier3d::prelude::*;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};

const NS: &str = "weapon_modernwarbattles";
const PISTOL: &str = "weapon_modernwarbattles:weapon/standinpistolitem";
const SHOTGUN: &str = "weapon_modernwarbattles:weapon/huntingshotgunitem";

struct Dir(PathBuf);
impl std::ops::Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The stand-in imported with the built-in ports into `<dir>/addons`.
fn imported(name: &str) -> (Dir, PathBuf, bri_addon_import::report::Report) {
    let dir =
        Dir(std::env::temp_dir().join(format!("bri-adventure-port-{}-{name}", std::process::id())));
    let _ = std::fs::remove_dir_all(&dir.0);
    let out = dir.0.join("addons").join(NS);
    let report = import(&Options {
        input: Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/ports/Weapon_ModernWarbattles"),
        out: out.clone(),
        reference: None,
        core: vec![],
        installed: None,
        version: "1.0.0".into(),
    })
    .unwrap();
    (dir, out, report)
}

fn pack(out: &Path) -> Pack {
    Pack::from_json(&std::fs::read(out.join("assets/weapons.json")).unwrap()).unwrap()
}

struct Empty;
impl Query for Empty {
    fn sweep(&mut self, _: Vec3, _: Vec3, _: Filter) -> Option<Hit> {
        None
    }
    fn radius(&mut self, _: Vec3, _: f32, _: usize) -> Vec<Nearby> {
        vec![]
    }
    fn can_affect(&self, _: ActorId, _: TargetId) -> bool {
        true
    }
    fn can_catch(&self, _: ActorId, _: ActorId) -> bool {
        false
    }
}

/// The stand-in's numbers, read as the original's hl2 ammo system reads
/// them: `maxmag` rounds, the reserve of the item's `ammotype` (32 of at
/// most 64 pistol rounds, from the port's table), and the reload its image
/// plays: ReloadStart 0.5 s, Reload 1.0 s and the 0.01 s load check before
/// the ammo is checked again.
#[test]
fn ammo_system_guns_get_magazines_that_reload_like_their_states() {
    let (_dir, out, report) = imported("magazines");
    let port = &report.ports[0];
    assert!(port.applied, "{:?}", port.reason);
    assert_eq!(
        (port.port.as_str(), port.status.as_str(), port.copy.as_str()),
        ("weapon_modernwarbattles", "partial", "unlisted")
    );
    bri_addon_import::ports::check_pins(&out).unwrap();
    let pack = pack(&out);
    let pistol = pack.images[&format!("{NS}:image/standinpistolimage")]
        .magazine
        .clone()
        .unwrap();
    assert_eq!(
        (
            pistol.size,
            pistol.ammo.as_str(),
            pistol.reserve,
            pistol.max_reserve
        ),
        (12, "pistol", 32, 64)
    );
    assert_eq!(
        (pistol.reload_ticks, pistol.one_by_one),
        (60 + 120 + 2, false)
    );
    assert_eq!(pistol.display, "Pistol");
    // The hunting shotgun loads a shell at a time; its Reload is 0.4 s.
    let shotgun = pack.images[&format!("{NS}:image/standinshotgunimage")]
        .magazine
        .clone()
        .unwrap();
    assert_eq!(
        (
            shotgun.size,
            shotgun.ammo.as_str(),
            shotgun.reserve,
            shotgun.one_by_one
        ),
        (5, "shotgun", 12, true)
    );
    assert_eq!(shotgun.reload_ticks, 60 + 48 + 2);
    // Twelve shots empty the pistol; the image goes through its reload
    // states once, and the rounds arrive as it checks its ammo again.
    let mut world = WeaponsWorld::new(pack).unwrap();
    world.add_actor(ActorId(1), 5).unwrap();
    let slot = world.give(ActorId(1), PISTOL).unwrap();
    world.equip(ActorId(1), Some(slot)).unwrap();
    let (mut reloads, mut last, mut shots) = (0, String::new(), 0);
    for tick in 0..900 {
        if tick >= 30 && tick % 30 == 0 && shots < 12 {
            world.trigger(ActorId(1), true).unwrap();
            shots += 1;
        } else if tick >= 30 && tick % 30 == 1 {
            world.trigger(ActorId(1), false).unwrap();
        }
        world.step(&mut Empty);
        let state = world.image_state(ActorId(1), 0).unwrap().1.name.clone();
        if state != last && state == "ReloadStart" {
            reloads += 1;
        }
        last = state;
    }
    assert_eq!(reloads, 1, "one reload for one empty magazine");
    assert_eq!(last, "Ready");
    let ammo = world.ammo(ActorId(1)).unwrap();
    assert_eq!((ammo.rounds, ammo.reserve), (12, Reserve::Rounds(20)));
}

/// A server Add-On that drops an item at a player's feet and notes their
/// magazine, to drive the port's rules as a game would.
const PROBE: &str = r#"
fn cmd_drop(p, item) {
    let me = player(p);
    drop_item(item, me.x, me.y + 0.5, me.z);
}
fn cmd_mag(p) {
    let m = player(p).magazine;
    set("mag", if m == () { "none" } else {
        `${m.rounds}|${m.size}|${m.ammo}|${m.reserve}`
    });
}
"#;

fn catalog(root: &Path) -> Arc<Catalog> {
    let dir = root.join("addons/probe");
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = json!({
        "schema_version": 1, "id": "probe", "version": "1.0.0", "api": 1,
        "name": "probe", "license": "CC0-1.0",
        "capabilities": ["player", "world.edit"],
        "provides": [
            { "kind": "behaviour", "id": "probe:behaviour/main", "file": "behaviour.json" },
            { "kind": "script", "id": "probe:script/main", "file": "main.rhai" }
        ]
    });
    let behaviour = json!({
        "schema_version": 1,
        "script": "main.rhai",
        "commands": [
            { "name": "drop", "args": ["string"] },
            { "name": "mag" }
        ],
        "state": { "global": { "mag": { "default": "", "visible": "everyone" } } }
    });
    std::fs::write(dir.join("package.json"), manifest.to_string()).unwrap();
    std::fs::write(dir.join("behaviour.json"), behaviour.to_string()).unwrap();
    std::fs::write(dir.join("main.rhai"), PROBE).unwrap();
    let entry = |id: &str, side| PackageEntry {
        id: id.into(),
        version: "1.0.0".into(),
        side,
        dir: format!("addons/{id}"),
        role: None,
    };
    let set = PackageSet {
        schema_version: 1,
        packages: vec![
            entry(NS, Side::Shared),
            entry(&format!("{NS}-rules"), Side::Server),
            entry("probe", Side::Server),
        ],
    };
    Arc::new(Catalog::load(root, &set, true).unwrap_or_else(|e| panic!("{e:#?}")))
}

struct Game {
    s: Session,
    seq: BTreeMap<OwnerId, u64>,
    moves: BTreeMap<OwnerId, u64>,
    looks: BTreeMap<OwnerId, MoveInput>,
}
impl Game {
    fn new(root: &Path, out: &Path) -> Self {
        let ground = ColliderBuilder::cuboid(100.0, 0.5, 100.0)
            .translation(Vector::new(0.0, -0.5, 0.0))
            .user_data(u128::MAX);
        let mut s = Session::new(
            Simulation::new(
                World::new("Range".into(), "range".into(), vec![[1.0; 4]]),
                Definitions::default(),
                vec![ground],
            )
            .unwrap(),
        );
        s.set_weapon_pack(pack(out)).unwrap();
        // Item boxes from the import's item physics, as a host loads them.
        let physics: Value =
            serde_json::from_slice(&std::fs::read(out.join("assets/item-physics.json")).unwrap())
                .unwrap();
        s.set_item_bounds(serde_json::from_value(physics["items"].clone()).unwrap())
            .unwrap();
        s.install_packages(catalog(root), None).unwrap();
        Self {
            s,
            seq: BTreeMap::new(),
            moves: BTreeMap::new(),
            looks: BTreeMap::new(),
        }
    }
    fn join(&mut self, name: &str, at: Vec3) -> OwnerId {
        let owner = self.s.join(name.into(), at, false).unwrap();
        self.looks.insert(owner, MoveInput::default());
        owner
    }
    fn cmd(&mut self, owner: OwnerId, command: Command) {
        let n = self.seq.entry(owner).or_default();
        *n += 1;
        let look = self.looks[&owner];
        let aim = Some(ActionAim {
            yaw: look.yaw,
            pitch: look.pitch,
        });
        self.s.command_with_aim(owner, *n, command, aim).unwrap();
    }
    fn probe(&mut self, owner: OwnerId, command: &str, args: Vec<PackageArg>) {
        self.cmd(
            owner,
            Command::Package(PackageCommand {
                package: "probe".into(),
                command: command.into(),
                args,
            }),
        );
    }
    fn mag(&mut self, owner: OwnerId) -> Value {
        self.probe(owner, "mag", vec![]);
        self.s
            .package_state()
            .packages
            .get("probe")
            .and_then(|ns| ns.global.get("mag").cloned())
            .unwrap_or(Value::Null)
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
    fn equip(&mut self, owner: OwnerId, item: &str) {
        let slot = self.s.tool_inventories()[&owner]
            .slots
            .iter()
            .position(|s| s.as_deref() == Some(item))
            .unwrap();
        self.cmd(owner, Command::EquipTool { slot: Some(slot) });
        self.steps(20);
    }
    /// A shoots at `height` above B's feet, from 6 units away.
    fn shoot_at(&mut self, a: OwnerId, b: OwnerId, height: f32) {
        let eye = self.feet(a).y + 2.156;
        let pitch = ((self.feet(b).y + height - eye) / (self.feet(a).z - self.feet(b).z)).atan();
        self.looks.get_mut(&a).unwrap().pitch = pitch;
        self.steps(4);
        self.cmd(a, Command::WeaponTrigger { down: true });
        self.steps(2);
        self.cmd(a, Command::WeaponTrigger { down: false });
        self.steps(60);
    }
    fn health(&self, owner: OwnerId) -> f32 {
        self.s.vitals()[&owner].health
    }
}

#[test]
fn ammo_boxes_and_headshots_play_in_a_hosted_game() {
    let (dir, out, report) = imported("hosted");
    assert!(report.ports[0].applied, "{:?}", report.ports[0].reason);
    let mut g = Game::new(&dir.0, &out);
    let a = g.join("A", Vec3::new(0.0, 0.05, 0.0));
    let b = g.join("B", Vec3::new(0.0, 0.05, -6.0));
    g.steps(2);
    g.s.set_spawn_points(vec![g.feet(a)]).unwrap();
    let mut loadout: [Option<String>; 5] = Default::default();
    loadout[0] = Some(PISTOL.into());
    loadout[1] = Some(SHOTGUN.into());
    g.cmd(
        a,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: Settings {
                loadout,
                ..Settings::default()
            },
        }),
    );
    let game = g.s.minigame_views()[0].id;
    g.s.set_spawn_points(vec![g.feet(b)]).unwrap();
    g.cmd(b, Command::MiniGame(MiniGameRequest::Join { game }));
    g.steps(330);

    g.equip(a, PISTOL);
    assert_eq!(g.mag(a), json!("12|12|pistol|32"));
    // A pistol box: twice its 32 rounds, capped at 64.
    g.probe(
        a,
        "drop",
        vec![PackageArg::String(format!(
            "{NS}:weapon/standinammopistolitem"
        ))],
    );
    g.steps(30);
    assert_eq!(g.mag(a), json!("12|12|pistol|64"));
    // The box of every type tops up the shotgun A carries but never drew.
    g.probe(
        a,
        "drop",
        vec![PackageArg::String(format!("{NS}:weapon/standinammoitem"))],
    );
    g.steps(30);
    g.equip(a, SHOTGUN);
    assert_eq!(g.mag(a), json!("5|5|shotgun|12"));
    // Neither box became a tool.
    let tools = &g.s.tool_inventories()[&a].slots;
    assert!(
        !tools.iter().flatten().any(|t| t.contains("ammo")),
        "{tools:?}"
    );

    // The pistol's 10 damage; ×1.5 on the head, and on a crouched target
    // (after v20's own ×2.1 for a direct hit on a crouched player).
    g.equip(a, PISTOL);
    g.shoot_at(a, b, 1.7);
    assert!((g.health(b) - 90.0).abs() < 0.5, "{}", g.health(b));
    g.shoot_at(a, b, 2.45);
    assert!((g.health(b) - 75.0).abs() < 0.5, "{}", g.health(b));
    g.looks.get_mut(&b).unwrap().crouch = true;
    g.steps(30);
    g.shoot_at(a, b, 0.9);
    assert!((g.health(b) - (75.0 - 31.5)).abs() < 0.5, "{}", g.health(b));
    assert_eq!(g.mag(a), json!("9|12|pistol|64"));
}
