//! How any body holds what is in its hands the way a Blockhead does: an
//! arm reached to a point (seat hand holds, a gun's second grip, an arm a
//! body has no aiming clip to raise) and a hand socket turned so that,
//! aiming, it lines up with the body as the Blockhead's `Mount0` does.
//! Host and client fit a body the same way, so the muzzle the host fires
//! from is the one every client draws.
use bri_console::Clamp;
use glam::{Mat4, Quat, Vec3};

/// Node `i`'s ancestors, nearest first.
fn ancestors(parents: &[Option<usize>], mut i: usize) -> Vec<usize> {
    let count = parents.len();
    let mut up = Vec::new();
    while let Some(p) = parents[i].filter(|p| *p < count) {
        if up.len() > count {
            break;
        }
        up.push(p);
        i = p;
    }
    up
}

/// The arm ending at `end`: its ancestors below the first one it shares
/// with `landmark` (the other hand, else the eye), nearest first; with no
/// landmark, the three nearest.
fn arm(parents: &[Option<usize>], end: usize, landmark: Option<usize>) -> Vec<usize> {
    let count = parents.len();
    let trunk = landmark.filter(|l| *l < count).map(|l| {
        let mut t = ancestors(parents, l);
        t.push(l);
        t
    });
    ancestors(parents, end)
        .into_iter()
        .take_while(|i| trunk.as_ref().is_none_or(|t| !t.contains(i)))
        .take(if trunk.is_some() { usize::MAX } else { 3 })
        .collect()
}

fn at(nodes: &[Mat4], i: usize) -> Vec3 {
    nodes[i].w_axis.truncate()
}

/// Of the arm, the (shoulder, elbow) pair giving the most even upper and
/// lower arm, with the shorter of the two.
fn even_pair(nodes: &[Mat4], arm: &[usize], end: usize) -> Option<(usize, usize, f32)> {
    let end_at = at(nodes, end);
    let mut best: Option<(usize, usize, f32)> = None;
    for (k, &mid) in arm.iter().enumerate() {
        for &root in &arm[k + 1..] {
            let (a, b) = (
                at(nodes, mid).distance(at(nodes, root)),
                end_at.distance(at(nodes, mid)),
            );
            let even = a.min(b);
            if best.is_none_or(|(_, _, e)| even > e) && even > 1e-4 {
                best = Some((root, mid, even));
            }
        }
    }
    best
}

