//! Loader for already converted native bundles. This module cannot read Torque
//! assets and never searches the original installation for missing resources.
use crate::{scene::*, terrain_scene::TerrainScene};
use anyhow::{Context, Result, ensure};
use bri_content::{
    Terrain,
    interior::Interior,
    scene::{Kind, Node, Scene},
    terrain_field::{TerrainField, TerrainInstance},
};
use glam::{Mat4, Vec3};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::{Cursor, Read},
    path::{Component, Path, PathBuf},
    sync::Arc,
};

/// A map's static scene plus its camera-following terrain placements.
pub struct MapScene {
    pub scene: SceneData,
    pub terrain: Vec<TerrainScene>,
}

fn file(root: &Path, name: &str) -> Result<PathBuf> {
    let path = Path::new(name);
    ensure!(
        !path.is_absolute()
            && path
                .components()
                .all(|p| matches!(p, Component::Normal(_) | Component::CurDir)),
        "Bundle path escapes native content: {name}"
    );
    let path = root
        .join(path)
        .canonicalize()
        .with_context(|| format!("Missing native bundle resource {name}"))?;
    ensure!(
        path.starts_with(root),
        "Native bundle resource escapes content root: {name}"
    );
    Ok(path)
}
fn read(root: &Path, name: &str) -> Result<Vec<u8>> {
    Ok(std::fs::read(file(root, name)?)?)
}
fn transform(node: &Node) -> Result<Mat4> {
    ensure!(
        node.transform.iter().all(|v| v.is_finite()),
        "Nonfinite placement for {}",
        node.name
    );
    let matrix = Mat4::from_cols_array(&node.transform);
    ensure!(
        matrix.determinant().abs() > 0.0000001,
        "Singular placement for {}",
        node.name
    );
    Ok(matrix)
}
fn decode(bytes: &[u8], label: &str, srgb: bool) -> Result<SceneImage> {
    let (width, height) = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()?
        .into_dimensions()
        .with_context(|| format!("Reading native image dimensions {label}"))?;
    ensure!(
        width > 0 && height > 0 && width <= 8192 && height <= 8192,
        "Native image dimensions exceed scene limit: {label}"
    );
    let image = image::load_from_memory(bytes)
        .with_context(|| format!("Decoding native image {label}"))?
        .to_rgba8();
    let (width, height) = image.dimensions();
    ensure!(
        width > 0 && height > 0 && width <= 8192 && height <= 8192,
        "Native image dimensions exceed scene limit: {label}"
    );
    Ok(SceneImage {
        label: label.into(),
        width,
        height,
        rgba: image.into_raw(),
        srgb,
    })
}
fn texture(
    root: &Path,
    name: &str,
    srgb: bool,
    out: &mut SceneData,
    cache: &mut BTreeMap<(String, bool), usize>,
) -> Result<usize> {
    let key = (name.to_string(), srgb);
    if let Some(&index) = cache.get(&key) {
        return Ok(index);
    }
    let index = out.images.len();
    out.images.push(decode(&read(root, name)?, name, srgb)?);
    cache.insert(key, index);
    Ok(index)
}
fn alpha(image: &SceneImage) -> AlphaMode {
    if image.rgba.chunks_exact(4).any(|p| p[3] > 0 && p[3] < 255) {
        AlphaMode::Blend
    } else if image.rgba.chunks_exact(4).any(|p| p[3] == 0) {
        AlphaMode::Mask(0.5)
    } else {
        AlphaMode::Opaque
    }
}
/// Per-axis `(offset, scale)` mapping a surface's lightmap coordinates onto the
/// centers of the texels it owns in its shared lightmap sheet: those whose
/// centers lie strictly inside the surface's lightmap footprint. Mission
/// lightmaps pack neighbouring surfaces edge to edge, and a footprint often
/// reaches the center of the next surface's first texel, so bilinear filtering
/// pulled that texel into polygon edges: bright seams across dark floors. The
/// remap is affine, so planar interpolation is kept.
fn lightmap_inset(
    vertices: &[bri_content::interior::Vertex],
    sheet: &SceneImage,
    lit: bool,
) -> [(f32, f32); 2] {
    std::array::from_fn(|a| {
        let size = [sheet.width, sheet.height][a] as f32;
        let (low, high) = vertices.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| {
            (lo.min(v.lightmap_uv[a]), hi.max(v.lightmap_uv[a]))
        });
        if !lit || high.partial_cmp(&low) != Some(std::cmp::Ordering::Greater) {
            return (0.0, 1.0);
        }
        let (low, high) = (low * size, high * size);
        // First/last texel whose center (k + 0.5) is inside, with float slack.
        let first = (low - 0.5 + 0.001).floor() + 1.0;
        let last = (high - 0.5 - 0.001).ceil() - 1.0;
        let (inner_low, inner_high) = if last >= first {
            (low.max(first + 0.5), high.min(last + 0.5))
        } else {
            let center = ((low + high) * 0.5).floor() + 0.5;
            (center, center)
        };
        let scale = (inner_high - inner_low) / (high - low);
        // u' * size = inner_low + (u * size - low) * scale
        ((inner_low - low * scale) / size, scale)
    })
}
fn centroid(vertices: &[SceneVertex], indices: &[u32]) -> [f32; 3] {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for &index in indices {
        let p = Vec3::from(vertices[index as usize].position);
        min = min.min(p);
        max = max.max(p);
    }
    if indices.is_empty() {
        [0.0; 3]
    } else {
        ((min + max) * 0.5).to_array()
    }
}
fn rgb(value: Option<&String>, default: [f32; 3]) -> [f32; 3] {
    let values: Vec<_> = value
        .into_iter()
        .flat_map(|s| s.split_whitespace())
        .take(3)
        .map(str::parse::<f32>)
        .collect();
    match values.as_slice() {
        [Ok(r), Ok(g), Ok(b)] if r.is_finite() && g.is_finite() && b.is_finite() => [*r, *g, *b],
        _ => default,
    }
}

