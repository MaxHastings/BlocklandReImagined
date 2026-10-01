//! The Adventure Pack (`packages/adventure`), played through the
//! authoritative session: magazines that count down and reload from the
//! reserve, the light key's reload, shell-by-shell reloads, ammo boxes,
//! a dropped gun that keeps its magazine, headshots from the engine's hit
//! regions and the hitscan revolver.
use bri_minigames::Settings;
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::Definitions,
    player::MoveInput,
    session::{ActionAim, Command, MiniGameRequest, Reply, Session},
    simulation::Simulation,
};
use bri_world::{OwnerId, World};
use glam::Vec3;
use rapier3d::prelude::*;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

const PISTOL: &str = "adventure-pack:weapon/servicepistol";
const SHOTGUN: &str = "adventure-pack:weapon/levershotgun";
const REVOLVER: &str = "adventure-pack:weapon/revolver";
const SMG: &str = "adventure-pack:weapon/submachinegun";
const PISTOL_AMMO: &str = "adventure-pack:weapon/ammopistol";

fn adventure() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/adventure")
}

fn add_ons() -> Arc<Catalog> {
    let packages = [
        ("adventure-pack", Side::Shared),
        ("adventure-pack-rules", Side::Server),
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
            &adventure(),
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
    let path = adventure().join("adventure-pack/assets/weapons.json");
    bri_weapons::Pack::from_json(&std::fs::read(path).unwrap()).unwrap()
}

struct Game {
    s: Session,
    seq: BTreeMap<OwnerId, u64>,
    moves: BTreeMap<OwnerId, u64>,
    looks: BTreeMap<OwnerId, MoveInput>,
}
impl Game {
    fn new() -> Self {
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
        s.set_weapon_pack(weapons()).unwrap();
        s.set_item_bounds(BTreeMap::new()).unwrap();
        s.install_packages(add_ons(), None).unwrap();
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
    fn cmd(&mut self, owner: OwnerId, command: Command) -> anyhow::Result<Reply> {
        let n = self.seq.entry(owner).or_default();
        *n += 1;
        let look = self.looks[&owner];
        self.s.command_with_aim(
            owner,
            *n,
            command,
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
    /// `owner` starts a minigame handing out `loadout` and `others` join,
    /// each respawning where they stood with the loadout's first tool out.
    fn minigame(&mut self, owner: OwnerId, others: &[OwnerId], loadout: &[&str]) {
        let mut tools: [Option<String>; 5] = Default::default();
        for (slot, item) in loadout.iter().enumerate() {
            tools[slot] = Some(item.to_string());
        }
        let here = self.feet(owner);
        self.s.set_spawn_points(vec![here]).unwrap();
        self.cmd(
            owner,
            Command::MiniGame(MiniGameRequest::Create {
                color: 0,
                settings: Settings {
                    loadout: tools,
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
    /// A new player who joins the minigame and respawns at `at`, with its
    /// loadout.
    fn arrive(&mut self, name: &str, at: Vec3) -> OwnerId {
        let owner = self.join(name, Vec3::new(-40.0, 0.05, -40.0));
        self.steps(2);
        let game = self.s.minigame_views()[0].id;
        self.s.set_spawn_points(vec![Vec3::new(at.x, 0.05, at.z)]).unwrap();
        self.cmd(owner, Command::MiniGame(MiniGameRequest::Join { game }))
            .unwrap();
        owner
    }
    fn draw(&mut self, owner: OwnerId, item: &str) {
        let slot = self.s.tool_inventories()[&owner]
            .slots
            .iter()
            .position(|s| s.as_deref() == Some(item))
            .unwrap_or_else(|| panic!("{owner} carries no {item}"));
        self.cmd(owner, Command::EquipTool { slot: Some(slot) })
            .unwrap();
        // Activate.
        self.steps(40);
    }
    /// One pull of the trigger, held `held` ticks, then `after` ticks more.
    fn pull(&mut self, owner: OwnerId, held: usize, after: usize) {
        self.cmd(owner, Command::WeaponTrigger { down: true }).unwrap();
        self.steps(held);
        self.cmd(owner, Command::WeaponTrigger { down: false })
            .unwrap();
        self.steps(after);
    }
    /// The ammo panel's line: "magazine | reserve".
    fn hud(&self, owner: OwnerId) -> String {
        self.s
            .package_state_for(owner)
            .packages
            .get("adventure-pack-rules")
            .and_then(|ns| ns.players.get(&owner))
            .and_then(|m| m.get("hud"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    }
    fn health(&self, owner: OwnerId) -> f32 {
        self.s.vitals()[&owner].health
    }
    fn holds(&self, owner: OwnerId, item: &str) -> bool {
        self.s.tool_inventories()[&owner]
            .slots
            .iter()
            .any(|s| s.as_deref() == Some(item))
    }
    fn drops(&self, item: &str) -> Vec<Vec3> {
        self.s
            .weapon_view()
            .drops
            .iter()
            .filter(|d| d.item == item)
            .map(|d| d.position)
            .collect()
    }
    fn diagnostics(&self) -> Vec<String> {
        self.s
            .package_diagnostics()
            .iter()
            .map(|d| format!("{}: {}", d.code, d.message))
            .collect()
    }
}

#[test]
fn a_magazine_counts_down_and_reloads_from_the_reserve() {
    let mut g = Game::new();
    let a = g.join("A", Vec3::new(0.0, 0.05, 0.0));
    g.steps(2);
    g.minigame(a, &[], &[PISTOL]);
    g.draw(a, PISTOL);
    assert_eq!(g.hud(a), "12 | 36");
    g.pull(a, 2, 30);
    assert_eq!(g.hud(a), "11 | 36");
    // Empty the magazine: the last round sends the pistol to its reload.
    for _ in 0..11 {
        g.pull(a, 2, 30);
    }
    g.steps(240);
    assert_eq!(g.hud(a), "12 | 24");
    // The light key reloads a part-spent magazine.
    g.pull(a, 2, 30);
    g.pull(a, 2, 30);
    assert_eq!(g.hud(a), "10 | 24");
    g.cmd(a, Command::ToggleLight).unwrap();
    g.steps(240);
    assert_eq!(g.hud(a), "12 | 22");
    assert!(g.diagnostics().is_empty(), "{:?}", g.diagnostics());
}

#[test]
fn a_lever_shotgun_loads_shell_by_shell_and_the_trigger_stops_it() {
    let mut g = Game::new();
    let a = g.join("A", Vec3::new(0.0, 0.05, 0.0));
    g.steps(2);
    g.minigame(a, &[], &[SHOTGUN]);
    g.draw(a, SHOTGUN);
    assert_eq!(g.hud(a), "5 | 16");
    for _ in 0..3 {
        g.pull(a, 2, 80);
    }
    assert_eq!(g.hud(a), "2 | 16");
    g.cmd(a, Command::ToggleLight).unwrap();
    // One shell in.
    g.steps(70);
    assert_eq!(g.hud(a), "3 | 15");
    // A pull stops the reload and fires, and no more shells go in.
    g.pull(a, 2, 80);
    assert_eq!(g.hud(a), "2 | 15");
    g.steps(160);
    assert_eq!(g.hud(a), "2 | 15");
    assert!(g.diagnostics().is_empty(), "{:?}", g.diagnostics());
}

#[test]
fn an_ammo_box_feeds_a_carried_gun_and_a_spare_gun_is_ammo() {
    let mut g = Game::new();
    let a = g.join("A", Vec3::new(0.0, 0.05, 0.0));
    g.steps(2);
    g.minigame(a, &[], &[PISTOL, PISTOL_AMMO, SMG]);
    let throw = |g: &mut Game, item: &str| {
        let slot = g.s.tool_inventories()[&a]
            .slots
            .iter()
            .position(|s| s.as_deref() == Some(item))
            .unwrap();
        g.cmd(a, Command::DropTool { slot }).unwrap();
    };
    // The box one way, the pistol the other.
    throw(&mut g, PISTOL_AMMO);
    g.look(a, std::f32::consts::PI, 0.0);
    g.steps(4);
    throw(&mut g, PISTOL);
    g.steps(360);
    // Nobody who carries no pistol takes pistol ammo.
    let [box_at] = g.drops(PISTOL_AMMO)[..] else {
        panic!("one dropped ammo box");
    };
    let c = g.join("C", box_at);
    g.steps(8);
    assert_eq!(g.drops(PISTOL_AMMO).len(), 1);
    assert!(!g.holds(c, PISTOL_AMMO));
    // B carries a pistol: the box gives double what a spare gun gives.
    let b = g.arrive("B", box_at);
    g.steps(8);
    assert!(g.drops(PISTOL_AMMO).is_empty());
    g.draw(b, PISTOL);
    assert_eq!(g.hud(b), "12 | 84");
    // The thrown pistol is a spare to B: ammo up to the most carried.
    let [gun_at] = g.drops(PISTOL)[..] else {
        panic!("one dropped pistol");
    };
    let b2 = g.arrive("B2", gun_at);
    g.steps(8);
    assert!(g.drops(PISTOL).is_empty());
    g.draw(b2, PISTOL);
    assert_eq!(g.hud(b2), "12 | 60");
    assert!(g.diagnostics().is_empty(), "{:?}", g.diagnostics());
}

#[test]
fn a_pistol_picked_up_keeps_the_magazine_it_was_dropped_with() {
    let mut g = Game::new();
    let a = g.join("A", Vec3::new(0.0, 0.05, 0.0));
    g.steps(2);
    g.minigame(a, &[], &[PISTOL]);
    g.draw(a, PISTOL);
    for _ in 0..5 {
        g.pull(a, 2, 30);
    }
    assert_eq!(g.hud(a), "7 | 36");
    let slot = g.s.tool_inventories()[&a].selected.unwrap();
    g.cmd(a, Command::DropTool { slot }).unwrap();
    g.steps(360);
    let [at] = g.drops(PISTOL)[..] else {
        panic!("one dropped pistol");
    };
    // Someone outside the minigame, carrying no pistol, picks it up.
    let b = g.join("B", at);
    g.steps(8);
    assert!(g.holds(b, PISTOL));
    g.draw(b, PISTOL);
    assert_eq!(g.hud(b), "7 | 36");
    assert!(g.diagnostics().is_empty(), "{:?}", g.diagnostics());
}

/// The revolver traces its shot: a round in the body hurts, one in the
/// head kills, as the engine's hit regions say.
#[test]
fn the_revolver_kills_with_a_headshot_and_hurts_in_the_body() {
    let mut g = Game::new();
    // B stands 6 units ahead of A (A looks along -z).
    let a = g.join("A", Vec3::new(0.0, 0.05, 0.0));
    let b = g.join("B", Vec3::new(0.0, 0.05, -6.0));
    g.steps(2);
    g.minigame(a, &[b], &[REVOLVER]);
    g.draw(a, REVOLVER);
    // The default blockhead: 2.65 tall, eyes 2.16 up; the head is the
    // top 15%.
    let eye = g.feet(a).y + 2.156;
    let aim_at = |g: &Game, y: f32| ((y - eye) / (g.feet(a).z - g.feet(b).z)).atan();
    // The chest: the body takes the round's damage.
    let chest = g.feet(b).y + 1.7;
    let pitch = aim_at(&g, chest);
    g.look(a, 0.0, pitch);
    g.steps(4);
    g.pull(a, 2, 60);
    let hurt = g.health(b);
    assert!((hurt - 52.0).abs() < 0.5, "a body shot leaves {hurt}");
    // The head: dead at once.
    let head = g.feet(b).y + 2.45;
    let pitch = aim_at(&g, head);
    g.look(a, 0.0, pitch);
    g.steps(4);
    g.pull(a, 2, 60);
    assert!(!g.s.vitals()[&b].alive, "a headshot kills");
    assert!(g.diagnostics().is_empty(), "{:?}", g.diagnostics());
}

/// The service pistol's rounds fly: the engine tells `on_damage` where a
/// round struck, and a round in the head kills where one in the body hurts.
#[test]
fn a_pistol_round_in_the_head_kills_through_on_damage() {
    let mut g = Game::new();
    let a = g.join("A", Vec3::new(0.0, 0.05, 0.0));
    let b = g.join("B", Vec3::new(0.0, 0.05, -6.0));
    g.steps(2);
    g.minigame(a, &[b], &[PISTOL]);
    g.draw(a, PISTOL);
    let eye = g.feet(a).y + 2.156;
    let aim_at = |g: &Game, y: f32| ((y - eye) / (g.feet(a).z - g.feet(b).z)).atan();
    let pitch = aim_at(&g, g.feet(b).y + 1.7);
    g.look(a, 0.0, pitch);
    g.steps(4);
    g.pull(a, 2, 60);
    let hurt = g.health(b);
    assert!((hurt - 80.0).abs() < 0.5, "a body shot leaves {hurt}");
    let pitch = aim_at(&g, g.feet(b).y + 2.45);
    g.look(a, 0.0, pitch);
    g.steps(4);
    g.pull(a, 2, 60);
    assert!(!g.s.vitals()[&b].alive, "a headshot kills: {}", g.health(b));
    assert!(g.diagnostics().is_empty(), "{:?}", g.diagnostics());
}