/// Reach the arm that ends at node `end` so `end` lies at `target` (model
/// space). The arm is `end`'s ancestors below the first one it shares
/// with `landmark` (the other hand, else the eye), so the chest and
/// everything above stay put. Of that chain the shoulder and elbow are
/// the pair giving the most even upper and lower arm; an even arm bends at
/// its elbow (two-bone IK, kept in the clip's bend plane) and then swings
/// from its shoulder, while a rigid arm (a Blockhead's, one piece with a
/// short hand) only swings. A target past the arm's reach gets the arm
/// held straight toward it.
pub fn reach(
    parents: &[Option<usize>],
    nodes: &mut [Mat4],
    end: usize,
    landmark: Option<usize>,
    target: Vec3,
) {
    let count = nodes.len();
    if end >= count || parents.len() != count || !target.is_finite() {
        return;
    }
    let arm = arm(parents, end, landmark);
    let under = |root: usize| -> Vec<usize> {
        (0..count)
            .filter(|&i| i == root || ancestors(parents, i).contains(&root))
            .collect()
    };
    let turn = |nodes: &mut [Mat4], root: usize, about: Vec3, by: Quat| {
        if !by.is_finite() {
            return;
        }
        let m =
            Mat4::from_translation(about) * Mat4::from_quat(by) * Mat4::from_translation(-about);
        for i in under(root) {
            nodes[i] = m * nodes[i];
        }
    };
    let end_at = at(nodes, end);
    let best = even_pair(nodes, &arm, end);
    let two_bone = best.filter(|&(root, mid, even)| {
        let longer = at(nodes, mid)
            .distance(at(nodes, root))
            .max(end_at.distance(at(nodes, mid)));
        even >= 0.35 * longer
    });
    let shoulder_node = match two_bone {
        Some((root, mid, _)) => {
            let (s, e, h) = (at(nodes, root), at(nodes, mid), end_at);
            let (a, b) = (e.distance(s), h.distance(e));
            let d = target
                .distance(s)
                .clamped((a - b).abs() + 1e-4, a + b - 1e-4);
            let want = ((a * a + b * b - d * d) / (2.0 * a * b))
                .clamped(-1.0, 1.0)
                .acos();
            let (u, v) = ((s - e).normalize(), (h - e).normalize());
            let current = u.dot(v).clamped(-1.0, 1.0).acos();
            let axis = [u.cross(v), u.cross(target - s), Vec3::NEG_X]
                .into_iter()
                .find(|k| k.length_squared() > 1e-8)
                .unwrap_or(Vec3::NEG_X)
                .normalize();
            turn(nodes, mid, e, Quat::from_axis_angle(axis, want - current));
            root
        }
        None => match best.map(|(root, _, _)| root).or(arm.last().copied()) {
            Some(root) => root,
            None => return,
        },
    };
    let s = at(nodes, shoulder_node);
    let (from, to) = (at(nodes, end) - s, target - s);
    if from.length_squared() > 1e-8 && to.length_squared() > 1e-8 {
        turn(
            nodes,
            shoulder_node,
            s,
            Quat::from_rotation_arc(from.normalize(), to.normalize()),
        );
    }
}

/// How a hand socket's frame sits in the body's frame while it aims, for
/// every fitted body: the Blockhead's `Mount0` in `armReadyRight` with
/// its look level (`root`, `armreadyright` at its end, `look` at its
/// middle, from the converted v20 `m.dts`). What a held image is authored
/// against, so an image in any fitted hand points as it does in the
/// Blockhead's.
pub const AIM_HAND: Quat = Quat::IDENTITY;

/// Where the Blockhead's aiming right hand is from its eye (body frame,
/// facing -Z, at scale 1), from the same pose as [`AIM_HAND`]. A fitted
/// body's first-person view draws a held image with no eye offset here
/// (the left hand mirrored), so a gun sits on screen where it does for a
/// Blockhead whatever the body's size or arms.
pub const FIRST_PERSON_HAND: Vec3 = Vec3::new(0.527, -0.585, -0.654);

/// Where a fitted hand goes while it holds something: from its shoulder,
/// in the body's frame (facing -Z), aiming level, turned about the
/// shoulder by the aim's pitch. Measured from the body's own aim pose, or
/// set by the engine in front of the chest for a body with no aim clip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Raise {
    pub shoulder: usize,
    pub offset: Vec3,
}
impl Raise {
    /// Where the hand goes with the shoulder at `shoulder` (model space),
    /// aiming `pitch` up.
    pub fn target(&self, shoulder: Vec3, pitch: f32) -> Vec3 {
        shoulder + Self::aim(pitch) * self.offset
    }
    /// How a held hand turns aiming `pitch` up, in the body's frame: the
    /// hand frame a fitted body holds with.
    pub fn aim(pitch: f32) -> Quat {
        let pitch = if pitch.is_finite() { pitch } else { 0.0 };
        Quat::from_rotation_x(pitch.clamped(-1.4, 1.4)) * AIM_HAND
    }
}

/// How a fitted body holds with one hand: the rotation after its hand
/// socket's node (before the socket's offset) that lines the socket up with
/// [`AIM_HAND`] in its aim pose, and where the hand goes while it holds
/// (`None` for a hand with no arm the engine can find, which then only
/// turns).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fit {
    pub correction: Quat,
    pub hold: Option<Raise>,
}

