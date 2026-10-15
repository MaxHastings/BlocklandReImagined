//! Where each body holds its images, so the host fires every shot from
//! the muzzle of the image in the shooter's hand (not the eye): the hand
//! sockets of the shooter's archetype, the rig's hands for sockets on
//! nodes (the Blockhead's `Mount0` raised by `armReadyRight`), and each
//! image's barrel.
use super::*;
use bri_content::animation::{Layer, sample_layers};
use bri_content::body::Role;
use bri_weapons::ActorId;
use bri_weapons::muzzle::Grip;
use glam::{Mat4, Quat};

/// What the host knows of bodies' hands and images' barrels.
#[derive(Debug, Clone, Default)]
pub(super) struct Grips {
    /// By body model, each named node's transform (lower-case name) in the
    /// aim pose, level: the rig's idle, its arm raised for that hand, its
    /// look level. Two-handed variants use a `#both` suffix.
    hands: BTreeMap<(String, bool), BTreeMap<String, Mat4>>,
    /// Each image's muzzle in its mount's frame ([`bri_weapons::muzzle::barrel`]).
    barrels: BTreeMap<String, Vec3>,
    /// Archetypes already reported as having no hand the host can find.
    reported: std::collections::BTreeSet<(String, u8)>,
}

/// The hand nodes of a rig in its aim pose: each image mount point's
/// node (`Mount0`, `Mount1`, and any other hand node a socket names in
/// `nodes`) with the rig's idle clip, the arm raised for that hand and the
/// look clip level. A rig without those clips holds them in its bind pose.
pub fn shape_grips(rig: &bri_content::avatar::Rig, nodes: &[&str]) -> BTreeMap<String, [f32; 16]> {
    let clip = |role: Role| rig.sequence(role.blockhead_clip());
    let mut out = BTreeMap::new();
    for (hand, both) in [(0, false), (1, false), (0, true), (1, true)] {
        let Some(pose) = aim_pose(rig, &clip, hand, both) else {
            continue;
        };
        let names =
            std::iter::once(bri_content::body::BLOCKHEAD_HANDS[hand]).chain(nodes.iter().copied());
        for name in names {
            let key = pose_key(name, both);
            // A node is its own hand's: Mount1 from the left arm's pose.
            if out.contains_key(&key)
                && key != pose_key(bri_content::body::BLOCKHEAD_HANDS[hand], both)
            {
                continue;
            }
            if let Some(m) = node_in(rig, &pose, name) {
                out.insert(key, m);
            }
        }
    }
    out
}

/// The hand nodes an imported body's archetype holds its images by (its
/// `sockets`), each in its own hand's aim pose, with the clips the
/// archetype names for its roles (`Look::animations`), else the
/// Blockhead's clip names, else the bind pose. A `fitted` body (an
/// imported glTF `body`) is held as every client draws it
/// ([`bri_content::hold::fit`]): an arm it has no aim clip for raised in
/// front of the chest, and the hand turned to hold as the Blockhead's.
pub fn body_grips(
    rig: &bri_content::avatar::Rig,
    sockets: &bri_content::body::Sockets,
    animations: &bri_content::body::Animations,
    fitted: bool,
) -> BTreeMap<String, [f32; 16]> {
    let clip = |role: Role| {
        animations
            .get(role.name())
            .and_then(|clip| rig.sequence(clip))
            .or_else(|| rig.sequence(role.blockhead_clip()))
    };
    let index = |name: &str| {
        rig.shape
            .nodes
            .iter()
            .position(|n| n.name.eq_ignore_ascii_case(name))
    };
    let parents: Vec<Option<usize>> = rig.shape.nodes.iter().map(|n| n.parent).collect();
    let mut out = BTreeMap::new();
    for (hand, both) in [(0, false), (1, false), (0, true), (1, true)] {
        let Some(name) = sockets.hand(hand as u32).and_then(|s| s.node) else {
            continue;
        };
        let Some(mut pose) = aim_pose(rig, &clip, hand, both) else {
            continue;
        };
        if fitted
            && !sockets.preserve_hand_pose
            && let Some(end) = index(&name)
        {
            let landmark = sockets
                .hand(1 - hand as u32)
                .and_then(|s| s.node)
                .and_then(|n| index(&n))
                .or_else(|| sockets.eye().node.and_then(|n| index(&n)));
            let aims = clip(if both {
                Role::AimBoth
            } else {
                [Role::AimRight, Role::AimLeft][hand]
            })
            .is_some();
            if let Some(fit) =
                bri_content::hold::fit(&parents, &mut pose.nodes, end, landmark, !aims, hand == 0)
            {
                pose.nodes[end] *= Mat4::from_quat(fit.correction);
                // The hand pitches with the aim about its shoulder.
                if let Some(hold) = fit.hold
                    && let Some(shoulder) = pose.nodes.get(hold.shoulder)
                {
                    out.insert(
                        format!("{}{SHOULDER}", pose_key(&name, both)),
                        Mat4::from_translation(shoulder.w_axis.truncate()).to_cols_array(),
                    );
                }
            }
        }
        if let Some(m) = node_in(rig, &pose, &name) {
            out.insert(pose_key(&name, both), m);
        }
    }
    out
}

