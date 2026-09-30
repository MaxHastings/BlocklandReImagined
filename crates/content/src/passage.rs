//! Openings that carry what crosses them somewhere else: the sides of
//! linked bricks ([`crate::brick::Link`]) as they stand in a world. Players,
//! vehicles, items and projectiles all move through them with these same
//! rules, on the host and in each player's prediction alike.
use glam::{Affine3A, Vec2, Vec3};

/// One opening in world space. Something crossing it from the front
/// (`normal` side) to the back inside its rectangle is carried by `carry`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Passage {
    /// The linked brick the opening belongs to.
    pub brick: u64,
    pub centre: Vec3,
    /// Unit, out of the front.
    pub normal: Vec3,
    /// Unit in-plane axes and the half size along each.
    pub u: Vec3,
    pub v: Vec3,
    pub half: Vec2,
    /// Takes a point behind this opening to where it is behind the
    /// partner's: a rigid move (quarter turns only, as bricks turn).
    pub carry: Affine3A,
}
impl Passage {
    /// Signed distance in front of the opening's plane.
    pub fn side(&self, p: Vec3) -> f32 {
        self.normal.dot(p - self.centre)
    }
    /// Whether `p`, projected onto the plane, lies within the opening
    /// grown by `margin`.
    pub fn within(&self, p: Vec3, margin: f32) -> bool {
        let d = p - self.centre;
        d.dot(self.u).abs() <= self.half.x + margin && d.dot(self.v).abs() <= self.half.y + margin
    }
    /// The fraction of the way from `a` to `b` where it goes in through
    /// the opening, front to back.
    pub fn crossing(&self, a: Vec3, b: Vec3) -> Option<f32> {
        let (sa, sb) = (self.side(a), self.side(b));
        if !(sa > 0.0 && sb <= 0.0) {
            return None;
        }
        let t = sa / (sa - sb);
        self.within(a.lerp(b, t), 0.0).then_some(t)
    }
    /// The four corners, counterclockwise seen from the front.
    pub fn corners(&self) -> [Vec3; 4] {
        let (du, dv) = (self.u * self.half.x, self.v * self.half.y);
        [
            self.centre - du - dv,
            self.centre + du - dv,
            self.centre + du + dv,
            self.centre - du + dv,
        ]
    }
    /// World box of the opening, grown by `margin` every way.
    pub fn bounds(&self, margin: f32) -> (Vec3, Vec3) {
        let reach = self.u.abs() * self.half.x + self.v.abs() * self.half.y + Vec3::splat(margin);
        (self.centre - reach, self.centre + reach)
    }
}

/// Most openings one move is carried through (two openings facing each
/// other a hair apart could otherwise hand a point back and forth).
pub const MAX_CARRIES: usize = 4;

/// Every opening in a world.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Passages {
    pub list: Vec<Passage>,
    /// Openings with nowhere to lead: shut panes (their `carry` unused).
    pub closed: Vec<Passage>,
}
impl Passages {
    pub fn is_empty(&self) -> bool {
        self.list.is_empty() && self.closed.is_empty()
    }
    /// The first opening the move from `a` to `b` goes in through, and how
    /// far along.
    pub fn first(&self, a: Vec3, b: Vec3) -> Option<(&Passage, f32)> {
        self.list
            .iter()
            .filter_map(|p| Some((p, p.crossing(a, b)?)))
            .min_by(|x, y| x.1.total_cmp(&y.1).then(x.0.brick.cmp(&y.0.brick)))
    }
    /// Follow a move from `a` to `b` through the openings it crosses:
    /// where it ends and every carry applied, composed (None when it went
    /// through none).
    pub fn travel(&self, a: Vec3, b: Vec3) -> (Vec3, Option<Affine3A>) {
        let (mut a, mut b, mut total) = (a, b, None::<Affine3A>);
        for _ in 0..MAX_CARRIES {
            let Some((passage, t)) = self.first(a, b) else {
                break;
            };
            let carry = passage.carry;
            // The rest of the move continues from the far side.
            a = carry.transform_point3(a.lerp(b, t));
            b = carry.transform_point3(b);
            total = Some(carry * total.unwrap_or(Affine3A::IDENTITY));
            // Past the plane by a hair, so it is not met again.
            if (b - a).length_squared() < 1e-12 {
                break;
            }
        }
        (b, total)
    }
    /// How a replicated body seen at `a` and then at `b` got there: the
    /// carry of the opening it went through in between, when that is a
    /// shorter way than straight across (poses are sent a tick or more
    /// apart, so the crossing itself is never seen).
    pub fn bridge(&self, a: Vec3, b: Vec3) -> Option<Affine3A> {
        let straight = a.distance(b);
        self.list
            .iter()
            .filter_map(|p| {
                let back = p.carry.inverse().transform_point3(b);
                p.crossing(a, back)?;
                Some((a.distance(back), p))
            })
            .filter(|(d, _)| *d < straight)
            .min_by(|x, y| x.0.total_cmp(&y.0))
            .map(|(_, p)| p.carry)
    }
    /// The openings whose front `p` is in and whose rectangle, grown by
    /// `reach`, lies within `reach` of it: the ones a body there may be
    /// part way through.
    pub fn near(&self, p: Vec3, reach: f32) -> impl Iterator<Item = &Passage> {
        self.list.iter().filter(move |o| {
            let side = o.side(p);
            side > -1e-4 && side <= reach && o.within(p, reach)
        })
    }
}

