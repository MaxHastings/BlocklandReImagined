//! The importer reads base datablocks from the installed game (`--installed`),
//! as players' Import does, which has no v20 install or recovered scripts:
//! a brick inheriting a stock brick keeps the stock brick's fields, and an
//! image naming a stock sound is no unknown. Everything here is written by
//! the test; all of it is ours (CC0).
use bri_addon_import::{Options, import};
use std::path::{Path, PathBuf};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bri-installed-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// A content root holding only a stock brick catalog and a sound list.
fn installed_game() -> PathBuf {
    let root = temp("content");
    write(
        &root.join("packages.json"),
        r#"{ "schema_version": 1, "packages": [
             { "id": "v20-bricks", "version": "4.0.0", "side": "shared", "dir": "bricks", "role": "brick_catalog" },
             { "id": "v20-audio", "version": "1.0.0", "side": "shared", "dir": "audio", "role": "audio" } ] }"#,
    );
    let entry = serde_json::json!({
        "id": "v20/brick/brick1x1data", "display_name": "1x1", "category": "Bricks",
        "subcategory": "1x", "mesh_id": "v20/base/data/bricks/bricks/1x1.blb",
        "collision_source": null, "icon_source": "base/client/ui/brickicons/1x1",
        "print_aspect_ratio": null, "orientation_fix": 0, "can_cover": true,
        "indestructible": false, "special_kind": null, "other_properties": {}
    });
    write(
        &root.join("bricks/stock-catalog.json"),
        &serde_json::json!({ "schema_version": 1, "bricks": [entry] }).to_string(),
    );
    write(
        &root.join("audio/manifest.json"),
        r#"{ "sounds": [ { "name": "weaponSwitchSound", "package": "base" } ] }"#,
    );
    root
}

fn addon() -> PathBuf {
    let dir = temp("addon").join("Brick_Test_Dirt");
    write(&dir.join("server.cs"), "exec(\"./Bricks.cs\");\n");
    write(
        &dir.join("Bricks.cs"),
        r#"datablock fxDTSBrickData (brick1x1TestDirtData : brick1x1Data)
{
	brickFile = "./Dirt.blb";
	category = "Dirt";
	uiName = "1x1 Test Dirt";
	isTestDirt = 1;
};
"#,
    );
    write(&dir.join("Dirt.blb"), "1 1 3\nBRICK\n");
    write(
        &dir.join("description.txt"),
        "Title: Test Dirt\nAuthor: Tester\nA brick.\n",
    );
    dir
}

#[test]
fn a_brick_inherits_a_stock_brick_from_the_installed_game() {
    let installed = installed_game();
    let run = |installed: Option<PathBuf>, name: &str| {
        import(&Options {
            input: addon(),
            out: temp(name).join("out"),
            installed,
            ..Default::default()
        })
        .unwrap()
    };
    // Without the installed game the parent is unknown and the brick is lost.
    let bare = serde_json::to_value(run(None, "bare")).unwrap();
    assert!(
        bare["unsupported"]
            .to_string()
            .contains("Unknown parent brick1x1data"),
        "{}",
        bare["unsupported"]
    );
    let out = temp("with").join("out");
    let report = import(&Options {
        input: addon(),
        out: out.clone(),
        installed: Some(installed),
        ..Default::default()
    })
    .unwrap();
    let r = serde_json::to_value(&report).unwrap();
    assert!(
        !r["unsupported"]
            .to_string()
            .contains("brick1x1TestDirtData"),
        "{}",
        r["unsupported"]
    );
    let block = r["datablocks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"] == "brick1x1TestDirtData")
        .unwrap()
        .clone();
    assert_eq!(block["status"], "converted", "{block}");
    // Its own fields win; the rest come from the stock 1x1.
    let catalog: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("assets/bricks.json")).unwrap()).unwrap();
    let brick = &catalog["bricks"][0];
    assert_eq!(brick["display_name"], "1x1 Test Dirt");
    assert_eq!(brick["category"], "Dirt");
    assert_eq!(brick["subcategory"], "1x");
    assert_eq!(brick["icon_source"], "base/client/ui/brickicons/1x1");
    assert_eq!(brick["other_properties"]["istestdirt"], "1");
    assert!(
        brick["mesh_id"]
            .as_str()
            .unwrap()
            .starts_with("brick_test_dirt:brick_geometry/"),
        "{brick}"
    );
}

/// A model material an Add-On does not carry (Loz's Hookshot leans on the
/// base game's `blank` and `black50`) draws with the installed game's
/// texture of that name, by the key players' games load it by.
#[test]
fn a_missing_material_draws_with_the_installed_games_texture() {
    let root = temp("presentation");
    write(
        &root.join("packages.json"),
        r#"{ "schema_version": 1, "packages": [
             { "id": "v20-item-presentation", "version": "1.0.0", "side": "shared", "dir": "items", "role": "item_presentation" } ] }"#,
    );
    write(
        &root.join("items/presentation.json"),
        r#"{ "schema_version": 2, "textures": {
             "base/data/shapes/black50.png": { "file": "textures/a.png", "sha256": "", "width": 1, "height": 1, "source": "base/data/shapes/black50.png" },
             "base/data/shapes/blank.png": { "file": "textures/b.png", "sha256": "", "width": 1, "height": 1, "source": "base/data/shapes/blank.png" } } }"#,
    );
    let mut reference = bri_addon_import::reference::Reference::core_only(&[]).unwrap();
    assert_eq!(
        reference.base_texture("black50"),
        None,
        "not without the game"
    );
    reference.add_installed(&root).unwrap();
    assert_eq!(
        reference.base_texture("Black50").as_deref(),
        Some("base/data/shapes/black50.png")
    );
    assert_eq!(
        reference.base_texture("blank").as_deref(),
        Some("base/data/shapes/blank.png")
    );
    assert_eq!(reference.base_texture("hookshotmetal"), None);
}
