//! Versioned source light descriptors prepared alongside a native map bundle.
//! Recovery may inspect original illumination; runtime reads only this file and
//! a fingerprint of bundle metadata (including content-addressed asset names).
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Read, path::Path};

pub const FILE: &str = "lighting-parameters.json";
const MAX_BYTES: u64 = 256 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceLight {
    pub position: [f32; 3],
    pub color: [f32; 3],
    pub inner: f32,
    pub outer: f32,
}
impl From<crate::map_lighting::MapLight> for SourceLight {
    fn from(l: crate::map_lighting::MapLight) -> Self {
        Self {
            position: l.position,
            color: l.color,
            inner: l.inner,
            outer: l.outer,
        }
    }
}
impl From<SourceLight> for crate::map_lighting::MapLight {
    fn from(l: SourceLight) -> Self {
        Self {
            position: l.position,
            color: l.color,
            inner: l.inner,
            outer: l.outer,
            channel: None,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Parameters {
    pub schema_version: u32,
    /// Metadata contains hashes/filenames for geometry, source lighting and
    /// placements. Validation never opens a baked illumination image.
    pub bundle_sha256: String,
    pub maps: BTreeMap<String, Vec<SourceLight>>,
}
pub fn fingerprint(root: &Path) -> Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(std::fs::read(root.join("bundle.json"))?)
    ))
}
impl Parameters {
    pub fn read(root: &Path) -> Result<Self> {
        let file = root.join(FILE);
        let mut bytes = Vec::new();
        std::fs::File::open(file)?
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= MAX_BYTES,
            "Light parameters exceed 256 KiB"
        );
        let data: Self = serde_json::from_slice(&bytes)?;
        ensure!(
            data.schema_version == 1 && data.bundle_sha256 == fingerprint(root)?,
            "Light parameters are stale or use an unsupported schema"
        );
        for lights in data.maps.values() {
            ensure!(
                lights.len() <= crate::map_lighting::MAX_LIGHTS
                    && lights.iter().all(|l| l
                        .position
                        .iter()
                        .chain(&l.color)
                        .chain([&l.inner, &l.outer])
                        .all(|v| v.is_finite())
                        && l.color.iter().all(|c| *c >= 0.0)
                        && l.inner >= 0.0
                        && l.outer > l.inner),
                "Invalid recovered light parameters"
            );
        }
        Ok(data)
    }
    pub fn lights(&self, id: &str) -> Result<Vec<crate::map_lighting::MapLight>> {
        Ok(self
            .maps
            .get(id)
            .context("Map has no prepared light parameters")?
            .iter()
            .cloned()
            .map(Into::into)
            .collect())
    }
}
