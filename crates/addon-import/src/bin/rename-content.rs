//! Temporary in-place content renamer (throwaway tool).
//!
//! Renames every hash-named asset file under a content root back to its
//! original Blockland file name, as recorded by the manifests that shipped
//! with it, and rewrites every JSON reference so nothing dangles. File
//! layout is preserved: each file keeps its directory and only its base
//! name changes (the importer's sub-path convention is intentionally NOT
//! replicated — moving files between directories would risk references no
//! manifest records).
//!
//! Name sources, in priority order (first hit wins):
//! 1. `import-report.json` `assets[]` (`output` -> `source`)
//! 2. `presentation.json` model/texture rows (`file` + `source`, or a
//!    filename-like `id`)
//! 3. `content.json` entries (`file` + `id`, e.g. `<pkg>:brick_geometry/NAME.blb`)
//! 4. any other manifest object that carries exactly one hash reference next
//!    to an original-name field (`source`, `id`, `mesh_id`, `virtual_path`,
//!    `path`, or the first entry of `source_paths`)
//! 5. `bundle.json` `assets` map keys (virtual paths) -> values
//! 6. audio inventory `audio_files[]` (`sha256` + `virtual_path`)
//! 7. a hash-named `.json`'s own top-level `id` field (sharded model files)
//! 8. content-hash match against the Blockland v20 reference installation
//!
//! Unmapped files are left untouched and listed in RENAME-REPORT.json.
//! `.downloads/` is excluded entirely (sealed download cache).
//!
//! Usage: `rename-content CONTENT_ROOT [--apply] [--ref REF_DIR]`
//!
//! Dry-run by default: computes and reports every rename without touching any
//! file (except writing RENAME-REPORT.json into the root). Pass `--apply` to
//! perform the renames and manifest rewrites.
//!
//! TEMPORARY TOOL: once all content is renamed, delete this file and its
//! `[[bin]]` entry from Cargo.toml.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use bri_addon_import::source::hash;
use serde_json::Value;

