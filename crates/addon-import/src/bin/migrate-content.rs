//! Temporary in-place content migrator (throwaway tool).
//!
//! Converts every legacy `.shape.json` / `.brick.json` model file — plus the
//! plain `.json` item models named by a base-game presentation table or a
//! `weapons.json` resource entry (vendored copies of those models) — under a
//! content root to `.glb` and rewrites each package's manifests so the
//! GLB-only runtime can draw them, then deletes texture files that now ride
//! inside their `.glb` when nothing else references them.
//!
//! Scope rules (verified against the runtime loaders):
//! - Item/weapon/explosion presentation models: required to become `.glb`
//!   (`crates/client/src/items.rs` faults on legacy JSON). Their locally
//!   referenced textures are embedded into the `.glb` and their presentation
//!   bindings cleared; stock `base/...` bindings stay untouched.
//! - Brick native meshes: required to be `.glb` (`crates/client/src/content.rs`).
//! - Vehicle / avatar model files not named by a presentation: converted
//!   geometry-only (their loaders resolve pack textures themselves).
//! - Map bundle assets (`bundle.json` `assets` values) are SKIPPED on purpose:
//!   the map scene loader (`crates/render/src/scene_loader.rs`) still reads
//!   those shapes as JSON and has no GLB path. Converting them would break
//!   maps until that loader gains a GLB branch (a separate follow-up).
//! - Texture-table rows and files are pruned/deleted only when unreferenced
//!   by EVERY manifest in the content root, not just their own package:
//!   item manifests merge across packs (`merge_part`), so an Add-On may bind
//!   a swatch row (e.g. `base/data/shapes/yellow.png`) that another pack
//!   provides; deleting that row or file would unbind its materials.
//!
//! Usage: `migrate-content CONTENT_ROOT [--apply]`
//!
//! Dry-run by default: computes and reports every change without touching any
//! file (except writing MIGRATE-REPORT.json into the root). Pass `--apply` to
//! write the `.glb`s, rewrite manifests, delete the old JSONs and dedupe
//! textures. In dry-run mode the report lists what WOULD be rewritten/deleted.
//!
//! TEMPORARY TOOL: once all content is converted, delete this file and its
//! `[[bin]]` entry from Cargo.toml.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use bri_addon_import::{source::hash, installed_brick_surfaces};
use bri_content::brick::Brick;
use bri_content::shape::Shape;
use serde_json::Value;

const MAX_TEXTURE: u64 = 16 * 1024 * 1024;

struct Args {
    root: PathBuf,
    apply: bool,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Ok(args) = parse_args(&args) else {
        eprintln!("usage: migrate-content CONTENT_ROOT [--apply]");
        return ExitCode::from(2);
    };
    match run(&args) {
        Ok(report) => {
            println!("{}", report.summary_line());
            if report.failed > 0 {
                println!(
                    "MIGRATE-REPORT: {} failure(s); see MIGRATE-REPORT.json. Old JSON files for failed entries were left in place.",
                    report.failed
                );
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("migrate-content: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn parse_args(args: &[String]) -> Result<Args> {
    let mut root = None;
    let mut apply = false;
    for a in args {
        match a.as_str() {
            "--apply" => apply = true,
            other if !other.starts_with('-') => {
                anyhow::ensure!(root.is_none(), "unexpected argument: {other}");
                root = Some(PathBuf::from(other));
            }
            other => anyhow::bail!("unknown flag: {other}"),
        }
    }
    let Some(root) = root else {
        anyhow::bail!("missing CONTENT_ROOT")
    };
    let root = root.canonicalize().context("content root does not exist")?;
    Ok(Args { root, apply })
}

/// What converting one shape file does to its presentation bindings.
struct ShapePlan {
    material_count: usize,
    /// Material index -> (embed name, bytes) for locally referenced textures.
    textures: BTreeMap<usize, (String, Vec<u8>)>,
    /// Binding indices cleared to "" (their texture now rides in the GLB or
    /// their colour rides in the material's factor).
    cleared: BTreeSet<usize>,
    /// Local texture files embedded into this model's GLB (dedup candidates).
    embedded_files: Vec<PathBuf>,
    /// Material index -> solid colour as a linear RGBA `baseColorFactor`,
    /// where the binding resolved to one single-colour picture. No bytes
    /// ride in the GLB; the factor carries the colour.
    solids: BTreeMap<usize, [f32; 4]>,
}

struct PackageReport {
    root: String,
    shapes: usize,
    bricks: usize,
    twin_reused: usize,
    skipped: Vec<(String, String)>,
    failures: Vec<(String, String)>,
    manifests: Vec<String>,
    pruned_rows: usize,
    deduped: Vec<String>,
}

struct Report {
    mode: &'static str,
    root: String,
    packages: Vec<PackageReport>,
    failed: usize,
    converted: usize,
    twin_reused: usize,
    skipped_existing_glb: usize,
    map_assets_skipped: usize,
    manifests: usize,
    pruned_rows: usize,
    deduped: usize,
    pins_refreshed: usize,
}

impl Report {
    fn summary_line(&self) -> String {
        format!(
            "[{}] root {}: {} models converted ({} shapes, {} bricks), {} twin GLBs reused, \
             {} skipped existing .glb, {} map assets left as JSON, {} failed; {} manifests \
             rewritten, {} texture-table rows pruned, {} textures deduped, \
             {} physics pins re-pinned",
            self.mode,
            self.root,
            self.converted,
            self.packages.iter().map(|p| p.shapes).sum::<usize>(),
            self.packages.iter().map(|p| p.bricks).sum::<usize>(),
            self.twin_reused,
            self.skipped_existing_glb,
            self.map_assets_skipped,
            self.failed,
            self.manifests,
            self.pruned_rows,
            self.deduped,
            self.pins_refreshed,
        )
    }
}

fn run(args: &Args) -> Result<Report> {
    let root = &args.root;
    let apply = args.apply;

    // The stock surface pictures converted bricks' runtime-coloured
    // placeholders carry (the bundle lives under the content root; absent:
    // plain factors).
    let surfaces = match installed_brick_surfaces(root) {
        Some(s) => s,
        None => {
            eprintln!(
                "migrate-content: no brick-materials bundle found under {}; brick placeholders will ship plain",
                root.display()
            );
            bri_gltf::SurfaceTextures::default()
        }
    };

    // --- Discover ---------------------------------------------------------
    let all_files = walk(root);
    let mut model_files: Vec<PathBuf> = all_files
        .iter()
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".shape.json") || n.ends_with(".brick.json"))
        })
        .cloned()
        .collect();

