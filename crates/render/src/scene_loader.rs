//! Loader for already converted native bundles. This module cannot read Torque
//! assets and never searches the original installation for missing resources.
use crate::{scene::*, terrain_scene::TerrainScene};
use anyhow::{Context, Result, ensure};
use bri_console::Clamp;
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
    pub repeated_water: Vec<crate::water_scene::WaterScene>,
    /// Index range of each static shape by scene node, so a smashed shape
    /// can stop drawing (`GpuScene::hide_indices`).
    pub shape_indices: BTreeMap<u32, std::ops::Range<u32>>,
    /// The models a package moves (doors), each its own scene about its
    /// own origin, by scene node: drawn instanced where the session's
    /// mover states put them rather than baked into the map.
    pub movers: Vec<(u32, SceneData)>,
    /// The map's scatter layers: one model each, repeated at many placements ([`bri_content::scatter`]).
    pub scatter: Vec<ScatterScene>,
    /// The map's backdrop, when it has one.
    pub backdrop: Option<BackdropScene>,
    /// The map's regions with their own fog, when it has them.
    pub regions: Option<bri_content::regions::RegionMap>,
    /// The map's decals and their pictures (in order), when it has them.
    pub decals: Option<crate::decals::LoadedDecals>,
    /// The map's triangles by place, for decals added while the game runs.
    pub receiver_index: std::sync::Arc<crate::decals::ReceiverIndex>,
    /// Set only by the modern loader, which never decodes illumination images.
    pub modern_lights: Option<Vec<crate::map_lighting::MapLight>>,
    /// Animated map lights: an index into `modern_lights` and how it
    /// changes over time (applied as its run-time tint).
    pub light_animations: Vec<(usize, bri_content::light_animation::LightAnimation)>,
    /// Spot lights: an index into `modern_lights` and its cone words
    /// (`scene::spot_words`), for `SceneRenderer::set_map_light_spots`.
    pub light_spots: Vec<(usize, [f32; 4])>,
    /// Plain lights past the shadowed budget (`map_lighting::MAX_LIGHTS`):
    /// they draw as unshadowed point lights with the same linear falloff, the
    /// nearest ones each frame (`SceneRenderer::update_lights`, colour w 1).
    pub overflow_lights: Vec<crate::map_lighting::MapLight>,
}

/// A map's backdrop ([`bri_content::backdrop`]): a model drawn before the map from `eye`, with only the viewer's rotation.
pub struct BackdropScene {
    pub scene: SceneData,
    pub eye: [f32; 3],
}

/// The backdrop `bundle` lists for `scene`, its model loaded as a shape about the origin.
fn load_backdrop(
    root: &Path,
    bundle: &Value,
    bindings: &[Value],
    scene: &Scene,
    legacy: bool,
) -> Result<Option<BackdropScene>> {
    let Some(record) = bundle
        .get("backdrops")
        .and_then(|s| s.get(&scene.id))
        .filter(|v| !v.is_null())
    else {
        return Ok(None);
    };
    let record: bri_content::backdrop::Record =
        serde_json::from_value(record.clone()).context("Reading the map's backdrop")?;
    record.validate()?;
    let mut alone = SceneData {
        id: format!("{}#backdrop", scene.id),
        name: "backdrop".into(),
        ..Default::default()
    };
    let at_origin = Node {
        name: "backdrop".into(),
        parent: None,
        kind: Kind::StaticModel,
        transform: Mat4::IDENTITY.to_cols_array(),
        asset: Some(record.asset.clone()),
        properties: BTreeMap::new(),
    };
    load_static_shape(
        root,
        bundle,
        bindings,
        &at_origin,
        &mut alone,
        &mut BTreeMap::new(),
        legacy,
    )
    .context("The map's backdrop")?;
    alone.validate()?;
    Ok(Some(BackdropScene {
        scene: alone,
        eye: record.eye,
    }))
}

/// The decals `bundle` lists for `scene` ([`bri_content::decals`]) with their pictures decoded.
fn load_decals(
    root: &Path,
    bundle: &Value,
    scene: &Scene,
    world: &SceneData,
    index: &crate::decals::ReceiverIndex,
) -> Result<Option<crate::decals::LoadedDecals>> {
    let Some(name) = bundle
        .get("decals")
        .and_then(|s| s.get(&scene.id))
        .and_then(Value::as_str)
    else {
        return Ok(None);
    };
    let set: bri_content::decals::DecalSet =
        serde_json::from_slice(&read(root, name)?).context("Reading the map's decals")?;
    set.validate()?;
    let mut pictures = Vec::with_capacity(set.decals.len());
    for decal in &set.decals {
        pictures.push(decode(&read(root, &decal.picture)?, &decal.picture, true)?);
    }
    let mut named = Vec::with_capacity(set.pictures.len());
    for picture in &set.pictures {
        named.push(decode(&read(root, &picture.file)?, &picture.file, true)?);
    }
    let receivers = set
        .decals
        .iter()
        .map(|decal| crate::decals::gather_indexed(world, index, decal))
        .collect();
    Ok(Some(crate::decals::LoadedDecals {
        set,
        pictures,
        named,
        receivers,
    }))
}

/// The regions `bundle` lists for `scene` ([`bri_content::regions`]).
fn load_regions(
    root: &Path,
    bundle: &Value,
    scene: &Scene,
) -> Result<Option<bri_content::regions::RegionMap>> {
    let Some(name) = bundle
        .get("regions")
        .and_then(|s| s.get(&scene.id))
        .and_then(Value::as_str)
    else {
        return Ok(None);
    };
    let map: bri_content::regions::RegionMap =
        serde_json::from_slice(&read(root, name)?).context("Reading the map's regions")?;
    map.validate()?;
    Ok(Some(map))
}

/// One scatter layer: the model about its own origin and where it is placed.
pub struct ScatterScene {
    pub name: String,
    pub scene: SceneData,
    pub instances: Arc<[bri_content::scatter::Instance]>,
    /// Near and far fade distances, in world units; None draws every instance at any distance.
    pub fade: Option<[f32; 2]>,
    pub order: bri_content::scatter::Order,
    pub modulate_2x: bool,
    pub detail: u8,
}

