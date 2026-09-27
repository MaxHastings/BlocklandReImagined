//! Native-only content index and lazy session loading. No Torque reader or
//! original installation is needed by this module.
use anyhow::{Context, Result, ensure};
use bri_content::{
    brick::Catalog,
    collision::CollisionLibrary,
    effects::Library,
    scene::{Kind, Scene},
};
use bri_sim::{definitions::Definitions, map::NativeMap, simulation::Simulation};
use bri_ui::{
    api::{BrickInfo, Choice, DatablockMenus, IconRef, MapInfo, PaintDivision},
    pack::Pack,
    schema::{PACK_SCHEMA_VERSION, UiPack},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    rc::Rc,
};

const INDEX_LIMIT: u64 = 32 * 1024 * 1024;
const GEOMETRY_LIMIT: u64 = 256 * 1024 * 1024;

/// Native map loading coverage; each map still needs full gameplay/fidelity acceptance.
pub const LOADABLE_MAPS: &[&str] = &[
    "v20/add-ons/map_bedroom/bedroom.mis",
    "v20/add-ons/map_kitchen/kitchen.mis",
    "v20/add-ons/map_slopes/slopes.mis",
    "v20/add-ons/map_slate/slate.mis",
    "v20/add-ons/map_bedroomdark/bedroomdark.mis",
    "v20/add-ons/map_construct/construct.mis",
    "v20/add-ons/map_destruct/destruct.mis",
    "v20/add-ons/map_halloween_slate/halloweenslate.mis",
    "v20/add-ons/map_kitchendark/kitchendark.mis",
    "v20/add-ons/map_skylands/skylands.mis",
    "v20/add-ons/map_slate_desert/slatedesert.mis",
    "v20/add-ons/map_slate_sea_revised/slatesearevised.mis",
    "v20/add-ons/map_slate_storm_revised/slatestormrevised.mis",
    "v20/add-ons/map_tutorial/tutorial.mis",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ContentConfig {
    pub schema_version: u32,
    pub map_bundle: String,
    pub brick_catalog: String,
    pub geometry: String,
    pub effects: String,
    pub worlds: String,
    pub ui_pack: String,
    pub brick_materials: String,
    pub avatar: String,
    pub effects_runtime: String,
    pub audio: String,
    pub weather: String,
    pub foliage: String,
    pub weapons: String,
    pub item_presentation: String,
    pub vehicles: String,
    pub events: String,
}
impl Default for ContentConfig {
    fn default() -> Self {
        Self {
            schema_version: 1,
            map_bundle: "map-bundle-015".into(),
            brick_catalog: "stock-catalog-004".into(),
            geometry: "maps-pass-003".into(),
            effects: "effects-pass-004".into(),
            worlds: "worlds-pass-005".into(),
            ui_pack: "ui-pack-003".into(),
            brick_materials: "brick-materials-001".into(),
            avatar: "avatar-pack-001".into(),
            effects_runtime: "effects-runtime-pack-002".into(),
            audio: "audio-pack-001".into(),
            weather: "weather-pack-001".into(),
            foliage: "foliage-pack-001".into(),
            weapons: "weapons-pack-003".into(),
            item_presentation: "item-presentation-pack-003".into(),
            vehicles: "vehicles-pack-007".into(),
            events: "events-pack-002".into(),
        }
    }
}

/// Send-friendly handles for a background hosting/loading worker. These paths
/// contain generated native content only; Rc<Pack> never crosses threads.
#[derive(Debug, Clone)]
pub struct ContentPaths {
    pub root: PathBuf,
    pub map_bundle: PathBuf,
    pub brick_catalog: PathBuf,
    pub geometry: PathBuf,
    pub effects: PathBuf,
    pub worlds: PathBuf,
    pub ui_pack: PathBuf,
    pub brick_materials: PathBuf,
    pub avatar: PathBuf,
    pub effects_runtime: PathBuf,
    pub audio: PathBuf,
    pub weather: PathBuf,
    pub foliage: PathBuf,
    pub weapons: PathBuf,
    pub item_presentation: PathBuf,
    pub vehicles: PathBuf,
    pub events: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorldEntry {
    pub id: String,
    pub name: String,
    pub map_id: String,
    /// Validated package-local native filename, not an original BLS path.
    pub file: String,
    pub brick_count: usize,
    pub loadable: bool,
}

pub struct LoadedMap {
    pub simulation: Simulation,
    pub scene: Scene,
    pub spawn: [f32; 3],
    pub spawn_points: Vec<glam::Vec3>,
    pub pending_objects: Vec<String>,
    /// Shared-shape copies for the client-side query mirror; server authority
    /// remains in simulation. No original assets are read by either consumer.
    pub query_colliders: Vec<rapier3d::prelude::ColliderBuilder>,
    /// Exact terrain placements for client collision streaming and queries.
    pub terrain: Vec<std::sync::Arc<bri_content::terrain_field::TerrainField>>,
}

pub struct ClientContent {
    pub paths: ContentPaths,
    pub ui_pack: Rc<Pack>,
    pub maps: Vec<MapInfo>,
    pub bricks: Vec<BrickInfo>,
    pub catalog: Catalog,
    pub paint: Vec<PaintDivision>,
    pub datablocks: DatablockMenus,
    pub worlds: Vec<WorldEntry>,
    pub effects: Library,
    pub weapons: bri_net::content_identity::WeaponContent,
    pub item_physics: bri_net::content_identity::ItemPhysicsContent,
    pub vehicles: bri_vehicles::Pack,
    /// Music-brick loops: (sound id, display name).
    pub music: Vec<(String, String)>,
    /// Wrench event catalog and the sounds `playSound` may use.
    pub events: bri_events::Catalog,
    pub event_sounds: Vec<(String, String)>,
    pub warnings: Vec<String>,
}

#[derive(Deserialize)]
struct Bundle {
    schema_version: u32,
    maps: Vec<MapEntry>,
    assets: BTreeMap<String, String>,
    textures: BTreeMap<String, String>,
    #[serde(default)]
    bindings: Vec<TextureBinding>,
    #[serde(default)]
    lighting: BTreeMap<String, Lighting>,
    #[serde(default)]
    unresolved_textures: Vec<serde_json::Value>,
    #[serde(default)]
    environments: BTreeMap<String, bri_content::environment::Environment>,
}
#[derive(Deserialize)]
struct MapEntry {
    id: String,
    name: String,
    file: String,
}
#[derive(Deserialize)]
struct TextureBinding {
    asset: String,
    texture: Option<String>,
}
#[derive(Default, Deserialize)]
struct Lighting {
    #[serde(default)]
    interiors: Vec<LightingFile>,
    #[serde(default)]
    terrain: Vec<LightingFile>,
}
#[derive(Deserialize)]
struct LightingFile {
    file: String,
    node: usize,
}
#[derive(Deserialize)]
struct CatalogAudit {
    resolved_meshes: Vec<MeshEntry>,
}
#[derive(Deserialize)]
struct MeshEntry {
    id: String,
    native_mesh: String,
}
#[derive(Deserialize)]
struct WorldReport {
    schema_version: u32,
    saves: Vec<WorldRecord>,
}
#[derive(Deserialize)]
struct WorldRecord {
    source: String,
    file: String,
    bricks: usize,
}

fn read_json<T: DeserializeOwned>(path: &Path, limit: u64) -> Result<T> {
    let file = fs::File::open(path)
        .with_context(|| format!("Opening native content {}", path.display()))?;
    ensure!(
        file.metadata()?.len() <= limit,
        "Native content file exceeds {limit} bytes: {}",
        path.display()
    );
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "Native content grew beyond limit: {}",
        path.display()
    );
    serde_json::from_slice(&bytes)
        .with_context(|| format!("Decoding native content {}", path.display()))
}

fn relative_name(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty() && !name.contains(['\\', ':', '\0', '<', '>', '"', '|', '?', '*']),
        "Invalid relative content path: {name}"
    );
    ensure!(
        name.split('/')
            .all(|s| !s.is_empty() && s != "." && s != ".." && !s.ends_with([' ', '.'])),
        "Ambiguous relative content path: {name}"
    );
    Ok(())
}

