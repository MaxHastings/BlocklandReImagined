//! The Ragdoll showcase Add-On's client code (`packages/showcase/ragdoll`):
//! built from its WebAssembly text, it turns a dead player's body into
//! jointed, shared local bodies and poses the body from them.
use bri_client_sandbox::{
    AddOn, AddOnCode, Budgets, FrameInput, Sandbox, TrustLevel, World,
    bodies::{BodyState, PhysicsCommand},
    world::Player,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[path = "support/blockhead.rs"]
mod blockhead;
use blockhead::blockhead;

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/showcase/ragdoll")
}

#[test]
fn the_ragdoll_module_is_built_from_its_source() {
    let built = wat::parse_file(dir().join("client/main.wat")).unwrap();
    let path = dir().join("client/main.wasm");
    if std::env::var_os("BRI_BLESS").is_some() {
        std::fs::write(&path, &built).unwrap();
    }
    assert_eq!(
        std::fs::read(&path).unwrap_or_default(),
        built,
        "ragdoll/client/main.wasm is stale; rerun with BRI_BLESS=1"
    );
}

fn start() -> AddOn {
    let code = AddOnCode::load(&dir()).unwrap().unwrap();
    assert_eq!(code.name, "Ragdoll");
    Sandbox::new()
        .unwrap()
        .start_in(&code, Budgets::default(), TrustLevel::Sandboxed, 2)
        .unwrap()
}

fn world(alive: bool, feet: [f32; 3]) -> Arc<World> {
    world_in(alive, feet, 100)
}

/// The player in the life that spawned at tick `life`.
fn world_in(alive: bool, feet: [f32; 3], life: u64) -> Arc<World> {
    Arc::new(World {
        local: 1,
        players: vec![Player {
            id: 7,
            alive,
            feet,
            velocity: [2.0, 0.0, 0.0],
            life,
            ..Default::default()
        }],
        skeletons: [(7, blockhead(feet))].into(),
        ..Default::default()
    })
}

fn frame(world: &Arc<World>, bodies: &Arc<BTreeMap<u32, BodyState>>) -> FrameInput {
    FrameInput {
        dt: 1.0 / 60.0,
        world: world.clone(),
        bodies: bodies.clone(),
        ..Default::default()
    }
}

#[test]
fn a_death_builds_a_jointed_shared_ragdoll_and_poses_the_body_from_it() {
    let mut addon = start();
    let none = Arc::default();
    // Alive: nothing.
    let out = addon.frame(frame(&world(true, [0.0; 3]), &none)).unwrap();
    assert!(out.physics.is_empty() && out.poses.is_empty());
    // Dies: nine bodies (every part on its own node, femchest sharing the
    // chest's) and eight joints hanging them from the torso.
    let dead = world(false, [0.0; 3]);
    let out = addon.frame(frame(&dead, &none)).unwrap().clone();
    let created: Vec<_> = out
        .physics
        .iter()
        .filter_map(|c| match c {
            PhysicsCommand::Create { body, spec } => Some((*body, *spec)),
            _ => None,
        })
        .collect();
    let joints = out
        .physics
        .iter()
        .filter(|c| matches!(c, PhysicsCommand::Joint { .. }))
        .count();
    assert_eq!((created.len(), joints), (9, 8), "{:#?}", out.physics);
    for (body, spec) in &created {
        assert_eq!(bri_client_sandbox::bodies::body_slot(*body), Some(2));
        assert!(spec.shared && spec.group == 1, "{spec:?}");
        assert!(
            spec.velocity[0] > -0.1 && spec.velocity[1] > 3.0,
            "moves as it died, with a pop"
        );
    }
    // The torso box sits round what is drawn on the torso, in its frame.
    let torso = created[0].1;
    assert_eq!(torso.position, [0.0, 1.3, 0.0]);
    assert!((torso.offset[1] - 0.5).abs() < 1e-5, "{torso:?}");
    // The head hangs from the torso at the neck, twisting up along it.
    let neck = out
        .physics
        .iter()
        .find_map(|c| match c {
            PhysicsCommand::Joint { a, b, spec } if *a == created[0].0 && *b == created[2].0 => {
                Some(*spec)
            }
            _ => None,
        })
        .expect("neck joint");
    assert_eq!(neck.anchor, [0.0, 2.3, 0.0]);
    assert!(neck.axis[1] > 0.99);
    // The game simulated them: the body is drawn where they lie.
    let bodies: Arc<BTreeMap<u32, BodyState>> = Arc::new(
        created
            .iter()
            .map(|(id, spec)| {
                (
                    *id,
                    BodyState {
                        position: [spec.position[0] + 1.0, spec.position[1], spec.position[2]],
                        rotation: spec.rotation,
                        velocity: [0.0; 3],
                        spin: [0.0; 3],
                        resting: false,
                        shared: true,
                        group: 1,
                        mass: 1.0,
                        radius: 0.5,
                    },
                )
            })
            .collect(),
    );
    let out = addon.frame(frame(&dead, &bodies)).unwrap().clone();
    assert!(
        out.physics
            .iter()
            .all(|c| !matches!(c, PhysicsCommand::Create { .. }))
    );
    assert_eq!(out.poses.len(), 1);
    let pose = &out.poses[0];
    assert_eq!((pose.player, pose.nodes.len()), (7, 9));
    assert!(
        pose.nodes
            .iter()
            .any(|(node, at, _)| *node == 3 && at[0] == 1.0)
    );
    // The ragdoll belongs to the life it died in, not to the alive flag:
    // the corpse read as alive for a frame keeps it.
    let out = addon
        .frame(frame(&world(true, [0.0; 3]), &bodies))
        .unwrap()
        .clone();
    assert!(
        out.physics
            .iter()
            .all(|c| !matches!(c, PhysicsCommand::Remove { .. }))
    );
    // A new body spawned (the owner id stays the same): the bodies go and
    // the body is the game's again.
    let out = addon
        .frame(frame(&world_in(true, [0.0; 3], 900), &bodies))
        .unwrap()
        .clone();
    let removed = out
        .physics
        .iter()
        .filter(|c| matches!(c, PhysicsCommand::Remove { .. }))
        .count();
    assert_eq!((removed, out.poses.len()), (9, 0));
}

#[test]
fn a_blast_on_the_corpse_throws_every_limb_but_landing_does_not() {
    let mut addon = start();
    let none = Arc::default();
    let mut corpse = World::clone(&world(false, [0.0; 3]));
    addon
        .frame(frame(&Arc::new(corpse.clone()), &none))
        .unwrap();
    corpse.players[0].velocity = [0.0, 0.0, 0.0];
    let landed = addon
        .frame(frame(&Arc::new(corpse.clone()), &none))
        .unwrap();
    assert!(
        landed
            .physics
            .iter()
            .all(|c| !matches!(c, PhysicsCommand::Push { .. }))
    );
    corpse.players[0].velocity = [0.0, 20.0, 12.0];
    let blasted = addon.frame(frame(&Arc::new(corpse), &none)).unwrap();
    let pushes: Vec<_> = blasted
        .physics
        .iter()
        .filter_map(|c| match c {
            PhysicsCommand::Push { velocity, .. } => Some(*velocity),
            _ => None,
        })
        .collect();
    assert_eq!(pushes, vec![[0.0, 20.0, 12.0]; 9]);
}
