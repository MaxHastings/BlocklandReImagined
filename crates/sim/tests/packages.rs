//! Package-defined gameplay through the session: the Stress Lab packages
//! generate a world, mine it, run an economy and a creature, and save.
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::{Definition, Definitions},
    player::MoveInput,
    session::{ActionAim, Command, Notice, PackageArg, PackageCommand, PackageSave, Session},
    simulation::Simulation,
};
use bri_world::World;
use glam::Vec3;
use std::{path::PathBuf, sync::Arc};

const CUBE: &str = "v20/brick/brick4xcubedata";

fn definitions() -> Definitions {
    let mesh = Mesh {
        schema_version: 1,
        id: CUBE.into(),
        footprint_studs: [4, 4],
        height_plates: 10,
        // One row per stud row per plate layer: 4 deep x 10 plates.
        attachment_rows: vec!["bbbb".into(); 40],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![],
    };
    let collision = CollisionBody {
        id: CUBE.into(),
        parts: vec![Part::Box {
            center: [0.0; 3],
            size: [2.0, 2.0, 2.0],
        }],
    };
    let shape = bri_physics::content::collider(&collision)
        .unwrap()
        .build()
        .shared_shape()
        .clone();
    Definitions {
        entries: [(
            CUBE.into(),
            Definition {
                mesh,
                collision,
                shape,
                indestructible: false,
                special: Default::default(),
            },
        )]
        .into(),
    }
}
fn catalog() -> Arc<Catalog> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/stresslab");
    let mut packages = vec![PackageEntry {
        id: "v20-bricks".into(),
        version: "4.0.0".into(),
        side: Side::Shared,
        dir: "unused".into(),
        role: Some("brick_catalog".into()),
    }];
    for (id, side) in [
        ("stresslab-world", Side::Server),
        ("stresslab-creeper", Side::Server),
        ("stresslab-creeper-model", Side::Client),
        ("stresslab-economy", Side::Server),
        ("stresslab-hud", Side::Client),
        ("stresslab-mode", Side::Server),
    ] {
        packages.push(PackageEntry {
            id: id.into(),
            version: "1.0.0".into(),
            side,
            dir: id.into(),
            role: None,
        });
    }
    Arc::new(
        Catalog::load(
            &root,
            &PackageSet {
                schema_version: 1,
                packages,
            },
            true,
        )
        .unwrap_or_else(|e| panic!("{e:#?}")),
    )
}
fn session(save: Option<PackageSave>) -> (Session, Vec<Vec3>) {
    let world = World::new("Stress Lab".into(), "stresslab".into(), vec![[1.0; 4]]);
    let mut session = Session::new(Simulation::new(world, definitions(), vec![]).unwrap());
    let spawns = session.install_packages(catalog(), save).unwrap();
    (session, spawns)
}
fn pkg(package: &str, command: &str, args: Vec<PackageArg>) -> Command {
    Command::Package(PackageCommand {
        package: package.into(),
        command: command.into(),
        args,
    })
}
fn look_down(s: &mut Session, owner: u64, sequence: u64) {
    s.movement(
        owner,
        sequence,
        MoveInput {
            pitch: -1.5,
            ..Default::default()
        },
    )
    .unwrap();
}
fn value(s: &Session, owner: u64, key: &str) -> i64 {
    s.package_value("stresslab-economy", owner, key)
        .and_then(|v| v.as_i64())
        .unwrap_or(-1)
}
fn code(error: &anyhow::Error) -> String {
    error
        .downcast_ref::<bri_package::diag::Rejected>()
        .map(|r| r.0.0[0].code.clone())
        .unwrap_or_else(|| format!("{error:#}"))
}
const DOWN: Option<ActionAim> = Some(ActionAim {
    yaw: 0.0,
    pitch: -1.5,
});

