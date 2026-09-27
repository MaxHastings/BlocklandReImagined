//! Physics adapter for portable native collision recipes.
use anyhow::{Context, Result};
use bri_content::collision::{CollisionBody, Part};
use rapier3d::prelude::*;

pub fn collider(body: &CollisionBody) -> Result<ColliderBuilder> {
    body.validate()?;
    let parts = body
        .parts
        .iter()
        .map(|part| match part {
            Part::Box { center, size } => Ok((
                Pose::translation(center[0], center[1], center[2]),
                SharedShape::cuboid(size[0] * 0.5, size[1] * 0.5, size[2] * 0.5),
            )),
            Part::Convex { vertices, .. } => {
                let points: Vec<_> = vertices.iter().map(|p| Vector::from_array(*p)).collect();
                Ok((
                    Pose::IDENTITY,
                    SharedShape::convex_hull(&points)
                        .context("Degenerate native convex collision")?,
                ))
            }
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(ColliderBuilder::compound(parts))
}

/// Weld duplicate source corners and suppress coplanar internal-edge contacts.
/// Architectural collision stays separate from visual faces and never becomes
/// a single hull spanning doorways or openings.
pub fn interior_collider(
    detail: &bri_content::interior::Detail,
    transform: glam::Mat4,
) -> Result<ColliderBuilder> {
    let geometry = &detail.collision_triangles;
    let points = geometry
        .iter()
        .flatten()
        .map(|v| Vector::from_array(transform.transform_point3(glam::Vec3::from(*v)).to_array()))
        .collect();
    let indices = (0..geometry.len() as u32)
        .map(|i| [i * 3, i * 3 + 1, i * 3 + 2])
        .collect();
    Ok(ColliderBuilder::trimesh_with_flags(
        points,
        indices,
        TriMeshFlags::FIX_INTERNAL_EDGES,
    )?)
}

/// Authored collision details for fixed map models. Visual leaves/billboards
/// never substitute for collision meshes; models without collision stay decorative.
pub fn static_shape_colliders(
    shape: &bri_content::shape::Shape,
    transform: glam::Mat4,
) -> Result<Vec<ColliderBuilder>> {
    anyhow::ensure!(
        transform.is_finite() && transform.determinant().abs() > 1e-8,
        "Invalid static shape placement"
    );
    shape.validate()?;
    let mut pose = bri_content::animation::sample(shape, None, 0.0)?;
    pose.visibility.fill(1.0);
    let mut colliders = Vec::new();
    for (detail, _) in shape
        .details
        .iter()
        .enumerate()
        .filter(|(_, d)| d.collision)
    {
        let triangles = bri_content::animation::triangles(shape, &pose, detail, |_| true)?;
        if triangles.is_empty() {
            continue;
        }
        let points = triangles
            .iter()
            .flat_map(|t| {
                t.vertices
                    .iter()
                    .map(|v| Vector::from_array(transform.transform_point3(v.position).to_array()))
            })
            .collect();
        let indices = (0..triangles.len() as u32)
            .map(|i| [i * 3, i * 3 + 1, i * 3 + 2])
            .collect();
        colliders.push(ColliderBuilder::trimesh_with_flags(
            points,
            indices,
            TriMeshFlags::FIX_INTERNAL_EDGES,
        )?);
    }
    Ok(colliders)
}

/// Precise local-space targeting against authored convex planes. Gameplay can
/// use physics broad-phase candidates, then this for selection/placement. This
/// avoids both GJK edge tolerance and triangle seam misses. Direction need not
/// be normalized: the returned parameter uses the caller's ray units.
pub fn raycast(
    body: &CollisionBody,
    origin: Vector,
    direction: Vector,
    max_time: f32,
) -> Option<(f32, Vector)> {
    body.parts
        .iter()
        .filter_map(|part| {
            let planes: Vec<_> = match part {
                Part::Box { center, size } => {
                    let center = Vector::from_array(*center);
                    let half = Vector::from_array(*size) * 0.5;
                    [
                        Vector::X,
                        -Vector::X,
                        Vector::Y,
                        -Vector::Y,
                        Vector::Z,
                        -Vector::Z,
                    ]
                    .map(|n| (center + half * n, n))
                    .to_vec()
                }
                Part::Convex {
                    vertices,
                    triangles,
                    ..
                } => {
                    let center = vertices
                        .iter()
                        .map(|p| Vector::from_array(*p))
                        .sum::<Vector>()
                        / vertices.len() as f32;
                    triangles
                        .iter()
                        .filter_map(|t| {
                            let [a, b, c] = t.map(|i| Vector::from_array(vertices[i as usize]));
                            let mut n = (b - a).cross(c - a).normalize_or_zero();
                            if n == Vector::ZERO {
                                return None;
                            }
                            if n.dot(center - a) > 0.0 {
                                n = -n;
                            }
                            Some((a, n))
                        })
                        .collect()
                }
            };
            let (mut enter, mut exit, mut normal) = (0.0_f32, max_time, Vector::ZERO);
            for (point, n) in planes {
                let distance = n.dot(origin - point);
                let speed = n.dot(direction);
                if speed.abs() < 1e-8 {
                    if distance > 1e-6 {
                        return None;
                    }
                    continue;
                }
                let crossing = -distance / speed;
                if speed < 0.0 {
                    if crossing > enter {
                        enter = crossing;
                        normal = n;
                    }
                } else {
                    exit = exit.min(crossing);
                }
                if enter > exit + 1e-6 {
                    return None;
                }
            }
            (enter <= max_time && exit >= 0.0).then_some((enter, normal))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn static_model_collision_uses_authored_detail_even_when_visually_hidden() -> Result<()> {
        use bri_content::shape::{Detail, Mesh, Object, Primitive, Shape};
        let mesh = |size: f32, z: f32| Mesh {
            frame_vertices: 3,
            positions: vec![[-size, -size, z], [size, -size, z], [0.0, size, z]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            uv: vec![[0.0; 2]; 3],
            primitives: vec![Primitive {
                material: None,
                triangles: vec![[0, 1, 2]],
            }],
            skin: None,
            billboard: false,
            billboard_y: false,
        };
        let mut shape = Shape {
            schema_version: 1,
            id: "fixture".into(),
            nodes: vec![],
            objects: vec![Object {
                name: "trunk".into(),
                node: None,
                meshes: vec![0, 1],
                visibility: 0.0,
                frame: 0,
                material_frame: 0,
            }],
            details: vec![
                Detail {
                    name: "visible".into(),
                    pixel_threshold: 100.0,
                    object_start: 0,
                    object_count: 1,
                    mesh_offset: 0,
                    collision: false,
                },
                Detail {
                    name: "collision".into(),
                    pixel_threshold: -1.0,
                    object_start: 0,
                    object_count: 1,
                    mesh_offset: 1,
                    collision: true,
                },
            ],
            meshes: vec![Some(mesh(20.0, 0.0)), Some(mesh(0.2, 1.0))],
            materials: vec![],
            animations: vec![],
        };
        let transform = glam::Mat4::from_scale_rotation_translation(
            glam::Vec3::new(2.0, 3.0, 4.0),
            glam::Quat::IDENTITY,
            glam::Vec3::new(3.0, 4.0, 5.0),
        );
        let colliders = static_shape_colliders(&shape, transform)?;
        assert_eq!(colliders.len(), 1);
        let collider = colliders.into_iter().next().unwrap().build();
        let cast = |x: f32| {
            collider.shape().cast_ray(
                &Pose::IDENTITY,
                &Ray::new(Vector::new(x, 4.0, 20.0), -Vector::Z),
                30.0,
                true,
            )
        };
        assert!((cast(3.0).unwrap() - 11.0).abs() < 1e-5);
        assert!(cast(4.6).is_none(), "Visual mesh must not become collision");
        shape.details.pop();
        assert!(static_shape_colliders(&shape, transform)?.is_empty());
        Ok(())
    }
    #[test]
    fn targeting_handles_seams_misses_and_inside_rays() {
        let body = CollisionBody {
            id: "tetra".into(),
            parts: vec![Part::Convex {
                label: "tetra".into(),
                vertices: vec![
                    [0.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                    [0.0, 0.0, 1.0],
                ],
                triangles: vec![[0, 1, 2], [0, 3, 1], [0, 2, 3], [1, 3, 2]],
            }],
        };
        let seam = raycast(&body, Vector::new(0.5, 2.0, 0.5), -Vector::Y, 10.0).unwrap();
        assert!((seam.0 - 2.0).abs() < 1e-6);
        assert!(raycast(&body, Vector::new(0.5, 2.0, 0.5001), -Vector::Y, 10.0).is_none());
        assert_eq!(
            raycast(&body, Vector::splat(0.1), Vector::Y, 10.0)
                .unwrap()
                .0,
            0.0
        );
        assert!(raycast(&body, Vector::new(0.2, 2.0, 0.2), -Vector::Y, 1.0).is_none());
    }
}
