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

/// How far past a partner's plane a carried move starts again. A carried
/// point lands on that plane only to within rounding, and a doorway's two
/// openings share one plane back to back, so without this hair the rest of
/// the move could go in through the other one and be carried straight back.
pub const PAST: f32 = 1e-4;

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
            let rest = b - a;
            if rest.length_squared() < 1e-12 {
                break;
            }
            a += rest.normalize() * PAST.min(rest.length());
        }
        (b, total)
    }
    /// A sight (a ray) from `from` along unit `direction` for `length`,
    /// through the openings it goes in by: one straight leg per side, in
    /// order. Where things meet it is up to the caller, leg by leg: a hit
    /// on a leg ends the sight there.
    pub fn sight(&self, from: Vec3, direction: Vec3, length: f32) -> Vec<Leg> {
        let mut legs = Vec::new();
        let (mut from, mut direction, mut start) = (from, direction, 0.0);
        let mut carry = Affine3A::IDENTITY;
        for _ in 0..=MAX_CARRIES {
            let left = length - start;
            if left <= 0.0 {
                break;
            }
            let opening = self.first(from, from + direction * left);
            let span = opening.map_or(left, |(_, t)| left * t);
            legs.push(Leg {
                from,
                direction,
                start,
                length: span,
                carry,
            });
            let Some((passage, _)) = opening else {
                break;
            };
            // On from the partner's side, past its plane by a hair.
            let end = passage.carry.transform_point3(from + direction * span);
            direction = passage.carry.transform_vector3(direction).normalize();
            from = end + direction * PAST;
            start += span + PAST;
            carry = passage.carry * carry;
        }
        legs
    }
    /// The straight ways from `a` to `b` no longer than `reach`: across,
    /// and in through each one opening that carries a sight from `a` on to
    /// `b`. Each is a sight ([`Self::sight`]) aimed at its `aim` that ends
    /// at `b`, as a shot fired at it would; what stands in its way is the
    /// caller's to find, leg by leg. What a bot sees and aims by.
    pub fn ways(&self, a: Vec3, b: Vec3, reach: f32) -> impl Iterator<Item = Way> + '_ {
        let across = std::iter::once((b, None));
        // `b` as seen from in front of an opening: where it would be if the
        // partner's side stood right behind it.
        let through = self
            .list
            .iter()
            .map(move |p| (p.carry.inverse().transform_point3(b), Some(p.carry)));
        across.chain(through).filter_map(move |(aim, carry)| {
            let length = a.distance(aim);
            if !(1e-3..=reach).contains(&length) {
                return None;
            }
            let legs = self.sight(a, (aim - a) / length, length);
            // Through that one opening, or none, and out at `b`.
            let openings = usize::from(carry.is_some());
            (legs.len() == openings + 1 && legs.last()?.at(length).distance(b) < 1e-3)
                .then_some(Way { aim, carry, length })
        })
    }
    /// A sight ([`Self::sight`]) asking `meet` what it meets on each leg
    /// in turn: the first answer and the leg it came on. Everything that
    /// aims (tools, clicks, the brick in hand) looks through openings this
    /// one way.
    pub fn cast<H, E>(
        &self,
        from: Vec3,
        direction: Vec3,
        length: f32,
        mut meet: impl FnMut(&Leg) -> Result<Option<H>, E>,
    ) -> Result<Option<(H, Leg)>, E> {
        for leg in self.sight(from, direction, length) {
            if let Some(hit) = meet(&leg)? {
                return Ok(Some((hit, leg)));
            }
        }
        Ok(None)
    }
    /// The shortest straight way from `a` to `b`: how long it is, and the
    /// carry of the one opening it goes in through, if it is shorter that
    /// way than straight across. Reaches are measured along it, so
    /// something seen through a portal is as near as it looks.
    pub fn shortest(&self, a: Vec3, b: Vec3) -> (f32, Option<Affine3A>) {
        self.list
            .iter()
            .filter_map(|p| {
                let back = p.carry.inverse().transform_point3(b);
                p.crossing(a, back)?;
                Some((a.distance(back), Some(p.carry)))
            })
            .fold((a.distance(b), None), |best, way| {
                if way.0 < best.0 { way } else { best }
            })
    }
    /// How a replicated body seen at `a` and then at `b` got there: the
    /// carry of the opening it went through in between, when that is a
    /// shorter way than straight across (poses are sent a tick or more
    /// apart, so the crossing itself is never seen).
    pub fn bridge(&self, a: Vec3, b: Vec3) -> Option<Affine3A> {
        self.shortest(a, b).1
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

/// One straight stretch of a sight ([`Passages::sight`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Leg {
    pub from: Vec3,
    /// Unit.
    pub direction: Vec3,
    /// How far along the whole sight it begins, and how long it is.
    pub start: f32,
    pub length: f32,
    /// Takes a point where the sight began to where this leg is: the
    /// openings before it, composed (identity on the first).
    pub carry: Affine3A,
}
impl Leg {
    /// Where the sight is `distance` along it, when that is on this leg.
    pub fn at(&self, distance: f32) -> Vec3 {
        self.from + self.direction * (distance - self.start)
    }
    /// How far `p` is from this leg.
    pub fn off(&self, p: Vec3) -> f32 {
        let along = (p - self.from).dot(self.direction).clamp(0.0, self.length);
        p.distance(self.from + self.direction * along)
    }
}

