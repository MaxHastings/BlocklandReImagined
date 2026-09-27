//! Versioned fingerprints of native content. Production peers use the full form.
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
};

pub const CONTENT_IDENTITY_VERSION: u32 = 9;
const WEAPON_INDEX_LIMIT: u64 = 32 * 1024 * 1024;
const WEAPON_RESOURCE_LIMIT: u64 = 64 * 1024 * 1024;
const WEAPON_TOTAL_LIMIT: u64 = 512 * 1024 * 1024;

/// Validated startup snapshot shared by host/join/dedicated catalog setup.
/// Source resource hashes describe originals, not converted native bytes.
#[derive(Clone)]
pub struct WeaponContent {
    pub pack: bri_weapons::Pack,
    pub item_choices: Vec<(String, String)>,
    aliases: BTreeMap<String, String>,
    fingerprint: String,
    manifest_sha256: String,
}
impl WeaponContent {
    pub fn load(root: &Path) -> Result<Self> {
        let root = root.canonicalize()?;
        let manifest = contained(&root, "weapons.json")?;
        let mut bytes = Vec::new();
        std::fs::File::open(&manifest)?
            .take(WEAPON_INDEX_LIMIT + 1)
            .read_to_end(&mut bytes)?;
        let pack = bri_weapons::Pack::from_json(&bytes)?;
        ensure!(
            pack.resources.len() <= 4096,
            "Weapon resource budget exceeded"
        );
        let mut files = BTreeMap::from([("weapons.json".into(), manifest)]);
        let mut total = bytes.len() as u64;
        for resource in &pack.resources {
            if let Some(name) = &resource.native_file {
                ensure!(name != "weapons.json", "Reserved weapon resource filename");
                if files.contains_key(name) {
                    continue; // Shared source resources may bind the same native file.
                }
                let path = contained(&root, name)?;
                let size = std::fs::metadata(&path)?.len();
                ensure!(
                    size <= WEAPON_RESOURCE_LIMIT,
                    "Oversized weapon resource: {name}"
                );
                total = total
                    .checked_add(size)
                    .context("Weapon byte budget overflow")?;
                ensure!(
                    total <= WEAPON_TOTAL_LIMIT,
                    "Weapon total byte budget exceeded"
                );
                files.insert(name.clone(), path);
            }
        }
        // The bounded read above supplied the actual parsed definitions. Require
        // identical bytes during hashing so a replacement cannot mix snapshots.
        let expected = BTreeMap::from([(
            "weapons.json".into(),
            format!("{:x}", Sha256::digest(&bytes)),
        )]);
        let fingerprint = hash_files_bounded(
            b"BRI_WEAPONS_V1\0",
            files,
            &expected,
            WEAPON_RESOURCE_LIMIT,
            WEAPON_TOTAL_LIMIT,
        )?;
        let mut item_choices: Vec<_> = pack
            .items
            .values()
            .map(|item| (item.id.clone(), item.ui_name.trim().to_string()))
            .collect();
        ensure!(
            item_choices.len() <= 1024,
            "Weapon item catalog budget exceeded"
        );
        let mut aliases = BTreeMap::new();
        let mut ids = std::collections::BTreeSet::new();
        for (id, name) in &item_choices {
            bri_world::ContentRef::Resolved(id.clone()).validate()?;
            ensure!(!id.chars().any(char::is_control), "Invalid weapon item ID");
            ensure!(ids.insert(id.clone()), "Duplicate weapon item ID: {id}");
            ensure!(
                !name.trim().is_empty() && name.len() <= 128 && !name.chars().any(char::is_control),
                "Invalid weapon item name"
            );
            ensure!(
                aliases
                    .insert(name.trim().to_ascii_lowercase(), id.clone())
                    .is_none(),
                "Ambiguous weapon item display name: {name}"
            );
        }
        item_choices.sort_by(|a, b| {
            a.1.to_ascii_lowercase()
                .cmp(&b.1.to_ascii_lowercase())
                .then(a.0.cmp(&b.0))
        });
        Ok(Self {
            pack,
            item_choices,
            aliases,
            fingerprint,
            manifest_sha256: format!("{:x}", Sha256::digest(&bytes)),
        })
    }

    pub fn extend_identity(&self, base: &str) -> String {
        let mut hash = Sha256::new();
        hash.update(b"BRI_NATIVE_RUNTIME_V8\0");
        hash.update(base);
        hash.update(&self.fingerprint);
        format!("{:x}", hash.finalize())
    }

    /// Catalogs are immutable during an App lifetime. Reloading requires restart
    /// so cached UI choices cannot disagree with newly loaded server definitions.
    pub fn ensure_same(&self, other: &Self) -> Result<()> {
        ensure!(
            self.fingerprint == other.fingerprint,
            "Native weapons changed after startup; restart before hosting or joining"
        );
        Ok(())
    }

    /// Resolve known imported display names; report the remaining unresolved count.
    /// Raw source records are preserved and never interpreted by this operation.
    pub fn resolve_world_items(&self, world: &mut bri_world::World) -> Result<usize> {
        let mut unresolved = 0;
        for brick in world.bricks.values_mut() {
            brick.item_spawn.resolve_item(&self.aliases)?;
            unresolved += usize::from(matches!(
                brick.item_spawn.item,
                Some(bri_world::ContentRef::Unresolved { .. })
            ));
        }
        Ok(unresolved)
    }
}

pub fn with_weapons(base: &str, root: &Path) -> Result<String> {
    Ok(WeaponContent::load(root)?.extend_identity(base))
}

