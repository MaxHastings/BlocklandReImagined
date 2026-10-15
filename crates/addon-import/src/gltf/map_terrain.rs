//! A map Add-On's heightmap terrain from a `--terrain` folder
//! (docs/modding/maps.md): `height.png`, up to eight ground textures
//! `layer0`..`layer7` with grey masks `weight0`..`weight7`, and an optional
//! `detail` texture, sized and placed by the scene's `bri_terrain...`
//! custom properties. It becomes the same native terrain the converted v20
//! maps use, with its sunlight baked as v20 baked a mission's.
use super::Report;
use super::map_environment::store;
use anyhow::{Context, Result, ensure};
use bri_console::Clamp;
use bri_content::scene::{Kind as NodeKind, Node as SceneNode};
use bri_content::terrain_field::{
    RepeatSource, TERRAIN_INSTANCE_SCHEMA, TerrainBump, TerrainInstance, TerrainTexture,
};
use bri_content::{TERRAIN_SCHEMA, Terrain, TerrainLayer};
use glam::{Mat4, Vec3};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

/// The most samples along a side: the terrain streams full-detail tiles,
/// and 1024 is four times v20's whole terrain block.
const MAX_SIDE: u32 = 1024;
const EXTENSIONS: [&str; 3] = ["png", "jpg", "jpeg"];

/// The terrain's scene node, its bundle records and its files.
pub(super) struct Built {
    /// Taken by the map's node list.
    pub node: Option<SceneNode>,
    pub asset: String,
    pub file: String,
    pub terrain: Terrain,
    pub instance: TerrainInstance,
    pub bindings: Vec<Value>,
    /// `bundle.lighting[map].terrain`'s record.
    pub lighting: Value,
    /// Half the terrain's width and its highest point, for fog and the
    /// default spawn.
    pub reach: f32,
    pub top: f32,
}

fn find(dir: &Path, stem: &str) -> Option<std::path::PathBuf> {
    EXTENSIONS
        .iter()
        .map(|e| dir.join(format!("{stem}.{e}")))
        .find(|p| p.is_file())
}

/// A grey picture resampled to `side` x `side`, 0 to 1.
fn grey(path: &Path, side: u32) -> Result<Vec<f32>> {
    let image = image::open(path)
        .with_context(|| format!("{} is not a picture", path.display()))?
        .into_luma16();
    let image = if image.dimensions() == (side, side) {
        image
    } else {
        image::imageops::resize(&image, side, side, image::imageops::FilterType::Triangle)
    };
    Ok(image
        .pixels()
        .map(|p| f32::from(p.0[0]) / 65535.0)
        .collect())
}

fn extra(extras: &Value, key: &str) -> Option<Vec<f32>> {
    let v = extras.get(key)?;
    let numbers: Vec<f32> = match v {
        Value::Array(a) => a
            .iter()
            .filter_map(Value::as_f64)
            .map(|v| v as f32)
            .collect(),
        _ => v.as_f64().map(|v| v as f32).into_iter().collect(),
    };
    Some(numbers.into_iter().filter(|v| v.is_finite()).collect())
}