    // Base-game presentation packs name their item models as plain `.json`
    // files (no `.shape.json` suffix): pick those up from every presentation's
    // `models` table so they convert like any other shape.
    let mut known: BTreeSet<PathBuf> = model_files.iter().cloned().collect();
    for pj in all_files
        .iter()
        .filter(|p| p.file_name().is_some_and(|n| n == "presentation.json"))
    {
        let Ok(text) = fs::read_to_string(pj) else { continue };
        let Ok(v) = serde_json::from_str::<Value>(&text) else { continue };
        let Some(models) = v.get("models").and_then(|m| m.as_object()) else {
            continue
        };
        for entry in models.values() {
            let Some(file) = entry.get("file").and_then(|f| f.as_str()) else {
                continue
            };
            let base = Path::new(file)
                .file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default();
            if !base.ends_with(".json")
                || base.ends_with(".shape.json")
                || base.ends_with(".brick.json")
            {
                continue;
            }
            let path = pj.parent().unwrap_or(root).join(file);
            if path.is_file() && known.insert(path.clone()) {
                model_files.push(path);
            }
        }
    }

    // weapons.json resources reference vendored shape copies by their original
    // DTS `path`; convert the local plain-`.json` ones too (the runtime only
    // checks safe-path + existence for them, and their sha256 fields stay the
    // original DTS hashes — provenance compares those).
    for wj in all_files
        .iter()
        .filter(|p| p.file_name().is_some_and(|n| n == "weapons.json"))
    {
        let Ok(text) = fs::read_to_string(wj) else { continue };
        let Ok(v) = serde_json::from_str::<Value>(&text) else { continue };
        let Some(resources) = v.get("resources").and_then(|r| r.as_array()) else {
            continue
        };
        for entry in resources {
            let (Some(path), Some(native)) = (
                entry.get("path").and_then(|p| p.as_str()),
                entry.get("native_file").and_then(|f| f.as_str()),
            ) else {
                continue
            };
            if !path.ends_with(".dts") {
                continue; // e.g. generated sound resources
            }
            let base = Path::new(native)
                .file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default();
            if !base.ends_with(".json")
                || base.ends_with(".shape.json")
                || base.ends_with(".brick.json")
            {
                continue;
            }
            let file = wj.parent().unwrap_or(root).join(native);
            if file.is_file() && known.insert(file.clone()) {
                model_files.push(file);
            }
        }
    }
    model_files.sort();

    // Map-bundle geometry stays JSON (the scene loader reads it as such).
    let mut map_assets: BTreeSet<String> = BTreeSet::new();
    for b in all_files
        .iter()
        .filter(|p| p.file_name().is_some_and(|n| n == "bundle.json"))
    {
        if let Ok(text) = fs::read_to_string(b)
            && let Ok(v) = serde_json::from_str::<Value>(&text)
            && let Some(assets) = v.get("assets").and_then(|a| a.as_object())
        {
            for file in assets.values().filter_map(|f| f.as_str()) {
                map_assets.insert(base_name(Path::new(file)));
            }
        }
    }

    // Group model files by package root.
    let mut by_root: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();
    for f in &model_files {
        let base = f
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if map_assets.contains(&base) {
            continue;
        }
        by_root
            .entry(package_root(f, root))
            .or_default()
            .push(f.clone());
    }

