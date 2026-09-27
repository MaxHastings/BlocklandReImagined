//! Offline collision baking from converted native assets, never visual fallback.
use anyhow::{Result, ensure};
use bri_content::{
    animation,
    brick::Brick,
    collision::{CollisionBody, Part},
    shape::Shape,
};
use glam::Vec3;
use std::collections::BTreeMap;

pub fn bake(id: String, brick: &Brick, external: Option<&Shape>) -> Result<CollisionBody> {
    let mut parts = Vec::new();
    if let Some(shape) = external {
        shape.validate()?;
        let mut pose = animation::sample(shape, None, 0.0)?;
        pose.visibility.fill(1.0);
        for (detail_index, detail) in shape
            .details
            .iter()
            .enumerate()
            .filter(|(_, d)| d.collision)
        {
            // Keep each detail/object convex piece, including disjoint collision details.
            let mut groups: BTreeMap<usize, Vec<animation::PosedTriangle>> = BTreeMap::new();
            for triangle in animation::triangles(shape, &pose, detail_index, |_| true)? {
                groups.entry(triangle.object).or_default().push(triangle);
            }
            for (object, geometry) in groups {
                let mut vertices = Vec::<[f32; 3]>::new();
                let mut triangles = Vec::new();
                for triangle in geometry {
                    let indices = triangle.vertices.map(|v| {
                        let p = v.position.to_array();
                        if let Some(index) = vertices.iter().position(|x| *x == p) {
                            index as u32
                        } else {
                            let index = vertices.len();
                            vertices.push(p);
                            index as u32
                        }
                    });
                    triangles.push(indices);
                }
                let label = format!("{}/{}", detail.name, shape.objects[object].name);
                verify_convex(&vertices, &triangles)?;
                parts.push(Part::Convex {
                    label,
                    vertices,
                    triangles,
                });
            }
        }
    } else {
        ensure!(
            !brick.needs_external_collision,
            "Brick needs external collision data"
        );
        parts.extend(brick.collision_boxes.iter().map(|b| Part::Box {
            center: b.center,
            size: b.size,
        }));
    }
    let body = CollisionBody { id, parts };
    body.validate()?;
    Ok(body)
}

/// A convex hull would silently fill a concavity. Reject authored faces with
/// vertices on both sides of their plane before handing the recipe to physics.
fn verify_convex(vertices: &[[f32; 3]], triangles: &[[u32; 3]]) -> Result<()> {
    let points: Vec<_> = vertices.iter().copied().map(Vec3::from).collect();
    for triangle in triangles {
        let [a, b, c] = triangle.map(|i| points[i as usize]);
        let normal = (b - a).cross(c - a).normalize_or_zero();
        if normal == Vec3::ZERO {
            continue;
        }
        let (mut low, mut high) = (0.0_f32, 0.0_f32);
        for p in &points {
            let d = normal.dot(*p - a);
            low = low.min(d);
            high = high.max(d);
        }
        ensure!(
            low >= -0.0001 || high <= 0.0001,
            "Authored collision is concave; needs explicit decomposition, not automatic hull replacement"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_faces_cutting_through_a_piece() {
        let points = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
        ];
        assert!(verify_convex(&points, &[[0, 1, 2]]).is_err());
        assert!(verify_convex(&points[..4], &[[0, 1, 2], [0, 3, 1], [0, 2, 3], [1, 3, 2]]).is_ok());
    }
}
