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
use glam::{Mat4, Quat, Vec3};

/// An existing action request in the caller's monotonic animation time domain.
#[derive(Clone, Copy)]
pub struct ActionRef<'input> {
    pub sequence: &'input str,
    pub started_at: f64,
}

/// An additive clip a seat holds at a place along its length, ahead of the
/// seat's aim and look overlays: Halo's acceleration overlays, sampled
/// continuously across their keys. `phase` runs 0..1 across the clip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeatOverlay<'input> {
    pub clip: &'input str,
    pub phase: f32,
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
    /// A seat's authored overlays, applied before `seat_aim`/`look`. Clips a
    /// body lacks, or that are not additive, are skipped and reported.
    pub seat_overlays: &'input [SeatOverlay<'input>],
    /// Normalized phase from the caller's existing pitch/limit mapping.
    pub look_phase: f32,
    pub head_yaw: f32,
    /// The body is crouched: its `look_crouch` clip, if it has one, replaces `look`.
    pub crouched: bool,
    /// The body is prone: its `look_prone` clip, if it has one, replaces `look_crouch`.
    pub prone: bool,
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
    for overlay in input.seat_overlays {
        match named(&overlay.clip.to_ascii_lowercase()).filter(|clip| clip.additive) {
            Some(clip) if overlay.phase.is_finite() => overlays.push(Layer {
                animation: clip,
                time: overlay.phase.clamped(0., 1.) * clip.duration,
                weight: 1.,
            }),
            Some(_) => {}
            None => skipped.push(overlay.clip.to_ascii_lowercase()),
        }
    }
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
    } else if let Some(look) = role(if input.prone {
        Role::LookProne
    } else if input.crouched {
        Role::LookCrouch
    } else {
        Role::Look
    }) {
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

/// Existing fitted-body payload; cache ownership and invalidation belong to the caller.
#[derive(Debug, Default, PartialEq)]
pub struct BodyFits {
    pub first_person: [bool; 2],
    pub hands: [Option<crate::hold::Fit>; 2],
    /// Legacy unscaled model-space seat shift relative to the reference body.
    pub seat: Option<Vec3>,
}

/// Resolved inputs from one immutable native rig and the actual body definition.
pub struct BodyFitInput<'a> {
    pub rig: &'a Rig,
    pub roles: &'a RoleClips,
    pub fitted: bool,
    pub stand_in: bool,
    pub preserve_hand_pose: bool,
    pub hands: [Option<usize>; 2],
    pub eye: Option<usize>,
    pub reference_seat_hip: Option<Vec3>,
}

/// Sample idle, own-hand ready and level look using existing native fitting.
/// Missing limbs or failed per-hand samples leave that fit absent.
pub fn build_body_fits(input: BodyFitInput<'_>) -> BodyFits {
    let roles = input.roles;
    let mut fits = BodyFits::default();
    if input.fitted && !input.stand_in {
        let clip = |role: Role| input.rig.sequence(roles.clip(role)?.0);
        let parents: Vec<Option<usize>> = input.rig.shape.nodes.iter().map(|n| n.parent).collect();
        let ends = input
            .hands
            .map(|n| n.filter(|&i| i < input.rig.shape.nodes.len()));
        for (hand, aim) in [Role::AimRight, Role::AimLeft].into_iter().enumerate() {
            if input.preserve_hand_pose {
                continue;
            }
            let Some(end) = ends[hand] else {
                continue;
            };
            let mut layers: Vec<Layer<'_>> = [
                (clip(Role::Idle), 0.0),
                (clip(aim), 1.0),
                (clip(Role::Look), 0.5),
            ]
            .into_iter()
            .filter_map(|(animation, at)| {
                let animation = animation?;
                Some(Layer {
                    animation,
                    time: animation.duration * at,
                    weight: 1.0,
                })
            })
            .collect();
            layers.sort_by_key(|l| (l.animation.additive, l.animation.priority));
            let Ok(mut pose) = crate::animation::sample_layers(&input.rig.shape, &layers) else {
                continue;
            };
            let landmark = ends[1 - hand].or(input.eye);
            if let Some(fit) = crate::hold::fit(
                &parents,
                &mut pose.nodes,
                end,
                landmark,
                clip(aim).is_none(),
                hand == 0,
            ) {
                fits.hands[hand] = Some(fit);
            }
        }
        fits.first_person = std::array::from_fn(|n| {
            if input.preserve_hand_pose {
                ends[n].is_some()
            } else {
                fits.hands[n].is_some()
            }
        });
        let hip = crate::hold::sit_hip(&input.rig.shape, clip(Role::Sit));
        fits.seat = input
            .reference_seat_hip
            .zip(hip)
            .map(|(blockhead, own)| blockhead - own)
            .filter(|shift| shift.is_finite() && shift.length() < 8.0);
    }
    fits
}

