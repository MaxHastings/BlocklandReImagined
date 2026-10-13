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
    /// look level.
    hands: BTreeMap<String, BTreeMap<String, Mat4>>,
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
    for hand in [0, 1] {
        let Some(pose) = aim_pose(rig, &clip, hand) else {
            continue;
        };
        let names =
            std::iter::once(bri_content::body::BLOCKHEAD_HANDS[hand]).chain(nodes.iter().copied());
        for name in names {
            let key = name.to_ascii_lowercase();
            // A node is its own hand's: Mount1 from the left arm's pose.
            if out.contains_key(&key)
                && !key.eq_ignore_ascii_case(bri_content::body::BLOCKHEAD_HANDS[hand])
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
    for hand in [0, 1] {
        let Some(name) = sockets.hand(hand as u32).and_then(|s| s.node) else {
            continue;
        };
        let Some(mut pose) = aim_pose(rig, &clip, hand) else {
            continue;
        };
        if fitted && let Some(end) = index(&name) {
            let landmark = sockets
                .hand(1 - hand as u32)
                .and_then(|s| s.node)
                .and_then(|n| index(&n))
                .or_else(|| sockets.eye().node.and_then(|n| index(&n)));
            let aims = clip([Role::AimRight, Role::AimLeft][hand]).is_some();
            if let Some(fit) =
                bri_content::hold::fit(&parents, &mut pose.nodes, end, landmark, !aims, hand == 0)
            {
                pose.nodes[end] *= Mat4::from_quat(fit.correction);
                // The hand pitches with the aim about its shoulder.
                if let Some(hold) = fit.hold
                    && let Some(shoulder) = pose.nodes.get(hold.shoulder)
                {
                    out.insert(
                        format!("{}{SHOULDER}", name.to_ascii_lowercase()),
                        Mat4::from_translation(shoulder.w_axis.truncate()).to_cols_array(),
                    );
                }
            }
        }
        if let Some(m) = node_in(rig, &pose, &name) {
            out.insert(name.to_ascii_lowercase(), m);
        }
    }
    out
}

/// Beside a fitted hand's node in [`body_grips`]: the shoulder it pitches
/// about with the aim.
const SHOULDER: &str = "#shoulder";

/// The rig's idle, its arm raised for `hand` and its look level.
fn aim_pose<'a>(
    rig: &'a bri_content::avatar::Rig,
    clip: &dyn Fn(Role) -> Option<&'a bri_content::shape::Animation>,
    hand: usize,
) -> Option<bri_content::animation::Pose> {
    let aim = [Role::AimRight, Role::AimLeft][hand];
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
        let nodes = nodes
            .into_iter()
            .map(|(name, m)| (name.to_ascii_lowercase(), Mat4::from_cols_array(&m)))
            .collect::<BTreeMap<_, _>>();
        ensure!(
            nodes.len() <= 64
                && nodes
                    .values()
                    .all(|m| m.is_finite() && m.w_axis.truncate().length() <= 64.0),
            "Invalid body hand nodes for {model}"
        );
        self.grips.hands.insert(model.to_ascii_lowercase(), nodes);
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
    pub(super) fn grip(&self, archetype: &crate::archetype::Archetype, hand: u8) -> Option<Grip> {
        let socket = archetype.sockets.hand(u32::from(hand))?;
        let offset = Mat4::from_translation(Vec3::from(socket.offset));
        let Some(node) = &socket.node else {
            return Some(Grip {
                hand: offset,
                pivot: Vec3::from(socket.offset),
            });
        };
        let hands = self
            .grips
            .hands
            .get(&archetype.look.model.to_ascii_lowercase())?;
        let node = node.to_ascii_lowercase();
        let hand_node = hands.get(&node)?;
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
        let Some(grip) = self.grip(archetype, hand) else {
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
