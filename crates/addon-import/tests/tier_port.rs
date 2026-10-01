//! The Tier+Tactical Tier 1 port (`ports/weapon_package_tier1` on the shared
//! `ports/_shared/tier-tactical` fragment) on our stand-in
//! (`tests/fixtures/ports/Weapon_Package_Tier1`, CC0): the same folder name
//! and the shape of Kai's ammo system and gun scripts, with our own guns,
//! names and numbers.
mod common;

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
            ..Check::default()
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
    // `setShapeName(getWord(%obj.TT_ammoPickup[0], 1))`: the box shows
    // its rounds; the pile names none.
    assert_eq!(
        pack.items[&format!("{NS}:weapon/standinnineitem")].label,
        "30"
    );
    assert_eq!(
        pack.items[&format!("{NS}:weapon/standinpileitem")].label,
        ""
    );
    // `%obj.rotate = true`: the box turns where it lies; the pile does not.
    assert!(pack.items[&format!("{NS}:weapon/standinnineitem")].rotate);
    assert!(!pack.items[&format!("{NS}:weapon/standinpileitem")].rotate);
    // TT_displayAmmo's four seconds, and a dry pull's TT_onEmptyFire.
    assert_eq!(mag.display_ticks, 480);
    assert_eq!(mag.display_scripts, ["TT_onEmptyFire"]);

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
fn cmd_who(p) {
    let me = player(p);
    set("who", `${me.archetype}|${me.image}`);
}
fn cmd_worn(p) {
    set("worn", player(p).emote);
}
fn cmd_hurt(p, amount) {
    damage(p, amount);
}
fn cmd_teams(p) {
    set_teams(player(p).minigame, [#{ name: "Red", color: 0 }, #{ name: "Blue", color: 1 }]);
}
fn cmd_team(p, name) {
    for t in minigame(player(p).minigame).teams {
        if t.name == name {
            set_team(p, t.id);
        }
    }
}
fn cmd_mag(p) {
    let m = player(p).magazine;
    set("mag", if m == () { "none" } else {
        `${m.rounds}|${m.size}|${m.ammo}|${m.reserve}`
    });
}
"#;

/// `ns`, its rules and the probe, with other imports in `<root>/addons`
/// enabled beside them by id.
fn catalog(root: &Path, ns: &str, extra: &[&str]) -> Arc<Catalog> {
    let dir = root.join("addons/probe");
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = json!({
        "schema_version": 1, "id": "probe", "version": "1.0.0", "api": 1,
        "name": "probe", "license": "CC0-1.0",
        "capabilities": ["player", "world.edit", "damage", "minigame"],
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
            { "name": "mag" },
            { "name": "who" },
            { "name": "worn" },
            { "name": "hurt", "args": ["float"] },
            { "name": "teams" },
            { "name": "team", "args": ["string"] }
        ],
        "state": { "global": {
            "mag": { "default": "", "visible": "everyone" },
            "who": { "default": "", "visible": "everyone" },
            "worn": { "default": "", "visible": "everyone" }
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
    let mut packages = vec![entry(ns, Side::Shared), entry("probe", Side::Server)];
    // Its host rules, when its port has any.
    let rules = format!("{ns}-rules");
    if root.join("addons").join(&rules).is_dir() {
        packages.push(entry(&rules, Side::Server));
    }
    packages.extend(common::base_entries(&root.join("addons").join(ns)));
    // A companion's host rules only the host loads.
    packages.extend(extra.iter().map(|id| {
        let side = if id.ends_with("-rules") {
            Side::Server
        } else {
            Side::Shared
        };
        entry(id, side)
    }));
    let set = PackageSet {
        schema_version: 1,
        packages,
    };
    Arc::new(Catalog::load(root, &set, true).unwrap_or_else(|e| panic!("{e:#?}")))
}

struct Game {
    s: Session,
    ns: String,
    seq: BTreeMap<OwnerId, u64>,
    moves: BTreeMap<OwnerId, u64>,
    looks: BTreeMap<OwnerId, MoveInput>,
}
impl Game {
    fn new(root: &Path, out: &Path) -> Self {
        Self::with_add_ons(root, out, NS, &[])
    }
    /// `ns` imported to `out`, with other imports in `<root>/addons`
    /// enabled beside it, by id.
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
            .filter(|id| !id.ends_with("-rules"))
            .map(|id| (format!("addons/{id}"), pack(&root.join("addons").join(id))))
            .collect();
        let (merged, notes) = pack(out).merge(parts);
        // Only a name two Add-Ons both declare, each keeping its own.
        assert!(
            notes.iter().all(|n| n.contains(" is kept as ")),
            "{notes:?}"
        );
        s.set_weapon_pack(merged).unwrap();
        let physics: Value =
            serde_json::from_slice(&std::fs::read(out.join("assets/item-physics.json")).unwrap())
                .unwrap();
        s.set_item_bounds(serde_json::from_value(physics["items"].clone()).unwrap())
            .unwrap();
        s.install_packages(catalog(root, ns, extra), None).unwrap();
        Self {
            s,
            ns: ns.into(),
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
    /// The player hosting the game, who changes its Server Settings.
    fn join_host(&mut self, name: &str, at: Vec3) -> OwnerId {
        let owner = self.s.join(name.into(), at, true).unwrap();
        self.looks.insert(owner, MoveInput::default());
        owner
    }
    /// The host sets server-wide Add-On settings, as the Admin menu's
    /// Add-On Settings sends them with its Server Settings.
    fn configure(
        &mut self,
        host: OwnerId,
        values: &[(&str, bri_package::setting::SettingValue)],
    ) -> anyhow::Result<()> {
        let mut settings = self.s.server_settings().clone();
        for (key, value) in values {
            settings.addon_settings.insert((*key).into(), value.clone());
        }
        let n = self.seq.entry(host).or_default();
        *n += 1;
        self.s
            .command(
                host,
                *n,
                Command::Admin(bri_admin::Request::new(bri_admin::Action::HostConfigure {
                    settings,
                })),
            )
            .map(|_| ())
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
        let item = format!("{}:weapon/{item}", self.ns);
        self.probe(owner, "drop", vec![PackageArg::String(item)]);
        self.steps(30);
    }
    fn mag(&mut self, owner: OwnerId) -> Value {
        self.ask(owner, "mag")
    }
    /// What `owner` is and holds: `<archetype>|<image>`.
    fn who(&mut self, owner: OwnerId) -> String {
        self.ask(owner, "who")
            .as_str()
            .unwrap_or_default()
            .to_owned()
    }
    fn ask(&mut self, owner: OwnerId, what: &str) -> Value {
        self.probe(owner, what, vec![]);
        self.s
            .package_state()
            .packages
            .get("probe")
            .and_then(|ns| ns.global.get(what).cloned())
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
    /// Draws `item`, of the Add-On under test unless it is a whole id.
    fn equip(&mut self, owner: OwnerId, item: &str) {
        let item = if item.contains(':') {
            item.to_owned()
        } else {
            format!("{}:weapon/{item}", self.ns)
        };
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

/// The copy's RTB preferences the rules read are server settings: the
/// report says so (and keeps one no rule reads, Recoil, as a gap at its
/// default), the host changes them in its Server Settings (a value out of
/// range is refused), and a new life, ammo items and a death follow them.
#[test]
fn tier_preferences_are_server_settings_the_host_changes() {
    use bri_package::setting::SettingValue as V;
    let (dir, out, report) = imported("prefs");
    assert!(report.ports[0].applied, "{:?}", report.ports[0].reason);
    for pref in [
        "Start9MM",
        "Max9MM",
        "PlayerAmmoDrop",
        "Recoil",
        "Ammo",
        "DisplayAmmo",
    ] {
        let what = format!("RTB_registerPref $Pref::Server::TT::{pref}");
        let row = report.ported.iter().find(|f| f.what == what);
        assert!(
            row.and_then(|f| f.resolution.as_deref())
                .is_some_and(|r| r.contains("server setting")),
            "{what}: {:?}",
            report.ported
        );
    }
    assert!(
        !report
            .unsupported
            .iter()
            .any(|f| f.what.contains("$Pref::")),
        "{:?}",
        report.unsupported
    );
    // The guns' fields the preferences decide, as bindings of the pack.
    let authored = pack(&out);
    let bound = authored.bound_settings();
    for global in [
        "Recoil",
        "Ammo",
        "DisplayAmmo",
        "DisplayTime",
        "DisableBulletSlow",
    ] {
        assert!(
            bound.contains(format!("$Pref::Server::TT::{global}").as_str()),
            "{bound:?}"
        );
    }
    let rules: Value = serde_json::from_slice(
        &std::fs::read(dir.0.join(format!("addons/{NS}-rules/behaviour.json"))).unwrap(),
    )
    .unwrap();
    let start = rules["settings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["key"] == "tt_start9mm")
        .expect("the starting 9mm setting");
    assert_eq!(start["scope"], "server");
    assert_eq!(start["default"], 140);
    assert_eq!(start["global"], "$Pref::Server::TT::Start9MM");

    let mut g = Game::new(&dir.0, &out);
    let a = g.join_host("A", Vec3::new(0.0, 0.05, 0.0));
    let b = g.join("B", Vec3::new(0.0, 0.05, -6.0));
    g.steps(2);
    let start = format!("{NS}-rules:tt_start9mm");
    let most = format!("{NS}-rules:tt_max9mm");
    let drops = format!("{NS}-rules:tt_playerammodrop");
    assert!(
        g.configure(a, &[(start.as_str(), V::Int(999))]).is_err(),
        "999 is past the setting's 280"
    );
    assert!(
        g.configure(a, &[(start.as_str(), V::Bool(true))]).is_err(),
        "a number, not on or off"
    );
    g.configure(
        a,
        &[
            (start.as_str(), V::Int(50)),
            (most.as_str(), V::Int(100)),
            (drops.as_str(), V::Bool(false)),
        ],
    )
    .unwrap();
    g.s.set_spawn_points(vec![g.feet(a)]).unwrap();
    let mut loadout: [Option<String>; 5] = Default::default();
    loadout[0] = Some(format!("{NS}:weapon/standinsidearmitem"));
    loadout[1] = Some(format!("{NS}:weapon/standinrifleitem"));
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
    assert_eq!(g.mag(a), json!("6|6|tt-9mm|50"), "Starting 9mm");
    g.drop(a, "standinpileitem");
    assert_eq!(
        g.mag(a),
        json!("6|6|tt-9mm|100"),
        "a full load is the host's most"
    );
    // With Players Drop Ammo off, B leaves no bag.
    g.equip(a, "standinrifleitem");
    for _ in 0..3 {
        g.shoot_at(a, b, 2.45);
    }
    g.steps(240);
    assert!(
        !g.s.weapon_view()
            .drops
            .iter()
            .any(|d| d.item == format!("{NS}:weapon/ammodroppeditem")),
        "no bag"
    );

    // The Ammo System and Recoil decide the guns' fields as the host
    // changes them, on the host and for players alike.
    let ammo = format!("{NS}-rules:tt_ammo");
    let recoil = format!("{NS}-rules:tt_recoil");
    let played = |g: &Game| {
        authored
            .with_settings(|name| g.s.weapon_settings().get(name).cloned())
            .unwrap()
    };
    let kicks = |p: &Pack| {
        p.images[&format!("{NS}:image/standinsidearmimage")]
            .shot
            .as_ref()
            .unwrap()
            .kick
            .is_some()
    };
    assert!(kicks(&played(&g)));
    g.equip(a, "standinsidearmitem");
    g.shoot_at(a, b, 1.2);
    assert_eq!(
        g.mag(a),
        json!("5|6|tt-9mm|100"),
        "T+T2: the magazine's round"
    );
    g.configure(
        a,
        &[
            (ammo.as_str(), V::Int(2)),
            (recoil.as_str(), V::Bool(false)),
        ],
    )
    .unwrap();
    assert_eq!(
        g.s.weapon_settings()[&"$Pref::Server::TT::Ammo".to_owned()],
        "2"
    );
    assert!(!kicks(&played(&g)), "Recoil off");
    for _ in 0..8 {
        g.shoot_at(a, b, 1.2);
    }
    assert_eq!(
        g.mag(a),
        json!("6|6|tt-9mm|100"),
        "Classic: nothing is used"
    );
    assert!(
        g.configure(a, &[(ammo.as_str(), V::Int(4))]).is_err(),
        "not a choice"
    );
    g.configure(a, &[(ammo.as_str(), V::Int(3))]).unwrap();
    g.shoot_at(a, b, 1.2);
    assert_eq!(
        g.mag(a),
        json!("99|6|tt-9mm|99"),
        "Arena: shots are the reserve's"
    );
}

/// The stand-in `addon` imported as `ns` with the stand-ins it requires
/// (`refs`) as its reference, as the drop folder imports one pack with the
/// others beside it.
fn imported_on(
    addon: &str,
    ns: &str,
    refs: &[&str],
    name: &str,
) -> (Dir, PathBuf, bri_addon_import::report::Report) {
    let dir =
        Dir(std::env::temp_dir().join(format!("bri-tier-port-{}-{ns}-{name}", std::process::id())));
    let _ = std::fs::remove_dir_all(&dir.0);
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ports");
    for r in refs {
        let copy = dir.0.join("reference/Add-Ons").join(r);
        std::fs::create_dir_all(&copy).unwrap();
        for f in std::fs::read_dir(fixtures.join(r)).unwrap() {
            let f = f.unwrap();
            std::fs::copy(f.path(), copy.join(f.file_name())).unwrap();
        }
    }
    let out = dir.0.join("addons").join(ns);
    let report = import(&Options {
        input: fixtures.join(addon),
        out: out.clone(),
        reference: (!refs.is_empty()).then(|| dir.0.join("reference")),
        core: vec![],
        installed: None,
        version: "1.0.0".into(),
    })
    .unwrap();
    (dir, out, report)
}

/// Another stand-in imported into `<root>/addons/<ns>`, with the stand-ins
/// it requires (`refs`) as its reference, to be enabled with the one under
/// test.
/// `pack` with Tier 1 imported beside it, as the game loads them together:
/// its guns fire Tier 1's projectiles.
fn with_tier1(dir: &Dir, pack: Pack) -> Pack {
    let tier1 = import_beside(&dir.0, "Weapon_Package_Tier1", NS, &[]);
    assert!(tier1.ports[0].applied, "{:?}", tier1.ports[0].reason);
    let (both, notes) = pack.merge(vec![(
        format!("addons/{NS}"),
        self::pack(&dir.0.join("addons").join(NS)),
    )]);
    assert!(notes.is_empty(), "{notes:?}");
    both.validate().unwrap();
    both
}

fn import_beside(
    root: &Path,
    addon: &str,
    ns: &str,
    refs: &[&str],
) -> bri_addon_import::report::Report {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ports");
    let reference = root.join(format!("reference-{ns}"));
    for r in refs {
        let copy = reference.join("Add-Ons").join(r);
        std::fs::create_dir_all(&copy).unwrap();
        for f in std::fs::read_dir(fixtures.join(r)).unwrap() {
            let f = f.unwrap();
            std::fs::copy(f.path(), copy.join(f.file_name())).unwrap();
        }
    }
    import(&Options {
        input: fixtures.join(addon),
        out: root.join("addons").join(ns),
        reference: (!refs.is_empty()).then_some(reference),
        core: vec![],
        installed: None,
        version: "1.0.0".into(),
    })
    .unwrap()
}

/// Tier 1A on Tier 1: the single shotgun shoves its shooter back as it
/// fires its pellets and blast, its recoil shake read from Tier 1's own
/// recoil projectile; the pepperbox casts several rays a shot; the
/// snubnose's headshots get their own kill message; the nailgun, an
/// easter egg the original loads only with a hidden setting, is hidden.
#[test]
fn tier1a_shotgun_knocks_back_and_the_nailgun_stays_hidden() {
    const NS1A: &str = "weapon_package_tier1a";
    let (dir, out, report) = imported_on(
        "Weapon_Package_Tier1A",
        NS1A,
        &["Weapon_Package_Tier1"],
        "guns",
    );
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
        (kick.amplitude, kick.frequency, kick.seconds, kick.radius),
        (0.4, 3.0, 0.4, 10.0),
        "Tier 1's recoil projectile's shake, felt within its radius"
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
    let both = with_tier1(&dir, pack.clone());
    let mut w = WeaponsWorld::new(both.clone()).unwrap();
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

const NS2: &str = "weapon_package_tier2";

/// Tier 2 on Tier 1 (and Tier 1 beside it in the game, as it requires):
/// the assault rifle's round after a pause is a truer one; the light
/// machine gun's second pull fires a free round, its click lays its holder
/// down slowed and its halt stands them up; the battle rifle's and machine
/// gun's rounds slow whoever they hit; the combat shotgun's jet press
/// mounts its blast mode, which fires two shells' worth and hands back;
/// the scoped magnum stays hidden as the original left it out of the spawn
/// list; and the military sniper's head hits crit while the Critical Hit
/// Emote is on.
#[test]
fn tier2_guns_rest_fire_twice_slow_and_switch_modes() {
    let (dir, out, report) = imported_on(
        "Weapon_Package_Tier2",
        NS2,
        &["Weapon_Package_Tier1"],
        "guns",
    );
    let port = &report.ports[0];
    assert!(port.applied, "{:?}", port.reason);
    assert_eq!(port.port, NS2);
    let pack = pack(&out);
    let image = |name: &str| pack.images[&format!("{NS2}:image/{name}")].clone();
    let projectile = |name: &str| format!("{NS2}:projectile/{name}");

    let ar = image("tassaultrifleimage").shot.unwrap();
    let rested = ar.rested.clone().unwrap();
    assert_eq!(rested.after_ticks, 48);
    assert_eq!(
        rested.projectile.as_deref(),
        Some(&*projectile("tassaultrifleprojectile2"))
    );
    let lmg = image("lightmachinegunimage");
    let second = &lmg.state_shots["onfire2"];
    assert!(second.free, "{second:?}");
    assert_eq!(
        pack.projectiles[&projectile("battlerifleprojectile1")].slow,
        Some(Slow { divisor: 3.0 })
    );
    assert_eq!(
        pack.projectiles[&projectile("lightmachinegunprojectile1")].slow,
        Some(Slow { divisor: 2.0 })
    );
    let alt = image("combatshotgunaltfireimage");
    assert_eq!(alt.magazine.as_ref().unwrap().per_shot, 2);
    assert_eq!(alt.volleys.len(), 1, "{:?}", alt.volleys);
    assert!(pack.items[&format!("{NS2}:weapon/scopedmagnumitem")].hidden);
    assert!(!pack.items[&format!("{NS2}:weapon/magnumitem")].hidden);
    let sniper = image("militarysniperimage").shot.unwrap().hitscan.unwrap();
    assert_eq!(sniper.range, 300.0);
    assert!(sniper.from_eye);
    // The slowed body the click lays on, a player type of the import.
    let laid: Value = serde_json::from_slice(
        &std::fs::read(out.join("assets/archetypes/lmgarmor.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(laid["movement"]["forward"], json!(3.0));
    assert_eq!(laid["movement"]["can_jet"], json!(false));
    // `firstPersonOnly`: the gunner sees from the eye; isSurvivor is a mark
    // only other Add-Ons read.
    assert_eq!(laid["first_person_only"], json!(true));
    assert!(
        !report
            .unsupported
            .iter()
            .any(|u| u.what.contains("LMGArmor")),
        "{:?}",
        report.unsupported
    );

    let both = with_tier1(&dir, pack.clone());
    let hold = |item: &str| {
        let mut w = WeaponsWorld::new(both.clone()).unwrap();
        w.add_actor(A, 5).unwrap();
        let slot = w.give(A, &format!("{NS2}:weapon/{item}")).unwrap();
        w.equip(A, Some(slot)).unwrap();
        steps(&mut w, 60);
        w
    };
    // A pause, then the truer round; straight after, the usual one.
    let mut w = hold("tassaultrifleitem");
    let first = spawned(&shoot(&mut w));
    let next = spawned(&shoot(&mut w));
    assert_eq!(first, ["tassaultrifleprojectile2"]);
    assert_eq!(next, ["tassaultrifleprojectile1"]);
    // One tap of the machine gun: two rounds out, one off the magazine.
    let mut w = hold("lightmachinegunitem");
    let rounds = w.ammo(A).unwrap().rounds;
    w.trigger(A, true).unwrap();
    let mut out = steps(&mut w, 2);
    w.trigger(A, false).unwrap();
    out.extend(steps(&mut w, 40));
    assert_eq!(
        spawned(&out),
        ["lightmachinegunprojectile1", "lightmachinegunprojectile1"]
    );
    assert_eq!(w.ammo(A).unwrap().rounds, rounds - 1);

    // In a game, with the Critical Hit Emote's stand-in installed.
    import_beside(&dir.0, "Emote_Critical", "emote_critical", &[]);
    let (mut g, a, b) = tier2_duel(&dir.0, &out_dir(&dir.0), &[NS, "emote_critical"]);

    // The machine gun lays A down while the trigger is held, and stands
    // them up when it lets go (shooting at the sky, to spare B).
    g.looks.get_mut(&a).unwrap().pitch = 1.2;
    g.equip(a, "lightmachinegunitem");
    assert_eq!(
        g.who(a),
        format!("v20.player.playerstandardarmor|{NS2}:image/lightmachinegunimage")
    );
    g.cmd(a, Command::WeaponTrigger { down: true });
    g.steps(10);
    assert_eq!(
        g.who(a),
        format!("{NS2}:archetype/lmgarmor|{NS2}:image/lightmachinegunimage")
    );
    g.cmd(a, Command::WeaponTrigger { down: false });
    g.steps(30);
    assert_eq!(
        g.who(a),
        format!("v20.player.playerstandardarmor|{NS2}:image/lightmachinegunimage")
    );

    // Jet with the combat shotgun: its blast mode fires two shells' worth
    // and hands the shotgun back.
    g.equip(a, "combatshotgunitem");
    assert_eq!(g.mag(a), json!("4|4|tt-shotgun|24"));
    g.looks.get_mut(&a).unwrap().jet = true;
    g.steps(4);
    g.looks.get_mut(&a).unwrap().jet = false;
    assert_eq!(
        g.who(a).split('|').nth(1),
        Some(&*format!("{NS2}:image/combatshotgunaltfireimage"))
    );
    g.steps(120);
    assert_eq!(
        g.who(a).split('|').nth(1),
        Some(&*format!("{NS2}:image/combatshotgunimage"))
    );
    assert_eq!(g.mag(a), json!("2|4|tt-shotgun|24"));

    assert_eq!(g.health(b), 100.0);
    // The sniper: 20 to the body, a crit (×3) to the head with the
    // emote's effects, and the crit's own kill message.
    g.equip(a, "militarysniperitem");
    g.shoot_at(a, b, 1.2);
    assert!((g.health(b) - 80.0).abs() < 0.5, "{}", g.health(b));
    g.s.take_cues();
    g.s.take_private_notices();
    g.shoot_at(a, b, 2.45);
    assert!(
        (g.health(b) - 20.0).abs() < 0.5,
        "{} {:?}",
        g.health(b),
        g.s.package_diagnostics()
    );
    let cues = g.s.take_cues();
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
    assert!(
        heard(b, "emote_critical:sound/critrecievesound"),
        "{notices:?}"
    );
    assert!(heard(a, "emote_critical:sound/crithitsound"), "{notices:?}");
    g.shoot_at(a, b, 2.45);
    assert_eq!(g.health(b), 0.0);
    let said: Vec<String> =
        g.s.take_private_notices()
            .into_iter()
            .filter_map(|(_, n)| match n {
                Notice::Chat(text) => Some(text),
                _ => None,
            })
            .collect();
    assert!(said.iter().any(|t| t.contains("A critted B")), "{said:?}");

    // Without the emote, a head hit is a plain one.
    let (mut g, a, b) = tier2_duel(&dir.0, &out_dir(&dir.0), &[NS]);
    g.equip(a, "militarysniperitem");
    g.shoot_at(a, b, 2.45);
    assert!((g.health(b) - 80.0).abs() < 0.5, "{}", g.health(b));
}

fn out_dir(root: &Path) -> PathBuf {
    root.join("addons").join(NS2)
}

/// A and B in a minigame with Tier 2's machine gun, combat shotgun and
/// sniper, 6 units apart, past their spawn protection.
fn tier2_duel(root: &Path, out: &Path, extra: &[&str]) -> (Game, OwnerId, OwnerId) {
    let mut g = Game::with_add_ons(root, out, NS2, extra);
    let a = g.join("A", Vec3::new(0.0, 0.05, 0.0));
    let b = g.join("B", Vec3::new(0.0, 0.05, -6.0));
    g.steps(2);
    g.s.set_spawn_points(vec![g.feet(a)]).unwrap();
    let mut loadout: [Option<String>; 5] = Default::default();
    for (i, item) in [
        "lightmachinegunitem",
        "combatshotgunitem",
        "militarysniperitem",
    ]
    .iter()
    .enumerate()
    {
        loadout[i] = Some(format!("{NS2}:weapon/{item}"));
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
    (g, a, b)
}

const NS2A: &str = "weapon_package_tier2a";

/// Tier 2A on Tier 1 and Tier 2: the burst rifle fires three a pull, its
/// burst checks taking their own rounds; the twin guns' left one fires free
/// beside the right; jet scopes the carbine in, laying Tier 2's slowed body
/// on its holder, and out again; a reload begun scoped hands the carbine
/// back unscoped to reload there. The match pistol, behind the hidden
/// setting, is hidden, and a gun server.cs never runs is not imported.
#[test]
fn tier2a_bursts_scopes_and_the_free_left_gun() {
    let (dir, out, report) = imported_on(
        "Weapon_Package_Tier2A",
        NS2A,
        &["Weapon_Package_Tier1", "Weapon_Package_Tier2"],
        "guns",
    );
    let port = &report.ports[0];
    assert!(port.applied, "{:?}", port.reason);
    assert_eq!(port.port, NS2A);
    let unused = report
        .assets
        .iter()
        .find(|a| a.source.ends_with("Weapon_Unused.cs"))
        .unwrap();
    assert!(unused.notes[0].contains("left out"), "{:?}", unused.notes);
    let pack = pack(&out);
    assert!(
        !pack
            .items
            .contains_key(&format!("{NS2A}:weapon/unusedstaritem"))
    );
    assert!(pack.items[&format!("{NS2A}:weapon/matchpistolitem")].hidden);
    let image = |name: &str| pack.images[&format!("{NS2A}:image/{name}")].clone();

    let burst = image("bullpupimage");
    let check = &burst.magazine.as_ref().unwrap().checks["onBurstCheck"];
    assert!(check.spend, "{check:?}");
    assert!(burst.state_shots["onburstfire"].free);
    assert!(!burst.state_shots.contains_key("onburstcheck"));
    let shot = burst.shot.unwrap();
    assert_eq!(
        (shot.spread, shot.moving_spread, shot.moving_speed),
        (0.0002, Some(0.0008), 0.1)
    );
    assert!(image("dualsmgleftimage").shot.unwrap().free);
    // `playThread(1, armreadyboth)` as the left gun mounts: both arms up.
    assert!(image("dualsmgleftimage").both_arms);
    assert!(!image("dualsmgsimage").shot.unwrap().free);
    let scope = image("snipercarbineimage").magazine.unwrap();
    assert!(scope.checks["TT_onLoadCheck"].keeps_reload);
    let rules = std::fs::read_to_string(
        out.with_file_name(format!("{NS2A}-rules"))
            .join("tier.rhai"),
    )
    .unwrap();
    let laid = rules.lines().find(|l| l.starts_with("fn laid()")).unwrap();
    assert!(
        laid.contains(r#""weapon_package_tier2a:image/sniperczoomedimage": #{"archetype": "weapon_package_tier2:archetype/lmgarmor"}"#),
        "{laid}"
    );

    // Tier 1 and Tier 2 beside it, as the game loads them together.
    for (addon, ns, refs) in [
        ("Weapon_Package_Tier1", NS, &[][..]),
        ("Weapon_Package_Tier2", NS2, &["Weapon_Package_Tier1"][..]),
    ] {
        let r = import_beside(&dir.0, addon, ns, refs);
        assert!(r.ports[0].applied, "{ns}: {:?}", r.ports[0].reason);
    }
    let (both, notes) = pack.clone().merge(vec![(
        format!("addons/{NS}"),
        self::pack(&dir.0.join("addons").join(NS)),
    )]);
    assert!(notes.is_empty(), "{notes:?}");
    let hold = |item: &str| {
        let mut w = WeaponsWorld::new(both.clone()).unwrap();
        w.add_actor(A, 5).unwrap();
        let slot = w.give(A, &format!("{NS2A}:weapon/{item}")).unwrap();
        w.equip(A, Some(slot)).unwrap();
        steps(&mut w, 60);
        w
    };
    // One pull of the burst rifle: three rounds out, three off.
    let mut w = hold("bullpupitem");
    assert_eq!(spawned(&shoot(&mut w)), ["bullpupprojectile1"; 3]);
    assert_eq!(w.ammo(A).unwrap().rounds, 4);
    // One pull of the twins: both fire, the right one pays.
    let mut w = hold("dualsmgsitem");
    assert_eq!(spawned(&shoot(&mut w)), ["dualsmgsprojectile1"; 2]);
    assert_eq!(w.ammo(A).unwrap().rounds, 9);

    // In a game with Tier 2's laid body on.
    let mut g = Game::with_add_ons(&dir.0, &out, NS2A, &[NS, NS2, &format!("{NS2}-rules")]);
    let a = g.join("A", Vec3::new(0.0, 0.05, 0.0));
    g.steps(2);
    g.s.set_spawn_points(vec![g.feet(a)]).unwrap();
    let mut loadout: [Option<String>; 5] = Default::default();
    loadout[0] = Some(format!("{NS2A}:weapon/snipercarbineitem"));
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
    g.steps(330);
    g.looks.get_mut(&a).unwrap().pitch = 1.2;
    g.equip(a, "snipercarbineitem");
    let standing = "v20.player.playerstandardarmor";
    let unscoped = format!("{standing}|{NS2A}:image/snipercarbineimage");
    let scoped = format!("{NS2}:archetype/lmgarmor|{NS2A}:image/sniperczoomedimage");
    assert_eq!(g.who(a), unscoped);
    let jet = |g: &mut Game| {
        g.looks.get_mut(&a).unwrap().jet = true;
        g.steps(4);
        g.looks.get_mut(&a).unwrap().jet = false;
        g.steps(30);
    };
    jet(&mut g);
    assert_eq!(g.who(a), scoped);
    jet(&mut g);
    assert_eq!(g.who(a), unscoped);
    // Scoped, a shot, then the light key: the reload goes back unscoped.
    jet(&mut g);
    g.cmd(a, Command::WeaponTrigger { down: true });
    g.steps(2);
    g.cmd(a, Command::WeaponTrigger { down: false });
    g.steps(60);
    assert_eq!(g.mag(a), json!("4|5|tt-556|90"));
    assert_eq!(g.who(a), scoped);
    g.cmd(a, Command::ToggleLight);
    g.steps(10);
    assert_eq!(g.who(a), unscoped);
    g.steps(240);
    assert_eq!(g.mag(a), json!("5|5|tt-556|89"));
    assert_eq!(g.who(a), unscoped);
    // A reload stops as the gun is put away (`SniperCarbineItem::onUse`
    // clearing `TT_forceToolReload`): drawn again, it fires what it has.
    g.cmd(a, Command::WeaponTrigger { down: true });
    g.steps(2);
    g.cmd(a, Command::WeaponTrigger { down: false });
    g.steps(60);
    g.cmd(a, Command::ToggleLight);
    g.steps(10);
    g.cmd(a, Command::EquipTool { slot: None });
    g.steps(20);
    g.equip(a, "snipercarbineitem");
    g.steps(240);
    assert_eq!(g.mag(a), json!("4|5|tt-556|89"), "no rounds moved");
    g.cmd(a, Command::WeaponTrigger { down: true });
    g.steps(2);
    g.cmd(a, Command::WeaponTrigger { down: false });
    g.steps(60);
    assert_eq!(g.mag(a), json!("3|5|tt-556|89"), "it fired");
}

const NSE: &str = "weapon_package_explosive1";

/// Explosive 1 on Tier 1: each grenade is counted from its holder's
/// grenades, a throw taking one; with none left the grenade leaves the
/// hand and comes back as a grenade bag brings more. The conc goes off on
/// its second knock with a knock at each; the firebomb bursts into two or
/// three embers thrown as its script threw them, which sear the players
/// near them with flames and a sizzle only they hear; its raised arm is
/// raised once while it waits to be thrown. The check for the base game's
/// rocket launcher is no gap.
#[test]
fn explosive1_grenades_count_down_and_the_molotov_burns() {
    let (dir, out, report) = imported_on(
        "Weapon_Package_Explosive1",
        NSE,
        &["Weapon_Package_Tier1"],
        "nades",
    );
    let port = &report.ports[0];
    assert!(port.applied, "{:?}", port.reason);
    assert_eq!(
        report.summary.needs_behaviour_ported,
        report.summary.needs_behaviour,
        "{:?}",
        report
            .needs_behaviour
            .iter()
            .filter(|b| b.port.as_ref().is_none_or(|p| !p.applied))
            .map(|b| &b.function)
            .collect::<Vec<_>>()
    );
    let gaps: Vec<_> = report.unsupported.iter().map(|u| &u.what).collect();
    assert!(!gaps.iter().any(|w| w.contains("isFile")), "{gaps:?}");
    let pack = pack(&out);
    let image = |name: &str| pack.images[&format!("{NSE}:image/{name}")].clone();
    let projectile = |name: &str| pack.projectiles[&format!("{NSE}:projectile/{name}")].clone();
    for (name, ammo, reserve) in [
        ("tierfraggrenadeimage", "tt-fragnades", 4),
        ("tierstickgrenadeimage", "tt-sticknades", 3),
        ("tmolotovimage", "tt-molnades", 2),
    ] {
        let mag = image(name).magazine.unwrap();
        assert!(mag.from_reserve, "{name}");
        assert_eq!((mag.ammo.as_str(), mag.reserve), (ammo, reserve));
    }
    let conc = projectile("tierfraggrenadeprojectile");
    assert_eq!(conc.max_bounces, 2);
    assert_eq!(
        pack.explosions[&conc.bounce_effect.to_ascii_lowercase()].sound,
        format!("{NSE}:sound/standinknocksound")
    );
    let embers = &projectile("tmolotovprojectile").children[0];
    assert_eq!((embers.count, embers.max_count), (2, 3));
    assert_eq!(
        embers.steps,
        Some(Steps {
            low: [-2, 0, -1],
            high: [2, 2, 3],
            offset: [-0.25; 3],
            step: [2.0, 2.0, -2.0],
        }),
        "x as the script's x, up as its z, back as its y turned round"
    );
    let aura = projectile("tierfireroastprojectile").aura.unwrap();
    assert_eq!(
        (aura.radius, aura.damage, aura.every_ticks, aura.max_pulses),
        (3.0, 5.0, 30, 4)
    );
    assert!(aura.players_only);
    assert_eq!(aura.effect, "standinSearExplosion");
    assert_eq!(aura.target_sound, format!("{NSE}:sound/standinsizzlesound"));
    let armed = image("tmolotovimage");
    let armed = armed.states.iter().find(|s| s.name == "Armed").unwrap();
    assert!(armed.arm_once && armed.arm == "spearReady");

    // In a game: A and B six apart, each with the three grenades.
    let tier1 = import_beside(&dir.0, "Weapon_Package_Tier1", NS, &[]);
    assert!(tier1.ports[0].applied);
    let mut g = Game::with_add_ons(&dir.0, &out, NSE, &[NS]);
    let a = g.join("A", Vec3::new(0.0, 0.05, 0.0));
    let b = g.join("B", Vec3::new(0.0, 0.05, -6.0));
    g.steps(2);
    g.s.set_spawn_points(vec![g.feet(a)]).unwrap();
    let mut loadout: [Option<String>; 5] = Default::default();
    for (i, item) in [
        "tierfraggrenadeitem",
        "tierstickgrenadeitem",
        "tmolotovitem",
    ]
    .iter()
    .enumerate()
    {
        loadout[i] = Some(format!("{NSE}:weapon/{item}"));
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

    // Four concs, thrown away from B.
    g.looks.get_mut(&a).unwrap().yaw = std::f32::consts::PI;
    g.equip(a, "tierfraggrenadeitem");
    assert_eq!(g.mag(a), json!("4|1|tt-fragnades|4"));
    let throw = |g: &mut Game| {
        g.cmd(a, Command::WeaponTrigger { down: true });
        g.steps(2);
        g.cmd(a, Command::WeaponTrigger { down: false });
        g.steps(150);
    };
    for left in [3, 2, 1] {
        throw(&mut g);
        assert_eq!(g.mag(a), json!(format!("{left}|1|tt-fragnades|{left}")));
    }
    throw(&mut g);
    assert_eq!(g.who(a), "v20.player.playerstandardarmor|", "none left");
    assert_eq!(g.mag(a), json!("0|1|tt-fragnades|0"));
    // A bag: one more conc, back in hand (the firebombs are full).
    g.drop(a, "grenadebagitem");
    assert_eq!(
        g.who(a),
        format!("v20.player.playerstandardarmor|{NSE}:image/tierfraggrenadeimage")
    );
    assert_eq!(g.mag(a), json!("1|1|tt-fragnades|1"));

    // A firebomb at B's feet: its embers sear B, who alone hears it.
    g.looks.get_mut(&a).unwrap().yaw = 0.0;
    g.equip(a, "tmolotovitem");
    let eye = g.feet(a).y + 2.156;
    g.looks.get_mut(&a).unwrap().pitch = ((g.feet(b).y - eye) / 6.0).atan();
    g.s.take_private_notices();
    g.cmd(a, Command::WeaponTrigger { down: true });
    g.steps(2);
    g.cmd(a, Command::WeaponTrigger { down: false });
    g.steps(270);
    assert!(g.health(b) < 100.0, "{}", g.health(b));
    let notices = g.s.take_private_notices();
    let sizzle = format!("{NSE}:sound/standinsizzlesound");
    let heard = |who: OwnerId| {
        notices
            .iter()
            .any(|(o, n)| *o == who && matches!(n, Notice::Sound(p) if *p == sizzle))
    };
    assert!(heard(b), "{notices:?}");
    assert!(!heard(a));
    assert_eq!(g.mag(a), json!("1|1|tt-molnades|1"));
}

/// Tier 1 uses a sound pack when the player has one and otherwise defines
/// its reload click from the base game's file. The check reads as absent,
/// so the pack is no missing dependency, and the click plays the base
/// game's own sound of that file.
#[test]
fn tier1_click_is_the_base_games_when_no_sound_pack_is_there() {
    let dir = Dir(std::env::temp_dir().join(format!("bri-tier-port-{}-click", std::process::id())));
    let _ = std::fs::remove_dir_all(&dir.0);
    // The base game's sound of that file, as recovered core scripts name it.
    let core = dir.0.join("core/sounds.cs");
    std::fs::create_dir_all(core.parent().unwrap()).unwrap();
    std::fs::write(
        &core,
        "datablock AudioProfile(clickMoveSound)\n{\n   filename = \"~/data/sound/clickMove.wav\";\n   description = AudioClosest3d;\n};\n",
    )
    .unwrap();
    let out = dir.0.join("addons").join(NS);
    let report = import(&Options {
        input: Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/ports/Weapon_Package_Tier1"),
        out: out.clone(),
        reference: None,
        core: vec![core],
        installed: None,
        version: "1.0.0".into(),
    })
    .unwrap();
    assert!(report.ports[0].applied, "{:?}", report.ports[0].reason);
    let pack_dep = report
        .dependencies
        .iter()
        .find(|d| d.addon == "Sound_Standin")
        .unwrap();
    assert_eq!(pack_dep.status, "if_present");
    assert_eq!(report.summary.dependencies_missing, 0);
    let click = report
        .datablocks
        .iter()
        .find(|d| d.name == "Block_MoveBrick_Sound")
        .unwrap();
    assert_eq!(click.status, "consumed", "{:?}", click.notes);
    let pack = pack(&out);
    let sidearm = &pack.images[&format!("{NS}:image/standinsidearmimage")];
    let wait = sidearm
        .states
        .iter()
        .find(|s| s.name == "ReloadWait")
        .unwrap();
    assert_eq!(wait.sound, "clickMoveSound");
}

const NSX: &str = "weapon_package_explosive2";

/// Kai's Explosive 2 on the stand-in Tier 1: the RPG a little wild on the
/// move, the flak round shedding sparks as it flies and bursting more at
/// each hit, and the hidden mortar lobbed by how far its holder's look
/// lands.
#[test]
fn explosive2_flak_sheds_sparks_and_the_mortar_lobs() {
    let (_dir, out, report) = imported_on(
        "Weapon_Package_Explosive2",
        NSX,
        &["Weapon_Package_Tier1"],
        "launchers",
    );
    let port = &report.ports[0];
    assert!(port.applied, "{:?}", port.reason);
    assert_eq!(
        report.summary.needs_behaviour_ported,
        report.summary.needs_behaviour,
        "{:?}",
        report
            .needs_behaviour
            .iter()
            .filter(|b| b.port.as_ref().is_none_or(|p| !p.applied))
            .map(|b| &b.function)
            .collect::<Vec<_>>()
    );
    assert!(report.unsupported.is_empty(), "{:?}", report.unsupported);
    assert_eq!(report.summary.dependencies_missing, 0);
    let pack = pack(&out);
    let projectile = |name: &str| pack.projectiles[&format!("{NSX}:projectile/{name}")].clone();
    let flak = projectile("flakcannonprojectile");
    let spark = format!("{NSX}:projectile/flakcannonsparkprojectile");
    let [flying, hits] = &flak.children[..] else {
        panic!("{:?}", flak.children)
    };
    assert_eq!(
        (
            flying.projectile.as_str(),
            flying.count,
            flying.speed,
            flying.angles,
            flying.every_ticks,
            flying.max_times
        ),
        (spark.as_str(), 3, 15.0, true, 30, 150),
        "every PrjLoop_tickTime, PrjLoop_maxTicks times at most"
    );
    assert_eq!(
        (
            hits.projectile.as_str(),
            hits.count,
            hits.max_count,
            hits.redraw,
            hits.speed,
            hits.angles,
            hits.on_hit,
            hits.on_bounce || hits.on_explode
        ),
        (spark.as_str(), 2, 4, true, 150.0, true, true, false)
    );
    let shot = |name: &str| {
        pack.images[&format!("{NSX}:image/{name}")]
            .shot
            .clone()
            .unwrap()
    };
    let rpg = shot("rpgimage");
    assert_eq!(
        (rpg.spread, rpg.moving_spread, rpg.moving_speed),
        (0.0, Some(0.0002), 0.1),
        "true standing, wild on the move"
    );
    assert_eq!(
        shot("mortarimage").lob,
        Some(Lob {
            speed: 15.0,
            range: 200.0,
            otherwise: 80.0,
            distance_divisor: 4.0,
            jitter_steps: [2, 2],
            jitter_divisor: [4.0, 4.0],
        })
    );
    assert!(pack.items[&format!("{NSX}:weapon/mortaritem")].hidden);
    assert!(!pack.items[&format!("{NSX}:weapon/rpgitem")].hidden);
}

const NSM: &str = "weapon_package_medic1";

/// The medical pack in a hosted minigame: the dart heals a hurt teammate's
/// burst at once and leaves its heal image on them, which heals a little
/// each pass and comes off after its count, while a hit ends it early; the
/// booster heals its holder, then refuses until it recharges and says so
/// when it does; jet throws it at someone close. Outside minigames, an
/// internet host's medic heals nobody; a LAN host's heals anyone in none.
#[test]
fn medic1_heals_over_time_and_the_syringe_recharges() {
    let (dir, out, report) = imported_on("Weapon_Package_Medic1", NSM, &[], "medic");
    let port = &report.ports[0];
    assert!(port.applied, "{:?}", port.reason);
    assert_eq!(
        report.summary.needs_behaviour_ported,
        report.summary.needs_behaviour,
        "{:?}",
        report
            .needs_behaviour
            .iter()
            .filter(|b| b.port.as_ref().is_none_or(|p| !p.applied))
            .map(|b| &b.function)
            .collect::<Vec<_>>()
    );
    assert!(report.unsupported.is_empty(), "{:?}", report.unsupported);
    assert_eq!(report.summary.dependencies_missing, 0);
    let pack = pack(&out);
    let image = |name: &str| pack.images[&format!("{NSM}:image/{name}")].clone();
    let rules = format!("{NSM}-rules");
    assert_eq!(
        image("medigunhealimage").commands.for_script("onHeal"),
        Some(&format!("{rules}:heal_tick"))
    );
    let fire = image("medigunimage").states[2].clone();
    assert_eq!(
        (fire.arm.as_str(), fire.sound.as_str()),
        ("shiftAway", &*format!("{NSM}:sound/medigunshot1sound"))
    );
    let mut g = Game::with_add_ons(&dir.0, &out, NSM, &[]);
    let a = g.join("A", Vec3::new(0.0, 0.05, 0.0));
    let b = g.join("B", Vec3::new(0.0, 0.05, -6.0));
    g.steps(2);
    let mut loadout: [Option<String>; 5] = Default::default();
    loadout[0] = Some(format!("{NSM}:weapon/medigunitem"));
    loadout[1] = Some(format!("{NSM}:weapon/stimpackitem"));
    // Outside minigames on an internet host the dart heals nobody.
    g.probe(
        a,
        "drop",
        vec![PackageArg::String(loadout[0].clone().unwrap())],
    );
    g.steps(330);
    g.probe(b, "hurt", vec![PackageArg::Float(50.0)]);
    g.steps(2);
    g.equip(a, "medigunitem");
    g.shoot_at(a, b, 1.0);
    g.steps(120);
    assert_eq!(g.health(b), 50.0, "not on an internet host");
    // On a LAN host it heals anyone in no minigame.
    g.s.set_lan_host(true);
    g.shoot_at(a, b, 1.0);
    assert_eq!(
        g.ask(b, "worn"),
        json!(format!("{NSM}:image/medigunhealimage"))
    );
    g.steps(240);
    assert_eq!(g.health(b), 80.0, "12 at once, then 3 six times");
    assert_eq!(
        g.ask(b, "worn"),
        json!(""),
        "the image comes off after its count"
    );
    g.s.set_lan_host(false);
    // In a minigame, players in it.
    g.s.set_spawn_points(vec![g.feet(a)]).unwrap();
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
    g.probe(b, "hurt", vec![PackageArg::Float(60.0)]);
    g.steps(2);
    g.equip(a, "medigunitem");
    g.s.take_cues();
    g.s.take_private_notices();
    g.shoot_at(a, b, 1.0);
    let cues = g.s.take_cues();
    let heal_image = format!("{NSM}:image/medigunhealimage");
    assert!(
        cues.iter().any(|c| matches!(&c.kind,
            bri_sim::presentation::CueKind::Emote { actor, name } if *actor == b && *name == heal_image)),
        "every client sees the heal image on B"
    );
    let printed: Vec<_> =
        g.s.take_private_notices()
            .into_iter()
            .filter_map(|(o, n)| match n {
                Notice::Bottom {
                    text,
                    seconds,
                    hide_bar,
                } => Some((o, text, seconds, hide_bar)),
                _ => None,
            })
            .collect();
    assert!(
        printed.contains(&(a, "\\c2B is patched up.".into(), 2.0, true)),
        "{printed:?}"
    );
    assert!(
        printed.contains(&(b, "\\c2A patched you up.".into(), 2.0, true)),
        "{printed:?}"
    );
    let healed = g.health(b);
    assert!((52.0..80.0).contains(&healed), "{healed}");
    // A hit replaces the heal image, so the heal over time stops.
    g.probe(b, "hurt", vec![PackageArg::Float(1.0)]);
    g.steps(2);
    assert_ne!(g.ask(b, "worn"), json!(heal_image));
    let after = g.health(b);
    g.steps(240);
    assert_eq!(g.health(b), after, "a hit ends the heal over time");
    // The booster: A, hurt by 40, heals 25 at once with the cross and the
    // heal image, then 18 over time.
    g.probe(a, "hurt", vec![PackageArg::Float(40.0)]);
    g.steps(2);
    g.equip(a, "stimpackitem");
    g.s.take_private_notices();
    let use_booster = |g: &mut Game| {
        g.cmd(a, Command::WeaponTrigger { down: true });
        g.steps(2);
        g.cmd(a, Command::WeaponTrigger { down: false });
        g.steps(70);
    };
    use_booster(&mut g);
    assert_eq!(g.ask(a, "worn"), json!(heal_image));
    g.steps(240);
    assert_eq!(
        g.health(a),
        100.0,
        "60, 25 at once, then 15 of the 18 over time"
    );
    // Again at once: it is still charging and says so.
    use_booster(&mut g);
    let centre = |g: &mut Game| -> Vec<String> {
        g.s.take_private_notices()
            .into_iter()
            .filter_map(|(o, n)| match n {
                Notice::Center { text, .. } if o == a => Some(text),
                _ => None,
            })
            .collect()
    };
    assert!(centre(&mut g).contains(&"\\c0Booster still charging.".into()));
    // Its notice comes 4 seconds after use, and then it heals again.
    g.probe(a, "hurt", vec![PackageArg::Float(20.0)]);
    g.steps(4 * 120);
    assert!(centre(&mut g).contains(&"\\c2Booster ready.".into()));
    use_booster(&mut g);
    assert_eq!(g.health(a), 100.0, "80 and 25, up to the most");
    assert_eq!(
        g.ask(a, "worn"),
        json!(""),
        "hurt by less than 25: no heal image"
    );
    // Jet throws it: B, hurt, is healed 32 by it from close by.
    let to_40 = g.health(b) - 40.0;
    g.probe(b, "hurt", vec![PackageArg::Float(to_40.into())]);
    g.steps(2);
    let before = g.health(b);
    g.looks.get_mut(&a).unwrap().jet = true;
    g.steps(2);
    g.looks.get_mut(&a).unwrap().jet = false;
    g.steps(30);
    assert_eq!(
        g.health(b),
        before + 32.0 + 3.0,
        "the thrown booster's 32 at once, then the first pass of its heal over time"
    );
    assert_eq!(g.ask(b, "worn"), json!(heal_image));
}

/// TT_canHeal's team check: in a minigame with teams a medic heals only
/// its own team, until the host turns Teams Can Heal Enemies on in the
/// server settings; the two preferences are settings, not gaps.
#[test]
fn medic1_heals_its_own_team_unless_the_host_lets_it_heal_others() {
    use bri_package::setting::SettingValue as V;
    let (dir, out, report) = imported_on("Weapon_Package_Medic1", NSM, &[], "medic-teams");
    assert!(report.ports[0].applied, "{:?}", report.ports[0].reason);
    assert!(report.unsupported.is_empty(), "{:?}", report.unsupported);
    assert!(
        report
            .ambiguous
            .iter()
            .any(|f| f.what == "isFunction(registerPreferenceAddon)" && f.detail.contains("absent")),
        "{:?}",
        report.ambiguous
    );
    let mut g = Game::with_add_ons(&dir.0, &out, NSM, &[]);
    let a = g.join_host("A", Vec3::new(0.0, 0.05, 0.0));
    let b = g.join("B", Vec3::new(0.0, 0.05, -6.0));
    g.steps(2);
    let mut loadout: [Option<String>; 5] = Default::default();
    loadout[0] = Some(format!("{NSM}:weapon/medigunitem"));
    g.s.set_spawn_points(vec![g.feet(a)]).unwrap();
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
    g.probe(a, "teams", vec![]);
    let on = |g: &mut Game, p: OwnerId, team: &str| {
        g.probe(p, "team", vec![PackageArg::String(team.into())]);
        g.steps(2);
    };
    on(&mut g, a, "Red");
    on(&mut g, b, "Blue");
    g.probe(b, "hurt", vec![PackageArg::Float(50.0)]);
    g.steps(2);
    g.equip(a, "medigunitem");
    g.shoot_at(a, b, 1.0);
    g.steps(120);
    assert_eq!(g.health(b), 50.0, "B is on the other team");
    on(&mut g, b, "Red");
    g.shoot_at(a, b, 1.0);
    g.steps(120);
    assert!(g.health(b) > 50.0, "B is A's teammate now: {}", g.health(b));
    on(&mut g, b, "Blue");
    let hurt = g.health(b) - 50.0;
    g.probe(b, "hurt", vec![PackageArg::Float(hurt.into())]);
    g.steps(2);
    g.configure(
        a,
        &[(&format!("{NSM}-rules:tt_medichealenemy"), V::Bool(true))],
    )
    .unwrap();
    g.shoot_at(a, b, 1.0);
    g.steps(120);
    assert!(
        g.health(b) > 50.0,
        "Teams Can Heal Enemies: {}",
        g.health(b)
    );
}

const NSME: &str = "weapon_melee_extended";

/// Melee Extended on its stand-in: each swing draws one of its pair of hit
/// sounds, the states play the arm and other-arm moves their scripts did,
/// and the knife stabs on a quick click (its stab's own damage) and
/// slashes once charged (Kai's damage held to the raycast script's 100).
#[test]
fn melee_extended_swings_draw_their_hit_sounds_and_the_knife_stabs() {
    let (dir, out, report) = imported_on("Weapon_Melee_Extended", NSME, &[], "melee");
    let port = &report.ports[0];
    assert!(port.applied, "{:?}", port.reason);
    assert_eq!(
        report.summary.needs_behaviour_ported,
        report.summary.needs_behaviour,
        "{:?}",
        report
            .needs_behaviour
            .iter()
            .filter(|b| b.port.as_ref().is_none_or(|p| !p.applied))
            .map(|b| &b.function)
            .collect::<Vec<_>>()
    );
    assert!(report.unsupported.is_empty(), "{:?}", report.unsupported);
    assert_eq!(report.summary.dependencies_missing, 0);
    let pack = pack(&out);
    let image = |name: &str| pack.images[&format!("{NSME}:image/{name}")].clone();
    let sound = |name: &str| Some(format!("{NSME}:sound/{name}"));
    let knife = image("combatknifeimage");
    let slash = knife.shot.as_ref().unwrap().hitscan.clone().unwrap();
    let stab = knife.state_shots["onstabfire"].hitscan.clone().unwrap();
    assert_eq!((slash.damage, stab.damage), (Some(100.0), Some(40.0)));
    assert_eq!(
        slash.sounds,
        [
            HitSounds {
                player: None,
                other: sound("standinclinksound")
            },
            HitSounds {
                player: None,
                other: sound("standinslicesound")
            }
        ]
    );
    assert_eq!(stab.sounds, slash.sounds);
    let state = |image: &Image, name: &str| {
        let s = image.states.iter().find(|s| s.name == name).unwrap();
        (s.arm.clone(), s.gesture.clone())
    };
    assert_eq!(
        state(&knife, "Slash"),
        ("shiftTo".into(), "spearThrow".into())
    );
    assert_eq!(
        state(&knife, "Stab"),
        ("shiftTo".into(), "shiftDown".into())
    );
    assert_eq!(state(&knife, "Charge"), ("plant".into(), String::new()));
    let katana = image("l4bkatanaimage");
    assert_eq!(
        state(&katana, "FireB"),
        ("shiftTo".into(), "shiftLeft".into())
    );
    let machete = image("l4bmacheteimage");
    assert_eq!(state(&machete, "FireA").0, "shiftAway");
    assert_eq!(state(&machete, "Activate").0, "shiftDown");
    let banjo = image("l4bguitarimage").shot.unwrap().hitscan.unwrap();
    assert_eq!(
        banjo.sounds[1],
        HitSounds {
            player: sound("standinthudsound"),
            other: sound("standinslicesound")
        }
    );

    // In a minigame, B two units ahead of A.
    let mut g = Game::with_add_ons(&dir.0, &out, NSME, &[]);
    let a = g.join("A", Vec3::new(0.0, 0.05, 0.0));
    let b = g.join("B", Vec3::new(0.0, 0.05, -2.0));
    g.steps(2);
    let mut loadout: [Option<String>; 5] = Default::default();
    loadout[0] = Some(format!("{NSME}:weapon/combatknifeitem"));
    g.s.set_spawn_points(vec![g.feet(a)]).unwrap();
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
    g.equip(a, "combatknifeitem");
    // Drawn, it readies a quarter of a second later.
    g.steps(20);
    let swing = |g: &mut Game, hold: usize| {
        let eye = g.feet(a).y + 2.156;
        let pitch = ((g.feet(b).y + 1.0 - eye) / (g.feet(a).z - g.feet(b).z)).atan();
        g.looks.get_mut(&a).unwrap().pitch = pitch;
        g.steps(4);
        g.cmd(a, Command::WeaponTrigger { down: true });
        g.steps(hold);
        g.cmd(a, Command::WeaponTrigger { down: false });
        g.steps(60);
    };
    // A quick click stabs.
    swing(&mut g, 2);
    assert_eq!(g.health(b), 60.0, "the stab's 40");
    // Held past the charge it slashes for 100, which kills.
    swing(&mut g, 90);
    assert!(!g.s.vitals()[&b].alive, "the slash's 100");
}

const NSME2: &str = "weapon_melee_extended_ii";

/// Melee Extended II on its stand-in, beside Melee Extended's: the riot
/// shield, raised, keeps a share of a bash from in front (its own share,
/// then the share of all hurt from in front) and sends the bash back as
/// its holder's, whose kill reads as sent back; the saw cuts while held
/// and plants its arm as it is drawn; the easter eggs are hidden.
#[test]
fn melee_extended_ii_shield_stops_shots_from_in_front_and_sends_them_back() {
    let (dir, out, report) = imported_on(
        "Weapon_Melee_Extended_II",
        NSME2,
        &["Weapon_Melee_Extended"],
        "shield",
    );
    let port = &report.ports[0];
    assert!(port.applied, "{:?}", port.reason);
    assert_eq!(
        report.summary.needs_behaviour_ported,
        report.summary.needs_behaviour
    );
    assert!(report.unsupported.is_empty(), "{:?}", report.unsupported);
    let pack = pack(&out);
    let image = |name: &str| pack.images[&format!("{NSME2}:image/{name}")].clone();
    let shield = image("shieldriotttimage");
    let guard = shield.guard.clone().unwrap();
    assert_eq!(
        (guard.projectile_damage, guard.damage, guard.push),
        (0.1, 0.25, 0.5)
    );
    assert_eq!(
        guard.front,
        Some(GuardFront {
            up: 0.7,
            above: 3.0,
            down: 0.8,
            below: 4.0
        })
    );
    assert_eq!(guard.states, ["Ready"]);
    assert!(shield.both_arms);
    let saw = image("l4bchainsawimage");
    assert_eq!(saw.states[0].cues[0].thread, Some(0));
    assert_eq!(saw.states[0].cues[0].sequence, "plant");
    assert!(saw.shot.unwrap().hitscan.is_some());
    for hidden in ["l4bbigstickitem", "l4bpipewrenchitem", "l4bgaffitem"] {
        assert!(pack.items[&format!("{NSME2}:weapon/{hidden}")].hidden);
    }
    assert!(!pack.items[&format!("{NSME2}:weapon/l4baxeitem")].hidden);
    assert!(pack.damage_types["reflected"].special);

    // In a minigame, B two units ahead of A, facing A, both holding the
    // shield up.
    let melee = import_beside(&dir.0, "Weapon_Melee_Extended", NSME, &[]);
    assert!(melee.ports[0].applied, "{:?}", melee.ports[0].reason);
    let mut g = Game::with_add_ons(&dir.0, &out, NSME2, &[NSME]);
    let a = g.join("A", Vec3::new(0.0, 0.05, 0.0));
    let b = g.join("B", Vec3::new(0.0, 0.05, -2.0));
    g.steps(2);
    let mut loadout: [Option<String>; 5] = Default::default();
    loadout[0] = Some(format!("{NSME2}:weapon/riotttshielditem"));
    g.s.set_spawn_points(vec![g.feet(a)]).unwrap();
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
    g.looks.get_mut(&b).unwrap().yaw = std::f32::consts::PI;
    g.equip(a, "riotttshielditem");
    g.equip(b, "riotttshielditem");
    g.steps(30);
    g.s.take_private_notices();
    // A bashes: B keeps 60 x 0.1 x 0.25 of it, and the bash flies back
    // into A, whose shield is still down from the swing.
    g.shoot_at(a, b, 1.2);
    assert!((g.health(b) - 98.5).abs() < 0.01, "{}", g.health(b));
    assert_eq!(g.health(a), 40.0);
    // Again, and A falls to the bash B sent back.
    g.shoot_at(a, b, 1.2);
    assert!((g.health(b) - 97.0).abs() < 0.01, "{}", g.health(b));
    assert!(!g.s.vitals()[&a].alive);
    let said: Vec<String> =
        g.s.take_private_notices()
            .into_iter()
            .filter_map(|(_, n)| match n {
                Notice::Chat(text) => Some(text),
                _ => None,
            })
            .collect();
    assert!(
        said.iter().any(|t| t == "B [sent back] [bash] A"),
        "{said:?}"
    );
}

/// Kai's twelve Skins packs on our stand-ins: each skin is its host gun
/// (Tier 1, 2 or 2A, or the Pistol skins) under its own name and numbers,
/// and its scripts are copies of the host's, so the shared Tier+Tactical
/// rules port every function. The Dualies hold twice their pistol's
/// magazine and hit as hard, as Kai's `classicPistolItem.TT_maxAmmo*2` and
/// `classicPistolImage.TT_raycastDirectDamage` fields read; the Light MG
/// skins lay and lift their gunner's slow body; the Bolt Rifle shifts its
/// arm as it is drawn.
#[test]
fn skins_are_their_hosts_guns_under_their_own_names() {
    let t1 = "Weapon_Package_Tier1";
    let t2 = "Weapon_Package_Tier2";
    let t2a = "Weapon_Package_Tier2A";
    let packs: [(&str, &[&str]); 12] = [
        ("Weapon_Skins_Pistol", &[t1]),
        ("Weapon_Skins_Dualies", &[t1, "Weapon_Skins_Pistol"]),
        ("Weapon_Skins_Rifles", &[t1]),
        ("Weapon_Skins_SMG", &[t1]),
        ("Weapon_Skins_Shotgun", &[t1]),
        ("Weapon_Skins_LMG", &[t1, t2]),
        ("Weapon_Skins_Magnum", &[t1, t2]),
        ("Weapon_Skins_RiflesT2", &[t1, t2]),
        ("Weapon_Skins_ShotgunT2", &[t1, t2]),
        ("Weapon_Skins_Sniper", &[t1, t2]),
        ("Weapon_Skins_Bullpup", &[t1, t2a]),
        ("Weapon_Skins_MPistol", &[t1, t2a]),
    ];
    for (addon, refs) in packs {
        let ns = addon.to_ascii_lowercase();
        let (_dir, out, report) = imported_on(addon, &ns, refs, "skins");
        let port = &report.ports[0];
        assert!(port.applied, "{addon}: {:?}", port.reason);
        assert_eq!(port.port, ns);
        assert!(
            report.needs_behaviour.iter().all(|b| b.port.is_some()),
            "{addon}: {:?}",
            report
                .needs_behaviour
                .iter()
                .filter(|b| b.port.is_none())
                .map(|b| &b.function)
                .collect::<Vec<_>>()
        );
        assert!(
            report.unsupported.is_empty(),
            "{addon}: {:?}",
            report.unsupported
        );
        let pack = pack(&out);
        assert!(!pack.items.is_empty(), "{addon}");
        for (id, item) in &pack.items {
            assert!(item.ui_name.starts_with("Stand-in "), "{id}");
            // Every skin keeps its host's ammo system.
            let image = &pack.images[&item.image];
            assert!(image.magazine.is_some(), "{id}");
        }
        match addon {
            "Weapon_Skins_Dualies" => {
                let image = &pack.images[&format!("{ns}:image/akimboclassicpistolimage")];
                assert_eq!(image.magazine.as_ref().unwrap().size, 12);
                assert_eq!(
                    image.left_image.as_deref(),
                    Some(format!("{ns}:image/lefthandedclassicpistolimage").as_str())
                );
                let ray = image.projectile.as_ref().unwrap();
                assert_eq!(pack.projectiles[ray].damage, 12.0);
            }
            "Weapon_Skins_LMG" => {
                let image = &pack.images[&format!("{ns}:image/classiclmgimage")];
                let commands = &image.commands;
                assert_eq!(commands.states["onclick"], format!("{ns}-rules:lay"));
                assert_eq!(
                    commands.unmount.as_deref(),
                    Some(format!("{ns}-rules:lift").as_str())
                );
            }
            "Weapon_Skins_Rifles" => {
                let image = &pack.images[&format!("{ns}:image/boltrifleimage")];
                let cue = &image.states[0].cues[0];
                assert_eq!((cue.thread, cue.sequence.as_str()), (Some(2), "shiftLeft"));
            }
            _ => {}
        }
    }
}

/// Kai's Impact Rifle on Tier 1: its spread holds while its holder stands
/// still, as its check reads (still, and a pause since a last shot only
/// Tier 2's guns note), and changes on the move.
#[test]
fn the_impact_rifle_spreads_by_whether_its_holder_stands_still() {
    let (_dir, out, report) = imported_on(
        "Weapon_Impact_Rifle",
        "weapon_impact_rifle",
        &["Weapon_Package_Tier1"],
        "impact",
    );
    let port = &report.ports[0];
    assert!(port.applied, "{:?}", port.reason);
    assert!(report.needs_behaviour.iter().all(|b| b.port.is_some()));
    let pack = pack(&out);
    let image = &pack.images["weapon_impact_rifle:image/impactrifleimage"];
    let shot = image.shot.as_ref().unwrap();
    assert_eq!(
        (shot.spread, shot.moving_spread, shot.moving_speed),
        (0.0007, Some(0.0003), 0.5)
    );
    assert_eq!(image.magazine.as_ref().unwrap().size, 3);
}

const NSSR: &str = "weapon_shortriflekai";

/// Kai's Short Rifle on Tier 1, with the Critical Hit Emote and the
/// Adventurer's Weapons' stand-in on beside it. Its round turns off what it
/// meets as many times as the gun's field says, 15 more for each landing
/// before (the stand-in's numbers, read from its onRaycastDamage), and
/// shoves whoever it hits. A hit straight on is no crit; one off the
/// floor into the head is, under the gun's own crit kill message, though
/// the Adventurer's Weapons' stand-in declares a crit type of the same name
/// with its own message, which its own crit kills still show.
#[test]
fn the_short_rifle_ricochets_and_crits_only_after_a_turn() {
    let (dir, out, report) = imported_on(
        "Weapon_ShortRifleKai",
        NSSR,
        &["Weapon_Package_Tier1"],
        "ricochets",
    );
    let port = &report.ports[0];
    assert!(port.applied, "{:?}", port.reason);
    assert!(report.unsupported.is_empty(), "{:?}", report.unsupported);
    assert!(report.needs_behaviour.iter().all(|b| b.port.is_some()));
    let image = pack(&out).images[&format!("{NSSR}:image/shortrifleimage")].clone();
    let hitscan = image.shot.unwrap().hitscan.unwrap();
    assert_eq!(
        hitscan.ricochet,
        Some(Ricochet {
            times: 2,
            damage: 15.0,
            shooter: 0.25
        })
    );
    assert!(hitscan.flown.is_empty(), "the line is the engine's streak");
    let rules = std::fs::read_to_string(
        out.with_file_name(format!("{NSSR}-rules"))
            .join("tier.rhai"),
    )
    .unwrap();
    let shots = rules
        .lines()
        .find(|l| l.starts_with("fn ricochet_shots()"))
        .unwrap();
    assert!(
        shots.contains(r#""weapon_shortriflekai:image/shortrifleimage": #{"after_turn": true, "ahead": 8, "below": 4, "head": 2, "type": "$DamageType::StandinCrit", "up": 3}"#),
        "{shots}"
    );

    import_beside(&dir.0, "Weapon_Package_Tier1", NS, &[]);
    import_beside(&dir.0, "Emote_Critical", "emote_critical", &[]);
    const MWB: &str = "weapon_modernwarbattles";
    import_beside(&dir.0, "Weapon_ModernWarbattles", MWB, &[]);
    let mut g = Game::with_add_ons(
        &dir.0,
        &out,
        NSSR,
        &[NS, "emote_critical", MWB, "weapon_modernwarbattles-rules"],
    );
    let a = g.join("A", Vec3::new(0.0, 0.05, 0.0));
    let b = g.join("B", Vec3::new(0.0, 0.05, -6.0));
    g.steps(2);
    g.s.set_spawn_points(vec![g.feet(a)]).unwrap();
    let rifle = format!("{NSSR}:weapon/shortrifleitem");
    let revolver = format!("{MWB}:weapon/revolveritem");
    let mut loadout: [Option<String>; 5] = Default::default();
    loadout[0] = Some(rifle.clone());
    loadout[1] = Some(revolver.clone());
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
    g.equip(a, "shortrifleitem");

    // Straight on: its damage and a shove away, no crit.
    let before = g.feet(b);
    g.s.take_cues();
    g.shoot_at(a, b, 1.2);
    assert!(
        (g.health(b) - 84.0).abs() < 0.5,
        "{} {:?}",
        g.health(b),
        g.s.package_diagnostics()
    );
    assert!(g.feet(b).z < before.z - 0.05, "{before} {}", g.feet(b));
    let crit = |cues: &[bri_sim::presentation::Cue]| {
        cues.iter().any(|c| {
            matches!(&c.kind,
            bri_sim::presentation::CueKind::WeaponEffect { definition, .. }
                if definition.eq_ignore_ascii_case("critexplosion"))
        })
    };
    assert!(!crit(&g.s.take_cues()));
    g.steps(120);

    // Off the floor halfway and up into the head: (16 + 15) * 2.
    let health = g.health(b);
    g.s.take_private_notices();
    bank_shot(&mut g, a, b);
    let cues = g.s.take_cues();
    assert!(crit(&cues), "{cues:?}");
    assert!(
        cues.iter()
            .any(|c| matches!(c.kind, bri_sim::presentation::CueKind::Beam { .. })),
        "the turn's streak: {cues:?}"
    );
    assert!(
        (health - g.health(b) - 62.0).abs() < 0.5,
        "{health} {}",
        g.health(b)
    );
    let said = |g: &mut Game, what: &str| {
        g.s.take_private_notices()
            .iter()
            .any(|(_, n)| matches!(n, Notice::Chat(t) if t.contains(what)))
    };
    // Another kills B under the gun's own crit kill message.
    g.steps(60);
    bank_shot(&mut g, a, b);
    assert_eq!(g.health(b), 0.0);
    assert!(said(&mut g, "A banked one off into B"));

    // The Adventurer's revolver crits with its own StandinCrit message.
    g.steps(300);
    g.cmd(b, Command::Respawn);
    g.steps(330);
    assert_eq!(g.health(b), 100.0);
    g.equip(a, &revolver);
    for _ in 0..3 {
        g.shoot_at(a, b, 1.2);
    }
    assert_eq!(g.health(b), 0.0);
    assert!(said(&mut g, "A hit B hard"));
}

/// A fires at the floor a little short of halfway to B, so the round turns
/// up into the top of B's body, the head.
fn bank_shot(g: &mut Game, a: OwnerId, b: OwnerId) {
    let eye = g.feet(a).y + 2.156;
    let run = (g.feet(a).z - g.feet(b).z) * 0.42;
    g.looks.get_mut(&a).unwrap().pitch = -(eye / run).atan();
    g.steps(4);
    g.cmd(a, Command::WeaponTrigger { down: true });
    g.steps(2);
    g.cmd(a, Command::WeaponTrigger { down: false });
    g.steps(60);
}
