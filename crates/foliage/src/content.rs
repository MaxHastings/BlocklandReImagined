use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Texture {
    pub id: String,
    pub path: String,
    pub sha256: String,
    pub source: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Evidence {
    pub source: String,
    pub sha256: String,
    pub line: usize,
    pub fields: BTreeMap<String, String>,
    pub adaptations: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Definition {
    pub id: String,
    pub scene: String,
    pub node: usize,
    pub texture: usize,
    pub origin: [f32; 3],
    pub seed: u32,
    pub count: u32,
    pub retries: u32,
    pub inner: [f32; 2],
    pub outer: [f32; 2],
    pub square: bool,
    pub offset: f32,
    pub allowed_slope: f32,
    pub allow_terrain: bool,
    pub allow_interior: bool,
    pub allow_static: bool,
    pub allow_water: bool,
    pub water_surface: bool,
    pub width: [f32; 2],
    pub height: [f32; 2],
    pub fixed_size: bool,
    pub fixed_aspect: bool,
    pub flip: bool,
    pub billboard: bool,
    pub random_rotation: bool,
    pub rotation: f32,
    pub sway: bool,
    pub sway_sync: bool,
    pub sway_magnitude: [f32; 2],
    pub sway_seconds: [f32; 2],
    pub light: bool,
    pub light_sync: bool,
    pub light_seconds: f32,
    pub luminance: [f32; 2],
    pub color_top: [f32; 4],
    pub color_bottom: [f32; 4],
    pub ground_alpha: f32,
    pub alpha_cutoff: f32,
    pub closest: f32,
    pub distance: f32,
    pub fade_near: f32,
    pub fade_far: f32,
    pub cull_size: f32,
    pub culling: bool,
    pub hidden: bool,
    pub evidence: Evidence,
}
impl Definition {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.count <= 100000
                && self.retries > 0
                && self.retries <= 1000
                && self.seed > 0
                && self.seed < 2147483647,
            "invalid foliage allocation/seed"
        );
        ensure!(
            self.origin.iter().all(|v| v.is_finite() && v.abs() < 1e7)
                && self.offset.is_finite()
                && self.rotation.is_finite(),
            "invalid foliage transform"
        );
        for pair in [
            self.inner,
            self.outer,
            self.width,
            self.height,
            self.sway_seconds,
            self.luminance,
        ] {
            ensure!(
                pair.iter().all(|v| v.is_finite() && *v >= 0.),
                "invalid foliage range"
            );
        }
        ensure!(
            self.width[0] > 0.
                && self.width[1] >= self.width[0]
                && self.height[0] > 0.
                && self.height[1] >= self.height[0]
                && self.sway_seconds[0] > 0.
                && self.sway_seconds[1] >= self.sway_seconds[0]
                && self.luminance[1] >= self.luminance[0],
            "inverted foliage range"
        );
        ensure!(
            (0..2).all(|i| self.outer[i] > 0.
                && self.outer[i] <= 10000.
                && self.inner[i] <= self.outer[i]),
            "invalid placement ellipse"
        );
        ensure!(
            self.allowed_slope.is_finite()
                && (0. ..=90.).contains(&self.allowed_slope)
                && self.cull_size.is_finite()
                && (8. ..=1024.).contains(&self.cull_size),
            "invalid slope/culling"
        );
        ensure!(
            [
                self.closest,
                self.distance,
                self.fade_near,
                self.fade_far,
                self.light_seconds
            ]
            .iter()
            .all(|v| v.is_finite() && *v >= 0.)
                && self.distance >= self.closest
                && self.distance + self.fade_far <= 10000.
                && self.light_seconds > 0.,
            "invalid foliage timing/view"
        );
        ensure!(
            self.sway_magnitude
                .iter()
                .all(|v| v.is_finite() && *v >= 0. && *v <= 100.)
                && self
                    .color_top
                    .iter()
                    .chain(&self.color_bottom)
                    .chain([&self.ground_alpha, &self.alpha_cutoff])
                    .all(|v| v.is_finite() && (0. ..=1.).contains(v)),
            "invalid foliage appearance"
        );
        ensure!(
            self.width[1] <= 1024.
                && self.height[1] <= 1024.
                && self.sway_seconds[1] <= 86400.
                && self.light_seconds <= 86400.
                && self.luminance[1] <= 16.,
            "foliage size/timing exceeds native bounds"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FoliagePack {
    pub schema_version: u32,
    pub definitions: Vec<Definition>,
    pub textures: Vec<Texture>,
    pub source_bundle: String,
}
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}
impl FoliagePack {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1 && self.definitions.len() <= 128 && self.textures.len() <= 128,
            "invalid foliage schema/count"
        );
        let mut ids = std::collections::BTreeSet::new();
        for d in &self.definitions {
            d.validate()?;
            ensure!(
                ids.insert(&d.id) && d.texture < self.textures.len(),
                "duplicate definition/missing texture"
            );
        }
        Ok(())
    }
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        ensure!(
            std::fs::metadata(path.as_ref())?.len() < 16 * 1024 * 1024,
            "oversized pack file"
        );
        let bytes = std::fs::read(path)?;
        ensure!(bytes.len() < 16 * 1024 * 1024, "oversized pack");
        let p: Self = serde_json::from_slice(&bytes)?;
        p.validate()?;
        Ok(p)
    }
    pub fn images(&self, root: impl AsRef<Path>) -> Result<Vec<Image>> {
        self.validate()?;
        let mut decoded_bytes = 0u64;
        self.textures
            .iter()
            .map(|t| {
                let path = Path::new(&t.path);
                ensure!(
                    !path.is_absolute()
                        && path
                            .components()
                            .all(|c| matches!(c, std::path::Component::Normal(_))),
                    "unsafe native texture path"
                );
                ensure!(
                    std::fs::metadata(root.as_ref().join(path))?.len() <= 16 * 1024 * 1024,
                    "oversized image file"
                );
                let bytes =
                    std::fs::read(root.as_ref().join(path)).context("foliage image missing")?;
                ensure!(
                    format!("{:x}", Sha256::digest(&bytes)) == t.sha256,
                    "foliage image hash mismatch"
                );
                let dimensions = image::ImageReader::new(std::io::Cursor::new(&bytes))
                    .with_guessed_format()?
                    .into_dimensions()?;
                ensure!(
                    dimensions.0 > 0
                        && dimensions.0 <= 4096
                        && dimensions.1 > 0
                        && dimensions.1 <= 4096,
                    "oversized foliage image"
                );
                decoded_bytes += u64::from(dimensions.0) * u64::from(dimensions.1) * 4;
                ensure!(
                    decoded_bytes <= 128 << 20,
                    "decoded foliage images exceed128MiB"
                );
                let image = image::load_from_memory(&bytes)?.to_rgba8();
                ensure!(
                    image.width() > 0
                        && image.width() <= 4096
                        && image.height() > 0
                        && image.height() <= 4096,
                    "invalid foliage image dimensions"
                );
                Ok(Image {
                    width: image.width(),
                    height: image.height(),
                    rgba: image.into_raw(),
                })
            })
            .collect()
    }
}
