//! Collision-checked host spawn candidates from native authored spawn regions.
use crate::player::PlayerTuning;
use anyhow::{Context, Result, ensure};
use bri_content::scene::{Kind, Scene};
use glam::Vec3;
use rapier3d::prelude::*;

pub fn candidates(
    physics: &PhysicsWorld,
    scene: &Scene,
    tuning: &PlayerTuning,
) -> Result<Vec<Vec3>> {
    let shape = SharedShape::cuboid(
        tuning.width * 0.5,
        tuning.stand_height * 0.5,
        tuning.width * 0.5,
    );
    let query = physics.query_pipeline_with_filter(QueryFilter::default().exclude_sensors());
    let clear = |p: Vec3| {
        query
            .intersect_shape(
                Pose::translation(p.x, p.y + tuning.stand_height * 0.5, p.z),
                shape.as_ref(),
            )
            .next()
            .is_none()
    };
    let mut points = vec![];
    for node in scene.nodes.iter().filter(|n| matches!(n.kind, Kind::Spawn)) {
        let origin = Vec3::new(node.transform[12], node.transform[13], node.transform[14]);
        let radius = node
            .properties
            .get("radius")
            .map(|v| v.parse::<f32>())
            .transpose()?
            .unwrap_or(0.0);
        let ray_height = node
            .properties
            .get("rayheight")
            .map(|v| v.parse::<f32>())
            .transpose()?
            .unwrap_or(150.0);
        ensure!(
            origin.is_finite()
                && radius.is_finite()
                && (0.0..=10000.).contains(&radius)
                && ray_height.is_finite()
                && (0.0..=10000.).contains(&ray_height),
            "Invalid authored spawn region"
        );
        // The exact legacy weighted random selection remains a fidelity adapter.
        // Stable center-first candidates preserve existing successful starts and
        // provide disjoint alternatives for later players, without moving peers.
        for attempt in 0..65 {
            let r = if attempt == 0 {
                0.0
            } else {
                radius * (attempt as f32 / 64.).sqrt()
            };
            let angle = attempt as f32 * 2.3999631;
            let point = origin + Vec3::new(r * angle.cos(), 0., r * angle.sin());
            let valid = if clear(point) {
                Some(point)
            } else {
                let from = point + Vec3::Y * ray_height.max(tuning.stand_height + 0.1);
                query
                    .cast_ray_and_get_normal(
                        &Ray::new(Vector::from_array(from.to_array()), -Vector::Y),
                        ray_height.max(tuning.stand_height + 0.1) * 2.,
                        true,
                    )
                    .filter(|(_, hit)| hit.normal.y >= tuning.slope_degrees.to_radians().cos())
                    .map(|(_, hit)| from + Vec3::Y * (0.01 - hit.time_of_impact))
                    .filter(|p| clear(*p))
            };
            if let Some(point) = valid
                && points
                    .iter()
                    .all(|p: &Vec3| p.distance(point) > tuning.width * 1.1)
            {
                points.push(point);
            }
            if points.len() >= 64 {
                return Ok(points);
            }
            if radius == 0.0 {
                break;
            }
        }
    }
    points
        .first()
        .context("No unobstructed player spawn in authored regions")?;
    Ok(points)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn touching_marker_is_lifted_to_floor_and_blocked_regions_reject() {
        let scene = Scene {
            schema_version: 1,
            id: "test".into(),
            name: "test".into(),
            pending_scripts: vec![],
            nodes: vec![bri_content::scene::Node {
                name: "spawn".into(),
                parent: None,
                kind: Kind::Spawn,
                transform: glam::Mat4::IDENTITY.to_cols_array(),
                asset: None,
                properties: Default::default(),
            }],
        };
        let mut world = bri_physics::new_world();
        world.insert_collider(
            ColliderBuilder::cuboid(10., 0.5, 10.).translation(Vector::new(0., -0.5, 0.)),
            None,
        );
        world.detect_collisions(&(), &());
        let points = candidates(&world, &scene, &PlayerTuning::default()).unwrap();
        assert_eq!(points.len(), 1);
        assert!((points[0].y - 0.01).abs() < 1e-4);
        world.insert_collider(ColliderBuilder::cuboid(20., 400., 20.), None);
        world.detect_collisions(&(), &());
        assert!(candidates(&world, &scene, &PlayerTuning::default()).is_err());
    }
}