    let mut report = Report {
        mode: if apply { "apply" } else { "dry-run" },
        root: root.display().to_string(),
        packages: Vec::new(),
        failed: 0,
        converted: 0,
        twin_reused: 0,
        skipped_existing_glb: 0,
        map_assets_skipped: model_files.len() - by_root.values().map(Vec::len).sum::<usize>(),
        manifests: 0,
        pruned_rows: 0,
        deduped: 0,
        pins_refreshed: 0,
    };

    // Source-bytes hash -> (GLB bytes, did the source carry a material plan).
    // A byte-identical vendored copy of a planned shape reuses that GLB so its
    // materials keep the colours/textures the first export embedded.
    let mut twins: BTreeMap<String, (Vec<u8>, bool)> = BTreeMap::new();

    for (pkg_root, files) in &by_root {
        let mut pr = PackageReport {
            root: pkg_root.display().to_string(),
            shapes: 0,
            bricks: 0,
            twin_reused: 0,
            skipped: Vec::new(),
            failures: Vec::new(),
            manifests: Vec::new(),
            pruned_rows: 0,
            deduped: Vec::new(),
        };

        // --- Pass 1: convert every model file -----------------------------
        let mut renames: BTreeMap<String, String> = BTreeMap::new();
        let mut hashes: BTreeMap<String, String> = BTreeMap::new();
        let mut plans: BTreeMap<String, ShapePlan> = BTreeMap::new();

        for f in files {
            let base = f
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let is_brick = base.ends_with(".brick.json");
            let stem = base
                .strip_suffix(".shape.json")
                .or_else(|| base.strip_suffix(".brick.json"))
                .or_else(|| base.strip_suffix(".json"))
                .unwrap_or(&base)
                .to_string();
            let glb = f.with_file_name(format!("{stem}.glb"));
            if glb.exists() {
                pr.skipped.push((
                    f.display().to_string(),
                    "a .glb sibling already exists".into(),
                ));
                report.skipped_existing_glb += 1;
                continue;
            }

            let bytes = match fs::read(f) {
                Ok(b) => b,
                Err(e) => {
                    pr.failures.push((f.display().to_string(), e.to_string()));
                    report.failed += 1;
                    continue;
                }
            };

            if !is_brick {
                let mut shape: Shape = match serde_json::from_slice(&bytes) {
                    Ok(s) => s,
                    Err(e) => {
                        pr.failures
                            .push((f.display().to_string(), format!("parse: {e}")));
                        report.failed += 1;
                        continue;
                    }
                };
                // A shape with no geometry at all (an empty anchor) cannot be
                // held by glTF and draws nothing in the game either; leave it.
                if shape.meshes.iter().flatten().count() == 0 {
                    pr.skipped.push((
                        f.display().to_string(),
                        "no geometry to convert".into(),
                    ));
                    continue;
                }
                // Presentation-named models embed their locally referenced
                // textures; everything else stays geometry-only.
                let plan = resolve_shape_plan(f, pkg_root, &shape);
                for (i, c) in &plan.solids {
                    if *i < shape.materials.len() {
                        shape.materials[*i].color = Some(*c);
                    }
                }
                let mut tex_map: BTreeMap<usize, Option<(String, Vec<u8>)>> = BTreeMap::new();
                for i in 0..shape.materials.len() {
                    match plan.textures.get(&i) {
                        Some((name, data)) => tex_map.insert(i, Some((name.clone(), data.clone()))),
                        None => tex_map.insert(i, None),
                    };
                }
                // A byte-identical copy of a shape that already exported with
                // its material plan reuses that GLB: a geometry-only export
                // would drop the colours/texture the twin carries.
                let had_plan = !plan.textures.is_empty() || !plan.solids.is_empty();
                let mut glb_bytes: Option<Vec<u8>> = None;
                if !had_plan {
                    if let Some((glb, planned)) = twins.get(&hash(&bytes)) && *planned {
                        pr.twin_reused += 1;
                        report.twin_reused += 1;
                        glb_bytes = Some(glb.clone());
                    }
                }
                if glb_bytes.is_none() {
                    let mut notes = Vec::new();
                    match bri_gltf::export_shape(&shape, &tex_map, &mut notes) {
                        Ok(v) => glb_bytes = Some(v.0),
                        Err(e) => {
                            pr.failures.push((
                                f.display().to_string(),
                                format!(
                                    "export: {e:#}{}",
                                    if notes.is_empty() {
                                        String::new()
                                    } else {
                                        format!(" ({notes:?})")
                                    }
                                ),
                            ));
                            report.failed += 1;
                            continue;
                        }
                    };
                }
                let glb_bytes = glb_bytes.unwrap();
                twins.entry(hash(&bytes)).or_insert((glb_bytes.clone(), had_plan));
                let new_base = format!("{stem}.glb");
                renames.insert(base.clone(), new_base.clone());
                hashes.insert(new_base, hash(&glb_bytes));
                plans.insert(base, plan);
                if apply {
                    write_glb(&glb, &glb_bytes)?;
                    fs::remove_file(f).with_context(|| format!("removing {}", f.display()))?;
                }
                pr.shapes += 1;
            } else {
                let brick: Brick = match serde_json::from_slice(&bytes) {
                    Ok(b) => b,
                    Err(e) => {
                        pr.failures
                            .push((f.display().to_string(), format!("parse: {e}")));
                        report.failed += 1;
                        continue;
                    }
                };
                // The file's own folder under the content root, so its
                // placeholders reference the shared pictures by path from there.
                let glb_dir = f.parent().and_then(|d| d.strip_prefix(root).ok());
                let glb_bytes = match bri_gltf::export_brick_with_surfaces(&brick, &surfaces, glb_dir) {
                    Ok(b) => b,
                    Err(e) => {
                        pr.failures
                            .push((f.display().to_string(), format!("export: {e}")));
                        report.failed += 1;
                        continue;
                    }
                };
                let new_base = format!("{stem}.glb");
                renames.insert(base.clone(), new_base.clone());
                hashes.insert(new_base, hash(&glb_bytes));
                if apply {
                    write_glb(&glb, &glb_bytes)?;
                    fs::remove_file(f).with_context(|| format!("removing {}", f.display()))?;
                }
                pr.bricks += 1;
            }
        }

        // Nothing converted in this package: nothing to rewrite.
        if renames.is_empty() {
            report.packages.push(pr);
            continue;
        }

        // --- Pass 2+3: rewrite manifests ----------------------------------
        // Lower-cased text of every manifest OUTSIDE this package, built lazily
        // and shared by pruning + de-duplication. Merged manifests share texture
        // keys across packs (an Add-On may bind a swatch row the base pack
        // provides), so only globally unreferenced rows/files may go.
        let mut external: Option<ExternalTexts> = None;
        let json_files: Vec<PathBuf> = all_files
            .iter()
            .filter(|p| {
                p.starts_with(pkg_root) && p.file_name().is_some_and(|n| n == "presentation.json")
            })
            .cloned()
            .collect();
        let mut other_json: Vec<PathBuf> = all_files
            .iter()
            .filter(|p| {
                p.starts_with(pkg_root)
                    && p.file_name().is_some_and(|n| {
                        n.to_string_lossy().ends_with(".json")
                            && n != "presentation.json"
                            && n != "MIGRATE-REPORT.json"
                    })
            })
            .cloned()
            .collect();
        other_json.sort();

        // Schema-aware presentation rewrite + texture-table pruning.
        let mut final_presentation_texts: BTreeMap<PathBuf, String> = BTreeMap::new();
        // The same texts minus their `textures` tables: a row's own key is not
        // a reference to the row, so it must not count when deciding whether
        // anything still uses an id.
        let mut stripped_presentation_texts: BTreeMap<PathBuf, String> = BTreeMap::new();
        let mut pruned_file_paths: Vec<PathBuf> = Vec::new();
        for pj in &json_files {
            let Ok(text) = fs::read_to_string(pj) else {
                continue;
            };
            let Ok(mut v) = serde_json::from_str::<Value>(&text) else {
                pr.failures.push((
                    pj.display().to_string(),
                    "presentation is not valid JSON".into(),
                ));
                report.failed += 1;
                // Keep the raw text in both scan sets: it may reference table
                // ids (pruning) or texture files (dedupe), and we cannot tell
                // which without parsing it.
                final_presentation_texts.insert(pj.clone(), text.clone());
                stripped_presentation_texts.insert(pj.clone(), text);
                continue;
            };
            let changed = rewrite_presentation(&mut v, &renames, &hashes, &plans);

            // Prune texture-table rows whose id no binding (or anything else in
            // the package) references. The scan covers every other manifest's
            // current text plus every presentation's post-rewrite text with its
            // own `textures` table stripped out.
            let mut pruned = 0usize;
            if changed && v.get("textures").and_then(|t| t.as_object()).is_some() {
                let others: Vec<String> = other_json
                    .iter()
                    .filter_map(|p| fs::read_to_string(p).ok())
                    .collect();
                let table_keys: BTreeSet<String> = v
                    .get("textures")
                    .and_then(|t| t.as_object())
                    .map(|o| o.keys().cloned().collect())
                    .unwrap_or_default();
                let mut referenced: BTreeSet<String> = BTreeSet::new();
                for t in others.iter().chain(stripped_presentation_texts.values()) {
                    collect_quoted_strings(t, &mut |s| {
                        if table_keys.contains(s) {
                            referenced.insert(s.to_string());
                        }
                    });
                }
                let mut own = v.clone();
                if let Some(o) = own.as_object_mut() {
                    o.remove("textures");
                }
                let own = serde_json::to_string(&own).unwrap_or_default();
                collect_quoted_strings(&own, &mut |s| {
                    if table_keys.contains(s) {
                        referenced.insert(s.to_string());
                    }
                });
                if let Some(table) = v.get_mut("textures").and_then(|t| t.as_object_mut()) {
                    let assets_dir = pj.parent().unwrap_or(pkg_root);
                    let keys: Vec<String> = table.keys().cloned().collect();
                    for k in keys {
                        if !referenced.contains(&k) {
                            // Another pack may bind this row through the merged
                            // manifest (Torque paths are case-insensitive).
                            let ext = external.get_or_insert_with(|| external_texts(&all_files, pkg_root));
                            let pat = format!("\"{}\"", k.to_ascii_lowercase());
                            if ext.blob.contains(&pat) {
                                continue;
                            }
                            if let Some(file) = table
                                .get(&k)
                                .and_then(|r| r.get("file"))
                                .and_then(|f| f.as_str())
                            {
                                pruned_file_paths.push(assets_dir.join(file));
                            }
                            table.remove(&k);
                            pruned += 1;
                        }
                    }
                }
            }
            pr.pruned_rows += pruned;

            let final_text = serde_json::to_string_pretty(&v).unwrap_or_default() + "\n";
            if changed || pruned > 0 {
                if apply {
                    fs::write(pj, &final_text)
                        .with_context(|| format!("writing {}", pj.display()))?;
                }
                pr.manifests.push(base_name(pj));
                report.manifests += 1;
            }
            let mut stripped = v.clone();
            if let Some(o) = stripped.as_object_mut() {
                o.remove("textures");
            }
            final_presentation_texts.insert(pj.clone(), final_text);
            stripped_presentation_texts.insert(
                pj.clone(),
                serde_json::to_string(&stripped).unwrap_or_default(),
            );
        }

        // Generic basename rewrite of every other manifest in the package.
        for oj in &other_json {
            let Ok(text) = fs::read_to_string(oj) else {
                continue;
            };
            if !text_contains_old_base(&text, &renames) {
                continue;
            }
            let Ok(mut v) = serde_json::from_str::<Value>(&text) else {
                pr.failures.push((
                    oj.display().to_string(),
                    "not valid JSON; left untouched".into(),
                ));
                report.failed += 1;
                continue;
            };
            let mut touched = false;
            rewrite_value(&mut v, &renames, &hashes, &mut touched);
            if !touched {
                continue;
            }
            let new_text = serde_json::to_string_pretty(&v).unwrap_or_default() + "\n";
            if apply {
                fs::write(oj, &new_text).with_context(|| format!("writing {}", oj.display()))?;
            }
            pr.manifests.push(base_name(oj));
            report.manifests += 1;
        }

        // --- Pass 4: texture de-duplication --------------------------------
        // Candidates: textures embedded into converted GLBs, plus files whose
        // table rows were pruned. A file stays while any manifest still names
        // a string ending in its basename (stock paths sharing a basename block
        // deletion — the safe direction).
        let mut candidates: BTreeSet<PathBuf> = BTreeSet::new();
        for plan in plans.values() {
            for c in &plan.embedded_files {
                if !c.as_os_str().is_empty() && c.starts_with(pkg_root) && c.is_file() {
                    candidates.insert(c.clone());
                }
            }
        }
        for c in &pruned_file_paths {
            if c.starts_with(pkg_root) && c.is_file() {
                candidates.insert(c.clone());
            }
        }
        if !candidates.is_empty() {
            let mut texts: Vec<String> = final_presentation_texts.values().cloned().collect();
            for oj in &other_json {
                // import-report.json's `package.files` is the importer's
                // inventory, not a runtime reference: nothing that loads an
                // Add-On checks those paths exist, so it must not block.
                if base_name(oj) == "import-report.json" {
                    continue;
                }
                if let Ok(t) = fs::read_to_string(oj) {
                    texts.push(t);
                }
            }
            for c in candidates.iter() {
                let base = c
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                let pat1 = format!("/{base}\"");
                let pat2 = format!("\"{base}\"");
                if texts.iter().any(|t| t.contains(&pat1) || t.contains(&pat2)) {
                    continue; // still referenced somewhere: keep it.
                }
                // Cross-pack guard: the same basename living in another pack is
                // that pack's own file, but a merged-manifest binding or table
                // row naming this key needs THIS one — when in doubt, keep it.
                let ext = external.get_or_insert_with(|| external_texts(&all_files, pkg_root));
                let lbase = base.to_ascii_lowercase();
                if ext.blob.contains(&format!("/{lbase}\"")) || ext.blob.contains(&format!("\"{lbase}\"")) {
                    continue;
                }
                if apply {
                    match fs::remove_file(c) {
                        Ok(()) => pr.deduped.push(c.display().to_string()),
                        Err(e) => {
                            pr.failures
                                .push((c.display().to_string(), format!("dedupe: {e}")));
                            report.failed += 1;
                            continue;
                        }
                    };
                } else {
                    // Dry-run: nothing is deleted, only reported.
                    pr.deduped.push(format!("{} (would delete)", c.display()));
                }
                report.deduped += 1;
            }
        }

        report.converted += pr.shapes + pr.bricks;
        report.pruned_rows += pr.pruned_rows;
        report.packages.push(pr);
    }

