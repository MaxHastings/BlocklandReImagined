//! The original's integer rotators and coordinate frames, as the emitter code uses them
//! (Core.dll: `FCoords::operator*(FRotator)`, `TransformVectorBy`, `FVector::Rotation`,
//! `RotateAngleAxis`). Angles are 65536 per turn; sine and cosine come from a
//! 16384-entry table indexed `(angle >> 2) & 0x3fff`, here built as an exact sine.
use glam::Vec3;

pub fn sin_tab(angle: i32) -> f32 {
    let i = ((angle >> 2) & 0x3fff) as f32;
    (std::f32::consts::TAU * i / 16384.0).sin()
}
pub fn cos_tab(angle: i32) -> f32 {
    sin_tab(angle.wrapping_add(0x4000))
}

/// `FRotator`: Pitch, Yaw, Roll in 65536ths of a turn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rotator {
    pub pitch: i32,
    pub yaw: i32,
    pub roll: i32,
}
impl Rotator {
    pub const fn new(pitch: i32, yaw: i32, roll: i32) -> Self {
        Self { pitch, yaw, roll }
    }
    /// `FRotator::operator+`.
    #[allow(clippy::should_implement_trait)]
    pub fn add(self, other: Rotator) -> Rotator {
        Rotator::new(
            self.pitch.wrapping_add(other.pitch),
            self.yaw.wrapping_add(other.yaw),
            self.roll.wrapping_add(other.roll),
        )
    }
}

/// `FCoords`: an origin and three axes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Coords {
    pub origin: Vec3,
    pub x: Vec3,
    pub y: Vec3,
    pub z: Vec3,
}

impl Coords {
    pub const UNIT: Coords = Coords {
        origin: Vec3::ZERO,
        x: Vec3::X,
        y: Vec3::Y,
        z: Vec3::Z,
    };

    /// `v.TransformVectorBy(coords)`: `v.x X + v.y Y + v.z Z`.
    pub fn transform_vector(&self, v: Vec3) -> Vec3 {
        v.x * self.x + v.y * self.y + v.z * self.z
    }
    fn transform_point(&self, p: Vec3) -> Vec3 {
        // `A *= B` transforms A's origin as a point by B: (P - B.origin) . axes.
        let d = p - self.origin;
        Vec3::new(d.dot(self.x), d.dot(self.y), d.dot(self.z))
    }
    fn mul_assign(&mut self, b: &Coords) {
        self.origin = b.transform_point(self.origin);
        self.x = b.transform_vector(self.x);
        self.y = b.transform_vector(self.y);
        self.z = b.transform_vector(self.z);
    }
    /// `FCoords::operator*(FRotator)`: self, then yaw, pitch and roll applied in turn.
    pub fn mul_rotator(self, r: Rotator) -> Coords {
        let mut c = self;
        let (sy, cy) = (sin_tab(r.yaw), cos_tab(r.yaw));
        let yaw = Coords {
            origin: Vec3::ZERO,
            x: Vec3::new(cy, sy, 0.0),
            y: Vec3::new(-sy, cy, 0.0),
            z: Vec3::Z,
        };
        c.mul_assign(&yaw);
        let (sp, cp) = (sin_tab(r.pitch), cos_tab(r.pitch));
        let pitch = Coords {
            origin: Vec3::ZERO,
            x: Vec3::new(cp, 0.0, sp),
            y: Vec3::Y,
            z: Vec3::new(-sp, 0.0, cp),
        };
        c.mul_assign(&pitch);
        let (sr, cr) = (sin_tab(r.roll), cos_tab(r.roll));
        let roll = Coords {
            origin: Vec3::ZERO,
            x: Vec3::X,
            y: Vec3::new(0.0, cr, -sr),
            z: Vec3::new(0.0, sr, cr),
        };
        c.mul_assign(&roll);
        c
    }
}