/// Sunlight on the terrain, as v20's lightmaps hold it: ambient plus the
/// sun where it reaches, with the terrain's own shadows. Shadows of the
/// level's models are not baked.
fn bake(
    elevations: &[f32],
    side: usize,
    spacing: f32,
    sun: &BTreeMap<String, String>,
    solid: impl Fn(usize, usize) -> bool,
) -> Vec<u8> {
    let three = |key: &str, default: [f32; 3]| {
        let v: Vec<f32> = sun
            .get(key)
            .map(|s| {
                s.split_whitespace()
                    .filter_map(|v| v.parse().ok())
                    .collect()
            })
            .unwrap_or_default();
        match v[..] {
            [r, g, b] => Vec3::new(r, g, b),
            _ => Vec3::from(default),
        }
    };
    let angle = |key: &str| {
        sun.get(key)
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(50.0)
            .to_radians()
    };
    let (azimuth, elevation) = (angle("azimuth"), angle("elevation"));
    let color = three("color", [0.85, 0.82, 0.75]);
    let ambient = three("ambient", [0.45, 0.45, 0.5]);
    // Toward the sun, in terrain grid space: x along the grid's x, y up,
    // z along the grid's rows (native -Z). The inverse of `map_environment::sun`.
    let t = Vec3::new(
        -azimuth.sin() * elevation.cos(),
        -azimuth.cos() * elevation.cos(),
        -elevation.sin(),
    );
    let native = Vec3::new(t.x, t.z, -t.y);
    let toward = -native;
    let grid_toward = Vec3::new(toward.x, toward.y, -toward.z);
    let h = |x: usize, y: usize| elevations[y * side + x];
    let top = elevations.iter().copied().fold(f32::MIN, f32::max);
    let last = side - 2;
    let mut out = Vec::with_capacity(side * side * 3);
    for y in 0..side {
        for x in 0..side {
            let (cx, cy) = (x.min(last), y.min(last));
            let (x0, x1) = (cx.saturating_sub(1), (cx + 1).min(last));
            let (y0, y1) = (cy.saturating_sub(1), (cy + 1).min(last));
            let dx = (h(x1, cy) - h(x0, cy)) / ((x1 - x0).max(1) as f32 * spacing);
            let dy = (h(cx, y1) - h(cx, y0)) / ((y1 - y0).max(1) as f32 * spacing);
            let normal = Vec3::new(-dx, 1.0, -dy).normalize();
            let facing = normal.dot(grid_toward).max(0.0);
            let mut lit = facing > 0.0 && solid(cx, cy);
            if lit && grid_toward.y < 0.999 {
                let flat = Vec3::new(grid_toward.x, 0.0, grid_toward.z).normalize();
                let rise = grid_toward.y / Vec3::new(grid_toward.x, 0.0, grid_toward.z).length();
                let start = h(cx, cy) + 0.01 * spacing;
                let mut d = 1.0;
                loop {
                    let height = start + rise * d * spacing;
                    if height > top {
                        break;
                    }
                    let (gx, gy) = (cx as f32 + flat.x * d, cy as f32 + flat.z * d);
                    if gx < 0.0 || gy < 0.0 || gx > last as f32 || gy > last as f32 {
                        break;
                    }
                    if h(gx.round() as usize, gy.round() as usize) > height {
                        lit = false;
                        break;
                    }
                    d += 1.0;
                }
            }
            let light = ambient + color * facing * f32::from(u8::from(lit));
            out.extend(
                light
                    .to_array()
                    .map(|c| (c.clamped(0.0, 1.0) * 255.0).round() as u8),
            );
        }
    }
    out
}

