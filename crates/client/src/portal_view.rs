//! The view camera and the openings of linked bricks
//! ([`bri_content::passage`]): a camera drawn from one side of an opening
//! sees exactly what the far side shows through it, so going through
//! changes nothing on screen, as in Valve's Portal. Only a leftover roll
//! (an opening in a floor or ceiling turns the view over) eases back
//! upright afterwards.
use anyhow::Result;
use bri_content::passage::Passages;
use glam::{Affine3A, Quat, Vec3};

/// A chase camera's boom from `pivot` back along `forward` for `distance`,
/// carried through any opening it goes back through: where the camera ends
/// up and the carries applied, composed (None when it went through none).
/// The pivot rides on a body at `from`, and is first carried through any
/// opening between them. `stop` is where the boom stops in one space (what
/// it bumps into).
pub fn boom(
    from: Vec3,
    pivot: Vec3,
    forward: Vec3,
    distance: f32,
    passages: &Passages,
    mut stop: impl FnMut(Vec3, Vec3, f32) -> Result<Vec3>,
) -> Result<(Vec3, Option<Affine3A>)> {
    let (mut eye, mut total) = match passages.is_empty() {
        true => (pivot, None),
        false => passages.travel(from, pivot),
    };
    let mut forward = total.map_or(forward, |c| c.transform_vector3(forward));
    let mut distance = distance;
    for _ in 0..bri_content::passage::MAX_CARRIES {
        let stopped = stop(eye, forward, distance)?;
        if passages.list.is_empty() || distance <= 0.0 {
            return Ok((stopped, total));
        }
        let end = eye - forward.normalize() * distance;
        let Some((passage, t)) = passages.first(eye, end) else {
            return Ok((stopped, total));
        };
        let through = eye.lerp(end, t);
        // Something between the camera's pivot and the opening stops it.
        if stopped.distance(eye) + 0.02 < through.distance(eye) {
            return Ok((stopped, total));
        }
        let carry = passage.carry;
        distance -= through.distance(eye);
        eye = carry.transform_point3(through);
        forward = carry.transform_vector3(forward);
        total = Some(carry * total.unwrap_or(Affine3A::IDENTITY));
        // Doorway faces are back-to-back on one plane. Rounding the carried
        // intersection can leave it a hair inside the other face, making
        // this boom immediately return to the original room for one frame.
        // Continue just past the plane, as player movement and camera rays
        // do, and charge that hair to the remaining boom length.
        let past = bri_content::passage::PAST.min(distance.max(0.0));
        eye -= forward.normalize() * past;
        distance -= past;
    }
    Ok((eye, total))
}

/// The openings a [`ray`] passed: how far along each was, and the carries
/// up to it composed.
pub type Through = Vec<(f32, Affine3A)>;

/// A camera's ray from `from` to `to`, cast on through any opening it goes
/// in through: the first thing `solid` finds (its distance along the ray
/// from `from`, and its normal turned back into `from`'s space), and the
/// openings passed, each with how far along it was and the carries so far
/// composed. `solid` casts in one space, as [`boom`]'s `stop` does.
pub fn ray(
    from: Vec3,
    to: Vec3,
    passages: &Passages,
    mut solid: impl FnMut(Vec3, Vec3) -> Result<Option<(f32, Vec3)>>,
) -> Result<(Option<(f32, Vec3)>, Through)> {
    let (mut a, mut b) = (from, to);
    let (mut travelled, mut through) = (0.0, Through::new());
    for _ in 0..=bri_content::passage::MAX_CARRIES {
        let hit = solid(a, b)?;
        let length = a.distance(b);
        let opening = (!passages.list.is_empty())
            .then(|| passages.first(a, b))
            .flatten()
            .filter(|(_, t)| hit.is_none_or(|(d, _)| d > t * length));
        let Some((opening, t)) = opening else {
            let back = through.last().map(|(_, c): &(f32, Affine3A)| c.inverse());
            return Ok((
                hit.map(|(d, n)| (travelled + d, back.map_or(n, |c| c.transform_vector3(n)))),
                through,
            ));
        };
        let total = opening.carry * through.last().map_or(Affine3A::IDENTITY, |(_, c)| *c);
        travelled += t * length;
        through.push((travelled, total));
        a = opening.carry.transform_point3(a.lerp(b, t));
        b = opening.carry.transform_point3(b);
        if a.distance(b) < 1e-6 {
            break;
        }
        // Past the partner's plane by a hair, as `Passages::travel` goes on.
        a += (b - a).normalize() * bri_content::passage::PAST;
    }
    Ok((None, through))
}

