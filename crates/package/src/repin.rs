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
//! fields (`source`, `source_sha256`) are never touched.
//!
//! Two links reach beyond a plain on-disk reference:
//! - A model's *virtual identity* — the original `.dts` path that keys its
//!   entry in a presentation's `models` map. Twin copies of one asset (the
//!   base game keeps each weapon shape in two packs) pin different files but
//!   share that key; re-stamping either copy re-pins every manifest whose
//!   reference equals the identity, so both sides stay equal.
//! - An *aggregate* pin beside no resolvable file reference (a presentation's
//!   `weapons_sha256`) records one file's previous bytes: when that file is
//!   itself re-stamped, pins holding its old hash move to the new one. Editing a model also rewrites the manifest that pins it, so aggregates recording that manifest's own bytes follow it as well — a presentation's `weapons_sha256` stays in step with an edited weapon GLB and the strict add-on check keeps passing.

use crate::defaults;
use anyhow::{Context, Result, bail};
use bri_content::brick_materials::safe_relative;
use sha2::{Digest, Sha256};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
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

/// Whether self-heal is armed in this process (see [`arm`]).
pub fn armed() -> bool {
    root().is_some()
}

/// Re-stamp an add-on's presentation aggregate pins (`weapons_sha256`,
/// `item_physics_sha256`) to the bytes of the files beside them, when
/// self-heal is armed and a pin disagrees. Those files are verified only as
/// aggregates of one another — a strict check beside no read that heals — so
/// rewriting one outside a healed read (a migration script, a hand edit)
/// leaves its presentation stale with no [`verify`] to fire; this closes the
/// gap at the checks' own sites. Returns whether a pin moved; `false` also
/// means nothing was healable here (self-heal off, pins already in step,
/// or an unexpected file), and the caller keeps its strict error then.
pub fn resync_presentation(assets: &Path) -> Result<bool> {
    if !armed() {
        return Ok(false);
    }
    let manifest = assets.join("presentation.json");
    let Ok(text) = std::fs::read_to_string(&manifest) else {
        return Ok(false);
    };
    let Ok(mut value) = serde_json::from_str::<Value>(&text) else {
        return Ok(false);
    };
    let Some(map) = value.as_object_mut() else {
        return Ok(false);
    };
    let mut touched = false;
    for (key, file) in [("weapons_sha256", "weapons.json"), ("item_physics_sha256", "item-physics.json")] {
        let path = assets.join(file);
        if !path.is_file() {
            return Ok(false);
        }
        let hash = format!("{:x}", Sha256::digest(std::fs::read(&path)?));
        restamp_slot(map, key, &hash, &mut touched);
    }
    if !touched {
        return Ok(false);
    }
    write_manifest(&manifest, &value)?;
    bri_console::warn(format!(
        "Self-heal: re-pinned the presentation's aggregate pins in {}",
        manifest.display()
    ));
    Ok(true)
}

/// Rewrite every stale live pin that references `target` to
/// `new_sha256_hex`; returns how many manifests changed. Aggregates recording
/// the bytes of any rewritten manifest (a presentation's `weapons_sha256`) are
/// carried forward to its new hash, so strict add-on checks keep passing after
/// a self-heal. Skips dot folders (the `.downloads` cache) and unparseable
/// JSON; writes atomically so a torn manifest can never survive a crash mid-write.
pub fn restamp(content_root: &Path, target: &Path, new_sha256_hex: &str) -> Result<usize> {
    let root = content_root
        .canonicalize()
        .with_context(|| format!("Missing content root {}", content_root.display()))?;
    let target = target.canonicalize()?;
    // Aggregates pin the file's previous bytes; that hash is what they hold.
    let old_hash = std::fs::read(&target)
        .ok()
        .map(|bytes| format!("{:x}", Sha256::digest(&bytes)));
    let identities = collect_identities(&root, &target);
    let mut changed = BTreeSet::new();
    // Manifests the direct pass rewrites, with their before/after byte hashes:
    // aggregates recording a manifest's own bytes must follow it.
    let mut rewritten: Vec<(String, String)> = Vec::new();
    walk(
        &root,
        &root,
        &target,
        new_sha256_hex,
        old_hash.as_deref(),
        &identities,
        &mut changed,
        &mut rewritten,
    )?;
    // Propagate to a fixpoint: rewriting one manifest can desync the
    // aggregates of manifests pinning it, and so on down the chain.
    let mut pending = BTreeMap::new();
    for (old, new) in rewritten {
        pending.insert(old, new);
    }
    for _ in 0..8 {
        if pending.is_empty() {
            break;
        }
        let mut next = BTreeMap::new();
        propagate(&root, &pending, &mut changed, &mut next)?;
        pending = next;
    }
    Ok(changed.len())
}

