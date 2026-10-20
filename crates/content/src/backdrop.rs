//! A map's backdrop: a model drawn before the map, from a fixed eye of its own, with only the viewer's rotation.
//!
//! The map's far scenery (a sky room, a distant city) lives in its own small space. Each frame the renderer draws it first, looking
//! from `eye` in the direction the player looks and with the player's projection, then clears depth and draws the map over it. Wherever
//! the map draws nothing (an opening cut into it) the backdrop shows. The bundle's `backdrops` table holds one [`Record`] for the map.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Record {
    /// The shape asset the backdrop draws.
    pub asset: String,
    /// Where the backdrop is viewed from, in the shape's own frame and the import's scale.
    pub eye: [f32; 3],
}

impl Record {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.asset.is_empty() && self.eye.iter().all(|v| v.is_finite() && v.abs() < 1e7),
            "A map's backdrop needs a model and a finite eye"
        );
        Ok(())
    }
}
