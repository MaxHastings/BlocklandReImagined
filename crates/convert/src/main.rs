use anyhow::{Context, Result, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::{Component, Path, PathBuf},
};

const MAX_ASSET: u64 = 16 * 1024 * 1024;

#[derive(Serialize)]
struct Record {
    source: String,
    virtual_path: String,
    source_sha256: String,
    output: Option<String>,
    error: Option<String>,
    warnings: Vec<String>,
}

fn bounded_read(input: impl Read) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    input.take(MAX_ASSET + 1).read_to_end(&mut data)?;
    ensure!(
        data.len() as u64 <= MAX_ASSET,
        "Asset exceeds {MAX_ASSET}-byte limit"
    );
    Ok(data)
}

fn normalize(value: &str) -> Result<String> {
    let value = value.replace('\\', "/");
    ensure!(
        !value.is_empty() && !value.contains(':') && !value.starts_with('/'),
        "Invalid virtual path"
    );
    ensure!(
        Path::new(&value)
            .components()
            .all(|c| matches!(c, Component::Normal(_))),
        "Unsafe virtual path"
    );
    Ok(value)
}

/// The add-on archives of the designated v20 reference
/// (`docs/vanilla-reference-inventory.json`): path and SHA-256.
struct Reference {
    archives: std::collections::BTreeMap<String, String>,
    seen: std::cell::RefCell<std::collections::BTreeSet<String>>,
}

impl Reference {
    fn load(path: &Path) -> Result<Self> {
        let inventory: serde_json::Value = serde_json::from_slice(
            &std::fs::read(path).with_context(|| format!("Reading {}", path.display()))?,
        )?;
        let mut archives = std::collections::BTreeMap::new();
        for package in inventory["packages"]
            .as_array()
            .context("Reference inventory lists no packages")?
        {
            let path = package["path"].as_str().context("Package lacks path")?;
            let sha256 = package["sha256"].as_str().context("Package lacks sha256")?;
            archives.insert(path.to_lowercase(), sha256.to_lowercase());
        }
        Ok(Self {
            archives,
            seen: Default::default(),
        })
    }

    /// Whether to convert this Add-Ons file: a listed archive with the
    /// reference's bytes. Anything else in the folder is skipped; a listed
    /// archive with other bytes is an error, as it would convert differently.
    fn check(&self, relative: &str, path: &Path) -> Result<bool> {
        let key = relative.to_lowercase();
        let Some(expected) = self.archives.get(&key) else {
            return Ok(false);
        };
        let actual = digest(&std::fs::read(path)?);
        ensure!(
            &actual == expected,
            "differs from the v20 reference archive (SHA-256 {actual}, expected {expected})"
        );
        self.seen.borrow_mut().insert(key);
        Ok(true)
    }

    fn missing(&self) -> Vec<String> {
        let seen = self.seen.borrow();
        self.archives
            .keys()
            .filter(|k| !seen.contains(*k))
            .map(|k| format!("{k}: missing; the v20 reference has it"))
            .collect()
    }
}