/// Where a point `distance` along a [`ray`] from `from` is: carried by the
/// openings the ray passed before it.
pub fn along(through: &Through, distance: f32, point: Vec3) -> (Vec3, Option<Affine3A>) {
    match through.iter().rev().find(|(d, _)| *d <= distance) {
        Some((_, carry)) => (carry.transform_point3(point), Some(*carry)),
        None => (point, None),
    }
}

/// The camera at `eye` looking (yaw, pitch, roll) from the body whose
/// middle is at `middle`, carried through any opening between them: a
/// first-person eye leading the middle through, or a chase camera whose
/// `boom` went back through one. Returns the eye and look.
pub fn through(
    eye: Vec3,
    look: (f32, f32, f32),
    boom: Option<Affine3A>,
    middle: Vec3,
    passages: &Passages,
) -> (Vec3, (f32, f32, f32)) {
    if let Some(carry) = boom {
        return (eye, carried_look(look, &carry));
    }
    if passages.is_empty() {
        return (eye, look);
    }
    match passages.travel(middle, eye) {
        (moved, Some(carry)) => (moved, carried_look(look, &carry)),
        (_, None) => (eye, look),
    }
}

/// The view rotation of a look (yaw, pitch, roll), as
/// [`crate::controls::view_frame`] builds it.
pub fn view_rotation((yaw, pitch, roll): (f32, f32, f32)) -> Quat {
    Quat::from_rotation_y(-yaw) * Quat::from_rotation_x(pitch) * Quat::from_rotation_z(roll)
}

/// A chase camera's look, `lean` further down than the player's as
/// `getCameraTransform` turns it (`cameraTilt`): about the view's own level
/// axis, so while a roll an opening left eases out the camera leans as the
/// view is rolled and the picture turns with it, never jumping.
pub fn leaned((yaw, pitch, roll): (f32, f32, f32), lean: f32) -> (f32, f32, f32) {
    if roll == 0.0 {
        // Past straight down too, as v20's chase camera swings over the head.
        return (yaw, pitch - lean, 0.0);
    }
    let view = view_rotation((yaw, pitch, roll)) * Quat::from_rotation_x(-lean);
    let (yaw, pitch) = crate::controls::angles(view * Vec3::NEG_Z, view * Vec3::Y);
    (yaw, pitch, crate::controls::roll(view))
}

/// A look turned the whole way by an opening's carry: the same direction
/// and the same up in the far space, so what it sees does not change. A
/// carry between upright openings only turns the yaw; one through a floor
/// or ceiling also pitches and rolls the view.
pub fn carried_look(look: (f32, f32, f32), carry: &Affine3A) -> (f32, f32, f32) {
    let turn = Quat::from_mat3a(&carry.matrix3).normalize();
    let view = turn * view_rotation(look);
    let (yaw, pitch) = crate::controls::angles(view * Vec3::NEG_Z, view * Vec3::Y);
    let carried = (yaw, pitch, crate::controls::roll(view));
    if carried.0.is_finite() && carried.1.is_finite() && carried.2.is_finite() {
        carried
    } else {
        look
    }
}

/// A body part way through an opening: drawn as itself cut at the
/// opening's plane (`near`, keeping the front) plus a copy moved by
/// `carry` and cut the other way (`far`), so the part already through
/// shows at the partner and nothing jumps when the body is carried.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Straddle {
    pub carry: Affine3A,
    pub near: bri_render::scene::ClipPlane,
    pub far: bri_render::scene::ClipPlane,
}
impl Straddle {
    /// The opening a body whose middle is at `middle` (the point the
    /// simulation carries it by), reaching `reach` from it, is part way
    /// through, if any. The body's drawn place is still in front of that
    /// opening: once its middle crosses it is carried and is in front of
    /// the partner's.
    pub fn find(passages: &Passages, middle: Vec3, reach: f32) -> Option<Self> {
        let opening = passages
            .near(middle, reach)
            .filter(|o| o.within(middle, 0.0))
            .min_by(|a, b| a.side(middle).total_cmp(&b.side(middle)))?;
        let plane = |normal: Vec3, at: Vec3| [normal.x, normal.y, normal.z, -normal.dot(at)];
        let normal = opening.carry.transform_vector3(opening.normal).normalize();
        let centre = opening.carry.transform_point3(opening.centre);
        Some(Self {
            carry: opening.carry,
            near: plane(opening.normal, opening.centre),
            far: plane(-normal, centre),
        })
    }
    /// Where the copy of a body drawn at `transform` draws.
    pub fn carried(&self, transform: glam::Mat4) -> glam::Mat4 {
        glam::Mat4::from(self.carry) * transform
    }
}

