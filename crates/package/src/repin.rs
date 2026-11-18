//! Self-healing re-stamp of stale content pins in a source checkout.
//!
//! Every installed content file carries a sha256 pin in its package's
//! manifest, and the loaders verify it on read so a truncated or half-synced
//! file fails loudly instead of loading broken. When an installed file is
//! edited by hand (a vertex fixed in a GLB, a brick image repainted), its
//! pin goes stale and the next launch faults with "checksum mismatch".
//!
//! In a source checkout — where content is generated from this repository
//! and edits are the norm rather than corruption — [`verify`] re-stamps the
//! stale pins instead of faulting: it scans the package manifests for
//! references to the edited file and rewrites only the live pin. Provenance
//! fields (`source`, `source_sha256`) are never touched, and aggregate pins
//! that name no single file (map lighting bundles, fx libraries, merged
//! weapons physics) still fail as before. Releases stay integrity-locked
//! unless [`REPIN_ENV`] is set to 1, an explicit bypass.

use crate::defaults;
use anyhow::{Context, Result, bail};
use bri_content::brick_materials::safe_relative;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// `0` disables self-heal even in a checkout. Unset: checkouts only (a
/// release's content is integrity-locked). `1`: force on anywhere — an
/// explicit bypass of the pin verification's corruption guard.
pub const REPIN_ENV: &str = "BRI_REPIN_ON_EDIT";

/// Manifest keys that hold a package-relative file name beside its pin.
const REF_KEYS: &[&str] = &["file", "path", "native_mesh", "native_file"];
/// Live pins rewritten by [`restamp`]. `source_sha256` is provenance of the
/// original asset and is never re-stamped, whatever it sits beside.
const PIN_KEYS: &[&str] = &["sha256", "native_sha256"];

static STATE: Mutex<Option<PathBuf>> = Mutex::new(None);

fn set_state(root: Option<PathBuf>) {
    *STATE.lock().unwrap() = root;
}

/// Arm self-heal for this process when appropriate; returns whether it is on.
pub fn arm(content_root: &Path) -> bool {
    let armed = match std::env::var_os(REPIN_ENV).as_deref().and_then(|v| v.to_str()) {
        Some("0") => false,
        Some("1") => true,
        _ => defaults::checkout_packages(content_root).is_some(),
    };
    set_state(armed.then(|| content_root.to_path_buf()));
    armed
}

fn root() -> Option<PathBuf> {
    STATE.lock().unwrap().clone()
}

/// Verify one file pin, self-healing when armed. `mismatch` is the exact
/// message a strict build would have reported; it is returned unchanged when
/// healing is off or finds no manifest pin to fix.
pub fn verify(
    mismatch: impl Into<String>,
    target: &Path,
    actual_hex: &str,
    expected_hex: &str,
) -> Result<()> {
    if actual_hex == expected_hex {
        return Ok(());
    }
    let Some(root) = root() else {
        bail!("{}", mismatch.into());
    };
    let n = restamp(&root, target, actual_hex)?;
    if n == 0 {
        bail!("{}", mismatch.into());
    }
    bri_console::warn(format!(
        "Self-heal: re-pinned {} manifest(s) for edited file {}",
        n,
        target.file_name().unwrap_or_default().to_string_lossy()
    ));
    Ok(())
}

/// Rewrite every stale live pin that references `target` to
/// `new_sha256_hex`; returns how many manifests changed. Skips dot folders
/// (the `.downloads` cache) and unparseable JSON; writes atomically so a torn
/// manifest can never survive a crash mid-write.
pub fn restamp(content_root: &Path, target: &Path, new_sha256_hex: &str) -> Result<usize> {
    let root = content_root
        .canonicalize()
        .with_context(|| format!("Missing content root {}", content_root.display()))?;
    let target = target.canonicalize()?;
    let mut changed = 0usize;
    walk(&root, &root, &target, new_sha256_hex, &mut changed)?;
    Ok(changed)
}

fn walk(
    dir: &Path,
    content_root: &Path,
    target: &Path,
    hash: &str,
    count: &mut usize,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            // `.downloads` is a download cache, never ours to re-stamp.
            if !entry.file_name().to_string_lossy().starts_with('.') {
                walk(&path, content_root, target, hash, count)?;
            }
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("json")) {
            restamp_file(&path, content_root, target, hash, count)?;
        }
    }
    Ok(())
}

fn restamp_file(
    manifest: &Path,
    content_root: &Path,
    target: &Path,
    hash: &str,
    count: &mut usize,
) -> Result<()> {
    let Ok(text) = std::fs::read_to_string(manifest) else {
        return Ok(());
    };
    // A few installed files are not JSON at all; they load through other
    // paths and stay out of this scan.
    let Ok(mut value) = serde_json::from_str::<Value>(&text) else {
        return Ok(());
    };
    let dir = manifest.parent().unwrap_or(Path::new("."));
    let mut touched = false;
    rewrite(&mut value, dir, content_root, target, hash, &mut touched);
    if !touched {
        return Ok(());
    }
    // The installed manifests are two-space pretty JSON with a trailing
    // newline and sorted keys: re-serializing changes nothing but the pins.
    let out = serde_json::to_string_pretty(&value)?;
    let name = manifest.file_name().unwrap_or_default().to_string_lossy();
    let tmp = manifest.with_file_name(format!("{name}.repin-{}~", std::process::id()));
    std::fs::write(&tmp, format!("{out}\n"))?;
    std::fs::rename(&tmp, manifest).with_context(|| format!("Re-pinning {}", manifest.display()))?;
    *count += 1;
    Ok(())
}

