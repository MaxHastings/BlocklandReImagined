//! Native scene placements and retained declarative environment settings.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Debug, Serialize, Deserialize)]
pub struct Scene {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub nodes: Vec<Node>,
    /// Provenance-only requirements. Runtime must not execute original scripts.
    #[serde(default)]
    pub pending_scripts: Vec<PendingScript>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct PendingScript {
    pub section: String,
    pub first_line: usize,
    pub last_line: usize,
    pub sha256: String,
}
impl PendingScript {
    pub fn diagnostic(&self) -> String {
        format!(
            "Mission behavior requires native adaptation: {} object export, source lines {}-{}, SHA-256 {}",
            self.section, self.first_line, self.last_line, self.sha256
        )
    }
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Node {
    pub name: String,
    pub parent: Option<usize>,
    pub kind: Kind,
    pub transform: [f32; 16],
    pub asset: Option<String>,
    pub properties: BTreeMap<String, String>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Group,
    Metadata,
    Interior,
    Terrain,
    StaticModel,
    DatablockModel,
    Spawn,
    Sky,
    Sun,
    Water,
    Precipitation,
    Foliage,
    Bounds,
    Unadapted,
}
