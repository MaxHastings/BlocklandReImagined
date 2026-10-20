//! The data of hit bones, shared by the packages that declare them and the
//! motor that places them (`bri_motor::bones` in the engine).
use anyhow::{Result, ensure};
use glam::Vec3;
use serde::{Deserialize, Serialize};

/// Most hit bones one archetype declares.
pub const MAX_HIT_BONES: usize = 64;
/// Most a bone's multiplier may be.
pub const MAX_MODIFIER: f32 = 100.0;

/// How a bone's cylinder ends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ends {
    /// Capped with half spheres: a limb.
    #[default]
    Round,
    /// Cut square: a torso or a foot.
    Flat,
}

/// The part of the body a bone belongs to, as rules see it
/// (`info.region`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Region {
    Head,
    Torso,
    Legs,
}
impl Region {
    pub fn name(self) -> &'static str {
        match self {
            Region::Head => "head",
            Region::Torso => "torso",
            Region::Legs => "legs",
        }
    }
}

/// One bone's cylinder, in the body's rest pose: from the feet, facing -Z,
/// at scale 1 (the frame of a mount point).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HitBone {
    /// The model node the cylinder follows, for the tools that draw it.
    pub bone: String,
    /// The two ends of the axis.
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub radius: f32,
    #[serde(default)]
    pub ends: Ends,
    /// Damage is multiplied by this; 0 and a shot passes through the bone.
    pub modifier: f32,
    pub region: Region,
}
impl HitBone {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.bone.is_empty()
                && self.bone.len() <= 64
                && !self.bone.chars().any(char::is_control)
                && self
                    .from
                    .iter()
                    .chain(&self.to)
                    .all(|v| v.is_finite() && v.abs() <= 64.0)
                && Vec3::from(self.from).distance(Vec3::from(self.to)) > 1e-4
                && self.radius.is_finite()
                && self.radius > 0.0
                && self.radius <= 16.0
                && self.modifier.is_finite()
                && (0.0..=MAX_MODIFIER).contains(&self.modifier),
            "Invalid hit bone {}",
            self.bone
        );
        Ok(())
    }
}