fn points_at(base: &Path, reference: &str, target: &Path) -> bool {
    base.join(reference)
        .canonicalize()
        .is_ok_and(|path| path == *target)
}

/// Rewrite the live pins of one object that point at `target`, then recurse.
fn rewrite(
    value: &mut Value,
    manifest_dir: &Path,
    content_root: &Path,
    target: &Path,
    hash: &str,
    touched: &mut bool,
) {
    match value {
        Value::Object(map) => {
            // A merged weapons resource names the package that holds its
            // files (content-root-relative); everything else sits beside the
            // manifest. Mirrors `bri_weapons::resource_root`.
            let base = map
                .get("package")
                .and_then(Value::as_str)
                .filter(|dir| safe_relative(dir))
                .map(|dir| content_root.join(dir))
                .unwrap_or_else(|| manifest_dir.to_path_buf());

            // `file`/`path`/... beside its live pin.
            for key in REF_KEYS {
                let Some(reference) = map.get(*key).and_then(Value::as_str) else {
                    continue;
                };
                if !safe_relative(reference) || !points_at(&base, reference, target) {
                    continue;
                }
                for pin in PIN_KEYS {
                    if let Some(slot) = map.get_mut(*pin)
                        && slot.as_str() != Some(hash)
                    {
                        *slot = Value::String(hash.into());
                        *touched = true;
                    }
                }
            }

            // `foo` beside `foo_sha256` (an avatar's rig, a debris model):
            // the pin is named after its reference. Provenance stays put.
            let pin_keys: Vec<String> = map
                .keys()
                .filter(|key| key.ends_with("_sha256"))
                .filter_map(|key| {
                    let base_key = key.strip_suffix("_sha256")?;
                    (!base_key.is_empty() && base_key != "source").then(|| key.clone())
                })
                .collect();
            for pin_key in pin_keys {
                let base_key = pin_key.strip_suffix("_sha256").unwrap();
                let Some(reference) = map.get(base_key).and_then(Value::as_str) else {
                    continue;
                };
                if safe_relative(reference) && points_at(&base, reference, target) {
                    if let Some(slot) = map.get_mut(&pin_key)
                        && slot.as_str() != Some(hash)
                    {
                        *slot = Value::String(hash.into());
                        *touched = true;
                    }
                }
            }

            for child in map.values_mut() {
                rewrite(child, manifest_dir, content_root, target, hash, touched);
            }
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                rewrite(item, manifest_dir, content_root, target, hash, touched);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn hex(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    struct Tree {
        dir: tempfile::TempDir,
        root: PathBuf,
    }

    impl Tree {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().to_path_buf();
            // pack-a: a model (live pin + provenance), a rig pair.
            std::fs::create_dir_all(root.join("pack-a")).unwrap();
            std::fs::write(root.join("pack-a/m.glb"), b"GLB").unwrap();
            std::fs::write(root.join("pack-a/r.json"), b"RIG").unwrap();
            std::fs::write(
                root.join("pack-a/manifest.json"),
                format!(
                    r#"{{
  "models": {{
    "m": {{
      "file": "m.glb",
      "sha256": "{}",
      "source_sha256": "{}"
    }}
  }},
  "rig": "r.json",
  "rig_sha256": "{}"
}}"#,
                    hex(b"OLD-GLB"),
                    hex(b"PROVENANCE"),
                    hex(b"OLD-RIG")
                ),
            )
            .unwrap();
            // merged: a weapons resource whose files live in another package.
            std::fs::create_dir_all(root.join("merged")).unwrap();
            std::fs::write(
                root.join("merged/weapons.json"),
                format!(
                    r#"{{
  "resources": {{
    "s": {{
      "native_file": "t.png",
      "package": "pack-d/assets",
      "path": "Add-Ons/x/t.png",
      "sha256": "{}"
    }}
  }}
}}"#,
                    hex(b"OLD-PNG")
                ),
            )
            .unwrap();
            std::fs::create_dir_all(root.join("pack-d/assets")).unwrap();
            std::fs::write(root.join("pack-d/assets/t.png"), b"PNG").unwrap();
            // The download cache and a non-JSON file must be left alone.
            std::fs::create_dir_all(root.join(".downloads/cached")).unwrap();
            std::fs::write(
                root.join(".downloads/cached/manifest.json"),
                format!(r#"{{ "file": "m.glb", "sha256": "{}" }}"#, hex(b"OLD-GLB")),
            )
            .unwrap();
            std::fs::create_dir_all(root.join("broken")).unwrap();
            std::fs::write(root.join("broken/broken.json"), b"{ not json").unwrap();
            Tree { dir, root }
        }

        fn read(&self, rel: &str) -> String {
            std::fs::read_to_string(self.root.join(rel)).unwrap()
        }
    }

    #[test]
    fn restamps_sibling_pin_and_leaves_provenance_alone() {
        let tree = Tree::new();
        let target = tree.root.join("pack-a/m.glb");
        assert_eq!(restamp(&tree.root, &target, &hex(b"GLB")).unwrap(), 1);

        let manifest: Value = serde_json::from_str(&tree.read("pack-a/manifest.json")).unwrap();
        let model = &manifest["models"]["m"];
        assert_eq!(model["sha256"], hex(b"GLB"));
        // Provenance of the original asset never moves.
        assert_eq!(model["source_sha256"], hex(b"PROVENANCE"));
        // The rig pin is untouched: r.json was not edited.
        assert_eq!(manifest["rig_sha256"], hex(b"OLD-RIG"));
        // The download cache and the broken file are out of bounds.
        let cached: Value =
            serde_json::from_str(&tree.read(".downloads/cached/manifest.json")).unwrap();
        assert_eq!(cached["sha256"], hex(b"OLD-GLB"));
        assert_eq!(tree.read("broken/broken.json"), "{ not json");
    }

    #[test]
    fn restamps_named_pin_pairs() {
        let tree = Tree::new();
        let target = tree.root.join("pack-a/r.json");
        assert_eq!(restamp(&tree.root, &target, &hex(b"RIG")).unwrap(), 1);

        let manifest: Value = serde_json::from_str(&tree.read("pack-a/manifest.json")).unwrap();
        assert_eq!(manifest["rig_sha256"], hex(b"RIG"));
    }

    #[test]
    fn resolves_merged_resources_through_their_package() {
        let tree = Tree::new();
        let target = tree.root.join("pack-d/assets/t.png");
        assert_eq!(restamp(&tree.root, &target, &hex(b"PNG")).unwrap(), 1);

        let manifest: Value = serde_json::from_str(&tree.read("merged/weapons.json")).unwrap();
        assert_eq!(manifest["resources"]["s"]["sha256"], hex(b"PNG"));
    }

    #[test]
    fn restamp_is_idempotent_and_ignores_unpinned_files() {
        let tree = Tree::new();
        let target = tree.root.join("pack-a/m.glb");
        assert_eq!(restamp(&tree.root, &target, &hex(b"GLB")).unwrap(), 1);
        // Second pass: the pin already matches, nothing is rewritten.
        assert_eq!(restamp(&tree.root, &target, &hex(b"GLB")).unwrap(), 0);

        std::fs::write(tree.root.join("pack-a/loose.bin"), b"FREE").unwrap();
        let loose = tree.root.join("pack-a/loose.bin");
        assert_eq!(restamp(&tree.root, &loose, &hex(b"FREE")).unwrap(), 0);
    }

    /// The armed-root global is process-wide; serialize the tests that set it.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn verify_heals_when_armed_and_stays_strict_when_off() {
        let _guard = TEST_LOCK.lock().unwrap();
        let tree = Tree::new();
        set_state(None);
        let target = tree.root.join("pack-a/m.glb");
        let err = verify(
            "Item resource checksum mismatch: m.glb",
            &target,
            &hex(b"GLB"),
            &hex(b"OLD-GLB"),
        )
        .unwrap_err();
        assert!(err.to_string().contains("checksum mismatch"));

        set_state(Some(tree.root.clone()));
        verify(
            "Item resource checksum mismatch: m.glb",
            &target,
            &hex(b"GLB"),
            &hex(b"OLD-GLB"),
        )
        .unwrap();
        let manifest: Value = serde_json::from_str(&tree.read("pack-a/manifest.json")).unwrap();
        assert_eq!(manifest["models"]["m"]["sha256"], hex(b"GLB"));

        // A pin no manifest owns still faults, even armed.
        std::fs::write(tree.root.join("orphan.bin"), b"ORPHAN").unwrap();
        let orphan = tree.root.join("orphan.bin");
        assert!(verify(
            "Item resource checksum mismatch: orphan.bin",
            &orphan,
            &hex(b"ORPHAN"),
            &hex(b"OLD-ORPHAN"),
        )
        .is_err());
        set_state(None);
    }

    #[test]
    fn verify_passes_matching_pins_without_scanning() {
        let _guard = TEST_LOCK.lock().unwrap();
        let tree = Tree::new();
        set_state(Some(tree.root.clone()));
        let target = tree.root.join("pack-a/m.glb");
        verify(
            "Item resource checksum mismatch: m.glb",
            &target,
            &hex(b"OLD-GLB"),
            &hex(b"OLD-GLB"),
        )
        .unwrap();
        set_state(None);
    }
}

