//! A map Add-On's sun, sky and water from the glTF scene
//! (docs/modding/maps.md): a Blender Sun light, the scene's `bri_sky...`
//! and `bri_fog...` custom properties or a `--sky` folder of six pictures,
//! and meshes named `water...`.
use super::{Document, Report, WaterMesh, make_material};
use anyhow::{Context, Result, bail, ensure};
use bri_console::Clamp;
use bri_content::environment::{Environment, Fog, Image};
use bri_content::scene::{Kind as NodeKind, Node as SceneNode};
use bri_content::shape::Shape;
use bri_content::water::Water;
use glam::{Mat4, Quat, Vec3};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// The sky a map gets when it names none: v20's daytime blue.
const SKY_COLOR: [f32; 3] = [0.55, 0.7, 0.9];
/// A sky folder's pictures, in the order a native sky keeps its faces
/// (native +Z, +X, -Z, -X, +Y, -Y). `front` faces Blender's +Y.
pub const SKY_FACES: [&str; 6] = ["front", "right", "back", "left", "up", "down"];
/// v20's liquid types (`WaterBlock.liquidType`); only lava harms players.
const LIQUIDS: [&str; 7] = [
    "OceanWater",
    "RiverWater",
    "StagnantWater",
    "Lava",
    "HotLava",
    "CrustyLava",
    "Quicksand",
];

/// A picture written into the map folder under its hash.
pub(super) fn store(
    bytes: &[u8],
    source: &str,
    put: &mut dyn FnMut(&str, &[u8]) -> Result<()>,
) -> Result<Image> {
    let decoded =
        image::load_from_memory(bytes).with_context(|| format!("{source} is not a picture"))?;
    ensure!(
        (1..=8192).contains(&decoded.width()) && (1..=8192).contains(&decoded.height()),
        "{source} is larger than 8192 pixels"
    );
    let (bytes, extension) = match image::guess_format(bytes)? {
        image::ImageFormat::Png => (bytes.to_vec(), "png"),
        image::ImageFormat::Jpeg => (bytes.to_vec(), "jpg"),
        _ => {
            let mut png = Vec::new();
            decoded.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)?;
            (png, "png")
        }
    };
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    let file = format!("{sha256}.{extension}");
    put(&file, &bytes)?;
    Ok(Image {
        file,
        source: source.into(),
        sha256,
        width: decoded.width(),
        height: decoded.height(),
    })
}

/// Three numbers from `[r, g, b]` or `"r g b"`.
fn color(value: &Value, what: &str) -> Result<[f32; 3]> {
    let numbers: Vec<f32> = match value {
        Value::Array(a) => a
            .iter()
            .filter_map(Value::as_f64)
            .map(|v| v as f32)
            .collect(),
        Value::String(s) => s
            .split_whitespace()
            .filter_map(|v| v.parse().ok())
            .collect(),
        _ => Vec::new(),
    };
    match numbers.as_slice() {
        [r, g, b, ..] if [r, g, b].iter().all(|v| (0.0..=1.0).contains(*v)) => Ok([*r, *g, *b]),
        _ => bail!("{what} must be three numbers from 0 to 1, like [0.5, 0.7, 0.9]"),
    }
}
fn number(value: &Value) -> Option<f32> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|s| s.trim().parse().ok()))
        .map(|v| v as f32)
        .filter(|v| v.is_finite())
}

