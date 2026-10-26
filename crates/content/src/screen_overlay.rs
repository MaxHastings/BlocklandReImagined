//! A full-screen overlay driven by a curve of seconds (rules' `screen_overlay`):
//! a signed value over time that tints the screen with the `light` colour
//! while above zero and the `dark` colour while below, its size the alpha.
//! Overlays running together add their values before the colour is chosen.
use bri_console::Clamp;
use serde::{Deserialize, Serialize};

/// Overlays one player has running at once; a new one past this replaces the
/// one with the least time left.
pub const MAX_OVERLAYS: usize = 16;
/// Pieces in one curve.
pub const MAX_SEGMENTS: usize = 8;
/// How long one overlay runs.
pub const SECONDS: std::ops::RangeInclusive<f32> = 0.05..=120.0;
/// How large a curve value or peak may be (a peak above 1 is saturated
/// colour for longer, as the curve falls through 1).
pub const MAGNITUDE: f32 = 100.0;

/// How a piece moves from its `start` to its `end`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ease {
    /// A straight line.
    #[default]
    Linear,
    /// Half a cosine: slow at both ends, `start + (end - start) * (1 - cos(pi u)) / 2`.
    Cosine,
}

/// One piece of a curve: from `from` seconds in until `to` seconds in
/// (omitted: until the overlay ends), the value runs from `start` to `end`
/// times the overlay's `peak`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Segment {
    pub from: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<f32>,
    pub start: f32,
    pub end: f32,
    #[serde(default)]
    pub ease: Ease,
}

/// A screen overlay: `seconds` long, `peak` times the curve.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Overlay {
    pub seconds: f32,
    pub peak: f32,
    /// RGB 0..1 drawn while the value is above zero.
    pub light: [f32; 3],
    /// RGB 0..1 drawn while the value is below zero.
    pub dark: [f32; 3],
    /// The pieces, in time order; a time none holds is zero.
    pub curve: Vec<Segment>,
}

impl Overlay {
    /// Whether every number is usable.
    pub fn bounded(&self) -> bool {
        let colour = |c: &[f32; 3]| c.iter().all(|v| (0.0..=1.0).contains(v));
        let size = |v: f32| v.is_finite() && v.abs() <= MAGNITUDE;
        SECONDS.contains(&self.seconds)
            && size(self.peak)
            && colour(&self.light)
            && colour(&self.dark)
            && (1..=MAX_SEGMENTS).contains(&self.curve.len())
            && self.curve.iter().all(|s| {
                let to = s.to.unwrap_or(self.seconds);
                s.from.is_finite()
                    && to.is_finite()
                    && s.from >= 0.0
                    && s.from < to
                    && to <= self.seconds
                    && size(s.start)
                    && size(s.end)
            })
    }

    /// The overlay's value `t` seconds in: the first piece holding `t`
    /// (a piece holds `from` up to `to`, the last also `to`), its ease from
    /// `start` to `end`, times `peak`.
    pub fn value(&self, t: f32) -> f32 {
        for s in &self.curve {
            let to = s.to.unwrap_or(self.seconds);
            if t >= s.from && (t < to || (t == to && s.to.is_none())) {
                let u = ((t - s.from) / (to - s.from)).clamped(0.0, 1.0);
                let u = match s.ease {
                    Ease::Linear => u,
                    Ease::Cosine => (1.0 - (std::f32::consts::PI * u).cos()) / 2.0,
                };
                return self.peak * (s.start + (s.end - s.start) * u);
            }
        }
        0.0
    }
}

/// The colour (RGBA bytes) a summed value draws: `light` above zero, `dark`
/// below, alpha the size capped at one, cut to a whole byte; none when that
/// byte is zero.
pub fn colour(sum: f32, light: [f32; 3], dark: [f32; 3]) -> Option<[u8; 4]> {
    let alpha = (sum.abs().min(1.0) * 255.0) as u8;
    let rgb = if sum > 0.0 { light } else { dark };
    (alpha > 0).then(|| {
        let byte = |v: f32| (v.clamped(0.0, 1.0) * 255.0).round() as u8;
        [byte(rgb[0]), byte(rgb[1]), byte(rgb[2]), alpha]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A curve as a rule would author it: a rise to the peak, a drop to
    /// dark, then a cosine fade out.
    fn flash(peak: f32, seconds: f32) -> Overlay {
        let seg = |from, to, start, end, ease| Segment {
            from,
            to,
            start,
            end,
            ease,
        };
        Overlay {
            seconds,
            peak,
            light: [1.0; 3],
            dark: [0.0; 3],
            curve: vec![
                seg(0.0, Some(0.15), 0.0, 1.0, Ease::Linear),
                seg(0.15, Some(0.3), 0.0, -1.0, Ease::Linear),
                seg(0.3, None, -1.0, 0.0, Ease::Cosine),
            ],
        }
    }

    #[test]
    fn the_curve_runs_light_then_dark_then_fades() {
        let o = flash(3.0, 12.0);
        assert!(o.bounded());
        assert!((o.value(0.075) - 1.5).abs() < 1e-4);
        assert!((o.value(0.149) - 3.0 * (0.149 / 0.15)).abs() < 1e-3);
        // Light is gone at 0.15 and the dark ramp starts from nothing.
        assert!(o.value(0.15).abs() < 1e-4);
        assert!((o.value(0.225) + 1.5).abs() < 1e-4);
        assert!((o.value(0.3) + 3.0).abs() < 1e-4);
        // Halfway through the fade: half the dark, by the cosine.
        let mid = 0.3 + (12.0 - 0.3) / 2.0;
        assert!((o.value(mid) + 1.5).abs() < 1e-3);
        assert!(o.value(12.0).abs() < 1e-4);
        assert_eq!(o.value(13.0), 0.0);
    }

    #[test]
    fn the_sum_picks_a_colour_and_a_whole_byte_of_alpha() {
        let (l, d) = ([1.0; 3], [0.0; 3]);
        assert_eq!(colour(0.5, l, d), Some([255, 255, 255, 127]));
        assert_eq!(colour(-2.0, l, d), Some([0, 0, 0, 255]));
        assert_eq!(colour(0.003, l, d), None);
        assert_eq!(colour(0.0, l, d), None);
    }

    #[test]
    fn unusable_overlays_are_refused() {
        let mut o = flash(1.0, 12.0);
        o.curve[0].to = Some(20.0);
        assert!(!o.bounded());
        let mut o = flash(1.0, 12.0);
        o.light = [2.0, 0.0, 0.0];
        assert!(!o.bounded());
        let mut o = flash(f32::NAN, 12.0);
        assert!(!o.bounded());
        o = flash(1.0, 12.0);
        o.curve.clear();
        assert!(!o.bounded());
    }
}
