//! Pure layer plans and body-basis calculations. Playback state, clocks and
//! gameplay origin policy remain with their existing owners.
use crate::{
    animation::{Layer, Pose},
    avatar::Rig,
    body::{Role, RoleClips},
    shape::{Animation, Shape},
};
use anyhow::{Context, Result};
use bri_console::Clamp;
use glam::{Quat, Vec3};

/// An existing action request in the caller's monotonic animation time domain.
#[derive(Clone, Copy)]
pub struct ActionRef<'input> {
    pub sequence: &'input str,
    pub started_at: f64,
}

/// Already selected/clocked inputs to the existing body layer composition.
/// The base and crouch clips must belong to the supplied immutable rig.
pub struct LayerPlanInput<'rig, 'input> {
    pub base: Option<Layer<'rig>>,
    pub ready_role: Option<Role>,
    /// The caller has verified the active owned seat clip and actual image set.
    pub omit_ready: bool,
    pub crouch: Option<Layer<'rig>>,
    /// Existing adapter order, without imposing a new action-count limit.
    pub actions: &'input [Option<ActionRef<'input>>],
    pub time: f64,
    pub head_up: bool,
    pub seat_aim: Option<f32>,
    /// Normalized phase from the caller's existing pitch/limit mapping.
    pub look_phase: f32,
    pub head_yaw: f32,
}

/// Layers borrow only the rig; temporary input names do not escape the call.
pub struct BodyLayerPlan<'rig> {
    pub layers: Vec<Layer<'rig>>,
    pub unacted: Vec<Layer<'rig>>,
    pub transition_at: usize,
    pub unacted_transition_at: usize,
    pub acted: bool,
    pub skipped: Vec<String>,
}

fn role_clip<'rig>(rig: &'rig Rig, roles: &RoleClips, role: Role) -> Option<&'rig Animation> {
    rig.sequence(roles.clip(role)?.0)
}

