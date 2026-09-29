//! Joining a server never fails over how its Add-Ons look. A host runs
//! Add-Ons whose item presentation is missing, names an icon that is not
//! there, points at a model file that is not there, or leaves a weapon out;
//! a clean client downloads them over loopback and loads the item art and
//! the item HUD the way a join does. Every weapon gets a HUD row, gaps show
//! stock art, no model or the item's first letter, and each stand-in is
//! logged naming its Add-On. A second player with an older copy of one of
//! those Add-Ons and one of their own joins too: the server's version
//! downloads, theirs sits the game out, and nothing asks. Content-free: the
//! base game here is a one-weapon synthetic pack.
use anyhow::{Context, Result};
use bri_client::{item_ui::ItemUi, items::ItemAssets};
use bri_net::content_identity::{ItemPhysicsContent, WeaponContent, kind_providers};
use bri_package::packages::PackageSet;
use bri_ui::api::IconRef;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

const PISTOL: &str = "add-ons/weapon_gun/pistol.dts";
const BULLET: &str = "add-ons/weapon_gun/bullet.dts";
const GUN_ICON: &str = "add-ons/weapon_gun/icon_gun.png";
const LETTERS: &str = "add-ons/print_letters_default/icons";

fn hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn write(path: &Path, bytes: &[u8]) -> Result<String> {
    std::fs::create_dir_all(path.parent().context("parent")?)?;
    std::fs::write(path, bytes)?;
    Ok(hex(bytes))
}
fn write_json(path: &Path, value: &Value) -> Result<(Vec<u8>, String)> {
    let bytes = serde_json::to_vec_pretty(value)?;
    let sha = write(path, &bytes)?;
    Ok((bytes, sha))
}
fn bubble() -> Result<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/samples/sample-bubble-blaster/assets/weapons.json");
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}
fn evidence() -> Value {
    json!({ "path": "synthetic", "sha256": "0".repeat(64), "line": 0 })
}
fn bounds() -> Value {
    json!({ "min": [-0.1, -0.1, -0.1], "max": [0.1, 0.1, 0.1] })
}
/// A one-node model with no geometry: enough to load and to bind.
fn shape(id: &str) -> Value {
    json!({
        "schema_version": 1, "id": id,
        "nodes": [{ "name": "root", "parent": null, "translation": [0.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0] }],
        "objects": [], "details": [], "meshes": [], "materials": [], "animations": [],
    })
}
fn png() -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    image::RgbaImage::from_pixel(2, 2, image::Rgba([200, 40, 40, 255]))
        .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)?;
    Ok(bytes)
}
fn model_entry(file: &str, sha: &str) -> Value {
    json!({
        "file": file, "sha256": sha, "source": "synthetic", "source_sha256": "0".repeat(64),
        "textures": [], "bounds_min": [-0.1, -0.1, -0.1], "bounds_max": [0.1, 0.1, 0.1],
    })
}

/// An Add-On weapons pack: one image with no projectile, and `items` as
/// (name, name players see, model, icon).
fn weapons(ns: &str, items: &[(&str, &str, &str, &str)]) -> Result<Value> {
    let base = bubble()?;
    let mut image = base["images"]["sample-bubble-blaster:image/bubble_blaster"].clone();
    let image_id = format!("{ns}:image/tool");
    image["id"] = json!(image_id);
    image["name"] = json!(format!("{ns}Image"));
    image["projectile"] = Value::Null;
    let mut pack = base.clone();
    pack["id"] = json!(format!("{ns}:weapons/main"));
    pack["images"] = json!({ image_id.clone(): image });
    pack["projectiles"] = json!({});
    pack["damage_types"] = json!({});
    pack["explosions"] = json!({});
    pack["items"] = json!({});
    for (name, ui_name, model, icon) in items {
        let mut item = base["items"]["sample-bubble-blaster:weapon/bubble_blaster"].clone();
        let id = format!("{ns}:weapon/{name}");
        item["id"] = json!(id);
        item["name"] = json!(format!("{name}Item"));
        item["ui_name"] = json!(ui_name);
        item["image"] = json!(image_id);
        item["model"] = json!(model);
        item["icon"] = json!(icon);
        pack["items"][&id] = item;
    }
    Ok(pack)
}