/// Resolve existing aliases before accepting a file. A junction/symlink inside
/// a package may not escape that package, even if its spelling looks relative.
fn contained(root: &Path, name: &str) -> Result<PathBuf> {
    relative_name(name)?;
    let root = root
        .canonicalize()
        .with_context(|| format!("Missing content directory {}", root.display()))?;
    let resolved = root
        .join(name)
        .canonicalize()
        .with_context(|| format!("Missing native content {name} in {}", root.display()))?;
    ensure!(
        resolved.starts_with(&root),
        "Content path escapes its package: {name}"
    );
    Ok(resolved)
}

fn file(root: &Path, name: &str, limit: u64) -> Result<PathBuf> {
    let path = contained(root, name)?;
    let meta = fs::metadata(&path)?;
    ensure!(
        meta.is_file() && meta.len() <= limit,
        "Missing, oversized or non-file native asset: {}",
        path.display()
    );
    Ok(path)
}

impl ContentPaths {
    pub fn resolve(root: &Path, config: &ContentConfig) -> Result<Self> {
        ensure!(
            config.schema_version == 1,
            "Unsupported client content config schema {}",
            config.schema_version
        );
        let root = root
            .canonicalize()
            .context("Content root is missing; supply the generated content directory")?;
        let package = |name: &str| -> Result<PathBuf> {
            let path = contained(&root, name)?;
            ensure!(path.is_dir(), "Expected native package directory: {name}");
            Ok(path)
        };
        Ok(Self {
            map_bundle: package(&config.map_bundle)?,
            brick_catalog: package(&config.brick_catalog)?,
            geometry: package(&config.geometry)?,
            effects: package(&config.effects)?,
            worlds: package(&config.worlds)?,
            ui_pack: package(&config.ui_pack)?,
            brick_materials: package(&config.brick_materials)?,
            avatar: package(&config.avatar)?,
            effects_runtime: package(&config.effects_runtime)?,
            audio: package(&config.audio)?,
            weather: package(&config.weather)?,
            foliage: package(&config.foliage)?,
            weapons: package(&config.weapons)?,
            item_presentation: package(&config.item_presentation)?,
            vehicles: package(&config.vehicles)?,
            events: package(&config.events)?,
            root,
        })
    }

