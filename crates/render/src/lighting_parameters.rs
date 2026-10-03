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
/// All geometry-shadow projectors require their far plane strictly beyond
/// the fixed near plane. Reject tiny radii consistently before GPU upload.
pub(crate) fn valid_radii(inner: f32, outer: f32) -> bool {
    inner.is_finite()
        && outer.is_finite()
        && inner >= 0.0
        && outer > inner
        && outer > crate::shadow::LAMP_NEAR
}
/// Finite fields alone cannot guarantee a finite projector: depth scaling
/// can overflow a finite translation. Reader, writer and GPU admission share
/// the actual shadow-matrix check without constraining authored coordinates.
pub(crate) fn valid_geometry(position: [f32; 3], inner: f32, outer: f32) -> bool {
    valid_radii(inner, outer) && crate::shadow::finite_lamp_faces(position.into(), outer)
}
impl Parameters {
    fn validate(&self, root: &Path) -> Result<()> {
        ensure!(
            self.schema_version == 1 && self.bundle_sha256 == fingerprint(root)?,
            "Light parameters are stale or use an unsupported schema"
        );
        for lights in self.maps.values() {
            ensure!(
                lights.len() <= crate::map_lighting::MAX_LIGHTS
                    && lights.iter().all(|l| l
                        .position
                        .iter()
                        .chain(&l.color)
                        .all(|v| v.is_finite())
                        && l.color.iter().all(|c| *c >= 0.0)
                        && valid_geometry(l.position, l.inner, l.outer)),
                "Invalid recovered light parameters"
            );
        }
        Ok(())
    }
    /// The offline generator validates the same fields as runtime, and the
    /// encoded size, before replacing an existing descriptor file.
    pub fn write_atomic(&self, root: &Path, output: &Path) -> Result<()> {
        self.validate(root)?;
        let bytes = serde_json::to_vec_pretty(self)?;
        ensure!(
            bytes.len() as u64 <= MAX_BYTES,
            "Light parameters exceed 256 KiB"
        );
        let partial = output.with_extension(format!("{}.partial", std::process::id()));
        let result =
            std::fs::write(&partial, bytes).and_then(|_| std::fs::rename(&partial, output));
        if result.is_err() {
            let _ = std::fs::remove_file(&partial);
        }
        result.context("Replacing prepared light parameters")
    }
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
        data.validate(root)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generator_validation_preserves_existing_file_on_bad_radii_or_size() -> Result<()> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "bri-light-parameters-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root)?;
        std::fs::write(root.join("bundle.json"), b"{}")?;
        let mut parameters = Parameters {
            schema_version: 1,
            bundle_sha256: fingerprint(&root)?,
            maps: BTreeMap::from([(
                "map".into(),
                vec![SourceLight {
                    position: [0.0; 3],
                    color: [1.0; 3],
                    inner: 0.0,
                    outer: 1.0,
                }],
            )]),
        };
        let output = root.join(FILE);
        parameters.write_atomic(&root, &output)?;
        let original = std::fs::read(&output)?;
        for outer in [0.001, crate::shadow::LAMP_NEAR] {
            parameters.maps.get_mut("map").unwrap()[0].outer = outer;
            assert!(parameters.write_atomic(&root, &output).is_err());
            assert_eq!(std::fs::read(&output)?, original);
        }
        let light = &mut parameters.maps.get_mut("map").unwrap()[0];
        light.position = [1e38, 0.0, 0.0];
        light.outer = 0.050001;
        assert!(valid_radii(light.inner, light.outer));
        assert!(parameters.write_atomic(&root, &output).is_err());
        assert_eq!(std::fs::read(&output)?, original);
        // A hash-matching descriptor written outside the generator is also
        // rejected by the runtime reader, not just by offline publication.
        std::fs::write(&output, serde_json::to_vec(&parameters)?)?;
        assert!(Parameters::read(&root).is_err());
        std::fs::write(&output, &original)?;
        let light = &mut parameters.maps.get_mut("map").unwrap()[0];
        light.position = [0.0; 3];
        light.outer = 1.0;
        parameters
            .maps
            .insert("x".repeat(MAX_BYTES as usize), Vec::new());
        assert!(parameters.write_atomic(&root, &output).is_err());
        assert_eq!(std::fs::read(&output)?, original);
        assert!(Parameters::read(&root).is_ok());
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
}
