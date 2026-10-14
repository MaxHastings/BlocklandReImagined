//! `set_pass_group` (docs/modding/rules.md): bodies an Add-On puts in the
//! same pass group walk through each other and still meet everyone else,
//! on the host and in a client's prediction alike.
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::Definitions,
    player::{MoveInput, PlayerState},
    prediction::{CollisionMirror, Predictor},
    session::{Command, PackageArg, PackageCommand, Session},
    simulation::Simulation,
};
use bri_world::{OwnerId, World};
use glam::Vec3;
use rapier3d::prelude::*;
use serde_json::json;
use std::{
    path::PathBuf,
    sync::Arc,
    sync::atomic::{AtomicUsize, Ordering},
};

const TEAMS: &str = r#"
fn cmd_team(p, group) { for q in players() { set_pass_group(q.id, group); } }
fn cmd_solo(p, group) { set_pass_group(p, group); }
fn on_contact(info) {
    if info.kind != "body" { return; }
    set("touches", get("touches") + 1);
}
"#;

struct Root(PathBuf);
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn catalog() -> Arc<Catalog> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = Root(std::env::temp_dir().join(format!(
        "bri-pass-groups-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    let dir = root.0.join("teams");
    std::fs::create_dir_all(&dir).unwrap();
    let write = |file: &str, value: String| std::fs::write(dir.join(file), value).unwrap();
    write(
        "package.json",
        json!({
            "schema_version": 1, "id": "teams", "version": "1.0.0", "api": 1,
            "name": "teams", "license": "CC0-1.0", "capabilities": ["physics"],
            "provides": [
                { "kind": "behaviour", "id": "teams:behaviour/main", "file": "behaviour.json" },
                { "kind": "script", "id": "teams:script/main", "file": "main.rhai" }
            ]
        })
        .to_string(),
    );
    let group = json!(["int"]);
    write(
        "behaviour.json",
        json!({
            "schema_version": 1, "script": "main.rhai", "on_contact": true,
            "state": { "global": { "touches": { "default": 0, "visible": "everyone" } } },
            "commands": [{ "name": "team", "args": group }, { "name": "solo", "args": group }]
        })
        .to_string(),
    );
    write("main.rhai", TEAMS.into());
    let set = PackageSet {
        schema_version: 1,
        packages: vec![PackageEntry {
            id: "teams".into(),
            version: "1.0.0".into(),
            side: Side::Server,
            dir: "teams".into(),
            role: None,
        }],
    };
    Arc::new(Catalog::load(&root.0, &set, true).unwrap_or_else(|e| panic!("{e:#?}")))
}

fn map() -> Vec<ColliderBuilder> {
    vec![
        ColliderBuilder::cuboid(100.0, 0.5, 100.0)
            .translation(Vector::new(0.0, -0.5, 0.0))
            .user_data(u128::MAX),
    ]
}

struct Game {
    s: Session,
    a: OwnerId,
    b: OwnerId,
    seq: u64,
}
impl Game {
    fn new() -> Self {
        let mut s = Session::new(
            Simulation::new(
                World::new("Teams".into(), "teams".into(), vec![[1.0; 4]]),
                Definitions::default(),
                map(),
            )
            .unwrap(),
        );
        s.install_packages(catalog(), None).unwrap();
        let a = s.join("A".into(), Vec3::new(0.0, 0.05, 0.0), true).unwrap();
        let b = s
            .join("B".into(), Vec3::new(0.0, 0.05, -3.0), false)
            .unwrap();
        for _ in 0..30 {
            s.step().unwrap();
        }
        Self { s, a, b, seq: 0 }
    }
    fn teams(&mut self, by: OwnerId, command: &str, group: i64) {
        self.seq += 1;
        self.s
            .command(
                by,
                self.seq,
                Command::Package(PackageCommand {
                    package: "teams".into(),
                    command: command.into(),
                    args: vec![PackageArg::Int(group)],
                }),
            )
            .unwrap();
        self.s.step().unwrap();
        let problems = self.s.take_package_problems();
        assert!(problems.is_empty(), "{problems:#?}");
    }
    fn state(&self, owner: OwnerId) -> PlayerState {
        self.s
            .motion_states()
            .into_iter()
            .find(|(s, _)| s.owner == owner)
            .unwrap()
            .0
    }
    /// A walks forward (toward B) for `ticks` and ends where?
    fn walk(&mut self, ticks: u64) -> f32 {
        for _ in 0..ticks {
            self.seq += 1;
            self.s
                .movement(
                    self.a,
                    self.seq,
                    MoveInput {
                        forward: 1.0,
                        ..Default::default()
                    },
                )
                .unwrap();
            self.s.step().unwrap();
        }
        self.state(self.a).feet[2]
    }
}

#[test]
fn a_team_in_one_pass_group_walks_through_each_other() {
    let mut g = Game::new();
    g.teams(g.a, "team", 3);
    assert_eq!(g.state(g.a).pass_group, 3);
    assert_eq!(g.state(g.b).pass_group, 3);
    let z = g.walk(240);
    assert!(z < -4.0, "walked through B at z -3: {z}");
}

#[test]
fn different_groups_and_no_group_still_block() {
    let mut g = Game::new();
    // A alone in a group meets B, who is in none.
    g.teams(g.a, "solo", 2);
    let z = g.walk(240);
    assert!(z > -3.0, "stopped at B: {z}");
    // Both in groups, but different ones.
    g.teams(g.b, "solo", 5);
    let z = g.walk(120);
    assert!(z > -3.0, "stopped at B: {z}");
    // Leaving the group (0) leaves A as solid as ever.
    g.teams(g.a, "team", 0);
    assert_eq!(g.state(g.b).pass_group, 0);
}

fn touches(s: &Session) -> i64 {
    s.package_state().packages["teams"].global["touches"]
        .as_i64()
        .unwrap()
}

/// `on_contact` "body" hears only bodies that meet: a team in one group
/// walks through each other without a touch, as the physics lets them.
#[test]
fn bodies_in_one_pass_group_do_not_touch_for_on_contact() {
    let mut g = Game::new();
    g.teams(g.a, "team", 4);
    g.walk(240);
    assert_eq!(touches(&g.s), 0, "walked through a teammate");
    let mut g = Game::new();
    g.teams(g.a, "solo", 4);
    g.walk(240);
    assert!(touches(&g.s) > 0, "A, alone in a group, ran into B");
}

#[test]
fn a_pass_group_outlives_a_respawn() {
    let mut g = Game::new();
    g.teams(g.a, "team", 7);
    g.s.command(g.b, 1, Command::Suicide).unwrap();
    for _ in 0..800 {
        g.s.step().unwrap();
    }
    g.s.command(g.b, 2, Command::Respawn).unwrap();
    g.s.step().unwrap();
    assert!(g.s.is_alive(g.b));
    assert_eq!(g.state(g.b).pass_group, 7);
}

#[test]
fn prediction_walks_through_the_same_bodies_the_host_does() {
    let mut g = Game::new();
    g.teams(g.a, "team", 4);
    let mirror = CollisionMirror::new(Definitions::default(), map(), vec![]);
    let mut prediction = Predictor::new(mirror, g.state(g.a), Default::default()).unwrap();
    let walk = MoveInput {
        forward: 1.0,
        ..Default::default()
    };
    let mut worst = 0.0_f32;
    for _ in 0..240 {
        let other = g.state(g.b);
        prediction.set_others([&other]).unwrap();
        let (sequence, _) = prediction.step(walk).unwrap();
        g.s.movement(g.a, sequence, walk).unwrap();
        g.s.step().unwrap();
        let host = Vec3::from(g.state(g.a).feet);
        worst = worst.max(host.distance(Vec3::from(prediction.state().feet)));
    }
    assert!(g.state(g.a).feet[2] < -4.0, "the host let A through");
    assert!(worst < 0.05, "prediction parted from the host by {worst}");
}