    /// Expensive geometry decoding and collider construction belongs on the host
    /// worker. Only the requested saved world is read, never the whole corpus.
    pub fn load_map(&self, map_id: &str, world_id: Option<&str>) -> Result<LoadedMap> {
        ensure!(
            LOADABLE_MAPS.contains(&map_id),
            "Map is not integrated for native loading yet: {map_id}"
        );
        let bundle = load_bundle(&self.map_bundle)?;
        let entry = bundle
            .maps
            .iter()
            .find(|m| m.id == map_id)
            .context("Selected map is missing from native map bundle")?;
        validate_scene(&self.map_bundle, entry, &bundle)?;
        validate_catalog(self)?;
        let mut world = if let Some(id) = world_id {
            let index = world_index(&self.worlds)?;
            let entry = index
                .iter()
                .find(|w| w.id == id)
                .context("Unknown native reference-world ID")?;
            ensure!(
                entry.map_id == map_id,
                "Reference world belongs to another map: {}",
                entry.name
            );
            let path = file(
                &self.worlds,
                &entry.file,
                bri_world::persistence::MAX_SAVE_BYTES,
            )?;
            let world = bri_world::persistence::load(&path)?;
            ensure!(
                world.map_id == map_id && world.bricks.len() == entry.brick_count,
                "Native reference world metadata disagrees with its index"
            );
            world
        } else {
            let pack = load_ui_schema(&self.ui_pack)?;
            let palette = palette(&pack)?.into_iter().flat_map(|p| p.colors).collect();
            bri_world::World::new(entry.name.clone(), map_id.into(), palette)
        };
        let weapons = bri_net::content_identity::WeaponContent::load(&self.weapons)?;
        let unresolved_items = weapons.resolve_world_items(&mut world)?;
        let definitions = Definitions::load(&self.brick_catalog, &self.geometry)
            .context("Loading native brick definitions")?;
        let native =
            NativeMap::load(&self.map_bundle, map_id).context("Loading native map collision")?;
        let spawn = native
            .scene
            .nodes
            .iter()
            .find(|n| matches!(n.kind, Kind::Spawn))
            .context("Native map has no authored spawn")?;
        let spawn = [
            spawn.transform[12],
            spawn.transform[13],
            spawn.transform[14],
        ];
        ensure!(
            spawn.iter().all(|v| v.is_finite()),
            "Native map has an invalid spawn"
        );
        let query_colliders = native.colliders.clone();
        let terrain = native.terrain.clone();
        let anchors = native.spawn_anchors()?;
        let mut simulation = Simulation::new(world, definitions, native.colliders)
            .context("Building native simulation")?;
        simulation.attach_terrain(native.terrain, anchors)?;
        simulation.waters = native.waters;
        let spawn_points =
            bri_sim::spawn::candidates(&simulation.physics, &native.scene, &Default::default())?;
        let spawn = spawn_points[0].to_array();
        let mut pending_objects = native.pending_objects;
        if unresolved_items > 0 {
            pending_objects.push(format!(
                "{unresolved_items} unresolved brick item references retained"
            ));
        }
        Ok(LoadedMap {
            simulation,
            scene: native.scene,
            spawn,
            spawn_points,
            pending_objects,
            query_colliders,
            terrain,
        })
    }
}

impl ClientContent {
    /// Optional root/client-content.json overrides versioned package locations.
    pub fn load(root: &Path) -> Result<Self> {
        let config = if root.join("client-content.json").exists() {
            read_json(
                &file(root, "client-content.json", INDEX_LIMIT)?,
                INDEX_LIMIT,
            )?
        } else {
            ContentConfig::default()
        };
        Self::load_config(root, &config)
    }