    // Re-pin physics manifests whose bound weapons.json was rewritten in a
    // different package (the base item-presentation pack pins the base
    // weapons pack's file). A stale pin fails game start with "Item physics
    // weapon checksum mismatch".
    report.pins_refreshed = refresh_physics_pins(root, &all_files, apply)?;

    // --- Report ------------------------------------------------------------
    let report_path = root.join("MIGRATE-REPORT.json");
    let json = serde_json::json!({
        "tool": "migrate-content",
        "mode": report.mode,
        "root": &report.root,
        "totals": {
            "converted": report.converted,
            "twin_reused": report.twin_reused,
            "skipped_existing_glb": report.skipped_existing_glb,
            "map_assets_left_as_json": report.map_assets_skipped,
            "failed": report.failed,
            "manifests_rewritten": report.manifests,
            "texture_rows_pruned": report.pruned_rows,
            "textures_deduped": report.deduped,
            "physics_pins_repinned": report.pins_refreshed,
        },
        "packages": report.packages.iter().map(|p| serde_json::json!({
            "root": &p.root,
            "shapes_converted": p.shapes,
            "bricks_converted": p.bricks,
            "twin_reused": p.twin_reused,
            "skipped": p.skipped,
            "failures": p.failures,
            "manifests_rewritten": p.manifests,
            "textures_deduped": p.deduped,
        })).collect::<Vec<_>>(),
    });
    fs::write(
        &report_path,
        serde_json::to_string_pretty(&json).unwrap_or_default() + "\n",
    )
    .with_context(|| format!("writing {}", report_path.display()))?;