/// Plan the existing ordered body layers without advancing or retaining state.
/// Sparse node channels, sampling and transition blending remain in animation's
/// native sampler. Callers retain their existing input validation and clocks.
pub fn plan_body_layers<'rig>(
    rig: &'rig Rig,
    roles: &RoleClips,
    input: LayerPlanInput<'rig, '_>,
) -> Result<BodyLayerPlan<'rig>> {
    let role = |r| role_clip(rig, roles, r);
    let named = |name: &str| match Role::from_blockhead_clip(name) {
        Some(r) => role(r),
        None => rig.sequence(name),
    };
    let base = input.base.map(|l| l.animation);
    let mut layers: Vec<_> = input.base.into_iter().collect();
    if !input.omit_ready
        && let Some(clip) = input.ready_role.and_then(role)
    {
        layers.push(Layer {
            animation: clip,
            time: clip.duration,
            weight: 1.,
        });
    }
    layers.extend(input.crouch);
    let mut absolute_actions = Vec::new();
    let mut additive_actions = Vec::new();
    let mut skipped = Vec::new();
    for action in input.actions.iter().flatten() {
        let name = action.sequence.to_ascii_lowercase();
        if name == "root" {
            continue;
        }
        let Some(clip) = named(&name) else {
            skipped.push(name);
            continue;
        };
        let played = (input.time - action.started_at).max(0.) as f32;
        if clip.additive {
            additive_actions.push(Layer {
                animation: clip,
                time: played,
                weight: 1.,
            });
        } else {
            let at = if clip.looping && clip.duration > 0. {
                played.rem_euclid(clip.duration)
            } else {
                played.min(clip.duration)
            };
            absolute_actions.push(Layer {
                animation: clip,
                time: at,
                weight: 1.,
            });
        }
    }
    let order = |layers: &mut Vec<Layer<'_>>| {
        layers.sort_by_key(|l| (l.animation.additive, l.animation.priority));
    };
    let mut unacted = layers.clone();
    order(&mut unacted);
    layers.extend(absolute_actions.iter().copied());
    order(&mut layers);
    let mut overlays = Vec::new();
    if input.head_up
        && let Some(clip) = role(Role::HeadUp)
    {
        overlays.push(Layer {
            animation: clip,
            time: clip.duration,
            weight: 1.,
        });
    }
    let seat_aim = input
        .seat_aim
        .filter(|a| a.is_finite())
        .zip(role(Role::SeatAim));
    if let Some((aim, clip)) = seat_aim {
        overlays.push(Layer {
            animation: clip,
            time: aim.clamped(0., 1.) * clip.duration,
            weight: 1.,
        });
    } else if let Some(look) = role(Role::Look) {
        overlays.push(Layer {
            animation: look,
            time: input.look_phase * look.duration,
            weight: 1.,
        });
    }
    if let Some(clip) = role(Role::HeadSide) {
        overlays.push(Layer {
            animation: clip,
            time: (0.5 + input.head_yaw / std::f32::consts::PI).clamped(0., 1.) * clip.duration,
            weight: 1.,
        });
    }
    layers.extend(overlays.iter().copied());
    unacted.extend(overlays);
    let acted = !absolute_actions.is_empty() || !additive_actions.is_empty();
    layers.extend(additive_actions);
    // This final partition deliberately does not re-sort priorities.
    layers.sort_by_key(|l| l.animation.additive);
    unacted.sort_by_key(|l| l.animation.additive);
    let boundary = |stack: &[Layer<'_>]| -> Result<usize> {
        Ok(match base {
            Some(clip) if !clip.additive => {
                stack
                    .iter()
                    .position(|l| std::ptr::eq(l.animation, clip))
                    .context("Missing avatar movement layer")?
                    + 1
            }
            _ => stack
                .iter()
                .position(|l| l.animation.additive)
                .unwrap_or(stack.len()),
        })
    };
    let transition_at = boundary(&layers)?;
    let unacted_transition_at = boundary(&unacted)?;
    Ok(BodyLayerPlan {
        layers,
        unacted,
        transition_at,
        unacted_transition_at,
        acted,
        skipped,
    })
}

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

    fn planning_clip(name: &str, priority: i32, additive: bool, duration: f32) -> Animation {
        Animation {
            name: name.into(),
            frames: 2,
            duration,
            looping: false,
            additive,
            priority,
            nodes: vec![crate::shape::NodeTrack {
                node: "Mount0".into(),
                translations: vec![[0.; 3]; 2],
                rotations: vec![],
                scales: vec![],
                scale_rotations: vec![],
            }],
            objects: vec![],
            rotation_interpolation: Default::default(),
            ground_translations: vec![],
            ground_rotations: vec![],
            triggers: vec![],
        }
    }
    fn planning_rig() -> Rig {
        let mut rig = crate::testing::avatar::rig();
        rig.sequences.clear();
        for c in [
            planning_clip("base", 12, false, 1.),
            planning_clip("body_action", 1, false, 1.),
            planning_clip("arm_action", 20, false, 1.),
            planning_clip("gesture_action", 0, true, 1.),
            planning_clip("look", 16, true, 2.),
            planning_clip("seataim", 17, false, 4.),
            planning_clip("armreadyright", 14, false, 3.),
        ] {
            rig.sequences.insert(c.name.clone(), c);
        }
        rig
    }
    fn planning_input<'rig, 'input>(
        base: Option<&'rig Animation>,
        actions: &'input [Option<ActionRef<'input>>],
    ) -> LayerPlanInput<'rig, 'input> {
        LayerPlanInput {
            base: base.map(|animation| Layer {
                animation,
                time: 0.1,
                weight: 1.,
            }),
            ready_role: None,
            omit_ready: false,
            crouch: None,
            actions,
            time: 0.1,
            head_up: false,
            seat_aim: None,
            look_phase: 0.25,
            head_yaw: 0.,
        }
    }
    fn names<'rig>(layers: &[Layer<'rig>]) -> Vec<&'rig str> {
        layers.iter().map(|l| l.animation.name.as_str()).collect()
    }

    #[test]
    fn plan_preserves_exact_stable_order_and_distinct_transition_boundaries() -> Result<()> {
        for base_additive in [false, true] {
            for look_additive in [false, true] {
                let mut rig = planning_rig();
                rig.sequences.get_mut("base").unwrap().additive = base_additive;
                rig.sequences.get_mut("look").unwrap().additive = look_additive;
                let roles = RoleClips::resolve(&Default::default(), |n| rig.sequence(n).is_some());
                let actions = ["body_action", "arm_action", "gesture_action"].map(|sequence| {
                    Some(ActionRef {
                        sequence,
                        started_at: 0.,
                    })
                });
                let plan =
                    plan_body_layers(&rig, &roles, planning_input(rig.sequence("base"), &actions))?;
                let (acted, unacted, at, unacted_at) = if !base_additive {
                    (
                        vec![
                            "body_action",
                            "base",
                            "arm_action",
                            "look",
                            "gesture_action",
                        ],
                        vec!["base", "look"],
                        2,
                        1,
                    )
                } else if look_additive {
                    (
                        vec![
                            "body_action",
                            "arm_action",
                            "base",
                            "look",
                            "gesture_action",
                        ],
                        vec!["base", "look"],
                        2,
                        0,
                    )
                } else {
                    (
                        vec![
                            "body_action",
                            "arm_action",
                            "look",
                            "base",
                            "gesture_action",
                        ],
                        vec!["look", "base"],
                        3,
                        1,
                    )
                };
                assert_eq!(names(&plan.layers), acted);
                assert_eq!(names(&plan.unacted), unacted);
                assert_eq!(plan.transition_at, at);
                assert_eq!(plan.unacted_transition_at, unacted_at);
                assert!(plan.acted && plan.skipped.is_empty());
                crate::animation::sample_layers_with_transition(
                    &rig.shape,
                    &plan.layers,
                    plan.transition_at,
                    None,
                )?;
                crate::animation::sample_layers_with_transition(
                    &rig.shape,
                    &plan.unacted,
                    plan.unacted_transition_at,
                    None,
                )?;
            }
        }
        Ok(())
    }
    #[test]
    fn plan_uses_actual_roles_ready_eligibility_and_selected_overlay_duration() -> Result<()> {
        let rig = planning_rig();
        let roles = RoleClips::resolve(&Default::default(), |n| rig.sequence(n).is_some());
        for (aim, overlay, time) in [
            (None, "look", 0.5),
            (Some(0.75), "seataim", 3.),
            (Some(2.), "seataim", 4.),
            (Some(f32::NAN), "look", 0.5),
        ] {
            let mut input = planning_input(rig.sequence("base"), &[]);
            input.ready_role = Some(Role::AimBoth);
            input.seat_aim = aim;
            let plan = plan_body_layers(&rig, &roles, input)?;
            let ready = plan
                .layers
                .iter()
                .find(|l| l.animation.name == "armreadyright")
                .context("resolved ready")?;
            assert_eq!(ready.time, 3.);
            assert_eq!(
                plan.layers
                    .iter()
                    .find(|l| l.animation.name == overlay)
                    .unwrap()
                    .time,
                time
            );
            assert!(!plan.acted);
        }
        let mut input = planning_input(rig.sequence("base"), &[]);
        input.ready_role = Some(Role::AimBoth);
        input.omit_ready = true;
        assert!(!names(&plan_body_layers(&rig, &roles, input)?.layers).contains(&"armreadyright"));
        Ok(())
    }
    #[test]
    fn plan_does_not_borrow_input_names_or_add_a_four_action_limit() -> Result<()> {
        let mut rig = planning_rig();
        let mut looping = planning_clip("loop_action", 20, false, 2.);
        looping.looping = true;
        rig.sequences.insert(looping.name.clone(), looping);
        let roles = RoleClips::resolve(&Default::default(), |n| rig.sequence(n).is_some());
        let plan = {
            let name = String::from("loop_action");
            let actions = [
                Some(ActionRef {
                    sequence: &name,
                    started_at: 0.,
                }),
                Some(ActionRef {
                    sequence: "gesture_action",
                    started_at: 0.,
                }),
                Some(ActionRef {
                    sequence: "ROOT",
                    started_at: 0.,
                }),
                Some(ActionRef {
                    sequence: "Missing_Name",
                    started_at: 0.,
                }),
            ];
            let mut input = planning_input(rig.sequence("base"), &actions);
            input.time = 3.5;
            plan_body_layers(&rig, &roles, input)?
        };
        assert_eq!(plan.skipped, ["missing_name"]);
        assert_eq!(
            plan.layers
                .iter()
                .find(|l| l.animation.name == "loop_action")
                .unwrap()
                .time,
            1.5
        );
        assert_eq!(
            plan.layers
                .iter()
                .find(|l| l.animation.name == "gesture_action")
                .unwrap()
                .time,
            3.5
        );
        let actions = vec![
            Some(ActionRef {
                sequence: "loop_action",
                started_at: 0.
            });
            7
        ];
        let plan = plan_body_layers(&rig, &roles, planning_input(rig.sequence("base"), &actions))?;
        assert_eq!(
            plan.layers
                .iter()
                .filter(|l| l.animation.name == "loop_action")
                .count(),
            7
        );
        assert_eq!(names(&plan.unacted), ["base", "look"]);
        Ok(())
    }

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