    pub fn load_config(root: &Path, config: &ContentConfig) -> Result<Self> {
        let paths = ContentPaths::resolve(root, config)?;
        let weapons = bri_net::content_identity::WeaponContent::load(&paths.weapons)?;
        let item_physics = bri_net::content_identity::ItemPhysicsContent::load(
            &paths.item_presentation,
            &weapons,
        )?;
        let schema = load_ui_schema(&paths.ui_pack)?;
        let paint = palette(&schema)?;
        let bundle = load_bundle(&paths.map_bundle)?;
        let mut maps = Vec::new();
        let mut warnings = vec![
            "All 14 reference maps load native architecture, fixtures and water; Tutorial behavior, weather, foliage and full gameplay fidelity remain required.".into(),
            "Native brick materials include verified default print packages; complete historical vanilla inventory and exact material/color fidelity remain acceptance work.".into(),
            "Native item choices are loaded; complete item presentation and pickup/respawn fidelity still require gameplay verification.".into(),
            "Reference worlds include installed sample/community builds; their index is not vanilla certification.".into(),
            "Terrain streams around the camera and bodies at full detail; terrain LOD, detail/bump texturing, underwater presentation and dynamic map-object behavior remain pending.".into(),
        ];
        for entry in &bundle.maps {
            if !LOADABLE_MAPS.contains(&entry.id.as_str()) {
                warnings.push(format!(
                    "Native bundle map retained but not integrated: {}",
                    entry.id
                ));
                continue;
            }
            validate_scene(&paths.map_bundle, entry, &bundle)?;
            let legacy = entry.id.strip_prefix("v20/").unwrap_or(&entry.id);
            let display = schema
                .maps
                .iter()
                .find(|m| m.mission.eq_ignore_ascii_case(legacy));
            let preview = display
                .and_then(|m| m.preview.clone())
                .filter(|id| schema.images.contains_key(id))
                .map_or(IconRef::None, IconRef::Pack);
            maps.push(MapInfo {
                id: entry.id.clone(),
                name: entry.name.clone(),
                description: display.map(|m| m.description.clone()).unwrap_or_default(),
                preview,
            });
        }
        for expected in LOADABLE_MAPS {
            ensure!(
                maps.iter().any(|m| m.id == *expected),
                "Required integrated map missing: {expected}"
            );
        }
        let catalog = validate_catalog(&paths)?;
        let mut bricks = Vec::new();
        for entry in catalog.bricks.iter().filter(|b| b.selectable()) {
            let icon = entry.icon_source.replace('\\', "/").to_ascii_lowercase();
            let icon = icon
                .strip_suffix(".png")
                .or_else(|| icon.strip_suffix(".jpg"))
                .unwrap_or(&icon)
                .to_string();
            ensure!(
                schema.images.contains_key(&icon),
                "UI pack lacks selectable brick icon {icon} for {}",
                entry.id
            );
            bricks.push(BrickInfo {
                id: entry.id.clone(),
                ui_name: entry.display_name.clone(),
                category: entry.category.clone(),
                subcategory: entry.subcategory.clone(),
                icon: IconRef::Pack(icon),
            });
        }
        let effects: Library = read_json(
            &file(&paths.effects, "effects.json", INDEX_LIMIT)?,
            INDEX_LIMIT,
        )?;
        effects
            .validate()
            .context("Invalid native effect catalog")?;
        for name in effects.textures.values() {
            file(&paths.effects, name, INDEX_LIMIT)?;
        }
        let mut datablocks = DatablockMenus::new();
        datablocks.insert(
            "ItemData".into(),
            weapons
                .item_choices
                .iter()
                .map(|(id, name)| Choice {
                    id: id.clone(),
                    name: name.clone(),
                })
                .collect(),
        );
        datablocks.insert(
            "FxLightData".into(),
            effects
                .lights
                .iter()
                .filter(|e| !e.name.is_empty())
                .map(|e| Choice {
                    id: e.id.clone(),
                    name: e.name.clone(),
                })
                .collect(),
        );
        datablocks.insert(
            "ParticleEmitterData".into(),
            effects
                .emitters
                .iter()
                .filter(|e| !e.name.is_empty())
                .map(|e| Choice {
                    id: e.id.clone(),
                    name: e.name.clone(),
                })
                .collect(),
        );
        let worlds = world_index(&paths.worlds)?;
        if !bundle.unresolved_textures.is_empty() {
            warnings.push(format!(
                "Map bundle reports {} unresolved textures",
                bundle.unresolved_textures.len()
            ));
        }
        let vehicles = bri_vehicles::Pack::load(paths.vehicles.join("vehicles.json"))
            .context("Loading native vehicles")?;
        let music = music_choices(&paths.audio)?;
        let event_sounds = event_sound_choices(&paths.audio)?;
        let events = bri_events::Catalog::load(paths.events.join("catalog.json"))
            .context("Loading the wrench event catalog")?;
        Ok(Self {
            vehicles,
            music,
            events,
            event_sounds,
            ui_pack: Rc::new(Pack::from_parts(schema, paths.ui_pack.clone())),
            paths,
            maps,
            bricks,
            catalog,
            paint,
            datablocks,
            worlds,
            effects,
            weapons,
            item_physics,
            warnings,
        })
    }

