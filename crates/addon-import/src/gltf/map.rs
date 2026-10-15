//! `--kind map`: a whole glTF scene as a map Add-On
//! (docs/architecture/map-packages.md, docs/modding/maps.md).
//!
//! Every visible mesh becomes one native model the map places; `col_`
//! meshes are what players collide with, or, when there are none, the
//! visible meshes themselves. Empties named `spawn...` are spawn points,
//! and Blender lights (`KHR_lights_punctual`) are map lights in map2dif's
//! types. The result is an Add-On folder holding `package.json` and a
//! native map bundle in `map/`, the same files the converted v20 maps use.
use super::{Converted, Document, Options, Point, Report, title};
use anyhow::{Context, Result, ensure};
use bri_content::scene::{Kind as NodeKind, LIGHT_TYPES, Node as SceneNode, Scene};
use bri_content::shape::Shape;
use glam::{Mat4, Quat, Vec3};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// The sun a map gets: high, slightly warm, with soft ambient, like v20's
/// outdoor maps.
const SUN: [(&str, &str); 4] = [
    ("azimuth", "30"),
    ("elevation", "50"),
    ("color", "0.85 0.82 0.75"),
    ("ambient", "0.45 0.45 0.5"),
];

/// Each shape node's transform in the model's frame.
fn global_transforms(shape: &Shape) -> Vec<Mat4> {
    let mut out: Vec<Mat4> = Vec::with_capacity(shape.nodes.len());
    for node in &shape.nodes {
        let local = Mat4::from_rotation_translation(
            Quat::from_array(node.rotation).normalize(),
            Vec3::from(node.translation),
        );
        out.push(match node.parent {
            Some(p) if p < out.len() => out[p] * local,
            _ => local,
        });
    }
    out
}

/// A number from a light's or its node's custom properties (`extras`).
fn extra<'a>(node: &'a Value, light: &'a Value, key: &str) -> Option<&'a Value> {
    node["extras"].get(key).or_else(|| light["extras"].get(key))
}
fn extra_number(node: &Value, light: &Value, key: &str) -> Option<f32> {
    extra(node, light, key)
        .and_then(Value::as_f64)
        .map(|v| v as f32)
        .filter(|v| v.is_finite())
}