/// FRotationMatrix (Engine 0xba0e0): the rows of the original's row-vector matrix, returned
/// as the matrix whose columns they are, so `m * v` equals the original's `v M`.
pub fn rotation_matrix(r: Rotator) -> glam::Mat3 {
    let (sp, cp) = (sin_tab(r.pitch), cos_tab(r.pitch));
    let (sy, cy) = (sin_tab(r.yaw), cos_tab(r.yaw));
    let (sr, cr) = (sin_tab(r.roll), cos_tab(r.roll));
    glam::Mat3::from_cols(
        Vec3::new(cp * cy, cp * sy, sp),
        Vec3::new(sr * sp * cy - cr * sy, sr * sp * sy + cr * cy, -sr * cp),
        Vec3::new(-(cr * sp * cy + sr * sy), cy * sr - cr * sp * sy, cr * cp),
    )
}

/// `FVector::Rotation()`: yaw from atan2(y, x), pitch from atan2(z, sqrt(x^2 + y^2)); no roll.
pub fn vector_rotation(v: Vec3) -> Rotator {
    let k = 65535.0 / std::f64::consts::TAU;
    let yaw = ((f64::from(v.y).atan2(f64::from(v.x))) * k) as i32;
    let flat = (f64::from(v.x) * f64::from(v.x) + f64::from(v.y) * f64::from(v.y)).sqrt();
    let pitch = ((f64::from(v.z).atan2(flat)) * k) as i32;
    Rotator::new(pitch, yaw, 0)
}

/// `FVector::RotateAngleAxis` (Core.dll 0x7e30): Rodrigues with the axis as given.
pub fn rotate_angle_axis(v: Vec3, angle: i32, axis: Vec3) -> Vec3 {
    let s = sin_tab(angle);
    let c = cos_tab(angle);
    let omc = 1.0 - c;
    let (x, y, z) = (axis.x, axis.y, axis.z);
    Vec3::new(
        (omc * x * x + c) * v.x + (omc * x * y - z * s) * v.y + (omc * z * x + y * s) * v.z,
        (omc * x * y + z * s) * v.x + (omc * y * y + c) * v.y + (omc * y * z - x * s) * v.z,
        (omc * z * x - y * s) * v.x + (omc * y * z + x * s) * v.y + (omc * z * z + c) * v.z,
    )
}

/// `v - 2 (v . n) n / |n|^2`.
pub fn mirror_by_vector(v: Vec3, n: Vec3) -> Vec3 {
    v - n * (2.0 * v.dot(n) / n.length_squared())
}

/// The vector's unit vector, or zero when its squared length is under 1e-8.
pub fn safe_normal(v: Vec3) -> Vec3 {
    let l2 = v.length_squared();
    if l2 >= 1e-8 {
        v * (1.0 / l2.sqrt())
    } else {
        Vec3::ZERO
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quarter_turn_of_yaw_takes_x_to_y() {
        let c = Coords::UNIT.mul_rotator(Rotator::new(0, 16384, 0));
        let v = c.transform_vector(Vec3::X);
        assert!((v - Vec3::Y).length() < 1e-4, "{v:?}");
    }

    #[test]
    fn rotation_of_a_direction_round_trips() {
        let r = vector_rotation(Vec3::new(0.0, 1.0, 0.0));
        assert_eq!(r.yaw, 16383); // ftol truncates 65535/4
        assert_eq!(r.pitch, 0);
        let up = vector_rotation(Vec3::Z);
        assert_eq!(up.pitch, 16383);
    }

    #[test]
    fn rodrigues_matches_the_right_handed_quarter_turn() {
        let v = rotate_angle_axis(Vec3::Y, 16384, Vec3::Z);
        assert!((v - Vec3::NEG_X).length() < 1e-4, "{v:?}");
    }

    #[test]
    fn mirror_reflects_about_the_normal() {
        let v = mirror_by_vector(Vec3::new(1.0, -1.0, 0.0), Vec3::Y);
        assert!((v - Vec3::new(1.0, 1.0, 0.0)).length() < 1e-6);
    }
}
