//! Coordinate conventions, distance attenuation, gain curve and panning.
//!
//! Coordinates are native world coordinates: right-handed, **Y up**, in original
//! Torque world units (the same units as `reference_distance`/`max_distance`).
//! Converted content maps Torque `(x, y, z)` to native `(x, z, -y)`; distances
//! are unchanged by that rotation, so authored audio distances apply as-is.

/// Native world-space vector `[x, y(up), z]`.
pub type Vec3 = [f32; 3];

#[inline]
pub(crate) fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
#[inline]
pub(crate) fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
#[inline]
pub(crate) fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
#[inline]
pub(crate) fn length(a: Vec3) -> f32 {
    dot(a, a).sqrt()
}
pub(crate) fn finite3(a: Vec3) -> bool {
    a.iter().all(|v| v.is_finite())
}

/// Listener pose. `forward` and `up` need not be normalised but must not be
/// parallel; invalid poses are ignored by the mixer (previous pose is kept).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Listener {
    pub position: Vec3,
    pub forward: Vec3,
    pub up: Vec3,
}

impl Default for Listener {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            forward: [0.0, 0.0, -1.0],
            up: [0.0, 1.0, 0.0],
        }
    }
}

impl Listener {
    pub fn is_valid(&self) -> bool {
        finite3(self.position)
            && finite3(self.forward)
            && finite3(self.up)
            && length(cross(self.forward, self.up)) > 1e-6
    }

    /// Unit vector to the listener's right (`forward x up`).
    pub fn right(&self) -> Vec3 {
        let r = cross(self.forward, self.up);
        let l = length(r);
        if l > 1e-12 {
            [r[0] / l, r[1] / l, r[2] / l]
        } else {
            [1.0, 0.0, 0.0]
        }
    }
}

/// Torque linear rolloff: 1 inside `reference`, 0 at/after `max`, linear between.
///
/// Evidence: TGE-family `alxUpdateMaxDistance`/`approximate3DVolume` with
/// `alDistanceModel(AL_NONE)` (engine-side attenuation), see
/// `docs/research/audio/runtime-model.md`.
pub fn torque_attenuation(distance: f32, reference: f32, max: f32) -> f32 {
    if !distance.is_finite() {
        return 0.0;
    }
    if max <= reference {
        return if distance <= reference { 1.0 } else { 0.0 };
    }
    let d = distance.clamp(reference, max);
    1.0 - (d - reference) / (max - reference)
}

/// TGE `linearToDB` lookup table (128 entries): maps the product of volume,
/// channel, master and attenuation to the gain handed to OpenAL.
pub const TORQUE_GAIN_TABLE: [f32; 128] = [
    0.00, 0.001, 0.002, 0.003, 0.004, 0.005, 0.01, 0.011, 0.012, 0.013, 0.014, 0.015, 0.016, 0.02,
    0.021, 0.022, 0.023, 0.024, 0.025, 0.03, 0.031, 0.032, 0.033, 0.034, 0.04, 0.041, 0.042, 0.043,
    0.044, 0.05, 0.051, 0.052, 0.053, 0.054, 0.06, 0.061, 0.062, 0.063, 0.064, 0.07, 0.071, 0.072,
    0.073, 0.08, 0.081, 0.082, 0.083, 0.084, 0.09, 0.091, 0.092, 0.093, 0.094, 0.10, 0.101, 0.102,
    0.103, 0.11, 0.111, 0.112, 0.113, 0.12, 0.121, 0.122, 0.123, 0.124, 0.13, 0.131, 0.132, 0.14,
    0.141, 0.142, 0.143, 0.15, 0.151, 0.152, 0.16, 0.161, 0.162, 0.17, 0.171, 0.172, 0.18, 0.181,
    0.19, 0.191, 0.192, 0.20, 0.201, 0.21, 0.211, 0.22, 0.221, 0.23, 0.231, 0.24, 0.25, 0.251,
    0.26, 0.27, 0.271, 0.28, 0.29, 0.30, 0.301, 0.31, 0.32, 0.33, 0.34, 0.35, 0.36, 0.37, 0.38,
    0.39, 0.40, 0.41, 0.43, 0.50, 0.60, 0.65, 0.70, 0.75, 0.80, 0.85, 0.90, 0.95, 0.97, 0.99,
];