/// Native metadata/resource integrity only: no renderer or original-file reader.
#[derive(Clone)]
pub struct ItemPhysicsContent {
    pub bounds: BTreeMap<String, bri_weapons::ItemBounds>,
    fingerprint: String,
}
#[derive(serde::Deserialize)]
struct PhysicsManifest {
    schema_version: u32,
    weapons_sha256: String,
    item_physics_sha256: String,
    models: BTreeMap<String, PhysicsModel>,
    textures: BTreeMap<String, PhysicsTexture>,
    items: BTreeMap<String, PhysicsItem>,
}
#[derive(serde::Deserialize)]
struct PhysicsModel {
    file: String,
    sha256: String,
    source_sha256: String,
    bounds_min: [f32; 3],
    bounds_max: [f32; 3],
}
#[derive(serde::Deserialize)]
struct PhysicsTexture {
    file: String,
    sha256: String,
}
#[derive(serde::Deserialize)]
struct PhysicsItem {
    model: String,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PhysicsCatalog {
    schema_version: u32,
    items: BTreeMap<String, bri_weapons::ItemBounds>,
}
fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn bounded_bytes(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    ensure!(
        file.metadata()?.len() <= limit,
        "Oversized native item metadata"
    );
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "Native item metadata grew beyond limit"
    );
    Ok(bytes)
}
impl ItemPhysicsContent {
    pub fn load(root: &Path, weapons: &WeaponContent) -> Result<Self> {
        let root = root.canonicalize()?;
        let manifest_path = contained(&root, "presentation.json")?;
        let physics_path = contained(&root, "item-physics.json")?;
        let manifest_bytes = bounded_bytes(&manifest_path, WEAPON_INDEX_LIMIT)?;
        let physics_bytes = bounded_bytes(&physics_path, 2 * 1024 * 1024)?;
        let manifest: PhysicsManifest = serde_json::from_slice(&manifest_bytes)?;
        let physics: PhysicsCatalog = serde_json::from_slice(&physics_bytes)?;
        ensure!(
            manifest.schema_version == 2 && physics.schema_version == 1,
            "Unsupported native item physics/presentation schema"
        );
        ensure!(
            manifest.weapons_sha256 == weapons.manifest_sha256,
            "Item physics weapon checksum mismatch"
        );
        ensure!(
            manifest.item_physics_sha256 == format!("{:x}", Sha256::digest(&physics_bytes)),
            "Item physics checksum mismatch"
        );
        ensure!(
            manifest.models.len() <= 1024 && manifest.textures.len() <= 4096,
            "Item presentation resource budget exceeded"
        );
        ensure!(
            physics.items.len() == weapons.item_choices.len()
                && manifest.items.len() == physics.items.len(),
            "Item physics catalog coverage mismatch"
        );
        for (id, _) in &weapons.item_choices {
            let bounds = physics
                .items
                .get(id)
                .context("Missing native item bounds")?;
            bounds.validate()?;
            let item = manifest
                .items
                .get(id)
                .context("Missing presentation item")?;
            let model = manifest
                .models
                .get(&item.model)
                .context("Missing item bounds model")?;
            ensure!(
                bounds.min == model.bounds_min && bounds.max == model.bounds_max,
                "Item bounds disagree with authored model: {id}"
            );
            if let Some(weapon) = weapons.pack.items.get(id) {
                ensure!(
                    item.model == weapon.model.replace('\\', "/").to_ascii_lowercase(),
                    "Item model disagrees with weapon definition: {id}"
                );
            }
        }
        let mut files = BTreeMap::from([
            ("presentation.json".into(), manifest_path),
            ("item-physics.json".into(), physics_path),
        ]);
        let mut expected = BTreeMap::from([
            (
                "presentation.json".into(),
                format!("{:x}", Sha256::digest(&manifest_bytes)),
            ),
            ("item-physics.json".into(), manifest.item_physics_sha256),
        ]);
        for model in manifest.models.values() {
            ensure!(
                valid_sha256(&model.source_sha256),
                "Invalid original model provenance hash"
            );
            bri_weapons::ItemBounds {
                min: model.bounds_min,
                max: model.bounds_max,
            }
            .validate()?;
        }
        for (name, checksum) in manifest
            .models
            .values()
            .map(|r| (&r.file, &r.sha256))
            .chain(manifest.textures.values().map(|r| (&r.file, &r.sha256)))
        {
            ensure!(valid_sha256(checksum), "Invalid native item resource hash");
            ensure!(
                name != "presentation.json" && name != "item-physics.json",
                "Reserved item resource filename"
            );
            if let Some(previous) = expected.get(name) {
                ensure!(
                    previous == checksum,
                    "Conflicting native item resource hash"
                );
                continue;
            }
            files.insert(name.clone(), contained(&root, name)?);
            expected.insert(name.clone(), checksum.clone());
        }
        let fingerprint = hash_files_bounded(
            b"BRI_ITEM_PRESENTATION_PHYSICS_V1\0",
            files,
            &expected,
            WEAPON_RESOURCE_LIMIT,
            WEAPON_TOTAL_LIMIT,
        )?;
        Ok(Self {
            bounds: physics.items,
            fingerprint,
        })
    }
    pub fn extend_identity(&self, base: &str) -> String {
        let mut hash = Sha256::new();
        hash.update(b"BRI_NATIVE_RUNTIME_V9\0");
        hash.update(base);
        hash.update(&self.fingerprint);
        format!("{:x}", hash.finalize())
    }
    pub fn ensure_same(&self, other: &Self) -> Result<()> {
        ensure!(
            self.fingerprint == other.fingerprint,
            "Native item physics/presentation changed after startup; restart before hosting or joining"
        );
        Ok(())
    }
}
fn base_files(catalog: &Path, converted: &Path, maps: &Path) -> Result<BTreeMap<String, PathBuf>> {
    let mut files = BTreeMap::<String, PathBuf>::new();
    for name in ["stock-catalog.json", "native-collisions.json"] {
        files.insert(format!("catalog/{name}"), catalog.join(name));
    }
    let audit: serde_json::Value =
        serde_json::from_slice(&std::fs::read(catalog.join("catalog-audit.json"))?)?;
    for entry in audit["resolved_meshes"]
        .as_array()
        .context("Missing mesh bindings")?
    {
        let name = entry["native_mesh"]
            .as_str()
            .context("Missing mesh filename")?;
        ensure!(
            !name.contains(['/', '\\', ':']) && name.ends_with(".brick.json"),
            "Invalid mesh filename"
        );
        files.insert(format!("bricks/{name}"), converted.join(name));
    }
    // The bundle is flat: JSON geometry/scenes plus converted texture resources.
    for entry in std::fs::read_dir(maps)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("Non-UTF8 native filename"))?;
            files.insert(format!("maps/{name}"), entry.path());
        }
    }
    Ok(files)
}
/// Legacy diagnostic/probe identity; deliberately retains its original digest.
/// Production host/join must use `fingerprint_full` so prints/effects participate.
pub fn fingerprint(catalog: &Path, converted: &Path, maps: &Path) -> Result<String> {
    hash_files(
        b"BRI_NATIVE_BRICKS_MAPS_V1\0",
        base_files(catalog, converted, maps)?,
        &BTreeMap::new(),
    )
}

