//! A client loads imported Add-On packages listed in packages.json beside the
//! base game: weapons, item presentation and HUD icons, vehicles and their
//! models. No window or GPU is used.
use bri_addon_import::{Options, import};
use bri_client::content::ClientContent;
use bri_package::packages::{PackageEntry, PackageSet, Side};
use std::path::{Path, PathBuf};

const ARCHIVE: &str = "C:/Users/Maxwell/Documents/_Blockland_Maxwell_1588_Archive/Addons";
const REFERENCE: &str = "E:/Downloads/B4v21Launcher/versions/Blockland v20";

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
#[ignore = "requires generated content (content/, see docs/content-regeneration.md)"]
fn client_loads_imported_packages_beside_the_base_game() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let scratch = Scratch(root.join(format!("_addon-client-{}", std::process::id())));
    let name = scratch.0.file_name().unwrap().to_string_lossy().into_owned();
    let mut imports = vec![(
        "weapon_synthetic_blaster",
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../addon-import/tests/fixtures/Weapon_Synthetic_Blaster"),
        None,
    )];
    let archive = std::env::var("BRI_ADDON_ARCHIVE").unwrap_or(ARCHIVE.into());
    let reference = std::env::var("BRI_V20_REFERENCE").unwrap_or(REFERENCE.into());
    let real = Path::new(&archive).is_dir() && Path::new(&reference).is_dir();
    if real {
        for (id, addon) in [("weapon_shotgun", "Weapon_Shotgun"), ("vehicle_blocko_car", "Vehicle_Blocko_Car")] {
            imports.push((id, Path::new(&archive).join(format!("{addon}.zip")), Some(PathBuf::from(&reference))));
        }
    } else {
        eprintln!("community archive or v20 reference absent: synthetic package only");
    }
    let mut set = PackageSet::base();
    for (id, input, reference) in imports {
        import(&Options {
            input,
            out: scratch.0.join(id),
            reference,
            ..Default::default()
        })
        .unwrap();
        set.packages.push(PackageEntry {
            id: id.into(),
            version: "1.0.0".into(),
            side: Side::Shared,
            dir: format!("{name}/{id}"),
            role: None,
        });
    }
    let content = ClientContent::load_packages(&root, &set).unwrap();
    let paths = &content.paths;
    let ids: Vec<&str> = content.weapons.item_choices.iter().map(|(id, _)| id.as_str()).collect();
    assert!(ids.contains(&"v20.weapon.gunitem"));
    assert!(ids.contains(&"weapon_synthetic_blaster:weapon/blasteritem"));
    // Item presentation, pickup bounds and HUD icons cover imported items.
    let items = bri_client::items::ItemAssets::load_with(
        &paths.item_presentation,
        &paths.weapons,
        &paths.weapon_extras,
    )
    .unwrap();
    bri_client::item_ui::ItemUi::new(&items, &content.weapons.item_choices, &content.ui_pack).unwrap();
    items
        .item_scene("weapon_synthetic_blaster:weapon/blasteritem", glam::Mat4::IDENTITY)
        .unwrap();
    assert!(content.item_physics.bounds.contains_key("weapon_synthetic_blaster:weapon/blasteritem"));
    bri_client::explosion_shapes::ExplosionShapes::load(&content.weapons.pack, &paths.weapons).unwrap();
    // The imported brick is in the brick menu under the category it declares,
    // with its own icon.
    let pad = content
        .bricks
        .iter()
        .find(|b| b.id == "weapon_synthetic_blaster:brick/brickblasterpaddata")
        .expect("imported brick in the brick menu");
    assert_eq!((pad.category.as_str(), pad.subcategory.as_str(), pad.ui_name.as_str()), ("Special", "Synthetic", "Blaster Pad"));
    let bri_ui::api::IconRef::Pack(icon) = &pad.icon else {
        panic!("imported brick has no icon: {:?}", pad.icon);
    };
    assert!(content.ui_pack.has_image(icon));
    let pixels = content
        .ui_pack
        .pixels(&bri_ui::pack::TexKey::Image(icon.clone()))
        .expect("the icon decodes");
    let _ = pixels;
    if real {
        assert!(ids.contains(&"weapon_shotgun:weapon/shotgunitem"));
        let shotgun = items
            .item_scene("weapon_shotgun:weapon/shotgunitem", glam::Mat4::IDENTITY)
            .unwrap();
        assert!(!shotgun.vertices.is_empty(), "the shotgun model draws");
        assert!(content.vehicles.definitions.iter().any(|d| d.id == "vehicle_blocko_car:vehicle/blockocarvehicle"));
        bri_client::vehicles::VehicleAssets::load_with(&paths.vehicles, &paths.vehicle_extras).unwrap();
    }
}

/// One enabled Add-On that cannot load (here its folder is gone) is left out
/// with its reason; the game still starts with the base game and the rest.
#[test]
#[ignore = "requires generated content (content/, see docs/content-regeneration.md)"]
fn a_broken_add_on_is_left_out_instead_of_stopping_the_game() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut set = PackageSet::base();
    set.packages.push(PackageEntry {
        id: "weapon_gone".into(),
        version: "1.0.0".into(),
        side: Side::Shared,
        dir: format!("_addon-gone-{}", std::process::id()),
        role: None,
    });
    assert!(ClientContent::load_packages(&root, &set).is_err());
    let (content, left_out) = ClientContent::load_leaving_out_broken(&root, &set).unwrap();
    assert_eq!(content.paths.packages, PackageSet::base());
    assert_eq!(left_out.len(), 1);
    assert!(left_out[0].starts_with("weapon_gone: "), "{left_out:?}");
}