/// Builds the terrain from `dir`; `put` writes its pictures into `map/`.
#[allow(clippy::too_many_arguments)]
pub(super) fn terrain(
    dir: &Path,
    extras: &Value,
    map_id: &str,
    node_index: usize,
    sun: &BTreeMap<String, String>,
    report: &mut Report,
    put: &mut dyn FnMut(&str, &[u8]) -> Result<()>,
) -> Result<Built> {
    let height_path = find(dir, "height")
        .with_context(|| format!("the terrain folder {} has no height.png", dir.display()))?;
    let (w, h) = image::image_dimensions(&height_path)
        .with_context(|| format!("{} is not a picture", height_path.display()))?;
    let wanted = w.max(h).clamp(16, MAX_SIDE);
    let side = if wanted.is_power_of_two() {
        wanted
    } else {
        wanted.next_power_of_two() / 2
    };
    if (w, h) != (side, side) {
        report.warnings.push(format!(
            "the terrain's height.png is {w}x{h}; resampled to {side}x{side}"
        ));
    }
    let n = side as usize;
    let size =
        extra(extras, "bri_terrain_size").map_or(512.0, |v| v.first().copied().unwrap_or(512.0));
    let height =
        extra(extras, "bri_terrain_height").map_or(64.0, |v| v.first().copied().unwrap_or(64.0));
    let center = match extra(extras, "bri_terrain_center").as_deref() {
        None => [0.0; 3],
        Some(&[x, y, z]) => [x, y, z],
        Some(_) => anyhow::bail!("bri_terrain_center must be [x, y, z] in Blender's coordinates"),
    };
    ensure!(
        size > 0.0 && height >= 0.0,
        "bri_terrain_size must be above 0 and bri_terrain_height 0 or more"
    );
    let scale = report.scale;
    let spacing = size * scale / (side - 1) as f32;
    // Picture rows run from Blender's +Y down, columns along +X; the
    // native grid runs along native +X (Blender -X) and native -Z
    // (Blender -Y), so columns are mirrored.
    let samples = grey(&height_path, side)?;
    let mut elevations = vec![0.0; n * n];
    for gy in 0..n {
        for gx in 0..n {
            elevations[gy * n + gx] = samples[gy * n + (n - 1 - gx)] * height * scale;
        }
    }
    let origin = [
        -(center[0] + size / 2.0) * scale,
        center[2] * scale,
        (center[1] + size / 2.0) * scale,
    ];
    // The block wraps around: its last row and column of squares would join
    // the far edge back to the near one, so they are left empty.
    let mut empty_runs = vec![[(side - 1) * side, side]];
    empty_runs.extend((0..side - 1).map(|y| [y * side + side - 1, 1]));

    let mut layers = Vec::new();
    let mut weights: Vec<Vec<f32>> = Vec::new();
    let mut bindings = Vec::new();
    let asset = format!("{map_id}/terrain");
    for slot in 0..8u8 {
        let Some(path) = find(dir, &format!("layer{slot}")) else {
            continue;
        };
        let bytes = std::fs::read(&path)?;
        let image = store(&bytes, &format!("terrain layer{slot}"), put)?;
        let weight = match find(dir, &format!("weight{slot}")) {
            Some(p) => grey(&p, side)?,
            None if slot == 0 => vec![-1.0; n * n],
            None => anyhow::bail!(
                "the terrain's layer{slot} has no weight{slot}.png saying where it shows"
            ),
        };
        weights.push(
            (0..n * n)
                .map(|i| {
                    let (gy, gx) = (i / n, i % n);
                    weight[gy * n + (n - 1 - gx)]
                })
                .collect(),
        );
        bindings.push(json!({ "asset": asset, "terrain_layer": slot, "texture": image.file, "source": image.source }));
        layers.push(TerrainLayer {
            slot,
            material: image.source,
            weights: Vec::new(),
        });
    }
    ensure!(
        !layers.is_empty(),
        "the terrain folder {} has no layer0.png ground texture",
        dir.display()
    );
    // A missing weight0 takes whatever the other layers leave.
    let mut primary_layers = vec![0u8; n * n];
    for i in 0..n * n {
        let rest: f32 = weights.iter().skip(1).map(|w| w[i]).sum();
        if weights[0][i] < 0.0 {
            weights[0][i] = (1.0 - rest).max(0.0);
        }
        let total: f32 = weights.iter().map(|w| w[i]).sum();
        let mut best = (0, -1.0);
        for (k, w) in weights.iter_mut().enumerate() {
            w[i] = if total > 0.0 {
                w[i] / total
            } else {
                f32::from(u8::from(k == 0))
            };
            if w[i] > best.1 {
                best = (k, w[i]);
            }
        }
        primary_layers[i] = layers[best.0].slot;
    }
    for (layer, w) in layers.iter_mut().zip(&weights) {
        layer.weights = w.iter().map(|v| (v * 255.0).round() as u8).collect();
    }
    let detail = match find(dir, "detail") {
        Some(p) => {
            let image = store(&std::fs::read(&p)?, "terrain detail", put)?;
            Some(TerrainTexture {
                file: image.file,
                source: image.source,
            })
        }
        None => None,
    };
    let terrain = Terrain {
        schema_version: TERRAIN_SCHEMA,
        id: asset.clone(),
        side,
        elevations,
        layers,
        primary_layers,
    };
    terrain.validate()?;
    let instance = TerrainInstance {
        schema_version: TERRAIN_INSTANCE_SCHEMA,
        node: node_index,
        terrain: asset.clone(),
        square_size: spacing,
        origin,
        repeat: false,
        repeat_source: RepeatSource::Authored,
        empty_runs,
        detail,
        bump: TerrainBump {
            texture: None,
            scale: 1.0,
            offset: 0.0,
            zero_scale: 8,
        },
        diagnostics: Vec::new(),
    };
    instance.validate(side)?;
    let lightmap = bake(&terrain.elevations, n, spacing, sun, |x, y| {
        x < n - 1 && y < n - 1
    });
    let mut png = Vec::new();
    image::DynamicImage::ImageRgb8(
        image::RgbImage::from_raw(side, side, lightmap).context("terrain lightmap")?,
    )
    .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)?;
    let lightmap = store(&png, "terrain lightmap", put)?;
    let top = origin[1] + terrain.elevations.iter().copied().fold(0.0, f32::max);
    Ok(Built {
        node: Some(SceneNode {
            name: "terrain".into(),
            parent: None,
            kind: NodeKind::Terrain,
            transform: Mat4::from_translation(Vec3::from(origin)).to_cols_array(),
            asset: Some(asset.clone()),
            properties: BTreeMap::new(),
        }),
        file: format!(
            "{}.terrain.json",
            map_id.rsplit('/').next().unwrap_or("map")
        ),
        asset,
        terrain,
        instance,
        bindings,
        lighting: json!({ "node": node_index, "file": lightmap.file, "source": "baked" }),
        reach: size * scale / 2.0,
        top,
    })
}
