//! Shared by the import tests.
use bri_package::packages::{PackageEntry, Side};
use std::path::Path;

/// Entries for the base packages an imported package's manifest names
/// (`v20-weapons` for an Add-On that required `Weapon_Gun`): the game
/// always has them, so a hermetic set lists them without their content.
pub fn base_entries(package: &Path) -> Vec<PackageEntry> {
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(package.join("package.json")).unwrap()).unwrap();
    manifest["dependencies"]
        .as_object()
        .into_iter()
        .flat_map(|d| d.keys())
        .filter(|id| bri_package::id::is_reserved(id))
        .map(|base| PackageEntry {
            id: base.clone(),
            version: "9.0.0".into(),
            side: Side::Shared,
            dir: "unused".into(),
            role: Some(base.trim_start_matches("v20-").into()),
        })
        .collect()
}
