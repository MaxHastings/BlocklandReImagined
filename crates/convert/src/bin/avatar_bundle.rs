//! Offline assembly of native avatar geometry and source-declared sequence aliases.
use anyhow::{Context, Result, ensure};
use bri_content::{
    avatar::{Rig, Source},
    shape::{ClipSet, Shape},
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
};

const LIMIT: u64 = 64 * 1024 * 1024;
fn read(path: &Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= LIMIT,
        "Avatar input exceeds size limit"
    );
    Ok(bytes)
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
#[derive(Deserialize)]
struct Manifest {
    records: Vec<Record>,
}
#[derive(Deserialize)]
struct Record {
    virtual_path: String,
    source_sha256: String,
    output: Option<String>,
}
fn asset<T: serde::de::DeserializeOwned>(
    root: &Path,
    manifest: &Manifest,
    name: &str,
    alias: &str,
    line: usize,
) -> Result<(T, Source)> {
    let records: Vec<_> = manifest
        .records
        .iter()
        .filter(|r| r.virtual_path.eq_ignore_ascii_case(name))
        .collect();
    ensure!(
        records.len() == 1,
        "Missing/ambiguous native avatar asset: {name}"
    );
    let record = records[0];
    let file = record
        .output
        .as_deref()
        .context("Avatar asset failed conversion")?;
    ensure!(
        !file.is_empty() && !file.contains(['/', '\\', ':']) && file != "." && file != "..",
        "Invalid native asset path"
    );
    let path = root.join(file).canonicalize()?;
    ensure!(
        path.starts_with(root),
        "Avatar input escapes native package"
    );
    let bytes = read(&path)?;
    let source = Source {
        alias: alias.into(),
        virtual_path: name.into(),
        source_sha256: record.source_sha256.clone(),
        native_sha256: hash(&bytes),
        constructor_line: line,
    };
    Ok((serde_json::from_slice(&bytes)?, source))
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(
        args.len() == 4,
        "Usage: avatar_bundle <original-install-read-only> <native-geometry> <recovered-stock-script> <new-output-directory>"
    );
    let original = PathBuf::from(&args[0]).canonicalize()?;
    let native = PathBuf::from(&args[1]).canonicalize()?;
    let script = PathBuf::from(&args[2]).canonicalize()?;
    let output = PathBuf::from(&args[3]);
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()?;
    let output = parent.join(output.file_name().context("Output must name a directory")?);
    ensure!(
        !output.starts_with(&original) && !output.starts_with(&native),
        "Output must be outside original/native inputs"
    );
    ensure!(
        std::fs::symlink_metadata(&output).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
        "Use a new output directory"
    );
    let script_bytes = read(&script)?;
    let constructor =
        bri_convert::avatar::constructor(std::str::from_utf8(&script_bytes)?, "mDts")?;
    let manifest: Manifest = serde_json::from_slice(&read(&native.join("manifest.json"))?)?;
    let (shape, source): (Shape, _) = asset(
        &native,
        &manifest,
        &constructor.shape.0,
        "base",
        constructor.shape.1,
    )?;
    let mut rig = Rig { schema_version: 1, id: "v20/avatar/blockhead".into(), shape, sequences: BTreeMap::new(), sources: vec![source], omissions: vec!["Material/face/decal bindings, customization, gameplay state selection and runtime rendering remain separate integration work".into()] };
    for (path, alias, line) in constructor.sequences {
        let (mut clips, source): (ClipSet, _) = asset(&native, &manifest, &path, &alias, line)?;
        ensure!(
            clips.schema_version == 1 && clips.animations.len() == 1,
            "Expected one source clip: {path}"
        );
        let mut clip = clips.animations.remove(0);
        clip.name = alias.clone();
        rig.sequences.insert(alias, clip);
        rig.sources.push(source);
    }
    rig.validate()?;
    let bytes = serde_json::to_vec(&rig)?;
    let report = serde_json::json!({"schema_version":1,"id":rig.id,"rig":"rig.json","sha256":hash(&bytes),"bytes":bytes.len(),"constructor_sha256":hash(&script_bytes),"sequence_count":rig.sequences.len(),"nodes":rig.shape.nodes.len(),"objects":rig.shape.objects.len(),"omissions":rig.omissions});
    std::fs::create_dir(&output)?;
    std::fs::write(output.join("rig.json"), bytes)?;
    std::fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{report}");
    Ok(())
}