/// A first-person body's copies stay hidden from a virtual camera occupying
/// that body's eye, just as from the main camera. `eye` is the body's
/// uncarried camera anchor, not the main camera which may already be through. Other portal and mirror views
/// still see it. The small tolerance covers the reflection clip-plane clearance;
/// it is not a distance-based body fade or a portal-wide visibility switch.
pub fn first_person_body_visible(eye: Vec3, virtual_eye: Vec3, straddle: Option<Straddle>) -> bool {
    const EYE_CLEARANCE: f32 = 0.02;
    let at_eye = |at: Vec3| at.distance_squared(virtual_eye) <= EYE_CLEARANCE * EYE_CLEARANCE;
    if at_eye(eye) {
        return false;
    }
    straddle.is_none_or(|s| !at_eye(s.carry.transform_point3(eye)))
}

/// Everywhere something at `target` shows to an eye at `eye`, as its body
/// does: straight on, unless an opening's view covers it there, and in
/// each opening whose view shows it, with the points the sight goes into
/// that opening and out of its partner. Empty when the openings hide it.
pub fn seen_at(passages: &Passages, eye: Vec3, target: Vec3) -> Vec<SeenAt> {
    let direct = passages.first(eye, target).is_none().then_some(SeenAt {
        at: target,
        through: None,
    });
    let through = passages.list.iter().filter_map(|p| {
        let at = p.carry.inverse().transform_point3(target);
        let (first, t) = passages.first(eye, at)?;
        let near = eye.lerp(at, t);
        let far = p.carry.transform_point3(near);
        // Seen through this opening, no other in the way beyond it.
        (first.brick == p.brick && passages.first(far, target).is_none()).then_some(SeenAt {
            at,
            through: Some((near, far)),
        })
    });
    direct.into_iter().chain(through).collect()
}
/// See [`seen_at`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeenAt {
    pub at: Vec3,
    /// Where the sight enters the opening, and leaves its partner.
    pub through: Option<(Vec3, Vec3)>,
}

/// Two doorways linked as the Portal Add-On's bricks are, for tests of
/// bodies drawn going through.
#[cfg(test)]
pub(crate) mod doorways {
    use super::*;
    use bri_content::passage::Passage;

    /// Doorway A's pane is z = 0 at the origin, open both ways; B's stands
    /// 30 east turned a quarter. Its four openings (A's two faces, then B's)
    /// carry by `carry` and back, as `Links` makes a doorway pair's.
    pub fn pair() -> (Passages, Affine3A) {
        let carry = Affine3A::from_translation(Vec3::new(30.0, 0.0, 0.0))
            * Affine3A::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let opening = |brick, carry: Affine3A, at: Affine3A, normal: Vec3| Passage {
            brick,
            centre: at.transform_point3(Vec3::Y * 1.5),
            normal: at.transform_vector3(normal),
            u: at.transform_vector3(Vec3::X),
            v: Vec3::Y,
            half: glam::Vec2::new(1.0, 1.5),
            carry,
        };
        let one = Affine3A::IDENTITY;
        let passages = Passages {
            list: vec![
                opening(1, carry, one, Vec3::Z),
                opening(1, carry, one, Vec3::NEG_Z),
                opening(2, carry.inverse(), carry, Vec3::Z),
                opening(2, carry.inverse(), carry, Vec3::NEG_Z),
            ],
            closed: vec![],
        };
        (passages, carry)
    }