/// Complete current native content identity, including all declared print/icon/
/// surface/effect images and their catalogs. Paths on disk do not enter the hash.
/// Different digest domains prevent a legacy peer matching a full-content host.
pub fn fingerprint_full(
    catalog: &Path,
    converted: &Path,
    maps: &Path,
    materials: &Path,
    effects: &Path,
) -> Result<String> {
    use bri_content::{
        brick_materials::{Bundle, safe_relative},
        effects::Library,
    };
    let mut files = base_files(catalog, converted, maps)?;
    files.insert(
        "catalog/catalog-audit.json".into(),
        catalog.join("catalog-audit.json"),
    );
    let materials = materials
        .canonicalize()
        .context("Missing native brick materials directory")?;
    let effects = effects
        .canonicalize()
        .context("Missing native effects directory")?;
    let read_index = |root: &Path, name: &str| -> Result<Vec<u8>> {
        let path = contained(root, name)?;
        ensure!(
            std::fs::metadata(&path)?.len() <= 64 * 1024 * 1024,
            "Content index too large"
        );
        Ok(std::fs::read(path)?)
    };
    let bundle: Bundle = serde_json::from_slice(&read_index(&materials, "brick-materials.json")?)?;
    bundle.validate().context("Invalid brick material bundle")?;
    let library: Library = serde_json::from_slice(&read_index(&effects, "effects.json")?)?;
    library.validate().context("Invalid effect library")?;
    files.insert(
        "materials/brick-materials.json".into(),
        materials.join("brick-materials.json"),
    );
    files.insert("effects/effects.json".into(), effects.join("effects.json"));
    let mut expected = BTreeMap::new();
    for image in bundle.images() {
        let key = format!("materials/{}", image.path);
        files.insert(key.clone(), contained(&materials, &image.path)?);
        expected.insert(key, image.sha256.clone());
    }
    for name in library.textures.values() {
        files.insert(format!("effects/{name}"), contained(&effects, name)?);
    }
    // Check original identity inputs too: a native package cannot smuggle a
    // symlink to another content root into an otherwise matching fingerprint.
    for (namespace, root) in [("catalog", catalog), ("bricks", converted), ("maps", maps)] {
        let root = root.canonicalize()?;
        for (name, path) in files
            .iter_mut()
            .filter(|(n, _)| n.starts_with(&format!("{namespace}/")))
        {
            let relative = &name[namespace.len() + 1..];
            ensure!(safe_relative(relative), "Unsafe native identity path");
            *path = contained(&root, relative)?;
        }
    }
    hash_files(
        b"BRI_NATIVE_BRICKS_MAPS_MATERIALS_EFFECTS_V2\0",
        files,
        &expected,
    )
}
fn contained(root: &Path, relative: &str) -> Result<PathBuf> {
    ensure!(
        bri_content::brick_materials::safe_relative(relative),
        "Unsafe native content path: {relative}"
    );
    let path = root
        .join(relative)
        .canonicalize()
        .with_context(|| format!("Missing native resource {relative}"))?;
    ensure!(
        path.starts_with(root) && path.is_file(),
        "Native resource escapes package: {relative}"
    );
    Ok(path)
}

/// Current runtime identity adds avatar rig, customization tables and every
/// declared original face/decal/surface byte to the existing native identity.
pub fn fingerprint_runtime(
    catalog: &Path,
    converted: &Path,
    maps: &Path,
    materials: &Path,
    effects: &Path,
    avatar: &Path,
) -> Result<String> {
    let base = fingerprint_full(catalog, converted, maps, materials, effects)?;
    let root = avatar.canonicalize()?;
    let index = contained(&root, "avatar.json")?;
    let mut bytes = Vec::new();
    std::fs::File::open(&index)?
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 8 * 1024 * 1024,
        "Avatar catalog exceeds limit"
    );
    let package: bri_content::avatar::Package = serde_json::from_slice(&bytes)?;
    package.validate()?;
    let mut files = BTreeMap::from([("avatar/avatar.json".into(), index)]);
    let mut expected = BTreeMap::new();
    for (name, sha) in std::iter::once((&package.rig, &package.rig_sha256))
        .chain(package.textures.values().map(|t| (&t.file, &t.sha256)))
    {
        let key = format!("avatar/{name}");
        files.insert(key.clone(), contained(&root, name)?);
        expected.insert(key, sha.clone());
    }
    let avatar = hash_files(b"BRI_AVATAR_V1\0", files, &expected)?;
    let mut digest = Sha256::new();
    digest.update(b"BRI_NATIVE_RUNTIME_V3\0");
    digest.update(base);
    digest.update(avatar);
    Ok(format!("{:x}", digest.finalize()))
}
/// Extend the runtime identity with the complete cosmetic pack: definitions,
/// composite bindings and original pixels. Server hashing needs no GPU dependency.
pub fn with_effects_runtime(base: &str, root: &Path) -> Result<String> {
    #[derive(serde::Deserialize)]
    struct Texture {
        file: String,
        sha256: String,
    }
    #[derive(serde::Deserialize)]
    struct Manifest {
        schema_version: u32,
        library_sha256: String,
        textures: BTreeMap<String, Texture>,
    }
    let root = root.canonicalize()?;
    let manifest = contained(&root, "manifest.json")?;
    let library = contained(&root, "effects.json")?;
    let read = |path: &Path, limit: u64| -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(limit + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= limit,
            "Oversized effects identity index"
        );
        Ok(bytes)
    };
    let index: Manifest = serde_json::from_slice(&read(&manifest, 8 << 20)?)?;
    ensure!(
        index.schema_version == 1,
        "Unsupported effects runtime schema"
    );
    let definitions: bri_content::effects::Library =
        serde_json::from_slice(&read(&library, 16 << 20)?)?;
    definitions.validate()?;
    ensure!(
        definitions.textures.len() == index.textures.len(),
        "Effects texture index mismatch"
    );
    let mut files = BTreeMap::from([
        ("manifest.json".into(), manifest),
        ("effects.json".into(), library),
    ]);
    let mut expected = BTreeMap::from([("effects.json".into(), index.library_sha256)]);
    for (id, record) in index.textures {
        ensure!(
            !files.contains_key(&record.file),
            "Duplicate/reserved effects filename"
        );
        ensure!(
            definitions.textures.get(&id) == Some(&record.file),
            "Effects texture binding mismatch"
        );
        files.insert(record.file.clone(), contained(&root, &record.file)?);
        expected.insert(record.file, record.sha256);
    }
    let effects = hash_files(b"BRI_EFFECTS_RUNTIME_V1\0", files, &expected)?;
    let mut digest = Sha256::new();
    digest.update(b"BRI_NATIVE_RUNTIME_V4\0");
    digest.update(base);
    digest.update(effects);
    Ok(format!("{:x}", digest.finalize()))
}

/// Audio definitions, trigger mappings and every declared original clip join the
/// peer identity. Hashing does not decode or open an audio device on the host.
pub fn with_audio(base: &str, root: &Path) -> Result<String> {
    #[derive(serde::Deserialize)]
    struct Clip {
        file: String,
        sha256: String,
    }
    #[derive(serde::Deserialize)]
    struct Manifest {
        schema_version: u32,
        clips: Vec<Clip>,
    }
    let root = root.canonicalize()?;
    let path = contained(&root, "manifest.json")?;
    let mut bytes = Vec::new();
    std::fs::File::open(&path)?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 16 * 1024 * 1024, "Oversized audio manifest");
    let manifest: Manifest = serde_json::from_slice(&bytes)?;
    ensure!(
        manifest.schema_version == 1 && manifest.clips.len() <= 65536,
        "Unsupported audio identity manifest"
    );
    let mut files = BTreeMap::from([("manifest.json".into(), path)]);
    let mut expected = BTreeMap::new();
    for clip in manifest.clips {
        ensure!(
            !files.contains_key(&clip.file),
            "Duplicate/reserved audio filename"
        );
        files.insert(clip.file.clone(), contained(&root, &clip.file)?);
        expected.insert(clip.file, clip.sha256);
    }
    let audio = hash_files(b"BRI_AUDIO_V1\0", files, &expected)?;
    let mut hash = Sha256::new();
    hash.update(b"BRI_NATIVE_RUNTIME_V5\0");
    hash.update(base);
    hash.update(audio);
    Ok(format!("{:x}", hash.finalize()))
}