/// Every lower-cased map key under which some object references `target` —
/// the virtual identities (original `.dts` paths) that name it. A resource in
/// another pack pins its twin copy of the same asset by that identity alone.
fn collect_identities(root: &Path, target: &Path) -> std::collections::BTreeSet<String> {
    let mut found = std::collections::BTreeSet::new();
    scan_dir(root, root, target, &mut found);
    found
}

fn scan_dir(dir: &Path, content_root: &Path, target: &Path, out: &mut BTreeSet<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // `.downloads` is a download cache, never ours to re-stamp.
            if !entry.file_name().to_string_lossy().starts_with('.') {
                scan_dir(&path, content_root, target, out);
            }
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("json")) {
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let Ok(value) = serde_json::from_str::<Value>(&text) else { continue };
            let dir = path.parent().unwrap_or(Path::new("."));
            scan_value(&value, dir, content_root, target, None, out);
        }
    }
}

/// Read-only twin of [`rewrite`]: records the map key of any object whose
/// reference resolves to `target`.
fn scan_value(
    value: &Value,
    manifest_dir: &Path,
    content_root: &Path,
    target: &Path,
    parent_key: Option<&str>,
    out: &mut BTreeSet<String>,
) {
    match value {
        Value::Object(map) => {
            let base = map
                .get("package")
                .and_then(Value::as_str)
                .filter(|dir| safe_relative(dir))
                .map(|dir| content_root.join(dir))
                .unwrap_or_else(|| manifest_dir.to_path_buf());
            for key in REF_KEYS {
                let Some(reference) = map.get(*key).and_then(Value::as_str) else {
                    continue;
                };
                if safe_relative(reference) && points_at(&base, reference, target) {
                    if let Some(k) = parent_key {
                        out.insert(k.to_ascii_lowercase());
                    }
                    return;
                }
            }
            for (k, v) in map {
                scan_value(v, manifest_dir, content_root, target, Some(k.as_str()), out);
            }
        }
        Value::Array(items) => {
            for item in items {
                scan_value(item, manifest_dir, content_root, target, None, out);
            }
        }
        _ => {}
    }
}

fn walk(
    dir: &Path,
    content_root: &Path,
    target: &Path,
    hash: &str,
    old_hash: Option<&str>,
    identities: &BTreeSet<String>,
    changed: &mut BTreeSet<PathBuf>,
    rewritten: &mut Vec<(String, String)>,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            // `.downloads` is a download cache, never ours to re-stamp.
            if !entry.file_name().to_string_lossy().starts_with('.') {
                walk(&path, content_root, target, hash, old_hash, identities, changed, rewritten)?;
            }
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("json")) {
            restamp_file(&path, content_root, target, hash, old_hash, identities, changed, rewritten)?;
        }
    }
    Ok(())
}

fn restamp_file(
    manifest: &Path,
    content_root: &Path,
    target: &Path,
    hash: &str,
    old_hash: Option<&str>,
    identities: &BTreeSet<String>,
    changed: &mut BTreeSet<PathBuf>,
    rewritten: &mut Vec<(String, String)>,
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
    rewrite(
        &mut value,
        dir,
        content_root,
        target,
        hash,
        old_hash,
        identities,
        None,
        &mut touched,
    );
    if !touched {
        return Ok(());
    }
    let old_hex = format!("{:x}", Sha256::digest(text.as_bytes()));
    let new_hex = write_manifest(manifest, &value)?;
    changed.insert(manifest.to_path_buf());
    rewritten.push((old_hex, new_hex));
    Ok(())
}

/// Move aggregate slots holding any of `pairs`' old hashes — the previous
/// bytes of manifests this restamp rewrote — to their new hashes; record the
/// before/after hashes of every manifest it itself rewrites.
fn propagate(
    content_root: &Path,
    pairs: &BTreeMap<String, String>,
    changed: &mut BTreeSet<PathBuf>,
    next: &mut BTreeMap<String, String>,
) -> Result<()> {
    walk_propagate(content_root, content_root, pairs, changed, next)?;
    Ok(())
}

fn walk_propagate(
    dir: &Path,
    content_root: &Path,
    pairs: &BTreeMap<String, String>,
    changed: &mut BTreeSet<PathBuf>,
    next: &mut BTreeMap<String, String>,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if !entry.file_name().to_string_lossy().starts_with('.') {
                walk_propagate(&path, content_root, pairs, changed, next)?;
            }
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("json")) {
            restamp_file_aggregate(&path, pairs, changed, next)?;
        }
    }
    Ok(())
}