/// The base game: the Bubble Blaster with its pistol, bullet and icon.
struct Base {
    weapons: PathBuf,
    items: PathBuf,
}
fn base_game(root: &Path) -> Result<Base> {
    let weapons = root.join("weapons");
    let items = root.join("items");
    let pack = bubble()?;
    let (weapon_bytes, weapons_sha) = write_json(&weapons.join("weapons.json"), &pack)?;
    let _ = weapon_bytes;
    let pistol = write_json(&items.join("pistol.json"), &shape("pistol"))?.1;
    let bullet = write_json(&items.join("bullet.json"), &shape("bullet"))?.1;
    let icon = write(&items.join("icon_gun.png"), &png()?)?;
    let item = "sample-bubble-blaster:weapon/bubble_blaster";
    let image_id = "sample-bubble-blaster:image/bubble_blaster";
    let image = &pack["images"][image_id];
    let (_, physics_sha) = write_json(
        &items.join("item-physics.json"),
        &json!({ "schema_version": 1, "items": { item: bounds() } }),
    )?;
    write_json(
        &items.join("presentation.json"),
        &json!({
            "schema_version": 2, "id": "synthetic:item-presentation/main",
            "weapons_sha256": weapons_sha, "item_physics_sha256": physics_sha,
            "models": { PISTOL: model_entry("pistol.json", &pistol), BULLET: model_entry("bullet.json", &bullet) },
            "textures": { GUN_ICON: { "file": "icon_gun.png", "sha256": icon, "width": 2, "height": 2, "source": "synthetic" } },
            "items": { item: { "model": PISTOL, "image": image_id, "tint": [1.0, 1.0, 1.0, 1.0], "icon": GUN_ICON, "evidence": evidence() } },
            "images": { image_id: {
                "model": PISTOL, "mount_point": image["mount_point"], "offset": image["offset"],
                "eye_offset": image["eye_offset"], "source_rotation_degrees": image["source_rotation_degrees"],
                "eye_rotation_degrees": [0.0, 0.0, 0.0], "tint": [1.0, 1.0, 1.0, 1.0], "evidence": evidence(),
            } },
            "projectiles": { "sample-bubble-blaster:projectile/bubble": { "model": BULLET, "tint": [1.0, 1.0, 1.0, 1.0] } },
            "diagnostics": [],
        }),
    )?;
    Ok(Base { weapons, items })
}

