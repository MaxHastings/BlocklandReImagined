//! Scattered instances of one model: grass over a hillside, rocks across a plain, trees along a ridge.
//!
//! A map names a layer by the model it repeats, a list of placements (position, rotation, scale and a tint colour each) and,
//! optionally, a fade range: an instance is drawn in full out to the near distance from the viewer, fades out linearly between
//! the near and far distances, and is not drawn beyond the far one. The placements are data: whatever produced them (an
//! importer replaying an authoring tool's scatter, a generator) is not this crate's business, and the engine gives the layer
//! no gameplay meaning.
//!
//! The placements of a layer live in one file beside the map's bundle (see [`encode`]); the bundle's `scatter` table lists the
//! layers ([`Record`]).
use anyhow::{Result, bail, ensure};
use glam::{Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};

/// The first four bytes of a placement file.
pub const MAGIC: [u8; 4] = *b"BRIS";
/// The only placement file version.
pub const VERSION: u32 = 1;
/// Bytes of one placement in the file: position, rotation (x, y, z, w), scale (nine `f32`) and the tint (four bytes).
pub const RECORD_BYTES: usize = 3 * 4 + 4 * 4 + 3 * 4 + 4;
/// Most placements one layer may hold.
pub const MAX_INSTANCES: usize = 1 << 20;

/// One placement.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Instance {
    pub position: [f32; 3],
    /// Unit quaternion, x, y, z, w.
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    /// Red, green, blue and alpha the model is multiplied by; 255 leaves it as it is.
    pub tint: [u8; 4],
}
impl Instance {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.position.iter().all(|v| v.is_finite() && v.abs() < 1e7)
                && self.rotation.iter().all(|v| v.is_finite())
                && self.scale.iter().all(|v| v.is_finite() && v.abs() < 1e4),
            "A scatter instance has a non-finite or enormous value"
        );
        let length = self.rotation.iter().map(|v| v * v).sum::<f32>().sqrt();
        ensure!(
            (length - 1.0).abs() < 1e-3,
            "A scatter instance's rotation is not a unit quaternion"
        );
        Ok(())
    }
    /// Model space to world space: scale, then rotation, then translation.
    pub fn matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(
            Vec3::from(self.scale),
            Quat::from_array(self.rotation),
            Vec3::from(self.position),
        )
    }
}

/// The order a layer's instances are drawn in, for the blending of those that are not opaque.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Order {
    /// As the placements file lists them.
    #[default]
    Unsorted,
    /// Farthest from the viewer first.
    FarToNear,
    /// Nearest to the viewer first.
    NearToFar,
}

/// One entry of a bundle's `scatter` table: a layer and where its placements are.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Record {
    /// What the layer is, for logs and reports.
    pub name: String,
    /// The bundle asset (a shape) the layer repeats.
    pub asset: String,
    /// The placement file in the map's folder.
    pub file: String,
    /// Near and far fade distances, in world units, or None to draw every instance at any distance.
    #[serde(default)]
    pub fade: Option<[f32; 2]>,
    /// How the instances are ordered for drawing, by distance from the viewer.
    #[serde(default)]
    pub order: Order,
    /// Draw colour times tint times two (a texture and a diffuse colour modulated 2x, saturating), so a tint of 0.5 leaves the texture as it is.
    #[serde(default)]
    pub modulate_2x: bool,
    /// The layer is drawn only when the viewer's detail setting is at least this (0 always). The setting is the game's own; with none, every layer draws.
    #[serde(default)]
    pub detail: u8,
}
impl Record {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.name.is_empty()
                && self.name.len() <= 128
                && !self.name.chars().any(char::is_control),
            "A scatter layer needs a name of 1 to 128 characters"
        );
        ensure!(
            !self.asset.is_empty() && !self.file.is_empty(),
            "Scatter layer {} needs an asset and a file",
            self.name
        );
        ensure!(
            self.detail <= 3,
            "Scatter layer {}'s detail is 0 to 3",
            self.name
        );
        if let Some([near, far]) = self.fade {
            ensure!(
                near.is_finite() && far.is_finite() && near >= 0.0 && far > near,
                "Scatter layer {}'s fade needs 0 <= near < far",
                self.name
            );
        }
        Ok(())
    }
}

/// How much of an instance `distance` from the viewer shows (1 whole, 0 gone), given a layer's `fade` range.
pub fn visibility(fade: Option<[f32; 2]>, distance: f32) -> f32 {
    match fade {
        None => 1.0,
        Some([near, far]) => {
            if distance > far {
                0.0
            } else if distance > near {
                1.0 - (distance - near) / (far - near)
            } else {
                1.0
            }
        }
    }
}

/// Placements as a file: [`MAGIC`], the [`VERSION`], the count (`u32`), then each as little-endian `f32`s and the tint bytes.
pub fn encode(instances: &[Instance]) -> Result<Vec<u8>> {
    ensure!(
        instances.len() <= MAX_INSTANCES,
        "A scatter layer holds at most {MAX_INSTANCES} instances"
    );
    let mut out = Vec::with_capacity(12 + instances.len() * RECORD_BYTES);
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&(instances.len() as u32).to_le_bytes());
    for i in instances {
        i.validate()?;
        for v in i.position.iter().chain(&i.rotation).chain(&i.scale) {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&i.tint);
    }
    Ok(out)
}

