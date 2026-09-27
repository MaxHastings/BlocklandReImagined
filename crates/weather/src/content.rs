use anyhow::{Context, Result, ensure};
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
pub struct Definition {
    pub id: String,
    pub drop_texture: String,
    pub splash_texture: Option<String>,
    pub drop_radius: f32,
    pub splash_radius: f32,
    pub true_billboards: bool,
    pub splash_seconds: f32,
    pub drop_animation_seconds: f32,
    pub animate_splashes: bool,
    pub drops_per_side: u32,
    pub splashes_per_side: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Placement {
    pub id: String,
    pub map_id: String,
    pub definition: String,
    pub position: [f32; 3],
    pub drops: u32,
    pub width: f32,
    pub height: f32,
    /// Original numeric fields, in distance per legacy tick.
    pub speed_per_tick: [f32; 2],
    pub mass: [f32; 2],
    pub turbulence_amplitude: f32,
    pub turbulence_radians_per_tick: f32,
    pub use_turbulence: bool,
    pub rotate_with_camera_velocity: bool,
    pub collision: bool,
    pub follow_camera: bool,
    pub use_wind: bool,
    /// Original map wind converted to native Y-up; not implicitly applied by runtime.
    pub authored_sky_wind: [f32; 3],
    /// Engine-family negative sky wind / tick_seconds, for explicit host selection.
    pub reference_wind_velocity: [f32; 3],
    pub original_fields: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextureRecord {
    pub file: String,
    pub sha256: String,
    pub rgba_sha256: String,
    pub width: u32,
    pub height: u32,
    pub source_paths: Vec<String>,
    pub alpha_policy: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceRecord {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
    pub kind: String,
    pub copy_file: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WeatherManifest {
    pub schema_version: u32,
    /// Explicit corroborating-family conversion; not an inferred seconds field.
    pub legacy_tick_seconds: f32,
    pub definitions: Vec<Definition>,
    pub placements: Vec<Placement>,
    pub textures: BTreeMap<String, TextureRecord>,
    pub sources: Vec<SourceRecord>,
    pub assumptions: Vec<String>,
}
pub struct WeatherTexture {
    pub id: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}
pub struct WeatherPack {
    pub manifest: WeatherManifest,
    pub textures: Vec<WeatherTexture>,
    pub(crate) texture_index: BTreeMap<String, usize>,
    pub(crate) definition_index: BTreeMap<String, usize>,
}
pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn safe_file(root: &Path, name: &str, limit: u64) -> Result<Vec<u8>> {
    ensure!(
        !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':']),
        "Unsafe native weather filename {name}"
    );
    let path = root
        .join(name)
        .canonicalize()
        .with_context(|| format!("Missing weather file {name}"))?;
    ensure!(path.starts_with(root), "Weather path escapes pack");
    let file = File::open(path)?;
    ensure!(
        file.metadata()?.is_file() && file.metadata()?.len() <= limit,
        "Weather input exceeds bound"
    );
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "Growing weather input exceeds bound"
    );
    Ok(bytes)
}
impl WeatherManifest {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1
                && self.legacy_tick_seconds.is_finite()
                && (0.001..=1.).contains(&self.legacy_tick_seconds),
            "Unsupported weather schema/timing"
        );
        ensure!(
            self.definitions.len() <= 128
                && self.placements.len() <= 512
                && self.textures.len() <= 256
                && self.sources.len() <= 4096,
            "Weather manifest budget exceeded"
        );
        let mut ids = BTreeSet::new();
        for d in &self.definitions {
            ensure!(
                !d.id.is_empty()
                    && ids.insert(&d.id)
                    && self.textures.contains_key(&d.drop_texture)
                    && d.splash_texture
                        .as_ref()
                        .is_none_or(|id| self.textures.contains_key(id)),
                "Invalid weather definition/reference"
            );
            ensure!(
                (1..=16).contains(&d.drops_per_side)
                    && (1..=16).contains(&d.splashes_per_side)
                    && [
                        d.drop_radius,
                        d.splash_radius,
                        d.splash_seconds,
                        d.drop_animation_seconds
                    ]
                    .iter()
                    .all(|v| v.is_finite() && *v >= 0. && *v <= 3600.)
                    && d.drop_radius > 0.,
                "Invalid weather size/animation"
            );
            for (id, side) in [
                (Some(&d.drop_texture), d.drops_per_side),
                (d.splash_texture.as_ref(), d.splashes_per_side),
            ] {
                if let Some(id) = id {
                    let t = &self.textures[id];
                    ensure!(
                        t.width.is_multiple_of(side)
                            && t.height.is_multiple_of(side)
                            && t.width >= side
                            && t.height >= side,
                        "Weather atlas dimensions incompatible"
                    );
                }
            }
        }
        let mut placement_ids = BTreeSet::new();
        for p in &self.placements {
            ensure!(
                !p.map_id.is_empty()
                    && !p.id.is_empty()
                    && placement_ids.insert(&p.id)
                    && ids.contains(&p.definition),
                "Invalid weather placement identity"
            );
            ensure!(
                p.drops <= 65536
                    && p.width.is_finite()
                    && p.height.is_finite()
                    && (0.01..=10000.).contains(&p.width)
                    && (0.01..=10000.).contains(&p.height)
                    && p.position
                        .iter()
                        .chain(&p.authored_sky_wind)
                        .chain(&p.reference_wind_velocity)
                        .all(|v| v.is_finite()),
                "Invalid weather volume"
            );
            ensure!(
                p.speed_per_tick[0] > 0.
                    && p.speed_per_tick[0] <= p.speed_per_tick[1]
                    && p.speed_per_tick[1] <= 1000.
                    && p.mass[0] > 0.
                    && p.mass[0] <= p.mass[1]
                    && p.mass[1] <= 10000.
                    && p.speed_per_tick
                        .iter()
                        .chain(&p.mass)
                        .all(|v| v.is_finite())
                    && p.turbulence_amplitude.is_finite()
                    && (0.0..=100.).contains(&p.turbulence_amplitude)
                    && p.turbulence_radians_per_tick.is_finite()
                    && p.turbulence_radians_per_tick.abs() <= 100.,
                "Invalid weather motion"
            );
        }
        let mut decoded = 0u64;
        for t in self.textures.values() {
            ensure!(
                t.width > 0
                    && t.height > 0
                    && t.width <= 4096
                    && t.height <= 4096
                    && !t.file.contains(['/', '\\', ':']),
                "Invalid weather image"
            );
            decoded += u64::from(t.width) * u64::from(t.height) * 4;
        }
        ensure!(decoded <= 128 << 20, "Weather decoded budget exceeded");
        Ok(())
    }
}
impl WeatherPack {
    pub fn load(root: impl AsRef<Path>) -> Result<Arc<Self>> {
        let root = root.as_ref().canonicalize()?;
        let manifest: WeatherManifest =
            serde_json::from_slice(&safe_file(&root, "weather.json", 8 << 20)?)?;
        manifest.validate()?;
        let mut textures = Vec::new();
        for (id, t) in &manifest.textures {
            let bytes = safe_file(&root, &t.file, 32 << 20)?;
            ensure!(
                sha256(&bytes) == t.sha256,
                "Weather texture checksum mismatch: {id}"
            );
            let reader =
                image::ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format()?;
            ensure!(
                reader.into_dimensions()? == (t.width, t.height),
                "Weather image dimensions mismatch"
            );
            let rgba = image::load_from_memory(&bytes)?.to_rgba8().into_raw();
            ensure!(
                sha256(&rgba) == t.rgba_sha256,
                "Weather decoded pixel checksum mismatch"
            );
            textures.push(WeatherTexture {
                id: id.clone(),
                width: t.width,
                height: t.height,
                rgba,
            });
        }
        Self::from_parts(manifest, textures)
    }
    pub fn from_parts(
        manifest: WeatherManifest,
        textures: Vec<WeatherTexture>,
    ) -> Result<Arc<Self>> {
        manifest.validate()?;
        let texture_index: BTreeMap<_, _> = textures
            .iter()
            .enumerate()
            .map(|(i, t)| (t.id.clone(), i))
            .collect();
        ensure!(
            texture_index.len() == textures.len() && textures.len() == manifest.textures.len(),
            "Weather texture index mismatch"
        );
        for t in &textures {
            let record = manifest
                .textures
                .get(&t.id)
                .context("Unknown weather image")?;
            ensure!(
                record.width == t.width
                    && record.height == t.height
                    && t.rgba.len() as u64 == u64::from(t.width) * u64::from(t.height) * 4,
                "Invalid weather pixel buffer"
            );
        }
        let definition_index = manifest
            .definitions
            .iter()
            .enumerate()
            .map(|(i, d)| (d.id.clone(), i))
            .collect();
        Ok(Arc::new(Self {
            manifest,
            textures,
            texture_index,
            definition_index,
        }))
    }
    pub fn placements_for<'a>(&'a self, map: &'a str) -> impl Iterator<Item = &'a Placement> {
        self.manifest
            .placements
            .iter()
            .filter(move |p| p.map_id == map)
    }
}