/// `map_id` is the stable native mission ID from bundle.json, never a display
/// name or an original install path. Missing bound materials are hard errors.
pub fn load_map_bundle(root: &Path, map_id: &str) -> Result<MapScene> {
    let root = root.canonicalize().context("Opening native map bundle")?;
    let bundle: Value = serde_json::from_slice(&read(&root, "bundle.json")?)?;
    ensure!(
        bundle["schema_version"].as_u64() == Some(1),
        "Unsupported native map bundle schema"
    );
    let record = bundle["maps"]
        .as_array()
        .context("Bundle maps missing")?
        .iter()
        .find(|m| m["id"].as_str() == Some(map_id))
        .with_context(|| format!("Map {map_id} not present in native bundle"))?;
    let scene: Scene = serde_json::from_slice(&read(
        &root,
        record["file"]
            .as_str()
            .context("Native scene filename missing")?,
    )?)?;
    ensure!(
        scene.schema_version == 1 && scene.id == map_id,
        "Invalid native map scene identity/schema"
    );
    let bindings = bundle["bindings"]
        .as_array()
        .context("Native texture bindings missing")?;
    let mut out = SceneData {
        id: scene.id.clone(),
        name: scene.name.clone(),
        ..Default::default()
    };
    out.omissions
        .extend(scene.pending_scripts.iter().map(|p| p.diagnostic()));
    if let Some(spawn) = scene.nodes.iter().find(|n| matches!(n.kind, Kind::Spawn)) {
        out.spawn = transform(spawn)?.transform_point3(Vec3::ZERO).to_array();
    } else {
        out.omissions
            .push("Map has no authored spawn; host must choose a valid spawn explicitly".into());
    }
    if let Some(sun) = scene.nodes.iter().find(|n| matches!(n.kind, Kind::Sun)) {
        let d = rgb(
            sun.properties.get("direction"),
            [0.57735, 0.57735, -0.57735],
        );
        out.sun_direction = [d[0], d[2], -d[1]]; // original Z-up to native Y-up
        out.sun_color = rgb(sun.properties.get("color"), out.sun_color);
        out.ambient = rgb(sun.properties.get("ambient"), out.ambient);
    }
    if let Some(sky) = scene.nodes.iter().find(|n| matches!(n.kind, Kind::Sky)) {
        let c = rgb(sky.properties.get("skysolidcolor"), [0.05, 0.08, 0.12]);
        out.clear_color = [c[0], c[1], c[2], 1.0];
    }
    let mut cache = BTreeMap::new();
    if let Some(environment) = bundle.get("environments").and_then(|e| e.get(&scene.id)) {
        use sha2::{Digest, Sha256};
        let environment: bri_content::environment::Environment =
            serde_json::from_value(environment.clone())?;
        environment.validate()?;
        let mut images = vec![];
        for image in environment
            .faces
            .iter()
            .chain(environment.clouds.iter().map(|c| &c.image))
        {
            let mut bytes = Vec::new();
            std::fs::File::open(file(&root, &image.file)?)?
                .take(32 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)?;
            ensure!(
                bytes.len() <= 32 * 1024 * 1024
                    && format!("{:x}", Sha256::digest(&bytes)) == image.sha256,
                "Sky image checksum mismatch: {}",
                image.source
            );
            let decoded = decode(&bytes, &image.source, true)?;
            ensure!(
                decoded.width == image.width && decoded.height == image.height,
                "Sky image dimensions changed"
            );
            let index = out.images.len();
            out.images.push(decoded);
            images.push(index);
        }
        let face_count = environment.faces.len();
        crate::environment_scene::append(
            &mut out,
            &environment,
            &images[..face_count],
            &images[face_count..],
        )?;
    } else {
        out.omissions
            .push("Native environment binding missing: authored sky/cloud/fog not rendered".into());
    }
    let fields = terrain_fields(&root, &bundle, &scene)?;
    let water_bound = load_waters(&root, &bundle, &scene, &fields, &mut out, &mut cache)?;
    let terrain = fields
        .iter()
        .map(|field| {
            load_terrain(
                &root,
                &bundle,
                bindings,
                &scene,
                field.clone(),
                out.sun_direction,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    for (node_index, node) in scene.nodes.iter().enumerate() {
        match node.kind {
            Kind::Interior=>load_interior(&root,&bundle,bindings,&scene,node_index,node,&mut out,&mut cache)?,
            Kind::StaticModel|Kind::DatablockModel if node.asset.is_some()=>load_static_shape(&root,&bundle,bindings,node,&mut out,&mut cache)?,
            Kind::StaticModel=>anyhow::bail!("Static model {} has no native asset",node.name),
            Kind::Water if water_bound=>{},
            Kind::DatablockModel|Kind::Foliage|Kind::Water|Kind::Precipitation|Kind::Unadapted=>out.omissions.push(format!("Node {node_index} {:?} '{}' is retained in the bundle but not drawn by this static architecture pass",node.kind,node.name)),
            _=>{},
        }
    }
    out.omissions.push("Storm transitions/fog volumes, dynamic lighting and texture mip/anisotropic filtering remain incomplete".into());
    out.omissions.push("Translucent geometry sorts by mesh-batch center; intersecting translucent surfaces need finer sorting".into());
    out.validate()?;
    Ok(MapScene {
        scene: out,
        terrain,
    })
}

fn terrain_fields(root: &Path, bundle: &Value, scene: &Scene) -> Result<Vec<Arc<TerrainField>>> {
    let instances: Vec<TerrainInstance> = match bundle
        .get("terrains")
        .context("Map bundle has no converted terrain instances")?
        .get(&scene.id)
    {
        Some(value) => {
            serde_json::from_value(value.clone()).context("Invalid native terrain instances")?
        }
        None => Vec::new(),
    };
    bri_content::terrain_field::map_fields(scene, instances, |id| {
        Ok(serde_json::from_slice::<Terrain>(&read(
            root,
            bundle["assets"][id]
                .as_str()
                .context("Terrain asset missing from bundle")?,
        )?)?)
    })
}

fn load_waters(
    root: &Path,
    bundle: &Value,
    scene: &Scene,
    fields: &[Arc<TerrainField>],
    out: &mut SceneData,
    cache: &mut BTreeMap<(String, bool), usize>,
) -> Result<bool> {
    let Some(records) = bundle.get("waters").and_then(|w| w.get(&scene.id)) else {
        return Ok(false);
    };
    let waters: Vec<bri_content::water::Water> = serde_json::from_value(records.clone())?;
    let expected: Vec<_> = scene
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| matches!(n.kind, Kind::Water))
        .map(|(i, _)| i)
        .collect();
    ensure!(
        waters.iter().map(|w| w.node).collect::<Vec<_>>() == expected,
        "Native water placements disagree with scene"
    );
    for water in &waters {
        water.validate()?;
        let mut textures = [0; 3];
        for (slot, image) in [
            Some(&water.surface),
            Some(&water.shore),
            water.reflection.as_ref(),
        ]
        .into_iter()
        .enumerate()
        {
            if let Some(image) = image {
                use sha2::{Digest, Sha256};
                let mut bytes = vec![];
                std::fs::File::open(file(root, &image.file)?)?
                    .take(32 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)?;
                ensure!(
                    bytes.len() <= 32 * 1024 * 1024
                        && format!("{:x}", Sha256::digest(&bytes)) == image.sha256,
                    "Water image checksum mismatch"
                );
                textures[slot] = texture(root, &image.file, true, out, cache)?;
                ensure!(
                    out.images[textures[slot]].width == image.width
                        && out.images[textures[slot]].height == image.height,
                    "Water image dimensions changed"
                );
            }
        }
        crate::water_scene::append(out, water, textures, |x, z| {
            fields
                .iter()
                .filter_map(|field| field.height(x, z))
                .max_by(f32::total_cmp)
        })?;
    }
    Ok(true)
}

fn load_static_shape(
    root: &Path,
    bundle: &Value,
    bindings: &[Value],
    node: &Node,
    out: &mut SceneData,
    cache: &mut BTreeMap<(String, bool), usize>,
) -> Result<()> {
    let id = node
        .asset
        .as_deref()
        .context("Static model has no native asset")?;
    let shape: bri_content::shape::Shape = serde_json::from_slice(&read(
        root,
        bundle["assets"][id]
            .as_str()
            .context("Static model asset missing")?,
    )?)?;
    shape.validate()?;
    let detail = shape
        .details
        .iter()
        .position(|d| !d.collision && d.pixel_threshold >= 0.0)
        .context("Static model has no visual detail")?;
    let mut materials = Vec::new();
    let skin = node.properties.get("skinname").map_or("", String::as_str);
    for (slot, authored) in shape.materials.iter().enumerate() {
        let candidates: Vec<_> = bindings
            .iter()
            .filter(|b| {
                b["asset"].as_str() == Some(id)
                    && b["shape_material"].as_u64() == Some(slot as u64)
                    && b["skin"].as_str().unwrap_or("") == skin
            })
            .collect();
        ensure!(
            candidates.len() == 1,
            "Missing/ambiguous static material binding {id}/{slot}"
        );
        let diffuse = texture(
            root,
            candidates[0]["texture"]
                .as_str()
                .context("Static texture unresolved")?,
            true,
            out,
            cache,
        )?;
        let mut material = if authored.unlit {
            Material::surface(format!("{id}/{}", authored.name), diffuse, 0)
        } else {
            Material::vertex_lit(format!("{id}/{}", authored.name), diffuse)
        };
        material.alpha = match authored.blend.as_str() {
            "opaque" => AlphaMode::Opaque,
            "alpha" => alpha(&out.images[diffuse]),
            other => anyhow::bail!("Unsupported static material blend {other} for {id}"),
        };
        if authored.environment || authored.detail_map.is_some() || authored.bump_map.is_some() {
            out.omissions.push(format!(
                "Static material {id}/{} has unbound reflection/detail/bump effects",
                authored.name
            ));
        }
        materials.push(out.materials.len());
        out.materials.push(material);
    }
    let fallback = out.materials.len();
    out.materials
        .push(Material::vertex_lit(format!("{id}/unassigned"), 0));
    let initial_sequence = node
        .properties
        .get("native_initial_sequence")
        .map(|name| {
            shape
                .animations
                .iter()
                .find(|a| a.name == *name)
                .with_context(|| format!("Missing initial sequence {name} for {}", node.name))
        })
        .transpose()?;
    let pose = bri_content::animation::sample(&shape, initial_sequence, 0.0)?;
    out.append_shape(
        crate::shape_scene::ShapeInstance {
            shape: &shape,
            pose: &pose,
            detail,
            transform: transform(node)?,
            materials: &materials,
            translucent_materials: None,
            unassigned_material: fallback,
        },
        |_| Some([1.0; 4]),
    )?;
    if shape.details.iter().filter(|d| !d.collision).count() > 1 {
        out.omissions.push(format!(
            "Static model {id} currently uses highest detail; distance LOD remains required"
        ));
    }
    if let Some(pending) = node.properties.get("native_behavior_pending") {
        out.omissions.push(format!(
            "Static object '{}' behavior pending: {pending}",
            node.name
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn load_interior(
    root: &Path,
    bundle: &Value,
    bindings: &[Value],
    scene: &Scene,
    node_index: usize,
    node: &Node,
    out: &mut SceneData,
    cache: &mut BTreeMap<(String, bool), usize>,
) -> Result<()> {
    let id = node
        .asset
        .as_deref()
        .context("Interior placement has no native asset")?;
    let interior: Interior = serde_json::from_slice(&read(
        root,
        bundle["assets"][id]
            .as_str()
            .context("Interior asset missing from bundle")?,
    )?)?;
    interior.validate()?;
    let detail = &interior.details[0];
    let placement = transform(node)?;
    let normal_transform = placement.inverse().transpose();
    let mirrored = placement.determinant() < 0.0;
    let baked = bundle["lighting"][&scene.id]["interiors"]
        .as_array()
        .context("Map is missing native interior mission-lighting bindings")?;
    let mut lightmaps = vec![];
    for (slot, lm) in detail.lightmaps.iter().enumerate() {
        let replacement = baked.iter().find(|r| {
            r["node"].as_u64() == Some(node_index as u64)
                && r["detail"].as_u64() == Some(0)
                && r["slot"].as_u64() == Some(slot as u64)
        });
        lightmaps.push(if let Some(record)=replacement {texture(root,record["file"].as_str().context("Baked interior lightmap filename missing")?,false,out,cache)?}
            else {let index=out.images.len();out.images.push(decode(&lm.png,&format!("{id}/base-lightmap-{slot}"),false)?);out.omissions.push(format!("Interior node {node_index} lightmap {slot} uses the original embedded lightmap: composed mission replacement absent"));index});
    }
    let mut materials = BTreeMap::new();
    for binding in bindings
        .iter()
        .filter(|b| b["asset"].as_str() == Some(id) && b["detail"].as_u64() == Some(0))
    {
        let material = binding["material"]
            .as_u64()
            .context("Interior material binding index missing")? as usize;
        let image = texture(
            root,
            binding["texture"]
                .as_str()
                .context("Interior texture binding missing")?,
            true,
            out,
            cache,
        )?;
        materials.insert(material, image);
    }
    let mut groups: BTreeMap<(usize, usize), Vec<u32>> = BTreeMap::new();
    for surface in &detail.surfaces {
        if surface.triangles.is_empty() {
            continue;
        }
        let diffuse = *materials.get(&surface.material).with_context(|| {
            format!(
                "Unresolved native interior texture {id} material {}",
                surface.material
            )
        })?;
        let lightmap = surface.lightmap.map_or(0, |i| lightmaps[i]);
        let group = groups.entry((diffuse, lightmap)).or_default();
        let base = u32::try_from(out.vertices.len()).context("Too many scene vertices")?;
        let inset = lightmap_inset(
            &surface.vertices,
            &out.images[lightmap],
            surface.lightmap.is_some(),
        );
        for vertex in &surface.vertices {
            out.vertices.push(SceneVertex {
                position: placement
                    .transform_point3(Vec3::from(vertex.position))
                    .to_array(),
                normal: normal_transform
                    .transform_vector3(Vec3::from(vertex.normal))
                    .normalize_or_zero()
                    .to_array(),
                uv: vertex.uv,
                lightmap_uv: std::array::from_fn(|a| {
                    inset[a].0 + vertex.lightmap_uv[a] * inset[a].1
                }),
                color: [1.0; 4],
            });
        }
        for triangle in &surface.triangles {
            let [a, b, c] = *triangle;
            group.extend(if mirrored {
                [base + a, base + c, base + b]
            } else {
                [base + a, base + b, base + c]
            });
        }
    }
    for ((diffuse, lightmap), indices) in groups {
        let material = out.materials.len();
        let mut m = Material::surface(format!("{id}/{diffuse}/{lightmap}"), diffuse, lightmap);
        m.alpha = alpha(&out.images[diffuse]);
        out.materials.push(m);
        let center = centroid(&out.vertices, &indices);
        let start = out.indices.len() as u32;
        out.indices.extend(indices);
        out.batches.push(MeshBatch {
            indices: start..out.indices.len() as u32,
            material,
            center,
        });
    }
    if !interior.subobjects.is_empty() {
        out.omissions.push(format!(
            "Interior {id} has {} subobjects not yet placed/rendered",
            interior.subobjects.len()
        ));
    }
    if detail.has_alarm {
        out.omissions.push(format!(
            "Interior {id} alarm lighting switching is not yet implemented"
        ));
    }
    Ok(())
}

fn load_terrain(
    root: &Path,
    bundle: &Value,
    bindings: &[Value],
    scene: &Scene,
    field: Arc<TerrainField>,
    sun_direction: [f32; 3],
) -> Result<TerrainScene> {
    let id = field.id.as_str();
    let terrain = &field.terrain;
    let node_index = field.node;
    let mut out = SceneData {
        id: format!("{}/terrain-{node_index}", scene.id),
        name: id.into(),
        ..Default::default()
    };
    let cache = &mut BTreeMap::new();
    let mut images = [0; 13];
    let mut bound = [false; 8];
    for binding in bindings.iter().filter(|b| b["asset"].as_str() == Some(id)) {
        let slot = binding["terrain_layer"]
            .as_u64()
            .context("Terrain material slot missing")? as usize;
        ensure!(slot < 8, "Terrain exceeds eight authored texture layers");
        images[slot] = texture(
            root,
            binding["texture"]
                .as_str()
                .context("Terrain texture filename missing")?,
            true,
            &mut out,
            cache,
        )?;
        bound[slot] = true;
    }
    for layer in &terrain.layers {
        ensure!(
            layer.slot < 8 && bound[layer.slot as usize],
            "Terrain {id} layer {} has no native texture binding",
            layer.slot
        );
    }
    let light = bundle["lighting"][&scene.id]["terrain"]
        .as_array()
        .context("Map terrain mission lighting missing")?
        .iter()
        .find(|r| r["node"].as_u64() == Some(node_index as u64))
        .context("Terrain placement has no native baked lightmap")?;
    images[8] = texture(
        root,
        light["file"]
            .as_str()
            .context("Terrain lightmap filename missing")?,
        false,
        &mut out,
        cache,
    )?;
    for group in 0..2 {
        let mut rgba = vec![0; terrain.side as usize * terrain.side as usize * 4];
        for layer in terrain
            .layers
            .iter()
            .filter(|l| l.slot as usize / 4 == group)
        {
            for (i, &weight) in layer.weights.iter().enumerate() {
                rgba[i * 4 + layer.slot as usize % 4] = weight;
            }
        }
        images[9 + group] = out.images.len();
        out.images.push(SceneImage {
            label: format!("{id}/weights-{group}"),
            width: terrain.side,
            height: terrain.side,
            rgba,
            srgb: false,
        });
    }
    // Detail and bump images keep their authored bytes: the classic passes
    // blend them in display space, not as linear colors.
    let detail = field
        .detail
        .as_ref()
        .map(|t| texture(root, &t.file, false, &mut out, cache))
        .transpose()?;
    let bump = field
        .bump
        .texture
        .as_ref()
        .map(|t| texture(root, &t.file, false, &mut out, cache))
        .transpose()?;
    images[11] = detail.unwrap_or(0);
    images[12] = bump.unwrap_or(0);
    let parameters = crate::terrain_scene::parameters(
        &field,
        sun_direction,
        detail.map(|i| [out.images[i].width, out.images[i].height]),
        bump.is_some(),
    );
    out.materials.push(Material {
        name: id.into(),
        images,
        kind: MaterialKind::Terrain,
        alpha: AlphaMode::Opaque,
        double_sided: false,
        parameters: Some(parameters),
    });
    out.omissions.push(format!(
        "Terrain {id} streams full-detail tiles around the camera; distance LOD is not yet implemented"
    ));
    TerrainScene::build(field, out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn vertex(u: f32, v: f32) -> bri_content::interior::Vertex {
        bri_content::interior::Vertex {
            position: [0.0; 3],
            normal: [0.0, 1.0, 0.0],
            uv: [0.0; 2],
            lightmap_uv: [u / 256.0, v / 256.0],
        }
    }
    #[test]
    fn lightmap_footprints_sample_only_texels_centered_inside_them() {
        let sheet = SceneImage {
            label: "sheet".into(),
            width: 256,
            height: 256,
            rgba: vec![],
            srgb: false,
        };
        // Bedroom Dark floor: rows 9..23 are its own, row 24 is the next surface.
        let inset = lightmap_inset(&[vertex(9.0, 9.0), vertex(159.5, 24.5)], &sheet, true);
        let map = |a: usize, t: f32| (inset[a].0 + t / 256.0 * inset[a].1) * 256.0;
        assert!((map(1, 9.0) - 9.5).abs() < 1e-3);
        assert!((map(1, 24.5) - 23.5).abs() < 1e-3);
        assert!((map(0, 159.5) - 158.5).abs() < 1e-3);
        // Quarter-texel edges keep every texel whose center is inside.
        let inset = lightmap_inset(&[vertex(4.25, 0.0), vertex(8.75, 1.0)], &sheet, true);
        let map = |t: f32| (inset[0].0 + t / 256.0 * inset[0].1) * 256.0;
        assert!((map(4.25) - 4.5).abs() < 1e-3 && (map(8.75) - 8.5).abs() < 1e-3);
        // A sliver narrower than a texel samples one texel center.
        let inset = lightmap_inset(&[vertex(3.1, 0.0), vertex(3.4, 1.0)], &sheet, true);
        assert_eq!(inset[0], (3.5 / 256.0, 0.0));
        // Unlit surfaces keep their coordinates.
        assert_eq!(
            lightmap_inset(&[vertex(1.0, 1.0)], &sheet, false),
            [(0.0, 1.0); 2]
        );
    }
}