/// Authored precipitation and its native texture bytes are part of the peer pack.
pub fn with_weather(base: &str, root: &Path) -> Result<String> {
    #[derive(serde::Deserialize)]
    struct Texture {
        file: String,
        sha256: String,
    }
    #[derive(serde::Deserialize)]
    struct Manifest {
        schema_version: u32,
        textures: BTreeMap<String, Texture>,
    }
    let root = root.canonicalize()?;
    let path = contained(&root, "weather.json")?;
    let mut bytes = Vec::new();
    std::fs::File::open(&path)?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 16 * 1024 * 1024,
        "Oversized weather manifest"
    );
    let manifest: Manifest = serde_json::from_slice(&bytes)?;
    ensure!(
        manifest.schema_version == 1 && manifest.textures.len() <= 256,
        "Unsupported weather identity manifest"
    );
    let mut files = BTreeMap::from([("weather.json".into(), path)]);
    let mut expected = BTreeMap::new();
    for texture in manifest.textures.into_values() {
        ensure!(
            !files.contains_key(&texture.file),
            "Duplicate/reserved weather filename"
        );
        files.insert(texture.file.clone(), contained(&root, &texture.file)?);
        expected.insert(texture.file, texture.sha256);
    }
    let weather = hash_files(b"BRI_WEATHER_V1\0", files, &expected)?;
    let mut hash = Sha256::new();
    hash.update(b"BRI_NATIVE_RUNTIME_V6\0");
    hash.update(base);
    hash.update(weather);
    Ok(format!("{:x}", hash.finalize()))
}

/// Static plant placement parameters and original atlas pixels join the native pack.
pub fn with_foliage(base: &str, root: &Path) -> Result<String> {
    #[derive(serde::Deserialize)]
    struct Texture {
        path: String,
        sha256: String,
    }
    #[derive(serde::Deserialize)]
    struct Manifest {
        schema_version: u32,
        textures: Vec<Texture>,
    }
    let root = root.canonicalize()?;
    let path = contained(&root, "foliage.json")?;
    let mut bytes = Vec::new();
    std::fs::File::open(&path)?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 16 * 1024 * 1024,
        "Oversized foliage manifest"
    );
    let manifest: Manifest = serde_json::from_slice(&bytes)?;
    ensure!(
        manifest.schema_version == 1 && manifest.textures.len() <= 128,
        "Unsupported foliage identity manifest"
    );
    let mut files = BTreeMap::from([("foliage.json".into(), path)]);
    let mut expected = BTreeMap::new();
    for texture in manifest.textures {
        ensure!(
            !files.contains_key(&texture.path),
            "Duplicate/reserved foliage filename"
        );
        files.insert(texture.path.clone(), contained(&root, &texture.path)?);
        expected.insert(texture.path, texture.sha256);
    }
    let foliage = hash_files(b"BRI_FOLIAGE_V1\0", files, &expected)?;
    let mut hash = Sha256::new();
    hash.update(b"BRI_NATIVE_RUNTIME_V7\0");
    hash.update(base);
    hash.update(foliage);
    Ok(format!("{:x}", hash.finalize()))
}

/// Vehicles join the runtime identity: definitions and every declared
/// converted model, clip and texture byte.
pub fn with_vehicles(base: &str, root: &Path) -> Result<String> {
    let root = root.canonicalize()?;
    let path = contained(&root, "vehicles.json")?;
    let pack = bri_vehicles::Pack::load(&path)?;
    let mut files = BTreeMap::from([("vehicles.json".to_string(), path)]);
    let mut expected = BTreeMap::new();
    for asset in &pack.assets {
        // Identical converted files (e.g. the shared blank paint texture) are
        // referenced from several original add-on paths.
        if let Some(sha) = expected.get(&asset.path) {
            ensure!(sha == &asset.sha256, "Conflicting vehicle asset digests");
            continue;
        }
        ensure!(asset.path != "vehicles.json", "Reserved vehicle asset path");
        files.insert(asset.path.clone(), contained(&root, &asset.path)?);
        expected.insert(asset.path.clone(), asset.sha256.clone());
    }
    let vehicles = hash_files(b"BRI_VEHICLES_V1\0", files, &expected)?;
    let mut hash = Sha256::new();
    hash.update(b"BRI_NATIVE_RUNTIME_V8\0");
    hash.update(base);
    hash.update(vehicles);
    Ok(format!("{:x}", hash.finalize()))
}

/// The wrench event catalog: hosts and joiners must offer the same events.
pub fn with_events(base: &str, root: &Path) -> Result<String> {
    let catalog = bri_events::Catalog::load(contained(&root.canonicalize()?, "catalog.json")?)?;
    let mut hash = Sha256::new();
    hash.update(b"BRI_EVENTS_V1 ");
    hash.update(base);
    hash.update(catalog.fingerprint());
    Ok(format!("{:x}", hash.finalize()))
}

fn hash_files(
    domain: &[u8],
    files: BTreeMap<String, PathBuf>,
    expected: &BTreeMap<String, String>,
) -> Result<String> {
    hash_files_bounded(domain, files, expected, 512 * 1024 * 1024, u64::MAX)
}

