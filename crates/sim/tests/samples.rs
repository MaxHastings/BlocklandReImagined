//! The Survival Points sample Add-On running in a headless session: points
//! for staying alive, a public leaderboard, an admin-only reset and a
//! chat line on join.
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::Definitions,
    session::{Command, Notice, PackageCommand, Session},
    simulation::Simulation,
};
use bri_world::World;
use glam::Vec3;
use std::{path::PathBuf, sync::Arc};

const RULE: &str = "sample-survival-points";
/// behaviour.json's tick_interval: five seconds at 120 ticks per second.
const EVERY: usize = 600;

fn session() -> Session {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/samples");
    let set = PackageSet {
        schema_version: 1,
        packages: vec![PackageEntry {
            id: RULE.into(),
            version: "1.0.0".into(),
            side: Side::Server,
            dir: RULE.into(),
            role: None,
        }],
    };
    let catalog = Catalog::load(&root, &set, true).unwrap_or_else(|e| panic!("{e:#?}"));
    let world = World::new("Samples".into(), "samples".into(), vec![[1.0; 4]]);
    let mut session = Session::new(Simulation::new(world, Definitions::default(), vec![]).unwrap());
    session.install_packages(Arc::new(catalog), None).unwrap();
    session
}
fn points(s: &Session, owner: u64, key: &str) -> i64 {
    s.package_value(RULE, owner, key)
        .and_then(|v| v.as_i64())
        .unwrap_or(-1)
}
fn send(s: &mut Session, owner: u64, seq: u64, command: &str) -> anyhow::Result<()> {
    s.command(
        owner,
        seq,
        Command::Package(PackageCommand {
            package: RULE.into(),
            command: command.into(),
            args: vec![],
        }),
    )
    .map(drop)
}
fn run(s: &mut Session, ticks: usize) {
    for _ in 0..ticks {
        s.step().unwrap();
    }
}

#[test]
fn survival_points_award_living_players_and_greet_them() {
    let mut s = session();
    let host = s
        .join("Host".into(), Vec3::new(0.0, 1.0, 0.0), true)
        .unwrap();
    let guest = s
        .join("Guest".into(), Vec3::new(4.0, 1.0, 0.0), false)
        .unwrap();
    let greeted = s.take_private_notices();
    for owner in [host, guest] {
        assert!(
            greeted.iter().any(|(o, n)| *o == owner
                && matches!(n, Notice::Chat(t) if t.starts_with("Survival Points:"))),
            "{greeted:?}"
        );
    }
    run(&mut s, EVERY * 2 + 1);
    assert_eq!(points(&s, host, "points"), 2);
    assert_eq!(points(&s, guest, "best"), 2);
    // Public keys reach every client; that is what the HUD sample draws.
    let view = s.package_state();
    let ns = &view.packages[RULE];
    assert_eq!(ns.players[&guest]["points"], 2);
    assert_eq!(ns.global["awarded"], 4);
    assert!(
        s.package_diagnostics().is_empty(),
        "{:?}",
        s.package_diagnostics()
    );
}

#[test]
fn survival_points_leaderboard_and_admin_reset() {
    let mut s = session();
    let host = s
        .join("Host".into(), Vec3::new(0.0, 1.0, 0.0), true)
        .unwrap();
    let guest = s
        .join("Guest".into(), Vec3::new(4.0, 1.0, 0.0), false)
        .unwrap();
    run(&mut s, EVERY + 1);
    send(&mut s, guest, 1, "top").unwrap();
    run(&mut s, 1);
    assert!(
        s.chat()
            .iter()
            .any(|l| l.text.starts_with("Survival Points leader: ") && l.text.ends_with("with 1.")),
        "{:?}",
        s.chat()
    );
    // Only administrators may reset.
    assert!(send(&mut s, guest, 2, "reset").is_err());
    assert_eq!(points(&s, guest, "best"), 1);
    send(&mut s, host, 1, "reset").unwrap();
    run(&mut s, 1);
    assert_eq!(points(&s, guest, "best"), 0);
    assert!(
        s.chat()
            .iter()
            .any(|l| l.text == "Survival Points were reset.")
    );
}