#[test]
fn generated_world_mines_into_server_owned_currency() {
    let (mut s, spawns) = session(None);
    assert!(!spawns.is_empty());
    let stats = s.package_stats();
    assert_eq!(stats.chunks, 25, "5x5 chunks around the origin");
    assert!(stats.voxels > 5_000, "{stats:?}");
    let a = s.join("Miner".into(), spawns[0], false).unwrap();
    look_down(&mut s, a, 1);
    for _ in 0..30 {
        s.step().unwrap();
    }
    let before = s.simulation().state().bricks.len();
    let mut seq = 1;
    let mut mined = 0;
    for _ in 0..40 {
        seq += 1;
        s.command_with_aim(a, seq, pkg("stresslab-economy", "mine", vec![]), DOWN)
            .unwrap();
        for _ in 0..20 {
            s.step().unwrap();
        }
        mined = value(&s, a, "mined");
    }
    assert!(mined >= 3, "mined {mined}");
    assert!(s.simulation().state().bricks.len() < before);
    assert!(s.package_stats().removed_voxels >= 3);
    // The cooldown is the server's: a second mine in the same tick fails.
    seq += 1;
    s.command_with_aim(a, seq, pkg("stresslab-economy", "mine", vec![]), DOWN)
        .unwrap();
    seq += 1;
    let fast = s
        .command_with_aim(a, seq, pkg("stresslab-economy", "mine", vec![]), DOWN)
        .unwrap_err();
    assert_eq!(code(&fast), "command.cooldown");
    // Selling turns ore into Bits; nothing else can.
    let ore = value(&s, a, "coal") * 2 + value(&s, a, "copper") * 5 + value(&s, a, "gold") * 20;
    seq += 1;
    s.command(a, seq, pkg("stresslab-economy", "sell_all", vec![]))
        .unwrap();
    assert_eq!(value(&s, a, "bits"), ore);
    // A purse reaches its owner's client only; the view names the owner.
    let view = s.package_state_for(a);
    assert_eq!(
        view.packages["stresslab-economy"].players[&a]["mined"],
        serde_json::json!(value(&s, a, "mined"))
    );
    assert!(
        s.package_state()
            .packages
            .get("stresslab-economy")
            .is_none_or(|n| !n.players.contains_key(&a))
    );
    assert!(value(&s, a, "mined") >= mined);
}

#[test]
fn clients_cannot_forge_package_commands() {
    let (mut s, spawns) = session(None);
    let a = s.join("Forger".into(), spawns[0], false).unwrap();
    let mut seq = 0;
    let mut attempt = |s: &mut Session, command: Command| {
        seq += 1;
        code(&s.command(a, seq, command).unwrap_err())
    };
    assert_eq!(
        attempt(&mut s, pkg("stresslab-economy", "give_bits", vec![])),
        "command.unknown"
    );
    assert_eq!(
        attempt(
            &mut s,
            pkg("stresslab-economy", "sell", vec![PackageArg::Int(5)])
        ),
        "command.args"
    );
    assert_eq!(
        attempt(
            &mut s,
            pkg("stresslab-economy", "sell_all", vec![PackageArg::Int(1)])
        ),
        "command.args"
    );
    assert_eq!(
        attempt(&mut s, pkg("stresslab-creeper", "spawn", vec![])),
        "command.admin"
    );
    assert_eq!(
        attempt(&mut s, pkg("not-a-package", "mine", vec![])),
        "command.package"
    );
    assert_eq!(value(&s, a, "bits"), 0);
}

#[test]
fn creeper_chases_explodes_and_damages_players_and_ground() {
    let (mut s, spawns) = session(None);
    let admin = s.join("Admin".into(), spawns[0], true).unwrap();
    // Stand still for the spawn protection to end.
    for _ in 0..320 {
        s.step().unwrap();
    }
    s.command(admin, 1, pkg("stresslab-creeper", "spawn", vec![]))
        .unwrap();
    let start = s.package_entities();
    assert_eq!(start.len(), 1, "{:?}", s.package_diagnostics());
    let feet = s.snapshot().players[0].feet;
    let first = Vec3::from(start[0].position).distance(Vec3::from(feet));
    let removed = s.package_stats().removed_voxels;
    let mut closest = first;
    let mut exploded_at = None;
    for tick in 0..1200 {
        s.step().unwrap();
        if let Some(e) = s.package_entities().first() {
            closest = closest.min(Vec3::from(e.position).distance(Vec3::from(feet)));
        } else {
            exploded_at = Some(tick);
            break;
        }
    }
    assert!(closest < first - 2.0, "it chased: {first} -> {closest}");
    assert!(exploded_at.is_some(), "{:?}", s.package_diagnostics());
    let health = s.vitals()[&admin].health;
    assert!(health < 100.0, "player took damage: {health}");
    assert!(
        s.package_stats().removed_voxels > removed,
        "the blast removed ground"
    );
    let explosions = &s.package_state().packages["stresslab-creeper"].global["explosions"];
    assert_eq!(explosions, &serde_json::json!(1));
    assert!(
        s.take_cues()
            .iter()
            .any(|c| matches!(c.kind, bri_sim::presentation::CueKind::Explosion { .. }))
    );
}