fn hash_files_bounded(
    domain: &[u8],
    files: BTreeMap<String, PathBuf>,
    expected: &BTreeMap<String, String>,
    file_limit: u64,
    total_limit: u64,
) -> Result<String> {
    let mut hash = Sha256::new();
    hash.update(domain);
    let mut buffer = [0_u8; 65536];
    let mut total = 0_u64;
    for (name, path) in files {
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        let mut file = std::fs::File::open(&path)
            .with_context(|| format!("Reading native identity resource {}", path.display()))?;
        let size = file.metadata()?.len();
        ensure!(
            size <= file_limit,
            "Native identity resource exceeds byte budget: {name}"
        );
        total = total
            .checked_add(size)
            .context("Native identity byte budget overflow")?;
        ensure!(
            total <= total_limit,
            "Native identity total byte budget exceeded"
        );
        hash.update(size.to_le_bytes());
        let mut file_hash = Sha256::new();
        let mut read = 0u64;
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
            file_hash.update(&buffer[..n]);
            read += n as u64;
            ensure!(read <= size, "Native content changed while hashing: {name}");
        }
        ensure!(read == size, "Native content changed while hashing: {name}");
        if let Some(expected) = expected.get(&name) {
            ensure!(
                &format!("{:x}", file_hash.finalize()) == expected,
                "Native material hash mismatch: {name}"
            );
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_content::brick_materials::{Bundle, Image, Package, Print, SURFACES, Source};
    use bri_content::effects::{Library, Light};
    /// The four core tools as pack items, each with its own v20-style image,
    /// named like the stock pack (including Hammer's trailing space).
    fn core_tool_items() -> (
        BTreeMap<String, bri_weapons::Item>,
        BTreeMap<String, bri_weapons::Image>,
    ) {
        let mut items = BTreeMap::new();
        let mut images = BTreeMap::new();
        for (id, ui_name) in bri_weapons::CORE_TOOLS
            .into_iter()
            .zip(["Hammer ", "Wrench", "Printer", "Wand"])
        {
            let stem = id
                .trim_start_matches("v20.weapon.")
                .trim_end_matches("item");
            let image = format!("v20.image.{stem}image");
            images.insert(
                image.clone(),
                bri_weapons::Image {
                    id: image.clone(),
                    name: format!("{stem}Image"),
                    model: "source.dts".into(),
                    projectile: None,
                    mount_point: 0,
                    offset: [0.; 3],
                    eye_offset: [0.; 3],
                    source_rotation_degrees: [0.; 3],
                    correct_muzzle: false,
                    melee: true,
                    color: [1.; 4],
                    color_shift: false,
                    arm_ready: true,
                    casing: String::new(),
                    min_shot_ticks: 0,
                    states: vec![],
                },
            );
            items.insert(
                id.into(),
                bri_weapons::Item {
                    id: id.into(),
                    name: format!("{stem}Item"),
                    ui_name: ui_name.into(),
                    image,
                    model: "source.dts".into(),
                    icon: String::new(),
                    can_drop: true,
                    sport: false,
                },
            );
        }
        (items, images)
    }
    fn weapon_fixture() -> (PathBuf, bri_weapons::Pack) {
        let root = Fixture::new().root.join("weapons");
        std::fs::create_dir(&root).unwrap();
        let (items, images) = core_tool_items();
        let pack = bri_weapons::Pack {
            schema_version: bri_weapons::SCHEMA,
            id: "test.weapons".into(),
            items,
            images,
            projectiles: BTreeMap::new(),
            damage_types: BTreeMap::new(),
            explosions: BTreeMap::new(),
            definitions: vec![],
            resources: vec![bri_weapons::Resource {
                path: "original.dts".into(),
                sha256: "a".repeat(64),
                native_file: Some("shape.json".into()),
                diagnostics: vec![],
            }],
            diagnostics: vec![],
        };
        std::fs::write(root.join("shape.json"), b"native shape bytes").unwrap();
        write_weapons(&root, &pack);
        (root, pack)
    }
    fn write_weapons(root: &Path, pack: &bri_weapons::Pack) {
        std::fs::write(root.join("weapons.json"), serde_json::to_vec(pack).unwrap()).unwrap();
    }
    #[test]
    fn weapons_identity_hashes_native_bytes_and_rejects_catalog_replacement() {
        let (root, mut pack) = weapon_fixture();
        let before = WeaponContent::load(&root).unwrap();
        assert_eq!(before.item_choices.len(), 4);
        assert_eq!(
            before.extend_identity("base"),
            with_weapons("base", &root).unwrap()
        );
        assert_ne!(
            before.extend_identity("base"),
            before.extend_identity("other base")
        );
        // Original DTS provenance is deliberately not a native integrity checksum.
        std::fs::write(root.join("shape.json"), b"different native bytes").unwrap();
        let changed = WeaponContent::load(&root).unwrap();
        assert_ne!(
            before.extend_identity("base"),
            changed.extend_identity("base")
        );
        assert!(before.ensure_same(&changed).is_err());
        pack.diagnostics.push("definition metadata change".into());
        write_weapons(&root, &pack);
        assert_ne!(
            changed.extend_identity("base"),
            with_weapons("base", &root).unwrap()
        );
    }
    #[test]
    fn weapons_resources_are_contained_and_have_count_file_and_total_budgets() {
        let (root, mut pack) = weapon_fixture();
        pack.resources[0].native_file = Some("../shape.json".into());
        write_weapons(&root, &pack);
        assert!(WeaponContent::load(&root).is_err());
        pack.resources[0].native_file = Some("weapons.json".into());
        write_weapons(&root, &pack);
        assert!(WeaponContent::load(&root).is_err());
        pack.resources[0].native_file = Some("missing.json".into());
        write_weapons(&root, &pack);
        assert!(WeaponContent::load(&root).is_err());
        pack.resources[0].native_file = Some("shape.json".into());
        std::fs::File::create(root.join("shape.json"))
            .unwrap()
            .set_len(WEAPON_RESOURCE_LIMIT + 1)
            .unwrap();
        write_weapons(&root, &pack);
        assert!(
            WeaponContent::load(&root)
                .err()
                .unwrap()
                .to_string()
                .contains("Oversized")
        );
        let resource = pack.resources[0].clone();
        pack.resources = vec![resource.clone(); 4097];
        write_weapons(&root, &pack);
        assert!(
            WeaponContent::load(&root)
                .err()
                .unwrap()
                .to_string()
                .contains("resource budget")
        );
        pack.resources.clear();
        for index in 0..9 {
            let name = format!("large-{index}.json");
            std::fs::File::create(root.join(&name))
                .unwrap()
                .set_len(WEAPON_RESOURCE_LIMIT)
                .unwrap();
            let mut resource = resource.clone();
            resource.native_file = Some(name);
            pack.resources.push(resource);
        }
        write_weapons(&root, &pack);
        assert!(
            WeaponContent::load(&root)
                .err()
                .unwrap()
                .to_string()
                .contains("total byte budget")
        );
        // Tests leave only bounded metadata fixtures, not hundreds of MiB.
        for resource in &pack.resources {
            std::fs::File::create(root.join(resource.native_file.as_ref().unwrap())).unwrap();
        }
        std::fs::File::create(root.join("shape.json")).unwrap();
    }
    #[test]
    fn weapon_alias_resolution_preserves_unknown_names_and_source_records() {
        let (root, _) = weapon_fixture();
        let content = WeaponContent::load(&root).unwrap();
        let mut world = bri_world::World::new("test".into(), "map".into(), vec![[1.; 4]]);
        for (id, name) in [(1, " Hammer "), (2, "unknown addon")] {
            let mut brick =
                bri_world::Brick::new(bri_world::ContentRef::Resolved("brick".into()), [0.; 3], 0);
            brick.item_spawn.item = Some(bri_world::ContentRef::Unresolved {
                namespace: "item_ui".into(),
                name: name.into(),
            });
            brick.source_records.push(bri_world::SourceRecord {
                line: 1,
                text: "+-ITEM untouched".into(),
                diagnostic: None,
            });
            world.bricks.insert(id, brick);
        }
        let original = world.bricks[&1].source_records.clone();
        assert_eq!(content.resolve_world_items(&mut world).unwrap(), 1);
        assert_eq!(
            world.bricks[&1].item_spawn.item,
            Some(bri_world::ContentRef::Resolved(
                bri_weapons::CORE_TOOLS[0].into()
            ))
        );
        assert_eq!(world.bricks[&1].source_records, original);
        assert!(matches!(
            world.bricks[&2].item_spawn.item,
            Some(bri_world::ContentRef::Unresolved { .. })
        ));
        assert_eq!(content.resolve_world_items(&mut world).unwrap(), 1);
    }
    #[test]
    #[ignore = "requires generated native weapons-pack-008; no window or audio"]
    fn native_weapons_pack_identity_and_all_21_choices() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/weapons-pack-008");
        let content = WeaponContent::load(&root).unwrap();
        // 17 weapons plus the four core tools, which are v20 images too.
        assert_eq!(content.pack.items.len(), 21);
        assert_eq!(content.item_choices.len(), 21);
        assert_eq!(content.extend_identity("base").len(), 64);
        content
            .ensure_same(&WeaponContent::load(&root).unwrap())
            .unwrap();
        let mut catalog = bri_sim::session::ToolCatalog::default();
        catalog
            .install_items(content.item_choices.into_iter().map(|(id, _)| id))
            .unwrap();
        assert_eq!(catalog.items.len(), 21);
    }
    struct Fixture {
        root: PathBuf,
        bundle: Bundle,
        effects: Library,
    }
    fn physics_fixture() -> (PathBuf, WeaponContent, serde_json::Value, serde_json::Value) {
        let (root, _) = weapon_fixture();
        let weapons = WeaponContent::load(&root).unwrap();
        std::fs::write(root.join("texture.png"), b"native texture").unwrap();
        let bounds = serde_json::json!({"min":[-0.2,-0.3,-0.4],"max":[0.4,0.6,0.8]});
        let physics = serde_json::json!({"schema_version":1,"items":weapons.item_choices.iter().map(|(id,_)|(id.clone(),bounds.clone())).collect::<BTreeMap<_,_>>()});
        let manifest = serde_json::json!({"schema_version":2,"weapons_sha256":weapons.manifest_sha256,
            "item_physics_sha256":"",
            "items":weapons.item_choices.iter().map(|(id,_)|(id.clone(),serde_json::json!({"model":"source.dts"}))).collect::<BTreeMap<_,_>>(),
            "models":{"source.dts":{"file":"shape.json","sha256":format!("{:x}",Sha256::digest(b"native shape bytes")),"source_sha256":"b".repeat(64),"bounds_min":bounds["min"],"bounds_max":bounds["max"]}},
            "textures":{"texture":{"file":"texture.png","sha256":format!("{:x}",Sha256::digest(b"native texture"))}}});
        write_physics(&root, &manifest, &physics);
        (root, weapons, manifest, physics)
    }
    fn write_physics(root: &Path, manifest: &serde_json::Value, physics: &serde_json::Value) {
        let bytes = serde_json::to_vec(physics).unwrap();
        let mut manifest = manifest.clone();
        manifest["item_physics_sha256"] =
            serde_json::json!(format!("{:x}", Sha256::digest(&bytes)));
        std::fs::write(root.join("item-physics.json"), bytes).unwrap();
        std::fs::write(
            root.join("presentation.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
    }
    #[test]
    fn item_physics_pins_bounds_catalog_and_resources_into_identity_nine() {
        let (root, weapons, mut manifest, mut physics) = physics_fixture();
        let before = ItemPhysicsContent::load(&root, &weapons).unwrap();
        assert_eq!(before.bounds.len(), 4);
        before
            .ensure_same(&ItemPhysicsContent::load(&root, &weapons).unwrap())
            .unwrap();
        assert_ne!(
            before.extend_identity("base"),
            weapons.extend_identity("base")
        );
        // A changed bound must be re-certified by the matching model metadata.
        physics["items"][bri_weapons::CORE_TOOLS[0]]["min"][0] = serde_json::json!(-0.5);
        write_physics(&root, &manifest, &physics);
        assert!(
            ItemPhysicsContent::load(&root, &weapons)
                .err()
                .unwrap()
                .to_string()
                .contains("disagree")
        );
        for item in physics["items"].as_object_mut().unwrap().values_mut() {
            item["min"][0] = serde_json::json!(-0.5);
        }
        manifest["models"]["source.dts"]["bounds_min"][0] = serde_json::json!(-0.5);
        write_physics(&root, &manifest, &physics);
        let after = ItemPhysicsContent::load(&root, &weapons).unwrap();
        assert_ne!(
            before.extend_identity("base"),
            after.extend_identity("base")
        );
        assert!(before.ensure_same(&after).is_err());
        std::fs::write(root.join("shape.json"), b"corrupted native shape").unwrap();
        assert!(ItemPhysicsContent::load(&root, &weapons).is_err());
        std::fs::write(root.join("shape.json"), b"native shape bytes").unwrap();
        std::fs::write(root.join("texture.png"), b"corrupted texture").unwrap();
        assert!(ItemPhysicsContent::load(&root, &weapons).is_err());
    }
    #[test]
    fn item_physics_rejects_stale_pins_incomplete_catalog_and_invalid_bounds() {
        let (root, weapons, manifest, physics) = physics_fixture();
        let mut invalid = manifest.clone();
        invalid["weapons_sha256"] = serde_json::json!("0".repeat(64));
        write_physics(&root, &invalid, &physics);
        assert!(
            ItemPhysicsContent::load(&root, &weapons)
                .err()
                .unwrap()
                .to_string()
                .contains("weapon checksum")
        );
        write_physics(&root, &manifest, &physics);
        std::fs::write(
            root.join("item-physics.json"),
            b"{\"schema_version\":1,\"items\":{}}",
        )
        .unwrap();
        assert!(
            ItemPhysicsContent::load(&root, &weapons)
                .err()
                .unwrap()
                .to_string()
                .contains("physics checksum")
        );
        for change in 0..4 {
            let mut invalid = physics.clone();
            match change {
                0 => {
                    invalid["items"]
                        .as_object_mut()
                        .unwrap()
                        .remove(bri_weapons::CORE_TOOLS[0]);
                }
                1 => {
                    let value = invalid["items"]
                        .as_object_mut()
                        .unwrap()
                        .remove(bri_weapons::CORE_TOOLS[0])
                        .unwrap();
                    invalid["items"]["unknown"] = value;
                }
                2 => {
                    invalid["items"][bri_weapons::CORE_TOOLS[0]]["min"][0] = serde_json::json!(2000)
                }
                _ => invalid["schema_version"] = serde_json::json!(2),
            }
            write_physics(&root, &manifest, &invalid);
            assert!(ItemPhysicsContent::load(&root, &weapons).is_err());
        }
    }
    #[test]
    fn item_physics_rejects_escape_oversize_and_conflicting_resource_pins() {
        let (root, weapons, manifest, physics) = physics_fixture();
        for name in ["../escape.json", "presentation.json", "missing.json"] {
            let mut invalid = manifest.clone();
            invalid["models"]["source.dts"]["file"] = serde_json::json!(name);
            write_physics(&root, &invalid, &physics);
            assert!(ItemPhysicsContent::load(&root, &weapons).is_err());
        }
        let mut invalid = manifest.clone();
        invalid["textures"]["texture"]["file"] = serde_json::json!("shape.json");
        write_physics(&root, &invalid, &physics);
        assert!(
            ItemPhysicsContent::load(&root, &weapons)
                .err()
                .unwrap()
                .to_string()
                .contains("Conflicting")
        );
        write_physics(&root, &manifest, &physics);
        std::fs::File::create(root.join("shape.json"))
            .unwrap()
            .set_len(WEAPON_RESOURCE_LIMIT + 1)
            .unwrap();
        assert!(
            ItemPhysicsContent::load(&root, &weapons)
                .err()
                .unwrap()
                .to_string()
                .contains("byte budget")
        );
        std::fs::File::create(root.join("shape.json")).unwrap();
        std::fs::File::create(root.join("item-physics.json"))
            .unwrap()
            .set_len(2 * 1024 * 1024 + 1)
            .unwrap();
        assert!(
            ItemPhysicsContent::load(&root, &weapons)
                .err()
                .unwrap()
                .to_string()
                .contains("Oversized")
        );
        std::fs::File::create(root.join("item-physics.json")).unwrap();
    }
    #[test]
    #[ignore = "requires native weapons/presentation pack003; no renderer or original readers"]
    fn native_item_physics_covers_all_21_and_pins_authored_bounds() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let weapons = WeaponContent::load(&root.join("weapons-pack-008")).unwrap();
        let physics =
            ItemPhysicsContent::load(&root.join("item-presentation-pack-009"), &weapons).unwrap();
        assert_eq!(physics.bounds.len(), 21);
        assert_eq!(
            physics
                .extend_identity(&weapons.extend_identity("base"))
                .len(),
            64
        );
        for bounds in physics.bounds.values() {
            bounds.validate().unwrap();
        }
    }
    impl Fixture {
        fn new() -> Self {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root =
                std::env::temp_dir().join(format!("bri-identity-{}-{unique}", std::process::id()));
            for dir in ["catalog", "converted", "maps", "materials", "effects"] {
                std::fs::create_dir_all(root.join(dir)).unwrap();
            }
            for name in ["stock-catalog.json", "native-collisions.json"] {
                std::fs::write(root.join("catalog").join(name), b"{}").unwrap();
            }
            std::fs::write(
                root.join("catalog/catalog-audit.json"),
                br#"{"resolved_meshes":[{"id":"brick/a","native_mesh":"a.brick.json"}]}"#,
            )
            .unwrap();
            std::fs::write(root.join("converted/a.brick.json"), b"mesh").unwrap();
            std::fs::write(root.join("maps/map.json"), b"map").unwrap();
            let image = |name: &str| {
                let sha = format!("{:x}", Sha256::digest(b"image"));
                Image {
                    path: format!("{name}.png"),
                    width: 1,
                    height: 1,
                    sha256: sha.clone(),
                    source: Source {
                        path: format!("{name}.png"),
                        archive: None,
                        sha256: sha,
                    },
                }
            };
            let bundle = Bundle {
                schema_version: 1,
                surfaces: SURFACES.into_iter().map(|s| (s.into(), image(s))).collect(),
                prints: vec![Print {
                    id: "print/print_letters_default/a".into(),
                    name: "A".into(),
                    aspect: "Letters".into(),
                    package: "Print_Letters_Default".into(),
                    aliases: vec!["Letters/A".into()],
                    diffuse: image("letter"),
                    icon: image("icon"),
                }],
                packages: vec![Package {
                    name: "Print_Letters_Default".into(),
                    archive: "Add-Ons/Print_Letters_Default.zip".into(),
                    archive_sha256: "a".repeat(64),
                    default_list_line: 1,
                }],
                evidence: vec![],
                excluded_installed_packages: vec![],
                warnings: vec![],
            };
            for image in bundle.images() {
                std::fs::write(root.join("materials").join(&image.path), b"image").unwrap();
            }
            let effects = Library {
                schema_version: 1,
                particles: vec![],
                emitters: vec![],
                lights: vec![Light {
                    id: "light/test".into(),
                    name: "Test".into(),
                    enabled: true,
                    color: [1.; 3],
                    brightness: 1.,
                    radius: 10.,
                    color_curves: None,
                    brightness_curve: None,
                    radius_curve: None,
                    flare: None,
                }],
                textures: [("texture/test".into(), "particle.png".into())].into(),
            };
            std::fs::write(root.join("effects/particle.png"), b"particle").unwrap();
            let f = Self {
                root,
                bundle,
                effects,
            };
            f.write_indexes();
            f
        }
        fn write_indexes(&self) {
            std::fs::write(
                self.root.join("materials/brick-materials.json"),
                serde_json::to_vec(&self.bundle).unwrap(),
            )
            .unwrap();
            std::fs::write(
                self.root.join("effects/effects.json"),
                serde_json::to_vec(&self.effects).unwrap(),
            )
            .unwrap();
        }
        fn full(&self) -> Result<String> {
            fingerprint_full(
                &self.root.join("catalog"),
                &self.root.join("converted"),
                &self.root.join("maps"),
                &self.root.join("materials"),
                &self.root.join("effects"),
            )
        }
        fn legacy(&self) -> String {
            fingerprint(
                &self.root.join("catalog"),
                &self.root.join("converted"),
                &self.root.join("maps"),
            )
            .unwrap()
        }
    }
    #[test]
    fn print_effect_resources_and_effect_parameters_change_peer_identity() {
        let mut f = Fixture::new();
        let original = f.full().unwrap();
        let legacy = f.legacy();
        assert_ne!(original, legacy);
        assert_eq!(original, Fixture::new().full().unwrap());
        let p = &mut f.bundle.prints[0].diffuse;
        std::fs::write(f.root.join("materials").join(&p.path), b"changed print").unwrap();
        p.sha256 = format!("{:x}", Sha256::digest(b"changed print"));
        p.source.sha256 = p.sha256.clone();
        f.write_indexes();
        let changed_print = f.full().unwrap();
        assert_ne!(original, changed_print);
        assert_eq!(legacy, f.legacy());
        f.effects.lights[0].brightness = 2.;
        f.write_indexes();
        let changed_effect = f.full().unwrap();
        assert_ne!(changed_print, changed_effect);
        std::fs::write(f.root.join("effects/particle.png"), b"changed effect image").unwrap();
        assert_ne!(changed_effect, f.full().unwrap());
    }
    #[test]
    fn rejects_corrupt_materials_and_escape_references() {
        let mut f = Fixture::new();
        std::fs::write(f.root.join("materials/icon.png"), b"corrupt").unwrap();
        assert!(f.full().unwrap_err().to_string().contains("hash mismatch"));
        std::fs::write(f.root.join("materials/icon.png"), b"image").unwrap();
        f.bundle.prints[0].icon.path = "../escape.png".into();
        f.write_indexes();
        assert!(f.full().is_err());
        assert!(contained(&f.root, "../escape").is_err());
    }
    #[test]
    fn full_identity_includes_mesh_binding_table() {
        let f = Fixture::new();
        let before = f.full().unwrap();
        let old = f.legacy();
        std::fs::write(
            f.root.join("catalog/catalog-audit.json"),
            br#"{"resolved_meshes":[{"id":"brick/b","native_mesh":"a.brick.json"}]}"#,
        )
        .unwrap();
        assert_ne!(before, f.full().unwrap());
        assert_eq!(old, f.legacy());
    }

    #[test]
    fn foliage_identity_covers_placement_and_original_texture_bytes() {
        let f = Fixture::new();
        let root = f.root.join("foliage");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("grass.png"), b"grass").unwrap();
        let mut manifest = serde_json::json!({"schema_version":1,
            "textures":[{"path":"grass.png","sha256":format!("{:x}",Sha256::digest(b"grass"))}],
            "definitions":[{"count":40000}]});
        let write = |m: &serde_json::Value| {
            std::fs::write(root.join("foliage.json"), serde_json::to_vec(m).unwrap()).unwrap()
        };
        write(&manifest);
        let before = with_foliage("base", &root).unwrap();
        manifest["definitions"][0]["count"] = serde_json::json!(100);
        write(&manifest);
        assert_ne!(before, with_foliage("base", &root).unwrap());
        std::fs::write(root.join("grass.png"), b"corrupt").unwrap();
        assert!(with_foliage("base", &root).is_err());
    }

    #[test]
    fn weather_identity_covers_authored_values_and_texture_integrity() {
        let f = Fixture::new();
        let root = f.root.join("weather");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("rain.png"), b"rain").unwrap();
        let mut manifest = serde_json::json!({"schema_version":1,
            "textures":{"rain":{"file":"rain.png","sha256":format!("{:x}",Sha256::digest(b"rain"))}},
            "placements":[{"drops":5000}]});
        let write = |m: &serde_json::Value| {
            std::fs::write(root.join("weather.json"), serde_json::to_vec(m).unwrap()).unwrap()
        };
        write(&manifest);
        let before = with_weather("base", &root).unwrap();
        manifest["placements"][0]["drops"] = serde_json::json!(100);
        write(&manifest);
        assert_ne!(before, with_weather("base", &root).unwrap());
        std::fs::write(root.join("rain.png"), b"corrupt").unwrap();
        assert!(with_weather("base", &root).is_err());
    }

    #[test]
    fn audio_identity_covers_bindings_and_validates_clip_bytes() {
        let f = Fixture::new();
        let root = f.root.join("audio");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("clip.wav"), b"original clip").unwrap();
        let mut manifest = serde_json::json!({"schema_version":1,
            "clips":[{"file":"clip.wav","sha256":format!("{:x}",Sha256::digest(b"original clip"))}],
            "triggers":[{"trigger":"player.jump","sound":"jump"}]});
        let write = |m: &serde_json::Value| {
            std::fs::write(root.join("manifest.json"), serde_json::to_vec(m).unwrap()).unwrap()
        };
        write(&manifest);
        let before = with_audio("base", &root).unwrap();
        manifest["triggers"][0]["sound"] = serde_json::json!("other");
        write(&manifest);
        assert_ne!(before, with_audio("base", &root).unwrap());
        std::fs::write(root.join("clip.wav"), b"corrupt").unwrap();
        assert!(with_audio("base", &root).is_err());
        std::fs::write(root.join("clip.wav"), b"original clip").unwrap();
        manifest["clips"][0]["file"] = serde_json::json!("../escape.wav");
        write(&manifest);
        assert!(with_audio("base", &root).is_err());
    }

    #[test]
    fn runtime_effect_identity_includes_bindings_and_validates_every_resource() {
        let f = Fixture::new();
        let root = f.root.join("effects");
        let mut manifest = serde_json::json!({"schema_version":1,
            "library_sha256":format!("{:x}",Sha256::digest(std::fs::read(root.join("effects.json")).unwrap())),
            "textures":{"texture/test":{"file":"particle.png","sha256":format!("{:x}",Sha256::digest(b"particle"))}},
            "bindings":[], "composites":[]});
        let write = |m: &serde_json::Value| {
            std::fs::write(root.join("manifest.json"), serde_json::to_vec(m).unwrap()).unwrap()
        };
        write(&manifest);
        let base = f.full().unwrap();
        let original = with_effects_runtime(&base, &root).unwrap();
        manifest["bindings"] = serde_json::json!([{"owner":"test","resource":"light/test"}]);
        write(&manifest);
        assert_ne!(original, with_effects_runtime(&base, &root).unwrap());
        std::fs::write(root.join("particle.png"), b"corrupted").unwrap();
        assert!(with_effects_runtime(&base, &root).is_err());
        std::fs::write(root.join("particle.png"), b"particle").unwrap();
        manifest["library_sha256"] = serde_json::json!("0".repeat(64));
        write(&manifest);
        assert!(with_effects_runtime(&base, &root).is_err());
    }

    #[test]
    #[ignore = "requires full generated native content"]
    fn runtime_identity_includes_avatar_catalog_and_rejects_changed_image_bytes() -> Result<()> {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()?;
        let content = workspace.join("content");
        let original = content.join("avatar-pack-001");
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let copy = workspace
            .join("target")
            .join(format!("avatar-identity-{stamp}"));
        std::fs::create_dir(&copy)?;
        for entry in std::fs::read_dir(&original)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                std::fs::copy(entry.path(), copy.join(entry.file_name()))?;
            }
        }
        let fingerprint = |avatar: &Path| {
            fingerprint_runtime(
                &content.join("stock-catalog-004"),
                &content.join("maps-pass-007"),
                &content.join("map-bundle-005"),
                &content.join("brick-materials-001"),
                &content.join("effects-pass-004"),
                avatar,
            )
        };
        let initial = fingerprint(&original)?;
        assert_eq!(initial, fingerprint(&copy)?);
        let mut package: bri_content::avatar::Package =
            serde_json::from_slice(&std::fs::read(copy.join("avatar.json"))?)?;
        package
            .defaults
            .colors
            .insert("head".into(), [0.25, 0.5, 0.75, 1.0]);
        std::fs::write(
            copy.join("avatar.json"),
            serde_json::to_vec_pretty(&package)?,
        )?;
        assert_ne!(initial, fingerprint(&copy)?);
        let image = package.textures.values().next().unwrap();
        std::fs::write(copy.join(&image.file), b"changed PNG bytes")?;
        assert!(
            fingerprint(&copy)
                .unwrap_err()
                .to_string()
                .contains("hash mismatch")
        );
        // Only the isolated ignored target directory is mutated; source package
        // remains byte-identical and the unchanged identity is checked again.
        assert_eq!(initial, fingerprint(&original)?);
        Ok(())
    }
}