/// An Add-On's own presentation: (item name, model key, icon key) per
/// presented item, then its model and texture entries.
type Own<'a> = (&'a [(&'a str, &'a str, &'a str)], Value, Value);

/// Write the Add-On `id` under `root/addons/<id>`: its manifest, weapons
/// pack and, when `own` is given, its own presentation of the items it
/// names as (item name, model key, icon key) with extra model and texture
/// entries (whose files may be missing).
fn add_on(
    root: &Path,
    id: &str,
    items: &[(&str, &str, &str, &str)],
    own: Option<Own>,
) -> Result<()> {
    let dir = root.join("addons").join(id);
    write_json(
        &dir.join("package.json"),
        &json!({
            "schema_version": 1, "id": id, "version": "1.0.0", "api": 1, "name": id,
            "license": "CC0-1.0", "provenance": { "source": "original" },
            "provides": [{ "kind": "weapons", "id": format!("{id}:weapons/main"), "file": "assets/weapons.json" }],
        }),
    )?;
    let assets = dir.join("assets");
    let pack = weapons(id, items)?;
    let (_, weapons_sha) = write_json(&assets.join("weapons.json"), &pack)?;
    let Some((presented, models, textures)) = own else {
        return Ok(());
    };
    let image_id = format!("{id}:image/tool");
    let image = &pack["images"][&image_id];
    let mut physics = json!({});
    let mut entries = json!({});
    for (name, model, icon) in presented {
        let item = format!("{id}:weapon/{name}");
        physics[&item] = bounds();
        entries[&item] = json!({
            "model": model, "image": image_id, "tint": [1.0, 1.0, 1.0, 1.0],
            "icon": (!icon.is_empty()).then_some(icon), "evidence": evidence(),
        });
    }
    let model = presented.first().map_or(PISTOL, |p| p.1);
    let (_, physics_sha) = write_json(
        &assets.join("item-physics.json"),
        &json!({ "schema_version": 1, "items": physics }),
    )?;
    write_json(
        &assets.join("presentation.json"),
        &json!({
            "schema_version": 2, "id": format!("{id}:item-presentation/main"),
            "weapons_sha256": weapons_sha, "item_physics_sha256": physics_sha,
            "models": models, "textures": textures, "items": entries,
            "images": { image_id.clone(): {
                "model": model, "mount_point": image["mount_point"], "offset": image["offset"],
                "eye_offset": image["eye_offset"], "source_rotation_degrees": image["source_rotation_degrees"],
                "eye_rotation_degrees": [0.0, 0.0, 0.0], "tint": [1.0, 1.0, 1.0, 1.0], "evidence": evidence(),
            } },
            "projectiles": {}, "diagnostics": [],
        }),
    )?;
    Ok(())
}

/// The four broken Add-Ons of this test, listed in a package set.
fn server_add_ons(root: &Path) -> Result<PackageSet> {
    // No presentation at all: stock art where the base game has it.
    add_on(
        root,
        "stock-art",
        &[
            ("wand", "Stock Wand", "Add-Ons/Weapon_Gun/pistol.dts", "Add-Ons/Weapon_Gun/icon_gun"),
            ("mystery", "Mystery Box", "Add-Ons/Nope/box.dts", "Add-Ons/Nope/icon_box"),
        ],
        None,
    )?;
    // Its icon is listed but the file is missing.
    add_on(
        root,
        "missing-icon",
        &[("sparkle", "Sparkle Gun", "Add-Ons/Weapon_Gun/pistol.dts", "icons/sparkle")],
        Some((
            &[("sparkle", PISTOL, "icons/sparkle.png")],
            json!({}),
            json!({ "icons/sparkle.png": { "file": "icons/sparkle.png", "sha256": "1".repeat(64), "width": 2, "height": 2, "source": "synthetic" } }),
        )),
    )?;
    // Its model is listed but the file is missing.
    add_on(
        root,
        "bad-model",
        &[("broken", "Broken Model Gun", "Add-Ons/Bad/model.dts", "")],
        Some((
            &[("broken", "add-ons/bad/model.dts", "")],
            json!({ "add-ons/bad/model.dts": model_entry("models/missing.json", &"2".repeat(64)) }),
            json!({}),
        )),
    )?;
    // Presents one of its two weapons.
    add_on(
        root,
        "extra-weapon",
        &[
            ("kept", "Presented Gun", "Add-Ons/Weapon_Gun/pistol.dts", "Add-Ons/Weapon_Gun/icon_gun"),
            ("spare", "Spare Blaster", "Add-Ons/Weapon_Gun/pistol.dts", "Add-Ons/Weapon_Gun/icon_gun"),
        ],
        Some((&[("kept", PISTOL, GUN_ICON)], json!({}), json!({}))),
    )?;
    let packages: Vec<Value> = ["stock-art", "missing-icon", "bad-model", "extra-weapon"]
        .iter()
        .map(|id| json!({ "id": id, "version": "1.0.0", "side": "shared", "dir": format!("addons/{id}") }))
        .collect();
    PackageSet::parse(&serde_json::to_vec(
        &json!({ "schema_version": 1, "packages": packages }),
    )?)
}

/// The synthetic host session the network tests use.
fn session() -> bri_sim::session::Session {
    use bri_content::{
        brick::Brick as Mesh,
        collision::{CollisionBody, Part},
    };
    use bri_sim::definitions::{Definition, Definitions};
    use rapier3d::prelude::*;
    let mesh = Mesh {
        schema_version: 1,
        id: "plate".into(),
        footprint_studs: [2, 1],
        height_plates: 1,
        attachment_rows: vec!["bb".into()],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![],
    };
    let collision = CollisionBody {
        id: "plate".into(),
        parts: vec![Part::Box {
            center: [0.0; 3],
            size: [1.0, 0.2, 0.5],
        }],
    };
    let shape = bri_physics::content::collider(&collision)
        .unwrap()
        .build()
        .shared_shape()
        .clone();
    let definitions = Definitions {
        entries: [(
            "plate".into(),
            Definition {
                mesh,
                collision,
                shape,
                indestructible: false,
                special: Default::default(),
            },
        )]
        .into(),
    };
    let mut session = bri_sim::session::Session::new(
        bri_sim::simulation::Simulation::new(
            bri_world::World::new("Fallbacks".into(), "fixture".into(), vec![[1.0; 4]]),
            definitions,
            vec![ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0))],
        )
        .unwrap(),
    );
    session
        .set_event_catalog(bri_events::testing::catalog(), Vec::new())
        .unwrap();
    session
}