#[test]
fn world_edits_and_durable_state_survive_save_and_reload() {
    let principal = bri_admin::Principal([7; 32]);
    let (mut s, spawns) = session(None);
    let a = s
        .join_verified("Keeper".into(), spawns[0], false, Some(principal))
        .unwrap();
    look_down(&mut s, a, 1);
    for _ in 0..30 {
        s.step().unwrap();
    }
    for seq in 1..20 {
        s.command_with_aim(a, seq, pkg("stresslab-economy", "mine", vec![]), DOWN)
            .unwrap();
        for _ in 0..20 {
            s.step().unwrap();
        }
    }
    let mined = value(&s, a, "mined");
    assert!(mined > 0);
    let removed = s.package_stats().removed_voxels;
    let holes = s.package_save().unwrap().world.unwrap().removed;
    // Round-trip through bytes, as a host restart would.
    let save = PackageSave::decode(&s.package_save().unwrap().encode().unwrap()).unwrap();
    let (mut restarted, spawns) = session(Some(save));
    assert_eq!(restarted.package_stats().removed_voxels, removed);
    let regenerated: Vec<[i64; 3]> = restarted
        .simulation()
        .state()
        .bricks
        .keys()
        .filter_map(|id| restarted.package_voxel(*id).map(|(v, _)| v))
        .collect();
    assert!(!regenerated.is_empty());
    assert!(
        regenerated.iter().all(|v| !holes.contains(v)),
        "mined voxels stay mined after a restart"
    );
    let back = restarted
        .join_verified("Keeper again".into(), spawns[0], false, Some(principal))
        .unwrap();
    assert_eq!(
        value(&restarted, back, "mined"),
        mined,
        "state follows the durable identity"
    );
    // Someone else starts from zero.
    let other = restarted
        .join_verified(
            "Stranger".into(),
            spawns[1],
            false,
            Some(bri_admin::Principal([9; 32])),
        )
        .unwrap();
    assert_eq!(value(&restarted, other, "mined"), 0);
}

/// A slash command typed in chat names no package: the host finds the
/// package declaring it and reads the words as its arguments, as v20 sends
/// any `/name` to the server.
#[test]
fn typed_chat_commands_reach_the_declaring_package() {
    let (mut s, spawns) = session(None);
    let a = s.join("Typist".into(), spawns[0], false).unwrap();
    s.take_private_notices();
    let typed = |command: &str, words: &[&str]| {
        pkg(
            "",
            command,
            words
                .iter()
                .map(|w| PackageArg::String((*w).into()))
                .collect(),
        )
    };
    // `/sell rocks` runs cmd_sell(player, "rocks") in stresslab-economy.
    s.command(a, 1, typed("Sell", &["rocks"])).unwrap();
    let told = s.take_private_notices();
    assert!(
        told.iter().any(
            |(o, n)| *o == a && matches!(n, Notice::Chat(t) if t.contains("Nobody buys rocks"))
        ),
        "{told:?}"
    );
    // Wrong word counts and unknown names are refused, not forwarded.
    assert_eq!(
        code(&s.command(a, 2, typed("sell_all", &["now"])).unwrap_err()),
        "command.args"
    );
    let unknown = s.command(a, 3, typed("nope", &[])).unwrap_err();
    assert!(format!("{unknown:#}").contains("Unknown command: /nope"));
}