/// The scatter layers `bundle` lists for `scene`: each model loaded as a shape about the origin, with its placements read and checked.
fn load_scatter(
    root: &Path,
    bundle: &Value,
    bindings: &[Value],
    scene: &Scene,
    legacy: bool,
) -> Result<Vec<ScatterScene>> {
    let Some(records) = bundle.get("scatter").and_then(|s| s.get(&scene.id)) else {
        return Ok(Vec::new());
    };
    let records: Vec<bri_content::scatter::Record> =
        serde_json::from_value(records.clone()).context("Reading the map's scatter layers")?;
    ensure!(records.len() <= 64, "A map names at most 64 scatter layers");
    let mut out = Vec::with_capacity(records.len());
    for record in records {
        record.validate()?;
        let mut alone = SceneData {
            id: format!("{}#scatter-{}", scene.id, record.name),
            name: record.name.clone(),
            ..Default::default()
        };
        let at_origin = Node {
            name: record.name.clone(),
            parent: None,
            kind: Kind::StaticModel,
            transform: Mat4::IDENTITY.to_cols_array(),
            asset: Some(record.asset.clone()),
            properties: BTreeMap::new(),
        };
        load_static_shape(
            root,
            bundle,
            bindings,
            &at_origin,
            &mut alone,
            &mut BTreeMap::new(),
            legacy,
        )
        .with_context(|| format!("Scatter layer {}", record.name))?;
        alone.validate()?;
        let instances = bri_content::scatter::decode(&read(root, &record.file)?)
            .with_context(|| format!("Scatter layer {}'s placements", record.name))?;
        out.push(ScatterScene {
            name: record.name,
            scene: alone,
            instances: instances.into(),
            fade: record.fade,
            order: record.order,
            modulate_2x: record.modulate_2x,
            detail: record.detail,
        });
    }
    Ok(out)
}

