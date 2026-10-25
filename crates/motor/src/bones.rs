//! Hit bones: the body parts a shot can meet, one cylinder per bone, each
//! with a damage multiplier and the part of the body it belongs to. An
//! archetype that declares none is hit as one box, and its region is read
//! from the height of the hit (`getDamageLocation`).
use bri_console::Clamp;
use glam::{Quat, Vec3};
use rapier3d::prelude::{Pose, Ray, SharedShape, Vector};

pub use bri_content::hit_bones::{Ends, HitBone, MAX_HIT_BONES, MAX_MODIFIER, Region};

/// A hit bone in the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub from: Vec3,
    pub to: Vec3,
    pub radius: f32,
    pub ends: Ends,
    pub modifier: f32,
    pub region: Region,
}
impl Placed {
    /// Where a ray from `origin` along `direction` first meets the bone
    /// within `reach`: the distance and the surface normal.
    pub fn cast(&self, origin: Vec3, direction: Vec3, reach: f32) -> Option<(f32, Vec3)> {
        let axis = self.to - self.from;
        let length = axis.length();
        if length <= 0.0 {
            return None;
        }
        let (shape, pose) = match self.ends {
            Ends::Round => (
                SharedShape::capsule(
                    Vector::from_array(self.from.to_array()),
                    Vector::from_array(self.to.to_array()),
                    self.radius,
                ),
                Pose::IDENTITY,
            ),
            Ends::Flat => {
                let middle = (self.from + self.to) * 0.5;
                (
                    SharedShape::cylinder(length * 0.5, self.radius),
                    Pose::from_parts(
                        Vector::from_array(middle.to_array()),
                        Quat::from_rotation_arc(Vec3::Y, axis / length),
                    ),
                )
            }
        };
        let ray = Ray::new(
            Vector::from_array(origin.to_array()),
            Vector::from_array(direction.to_array()),
        );
        shape
            .cast_ray_and_get_normal(&pose, &ray, reach, true)
            .map(|hit| (hit.time_of_impact, Vec3::from_array(hit.normal.to_array())))
    }
    /// How far `point` is from the bone's surface (0 on or inside it).
    pub fn gap(&self, point: Vec3) -> f32 {
        let axis = self.to - self.from;
        let along =
            ((point - self.from).dot(axis) / axis.length_squared().max(1e-12)).clamped(0.0, 1.0);
        ((point - (self.from + axis * along)).length() - self.radius).max(0.0)
    }
}

/// An archetype's bones for a body at `feet` facing `yaw`, `scale` times
/// the size it was declared at, its height `squash` times a standing body's
/// (a crouched or lying body's lower box).
pub fn place(bones: &[HitBone], feet: Vec3, yaw: f32, scale: f32, squash: f32) -> Vec<Placed> {
    let turn = Quat::from_rotation_y(-yaw);
    let at = |p: [f32; 3]| {
        let p = Vec3::from(p) * scale;
        feet + turn * Vec3::new(p.x, p.y * squash, p.z)
    };
    bones
        .iter()
        .map(|b| Placed {
            from: at(b.from),
            to: at(b.to),
            radius: b.radius * scale,
            ends: b.ends,
            modifier: b.modifier,
            region: b.region,
        })
        .collect()
}

/// The bones a body in this posture is hit through, and whether they are the
/// standing ones that the body's squeezed box must squeeze down. A posture
/// with bones of its own holds them where its animation does, in the feet
/// frame; without, the standing ones follow the box.
pub fn for_posture<'a>(
    standing: &'a [HitBone],
    crouched_set: &'a [HitBone],
    prone_set: &'a [HitBone],
    crouched: bool,
    prone: bool,
) -> (&'a [HitBone], bool) {
    if prone && !prone_set.is_empty() {
        (prone_set, false)
    } else if crouched && !prone && !crouched_set.is_empty() {
        (crouched_set, false)
    } else {
        (standing, true)
    }
}

/// A body's bones where it stands, with the box shots meet them through.
#[derive(Clone, Debug, PartialEq)]
pub struct Body {
    pub bones: Vec<Placed>,
    /// The body's box, which a ray crosses before it can meet a bone.
    pub min: Vec3,
    pub max: Vec3,
}
impl Body {
    /// The same body `by` further on (a body seen where it stood).
    pub fn shifted(&self, by: Vec3) -> Self {
        Self {
            bones: self
                .bones
                .iter()
                .map(|b| Placed {
                    from: b.from + by,
                    to: b.to + by,
                    ..*b
                })
                .collect(),
            min: self.min + by,
            max: self.max + by,
        }
    }
    /// The first bone with a say (a multiplier above 0) a ray meets within
    /// `reach`: its index, the distance and the surface normal.
    pub fn first_hit(
        &self,
        origin: Vec3,
        direction: Vec3,
        reach: f32,
    ) -> Option<(usize, f32, Vec3)> {
        self.bones
            .iter()
            .enumerate()
            .filter(|(_, b)| b.modifier > 0.0)
            .filter_map(|(i, b)| b.cast(origin, direction, reach).map(|(t, n)| (i, t, n)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }
    /// How far along a ray from `origin`, inside the box, it leaves it.
    pub fn leave(&self, origin: Vec3, direction: Vec3) -> Option<f32> {
        let mut out = f32::INFINITY;
        for axis in 0..3 {
            let (d, o) = (direction[axis], origin[axis]);
            if d > 0.0 {
                out = out.min((self.max[axis] - o) / d);
            } else if d < 0.0 {
                out = out.min((self.min[axis] - o) / d);
            }
        }
        (out.is_finite() && out >= 0.0).then_some(out)
    }
    /// The bone nearest `point`, and so the one a hit there struck.
    pub fn nearest(&self, point: Vec3) -> Option<&Placed> {
        self.bones
            .iter()
            .min_by(|a, b| a.gap(point).total_cmp(&b.gap(point)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bone(name: &str) -> HitBone {
        HitBone {
            bone: name.into(),
            from: [0.0, 0.0, 0.0],
            to: [0.0, 1.0, 0.0],
            radius: 0.1,
            ends: Ends::Round,
            modifier: 1.0,
            region: Region::Torso,
        }
    }
    #[test]
    fn a_posture_with_bones_of_its_own_is_hit_through_them_unsqueezed() {
        let (stand, crouch, prone) = ([bone("s")], [bone("c")], [bone("p")]);
        let pick = |c, p| {
            let (set, squeeze) = for_posture(&stand, &crouch, &prone, c, p);
            (set[0].bone.clone(), squeeze)
        };
        assert_eq!(pick(false, false), ("s".into(), true));
        assert_eq!(pick(true, false), ("c".into(), false));
        // A lying body also has its crouch flag set.
        assert_eq!(pick(true, true), ("p".into(), false));
        // Without a set of its own, the standing bones follow the box.
        let (set, squeeze) = for_posture(&stand, &[], &[], true, true);
        assert_eq!((set[0].bone.as_str(), squeeze), ("s", true));
    }
}
