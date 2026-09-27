//! Native collision recipes. Physics engines build their own cached shapes from these.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct CollisionLibrary {
    pub schema_version: u32,
    pub bodies: Vec<CollisionBody>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollisionBody {
    pub id: String,
    pub parts: Vec<Part>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Part {
    Box {
        center: [f32; 3],
        size: [f32; 3],
    },
    /// Each authored convex piece stays separate; never hull the entire object.
    Convex {
        label: String,
        vertices: Vec<[f32; 3]>,
        triangles: Vec<[u32; 3]>,
    },
}

impl CollisionBody {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.id.is_empty() && !self.parts.is_empty(),
            "Empty collision body"
        );
        for part in &self.parts {
            match part {
                Part::Box { center, size } => ensure!(
                    center.iter().all(|v| v.is_finite())
                        && size.iter().all(|v| v.is_finite() && *v > 0.0),
                    "Invalid collision box"
                ),
                Part::Convex {
                    vertices,
                    triangles,
                    ..
                } => {
                    ensure!(
                        vertices.len() >= 4 && !triangles.is_empty(),
                        "Empty convex piece"
                    );
                    ensure!(
                        vertices.iter().flatten().all(|v| v.is_finite()),
                        "Non-finite collision point"
                    );
                    ensure!(
                        triangles
                            .iter()
                            .flatten()
                            .all(|v| (*v as usize) < vertices.len()),
                        "Invalid collision triangle"
                    );
                }
            }
        }
        Ok(())
    }
}