/// The Sun node's settings from the scene's first Blender Sun light, if it
/// has one: where it shines from, its colour, and `bri_ambient`.
pub(super) fn sun(
    doc: &Document,
    shape: &Shape,
    globals: &[Mat4],
    report: &mut Report,
) -> Result<Option<BTreeMap<String, String>>> {
    let json = &doc.json;
    let lights = json["extensions"]["KHR_lights_punctual"]["lights"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut found = None;
    for node in json["nodes"].as_array().into_iter().flatten() {
        let Some(index) = node["extensions"]["KHR_lights_punctual"]["light"].as_u64() else {
            continue;
        };
        let light = lights.get(index as usize).unwrap_or(&Value::Null);
        if light["type"].as_str() != Some("directional") {
            continue;
        }
        let name = node["name"].as_str().unwrap_or_default().trim().to_string();
        if found.is_some() {
            report.warnings.push(format!(
                "`{name}` is a second sun; a map has one, so it is left out"
            ));
            continue;
        }
        let Some(k) = shape.nodes.iter().position(|n| n.name == name) else {
            continue;
        };
        // A glTF light shines down its node's -Z; turned half about Y into
        // the native frame, that is the node's +Z.
        let n = globals[k].transform_vector3(Vec3::Z).normalize_or_zero();
        ensure!(n != Vec3::ZERO, "sun `{name}` has no direction");
        // Torque's frame (Z up) and the inverse of `scene::sun_direction`.
        let t = Vec3::new(n.x, -n.z, n.y);
        let elevation = (-t.z).clamped(-1.0, 1.0).asin().to_degrees();
        let azimuth = (-t.x).atan2(-t.y).to_degrees().rem_euclid(360.0).min(359.0);
        let c = color(&light["color"], "a sun's colour").unwrap_or([1.0; 3]);
        let ambient = match node["extras"].get("bri_ambient") {
            Some(v) => color(v, "bri_ambient").or_else(|_| {
                number(v)
                    .map(|a| [a; 3])
                    .context("bri_ambient must be a number or a colour")
            })?,
            None => [0.45, 0.45, 0.5],
        };
        found = Some(BTreeMap::from([
            ("azimuth".to_string(), azimuth.to_string()),
            ("elevation".to_string(), elevation.to_string()),
            ("color".to_string(), format!("{} {} {}", c[0], c[1], c[2])),
            (
                "ambient".to_string(),
                format!("{} {} {}", ambient[0], ambient[1], ambient[2]),
            ),
        ]));
    }
    Ok(found)
}

/// The map's sky: six pictures from `sky_dir`, or else a plain colour
/// (`bri_sky_color`, v20's blue by default), with the scene's fog
/// (`bri_fog` as `[start, end]`, `bri_fog_color`). Fog ends past the
/// map's far side unless the scene says otherwise, so nothing of the map
/// fades out.
pub(super) fn sky(
    doc: &Document,
    sky_dir: Option<&Path>,
    width: f32,
    put: &mut dyn FnMut(&str, &[u8]) -> Result<()>,
) -> Result<Environment> {
    let scene = doc.json["scene"].as_u64().unwrap_or(0) as usize;
    let extras = &doc.json["scenes"][scene]["extras"];
    let solid_color = match extras.get("bri_sky_color") {
        Some(v) => color(v, "bri_sky_color")?,
        None => SKY_COLOR,
    };
    let mut fog = Fog {
        start: 0.0,
        end: (2.0 * width).max(1000.0),
        color: solid_color,
    };
    fog.start = 0.7 * fog.end;
    if let Some(v) = extras.get("bri_fog") {
        let range: Vec<f32> = v
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(number)
            .collect();
        let [start, end] = range[..] else {
            bail!("bri_fog must be [start, end] in units, like [100, 600]");
        };
        fog.start = start;
        fog.end = end;
    }
    if let Some(v) = extras.get("bri_fog_color") {
        fog.color = color(v, "bri_fog_color")?;
    }
    ensure!(
        fog.end > 0.0 && fog.start >= 0.0 && fog.end >= fog.start,
        "bri_fog must be [start, end] with 0 <= start <= end and end above 0"
    );
    let mut faces = Vec::new();
    let mut hasher = Sha256::new();
    hasher.update(serde_json::to_vec(extras)?);
    if let Some(dir) = sky_dir {
        for face in SKY_FACES {
            let file = ["png", "jpg", "jpeg"]
                .iter()
                .map(|e| dir.join(format!("{face}.{e}")))
                .find(|p| p.is_file())
                .with_context(|| {
                    format!(
                        "--sky {}: no {face}.png or {face}.jpg (a sky needs {})",
                        dir.display(),
                        SKY_FACES.join(", ")
                    )
                })?;
            let bytes = std::fs::read(&file)?;
            hasher.update(&bytes);
            faces.push(store(&bytes, &format!("sky/{face}"), put)?);
        }
    }
    let textures = !faces.is_empty();
    let environment = Environment {
        schema_version: 2,
        source_materials: if textures {
            "sky folder".into()
        } else {
            "plain colour".into()
        },
        source_sha256: format!("{:x}", hasher.finalize()),
        faces,
        reflection: None,
        clouds: Vec::new(),
        textures,
        bottom: textures,
        horizon_band: false,
        solid_color,
        fog,
        warnings: Vec::new(),
    };
    environment.validate()?;
    Ok(environment)
}

/// A small original ripple texture for water painted no picture.
fn ripples(tint: [f32; 4]) -> Result<Vec<u8>> {
    let size = 64;
    let img = image::RgbaImage::from_fn(size, size, |x, y| {
        let (u, v) = (
            x as f32 / size as f32 * std::f32::consts::TAU,
            y as f32 / size as f32 * std::f32::consts::TAU,
        );
        let wave = 0.5 + 0.25 * ((u * 2.0).sin() * (v * 3.0).cos() + (u + 2.0 * v).sin());
        let shade = 0.8 + 0.3 * wave;
        image::Rgba([
            (tint[0] * shade * 255.0).clamped(0.0, 255.0) as u8,
            (tint[1] * shade * 255.0).clamped(0.0, 255.0) as u8,
            (tint[2] * shade * 255.0).clamped(0.0, 255.0) as u8,
            255,
        ])
    });
    let mut png = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)?;
    Ok(png)
}

