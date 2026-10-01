//! The Tier+Tactical Tier 1 port (`ports/weapon_package_tier1` on the shared
//! `ports/_shared/tier-tactical` fragment) on our stand-in
//! (`tests/fixtures/ports/Weapon_Package_Tier1`, CC0): the same folder name
//! and the shape of Kai's ammo system and gun scripts, with our own guns,
//! names and numbers.
use bri_addon_import::{Options, import};
use bri_minigames::Settings;
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::Definitions,
    player::MoveInput,
    session::{ActionAim, Command, MiniGameRequest, Notice, PackageArg, PackageCommand, Session},
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

const NS: &str = "weapon_package_tier1";

struct Dir(PathBuf);
impl std::ops::Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The stand-in imported with the built-in ports into `<dir>/addons`.
fn imported(name: &str) -> (Dir, PathBuf, bri_addon_import::report::Report) {
    let dir =
        Dir(std::env::temp_dir().join(format!("bri-tier-port-{}-{name}", std::process::id())));
    let _ = std::fs::remove_dir_all(&dir.0);
    let out = dir.0.join("addons").join(NS);
    let report = import(&Options {
        input: Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/ports/Weapon_Package_Tier1"),
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

const A: ActorId = ActorId(1);

/// A world with one shooter holding `item`, ready to fire.
fn holding(pack: &Pack, item: &str) -> WeaponsWorld {
    let mut w = WeaponsWorld::new(pack.clone()).unwrap();
    w.add_actor(A, 5).unwrap();
    let slot = w.give(A, &format!("{NS}:weapon/{item}")).unwrap();
    w.equip(A, Some(slot)).unwrap();
    steps(&mut w, 60);
    w
}

fn steps(w: &mut WeaponsWorld, n: usize) -> Vec<Event> {
    (0..n).flat_map(|_| w.step(&mut Empty)).collect()
}

/// One pull of the trigger and what came of it.
fn shoot(w: &mut WeaponsWorld) -> Vec<Event> {
    w.trigger(A, true).unwrap();
    let mut events = steps(w, 2);
    w.trigger(A, false).unwrap();
    events.extend(steps(w, 40));
    events
}

fn moving(w: &mut WeaponsWorld, speed: f32) {
    w.set_frame(
        A,
        Frame {
            velocity: Vec3::new(speed, 0.0, 0.0),
            ..Frame::default()
        },
    )
    .unwrap();
}

/// Where each hand's rays ended.
fn tracers(events: &[Event]) -> Vec<(u8, f32)> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Tracer { hand, to, .. } => Some((*hand, to.length().round())),
            _ => None,
        })
        .collect()
}

fn spawned(events: &[Event]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Spawned { definition, .. } => {
                Some(definition.rsplit('/').next().unwrap().to_owned())
            }
            _ => None,
        })
        .collect()
}