/// How the combined linear gain becomes output amplitude.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GainCurve {
    /// TGE lookup table, linearly interpolated between entries so moving
    /// sources do not step audibly. Default: best available engine evidence.
    #[default]
    TorqueTable,
    /// Plain linear amplitude (modern behaviour; louder at partial gains).
    Linear,
}

impl GainCurve {
    pub fn apply(self, linear: f32) -> f32 {
        if linear.is_nan() || linear <= 0.0 {
            return 0.0;
        }
        if linear >= 1.0 {
            return 1.0;
        }
        match self {
            GainCurve::Linear => linear,
            GainCurve::TorqueTable => {
                let n = TORQUE_GAIN_TABLE.len();
                let x = linear * n as f32;
                let i = (x as usize).min(n - 1);
                let a = TORQUE_GAIN_TABLE[i];
                let b = if i + 1 < n {
                    TORQUE_GAIN_TABLE[i + 1]
                } else {
                    1.0
                };
                a + (b - a) * (x - i as f32)
            }
        }
    }
}

/// Equal-power stereo gains for a mono source at `position`.
/// Returns `(left, right)`; centred sources get `(√½, √½)`.
pub fn pan_gains(listener: &Listener, position: Vec3) -> (f32, f32) {
    let rel = sub(position, listener.position);
    let dist = length(rel);
    let pan = if dist > 1e-4 {
        (dot(rel, listener.right()) / dist).clamp(-1.0, 1.0)
    } else {
        0.0
    };
    let angle = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
    (angle.cos(), angle.sin())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attenuation_matches_torque_linear_model() {
        assert_eq!(torque_attenuation(0.0, 10.0, 60.0), 1.0);
        assert_eq!(torque_attenuation(10.0, 10.0, 60.0), 1.0);
        assert!((torque_attenuation(35.0, 10.0, 60.0) - 0.5).abs() < 1e-6);
        assert_eq!(torque_attenuation(60.0, 10.0, 60.0), 0.0);
        assert_eq!(torque_attenuation(1000.0, 10.0, 60.0), 0.0);
        assert_eq!(torque_attenuation(f32::NAN, 10.0, 60.0), 0.0);
        assert_eq!(torque_attenuation(5.0, 10.0, 10.0), 1.0);
        assert_eq!(torque_attenuation(11.0, 10.0, 10.0), 0.0);
    }

    #[test]
    fn gain_curve_is_monotonic_and_bounded() {
        let mut prev = 0.0;
        for i in 0..=1000 {
            let g = GainCurve::TorqueTable.apply(i as f32 / 1000.0);
            assert!((0.0..=1.0).contains(&g));
            assert!(g + 1e-6 >= prev, "not monotonic at {i}");
            prev = g;
        }
        assert_eq!(GainCurve::TorqueTable.apply(1.0), 1.0);
        assert_eq!(GainCurve::TorqueTable.apply(0.0), 0.0);
        assert_eq!(GainCurve::Linear.apply(0.25), 0.25);
        assert_eq!(GainCurve::TorqueTable.apply(f32::NAN), 0.0);
    }

    #[test]
    fn panning_follows_listener_right_in_y_up_space() {
        let l = Listener::default(); // facing -Z, up +Y => right is +X
        assert_eq!(l.right(), [1.0, 0.0, 0.0]);
        let (left, right) = pan_gains(&l, [5.0, 0.0, 0.0]);
        assert!(right > 0.99 && left < 0.01);
        let (left, right) = pan_gains(&l, [-5.0, 0.0, 0.0]);
        assert!(left > 0.99 && right < 0.01);
        let (left, right) = pan_gains(&l, [0.0, 0.0, -5.0]);
        assert!((left - right).abs() < 1e-6);
        let turned = Listener {
            forward: [1.0, 0.0, 0.0],
            ..l
        }; // facing +X => right is +Z
        let (left, right) = pan_gains(&turned, [0.0, 0.0, 5.0]);
        assert!(right > 0.99 && left < 0.01);
    }
}
