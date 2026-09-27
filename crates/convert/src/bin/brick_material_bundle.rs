//! Offline, evidence-gated conversion. No source files are changed.
use anyhow::{Context, Result, ensure};
use bri_content::brick_materials::{Bundle, Image, Package, SCHEMA, Source, safe_relative};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

const MAX_IMAGE: u64 = 16 * 1024 * 1024;
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn read_bounded(path: &Path, max: u64) -> Result<Vec<u8>> {
    ensure!(
        std::fs::metadata(path)?.len() <= max,
        "Input too large: {}",
        path.display()
    );
    let mut bytes = vec![];
    std::fs::File::open(path)?
        .take(max + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= max, "Input grew past limit");
    Ok(bytes)
}
fn guarded_output(root: &Path, output: &Path) -> Result<PathBuf> {
    let root = root.canonicalize()?;
    ensure!(
        output.file_name().is_some(),
        "Output must name a new directory"
    );
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = parent
        .canonicalize()
        .context("Output parent must already exist")?;
    let output = parent.join(output.file_name().unwrap());
    ensure!(
        !output.starts_with(&root),
        "Refusing to write within the original installation"
    );
    ensure!(
        std::fs::symlink_metadata(&output).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
        "Use a new output directory"
    );
    Ok(output)
}
fn defaults(text: &str) -> Result<BTreeMap<String, usize>> {
    let mut result = BTreeMap::new();
    for (line, s) in text.lines().enumerate() {
        let s = s.split("//").next().unwrap().trim();
        let Some(s) = s.strip_prefix("$AddOn__Print_") else {
            continue;
        };
        let (name, value) = s
            .split_once('=')
            .context("Malformed print default declaration")?;
        if value.trim().trim_end_matches(';').trim() != "1" {
            continue;
        }
        let name = format!("Print_{}", name.trim());
        ensure!(
            name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_'),
            "Invalid stock package name"
        );
        ensure!(
            result.insert(name, line + 1).is_none(),
            "Duplicate stock print declaration"
        );
    }
    ensure!(
        !result.is_empty(),
        "No default-enabled stock print packages found"
    );
    Ok(result)
}
fn image(
    bytes: Vec<u8>,
    path: String,
    source_path: String,
    archive: Option<String>,
    outputs: &mut Vec<(String, Vec<u8>)>,
) -> Result<Image> {
    ensure!(bytes.len() as u64 <= MAX_IMAGE, "PNG exceeds byte limit");
    ensure!(
        bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "Expected PNG {source_path}"
    );
    let reader = image::ImageReader::with_format(Cursor::new(&bytes), image::ImageFormat::Png);
    let (width, height) = reader.into_dimensions()?;
    ensure!(
        (1..=8192).contains(&width)
            && (1..=8192).contains(&height)
            && u64::from(width) * u64::from(height) <= 16_777_216,
        "Oversized PNG dimensions"
    );
    // A dimensions-only check misses damaged IDAT streams. Decode bounded pixels
    // once offline; output still preserves the exact original PNG bytes.
    image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)?;
    let sha256 = hash(&bytes);
    let result = Image {
        path: path.clone(),
        width,
        height,
        sha256: sha256.clone(),
        source: Source {
            path: source_path,
            archive,
            sha256,
        },
    };
    outputs.push((path, bytes));
    Ok(result)
}
fn convert(
    root: &Path,
    default_file: &Path,
    inventory_file: &Path,
    output: &Path,
) -> Result<Bundle> {
    let root = root.canonicalize()?;
    let output = guarded_output(&root, output)?;
    let default_bytes = read_bounded(default_file, 1024 * 1024)?;
    let inventory_bytes = read_bounded(inventory_file, 8 * 1024 * 1024)?;
    let stock = defaults(std::str::from_utf8(&default_bytes)?)?;
    let inventory: serde_json::Value = serde_json::from_slice(&inventory_bytes)?;
    let inventory = inventory["packages"]
        .as_array()
        .context("Missing inventory packages")?;
    let mut bundle = Bundle {
        schema_version: SCHEMA,
        surfaces: BTreeMap::new(),
        prints: vec![],
        packages: vec![],
        evidence: vec![
            Source {
                path: default_file.to_string_lossy().into(),
                archive: None,
                sha256: hash(&default_bytes),
            },
            Source {
                path: inventory_file.to_string_lossy().into(),
                archive: None,
                sha256: hash(&inventory_bytes),
            },
        ],
        excluded_installed_packages: vec![],
        warnings: vec![],
    };
    let mut outputs = vec![];
    for (key, name) in [
        ("top", "brickTOP"),
        ("side", "brickSIDE"),
        ("bottom_edge", "brickBOTTOMEDGE"),
        ("bottom_loop", "brickBOTTOMLOOP"),
        ("ramp", "brickRAMP"),
    ] {
        let source = format!("base/data/shapes/{name}.png");
        let file = root.join(&source).canonicalize()?;
        ensure!(
            file.starts_with(&root),
            "Surface source escapes installation"
        );
        let img = image(
            read_bounded(&file, MAX_IMAGE)?,
            format!("surfaces/{key}.png"),
            source,
            None,
            &mut outputs,
        )?;
        bundle.surfaces.insert(key.into(), img);
    }
    let installed = std::fs::read_dir(root.join("Add-Ons"))?.collect::<Result<Vec<_>, _>>()?;
    for entry in &installed {
        let name = entry.file_name().to_string_lossy().into_owned();
        let stem = Path::new(&name).file_stem().unwrap().to_string_lossy();
        if stem.to_ascii_lowercase().starts_with("print_")
            && !stock.keys().any(|s| s.eq_ignore_ascii_case(&stem))
        {
            bundle.excluded_installed_packages.push(name);
        }
    }
    bundle.excluded_installed_packages.sort();
    for (name, line) in stock {
        let records: Vec<_> = inventory
            .iter()
            .filter(|p| p["name"].as_str() == Some(&name))
            .collect();
        ensure!(
            records.len() == 1
                && records[0]["family"] == "print"
                && records[0]["default_list_line"].as_u64() == Some(line as u64),
            "Stock inventory does not confirm {name}"
        );
        let archive = format!("Add-Ons/{name}.zip");
        let file = root
            .join(&archive)
            .canonicalize()
            .with_context(|| format!("Missing stock package {name}"))?;
        ensure!(
            file.starts_with(&root),
            "Package source escapes installation"
        );
        let bytes = read_bounded(&file, 64 * 1024 * 1024)?;
        let archive_sha256 = hash(&bytes);
        ensure!(
            records[0]["archive_sha256"].as_str() == Some(&archive_sha256),
            "Installed archive changed since stock inventory: {name}"
        );
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes))?;
        ensure!(zip.len() <= 4096, "Too many archive members");
        let mut members = BTreeMap::new();
        let mut total = 0u64;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            if entry.is_dir() {
                continue;
            }
            let member = entry.name().to_owned();
            ensure!(safe_relative(&member), "Unsafe archive member {member}");
            ensure!(entry.size() <= MAX_IMAGE, "Oversized archive member");
            total = total
                .checked_add(entry.size())
                .context("Archive size overflow")?;
            ensure!(total <= 256 * 1024 * 1024, "Expanded archive exceeds bound");
            let mut bytes = vec![];
            (&mut entry).take(MAX_IMAGE + 1).read_to_end(&mut bytes)?;
            ensure!(
                bytes.len() as u64 == entry.size(),
                "Archive member size mismatch"
            );
            ensure!(
                members
                    .insert(member.to_ascii_lowercase(), (member, bytes))
                    .is_none(),
                "Case-ambiguous archive members"
            );
        }
        let aspect = name
            .strip_prefix("Print_")
            .unwrap()
            .split_once('_')
            .context("Package missing aspect")?
            .0
            .to_owned();
        let mut used_icons = BTreeSet::new();
        for (member, bytes) in members.values().filter(|(m, _)| {
            m.to_ascii_lowercase().starts_with("prints/")
                && m.to_ascii_lowercase().ends_with(".png")
        }) {
            let stem = Path::new(member)
                .file_stem()
                .unwrap()
                .to_str()
                .context("Non-UTF8 print")?;
            ensure!(
                !stem.contains(' ') && member.split('/').count() == 2,
                "Original loader would skip print {member}"
            );
            let icon_key = format!("icons/{stem}.png").to_ascii_lowercase();
            let (icon_name, icon_bytes) = members
                .get(&icon_key)
                .with_context(|| format!("Missing original icon for {name}/{stem}"))?;
            used_icons.insert(icon_key);
            let id = format!(
                "print/{}/{}",
                name.to_ascii_lowercase(),
                stem.to_ascii_lowercase()
            );
            let base = format!(
                "prints/{}/{}",
                name.to_ascii_lowercase(),
                stem.to_ascii_lowercase()
            );
            let diffuse = image(
                bytes.clone(),
                format!("{base}.png"),
                member.clone(),
                Some(archive.clone()),
                &mut outputs,
            )?;
            let icon = image(
                icon_bytes.clone(),
                format!("{base}-icon.png"),
                icon_name.clone(),
                Some(archive.clone()),
                &mut outputs,
            )?;
            bundle.prints.push(bri_content::brick_materials::Print {
                id,
                name: stem.into(),
                aspect: aspect.clone(),
                package: name.clone(),
                aliases: vec![format!("{aspect}/{stem}")],
                diffuse,
                icon,
            });
        }
        for key in members
            .keys()
            .filter(|m| m.starts_with("icons/") && m.ends_with(".png"))
        {
            ensure!(
                used_icons.contains(key),
                "Unpaired original icon: {name}/{key}"
            );
        }
        bundle.packages.push(Package {
            name,
            archive,
            archive_sha256,
            default_list_line: line,
        });
    }
    bundle.prints.sort_by(|a, b| a.id.cmp(&b.id));
    bundle.validate()?;
    // Verify everything before creating the fresh output directory.
    std::fs::create_dir(&output)?;
    for (path, bytes) in outputs {
        let target = output.join(path);
        std::fs::create_dir_all(target.parent().unwrap())?;
        std::fs::write(target, bytes)?;
    }
    std::fs::write(
        output.join("brick-materials.json"),
        serde_json::to_vec_pretty(&bundle)?,
    )?;
    Ok(bundle)
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    ensure!(
        args.len() == 4,
        "Usage: brick_material_bundle <original-install> <recovered-defaultAddOnList.cs> <vanilla-inventory.json> <new-output-dir>"
    );
    let bundle = convert(&args[0], &args[1], &args[2], &args[3])?;
    println!(
        "{} surfaces, {} prints with original icons, {} verified packages, {} excluded packages, {} warnings",
        bundle.surfaces.len(),
        bundle.prints.len(),
        bundle.packages.len(),
        bundle.excluded_installed_packages.len(),
        bundle.warnings.len()
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stock_scope_uses_enabled_declarations_and_rejects_duplicates() {
        assert_eq!(defaults("// $AddOn__Print_bad = 1;\n$AddOn__Print_2x2f_Default = 1;\n$AddOn__Print_Community = 0;").unwrap().len(),1);
        assert!(defaults("$AddOn__Print_A = 1;\n$AddOn__Print_A = 1;").is_err());
    }
    #[test]
    fn output_guard_resolves_dotdot_before_protecting_source() {
        let dir = std::env::temp_dir().join(format!("bri-material-guard-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("source/child")).unwrap();
        assert!(guarded_output(&dir.join("source"), &dir.join("source/child/../out")).is_err());
        assert!(guarded_output(&dir.join("source"), &dir.join("out")).is_ok());
        assert!(guarded_output(&dir.join("source"), &dir.join("source/child")).is_err());
        // Test artifacts remain outside both the workspace and original install.
    }
}
