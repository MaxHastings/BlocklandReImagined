//! Texture-coordinate motion as a two-dimensional affine map, evaluated on the CPU.
//!
//! A [`UvMotion`] is what one Unreal Engine 2 texture modifier does to a picture's
//! coordinates; a chain of them (a rotator around a panner around a texture) multiplies
//! out, outermost first, into one map per frame, `acc = acc x M_i` in the original's
//! row-vector convention (D3DDrv.dll 0xc1d0). The map is `u' = a u + c v + tx`,
//! `v' = b u + d v + ty`, the layout of a 3x3 row-vector matrix whose last row is the
//! translation (Direct3D extends 2D coordinates with a one).
//!
//! The evaluation mirrors the scene shader's single-motion path exactly, so a chain of
//! one equals what the GPU does for a material with just that motion.
use crate::shape::{UvMotion, UvRotator, UvWave, UvWaveKind};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub tx: f64,
    pub ty: f64,
}

impl Affine {
    pub const IDENTITY: Affine = Affine {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    pub fn translate(x: f64, y: f64) -> Self {
        Self {
            tx: x,
            ty: y,
            ..Self::IDENTITY
        }
    }

    pub fn scale(x: f64, y: f64) -> Self {
        Self {
            a: x,
            d: y,
            ..Self::IDENTITY
        }
    }

    /// Apply `self` first, then `next`: the row-vector product `self x next`.
    pub fn then(self, next: Affine) -> Affine {
        Affine {
            a: self.a * next.a + self.b * next.c,
            b: self.a * next.b + self.b * next.d,
            c: self.c * next.a + self.d * next.c,
            d: self.c * next.b + self.d * next.d,
            tx: self.tx * next.a + self.ty * next.c + next.tx,
            ty: self.tx * next.b + self.ty * next.d + next.ty,
        }
    }

    pub fn apply(self, u: f64, v: f64) -> (f64, f64) {
        (
            self.a * u + self.c * v + self.tx,
            self.b * u + self.d * v + self.ty,
        )
    }

    /// The six numbers in the order the scene shader reads them: `a, b, c, d, tx, ty`.
    pub fn to_array(self) -> [f32; 6] {
        [
            self.a as f32,
            self.b as f32,
            self.c as f32,
            self.d as f32,
            self.tx as f32,
            self.ty as f32,
        ]
    }
}

fn hash(i: f64) -> f64 {
    let mut h = (i.abs() as u32)
        .wrapping_mul(747_796_405)
        .wrapping_add(2_891_336_453);
    h = ((h >> ((h >> 28) + 4)) ^ h).wrapping_mul(277_803_737);
    f64::from(((h >> 22) ^ h) & 32767) / 32768.0
}

impl UvWave {
    /// The offset and scale this wave gives its axis at `time` seconds.
    pub fn sample(&self, time: f64) -> (f64, f64) {
        let tau = std::f64::consts::TAU;
        let cycles = time * f64::from(self.rate);
        let f = cycles - cycles.floor();
        let (phase, amplitude) = (f64::from(self.phase), f64::from(self.amplitude));
        match self.kind {
            UvWaveKind::None => (0.0, 1.0),
            UvWaveKind::Pan => ((tau * (f + phase)).sin() * amplitude, 1.0),
            UvWaveKind::Stretch => (0.0, 1.0 + (tau * (f + phase)).sin() * amplitude),
            UvWaveKind::StretchRepeat => (
                0.0,
                1.0 + (f * std::f64::consts::FRAC_PI_2 + tau * phase).sin() * amplitude,
            ),
            UvWaveKind::Jitter => {
                let held = if f > phase {
                    time
                } else {
                    cycles.floor() / f64::from(self.rate).abs().max(1e-6)
                };
                (hash((held * 60.0).floor()) * amplitude, 1.0)
            }
        }
    }
}

impl UvRotator {
    /// The upper-left two-by-two and the translation row of the rotation matrix
    /// (`FRotationMatrix`, Engine.dll 0xba0e0) for the axis angles at `time`, as an
    /// [`Affine`] (rows `(cp cy, cp sy)`, `(sr sp cy - cr sy, sr sp sy + cr cy)` and the
    /// translation `(-(cr sp cy + sr sy), cy sr - cr sp sy)`).
    pub fn affine(&self, time: f64) -> Affine {
        let tau = std::f64::consts::TAU;
        let angle = |k: usize| {
            let cycles = time * f64::from(self.wave_rate[k]);
            let f = cycles - cycles.floor();
            (f64::from(self.base_degrees[k]) + f64::from(self.rate_degrees[k]) * time).to_radians()
                + f64::from(self.wave_amplitude_degrees[k]).to_radians()
                    * (tau * (f + f64::from(self.wave_phase[k]))).sin()
        };
        let (pitch, yaw, roll) = (angle(0), angle(1), angle(2));
        let (cp, sp) = (pitch.cos(), pitch.sin());
        let (cy, sy) = (yaw.cos(), yaw.sin());
        let (cr, sr) = (roll.cos(), roll.sin());
        Affine {
            a: cp * cy,
            b: cp * sy,
            c: sr * sp * cy - cr * sy,
            d: sr * sp * sy + cr * cy,
            tx: -(cr * sp * cy + sr * sy),
            ty: cy * sr - cr * sp * sy,
        }
    }
}

impl UvMotion {
    /// This motion at `time` seconds as one map, in the order the scene shader applies it:
    /// the waves' stretch about the centre, the scale, the turn, then the offset, scroll
    /// and the waves' offset.
    pub fn affine(&self, time: f64) -> Affine {
        let (cx, cy) = (f64::from(self.center[0]), f64::from(self.center[1]));
        let (wu, wv) = (self.waves[0].sample(time), self.waves[1].sample(time));
        let turn = match &self.rotator {
            Some(r) => r.affine(time),
            None => {
                let angle = f64::from(self.rotation_degrees).to_radians()
                    + time * f64::from(self.rotation_rate_degrees).to_radians();
                Affine {
                    a: angle.cos(),
                    b: angle.sin(),
                    c: -angle.sin(),
                    d: angle.cos(),
                    tx: 0.0,
                    ty: 0.0,
                }
            }
        };
        Affine::translate(-cx, -cy)
            .then(Affine::scale(wu.1, wv.1))
            .then(Affine::translate(cx, cy))
            .then(Affine::scale(
                f64::from(self.scale[0]),
                f64::from(self.scale[1]),
            ))
            .then(Affine::translate(-cx, -cy))
            .then(turn)
            .then(Affine::translate(
                cx + f64::from(self.offset[0]) + time * f64::from(self.scroll[0]) + wu.0,
                cy + f64::from(self.offset[1]) + time * f64::from(self.scroll[1]) + wv.0,
            ))
    }
}

/// A chain of motions, outermost modifier first, multiplied out at `time`.
pub fn chain_affine(chain: &[UvMotion], time: f64) -> Affine {
    chain
        .iter()
        .fold(Affine::IDENTITY, |acc, m| acc.then(m.affine(time)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: (f64, f64), b: (f64, f64)) {
        assert!(
            (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6,
            "{a:?} vs {b:?}"
        );
    }

    #[test]
    fn a_default_motion_is_the_identity() {
        near(UvMotion::default().affine(3.7).apply(0.3, 0.8), (0.3, 0.8));
    }

    #[test]
    fn rotation_about_the_centre_matches_the_shader_probe() {
        // The animated-uv render probe: 90 degrees about (0.5, 0.5) takes (0.25, 0.25) to (0.75, 0.25).
        let m = UvMotion {
            center: [0.5; 2],
            rotation_degrees: 90.0,
            ..Default::default()
        };
        near(m.affine(0.0).apply(0.25, 0.25), (0.75, 0.25));
        // A yaw rotator does the same.
        let r = UvMotion {
            center: [0.5; 2],
            rotator: Some(UvRotator {
                base_degrees: [0.0, 90.0, 0.0],
                ..Default::default()
            }),
            ..Default::default()
        };
        near(r.affine(0.0).apply(0.25, 0.25), (0.75, 0.25));
    }

    #[test]
    fn waves_follow_the_engine_formulas() {
        let pan = UvWave {
            kind: UvWaveKind::Pan,
            rate: 1.0,
            phase: 0.0,
            amplitude: 0.5,
        };
        let m = UvMotion {
            waves: [pan, UvWave::default()],
            ..Default::default()
        };
        // A quarter cycle in, the pan has moved u by the full amplitude.
        near(m.affine(0.25).apply(0.25, 0.25), (0.75, 0.25));
        let stretch = UvWave {
            kind: UvWaveKind::Stretch,
            rate: 1.0,
            phase: 0.0,
            amplitude: 2.0,
        };
        let m = UvMotion {
            waves: [stretch, UvWave::default()],
            ..Default::default()
        };
        near(m.affine(0.25).apply(0.25, 0.25), (0.75, 0.25));
    }

    #[test]
    fn a_chain_multiplies_outermost_first() {
        // A scaler (by 0.5, the texture-scaler (2, 2)) outside a panner: the pan is not scaled.
        let scaler = UvMotion {
            scale: [0.5, 0.5],
            ..Default::default()
        };
        let panner = UvMotion {
            offset: [0.1, 0.2],
            ..Default::default()
        };
        near(
            chain_affine(&[scaler, panner], 0.0).apply(1.0, 1.0),
            (0.5 + 0.1, 0.5 + 0.2),
        );
        // Reversed, the pan is scaled too.
        near(
            chain_affine(&[panner, scaler], 0.0).apply(1.0, 1.0),
            (0.5 * 1.1, 0.5 * 1.2),
        );
        assert_eq!(chain_affine(&[], 1.0), Affine::IDENTITY);
    }

    #[test]
    fn a_rotator_with_pitch_and_roll_carries_the_matrix_translation() {
        let r = UvRotator {
            base_degrees: [30.0, 0.0, 45.0],
            ..Default::default()
        };
        let m = r.affine(0.0);
        let (sp, cp) = (30f64.to_radians().sin(), 30f64.to_radians().cos());
        let (sr, cr) = (45f64.to_radians().sin(), 45f64.to_radians().cos());
        assert!((m.tx + cr * sp).abs() < 1e-12);
        assert!((m.ty - sr).abs() < 1e-12);
        assert!((m.a - cp).abs() < 1e-12 && (m.d - cr).abs() < 1e-12);
    }
}
