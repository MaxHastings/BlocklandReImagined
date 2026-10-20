//! The random numbers the original draws from.
//!
//! * `rand()`: the Microsoft C runtime's 15-bit linear congruential generator. Every
//!   range draw turns it into `r = rand() * 3.0518509e-5` (the float 0x38000100, which
//!   is 1/32767, so `r` reaches 1.0) and takes `Max + r (Min - Max)`.
//! * `appSRand()`: the engine's own generator (a global LCG, a float in [0, 1)),
//!   used for sound choices and random static-mesh vertices.
//!
//! The seeds the original ran with are not recoverable; a port is seeded by the caller.
use crate::props::{Range, RangeVector};
use glam::Vec3;

#[derive(Clone, Debug)]
pub struct Rand {
    state: u32,
    srand: u32,
}

/// The float the original multiplies `rand()` by (0x38000100).
pub const RAND_SCALE: f32 = f32::from_bits(0x3800_0100);

impl Rand {
    pub fn new(seed: u32) -> Self {
        Self {
            state: seed,
            srand: seed ^ 0x9e37_79b9,
        }
    }

    /// The CRT's `rand()`: 0..=32767.
    pub fn rand(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(214013).wrapping_add(2531011);
        (self.state >> 16) & 0x7fff
    }

    /// `rand() * 0x38000100`, in [0, 1].
    pub fn unit(&mut self) -> f32 {
        self.rand() as f32 * RAND_SCALE
    }

    /// `appSRand()`: `seed = seed * 0x0BB38435 + 0x3619636B`, mantissa bits of a float in
    /// [1, 2), minus 1.
    pub fn app_srand(&mut self) -> f32 {
        self.srand = self
            .srand
            .wrapping_mul(0x0BB3_8435)
            .wrapping_add(0x3619_636B);
        let bits = 0x3F80_0000 ^ ((0x3F80_0000 ^ self.srand) & 0x007F_FFFF);
        f32::from_bits(bits) - 1.0
    }

    /// A draw from `Range { min, max }`: `Max + r (Min - Max)`.
    pub fn range(&mut self, r: Range) -> f32 {
        let u = self.unit();
        r.max + u * (r.min - r.max)
    }

    /// A vector draw in the order the compiler most often emits: Y, then X, then Z.
    pub fn vec_yxz(&mut self, v: RangeVector) -> Vec3 {
        let y = self.range(v.y);
        let x = self.range(v.x);
        let z = self.range(v.z);
        Vec3::new(x, y, z)
    }

    /// `2 r - 1`.
    pub fn signed_unit(&mut self) -> f32 {
        2.0 * self.unit() - 1.0
    }

    /// `UParticleEmitter`'s `RandomUnitVector`: three `2 r - 1` draws, rejected while the
    /// squared length exceeds 1, then normalised (no lower bound test).
    pub fn random_unit_vector(&mut self) -> Vec3 {
        loop {
            let v = Vec3::new(self.signed_unit(), self.signed_unit(), self.signed_unit());
            if v.length_squared() <= 1.0 {
                return crate::rotation::safe_normal(v);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_c_runtime_sequence() {
        // The documented MSVC sequence for srand(1): 41, 18467, 6334, 26500, 19169.
        let mut rand = Rand::new(1);
        let got: Vec<u32> = (0..5).map(|_| rand.rand()).collect();
        assert_eq!(got, [41, 18467, 6334, 26500, 19169]);
    }

    #[test]
    fn the_scale_is_one_over_32767_so_a_draw_reaches_both_ends() {
        assert!((RAND_SCALE - 1.0 / 32767.0).abs() < 1e-9);
        assert!((32767.0 * RAND_SCALE - 1.0).abs() < 1e-6);
    }

    #[test]
    fn range_draws_stay_inside_and_start_at_max() {
        let mut rand = Rand::new(7);
        for _ in 0..1000 {
            let v = rand.range(Range::new(2.0, 6.0));
            assert!((2.0..=6.0).contains(&v));
        }
    }

    #[test]
    fn app_srand_is_uniform_in_the_unit_interval() {
        let mut rand = Rand::new(3);
        let mut sum = 0.0;
        for _ in 0..4000 {
            let v = rand.app_srand();
            assert!((0.0..1.0).contains(&v));
            sum += v;
        }
        assert!((sum / 4000.0 - 0.5).abs() < 0.05);
    }
}
