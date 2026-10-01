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

#[cfg(test)]
mod tests {
    use super::*;
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
            Ok((a.x < 12.0 && b.x >= 12.0).then(|| {
                ((12.0 - a.x) / (b.x - a.x) * a.distance(b), Vec3::NEG_X)
            }))
        };
        let (hit, through) =
            ray(Vec3::new(0.0, 1.0, 1.0), Vec3::new(0.0, 1.0, -5.0), &passages, wall).unwrap();
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
}
