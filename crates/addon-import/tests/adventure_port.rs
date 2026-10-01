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
    imported_as("Weapon_ModernWarbattles", NS, name)
}

/// The stand-in `addon` imported as `ns` with the built-in ports.
fn imported_as(
    addon: &str,
    ns: &str,
    name: &str,
) -> (Dir, PathBuf, bri_addon_import::report::Report) {
    let dir =
        Dir(std::env::temp_dir().join(format!("bri-adventure-port-{}-{name}", std::process::id())));
    let _ = std::fs::remove_dir_all(&dir.0);
    let out = dir.0.join("addons").join(ns);
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
        ("weapon_modernwarbattles", "verified", "unlisted")
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
    // The hunting shotgun loads a shell at a time (its image runs
    // onReloadSingle): each round is its Reload, 0.4 s, and the 0.3 s
    // CheckChamber that loops back to it.
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
    assert_eq!(shotgun.reload_ticks, 48 + 36);
    // Each onFire's spread code: the pistol's one round and its kick, the
    // shotgun's pellets and then its blast as a second volley.
    let image = |name: &str| pack.images[&format!("{NS}:image/{name}")].clone();
    let shot = image("standinpistolimage").shot.unwrap();
    assert_eq!(
        (shot.projectiles, shot.spread, shot.recoil),
        (1, 0.0002, 1.0)
    );
    let shotgun = image("standinshotgunimage");
    let shot = shotgun.shot.unwrap();
    assert_eq!(
        (shot.projectiles, shot.spread, shot.recoil),
        (6, 0.004, 2.0)
    );
    assert_eq!(
        shotgun.volleys,
        [Volley {
            projectile: format!("{NS}:projectile/huntingshotgunblastprojectile"),
            projectiles: 1,
            spread: 0.0005,
        }]
    );
    // The heavy gun's fire states each run a script of their own, spread
    // wider as the trigger stays down; every round is made twice the size,
    // which its own damage method ignored.
    let heavy = image("heavymachinegunimage");
    let shot = heavy.shot.unwrap();
    assert_eq!(
        (shot.projectiles, shot.spread, shot.recoil, shot.scale),
        (1, 0.001, 1.0, 2.0)
    );
    let fires: Vec<_> = heavy
        .state_shots
        .iter()
        .map(|(s, shot)| (s.as_str(), shot.spread, shot.recoil, shot.scale))
        .collect();
    assert_eq!(
        fires,
        [("onfire2", 0.002, 0.5, 2.0), ("onfire3", 0.003, 0.5, 2.0)]
    );
    assert!(pack.projectiles[&format!("{NS}:projectile/heavymachinegunprojectile")].fixed_damage);
    // Each fire state's own arm moves, both hands.
    let fire2 = heavy.states.iter().find(|s| s.script == "onFire2").unwrap();
    assert_eq!(
        (fire2.arm.as_str(), fire2.gesture.as_str()),
        ("shiftright", "shiftleft")
    );
    assert!(!pack.projectiles[&format!("{NS}:projectile/standinshotgunprojectile")].fixed_damage);
    // The raycast guns: hitscan with a ray projectile of their own that
    // carries the image's damage, the revolver's kick from its onFire.
    let revolver = image("revolverimage");
    let shot = revolver.shot.unwrap();
    let hitscan = shot.hitscan.unwrap();
    assert_eq!(
        (
            shot.projectiles,
            shot.recoil,
            hitscan.range,
            hitscan.from_eye
        ),
        (1, 3.0, 200.0, true)
    );
    let ray = &pack.projectiles[revolver.projectile.as_deref().unwrap()];
    assert_eq!(
        (ray.id.as_str(), ray.damage, ray.damage_type.as_str()),
        (
            "weapon_modernwarbattles:projectile/revolverimageray",
            15.0,
            "$DamageType::StandinPistol"
        )
    );
    assert_eq!(
        image("batonimage").shot.unwrap().hitscan.unwrap().range,
        4.0
    );
    // The grenade bursts into its cluster shrapnel and its smoke trails.
    let children: Vec<_> = pack.projectiles[&format!("{NS}:projectile/shrapgrenprojectile")]
        .children
        .iter()
        .map(|c| (c.projectile.clone(), c.count, c.on_explode))
        .collect();
    assert_eq!(
        children,
        [
            (
                format!("{NS}:projectile/shrapgrenclusterprojectile"),
                4,
                true
            ),
            (format!("{NS}:projectile/shrapgrentrailprojectile"), 3, true)
        ]
    );
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
fn cmd_state(p) {
    let me = player(p);
    set("state", `${me.mounted}|${me.health}`);
}
fn cmd_mag(p) {
    let m = player(p).magazine;
    set("mag", if m == () { "none" } else {
        `${m.rounds}|${m.size}|${m.ammo}|${m.reserve}`
    });
}
"#;

fn catalog(root: &Path, ns: &str, extra: &[&str]) -> Arc<Catalog> {
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
            { "name": "mag" },
            { "name": "state" }
        ],
        "state": { "global": {
            "mag": { "default": "", "visible": "everyone" },
            "state": { "default": "", "visible": "everyone" }
        } }
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
    let mut packages = vec![
        entry(ns, Side::Shared),
        entry(&format!("{ns}-rules"), Side::Server),
        entry("probe", Side::Server),
    ];
    packages.extend(extra.iter().map(|id| entry(id, Side::Shared)));
    let set = PackageSet {
        schema_version: 1,
        packages,
    };
    Arc::new(Catalog::load(root, &set, true).unwrap_or_else(|e| panic!("{e:#?}")))
}

