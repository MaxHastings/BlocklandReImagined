//! Native static architecture, with separate render and authored collision data.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct Interior {
    pub schema_version: u32,
    pub id: String,
    pub details: Vec<Detail>,
    pub subobjects: Vec<Detail>,
    pub vehicle_collision: Option<VehicleCollision>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct VehicleCollision {
    pub convex_hulls: Vec<Vec<[f32; 3]>>,
    pub triangles: Vec<[[f32; 3]; 3]>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Detail {
    pub minimum_pixels: u32,
    pub materials: Vec<String>,
    pub surfaces: Vec<Surface>,
    pub lightmaps: Vec<Lightmap>,
    pub collision_triangles: Vec<[[f32; 3]; 3]>,
    pub convex_hulls: Vec<Vec<[f32; 3]>>,
    pub ambient: [u8; 4],
    pub alarm_ambient: [u8; 4],
    pub has_alarm: bool,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Surface {
    pub source_index: usize,
    pub material: usize,
    pub flags: u8,
    pub vertices: Vec<Vertex>,
    pub triangles: Vec<[u32; 3]>,
    pub lightmap: Option<usize>,
    pub alarm_lightmap: Option<usize>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub lightmap_uv: [f32; 2],
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Lightmap {
    pub png: Vec<u8>,
    pub auxiliary_png: Option<Vec<u8>>,
    pub keep: bool,
}
impl Interior {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1 && !self.id.is_empty() && !self.details.is_empty(),
            "Invalid native interior"
        );
        for detail in self.details.iter().chain(&self.subobjects) {
            for surface in &detail.surfaces {
                ensure!(
                    surface.material < detail.materials.len(),
                    "Interior material out of range"
                );
                ensure!(
                    surface.lightmap.is_none_or(|i| i < detail.lightmaps.len())
                        && surface
                            .alarm_lightmap
                            .is_none_or(|i| i < detail.lightmaps.len()),
                    "Interior lightmap out of range"
                );
                ensure!(
                    surface
                        .triangles
                        .iter()
                        .flatten()
                        .all(|i| (*i as usize) < surface.vertices.len()),
                    "Interior triangle out of range"
                );
                ensure!(
                    surface.vertices.iter().all(|v| v
                        .position
                        .iter()
                        .chain(&v.normal)
                        .chain(&v.uv)
                        .chain(&v.lightmap_uv)
                        .all(|x| x.is_finite())),
                    "Non-finite interior vertex"
                );
            }
            ensure!(
                detail
                    .collision_triangles
                    .iter()
                    .flatten()
                    .flatten()
                    .all(|v| v.is_finite()),
                "Non-finite interior collision"
            );
            ensure!(
                detail
                    .convex_hulls
                    .iter()
                    .all(|h| h.len() >= 4 && h.iter().flatten().all(|v| v.is_finite())),
                "Invalid interior hull"
            );
        }
        Ok(())
    }
}
