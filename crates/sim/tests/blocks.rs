//! E32 (Max, 2026-09-28): a mod's own cube, as data. A package block has a
//! texture per face and flipbook states; a package dig tool's server script
//! switches a dug block's state, which replicates with the brick like any
//! other brick field. Nothing here is engine code for mining or cracks.
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::{Catalog, content::FaceLook};
use bri_sim::{
    definitions::{Definition, Definitions},
    session::{ActionAim, Command, PackageCommand, Session},
    simulation::Simulation,
};
use bri_world::{BlockLook, World};
use std::{path::Path, sync::Arc};

const CUBE: &str = "v20/brick/brick4xcubedata";
/// A real 1x1 PNG; each texture is its own file with its own id.
const PNG: &str = "89504e470d0a1a0a0000000d4948445200000001000000010804000000b51c0c020000000b4944415478da6364600000000600023081d02f0000000049454e44ae426082";
const DOWN: Option<ActionAim> = Some(ActionAim {
    yaw: 0.0,
    pitch: -1.5,
});

fn definitions() -> Definitions {
    let mesh = Mesh {
        schema_version: 1,
        id: CUBE.into(),
        footprint_studs: [4, 4],
        height_plates: 10,
        attachment_rows: vec!["bbbb".into(); 4],
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
                reflection: None,
                link: None,
                glass: [0.0; 4],
                bot: None,
            },
        )]
        .into(),
    }
}

fn manifest(id: &str, capabilities: &[&str], provides: &[(&str, &str, &str)]) -> String {
    serde_json::json!({
        "schema_version": 1, "id": id, "version": "1.0.0", "api": 1, "name": id,
        "license": "CC0-1.0", "capabilities": capabilities,
        "provides": provides.iter().map(|(kind, name, file)| serde_json::json!({
            "kind": kind, "id": format!("{id}:{kind}/{name}"), "file": file
        })).collect::<Vec<_>>(),
    })
    .to_string()
}

const SCRIPT: &str = r#"
fn generate_chunk(cx, cz) {
    let voxels = [];
    for x in 0..8 {
        for z in 0..8 {
            voxels.push([cx * 8 + x, 0, cz * 8 + z, 0]);
        }
    }
    voxels
}
// The dig tool: the first hit starts a crack, the second leaves it cracked,
// the third digs the block out.
fn cmd_dig(player) {
    let hit = aim();
    if hit == () || hit.brick == () || hit.block == () {
        return;
    }
    switch hit.state {
        "" => set_block_state(hit.brick, "cracking"),
        "cracking" => set_block_state(hit.brick, "cracked"),
        _ => remove_brick(hit.brick),
    }
}
fn cmd_melt(player) {
    let hit = aim();
    set_block_state(hit.brick, "melting");
}
"#;

const BLOCK: &str = r#"{
  "schema_version": 1,
  "name": "Glow Ore",
  "faces": {
    "top": "glow-look:texture/top",
    "bottom": "glow-look:texture/bottom",
    "side": "glow-look:texture/side"
  },
  "states": {
    "cracking": {
      "top": { "frames": ["glow-look:texture/crack0", "glow-look:texture/crack1", "glow-look:texture/crack2"], "fps": 8, "once": true }
    },
    "cracked": { "all": { "frames": ["glow-look:texture/crack2", "glow-look:texture/side"], "fps": 2 } }
  }
}"#;

/// Write the packages and load them; `.png` files are given as hex.
fn packages(root: &Path, block: &str) -> Result<Catalog, Vec<bri_package::diag::Diagnostic>> {
    let _ = std::fs::remove_dir_all(root);
    let world = serde_json::json!({
        "schema_version": 1, "generate": "generate_chunk", "chunk_voxels": 8, "voxel_size": 2.0,
        "voxel_brick": CUBE, "view_chunks": 1, "radius_chunks": 1, "seed": 1,
        "materials": [
            { "id": "glow:material/ore", "name": "Glow ore", "color": [0.3, 0.9, 0.5, 1.0], "block": "glow-look:block/ore" }
        ]
    })
    .to_string();
    let server = manifest(
        "glow",
        &["world.edit"],
        &[
            ("behaviour", "dig", "behaviour.json"),
            ("script", "dig", "dig.rhai"),
            ("world", "cave", "world.json"),
        ],
    );
    let textures = ["top", "bottom", "side", "crack0", "crack1", "crack2"];
    let mut provides: Vec<(&str, &str, String)> = textures
        .iter()
        .map(|t| ("texture", *t, format!("{t}.png")))
        .collect();
    provides.push(("block", "ore", "ore.json".into()));
    let provides: Vec<(&str, &str, &str)> = provides
        .iter()
        .map(|(k, n, f)| (*k, *n, f.as_str()))
        .collect();
    let look = manifest("glow-look", &[], &provides);
    let behaviour = r#"{ "schema_version": 1, "script": "dig.rhai",
      "commands": [{ "name": "dig", "aim_reach": 8.0 }, { "name": "melt", "aim_reach": 8.0 }] }"#;
    let mut files: Vec<(String, Vec<u8>)> = vec![
        ("glow/package.json".into(), server.into_bytes()),
        ("glow/behaviour.json".into(), behaviour.into()),
        ("glow/dig.rhai".into(), SCRIPT.into()),
        ("glow/world.json".into(), world.into_bytes()),
        ("glow-look/package.json".into(), look.into_bytes()),
        ("glow-look/ore.json".into(), block.into()),
    ];
    let png: Vec<u8> = (0..PNG.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&PNG[i..i + 2], 16).unwrap())
        .collect();
    for t in textures {
        files.push((format!("glow-look/{t}.png"), png.clone()));
    }
    for (path, bytes) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    let entry = |id: &str, side| PackageEntry {
        id: id.into(),
        version: "1.0.0".into(),
        side,
        dir: id.into(),
        role: None,
    };
    Catalog::load(
        root,
        &PackageSet {
            schema_version: 1,
            packages: vec![
                entry("glow", Side::Server),
                entry("glow-look", Side::Client),
            ],
        },
        true,
    )
}

