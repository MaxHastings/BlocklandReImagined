//! The importer reads base datablocks from the installed game (`--installed`),
//! as players' Import does, which has no v20 install or recovered scripts:
//! a brick inheriting a stock brick keeps the stock brick's fields, and an
//! image naming a stock sound is no unknown. Everything here is written by
//! the test; all of it is ours (CC0).
use bri_addon_import::{Options, import};
use std::path::{Path, PathBuf};

/// A fresh folder of its own for each call: tests run in parallel, and
/// several build the same installed game or Add-On.
fn temp(name: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("bri-installed-{}-{n}-{name}", std::process::id()));
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

/// A model material or particle texture an Add-On does not carry (Loz's
/// Hookshot leans on the base game's `blank` and `black50`) draws with the
/// installed game's texture of that name, by the key players' games load
/// it by.
#[test]
fn a_missing_material_draws_with_the_installed_games_texture() {
    let root = temp("presentation");
    write(
        &root.join("packages.json"),
        r#"{ "schema_version": 1, "packages": [
             { "id": "v20-item-presentation", "version": "1.0.0", "side": "shared", "dir": "items", "role": "item_presentation" },
             { "id": "v20-effects", "version": "1.0.0", "side": "shared", "dir": "effects", "role": "effects" } ] }"#,
    );
    write(
        &root.join("effects/effects.json"),
        r#"{ "textures": { "base/data/particles/dot": "dot.png" }, "particles": [], "emitters": [] }"#,
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
    // A particle drawing the base game's dot (the Grapple Rope's chain
    // trail) finds it in the installed effects.
    assert!(reference.has_file("base/data/particles/dot").is_some());
    assert!(reference.has_file("base/data/particles/nothing").is_none());
}

/// v20 force-loads a required Add-On the player had turned off, and a
/// script may then hide what it adds (`BowItem.uiName = "";`). Turning a
/// package on here turns what it needs on with it, so that branch never
/// runs: it is noted, not a gap.
#[test]
fn hiding_a_force_loaded_add_on_is_not_a_gap() {
    let dir = temp("forced").join("Tool_Test_Forced");
    write(
        &dir.join("server.cs"),
        r#"%result = ForceRequiredAddOn("Weapon_Test");
if($Error::AddOn_Disabled == %result)
{
	TestItem.uiName = "";
}
if(%result == $Error::AddOn_Disabled)
	TestImage.color = "1 1 1 1";
OtherItem.uiName = "Shown";
"#,
    );
    write(
        &dir.join("description.txt"),
        "Title: Forced\nAuthor: Tester\nA test.\n",
    );
    let report = import(&Options {
        input: dir,
        out: temp("forced-out").join("out"),
        ..Default::default()
    })
    .unwrap();
    let r = serde_json::to_value(&report).unwrap();
    let unsupported = r["unsupported"].to_string();
    assert!(
        !unsupported.contains("TestItem") && !unsupported.contains("TestImage"),
        "{unsupported}"
    );
    assert!(
        unsupported.contains("OtherItem.uiName"),
        "a write that always runs stays a gap: {unsupported}"
    );
    let ambiguous = r["ambiguous"].as_array().unwrap();
    for what in ["TestItem.uiName", "TestImage.color"] {
        let note = ambiguous
            .iter()
            .find(|a| a["what"].as_str().unwrap().starts_with(what))
            .unwrap_or_else(|| panic!("{what}: {ambiguous:?}"));
        assert!(
            note["resolution"].as_str().unwrap().contains("Weapon_Test"),
            "{note}"
        );
    }
}

/// An Add-On's `AudioProfile` may name a file of the game itself rather
/// than its own (the HE Grenade's explosion sound is
/// `base/data/sound/vehicleExplosion.wav`): Torque plays the game's file,
/// so the import keeps the sound, naming that file, and the explosion and
/// the image state play it by its id.
#[test]
fn a_sound_naming_the_games_own_file_plays_that_file() {
    let root = temp("stock-sound-game");
    write(
        &root.join("packages.json"),
        r#"{ "schema_version": 1, "packages": [
             { "id": "v20-audio", "version": "1.0.0", "side": "shared", "dir": "audio", "role": "audio" } ] }"#,
    );
    write(
        &root.join("audio/manifest.json"),
        r#"{ "clips": [ { "id": "v20/clip/boom", "sources": [ { "virtual_path": "base/data/sound/testBoom.wav" } ] } ],
             "sounds": [ { "name": "testBoomSound", "package": "base" } ] }"#,
    );
    let dir = temp("stock-sound").join("Weapon_Test_Boom");
    write(&dir.join("server.cs"), "exec(\"./boom.cs\");\n");
    write(
        &dir.join("description.txt"),
        "Title: Boom\nAuthor: Tester\nA test.\n",
    );
    write(
        &dir.join("boom.cs"),
        r#"datablock AudioProfile(boomExplosionSound) { filename = "base/data/sound/TestBoom.wav"; description = AudioDefault3d; preload = false; };
datablock AudioProfile(boomGoneSound) { filename = "base/data/sound/notThere.wav"; description = AudioDefault3d; };
datablock ExplosionData(boomExplosion) { lifetimeMS = 150; soundProfile = boomExplosionSound; radiusDamage = 10; damageRadius = 3; };
datablock ProjectileData(boomProjectile) { explosion = boomExplosion; muzzleVelocity = 20; lifetime = 1000; };
datablock ItemData(boomItem) { shapeFile = "./boom.dts"; uiName = "Boom"; image = boomImage; };
datablock ShapeBaseImageData(boomImage)
{
   shapeFile = "./boom.dts"; item = boomItem; projectile = boomProjectile;
   stateName[0] = "Ready"; stateTransitionOnTriggerDown[0] = "Fire";
   stateName[1] = "Fire"; stateFire[1] = true; stateSound[1] = boomGoneSound;
   stateTimeoutValue[1] = 0.2; stateTransitionOnTimeout[1] = "Ready";
};
"#,
    );
    let out = temp("stock-sound-out").join("out");
    import(&Options {
        input: dir,
        out: out.clone(),
        installed: Some(root),
        ..Default::default()
    })
    .unwrap();
    let pack =
        bri_weapons::Pack::from_json(&std::fs::read(out.join("assets/weapons.json")).unwrap())
            .unwrap();
    let id = "weapon_test_boom:sound/boomexplosionsound";
    let sound = pack.sound(id).expect("the game's file is the sound's");
    assert!(sound.stock, "{sound:?}");
    assert_eq!(sound.file, "base/data/sound/testboom.wav");
    assert_eq!(pack.explosions["boomexplosion"].sound, id);
    // A file neither the Add-On nor the game has stays unconverted.
    assert!(pack.sound("weapon_test_boom:sound/boomgonesound").is_none());
}

