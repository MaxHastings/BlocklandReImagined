use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const BRICK_SCHEMA: u32 = 1;
pub const STUD: f32 = 0.5;
pub const PLATE: f32 = 0.2;

#[derive(Debug, Serialize, Deserialize)]
pub struct Catalog {
    pub schema_version: u32,
    pub bricks: Vec<CatalogEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub id: String,
    pub display_name: String,
    pub category: String,
    pub subcategory: String,
    pub mesh_id: String,
    pub collision_source: Option<String>,
    pub icon_source: String,
    pub print_aspect_ratio: Option<String>,
    pub orientation_fix: u8,
    pub can_cover: bool,
    pub indestructible: bool,
    pub special_kind: Option<String>,
    /// Declarative source expressions retained for later feature adaptation.
    pub other_properties: std::collections::BTreeMap<String, String>,
}

impl CatalogEntry {
    /// Hidden state variants still need native geometry and stable save identity.
    pub fn selectable(&self) -> bool {
        !self.display_name.is_empty() && !self.category.is_empty() && !self.subcategory.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Brick {
    pub schema_version: u32,
    pub id: String,
    pub footprint_studs: [u32; 2],
    pub height_plates: u32,
    /// Slice order: original depth, descending height, then width left to right.
    /// b=attach both, u=above, d=below, x=occupied, -=empty.
    pub attachment_rows: Vec<String>,
    pub collision_boxes: Vec<CollisionBox>,
    /// A missing BLB box is not permission to use visual triangles for collision.
    pub needs_external_collision: bool,
    /// In order top, bottom, original north/east/south/west.
    pub coverage: Option<[Coverage; 6]>,
    pub quads: Vec<Quad>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollisionBox {
    pub center: [f32; 3],
    pub size: [f32; 3],
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Coverage {
    pub hides_adjacent: bool,
    pub required_area: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    Top,
    Side,
    BottomEdge,
    BottomLoop,
    Ramp,
    Print,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Face {
    Top,
    Bottom,
    North,
    East,
    South,
    West,
    Omni,
}
pub const FACES: [Face; 7] = [
    Face::Top,
    Face::Bottom,
    Face::North,
    Face::East,
    Face::South,
    Face::West,
    Face::Omni,
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quad {
    pub face: Face,
    pub surface: Surface,
    /// Counterclockwise winding in native X-right/Y-up/-Z-forward coordinates.
    pub vertices: [Vertex; 4],
    /// Authored RGBA values may include legacy additive/subtractive sentinels.
    /// Absent means use the player's paint color, not authored white.
    pub colors: Option<[[f32; 4]; 4]>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

impl Brick {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema_version == BRICK_SCHEMA, "Unknown brick schema");
        ensure!(!self.id.is_empty(), "Empty brick ID");
        let [width, depth] = self.footprint_studs;
        ensure!(
            width > 0 && depth > 0 && self.height_plates > 0,
            "Empty brick dimensions"
        );
        ensure!(
            u64::from(width) * u64::from(depth) * u64::from(self.height_plates) <= 2_000_000,
            "Brick grid too large"
        );
        ensure!(
            self.attachment_rows.len() == (depth * self.height_plates) as usize,
            "Attachment row count mismatch"
        );
        for row in &self.attachment_rows {
            ensure!(
                row.len() == width as usize && row.bytes().all(|b| b"budx-".contains(&b)),
                "Invalid attachment grid row"
            );
        }
        ensure!(
            !self.quads.is_empty() && self.quads.len() <= 100_000,
            "Invalid quad count"
        );
        for quad in &self.quads {
            for v in &quad.vertices {
                ensure!(
                    v.position
                        .iter()
                        .chain(&v.normal)
                        .chain(&v.uv)
                        .all(|v| v.is_finite()),
                    "Non-finite brick vertex"
                );
                let length: f32 = v.normal.iter().map(|v| v * v).sum();
                ensure!((length - 1.0).abs() < 0.001, "Non-unit normal");
            }
            if let Some(colors) = &quad.colors {
                ensure!(
                    colors.iter().flatten().all(|c| c.is_finite()),
                    "Non-finite color"
                );
            }
        }
        for b in &self.collision_boxes {
            ensure!(
                b.center.iter().all(|v| v.is_finite())
                    && b.size.iter().all(|v| v.is_finite() && *v > 0.0),
                "Invalid collision box"
            );
        }
        Ok(())
    }
}