/// v20's tumble body (`deathVehicle`), as a host loads it from the
/// converted vehicles, on the Steel Ball Kit's pack.
fn tumble_pack() -> bri_vehicles::Pack {
    let mut pack = bri_vehicles::Pack::load(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packages/showcase/steel-ball-kit/assets/vehicles.json"),
    )
    .unwrap();
    let mut tumble = pack.definitions[0].clone();
    let (x, y, z) = (0.6f32, 1.25f32, 0.4f32);
    tumble.id = "v20.vehicle.deathvehicle".into();
    tumble.family = bri_vehicles::schema::Family::Tumble;
    tumble.collision_hulls = vec![
        (0..8)
            .map(|i| {
                [
                    if i & 1 == 1 { x } else { -x },
                    if i & 2 == 2 { y } else { -y },
                    if i & 4 == 4 { z } else { -z },
                ]
            })
            .collect(),
    ];
    tumble.bounds_min = [-x, -y, -z];
    tumble.bounds_max = [x, y, z];
    tumble.inertia_box = [2.0 * x, 2.0 * y, 2.0 * z];
    tumble.mass = 90.0;
    tumble.runover_speed = 3.4e38;
    tumble.runover_damage = 0.0;
    tumble.smash = None;
    tumble.shove = false;
    tumble.harms_only_in_minigames = false;
    tumble.seats = vec![bri_vehicles::schema::Seat {
        node: "mount0".into(),
        transform: bri_vehicles::schema::Transform::default(),
        pose: "root".into(),
        controls: false,
        weapon: false,
    }];
    pack.definitions.push(tumble);
    // A vehicle a swing can wreck.
    let mut target = pack.definitions[0].clone();
    target.id = "adventure-test:vehicle/target".into();
    target.max_damage = 120.0;
    pack.definitions.push(target);
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
    fn new(root: &Path, out: &Path) -> Self {
        Self::with(root, out, NS)
    }
    fn with(root: &Path, out: &Path, ns: &str) -> Self {
        Self::with_add_ons(root, out, ns, &[])
    }
    /// With other imports in `<root>/addons` enabled beside it, by id.
    fn with_add_ons(root: &Path, out: &Path, ns: &str, extra: &[&str]) -> Self {
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
        let parts = extra
            .iter()
            .map(|id| (format!("addons/{id}"), pack(&root.join("addons").join(id))))
            .collect();
        let (merged, notes) = pack(out).merge(parts);
        assert!(notes.is_empty(), "{notes:?}");
        s.set_weapon_pack(merged).unwrap();
        // Item boxes from the import's item physics, as a host loads them.
        let physics: Value =
            serde_json::from_slice(&std::fs::read(out.join("assets/item-physics.json")).unwrap())
                .unwrap();
        s.set_item_bounds(serde_json::from_value(physics["items"].clone()).unwrap())
            .unwrap();
        s.set_vehicle_pack(tumble_pack(), Vec::new()).unwrap();
        s.install_packages(catalog(root, ns, extra), None).unwrap();
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
        self.noted("mag")
    }
    /// Whether `owner` rides something (a tumble), and their health.
    fn state(&mut self, owner: OwnerId) -> Value {
        self.probe(owner, "state", vec![]);
        self.noted("state")
    }
    fn noted(&self, key: &str) -> Value {
        self.s
            .package_state()
            .packages
            .get("probe")
            .and_then(|ns| ns.global.get(key).cloned())
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

/// The body animations played on `owner` since the last look, as
/// `(ticks after the first, thread, sequence)`.
fn flinches(g: &mut Game, owner: OwnerId) -> Vec<(u64, u8, String)> {
    let cues: Vec<_> = g
        .s
        .take_cues()
        .into_iter()
        .filter_map(|c| match c.kind {
            bri_sim::presentation::CueKind::WeaponAnimation {
                actor,
                thread,
                sequence,
                image_hand: None,
            } if actor == owner => Some((c.tick, thread, sequence)),
            _ => None,
        })
        .collect();
    let first = cues.first().map_or(0, |c| c.0);
    cues.into_iter()
        .map(|(tick, thread, sequence)| (tick - first, thread, sequence))
        .collect()
}
fn flinch() -> Vec<(u64, u8, String)> {
    [(0, 0, "jump"), (0, 2, "jump"), (6, 0, "plant"), (6, 2, "plant")]
        .map(|(t, thread, s)| (t, thread, s.to_string()))
        .into()
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
    // A head hit flinches the body as the hitbox test did: threads 0 and 2
    // jump, then plant 50 ms (6 ticks) later; a body hit does not.
    g.equip(a, PISTOL);
    g.s.take_cues();
    g.shoot_at(a, b, 1.7);
    assert!((g.health(b) - 90.0).abs() < 0.5, "{}", g.health(b));
    assert_eq!(flinches(&mut g, b), vec![]);
    g.shoot_at(a, b, 2.45);
    assert!((g.health(b) - 75.0).abs() < 0.5, "{}", g.health(b));
    assert_eq!(flinches(&mut g, b), flinch());
    g.looks.get_mut(&b).unwrap().crouch = true;
    g.steps(30);
    g.shoot_at(a, b, 0.9);
    assert!((g.health(b) - (75.0 - 31.5)).abs() < 0.5, "{}", g.health(b));
    assert_eq!(flinches(&mut g, b), flinch());
    assert_eq!(g.mag(a), json!("9|12|pistol|64"));

    // The light key reloads a magazine that is not full from the ready
    // state; with a full one it works the light, as the ammo system's
    // serverCmdLight fell through to the original.
    let light = |g: &Game| g.s.vitals()[&a].light;
    g.cmd(a, Command::ToggleLight);
    g.steps(600);
    assert_eq!(g.mag(a), json!("12|12|pistol|61"));
    assert!(!light(&g));
    g.cmd(a, Command::ToggleLight);
    assert!(light(&g));
}

/// A dropped gun touched by a player who carries it and is short of its
/// ammo empties its magazine into their reserve, then is picked up as
/// usual with what is left; one with no magazine of its own (put in the
/// world by a rule) gives a box's worth, capped as the original capped it.
#[test]
fn a_dropped_spare_gun_empties_its_magazine_into_the_reserve() {
    let (dir, out, report) = imported("dropped");
    assert!(report.ports[0].applied, "{:?}", report.ports[0].reason);
    let mut g = Game::new(&dir.0, &out);
    let (a, b) = duel(&mut g, &[PISTOL], 1.5);
    g.equip(a, PISTOL);
    g.equip(b, PISTOL);
    for _ in 0..3 {
        g.cmd(b, Command::WeaponTrigger { down: true });
        g.steps(2);
        g.cmd(b, Command::WeaponTrigger { down: false });
        g.steps(40);
    }
    assert_eq!(g.mag(b), json!("9|12|pistol|32"));
    // B throws it down at their feet and steps aside; A walks over it.
    g.looks.get_mut(&b).unwrap().pitch = -1.5;
    g.steps(4);
    g.cmd(b, Command::DropTool { slot: 0 });
    g.looks.get_mut(&b).unwrap().right = 1.0;
    g.steps(60);
    g.looks.get_mut(&b).unwrap().right = 0.0;
    g.looks.get_mut(&a).unwrap().forward = 1.0;
    g.steps(40);
    g.looks.get_mut(&a).unwrap().forward = 0.0;
    g.steps(20);
    // A keeps their own full magazine; the 9 rounds join the reserve.
    assert_eq!(g.mag(a), json!("12|12|pistol|41"));
    assert_eq!(g.s.tool_inventories()[&a].slots.iter().flatten().count(), 2);
    // A pistol with no magazine: a box's worth, capped at 64.
    g.probe(a, "drop", vec![PackageArg::String(PISTOL.into())]);
    g.steps(30);
    assert_eq!(g.mag(a), json!("12|12|pistol|64"));
}

/// A minigame of A and B, B `distance` in front of A, A carrying `items`.
fn duel(g: &mut Game, items: &[&str], distance: f32) -> (OwnerId, OwnerId) {
    let a = g.join("A", Vec3::new(0.0, 0.05, 0.0));
    let b = g.join("B", Vec3::new(0.0, 0.05, -distance));
    g.steps(2);
    g.s.set_spawn_points(vec![g.feet(a)]).unwrap();
    let mut loadout: [Option<String>; 5] = Default::default();
    for (slot, item) in items.iter().enumerate() {
        loadout[slot] = Some((*item).into());
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
    (a, b)
}

/// The raycast guns' host rules without the crit Add-On: the baton's swing
/// kills outright, and the revolver's hit neither crits nor shoves, as the
/// original's crit test never ran without `CritProjectile`.
#[test]
fn hitscan_crits_and_melee_kills_play_in_a_hosted_game() {
    let (dir, out, report) = imported("rays");
    assert!(report.ports[0].applied, "{:?}", report.ports[0].reason);
    let mut g = Game::new(&dir.0, &out);
    let revolver = format!("{NS}:weapon/revolveritem");
    let baton = format!("{NS}:weapon/batonitem");
    let (a, b) = duel(&mut g, &[&revolver, &baton], 3.0);
    g.equip(a, &baton);
    g.s.take_cues();
    g.shoot_at(a, b, 1.2);
    assert_eq!(g.health(b), 0.0);
    // Its own kill message, and one of its pair of hit sounds.
    let said = |g: &mut Game, text: &str| {
        let notices = g.s.take_private_notices();
        let found = notices
            .iter()
            .any(|(_, n)| matches!(n, Notice::Chat(t) if t.contains(text)));
        assert!(found, "{text}: {notices:?}");
    };
    said(&mut g, "clubbed");
    let sounds: Vec<_> =
        g.s.take_cues()
            .into_iter()
            .filter_map(|c| match c.kind {
                bri_sim::presentation::CueKind::WeaponSound { profile } => Some(profile),
                _ => None,
            })
            .collect();
    assert!(
        sounds
            .iter()
            .any(|p| p.starts_with(&format!("{NS}:sound/standinclubsound"))),
        "{sounds:?}"
    );
    // Back at the same spot once respawned (a click after the delay).
    g.steps(300);
    g.cmd(b, Command::Respawn);
    // Past the spawn protection.
    g.steps(330);
    assert_eq!(g.health(b), 100.0);
    g.equip(a, &revolver);
    let before = g.feet(b);
    g.shoot_at(a, b, 1.2);
    assert!((g.health(b) - 85.0).abs() < 0.5, "{}", g.health(b));
    assert!((g.feet(b) - before).length() < 0.01, "{before} {}", g.feet(b));
}

/// With the Critical Hit Emote's stand-in imported and on, the revolver's
/// hit is a crit (×3, as nearly every body hit is under the original's
/// height test) that shoves its target away and up, bursts the crit
/// effect on them with its sound in their ears, and plays the crit sounds
/// for the shooter; a crit kill shows the gun's crit kill message.
#[test]
fn crits_play_with_the_critical_hit_emote() {
    let (dir, out, report) = imported("crits");
    assert!(report.ports[0].applied, "{:?}", report.ports[0].reason);
    let critical = import(&Options {
        input: Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ports/Emote_Critical"),
        out: dir.0.join("addons/emote_critical"),
        reference: None,
        core: vec![],
        version: "1.0.0".into(),
    })
    .unwrap();
    // Its burst draws its own particle, though the emitter names a base
    // game brick node this import cannot see.
    let effects: Value = serde_json::from_slice(
        &std::fs::read(dir.0.join("addons/emote_critical/assets/weapons.json")).unwrap(),
    )
    .unwrap();
    assert!(
        critical
            .unsupported
            .iter()
            .all(|f| f.what.starts_with("file ")),
        "{:?}",
        critical.unsupported
    );
    assert!(effects["effects"]["emitters"].as_array().is_some_and(|e| e.len() == 1), "{effects}");
    let rules: Value = serde_json::from_slice(
        &std::fs::read(dir.0.join(format!("addons/{NS}-rules/package.json"))).unwrap(),
    )
    .unwrap();
    assert_eq!(rules["optional_dependencies"], json!({ "emote_critical": "*" }));

    let mut g = Game::with_add_ons(&dir.0, &out, NS, &["emote_critical"]);
    let revolver = format!("{NS}:weapon/revolveritem");
    let (a, b) = duel(&mut g, &[&revolver], 3.0);
    g.equip(a, &revolver);
    g.s.take_cues();
    g.s.take_private_notices();
    let before = g.feet(b);
    g.shoot_at(a, b, 1.2);
    assert!((g.health(b) - 55.0).abs() < 0.5, "{} {:?}", g.health(b), g.s.package_diagnostics());
    assert!(g.feet(b).z < before.z - 0.05, "{before} {}", g.feet(b));
    let cues = g.s.take_cues();
    assert!(
        cues.iter().any(|c| matches!(&c.kind,
            bri_sim::presentation::CueKind::WeaponEffect { definition, scale, .. }
                if definition.eq_ignore_ascii_case("critexplosion") && *scale == 1.0)),
        "{cues:?}"
    );
    assert!(
        cues.iter().any(|c| matches!(&c.kind,
            bri_sim::presentation::CueKind::WeaponSound { profile }
                if profile == "emote_critical:sound/critfiresound")),
        "{cues:?}"
    );
    let notices = g.s.take_private_notices();
    let heard = |who: OwnerId, sound: &str| {
        notices
            .iter()
            .any(|(o, n)| *o == who && matches!(n, Notice::Sound(p) if p == sound))
    };
    assert!(heard(b, "emote_critical:sound/critrecievesound"), "{notices:?}");
    assert!(heard(a, "emote_critical:sound/crithitsound"), "{notices:?}");
    g.shoot_at(a, b, 1.2);
    g.shoot_at(a, b, 1.2);
    assert_eq!(g.health(b), 0.0);
    let notices = g.s.take_private_notices();
    assert!(
        notices
            .iter()
            .any(|(_, n)| matches!(n, Notice::Chat(t) if t.contains("hit B hard"))),
        "{notices:?}"
    );
}

/// The melee swings wreck a vehicle as they kill a player (twice what it
/// can take); a revolver shot only hurts it.
#[test]
fn melee_swings_wreck_vehicles() {
    let (dir, out, report) = imported("wreck");
    assert!(report.ports[0].applied, "{:?}", report.ports[0].reason);
    let mut g = Game::new(&dir.0, &out);
    let revolver = format!("{NS}:weapon/revolveritem");
    let baton = format!("{NS}:weapon/batonitem");
    let (a, _) = duel(&mut g, &[&revolver, &baton], 12.0);
    let target = "adventure-test:vehicle/target";
    let spawn = |g: &mut Game| {
        let id =
            g.s.spawn_vehicle_at(a, target, Vec3::new(0.0, 2.0, -3.0), 0.0, Vec3::ZERO)
                .unwrap();
        g.steps(120);
        id
    };
    let pose = |g: &Game, id: u64| {
        let v =
            g.s.vehicle_poses()
                .into_iter()
                .find(|v| v.id == id)
                .unwrap();
        Vec3::from(v.position)
    };
    let destroyed = |g: &Game, id: u64| {
        g.s.vehicle_infos()
            .into_iter()
            .find(|v| v.id == id)
            .is_none_or(|v| v.destroyed)
    };
    let swing_at = |g: &mut Game, id: u64| {
        let at = pose(g, id);
        let eye = g.feet(a).y + 2.156;
        g.looks.get_mut(&a).unwrap().pitch = ((at.y - eye) / (g.feet(a).z - at.z)).atan();
        g.steps(4);
        g.cmd(a, Command::WeaponTrigger { down: true });
        g.steps(2);
        g.cmd(a, Command::WeaponTrigger { down: false });
        g.steps(10);
    };
    let first = spawn(&mut g);
    g.equip(a, &revolver);
    swing_at(&mut g, first);
    assert!(!destroyed(&g, first));
    g.equip(a, &baton);
    swing_at(&mut g, first);
    assert!(destroyed(&g, first));
}

const GLASS: &str = "weapon_adventurepack";

/// The Glass release's port on its stand-in
/// (`tests/fixtures/ports/Weapon_AdventurePack`, CC0): magazines with this
/// release's reserves, a round-at-a-time reload from its states, the
/// Paired Shotgun's full two-barrel shot (the second of its onFire's two
/// shots, two rounds a pull), the hitscan sniper from its raycast fields,
/// and a taser whose own hit does no damage.
#[test]
fn glass_release_guns_shoot_like_their_scripts() {
    let (_dir, out, report) = imported_as("Weapon_AdventurePack", GLASS, "glass");
    let port = &report.ports[0];
    assert!(port.applied, "{:?}", port.reason);
    assert_eq!(port.port, "weapon_adventurepack");
    bri_addon_import::ports::check_pins(&out).unwrap();
    let pack = pack(&out);
    let image = |name: &str| pack.images[&format!("{GLASS}:image/{name}")].clone();
    let pistol = image("standinpistolimage");
    let m = pistol.magazine.unwrap();
    assert_eq!(
        (m.size, m.ammo.as_str(), m.reserve, m.max_reserve),
        (10, "pistol", 24, 120)
    );
    let shot = pistol.shot.unwrap();
    assert_eq!(
        (shot.projectiles, shot.spread, shot.recoil),
        (1, 0.0002, 1.0)
    );
    // What its onFire did by hand: the fire sound and arm move on its Fire
    // state, the recoil blast's camera shake as the shot's kick.
    let fire = pistol.states.iter().find(|s| s.script == "onFire").unwrap();
    assert_eq!(
        (fire.sound.as_str(), fire.arm.as_str()),
        ("weapon_adventurepack:sound/standinfiresound", "shiftright")
    );
    let kick = shot.kick.unwrap();
    assert_eq!(
        (kick.amplitude, kick.frequency, kick.seconds),
        (0.9, 3.0, 0.4)
    );
    let paired = image("pairedshotgunimage");
    let m = paired.magazine.unwrap();
    assert_eq!(
        (m.size, m.per_shot, m.one_by_one, m.reload_ticks),
        (6, 2, true, 60 + 30)
    );
    let shot = paired.shot.unwrap();
    assert_eq!(
        (shot.projectiles, shot.spread, shot.recoil),
        (8, 0.004, 4.0)
    );
    assert_eq!(
        paired.volleys,
        [Volley {
            projectile: format!("{GLASS}:projectile/pairedshotgunblastprojectile"),
            projectiles: 1,
            spread: 0.0005,
        }]
    );
    // Its last two rounds fire the single barrel, whatever is left.
    let last = paired.last_shot.unwrap();
    assert_eq!(
        (last.shot.projectiles, last.shot.spread, last.shot.recoil),
        (4, 0.002, 2.0)
    );
    assert_eq!(last.volleys.len(), 1);
    assert_eq!(m.last_rounds, 2);
    let sniper = image("sniperrifleimage2");
    let hitscan = sniper.shot.unwrap().hitscan.unwrap();
    assert_eq!((hitscan.range, hitscan.from_eye), (300.0, false));
    assert!(hitscan.tracer.is_some());
    let ray = &pack.projectiles[sniper.projectile.as_deref().unwrap()];
    assert_eq!(
        (ray.id.as_str(), ray.damage),
        ("weapon_adventurepack:projectile/sniperrifleimage2ray", 40.0)
    );
    assert_eq!(
        pack.projectiles[&format!("{GLASS}:projectile/taserprojectile")].damage,
        0.0
    );
}

/// The Glass release's host rules in a hosted game: the hitscan sniper's
/// headshot multiplier (from its image, as its onRaycastCollision used
/// it), the taser's tumble for its length and no damage, and its boxes:
/// a typed box twice its amount, the box of every type once per type.
#[test]
fn glass_release_taser_tumbles_and_sniper_headshots() {
    let (dir, out, report) = imported_as("Weapon_AdventurePack", GLASS, "glass-hosted");
    assert!(report.ports[0].applied, "{:?}", report.ports[0].reason);
    let mut g = Game::with(&dir.0, &out, GLASS);
    let pistol = format!("{GLASS}:weapon/standinpistolitem");
    let sniper = format!("{GLASS}:weapon/sniperrifleitem");
    let taser = format!("{GLASS}:weapon/taseritem");
    let (a, b) = duel(&mut g, &[&pistol, &sniper, &taser], 6.0);

    g.equip(a, &pistol);
    assert_eq!(g.mag(a), json!("10|10|pistol|24"));
    g.probe(
        a,
        "drop",
        vec![PackageArg::String(format!(
            "{GLASS}:weapon/standinammopistolitem"
        ))],
    );
    g.steps(30);
    assert_eq!(g.mag(a), json!("10|10|pistol|72"));
    g.probe(
        a,
        "drop",
        vec![PackageArg::String(format!(
            "{GLASS}:weapon/standinammoitem"
        ))],
    );
    g.steps(30);
    assert_eq!(g.mag(a), json!("10|10|pistol|96"));

    // The sniper's 40, doubled on the head, which flinches the body even
    // as the shot kills.
    g.equip(a, &sniper);
    g.s.take_cues();
    g.shoot_at(a, b, 1.2);
    assert!((g.health(b) - 60.0).abs() < 0.5, "{}", g.health(b));
    assert_eq!(flinches(&mut g, b), vec![]);
    g.shoot_at(a, b, 2.45);
    assert_eq!(g.health(b), 0.0);
    assert_eq!(flinches(&mut g, b), flinch());
    g.steps(300);
    g.cmd(b, Command::Respawn);
    g.steps(330);

    // The taser: no damage, a tumble that ends after its 4 seconds.
    g.equip(a, &taser);
    g.shoot_at(a, b, 1.2);
    assert_eq!(g.state(b), json!("true|100.0"));
    g.steps(4 * 120);
    assert_eq!(g.state(b), json!("false|100.0"));
}

/// The frag grenade cooks: its fuse lights as the pin drops, counting down
/// in the middle of the thrower's screen, and the thrown grenade goes off
/// with what is left of it, its bomblets bursting one after another within
/// their own fuses. Held for the whole fuse, it goes off in the hand: the
/// holder puts it away and keeps the grenade.
#[test]
fn a_grenade_cooks_in_the_hand() {
    let (dir, out, report) = imported("cook");
    assert!(report.ports[0].applied, "{:?}", report.ports[0].reason);
    let mut g = Game::new(&dir.0, &out);
    let grenade = format!("{NS}:weapon/shrapgrenitem");
    let (a, _) = duel(&mut g, &[&grenade], 30.0);
    g.equip(a, &grenade);
    // Up into the open sky, where it meets nothing.
    g.looks.get_mut(&a).unwrap().pitch = 1.2;
    let live = |g: &Game, name: &str| {
        g.s.snapshot()
            .weapons
            .projectiles
            .iter()
            .filter(|p| p.definition == format!("{NS}:projectile/{name}"))
            .count()
    };
    g.s.take_private_notices();
    g.cmd(a, Command::WeaponTrigger { down: true });
    g.steps(240);
    let prints: Vec<String> = g
        .s
        .take_private_notices()
        .into_iter()
        .filter_map(|(o, n)| match n {
            Notice::Center { text, .. } if o == a => Some(text),
            _ => None,
        })
        .collect();
    let line = |left: &str| format!("\u{E005}{left}\u{E006} cooking time left.");
    assert_eq!(prints.first(), Some(&line("4 Seconds")), "{prints:?}");
    assert_eq!(prints.get(1), Some(&line("3.9 seconds")), "{prints:?}");
    // A tenth of a second apart, from a tenth after the pin dropped.
    assert!((19..=20).contains(&prints.len()), "{prints:?}");
    g.cmd(a, Command::WeaponTrigger { down: false });
    g.steps(4);
    assert_eq!(live(&g, "shrapgrenprojectile"), 1);
    // About two seconds of fuse were left, well short of its lifetime.
    g.steps(220);
    assert_eq!(live(&g, "shrapgrenprojectile"), 1);
    g.steps(30);
    assert_eq!(live(&g, "shrapgrenprojectile"), 0);
    assert!(live(&g, "shrapgrenclusterprojectile") > 0);
    // The bomblets go off within their 400 ms, before their own 500.
    g.steps(50);
    assert_eq!(live(&g, "shrapgrenclusterprojectile"), 0);

    // The next grenade, from the reserve, held past its fuse.
    g.steps(240);
    assert_eq!(g.mag(a), json!("1|1|frag-grenades|0"));
    g.cmd(a, Command::WeaponTrigger { down: true });
    g.steps(485);
    assert!(live(&g, "shrapgrenclusterprojectile") > 0);
    let held = g.s.snapshot().weapons.images.get(&a).cloned().unwrap_or_default();
    assert!(held.is_empty(), "{held:?}");
    assert!(
        g.s.tool_inventories()[&a]
            .slots
            .iter()
            .any(|s| s.as_deref() == Some(grenade.as_str()))
    );
    g.cmd(a, Command::WeaponTrigger { down: false });
}
