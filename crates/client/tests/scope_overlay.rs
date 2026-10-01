//! A scope picture (`Zoom::overlay`) an Add-On brings loads the way a game
//! with it on does, and the HUD hands it to the play screen while aiming.
//! Content-free: the base game is the one-weapon sample pack, the Add-On a
//! copy of it with a scope and a picture drawn here.
use anyhow::{Context, Result};
use bri_client::{item_ui::ItemUi, items::ItemAssets};
use bri_net::content_identity::WeaponContent;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const SCOPED: &str = "scope-test:weapon/bubble_blaster";
const IMAGE: &str = "scope-test:image/bubble_blaster";
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

/// An Add-On: the sample blaster under its own ids, aimed through a scope
/// with `overlay` as its picture, and `picture` (if any) written as
/// `<overlay>.png` beside its weapons.
fn scoped_add_on(root: &Path, overlay: &str, picture: Option<(u32, u32)>) -> Result<PathBuf> {
    let dir = root.join("scope-test/assets");
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/samples/sample-bubble-blaster/assets/weapons.json");
    let text = std::fs::read_to_string(source)?.replace("sample-bubble-blaster", "scope-test");
    let mut pack: Value = serde_json::from_str(&text)?;
    pack["items"][SCOPED]["ui_name"] = json!("Scoped Blaster");
    pack["images"][IMAGE]["zoom"] = json!({ "fov": 22.0, "on_jet": true, "overlay": overlay });
    write_json(&dir.join("weapons.json"), &pack)?;
    if let Some((width, height)) = picture {
        // A clear lens in a black surround.
        let lens = image::RgbaImage::from_fn(width, height, |x, y| {
            let (dx, dy) = (
                x as f32 - width as f32 / 2.0,
                y as f32 - height as f32 / 2.0,
            );
            let inside = dx * dx + dy * dy < (height as f32 / 3.0).powi(2);
            image::Rgba([0, 0, 0, if inside { 0 } else { 255 }])
        });
        let file = dir.join(format!("{overlay}.png"));
        std::fs::create_dir_all(file.parent().context("parent")?)?;
        lens.save(file)?;
    }
    Ok(dir.canonicalize()?)
}

#[test]
fn an_add_ons_scope_picture_reaches_the_play_screen() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (weapons_root, items_root) = base_game(root.path())?;
    let dir = scoped_add_on(root.path(), "scope/lens", Some((128, 64)))?;
    let extras = vec![("scope-test/assets".to_string(), dir)];
    let weapons = WeaponContent::load_with(&weapons_root, &extras)?;
    let assets = ItemAssets::load_with(&items_root, &weapons_root, &extras)?;
    let overlay = assets.presentation.images[IMAGE]
        .overlay
        .as_deref()
        .context("the scope has a picture")?;
    let picture = assets.texture(overlay).context("scope picture loaded")?;
    assert_eq!((picture.width, picture.height), (128, 64));
    let at = |x: u32, y: u32| picture.rgba[((y * 128 + x) * 4 + 3) as usize];
    assert_eq!(at(64, 32), 0, "clear glass");
    assert_eq!(at(1, 1), 255, "black outside the lens");

    let ui = ItemUi::new(
        &assets,
        &weapons.item_choices,
        &bri_ui::pack::Pack::from_parts(Default::default(), PathBuf::new()),
    )?;
    assert!(ui.catalog().contains_key(SCOPED));
    let (_, aspect) = ui.scope_overlay(IMAGE).context("overlay registered")?;
    assert_eq!(aspect, 2.0);
    Ok(())
}

/// A picture the Add-On does not have leaves the plain zoom, and says so.
#[test]
fn a_missing_scope_picture_aims_without_one() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (weapons_root, items_root) = base_game(root.path())?;
    let dir = scoped_add_on(root.path(), "scope/lens", None)?;
    let extras = vec![("scope-test/assets".to_string(), dir)];
    let assets = ItemAssets::load_with(&items_root, &weapons_root, &extras)?;
    assert!(assets.presentation.images[IMAGE].overlay.is_none());
    assert!(
        assets
            .faults
            .iter()
            .any(|f| f.contains("scope overlay scope/lens.png")),
        "{:?}",
        assets.faults
    );
    Ok(())
}