/// Already selected hold eligibility; thread2 liveness remains with the caller.
#[derive(Clone, Copy, Debug, Default)]
pub struct BodyHolds {
    pub raised: [bool; 2],
    pub pitch: f32,
    pub support: Option<Vec3>,
    /// World-space seat targets, right then left.
    pub seat: [Option<Vec3>; 2],
}

/// Nodes and indices must describe the same native rig. Invalid hand indices
/// are absent operations; a parent-count mismatch leaves all nodes unchanged.
pub struct BodyFittingInput<'a> {
    pub parents: &'a [Option<usize>],
    pub hands: [Option<usize>; 2],
    pub eye: Option<usize>,
    pub fits: Option<&'a BodyFits>,
    pub right_socket_offset: Vec3,
    /// Completed model frame including any root/seat/posture composition.
    pub model_transform: Mat4,
    pub holds: BodyHolds,
}

/// Apply existing generic raising/support, then independent explicit seat reaches.
/// Authored-hand generic bypass must never suppress the world seat targets.
pub fn apply_body_fitting(nodes: &mut [Mat4], input: BodyFittingInput<'_>) {
    apply_generic_fitting(nodes, &input);
    if input.holds.seat.iter().any(Option::is_some) {
        let inverse = input.model_transform.inverse();
        let ends = input.hands.map(|n| n.filter(|&i| i < nodes.len()));
        if inverse.is_finite() && input.parents.len() == nodes.len() {
            for n in 0..2 {
                if let (Some(end), Some(hold)) = (ends[n], input.holds.seat[n])
                    && hold.is_finite()
                {
                    crate::hold::reach(
                        input.parents,
                        nodes,
                        end,
                        ends[1 - n].or(input.eye),
                        inverse.transform_point3(hold),
                    );
                }
            }
        }
    }
}

fn apply_generic_fitting(nodes: &mut [Mat4], input: &BodyFittingInput<'_>) {
    let Some(fits) = input.fits else {
        return;
    };
    if fits.hands.iter().all(Option::is_none) {
        return;
    }
    let parents = input.parents;
    if parents.len() != nodes.len() {
        return;
    }
    let ends = input.hands.map(|n| n.filter(|&i| i < nodes.len()));
    let landmark = |n: usize| ends[1 - n].or(input.eye);
    let pitch = if input.holds.pitch.is_finite() {
        input.holds.pitch
    } else {
        0.0
    };
    for (n, end) in ends.into_iter().enumerate() {
        if !input.holds.raised[n] || input.holds.seat[n].is_some() {
            continue;
        }
        let (Some(end), Some(fit)) = (end, &fits.hands[n]) else {
            continue;
        };
        if let Some(raise) = fit.hold
            && raise.shoulder < nodes.len()
        {
            let shoulder = nodes[raise.shoulder].w_axis.truncate();
            crate::hold::reach(
                parents,
                nodes,
                end,
                landmark(n),
                raise.target(shoulder, pitch),
            );
        }
    }
    if let (Some(point), Some(right), Some(left), Some(fit)) =
        (input.holds.support, ends[0], ends[1], &fits.hands[0])
        && input.holds.seat[1].is_none()
        && point.is_finite()
    {
        let offset = input.right_socket_offset;
        // The right hand as `mount_node` draws it.
        let hand = if input.holds.raised[0] && fit.hold.is_some() {
            Mat4::from_rotation_translation(
                crate::hold::Raise::aim(pitch),
                nodes[right].w_axis.truncate(),
            )
        } else {
            nodes[right] * Mat4::from_quat(fit.correction)
        };
        let grip = hand * Mat4::from_translation(offset);
        crate::hold::reach(
            parents,
            nodes,
            left,
            Some(right),
            grip.transform_point3(point),
        );
    }
}