/// The map's light nodes from the glTF scene's `KHR_lights_punctual`
/// lights, placed where the converted model put their nodes.
fn lights(
    doc: &Document,
    shape: &Shape,
    globals: &[Mat4],
    report: &mut Report,
) -> Result<Vec<SceneNode>> {
    let json = &doc.json;
    let defined = json["extensions"]["KHR_lights_punctual"]["lights"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut out = Vec::new();
    for node in json["nodes"].as_array().into_iter().flatten() {
        let Some(index) = node["extensions"]["KHR_lights_punctual"]["light"].as_u64() else {
            continue;
        };
        let name = node["name"].as_str().unwrap_or_default().trim().to_string();
        let light = defined
            .get(index as usize)
            .with_context(|| format!("light node `{name}` names missing light {index}"))?;
        let gltf_type = light["type"].as_str().unwrap_or("point");
        if gltf_type == "directional" {
            report.warnings.push(format!(
                "`{name}` is a sun light; the map has one sun of its own, so it is left out"
            ));
            continue;
        }
        let kind = extra(node, light, "bri_light_type")
            .and_then(Value::as_str)
            .map(str::to_ascii_lowercase)
            .unwrap_or_else(|| if gltf_type == "spot" { "spot" } else { "omni" }.into());
        ensure!(
            LIGHT_TYPES.contains(&kind.as_str()),
            "light `{name}`: bri_light_type `{kind}` is not one of {}",
            LIGHT_TYPES.join(", ")
        );
        let Some(k) = shape.nodes.iter().position(|n| n.name == name) else {
            report
                .warnings
                .push(format!("light `{name}` is outside the scene; left out"));
            continue;
        };
        let color: Vec<f32> = light["color"]
            .as_array()
            .map(|a| a.iter().map(|v| v.as_f64().unwrap_or(1.0) as f32).collect())
            .unwrap_or_else(|| vec![1.0; 3]);
        ensure!(color.len() == 3, "light `{name}` has a bad colour");
        let outer = extra_number(node, light, "bri_radius")
            .or_else(|| light["range"].as_f64().map(|r| r as f32))
            .unwrap_or(10.0)
            * report.scale;
        let inner = extra_number(node, light, "bri_inner").unwrap_or(0.0) * report.scale;
        let brightness = extra_number(node, light, "bri_brightness").unwrap_or(1.0);
        ensure!(
            outer > inner && inner >= 0.0 && brightness >= 0.0,
            "light `{name}`: needs 0 <= bri_inner < its radius and bri_brightness 0 or more"
        );
        let mut properties = BTreeMap::from([
            ("type".to_string(), kind.clone()),
            (
                "color".to_string(),
                format!("{} {} {}", color[0], color[1], color[2]),
            ),
            ("brightness".to_string(), brightness.to_string()),
            ("inner".to_string(), inner.to_string()),
            ("outer".to_string(), outer.to_string()),
        ]);
        if let Some(spot) = light.get("spot") {
            for (key, value) in [
                ("inner_cone", "innerConeAngle"),
                ("outer_cone", "outerConeAngle"),
            ] {
                if let Some(v) = spot[value].as_f64() {
                    properties.insert(key.into(), (v as f32).to_degrees().to_string());
                }
            }
        }
        // map2dif's animated types keep their own settings for when the
        // engine plays them (`bri_speed`, `bri_color2`, ...).
        if let Some(Value::Object(extras)) = node.get("extras") {
            for (key, value) in extras {
                if let Some(setting) = key.strip_prefix("bri_")
                    && !["light_type", "radius", "inner", "brightness"].contains(&setting)
                {
                    // Numbers as the scene writes them (`2`, not `2.0`),
                    // a list (`bri_color2`) as space-separated numbers.
                    let number = |v: &Value| match v.as_f64() {
                        Some(n) => (n as f32).to_string(),
                        None => v.as_str().map_or_else(|| v.to_string(), str::to_owned),
                    };
                    let text = match value {
                        Value::Array(a) => a.iter().map(number).collect::<Vec<_>>().join(" "),
                        other => number(other),
                    };
                    properties.insert(setting.into(), text);
                }
            }
        }
        out.push(SceneNode {
            name,
            parent: None,
            kind: NodeKind::Light,
            transform: globals[k].to_cols_array(),
            asset: None,
            properties,
        });
    }
    let animated: Vec<_> = out
        .iter()
        .filter(|n| n.properties["type"] != "omni")
        .map(|n| format!("`{}` ({})", n.name, n.properties["type"]))
        .collect();
    if !animated.is_empty() {
        report.warnings.push(format!(
            "{} keep their type, but draw as steady omni lights until the engine plays map light animation and spot cones",
            animated.join(", ")
        ));
    }
    Ok(out)
}

/// Writes the map Add-On into `opts.out`.
pub(super) fn write(
    opts: &Options,
    id: &str,
    converted: Converted,
    doc: &Document,
) -> Result<Report> {
    let Converted {
        shape,
        textures,
        mut report,
        material_sources,
    } = converted;
    let out = &opts.out;
    let put = |rel: &str, bytes: &[u8], report: &mut Report| -> Result<()> {
        let path = out.join(rel);
        std::fs::create_dir_all(path.parent().context("output path")?)?;
        std::fs::write(path, bytes)?;
        report.files.push(rel.into());
        Ok(())
    };
    let ns = crate::namespace_for(id)?;
    let map_id = format!("{ns}:map/{id}");
    let name = opts.name.clone().unwrap_or_else(|| title(id));
    report.id = ns.clone();
    report.model = map_id.clone();

    let globals = global_transforms(&shape);
    let mut nodes = vec![SceneNode {
        name: "sun".into(),
        parent: None,
        kind: NodeKind::Sun,
        transform: Mat4::IDENTITY.to_cols_array(),
        asset: None,
        properties: SUN
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect(),
    }];
    let model = format!("{map_id}/model");
    nodes.push(SceneNode {
        name: id.into(),
        parent: None,
        kind: NodeKind::StaticModel,
        transform: Mat4::IDENTITY.to_cols_array(),
        asset: Some(model.clone()),
        properties: BTreeMap::new(),
    });
    for (k, node) in shape.nodes.iter().enumerate() {
        if node.name.to_ascii_lowercase().starts_with("spawn") {
            let (_, rotation, position) = globals[k].to_scale_rotation_translation();
            nodes.push(SceneNode {
                name: node.name.clone(),
                parent: None,
                kind: NodeKind::Spawn,
                transform: Mat4::from_rotation_translation(rotation, position).to_cols_array(),
                asset: None,
                properties: BTreeMap::new(),
            });
            report.points.push(Point {
                node: node.name.clone(),
                position: position.to_array(),
            });
        }
    }
    if !nodes.iter().any(|n| matches!(n.kind, NodeKind::Spawn)) {
        // Above the middle of the map, so players land on whatever is there.
        let top = report.height;
        report.warnings.push(format!(
            "no Empty named `spawn`: players spawn above the middle of the map, {top:.1} units up; add one or more spawn Empties where they should start"
        ));
        nodes.push(SceneNode {
            name: "spawn".into(),
            parent: None,
            kind: NodeKind::Spawn,
            transform: Mat4::from_translation(Vec3::new(0.0, top + 1.0, 0.0)).to_cols_array(),
            asset: None,
            properties: BTreeMap::new(),
        });
    }
    nodes.extend(lights(doc, &shape, &globals, &mut report)?);
    let scene = Scene {
        schema_version: 1,
        id: map_id.clone(),
        name: name.clone(),
        nodes,
        pending_scripts: Vec::new(),
    };
    scene.lights()?;

    let model_file = format!("{id}.shape.json");
    let scene_file = format!("{id}.scene.json");
    put(
        &format!("map/{model_file}"),
        &serde_json::to_vec(&shape)?,
        &mut report,
    )?;
    put(
        &format!("map/{scene_file}"),
        &serde_json::to_vec_pretty(&scene)?,
        &mut report,
    )?;
    let mut texture_files = BTreeMap::new();
    for (material, png) in &textures {
        let file = format!("{material}.png");
        put(&format!("map/{file}"), png, &mut report)?;
        texture_files.insert(file.clone(), file);
    }
    let bindings: Vec<Value> = shape
        .materials
        .iter()
        .enumerate()
        .map(|(slot, m)| {
            json!({ "asset": model, "shape_material": slot, "skin": "", "texture": format!("{}.png", m.name) })
        })
        .collect();
    // A material's custom property `bri_surface` names what it is made of
    // (`dirt`, `metal`), for footsteps and impacts.
    let mut surfaces = serde_json::Map::new();
    for (m, source) in shape.materials.iter().zip(&material_sources) {
        let Some(kind) = source
            .and_then(|i| doc.json["materials"].get(i))
            .and_then(|g| g["extras"].get("bri_surface"))
        else {
            continue;
        };
        match kind
            .as_str()
            .filter(|k| bri_content::surface::valid_name(k))
        {
            Some(kind) => {
                surfaces.insert(m.name.clone(), kind.into());
            }
            None => report.warnings.push(format!(
                "material `{}`'s bri_surface must be 1 to 32 letters, digits, `_` or `-`; left out",
                m.name
            )),
        }
    }
    let bundle = json!({
        "schema_version": 1,
        "maps": [{ "id": map_id, "name": name, "file": scene_file }],
        "surfaces": { map_id.clone(): surfaces },
        "assets": { model.clone(): model_file },
        "textures": texture_files,
        "bindings": bindings,
        "lighting": {},
        "terrains": { map_id.clone(): [] },
        "environments": {},
    });
    put(
        "map/bundle.json",
        &serde_json::to_vec_pretty(&bundle)?,
        &mut report,
    )?;
    let manifest = json!({
        "schema_version": 1,
        "id": ns,
        "version": opts.version,
        "api": 1,
        "name": name,
        "description": format!("{name}, a map imported from {} by bri-import-addon gltf.", report.source),
        "authors": [],
        "license": "proprietary",
        "provenance": { "source": format!("glTF scene {}", report.source), "notes": "Imported by bri-import-addon gltf. Set the licence of the original scene." },
        "dependencies": {},
        "capabilities": [],
        "provides": [
            { "kind": "map", "id": map_id, "file": "map/bundle.json" },
        ],
    });
    put(
        "package.json",
        &serde_json::to_vec_pretty(&manifest)?,
        &mut report,
    )?;
    Ok(report)
}
