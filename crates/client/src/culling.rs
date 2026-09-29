//! What the camera can see, for skipping work on entities it cannot: an
//! avatar's mesh, a vehicle's instances. Conservative: a sphere or box that
//! may touch the view counts as seen.
use glam::{Mat4, Vec3, Vec4};

/// The view volume as six planes (inside: `n . p + d >= 0`).
#[derive(Clone, Copy, Debug)]
pub struct Frustum([Vec4; 6]);

impl Frustum {
    pub fn new(view_projection: Mat4) -> Self {
        let (r0, r1, r2, r3) = (
            view_projection.row(0),
            view_projection.row(1),
            view_projection.row(2),
            view_projection.row(3),
        );
        // wgpu's 0..1 depth: the near plane is row 2 alone.
        Self([r3 + r0, r3 - r0, r3 + r1, r3 - r1, r2, r3 - r2].map(|p| {
            let length = p.truncate().length();
            if length > 0.0 { p / length } else { p }
        }))
    }
    /// Whether any of the sphere may be in view. A camera with non-finite
    /// planes sees everything.
    pub fn sees_sphere(&self, center: Vec3, radius: f32) -> bool {
        self.0
            .iter()
            .all(|p| !p.is_finite() || p.truncate().dot(center) + p.w >= -radius)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spheres_ahead_show_and_behind_or_aside_do_not() {
        let view_projection =
            glam::camera::rh::proj::directx::perspective(90f32.to_radians(), 1.0, 0.1, 100.0)
                * glam::camera::rh::view::look_at_mat4(Vec3::ZERO, Vec3::NEG_Z, Vec3::Y);
        let f = Frustum::new(view_projection);
        assert!(f.sees_sphere(Vec3::new(0.0, 0.0, -10.0), 1.0));
        assert!(!f.sees_sphere(Vec3::new(0.0, 0.0, 10.0), 1.0));
        assert!(!f.sees_sphere(Vec3::new(30.0, 0.0, -10.0), 1.0));
        // Straddling the edge of the view still counts.
        assert!(f.sees_sphere(Vec3::new(11.0, 0.0, -10.0), 2.0));
        assert!(!f.sees_sphere(Vec3::new(0.0, 0.0, -200.0), 1.0));
    }
}
