//! The counted fallbacks of a load (`bri_console::fallback`) as a
//! machine-readable report, `fallbacks.json`, next to the Add-On health
//! report in the game's `logs` folder. `bri-client --check` writes it; the
//! push gate and the packager read it and refuse any entry that is not on
//! the stock allow list.
use anyhow::{Context, Result};
use serde_json::json;
use std::path::{Path, PathBuf};

/// The report's file name, in the game's `logs` folder.
pub const REPORT_FILE: &str = "fallbacks.json";

/// Exercise the loaders `--check` would otherwise leave for a player's first
/// match: every listed map's visual bundle and native collision. A map that
/// does not load is a fallback of its own, not a failed check, so the full list
/// comes back in one run.
pub fn load_maps(paths: &crate::content::ContentPaths) {
    for map in paths.maps.listed() {
        let owner = map.package.as_deref().unwrap_or(&map.id);
        if let Err(error) = bri_render::scene_loader::load_map_bundle_dynamic(&map.root, &map.id) {
            bri_console::fallback::report(
                bri_console::fallback::kind::OTHER,
                owner,
                &format!("map {} (visual)", map.id),
                &format!("the map does not load: {error:#}"),
            );
        }
        if let Err(error) = bri_sim::map::NativeMap::load(&map.root, &map.id) {
            bri_console::fallback::report(
                bri_console::fallback::kind::OTHER,
                owner,
                &format!("map {} (collision)", map.id),
                &format!("the map does not load: {error:#}"),
            );
        }
    }
}

/// Write every fallback reported so far to `<state>/logs/fallbacks.json`, one
/// object per distinct fallback, and say where and how many.
pub fn write(state: &Path) -> Result<(PathBuf, usize)> {
    let entries = bri_console::fallback::snapshot();
    let list: Vec<_> = entries
        .iter()
        .map(|e| {
            json!({
                "kind": e.kind,
                "addon": e.addon,
                "item": e.item,
                "fallback": e.fallback,
                "count": e.count,
            })
        })
        .collect();
    let dir = state.join("logs");
    std::fs::create_dir_all(&dir).with_context(|| format!("Creating {}", dir.display()))?;
    let path = dir.join(REPORT_FILE);
    let overflow = bri_console::fallback::overflow();
    let text = serde_json::to_string_pretty(&json!({
        "schema": 1,
        "overflow": overflow,
        "fallbacks": list,
    }))?;
    std::fs::write(&path, text).with_context(|| format!("Writing {}", path.display()))?;
    Ok((path, entries.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_report_lists_each_fallback_with_its_count() {
        let addon = "fallbacks-report-test";
        bri_console::fallback::report("missing_texture", addon, "smoke.png", "a cloud");
        bri_console::fallback::report("missing_texture", addon, "smoke.png", "a cloud");
        let dir = tempfile::tempdir().unwrap();
        let (path, count) = write(dir.path()).unwrap();
        assert!(count >= 1);
        assert_eq!(path.file_name().unwrap(), REPORT_FILE);
        assert_eq!(path.parent().unwrap().file_name().unwrap(), "logs");
        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(report["schema"], 1);
        let mine = report["fallbacks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["addon"] == addon)
            .unwrap();
        assert_eq!(mine["kind"], "missing_texture");
        assert_eq!(mine["item"], "smoke.png");
        assert_eq!(mine["fallback"], "a cloud");
        assert_eq!(mine["count"], 2);
    }
}
