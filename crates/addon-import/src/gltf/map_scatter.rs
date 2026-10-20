//! A map Add-On's scatter layers from `--scatter` files (docs/modding/maps.md): each is a glTF file holding the one model a layer repeats, with the
//! scene's custom property `bri_scatter` naming the placements file and, optionally, the fade range:
//!
//! ```json
//! "scenes": [{ "extras": { "bri_scatter": { "instances": "grass.bin", "fade": [2500, 3500] } } }]
//! ```
//!
//! The placements file is [`bri_content::scatter`]'s, written in the glTF file's own frame (position and rotation as the model's nodes have them) and
//! lengths in the same unit as the model; the importer moves them into the native frame and the import's scale exactly as it does the map's meshes.
//! The model becomes a native shape the map repeats; the placements become a file beside the map's bundle and an entry in its `scatter` table.
use super::{Document, Kind, Options, Report, convert, point, safe_name, turn};
use anyhow::{Context, Result, ensure};
use bri_content::scatter::{self, Instance, Record};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

/// The most layers a map may name.
const MAX_LAYERS: usize = 64;

/// Writes a file below the output folder, noting it in the report.
pub(super) type Put<'a> = &'a mut dyn FnMut(&str, &[u8], &mut Report) -> Result<()>;

/// What the layers add to the map's bundle.
#[derive(Default)]
pub(super) struct Built {
    /// Asset id to the shape's file in `map/`.
    pub assets: BTreeMap<String, String>,
    /// The layers' textures, file to file (in `map/`).
    pub textures: BTreeMap<String, String>,
    /// Texture bindings for the layers' shapes.
    pub bindings: Vec<Value>,
    /// The bundle's `scatter` table for this map.
    pub records: Vec<Record>,
}

/// Convert every `--scatter` file, writing the shapes, textures and placements through `put` (paths below `map/`).
pub(super) fn layers(
    opts: &Options,
    map_key: &str,
    id: &str,
    put: Put<'_>,
    report: &mut Report,
) -> Result<Built> {
    ensure!(
        opts.scatter.len() <= MAX_LAYERS,
        "A map names at most {MAX_LAYERS} scatter layers"
    );
    let mut built = Built::default();
    let scale = opts.scale.unwrap_or(1.0);
    for (number, path) in opts.scatter.iter().enumerate() {
        let layer = safe_name(
            &path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| format!("layer{number}")),
        );
        let doc = Document::open(path)
            .with_context(|| format!("Reading scatter layer {}", path.display()))?;
        let scene = doc.json["scene"].as_u64().unwrap_or(0) as usize;
        let extras = &doc.json["scenes"][scene]["extras"]["bri_scatter"];
        let file = extras["instances"]
            .as_str()
            .with_context(|| format!("Scatter layer {layer}: the scene's bri_scatter needs `instances`, the placements file"))?;
        let fade = match extras.get("fade").filter(|v| !v.is_null()) {
            None => None,
            Some(v) => {
                let list: Vec<f32> = v
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_f64)
                            .map(|f| f as f32)
                            .collect()
                    })
                    .unwrap_or_default();
                ensure!(
                    list.len() == 2,
                    "Scatter layer {layer}: bri_scatter `fade` is [near, far]"
                );
                Some([list[0] * scale, list[1] * scale])
            }
        };
        let order = match extras.get("order").and_then(Value::as_str) {
            None | Some("unsorted") => scatter::Order::Unsorted,
            Some("far_to_near") => scatter::Order::FarToNear,
            Some("near_to_far") => scatter::Order::NearToFar,
            Some(other) => anyhow::bail!(
                "Scatter layer {layer}: bri_scatter `order` is unsorted, far_to_near or near_to_far, not {other}"
            ),
        };
        let modulate_2x = extras
            .get("modulate_2x")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let detail = extras.get("detail").and_then(Value::as_u64).unwrap_or(0);
        ensure!(
            detail <= 3,
            "Scatter layer {layer}: bri_scatter `detail` is 0 to 3"
        );
        let mut layer_opts = Options::new(path.clone(), opts.out.clone(), Kind::Prop);
        layer_opts.scale = opts.scale;
        layer_opts.fps = opts.fps;
        let prefix = format!("{id}_{layer}_");
        let converted = convert(&doc, path.parent(), &layer_opts, &layer, &prefix)
            .with_context(|| format!("Converting scatter layer {layer}"))?;
        report.warnings.extend(
            converted
                .report
                .warnings
                .iter()
                .map(|w| format!("scatter layer {layer}: {w}")),
        );
        let bytes = std::fs::read(path.parent().unwrap_or(Path::new(".")).join(file))
            .with_context(|| format!("Reading scatter layer {layer}'s placements {file}"))?;
        let given = scatter::decode(&bytes)
            .with_context(|| format!("Scatter layer {layer}'s placements"))?;
        ensure!(!given.is_empty(), "Scatter layer {layer} has no placements");
        let placed: Vec<Instance> = given
            .iter()
            .map(|i| {
                let position = point(glam::Vec3::from(i.position)) * scale;
                let rotation = turn(glam::Quat::from_array(i.rotation));
                Instance {
                    position: position.to_array(),
                    rotation: rotation.to_array(),
                    scale: i.scale,
                    tint: i.tint,
                }
            })
            .collect();
        let asset = format!("{map_key}/scatter/{layer}");
        let shape_file = format!("{id}_{layer}.shape.json");
        put(
            &format!("map/{shape_file}"),
            &serde_json::to_vec(&converted.shape)?,
            report,
        )?;
        for (material, png) in &converted.textures {
            let name = format!("{material}.png");
            put(&format!("map/{name}"), png, report)?;
            built.textures.insert(name.clone(), name);
        }
        for (slot, m) in converted.shape.materials.iter().enumerate() {
            built.bindings.push(
                json!({ "asset": asset, "shape_material": slot, "skin": "", "texture": format!("{}.png", m.name) }),
            );
        }
        let placements = format!("{id}_{layer}.scatter");
        put(
            &format!("map/{placements}"),
            &scatter::encode(&placed)?,
            report,
        )?;
        let record = Record {
            name: layer.clone(),
            asset: asset.clone(),
            file: placements,
            fade,
            order,
            modulate_2x,
            detail: detail as u8,
        };
        record.validate()?;
        built.assets.insert(asset, shape_file);
        built.records.push(record);
        report.warnings.push(format!(
            "scatter layer {layer}: {} instances of one model",
            placed.len()
        ));
    }
    Ok(built)
}
