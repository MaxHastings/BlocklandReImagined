//! Native authored atmosphere. Original formats are read only by the converter.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Image {
    pub file: String,
    pub source: String,
    pub sha256: String,
    pub width: u32,
    pub height: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cloud {
    pub image: Image,
    pub center_height: f32,
    /// Original cloud UV motion converted to cycles per second.
    pub velocity: [f32; 2],
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fog {
    pub start: f32,
    pub end: f32,
    pub color: [f32; 3],
}
impl Fog {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.start.is_finite()
                && self.end.is_finite()
                && self.start >= 0.0
                && self.end >= self.start
                && self.end <= 1_000_000.0,
            "Invalid fog range"
        );
        ensure!(
            self.color
                .iter()
                .all(|x| x.is_finite() && (0.0..=1.0).contains(x)),
            "Invalid fog color"
        );
        Ok(())
    }
    pub fn amount(&self, distance: f32) -> f32 {
        if self.end == 0.0 || distance <= self.start {
            return 0.0;
        }
        let t = ((distance - self.start) / (self.end - self.start).max(0.001)).clamp(0.0, 1.0);
        1.0 - (1.0 - t) * (1.0 - t)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Environment {
    pub schema_version: u32,
    pub source_materials: String,
    pub source_sha256: String,
    /// Original DML ordering: native +Z, +X, -Z, -X, +Y, -Y.
    pub faces: Vec<Image>,
    /// DML slot six is an environment/reflection map, never a cloud layer.
    pub reflection: Option<Image>,
    pub clouds: Vec<Cloud>,
    pub textures: bool,
    pub bottom: bool,
    pub horizon_band: bool,
    pub solid_color: [f32; 3],
    pub fog: Fog,
    pub warnings: Vec<String>,
}
impl Environment {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 2
                && (self.faces.len() == 6
                    || (self.faces.is_empty() && !self.textures && self.clouds.is_empty()))
                && self.clouds.len() <= 3,
            "Invalid sky schema/layers"
        );
        self.fog.validate()?;
        ensure!(
            self.source_sha256.len() == 64
                && self.source_sha256.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid sky source checksum"
        );
        ensure!(
            self.fog.end > 0.0
                && self
                    .solid_color
                    .iter()
                    .all(|x| x.is_finite() && (0.0..=1.0).contains(x)),
            "Invalid sky range/color"
        );
        let mut decoded_bytes = 0_u64;
        for image in self
            .faces
            .iter()
            .chain(self.reflection.iter())
            .chain(self.clouds.iter().map(|c| &c.image))
        {
            ensure!(
                crate::brick_materials::safe_relative(&image.file)
                    && !image.file.contains('/')
                    && image.sha256.len() == 64
                    && image.sha256.bytes().all(|b| b.is_ascii_hexdigit())
                    && (1..=8192).contains(&image.width)
                    && (1..=8192).contains(&image.height),
                "Invalid sky image"
            );
            decoded_bytes += u64::from(image.width) * u64::from(image.height) * 4;
        }
        ensure!(
            decoded_bytes <= 256 * 1024 * 1024,
            "Sky decoded image budget exceeded"
        );
        for cloud in &self.clouds {
            ensure!(
                cloud.center_height.is_finite()
                    && (0.05..=1.0).contains(&cloud.center_height)
                    && cloud
                        .velocity
                        .iter()
                        .all(|x| x.is_finite() && x.abs() <= 100.0),
                "Invalid cloud motion/height"
            );
        }
        Ok(())
    }
}