fn restamp_file_aggregate(
    manifest: &Path,
    pairs: &BTreeMap<String, String>,
    changed: &mut BTreeSet<PathBuf>,
    next: &mut BTreeMap<String, String>,
) -> Result<()> {
    let Ok(text) = std::fs::read_to_string(manifest) else {
        return Ok(());
    };
    let Ok(mut value) = serde_json::from_str::<Value>(&text) else {
        return Ok(());
    };
    let mut touched = false;
    move_aggregates(&mut value, pairs, &mut touched);
    if !touched {
        return Ok(());
    }
    let old_hex = format!("{:x}", Sha256::digest(text.as_bytes()));
    let new_hex = write_manifest(manifest, &value)?;
    changed.insert(manifest.to_path_buf());
    next.insert(old_hex, new_hex);
    Ok(())
}

/// Rewrite aggregate slots — `X_sha256` beside no sibling reference named
/// `X`, provenance excluded — that hold a recorded old hash. Live pins sit
/// beside their reference and are matched by value alone here: only the bytes
/// of manifests this restamp rewrote are in play.
fn move_aggregates(value: &mut Value, pairs: &BTreeMap<String, String>, touched: &mut bool) {
    match value {
        Value::Object(map) => {
            let keys: Vec<String> = map
                .keys()
                .filter(|key| key.ends_with("_sha256"))
                .filter_map(|key| key.strip_suffix("_sha256").map(str::to_string))
                .filter(|base| {
                    !base.is_empty() && !base.contains("source") && !map.contains_key(base.as_str())
                })
                .map(|base| format!("{base}_sha256"))
                .collect();
            for key in keys {
                if let Some(slot) = map.get(&key).and_then(Value::as_str)
                    && let Some(new) = pairs.get(slot)
                {
                    map.insert(key.clone(), Value::String(new.clone()));
                    *touched = true;
                }
            }
            for child in map.values_mut() {
                move_aggregates(child, pairs, touched);
            }
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                move_aggregates(item, pairs, touched);
            }
        }
        _ => {}
    }
}

/// Serialize a manifest the way installed files are written (two-space pretty
/// JSON with sorted keys and a trailing newline) and replace it atomically so
/// a torn manifest can never survive a crash mid-write; returns its new byte hash.
fn write_manifest(manifest: &Path, value: &Value) -> Result<String> {
    let out = serde_json::to_string_pretty(value)?;
    let bytes = format!("{out}\n");
    let name = manifest.file_name().unwrap_or_default().to_string_lossy();
    let tmp = manifest.with_file_name(format!("{name}.repin-{}~", std::process::id()));
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, manifest).with_context(|| format!("Re-pinning {}", manifest.display()))?;
    Ok(format!("{:x}", Sha256::digest(bytes.as_bytes())))
}

fn points_at(base: &Path, reference: &str, target: &Path) -> bool {
    base.join(reference)
        .canonicalize()
        .is_ok_and(|path| path == *target)
}

/// Set one pin slot to `hash` when it still holds a different value.
fn restamp_slot(
    map: &mut serde_json::Map<String, Value>,
    pin_key: &str,
    hash: &str,
    touched: &mut bool,
) {
    if let Some(slot) = map.get_mut(pin_key)
        && slot.as_str() != Some(hash)
    {
        *slot = Value::String(hash.into());
        *touched = true;
    }
}