    /// A body with its middle at `middle` (reaching `reach`) is drawn only
    /// where a body going in through A's front (+z) face toward -z can be:
    /// whole in front of A, cut at A with its far part out of B's -z face
    /// (either way round), or whole out of B. Err names how it is drawn instead.
    pub fn drawn_on_its_way(middle: Vec3, reach: f32) -> Result<(), String> {
        let (passages, carry) = pair();
        let close = |a: [f32; 4], b: [f32; 4]| a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-4);
        let entering = Straddle::find(&passages, Vec3::new(0.0, 1.5, 0.3), 1.0).unwrap();
        match Straddle::find(&passages, middle, reach) {
            // Cut at A, the rest out of B; or once its middle is out, cut
            // at B with the rest still coming in at A: the same picture.
            Some(s)
                if close(s.near, entering.near) && close(s.far, entering.far)
                    || close(s.near, entering.far) && close(s.far, entering.near) =>
            {
                Ok(())
            }
            Some(s) => Err(format!(
                "at {middle} cut at {:?} and {:?}, not A's front and B's back",
                s.near, s.far
            )),
            None => {
                let before = middle.z > 0.0 && middle.length() < 10.0;
                let out = carry.inverse().transform_point3(middle);
                let after = out.z < 0.0 && out.length() < 10.0;
                (before || after)
                    .then_some(())
                    .ok_or(format!("whole at {middle}, out of its way"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portal_views_hide_only_the_first_person_body_at_their_own_eye() {
        use bri_content::passage::Passage;
        use bri_render::reflection::{Looks, Mirror, ReflectionSettings, plan};
        use bri_render::scene::Camera;
        for turn in [0.0, FRAC_PI_2, PI] {
            let a = Affine3A::from_translation(Vec3::new(3.0, 2.0, -7.0));
            let b = Affine3A::from_translation(Vec3::new(23.0, 4.0, 11.0))
                * Affine3A::from_rotation_y(turn);
            let half = Affine3A::from_rotation_y(PI);
            let carry = b * half * a.inverse();
            let opening = |brick, pose: Affine3A, carry| Passage {
                brick,
                centre: pose.transform_point3(Vec3::ZERO),
                normal: pose.transform_vector3(Vec3::Z),
                u: pose.transform_vector3(Vec3::X),
                v: Vec3::Y,
                half: glam::Vec2::splat(2.0),
                carry,
            };
            let passages = Passages {
                list: vec![opening(1, a, carry), opening(2, b, carry.inverse())],
                closed: vec![],
            };
            let window = |pose: Affine3A, carry: Affine3A| Mirror {
                corners: [
                    Vec3::new(-2.0, -2.0, 0.0),
                    Vec3::new(2.0, -2.0, 0.0),
                    Vec3::new(2.0, 2.0, 0.0),
                    Vec3::new(-2.0, 2.0, 0.0),
                ]
                .map(|p| pose.transform_point3(p)),
                tint: [1.0; 3],
                strength: 1.0,
                looks: Looks::Through(glam::Mat4::from(carry.inverse())),
                fallback: [0.0; 3],
                recess: 0.2,
            };
            let windows = [window(a, carry), window(b, carry.inverse())];
            for crossed_eye in [false, true] {
                let middle = a.transform_point3(Vec3::new(0.0, 0.0, 0.15));
                let raw_eye =
                    a.transform_point3(Vec3::new(0.0, 0.0, if crossed_eye { -0.1 } else { 0.25 }));
                let (eye, _) = through(raw_eye, (0.0, 0.0, 0.0), None, middle, &passages);
                // Once the eye crosses, look back at the still-straddling body.
                let forward = if crossed_eye {
                    carry.transform_vector3(Vec3::Z)
                } else {
                    Vec3::NEG_Z
                };
                let camera = Camera::perspective(
                    eye.to_array(),
                    (eye + forward).to_array(),
                    1.0,
                    1.0,
                    0.05,
                    100.0,
                );
                let views = plan(
                    &windows,
                    glam::Mat4::from_cols_array(&camera.view_projection),
                    eye,
                    &ReflectionSettings::HIGH,
                    (128, 128),
                );
                assert!(
                    !views.planes.is_empty(),
                    "crossing must actually render a portal: turn={turn} crossed_eye={crossed_eye} eye={eye:?}"
                );
                let straddle =
                    Straddle::find(&passages, middle, 0.9).expect("body spans the opening");
                let near_view = views
                    .planes
                    .iter()
                    .find(|v| {
                        v.eye.distance(raw_eye) < 0.001
                            || v.eye.distance(carry.transform_point3(raw_eye)) < 0.001
                    })
                    .expect("portal camera occupies one of the split body's eyes");
                assert!(!first_person_body_visible(
                    raw_eye,
                    near_view.eye,
                    Some(straddle)
                ));
                // A distant view of this same split body still shows it; so does
                // an ordinary mirror. Neither depends on the content's name.
                assert!(first_person_body_visible(
                    raw_eye,
                    near_view.eye + Vec3::Y * 2.0,
                    Some(straddle)
                ));
                assert!(first_person_body_visible(eye, eye + Vec3::X * 3.0, None));
            }
        }
    }

    #[test]
    fn a_portal_cannot_hide_a_body_at_a_nonexistent_inverse_eye_copy() {
        let eye = Vec3::new(1.0, 2.0, 3.0);
        let carry = Affine3A::from_translation(Vec3::new(20.0, 0.0, 0.0))
            * Affine3A::from_rotation_y(FRAC_PI_2);
        let straddle = Straddle {
            carry,
            near: [0.0, 0.0, 1.0, 0.0],
            far: [1.0, 0.0, 0.0, -20.0],
        };
        assert!(!first_person_body_visible(
            eye,
            carry.transform_point3(eye),
            Some(straddle)
        ));
        assert!(first_person_body_visible(
            eye,
            carry.inverse().transform_point3(eye),
            Some(straddle)
        ));
    }

    #[test]
    fn a_name_shows_where_the_portal_shows_its_body() {
        use bri_content::passage::Passage;
        // An opening facing the eye at z = -5 leads to one 40 along x,
        // facing back; a body 3 behind that partner shows 3 past the near one.
        let carry = Affine3A::from_translation(Vec3::new(40., 0., -5.))
            * Affine3A::from_rotation_y(PI)
            * Affine3A::from_translation(Vec3::new(0., 0., 5.));
        let opening = |brick, centre: Vec3, normal: Vec3, carry| Passage {
            brick,
            centre,
            normal,
            u: Vec3::X,
            v: Vec3::Y,
            half: glam::Vec2::new(2., 2.),
            carry,
        };
        let passages = Passages {
            list: vec![
                opening(1, Vec3::new(0., 0., -5.), Vec3::Z, carry),
                opening(2, Vec3::new(40., 0., -5.), Vec3::Z, carry.inverse()),
            ],
            closed: vec![],
        };
        // Straight behind the near opening: hidden by its view.
        assert_eq!(
            seen_at(&passages, Vec3::ZERO, Vec3::new(0., 0., -9.)),
            vec![]
        );
        // In the open: where it stands.
        let open = Vec3::new(10., 0., -9.);
        let at = |target| {
            seen_at(&passages, Vec3::ZERO, target)
                .iter()
                .map(|s| s.at)
                .collect()
        };
        assert_eq!(at(open), vec![open]);
        // Out of the partner: where it stands, and past the near opening.
        let seen: Vec<Vec3> = at(Vec3::new(40., 0., -2.));
        assert_eq!(seen.len(), 2);
        assert!(seen[1].distance(Vec3::new(0., 0., -8.)) < 1e-4, "{seen:?}");
    }
    use std::f32::consts::{FRAC_PI_2, PI};

    fn same(a: Quat, b: Quat) -> bool {
        a.dot(b).abs() > 1.0 - 1e-5
    }

    #[test]
    fn a_carried_look_sees_along_the_carried_view() {
        let carries = [
            Affine3A::from_rotation_y(FRAC_PI_2),
            Affine3A::from_rotation_y(PI),
            // A floor opening onto another floor: a half turn about a level axis.
            Affine3A::from_rotation_x(PI),
            Affine3A::from_rotation_z(PI),
            Affine3A::from_rotation_x(FRAC_PI_2),
        ];
        for carry in carries {
            for look in [(0.3, -0.4, 0.0), (-2.0, 1.2, 0.0), (1.0, -1.0, 0.5)] {
                let carried = carried_look(look, &carry);
                let turn = Quat::from_mat3a(&carry.matrix3);
                assert!(
                    same(view_rotation(carried), turn * view_rotation(look)),
                    "{look:?} through {carry:?} gave {carried:?}"
                );
            }
        }
        // A leaned chase look is the look turned down about its level axis.
        for look in [(0.3, -0.4, 0.0), (1.0, -1.0, 0.5), (-2.0, 1.2, 3.0)] {
            assert!(same(
                view_rotation(leaned(look, 0.26)),
                view_rotation(look) * Quat::from_rotation_x(-0.26)
            ));
        }
        // Upright openings only turn the yaw.
        let (yaw, pitch, roll) = carried_look((0.3, -0.4, 0.0), &carries[0]);
        assert!(((yaw - 0.3 + FRAC_PI_2 + PI).rem_euclid(2.0 * PI) - PI).abs() < 1e-5);
        assert!((pitch + 0.4).abs() < 1e-5 && roll.abs() < 1e-5);
        // A floor onto a floor turns the view over: looking up, upside down.
        let (_, pitch, roll) = carried_look((0.3, -0.4, 0.0), &carries[2]);
        assert!((pitch - 0.4).abs() < 1e-5 && (roll.abs() - PI).abs() < 1e-4);
    }

    #[test]
    fn a_camera_ray_goes_on_through_an_opening_into_the_far_space() {
        use bri_content::passage::{Passage, Passages};
        // In through z = 0 going -z, out ten units along x and turned a
        // quarter, where a wall stands at x = 12.
        let carry =
            Affine3A::from_translation(Vec3::X * 10.0) * Affine3A::from_rotation_y(-FRAC_PI_2);
        let passages = Passages {
            list: vec![Passage {
                brick: 1,
                centre: Vec3::Y,
                normal: Vec3::Z,
                u: Vec3::X,
                v: Vec3::Y,
                half: glam::Vec2::new(1.0, 1.0),
                carry,
            }],
            closed: vec![],
        };
        let wall = |a: Vec3, b: Vec3| {
            Ok((a.x < 12.0 && b.x >= 12.0)
                .then(|| ((12.0 - a.x) / (b.x - a.x) * a.distance(b), Vec3::NEG_X)))
        };
        let (hit, through) = ray(
            Vec3::new(0.0, 1.0, 1.0),
            Vec3::new(0.0, 1.0, -5.0),
            &passages,
            wall,
        )
        .unwrap();
        let (distance, normal) = hit.unwrap();
        assert!((distance - 3.0).abs() < 1e-3, "{distance}");
        // The wall faces back along the ray, in the ray's own space.
        assert!(normal.abs_diff_eq(Vec3::Z, 1e-5), "{normal}");
        assert_eq!(through.len(), 1);
        assert!((through[0].0 - 1.0).abs() < 1e-5);
        let (eye, turned) = along(&through, 2.5, Vec3::new(0.0, 1.0, -1.5));
        assert!(eye.abs_diff_eq(Vec3::new(11.5, 1.0, 0.0), 1e-4), "{eye}");
        assert_eq!(turned, Some(carry));
        assert_eq!(along(&through, 0.5, Vec3::ZERO), (Vec3::ZERO, None));
    }

    #[test]
    fn a_body_part_way_through_draws_its_far_half_at_the_partner() {
        use bri_content::passage::{Passage, Passages};
        let carry =
            Affine3A::from_translation(Vec3::X * 10.0) * Affine3A::from_rotation_y(-FRAC_PI_2);
        let passages = Passages {
            list: vec![Passage {
                brick: 1,
                centre: Vec3::Y,
                normal: Vec3::Z,
                u: Vec3::X,
                v: Vec3::Y,
                half: glam::Vec2::new(1.0, 1.0),
                carry,
            }],
            closed: vec![],
        };
        let kept =
            |plane: [f32; 4], p: Vec3| Vec3::from_slice(&plane[..3]).dot(p) + plane[3] >= 0.0;
        let s = Straddle::find(&passages, Vec3::new(0.0, 1.0, 0.3), 1.0).unwrap();
        assert_eq!(s.carry, carry);
        // A hand already through shows only at the partner; the back of the
        // body still in front shows only here.
        let (hand, back) = (Vec3::new(0.2, 1.2, -0.4), Vec3::new(0.0, 1.0, 0.6));
        assert!(!kept(s.near, hand) && kept(s.far, carry.transform_point3(hand)));
        assert!(kept(s.near, back) && !kept(s.far, carry.transform_point3(back)));
        let moved = s.carried(glam::Mat4::from_translation(hand));
        assert!(
            moved
                .w_axis
                .truncate()
                .abs_diff_eq(carry.transform_point3(hand), 1e-5)
        );
        // Beside the opening, too far in front, or already carried: whole.
        assert!(Straddle::find(&passages, Vec3::new(2.5, 1.0, 0.3), 1.0).is_none());
        assert!(Straddle::find(&passages, Vec3::new(0.0, 1.0, 1.5), 1.0).is_none());
        assert!(Straddle::find(&passages, Vec3::new(0.0, 1.0, -0.3), 1.0).is_none());
    }
}