    pub fn load_map(&self, map_id: &str, world_id: Option<&str>) -> Result<LoadedMap> {
        self.paths.load_map(map_id, world_id)
    }
}

/// Music bricks play the `music-brick:*` loops declared by the audio pack.
/// Sounds the `playSound` event may play: (sound id, name).
fn event_sound_choices(audio: &Path) -> Result<Vec<(String, String)>> {
    let manifest: serde_json::Value =
        read_json(&file(audio, "manifest.json", INDEX_LIMIT)?, INDEX_LIMIT)?;
    Ok(manifest["sounds"]
        .as_array()
        .context("Audio manifest has no sounds")?
        .iter()
        .filter(|s| {
            s["lists"]
                .as_array()
                .is_some_and(|l| l.iter().any(|v| v == "event-param:Sound"))
        })
        .filter_map(|s| {
            let id = s["id"].as_str()?.to_string();
            let name = s["ui_name"]
                .as_str()
                .or(s["name"].as_str())
                .unwrap_or(&id)
                .to_string();
            Some((id, name))
        })
        .collect())
}
fn music_choices(audio: &Path) -> Result<Vec<(String, String)>> {
    #[derive(Deserialize)]
    struct Trigger {
        key: String,
        sound: String,
    }
    #[derive(Deserialize)]
    struct Manifest {
        triggers: Vec<Trigger>,
    }
    let manifest: Manifest = read_json(&file(audio, "manifest.json", INDEX_LIMIT)?, INDEX_LIMIT)?;
    Ok(manifest
        .triggers
        .into_iter()
        .filter_map(|t| {
            let name = t.key.strip_prefix("music-brick:")?;
            Some((t.sound, name.replace('_', " ").trim().to_string()))
        })
        .collect())
}

fn load_bundle(root: &Path) -> Result<Bundle> {
    let bundle: Bundle = read_json(&file(root, "bundle.json", INDEX_LIMIT)?, INDEX_LIMIT)?;
    ensure!(
        bundle.schema_version == 1,
        "Unsupported native map-bundle schema {}",
        bundle.schema_version
    );
    ensure!(
        bundle.maps.len() <= 1024 && bundle.assets.len() <= 65536,
        "Oversized native map index"
    );
    let mut ids = BTreeSet::new();
    for entry in &bundle.maps {
        ensure!(
            ids.insert(&entry.id),
            "Duplicate native map ID {}",
            entry.id
        );
        file(root, &entry.file, INDEX_LIMIT)?;
    }
    for name in bundle.assets.values() {
        file(root, name, GEOMETRY_LIMIT)?;
    }
    for name in bundle.textures.values() {
        file(root, name, INDEX_LIMIT)?;
    }
    for (map, environment) in &bundle.environments {
        ensure!(ids.contains(map), "Environment references an unknown map");
        environment.validate()?;
        for image in environment
            .faces
            .iter()
            .chain(environment.reflection.iter())
            .chain(environment.clouds.iter().map(|c| &c.image))
        {
            file(root, &image.file, INDEX_LIMIT)?;
        }
    }
    for binding in &bundle.bindings {
        ensure!(
            bundle.assets.contains_key(&binding.asset),
            "Texture binding has unknown asset {}",
            binding.asset
        );
        if let Some(name) = &binding.texture {
            file(root, name, INDEX_LIMIT)?;
        }
    }
    for (id, light) in &bundle.lighting {
        ensure!(
            bundle.maps.iter().any(|m| &m.id == id),
            "Lighting has unknown map ID {id}"
        );
        for entry in light.interiors.iter().chain(&light.terrain) {
            file(root, &entry.file, INDEX_LIMIT)?;
        }
    }
    Ok(bundle)
}

fn validate_scene(root: &Path, entry: &MapEntry, bundle: &Bundle) -> Result<()> {
    let scene: Scene = read_json(&file(root, &entry.file, INDEX_LIMIT)?, INDEX_LIMIT)?;
    ensure!(
        scene.schema_version == 1 && scene.id == entry.id,
        "Native scene schema/identity mismatch: {}",
        entry.id
    );
    ensure!(
        !scene.nodes.is_empty() && scene.nodes.len() <= 65536,
        "Invalid native scene node count"
    );
    let mut physics = false;
    let mut spawn = false;
    for (i, node) in scene.nodes.iter().enumerate() {
        ensure!(
            node.transform.iter().all(|v| v.is_finite()),
            "Non-finite scene transform in {}",
            node.name
        );
        ensure!(
            node.parent.is_none_or(|p| p < i),
            "Invalid/cyclic scene parent at node {i}"
        );
        if matches!(node.kind, Kind::Interior | Kind::Terrain) {
            let asset = node
                .asset
                .as_ref()
                .context("Physical scene node lacks asset")?;
            ensure!(
                bundle.assets.contains_key(asset),
                "Scene references missing geometry {asset}"
            );
            physics = true;
        }
        spawn |= matches!(node.kind, Kind::Spawn);
    }
    ensure!(
        physics && spawn,
        "Native scene lacks physical geometry or authored spawn: {}",
        entry.id
    );
    if let Some(light) = bundle.lighting.get(&entry.id) {
        ensure!(
            light
                .interiors
                .iter()
                .chain(&light.terrain)
                .all(|l| l.node < scene.nodes.len()),
            "Lighting references missing scene node"
        );
    }
    Ok(())
}

