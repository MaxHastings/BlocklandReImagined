use anyhow::{Context, Result, ensure};
use bri_content::effects::Library;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::Read,
    path::Path,
    sync::Arc,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextureRecord {
    pub file: String,
    pub sha256: String,
    pub width: u32,
    pub height: u32,
}
/// Literal native resource relationship, not executable source behavior.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Binding {
    pub owner: String,
    pub field: String,
    pub resource: String,
    pub source: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Composite {
    pub id: String,
    pub lifetime: f32,
    pub emitters: Vec<String>,
    pub light: Option<String>,
    pub burst: Option<(String, u32, f32)>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: u32,
    pub library_sha256: String,
    pub textures: BTreeMap<String, TextureRecord>,
    /// Emitter-level useInvAlpha overrides found in source.
    pub emitter_alpha: BTreeMap<String, bool>,
    pub bindings: Vec<Binding>,
    /// Defaulted: a generated effects pack may predate composites.
    #[serde(default)]
    pub composites: Vec<Composite>,
    pub unresolved: Vec<String>,
}
pub struct TextureImage {
    pub id: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}
pub struct EffectsPack {
    pub library: Library,
    pub manifest: Manifest,
    pub textures: Vec<TextureImage>,
    pub(crate) particle_index: BTreeMap<String, usize>,
    pub(crate) emitter_index: BTreeMap<String, usize>,
    pub(crate) light_index: BTreeMap<String, usize>,
    pub(crate) texture_index: BTreeMap<String, usize>,
}
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn bounded(root: &Path, name: &str, limit: u64) -> Result<Vec<u8>> {
    ensure!(
        !name.is_empty() && !name.contains(['/', '\\', ':']) && name != "." && name != "..",
        "Unsafe effects filename: {name}"
    );
    let path = root
        .join(name)
        .canonicalize()
        .with_context(|| format!("Missing effects file {name}"))?;
    ensure!(path.starts_with(root), "Effects file escapes pack: {name}");
    let file = File::open(path)?;
    ensure!(
        file.metadata()?.is_file() && file.metadata()?.len() <= limit,
        "Effects file exceeds {limit} bytes: {name}"
    );
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "Effects file grew beyond limit: {name}"
    );
    Ok(bytes)
}
impl EffectsPack {
    pub fn load(root: impl AsRef<Path>) -> Result<Arc<Self>> {
        let root = root.as_ref().canonicalize()?;
        let manifest: Manifest =
            serde_json::from_slice(&bounded(&root, "manifest.json", 8 << 20)?)?;
        ensure!(
            manifest.schema_version == 1,
            "Unsupported effects runtime schema"
        );
        let bytes = bounded(&root, "effects.json", 16 << 20)?;
        ensure!(
            digest(&bytes) == manifest.library_sha256,
            "Effects library checksum mismatch"
        );
        let library: Library = serde_json::from_slice(&bytes)?;
        library.validate()?;
        ensure!(
            manifest.textures.len() == library.textures.len(),
            "Texture manifest/library mismatch"
        );
        let mut textures = Vec::new();
        let mut decoded = 0u64;
        for (id, file) in &library.textures {
            let record = manifest
                .textures
                .get(id)
                .context("Texture omitted from manifest")?;
            ensure!(
                &record.file == file
                    && record.width > 0
                    && record.height > 0
                    && record.width <= 4096
                    && record.height <= 4096,
                "Invalid texture metadata: {id}"
            );
            decoded += u64::from(record.width) * u64::from(record.height) * 4;
            ensure!(
                decoded <= 256 << 20,
                "Effects decoded image budget exceeded"
            );
            let bytes = bounded(&root, file, 32 << 20)?;
            ensure!(
                digest(&bytes) == record.sha256,
                "Texture checksum mismatch: {id}"
            );
            let reader =
                image::ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format()?;
            ensure!(
                reader.into_dimensions()? == (record.width, record.height),
                "Texture dimensions mismatch: {id}"
            );
            let rgba = image::load_from_memory(&bytes)?.to_rgba8().into_raw();
            textures.push(TextureImage {
                id: id.clone(),
                width: record.width,
                height: record.height,
                rgba,
            });
        }
        Self::from_parts(library, manifest, textures)
    }
    /// For generated native fixtures/in-memory hosts. Also validates all cross references.
    pub fn from_parts(
        library: Library,
        manifest: Manifest,
        textures: Vec<TextureImage>,
    ) -> Result<Arc<Self>> {
        library.validate()?;
        ensure!(
            manifest.schema_version == 1 && textures.len() == library.textures.len(),
            "Invalid native effects pack"
        );
        let texture_index: BTreeMap<_, _> = textures
            .iter()
            .enumerate()
            .map(|(i, t)| (t.id.clone(), i))
            .collect();
        ensure!(
            texture_index.len() == textures.len(),
            "Duplicate texture IDs"
        );
        for t in &textures {
            ensure!(
                library.textures.contains_key(&t.id)
                    && t.width > 0
                    && t.height > 0
                    && t.width <= 4096
                    && t.height <= 4096
                    && t.rgba.len() as u64 == u64::from(t.width) * u64::from(t.height) * 4,
                "Invalid decoded texture: {}",
                t.id
            );
        }
        let particle_index = library
            .particles
            .iter()
            .enumerate()
            .map(|(i, p)| (p.id.clone(), i))
            .collect();
        let emitter_index: BTreeMap<_, _> = library
            .emitters
            .iter()
            .enumerate()
            .map(|(i, e)| (e.id.clone(), i))
            .collect();
        ensure!(
            manifest
                .emitter_alpha
                .keys()
                .all(|id| emitter_index.contains_key(id)),
            "Unknown emitter alpha override"
        );
        let light_index: BTreeMap<_, _> = library
            .lights
            .iter()
            .enumerate()
            .map(|(i, l)| (l.id.clone(), i))
            .collect();
        ensure!(
            manifest.composites.len() <= 4096 && manifest.bindings.len() <= 65536,
            "Effects extension budget exceeded"
        );
        let composite_ids: BTreeSet<_> = manifest.composites.iter().map(|c| &c.id).collect();
        ensure!(
            composite_ids.len() == manifest.composites.len(),
            "Duplicate composite IDs"
        );
        for b in &manifest.bindings {
            ensure!(
                !b.owner.is_empty()
                    && !b.field.is_empty()
                    && (emitter_index.contains_key(&b.resource)
                        || light_index.contains_key(&b.resource)
                        || composite_ids.contains(&b.resource)),
                "Invalid effect relationship {}.{}",
                b.owner,
                b.field
            );
        }
        for c in &manifest.composites {
            ensure!(
                c.lifetime.is_finite()
                    && c.lifetime > 0.
                    && c.lifetime <= 3600.
                    && c.emitters.len() <= 32
                    && c.emitters.iter().all(|id| emitter_index.contains_key(id)),
                "Invalid composite {}",
                c.id
            );
            if let Some((id, count, radius)) = &c.burst {
                ensure!(
                    emitter_index.contains_key(id)
                        && *count <= 32768
                        && radius.is_finite()
                        && *radius >= 0.,
                    "Invalid composite burst"
                );
            }
            if let Some(id) = &c.light {
                ensure!(
                    library.lights.iter().any(|l| &l.id == id),
                    "Unknown composite light"
                );
            }
        }
        Ok(Arc::new(Self {
            library,
            manifest,
            textures,
            particle_index,
            emitter_index,
            light_index,
            texture_index,
        }))
    }
    pub fn emitter_ids(&self) -> impl Iterator<Item = &str> {
        self.emitter_index.keys().map(String::as_str)
    }
}
