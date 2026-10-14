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
                // A flat or point-like piece encloses nothing to stand on.
                if !has_volume(&vertices) {
                    continue;
                }
                if verify_convex(&vertices, &triangles).is_ok() {
                    parts.push(Part::Convex {
                        label,
                        vertices,
                        triangles,
                    });
                    continue;
                }
                // A concave authored piece (an inside corner) becomes convex
                // pieces hugging its own surface, so its hollow stays open.
                for (n, (vertices, triangles)) in
                    decompose(&vertices, &triangles).into_iter().enumerate()
                {
                    verify_convex(&vertices, &triangles)?;
                    parts.push(Part::Convex {
                        label: format!("{label}#{n}"),
                        vertices,
                        triangles,
                    });
                }
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

/// A convex piece's points and its outward triangles.
type Piece = (Vec<[f32; 3]>, Vec<[u32; 3]>);

/// Whether `vertices` span a volume: four points not on one plane.
fn has_volume(vertices: &[[f32; 3]]) -> bool {
    let points: Vec<Vec3> = vertices.iter().copied().map(Vec3::from).collect();
    let Some(&a) = points.first() else {
        return false;
    };
    let Some(&b) = points.iter().find(|p| p.distance(a) > 1e-4) else {
        return false;
    };
    let Some(normal) = points
        .iter()
        .map(|p| (b - a).cross(*p - a))
        .find(|n| n.length() > 1e-6)
    else {
        return false;
    };
    points
        .iter()
        .any(|p| normal.normalize().dot(*p - a).abs() > 1e-4)
}

/// A concave piece as convex pieces: V-HACD splits the authored triangles
/// into parts, and each piece is the convex hull of the authored surface
/// inside one part, so no piece reaches across the hollow. Pieces without
/// volume are dropped.
fn decompose(vertices: &[[f32; 3]], triangles: &[[u32; 3]]) -> Vec<Piece> {
    use parry3d::{
        math::Vector,
        transformation::vhacd::{VHACD, VHACDParameters},
    };
    let points: Vec<Vector> = vertices
        .iter()
        .map(|p| Vector::new(p[0], p[1], p[2]))
        .collect();
    let params = VHACDParameters {
        // Brick collision is a few studs of slopes and corners: fine voxels
        // and a tight concavity keep each inside corner open.
        resolution: 128,
        concavity: 0.001,
        ..VHACDParameters::default()
    };
    VHACD::decompose(&params, &points, triangles, true)
        .compute_exact_convex_hulls(&points, triangles)
        .into_iter()
        // The hull of each part's points, made again: V-HACD's faces, and
        // a float hull of a thin part, are not always convex.
        .filter_map(|(part, _)| hull(&part.iter().map(|p| [p.x, p.y, p.z]).collect::<Vec<_>>()))
        .collect()
}

/// The convex hull of a few points, exactly: every triangle of them with
/// no point beyond its plane (in f64, within a tolerance of the points'
/// extent), facing out. `None` for more than 96 points or no volume.
fn hull(points: &[[f32; 3]]) -> Option<Piece> {
    let mut unique: Vec<[f32; 3]> = vec![];
    for p in points {
        if !unique.contains(p) {
            unique.push(*p);
        }
    }
    if unique.len() > 96 || !has_volume(&unique) {
        return None;
    }
    let v: Vec<glam::DVec3> = unique.iter().map(|p| Vec3::from(*p).as_dvec3()).collect();
    let (lo, hi) = v
        .iter()
        .fold((v[0], v[0]), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
    let eps = (hi - lo).max_element() * 1e-6;
    let mut triangles = vec![];
    for i in 0..v.len() {
        for j in i + 1..v.len() {
            for k in j + 1..v.len() {
                let normal = (v[j] - v[i]).cross(v[k] - v[i]);
                if normal.length() <= eps * eps {
                    continue;
                }
                let normal = normal.normalize();
                let (mut below, mut above) = (false, false);
                for p in &v {
                    let d = normal.dot(*p - v[i]);
                    below |= d < -eps;
                    above |= d > eps;
                }
                match (below, above) {
                    (true, false) => triangles.push([i as u32, j as u32, k as u32]),
                    (false, true) => triangles.push([i as u32, k as u32, j as u32]),
                    _ => {}
                }
            }
        }
    }
    (!triangles.is_empty()).then_some((unique, triangles))
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

    /// An L-shaped prism (a 2x2 block missing one 1x1 quarter) splits into
    /// convex pieces that leave the missing quarter open.
    #[test]
    fn splits_a_concave_piece_and_keeps_its_hollow_open() {
        // Outline in XY, counter-clockwise, extruded 0..1 in Z.
        let outline = [
            [0.0, 0.0],
            [2.0, 0.0],
            [2.0, 1.0],
            [1.0, 1.0],
            [1.0, 2.0],
            [0.0, 2.0],
        ];
        let mut vertices = vec![];
        for z in [0.0, 1.0] {
            vertices.extend(outline.iter().map(|[x, y]| [*x, *y, z]));
        }
        let n = outline.len() as u32;
        // Caps as a fan from the convex corner 0 (valid for this outline).
        let mut triangles = vec![];
        for i in 1..n - 1 {
            triangles.push([0, i + 1, i]);
            triangles.push([n, n + i, n + i + 1]);
        }
        for i in 0..n {
            let j = (i + 1) % n;
            triangles.push([i, j, n + j]);
            triangles.push([i, n + j, n + i]);
        }
        assert!(verify_convex(&vertices, &triangles).is_err());
        let pieces = decompose(&vertices, &triangles);
        assert!(pieces.len() >= 2, "{} pieces", pieces.len());
        let inside = |points: &[[f32; 3]], triangles: &[[u32; 3]], p: Vec3| {
            triangles.iter().all(|t| {
                let [a, b, c] = t.map(|i| Vec3::from(points[i as usize]));
                (b - a).cross(c - a).normalize().dot(p - a) <= 1e-4
            })
        };
        for (points, triangles) in &pieces {
            verify_convex(points, triangles).unwrap();
            assert!(
                !inside(points, triangles, Vec3::new(1.6, 1.6, 0.5)),
                "a piece fills the notch"
            );
        }
        // The pieces still cover the solid's own corners.
        for corner in [
            Vec3::new(0.2, 0.2, 0.5),
            Vec3::new(1.8, 0.2, 0.5),
            Vec3::new(0.2, 1.8, 0.5),
        ] {
            assert!(
                pieces.iter().any(|(p, t)| inside(p, t, corner)),
                "nothing covers {corner}"
            );
        }
        assert!(!has_volume(&[
            [0.0; 3],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0]
        ]));
    }
}
