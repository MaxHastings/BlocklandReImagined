//! Editable native content. This crate has no dependency on legacy readers.

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub mod animation;
pub mod atmosphere;
pub mod avatar;
pub mod brick;
pub mod collision;
pub mod effects;
pub mod interior;
pub mod passage;
pub mod scene;
pub mod shape;
pub mod terrain_field;
pub mod terrain_mesh;
pub mod tutorial;
pub mod water;

pub const TERRAIN_SCHEMA: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Terrain {
    pub schema_version: u32,
    /// Stable authored identity; never a GPU or physics handle.
    pub id: String,
    pub side: u32,
    /// Row-major elevations in native world units (one original world unit).
    /// Map placement and horizontal spacing belong to the map instance.
    pub elevations: Vec<f32>,
    pub layers: Vec<TerrainLayer>,
    /// Original primary layer indices, retained independently of blend weights.
    pub primary_layers: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TerrainLayer {
    pub slot: u8,
    /// Unresolved source reference until the material conversion pass resolves it.
    pub material: String,
    /// Full authored per-sample blend weights, not a dominant-layer approximation.
    pub weights: Vec<u8>,
}

impl Terrain {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == TERRAIN_SCHEMA,
            "Unknown terrain schema {}",
            self.schema_version
        );
        ensure!(!self.id.is_empty(), "Empty content ID");
        ensure!(
            (2..=4096).contains(&self.side),
            "Invalid terrain dimensions"
        );
        let count = (self.side as usize)
            .checked_mul(self.side as usize)
            .unwrap();
        ensure!(
            self.elevations.len() == count && self.primary_layers.len() == count,
            "Terrain sample count mismatch"
        );
        ensure!(
            self.elevations.iter().all(|h| h.is_finite()),
            "Non-finite terrain elevation"
        );
        ensure!(
            !self.layers.is_empty() && self.layers.len() <= 8,
            "Invalid layer count"
        );
        let mut slots = [false; 8];
        for layer in &self.layers {
            ensure!(
                layer.slot < 8 && !slots[layer.slot as usize],
                "Invalid/duplicate layer slot"
            );
            slots[layer.slot as usize] = true;
            ensure!(
                !layer.material.is_empty() && layer.weights.len() == count,
                "Invalid material or blend sample count"
            );
        }
        ensure!(
            self.primary_layers
                .iter()
                .all(|i| *i < 8 && slots[*i as usize]),
            "Primary layer references absent material"
        );
        Ok(())
    }
}
pub mod brick_materials;
pub mod environment;
