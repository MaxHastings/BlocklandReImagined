//! Mass and radial release impulse over actual member boxes, accumulated during
//! bounded admission. Empty space in a section's bounds contributes nothing.
use super::*;
use glam::{DMat3, DVec3};

pub(super) struct Mass {
    mass: f64,
    first: DVec3,
    inertia: DMat3,
    momentum: DVec3,
    angular: DVec3,
    radius: f64,
}

impl Default for Mass {
    fn default() -> Self {
        Self {
            mass: 0.,
            first: DVec3::ZERO,
            inertia: DMat3::ZERO,
            momentum: DVec3::ZERO,
            angular: DVec3::ZERO,
            radius: 0.,
        }
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct Motion {
    pub center: Vec3,
    pub velocity: Vec3,
    pub spin: Vec3,
}

impl Motion {
    pub(super) fn pose(self, origin: Vec3, age: f32) -> (Vec3, Quat) {
        let turn = Quat::from_scaled_axis(self.spin * age);
        (
            origin + self.center + self.velocity * age
                - Vec3::Y * (0.5 * GRAVITY * age * age)
                - turn * self.center,
            turn,
        )
    }
}

fn outer(v: DVec3) -> DMat3 {
    DMat3::from_cols(v * v.x, v * v.y, v * v.z)
}

impl Mass {
    fn inertia_at_center(&self, center: DVec3) -> DMat3 {
        self.inertia - self.mass * (DMat3::IDENTITY * center.length_squared() - outer(center))
    }

    /// Transfer a fixed-world contact's observed normal reaction to the local
    /// clumps. This changes their rigid velocity field, never a render pivot.
    pub(super) fn impact(
        &self,
        mut motion: Motion,
        turn: Quat,
        point: Vec3,
        normal: Vec3,
    ) -> Motion {
        let Some(normal) = normal.try_normalize() else {
            return motion;
        };
        if self.mass <= 0. {
            return motion;
        }
        let r = (point - turn * motion.center).as_dvec3();
        let n = normal.as_dvec3();
        let axes = DMat3::from_cols(
            (turn * Vec3::X).as_dvec3(),
            (turn * Vec3::Y).as_dvec3(),
            (turn * Vec3::Z).as_dvec3(),
        );
        let inverse =
            axes * self.inertia_at_center(motion.center.as_dvec3()).inverse() * axes.transpose();
        if !inverse.is_finite() {
            return motion;
        }
        let velocity = motion.velocity.as_dvec3();
        let spin = motion.spin.as_dvec3();
        let closing = (velocity + spin.cross(r)).dot(n);
        if closing >= 0. {
            return motion;
        }
        let response = inverse * r.cross(n);
        let effective = 1. / self.mass + n.dot(response.cross(r));
        if effective <= 0. || !effective.is_finite() {
            return motion;
        }
        let impulse = -(1. + f64::from(RESTITUTION)) * closing / effective;
        motion.velocity = (velocity + n * (impulse / self.mass)).as_vec3();
        motion.spin = (spin + response * impulse).as_vec3();
        motion
    }
    pub(super) fn add(
        &mut self,
        p: &SectionPiece,
        impulse: Option<bri_sim::presentation::RadialImpulse>,
        sign: f64,
    ) {
        let half = p.half.as_dvec3();
        let r = p.offset.as_dvec3();
        // A common density cancels from both impulse and inertia. Use the
        // actual material volume, not the section's mostly empty AABB.
        let mass = 8. * half.x * half.y * half.z * sign;
        let axes = DMat3::from_cols(
            (p.turn * Vec3::X).as_dvec3(),
            (p.turn * Vec3::Y).as_dvec3(),
            (p.turn * Vec3::Z).as_dvec3(),
        );
        let h2 = half * half;
        let intrinsic =
            DMat3::from_diagonal(DVec3::new(h2.y + h2.z, h2.x + h2.z, h2.x + h2.y) * (mass / 3.));
        self.mass += mass;
        self.first += mass * r;
        self.inertia += axes * intrinsic * axes.transpose()
            + mass * (DMat3::IDENTITY * r.length_squared() - outer(r));
        self.radius = self.radius.max(r.length() + half.length());

        let Some(impulse) = impulse.filter(|i| i.force > 0. && i.radius > 0.) else {
            return;
        };
        let origin = Vec3::from(impulse.origin);
        let center = p.origin + p.offset;
        let local = p.turn.inverse() * (origin - center);
        let contact = center + p.turn * local.clamp(-p.half, p.half);
        let distance = contact.distance(origin);
        if distance >= impulse.radius {
            return;
        }
        // The closest real surface receives the impulse. A blast in a hole
        // cannot push a section just because it overlaps its enclosing box.
        let direction = (contact - origin)
            .try_normalize()
            .or_else(|| (center - origin).try_normalize())
            .unwrap_or(Vec3::ZERO);
        let speed =
            (impulse.force * FORCE_TO_SPEED * (1. - distance / impulse.radius)).min(MAX_SPEED);
        let momentum = (direction * speed).as_dvec3() * mass;
        self.momentum += momentum;
        self.angular += (contact - p.origin).as_dvec3().cross(momentum);
    }

    pub(super) fn motion(&self) -> Motion {
        if self.mass <= 0. {
            return Motion::default();
        }
        let center = self.first / self.mass;
        let inertia =
            self.inertia - self.mass * (DMat3::IDENTITY * center.length_squared() - outer(center));
        let velocity = self.momentum / self.mass;
        let inverse = inertia.inverse();
        let spin = if inverse.is_finite() {
            inverse * (self.angular - center.cross(self.momentum))
        } else {
            DVec3::ZERO
        };
        // Bound the speed of every material point with the same debris speed
        // envelope. Scaling the whole response preserves its force/torque ratio.
        let bound = velocity.length() + spin.length() * (self.radius + center.length());
        let scale = (f64::from(MAX_SPEED) / bound).min(1.);
        Motion {
            center: center.as_vec3(),
            velocity: (velocity * scale).as_vec3(),
            spin: (spin * scale).as_vec3(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brick_debris::sections::landing_regressions::piece;
    use bri_sim::presentation::RadialImpulse;

    fn blast(origin: Vec3, radius: f32) -> Option<RadialImpulse> {
        Some(RadialImpulse {
            origin: origin.to_array(),
            force: 20.,
            radius,
        })
    }

    #[test]
    fn centered_landing_bounces_without_inventing_rotation() {
        let p = piece(1, Vec3::ZERO);
        let mut m = Mass::default();
        m.add(&p, None, 1.);
        let incoming = Motion {
            velocity: Vec3::NEG_Y * 10.,
            ..m.motion()
        };
        let out = m.impact(incoming, Quat::IDENTITY, Vec3::NEG_Y * p.half.y, Vec3::Y);
        assert!((out.velocity.y - 10. * RESTITUTION).abs() < 1e-5);
        assert!(out.spin.length() < 1e-5);
    }

    #[test]
    fn eccentric_contact_transfers_rotation_without_adding_energy() {
        let p = piece(1, Vec3::ZERO);
        let mut m = Mass::default();
        m.add(&p, None, 1.);
        let incoming = Motion {
            velocity: Vec3::NEG_Y * 10.,
            ..m.motion()
        };
        let point = Vec3::new(-p.half.x, -p.half.y, 0.);
        let out = m.impact(incoming, Quat::IDENTITY, point, Vec3::Y);
        assert!(out.spin.z < 0.);
        let contact_velocity = out.velocity + out.spin.cross(point);
        assert!((contact_velocity.y - 10. * RESTITUTION).abs() < 1e-4);
        let spin = out.spin.as_dvec3();
        let energy = m.mass * f64::from(out.velocity.length_squared())
            + spin.dot(m.inertia_at_center(DVec3::ZERO) * spin);
        assert!(energy <= m.mass * 100. + 1e-4);
        let separating = m.impact(out, Quat::IDENTITY, point, Vec3::Y);
        assert_eq!(separating.velocity, out.velocity);
        assert_eq!(separating.spin, out.spin);
    }

    #[test]
    fn off_center_blast_turns_about_material_center_and_preserves_point_velocity() {
        let mut low = piece(1, Vec3::new(0., 1., 0.));
        low.impulse = blast(Vec3::new(-2., 1., 0.), 3.);
        let high = piece(2, Vec3::new(0., 5., 0.));
        let mut s = Section::new(&low);
        s.admit(low);
        s.admit(high);
        let motion = s.mass.motion();
        assert!((motion.center.y - 3.).abs() < 1e-5);
        assert!(motion.velocity.x > 0.);
        assert!(motion.spin.z > 0.);
        let age = 0.2;
        let (at, turn) = s.pose(age);
        let expected =
            motion.center + motion.velocity * age - Vec3::Y * (0.5 * GRAVITY * age * age);
        assert!((at + turn * motion.center).distance(expected) < 1e-5);
        assert!(turn.angle_between(Quat::IDENTITY) > 0.01);
    }

    #[test]
    fn no_impulse_and_centered_impulse_have_no_invented_spin() {
        let p = piece(1, Vec3::ZERO);
        let mut m = Mass::default();
        m.add(&p, None, 1.);
        assert_eq!(m.motion().velocity, Vec3::ZERO);
        assert_eq!(m.motion().spin, Vec3::ZERO);
        let mut m = Mass::default();
        m.add(&p, blast(Vec3::NEG_X * 2., 4.), 1.);
        assert!(m.motion().velocity.x > 0.);
        assert!(m.motion().spin.length() < 1e-5);
    }

    #[test]
    fn blast_in_empty_gap_does_not_push_surrounding_members() {
        let mut m = Mass::default();
        for x in [-4., 4.] {
            m.add(&piece(1, Vec3::X * x), blast(Vec3::ZERO, 1.), 1.);
        }
        assert_eq!(m.motion().velocity, Vec3::ZERO);
        assert_eq!(m.motion().spin, Vec3::ZERO);
    }

    #[test]
    fn removed_staged_member_removes_its_mass_and_impulse() {
        let p = piece(1, Vec3::new(1., 2., 3.));
        let q = piece(2, Vec3::new(0., 5., 0.));
        let impulse = blast(Vec3::ZERO, 8.);
        let mut m = Mass::default();
        m.add(&p, impulse, 1.);
        m.add(&q, impulse, 1.);
        m.add(&p, impulse, -1.);
        let mut expected = Mass::default();
        expected.add(&q, impulse, 1.);
        assert!(m.motion().center.distance(expected.motion().center) < 1e-5);
        assert!(m.motion().velocity.distance(expected.motion().velocity) < 1e-5);
        assert!(m.motion().spin.distance(expected.motion().spin) < 1e-5);
    }
}