fn validate_catalog(paths: &ContentPaths) -> Result<Catalog> {
    let catalog: Catalog = read_json(
        &file(&paths.brick_catalog, "stock-catalog.json", INDEX_LIMIT)?,
        INDEX_LIMIT,
    )?;
    let audit: CatalogAudit = read_json(
        &file(&paths.brick_catalog, "catalog-audit.json", INDEX_LIMIT)?,
        INDEX_LIMIT,
    )?;
    let collisions: CollisionLibrary = read_json(
        &file(&paths.brick_catalog, "native-collisions.json", INDEX_LIMIT)?,
        INDEX_LIMIT,
    )?;
    ensure!(
        catalog.schema_version == 1 && collisions.schema_version == 1,
        "Unsupported native brick/collision catalog schema"
    );
    ensure!(
        !catalog.bricks.is_empty() && catalog.bricks.len() <= 65536,
        "Invalid native brick catalog count"
    );
    let mut ids = BTreeSet::new();
    for brick in &catalog.bricks {
        ensure!(
            !brick.id.is_empty() && ids.insert(brick.id.as_str()),
            "Duplicate/empty brick ID {}",
            brick.id
        );
    }
    let mut mesh_ids = BTreeSet::new();
    for mesh in &audit.resolved_meshes {
        ensure!(
            mesh_ids.insert(mesh.id.as_str()) && ids.contains(mesh.id.as_str()),
            "Duplicate/unbound native mesh ID {}",
            mesh.id
        );
        ensure!(
            mesh.native_mesh.ends_with(".brick.json"),
            "Expected native brick JSON: {}",
            mesh.native_mesh
        );
        file(&paths.geometry, &mesh.native_mesh, GEOMETRY_LIMIT)?;
    }
    ensure!(
        mesh_ids == ids,
        "Native mesh bindings do not cover the brick catalog"
    );
    let mut collision_ids = BTreeSet::new();
    for body in &collisions.bodies {
        body.validate()?;
        ensure!(
            collision_ids.insert(body.id.as_str()),
            "Duplicate collision ID {}",
            body.id
        );
    }
    ensure!(
        collision_ids == ids,
        "Native collision bindings do not cover the brick catalog"
    );
    Ok(catalog)
}

fn load_ui_schema(root: &Path) -> Result<UiPack> {
    let pack: UiPack = read_json(&file(root, "ui-pack.json", INDEX_LIMIT)?, INDEX_LIMIT)?;
    ensure!(
        pack.schema_version == PACK_SCHEMA_VERSION,
        "Unsupported UI pack schema {}",
        pack.schema_version
    );
    for (id, image) in &pack.images {
        ensure!(
            image.width > 0
                && image.height > 0
                && image.width <= 8192
                && image.height <= 8192
                && u64::from(image.width) * u64::from(image.height) <= 16_777_216,
            "Invalid UI image size: {id}"
        );
        file(root, &image.file, INDEX_LIMIT)?;
    }
    for (id, font) in &pack.fonts {
        ensure!(
            font.glyphs.len() == 256 && !font.sheets.is_empty() && font.sheets.len() <= 64,
            "Invalid UI font {id}"
        );
        for sheet in &font.sheets {
            file(root, sheet, INDEX_LIMIT)?;
        }
        ensure!(
            font.glyphs
                .iter()
                .flatten()
                .all(|g| (g.sheet as usize) < font.sheets.len()),
            "UI font glyph references missing sheet: {id}"
        );
    }
    for style in pack.styles.values() {
        if let Some(font) = &style.font {
            ensure!(
                pack.fonts.contains_key(font),
                "UI style references missing font {font}"
            );
        }
    }
    for id in pack.skins.keys() {
        ensure!(
            pack.images.contains_key(id),
            "UI skin references missing bitmap {id}"
        );
    }
    Ok(pack)
}