/// Water nodes and their bodies of water: each mesh named `water...` fills
/// its box from its top (the surface) down to `bri_depth` below it, or to
/// the bottom of the map. Its material's picture is the surface; its
/// alpha the opacity; `bri_liquid` says `water` or `lava` (or one of v20's
/// liquid types).
#[allow(clippy::too_many_arguments)]
pub(super) fn waters(
    doc: &Document,
    folder: Option<&Path>,
    meshes: &[WaterMesh],
    globals: &[Mat4],
    bottom: f32,
    map_id: &str,
    first_node: usize,
    put: &mut dyn FnMut(&str, &[u8]) -> Result<()>,
    report: &mut Report,
) -> Result<(Vec<SceneNode>, Vec<Water>)> {
    let mut nodes = Vec::new();
    let mut waters = Vec::new();
    let mut taken = BTreeSet::new();
    for mesh in meshes {
        let at = globals[mesh.node];
        let (mut min, mut max) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for p in &mesh.positions {
            let p = at.transform_point3(Vec3::from(*p));
            min = min.min(p);
            max = max.max(p);
        }
        ensure!(
            min.x < max.x && min.z < max.z,
            "water `{}` is not flat and wide",
            mesh.name
        );
        let node_json = doc.json["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|n| n["name"].as_str().map(str::trim) == Some(mesh.name.as_str()));
        let extra = |key: &str| node_json.and_then(|n| n["extras"].get(key));
        let surface = max.y;
        let floor = match extra("bri_depth").and_then(number) {
            Some(depth) => {
                ensure!(
                    depth > 0.0,
                    "water `{}`: bri_depth must be above 0",
                    mesh.name
                );
                surface - depth * report.scale
            }
            None => min.y.min(bottom),
        };
        let floor = floor.min(surface - 0.1);
        let liquid = match extra("bri_liquid").and_then(Value::as_str) {
            None => "OceanWater".to_string(),
            Some(s) if s.eq_ignore_ascii_case("water") => "OceanWater".into(),
            Some(s) => LIQUIDS
                .iter()
                .find(|l| l.eq_ignore_ascii_case(s))
                .map(|l| l.to_string())
                .with_context(|| {
                    format!(
                        "water `{}`: bri_liquid `{s}` is not water, lava or one of {}",
                        mesh.name,
                        LIQUIDS.join(", ")
                    )
                })?,
        };
        let m = mesh.material.and_then(|i| doc.json["materials"].get(i));
        let factor: Vec<f32> = m
            .and_then(|m| m["pbrMetallicRoughness"]["baseColorFactor"].as_array())
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_f64)
                    .map(|v| v as f32)
                    .collect()
            })
            .unwrap_or_else(|| vec![0.2, 0.4, 0.6, 0.75]);
        let opacity = factor.get(3).copied().unwrap_or(0.75).clamped(0.05, 1.0);
        let textured = m.is_some_and(|m| m["pbrMetallicRoughness"]["baseColorTexture"].is_object());
        let png = if textured {
            make_material(doc, folder, mesh.material, "", &mut taken, report)?.1
        } else {
            ripples([factor[0], factor[1], factor[2], 1.0])?
        };
        let image = store(&png, &format!("water/{}", mesh.name), put)?;
        let node = first_node + nodes.len();
        let extent = Vec3::new(max.x - min.x, surface - floor, max.z - min.z);
        let corner = Vec3::new(min.x, floor, min.z);
        nodes.push(SceneNode {
            name: mesh.name.clone(),
            parent: None,
            kind: NodeKind::Water,
            transform: Mat4::from_scale_rotation_translation(extent, Quat::IDENTITY, corner)
                .to_cols_array(),
            asset: None,
            properties: BTreeMap::from([("liquidtype".to_string(), liquid.clone())]),
        });
        let lava = liquid.contains("Lava");
        let water = Water {
            schema_version: 1,
            node,
            id: format!("{map_id}/water/{}", waters.len()),
            min: corner.to_array(),
            max: [max.x, surface, max.z],
            repeat_period: None,
            liquid_type: liquid,
            density: 1.0,
            viscosity: if lava { 40.0 } else { 15.0 },
            surface: image.clone(),
            shore: image,
            reflection: None,
            opacity,
            wave_amplitude: 1.0,
            flow: [0.0; 2],
            distortion: [0.1, 0.05, 0.5],
            // v20's `WaterBlock` defaults.
            tiles: [50.0, 60.0],
            depth_mask: true,
            depth_alpha: [0.3, 1.0, 20.0, 1.0],
            reflection_intensity: 0.0,
            parallax: 0.5,
            warnings: Vec::new(),
            current: [0.0; 3],
            tint: None,
        };
        water.validate()?;
        waters.push(water);
    }
    Ok((nodes, waters))
}