/// Fit the hand whose socket is node `end` to a body posed for aiming
/// (`aim`: model-space nodes of its idle, aim clip and level look). With
/// `raise` (the body has no aim clip for this hand) the engine first
/// raises the arm in place, in front of the chest; `right` says which side
/// is inward. `landmark` is the other hand's node, else the eye. `None`
/// when the socket is not in the rig.
pub fn fit(
    parents: &[Option<usize>],
    aim: &mut [Mat4],
    end: usize,
    landmark: Option<usize>,
    raise: bool,
    right: bool,
) -> Option<Fit> {
    if end >= aim.len() || parents.len() != aim.len() {
        return None;
    }
    let arm = arm(parents, end, landmark);
    let shoulder = even_pair(aim, &arm, end).map(|(shoulder, elbow, _)| {
        let length =
            at(aim, shoulder).distance(at(aim, elbow)) + at(aim, elbow).distance(at(aim, end));
        (shoulder, length)
    });
    if raise && let Some((shoulder, length)) = shoulder.filter(|(_, l)| *l > 1e-3) {
        // In front of the chest, a little low and in, the elbow bent: as
        // the Blockhead's arm points at what it holds.
        let inward = if right { -1.0 } else { 1.0 };
        let offset = Vec3::new(0.15 * inward, -0.3, -0.75) * length;
        let target = at(aim, shoulder) + offset;
        reach(parents, aim, end, landmark, target);
    }
    let hold = shoulder
        .map(|(shoulder, _)| Raise {
            shoulder,
            offset: at(aim, end) - at(aim, shoulder),
        })
        .filter(|h| h.offset.is_finite());
    let (_, rotation, _) = aim[end].to_scale_rotation_translation();
    let correction = (rotation.inverse() * AIM_HAND).normalize();
    correction.is_finite().then_some(Fit { correction, hold })
}

/// A rig's hip: the nearest node both legs hang from, the legs ending at
/// the lowest node left of the middle and the lowest right of it in its
/// bind pose (`nodes`, model space). `None` for a rig without two legs.
pub fn hip(parents: &[Option<usize>], nodes: &[Mat4]) -> Option<usize> {
    if parents.len() != nodes.len() {
        return None;
    }
    let lowest = |side: f32| {
        (0..nodes.len())
            .filter(|&i| at(nodes, i).x * side > 0.05 && at(nodes, i).is_finite())
            .min_by(|&a, &b| at(nodes, a).y.total_cmp(&at(nodes, b).y))
    };
    let (left, right) = (lowest(-1.0)?, lowest(1.0)?);
    let up = ancestors(parents, right);
    ancestors(parents, left)
        .into_iter()
        .find(|i| up.contains(i))
}