/// A direction's heading after `carry` turns it: the yaw (native, 0 facing
/// -Z, turning toward +X) that `yaw` becomes.
pub fn carried_yaw(carry: &Affine3A, yaw: f32) -> f32 {
    let forward = carry.transform_vector3(Vec3::new(yaw.sin(), 0.0, -yaw.cos()));
    if forward.x.abs() + forward.z.abs() < 1e-6 {
        return yaw;
    }
    forward.x.atan2(-forward.z)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2x3 opening in the plane z = 0 facing +z, carried by `carry`.
    fn door(carry: Affine3A) -> Passage {
        Passage {
            brick: 1,
            centre: Vec3::new(0.0, 1.5, 0.0),
            normal: Vec3::Z,
            u: Vec3::X,
            v: Vec3::Y,
            half: Vec2::new(1.0, 1.5),
            carry,
        }
    }

    #[test]
    fn a_move_through_the_opening_comes_out_of_the_partner() {
        let carry = Affine3A::from_translation(Vec3::new(10.0, 0.0, 0.0))
            * Affine3A::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let passages = Passages {
            list: vec![door(carry)],
            closed: vec![],
        };
        let (end, total) = passages.travel(Vec3::new(0.2, 1.0, 0.5), Vec3::new(0.2, 1.0, -0.5));
        let total = total.unwrap();
        assert!(end.abs_diff_eq(carry.transform_point3(Vec3::new(0.2, 1.0, -0.5)), 1e-5));
        assert_eq!(total, carry);
        // Beside the opening, from behind, or along it: no carry.
        for (a, b) in [
            (Vec3::new(1.5, 1.0, 0.5), Vec3::new(1.5, 1.0, -0.5)),
            (Vec3::new(0.0, 1.0, -0.5), Vec3::new(0.0, 1.0, 0.5)),
            (Vec3::new(0.0, 1.0, 0.5), Vec3::new(0.5, 1.0, 0.5)),
        ] {
            assert_eq!(passages.travel(a, b), (b, None));
        }
        // A heading turns with the move it carried.
        let yaw = carried_yaw(&carry, 0.0);
        let moved = carry.transform_vector3(Vec3::NEG_Z);
        assert!(Vec3::new(yaw.sin(), 0.0, -yaw.cos()).abs_diff_eq(moved, 1e-5));
    }

    #[test]
    fn two_openings_handing_a_point_back_stop_after_a_few_carries() {
        // Each leads straight back into the other's front.
        let back = Affine3A::from_translation(Vec3::new(0.0, 0.0, 0.2));
        let passages = Passages {
            list: vec![door(back)],
            closed: vec![],
        };
        let (_, total) = passages.travel(Vec3::new(0.0, 1.0, 0.1), Vec3::new(0.0, 1.0, -0.5));
        assert!(total.is_some());
    }
}