pub(crate) fn file(root: &Path, name: &str) -> Result<PathBuf> {
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
pub(crate) fn read(root: &Path, name: &str) -> Result<Vec<u8>> {
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
pub(crate) fn decode(bytes: &[u8], label: &str, srgb: bool) -> Result<SceneImage> {
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
/// A cutout texture with soft edges (tree leaves): a large share of fully clear
/// and fully solid texels. Distinct from uniformly translucent glass.
fn soft_cutout(image: &SceneImage) -> bool {
    let (mut clear, mut solid, mut total) = (0usize, 0usize, 0usize);
    for p in image.rgba.chunks_exact(4) {
        total += 1;
        clear += usize::from(p[3] == 0);
        solid += usize::from(p[3] == 255);
    }
    total > 0 && clear * 10 >= total && solid * 10 >= total
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
/// A colour property of a scene node: three finite numbers, or the default.
/// Absent is the default by design; present and unreadable is a reported
/// fallback under the map the loader is reading (`with_owner`).
fn rgb(value: Option<&String>, default: [f32; 3]) -> [f32; 3] {
    let values: Vec<_> = value
        .into_iter()
        .flat_map(|s| s.split_whitespace())
        .take(3)
        .map(str::parse::<f32>)
        .collect();
    match values.as_slice() {
        [Ok(r), Ok(g), Ok(b)] if r.is_finite() && g.is_finite() && b.is_finite() => [*r, *g, *b],
        _ => {
            if let Some(value) = value {
                bri_console::fallback::report(
                    bri_console::fallback::kind::DEFAULTED_FIELD,
                    "",
                    &format!("colour {value:?}"),
                    "the default colour",
                );
            }
            default
        }
    }
}

#[cfg(test)]
mod rgb_fallback_tests {
    use super::*;

    #[test]
    fn a_colour_that_is_present_and_unreadable_is_a_reported_fallback_but_absent_is_not() {
        let scene = "fallback-test-colour";
        let good = "0.1 0.2 0.3".to_string();
        let bad = "red".to_string();
        bri_console::fallback::with_owner(scene, || {
            assert_eq!(rgb(Some(&good), [1.0; 3]), [0.1, 0.2, 0.3]);
            assert_eq!(rgb(None, [1.0; 3]), [1.0; 3]);
            assert!(
                bri_console::fallback::snapshot()
                    .iter()
                    .all(|e| e.addon != scene)
            );
            assert_eq!(rgb(Some(&bad), [1.0; 3]), [1.0; 3]);
        });
        let mine: Vec<_> = bri_console::fallback::snapshot()
            .into_iter()
            .filter(|e| e.addon == scene)
            .collect();
        assert_eq!(mine.len(), 1, "{mine:?}");
        assert_eq!(mine[0].kind, "defaulted_field");
        assert!(mine[0].item.contains("red"));
    }
}

/// `map_id` is the stable native mission ID from bundle.json, never a display
/// name or an original install path. Missing bound materials are hard errors.
pub fn load_map_bundle(root: &Path, map_id: &str) -> Result<MapScene> {
    load_bundle(root, map_id, true)
}
/// Dynamic loads material/geometry data and prepared source descriptors only.
/// Missing/stale descriptors use live sun/ambient, with an actionable diagnostic.
pub fn load_map_bundle_dynamic(root: &Path, map_id: &str) -> Result<MapScene> {
    load_bundle(root, map_id, false)
}
fn load_bundle(root: &Path, map_id: &str, legacy: bool) -> Result<MapScene> {
    bri_console::fallback::with_owner(map_id, || load_bundle_unscoped(root, map_id, legacy))
}
fn load_bundle_unscoped(root: &Path, map_id: &str, legacy: bool) -> Result<MapScene> {
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
    let mut scene: Scene = bri_content::strict::from_slice(&read(
        &root,
        record["file"]
            .as_str()
            .context("Native scene filename missing")?,
    )?)?;
    ensure!(
        scene.schema_version == 1 && scene.id == map_id,
        "Invalid native map scene identity/schema"
    );
    let mut interiors = BTreeMap::new();
    for node in scene
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, Kind::Interior))
    {
        let id = node
            .asset
            .as_deref()
            .context("Interior placement has no native asset")?;
        if !interiors.contains_key(id) {
            let interior: Interior = bri_content::strict::from_slice(&read(
                &root,
                bundle["assets"][id]
                    .as_str()
                    .context("Interior asset missing from bundle")?,
            )?)?;
            interior.validate()?;
            interiors.insert(id.to_owned(), interior);
        }
    }
    // The same lift `NativeMap::load` applies, so collision and view agree.
    let lift = scene.floor_lift(|id| interiors.get(id));
    scene.lift(lift);
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
        // The same angles the lighting bake uses; `direction` is stale.
        let angle = |key, default| {
            let read = sun
                .properties
                .get(key)
                .and_then(|v| v.trim().parse::<f32>().ok())
                .filter(|v| v.is_finite());
            if read.is_none()
                && let Some(value) = sun.properties.get(key)
            {
                bri_console::fallback::report(
                    bri_console::fallback::kind::DEFAULTED_FIELD,
                    &scene.id,
                    &format!("sun {key} = {value:?}"),
                    &format!("{default}"),
                );
            }
            read.unwrap_or(default)
        };
        let d = bri_content::scene::sun_direction(
            angle("azimuth", 0.0),
            angle("elevation", 35.0),
            f32::sin,
            f32::cos,
        );
        out.sun_direction = [d.x, d.z, -d.y]; // original Z-up to native Y-up
        out.sun_color = rgb(sun.properties.get("color"), out.sun_color);
        out.ambient = rgb(sun.properties.get("ambient"), out.ambient);
        // The share of a lightmap's static light that came from the sun and
        // sky; absent, none (it follows only the ambient).
        out.lightmap_sky_share = angle("lightmap_sky_share", 0.0).clamped(0.0, 1.0);
        // Light in spaces the sky cannot see; absent, they keep the ambient.
        out.enclosed_ambient = sun
            .properties
            .get("enclosed")
            .map(|v| rgb(Some(v), out.ambient));
    }
    if let Some(sky) = scene.nodes.iter().find(|n| matches!(n.kind, Kind::Sky)) {
        let c = rgb(sky.properties.get("skysolidcolor"), [0.05, 0.08, 0.12]);
        out.clear_color = [c[0], c[1], c[2], 1.0];
    }
    let mut cache = BTreeMap::new();
    let sky = bundle.get("environments").and_then(|e| e.get(&scene.id));
    if let Some(environment) = sky {
        let environment: bri_content::environment::Environment =
            serde_json::from_value(environment.clone())?;
        crate::environment_choices::append_sky(&root, &mut out, &environment)?;
    } else {
        out.omissions
            .push("Native environment binding missing: authored sky/cloud/fog not rendered".into());
    }
    // A map Add-On's own fog (its `maps` entry's `fog`) replaces its sky's;
    // with no sky, the fog colour is what lies behind the world.
    if let Some(fog) = record.get("fog") {
        let fog: bri_content::environment::Fog =
            serde_json::from_value(fog.clone()).context("Reading the map's fog")?;
        fog.validate().context("The map's fog")?;
        if sky.is_none() {
            out.clear_color = [fog.color[0], fog.color[1], fog.color[2], 1.0];
        }
        out.fog = fog;
    }
    let fields = terrain_fields(&root, &bundle, &scene, lift)?;
    let (water_bound, repeated_water) =
        load_waters(&root, &bundle, &scene, lift, &fields, &mut out, &mut cache)?;
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
                legacy,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let mut shape_indices = BTreeMap::new();
    let mut movers = Vec::new();
    for (node_index, node) in scene.nodes.iter().enumerate() {
        let first = out.indices.len() as u32;
        if node.properties.contains_key("mover") && node.asset.is_some() {
            // Its own scene, at the origin: the client places it each frame.
            let mut alone = SceneData {
                id: format!("{}#mover{node_index}", scene.id),
                name: node.name.clone(),
                ..Default::default()
            };
            let at_origin = Node {
                name: node.name.clone(),
                parent: None,
                kind: Kind::StaticModel,
                transform: Mat4::IDENTITY.to_cols_array(),
                asset: node.asset.clone(),
                properties: node.properties.clone(),
            };
            load_static_shape(
                &root,
                &bundle,
                bindings,
                &at_origin,
                &mut alone,
                &mut BTreeMap::new(),
                legacy,
            )?;
            movers.push((u32::try_from(node_index)?, alone));
            continue;
        }
        match node.kind {
            Kind::Interior=>{
                let interior = &interiors[node.asset.as_deref().context("Interior placement has no native asset")?];
                load_interior(&root,&bundle,bindings,&scene,node_index,node,interior,&mut out,&mut cache,legacy)?
            }
            Kind::StaticModel|Kind::DatablockModel if node.asset.is_some()=>{
                load_static_shape(&root,&bundle,bindings,node,&mut out,&mut cache,legacy)?;
                shape_indices.insert(u32::try_from(node_index)?, first..out.indices.len() as u32);
            }
            Kind::StaticModel=>anyhow::bail!("Static model {} has no native asset",node.name),
            Kind::Water if water_bound=>{},
            Kind::DatablockModel|Kind::Foliage|Kind::Water|Kind::Precipitation|Kind::Unadapted=>out.omissions.push(format!("Node {node_index} {:?} '{}' is retained in the bundle but not drawn by this static architecture pass",node.kind,node.name)),
            _=>{},
        }
    }
    // What the pass above left out: counted fallbacks (the loop's own arms
    // stay as they are).
    for node in &scene.nodes {
        if node.properties.contains_key("mover") && node.asset.is_none() {
            bri_console::fallback::report(
                bri_console::fallback::kind::DROPPED_ITEM,
                &scene.id,
                &node.name,
                "drawn as a fixed node: its mover has no asset",
            );
        }
        let undrawn = match node.kind {
            Kind::DatablockModel => node.asset.is_none(),
            Kind::Water => !water_bound,
            Kind::Foliage | Kind::Precipitation | Kind::Unadapted => true,
            _ => false,
        };
        if undrawn {
            bri_console::fallback::report(
                bri_console::fallback::kind::DROPPED_ITEM,
                &scene.id,
                &format!("{:?} '{}'", node.kind, node.name),
                "not drawn (retained in the bundle only)",
            );
        }
    }
    out.omissions.push(
        "Storm transitions/fog volumes and texture mip/anisotropic filtering remain incomplete"
            .into(),
    );
    out.omissions.push("Translucent geometry sorts by mesh-batch center; intersecting translucent surfaces need finer sorting".into());
    // A map Add-On's own lights (`Kind::Light`) join any recovered ones.
    let authored = scene.lights()?;
    let mut light_animations = Vec::new();
    let mut light_spots = Vec::new();
    let mut overflow_lights = Vec::new();
    let modern_lights = if legacy {
        None
    } else {
        let prepared = root.join(crate::lighting_parameters::FILE).is_file();
        let mut lights = if authored.is_empty() || prepared {
            let result =
                crate::lighting_parameters::Parameters::read(&root).and_then(|p| p.lights(map_id));
            result.unwrap_or_else(|e| {
                let warning = format!("Dynamic uses live sun/ambient only: {e:#}. Prepare source lights with prepare_lighting {}",root.display());
                eprintln!("{warning}"); out.omissions.push(warning); Vec::new()
            })
        } else {
            Vec::new()
        };
        for light in &authored {
            if !crate::lighting_parameters::valid_geometry(light.position, light.inner, light.outer)
            {
                out.omissions.push(format!(
                    "Map light at node {} left out: its falloff is too small to draw",
                    light.node
                ));
                continue;
            }
            if lights.len() >= crate::map_lighting::MAX_LIGHTS {
                // Past the shadowed budget only plain lights can draw: a cone
                // or an animation needs a shaded slot.
                if light.spot.is_some() || light.animation.is_some() {
                    out.omissions.push(format!(
                        "Map light at node {} left out: a map shades at most {} lights, and only plain lights draw past them",
                        light.node,
                        crate::map_lighting::MAX_LIGHTS
                    ));
                } else {
                    overflow_lights.push(crate::map_lighting::MapLight {
                        position: light.position,
                        color: light.color,
                        inner: light.inner,
                        outer: light.outer,
                        channel: None,
                    });
                }
                continue;
            }
            if let Some(spot) = light.spot {
                light_spots.push((
                    lights.len(),
                    crate::scene::spot_words(Vec3::from(spot.direction), spot.inner, spot.outer),
                ));
            }
            if let Some(animation) = light.animation {
                light_animations.push((lights.len(), animation));
            }
            lights.push(crate::map_lighting::MapLight {
                position: light.position,
                color: light.color,
                inner: light.inner,
                outer: light.outer,
                channel: None,
            });
        }
        Some(lights)
    };
    out.validate()?;
    let scatter = load_scatter(&root, &bundle, bindings, &scene, legacy)?;
    let backdrop = load_backdrop(&root, &bundle, bindings, &scene, legacy)?;
    let regions = load_regions(&root, &bundle, &scene)?;
    let receiver_index = std::sync::Arc::new(crate::decals::ReceiverIndex::build(&out));
    let decals = load_decals(&root, &bundle, &scene, &out, &receiver_index)?;
    Ok(MapScene {
        scene: out,
        terrain,
        repeated_water,
        shape_indices,
        movers,
        scatter,
        backdrop,
        regions,
        decals,
        receiver_index,
        modern_lights,
        light_animations,
        light_spots,
        overflow_lights,
    })
}

fn terrain_fields(
    root: &Path,
    bundle: &Value,
    scene: &Scene,
    lift: f32,
) -> Result<Vec<Arc<TerrainField>>> {
    let mut instances: Vec<TerrainInstance> = match bundle
        .get("terrains")
        .context("Map bundle has no converted terrain instances")?
        .get(&scene.id)
    {
        Some(value) => {
            serde_json::from_value(value.clone()).context("Invalid native terrain instances")?
        }
        None => Vec::new(),
    };
    for instance in &mut instances {
        instance.origin[1] += lift;
    }
    bri_content::terrain_field::map_fields(scene, instances, |id| {
        Ok(bri_content::strict::from_slice::<Terrain>(&read(
            root,
            bundle["assets"][id]
                .as_str()
                .context("Terrain asset missing from bundle")?,
        )?)?)
    })
}

#[allow(clippy::too_many_arguments)]
fn load_waters(
    root: &Path,
    bundle: &Value,
    scene: &Scene,
    lift: f32,
    fields: &[Arc<TerrainField>],
    out: &mut SceneData,
    cache: &mut BTreeMap<(String, bool), usize>,
) -> Result<(bool, Vec<crate::water_scene::WaterScene>)> {
    let Some(records) = bundle.get("waters").and_then(|w| w.get(&scene.id)) else {
        return Ok((false, Vec::new()));
    };
    let mut waters: Vec<bri_content::water::Water> = serde_json::from_value(records.clone())?;
    for water in &mut waters {
        water.min[1] += lift;
        water.max[1] += lift;
    }
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
    let mut repeated = Vec::new();
    for water in &waters {
        let mut local = SceneData::default();
        let mut local_cache = BTreeMap::new();
        let (dest, images) = if water.repeat_period.is_some() {
            (&mut local, &mut local_cache)
        } else {
            (&mut *out, &mut *cache)
        };
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
                textures[slot] = texture(root, &image.file, true, dest, images)?;
                ensure!(
                    dest.images[textures[slot]].width == image.width
                        && dest.images[textures[slot]].height == image.height,
                    "Water image dimensions changed"
                );
            }
        }
        let specular = water_specular(&scene.nodes[water.node].properties);
        if water.repeat_period.is_some() {
            out.omissions
                .extend(water.warnings.iter().map(|w| format!("{}: {w}", water.id)));
            repeated.push(crate::water_scene::WaterScene::new(
                water.clone(),
                local,
                textures,
                specular,
                fields.to_vec(),
            )?);
        } else {
            crate::water_scene::append(
                out,
                water,
                textures,
                specular,
                !fields.is_empty(),
                |x, z| {
                    fields
                        .iter()
                        .filter_map(|field| field.height(x, z))
                        .max_by(f32::total_cmp)
                },
            )?;
        }
    }
    Ok((true, repeated))
}

