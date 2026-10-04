//! Coordinate conventions, distance attenuation, gain curve and panning.
//!
//! Coordinates are native world coordinates: right-handed, **Y up**, in original
//! Torque world units (the same units as `reference_distance`/`max_distance`).
//! Converted content maps Torque `(x, y, z)` to native `(x, z, -y)`; distances
//! are unchanged by that rotation, so authored audio distances apply as-is.

use bri_console::Clamp;

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
    let d = distance.clamped(reference, max);
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
        (dot(rel, listener.right()) / dist).clamped(-1.0, 1.0)
    } else {
        0.0
    };
    let angle = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
    (angle.cos(), angle.sin())
}

/// Most windows ([`Window`]) the listener hears through at once.
pub const MAX_WINDOWS: usize = 16;

/// A window the listener also hears through: an opening that shows (and
/// carries bodies, shots and sight to) somewhere else, a portal. A source
/// beyond it is heard from `ear`, the listener as the window carries it to
/// the far side, when the straight way from `ear` to the source goes out
/// through the window's far face; it is heard by whichever way, direct or
/// through a window, is shortest. So a crash seen through a portal sounds
/// as near and from where it shows, not from wherever it is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Window {
    /// The listener carried to the far side.
    pub ear: Listener,
    /// The window's far face: its middle, unit normal pointing out into the
    /// far side (where the sources it lets through stand), unit in-plane
    /// axes and the half size along each.
    pub centre: Vec3,
    pub normal: Vec3,
    pub u: Vec3,
    pub v: Vec3,
    pub half: [f32; 2],
}

impl Window {
    pub fn is_valid(&self) -> bool {
        self.ear.is_valid()
            && [self.centre, self.normal, self.u, self.v]
                .iter()
                .all(|v| finite3(*v))
            && self.half.iter().all(|h| h.is_finite() && *h >= 0.0)
    }
    /// Whether the straight way from the ear to `position` goes out through
    /// the far face.
    fn lets_through(&self, position: Vec3) -> bool {
        let side = |p: Vec3| dot(self.normal, sub(p, self.centre));
        let (a, b) = (side(self.ear.position), side(position));
        if !(a <= 0.0 && b > 0.0) {
            return false;
        }
        let t = -a / (b - a);
        let rel = sub(position, self.ear.position);
        let at = [
            self.ear.position[0] + rel[0] * t,
            self.ear.position[1] + rel[1] * t,
            self.ear.position[2] + rel[2] * t,
        ];
        let d = sub(at, self.centre);
        dot(d, self.u).abs() <= self.half[0] && dot(d, self.v).abs() <= self.half[1]
    }
}

/// The ear `position` is heard by, of the listener and its windows: the
/// nearest that hears it, and how far it is from that ear.
pub fn heard<'a>(
    listener: &'a Listener,
    windows: &'a [Window],
    position: Vec3,
) -> (&'a Listener, f32) {
    let direct = (listener, length(sub(position, listener.position)));
    windows
        .iter()
        .filter(|w| w.lets_through(position))
        .map(|w| (&w.ear, length(sub(position, w.ear.position))))
        .fold(direct, |best, way| if way.1 < best.1 { way } else { best })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_source_beyond_a_window_is_heard_as_near_as_it_shows() {
        // A window in the plane z = -2 in front of the listener leads 100
        // along x, facing the same way; the far face is at x = 100.
        let listener = Listener::default();
        let ear = Listener {
            position: [100.0, 0.0, 0.0],
            ..listener
        };
        let window = Window {
            ear,
            centre: [100.0, 0.0, -2.0],
            normal: [0.0, 0.0, -1.0],
            u: [1.0, 0.0, 0.0],
            v: [0.0, 1.0, 0.0],
            half: [1.0, 1.5],
        };
        assert!(window.is_valid());
        let windows = [window];
        // Five past the far face, straight ahead of the ear.
        let (from, d) = heard(&listener, &windows, [100.0, 0.0, -5.0]);
        assert_eq!(from.position, ear.position);
        assert!((d - 5.0).abs() < 1e-5);
        // Off to the side of the far face: only the long way.
        let (from, d) = heard(&listener, &windows, [110.0, 0.0, -5.0]);
        assert_eq!(from.position, listener.position);
        assert!(d > 100.0);
        // Behind the far face (the ear's side): direct.
        let (from, _) = heard(&listener, &windows, [100.0, 0.0, 3.0]);
        assert_eq!(from.position, listener.position);
    }

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
