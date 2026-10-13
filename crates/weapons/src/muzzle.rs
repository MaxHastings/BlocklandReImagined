//! Where a shot leaves a held image: its barrel's mouth (`muzzlePoint`)
//! in the frame of the hand that holds it, so the host fires from the
//! muzzle each body holds the image in, as every client draws it.
use bri_content::shape::Shape;
use glam::{Mat4, Quat, Vec3};

/// A node's rest transform in the shape's frame (its bind pose).
fn rest(shape: &Shape, mut node: usize) -> Mat4 {
    let mut transform = Mat4::IDENTITY;
    for _ in 0..shape.nodes.len() {
        let n = &shape.nodes[node];
        transform = Mat4::from_rotation_translation(
            Quat::from_array(n.rotation).normalize(),
            Vec3::from(n.translation),
        ) * transform;
        match n.parent {
            Some(p) if p < shape.nodes.len() => node = p,
            _ => break,
        }
    }
    transform
}
fn node(shape: &Shape, name: &str) -> Option<usize> {
    shape
        .nodes
        .iter()
        .position(|n| n.name.eq_ignore_ascii_case(name))
}

/// The muzzle of an image held at its mount, in that mount's frame: the
/// image's `offset` and rotation, its `mountPoint` node undone, then its
/// `muzzlePoint` node in the bind pose (Torque's `getMuzzlePoint` on the
/// third-person mount). An image with no model or no `muzzlePoint` fires
/// from where it is held.
pub fn barrel(offset: [f32; 3], rotation_degrees: [f32; 3], shape: Option<&Shape>) -> Vec3 {
    let in_hand = Mat4::from_rotation_translation(
        crate::rotation::native(rotation_degrees),
        Vec3::from(offset),
    );
    let Some(shape) = shape else {
        return in_hand.w_axis.truncate();
    };
    let correction = node(shape, "mountPoint").map_or(Mat4::IDENTITY, |i| rest(shape, i).inverse());
    let muzzle = node(shape, "muzzlePoint").map_or(Mat4::IDENTITY, |i| rest(shape, i));
    let at = (in_hand * correction * muzzle).w_axis.truncate();
    if at.is_finite() && at.length() <= 16.0 {
        at
    } else {
        in_hand.w_axis.truncate()
    }
}

/// Where a body holds an image, for the host's shots: the hand's
/// transform in the body's frame (feet at the origin, facing -Z, unscaled,
/// level aim), and the point it turns about as the aim pitches.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grip {
    pub hand: Mat4,
    pub pivot: Vec3,
}
impl Grip {
    /// The muzzle of an image whose `barrel` is held in this grip, for a
    /// body at `feet` facing `yaw`, aiming `pitch` up, at `scale`, its eye
    /// moved `eye_shift` from standing (crouched or seated), body frame.
    pub fn muzzle(
        &self,
        barrel: Vec3,
        feet: Vec3,
        yaw: f32,
        pitch: f32,
        scale: f32,
        eye_shift: Vec3,
    ) -> Vec3 {
        let local = self.hand.transform_point3(barrel);
        let aimed = self.pivot + Quat::from_rotation_x(pitch) * (local - self.pivot) + eye_shift;
        feet + Quat::from_rotation_y(-yaw) * aimed * scale
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_grip_pitches_the_barrel_about_its_pivot_and_turns_with_the_body() {
        let grip = Grip {
            hand: Mat4::from_translation(Vec3::new(0.5, 1.0, -0.2)),
            pivot: Vec3::new(0.5, 1.0, -0.2),
        };
        let barrel = Vec3::new(0.0, 0.0, -1.0);
        let level = grip.muzzle(barrel, Vec3::ZERO, 0.0, 0.0, 1.0, Vec3::ZERO);
        assert!(
            level.abs_diff_eq(Vec3::new(0.5, 1.0, -1.2), 1e-5),
            "{level}"
        );
        let up = grip.muzzle(
            barrel,
            Vec3::ZERO,
            0.0,
            std::f32::consts::FRAC_PI_2,
            1.0,
            Vec3::ZERO,
        );
        assert!(up.abs_diff_eq(Vec3::new(0.5, 2.0, -0.2), 1e-5), "{up}");
        // Facing +X (yaw 90 degrees), the right hand is toward +Z.
        let turned = grip.muzzle(
            barrel,
            Vec3::ZERO,
            std::f32::consts::FRAC_PI_2,
            0.0,
            2.0,
            Vec3::ZERO,
        );
        assert!(
            turned.abs_diff_eq(Vec3::new(2.4, 2.0, 1.0), 1e-4),
            "{turned}"
        );
    }

    #[test]
    fn a_model_less_image_fires_from_its_mount_offset() {
        assert_eq!(
            barrel([0.1, 0.0, 0.0], [0.0; 3], None),
            Vec3::new(0.1, 0.0, 0.0)
        );
    }
}