/// The placements of a file written by [`encode`], each checked.
pub fn decode(bytes: &[u8]) -> Result<Vec<Instance>> {
    ensure!(
        bytes.len() >= 12 && bytes[..4] == MAGIC,
        "Not a scatter placement file"
    );
    let version = u32::from_le_bytes(bytes[4..8].try_into()?);
    if version != VERSION {
        bail!("Scatter placement file version {version} is not supported (expected {VERSION})");
    }
    let count = u32::from_le_bytes(bytes[8..12].try_into()?) as usize;
    ensure!(
        count <= MAX_INSTANCES,
        "A scatter layer holds at most {MAX_INSTANCES} instances"
    );
    ensure!(
        bytes.len() == 12 + count * RECORD_BYTES,
        "Scatter placement file is {} bytes, {} expected for {count} instances",
        bytes.len(),
        12 + count * RECORD_BYTES
    );
    let float =
        |at: usize| f32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    let mut out = Vec::with_capacity(count);
    for n in 0..count {
        let at = 12 + n * RECORD_BYTES;
        let instance = Instance {
            position: [float(at), float(at + 4), float(at + 8)],
            rotation: [
                float(at + 12),
                float(at + 16),
                float(at + 20),
                float(at + 24),
            ],
            scale: [float(at + 28), float(at + 32), float(at + 36)],
            tint: [
                bytes[at + 40],
                bytes[at + 41],
                bytes[at + 42],
                bytes[at + 43],
            ],
        };
        instance.validate()?;
        out.push(instance);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instance(x: f32) -> Instance {
        Instance {
            position: [x, 2.0, -3.0],
            rotation: Quat::from_rotation_y(0.5).to_array(),
            scale: [1.0, 2.0, 3.0],
            tint: [10, 20, 30, 255],
        }
    }

    #[test]
    fn placements_survive_a_round_trip() -> Result<()> {
        let list = vec![instance(1.0), instance(-4.5)];
        let bytes = encode(&list)?;
        assert_eq!(bytes.len(), 12 + 2 * RECORD_BYTES);
        assert_eq!(decode(&bytes)?, list);
        Ok(())
    }

    #[test]
    fn a_damaged_file_is_refused() -> Result<()> {
        let bytes = encode(&[instance(1.0)])?;
        assert!(decode(&bytes[..bytes.len() - 1]).is_err(), "truncated");
        let mut wrong = bytes.clone();
        wrong[0] = b'X';
        assert!(decode(&wrong).is_err(), "magic");
        let mut version = bytes.clone();
        version[4] = 2;
        assert!(decode(&version).is_err(), "version");
        let mut count = bytes.clone();
        count[8] = 9;
        assert!(decode(&count).is_err(), "count");
        let mut nan = bytes;
        nan[12..16].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(decode(&nan).is_err(), "non-finite");
        Ok(())
    }

    #[test]
    fn an_instance_needs_a_unit_rotation() {
        let mut bad = instance(0.0);
        bad.rotation = [0.0, 0.0, 0.0, 2.0];
        assert!(bad.validate().is_err());
    }

    #[test]
    fn the_matrix_scales_then_rotates_then_moves() {
        let i = Instance {
            position: [10.0, 0.0, 0.0],
            rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2).to_array(),
            scale: [2.0, 1.0, 1.0],
            tint: [255; 4],
        };
        let moved = i.matrix().transform_point3(Vec3::X);
        assert!(
            (moved - Vec3::new(10.0, 2.0, 0.0)).length() < 1e-5,
            "{moved}"
        );
    }

    #[test]
    fn visibility_is_whole_inside_the_near_distance_fades_to_nothing_and_stops() {
        let fade = Some([10.0, 30.0]);
        assert_eq!(visibility(fade, 5.0), 1.0);
        assert_eq!(visibility(fade, 10.0), 1.0);
        assert!((visibility(fade, 20.0) - 0.5).abs() < 1e-6);
        assert_eq!(visibility(fade, 30.0), 0.0);
        assert_eq!(visibility(fade, 31.0), 0.0);
        assert_eq!(visibility(None, 1e6), 1.0);
    }

    #[test]
    fn a_record_needs_a_proper_fade_range() {
        let mut record = Record {
            name: "grass".into(),
            asset: "m/grass".into(),
            file: "grass.bin".into(),
            fade: Some([5.0, 50.0]),
            order: Order::FarToNear,
            modulate_2x: true,
            detail: 1,
        };
        assert!(record.validate().is_ok());
        record.fade = Some([50.0, 5.0]);
        assert!(record.validate().is_err());
        record.fade = Some([-1.0, 5.0]);
        assert!(record.validate().is_err());
        record.fade = None;
        record.detail = 4;
        assert!(record.validate().is_err());
    }
}
