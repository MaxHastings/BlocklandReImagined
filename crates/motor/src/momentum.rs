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