    Ok(report)
}

/// For a presentation-named shape, decide which materials embed their local
/// texture (binding resolves to a file inside the package). Stock `base/...`
/// and empty bindings are left alone on purpose.
fn resolve_shape_plan(model_file: &Path, pkg_root: &Path, shape: &Shape) -> ShapePlan {
    let mut plan = ShapePlan {
        material_count: shape.materials.len(),
        textures: BTreeMap::new(),
        cleared: BTreeSet::new(),
        embedded_files: Vec::new(),
        solids: BTreeMap::new(),
    };

    // Find every presentation.json in this package.
    let presentations: Vec<PathBuf> = walk(pkg_root)
        .into_iter()
        .filter(|p| p.file_name().is_some_and(|n| n == "presentation.json"))
        .collect();

    let base = model_file.file_name().unwrap_or_default().to_string_lossy();
    for pj in &presentations {
        let Ok(text) = fs::read_to_string(pj) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let Some(models) = v.get("models").and_then(|m| m.as_object()) else {
            continue;
        };
        let table = v
            .get("textures")
            .and_then(|t| t.as_object())
            .cloned()
            .unwrap_or_default();
        let assets_dir = pj.parent().unwrap_or(pkg_root);
        for entry in models.values() {
            let Some(file) = entry.get("file").and_then(|f| f.as_str()) else {
                continue;
            };
            if Path::new(file)
                .file_name()
                .map(|n| n.to_string_lossy() != base)
                .unwrap_or(true)
            {
                continue;
            }
            let Some(bindings) = entry.get("textures").and_then(|t| t.as_array()) else {
                continue;
            };
            for (i, b) in bindings.iter().enumerate() {
                let Some(b) = b.as_str() else { continue };
                if i >= shape.materials.len()
                    || plan.cleared.contains(&i)
                    || b.is_empty()
                    || b.starts_with("base/")
                {
                    continue;
                }
                // Resolve the binding to a local file: table id first, then a
                // direct path relative to the assets dir.
                let candidate = if let Some(row) = table.get(b) {
                    row.get("file")
                        .and_then(|f| f.as_str())
                        .map(|f| assets_dir.join(f))
                } else if b.contains('/') {
                    Some(assets_dir.join(b))
                } else {
                    Some(assets_dir.join(b))
                        .filter(|p| p.is_file())
                        .or_else(|| Some(assets_dir.join("textures").join(b)))
                };
                let Some(path) = candidate else { continue };
                if !path.starts_with(pkg_root) || !path.is_file() {
                    continue;
                }
                let Ok(meta) = path.metadata() else { continue };
                if meta.len() > MAX_TEXTURE {
                    continue;
                }
                let Ok(bytes) = fs::read(&path) else { continue };
                // One single-colour picture needs no bytes at all: glTF's
                // baseColorFactor carries the colour (linear). A patterned
                // picture rides in as-is. Where the file is not a decodable
                // picture, keep the legacy behaviour: the binding stays and
                // the game draws the swatch from its table.
                let Some(img) = image::load_from_memory(&bytes).ok() else {
                    continue
                };
                let rgba = img.to_rgba8();
                let solid_pixel = match rgba.pixels().next() {
                    Some(p0) if rgba.pixels().all(|p| *p == *p0) => Some(*p0),
                    _ => None,
                };
                // glTF's default white factor means "no colour", so a pure
                // white opaque swatch must keep its picture.
                let embed = match solid_pixel {
                    Some(px) if px == image::Rgba([255, 255, 255, 255]) => true,
                    Some(px) => {
                        plan.solids.insert(
                            i,
                            [
                                linear_byte(px[0]),
                                linear_byte(px[1]),
                                linear_byte(px[2]),
                                px[3] as f32 / 255.0,
                            ],
                        );
                        false
                    }
                    None => true,
                };
                if embed {
                    plan.textures.insert(i, (shape.materials[i].name.clone(), bytes));
                    plan.embedded_files.push(path);
                }
                plan.cleared.insert(i);
            }
        }
    }
    plan
}