fn digest(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

fn convert(source: String, virtual_path: String, input: Result<Vec<u8>>, output: &Path) -> Record {
    let mut record = Record {
        source,
        virtual_path,
        source_sha256: String::new(),
        output: None,
        error: None,
        warnings: vec![],
    };
    let result = (|| -> Result<()> {
        let data = input?;
        record.source_sha256 = digest(&data);
        let id = format!("v20/{}", record.virtual_path.to_lowercase());
        // Include physical origin so duplicate virtual paths cannot overwrite each other.
        let key = digest(format!("{}\n{}", record.source, record.virtual_path).as_bytes());
        let (kind, native, provenance, warnings) = if record
            .virtual_path
            .to_lowercase()
            .ends_with(".ter")
        {
            let (terrain, source) = bri_convert::terrain::read_v3(&data, id)?;
            (
                "terrain",
                serde_json::to_vec(&terrain)?,
                serde_json::to_vec(&source)?,
                source.warnings,
            )
        } else if record.virtual_path.to_lowercase().ends_with(".mis") {
            let scene =
                bri_convert::mission::read(std::str::from_utf8(&data)?, &record.virtual_path)?;
            let mut warnings = vec!["Scene declarations preserved; datablock behaviors and environment integration remain native adaptation work".into()];
            warnings.extend(scene.pending_scripts.iter().map(|p| p.diagnostic()));
            (
                "scene",
                serde_json::to_vec(&scene)?,
                serde_json::to_vec(
                    &serde_json::json!({"original_text":std::str::from_utf8(&data)?}),
                )?,
                warnings,
            )
        } else if record.virtual_path.to_lowercase().ends_with(".dif") {
            let (interior, source) = bri_convert::interior::read(&data, id)?;
            std::fs::write(output.join(format!("{key}.source.dif")), &data)?;
            (
                "interior",
                serde_json::to_vec(&interior)?,
                serde_json::to_vec(&source)?,
                source.warnings,
            )
        } else if record.virtual_path.to_lowercase().ends_with(".dts") {
            let (shape, source) = bri_convert::shape::read_dts(&data, id)?;
            (
                "shape",
                serde_json::to_vec(&shape)?,
                serde_json::to_vec(&source)?,
                source.warnings,
            )
        } else if record.virtual_path.to_lowercase().ends_with(".dsq") {
            let (clips, source) = bri_convert::shape::read_dsq(&data, id)?;
            (
                "clips",
                serde_json::to_vec(&clips)?,
                serde_json::to_vec(&source)?,
                source.warnings,
            )
        } else {
            let (brick, source) = bri_convert::brick::read(&data, id)?;
            (
                "brick",
                serde_json::to_vec(&brick)?,
                serde_json::to_vec(&source)?,
                source.warnings,
            )
        };
        let filename = format!("{key}.{kind}.json");
        std::fs::write(output.join(&filename), native)?;
        std::fs::write(output.join(format!("{key}.source.json")), provenance)?;
        record.warnings = warnings;
        record.output = Some(filename);
        Ok(())
    })();
    if let Err(error) = result {
        record.error = Some(format!("{error:#}"));
    }
    record
}

fn main() -> Result<()> {
    let mut args: Vec<_> = std::env::args_os().skip(1).collect();
    // `--reference <inventory.json>` converts only the add-on archives the
    // v20 reference inventory lists, and only when their bytes match it, so
    // every player's install converts to the same pack whatever else their
    // Add-Ons folder holds.
    let reference = match args.iter().position(|a| a == "--reference") {
        Some(at) => {
            ensure!(at + 1 < args.len(), "Missing reference inventory");
            let path = PathBuf::from(args.remove(at + 1));
            args.remove(at);
            Some(Reference::load(&path)?)
        }
        None => None,
    };
    ensure!(
        args.len() == 2,
        "Usage: bri-convert <original-v20-directory> <new-output-directory> [--reference <inventory.json>] (terrain, brick, model and animation pass)"
    );
    let root = PathBuf::from(&args[0])
        .canonicalize()
        .context("Original installation missing")?;
    let requested_output = PathBuf::from(&args[1]);
    let parent = requested_output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = parent
        .canonicalize()
        .context("Output parent must already exist")?;
    ensure!(
        !parent.starts_with(&root),
        "Output must be outside original installation"
    );
    ensure!(
        !requested_output.exists(),
        "Use a new output directory; prior conversions are never overwritten"
    );
    std::fs::create_dir(&requested_output)?;
    let output = requested_output.canonicalize()?;
    ensure!(
        !root.starts_with(&output),
        "Output must not contain original installation"
    );
    let mut records = Vec::new();
    let mut scan_errors = Vec::new();
    for folder in ["base", "Add-Ons"] {
        for entry in walkdir::WalkDir::new(root.join(folder))
            .follow_links(false)
            .sort_by_file_name()
        {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    scan_errors.push(e.to_string());
                    continue;
                }
            };
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let relative = path
                .strip_prefix(&root)?
                .to_string_lossy()
                .replace('\\', "/");
            if folder == "Add-Ons"
                && let Some(reference) = &reference
            {
                match reference.check(&relative, path) {
                    Ok(true) => {}
                    Ok(false) => continue,
                    Err(error) => {
                        scan_errors.push(format!("{relative}: {error:#}"));
                        continue;
                    }
                }
            }
            let extension = path
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            if ["ter", "blb", "dts", "dsq", "dif", "mis"].contains(&extension.as_str()) {
                records.push(convert(
                    relative.clone(),
                    relative,
                    File::open(path).map_err(Into::into).and_then(bounded_read),
                    &output,
                ));
            } else if extension == "zip" {
                let archive_result = (|| -> Result<()> {
                    if bri_convert::archive::is_rar(path)? {
                        for (name, data) in bri_convert::archive::rar_assets(path)? {
                            let name = normalize(&name)?;
                            let virtual_path = format!(
                                "Add-Ons/{}/{}",
                                path.file_stem().unwrap().to_string_lossy(),
                                name
                            );
                            let mut record = convert(
                                format!("{relative}::{name}"),
                                virtual_path,
                                Ok(data),
                                &output,
                            );
                            record.warnings.push("Archive identified as RAR by signature despite .zip extension; read through 7z stdout without filesystem extraction".into());
                            records.push(record);
                        }
                        return Ok(());
                    }
                    let mut archive = zip::ZipArchive::new(File::open(path)?)?;
                    for index in 0..archive.len() {
                        let mut member = archive.by_index(index)?;
                        let name_lower = member.name().to_lowercase();
                        if member.is_dir()
                            || ![".ter", ".blb", ".dts", ".dsq", ".dif", ".mis"]
                                .iter()
                                .any(|ext| name_lower.ends_with(ext))
                        {
                            continue;
                        }
                        let name = normalize(member.name())?;
                        let virtual_path = format!(
                            "Add-Ons/{}/{}",
                            path.file_stem().unwrap().to_string_lossy(),
                            name
                        );
                        let data = if member.size() > MAX_ASSET {
                            Err(anyhow::anyhow!("Oversized terrain member"))
                        } else {
                            bounded_read(&mut member)
                        };
                        records.push(convert(
                            format!("{relative}::{name}"),
                            virtual_path,
                            data,
                            &output,
                        ));
                    }
                    Ok(())
                })();
                if let Err(error) = archive_result {
                    scan_errors.push(format!("{relative}: {error:#}"));
                }
            }
        }
    }
    if let Some(reference) = &reference {
        scan_errors.extend(reference.missing());
    }
    records.sort_by(|a, b| {
        a.virtual_path
            .cmp(&b.virtual_path)
            .then(a.source.cmp(&b.source))
    });
    let mut paths = std::collections::BTreeMap::<String, usize>::new();
    for record in &records {
        *paths.entry(record.virtual_path.to_lowercase()).or_default() += 1;
    }
    for record in &mut records {
        if paths[&record.virtual_path.to_lowercase()] > 1 {
            record
                .warnings
                .push("Duplicate virtual path: explicit source selection required".into());
        }
    }
    let converted = records.iter().filter(|r| r.error.is_none()).count();
    let failed = records.len() - converted;
    let report = serde_json::json!({
        "manifest_version": 1, "converter": env!("CARGO_PKG_VERSION"), "pass": "terrain-bricks-shapes-clips-interiors-missions",
        "scope": "native asset data; dependent materials and gameplay integration pending",
        "converted": converted, "failed": failed, "scan_errors": scan_errors, "records": records,
    });
    std::fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!(
        "Content conversion: {converted} succeeded, {failed} failed, {} archive/scan errors. Manifest: {}",
        scan_errors.len(),
        output.join("manifest.json").display()
    );
    ensure!(
        failed == 0 && scan_errors.is_empty(),
        "Conversion incomplete; see manifest diagnostics"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unsafe_archive_paths() {
        for path in ["../bad.ter", "C:/bad.ter", "/bad.ter", "a/../bad.ter", ""] {
            assert!(normalize(path).is_err(), "{path}");
        }
        assert_eq!(normalize("maps\\test.ter").unwrap(), "maps/test.ter");
    }
}