/// One already resolved native socket. The caller owns mount lookup, body
/// identity and eligibility; this payload carries no clocks or gameplay policy.
pub struct BodySocketInput<'a> {
    pub model_transform: Mat4,
    pub node: Mat4,
    pub fit: Option<&'a crate::hold::Fit>,
    pub raised: bool,
    pub pitch: f32,
    pub offset: Vec3,
    pub tool_action: Option<Mat4>,
}

fn hand_correction(fit: Option<&crate::hold::Fit>) -> Mat4 {
    fit.map_or(Mat4::IDENTITY, |fit| Mat4::from_quat(fit.correction))
}

/// Existing node/correction/offset/tool-action multiplication order. Raised
/// fitted hands keep their sampled position while pointing along the aim.
/// As in the native caller, this does not impose a new matrix validity filter.
pub fn body_mount_node(input: BodySocketInput<'_>) -> Mat4 {
    let hand = if input.raised && input.fit.is_some_and(|fit| fit.hold.is_some()) {
        Mat4::from_rotation_translation(
            crate::hold::Raise::aim(input.pitch),
            input.node.w_axis.truncate(),
        )
    } else {
        input.node * hand_correction(input.fit)
    };
    input.model_transform
        * hand
        * Mat4::from_translation(input.offset)
        * input.tool_action.unwrap_or(Mat4::IDENTITY)
}

/// Existing offset-only socket in the caller's completed seated presentation
/// frame. Caller retains the offset-socket and finite aim-pitch eligibility.
pub fn body_offset_socket(
    seated_frame: Mat4,
    offset: Vec3,
    pitch: f32,
    tool_action: Option<Mat4>,
) -> Mat4 {
    seated_frame
        * Mat4::from_translation(offset)
        * Mat4::from_rotation_x(pitch)
        * tool_action.unwrap_or(Mat4::IDENTITY)
}

/// Existing acted/unacted delta in the corrected mount frame. A foreign-tool
/// override is selected before this call, even when no unacted sample exists.
pub fn body_mount_action(
    unacted_node: Mat4,
    posed_node: Mat4,
    fit: Option<&crate::hold::Fit>,
) -> Option<Mat4> {
    let correction = hand_correction(fit);
    let rest = unacted_node * correction;
    let moved = rest.inverse() * posed_node * correction;
    nonidentity_action(moved)
}

fn nonidentity_action(delta: Mat4) -> Option<Mat4> {
    (delta.is_finite() && !delta.abs_diff_eq(Mat4::IDENTITY, 1e-6)).then_some(delta)
}

/// Presentation-only fitted-hand fallback in the eye's scaled frame; never a
/// gameplay eye or shot-origin rule. The caller resolves the actual socket offset.
pub fn first_person_hand_frame(
    fits: &BodyFits,
    hand: usize,
    model_transform: Mat4,
    offset: Vec3,
) -> Option<Mat4> {
    if !*fits.first_person.get(hand)? {
        return None;
    }
    let mut at = crate::hold::FIRST_PERSON_HAND;
    if hand == 1 {
        at.x = -at.x;
    }
    let scale = model_transform.x_axis.truncate().length();
    let local = Mat4::from_scale(Vec3::splat(scale))
        * Mat4::from_rotation_translation(crate::hold::AIM_HAND, at)
        * Mat4::from_translation(offset);
    local.is_finite().then_some(local)
}

