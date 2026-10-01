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
    definitions::Definitions,
    player::{MoveInput, PlayerState},
    prediction::{CollisionMirror, Predictor},
    session::{ActionAim, Command, PackageCommand, Session},
};
use bri_world::OwnerId;
use glam::Vec3;
use rapier3d::prelude::*;
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::Arc,
};

fn fresh(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bri-grapples-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// How an Add-On reaches the game: a player's Import Add-On, or the
/// release's bundle of originals (`tools/addon_bundle.py build`, then
/// `install`, as the packagers lay it out).
#[derive(Clone, Copy)]
enum Via {
    Import,
    Bundle,
}

/// Where the copies come from: our CC0 stand-ins, or, for the bundled
/// tests, the folder `BRI_ADDON_SEARCH` names (Maxwell's real copies, on
/// the PC).
fn copies(via: Via) -> PathBuf {
    match (via, std::env::var_os("BRI_ADDON_SEARCH")) {
        (Via::Bundle, Some(dir)) => PathBuf::from(dir),
        _ => Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ports"),
    }
}

/// The copy of `addon` in `dir`: a folder or a zip.
fn copy_of(dir: &Path, addon: &str) -> PathBuf {
    let zip = dir.join(format!("{addon}.zip"));
    if zip.is_file() { zip } else { dir.join(addon) }
}

/// Puts `addon` into the content root `root` the way `via` does, turns it
/// on as the Add-Ons screen does (with what it turns on with it), and loads
/// what is on as a host does.
fn imported(
    root: &Path,
    addon: &str,
    namespace: &str,
    via: Via,
) -> (bri_weapons::Pack, Arc<Catalog>) {
    // The made-up base game, whose packages (`v20-weapons`...) a real
    // copy that borrows the stock weapons depends on.
    bri_net::testing::write_root(root, &[]).unwrap();
    match via {
        Via::Import => {
            let report = import(&Options {
                input: copy_of(&copies(via), addon),
                out: root.join(Library::scan(root).unwrap().import_dir(addon)),
                ..Default::default()
            })
            .unwrap();
            let port = &report.ports[0];
            assert!(port.applied, "{:?}", port.reason);
            assert!(
                report.needs_behaviour.iter().all(|b| b.port.is_some()),
                "every function is ported: {:?}",
                report.needs_behaviour
            );
            // Ported, nothing is left a gap: each image state script runs
            // the port or the engine's own.
            for d in &report.datablocks {
                assert_eq!(d.status, "converted", "{} {:?}", d.name, d.notes);
            }
            assert_eq!(
                report.summary.verdict, "converted",
                "{:?}",
                report.unsupported
            );
        }
        Via::Bundle => bundled(root, addon, namespace),
    }
    let mut library = Library::scan(root).unwrap();
    let plan = library.plan(namespace, true);
    library.apply(&plan).unwrap();
    let set = PackageSet::load_root(root).unwrap();
    assert!(
        set.packages.iter().any(|p| p.id == format!("{namespace}-rules")),
        "turning {namespace} on turns its rules on: {:?}",
        set.packages.iter().map(|p| &p.id).collect::<Vec<_>>()
    );
    let catalog = Catalog::load(root, &set, true).unwrap_or_else(|e| panic!("{e:#?}"));
    let pack = bri_weapons::Pack::from_json(
        &std::fs::read(root.join("addons").join(namespace).join("assets/weapons.json")).unwrap(),
    )
    .unwrap();
    (pack, Arc::new(catalog))
}

/// The release's path for an original: a stand-in checkout whose default
/// list pins this copy, `addon_bundle.py build` from it, and `install`
/// into `root`, where a release's content holds its Add-Ons.
fn bundled(root: &Path, addon: &str, namespace: &str) {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let checkout = root.with_extension("checkout");
    let _ = std::fs::remove_dir_all(&checkout);
    let search = copies(Via::Bundle);
    // The hash Import Add-On records for this copy, pinned in the list.
    let sha = import(&Options {
        input: copy_of(&search, addon),
        out: checkout.join("sha").join(namespace),
        ..Default::default()
    })
    .unwrap()
    .source
    .sha256;
    let list = serde_json::json!({ "schema_version": 2, "addons": [{
        "id": namespace, "enabled": false,
        "original": { "addon": addon, "title": addon, "authors": ["Tester"],
                      "version": "1.0.0", "sha256": [sha] } }] });
    copy_port_entries(&repo, &checkout);
    for (to, from) in [
        ("crates/package/base-packages.json", Some("crates/package/base-packages.json")),
        ("packages/default-addons.json", None),
        ("core.cs", None),
        ("game/packages.json", None),
    ] {
        let to = checkout.join(to);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        match (from, to.file_name().unwrap().to_str().unwrap()) {
            (Some(from), _) => {
                std::fs::copy(repo.join(from), &to).unwrap();
            }
            (None, "default-addons.json") => {
                std::fs::write(&to, serde_json::to_vec_pretty(&list).unwrap()).unwrap()
            }
            (None, "core.cs") => std::fs::write(&to, "").unwrap(),
            (None, _) => {
                std::fs::write(&to, r#"{ "schema_version": 1, "packages": [] }"#).unwrap()
            }
        }
    }
    std::fs::create_dir_all(checkout.join("v20/base")).unwrap();
    std::fs::create_dir_all(checkout.join("v20/Add-Ons")).unwrap();
    let python = ["python3", "python"]
        .into_iter()
        .find(|p| {
            std::process::Command::new(p)
                .arg("--version")
                .output()
                .is_ok_and(|o| o.status.success())
        })
        .expect("Python 3 runs tools/addon_bundle.py");
    let run = |args: &[&str]| {
        let out = std::process::Command::new(python)
            .arg(repo.join("tools/addon_bundle.py"))
            .args(args)
            .arg("--repo")
            .arg(&checkout)
            .env_remove("BRI_ADDON_SEARCH")
            .env_remove("BRI_V20")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "addon_bundle.py {args:?}: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    };
    let path = |p: &Path| p.to_str().unwrap().to_owned();
    run(&[
        "build",
        "--search",
        &path(&search),
        "--v20",
        &path(&checkout.join("v20")),
        "--core",
        &path(&checkout.join("core.cs")),
        "--importer",
        env!("CARGO_BIN_EXE_bri-import-addon"),
        "--out",
        &path(&checkout.join("bundle")),
        "--content-root",
        &path(&checkout.join("game")),
    ]);
    run(&[
        "install",
        "--bundle",
        &path(&checkout.join("bundle")),
        "--content-root",
        &path(root),
    ]);
    std::fs::remove_dir_all(&checkout).unwrap();
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
    /// The player as their own client sees them (`Via::Bundle`): the
    /// game's prediction, its inputs reaching the host and the host's
    /// poses coming back a few ticks late, as over the network.
    client: Option<Client>,
}

/// One-way delay, in ticks, between the client and the host.
const LAG: u64 = 4;

/// What the client sends the host.
enum Send {
    Move(u64, MoveInput),
    Command(u64, Box<Command>, ActionAim),
}

struct Client {
    predictor: Predictor,
    to_host: VecDeque<(u64, Send)>,
    to_client: VecDeque<(u64, u64, u64, PlayerState)>,
    tick: u64,
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
        via: Via,
    ) -> Self {
        let (pack, catalog) = imported(root, addon, namespace, via);
        let mut map = vec![
            ColliderBuilder::cuboid(200.0, 0.5, 200.0).translation(Vector::new(0.0, -0.5, 0.0)),
        ];
        map.extend(extra);
        let mirror = map.clone();
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
        let client = matches!(via, Via::Bundle).then(|| {
            let (state, _) = s
                .motion_states()
                .into_iter()
                .find(|(p, _)| p.owner == player)
                .unwrap();
            Client {
                predictor: Predictor::new(
                    CollisionMirror::new(Definitions::default(), mirror, vec![]),
                    state,
                    Default::default(),
                )
                .unwrap(),
                to_host: VecDeque::new(),
                to_client: VecDeque::new(),
                tick: 0,
            }
        });
        let mut g = Self {
            s,
            rules: format!("{namespace}-rules"),
            seq: 0,
            moves: 0,
            input: MoveInput::default(),
            player,
            others: vec![],
            client,
        };
        g.steps(30);
        g.s.give_item(player, item).unwrap();
        let slot = g.s.tool_inventories()[&player]
            .slots
            .iter()
            .position(|s| s.as_deref() == Some(item))
            .unwrap();
        g.cmd(Command::EquipTool { slot: Some(slot) });
        // Its image raises (the copy's own Activate time) before a click
        // fires it, as a player waits for it to come up.
        for _ in 0..1200 {
            g.steps(1);
            if g.held_state().eq_ignore_ascii_case("ready") {
                return g;
            }
        }
        panic!("{item} never came up ready: {}", g.held_state());
    }
    fn held_state(&self) -> String {
        self.s
            .weapon_view()
            .images
            .get(&self.player)
            .and_then(|i| i.first())
            .map(|i| i.state.clone())
            .unwrap_or_default()
    }
    /// A command, with the aim it was given at, as the game's client
    /// sends every command.
    fn cmd(&mut self, command: Command) {
        self.seq += 1;
        let aim = ActionAim {
            yaw: self.input.yaw,
            pitch: self.input.pitch,
        };
        match &mut self.client {
            Some(c) => c
                .to_host
                .push_back((c.tick + LAG, Send::Command(self.seq, Box::new(command), aim))),
            None => {
                self.s
                    .command_with_aim(self.player, self.seq, command, Some(aim))
                    .unwrap();
            }
        }
    }
    fn rule(&mut self, command: &str) {
        self.cmd(Command::Package(PackageCommand {
            package: self.rules.clone(),
            command: command.into(),
            args: vec![],
        }));
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
            if let Some(c) = &mut self.client {
                c.tick += 1;
                c.predictor.step(self.input).unwrap();
                c.to_host.push_back((
                    c.tick + LAG,
                    Send::Move(c.predictor.sequence(), self.input),
                ));
                while c.to_host.front().is_some_and(|(due, _)| *due <= c.tick) {
                    match c.to_host.pop_front().unwrap().1 {
                        Send::Move(sequence, input) => {
                            let _ = self.s.movement(self.player, sequence, input);
                        }
                        Send::Command(sequence, command, aim) => {
                            self.s
                                .command_with_aim(self.player, sequence, *command, Some(aim))
                                .unwrap();
                        }
                    }
                }
            } else {
                self.moves += 1;
                let _ = self.s.movement(self.player, self.moves, self.input);
            }
            for (owner, input, n) in &mut self.others {
                *n += 1;
                let _ = self.s.movement(*owner, *n, *input);
            }
            self.s.step().unwrap();
            if let Some(c) = &mut self.client {
                let (state, ack) = self
                    .s
                    .motion_states()
                    .into_iter()
                    .find(|(p, _)| p.owner == self.player)
                    .unwrap();
                c.to_client.push_back((c.tick + LAG, c.tick, ack, state));
                while c.to_client.front().is_some_and(|(due, ..)| *due <= c.tick) {
                    let (_, tick, ack, state) = c.to_client.pop_front().unwrap();
                    c.predictor.reconcile(tick, ack, state).unwrap();
                }
            }
        }
    }
    /// The player as they see themselves: their client's prediction when
    /// there is one, else the host's.
    fn shown(&self) -> PlayerState {
        match &self.client {
            Some(c) => c.predictor.state().clone(),
            None => self.me(),
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
        Vec3::from(self.shown().feet)
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
        Vec3::from(self.shown().velocity)
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
        Via::Import,
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
        Via::Import,
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
        Via::Import,
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

/// The rope is as long as the hook flew: a ceiling 250 up, within the
/// stand-in hook's reach (150 a second for 2 s), holds the holder on a
/// rope that long.
#[test]
fn a_rope_is_as_long_as_the_hook_reaches() {
    let dir = fresh("long");
    let high = 250.0;
    let ceiling =
        ColliderBuilder::cuboid(60.0, 0.5, 60.0).translation(Vector::new(0.0, high + 0.5, -20.0));
    let mut g = Game::new(
        &dir,
        "Tool_GrappleRope",
        "tool_grapplerope",
        "tool_grapplerope:weapon/grapplerope",
        vec![ceiling],
        Via::Import,
    );
    g.look(0.0, 1.4);
    g.trigger(true);
    g.steps(240);
    let rope = g.s.tether_of(g.player).expect("roped to the far ceiling");
    assert!((rope.anchor[1] - high).abs() < 0.1, "{rope:?}");
    assert!(rope.length > 240.0, "{rope:?}");
    assert!(
        g.s.package_diagnostics().is_empty(),
        "{:?}",
        g.s.package_diagnostics()
    );
    std::fs::remove_dir_all(dir).unwrap();
}

/// The copy turns `$Pref::Server::GrappleRopeAnywhere` on at load and the
/// hook's collision reads it. For a copy the port was checked against, the
/// report says the port covers it (the rules hook anything, as with it
/// on); for any other copy it stays a note to check.
#[test]
fn the_anywhere_setting_is_covered_for_a_checked_copy() {
    let input = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ports/Tool_GrappleRope");
    let run = |ports: &bri_addon_import::ports::Ports, name: &str| {
        bri_addon_import::import_with(
            &Options {
                input: input.clone(),
                out: fresh(name).join("out"),
                ..Default::default()
            },
            ports,
        )
        .unwrap()
    };
    let anywhere = |r: &bri_addon_import::report::Report| {
        r.ambiguous
            .iter()
            .find(|a| {
                a.what
                    .starts_with("global $Pref::Server::GrappleRopeAnywhere")
            })
            .expect("noted")
            .resolution
            .clone()
    };
    let mut ports = bri_addon_import::ports::Ports::builtin();
    let unlisted = run(&ports, "anywhere-unlisted");
    assert_eq!(unlisted.ports[0].copy, "unlisted");
    assert_eq!(anywhere(&unlisted), None);
    let entry = ports
        .list
        .ports
        .iter_mut()
        .find(|e| e.addon == "Tool_GrappleRope")
        .unwrap();
    entry.sha256.push(unlisted.source.sha256.clone());
    let listed = run(&ports, "anywhere-listed");
    assert_eq!(listed.ports[0].copy, "listed");
    let resolution = anywhere(&listed).expect("covered");
    assert!(
        resolution.contains("GrappleRopeProjectile::onCollision")
            && resolution.contains("tool_grapplerope"),
        "{resolution}"
    );
}

/// The Hookshot as a release ships it (bundled, installed, turned on in
/// the Add-Ons screen) pulls its shooter to the wall it strikes. Its
/// numbers are whatever the copy's are: with `BRI_ADDON_SEARCH` naming
/// Maxwell's Add-Ons folder this runs his real copy.
#[test]
fn a_bundled_hookshot_pulls_its_shooter_to_the_wall() {
    let dir = fresh("bundled-hookshot");
    let wall =
        ColliderBuilder::cuboid(20.0, 20.0, 0.5).translation(Vector::new(0.0, 20.0, -WALL - 0.5));
    let mut g = Game::new(
        &dir,
        "Weapon_Loz_Hookshot",
        "weapon_loz_hookshot",
        "weapon_loz_hookshot:weapon/hookshotitem",
        vec![wall],
        Via::Bundle,
    );
    let start = g.feet();
    g.look(0.0, 0.0);
    g.trigger(true);
    g.trigger(false);
    let mut fastest = 0.0f32;
    for _ in 0..600 {
        g.steps(1);
        fastest = fastest.max(-g.velocity().z);
    }
    let end = g.feet();
    assert!(fastest > 15.0, "pulled toward the wall: at most {fastest}");
    assert!(
        start.z - end.z > WALL * 0.75,
        "carried most of the way to the wall: {start} to {end}"
    );
    assert!(
        g.s.package_diagnostics().is_empty(),
        "{:?}",
        g.s.package_diagnostics()
    );
    std::fs::remove_dir_all(dir).unwrap();
}

/// The Grapple Rope as a release ships it ropes its holder to the ceiling
/// the hook strikes, and the rope holds them there.
#[test]
fn a_bundled_grapple_rope_ropes_its_holder() {
    let dir = fresh("bundled-rope");
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
        Via::Bundle,
    );
    g.look(0.0, 1.0);
    g.trigger(true);
    g.steps(90);
    let rope =
        g.s.tether_of(g.player)
            .expect("roped where the hook struck");
    assert!((rope.anchor[1] - CEILING).abs() < 0.1, "{rope:?}");
    assert!(
        g.shown().tether.is_some(),
        "the holder's own client sees the rope"
    );
    // Walking away, the rope stops them.
    g.look(std::f32::consts::PI, 0.0);
    g.input.forward = 1.0;
    g.steps(240);
    let held = g.s.tether_of(g.player).expect("still roped while held");
    assert!(
        (g.feet() + Vec3::Y * 2.65 * 0.85).distance(Vec3::from(held.anchor)) < held.length + 0.6,
        "leashed"
    );
    assert!(
        g.s.package_diagnostics().is_empty(),
        "{:?}",
        g.s.package_diagnostics()
    );
    std::fs::remove_dir_all(dir).unwrap();
}

/// Each port's `entry.json`, as the importer finds them.
fn port_entries(repo: &Path) -> Vec<PathBuf> {
    let mut out: Vec<_> = std::fs::read_dir(repo.join("crates/addon-import/ports"))
        .unwrap()
        .flatten()
        .map(|e| e.path().join("entry.json"))
        .filter(|p| p.is_file())
        .collect();
    out.sort();
    out
}

/// Copy every port's `entry.json` into a checkout at `root`.
fn copy_port_entries(repo: &Path, root: &Path) {
    for entry in port_entries(repo) {
        let to = root.join(entry.strip_prefix(repo).unwrap());
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(&entry, to).unwrap();
    }
}