/// Beside a fitted hand's node in [`body_grips`]: the shoulder it pitches
/// about with the aim.
const SHOULDER: &str = "#shoulder";

fn pose_key(node: &str, both: bool) -> String {
    let key = node.to_ascii_lowercase();
    if both { format!("{key}#both") } else { key }
}

/// The rig's idle, its one- or two-handed ready role and its look level.
fn aim_pose<'a>(
    rig: &'a bri_content::avatar::Rig,
    clip: &dyn Fn(Role) -> Option<&'a bri_content::shape::Animation>,
    hand: usize,
    both: bool,
) -> Option<bri_content::animation::Pose> {
    let aim = if both {
        Role::AimBoth
    } else {
        [Role::AimRight, Role::AimLeft][hand]
    };
    let mut layers = Vec::new();
    for (animation, at) in [
        (clip(Role::Idle), 0.0),
        (clip(aim), 1.0),
        (clip(Role::Look), 0.5),
    ] {
        if let Some(animation) = animation {
            layers.push(Layer {
                animation,
                time: animation.duration * at,
                weight: 1.0,
            });
        }
    }
    layers.sort_by_key(|l| (l.animation.additive, l.animation.priority));
    sample_layers(&rig.shape, &layers).ok()
}

fn node_in(
    rig: &bri_content::avatar::Rig,
    pose: &bri_content::animation::Pose,
    name: &str,
) -> Option<[f32; 16]> {
    let i = rig
        .shape
        .nodes
        .iter()
        .position(|n| n.name.eq_ignore_ascii_case(name))?;
    pose.nodes
        .get(i)
        .filter(|m| m.is_finite())
        .map(|m| m.to_cols_array())
}