struct Args {
    root: PathBuf,
    apply: bool,
    ref_dir: Option<PathBuf>,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Ok(args) = parse_args(&args) else {
        eprintln!("usage: rename-content CONTENT_ROOT [--apply] [--ref REF_DIR]");
        return ExitCode::from(2);
    };
    match run(&args) {
        Ok(report) => {
            println!("{}", report.summary_line());
            if !report.unmapped.is_empty() {
                println!(
                    "RENAME-REPORT: {} file(s) left unmapped (untouched); see RENAME-REPORT.json",
                    report.unmapped.len()
                );
            }
            if !report.failures.is_empty() {
                println!(
                    "RENAME-REPORT: {} failure(s); see RENAME-REPORT.json",
                    report.failures.len()
                );
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("rename-content: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn parse_args(args: &[String]) -> Result<Args> {
    let mut root = None;
    let mut apply = false;
    let mut ref_dir = None;
    for a in args {
        if a == "--apply" {
            apply = true;
        } else if let Some(r) = a.strip_prefix("--ref=") {
            anyhow::ensure!(ref_dir.is_none(), "duplicate --ref");
            ref_dir = Some(PathBuf::from(r));
        } else if !a.starts_with('-') {
            anyhow::ensure!(root.is_none(), "unexpected argument: {a}");
            root = Some(PathBuf::from(a));
        } else {
            anyhow::bail!("unknown flag: {a}");
        }
    }
    let Some(root) = root else {
        anyhow::bail!("missing CONTENT_ROOT")
    };
    let root = root.canonicalize().context("content root does not exist")?;
    Ok(Args {
        root,
        apply,
        ref_dir,
    })
}

struct Report {
    mode: &'static str,
    root: String,
    ref_used: bool,
    candidates: usize,
    mapped: usize,
    by_source: BTreeMap<String, usize>,
    unmapped: Vec<String>,
    propagated: usize,
    collisions: usize,
    ambiguous_bases: Vec<(String, Vec<String>)>,
    manifests_rewritten: usize,
    downloads_excluded: usize,
    failures: Vec<(String, String)>,
}

impl Report {
    fn summary_line(&self) -> String {
        let sources: Vec<String> = self
            .by_source
            .iter()
            .map(|(k, v)| format!("{k}:{v}"))
            .collect();
        format!(
            "[{}] root {}: {} hash-named files, {} renamed ({}), {} propagated-by-basename, \
             {} collisions suffixed, {} ambiguous basenames, {} unmapped, {} manifests rewritten, \
             {} download-cache files excluded",
            self.mode,
            self.root,
            self.candidates,
            self.mapped,
            sources.join(", "),
            self.propagated,
            self.collisions,
            self.ambiguous_bases.len(),
            self.unmapped.len(),
            self.manifests_rewritten,
            self.downloads_excluded,
        )
    }
}

fn run(args: &Args) -> Result<Report> {
    let root = &args.root;
    let apply = args.apply;

    // --- Discover ---------------------------------------------------------
    let (all_files, downloads_excluded) = walk_skip_downloads(root);
    let candidates: BTreeSet<PathBuf> = all_files
        .iter()
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(is_candidate_base)
        })
        .cloned()
        .collect();

    let jsons: Vec<&PathBuf> = all_files
        .iter()
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".json"))
        })
        .collect();

    // path -> (new basename, source label)
    let mut map: BTreeMap<PathBuf, (String, &'static str)> = BTreeMap::new();
    let mut by_source: BTreeMap<String, usize> = BTreeMap::new();

    fn assign(
        map: &mut BTreeMap<PathBuf, (String, &'static str)>,
        by_source: &mut BTreeMap<String, usize>,
        target: &Path,
        new_base: String,
        source: &'static str,
    ) {
        if new_base.is_empty() || map.contains_key(target) {
            return;
        }
        map.insert(target.to_path_buf(), (new_base, source));
        *by_source.entry(source.to_string()).or_insert(0) += 1;
    }

    // --- S1: import-report.json assets[] ----------------------------------
    for j in &jsons {
        if base_name(j) != "import-report.json" {
            continue;
        }
        let Ok(text) = fs::read_to_string(j) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let Some(assets) = v.get("assets").and_then(|a| a.as_array()) else {
            continue;
        };
        let dir = j.parent().unwrap_or(root);
        for a in assets {
            let Some(out) = a.get("output").and_then(|o| o.as_str()) else {
                continue;
            };
            let Some(src) = a.get("source").and_then(|s| s.as_str()) else {
                continue;
            };
            if out.is_empty() || src.is_empty() {
                continue;
            }
            let target = dir.join(norm_rel(out));
            if !candidates.contains(&target) {
                continue;
            }
            let Some(stem) = file_stem_of(src) else {
                continue;
            };
            let rest = hash_rest(
                target
                    .file_name()
                    .unwrap_or_default()
                    .to_str()
                    .unwrap_or(""),
            );
            assign(
                &mut map,
                &mut by_source,
                &target,
                format!("{stem}{rest}"),
                "import-report",
            );
        }
    }

    // --- S2: presentation.json model + texture rows -----------------------
    for j in &jsons {
        if base_name(j) != "presentation.json" {
            continue;
        }
        let Ok(text) = fs::read_to_string(j) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let dir = j.parent().unwrap_or(root);
        if let Some(models) = v.get("models").and_then(|m| m.as_object()) {
            for entry in models.values() {
                handle_presentation_row(entry, dir, &candidates, &mut map, &mut by_source);
            }
        }
        if let Some(t) = v.get("textures") {
            match t {
                Value::Object(rows) => {
                    for row in rows.values() {
                        handle_presentation_row(row, dir, &candidates, &mut map, &mut by_source);
                    }
                }
                Value::Array(rows) => {
                    for row in rows {
                        handle_presentation_row(row, dir, &candidates, &mut map, &mut by_source);
                    }
                }
                _ => {}
            }
        }
    }

    // --- S3: content.json entries ------------------------------------------
    for j in &jsons {
        if base_name(j) != "content.json" {
            continue;
        }
        let Ok(text) = fs::read_to_string(j) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let dir = j.parent().unwrap_or(root);
        let entries: &Vec<Value> = match &v {
            Value::Array(a) => a,
            Value::Object(o) if o.get("entries").is_some_and(Value::is_array) => {
                o["entries"].as_array().unwrap()
            }
            _ => continue,
        };
        for e in entries {
            let Some(file) = e.get("file").and_then(|f| f.as_str()) else {
                continue;
            };
            let Some(id) = e.get("id").and_then(|i| i.as_str()) else {
                continue;
            };
            let target = dir.join(norm_rel(file));
            if !candidates.contains(&target) {
                continue;
            }
            // id looks like `<pkg>:brick_geometry/NAME.blb` or `<pkg>:asset/NAME.png`.
            let Some(stem) = file_stem_of(id) else {
                continue;
            };
            let rest = hash_rest(
                target
                    .file_name()
                    .unwrap_or_default()
                    .to_str()
                    .unwrap_or(""),
            );
            assign(
                &mut map,
                &mut by_source,
                &target,
                format!("{stem}{rest}"),
                "content.json",
            );
        }
    }

    // Content-hash index of candidates: full hex prefix -> file paths. Used to
    // associate objects that name a file only via its `sha256` field (audio
    // provenance, inventories).
    let mut sha_index: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for c in &candidates {
        if let Some((prefix, _)) = split_hash(base_name(c).as_str()) {
            sha_index
                .entry(prefix.to_string())
                .or_default()
                .push(c.clone());
        }
    }

    // --- S4: generic sibling scan over every manifest -----------------------
    // Schema-specific files were already handled above; the generic scan may
    // still fill gaps they left (nested tables, keyed source-path maps).
    for j in &jsons {
        let b = base_name(j);
        if b == "MIGRATE-REPORT.json" || b == "RENAME-REPORT.json" {
            continue;
        }
        let Ok(text) = fs::read_to_string(j) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let dir = j.parent().unwrap_or(root);
        scan_objects(&v, dir, &candidates, &sha_index, &mut map, &mut by_source);
    }

    // --- S5: bundle.json assets (key -> value) ------------------------------
    for j in &jsons {
        if base_name(j) != "bundle.json" {
            continue;
        }
        let Ok(text) = fs::read_to_string(j) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let Some(assets) = v.get("assets").and_then(|a| a.as_object()) else {
            continue;
        };
        let dir = j.parent().unwrap_or(root);
        for (key, val) in assets {
            let Some(file) = val.as_str() else { continue };
            let target = dir.join(norm_rel(file));
            if !candidates.contains(&target) {
                continue;
            }
            // key is a virtual source path like `v20/add-ons/map_creek/rock1.dts`.
            let Some(stem) = file_stem_of(key) else {
                continue;
            };
            let rest = hash_rest(
                target
                    .file_name()
                    .unwrap_or_default()
                    .to_str()
                    .unwrap_or(""),
            );
            assign(
                &mut map,
                &mut by_source,
                &target,
                format!("{stem}{rest}"),
                "bundle.json",
            );
        }
    }

    // --- S6: audio inventory audio_files[] (sha256 + virtual_path) ----------
    let mut by_prefix: BTreeMap<String, Vec<&PathBuf>> = BTreeMap::new();
    for c in &candidates {
        if let Some((prefix, _)) = split_hash(base_name(c).as_str()) {
            by_prefix.entry(prefix.to_string()).or_default().push(c);
        }
    }
    for j in &jsons {
        let Ok(text) = fs::read_to_string(j) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let Some(files) = v.get("audio_files").and_then(|a| a.as_array()) else {
            continue;
        };
        for e in files {
            let Some(sha) = e.get("sha256").and_then(|s| s.as_str()) else {
                continue;
            };
            let Some(vp) = e.get("virtual_path").and_then(|s| s.as_str()) else {
                continue;
            };
            let Some(stem) = file_stem_of(vp) else {
                continue;
            };
            if let Some(paths) = by_prefix.get(sha) {
                for p in paths {
                    if map.contains_key(*p) {
                        continue;
                    }
                    let rest = hash_rest(base_name(p).as_str());
                    assign(
                        &mut map,
                        &mut by_source,
                        p,
                        format!("{stem}{rest}"),
                        "audio-inventory",
                    );
                }
            }
        }
    }

    // --- S7: hash-named .json with an internal `id` field --------------------
    for c in candidates.iter() {
        if map.contains_key(c) {
            continue;
        }
        let base = base_name(c);
        let Some((_, rest)) = split_hash(&base) else {
            continue;
        };
        if rest != ".json" {
            continue; // sharded model files are plain `<hash>.json`
        }
        let Ok(bytes) = fs::read(c) else { continue };
        let Ok(v) = serde_json::from_slice::<Value>(&bytes) else {
            continue;
        };
        let Some(id) = v.get("id").and_then(|i| i.as_str()) else {
            continue;
        };
        if !id.contains('/') {
            continue;
        }
        let Some(stem) = file_stem_of(id) else {
            continue;
        };
        assign(
            &mut map,
            &mut by_source,
            c,
            format!("{stem}.json"),
            "internal-id",
        );
    }

    // --- S8: v20 reference content-hash fallback -----------------------------
    let ref_dir = args.ref_dir.clone().or_else(|| {
        root.parent()
            .map(|p| p.join(".research/reference/Blockland v20"))
    });
    let mut ref_used = false;
    if let Some(rd) = &ref_dir
        && rd.is_dir()
    {
        let mut index: BTreeMap<String, String> = BTreeMap::new();
        for f in walk_skip_downloads(rd).0 {
            if let Ok(bytes) = fs::read(&f)
                && let Some(name) = f.file_name().and_then(|n| n.to_str())
            {
                index.insert(hash(&bytes), name.to_string());
            }
        }
        ref_used = true;
        for c in candidates.iter() {
            if map.contains_key(c) {
                continue;
            }
            let Ok(bytes) = fs::read(c) else { continue };
            let Some(name) = index.get(&hash(&bytes)) else {
                continue;
            };
            let stem = file_stem_of(name).unwrap_or_default();
            if stem.is_empty() {
                continue;
            }
            let rest = hash_rest(base_name(c).as_str());
            assign(
                &mut map,
                &mut by_source,
                c,
                format!("{stem}{rest}"),
                "v20-sha",
            );
        }
    }

    // --- Propagation: same old basename, one agreed new name -----------------
    let mut names_by_old_base: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (p, (nb, _)) in &map {
        names_by_old_base
            .entry(base_name(p).clone())
            .or_default()
            .insert(nb.clone());
    }
    let mut propagated = 0usize;
    for c in candidates.iter() {
        if map.contains_key(c) {
            continue;
        }
        let ob = base_name(c);
        if let Some(set) = names_by_old_base.get(&ob)
            && set.len() == 1
        {
            let nb = set.iter().next().unwrap().clone();
            map.insert(c.to_path_buf(), (nb, "propagated"));
            *by_source.entry("propagated".to_string()).or_insert(0) += 1;
            propagated += 1;
        }
    }

    // --- Sidecar propagation: same hash prefix in the same directory ----------
    // Derived companions (`.source.json`, `.clips.json`, ...) share their asset's
    // content-hash prefix with its named primary output. Resolution is scoped to
    // one directory so that twins of the same file in different packages (which
    // may carry differently cased names) never conflict.
    let mut dir_fam: BTreeMap<PathBuf, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    for (p, (nb, _)) in &map {
        let ob = base_name(p);
        if let Some((prefix, rest)) = split_hash(ob.as_str())
            && let Some(dir) = p.parent()
        {
            let stem = nb.strip_suffix(rest).unwrap_or(nb.as_str());
            dir_fam
                .entry(dir.to_path_buf())
                .or_default()
                .entry(prefix.to_string())
                .or_default()
                .push(stem.to_string());
        }
    }
    for c in candidates.iter() {
        if map.contains_key(c) {
            continue;
        }
        let ob = base_name(c);
        let Some((prefix, rest)) = split_hash(ob.as_str()) else {
            continue;
        };
        let Some(dir) = c.parent() else { continue };
        let Some(stems) = dir_fam.get(dir).and_then(|m| m.get(prefix)) else {
            continue;
        };
        // All sibling stems must agree up to case.
        let folded: BTreeSet<String> = stems.iter().map(|s| s.to_ascii_lowercase()).collect();
        if !stems.is_empty() && folded.len() == 1 {
            map.insert(
                c.to_path_buf(),
                (format!("{}{}", stems[0], rest), "sidecar"),
            );
            *by_source.entry("sidecar".to_string()).or_insert(0) += 1;
        }
    }

    // --- Collision resolution per directory ----------------------------------
    let mut taken: BTreeMap<PathBuf, BTreeSet<String>> = BTreeMap::new();
    for f in &all_files {
        if let Some(dir) = f.parent() {
            taken
                .entry(dir.to_path_buf())
                .or_default()
                .insert(base_name(f).to_ascii_lowercase());
        }
    }
    let mut renames: BTreeMap<PathBuf, String> = BTreeMap::new();
    let mut collisions = 0usize;
    for (p, (nb, _)) in &map {
        let dir = p.parent().unwrap_or(root).to_path_buf();
        let set = taken.entry(dir.clone()).or_default();
        let prefix8: String = split_hash(base_name(p).as_str())
            .map(|(h, _)| h.chars().take(8).collect())
            .unwrap_or_default();
        let mut candidate = nb.clone();
        if set.contains(&candidate.to_ascii_lowercase()) {
            // The suffix goes before the extension ("pineTree-d60e7be3.glb",
            // not "pineTree.glb-d60e7be3"): runtime loaders check
            // ".ends_with(\".glb\")" and image readers need a real extension.
            let (stem, ext) = match nb.rfind('.') {
                Some(i) if i > 0 => (&nb[..i], &nb[i..]),
                _ => (nb.as_str(), ""),
            };
            candidate = format!("{stem}-{prefix8}{ext}");
            collisions += 1;
        }
        if set.contains(&candidate.to_ascii_lowercase()) {
            // Pathological double collision: leave the file untouched.
            renames.remove(p);
            continue;
        }
        set.insert(candidate.to_ascii_lowercase());
        renames.insert(p.clone(), candidate);
    }

    let unmapped: Vec<String> = candidates
        .iter()
        .filter(|c| !renames.contains_key(*c))
        .map(|p| rel_to(root, p).replace('\\', "/"))
        .collect();

    // --- Reference rewrite maps (package-scoped) ------------------------------
    // Resolution is package-first so identically named files in different
    // packages never collide; a global unambiguous basename map remains as the
    // fallback for cross-package references.
    let mut pkg_rel: BTreeMap<PathBuf, BTreeMap<String, String>> = BTreeMap::new();
    let mut pkg_last2: BTreeMap<PathBuf, BTreeMap<(String, String), String>> = BTreeMap::new();
    let mut pkg_base: BTreeMap<PathBuf, BTreeMap<String, String>> = BTreeMap::new();
    let mut base_map: BTreeMap<String, String> = BTreeMap::new();
    for (p, nb) in &renames {
        let ob = base_name(p);
        put_unique(&mut base_map, ob.clone(), nb.clone());
        for scope in package_scopes(p, root) {
            let rel = rel_to(&scope, p).replace('\\', "/");
            if !rel.is_empty() && rel != "." {
                // One file per path within a package: no conflict possible.
                pkg_rel
                    .entry(scope.clone())
                    .or_default()
                    .insert(rel, nb.clone());
            }
            let parent = p
                .parent()
                .and_then(|d| d.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            put_unique(
                pkg_last2.entry(scope.clone()).or_default(),
                (parent.clone(), ob.clone()),
                nb.clone(),
            );
            put_unique(
                pkg_base.entry(scope.clone()).or_default(),
                ob.clone(),
                nb.clone(),
            );
        }
    }
    let ambiguous_bases: Vec<(String, Vec<String>)> = names_by_old_base
        .iter()
        .filter(|(_, set)| set.len() > 1)
        .map(|(k, v)| (k.clone(), v.iter().cloned().collect()))
        .collect();

    // --- Rewrite every manifest in memory ------------------------------------
    let mut new_texts: BTreeMap<PathBuf, String> = BTreeMap::new();
    for j in &jsons {
        let b = base_name(j);
        if b == "MIGRATE-REPORT.json" || b == "RENAME-REPORT.json" {
            continue;
        }
        let Ok(text) = fs::read_to_string(j) else {
            continue;
        };
        let Ok(mut v) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let pkg = package_root(j, root);
        let ctx = RewriteCtx {
            rel: pkg_rel.get(&pkg).unwrap_or(&EMPTY_MAP),
            last2: pkg_last2.get(&pkg).unwrap_or(&EMPTY_LAST2),
            base: pkg_base.get(&pkg).unwrap_or(&EMPTY_MAP),
            global_base: &base_map,
        };
        let mut touched = false;
        rewrite_value(&mut v, &ctx, &mut touched);
        if !touched {
            continue;
        }
        new_texts.insert(
            j.to_path_buf(),
            serde_json::to_string_pretty(&v).unwrap_or_default() + "\n",
        );
    }

    // --- Apply -----------------------------------------------------------------
    let mut failures: Vec<(String, String)> = Vec::new();
    if apply {
        for (old, nb) in &renames {
            let dir = old.parent().unwrap_or(root);
            let new_path = dir.join(nb);
            if fs::rename(old, &new_path).is_err() {
                // Target may exist under a different case; retry via temp.
                let tmp = dir.join(format!(".renaming-{nb}"));
                match (fs::rename(old, &tmp), fs::rename(&tmp, &new_path)) {
                    (Ok(()), Ok(())) => {}
                    _ => failures
                        .push((rel_to(root, old).replace('\\', "/"), "rename failed".into())),
                }
            }
        }
        for (path, text) in &new_texts {
            // The manifest itself may have been renamed above.
            let dst = renames
                .get(path)
                .map(|nb| path.parent().unwrap_or(root).join(nb))
                .unwrap_or_else(|| path.clone());
            if fs::write(&dst, text).is_err() {
                failures.push((
                    rel_to(root, path).replace('\\', "/"),
                    "manifest rewrite failed".into(),
                ));
            }
        }
    }

    // --- Report ------------------------------------------------------------------
    let report = Report {
        mode: if apply { "apply" } else { "dry-run" },
        root: root.display().to_string(),
        ref_used,
        candidates: candidates.len(),
        mapped: renames.len(),
        by_source,
        unmapped,
        propagated,
        collisions,
        ambiguous_bases,
        manifests_rewritten: new_texts.len(),
        downloads_excluded,
        failures: failures.clone(),
    };
    let json = serde_json::json!({
        "tool": "rename-content",
        "mode": report.mode,
        "root": &report.root,
        "reference_used": report.ref_used,
        "totals": {
            "candidates": report.candidates,
            "renamed": report.mapped,
            "by_source": report.by_source.clone(),
            "propagated_by_basename": report.propagated,
            "collisions_suffixed": report.collisions,
            "ambiguous_basenames": report.ambiguous_bases.len(),
            "unmapped": report.unmapped.len(),
            "manifests_rewritten": report.manifests_rewritten,
            "download_cache_excluded": report.downloads_excluded,
            "failures": failures.len(),
        },
        "ambiguous_basenames": report.ambiguous_bases,
        "unmapped": &report.unmapped,
        "failures": &failures,
    });
    // Full old->new plan for audit/rollback (content/ is not under git control).
    let mut plan = serde_json::Map::new();
    for (old, nb) in &renames {
        plan.insert(
            rel_to(root, old).replace('\\', "/"),
            Value::String(nb.clone()),
        );
    }
    fs::write(
        root.join("RENAME-PLAN.json"),
        serde_json::to_string_pretty(&plan).unwrap_or_default() + "\n",
    )
    .with_context(|| format!("writing {}", root.display()))?;

    fs::write(
        root.join("RENAME-REPORT.json"),
        serde_json::to_string_pretty(&json).unwrap_or_default() + "\n",
    )
    .with_context(|| format!("writing {}", root.display()))?;

    Ok(report)
}

/// One presentation model/texture row: `file` is the hash reference, and the
/// original name comes from `source` (or a filename-like `id`).
fn handle_presentation_row(
    row: &Value,
    dir: &Path,
    candidates: &BTreeSet<PathBuf>,
    map: &mut BTreeMap<PathBuf, (String, &'static str)>,
    by_source: &mut BTreeMap<String, usize>,
) {
    let Some(file) = row.get("file").and_then(|f| f.as_str()) else {
        return;
    };
    if file.is_empty() {
        return;
    }
    let target = dir.join(norm_rel(file));
    if !candidates.contains(&target) {
        return;
    }
    let stem = row
        .get("source")
        .and_then(|s| s.as_str())
        .and_then(file_stem_of)
        .or_else(|| {
            // Only accept an `id` that looks like a real file name.
            row.get("id")
                .and_then(|i| i.as_str())
                .map(basename)
                .filter(|b| b.contains('.'))
                .and_then(file_stem_of)
        });
    let Some(stem) = stem else { return };
    let rest = hash_rest(
        target
            .file_name()
            .unwrap_or_default()
            .to_str()
            .unwrap_or(""),
    );
    if map.contains_key(&target) {
        return;
    }
    map.insert(
        target.clone(),
        (format!("{stem}{rest}"), "presentation.json"),
    );
    *by_source
        .entry("presentation.json".to_string())
        .or_insert(0) += 1;
}

/// Generic scan, fully recursive: any JSON object holding exactly one
/// resolvable hash reference next to an original-name field yields a mapping.
/// Objects whose values are keyed by source paths (bundle-style maps) name
/// their files from the keys instead.
fn scan_objects(
    v: &Value,
    dir: &Path,
    candidates: &BTreeSet<PathBuf>,
    sha_index: &BTreeMap<String, Vec<PathBuf>>,
    map: &mut BTreeMap<PathBuf, (String, &'static str)>,
    by_source: &mut BTreeMap<String, usize>,
) {
    match v {
        Value::Object(obj) => {
            let mut refs: Vec<(PathBuf, String)> = Vec::new(); // (target, raw value)
            for val in obj.values() {
                if let Some(s) = val.as_str() {
                    let b = basename(s);
                    if !is_candidate_base(b) {
                        continue;
                    }
                    let p = if s.contains('/') {
                        dir.join(norm_rel(s))
                    } else {
                        dir.join(s)
                    };
                    if candidates.contains(&p) {
                        refs.push((p, b.to_string()));
                    }
                }
            }
            if refs.len() == 1 {
                let (target, _raw) = &refs[0];
                if let Some(stem) = original_stem(obj)
                    && !map.contains_key(target)
                {
                    map.insert(
                        target.clone(),
                        (
                            format!("{stem}{}", hash_rest(base_name(target).as_str())),
                            "sibling-field",
                        ),
                    );
                    *by_source.entry("sibling-field".to_string()).or_insert(0) += 1;
                }
            } else if refs.len() > 1 {
                // Keyed source-path maps: `{"v20/.../name.dts": "<hash>.glb", ...}`.
                for (key, val) in obj {
                    let Some(s) = val.as_str() else { continue };
                    if !is_candidate_base(basename(s)) {
                        continue;
                    }
                    let p = if s.contains('/') {
                        dir.join(norm_rel(s))
                    } else {
                        dir.join(s)
                    };
                    if !candidates.contains(&p) || map.contains_key(&p) {
                        continue;
                    }
                    // Only accept keys that look like real file names.
                    let Some(stem) = key_stem(key) else { continue };
                    let rest = hash_rest(base_name(&p).as_str());
                    map.insert(p, (format!("{stem}{rest}"), "key-path"));
                    *by_source.entry("key-path".to_string()).or_insert(0) += 1;
                }
            }
            if refs.is_empty() {
                // Content-hash association: an object carrying a bare `sha256`
                // plus an original-name field points at `<sha>.<ext>`.
                if let Some(sha) = obj.get("sha256").and_then(Value::as_str)
                    && is_hex_like(sha)
                {
                    // Desired extension from the name field, when it carries one.
                    let ext: Option<String> = ["source", "mesh_id", "virtual_path", "path"]
                        .iter()
                        .find_map(|f| obj.get(*f).and_then(Value::as_str))
                        .map(effective_base)
                        .filter(|b| b.contains('.'))
                        .map(|b| b[b.rfind('.').unwrap()..].to_string());
                    if let Some(stem) = original_stem(obj) {
                        let hits: Vec<&PathBuf> = sha_index
                            .get(sha)
                            .map(|v| {
                                v.iter()
                                    .filter(|p| {
                                        ext.as_deref()
                                            .is_none_or(|e| hash_rest(base_name(p).as_str()) == e)
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        if hits.len() == 1 && !map.contains_key(hits[0]) {
                            let rest = hash_rest(base_name(hits[0]).as_str());
                            map.insert(hits[0].clone(), (format!("{stem}{rest}"), "sha-field"));
                            *by_source.entry("sha-field".to_string()).or_insert(0) += 1;
                        }
                    }
                }
            }
            for val in obj.values() {
                scan_objects(val, dir, candidates, sha_index, map, by_source);
            }
        }
        Value::Array(items) => {
            for item in items {
                scan_objects(item, dir, candidates, sha_index, map, by_source);
            }
        }
        _ => {}
    }
}

/// Stem of a source-path key: the effective basename (after any `::`). A file
/// extension is required unless the whole key looks like a path — Blockland
/// references textures without an extension, and the current file keeps its
/// own.
fn key_stem(key: &str) -> Option<String> {
    let b = effective_base(key);
    let stem = if b.contains('.') {
        Path::new(&b).file_stem()?.to_string_lossy().into_owned()
    } else if key.contains('/') && !key.contains(':') && !is_hex_like(&b) {
        b.clone()
    } else {
        return None;
    };
    let s = part(&stem);
    (!s.is_empty()).then_some(s)
}

/// First original-name field that yields a usable stem. Path-like fields may
/// omit the extension (the engine appends it); `id` values without one (brick
/// data ids) are rejected on purpose.
fn original_stem(obj: &serde_json::Map<String, Value>) -> Option<String> {
    for f in ["source", "mesh_id", "virtual_path", "path"] {
        if let Some(s) = obj.get(f).and_then(|v| v.as_str())
            && !s.is_empty()
        {
            let b = effective_base(s);
            if b.contains('.') {
                if let Some(stem) = file_stem_of(s) {
                    return Some(stem);
                }
            } else if !is_hex_like(&b) {
                let stem = part(&b);
                if !stem.is_empty() {
                    return Some(stem);
                }
            }
        }
    }
    if let Some(s) = obj.get("id").and_then(|v| v.as_str())
        && !s.is_empty()
        && effective_base(s).contains('.')
        && let Some(stem) = file_stem_of(s)
    {
        return Some(stem);
    }
    if let Some(arr) = obj.get("source_paths").and_then(|v| v.as_array()) {
        for s in arr.iter().filter_map(Value::as_str) {
            let b = basename(s);
            if b.contains('.')
                && let Some(stem) = file_stem_of(s)
                && !stem.is_empty()
            {
                return Some(stem);
            }
        }
    }
    None
}

static EMPTY_MAP: BTreeMap<String, String> = BTreeMap::new();
static EMPTY_LAST2: BTreeMap<(String, String), String> = BTreeMap::new();

/// Per-package rewrite context for one manifest.
struct RewriteCtx<'a> {
    rel: &'a BTreeMap<String, String>,
    last2: &'a BTreeMap<(String, String), String>,
    base: &'a BTreeMap<String, String>,
    global_base: &'a BTreeMap<String, String>,
}

/// Recursive reference rewrite. Resolution order for a string value:
/// 1. package-relative path (normalized) — exact file identity;
/// 2. (parent dir name, basename) within the same package;
/// 3. any basename unique within the same package;
/// 4. globally unambiguous basename (cross-package references).
fn rewrite_value(v: &mut Value, ctx: &RewriteCtx, touched: &mut bool) {
    match v {
        Value::String(s) => {
            if let Some(idx) = s.rfind('/') {
                let norm = norm_rel(s);
                if let Some(nb) = ctx.rel.get(&norm) {
                    *s = format!("{}/{}", &s[..idx], nb);
                    *touched = true;
                    return;
                }
                let parent = s[..idx].rfind('/').map(|i| &s[i + 1..]).unwrap_or("");
                let b = &s[idx + 1..];
                if !parent.is_empty()
                    && let Some(nb) = ctx.last2.get(&(parent.to_string(), b.to_string()))
                {
                    *s = format!("{}/{}", &s[..idx], nb);
                    *touched = true;
                    return;
                }
            }
            let b = s.rsplit('/').next().unwrap_or(s);
            if let Some(nb) = ctx.base.get(b).or_else(|| ctx.global_base.get(b)) {
                *s = if s.contains('/') {
                    format!("{}{}", &s[..s.len() - b.len()], nb)
                } else {
                    nb.clone()
                };
                *touched = true;
            }
        }
        Value::Object(map) => {
            for val in map.values_mut() {
                rewrite_value(val, ctx, touched);
            }
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                rewrite_value(item, ctx, touched);
            }
        }
        _ => {}
    }
}

const ANCHORS: &[&str] = &[
    "package.json",
    "addon.json",
    "import-report.json",
    "inventory.json",
];

/// Rewrite scopes for a file. Anchored files scope to their package root only.
/// Unanchored (flat-pass) files store entries under every ancestor directory
/// up to the content root, so a flat pack's manifest and its assets share a
/// common lookup map even though neither carries an anchor file.
fn package_scopes(file: &Path, root: &Path) -> Vec<PathBuf> {
    let pkg = package_root(file, root);
    if ANCHORS.iter().any(|a| pkg.join(a).exists()) {
        return vec![pkg];
    }
    let mut chain = Vec::new();
    let mut dir = file
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| root.to_path_buf());
    loop {
        chain.push(dir.clone());
        if dir == *root {
            break;
        }
        match dir.parent() {
            Some(p) => dir = p.to_path_buf(),
            None => break,
        }
    }
    chain
}

/// Nearest directory containing a package anchor; flat conversion passes and
/// unanchored files fall back to their own directory (or the parent of a
/// models/brick-catalog/bricks folder), mirroring migrate-content.
fn package_root(file: &Path, root: &Path) -> PathBuf {
    let mut dir = file
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| file.to_path_buf());
    loop {
        if ANCHORS.iter().any(|a| dir.join(a).exists()) {
            return dir;
        }
        if dir == root {
            break;
        }
        match dir.parent() {
            Some(p) => dir = p.to_path_buf(),
            None => break,
        }
    }
    if let Some(parent) = file.parent() {
        let name = parent.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if matches!(name, "models" | "brick-catalog" | "bricks")
            && parent.starts_with(root)
            && let Some(p) = parent.parent()
        {
            return p.to_path_buf();
        }
    }
    file.parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| root.to_path_buf())
}

/// Insert into a map, dropping the key when two different values conflict.
fn put_unique<K: Ord + Clone, V: PartialEq + Clone>(m: &mut BTreeMap<K, V>, k: K, v: V) {
    match m.insert(k.clone(), v.clone()) {
        Some(prev) if prev != v => {
            m.remove(&k);
        }
        _ => {}
    }
}

// --- Small helpers -----------------------------------------------------------

/// A basename is a rename candidate when it starts with >= 16 lowercase hex
/// characters followed by one of the known asset extensions.
fn is_candidate_base(base: &str) -> bool {
    let Some((_, rest)) = split_hash(base) else {
        return false;
    };
    matches!(
        rest,
        ".glb"
            | ".png"
            | ".jpg"
            | ".jpeg"
            | ".wav"
            | ".ogg"
            | ".source"
            | ".json"
            | ".scene.json"
            | ".terrain.json"
            | ".interior.json"
            | ".shape.json"
            | ".source.json"
            | ".clips.json"
            | ".world.json"
            | ".preview.jpg"
            | ".preview.png"
    )
}

/// Split a basename into (hex prefix, rest-including-dot) when the name starts
/// with >= 16 lowercase hex characters.
fn split_hash(base: &str) -> Option<(&str, &str)> {
    let bytes = base.as_bytes();
    let mut n = 0;
    while n < bytes.len() && matches!(bytes[n], b'0'..=b'9' | b'a'..=b'f') {
        n += 1;
    }
    if n >= 16 && n < bytes.len() {
        Some((&base[..n], &base[n..]))
    } else {
        None
    }
}

/// The extension part of a candidate basename, dot included (`.glb`, `.terrain.json`).
fn hash_rest(base: &str) -> String {
    split_hash(base)
        .map(|(_, rest)| rest.to_string())
        .unwrap_or_default()
}

/// Sanitize one part of an original name exactly like the importer does
/// (`Names::part` in lib.rs): Windows-forbidden and control characters become
/// `_`, trailing spaces/dots are dropped.
fn part(p: &str) -> String {
    p.chars()
        .map(|c| {
            if c.is_control() || matches!(c, '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .collect::<String>()
        .trim_end_matches([' ', '.'])
        .to_owned()
}

/// Sanitized file stem of a (possibly virtual or zip-qualified) source path.
fn file_stem_of(src: &str) -> Option<String> {
    let b = effective_base(src);
    if !b.contains('.') {
        return None;
    }
    Path::new(&b)
        .file_stem()
        .map(|s| part(s.to_string_lossy().as_ref()))
        .filter(|s| !s.is_empty())
}

/// Effective basename: after the last path separator and any `::` (zip-entry
/// sources like `Add-Ons/x.zip::name.png`).
fn effective_base(s: &str) -> String {
    let b = s.rsplit('/').next().unwrap_or(s);
    match b.rfind("::") {
        Some(i) => b[i + 2..].to_string(),
        None => b.to_string(),
    }
}

/// True for strings that are themselves hash-like (>= 16 lowercase hex chars):
/// never usable as an original file name.
fn is_hex_like(s: &str) -> bool {
    s.len() >= 16 && s.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
}

fn basename(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

fn base_name(p: &Path) -> String {
    p.file_name()
        .and_then(|n| n.to_str())
        .map(str::to_owned)
        .unwrap_or_default()
}

/// Collapse empty segments and leading `./` in a relative path.
fn norm_rel(p: &str) -> String {
    p.split('/')
        .filter(|s| !s.is_empty() && *s != ".")
        .collect::<Vec<_>>()
        .join("/")
}

/// Path relative to `root`, for reports.
fn rel_to(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).display().to_string()
}

/// Walk a tree, skipping any `.downloads` directory (sealed cache). Returns the
/// file list and how many files were skipped.
fn walk_skip_downloads(dir: &Path) -> (Vec<PathBuf>, usize) {
    let mut out = Vec::new();
    let mut skipped = 0usize;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if p.file_name().and_then(|n| n.to_str()) == Some(".downloads") {
                    // Count what we are skipping, without descending.
                    for f in walk_skip_downloads(&p).0 {
                        let _ = f;
                        skipped += 1;
                    }
                } else {
                    stack.push(p);
                }
            } else if p.is_file() {
                out.push(p);
            }
        }
    }
    (out, skipped)
}