/// A picture's sRGB byte as glTF's linear factor channel.
fn linear_byte(b: u8) -> f32 {
    let c = b as f32 / 255.0;
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Schema-aware rewrite of one presentation.json's models table. Returns true
/// when anything changed.
fn rewrite_presentation(
    v: &mut Value,
    renames: &BTreeMap<String, String>,
    hashes: &BTreeMap<String, String>,
    plans: &BTreeMap<String, ShapePlan>,
) -> bool {
    let Some(models) = v.get_mut("models").and_then(|m| m.as_object_mut()) else {
        return false;
    };
    let mut changed = false;
    for entry in models.values_mut() {
        let Some(obj) = entry.as_object_mut() else {
            continue;
        };
        let Some(file) = obj.get("file").and_then(|f| f.as_str()) else {
            continue;
        };
        let base = Path::new(file)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Some(new_base) = renames.get(&base) else {
            continue;
        };
        let dir = file.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        obj.insert(
            "file".into(),
            Value::String(if dir.is_empty() {
                new_base.clone()
            } else {
                format!("{dir}/{new_base}")
            }),
        );
        if let Some(h) = hashes.get(new_base) {
            obj.insert("sha256".into(), Value::String(h.clone()));
        }
        changed = true;
        // Clear the bindings of materials whose texture now rides in the GLB.
        if let (Some(plan), Some(bindings)) = (
            plans.get(&base),
            obj.get_mut("textures").and_then(|t| t.as_array_mut()),
        ) && bindings.len() == plan.material_count
        {
            for (i, slot) in bindings.iter_mut().enumerate() {
                if plan.cleared.contains(&i) && slot.is_string() {
                    *slot = Value::String(String::new());
                }
            }
        }
    }
    changed
}

/// Generic recursive rewrite: any string whose basename is a converted model
/// file gets the new basename; if its object also carries a "sha256" key next
/// to a `file`/`native_mesh` reference, that becomes the new file's hash.
fn rewrite_value(
    v: &mut Value,
    renames: &BTreeMap<String, String>,
    hashes: &BTreeMap<String, String>,
    touched: &mut bool,
) {
    match v {
        Value::String(s) => {
            let base = s.rsplit('/').next().unwrap_or(s);
            if let Some(new_base) = renames.get(base) {
                // Strip the separator that preceded the old basename so the
                // join below cannot produce a double slash (the runtime's
                // safe_relative rejects empty path segments).
                let mut prefix = &s[..s.len() - base.len()];
                while prefix.ends_with('/') {
                    prefix = &prefix[..prefix.len() - 1];
                }
                *s = if prefix.is_empty() {
                    new_base.clone()
                } else {
                    format!("{prefix}/{new_base}")
                };
                *touched = true;
            }
        }
        Value::Object(map) => {
            let before = *touched;
            for val in map.values_mut() {
                rewrite_value(val, renames, hashes, touched);
            }
            if !before && *touched {
                // A string inside this object was just renamed: refresh the
                // sibling hash when it points at the same file. The reference
                // may already carry the new name (rewritten above), or still
                // the old one; `hashes` is keyed by the new basename.
                let file_base = map
                    .get("file")
                    .or_else(|| map.get("native_mesh"))
                    .or_else(|| map.get("path"))
                    .and_then(|f| f.as_str())
                    .map(Path::new)
                    .and_then(Path::file_name)
                    .map(|n| n.to_string_lossy().into_owned());
                if let Some(file_base) = file_base
                    && let Some(h) = hashes
                        .get(&file_base)
                        .or_else(|| renames.get(&file_base).and_then(|nb| hashes.get(nb)))
                        .cloned()
                        && let Some(hash_slot) = map.get_mut("sha256")
                {
                    *hash_slot = Value::String(h);
                }
            }
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                rewrite_value(item, renames, hashes, touched);
            }
        }
        _ => {}
    }
}