impl Session {
    /// The hand nodes of the body drawn with `model` in its aim pose
    /// ([`shape_grips`]), so shots leave its hands' muzzles.
    pub fn set_body_grips(
        &mut self,
        model: &str,
        nodes: BTreeMap<String, [f32; 16]>,
    ) -> Result<()> {
        self.set_body_grips_policy(model, false, nodes)
    }
    /// Separate authored/fitted hand frames even when archetypes share a model.
    pub fn set_body_grips_policy(
        &mut self,
        model: &str,
        preserve_hand_pose: bool,
        nodes: BTreeMap<String, [f32; 16]>,
    ) -> Result<()> {
        let nodes = nodes
            .into_iter()
            .map(|(name, m)| (name.to_ascii_lowercase(), Mat4::from_cols_array(&m)))
            .collect::<BTreeMap<_, _>>();
        ensure!(
            nodes.len() <= 128
                && nodes
                    .values()
                    .all(|m| m.is_finite() && m.w_axis.truncate().length() <= 64.0),
            "Invalid body hand nodes for {model}"
        );
        self.grips
            .hands
            .insert((model.to_ascii_lowercase(), preserve_hand_pose), nodes);
        Ok(())
    }
    /// Each image's muzzle in the frame of the hand holding it
    /// ([`bri_weapons::muzzle::barrel`]), by image id.
    pub fn set_image_barrels(&mut self, barrels: BTreeMap<String, [f32; 3]>) -> Result<()> {
        ensure!(
            barrels
                .values()
                .all(|b| b.iter().all(|v| v.is_finite() && v.abs() <= 16.0)),
            "Invalid image barrels"
        );
        self.grips.barrels = barrels
            .into_iter()
            .map(|(id, b)| (id, Vec3::from(b)))
            .collect();
        Ok(())
    }
    /// How a player of `archetype` holds image mount point `hand`: an
    /// offset socket in its own frame, pitching about itself; a socket on a
    /// node, at that node's aim pose, pitching about the eye. `None` when
    /// the host knows no such hand.
    pub(super) fn grip(
        &self,
        archetype: &crate::archetype::Archetype,
        hand: u8,
        both: bool,
    ) -> Option<Grip> {
        let socket = archetype.sockets.hand(u32::from(hand))?;
        let offset = Mat4::from_translation(Vec3::from(socket.offset));
        let Some(node) = &socket.node else {
            return Some(Grip {
                hand: offset,
                pivot: Vec3::from(socket.offset),
            });
        };
        let hands = self.grips.hands.get(&(
            archetype.look.model.to_ascii_lowercase(),
            archetype.sockets.preserve_hand_pose,
        ))?;
        let node = pose_key(node, both);
        let hand_node = hands.get(&node).or_else(|| {
            both.then(|| hands.get(&node[..node.len() - "#both".len()]))
                .flatten()
        })?;
        let m = &archetype.movement;
        let pivot = hands
            .get(&format!("{node}{SHOULDER}"))
            .map_or(Vec3::new(0.0, m.stand_eye, -m.eye_forward), |s| {
                s.w_axis.truncate()
            });
        Some(Grip {
            hand: *hand_node * offset,
            pivot,
        })
    }
    /// Where `owner`'s shot from `hand` leaves: the muzzle of the image in
    /// that hand, as their body holds it, aimed where they look; the eye
    /// for a body whose hand the host does not know (said once).
    pub(super) fn muzzle(&mut self, owner: OwnerId, hand: u8, eye: Vec3) -> Vec3 {
        let Some(peer) = self.peers.get(&owner) else {
            return eye;
        };
        let state = peer.player.state();
        let archetype = self.archetypes.resolve(state.archetype);
        let both = self
            .weapons
            .image_id(ActorId(owner), hand)
            .and_then(|id| self.weapons.pack.images.get(id))
            .is_some_and(|image| image.both_arms);
        let Some(grip) = self.grip(archetype, hand, both) else {
            let key = (archetype.id.clone(), hand);
            if hand < 2 && !self.grips.reported.contains(&key) {
                bri_console::warn(format!(
                    "{}: no hand {hand} the host can find (model {}), so its shots leave the eye",
                    archetype.id, archetype.look.model
                ));
                self.grips.reported.insert(key);
            }
            return eye;
        };
        let barrel = self
            .weapons
            .image_id(ActorId(owner), hand)
            .and_then(|image| self.grips.barrels.get(image))
            .copied()
            .unwrap_or(Vec3::ZERO);
        let feet = Vec3::from(state.feet);
        let scale = if state.scale.is_finite() && state.scale > 0.0 {
            state.scale
        } else {
            1.0
        };
        // The eye moved from standing (crouched, seated) moves the hands.
        let m = &archetype.movement;
        let standing = Vec3::new(0.0, m.stand_eye, -m.eye_forward);
        let eye_local = Quat::from_rotation_y(state.yaw) * (eye - feet) / scale;
        let muzzle = grip.muzzle(
            barrel,
            feet,
            state.yaw,
            state.pitch,
            scale,
            eye_local - standing,
        );
        if muzzle.is_finite() { muzzle } else { eye }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authored_two_handed_grips_use_the_two_handed_role() {
        let mut rig = bri_content::testing::avatar::rig();
        // A real imported rig may give its one- and two-handed holds different
        // right-arm poses; do not assume they share the same muzzle origin.
        let both = rig.sequences.get_mut("armreadyboth").unwrap();
        let arm = both
            .nodes
            .iter_mut()
            .find(|t| t.node == "RightArm")
            .unwrap();
        *arm.rotations.last_mut().unwrap() = Quat::from_rotation_x(0.7).to_array();
        let sockets = bri_content::body::Sockets {
            preserve_hand_pose: true,
            ..Default::default()
        };
        let clips = Default::default();
        let grips = body_grips(&rig, &sockets, &clips, true);
        let single = Mat4::from_cols_array(&grips["mount0"]);
        let both = Mat4::from_cols_array(&grips["mount0#both"]);
        assert!(!single.abs_diff_eq(both, 1e-3));
        let lookup = |role: Role| rig.sequence(role.blockhead_clip());
        let expected = aim_pose(&rig, &lookup, 0, true).unwrap();
        assert!(both.abs_diff_eq(
            Mat4::from_cols_array(&node_in(&rig, &expected, "Mount0").unwrap()),
            1e-5
        ));
        assert!(shape_grips(&rig, &[]).contains_key("mount0#both"));
    }
}
