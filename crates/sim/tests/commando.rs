//! The Commando sample (`packages/samples/sample-commando*`), a small total
//! conversion, played headless through the authoritative session: every
//! player becomes a commando with the Commando Rifle, rounds leave the
//! clip the rules keep, `/reload` and an empty clip refill it from the
//! reserve through image state hooks, rifle shots hurt and kill
//! target dummies (Add-On entities), and a dummy's death scores and pays
//! ammo through the package's hooks; a sentry creature fires the
//! package's own rounds at players.
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::Definitions,
    player::MoveInput,
    session::{Command, PackageCommand, Session},
    simulation::Simulation,
};
use bri_world::{OwnerId, World};
use glam::Vec3;
use rapier3d::prelude::*;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

const RULES: &str = "sample-commando";

fn samples() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/samples")
}

fn add_ons() -> Arc<Catalog> {
    let packages = [
        ("sample-commando-rifle", Side::Shared),
        ("sample-commando-look", Side::Client),
        ("sample-commando", Side::Server),
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
    let set = PackageSet {
        schema_version: 1,
        packages,
    };
    Arc::new(Catalog::load(&samples(), &set, true).unwrap_or_else(|e| panic!("{e:#?}")))
}

fn weapons() -> bri_weapons::Pack {
    let path = samples().join("sample-commando-rifle/assets/weapons.json");
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
        let mut s = Session::new(
            Simulation::new(
                World::new("Commando".into(), "commando".into(), vec![[1.0; 4]]),
                Definitions::default(),
                vec![
                    ColliderBuilder::cuboid(200.0, 0.5, 200.0)
                        .translation(Vector::new(0.0, -0.5, 0.0)),
                ],
            )
            .unwrap(),
        );
        s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 0.0)]).unwrap();
        s.set_weapon_pack(weapons()).unwrap();
        s.install_packages(add_ons(), None).unwrap();
        Self {
            s,
            seq: BTreeMap::new(),
            moves: BTreeMap::new(),
            looks: BTreeMap::new(),
        }
    }
    fn join(&mut self, name: &str) -> OwnerId {
        let owner = self
            .s
            .join(name.into(), Vec3::new(0.0, 0.05, 0.0), true)
            .unwrap();
        self.looks.insert(owner, MoveInput::default());
        owner
    }
    fn cmd(&mut self, owner: OwnerId, command: Command) -> anyhow::Result<()> {
        let n = self.seq.entry(owner).or_default();
        *n += 1;
        self.s.command(owner, *n, command).map(drop)
    }
    fn rule(&mut self, owner: OwnerId, command: &str) {
        self.cmd(
            owner,
            Command::Package(PackageCommand {
                package: RULES.into(),
                command: command.into(),
                args: vec![],
            }),
        )
        .unwrap();
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
    /// One pull of the trigger, then time for the bolt to cycle.
    fn shoot(&mut self, owner: OwnerId) {
        self.cmd(owner, Command::WeaponTrigger { down: true })
            .unwrap();
        self.steps(2);
        self.cmd(owner, Command::WeaponTrigger { down: false })
            .unwrap();
        self.steps(70);
    }
    /// The clip and reserve the rules keep for the player.
    fn ammo(&self, owner: OwnerId) -> (i64, i64) {
        (
            self.value(Some(owner), "clip"),
            self.value(Some(owner), "reserve"),
        )
    }
    fn value(&self, owner: Option<OwnerId>, key: &str) -> i64 {
        match owner {
            Some(owner) => self.s.package_value(RULES, owner, key),
            None => self
                .s
                .package_state()
                .packages
                .get(RULES)
                .and_then(|ns| ns.global.get(key).cloned()),
        }
        .and_then(|v| v.as_i64())
        .unwrap_or(-1)
    }
}

#[test]
fn a_commando_joins_armed_and_reloads_from_the_reserve() {
    let mut g = Game::new();
    let host = g.join("Host");
    g.steps(40);
    // on_join made them a commando: no jet, 150 health.
    let state =
        g.s.motion_states()
            .into_iter()
            .find(|(p, _)| p.owner == host)
            .unwrap()
            .0;
    let commando = g.s.archetypes().resolve(state.archetype);
    assert_eq!(commando.id, "sample-commando:archetype/commando");
    assert!(!commando.movement.can_jet);
    assert_eq!(g.s.vitals()[&host].health, 150.0);
    // on_loadout handed them the rifle, raised and full.
    assert_eq!(g.ammo(host), (8, 24));
    g.shoot(host);
    g.shoot(host);
    assert_eq!(g.ammo(host), (6, 24));
    // /reload moves two rounds from the reserve into the clip.
    g.rule(host, "reload");
    g.steps(200);
    assert_eq!(g.ammo(host), (8, 22));
    // Emptying the clip reloads by itself once the trigger is let go.
    for _ in 0..8 {
        g.shoot(host);
    }
    assert_eq!(g.ammo(host), (0, 22));
    g.steps(200);
    assert_eq!(g.ammo(host), (8, 14));
    // Out of everything, the trigger only clicks: no round leaves.
    for _ in 0..3 {
        for _ in 0..8 {
            g.shoot(host);
        }
        g.steps(200);
    }
    assert_eq!(g.ammo(host), (0, 0));
    g.steps(300);
    g.cmd(host, Command::WeaponTrigger { down: true }).unwrap();
    g.steps(2);
    assert_eq!(g.s.weapon_view().fired().count(), 0);
    g.cmd(host, Command::WeaponTrigger { down: false }).unwrap();
    assert!(
        g.s.package_diagnostics().is_empty(),
        "{:?}",
        g.s.package_diagnostics()
    );
}