/// A WaterBlock's authored `specularColor` and `specularPower`, or the
/// block defaults (white, and `mSpecPower` 6 from the v20 constructor).
fn water_specular(node: &BTreeMap<String, String>) -> ([f32; 4], f32) {
    let report = |key: &str, value: &str, used: &str| {
        bri_console::fallback::report(
            bri_console::fallback::kind::DEFAULTED_FIELD,
            "",
            &format!("water {key} = {value:?}"),
            used,
        );
    };
    let color = node.get("specularcolor").and_then(|c| {
        let read = (|| {
            let v: Vec<f32> = c
                .split_whitespace()
                .map(|v| v.parse().ok())
                .collect::<Option<_>>()?;
            (v.len() == 4 && v.iter().all(|v| v.is_finite())).then(|| [v[0], v[1], v[2], v[3]])
        })();
        if read.is_none() {
            report("specularcolor", c, "white");
        }
        read
    });
    let power = node.get("specularpower").and_then(|p| {
        let read = p.parse::<f32>().ok().filter(|p| p.is_finite() && *p >= 0.0);
        if read.is_none() {
            report("specularpower", p, "6");
        }
        read
    });
    (color.unwrap_or([1.0; 4]), power.unwrap_or(6.0))
}

fn load_static_shape(
    root: &Path,
    bundle: &Value,
    bindings: &[Value],
    node: &Node,
    out: &mut SceneData,
    cache: &mut BTreeMap<(String, bool), usize>,
    legacy: bool,
) -> Result<()> {
    let id = node
        .asset
        .as_deref()
        .context("Static model has no native asset")?;
    let shape: bri_content::shape::Shape = bri_content::strict::from_slice(&read(
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
    let mut soft_edges = BTreeMap::new();
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
        // Dynamic has no baked lighting to supply the authored unlit face.
        // Keep the compatibility path's existing lightmap behavior unchanged.
        if authored.unlit && !legacy {
            material.kind = MaterialKind::Unlit;
        }
        // The full physically based look on an Add-On map: its maps are the
        // textures bound to the materials it names.
        let physical = authored
            .surface
            .as_ref()
            .filter(|_| !authored.unlit && !legacy);
        if let Some(surface) = physical {
            let mut maps = [0; 7];
            for (k, map) in crate::scene::SurfaceMap::ALL.into_iter().enumerate() {
                maps[k] = match map.of(surface) {
                    Some(m) => {
                        let file = bindings
                            .iter()
                            .find(|b| {
                                b["asset"].as_str() == Some(id)
                                    && b["shape_material"].as_u64() == Some(m as u64)
                                    && b["skin"].as_str().unwrap_or("") == skin
                            })
                            .and_then(|b| b["texture"].as_str())
                            .with_context(|| format!("Missing static surface map {id}/{m}"))?;
                        texture(root, file, map.srgb(), out, cache)?
                    }
                    None => {
                        out.images.push(map.stand_in());
                        out.images.len() - 1
                    }
                };
            }
            material = Material::physical(material.name.clone(), diffuse, maps, surface);
        }
        // A baked lightmap (second UV set) on a surface: Dynamic adds the
        // lights to it in place of live sun and ambient; Classic and Unified
        // draw it as an interior lightmap (without its strength).
        if let Some(surface) = authored
            .surface
            .as_ref()
            .filter(|s| s.lightmap.is_some() && !authored.unlit)
        {
            let index = surface.lightmap.expect("filtered");
            let file = bindings
                .iter()
                .find(|b| {
                    b["asset"].as_str() == Some(id)
                        && b["shape_material"].as_u64() == Some(index as u64)
                        && b["skin"].as_str().unwrap_or("") == skin
                })
                .and_then(|b| b["texture"].as_str())
                .with_context(|| format!("Missing static lightmap {id}/{index}"))?;
            let lightmap = texture(root, file, false, out, cache)?;
            if legacy {
                material = Material::surface(material.name.clone(), diffuse, lightmap);
            } else {
                material.images[8] = lightmap;
                material.baked_lightmap = Some(surface.lightmap_strength);
                material.baked_exact = surface.lightmap_exact;
            }
        }
        // Bare metal on an Add-On map: its texture tints the reflectance and
        // a flat detail leaves the authored roughness as it is.
        if let Some(metal) = authored
            .metal
            .as_ref()
            .filter(|_| !authored.unlit && !legacy && physical.is_none())
        {
            material.kind = MaterialKind::Metal;
            material.images[1] = out.images.len();
            out.images.push(SceneImage {
                label: "flat metal detail".into(),
                width: 1,
                height: 1,
                rgba: vec![128, 255, 128, 128],
                srgb: false,
            });
            material.parameters = Some([
                [metal.roughness, 1.0, 0.0, 0.0],
                [metal.color[0], metal.color[1], metal.color[2], 0.0],
                [0.0; 4],
                [0.0; 4],
            ]);
        }
        if authored.vertex_light && authored.layers.is_none() {
            material.kind = MaterialKind::VertexLight;
        }
        // A terrain's layers: up to four textures weighted by one map's channels (its materials name them), lit by the vertex
        // light only. Layers the material does not have repeat the first and carry no weight.
        if let Some(blend) = authored.layers.as_ref() {
            let image_of =
                |index: usize, srgb: bool, out: &mut SceneData, cache: &mut _| -> Result<usize> {
                    let file = bindings
                        .iter()
                        .find(|b| {
                            b["asset"].as_str() == Some(id)
                                && b["shape_material"].as_u64() == Some(index as u64)
                                && b["skin"].as_str().unwrap_or("") == skin
                        })
                        .and_then(|b| b["texture"].as_str())
                        .with_context(|| format!("Missing layer texture {id}/{index}"))?;
                    texture(root, file, srgb, out, cache)
                };
            ensure!(
                (1..=4).contains(&blend.layers.len())
                    && blend.transforms.len() == blend.layers.len(),
                "Static material {id}/{} needs one to four layers with a transform each",
                authored.name
            );
            let first = image_of(blend.layers[0], true, out, cache)?;
            material.kind = MaterialKind::LayerBlend;
            material.images[0] = first;
            let mut transforms = [[1.0, 0.0, 0.0, 1.0, 0.0, 0.0]; 4];
            for (i, layer) in blend.layers.iter().enumerate() {
                material.images[i] = if i == 0 {
                    first
                } else {
                    image_of(*layer, true, out, cache)?
                };
                transforms[i] = blend.transforms[i];
            }
            for i in blend.layers.len()..4 {
                material.images[i] = first;
            }
            material.images[9] = image_of(blend.weights, false, out, cache)?;
            material.layer_transforms = Some(transforms);
        }
        // Torque culls DTS back faces and blends only Translucent materials;
        // opaque ones ignore texture alpha entirely (TSMesh::initMaterials,
        // TSMesh::setMaterial). Palm frond stems sit on alpha-0 texels. A
        // cutout (glTF MASK) is the exception: its cutoff is the material's
        // own alpha test, applied by the shared shape-material mapping.
        material.ignore_texture_alpha = authored.blend == "opaque";
        crate::shape_material::apply_with(
            &mut material,
            authored,
            &format!("static material {id}"),
            true,
            |blend| {
                Ok(match blend {
                    "opaque" => AlphaMode::Opaque,
                    // A layer pass blended over the one before (a terrain's alpha-map layers) blends by its weights, whatever its stand-in colour image says.
                    "alpha" if authored.layers.is_some() => AlphaMode::Blend,
                    "alpha" => alpha(&out.images[diffuse]),
                    // Unreal Engine 2's frame-buffer blends (docs/modding/materials.md):
                    // true modulate, modulate (2x), darken, translucent/brighten
                    // smooth, alpha modulate and invisible, drawn unlit.
                    "multiply" => AlphaMode::Multiply,
                    "modulate" => AlphaMode::Frame(FrameBlend::Modulate),
                    "darken" => AlphaMode::Frame(FrameBlend::Darken),
                    "screen" => AlphaMode::Frame(FrameBlend::Screen),
                    "premultiplied" => AlphaMode::Frame(FrameBlend::Premultiplied),
                    "invisible" => AlphaMode::Frame(FrameBlend::Invisible),
                    "true_additive" => AlphaMode::Frame(FrameBlend::TrueAdditive),
                    // Torque's additive DTS materials (glass, glows).
                    _ => AlphaMode::Additive,
                })
            },
        )?;
        // One blended batch holds a whole crown of leaves in mesh order, so
        // back leaves painted over front ones. Solid texels now write depth in
        // a cutout pass; a blended twin then adds only the soft edges.
        let unbound_effects =
            authored.environment || authored.detail_map.is_some() || authored.bump_map.is_some();
        if unbound_effects {
            bri_console::fallback::report(
                bri_console::fallback::kind::DROPPED_ITEM,
                id,
                &format!("material {} reflection/detail/bump", authored.name),
                "drawn without them (not bound)",
            );
        }
        if material.alpha == AlphaMode::Blend && soft_cutout(&out.images[diffuse]) {
            let mut edges = material.clone();
            edges.name = format!("{}/soft-edges", material.name);
            material.alpha = AlphaMode::Mask(0.5);
            soft_edges.insert(out.materials.len(), out.materials.len() + 1);
            materials.push(out.materials.len());
            material.uv_motion = authored.uv_motion;
            material.uv_chain.clone_from(&authored.uv_chain);
            out.materials.push(material);
            out.materials.push(edges);
            continue;
        }
        if unbound_effects {
            out.omissions.push(format!(
                "Static material {id}/{} has unbound reflection/detail/bump effects",
                authored.name
            ));
        }
        materials.push(out.materials.len());
        material.uv_motion = authored.uv_motion;
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
    let first_batch = out.batches.len();
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
    for i in first_batch..out.batches.len() {
        if let Some(&edges) = soft_edges.get(&out.batches[i].material) {
            out.batches.push(MeshBatch {
                material: edges,
                ..out.batches[i].clone()
            });
        }
    }
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
    interior: &Interior,
    out: &mut SceneData,
    cache: &mut BTreeMap<(String, bool), usize>,
    legacy: bool,
) -> Result<()> {
    let id = node
        .asset
        .as_deref()
        .context("Interior placement has no native asset")?;
    let detail = &interior.details[0];
    let placement = transform(node)?;
    let normal_transform = placement.inverse().transpose();
    let mirrored = placement.determinant() < 0.0;
    let mut lightmaps = vec![(0, 0); detail.lightmaps.len()];
    if legacy {
        let baked = bundle["lighting"][&scene.id]["interiors"]
            .as_array()
            .context("Map is missing native interior mission-lighting bindings")?;
        // Each lightmap splits into its static light and baked sun visibility
        // (`map_lighting::decompose_sheet`), so the sun can be shaded live.
        let sun = (bundle["lighting"][&scene.id]["status"].as_str() == Some("baked")).then(|| {
            crate::map_lighting::BakeSun {
                direction: Vec3::from(out.sun_direction).normalize_or_zero(),
                color: Vec3::from(out.sun_color).clamp(Vec3::ZERO, Vec3::ONE),
                ambient: Vec3::from(out.ambient).clamp(Vec3::ZERO, Vec3::ONE),
            }
        });
        let mut sheets = vec![];
        for (slot, lm) in detail.lightmaps.iter().enumerate() {
            let replacement = baked.iter().find(|r| {
                r["node"].as_u64() == Some(node_index as u64)
                    && r["detail"].as_u64() == Some(0)
                    && r["slot"].as_u64() == Some(slot as u64)
            });
            let base = decode(&lm.png, &format!("{id}/base-lightmap-{slot}"), false)?;
            let lightmap = match replacement {
                Some(record) => texture(
                    root,
                    record["file"]
                        .as_str()
                        .context("Baked interior lightmap filename missing")?,
                    false,
                    out,
                    cache,
                )?,
                None => {
                    bri_console::fallback::report(
                        bri_console::fallback::kind::MISSING_TEXTURE,
                        id,
                        &format!("interior node {node_index} lightmap {slot}"),
                        "the original embedded lightmap (composed replacement absent)",
                    );
                    out.omissions.push(format!("Interior node {node_index} lightmap {slot} uses the original embedded lightmap: composed mission replacement absent"));
                    let index = out.images.len();
                    out.images.push(base.clone());
                    index
                }
            };
            sheets.push((
                base,
                replacement.is_some().then_some(lightmap),
                lightmap,
                Vec::<crate::map_lighting::SheetSurface>::new(),
            ));
        }
        for surface in &detail.surfaces {
            let Some(slot) = surface.lightmap else {
                continue;
            };
            let Some(first) = surface.vertices.first() else {
                continue;
            };
            let world = |v: &bri_content::interior::Vertex| {
                (
                    placement
                        .transform_point3(Vec3::from(v.position))
                        .to_array(),
                    v.lightmap_uv,
                )
            };
            sheets[slot].3.push(crate::map_lighting::SheetSurface {
                triangles: surface
                    .triangles
                    .iter()
                    .map(|t| t.map(|i| world(&surface.vertices[i as usize])))
                    .collect(),
                normal: normal_transform
                    .transform_vector3(Vec3::from(first.normal))
                    .normalize_or_zero(),
                outside: surface.flags & crate::map_lighting::OUTSIDE_VISIBLE != 0,
            });
        }
        // Per slot: the lightmap drawn (mission or original) and its decomposition.
        lightmaps.clear();
        for (base, mission, lightmap, surfaces) in sheets {
            let index = out.images.len();
            let decomposed = crate::map_lighting::decompose_sheet(
                &base,
                mission.map(|m| &out.images[m]),
                &surfaces,
                sun,
            );
            out.images.push(decomposed);
            out.lightmap_bases.push((lightmap, Arc::new(base)));
            lightmaps.push((lightmap, index));
        }
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
    let mut groups: BTreeMap<(usize, usize, usize), Vec<u32>> = BTreeMap::new();
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
        let (lightmap, decomposed) = surface.lightmap.map_or((0, 0), |i| lightmaps[i]);
        let group = groups.entry((diffuse, lightmap, decomposed)).or_default();
        let base = u32::try_from(out.vertices.len()).context("Too many scene vertices")?;
        let inset = lightmap_inset(
            &surface.vertices,
            &out.images[lightmap],
            legacy && surface.lightmap.is_some(),
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
                fx: [0.; 4],
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
    for ((diffuse, lightmap, decomposed), indices) in groups {
        let material = out.materials.len();
        let mut m = Material::surface(format!("{id}/{diffuse}/{lightmap}"), diffuse, lightmap);
        m.alpha = alpha(&out.images[diffuse]);
        // Surfaces without a lightmap draw from the white image, fully lit.
        if decomposed != 0 {
            m.images[9] = decomposed;
            m.parameters = Some(DECOMPOSED_LIGHTMAP);
        }
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
    legacy: bool,
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
    if legacy {
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
    }
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
        uv_motion: None,
        uv_chain: Vec::new(),
        layer_transforms: None,
        clamp_nearest: false,
        temp_brick_flash: false,
        ignore_texture_alpha: false,
        untinted: false,
        baked_lightmap: None,
        baked_exact: false,
        tint_mask: false,
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

    /// A map static model with this authored material, loaded the way a map
    /// bundle loads it.
    fn load_leaf(authored: bri_content::shape::Material) -> Result<SceneData> {
        use bri_content::testing::{plain, png, rigid_shape, write_file};
        let dir = tempfile::tempdir()?;
        let root = dir.path().canonicalize()?;
        let shape = rigid_shape(
            "leaf",
            &[("root", None, [0.0; 3])],
            &[(0, [0.0; 3], [1.0; 3], plain(0))],
            vec![authored],
        );
        write_file(&root, "leaf.json", &serde_json::to_vec(&shape)?)?;
        // Left half a clear black texel, right half solid green: the shape of a
        // leaf card, whose clear texels hold black RGB.
        let image = png(2, 1, |x, _| {
            if x == 0 {
                [0, 0, 0, 0]
            } else {
                [0, 200, 0, 255]
            }
        })?;
        write_file(&root, "leaf.png", &image)?;
        let bundle = serde_json::json!({ "assets": { "leaf": "leaf.json" } });
        let bindings = vec![serde_json::json!(
            { "asset": "leaf", "shape_material": 0, "skin": "", "texture": "leaf.png" }
        )];
        let node = Node {
            name: "tree".into(),
            parent: None,
            kind: bri_content::scene::Kind::StaticModel,
            transform: Mat4::IDENTITY.to_cols_array(),
            asset: Some("leaf".into()),
            properties: BTreeMap::new(),
        };
        let mut out = SceneData::default();
        load_static_shape(
            &root,
            &bundle,
            &bindings,
            &node,
            &mut out,
            &mut BTreeMap::new(),
            false,
        )?;
        Ok(out)
    }
    fn leaf_material(blend: &str, cutoff: Option<f32>) -> bri_content::shape::Material {
        let mut m = bri_content::testing::material("leaf", blend);
        m.alpha_cutoff = cutoff;
        m
    }
    /// An AlphaRef 120 FinalBlend arrives as glTF MASK: blend "opaque" plus a
    /// cutoff. The loader drew it opaque and ignored texture alpha, so the clear
    /// texels showed their black RGB as solid squares.
    #[test]
    fn a_cutout_map_model_keeps_its_cutoff_and_texture_alpha() {
        let out = load_leaf(leaf_material("opaque", Some(120.0 / 255.0))).unwrap();
        let leaf = out
            .materials
            .iter()
            .find(|m| m.name.ends_with("leaf"))
            .unwrap();
        assert_eq!(leaf.alpha, AlphaMode::Mask(120.0 / 255.0));
        assert!(!leaf.ignore_texture_alpha);
    }
    #[test]
    fn a_plain_opaque_map_model_still_ignores_texture_alpha() {
        let out = load_leaf(leaf_material("opaque", None)).unwrap();
        let leaf = out
            .materials
            .iter()
            .find(|m| m.name.ends_with("leaf"))
            .unwrap();
        assert_eq!(leaf.alpha, AlphaMode::Opaque);
        assert!(leaf.ignore_texture_alpha);
    }
    /// A bundle's scatter table loads each layer's model about its own origin and reads its placements.
    #[test]
    fn a_scatter_layer_loads_its_model_and_its_placements() -> Result<()> {
        use bri_content::scatter::{Instance, Record, encode};
        use bri_content::testing::{plain, png, rigid_shape, write_file};
        let dir = tempfile::tempdir()?;
        let root = dir.path().canonicalize()?;
        let shape = rigid_shape(
            "grass",
            &[("root", None, [0.0; 3])],
            &[(0, [0.0; 3], [1.0; 3], plain(0))],
            vec![bri_content::testing::material("grass", "opaque")],
        );
        write_file(&root, "grass.json", &serde_json::to_vec(&shape)?)?;
        write_file(&root, "grass.png", &png(1, 1, |_, _| [0, 200, 0, 255])?)?;
        let placed = Instance {
            position: [4.0, 0.0, 9.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
            tint: [255; 4],
        };
        write_file(&root, "grass.scatter", &encode(&[placed, placed])?)?;
        let record = Record {
            name: "grass".into(),
            asset: "grass".into(),
            file: "grass.scatter".into(),
            fade: Some([10.0, 40.0]),
            order: bri_content::scatter::Order::FarToNear,
            modulate_2x: true,
            detail: 1,
        };
        let bundle = serde_json::json!({
            "assets": { "grass": "grass.json" },
            "scatter": { "m": [record] },
        });
        let bindings = vec![serde_json::json!(
            { "asset": "grass", "shape_material": 0, "skin": "", "texture": "grass.png" }
        )];
        let scene = Scene {
            schema_version: 1,
            id: "m".into(),
            name: "m".into(),
            nodes: Vec::new(),
            pending_scripts: Vec::new(),
        };
        let layers = load_scatter(&root, &bundle, &bindings, &scene, false)?;
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].instances.len(), 2);
        assert_eq!(layers[0].fade, Some([10.0, 40.0]));
        assert_eq!(layers[0].order, bri_content::scatter::Order::FarToNear);
        assert!(layers[0].modulate_2x);
        assert_eq!(layers[0].detail, 1);
        assert!(!layers[0].scene.indices.is_empty(), "the model is drawn");
        // A map with no table has no layers; a damaged placements file is refused.
        assert!(load_scatter(&root, &serde_json::json!({}), &bindings, &scene, false)?.is_empty());
        write_file(&root, "grass.scatter", b"nonsense")?;
        assert!(load_scatter(&root, &bundle, &bindings, &scene, false).is_err());
        Ok(())
    }
    /// A bundle's backdrop loads its model about its own origin, with the eye the table names.
    #[test]
    fn a_backdrop_loads_its_model_and_its_eye() -> Result<()> {
        use bri_content::testing::{plain, png, rigid_shape, write_file};
        let dir = tempfile::tempdir()?;
        let root = dir.path().canonicalize()?;
        let shape = rigid_shape(
            "sky",
            &[("root", None, [0.0; 3])],
            &[(0, [0.0; 3], [1.0; 3], plain(0))],
            vec![bri_content::testing::material("sky", "opaque")],
        );
        write_file(&root, "sky.json", &serde_json::to_vec(&shape)?)?;
        write_file(&root, "sky.png", &png(1, 1, |_, _| [90, 140, 220, 255])?)?;
        let bundle = serde_json::json!({
            "assets": { "sky": "sky.json" },
            "backdrops": { "m": { "asset": "sky", "eye": [1.0, 2.0, 3.0] } },
        });
        let bindings = vec![serde_json::json!(
            { "asset": "sky", "shape_material": 0, "skin": "", "texture": "sky.png" }
        )];
        let scene = Scene {
            schema_version: 1,
            id: "m".into(),
            name: "m".into(),
            nodes: Vec::new(),
            pending_scripts: Vec::new(),
        };
        let backdrop = load_backdrop(&root, &bundle, &bindings, &scene, false)?.unwrap();
        assert_eq!(backdrop.eye, [1.0, 2.0, 3.0]);
        assert!(!backdrop.scene.indices.is_empty(), "the model is drawn");
        let none = serde_json::json!({ "backdrops": { "m": null } });
        assert!(load_backdrop(&root, &none, &bindings, &scene, false)?.is_none());
        assert!(load_backdrop(&root, &serde_json::json!({}), &bindings, &scene, false)?.is_none());
        Ok(())
    }
    #[test]
    fn a_map_model_blend_the_loader_cannot_honour_is_refused() {
        // The Unreal Engine 2 frame-buffer blends are honoured for map models; a name
        // nothing maps is still refused.
        assert!(load_leaf(leaf_material("multiply", None)).is_ok());
        let error = load_leaf(leaf_material("sepia", None)).unwrap_err();
        assert!(error.to_string().contains("sepia"), "{error}");
    }
}