fn palette(pack: &UiPack) -> Result<Vec<PaintDivision>> {
    let paint: Vec<_> = pack
        .data
        .brick_colorset
        .iter()
        .map(|d| PaintDivision {
            name: d.name.clone(),
            colors: d.colors.clone(),
        })
        .collect();
    let count: usize = paint.iter().map(|d| d.colors.len()).sum();
    ensure!(
        (1..=256).contains(&count)
            && paint
                .iter()
                .flat_map(|d| &d.colors)
                .flatten()
                .all(|c| c.is_finite() && (0.0..=1.0).contains(c)),
        "Invalid native default paint palette"
    );
    Ok(paint)
}

fn world_index(root: &Path) -> Result<Vec<WorldEntry>> {
    let report: WorldReport = read_json(&file(root, "report.json", INDEX_LIMIT)?, INDEX_LIMIT)?;
    ensure!(
        report.schema_version == 1,
        "Unsupported reference-world index schema"
    );
    let mut ids = BTreeSet::new();
    let mut out = Vec::new();
    for entry in report.saves {
        relative_name(&entry.source)?;
        ensure!(
            entry.file.ends_with(".world.json") && entry.bricks <= bri_world::MAX_BRICKS,
            "Invalid reference-world entry"
        );
        file(root, &entry.file, bri_world::persistence::MAX_SAVE_BYTES)?;
        let source = entry.source.replace('\\', "/");
        let (folder, name) = source
            .split_once('/')
            .context("Reference-world source lacks map folder")?;
        let name = name
            .strip_suffix(".bls")
            .context("Reference-world source lacks BLS provenance suffix")?;
        let map_id = match folder.to_ascii_lowercase().as_str() {
            "bedroom" => LOADABLE_MAPS[0],
            "kitchen" => LOADABLE_MAPS[1],
            "slopes" => LOADABLE_MAPS[2],
            "slate" => LOADABLE_MAPS[3],
            "tutorial" => LOADABLE_MAPS[13],
            _ => "",
        }
        .to_string();
        let id = format!(
            "v20/world/{}/{}",
            folder.to_ascii_lowercase(),
            name.to_ascii_lowercase()
        );
        ensure!(ids.insert(id.clone()), "Duplicate reference-world ID {id}");
        out.push(WorldEntry {
            id,
            name: name.into(),
            loadable: LOADABLE_MAPS.contains(&map_id.as_str()),
            map_id,
            file: entry.file,
            brick_count: entry.bricks,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().canonicalize().unwrap();
            let path = root.join(format!(
                "bri-native-content-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let temp = std::env::temp_dir().canonicalize().unwrap();
            assert_eq!(self.0.parent(), Some(temp.as_path()));
            assert!(
                self.0
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("bri-native-content-")
            );
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn reference_world_index_is_lazy_and_only_advertises_integrated_maps() {
        let fixture = Fixture::new();
        // Deliberately invalid world bytes: an index scan must not parse them.
        fs::write(
            fixture.0.join("lazy.world.json"),
            b"not decoded during startup",
        )
        .unwrap();
        fs::write(
            fixture.0.join("report.json"),
            serde_json::to_vec(&serde_json::json!({"schema_version":1,"saves":[
                {"source":"Bedroom/Example.bls","file":"lazy.world.json","bricks":12},
                {"source":"Slate/Example.bls","file":"lazy.world.json","bricks":34},
                {"source":"Unsupported/Example.bls","file":"lazy.world.json","bricks":56}
            ]}))
            .unwrap(),
        )
        .unwrap();
        let index = world_index(&fixture.0).unwrap();
        assert_eq!(index[0].id, "v20/world/bedroom/example");
        assert!(index[0].loadable);
        assert!(index[1].loadable);
        assert_eq!(index[1].map_id, LOADABLE_MAPS[3]);
        assert!(!index[2].loadable);
        assert!(bri_world::persistence::load(&fixture.0.join(&index[0].file)).is_err());
    }

    #[test]
    fn map_index_rejects_unsupported_schema_and_missing_geometry() {
        let fixture = Fixture::new();
        let mut value = serde_json::json!({"schema_version":2,"maps":[],"assets":{},"textures":{}});
        fs::write(
            fixture.0.join("bundle.json"),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
        assert!(
            load_bundle(&fixture.0)
                .err()
                .unwrap()
                .to_string()
                .contains("schema")
        );
        value["schema_version"] = serde_json::json!(1);
        value["assets"] = serde_json::json!({"v20/geometry":"missing.terrain.json"});
        fs::write(
            fixture.0.join("bundle.json"),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
        assert!(
            load_bundle(&fixture.0)
                .err()
                .unwrap()
                .to_string()
                .contains("missing.terrain.json")
        );
        assert!(file(&fixture.0, "../escape.world.json", 1024).is_err());
    }
    #[test]
    fn config_and_paths_are_send_without_ui_pack() {
        fn send<T: Send + Sync>() {}
        send::<ContentConfig>();
        send::<ContentPaths>();
    }
    #[test]
    fn old_content_config_defaults_native_item_presentation_path() {
        let config: ContentConfig =
            serde_json::from_str(r#"{"schema_version":1,"weapons":"weapons-pack-003"}"#).unwrap();
        assert_eq!(config.item_presentation, "item-presentation-pack-003");
        assert!(serde_json::from_str::<ContentConfig>(r#"{"item_presentaton":"typo"}"#).is_err());
    }
    #[test]
    fn portable_content_paths_reject_traversal_and_alias_spellings() {
        for p in [
            "../pack",
            "/pack",
            "x/../pack",
            "C:/pack",
            "x\\pack",
            "x:stream",
            "x/.",
            "x/name.",
            "x/name ",
        ] {
            assert!(relative_name(p).is_err(), "{p}");
        }
        assert!(relative_name("ui-pack-003/images/base/client/ui/btn.png").is_ok());
    }
    #[test]
    fn invalid_config_fails_before_filesystem_loading() {
        let c = ContentConfig {
            schema_version: 99,
            ..Default::default()
        };
        assert!(
            ContentPaths::resolve(Path::new("nonexistent"), &c)
                .unwrap_err()
                .to_string()
                .contains("schema")
        );
    }
    #[test]
    fn missing_native_reference_has_actionable_error() {
        let err = file(
            Path::new(env!("CARGO_MANIFEST_DIR")),
            "definitely-missing-native.json",
            1024,
        )
        .unwrap_err();
        assert!(err.to_string().contains("definitely-missing-native.json"));
    }
    #[test]
    #[ignore = "requires ignored generated native content; no game window or OS input"]
    fn local_native_content_index_and_lazy_maps() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let content = ClientContent::load(&root).unwrap();
        assert_eq!(
            (
                content.maps.len(),
                content.bricks.len(),
                content.worlds.len()
            ),
            (14, 166, 35)
        );
        assert_eq!(content.datablocks["FxLightData"].len(), 13);
        assert_eq!(content.datablocks["ParticleEmitterData"].len(), 102);
        assert!(
            content
                .maps
                .iter()
                .all(|m| matches!(m.preview, IconRef::Pack(_)))
        );
        assert_eq!(content.datablocks["ItemData"].len(), 21);
        assert!(
            content
                .paths
                .load_map("v20/add-ons/map_unsupported/unsupported.mis", None)
                .is_err()
        );
        let mut maps = Vec::new();
        for map in &content.maps {
            let loaded = content.paths.load_map(&map.id, None).unwrap();
            assert!(loaded.simulation.state().bricks.is_empty());
            assert_eq!(loaded.scene.id, map.id);
            assert!(loaded.spawn.iter().all(|v| v.is_finite()));
            maps.push(serde_json::json!({"id":map.id,"spawn":loaded.spawn,"scene_nodes":loaded.scene.nodes.len(),
                "colliders":loaded.simulation.physics.colliders.len(),"water_regions":loaded.simulation.waters.len(),"empty_world":true,"pending_objects":loaded.pending_objects}));
            if map.id == "v20/add-ons/map_slate_sea_revised/slatesearevised.mis" {
                assert_eq!(loaded.simulation.waters.len(), 2);
                let mut session = bri_sim::session::Session::new(loaded.simulation);
                session
                    .join("Water check".into(), glam::Vec3::from(loaded.spawn), true)
                    .unwrap();
                for _ in 0..1800 {
                    session.step().unwrap();
                }
                let player = &session.snapshot().players[0];
                assert!(
                    (player.feet[1] - (9.0 - 2.65 * 0.7)).abs() < 0.08,
                    "Native Sea spawn did not float to authored surface: {:?}",
                    player
                );
            }
        }
        let demo = content
            .worlds
            .iter()
            .find(|w| w.name == "Demo House")
            .unwrap();
        let loaded = content
            .paths
            .load_map(&demo.map_id, Some(&demo.id))
            .unwrap();
        assert_eq!(loaded.simulation.state().bricks.len(), demo.brick_count);
        assert!(
            content
                .paths
                .load_map(LOADABLE_MAPS[1], Some(&demo.id))
                .is_err()
        );
        let report = serde_json::json!({"schema_version":1,"passed":true,"maps":maps,"selectable_bricks":content.bricks.len(),
            "reference_world_entries":content.worlds.len(),"light_menu_entries":content.datablocks["FxLightData"].len(),
            "emitter_menu_entries":content.datablocks["ParticleEmitterData"].len(),"default_palette_colors":content.paint.iter().map(|d|d.colors.len()).sum::<usize>(),
            "reference_world_loaded":{"id":demo.id,"bricks":demo.brick_count},"warnings":content.warnings,
            "scope":"native content index and lazy map/reference-world collider construction; not gameplay or full vanilla acceptance"});
        let output =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../artifacts/native-client-content");
        fs::create_dir_all(&output).unwrap();
        fs::write(
            output.join("integration.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
    }
}
