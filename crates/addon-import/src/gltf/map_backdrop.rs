//! A map Add-On's backdrop from `--backdrop FILE.gltf` (docs/modding/maps.md): a glTF file holding the model drawn behind the map, with
//! the scene's custom property `bri_backdrop` naming the eye it is viewed from, in the file's own frame and length unit:
//!
//! ```json
//! "scenes": [{ "extras": { "bri_backdrop": { "eye": [368.25, 1.1, -368.2] } } }]
//! ```
//!
//! The model becomes a native shape and the eye moves into the native frame and the import's scale as the map's meshes do; the bundle's
//! `backdrops` table lists it ([`bri_content::backdrop`]).
use super::map_scatter::Put;
use super::{Document, Kind, Options, Report, convert, point, safe_name};
use anyhow::{Context, Result, ensure};
use bri_content::backdrop::Record;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// What the backdrop adds to the map's bundle.
#[derive(Default)]
pub(super) struct Built {
    pub assets: BTreeMap<String, String>,
    pub textures: BTreeMap<String, String>,
    pub bindings: Vec<Value>,
    pub record: Option<Record>,
}

pub(super) fn backdrop(
    opts: &Options,
    map_key: &str,
    id: &str,
    put: Put<'_>,
    report: &mut Report,
) -> Result<Built> {
    let mut built = Built::default();
    let Some(path) = &opts.backdrop else {
        return Ok(built);
    };
    let scale = opts.scale.unwrap_or(1.0);
    let doc =
        Document::open(path).with_context(|| format!("Reading backdrop {}", path.display()))?;
    let scene = doc.json["scene"].as_u64().unwrap_or(0) as usize;
    let eye: Vec<f32> = doc.json["scenes"][scene]["extras"]["bri_backdrop"]["eye"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_f64)
                .map(|f| f as f32)
                .collect()
        })
        .unwrap_or_default();
    ensure!(
        eye.len() == 3,
        "The backdrop's scene needs bri_backdrop `eye`, three numbers"
    );
    let eye = point(glam::Vec3::new(eye[0], eye[1], eye[2])) * scale;
    let name = safe_name(
        &path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "backdrop".into()),
    );
    let mut backdrop_opts = Options::new(path.clone(), opts.out.clone(), Kind::Prop);
    backdrop_opts.scale = opts.scale;
    backdrop_opts.fps = opts.fps;
    let prefix = format!("{id}_{name}_");
    let converted = convert(&doc, path.parent(), &backdrop_opts, &name, &prefix)
        .context("Converting the backdrop")?;
    report.warnings.extend(
        converted
            .report
            .warnings
            .iter()
            .map(|w| format!("backdrop: {w}")),
    );
    let asset = format!("{map_key}/backdrop/{name}");
    let shape_file = format!("{id}_{name}.shape.json");
    put(
        &format!("map/{shape_file}"),
        &serde_json::to_vec(&converted.shape)?,
        report,
    )?;
    for (material, png) in &converted.textures {
        let file = format!("{material}.png");
        put(&format!("map/{file}"), png, report)?;
        built.textures.insert(file.clone(), file);
    }
    for (slot, m) in converted.shape.materials.iter().enumerate() {
        built.bindings.push(
            json!({ "asset": asset, "shape_material": slot, "skin": "", "texture": format!("{}.png", m.name) }),
        );
    }
    let record = Record {
        asset: asset.clone(),
        eye: eye.to_array(),
    };
    record.validate()?;
    built.assets.insert(asset, shape_file);
    built.record = Some(record);
    Ok(built)
}
