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
/// thickens from `start` to `FOG_DEPTH` optical depth at `density_end`
/// (or `end` when unset); above the
/// eye it thins with height (`height` units per e-fold, `FOG_HEIGHT` unless
/// the map says). The sky is the backdrop at the world's edge, `end` away,
/// and takes the fog of the air there: fog-coloured at the horizon, clear
/// once its edge is a few `height` above the eye, so thick fog (a near
/// edge) covers more of it. Geometry does not fog into the sky: over the
/// last quarter of the range it fades out into whatever is behind it, and
/// past `end` it is not drawn, so nothing is cut out against the sky where
/// the world ends. Below the eye the fog stays level-density, its backdrop
/// the plain fog colour, unless the sky goes on below the horizon (a bottom
/// face, Skylands' mirrored floor): then the fog is symmetric about the eye
/// and the floor hazes only toward the horizon.
///
/// A haze (`max` below 1) never covers more than `max` of anything, so far
/// cliffs soften into it but stay visible; geometry then does not fade out
/// over the last quarter but stops at `end`.
pub const FOG_DEPTH: f32 = 4.0;
pub const FOG_HEIGHT: f32 = 60.0;
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fog {
    pub start: f32,
    /// Geometry visibility cutoff. Density can reach its authored depth sooner.
    pub end: f32,
    /// Distance at which level fog reaches FOG_DEPTH optical depth. Defaults
    /// to `end`; a capped haze can thicken without hiding distant geometry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub density_end: Option<f32>,
    pub color: [f32; 3],
    /// Units the fog thins by an e-fold for, above the eye (and below it
    /// when the sky goes on below the horizon): smaller keeps the haze
    /// nearer the ground.
    #[serde(
        default = "Fog::default_height",
        skip_serializing_if = "Fog::is_default_height"
    )]
    pub height: f32,
    /// The most the fog ever covers, above 0 to 1 (complete fog).
    #[serde(
        default = "Fog::default_max",
        skip_serializing_if = "Fog::is_default_max"
    )]
    pub max: f32,
    /// Fog grows in a straight line with distance, from nothing at `start`
    /// to `max` at the density distance (`density_end`, else `end`), with no
    /// thinning above the eye: a Halo sky's fog, whose density texture is a
    /// linear ramp. Off: the exponential fog described above.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub linear: bool,
}
impl Default for Fog {
    fn default() -> Self {
        Self {
            start: 0.0,
            end: 0.0,
            density_end: None,
            color: [0.0; 3],
            height: FOG_HEIGHT,
            max: 1.0,
            linear: false,
        }
    }
}
impl Fog {
    fn default_height() -> f32 {
        FOG_HEIGHT
    }
    fn default_max() -> f32 {
        1.0
    }
    // Unset values stay out of the files, so packs written before them
    // keep their bytes and digests.
    fn is_default_height(height: &f32) -> bool {
        *height == FOG_HEIGHT
    }
    fn is_default_max(max: &f32) -> bool {
        *max == 1.0
    }
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
            self.density_end
                .is_none_or(|end| end.is_finite() && end > self.start && end <= 1_000_000.0),
            "Invalid fog density distance"
        );
        ensure!(
            self.color
                .iter()
                .all(|x| x.is_finite() && (0.0..=1.0).contains(x)),
            "Invalid fog color"
        );
        ensure!(
            FOG_HEIGHTS.contains(&self.height),
            "Fog height must be 1 to 100000 units"
        );
        ensure!(
            self.max > 0.0 && self.max <= 1.0,
            "Fog max must be above 0, at most 1"
        );
        Ok(())
    }
    /// A haze: the fog never covers everything, and geometry stops at
    /// `end` instead of fading out before it.
    pub fn haze(&self) -> bool {
        self.max < 1.0
    }
    /// The shaders' `haze` uniform: height, max, optional density end.
    pub fn haze_uniform(&self) -> [f32; 4] {
        [
            self.height,
            self.max,
            self.density_end.unwrap_or(0.0),
            f32::from(self.linear),
        ]
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
        if self.linear {
            let reach = (self.density_end.unwrap_or(self.end) - self.start).max(0.001);
            return ((distance - self.start) / reach).clamped(0.0, 1.0) * self.max;
        }
        let up = offset[1] / distance.max(0.0001);
        let inside = (distance - self.start).max(0.0);
        let rise = rising(up, sky_below, self.height);
        let x = rise * inside;
        let spread = if x < 0.0001 {
            1.0 - 0.5 * x
        } else {
            (1.0 - (-x).exp()) / x
        };
        let depth = self.density() * (-rise * self.start).exp() * inside * spread;
        (1.0 - (-depth).exp()) * self.max
    }
    /// How far a point `distance` away has faded out into what lies behind
    /// it: 0 before the last quarter of the range, 1 from `end` on. A haze
    /// does not fade: 0 before `end`, 1 from it.
    pub fn edge(&self, distance: f32) -> f32 {
        if self.end <= 0.0 {
            return 0.0;
        }
        if self.haze() {
            return if distance >= self.end { 1.0 } else { 0.0 };
        }
        let t = ((distance - self.start - 0.75 * (self.end - self.start))
            / (0.25 * (self.end - self.start)).max(0.001))
        .clamped(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }
    /// Fog over the sky along a ray whose direction rises `up` (its y):
    /// the air at the world's edge in front of it. `sky_below`: the sky
    /// goes on below the horizon.
    pub fn sky_amount(&self, up: f32, sky_below: bool) -> f32 {
        if self.end <= 0.0 {
            return 0.0;
        }
        let depth = self.density() * (self.end - self.start).max(0.001);
        (1.0 - (-depth * (-rising(up, sky_below, self.height) * self.end).exp()).exp()) * self.max
    }
    fn density(&self) -> f32 {
        FOG_DEPTH / (self.density_end.unwrap_or(self.end) - self.start).max(0.001)
    }
}
/// The fog heights a map or setting may give, units per e-fold.
pub const FOG_HEIGHTS: std::ops::RangeInclusive<f32> = 1.0..=100_000.0;
/// How fast a ray whose direction rises `up` leaves fog that thins by an
/// e-fold every `height` units, per unit.
fn rising(up: f32, sky_below: bool, height: f32) -> f32 {
    (if sky_below { up.abs() } else { up.max(0.0) }) / height.max(0.001)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fog(height: f32, max: f32) -> Fog {
        Fog {
            start: 100.0,
            end: 500.0,
            color: [0.5; 3],
            height,
            max,
            density_end: None,
            linear: false,
        }
    }

    #[test]
    fn a_capped_haze_can_thicken_without_clipping_distant_geometry() {
        let mut haze = fog(FOG_HEIGHT, 0.4);
        haze.end = 2000.0;
        let thin = haze.amount(400.0);
        haze.density_end = Some(400.0);
        haze.validate().unwrap();
        assert!(haze.amount(400.0) > thin);
        assert!((haze.amount(400.0) - 0.4 * (1.0 - (-FOG_DEPTH).exp())).abs() < 1e-6);
        assert_eq!(haze.edge(1500.0), 0.0);
        assert_eq!(haze.edge(2000.0), 1.0);
        assert!(haze.sky_amount(0.0, false) > 0.399);
        for end in [f32::NAN, 0.0, haze.start, 1_000_001.0] {
            haze.density_end = Some(end);
            assert!(haze.validate().is_err());
        }
    }
    #[test]
    fn a_linear_fog_runs_straight_from_the_start_to_the_density_distance() {
        let mut fog = fog(FOG_HEIGHT, 0.4);
        fog.start = 12.0;
        fog.end = 1200.0;
        fog.density_end = Some(412.0);
        fog.linear = true;
        fog.validate().unwrap();
        assert_eq!(fog.amount(12.0), 0.0);
        assert!((fog.amount(212.0) - 0.2).abs() < 1e-6);
        assert!((fog.amount(412.0) - 0.4).abs() < 1e-6);
        assert!((fog.amount(900.0) - 0.4).abs() < 1e-6);
        // Level fog: the same up, down and level.
        for offset in [[0.0, 212.0, 0.0], [0.0, -212.0, 0.0], [212.0, 0.0, 0.0]] {
            assert!((fog.amount_along(offset, false) - 0.2).abs() < 1e-6);
        }
        assert_eq!(fog.haze_uniform()[3], 1.0);
        fog.linear = false;
        assert_eq!(fog.haze_uniform()[3], 0.0);
    }
    #[test]
    fn unset_haze_reads_as_the_fog_before_it() {
        let read: Fog =
            serde_json::from_str(r#"{"start":100,"end":500,"color":[0.5,0.5,0.5]}"#).unwrap();
        assert_eq!((read.height, read.max), (FOG_HEIGHT, 1.0));
        let unset = Fog::default();
        assert_eq!((unset.height, unset.max), (FOG_HEIGHT, 1.0));
        read.validate().unwrap();
        assert!(!read.haze());
        // The fog as it was before height and max: FOG_HEIGHT, no ceiling.
        let before = |offset: [f32; 3]| {
            let distance = offset.iter().map(|v| v * v).sum::<f32>().sqrt();
            let rise = (offset[1] / distance).max(0.0) / FOG_HEIGHT;
            let inside = (distance - 100.0f32).max(0.0);
            let x = rise * inside;
            let spread = if x < 0.0001 {
                1.0 - 0.5 * x
            } else {
                (1.0 - (-x).exp()) / x
            };
            1.0 - (-(FOG_DEPTH / 400.0) * (-rise * 100.0).exp() * inside * spread).exp()
        };
        for offset in [[300.0, 0.0, 0.0], [400.0, 60.0, 30.0], [50.0, 200.0, 0.0]] {
            assert!((read.amount_along(offset, false) - before(offset)).abs() < 1e-6);
        }
        let sky = 1.0 - (-FOG_DEPTH * (-0.2 / FOG_HEIGHT * 500.0f32).exp()).exp();
        assert!((read.sky_amount(0.2, false) - sky).abs() < 1e-6);
        assert_eq!(read.edge(400.0), 0.0);
        assert!(read.edge(475.0) > 0.0 && read.edge(475.0) < 1.0);
        assert_eq!(read.edge(500.0), 1.0);
    }

    #[test]
    fn a_haze_never_covers_everything_and_stops_at_the_far_plane() {
        let haze = fog(FOG_HEIGHT, 0.7);
        assert!(haze.haze());
        assert!((haze.amount(10_000.0) - 0.7).abs() < 1e-4);
        let complete = fog(FOG_HEIGHT, 1.0);
        assert!((haze.sky_amount(0.0, false) - 0.7 * complete.sky_amount(0.0, false)).abs() < 1e-6);
        // No dithered fade-out: the far cliffs stay until the far plane.
        assert_eq!(haze.edge(499.0), 0.0);
        assert_eq!(haze.edge(500.0), 1.0);
        assert!(fog(FOG_HEIGHT, 0.0).validate().is_err());
        assert!(fog(FOG_HEIGHT, 1.5).validate().is_err());
    }

    #[test]
    fn a_lower_fog_height_keeps_the_haze_near_the_ground() {
        let (low, high) = (fog(15.0, 1.0), fog(240.0, 1.0));
        let level = [400.0, 0.0, 0.0];
        assert_eq!(
            low.amount_along(level, false),
            high.amount_along(level, false)
        );
        let raised = [400.0, 80.0, 0.0];
        assert!(low.amount_along(raised, false) < high.amount_along(raised, false));
        assert!(low.sky_amount(0.2, false) < high.sky_amount(0.2, false));
        assert!(fog(0.5, 1.0).validate().is_err());
    }
}