/// Where a rig's [`hip`] is while it sits (its `sit` clip at its end, else
/// its bind pose), in model space: a seat holds every fitted body with its
/// hip where the Blockhead's is.
pub fn sit_hip(shape: &crate::shape::Shape, sit: Option<&crate::shape::Animation>) -> Option<Vec3> {
    let parents: Vec<Option<usize>> = shape.nodes.iter().map(|n| n.parent).collect();
    let bind = crate::animation::sample(shape, None, 0.0).ok()?;
    let hip = hip(&parents, &bind.nodes)?;
    let seated = crate::animation::sample(shape, sit, sit.map_or(0.0, |s| s.duration)).ok()?;
    let at = at(&seated.nodes, hip);
    at.is_finite().then_some(at)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Chest, shoulder, elbow, hand, and the other hand on the chest; the
    /// hand bone turned along the arm, as an imported rig's is.
    fn rig() -> ([Option<usize>; 5], [Mat4; 5]) {
        let down = Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2);
        let parents = [None, Some(0), Some(1), Some(2), Some(0)];
        let nodes = [
            Mat4::from_translation(Vec3::new(0.0, 1.5, 0.0)),
            Mat4::from_rotation_translation(down, Vec3::new(0.5, 1.5, 0.0)),
            Mat4::from_rotation_translation(down, Vec3::new(0.5, 1.0, 0.0)),
            Mat4::from_rotation_translation(down, Vec3::new(0.5, 0.5, 0.0)),
            Mat4::from_translation(Vec3::new(-0.5, 0.5, 0.0)),
        ];
        (parents, nodes)
    }

    #[test]
    fn an_even_arm_bends_its_elbow_to_reach_a_hold() {
        let (parents, rest) = rig();
        let target = Vec3::new(0.2, 1.3, -0.6);
        let mut nodes = rest;
        reach(&parents, &mut nodes, 3, Some(4), target);
        assert!(at(&nodes, 3).distance(target) < 1e-3, "{}", at(&nodes, 3));
        assert!((at(&nodes, 2).distance(at(&nodes, 1)) - 0.5).abs() < 1e-4);
        assert!((at(&nodes, 3).distance(at(&nodes, 2)) - 0.5).abs() < 1e-4);
        assert_eq!(nodes[0], rest[0], "the chest stays");
        assert_eq!(nodes[4], rest[4], "the other arm stays");
        // Out of reach: held straight toward it.
        let far = Vec3::new(0.5, 1.5, -3.0);
        let mut nodes = rest;
        reach(&parents, &mut nodes, 3, Some(4), far);
        assert!(at(&nodes, 3).abs_diff_eq(Vec3::new(0.5, 1.5, -1.0), 1e-3));
    }

    #[test]
    fn a_hanging_arm_is_raised_in_front_and_its_socket_turned_to_the_body() {
        let (parents, rest) = rig();
        let mut aim = rest;
        let fit = fit(&parents, &mut aim, 3, Some(4), true, true).unwrap();
        let raise = fit.hold.expect("a hold");
        assert_eq!(raise.shoulder, 1);
        // In front of the chest, below the shoulder and in from it.
        let hand = at(&aim, 3);
        assert!(hand.z < -0.6 && hand.y < 1.5 && hand.x < 0.5, "{hand}");
        assert!(hand.distance(raise.target(at(&aim, 1), 0.0)) < 1e-3);
        // The socket's frame, after its correction, is the body's.
        let socket = aim[3] * Mat4::from_quat(fit.correction);
        let (_, rotation, _) = socket.to_scale_rotation_translation();
        assert!(rotation.abs_diff_eq(AIM_HAND, 1e-4), "{rotation}");
        // Aiming up raises the hand about the shoulder.
        let up = raise.target(at(&aim, 1), 0.5);
        assert!(up.y > hand.y + 0.2);
    }

    #[test]
    fn the_hip_is_where_both_legs_hang_from() {
        // Pelvis, two thighs and feet, a spine and a hand above.
        let parents = [None, Some(0), Some(1), Some(0), Some(3), Some(0), Some(5)];
        let nodes = [
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(-0.2, 0.9, 0.0),
            Vec3::new(-0.2, 0.0, 0.0),
            Vec3::new(0.2, 0.9, 0.0),
            Vec3::new(0.2, 0.0, 0.0),
            Vec3::new(0.0, 1.5, 0.0),
            Vec3::new(0.5, 1.2, 0.0),
        ]
        .map(Mat4::from_translation);
        assert_eq!(hip(&parents, &nodes), Some(0));
    }

    #[test]
    fn an_arm_with_its_own_aim_clip_keeps_its_pose_and_only_the_socket_turns() {
        let (parents, rest) = rig();
        let mut aim = rest;
        let fit = fit(&parents, &mut aim, 3, Some(4), false, true).unwrap();
        assert_eq!(aim, rest);
        // It holds where its own clip puts the hand.
        let hold = fit.hold.expect("a hold");
        assert!(hold.offset.abs_diff_eq(Vec3::new(0.0, -1.0, 0.0), 1e-4));
        let socket = aim[3] * Mat4::from_quat(fit.correction);
        let (_, rotation, _) = socket.to_scale_rotation_translation();
        assert!(rotation.abs_diff_eq(AIM_HAND, 1e-4));
    }
}
