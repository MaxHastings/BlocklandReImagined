//! A glTF 2.0 model (`.glb`, or `.gltf` with its files) to a native
//! [`Shape`] and the PNGs its materials name: what Blender and most modelling
//! tools export, for an Add-On's own bodies, held items and vehicles.
//!
//! The authoring contract (names, scale, axes, clips) is
//! `docs/modding/gltf-models.md`. glTF is Y-up with the model facing +Z;
//! the native frame is Y-up facing -Z, so everything turns half a turn
//! about Y. Node scale, which native nodes do not carry, is pushed into
//! child translations, mesh vertices and inverse bind matrices. Animation
//! clips are sampled at a fixed rate; clips named after a body's animation
//! roles take the Blockhead's clip names, so a body needs no mapping.
mod brick;
mod export;
mod map;
mod map_backdrop;
mod map_decals;
mod map_environment;
mod map_regions;
mod map_terrain;

#[doc(hidden)]
pub use bri_gltf::testing;
pub use bri_gltf::*;
pub use bri_gltf::{map_scatter, map_zones};
pub use export::{ExportReport, export_map, find_map};
pub use map_environment::SKY_FACES;

use anyhow::{Context, Result, ensure};
use serde_json::json;
use std::path::Path;

/// Convert and write the model as `opts.kind` says. `opts.out` must not
/// exist or be empty.
pub fn import(opts: &Options) -> Result<Report> {
    ensure!(
        opts.fps.is_finite() && (1.0..=120.0).contains(&opts.fps),
        "--fps is 1 to 120"
    );
    if opts.out.exists() {
        ensure!(
            std::fs::read_dir(&opts.out)?.next().is_none(),
            "{} is not empty: give a fresh output folder",
            opts.out.display()
        );
    }
    let stem = opts
        .input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let id = safe_name(opts.id.as_deref().unwrap_or(&stem));
    let doc = Document::open(&opts.input)?;
    let folder = opts.input.parent().map(Path::to_path_buf);
    let prefix = match opts.kind {
        Kind::Body => String::new(),
        _ => format!("{id}_"),
    };
    let mut converted = convert(&doc, folder.as_deref(), opts, &id, &prefix)?;
    converted.report.source = opts
        .input
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    if matches!(opts.kind, Kind::Map | Kind::Brick) {
        let mut report = if opts.kind == Kind::Map {
            map::write(opts, &id, converted, &doc)?
        } else {
            brick::write(opts, &id, converted)?
        };
        report.files.push("IMPORT-REPORT.md".into());
        std::fs::write(opts.out.join("IMPORT-REPORT.md"), report.markdown())?;
        return Ok(report);
    }
    write(opts, &id, converted)
}

fn write(opts: &Options, id: &str, converted: Converted) -> Result<Report> {
    let Converted {
        shape,
        textures,
        mut report,
        ..
    } = converted;
    let out = &opts.out;
    let put = |rel: &str, bytes: &[u8], report: &mut Report| -> Result<()> {
        let path = out.join(rel);
        std::fs::create_dir_all(path.parent().context("output path")?)?;
        std::fs::write(path, bytes)?;
        report.files.push(rel.into());
        Ok(())
    };
    let shape_bytes = serde_json::to_vec(&shape)?;
    match opts.kind {
        Kind::Body => {
            let ns = crate::namespace_for(id)?;
            report.id = ns.clone();
            // Provided as `body` beside the archetype, so the Add-On is
            // shared: joiners download the body with its textures.
            let model = format!("{ns}:body/{id}");
            report.model = model.clone();
            let mut content = Vec::new();
            put(
                &format!("assets/{id}.shape.json"),
                &shape_bytes,
                &mut report,
            )?;
            for (name, png) in &textures {
                put(&format!("assets/{name}.png"), png, &mut report)?;
                content.push(json!({ "kind": "asset", "id": format!("{ns}:asset/{name}.png"), "file": format!("assets/{name}.png") }));
            }
            put(
                "assets/content.json",
                &serde_json::to_vec_pretty(&json!({ "schema_version": 1, "content": content }))?,
                &mut report,
            )?;
            let name = opts.name.clone().unwrap_or_else(|| title(id));
            let archetype = json!({
                "schema_version": 1,
                "name": name,
                "model": model,
                "max_health": 100,
                "movement": report.movement.map_or(json!({}), |m| json!(m)),
            });
            put(
                &format!("assets/archetypes/{id}.json"),
                &serde_json::to_vec_pretty(&archetype)?,
                &mut report,
            )?;
            let manifest = json!({
                "schema_version": 1,
                "id": ns,
                "version": opts.version,
                "api": 1,
                "name": name,
                "description": format!("{name}, a body imported from {} by bri-import-addon gltf.", report.source),
                "authors": [],
                "license": "proprietary",
                "provenance": { "source": format!("glTF model {}", report.source), "notes": "Imported by bri-import-addon gltf. Set the licence of the original model." },
                "dependencies": {},
                "capabilities": [],
                "provides": [
                    { "kind": "archetype", "id": format!("{ns}:archetype/{id}"), "file": format!("assets/archetypes/{id}.json") },
                    { "kind": "body", "id": model, "file": format!("assets/{id}.shape.json") },
                ],
            });
            put(
                "package.json",
                &serde_json::to_vec_pretty(&manifest)?,
                &mut report,
            )?;
        }
        Kind::Map | Kind::Brick => unreachable!("maps and bricks are written by their own modules"),
        Kind::Item | Kind::Prop => {
            report.id = id.into();
            report.model = format!("models/{id}.shape.json");
            put(
                &format!("models/{id}.shape.json"),
                &shape_bytes,
                &mut report,
            )?;
            for (name, png) in &textures {
                put(&format!("models/{name}.png"), png, &mut report)?;
            }
        }
    }
    report.files.push("IMPORT-REPORT.md".into());
    let markdown = report.markdown();
    std::fs::write(out.join("IMPORT-REPORT.md"), markdown)?;
    Ok(report)
}
