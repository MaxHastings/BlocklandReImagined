// A Blockhead-shaped skeleton for Ragdoll tests, shared with the client
// crate's (`crates/client/src/addon_physics.rs`, through `include!`).
use bri_client_sandbox::world::{Rig, Skeleton};
use std::sync::Arc;

/// A Blockhead-shaped test rig standing at `feet`: a root at the feet, a
/// hip, torso, head, arms with hands and legs, each part on its own node.
pub fn blockhead(feet: [f32; 3]) -> Skeleton {
    let nodes = [
        ("Root", -1, [0.0, 0.0, 0.0], None),
        (
            "Hip",
            0,
            [0.0, 1.0, 0.0],
            Some(([-0.5, -0.3, -0.25], [0.5, 0.2, 0.25])),
        ),
        (
            "Torso",
            1,
            [0.0, 0.3, 0.0],
            Some(([-0.5, 0.0, -0.25], [0.5, 1.0, 0.25])),
        ),
        (
            "Head",
            2,
            [0.0, 1.0, 0.0],
            Some(([-0.4, 0.0, -0.4], [0.4, 0.8, 0.4])),
        ),
        (
            "RightArm",
            2,
            [0.6, 0.9, 0.0],
            Some(([-0.1, -0.8, -0.2], [0.2, 0.1, 0.2])),
        ),
        (
            "LeftArm",
            2,
            [-0.6, 0.9, 0.0],
            Some(([-0.2, -0.8, -0.2], [0.1, 0.1, 0.2])),
        ),
        (
            "RightHand",
            4,
            [0.0, -0.8, 0.0],
            Some(([-0.1, -0.3, -0.1], [0.1, 0.0, 0.1])),
        ),
        (
            "LeftHand",
            5,
            [0.0, -0.8, 0.0],
            Some(([-0.1, -0.3, -0.1], [0.1, 0.0, 0.1])),
        ),
        (
            "RightLeg",
            1,
            [0.25, -0.3, 0.0],
            Some(([-0.2, -0.7, -0.25], [0.2, 0.0, 0.25])),
        ),
        (
            "LeftLeg",
            1,
            [-0.25, -0.3, 0.0],
            Some(([-0.2, -0.7, -0.25], [0.2, 0.0, 0.25])),
        ),
        ("Eye", 3, [0.0, 0.5, -0.3], None),
    ];
    let parts = [
        ("pants", 1),
        ("chest", 2),
        ("femchest", 2),
        ("headskin", 3),
        ("rarm", 4),
        ("larm", 5),
        ("rhand", 6),
        ("lhand", 7),
        ("rshoe", 8),
        ("lshoe", 9),
    ];
    let mut world = Vec::new();
    for (_, parent, local, _) in &nodes {
        let base = if *parent < 0 {
            glam::Vec3::from(feet)
        } else {
            world[*parent as usize]
        };
        world.push(base + glam::Vec3::from(*local));
    }
    Skeleton {
        rig: Arc::new(Rig {
            names: nodes.iter().map(|n| n.0.to_string()).collect(),
            parents: nodes.iter().map(|n| n.1).collect(),
            parts: parts.iter().map(|(n, i)| (n.to_string(), *i)).collect(),
        }),
        nodes: world
            .iter()
            .map(|p| glam::Mat4::from_translation(*p).to_cols_array())
            .collect(),
        bounds: Arc::new(nodes.iter().map(|n| n.3.map(|(a, b)| [a, b])).collect()),
    }
}
