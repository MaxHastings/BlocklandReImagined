//! Projected pictures on the map: blob shadows, stains, light patterns.
//!
//! A decal is a picture projected onto the map's surfaces through a volume: every triangle of the map that the volume touches is drawn
//! again with the picture on it, a little in front of itself ([`bri_render::decals`]). The volume is a world-to-picture matrix
//! (orthographic, or perspective with a divide by the third component) and six planes that choose the triangles. A decal says how the
//! picture joins the frame, whether it wraps, and which surfaces take it.
//!
//! Everything is in the map's native frame; whatever computed the matrix and planes (a baker for another engine's projector) is
//! not this crate's business. The bundle's `decals` table lists the decals; the pictures are PNG files beside it.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

/// How the picture joins what is already on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Blend {
    /// 2 x picture x frame: mid-grey leaves the frame as it is, darker darkens it.
    Modulate2x,
    /// The picture by its own alpha.
    Alpha,
    /// Added to the frame.
    Add,
}

/// Which triangles of one class take the decal, and how the picture fades on them.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Receivers {
    pub enabled: bool,
    /// A triangle whose unit normal `n` has `n . forward >= facing_limit` is left out (use 0 to take all front faces, a
    /// positive number to take everything).
    pub facing_limit: f32,
    /// The picture fades with `max(0, -n . forward)` at each vertex.
    pub fade_by_facing: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Decal {
    /// The picture's PNG, relative to the decals file.
    pub picture: String,
    /// World position to `(s, t, q, 1)`, column-major.
    pub world_to_texture: [f32; 16],
    /// Divide `s` and `t` by `q`.
    pub perspective: bool,
    /// Inside when `n . p - w >= 0`: near, four sides, far.
    pub planes: [[f32; 4]; 6],
    /// The direction the volume looks along.
    pub forward: [f32; 3],
    pub blend: Blend,
    /// Repeat the picture past its edge on `s` and `t` (otherwise the edge is extended).
    pub wrap: [bool; 2],
    pub two_sided: bool,
    /// The picture turns about its centre by this many degrees a second (a fan's shadow); the receivers are chosen for the
    /// volume as given, so give planes wide enough for the whole turn.
    #[serde(default)]
    pub spin_degrees_per_second: f32,
    /// The map's flat surfaces (walls, floors, meshes).
    pub surfaces: Receivers,
    /// The map's ground (blended-layer materials).
    pub ground: Receivers,
}

impl Decal {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.picture.is_empty()
                && !self.picture.contains(['/', '\\'])
                && self.world_to_texture.iter().all(|v| v.is_finite())
                && self.planes.iter().flatten().all(|v| v.is_finite())
                && self.forward.iter().all(|v| v.is_finite())
                && self.spin_degrees_per_second.is_finite()
                && self.surfaces.facing_limit.is_finite()
                && self.ground.facing_limit.is_finite(),
            "A decal needs a picture file name and finite numbers"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DecalSet {
    pub decals: Vec<Decal>,
    /// What the baker could not honour (receiver classes it cannot tell apart, ...), for the log.
    #[serde(default)]
    pub notes: Vec<String>,
}

impl DecalSet {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.decals.len() <= 4096, "A map names at most 4096 decals");
        for d in &self.decals {
            d.validate()?;
        }
        Ok(())
    }
}