/// A newer copy of a base Add-On declares the base game's brick again
/// beside its own new one. In v20 that changes the same datablock, so the
/// import adds only the new brick: a save's brick of the base name stays
/// the base game's, with its behaviour (a stock pumpkin keeps carving).
#[test]
fn a_base_brick_declared_again_stays_the_base_games() {
    let dir = temp("redeclare").join("Brick_Test_Newer");
    write(&dir.join("server.cs"), "exec(\"./Bricks.cs\");\n");
    write(
        &dir.join("Bricks.cs"),
        r#"datablock fxDTSBrickData (brick1x1Data)
{
	brickFile = "./One.blb";
	category = "Bricks";
	uiName = "1x1";
};
datablock fxDTSBrickData (brick1x1NewData)
{
	brickFile = "./One.blb";
	category = "Bricks";
	uiName = "1x1 New";
};
"#,
    );
    write(&dir.join("One.blb"), "1 1 3\nBRICK\n");
    write(
        &dir.join("description.txt"),
        "Title: Newer\nAuthor: Tester\n",
    );
    let out = temp("redeclare-out").join("out");
    let report = import(&Options {
        input: dir,
        out: out.clone(),
        installed: Some(installed_game()),
        ..Default::default()
    })
    .unwrap();
    let catalog: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("assets/bricks.json")).unwrap()).unwrap();
    let names: Vec<&str> = catalog["bricks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["display_name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["1x1 New"]);
    let r = serde_json::to_value(&report).unwrap();
    let base = r["datablocks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"] == "brick1x1Data")
        .unwrap()
        .clone();
    assert_eq!(base["status"], "consumed", "{base}");
}

/// A door's `isDoor` fields become a click swap between the Add-On's own
/// bricks, as Brick_Doors' script swaps the datablock: closed opens to
/// `openCW`/`openCCW`, open closes to `closedCW`/`closedCCW`.
#[test]
fn a_door_swaps_to_its_open_and_closed_bricks() {
    let dir = temp("door").join("Brick_Test_Door");
    write(&dir.join("server.cs"), "exec(\"./Bricks.cs\");\n");
    write(
        &dir.join("Bricks.cs"),
        r#"datablock fxDTSBrickData (brickTestDoorOpenData)
{
	brickFile = "./Door.blb";
	uiName = "Test Door Open";
	isDoor = 1;
	isOpen = 1;
	closedCW = "brickTestDoorData";
	openCW = "brickTestDoorOpenData";
	closedCCW = "brickTestDoorData";
	openCCW = "brickTestDoorOpenData";
};
datablock fxDTSBrickData (brickTestDoorData : brickTestDoorOpenData)
{
	uiName = "Test Door";
	isOpen = 0;
};
"#,
    );
    write(&dir.join("Door.blb"), "1 1 3\nBRICK\n");
    write(
        &dir.join("description.txt"),
        "Title: Door\nAuthor: Tester\n",
    );
    let out = temp("door-out").join("out");
    import(&Options {
        input: dir,
        out: out.clone(),
        ..Default::default()
    })
    .unwrap();
    let catalog: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("assets/bricks.json")).unwrap()).unwrap();
    let swap = |name: &str| {
        catalog["bricks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["display_name"] == name)
            .unwrap()["swap"]
            .clone()
    };
    let (open, closed) = (swap("Test Door Open"), swap("Test Door"));
    assert!(
        open["front"]
            .as_str()
            .unwrap()
            .ends_with("/bricktestdoordata"),
        "{open}"
    );
    assert!(
        closed["back"]
            .as_str()
            .unwrap()
            .ends_with("/bricktestdooropendata"),
        "{closed}"
    );
}
