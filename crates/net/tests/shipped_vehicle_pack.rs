//! The shipped vehicle folders must merge into one pack that still contains
//! every base vehicle, HorseArmor chief among them. The client's avatar
//! loader (`AvatarAssets::load_horse`) reads the `vehicles` role's base pack
//! plus every other package's `assets/vehicles.json`; a merged pack without
//! the horse aborts game load with "Vehicle pack has no HorseArmor".

use bri_net::content_identity::kind_providers;
use bri_package::packages::PackageSet;
use std::path::{Path, PathBuf};

fn content_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")
}

#[test]
fn the_shipped_vehicle_packs_merge_with_the_horse() {
    let root = content_root();
    let packages = PackageSet::load_root(&root).expect("the shipped package set loads");
    let base_dir = packages
        .role_dir(&root, "vehicles")
        .expect("a vehicles role is filled");
    let extras = kind_providers(&root, &packages, "vehicles.json")
        .expect("vehicle providers resolve");
    eprintln!("base: {}", base_dir.display());
    for (dir, abs) in &extras {
        eprintln!("extra: {} -> {}", dir, abs.display());
    }

    let mut parts = Vec::new();
    for (dir, abs) in extras {
        let part = bri_vehicles::Pack::load(abs.join("vehicles.json"))
            .unwrap_or_else(|error| panic!("part pack fails to load: {dir}: {error:#}"));
        part.verify_assets(&abs)
            .unwrap_or_else(|error| panic!("part assets fail verification: {dir}: {error:#}"));
        parts.push((dir, part));
    }

    let base = bri_vehicles::Pack::load(base_dir.join("vehicles.json"))
        .expect("the base vehicle pack loads");
    let (merged, notes) = base.merge(parts);
    for note in &notes {
        eprintln!("merge note: {note}");
    }

    let ids: Vec<&str> = merged.definitions.iter().map(|d| d.id.as_str()).collect();
    eprintln!("merged definitions ({}): {ids:?}", ids.len());
    assert!(
        merged.definitions.iter().any(|d| d.id == "v20.vehicle.horsearmor"),
        "the merged vehicle pack lost HorseArmor"
    );
}
