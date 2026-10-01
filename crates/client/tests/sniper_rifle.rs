//! The Sniper Rifle Add-On's own art loads the way a game with it on does:
//! its rifle model (the receiver's bolt moves in the `Bolt` sequence), the
//! muzzle the shot leaves from, its HUD icon and the scope picture shown
//! while aiming, all from `packages/showcase/sniper-rifle/assets` and with
//! no stand-ins. Content-free: the base game is the one-weapon sample pack.
use anyhow::{Context, Result};
use bri_client::{item_ui::ItemUi, items::ItemAssets};
use bri_net::content_identity::WeaponContent;
use glam::Mat4;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const RIFLE: &str = "sniper-rifle:weapon/sniperrifle";
const IMAGE: &str = "sniper-rifle:image/sniperrifle";
const MODEL: &str = "sniper-rifle:model/rifle";
const PISTOL: &str = "add-ons/weapon_gun/pistol.dts";
const BULLET: &str = "add-ons/weapon_gun/bullet.dts";

fn write_json(path: &Path, value: &Value) -> Result<String> {
    let bytes = serde_json::to_vec_pretty(value)?;
    std::fs::create_dir_all(path.parent().context("parent")?)?;
    std::fs::write(path, &bytes)?;
    Ok(format!("{:x}", Sha256::digest(&bytes)))
}
fn shape(id: &str) -> Value {
    json!({
        "schema_version": 1, "id": id,
        "nodes": [{ "name": "root", "parent": null, "translation": [0.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0] }],
        "objects": [], "details": [], "meshes": [], "materials": [], "animations": [],
    })
}
fn model_entry(file: &str, sha: &str) -> Value {
    json!({
        "file": file, "sha256": sha, "source": "synthetic", "source_sha256": "0".repeat(64),
        "textures": [], "bounds_min": [-0.1, -0.1, -0.1], "bounds_max": [0.1, 0.1, 0.1],
    })
}

/// The base game: the sample Bubble Blaster with stand-in models.
fn base_game(root: &Path) -> Result<(PathBuf, PathBuf)> {
    let weapons = root.join("weapons");
    let items = root.join("items");
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/samples/sample-bubble-blaster/assets/weapons.json");
    let pack: Value = serde_json::from_slice(&std::fs::read(source)?)?;
    let weapons_sha = write_json(&weapons.join("weapons.json"), &pack)?;
    let pistol = write_json(&items.join("pistol.json"), &shape("pistol"))?;
    let bullet = write_json(&items.join("bullet.json"), &shape("bullet"))?;
    let item = "sample-bubble-blaster:weapon/bubble_blaster";
    let image_id = "sample-bubble-blaster:image/bubble_blaster";
    let image = &pack["images"][image_id];
    let bounds = json!({ "min": [-0.1, -0.1, -0.1], "max": [0.1, 0.1, 0.1] });
    let physics_sha = write_json(
        &items.join("item-physics.json"),
        &json!({ "schema_version": 1, "items": { item: bounds } }),
    )?;
    let evidence = json!({ "path": "synthetic", "sha256": "0".repeat(64), "line": 0 });
    write_json(
        &items.join("presentation.json"),
        &json!({
            "schema_version": 2, "id": "synthetic:item-presentation/main",
            "weapons_sha256": weapons_sha, "item_physics_sha256": physics_sha,
            "models": { PISTOL: model_entry("pistol.json", &pistol), BULLET: model_entry("bullet.json", &bullet) },
            "textures": {},
            "items": { item: { "model": PISTOL, "image": image_id, "tint": [1.0, 1.0, 1.0, 1.0], "icon": null, "evidence": evidence } },
            "images": { image_id: {
                "model": PISTOL, "mount_point": image["mount_point"], "offset": image["offset"],
                "eye_offset": image["eye_offset"], "source_rotation_degrees": image["source_rotation_degrees"],
                "eye_rotation_degrees": [0.0, 0.0, 0.0], "tint": [1.0, 1.0, 1.0, 1.0], "evidence": evidence,
            } },
            "projectiles": { "sample-bubble-blaster:projectile/bubble": { "model": BULLET, "tint": [1.0, 1.0, 1.0, 1.0] } },
            "diagnostics": [],
        }),
    )?;
    Ok((weapons, items))
}

#[test]
fn the_rifle_brings_its_own_model_icon_and_scope() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (weapons_root, items_root) = base_game(root.path())?;
    let assets_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/showcase/sniper-rifle/assets")
        .canonicalize()?;
    let extras = vec![("showcase/sniper-rifle/assets".to_string(), assets_dir)];
    let weapons = WeaponContent::load_with(&weapons_root, &extras)?;
    let assets = ItemAssets::load_with(&items_root, &weapons_root, &extras)?;
    let own: Vec<_> = assets
        .faults
        .iter()
        .filter(|f| f.contains("sniper-rifle"))
        // With no stock rocket launcher here to pose like, the icon is
        // its own picture rather than a render (the real game renders it).
        .filter(|f| !f.contains("rocketlauncheritem is not an item"))
        .collect();
    assert!(own.is_empty(), "no stand-ins: {own:?}");

    // Its own model, held at its grip.
    assert_eq!(assets.presentation.items[RIFLE].model, MODEL);
    assert_eq!(assets.presentation.images[IMAGE].model, MODEL);
    let shape = assets.shape(MODEL)?;
    assert!(!shape.meshes.is_empty());
    let rest = assets.pose(MODEL, None, 0.0)?;
    let muzzle = assets.node_transform(MODEL, &rest, Mat4::IDENTITY, "muzzlePoint")?;
    let grip = assets.node_transform(MODEL, &rest, Mat4::IDENTITY, "mountPoint")?;
    // The muzzle is well forward (-Z) of the hand.
    assert!(muzzle.w_axis.z < grip.w_axis.z - 1.0, "{muzzle:?} {grip:?}");
    // Working the bolt lifts and draws it back, then it comes home.
    let bolt = |seconds| -> Result<Mat4> {
        let pose = assets.pose(MODEL, Some("Bolt"), seconds)?;
        assets.node_transform(MODEL, &pose, Mat4::IDENTITY, "bolt")
    };
    let home = bolt(0.0)?;
    let back = bolt(0.45)?;
    assert!(
        back.w_axis.z > home.w_axis.z + 0.1,
        "drawn back: {home:?} {back:?}"
    );
    assert!(bolt(0.9)?.abs_diff_eq(home, 1e-4), "home again");

    // Its icon and the scope picture, square and full size.
    let icon = assets.icon(RIFLE)?.context("the rifle has an icon")?;
    assert!(icon.width >= 64 && icon.width == icon.height);
    let overlay = assets.presentation.images[IMAGE]
        .overlay
        .as_deref()
        .context("the scope has a picture")?;
    let picture = assets.texture(overlay).context("scope picture loaded")?;
    assert_eq!((picture.width, picture.height), (1024, 1024));
    // The middle of the lens is clear, its rim black.
    let at = |x: u32, y: u32| picture.rgba[((y * 1024 + x) * 4 + 3) as usize];
    assert_eq!(at(300, 300), 0, "clear glass");
    assert_eq!(at(2, 2), 255, "black outside the lens");

    // The HUD offers it, and hands the scope picture to the play screen.
    let ui = ItemUi::new(
        &assets,
        &weapons.item_choices,
        &bri_ui::pack::Pack::from_parts(Default::default(), PathBuf::new()),
    )?;
    assert!(ui.catalog().contains_key(RIFLE));
    let (_, aspect) = ui.scope_overlay(IMAGE).context("overlay registered")?;
    assert_eq!(aspect, 1.0);
    Ok(())
}