/// Rewrite the live pins of one object that point at `target`, then recurse.
fn rewrite(
    value: &mut Value,
    manifest_dir: &Path,
    content_root: &Path,
    target: &Path,
    hash: &str,
    old_hash: Option<&str>,
    identities: &BTreeSet<String>,
    parent_key: Option<&str>,
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
            let mut points = false;
            for key in REF_KEYS {
                let Some(reference) = map.get(*key).and_then(Value::as_str) else {
                    continue;
                };
                if !safe_relative(reference) || !points_at(&base, reference, target) {
                    // The virtual identity still names the asset: a twin copy
                    // in another pack pins it by its original `.dts` key.
                    if identities.contains(&reference.to_ascii_lowercase()) {
                        points = true;
                    }
                    continue;
                }
                points = true;
            }

            // The object's own key is a virtual identity of `target` (its
            // entry in another pack's `models` map names this file).
            let mut keyed = false;
            if let Some(k) = parent_key {
                keyed = identities.contains(&k.to_ascii_lowercase());
            }

            // An aggregate beside no resolvable reference records the target
            // file's previous bytes: a presentation's `weapons_sha256`.
            let mut aggregate = false;
            if !points && !keyed && old_hash.is_some() {
                aggregate = !REF_KEYS.iter().any(|key| {
                    map.get(*key)
                        .and_then(Value::as_str)
                        .is_some_and(|reference| safe_relative(reference) && points_at(&base, reference, target))
                });
            }

            if points || keyed {
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
                // The sibling reference this pin is named after (the key it
                // sits beside) either points at the edited file or names it by
                // its virtual identity — only then is the pair this asset's.
                let pair_is_this_asset = map.get(base_key).and_then(Value::as_str).is_some_and(
                    |reference| {
                        (safe_relative(reference) && points_at(&base, reference, target))
                            || identities.contains(&reference.to_ascii_lowercase())
                    },
                );

                if pair_is_this_asset {
                    restamp_slot(map, &pin_key, hash, touched);
                } else if aggregate && !base_key.contains("source") {
                    // An aggregate pin records the file's previous bytes; move it.
                    let holds_old = old_hash.is_some_and(|old| {
                        map.get(&pin_key).and_then(Value::as_str) == Some(old)
                    });
                    if holds_old {
                        restamp_slot(map, &pin_key, hash, touched);
                    }
                }
            }

            for (k, child) in map.iter_mut() {
                rewrite(
                    child,
                    manifest_dir,
                    content_root,
                    target,
                    hash,
                    old_hash,
                    identities,
                    Some(k.as_str()),
                    touched,
                );
            }
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                rewrite(
                    item,
                    manifest_dir,
                    content_root,
                    target,
                    hash,
                    old_hash,
                    identities,
                    None,
                    touched,
                );
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// A presentation whose aggregate pins went stale because a sibling was
    /// rewritten outside any healed read is re-stamped at its own check site
    /// when armed, and stays strict when off.
    #[test]
    fn resync_presentation_heals_stale_aggregate_pins() {
        let _guard = TEST_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join("assets")).unwrap();
        let physics: Vec<u8> = b"{\"schema_version\": 1, \"items\": {}}".to_vec();
        std::fs::write(root.join("assets/item-physics.json"), &physics).unwrap();
        // Stale on purpose: the pin records an earlier weapons.json.
        let before = format!(
            r#"{{ "item_physics_sha256": "{}", "weapons_sha256": "{}" }}"#,
            hex(&physics),
            hex(b"OLD-WEAPONS")
        );
        std::fs::write(root.join("assets/presentation.json"), &before).unwrap();

        set_state(None);
        assert!(!resync_presentation(&root.join("assets")).unwrap());
        assert_eq!(
            std::fs::read_to_string(root.join("assets/presentation.json")).unwrap(),
            before
        );

        // The rewrite that left the pin stale, landed outside a healed read.
        let weapons_v2: Vec<u8> = b"{\"items\": [1]}".to_vec();
        std::fs::write(root.join("assets/weapons.json"), &weapons_v2).unwrap();

        set_state(Some(root.clone()));
        assert!(resync_presentation(&root.join("assets")).unwrap());
        let pres: Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("assets/presentation.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(pres["weapons_sha256"], hex(&weapons_v2));
        // The in-step item-physics pin is untouched.
        assert_eq!(pres["item_physics_sha256"], hex(&physics));

        // Idempotent: a second pass finds nothing stale and rewrites nothing.
        let after = std::fs::read_to_string(root.join("assets/presentation.json")).unwrap();
        assert!(!resync_presentation(&root.join("assets")).unwrap());
        assert_eq!(
            std::fs::read_to_string(root.join("assets/presentation.json")).unwrap(),
            after
        );
        set_state(None);
    }

    #[test]
    fn resync_presentation_refuses_without_both_siblings() {
        let _guard = TEST_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join("assets")).unwrap();
        std::fs::write(root.join("assets/weapons.json"), b"{}").unwrap();
        // No item-physics.json: nothing is healable, even armed.
        std::fs::write(
            root.join("assets/presentation.json"),
            r#"{ "weapons_sha256": "00" }"#,
        )
        .unwrap();
        set_state(Some(root.clone()));
        assert!(!resync_presentation(&root.join("assets")).unwrap());
        set_state(None);
    }

    /// Editing one twin copy of a base-game asset re-pins every manifest that
    /// names it by its virtual `.dts` identity, so the two packs stay equal.
    #[test]
    fn heals_twin_packs_by_virtual_identity() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        // pack-a holds a presentation and its copy of the model.
        std::fs::create_dir_all(root.join("pack-a")).unwrap();
        std::fs::write(root.join("pack-a/a.glb"), b"GLB").unwrap();
        std::fs::write(
            root.join("pack-a/presentation.json"),
            format!(
                r#"{{
  "models": {{
    "add-ons/x/a.dts": {{
      "file": "a.glb",
      "sha256": "{}",
      "source_sha256": "{}"
    }}
  }}
}}"#,
                hex(b"OLD"),
                hex(b"PROV")
            ),
        )
        .unwrap();
        // merged holds a weapons resource pinning the twin copy by identity.
        std::fs::create_dir_all(root.join("merged/shapes")).unwrap();
        std::fs::write(root.join("merged/shapes/a.glb"), b"TWIN").unwrap();
        std::fs::write(
            root.join("merged/weapons.json"),
            format!(
                r#"{{
  "resources": [
    {{
      "native_file": "shapes/a.glb",
      "path": "Add-Ons/X/A.dts",
      "sha256": "{}"
    }}
  ]
}}"#,
                hex(b"OLD")
            ),
        )
        .unwrap();

        let target = root.join("pack-a/a.glb");
        // Both manifests change: the direct pin and the identity twin.
        assert_eq!(restamp(&root, &target, &hex(b"GLB")).unwrap(), 2);

        let pres: Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("pack-a/presentation.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(pres["models"]["add-ons/x/a.dts"]["sha256"], hex(b"GLB"));
        // Provenance of the original asset never moves.
        assert_eq!(pres["models"]["add-ons/x/a.dts"]["source_sha256"], hex(b"PROV"));

        let wep: Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("merged/weapons.json")).unwrap(),
        )
        .unwrap();
        // The twin copy's pin follows the edited file through its identity.
        assert_eq!(wep["resources"][0]["sha256"], hex(b"GLB"));
    }

    /// An aggregate pin beside no resolvable reference (a presentation's
    /// `weapons_sha256`) records one file's previous bytes; re-stamping that
    /// file moves the pin forward to its new hash.
    #[test]
    fn heals_aggregate_pin_of_restamped_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::write(root.join("W.json"), b"{\"note\": \"w\"}").unwrap();
        let w_hash = hex(
            &std::fs::read(root.join("W.json")).unwrap(),
        );
        // A presentation pins W.json's bytes with no live reference beside it.
        std::fs::write(
            root.join("presentation.json"),
            format!(r#"{{ "weapons_sha256": "{}" }}"#, w_hash),
        )
        .unwrap();

        let target = root.join("W.json");
        assert_eq!(restamp(&root, &target, &hex(b"NEW-W")).unwrap(), 1);

        let pres: Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("presentation.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(pres["weapons_sha256"], hex(b"NEW-W"));
    }

    /// Editing a model rewrites the manifest that pins it, changing that file's
    /// bytes; the presentation's aggregate `weapons_sha256` must follow to the
    /// new bytes so the strict add-on check keeps passing.
    #[test]
    fn propagates_aggregate_pin_of_rewritten_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join("assets")).unwrap();
        std::fs::write(root.join("assets/x.glb"), b"OLD-GLB").unwrap();
        let weapons_v1: Vec<u8> = format!(
            r#"{{
  "resources": [
    {{
      "native_file": "x.glb",
      "path": "Add-Ons/X.dts",
      "sha256": "{}"
    }}
  ]
}}"#,
            hex(b"OLD-GLB")
        )
        .into_bytes();
        std::fs::write(root.join("assets/weapons.json"), &weapons_v1).unwrap();
        std::fs::write(
            root.join("assets/presentation.json"),
            format!(r#"{{ "weapons_sha256": "{}" }}"#, hex(&weapons_v1)),
        )
        .unwrap();

        // Edit the model: weapons.json's pin moves, and so do its bytes.
        let target = root.join("assets/x.glb");
        assert_eq!(restamp(&root, &target, &hex(b"NEW-GLB")).unwrap(), 2);

        let wep: Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("assets/weapons.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(wep["resources"][0]["sha256"], hex(b"NEW-GLB"));
        // The aggregate records the manifest's final bytes, exactly as the
        // strict check compares them.
        let wep_bytes = std::fs::read(root.join("assets/weapons.json")).unwrap();
        let pres: Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("assets/presentation.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(pres["weapons_sha256"], hex(&wep_bytes));

        // Idempotent: a second pass finds nothing stale.
        assert_eq!(restamp(&root, &target, &hex(b"NEW-GLB")).unwrap(), 0);
    }
}