/// One straight way between two points ([`Passages::ways`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Way {
    /// The point to look or shoot at: the far end itself, or it as seen
    /// through the opening.
    pub aim: Vec3,
    /// The opening's carry, for a way through one.
    pub carry: Option<Affine3A>,
    /// How far it goes.
    pub length: f32,
}
impl Way {
    /// A point near the far end as seen along this way (`carry` undone).
    pub fn seen(&self, p: Vec3) -> Vec3 {
        self.carry.map_or(p, |c| c.inverse().transform_point3(p))
    }
    /// A direction at the far end as seen along this way.
    pub fn seen_vector(&self, v: Vec3) -> Vec3 {
        self.carry.map_or(v, |c| c.inverse().transform_vector3(v))
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
    fn a_sight_through_the_opening_goes_on_from_the_partner() {
        let carry = Affine3A::from_translation(Vec3::new(10.0, 0.0, 0.0))
            * Affine3A::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let passages = Passages {
            list: vec![door(carry)],
            closed: vec![],
        };
        let legs = passages.sight(Vec3::new(0.2, 1.0, 4.0), Vec3::NEG_Z, 10.0);
        assert_eq!(legs.len(), 2);
        assert!((legs[0].length - 4.0).abs() < 1e-5 && legs[0].carry == Affine3A::IDENTITY);
        let (far, total) = (legs[1], 4.0 + PAST);
        assert_eq!(far.carry, carry);
        assert!((far.start - total).abs() < 1e-5 && (far.start + far.length - 10.0).abs() < 1e-5);
        // Each point along it is the straight sight's, carried.
        let straight = Vec3::new(0.2, 1.0, 4.0 - 7.0);
        assert!(far.at(7.0).abs_diff_eq(carry.transform_point3(straight), 1e-4));
        assert!(far.off(far.at(7.0)) < 1e-4 && legs[0].off(far.at(7.0)) > 1.0);
        // Beside the opening: one leg, straight on.
        let legs = passages.sight(Vec3::new(1.5, 1.0, 4.0), Vec3::NEG_Z, 10.0);
        assert_eq!(legs.len(), 1);
        assert!((legs[0].length - 10.0).abs() < 1e-5);
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

    #[test]
    fn a_point_beyond_the_partner_is_seen_through_the_opening() {
        // In through the door at z = 0 (from +z), out ten units east facing
        // +z again.
        let carry = Affine3A::from_translation(Vec3::new(10.0, 0.0, 0.0));
        let passages = Passages {
            list: vec![door(carry)],
            closed: vec![],
        };
        let (a, b) = (Vec3::new(0.0, 1.0, 4.0), Vec3::new(10.0, 1.0, -3.0));
        let ways: Vec<Way> = passages.ways(a, b, 50.0).collect();
        assert_eq!(ways.len(), 2, "across and through: {ways:?}");
        let through = ways.iter().find(|w| w.carry.is_some()).unwrap();
        assert!(through.aim.abs_diff_eq(Vec3::new(0.0, 1.0, -3.0), 1e-5));
        assert!((through.length - 7.0).abs() < 1e-4);
        // A shot at the aim arrives at `b`.
        assert!(passages.travel(a, through.aim).0.abs_diff_eq(b, 1e-4));
        // Straight across through the opening's own plane is no way: what
        // goes in comes out elsewhere.
        let behind = Vec3::new(0.0, 1.0, -3.0);
        assert!(passages.ways(a, behind, 50.0).all(|w| w.carry.is_some()));
        // Too far either way, or the opening missed: none.
        assert_eq!(passages.ways(a, b, 6.0).count(), 0);
        let wide = Vec3::new(14.0, 1.0, -3.0);
        assert!(passages.ways(a, wide, 50.0).all(|w| w.carry.is_none()));
    }
}
