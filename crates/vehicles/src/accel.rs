//! A seat's felt acceleration, as Halo's units pose their bodies with it.
//!
//! Every Halo tick a unit takes how much its parent's origin changed its
//! velocity, along the seat's forward, left and up axes (`unit_seat_update`),
//! scales it by the seat's `acceleration_scale`, offsets it to the middle of
//! 0..1 and smooths it by 30% a tick. Each axis then picks a place along one
//! authored overlay clip whose middle key is the unaccelerated pose
//! (`overlay_animation_apply_continuous` at `(frames - 1) * value`). This
//! module is that filter; the clips and scales are a package's policy
//! ([`AccelerationOverlays`]).
use bri_console::Clamp;
use crate::schema::AccelerationOverlays;
use glam::Vec3;

/// Halo ticks a second: the rate the filter runs at.
pub const TICK_RATE: f64 = 30.;
/// Longest gap, in ticks, one call replays before taking the body as having
/// jumped: a stalled frame must not read as a violent push.
const MAX_REPLAY: i64 = 6;
/// Weight a tick's reading takes against the smoothed value.
const SMOOTHING: f32 = 0.3;

/// One seat's smoothed acceleration, fed the body's pose each frame.
#[derive(Clone, Debug, Default)]
pub struct SeatAcceleration {
    sampled: Option<Sampled>,
    /// Where the body was at the last whole tick, and how far it moved over
    /// that tick, native units.
    tick_position: Vec3,
    step: Vec3,
    /// Smoothed readings, 0..1 with 0.5 for none: front/back, left/right,
    /// up/down.
    value: [f32; 3],
}
#[derive(Clone, Copy, Debug)]
struct Sampled {
    clock: f64,
    position: Vec3,
}
impl SeatAcceleration {
    /// The smoothed readings: where each overlay clip is sampled, as a
    /// fraction of its length (0.5, the middle key, when unaccelerated).
    pub fn value(&self) -> [f32; 3] {
        if self.sampled.is_some() {
            self.value
        } else {
            [0.5; 3]
        }
    }
    /// Advance to `clock` seconds with the body's origin at `position`
    /// (native units) and its `forward` and `up` axes in the world.
    pub fn step(
        &mut self,
        clock: f64,
        position: Vec3,
        forward: Vec3,
        up: Vec3,
        overlays: &AccelerationOverlays,
    ) -> [f32; 3] {
        let Some(then) = self
            .sampled
            .filter(|s| clock >= s.clock && position.is_finite())
        else {
            *self = Self {
                sampled: (clock.is_finite() && position.is_finite())
                    .then_some(Sampled { clock, position }),
                value: [0.5; 3],
                tick_position: position,
                step: Vec3::ZERO,
            };
            return self.value();
        };
        let (from, to) = (
            (then.clock * TICK_RATE).floor() as i64,
            (clock * TICK_RATE).floor() as i64,
        );
        if to - from > MAX_REPLAY {
            self.step = Vec3::ZERO;
            self.tick_position = position;
            self.value = [0.5; 3];
        } else if to > from {
            let left = up.cross(forward);
            let span = clock - then.clock;
            for tick in from + 1..=to {
                let along = (((tick as f64) / TICK_RATE - then.clock) / span).clamped(0., 1.);
                let reached = then.position.lerp(position, along as f32);
                let step = reached - self.tick_position;
                let change = (step - self.step) / overlays.native_per_unit;
                let reading = [
                    change.dot(forward) * overlays.scale[0],
                    change.dot(left) * overlays.scale[1],
                    change.dot(up) * overlays.scale[2],
                ];
                for (value, reading) in self.value.iter_mut().zip(reading) {
                    let reading = (reading + 0.5).clamped(0., 1.);
                    *value = reading * SMOOTHING + *value * (1. - SMOOTHING);
                }
                (self.tick_position, self.step) = (reached, step);
            }
        }
        self.sampled = Some(Sampled { clock, position });
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn halo(scale: [f32; 3]) -> AccelerationOverlays {
        AccelerationOverlays {
            clips: ["a".into(), "b".into(), "c".into()],
            scale,
            native_per_unit: 4.12,
        }
    }
    /// Drive a body along `path(seconds)` at 60 frames a second.
    fn drive(
        overlays: &AccelerationOverlays,
        seconds: f64,
        path: impl Fn(f64) -> Vec3,
    ) -> [f32; 3] {
        let mut seat = SeatAcceleration::default();
        let mut value = [0.5; 3];
        let mut t = 0.;
        while t <= seconds {
            value = seat.step(t, path(t), Vec3::NEG_Z, Vec3::Y, overlays);
            t += 1. / 60.;
        }
        value
    }
    /// The Warthog's `speed_acceleration`, 0.0028 of a Halo world unit a
    /// tick each tick, as native units a second a second.
    const WARTHOG_PUSH: f32 = 0.0028 * 4.12 * 30. * 30.;
    fn pushed(t: f64) -> f32 {
        0.5 * WARTHOG_PUSH * (t * t) as f32
    }
    #[test]
    fn a_body_at_rest_or_cruising_reads_neutral() {
        let o = halo([200., 200., 150.]);
        let rest = drive(&o, 2., |_| Vec3::new(1., 2., 3.));
        assert!(rest.iter().all(|v| (v - 0.5).abs() < 1e-6), "{rest:?}");
        let cruising = drive(&o, 2., |t| Vec3::new(0., 0., -34. * t as f32));
        assert!(
            cruising.iter().all(|v| (v - 0.5).abs() < 0.02),
            "{cruising:?}"
        );
    }
    #[test]
    fn speeding_up_reads_as_the_original_scales_it() {
        let o = halo([200., 200., 150.]);
        let value = drive(&o, 0.5, |t| Vec3::new(0., 0., -pushed(t)));
        // 0.0028 * 200 = 0.56 above the middle, past the end: saturated.
        assert!(value[0] > 0.95, "{value:?}");
        assert!((value[1] - 0.5).abs() < 0.02 && (value[2] - 0.5).abs() < 0.02);
        // Braking is the mirror image.
        let braking = drive(&o, 0.5, |t| Vec3::new(0., 0., pushed(t)));
        assert!(braking[0] < 0.05, "{braking:?}");
    }
    #[test]
    fn a_seat_scales_what_the_vehicle_scales_differently() {
        let path = |t: f64| Vec3::new(0., 0., -pushed(t));
        let vehicle = drive(&halo([200., 200., 150.]), 0.5, path);
        let rider = drive(&halo([100., 100., 50.]), 0.5, path);
        // 0.28 from the middle for a rider: well short of the ends.
        assert!(
            vehicle[0] > rider[0] && rider[0] > 0.7 && rider[0] < 0.9,
            "{rider:?}"
        );
    }
    #[test]
    fn the_reading_eases_in_over_several_ticks() {
        let o = halo([200., 200., 150.]);
        let mut seat = SeatAcceleration::default();
        let a = 0.0028 * 4.12;
        let mut readings = vec![];
        for tick in 0..10 {
            // One sample a tick, a steady push from tick 2.
            let t = f64::from(tick) / TICK_RATE + 1e-6;
            let speeds = (tick - 2).max(0);
            let along = f64::from(speeds * (speeds + 1)) / 2.;
            seat.step(
                t,
                Vec3::new(0., 0., -(a * along as f32)),
                Vec3::NEG_Z,
                Vec3::Y,
                &o,
            );
            readings.push(seat.value()[0]);
        }
        assert!(readings[1] == 0.5, "{readings:?}");
        // 0.56 offset saturates the reading at 1; the filter takes 30% of
        // the way a tick: 0.65, 0.755, 0.83, ...
        assert!(readings[3] > 0.6 && readings[3] < 0.7, "{readings:?}");
        assert!(
            readings[9] > readings[3] && readings[9] < 1.,
            "{readings:?}"
        );
    }
    #[test]
    fn a_stalled_frame_is_not_a_shove() {
        let o = halo([200., 200., 150.]);
        let mut seat = SeatAcceleration::default();
        seat.step(0., Vec3::ZERO, Vec3::NEG_Z, Vec3::Y, &o);
        let v = seat.step(5., Vec3::new(0., 0., -300.), Vec3::NEG_Z, Vec3::Y, &o);
        assert!(v.iter().all(|x| (x - 0.5).abs() < 1e-6), "{v:?}");
    }
    #[test]
    fn left_is_up_cross_forward() {
        // Pushing the body along its left (-X for a body facing -Z) moves
        // only the left/right reading, upward.
        let o = halo([200., 200., 150.]);
        let value = drive(&o, 0.5, |t| Vec3::new(-pushed(t), 0., 0.));
        assert!(
            value[1] > 0.95 && (value[0] - 0.5).abs() < 0.02,
            "{value:?}"
        );
    }
}
