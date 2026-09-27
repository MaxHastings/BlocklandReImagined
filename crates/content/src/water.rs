//! Native liquid regions, shared by presentation and authoritative queries.
use crate::environment::Image;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Water {
    pub schema_version: u32,
    pub node: usize,
    pub id: String,
    /// Native Y-up bounds. The top is the still-water surface, not the origin.
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub repeat_period: Option<f32>,
    pub liquid_type: String,
    pub density: f32,
    pub viscosity: f32,
    pub surface: Image,
    pub shore: Image,
    pub reflection: Option<Image>,
    pub opacity: f32,
    pub wave_amplitude: f32,
    pub flow: [f32; 2],
    pub distortion: [f32; 3], // spatial frequency, magnitude, seconds per radian
    pub tiles: [f32; 2],      // surface, shore
    pub depth_mask: bool,
    pub depth_alpha: [f32; 4], // min, max, shore depth, gradient (authored, unclamped)
    pub reflection_intensity: f32,
    pub parallax: f32,
    pub warnings: Vec<String>,
}
impl Water {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1 && !self.id.is_empty(),
            "Invalid native water identity/schema"
        );
        ensure!(
            self.min
                .iter()
                .chain(&self.max)
                .all(|x| x.is_finite() && x.abs() <= 1_000_000.0)
                && (0..3)
                    .all(|i| self.max[i] > self.min[i] && self.max[i] - self.min[i] <= 65536.0),
            "Invalid water bounds"
        );
        ensure!(
            self.repeat_period.is_none_or(|p| p.is_finite()
                && p > 0.0
                && p <= 65536.0
                && self.max[0] - self.min[0] <= p
                && self.max[2] - self.min[2] <= p),
            "Invalid water repetition"
        );
        ensure!(
            [
                self.density,
                self.viscosity,
                self.opacity,
                self.wave_amplitude,
                self.reflection_intensity,
                self.parallax
            ]
            .iter()
            .chain(&self.flow)
            .chain(&self.distortion)
            .chain(&self.tiles)
            .chain(&self.depth_alpha)
            .all(|x| x.is_finite() && x.abs() <= 10000.0),
            "Invalid water parameters"
        );
        ensure!(
            self.density >= 0.0
                && self.viscosity >= 0.0
                && self.wave_amplitude >= 0.0
                && self.distortion[2] > 0.0
                && self.tiles.iter().all(|t| *t > 0.0)
                && self.depth_alpha[2] >= 0.0
                && self.depth_alpha[3] >= 0.0,
            "Invalid water parameter range"
        );
        for image in [&self.surface, &self.shore]
            .into_iter()
            .chain(self.reflection.as_ref())
        {
            ensure!(
                !image.file.is_empty()
                    && !image.file.contains(['/', '\\', ':'])
                    && image.file != "."
                    && image.file != ".."
                    && image.sha256.len() == 64
                    && image.sha256.bytes().all(|b| b.is_ascii_hexdigit())
                    && image.width > 0
                    && image.height > 0
                    && image.width <= 8192
                    && image.height <= 8192,
                "Invalid native water image"
            );
        }
        Ok(())
    }
    /// UV within the authored footprint. Repetition never fills gaps in a tile.
    pub fn footprint(&self, x: f32, z: f32) -> Option<[f32; 2]> {
        if !x.is_finite() || !z.is_finite() {
            return None;
        }
        let mut dx = x - self.min[0];
        let mut dz = self.max[2] - z;
        if let Some(period) = self.repeat_period {
            dx = dx.rem_euclid(period);
            dz = dz.rem_euclid(period);
        }
        let width = self.max[0] - self.min[0];
        let depth = self.max[2] - self.min[2];
        (dx >= 0.0 && dx < width && dz >= 0.0 && dz < depth).then_some([dx / width, dz / depth])
    }
    pub fn coverage(&self, feet: [f32; 3], height: f32) -> f32 {
        if !feet[1].is_finite()
            || !height.is_finite()
            || height <= 0.0
            || self.footprint(feet[0], feet[2]).is_none()
        {
            return 0.0;
        }
        ((self.max[1].min(feet[1] + height) - self.min[1].max(feet[1])) / height).clamp(0.0, 1.0)
    }
    /// Surface/shore opacity from authored depth controls, with defined behavior
    /// for legacy zero-gradient and out-of-range alpha values.
    pub fn depth_opacity(&self, depth: f32) -> [f32; 2] {
        if !self.depth_mask {
            return [self.opacity.clamp(0.0, 1.0), 0.0];
        }
        let [low, high, shore, gradient] = self.depth_alpha;
        let ramp = |d: f32| {
            if shore <= 0.0 || d >= shore {
                1.0
            } else if gradient <= 0.0 {
                0.0
            } else {
                (d.max(0.0) / shore).powf(1.0 / gradient)
            }
        };
        let surface = (low + (high - low) * ramp(depth)).clamp(0.0, 1.0);
        let edge = if shore > 0.0 && depth > shore {
            ((high - low) * (1.0 - ramp(depth - shore))).clamp(0.0, 1.0)
        } else {
            surface
        };
        [surface, edge]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn volume_top_repetition_and_depth_are_not_visual_wave_displacement() {
        let image = Image {
            file: "test.png".into(),
            source: "fixture".into(),
            sha256: "0".repeat(64),
            width: 1,
            height: 1,
        };
        let w = Water {
            schema_version: 1,
            node: 0,
            id: "fixture".into(),
            min: [-20., -91., -30.],
            max: [12., 9., 2.],
            repeat_period: Some(2048.),
            liquid_type: "water".into(),
            density: 1.,
            viscosity: 40.,
            surface: image.clone(),
            shore: image,
            reflection: None,
            opacity: 0.25,
            wave_amplitude: 0.5,
            flow: [0.; 2],
            distortion: [0.1, 0.01, 0.5],
            tiles: [50., 60.],
            depth_mask: true,
            depth_alpha: [0.03, 0.5, 20., 1.],
            reflection_intensity: 0.,
            parallax: 0.5,
            warnings: vec![],
        };
        w.validate().unwrap();
        assert_eq!(w.coverage([0., 8., 0.], 2.), 0.5);
        assert_eq!(w.coverage([0., -100., 0.], 2.), 0.);
        assert_eq!(w.coverage([2048., 8., 0.], 2.), 0.5);
        assert_eq!(w.coverage([100., 8., 0.], 2.), 0.);
        assert_eq!(w.coverage([f32::NAN, 8., 0.], 2.), 0.);
        assert!((w.depth_opacity(10.)[0] - 0.265).abs() < 1e-6);
        assert_eq!(w.depth_opacity(45.), [0.5, 0.]);
    }
}