fn dig(s: &mut Session, player: u64, sequence: u64, command: &str) -> anyhow::Result<()> {
    s.command_with_aim(
        player,
        sequence,
        Command::Package(PackageCommand {
            package: "glow".into(),
            command: command.into(),
            args: vec![],
        }),
        DOWN,
    )
    .map(|_| ())
}

fn looked_at(s: &Session, player: u64) -> Option<(u64, Option<BlockLook>)> {
    let feet = glam::Vec3::from(
        s.snapshot()
            .players
            .iter()
            .find(|p| p.owner == player)?
            .feet,
    );
    s.simulation()
        .state()
        .bricks
        .iter()
        .filter(|(_, b)| (glam::Vec3::from(b.position) - feet).truncate().length() < 1.5)
        .min_by(|(_, a), (_, b)| {
            let d = |p: [f32; 3]| (glam::Vec3::from(p) - feet).length();
            d(a.position).total_cmp(&d(b.position))
        })
        .map(|(id, b)| (*id, b.look.as_deref().cloned()))
}

#[test]
fn a_mod_cube_has_its_own_faces_and_a_dig_tool_cracks_it() {
    let root = std::env::temp_dir().join(format!("bri-blocks-{}", std::process::id()));
    let catalog = packages(&root, BLOCK).unwrap_or_else(|e| panic!("{e:#?}"));
    let _ = std::fs::remove_dir_all(&root);

    // The block's faces: the most specific face wins, states fall back to
    // the block's own faces, and flipbooks advance with time.
    let block = catalog.block("glow-look:block/ore").unwrap();
    let face = |face: &str, state: &str, seconds: f32| {
        block.look(face, state).unwrap().frame(seconds).to_string()
    };
    assert_eq!(face("top", "", 0.0), "glow-look:texture/top");
    assert_eq!(face("north", "", 0.0), "glow-look:texture/side");
    assert_eq!(face("bottom", "", 0.0), "glow-look:texture/bottom");
    assert_eq!(face("top", "cracking", 0.0), "glow-look:texture/crack0");
    assert_eq!(face("top", "cracking", 0.2), "glow-look:texture/crack1");
    assert_eq!(
        face("top", "cracking", 9.0),
        "glow-look:texture/crack2",
        "once holds the last frame"
    );
    assert_eq!(
        face("east", "cracking", 0.0),
        "glow-look:texture/side",
        "unchanged faces stay"
    );
    assert_eq!(
        face("east", "cracked", 0.6),
        "glow-look:texture/side",
        "a loop wraps"
    );
    assert!(matches!(
        block.look("top", "cracked"),
        Some(FaceLook::Flipbook(_))
    ));
    assert_eq!(
        catalog.texture("glow-look:texture/crack1").unwrap().width,
        1
    );

    let world = World::new("Glow".into(), "glow".into(), vec![[1.0; 4]]);
    let mut s = Session::new(Simulation::new(world, definitions(), vec![]).unwrap());
    let spawns = s.install_packages(Arc::new(catalog), None).unwrap();
    let p = s.join("Digger".into(), spawns[0], false).unwrap();
    for _ in 0..60 {
        s.step().unwrap();
    }
    // Generated voxels of the material show its block.
    let (brick, look) = looked_at(&s, p).expect("standing on the generated floor");
    assert_eq!(
        look,
        Some(BlockLook {
            block: "glow-look:block/ore".into(),
            state: String::new()
        })
    );
    // The dig tool cracks it; each state is on the brick, so it replicates
    // and saves with the world.
    let mut sequence = 0;
    for expected in ["cracking", "cracked"] {
        sequence += 1;
        dig(&mut s, p, sequence, "dig").unwrap_or_else(|e| panic!("{e:#}"));
        let b = &s.simulation().state().bricks[&brick];
        assert_eq!(b.look.as_ref().unwrap().state, expected);
    }
    // A state the block does not declare is refused, with the reason.
    sequence += 1;
    dig(&mut s, p, sequence, "melt").unwrap_or_else(|e| panic!("{e:#}"));
    assert!(
        s.package_diagnostics()
            .iter()
            .any(|d| d.code == "op.failed" && d.message.contains("no state `melting`")),
        "{:#?}",
        s.package_diagnostics()
    );
    assert_eq!(
        s.simulation().state().bricks[&brick]
            .look
            .as_ref()
            .unwrap()
            .state,
        "cracked"
    );
    // The third hit digs it out.
    for _ in 0..30 {
        s.step().unwrap();
    }
    sequence += 1;
    dig(&mut s, p, sequence, "dig").unwrap_or_else(|e| panic!("{e:#}"));
    assert!(!s.simulation().state().bricks.contains_key(&brick));
}

#[test]
fn a_block_naming_a_texture_no_package_provides_is_refused() {
    let root = std::env::temp_dir().join(format!("bri-blocks-missing-{}", std::process::id()));
    let broken = BLOCK.replace("glow-look:texture/crack1", "glow-look:texture/crack9");
    let problems = packages(&root, &broken).unwrap_err();
    let _ = std::fs::remove_dir_all(&root);
    assert!(
        problems.iter().any(|d| d.code == "set.texture.unknown"),
        "{problems:#?}"
    );
}