#[test]
fn rifle_shots_drop_target_dummies_and_the_rule_pays_for_them() {
    let mut g = Game::new();
    let host = g.join("Host");
    g.steps(40);
    // Three dummies twelve units ahead (the player faces -z).
    g.rule(host, "dummies");
    g.steps(30);
    let dummies = g.s.package_entities();
    assert_eq!(dummies.len(), 3, "{dummies:?}");
    let ahead = dummies
        .iter()
        .find(|d| d.position[0].abs() < 0.5)
        .expect("one straight ahead")
        .id;
    // Aim at the middle one's chest from the eye.
    let eye = g.s.archetypes().eye(
        &g.s.motion_states()
            .into_iter()
            .find(|(p, _)| p.owner == host)
            .unwrap()
            .0,
    );
    let target =
        Vec3::from(dummies.iter().find(|d| d.id == ahead).unwrap().position) + Vec3::Y * 1.4;
    let to = (target - eye).normalize();
    g.looks.get_mut(&host).unwrap().pitch = to.y.asin();
    g.steps(5);
    // 80 health, 40 a round: two hits drop it.
    g.shoot(host);
    assert!(
        g.s.package_entities().iter().any(|d| d.id == ahead),
        "one hit"
    );
    g.shoot(host);
    assert!(
        !g.s.package_entities().iter().any(|d| d.id == ahead),
        "two hits drop it"
    );
    // on_entity_death: a point, the global count, and eight rounds.
    assert_eq!(g.value(Some(host), "score"), 1);
    assert_eq!(g.value(None, "dummies_down"), 1);
    assert_eq!(g.ammo(host), (6, 32));
    assert!(
        g.s.package_diagnostics().is_empty(),
        "{:?}",
        g.s.package_diagnostics()
    );
}

#[test]
fn a_kill_in_a_minigame_scores_and_refills_the_killer() {
    use bri_sim::session::MiniGameRequest;
    let mut g = Game::new();
    let host = g.join("Host");
    let guest = g.join("Guest");
    g.steps(40);
    g.cmd(
        host,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: bri_minigames::Settings {
                loadout: Default::default(),
                ..Default::default()
            },
        }),
    )
    .unwrap();
    let game = g.s.minigame_views()[0].id;
    // The guest respawns ten units ahead of the host.
    g.s.set_spawn_points(vec![Vec3::new(0.0, 0.05, -10.0)])
        .unwrap();
    g.cmd(guest, Command::MiniGame(MiniGameRequest::Join { game }))
        .unwrap();
    // Past spawn protection; the minigame's loadout comes from on_loadout.
    g.steps(330);
    let eye = |g: &Game, owner| {
        let state =
            g.s.motion_states()
                .into_iter()
                .find(|(p, _)| p.owner == owner)
                .unwrap()
                .0;
        (g.s.archetypes().eye(&state), Vec3::from(state.feet))
    };
    let (from, _) = eye(&g, host);
    let (_, feet) = eye(&g, guest);
    let to = (feet + Vec3::Y * 1.2 - from).normalize();
    let look = g.looks.get_mut(&host).unwrap();
    look.pitch = to.y.asin();
    look.yaw = to.x.atan2(-to.z);
    g.steps(5);
    // 150 health, 40 a round: the fourth drops them.
    for _ in 0..3 {
        g.shoot(host);
    }
    assert!(g.s.vitals()[&guest].alive);
    assert_eq!(g.s.vitals()[&guest].health, 30.0);
    g.shoot(host);
    assert!(!g.s.vitals()[&guest].alive);
    // on_death runs at the start of the next tick.
    g.steps(2);
    assert_eq!(g.value(Some(host), "score"), 1);
    assert_eq!(g.value(Some(host), "streak"), 1);
    assert_eq!(g.ammo(host), (4, 32), "eight rounds for the kill");
    assert!(
        g.s.package_diagnostics().is_empty(),
        "{:?}",
        g.s.package_diagnostics()
    );
}

#[test]
fn a_sentry_fires_its_own_rounds_at_players_outside_any_minigame() {
    let mut g = Game::new();
    let host = g.join("Host");
    g.steps(40);
    g.rule(host, "sentry");
    g.steps(10);
    let sentries = g.s.package_entities();
    assert_eq!(sentries.len(), 1, "{sentries:?}");
    // A think every 90 ticks, a round of 10 each: past v20's spawn
    // protection, with no minigame to allow player damage, they hit.
    // Nobody is credited.
    g.steps(600);
    let health = g.s.vitals()[&host].health;
    assert!(health <= 120.0, "the sentry's rounds hit: {health}");
    assert_eq!(g.value(Some(host), "score"), 0);
    // The package's own fire passes over its own creatures.
    assert_eq!(g.s.package_entities().len(), 1);
    assert!(
        g.s.package_diagnostics().is_empty(),
        "{:?}",
        g.s.package_diagnostics()
    );
}