/// Lower-cased concatenation of every manifest outside one package.
struct ExternalTexts {
    blob: String,
}

/// Re-pin the `weapons_sha256` / `item_physics_sha256` fields of every
/// presentation.json against the files they bind, rewriting any that went
/// stale. Add-ons pin their sibling file; the base item-presentation pack
/// pins the base weapons pack's `weapons.json`, which lives in a different
/// package — when this run rewrites it, the old pin fails game start with
/// "Item physics weapon checksum mismatch". Returns how many presentations
/// were rewritten.
fn refresh_physics_pins(
    root: &Path,
    all_files: &[PathBuf],
    apply: bool,
) -> Result<usize> {
    // role -> dir from packages.json (the base pairing rule).
    let mut roles: BTreeMap<String, String> = BTreeMap::new();
    if let Ok(text) = fs::read_to_string(root.join("packages.json"))
        && let Ok(v) = serde_json::from_str::<Value>(&text)
        && let Some(pkgs) = v.get("packages").and_then(|p| p.as_array())
    {
        for p in pkgs {
            if let (Some(role), Some(dir)) = (
                p.get("role").and_then(|r| r.as_str()),
                p.get("dir").and_then(|d| d.as_str()),
            ) {
                roles.insert(role.to_string(), dir.to_string());
            }
        }
    }

    let mut refreshed = 0usize;
    for pj in all_files.iter().filter(|p| p.file_name().is_some_and(|n| n == "presentation.json"))
    {
        if pj.components().any(|c| c.as_os_str() == ".downloads") {
            continue; // download cache: never ours to re-stamp
        }
        let Ok(text) = fs::read_to_string(pj) else { continue };
        let Ok(mut v) = serde_json::from_str::<Value>(&text) else { continue };
        if !v.get("weapons_sha256").is_some() && !v.get("item_physics_sha256").is_some() {
            continue;
        }
        let dir = pj.parent().unwrap_or(root);

        // The weapons.json this presentation binds: its sibling, else — for
        // the base item-presentation pack — the base weapons pack's file.
        let mut weapons_target: Option<PathBuf> = None;
        if v.get("weapons_sha256").is_some() {
            let sibling = dir.join("weapons.json");
            if sibling.is_file() {
                weapons_target = Some(sibling);
            } else if let (Some(pres_dir), Some(wep_dir)) =
                (roles.get("item_presentation"), roles.get("weapons"))
                && dir.strip_prefix(root).ok() == Some(Path::new(pres_dir))
                && root.join(wep_dir).join("weapons.json").is_file()
            {
                weapons_target = Some(root.join(wep_dir).join("weapons.json"));
            }
        }

        let mut changed = false;
        if let (Some(target), Some(slot)) = (&weapons_target, v.get_mut("weapons_sha256")) {
            let bytes = fs::read(target)
                .with_context(|| format!("reading {}", target.display()))?;
            let fresh = hash(&bytes);
            if slot.as_str() != Some(fresh.as_str()) {
                *slot = Value::String(fresh);
                changed = true;
            }
        }
        if v.get("item_physics_sha256").is_some() && dir.join("item-physics.json").is_file() {
            let target = dir.join("item-physics.json");
            let bytes = fs::read(&target)
                .with_context(|| format!("reading {}", target.display()))?;
            let fresh = hash(&bytes);
            if v.get("item_physics_sha256").and_then(|s| s.as_str()) != Some(fresh.as_str()) {
                v["item_physics_sha256"] = Value::String(fresh);
                changed = true;
            }
        }

        if changed {
            if apply {
                let text = serde_json::to_string_pretty(&v).unwrap_or_default() + "\n";
                fs::write(pj, &text)
                    .with_context(|| format!("writing {}", pj.display()))?;
            }
            refreshed += 1;
        }
    }
    Ok(refreshed)
}

