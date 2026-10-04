//! Native authored atmosphere. Original formats are read only by the converter.
use anyhow::{Result, ensure};
use bri_console::Clamp;
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
/// Fog, the one atmosphere every pass shares (its shader twin is
/// `bri-render`'s fog.wgsl). Exponential fog: level with the eye it
/// thickens from `start` to `FOG_DEPTH` optical depth at `end`; above the
/// eye it thins with height (`FOG_HEIGHT` units per e-fold), so the sky is
/// fog-coloured at the horizon and clearer overhead, and thick fog covers
/// more of it. Geometry ends as fogged as the sky behind it: over the last
/// quarter of the range it fades to the sky's own fog, so nothing is cut
/// out against the sky where the world ends. Below the eye the fog stays
/// level-density, its backdrop the plain fog colour, unless the sky goes on
/// below the horizon (a bottom face, Skylands' mirrored floor): then the fog
/// is symmetric about the eye and the floor hazes only toward the horizon.
pub const FOG_DEPTH: f32 = 4.0;
pub const FOG_HEIGHT: f32 = 60.0;
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
    /// Fog over a point `distance` away, level with the eye.
    pub fn amount(&self, distance: f32) -> f32 {
        self.amount_along([distance, 0.0, 0.0], false)
    }
    /// Fog over a point at `offset` from the eye (y up); `sky_below`: the
    /// sky goes on below the horizon.
    pub fn amount_along(&self, offset: [f32; 3], sky_below: bool) -> f32 {
        if self.end <= 0.0 {
            return 0.0;
        }
        let distance = offset.iter().map(|v| v * v).sum::<f32>().sqrt();
        let up = offset[1] / distance.max(0.0001);
        let inside = (distance - self.start).max(0.0);
        let rise = rising(up, sky_below);
        let x = rise * inside;
        let spread = if x < 0.0001 {
            1.0 - 0.5 * x
        } else {
            (1.0 - (-x).exp()) / x
        };
        let depth = self.density() * (-rise * self.start).exp() * inside * spread;
        let t = ((distance - self.start - 0.75 * (self.end - self.start))
            / (0.25 * (self.end - self.start)).max(0.001))
        .clamped(0.0, 1.0);
        let edge = t * t * (3.0 - 2.0 * t);
        (1.0 - (-depth).exp()).max(edge * self.sky_amount(up, sky_below))
    }
    /// Fog over the sky along a ray whose direction rises `up` (its y);
    /// `sky_below`: the sky goes on below the horizon.
    pub fn sky_amount(&self, up: f32, sky_below: bool) -> f32 {
        if self.end <= 0.0 {
            return 0.0;
        }
        let rise = rising(up, sky_below).max(1e-6);
        1.0 - (-self.density() * (-rise * self.start).exp() / rise).exp()
    }
    fn density(&self) -> f32 {
        FOG_DEPTH / (self.end - self.start).max(0.001)
    }
}
/// How fast a ray whose direction rises `up` leaves the fog, per unit.
fn rising(up: f32, sky_below: bool) -> f32 {
    (if sky_below { up.abs() } else { up.max(0.0) }) / FOG_HEIGHT
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