fn letters() -> Result<bri_ui::pack::Pack> {
    let mut data = bri_ui::schema::UiPack::default();
    for letter in 'a'..='z' {
        data.images.insert(
            format!("{LETTERS}/{letter}"),
            serde_json::from_value(json!({
                "file": format!("{letter}.png"), "width": 2, "height": 2,
                "sha256": "0".repeat(64), "source": "synthetic",
            }))?,
        );
    }
    Ok(bri_ui::pack::Pack::from_parts(data, PathBuf::new()))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn joining_downloads_the_servers_add_ons_and_loads_bad_art_with_stand_ins() -> Result<()> {
    let server_root = tempfile::tempdir()?;
    let set = server_add_ons(server_root.path())?;
    let environment = bri_package::environment::Environment::load(server_root.path(), &set)?;
    let shelf = bri_net::packages::PackageShelf::new(server_root.path(), &set, &environment)?;
    let server = bri_net::server::start(
        session(),
        bri_net::server::ServerOptions {
            bind: "127.0.0.1:0".parse()?,
            environment: environment.clone(),
            spawn_points: vec![glam::Vec3::new(0.0, 0.05, 0.0); 4],
            certificate: None,
            map_loader: None,
            autosave: None,
            packages: Some(Arc::new(shelf)),
        },
    )?;

    // A clean client: the base game only, downloads into its `.downloads`.
    let client_root = tempfile::tempdir()?;
    let base = base_game(client_root.path())?;
    let identity =
        bri_identity::ClientIdentity::load_or_create(client_root.path().join("client.identity"))?;
    let cache = bri_package::sync::Cache::open(&client_root.path().join(".downloads"))?;
    let own = PackageSet {
        schema_version: 1,
        packages: Vec::new(),
    };
    let (client, fetched, dropped) = bri_net::client::Client::connect_fetching(
        server.address,
        bri_net::client::HostPin::from(&server.certificate[..]),
        "Joiner".into(),
        Vec::new(),
        None,
        &identity,
        &cache,
        bri_progress::Progress::default(),
        |fetched, dropped| {
            Ok(bri_client::mods::load_fetched(client_root.path(), &own, &[], fetched, dropped)?.1)
        },
    )
    .await?;
    assert!(client.owner > 0, "joined");
    assert_eq!(fetched.len(), 4, "every Add-On downloaded");
    assert!(dropped.is_empty());
    drop(client);

    // What the join then loads: the same extras `ContentPaths` resolves.
    let joined = bri_client::mods::joined_set(client_root.path(), &own, &fetched, &[])?;
    let extras = kind_providers(client_root.path(), &joined, "weapons.json")?;
    assert_eq!(extras.len(), 4);
    let weapons = WeaponContent::load_with(&base.weapons, &extras)?;
    let physics = ItemPhysicsContent::load_with(&base.items, &weapons, &extras)?;
    // Every weapon has drop bounds, so the wrench can put any of them on a
    // brick: none of these Add-Ons ships item physics. A stock model lends
    // its box; an item with no art gets the stand-in box.
    for id in weapons.pack.items.keys() {
        assert!(physics.bounds.contains_key(id), "{id} has bounds");
    }
    let stock = bri_weapons::ItemBounds {
        min: [-0.1; 3],
        max: [0.1; 3],
    };
    assert_eq!(physics.bounds["stock-art:weapon/wand"], stock);
    assert_eq!(
        physics.bounds["stock-art:weapon/mystery"],
        bri_weapons::ItemBounds::FALLBACK
    );
    let spawners = bri_sim::item_spawners::ItemSpawners::new(physics.bounds.clone());
    let world = bri_world::World::new("Wrench".into(), "test".into(), vec![[1.0; 4]]);
    let mut brick = bri_world::Brick::new(
        bri_world::ContentRef::Resolved("plate".into()),
        [0.0, 0.1, 0.0],
        1,
    );
    brick.item_spawn.item = Some(bri_world::ContentRef::Resolved(
        "stock-art:weapon/mystery".into(),
    ));
    brick.item_spawn.respawn_ms = 1000;
    let edit = bri_world::authority::Edit::Properties(bri_world::authority::WrenchProperties {
        item_spawn: brick.item_spawn,
        ..Default::default()
    });
    spawners.validate_edit(&world, 1, &edit)?;
    let assets = ItemAssets::load_with(&base.items, &base.weapons, &extras)?;
    let ui = ItemUi::new(&assets, &weapons.item_choices, &letters()?)?;
    let catalog = ui.catalog();
    assert_eq!(weapons.item_choices.len(), 7);
    assert_eq!(catalog.len(), weapons.item_choices.len(), "a HUD row per weapon");
    for (id, _) in &weapons.item_choices {
        assert!(assets.presentation.items.contains_key(id), "{id} presented");
    }
    let row = |name: &str| {
        catalog
            .values()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("no HUD row for {name}"))
    };
    let model = |name: &str| assets.presentation.items[&row(name).id].model.clone();
    let letter = |l: &str| IconRef::Pack(format!("{LETTERS}/{l}"));
    // Stock art borrowed; the stock icon is an uploaded texture.
    assert!(matches!(row("Stock Wand").icon, IconRef::External(_)));
    assert_eq!(model("Stock Wand"), PISTOL);
    assert!(matches!(row("Spare Blaster").icon, IconRef::External(_)));
    assert_eq!(model("Spare Blaster"), PISTOL);
    assert!(matches!(row("Presented Gun").icon, IconRef::External(_)));
    // No art anywhere: no model and the first letter.
    assert_eq!(row("Mystery Box").icon, letter("m"));
    assert_eq!(model("Mystery Box"), "");
    assert_eq!(row("Sparkle Gun").icon, letter("s"));
    assert_eq!(model("Sparkle Gun"), PISTOL);
    assert_eq!(row("Broken Model Gun").icon, letter("b"));
    assert_eq!(model("Broken Model Gun"), "");
    // A model-less item draws nothing rather than failing.
    let mystery = assets.item_scene(&row("Mystery Box").id, glam::Mat4::IDENTITY)?;
    assert!(mystery.vertices.is_empty());
    // Each stand-in is logged naming its Add-On.
    let faults = assets.faults.join("\n");
    for add_on in ["stock-art", "missing-icon", "bad-model"] {
        assert!(faults.contains(&format!("Add-On {add_on}:")), "{add_on} not named in:\n{faults}");
    }
    assert!(!faults.contains("extra-weapon"), "borrowing stock art is no fault:\n{faults}");

    // A player with an older copy of one of the server's Add-Ons, and an
    // Add-On of their own the server does not run, joins the same way: the
    // server's version downloads (never the stale copy), their own sits the
    // game out, and the game runs exactly the server's Add-Ons.
    let stale_root = tempfile::tempdir()?;
    let stale_base = base_game(stale_root.path())?;
    add_on(
        stale_root.path(),
        "stock-art",
        &[("wand", "Stock Wand", "Add-Ons/Weapon_Gun/pistol.dts", "Add-Ons/Weapon_Gun/icon_gun")],
        None,
    )?;
    add_on(
        stale_root.path(),
        "mine-only",
        &[("toy", "Toy Gun", "Add-Ons/Weapon_Gun/pistol.dts", "Add-Ons/Weapon_Gun/icon_gun")],
        None,
    )?;
    let mine = PackageSet::parse(&serde_json::to_vec(&json!({ "schema_version": 1, "packages": [
        { "id": "stock-art", "version": "1.0.0", "side": "shared", "dir": "addons/stock-art" },
        { "id": "mine-only", "version": "1.0.0", "side": "shared", "dir": "addons/mine-only" },
    ] }))?)?;
    let local = bri_package::environment::Environment::load(stale_root.path(), &mine)?.client_packages();
    let server_copy = environment.packages.iter().find(|p| p.id == "stock-art").unwrap();
    assert!(!local.contains(server_copy), "the local copy is stale");
    let cache = bri_package::sync::Cache::open(&stale_root.path().join(".downloads"))?;
    let (client, fetched, dropped) = bri_net::client::Client::connect_fetching(
        server.address,
        bri_net::client::HostPin::from(&server.certificate[..]),
        "Stale".into(),
        local.clone(),
        None,
        &identity,
        &cache,
        bri_progress::Progress::default(),
        |fetched, dropped| {
            Ok(bri_client::mods::load_fetched(stale_root.path(), &mine, &local, fetched, dropped)?.1)
        },
    )
    .await?;
    assert!(client.owner > 0, "the stale player joined");
    drop(client);
    assert_eq!(dropped.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["mine-only"]);
    let joined = bri_client::mods::joined_set(stale_root.path(), &mine, &fetched, &dropped)?;
    let mut ids: Vec<_> = joined.packages.iter().map(|p| p.id.clone()).collect();
    ids.sort();
    assert_eq!(ids, ["bad-model", "extra-weapon", "missing-icon", "stock-art"]);
    let stock = joined.packages.iter().find(|p| p.id == "stock-art").unwrap();
    assert!(stock.dir.starts_with(".downloads/"), "the server's copy: {}", stock.dir);
    let dir = bri_package::packages::package_dir(stale_root.path(), stock)?;
    assert_eq!(bri_package::environment::hash_dir(&dir)?.0, server_copy.hash);
    let extras = kind_providers(stale_root.path(), &joined, "weapons.json")?;
    let weapons = WeaponContent::load_with(&stale_base.weapons, &extras)?;
    let names: Vec<_> = weapons.item_choices.iter().map(|(_, n)| n.as_str()).collect();
    assert!(names.contains(&"Mystery Box"), "the server's version: {names:?}");
    assert!(!names.contains(&"Toy Gun"), "their own Add-On sits out: {names:?}");
    let assets = ItemAssets::load_with(&stale_base.items, &stale_base.weapons, &extras)?;
    let ui = ItemUi::new(&assets, &weapons.item_choices, &letters()?)?;
    assert_eq!(ui.catalog().len(), weapons.item_choices.len());
    server.stop().await?;
    Ok(())
}
