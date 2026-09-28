//! Native scene placements and retained declarative environment settings.
use crate::interior::Interior;
use glam::{Mat4, Vec3};
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

/// Height of one brick plate: v20 stacks bricks on this lattice, anchored at
/// world height zero.
pub const PLATE_HEIGHT: f32 = 0.2;

impl Scene {
    /// The height to move this map by so the interior floor under its first
    /// spawn lies on the nearest plate plane. Map floors need not lie on the
    /// lattice: Bedroom's carpet is at 286.312, so v20 rests bricks 0.088
    /// above it (every stock Bedroom save has its baseplates at 286.4), and
    /// Tutorial's floor is at 94.406, where its own saved bricks dip 0.006 in.
    /// The nearest plane is where v20's shipped layouts sit on every stock
    /// map, so moving the whole map there, rather than moving the lattice,
    /// keeps those layouts in place and brings the floor to meet them. Maps
    /// without an interior floor under the spawn (terrain maps) do not move.
    pub fn floor_lift<'a>(&self, interiors: impl Fn(&str) -> Option<&'a Interior>) -> f32 {
        let Some(spawn) = self.nodes.iter().find(|n| matches!(n.kind, Kind::Spawn)) else {
            return 0.0;
        };
        let spawn = Vec3::new(
            spawn.transform[12],
            spawn.transform[13],
            spawn.transform[14],
        );
        let mut floor = None::<f32>;
        for node in self
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, Kind::Interior))
        {
            let Some(interior) = node.asset.as_deref().and_then(&interiors) else {
                continue;
            };
            let Some(detail) = interior.details.first() else {
                continue;
            };
            let matrix = Mat4::from_cols_array(&node.transform);
            for triangle in &detail.collision_triangles {
                let [a, b, c] = triangle.map(|p| matrix.transform_point3(Vec3::from(p)));
                let flat = (a.y - b.y).abs() < 1e-3 && (a.y - c.y).abs() < 1e-3;
                if flat
                    && a.y <= spawn.y
                    && floor.is_none_or(|f| a.y > f)
                    && covers([a, b, c], spawn)
                {
                    floor = Some(a.y);
                }
            }
        }
        floor.map_or(0.0, |floor| {
            (floor / PLATE_HEIGHT).round() * PLATE_HEIGHT - floor
        })
    }
    /// Move every placement up by `height` (see [`Scene::floor_lift`]).
    /// Terrain origins and water volumes live outside the scene; callers
    /// raise them by the same height.
    pub fn lift(&mut self, height: f32) {
        for node in &mut self.nodes {
            node.transform[13] += height;
        }
    }
}
/// Whether the XZ projection of `triangle` contains `point`.
fn covers([a, b, c]: [Vec3; 3], point: Vec3) -> bool {
    let side = |p: Vec3, q: Vec3| (point.x - q.x) * (p.z - q.z) - (p.x - q.x) * (point.z - q.z);
    let d = [side(a, b), side(b, c), side(c, a)];
    d.iter().all(|&v| v >= 0.0) || d.iter().all(|&v| v <= 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interior::Detail;

    fn node(kind: Kind, y: f32, asset: Option<&str>) -> Node {
        let mut transform = Mat4::IDENTITY.to_cols_array();
        transform[13] = y;
        Node {
            name: String::new(),
            parent: None,
            kind,
            transform,
            asset: asset.map(str::to_owned),
            properties: BTreeMap::new(),
        }
    }

    #[test]
    fn floor_under_spawn_moves_to_the_nearest_plate_plane() {
        // Bedroom: an interior placed at 348.812 with its carpet 62.5 below.
        let carpet = [
            [-10.0, -62.5, -10.0],
            [10.0, -62.5, -10.0],
            [0.0, -62.5, 10.0],
        ];
        let interior = Interior {
            schema_version: 1,
            id: "room".into(),
            details: vec![Detail {
                minimum_pixels: 0,
                materials: vec![],
                surfaces: vec![],
                lightmaps: vec![],
                collision_triangles: vec![carpet],
                convex_hulls: vec![],
                ambient: [0; 4],
                alarm_ambient: [0; 4],
                has_alarm: false,
            }],
            subobjects: vec![],
            vehicle_collision: None,
        };
        let mut scene = Scene {
            schema_version: 1,
            id: "room".into(),
            name: "Room".into(),
            nodes: vec![
                node(Kind::Spawn, 287.415, None),
                node(Kind::Interior, 348.812, Some("room")),
            ],
            pending_scripts: vec![],
        };
        let lift = scene.floor_lift(|id| (id == "room").then_some(&interior));
        assert!((lift - 0.088).abs() < 1e-3, "{lift}");
        scene.lift(lift);
        assert!((scene.nodes[1].transform[13] - 62.5 - 286.4).abs() < 1e-3);
        // Nothing under the spawn: the map stays put.
        scene.nodes[0].transform[12] = 100.0;
        assert_eq!(scene.floor_lift(|_| Some(&interior)), 0.0);
    }
}