/// The stand-in's guns read as Tier+Tactical's ammo system and gun scripts
/// read them: each image's magazine from its item's TT_maxAmmo and
/// TT_ammoType with the T+T2 reserves, raycasts reaching less on the move,
/// the pump's pellets and blast loaded a shell at a time, the rifle's weak
/// round on the move, the SMG's slowing bullet and the pair's left hand.
#[test]
fn tier1_guns_get_magazines_hitscans_and_volleys() {
    let (_dir, out, report) = imported("guns");
    let port = &report.ports[0];
    assert!(port.applied, "{:?}", port.reason);
    assert_eq!(
        (port.port.as_str(), port.status.as_str(), port.copy.as_str()),
        ("weapon_package_tier1", "partial", "unlisted")
    );
    bri_addon_import::ports::check_pins(&out).unwrap();
    // The rules hand out the types the copy registers, and only those.
    let rules =
        std::fs::read_to_string(out.with_file_name(format!("{NS}-rules")).join("tier.rhai"))
            .unwrap();
    let registered = rules
        .lines()
        .find(|l| l.starts_with("fn registered()"))
        .unwrap();
    for t in ["\"9MM\"", "\"556\"", "\"shotgun\""] {
        assert!(registered.contains(t), "{registered}");
    }
    assert!(!registered.contains("\"270\""), "{registered}");
    let pack = pack(&out);
    let image = |name: &str| pack.images[&format!("{NS}:image/{name}")].clone();
    let projectile = |name: &str| pack.projectiles[&format!("{NS}:projectile/{name}")].clone();

    // The sidearm: 6 rounds of 9mm (140 of at most 280), a 150 reach
    // standing still and 60 on the move.
    let sidearm = image("standinsidearmimage");
    let mag = sidearm.magazine.clone().unwrap();
    assert_eq!(
        (
            mag.size,
            mag.ammo.as_str(),
            mag.reserve,
            mag.max_reserve,
            mag.one_by_one
        ),
        (6, "tt-9mm", 140, 280, false)
    );
    assert_eq!(
        mag.on_loaded,
        Some(Check {
            loaded: Some(Cond::Is(true)),
            ammo: None
        })
    );
    let shot = sidearm.shot.clone().unwrap();
    assert_eq!(
        (shot.spread, shot.moving_spread, shot.moving_speed),
        (0.0004, Some(0.002), 0.1)
    );
    assert!(shot.kick.is_some(), "recoil shakes the view");
    let mut w = holding(&pack, "standinsidearmitem");
    assert_eq!(tracers(&shoot(&mut w)), [(0, 150.0)]);
    moving(&mut w, 5.0);
    assert_eq!(tracers(&shoot(&mut w)), [(0, 60.0)]);
    moving(&mut w, 0.0);
    for _ in 0..4 {
        shoot(&mut w);
    }
    // Empty, it reloads from the reserve by its own states.
    steps(&mut w, 300);
    let ammo = w.ammo(A).unwrap();
    assert_eq!((ammo.rounds, ammo.reserve), (6, Reserve::Rounds(134)));

    // The pump: 5 pellets and a blast; its shells load one at a time.
    let pump = image("standinpumpimage");
    let mag = pump.magazine.clone().unwrap();
    assert_eq!(
        (mag.size, mag.ammo.as_str(), mag.one_by_one, mag.on_loaded),
        (3, "tt-shotgun", true, None)
    );
    assert_eq!(mag.empty_sound, "standinJamSound");
    let mut w = holding(&pack, "standinpumpitem");
    let mut fired = spawned(&shoot(&mut w));
    fired.sort();
    assert_eq!(
        fired,
        [
            "standinblastprojectile",
            "standinpelletprojectile",
            "standinpelletprojectile",
            "standinpelletprojectile",
            "standinpelletprojectile",
            "standinpelletprojectile"
        ]
    );
    w.reload(A).unwrap();
    steps(&mut w, 400);
    let ammo = w.ammo(A).unwrap();
    assert_eq!((ammo.rounds, ammo.reserve), (3, Reserve::Rounds(23)));

    // The rifle fires its weaker round on the move.
    let shot = image("standinrifleimage").shot.unwrap();
    assert_eq!(
        (shot.spread, shot.moving_spread, shot.moving_speed),
        (0.0001, Some(0.001), 3.0)
    );
    let mut w = holding(&pack, "standinrifleitem");
    assert_eq!(spawned(&shoot(&mut w)), ["standinrifleprojectile"]);
    moving(&mut w, 5.0);
    assert_eq!(spawned(&shoot(&mut w)), ["standinrifleweakprojectile"]);

    // The SMG's bullet slows whoever it hits.
    assert_eq!(
        projectile("standinsmgprojectile").slow,
        Some(Slow { divisor: 2.0 })
    );
    assert_eq!(projectile("standinrifleprojectile").slow, None);

    // The bag a dead player's ammo spills into has no name: only scripts
    // drop it, and no spawn list shows it.
    assert!(pack.items[&format!("{NS}:weapon/ammodroppeditem")].hidden);
    assert!(!pack.items[&format!("{NS}:weapon/standinnineitem")].hidden);

    // The pair fires both hands from one magazine of 4.
    let pair = image("standinpairimage");
    assert_eq!(
        pair.left_image.as_deref(),
        Some(&*format!("{NS}:image/standinleftimage"))
    );
    let left = image("standinleftimage").shot.unwrap();
    assert_eq!(left.hitscan.unwrap().moving_range, Some(50.0));
    let mut w = holding(&pack, "standinpairitem");
    let mut hands = tracers(&shoot(&mut w));
    hands.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(hands, [(0, 150.0), (1, 120.0)]);
    assert_eq!(w.ammo(A).unwrap().rounds, 2);
}

