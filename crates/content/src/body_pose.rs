//! Pure body-basis calculations shared by pose consumers. Clip selection,
//! playback state and gameplay origin policy belong to their existing owners.
use crate::{animation::Pose, shape::Shape};
use glam::{Quat, Vec3};

/// Model-space root shift for an already selected active authored seat context.
/// The caller retains its mounted sit/root/dead eligibility checks. An owned
/// clip preserves its model root; a missing clip anchors its sampled hip to an
/// explicit world point or, without one, to the legacy seat/model origin.
/// Raw rider frame components preserve the existing invalid-input guards.
pub fn seat_root_shift(
    shape: &Shape,
    sampled: &Pose,
    owns_clip: bool,
    fallback_world_hip: Option<Vec3>,
    rider_feet: Vec3,
    rider_rotation: Option<Quat>,
    rider_scale: f32,
) -> Option<Vec3> {
    if owns_clip {
        return Some(Vec3::ZERO);
    }
    let parents: Vec<_> = shape.nodes.iter().map(|n| n.parent).collect();
    let bind = crate::animation::sample(shape, None, 0.0).ok()?;
    let hip = crate::hold::hip(&parents, &bind.nodes)?;
    let target = fallback_world_hip
        .filter(|p| p.is_finite())
        .zip(rider_rotation.filter(|q| q.is_finite() && q.is_normalized()))
        .filter(|_| rider_scale.is_finite() && rider_scale > 0. && rider_feet.is_finite())
        .map(|(world, rotation)| rotation.inverse() * (world - rider_feet) / rider_scale)
        .filter(|p| p.is_finite());
    let shift = target.unwrap_or(Vec3::ZERO) - sampled.nodes.get(hip)?.w_axis.truncate();
    (shift.is_finite() && (target.is_some() || shift.length() < 8.)).then_some(shift)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::{Context, Result};
    use glam::Mat4;

    fn fixture() -> Result<(Shape, Pose, usize)> {
        let rig = crate::testing::avatar::rig();
        let sit = rig.sequence("sit").context("fixture sit")?;
        let sampled = crate::animation::sample(&rig.shape, Some(sit), sit.duration)?;
        let bind = crate::animation::sample(&rig.shape, None, 0.)?;
        let parents: Vec<_> = rig.shape.nodes.iter().map(|n| n.parent).collect();
        let hip = crate::hold::hip(&parents, &bind.nodes).context("fixture hip")?;
        Ok((rig.shape, sampled, hip))
    }
    #[test]
    fn explicit_world_hip_is_independent_of_rider_scale_and_rotation() -> Result<()> {
        let (shape, sampled, hip) = fixture()?;
        let feet = Vec3::new(3., 4., 5.);
        let target = Vec3::new(-2., 3., 8.);
        for scale in [0.7, 1., 1.8] {
            for yaw in [-0.8, 0.4] {
                let rotation = Quat::from_rotation_z(0.3) * Quat::from_rotation_y(yaw);
                let shift = seat_root_shift(
                    &shape,
                    &sampled,
                    false,
                    Some(target),
                    feet,
                    Some(rotation),
                    scale,
                )
                .context("shift")?;
                let model =
                    Mat4::from_scale_rotation_translation(Vec3::splat(scale), rotation, feet);
                let actual = model.transform_point3(sampled.nodes[hip].w_axis.truncate() + shift);
                assert!(actual.abs_diff_eq(target, 1e-5));
            }
        }
        Ok(())
    }
    #[test]
    fn owned_root_and_legacy_invalid_input_behavior_are_preserved() -> Result<()> {
        let (shape, sampled, hip) = fixture()?;
        assert_eq!(
            seat_root_shift(
                &shape,
                &sampled,
                true,
                Some(Vec3::NAN),
                Vec3::NAN,
                None,
                f32::NAN
            ),
            Some(Vec3::ZERO)
        );
        let legacy = -sampled.nodes[hip].w_axis.truncate();
        for (target, feet, rotation, scale) in [
            (None, Vec3::ZERO, Some(Quat::IDENTITY), 1.),
            (Some(Vec3::NAN), Vec3::ZERO, Some(Quat::IDENTITY), 1.),
            (Some(Vec3::ONE), Vec3::NAN, Some(Quat::IDENTITY), 1.),
            (Some(Vec3::ONE), Vec3::ZERO, None, 1.),
            (Some(Vec3::ONE), Vec3::ZERO, Some(Quat::NAN), 1.),
            (
                Some(Vec3::ONE),
                Vec3::ZERO,
                Some(Quat::from_array([0., 0., 0., 2.])),
                1.,
            ),
            (Some(Vec3::ONE), Vec3::ZERO, Some(Quat::IDENTITY), 0.),
            (
                Some(Vec3::ONE),
                Vec3::ZERO,
                Some(Quat::IDENTITY),
                f32::INFINITY,
            ),
        ] {
            assert_eq!(
                seat_root_shift(&shape, &sampled, false, target, feet, rotation, scale),
                Some(legacy)
            );
        }
        Ok(())
    }
    #[test]
    fn legacy_bound_and_unavailable_hip_remain_explicit() -> Result<()> {
        let (shape, mut sampled, hip) = fixture()?;
        sampled.nodes[hip] = Mat4::from_translation(Vec3::Y * 8.);
        assert_eq!(
            seat_root_shift(
                &shape,
                &sampled,
                false,
                None,
                Vec3::ZERO,
                Some(Quat::IDENTITY),
                1.
            ),
            None
        );
        assert_eq!(
            seat_root_shift(
                &shape,
                &sampled,
                false,
                Some(Vec3::ZERO),
                Vec3::ZERO,
                Some(Quat::IDENTITY),
                1.
            ),
            Some(-Vec3::Y * 8.)
        );
        sampled.nodes[hip] = Mat4::NAN;
        assert_eq!(
            seat_root_shift(
                &shape,
                &sampled,
                false,
                Some(Vec3::ZERO),
                Vec3::ZERO,
                Some(Quat::IDENTITY),
                1.
            ),
            None
        );
        let legless =
            crate::testing::rigid_shape("test:legless", &[("root", None, [0.; 3])], &[], vec![]);
        let rest = crate::animation::sample(&legless, None, 0.)?;
        assert_eq!(
            seat_root_shift(
                &legless,
                &rest,
                false,
                None,
                Vec3::ZERO,
                Some(Quat::IDENTITY),
                1.
            ),
            None
        );
        Ok(())
    }
}
