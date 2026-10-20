//! [`Momentum`]: a body that keeps its speed. v20's players are steered to a target velocity, so releasing the keys stops them
//! at once; a body with momentum is pushed by what it is told to do, bleeds speed to the floor's friction, and steers weakly in
//! the air.
use anyhow::{Result, ensure};
use glam::Vec3;
use serde::{Deserialize, Serialize};

/// The longest stretch braking works in at a time, seconds. A longer step brakes in several of these, and the speed it keeps is
/// the mean over them, so the stop does not depend on how the caller splits time.
const BRAKE_STEP: f32 = 0.03;

/// [`PlayerTuning::momentum`](crate::player::PlayerTuning::momentum).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Momentum {
    /// Speed a held move adds each second on the ground (the most it adds; the move's own speed still caps the result).
    pub acceleration: f32,
    /// How fast the floor takes speed off, per second: with nothing held a body loses `2 x friction` of its speed each second,
    /// and a held move turns the velocity toward it at `friction` per second.
    pub friction: f32,
    /// Braking stops a body dead under this speed.
    pub stop_speed: f32,
    /// The share of `acceleration` a held move adds while airborne, 0 to 1. A body already at its move's speed cannot gain more.
    pub air_control: f32,
}
impl Momentum {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [self.acceleration, self.friction, self.stop_speed]
                .iter()
                .all(|n| n.is_finite() && (0.0..=1000.0).contains(n))
                && self.air_control.is_finite()
                && (0.0..=1.0).contains(&self.air_control),
            "Invalid momentum: acceleration, friction and stop speed 0 to 1000, air control 0 to 1"
        );
        Ok(())
    }

    /// `velocity` after `dt` seconds on the floor with nothing held: it loses `2 x friction` of itself a second, and stops dead
    /// once it has fallen under `stop_speed` or turned round.
    pub fn brake(&self, velocity: Vec3, dt: f32) -> Vec3 {
        let mut v = velocity;
        let mut mean = Vec3::ZERO;
        let mut left = dt;
        while left > 0.0 {
            let step = left.min(BRAKE_STEP);
            v -= v * (2.0 * step * self.friction);
            if v.dot(velocity) > 0.0 {
                mean += v * (step / dt);
            }
            left -= step;
        }
        if mean.dot(velocity) < 0.0 || mean.length() < self.stop_speed {
            Vec3::ZERO
        } else {
            mean
        }
    }

    /// `velocity` after `dt` seconds on the floor with a move held: the velocity turns toward `wish`'s direction, the move adds
    /// `acceleration` along it, and the speed is capped at the move's own (`wish`'s length).
    pub fn drive(&self, velocity: Vec3, wish: Vec3, dt: f32) -> Vec3 {
        let toward = wish.normalize_or_zero();
        let mut v =
            velocity - (velocity - toward * velocity.length()) * (dt * self.friction).min(1.0);
        v += toward * (self.acceleration * dt);
        v.clamp_length_max(wish.length())
    }

    /// The push a held move gives a body in the air whose horizontal velocity is `velocity`: at most
    /// `air_control x acceleration` along `wish`, topped up to reach a crawl from rest, and none that would make a body already
    /// at the move's speed (`wish`'s length) go faster.
    pub fn air_push(&self, velocity: Vec3, wish: Vec3, dt: f32) -> Vec3 {
        let toward = wish.normalize_or_zero();
        if toward == Vec3::ZERO || self.air_control == 0.0 {
            return Vec3::ZERO;
        }
        let speed = velocity.length();
        let mut limit = self.air_control * self.acceleration;
        if speed < self.stop_speed {
            limit += (self.stop_speed - speed) / dt;
        }
        let push = toward * (limit * dt);
        let reaches = velocity + push;
        if speed >= wish.length() && reaches.length() > speed {
            return reaches.clamp_length_max(speed) - velocity;
        }
        push
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AA: Momentum = Momentum {
        acceleration: 81.92,
        friction: 8.0,
        stop_speed: 0.4,
        air_control: 0.35,
    };
    const DT: f32 = 1.0 / 120.0;

    fn run(mut v: Vec3, wish: Vec3, seconds: f32) -> Vec3 {
        for _ in 0..(seconds / DT) as usize {
            v = if wish == Vec3::ZERO {
                AA.brake(v, DT)
            } else {
                AA.drive(v, wish, DT)
            };
        }
        v
    }

    #[test]
    fn a_held_move_builds_to_its_speed_and_no_more() {
        let wish = Vec3::new(0.0, 0.0, 8.8);
        let v = run(Vec3::ZERO, wish, 1.0);
        assert!((v.length() - 8.8).abs() < 1e-3, "{v}");
        let after = AA.drive(v, wish, DT);
        assert!(after.length() <= 8.8 + 1e-4);
    }

    #[test]
    fn it_does_not_start_at_full_speed() {
        let v = AA.drive(Vec3::ZERO, Vec3::new(0.0, 0.0, 8.8), DT);
        assert!(v.length() < 1.0);
    }

    #[test]
    fn releasing_the_move_slides_to_a_stop() {
        let moving = Vec3::new(0.0, 0.0, 8.8);
        let soon = run(moving, Vec3::ZERO, 0.1);
        assert!(soon.length() > 0.0 && soon.length() < 8.8, "{soon}");
        assert_eq!(run(moving, Vec3::ZERO, 1.0), Vec3::ZERO);
    }

    #[test]
    fn braking_never_speeds_a_body_up_however_the_time_is_cut() {
        let moving = Vec3::new(0.0, 0.0, 8.8);
        for dt in [0.004, 0.0083, 0.03, 0.1, 0.4] {
            assert!(AA.brake(moving, dt).length() < moving.length(), "{dt}");
        }
    }

    #[test]
    fn a_turn_swings_the_velocity_toward_the_new_move() {
        let wish = Vec3::new(8.8, 0.0, 0.0);
        let v = run(Vec3::new(0.0, 0.0, 8.8), wish, 0.05);
        assert!(v.x > 0.0 && v.z < 8.8, "{v}");
    }

    #[test]
    fn the_air_pushes_less_than_the_ground_and_never_past_the_move_speed() {
        let wish = Vec3::new(0.0, 0.0, 8.8);
        assert!(AA.air_push(Vec3::new(0.0, 0.0, 4.0), wish, DT).length() < 81.92 * DT);
        let fast = Vec3::new(0.0, 0.0, 9.0);
        let after = fast + AA.air_push(fast, wish, DT);
        assert!(after.length() <= 9.0 + 1e-4);
        let slow = AA.air_push(Vec3::ZERO, wish, DT);
        assert!(slow.z > 0.0);
    }

    #[test]
    fn validation_rejects_a_share_over_one() {
        assert!(AA.validate().is_ok());
        assert!(
            Momentum {
                air_control: 1.5,
                ..AA
            }
            .validate()
            .is_err()
        );
    }
}