/// Sample only the reference tool's right-ready/action socket displacements,
/// never reference skeleton deformation for another body. `action` belongs to
/// this immutable reference rig; played time and fallback eligibility stay with
/// the caller. The two indices explicitly describe its existing hand mounts.
pub fn sample_reference_tool_action(
    rig: &Rig,
    mounts: [Option<usize>; 2],
    action: &Animation,
    played: f32,
) -> Result<[Option<Mat4>; 2]> {
    let mut base = Vec::new();
    if let Some(ready) = rig.sequence("armreadyright") {
        base.push(Layer {
            animation: ready,
            time: ready.duration,
            weight: 1.,
        });
    }
    let rest = crate::animation::sample_layers(&rig.shape, &base)?;
    base.push(Layer {
        animation: action,
        time: played,
        weight: 1.,
    });
    base.sort_by_key(|layer| (layer.animation.additive, layer.animation.priority));
    let moved = crate::animation::sample_layers(&rig.shape, &base)?;
    Ok(mounts.map(|index| {
        let index = index?;
        let delta = rest.nodes.get(index)?.inverse() * *moved.nodes.get(index)?;
        nonidentity_action(delta)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::{Context, Result};
    use glam::Mat4;

    #[test]
    fn socket_helpers_keep_absence_nonidentity_and_finite_contracts() -> Result<()> {
        let rig = crate::testing::avatar::rig();
        let action = rig.sequence("activate2").context("reference action")?;
        assert_eq!(
            sample_reference_tool_action(&rig, [Some(usize::MAX), None], action, 0.1)?,
            [None; 2]
        );
        assert_eq!(
            body_mount_action(Mat4::IDENTITY, Mat4::IDENTITY, None),
            None
        );
        assert_eq!(body_mount_action(Mat4::ZERO, Mat4::IDENTITY, None), None);
        assert_eq!(body_mount_action(Mat4::IDENTITY, Mat4::NAN, None), None);
        let fits = BodyFits {
            first_person: [true; 2],
            ..Default::default()
        };
        assert!(first_person_hand_frame(&fits, usize::MAX, Mat4::IDENTITY, Vec3::ZERO).is_none());
        assert!(first_person_hand_frame(&fits, 0, Mat4::NAN, Vec3::ZERO).is_none());
        assert!(
            first_person_hand_frame(&fits, 0, Mat4::ZERO, Vec3::ZERO).is_some(),
            "legacy presentation checks finite values, not invertibility"
        );
        let right = first_person_hand_frame(&fits, 0, Mat4::IDENTITY, Vec3::ZERO).unwrap();
        let left = first_person_hand_frame(&fits, 1, Mat4::IDENTITY, Vec3::ZERO).unwrap();
        assert_eq!(right.w_axis.x, -left.w_axis.x);
        assert_eq!(right.w_axis.y, left.w_axis.y);
        assert_eq!(right.w_axis.z, left.w_axis.z);
        assert!(
            first_person_hand_frame(&BodyFits::default(), 0, Mat4::IDENTITY, Vec3::ZERO).is_none()
        );
        Ok(())
    }

    #[test]
    fn fitting_invalid_indices_and_parent_counts_are_bounded() -> Result<()> {
        let rig = crate::testing::avatar::rig();
        let roles = RoleClips::resolve(&Default::default(), |n| rig.sequence(n).is_some());
        for (fitted, stand_in, preserve) in [
            (true, false, false),
            (true, false, true),
            (true, true, false),
            (false, false, false),
        ] {
            let fits = build_body_fits(BodyFitInput {
                rig: &rig,
                roles: &roles,
                fitted,
                stand_in,
                preserve_hand_pose: preserve,
                hands: [Some(usize::MAX), None],
                eye: Some(usize::MAX),
                reference_seat_hip: Some(Vec3::NAN),
            });
            assert_eq!(fits, BodyFits::default());
        }
        let mut nodes = crate::animation::sample(&rig.shape, None, 0.)?.nodes;
        let before = nodes.clone();
        let parents: Vec<_> = rig.shape.nodes.iter().map(|n| n.parent).collect();
        for bad_parents in [false, true] {
            apply_body_fitting(
                &mut nodes,
                BodyFittingInput {
                    parents: if bad_parents { &parents[..1] } else { &parents },
                    hands: [Some(usize::MAX), None],
                    eye: Some(usize::MAX),
                    fits: None,
                    right_socket_offset: Vec3::ZERO,
                    model_transform: Mat4::IDENTITY,
                    holds: BodyHolds {
                        seat: [Some(Vec3::ONE); 2],
                        ..Default::default()
                    },
                },
            );
            assert_eq!(nodes, before);
        }
        Ok(())
    }

    #[test]
    fn fitting_seat_inverse_and_generic_bypass_remain_independent() -> Result<()> {
        let rig = crate::testing::avatar::rig();
        let parents: Vec<_> = rig.shape.nodes.iter().map(|n| n.parent).collect();
        let ends = ["Mount0", "Mount1"]
            .map(|name| rig.shape.nodes.iter().position(|n| n.name == name).unwrap());
        let bind = crate::animation::sample(&rig.shape, None, 0.)?.nodes;
        let fits = BodyFits::default();
        let frame = Mat4::from_scale_rotation_translation(
            Vec3::splat(1.7),
            Quat::from_rotation_z(0.4),
            Vec3::new(2., 3., 4.),
        );
        let targets = ends.map(|i| bind[i].w_axis.truncate() + Vec3::new(0.02, 0.08, -0.1));
        let mut expected = bind.clone();
        for n in 0..2 {
            crate::hold::reach(
                &parents,
                &mut expected,
                ends[n],
                Some(ends[1 - n]),
                targets[n],
            );
        }
        for fit in [None, Some(&fits)] {
            for model in [frame, Mat4::ZERO, Mat4::NAN] {
                let mut nodes = bind.clone();
                apply_body_fitting(
                    &mut nodes,
                    BodyFittingInput {
                        parents: &parents,
                        hands: ends.map(Some),
                        eye: None,
                        fits: fit,
                        right_socket_offset: Vec3::ZERO,
                        model_transform: model,
                        holds: BodyHolds {
                            raised: [true; 2],
                            pitch: f32::NAN,
                            support: Some(Vec3::ONE),
                            seat: targets.map(|v| Some(frame.transform_point3(v))),
                        },
                    },
                );
                let want = if model == frame { &expected } else { &bind };
                assert!(nodes.iter().zip(want).all(|(a, b)| a.abs_diff_eq(*b, 2e-5)));
            }
        }
        assert!(
            expected
                .iter()
                .zip(&bind)
                .any(|(a, b)| !a.abs_diff_eq(*b, 1e-4))
        );
        Ok(())
    }

    fn planning_clip(name: &str, priority: i32, additive: bool, duration: f32) -> Animation {
        Animation {
            name: name.into(),
            frames: 2,
            duration,
            looping: false,
            additive,
            parent_translations: false,
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
            seat_overlays: &[],
            look_phase: 0.25,
            head_yaw: 0.,
            crouched: false,
            prone: false,
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
    fn a_crouched_body_uses_its_crouch_look_and_falls_back_to_look() -> Result<()> {
        let mut rig = planning_rig();
        let roles = RoleClips::resolve(&Default::default(), |n| rig.sequence(n).is_some());
        let mut input = planning_input(rig.sequence("base"), &[]);
        input.crouched = true;
        let plain = plan_body_layers(&rig, &roles, input)?;
        assert_eq!(
            names(&plain.unacted),
            ["base", "look"],
            "no crouch look: look"
        );
        let crouch_look = planning_clip("lookcrouch", 16, true, 2.);
        rig.sequences.insert(crouch_look.name.clone(), crouch_look);
        let roles = RoleClips::resolve(&Default::default(), |n| rig.sequence(n).is_some());
        let mut input = planning_input(rig.sequence("base"), &[]);
        input.crouched = true;
        let crouched = plan_body_layers(&rig, &roles, input)?;
        assert_eq!(names(&crouched.unacted), ["base", "lookcrouch"]);
        let standing = plan_body_layers(&rig, &roles, planning_input(rig.sequence("base"), &[]))?;
        assert_eq!(
            names(&standing.unacted),
            ["base", "look"],
            "standing keeps look"
        );
        assert!(
            !roles.report().iter().any(|l| l.contains("look_crouch")),
            "the optional crouch look is not reported missing"
        );
        Ok(())
    }
    #[test]
    fn a_prone_body_uses_its_prone_look_then_its_crouch_look_then_look() -> Result<()> {
        let mut rig = planning_rig();
        let prone = |rig: &Rig| -> Result<Vec<String>> {
            let roles = RoleClips::resolve(&Default::default(), |n| rig.sequence(n).is_some());
            let mut input = planning_input(rig.sequence("base"), &[]);
            input.crouched = true;
            input.prone = true;
            let plan = plan_body_layers(rig, &roles, input)?;
            Ok(names(&plan.unacted).iter().map(|n| n.to_string()).collect())
        };
        assert_eq!(
            prone(&rig)?,
            ["base", "look"],
            "no prone or crouch look: look"
        );
        let crouch_look = planning_clip("lookcrouch", 16, true, 2.);
        rig.sequences.insert(crouch_look.name.clone(), crouch_look);
        assert_eq!(
            prone(&rig)?,
            ["base", "lookcrouch"],
            "no prone look: the crouch look"
        );
        let prone_look = planning_clip("lookprone", 16, true, 2.);
        rig.sequences.insert(prone_look.name.clone(), prone_look);
        assert_eq!(prone(&rig)?, ["base", "lookprone"]);
        let roles = RoleClips::resolve(&Default::default(), |n| rig.sequence(n).is_some());
        let mut input = planning_input(rig.sequence("base"), &[]);
        input.crouched = true;
        let crouched = plan_body_layers(&rig, &roles, input)?;
        assert_eq!(
            names(&crouched.unacted),
            ["base", "lookcrouch"],
            "crouching only is not prone"
        );
        assert!(!roles.report().iter().any(|l| l.contains("look_prone")));
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
    fn seat_overlays_sample_additive_clips_ahead_of_the_aim_and_report_the_rest() -> Result<()> {
        let mut rig = planning_rig();
        for (name, additive) in [("accel_a", true), ("accel_b", true), ("rigid", false)] {
            let clip = planning_clip(name, 3, additive, 4.);
            rig.sequences.insert(clip.name.clone(), clip);
        }
        let overlays = [
            SeatOverlay {
                clip: "ACCEL_A",
                phase: 0.25,
            },
            SeatOverlay {
                clip: "accel_b",
                phase: 7.,
            },
            SeatOverlay {
                clip: "rigid",
                phase: 0.5,
            },
            SeatOverlay {
                clip: "absent",
                phase: 0.5,
            },
            SeatOverlay {
                clip: "accel_a",
                phase: f32::NAN,
            },
        ];
        rig.sequences.get_mut("seataim").unwrap().additive = true;
        let roles = RoleClips::resolve(&Default::default(), |n| rig.sequence(n).is_some());
        let mut input = planning_input(rig.sequence("base"), &[]);
        input.seat_aim = Some(0.5);
        input.seat_overlays = &overlays;
        let plan = plan_body_layers(&rig, &roles, input)?;
        let layer = |name: &str| plan.layers.iter().position(|l| l.animation.name == name);
        let at = |name: &str| plan.layers[layer(name).unwrap()].time;
        // Phase runs 0..1 across the clip's own length, clamped.
        assert_eq!(at("accel_a"), 1.);
        assert_eq!(at("accel_b"), 4.);
        // They sit before the aim overlay, as Halo applies them.
        assert!(layer("accel_a") < layer("seataim") && layer("accel_b") < layer("seataim"));
        // A rigid clip is no overlay and a missing one is none: both are
        // reported by name.
        assert!(layer("rigid").is_none());
        assert_eq!(
            plan.skipped,
            vec!["rigid".to_string(), "absent".to_string()]
        );
        // Still a sampleable stack.
        crate::animation::sample_layers_with_transition(
            &rig.shape,
            &plan.layers,
            plan.transition_at,
            None,
        )?;
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