/// A server Add-On that drops an item at a player's feet, moves them and
/// notes their magazine, to drive the port's rules as a game would.
const PROBE: &str = r#"
fn cmd_drop(p, item) {
    let me = player(p);
    drop_item(item, me.x, me.y + 0.5, me.z);
}
fn cmd_goto(p, x, y, z) {
    teleport(p, x, y, z);
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
            { "name": "goto", "args": ["float", "float", "float"] },
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
    fn drop(&mut self, owner: OwnerId, item: &str) {
        let item = format!("{NS}:weapon/{item}");
        self.probe(owner, "drop", vec![PackageArg::String(item)]);
        self.steps(30);
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
        let item = format!("{NS}:weapon/{item}");
        let slot = self.s.tool_inventories()[&owner]
            .slots
            .iter()
            .position(|s| s.as_deref() == Some(&*item))
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
    fn tools(&self, owner: OwnerId) -> Vec<String> {
        self.s.tool_inventories()[&owner]
            .slots
            .iter()
            .flatten()
            .cloned()
            .collect()
    }
}

/// The host rules in play: an ammo item tops up each type it names to the
/// type's most and is used up only when it added any; the sport rifle's
/// round does 2.5 times its damage to the head under its own kill message;
/// a dead player's ammo spills into a bag that whoever picks up takes from.
#[test]
fn ammo_items_bags_and_headshots_play_in_a_hosted_game() {
    let (dir, out, report) = imported("hosted");
    assert!(report.ports[0].applied, "{:?}", report.ports[0].reason);
    let mut g = Game::new(&dir.0, &out);
    let a = g.join("A", Vec3::new(0.0, 0.05, 0.0));
    let b = g.join("B", Vec3::new(0.0, 0.05, -6.0));
    g.steps(2);
    g.s.set_spawn_points(vec![g.feet(a)]).unwrap();
    let mut loadout: [Option<String>; 5] = Default::default();
    for (i, item) in ["standinsidearmitem", "standinpumpitem", "standinrifleitem"]
        .iter()
        .enumerate()
    {
        loadout[i] = Some(format!("{NS}:weapon/{item}"));
    }
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

    g.equip(a, "standinsidearmitem");
    assert_eq!(g.mag(a), json!("6|6|tt-9mm|140"));
    // 30 more 9mm.
    g.drop(a, "standinnineitem");
    assert_eq!(g.mag(a), json!("6|6|tt-9mm|170"));
    // A full load of 9mm, and 2 shells for the pump A never drew.
    g.drop(a, "standinpileitem");
    assert_eq!(g.mag(a), json!("6|6|tt-9mm|280"));
    // Full of 9mm, the next box stays where it fell.
    g.drop(a, "standinnineitem");
    assert_eq!(g.mag(a), json!("6|6|tt-9mm|280"));
    let lying = |g: &Game, item: &str| {
        g.s.weapon_view()
            .drops
            .iter()
            .filter(|d| d.item == format!("{NS}:weapon/{item}"))
            .count()
    };
    assert_eq!(lying(&g, "standinnineitem"), 1);
    g.equip(a, "standinpumpitem");
    assert_eq!(g.mag(a), json!("3|3|tt-shotgun|26"));
    assert!(
        !g.tools(a)
            .iter()
            .any(|t| t.contains("nine") || t.contains("pile"))
    );

    // The rifle's 20 to the body, 50 to the head, and the head's own
    // kill message.
    g.equip(a, "standinrifleitem");
    g.shoot_at(a, b, 1.2);
    assert!((g.health(b) - 80.0).abs() < 0.5, "{}", g.health(b));
    g.shoot_at(a, b, 2.45);
    assert!((g.health(b) - 30.0).abs() < 0.5, "{}", g.health(b));
    g.shoot_at(a, b, 2.45);
    let said: Vec<String> =
        g.s.take_private_notices()
            .into_iter()
            .filter_map(|(_, n)| match n {
                Notice::Chat(text) => Some(text),
                _ => None,
            })
            .collect();
    assert!(said.iter().any(|t| t.contains("A headshot B")), "{said:?}");

    // B's ammo lies in a bag; A takes the shells that fit.
    g.steps(240);
    let bag =
        g.s.weapon_view()
            .drops
            .iter()
            .find(|d| d.item == format!("{NS}:weapon/ammodroppeditem"))
            .map(|d| d.position)
            .expect("a bag");
    g.probe(
        a,
        "goto",
        vec![
            PackageArg::Float(bag.x as f64),
            PackageArg::Float(bag.y as f64),
            PackageArg::Float(bag.z as f64),
        ],
    );
    g.steps(30);
    assert_eq!(lying(&g, "ammodroppeditem"), 0, "picked up");
    g.equip(a, "standinpumpitem");
    assert_eq!(g.mag(a), json!("3|3|tt-shotgun|48"));
}

/// The stand-in Tier 1A imported beside the stand-in Tier 1 it requires,
/// as the drop folder imports one pack with the others as its reference.
fn imported_1a(name: &str) -> (Dir, PathBuf, bri_addon_import::report::Report) {
    let dir =
        Dir(std::env::temp_dir().join(format!("bri-tier1a-port-{}-{name}", std::process::id())));
    let _ = std::fs::remove_dir_all(&dir.0);
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ports");
    let tier1 = dir.0.join("reference/Add-Ons/Weapon_Package_Tier1");
    std::fs::create_dir_all(&tier1).unwrap();
    for f in std::fs::read_dir(fixtures.join("Weapon_Package_Tier1")).unwrap() {
        let f = f.unwrap();
        std::fs::copy(f.path(), tier1.join(f.file_name())).unwrap();
    }
    let out = dir.0.join("addons").join("weapon_package_tier1a");
    let report = import(&Options {
        input: fixtures.join("Weapon_Package_Tier1A"),
        out: out.clone(),
        reference: Some(dir.0.join("reference")),
        core: vec![],
        installed: None,
        version: "1.0.0".into(),
    })
    .unwrap();
    (dir, out, report)
}

/// Tier 1A on Tier 1: the single shotgun shoves its shooter back as it
/// fires its pellets and blast, its recoil shake read from Tier 1's own
/// recoil projectile; the pepperbox casts several rays a shot; the
/// snubnose's headshots get their own kill message; the nailgun, an
/// easter egg the original loads only with a hidden setting, is hidden.
#[test]
fn tier1a_shotgun_knocks_back_and_the_nailgun_stays_hidden() {
    const NS1A: &str = "weapon_package_tier1a";
    let (_dir, out, report) = imported_1a("guns");
    let port = &report.ports[0];
    assert!(port.applied, "{:?}", port.reason);
    assert_eq!(port.port, NS1A);
    let pack = pack(&out);
    let image = |name: &str| pack.images[&format!("{NS1A}:image/{name}")].clone();

    let single = image("singleshotgunimage");
    let shot = single.shot.clone().unwrap();
    assert_eq!((shot.projectiles, shot.spread), (6, 0.002));
    assert_eq!((shot.recoil, shot.recoil_vertical), (3.0, Some(3.0)));
    let kick = shot.kick.unwrap();
    assert_eq!(
        (kick.amplitude, kick.frequency, kick.seconds),
        (0.4, 4.0, 0.4),
        "Tier 1's recoil projectile's shake"
    );
    assert_eq!(single.volleys.len(), 1);
    assert!(
        single.volleys[0]
            .projectile
            .ends_with("singleshotgunblastprojectile")
    );
    let mag = single.magazine.unwrap();
    assert_eq!((mag.size, mag.ammo.as_str()), (2, "tt-shotgun"));
    // Fired facing -Z, the shooter is pushed back along +Z.
    let mut w = WeaponsWorld::new(pack.clone()).unwrap();
    w.add_actor(A, 5).unwrap();
    let slot = w
        .give(A, &format!("{NS1A}:weapon/singleshotgunitem"))
        .unwrap();
    w.equip(A, Some(slot)).unwrap();
    steps(&mut w, 60);
    let pushed: Vec<Vec3> = shoot(&mut w)
        .iter()
        .filter_map(|e| match e {
            Event::Recoil { velocity, .. } => Some(*velocity),
            _ => None,
        })
        .collect();
    assert_eq!(pushed, [Vec3::new(0.0, 0.0, 3.0)]);

    let pepper = image("pepperpistolimage").shot.unwrap();
    assert_eq!((pepper.projectiles, pepper.spread), (3, 0.004));
    assert!(pepper.hitscan.is_some());

    let rules = std::fs::read_to_string(
        out.with_file_name(format!("{NS1A}-rules"))
            .join("tier.rhai"),
    )
    .unwrap();
    let headshots = rules
        .lines()
        .find(|l| l.starts_with("fn headshots()"))
        .unwrap();
    assert!(
        headshots.contains(r#""weapon_package_tier1a:projectile/snubnoseprojectile": #{"multiplier": 2, "type": "SnubnoseHeadshot"}"#),
        "{headshots}"
    );

    assert!(pack.items[&format!("{NS1A}:weapon/nailgunitem")].hidden);
    assert!(!pack.items[&format!("{NS1A}:weapon/snubnoseitem")].hidden);
    assert_eq!(
        pack.projectiles[&format!("{NS1A}:projectile/nailgunprojectile1")].slow,
        Some(Slow { divisor: 1.5 })
    );
}
