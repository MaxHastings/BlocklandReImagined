//! The repository's own builds (`saves/<Map>/<name>.world.json`) ship in
//! the worlds pack: `import_saves --bundled` checks each holds only stock
//! bricks and copies it in unchanged, listed under `bundled`.
use std::path::{Path, PathBuf};

const STOCK: [&str; 3] = [
    "v20/brick/brick4xcubedata",
    "v20/brick/brickvehiclespawndata",
    "v20/brick/brick4x4fdata",
];

fn catalog(ids: &[&str]) -> serde_json::Value {
    let bricks: Vec<_> = ids
        .iter()
        .map(|id| {
            serde_json::json!({
                "id": id, "display_name": id, "category": "Baseplates",
                "subcategory": "Plain", "mesh_id": id, "collision_source": null,
                "icon_source": "", "print_aspect_ratio": null, "orientation_fix": 0,
                "can_cover": false, "indestructible": false, "special_kind": null,
                "other_properties": {}
            })
        })
        .collect();
    serde_json::json!({"schema_version": 1, "bricks": bricks})
}

fn repo_saves() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../saves")
}

/// Runs `import_saves` over no originals and the repository's builds,
/// against a catalog of `ids`, in a scratch folder named `tag`.
fn import(tag: &str, ids: &[&str]) -> (std::process::Output, PathBuf) {
    let scratch = std::env::temp_dir().join(format!("bri-bundled-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(scratch.join("originals")).unwrap();
    std::fs::write(scratch.join("catalog.json"), catalog(ids).to_string()).unwrap();
    let out = scratch.join("worlds");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_import_saves"))
        .arg(scratch.join("originals"))
        .arg(scratch.join("catalog.json"))
        .arg(&out)
        .arg("--bundled")
        .arg(repo_saves())
        .output()
        .unwrap();
    (output, out)
}

#[test]
fn the_repository_builds_ship_unchanged_in_the_worlds_pack() {
    let (output, out) = import("ok", &STOCK);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("report.json")).unwrap()).unwrap();
    let soccer = report["bundled"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["source"] == "Slate/Soccer 2v2.world.json")
        .expect("the soccer field ships")
        .clone();
    let original = std::fs::read(repo_saves().join("Slate/Soccer 2v2.world.json")).unwrap();
    let copied = std::fs::read(out.join(soccer["file"].as_str().unwrap())).unwrap();
    assert_eq!(copied, original, "copied as it is");
    let build = bri_world::build::decode(&copied).unwrap();
    assert_eq!(soccer["bricks"], build.world.bricks.len());
    // The originals' list is the originals' alone.
    assert!(report["saves"].as_array().unwrap().is_empty());
}

#[test]
fn a_build_with_a_brick_the_game_lacks_is_refused() {
    // A catalog without the vehicle spawn: the field would lose its pads.
    let (output, _) = import("missing", &[STOCK[0], STOCK[2]]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("not a stock brick"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
