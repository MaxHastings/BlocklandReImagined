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
        // 0..1 depth: row 2 bounds one end and row 3 - row 2 the other,
        // whichever way round depth runs.
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

/// Every place a body can show this frame besides the player's own view:
/// the other views' frusta (mirror planes, probe faces) and where its
/// shadow can fall ([`bri_render::scene::ShadowReach`]).
#[derive(Clone, Debug, Default)]
pub struct Elsewhere {
    frusta: Vec<Frustum>,
    spheres: Vec<(Vec3, f32)>,
}

impl Elsewhere {
    pub fn new(
        views: impl IntoIterator<Item = Mat4>,
        shadows: bri_render::scene::ShadowReach,
    ) -> Self {
        Self {
            frusta: views
                .into_iter()
                .chain(shadows.boxes)
                .map(Frustum::new)
                .collect(),
            spheres: shadows.spheres,
        }
    }
    /// Whether any of the sphere may show in another view or a shadow.
    pub fn sees_sphere(&self, center: Vec3, radius: f32) -> bool {
        self.frusta.iter().any(|f| f.sees_sphere(center, radius))
            || self
                .spheres
                .iter()
                .any(|(c, r)| c.distance(center) <= r + radius)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spheres_ahead_show_and_behind_or_aside_do_not() {
        let view_projection = bri_render::scene::perspective(90f32.to_radians(), 1.0, 0.1, 100.0)
            * glam::camera::rh::view::look_at_mat4(Vec3::ZERO, Vec3::NEG_Z, Vec3::Y);
        let f = Frustum::new(view_projection);
        assert!(f.sees_sphere(Vec3::new(0.0, 0.0, -10.0), 1.0));
        assert!(!f.sees_sphere(Vec3::new(0.0, 0.0, 10.0), 1.0));
        assert!(!f.sees_sphere(Vec3::new(30.0, 0.0, -10.0), 1.0));
        // Straddling the edge of the view still counts.
        assert!(f.sees_sphere(Vec3::new(11.0, 0.0, -10.0), 2.0));
        assert!(!f.sees_sphere(Vec3::new(0.0, 0.0, -200.0), 1.0));
    }

    /// A body far behind the camera is seen nowhere: neither in view nor
    /// in a shadow cascade, so its mesh is not built. One beside the camera
    /// but in a cascade's reach is, and one inside a lamp's reach too.
    #[test]
    fn bodies_out_of_view_and_shadow_reach_are_not_built() {
        let eye = Vec3::new(0.0, 2.0, 0.0);
        let view = bri_render::scene::perspective(70f32.to_radians(), 1.6, 0.05, 1000.0)
            * glam::camera::rh::view::look_at_mat4(eye, eye + Vec3::NEG_Z, Vec3::Y);
        // A sun cascade: 80 units across around the eye, deep toward the sun.
        let cascade = glam::camera::rh::proj::directx::orthographic(-40., 40., -40., 40., 0., 400.)
            * glam::camera::rh::view::look_at_mat4(eye + Vec3::Y * 200.0, eye, Vec3::Z);
        let elsewhere = Elsewhere::new(
            [],
            bri_render::scene::ShadowReach {
                boxes: vec![cascade],
                spheres: vec![(Vec3::new(500.0, 2.0, 500.0), 20.0)],
            },
        );
        let in_view = Frustum::new(view);
        let seen = |c: Vec3| in_view.sees_sphere(c, 3.0) || elsewhere.sees_sphere(c, 3.0);
        assert!(!seen(Vec3::new(0.0, 0.0, 1000.0)), "far behind");
        assert!(seen(Vec3::new(0.0, 0.0, -50.0)), "ahead");
        assert!(seen(Vec3::new(0.0, 0.0, 20.0)), "behind, casting into view");
        assert!(seen(Vec3::new(510.0, 0.0, 505.0)), "in a lamp's reach");
    }
}
