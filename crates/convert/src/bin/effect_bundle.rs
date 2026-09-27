//! Offline stock effect extraction and original texture packaging.
use anyhow::{Context, Result, ensure};
use bri_content::effects::Library;
use bri_convert::{effect_script::Declarations, effects};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::{Path, PathBuf},
};

fn safe(path: &str) -> Result<()> {
    ensure!(
        !path.starts_with('/')
            && !path.contains([':', '\\'])
            && path
                .split('/')
                .all(|p| !p.is_empty() && p != ".." && p != "."),
        "Unsafe virtual path {path}"
    );
    Ok(())
}
fn read(root: &Path, path: &str) -> Result<Vec<u8>> {
    safe(path)?;
    if path.to_lowercase().starts_with("add-ons/") {
        let (package, member) = path[8..]
            .split_once('/')
            .context("Missing archive member")?;
        let dir = root.join("Add-Ons");
        let archives: Vec<_> = std::fs::read_dir(dir)?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|p| {
                p.file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&format!("{package}.zip"))
            })
            .collect();
        ensure!(
            archives.len() == 1,
            "Missing or ambiguous archive {package}"
        );
        let mut zip = zip::ZipArchive::new(std::fs::File::open(archives[0].path())?)?;
        let matches: Vec<_> = zip
            .file_names()
            .filter(|n| n.eq_ignore_ascii_case(member))
            .map(str::to_owned)
            .collect();
        ensure!(matches.len() == 1, "Missing or ambiguous member {path}");
        let mut bytes = vec![];
        zip.by_name(&matches[0])?
            .take(16 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 16 * 1024 * 1024, "Oversized resource {path}");
        Ok(bytes)
    } else {
        // Resolve case explicitly so the converter behaves the same on Unix.
        let mut current = root.to_path_buf();
        for part in path.split('/') {
            let matches: Vec<_> = std::fs::read_dir(&current)?
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .filter(|p| p.file_name().to_string_lossy().eq_ignore_ascii_case(part))
                .collect();
            ensure!(matches.len() == 1, "Missing or ambiguous resource {path}");
            current = matches[0].path();
        }
        ensure!(
            std::fs::metadata(&current)?.len() <= 16 * 1024 * 1024,
            "Oversized resource"
        );
        Ok(std::fs::read(current)?)
    }
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    ensure!(
        args.len() == 4,
        "Usage: effect_bundle <v20-root> <recovered-core-script> <vanilla-inventory.json> <new-output-dir>"
    );
    let root = &args[0];
    let output = &args[3];
    ensure!(!output.exists(), "Use a new output directory");
    let core = std::fs::read(&args[1])?;
    let inventory: serde_json::Value = serde_json::from_slice(&std::fs::read(&args[2])?)?;
    let mut declarations = Declarations::default();
    declarations.read(
        std::str::from_utf8(&core)?,
        "base/server/scripts/allGameScripts.cs",
    )?;
    let mut sources = vec![
        serde_json::json!({"path":"core/allGameScripts-Vanilla.cs","sha256":format!("{:x}",Sha256::digest(&core))}),
    ];
    let mut errors = vec![];
    for package in inventory["packages"]
        .as_array()
        .context("Missing vanilla packages")?
    {
        let name = package["name"].as_str().context("Missing package name")?;
        // Print packages use client print discovery; they have no server effects.
        if name.starts_with("Print_") {
            continue;
        }
        let origin = format!("Add-Ons/{name}/server.cs");
        let bytes = read(root, &origin)?;
        sources.push(
            serde_json::json!({"path":origin,"sha256":format!("{:x}",Sha256::digest(&bytes))}),
        );
        declarations.read_with_includes(std::str::from_utf8(&bytes)?,&origin,&mut |path,parent|{
            let path=effects::texture_path(path.into(),parent)?;
            let bytes=read(root,&path)?;
            sources.push(serde_json::json!({"path":path,"sha256":format!("{:x}",Sha256::digest(&bytes))}));
            Ok((String::from_utf8(bytes)?,path))
        }).with_context(||format!("Reading {name}"))?;
    }
    let mut library = Library {
        schema_version: 1,
        lights: vec![],
        particles: vec![],
        emitters: vec![],
        textures: BTreeMap::new(),
    };
    let mut diagnostics = vec![];
    let mut nodes = BTreeMap::new();
    for d in &declarations.entries {
        if d.class == "particleemitternodedata" {
            let mut f = effects::Fields::new(d);
            nodes.insert(d.name.to_lowercase(), f.number("timemultiple", 1.0)?);
        }
    }
    for d in &declarations.entries {
        let result: Result<Vec<String>> = match d.class.as_str() {
            "particledata" => effects::particle(d).map(|(p, n)| {
                library.particles.push(p);
                n
            }),
            "particleemitterdata" => effects::emitter(d, &nodes).map(|(e, n)| {
                library.emitters.push(e);
                n
            }),
            "fxlightdata" => effects::light(d).map(|(l, n)| {
                library.lights.push(l);
                n
            }),
            _ => Ok(vec![]),
        };
        match result {
            Ok(notes) => {
                if !notes.is_empty() {
                    diagnostics.push(serde_json::json!({"name":d.name,"notes":notes}));
                }
            }
            Err(error) => errors.push(serde_json::json!({"name":d.name,"error":error.to_string()})),
        }
    }
    std::fs::create_dir_all(output.join("provenance"))?;
    std::fs::write(
        output.join("provenance/effective-declarations.json"),
        serde_json::to_vec_pretty(&declarations.entries)?,
    )?;
    let paths: BTreeSet<_> = library
        .particles
        .iter()
        .map(|p| p.texture.clone())
        .chain(
            library
                .lights
                .iter()
                .filter_map(|l| l.flare.as_ref().map(|f| f.texture.clone())),
        )
        .collect();
    let mut bindings = vec![];
    for path in paths {
        let mut resolved = None;
        for extension in ["png", "jpg", "jpeg"] {
            let candidate = if Path::new(&path).extension().is_some() {
                path.clone()
            } else {
                format!("{path}.{extension}")
            };
            if let Ok(bytes) = read(root, &candidate) {
                resolved = Some((candidate, bytes));
                break;
            }
        }
        if let Some((source, bytes)) = resolved {
            let decoded = image::load_from_memory(&bytes)?;
            ensure!(
                decoded.width() <= 8192 && decoded.height() <= 8192,
                "Oversized effect image"
            );
            let sha = format!("{:x}", Sha256::digest(&bytes));
            let file = format!(
                "{sha}.{}",
                Path::new(&source).extension().unwrap().to_string_lossy()
            );
            std::fs::write(output.join(&file), &bytes)?;
            library.textures.insert(path.clone(), file);
            bindings.push(serde_json::json!({"id":path,"source":source,"sha256":sha,"width":decoded.width(),"height":decoded.height()}));
        } else {
            errors.push(serde_json::json!({"texture":path,"error":"Missing original texture"}));
        }
    }
    if let Err(e) = library.validate() {
        errors.push(serde_json::json!({"validation":e.to_string()}));
    }
    std::fs::write(
        output.join("effects.json"),
        serde_json::to_vec_pretty(&library)?,
    )?;
    let report = serde_json::json!({"sources":sources,"lights":library.lights.len(),"particles":library.particles.len(),"emitters":library.emitters.len(),"listed_emitters":library.emitters.iter().filter(|e|!e.name.is_empty()).count(),"texture_bindings":bindings,"diagnostics":diagnostics,"script_diagnostics":declarations.diagnostics,"errors":errors,"scope":"Core and default stock packages' unconditional literal effects and includes. Conditional registrations and runtime-generated paint variants need explicit adaptation. Native schemas and original textures; renderer/attachment behavior not accepted."});
    std::fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!(
        "{} lights, {} particles, {} emitters, {} textures; {} errors",
        library.lights.len(),
        library.particles.len(),
        library.emitters.len(),
        library.textures.len(),
        errors.len()
    );
    ensure!(
        errors.is_empty(),
        "Effect conversion has explicit failures; inspect report.json"
    );
    Ok(())
}
