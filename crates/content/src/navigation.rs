//! Prepared walking topology. The motor owns physical support and clearance.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WalkingProfile {
    pub round: bool,
    pub wall_step: Option<[f32; 2]>,
    pub width: f32,
    pub height: f32,
    pub crouch_height: f32,
    pub step: f32,
    pub crawl_step: f32,
    pub drop: f32,
    pub floor_cos: f32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WalkingMap {
    pub schema_version: u32,
    pub profile: WalkingProfile,
    pub mesh: WalkingMesh,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}
impl WalkingMap {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1,
            "Unsupported walking topology schema"
        );
        self.mesh.validate()?;
        let p = &self.profile;
        ensure!(
            p.wall_step
                .is_none_or(|s| s.iter().all(|v| v.is_finite() && *v > 0.)),
            "Invalid walking step-over profile"
        );
        ensure!(
            [p.width, p.height, p.crouch_height, p.step, p.drop]
                .iter()
                .all(|v| v.is_finite() && *v > 0.)
                && p.crouch_height <= p.height
                && p.crawl_step.is_finite()
                && p.crawl_step >= 0.
                && p.floor_cos.is_finite()
                && (0. ..=1.).contains(&p.floor_cos),
            "Invalid walking clearance profile"
        );
        Ok(())
    }
}

/// Portable, bounded Recast polygon input, rather than an opaque native memory
/// image. Neighbours use polygon indices; absent neighbours are solid boundaries.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WalkingMesh {
    pub vertices: Vec<[u16; 3]>,
    pub polygons: Vec<Vec<u16>>,
    pub neighbours: Vec<Vec<Option<u16>>>,
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub cell_size: f32,
    pub cell_height: f32,
}
impl WalkingMesh {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (3..65535).contains(&self.vertices.len())
                && !self.polygons.is_empty()
                && self.polygons.len() < 32768,
            "Walking mesh size limit"
        );
        ensure!(
            self.neighbours.len() == self.polygons.len(),
            "Walking mesh neighbour count"
        );
        ensure!(
            self.cell_size.is_finite()
                && self.cell_size > 0.
                && self.cell_height.is_finite()
                && self.cell_height > 0.,
            "Walking mesh cell scale"
        );
        ensure!(
            self.min.iter().chain(&self.max).all(|x| x.is_finite())
                && (0..3).all(|i| self.min[i] <= self.max[i]),
            "Walking mesh bounds"
        );
        for (poly, adjacent) in self.polygons.iter().zip(&self.neighbours) {
            ensure!(
                (3..=6).contains(&poly.len())
                    && poly.len() == adjacent.len()
                    && poly.iter().all(|v| usize::from(*v) < self.vertices.len()),
                "Walking mesh polygon"
            );
            ensure!(
                poly.iter().collect::<BTreeSet<_>>().len() == poly.len(),
                "Repeated walking mesh vertex"
            );
        }
        for (i, (poly, adjacent)) in self.polygons.iter().zip(&self.neighbours).enumerate() {
            for (edge, other) in adjacent.iter().enumerate() {
                if let Some(other) = other {
                    let j = usize::from(*other);
                    ensure!(
                        j < self.polygons.len() && j != i,
                        "Walking mesh neighbour index"
                    );
                    let a = poly[edge];
                    let b = poly[(edge + 1) % poly.len()];
                    let reverse = self.polygons[j].iter().enumerate().position(|(k, v)| {
                        *v == b && self.polygons[j][(k + 1) % self.polygons[j].len()] == a
                    });
                    ensure!(
                        reverse.is_some_and(|k| self.neighbours[j].get(k) == Some(&Some(i as u16))),
                        "Walking mesh asymmetric connection"
                    );
                }
            }
        }
        Ok(())
    }
}
