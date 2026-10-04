//! v20's crouch pose thread. Crouching does not snap the body down: the
//! original player keeps a dedicated thread on the `crouch` sequence
//! (blocklandv20.exe Player::updateMove, near 0x5ae8c1/0x5ae80c; ghosts
//! follow the same rules from the network crouch flag near 0x5b2435).
//!
//! - Starting to crouch, or crouching while the thread is not already
//!   playing forward, restarts the sequence at position 0 (standing) and
//!   plays it forward.
//! - Standing up only reverses the time scale, so the pose rises from
//!   wherever it currently is.
//! - Once a reversed thread reaches position 0 it is parked on the empty
//!   root sequence (Player::advanceTime near 0x5a6b10) and stops overriding
//!   locomotion.
//!
//! The first two rules are the "humping" quirk: re-pressing crouch while the
//! pose is still rising snaps it to standing and sinks it again, so tapping
//! crouch pumps the hips (and the first-person view) instead of blending.

use bri_console::Clamp;

/// Authored `crouch` Eye node heights (m.dts), one per keyframe. The eye
/// follows the thread, so the view dips and snaps with the body.
const EYE_KEYS: [f32; 4] = [2.156_496_5, 1.759_874_3, 1.023_290_5, 0.626_668_45];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CrouchThread {
    /// Seconds into the crouch sequence.
    position: f32,
    /// Thread time scale: 1 crouching, -1 standing up, 0 before first use.
    scale: f32,
    /// False once parked on the root sequence.
    active: bool,
    started: bool,
}

impl CrouchThread {
    /// Applies the crouch flag, then advances by `elapsed` seconds of a
    /// sequence lasting `duration` seconds.
    pub fn update(&mut self, crouched: bool, elapsed: f32, duration: f32) {
        if !self.started {
            // A newly created thread starts at the end when already crouched.
            self.started = true;
            if crouched {
                *self = Self {
                    position: duration,
                    scale: 1.0,
                    active: true,
                    started: true,
                };
            }
            return;
        }
        if crouched && self.scale != 1.0 {
            self.position = 0.0;
            self.scale = 1.0;
            self.active = true;
        } else if !crouched && self.scale == 1.0 {
            self.scale = -1.0;
        }
        if self.active {
            self.position = (self.position + elapsed * self.scale).clamped(0.0, duration);
            if self.position == 0.0 && self.scale != 1.0 {
                self.active = false;
            }
        }
    }
    /// Sequence time to sample, or `None` while parked on root.
    pub fn time(&self) -> Option<f32> {
        self.active.then_some(self.position)
    }
    /// How far the eye has dropped from standing (0) to crouched (1).
    pub fn eye_fraction(&self, duration: f32) -> f32 {
        let Some(time) = self.time().filter(|_| duration > 0.0) else {
            return 0.0;
        };
        let frame = (time / duration).clamped(0.0, 1.0) * (EYE_KEYS.len() - 1) as f32;
        let a = (frame.floor() as usize).min(EYE_KEYS.len() - 2);
        let height = EYE_KEYS[a] + (EYE_KEYS[a + 1] - EYE_KEYS[a]) * (frame - a as f32);
        (EYE_KEYS[0] - height) / (EYE_KEYS[0] - EYE_KEYS[3])
    }
}

/// v20 `crouch` sequence length (four frames at 15 fps).
pub const CROUCH_SECONDS: f32 = 0.2;

#[cfg(test)]
mod tests {
    use super::*;
    const D: f32 = CROUCH_SECONDS;

    #[test]
    fn crouch_sinks_over_the_sequence_and_rises_from_where_it_is() {
        let mut t = CrouchThread::default();
        t.update(false, 0.0, D);
        assert_eq!(t.time(), None);
        t.update(true, 0.05, D);
        assert!((t.time().unwrap() - 0.05).abs() < 1e-6);
        t.update(true, 0.5, D);
        assert_eq!(t.time(), Some(D));
        assert_eq!(t.eye_fraction(D), 1.0);
        t.update(false, 0.05, D);
        assert!((t.time().unwrap() - 0.15).abs() < 1e-6);
        t.update(false, 0.5, D);
        assert_eq!(t.time(), None);
        assert_eq!(t.eye_fraction(D), 0.0);
    }

    #[test]
    fn recrouching_while_rising_snaps_to_standing_first() {
        let mut t = CrouchThread::default();
        t.update(false, 0.0, D);
        t.update(true, 1.0, D);
        t.update(false, 0.05, D);
        let rising = t.time().unwrap();
        assert!(rising > 0.1);
        // The humping snap: back to standing, then sinking again.
        t.update(true, 0.0, D);
        assert_eq!(t.time(), Some(0.0));
        assert_eq!(t.eye_fraction(D), 0.0);
        t.update(true, 0.02, D);
        assert!(t.time().unwrap() < rising);
    }

    #[test]
    fn a_player_first_seen_crouched_starts_fully_crouched() {
        let mut t = CrouchThread::default();
        t.update(true, 0.0, D);
        assert_eq!(t.time(), Some(D));
        t.update(true, 0.1, D);
        assert_eq!(t.time(), Some(D));
    }
}