fn external_texts(all_files: &[PathBuf], pkg_root: &Path) -> ExternalTexts {
    let mut blob = String::new();
    for p in all_files {
        if !p.file_name().is_some_and(|n| n.to_string_lossy().ends_with(".json")) || base_name(p) == "MIGRATE-REPORT.json"
        {
            continue;
        }
        if p.starts_with(pkg_root) {
            continue;
        }
        if let Ok(t) = fs::read_to_string(p) {
            blob.push_str(&t.to_ascii_lowercase());
            blob.push('\n');
        }
    }
    ExternalTexts { blob }
}

fn text_contains_old_base(text: &str, renames: &BTreeMap<String, String>) -> bool {
    renames
        .keys()
        .any(|k| text.contains(&format!("\"{k}\"")) || text.contains(&format!("/{k}\"")))
}

/// Collects every JSON string value in `text` (escape-aware) and calls `f`.
fn collect_quoted_strings(text: &str, f: &mut dyn FnMut(&str)) {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'"' {
            i += 1;
            continue;
        }
        let start = i + 1;
        i += 1;
        while i < bytes.len() {
            match bytes[i] {
                b'\\' => i += 2,
                b'"' => break,
                _ => i += 1,
            }
        }
        if i >= bytes.len() {
            break;
        }
        f(&text[start..i]);
        i += 1;
    }
}

fn base_name(p: &Path) -> String {
    p.file_name()
        .and_then(|n| n.to_str())
        .map(str::to_owned)
        .unwrap_or_default()
}

fn write_glb(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("glb.tmp");
    fs::write(&tmp, bytes).with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("moving {} into place", path.display()))?;
    Ok(())
}

fn package_root(file: &Path, root: &Path) -> PathBuf {
    let mut dir = file
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| file.to_path_buf());
    loop {
        if dir.join("package.json").exists() || dir.join("addon.json").exists() {
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
    // Fallback: the parent of a models/brick-catalog/bricks folder, else the
    // file's own directory (flat conversion passes like maps-pass-008).
    if let Some(parent) = file.parent() {
        let name = parent.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if matches!(name, "models" | "brick-catalog" | "bricks" | "shapes")
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

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.is_file() {
                out.push(p);
            }
        }
    }
    out
}
