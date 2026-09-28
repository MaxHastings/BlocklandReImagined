//! Image `rotation`/`eyeRotation` written as `eulerToMatrix("x y z")`.
//!
//! The pack stores image rotations as Euler degrees for Torque's
//! `MatrixF(EulerF)` (`m_matF_set_euler`), which the client composes as
//! Ry(-y)·Rx(-x)·Rz(-z). That is right for a literal axis-angle such as
//! HateImage's `"1 0 0 -90"`, which the importer lowers to one axis. But
//! v20's `eulerToMatrix` goes through `MatrixCreateFromEuler`, which builds
//! `QuatF(EulerF)` and returns its axis-angle; `TypeMatrixRotation` turns
//! that back into a matrix through the same quaternion. The result is the
//! transpose, Rz(z)·Rx(x)·Ry(y) (TGE lineage: OpenMBG `mathTypes.cc`
//! `MatrixCreateFromEuler`, `mQuat.cc` `QuatF::set(EulerF)`). Skis were held
//! sideways and every other two-axis or signed one-axis `eulerToMatrix`
//! image was mirrored.
use crate::{Definition, Pack};
use glam::{Mat3, Quat};
use std::collections::BTreeMap;

/// Degrees in the pack's Euler convention for `eulerToMatrix(degrees)`.
pub fn euler_to_matrix(degrees: [f32; 3]) -> [f32; 3] {
    let [x, y, z] = degrees.map(f32::to_radians);
    // Source space (z up): Rz(z)·Rx(x)·Ry(y).
    let r = Mat3::from_quat(
        Quat::from_rotation_z(z) * Quat::from_rotation_x(x) * Quat::from_rotation_y(y),
    );
    let m = |row: usize, col: usize| r.col(col)[row];
    // Write it as Ry(a)·Rx(b)·Rz(c); the pack's order is Ry(-y')·Rx(-x')·Rz(-z').
    let b = (-m(1, 2)).atan2(m(1, 0).hypot(m(1, 1)));
    // Pitched straight up or down, yaw and roll share an axis: roll 0.
    let (a, c) = if m(1, 2).abs() < 0.9999 {
        (m(0, 2).atan2(m(2, 2)), m(1, 0).atan2(m(1, 1)))
    } else {
        ((-m(2, 0)).atan2(m(0, 0)), 0.0)
    };
    [-b, -a, -c].map(|v| {
        let d = v.to_degrees();
        // Keep exact zeros tidy for readers of the pack.
        if d.abs() < 1e-4 { 0.0 } else { d }
    })
}

/// Whether the image's literal field (with inheritance) is an
/// `eulerToMatrix(...)` call.
pub fn is_euler_to_matrix(pack: &Pack, image: &str, field: &str) -> bool {
    let by_name: BTreeMap<String, &Definition> = pack
        .definitions
        .iter()
        .map(|d| (d.name.to_ascii_lowercase(), d))
        .collect();
    let mut at = by_name.get(&image.to_ascii_lowercase()).copied();
    for _ in 0..16 {
        let Some(d) = at else { return false };
        if let Some(v) = d.fields.get(&field.to_ascii_lowercase()) {
            return v.to_ascii_lowercase().contains("eulertomatrix");
        }
        at = d
            .parent
            .as_ref()
            .and_then(|p| by_name.get(&p.to_ascii_lowercase()).copied());
    }
    false
}

/// Correct every image whose `rotation` is an `eulerToMatrix` call.
pub fn correct_image_rotations(pack: &mut Pack) {
    let fixes: Vec<(String, [f32; 3])> = pack
        .images
        .iter()
        .filter(|(_, i)| is_euler_to_matrix(pack, &i.name, "rotation"))
        .map(|(id, i)| (id.clone(), euler_to_matrix(i.source_rotation_degrees)))
        .collect();
    for (id, degrees) in fixes {
        if let Some(image) = pack.images.get_mut(&id) {
            image.source_rotation_degrees = degrees;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// The client's composition of pack degrees (`items::source_euler`,
    /// before its basis change).
    fn pack_rotation(d: [f32; 3]) -> Quat {
        let [x, y, z] = d.map(f32::to_radians);
        Quat::from_rotation_y(-y) * Quat::from_rotation_x(-x) * Quat::from_rotation_z(-z)
    }
    fn same(a: Quat, b: Quat) -> bool {
        a.dot(b).abs() > 0.9999
    }
    #[test]
    fn converted_degrees_compose_to_the_quaternion_path_matrix() {
        for d in [
            [-90.0, 90.0, 0.0],
            [90.0, -90.0, 0.0],
            [0.0, 35.0, 90.0],
            [90.0, 180.0, 0.0],
            [0.0, 0.0, 10.0],
            [90.0, 0.0, 0.0],
            [0.0, -90.0, 0.0],
            [12.0, -40.0, 75.0],
        ] {
            let [x, y, z] = d.map(f32::to_radians);
            let want =
                Quat::from_rotation_z(z) * Quat::from_rotation_x(x) * Quat::from_rotation_y(y);
            assert!(same(pack_rotation(euler_to_matrix(d)), want), "{d:?}");
        }
    }
    #[test]
    fn one_axis_turns_are_mirrored_and_half_turns_kept() {
        assert_eq!(euler_to_matrix([0.0, 0.0, 10.0]), [0.0, 0.0, -10.0]);
        assert_eq!(euler_to_matrix([90.0, 0.0, 0.0]), [-90.0, 0.0, 0.0]);
        assert!(same(
            pack_rotation(euler_to_matrix([0.0, 180.0, 0.0])),
            pack_rotation([0.0, 180.0, 0.0])
        ));
    }
    #[test]
    fn skis_point_down_in_the_hand_not_sideways() {
        // ski.dts lies along +Y (source). v20 holds it pointing down.
        let q = pack_rotation(euler_to_matrix([-90.0, 90.0, 0.0]));
        let along = q * glam::Vec3::Y;
        assert!((along - glam::Vec3::NEG_Z).length() < 1e-4, "{along}");
    }
}
