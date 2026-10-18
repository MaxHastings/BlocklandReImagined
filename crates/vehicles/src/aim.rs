//! A turret's barrel following its gunner's aim with a rate limit and lag.
//!
//! Halo's units do not point where they are told: `unit_update` moves the
//! unit's aiming vector toward the desired one with `aiming_velocity_maximum`
//! and `aiming_acceleration_maximum`, by planning for each axis the quickest
//! accelerate, coast and brake that arrives at rest on the target
//! (`unit_euler_aiming_update`). The planner and its coupling of the two axes
//! are ported here step for step; the engine owns the mechanism and a
//! definition's [`AimLimits`] choose the policy.
//!
//! The law runs on Halo's 30 Hz tick. [`AimFollow`] advances one 120 Hz
//! physics tick at a time, runs the law every fourth one and draws the aim
//! between the last two results, as the original draws its objects between
//! ticks.
//!
//! Simplifications against the source, which keeps each axis's speed as an
//! Euler pair rebuilt through a 3D rotation every tick: the speeds are kept
//! as the Euler pair itself, and the combined speed is held to the limit
//! afterwards by scaling both. They agree wherever the barrel is far from
//! straight up or down, which its pitch range guarantees.
use serde::{Deserialize, Serialize};

/// Physics ticks in one 30 Hz Halo tick.
pub const TICKS_PER_LAW_TICK: u8 = 4;
const _: () = assert!(
    (TICKS_PER_LAW_TICK as f32) * bri_physics::FIXED_DT > 0.0333
        && (TICKS_PER_LAW_TICK as f32) * bri_physics::FIXED_DT < 0.0334
);
/// Halo ticks a second.
const LAW_RATE: f32 = 30.;

/// How fast a turret's barrel may swing: Halo's `aiming_velocity_maximum`
/// (radians a second) and `aiming_acceleration_maximum` (radians a second
/// squared). Zero for both aims instantly, as a unit without them does.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AimLimits {
    pub max_velocity: f32,
    pub max_acceleration: f32,
}
impl AimLimits {
    pub fn valid(&self) -> bool {
        self.max_velocity.is_finite()
            && self.max_acceleration.is_finite()
            && (0.0..=100.).contains(&self.max_velocity)
            && (0.0..=10_000.).contains(&self.max_acceleration)
            // The source asserts a positive acceleration limit whenever the
            // follow runs: a speed limit alone has nothing to brake with.
            && (self.max_acceleration > 0. || self.max_velocity == 0.)
    }
}

/// One axis's plan to arrive at rest on its target (Halo's
/// `unit_acceleration_plan`): accelerate, coast, then brake.
#[derive(Clone, Copy, Debug, Default)]
struct Plan {
    at_rest: bool,
    initial_position: f32,
    initial_velocity: f32,
    acceleration: f32,
    acceleration_time: f32,
    coast_time: f32,
    deceleration: f32,
    deceleration_time: f32,
}

/// `unit_euler_axis_buildplan`: the plan for an axis that is `position` from
/// its target (the aim less the target) moving at `velocity`.
fn build_plan(position: f32, velocity: f32, max_velocity: f32, max_acceleration: f32) -> Plan {
    let mut plan = Plan {
        initial_position: position,
        initial_velocity: velocity,
        at_rest: position.abs() < 0.001 && velocity.abs() < 0.001,
        ..Plan::default()
    };
    if plan.at_rest {
        return plan;
    }
    let time = velocity.abs() / max_acceleration;
    let moving_positive = velocity > 0.;
    let half_time = time * 0.5;
    if half_time * velocity * 0.5 + position < 0. {
        // Past the target on the far side: plan the mirror image.
        let mut mirrored = build_plan(-position, -velocity, max_velocity, max_acceleration);
        mirrored.initial_position = -mirrored.initial_position;
        mirrored.initial_velocity = -mirrored.initial_velocity;
        mirrored.acceleration = -mirrored.acceleration;
        mirrored.deceleration = -mirrored.deceleration;
        return mirrored;
    }
    let stopping_position = velocity * 0.5 * time + position;
    if stopping_position < 0. {
        // Moving toward the target too fast to stop on it: brake only.
        plan.deceleration = velocity * velocity / (position + position);
        plan.deceleration_time = -(velocity / plan.deceleration);
        return plan;
    }
    let acceleration_time = if moving_positive {
        (stopping_position / max_acceleration).sqrt()
    } else {
        let negative = -max_acceleration;
        let doubled = velocity + velocity;
        let discriminant = (doubled * doubled - negative * stopping_position * 4.).max(0.);
        let root = discriminant.sqrt();
        let first = (-doubled - root) / (negative + negative);
        let second = (root - doubled) / (negative + negative);
        if first >= 0. && (second < 0. || first < second) {
            first
        } else if second < 0. {
            0.
        } else {
            second
        }
    };
    let first_root = if max_velocity > 0. {
        let to_limit = if moving_positive {
            max_velocity
        } else {
            max_velocity + velocity
        } / max_acceleration;
        acceleration_time.min(to_limit.max(0.))
    } else {
        acceleration_time
    };
    plan.acceleration = -max_acceleration;
    plan.deceleration = max_acceleration;
    if moving_positive {
        plan.acceleration_time = first_root + time;
        plan.deceleration_time = first_root;
    } else {
        plan.acceleration_time = first_root;
        plan.deceleration_time = first_root + time;
    }
    if first_root < acceleration_time {
        let coasting = plan.acceleration * plan.acceleration_time + velocity;
        let remaining = acceleration_time - first_root;
        let discriminant = remaining * coasting * 2. - remaining * remaining * max_acceleration;
        plan.coast_time = if coasting == 0. {
            0.
        } else {
            (discriminant / coasting).max(0.)
        };
    }
    plan
}

/// `unit_euler_axis_couple`: stretch the shorter axis's plan to take as long
/// as the longer so the two arrive together, rather than turning at a corner.
fn couple(first: &mut Plan, second: &mut Plan, max_acceleration: f32) {
    if first.at_rest || second.at_rest {
        return;
    }
    let first_total = first.deceleration_time + first.coast_time + first.acceleration_time;
    let second_total = second.deceleration_time + second.coast_time + second.acceleration_time;
    let (adjust, extension) = if first.acceleration_time > 0. && first_total < second_total {
        (first, second_total - first_total)
    } else if second.acceleration_time > 0. && second_total < first_total {
        (second, first_total - second_total)
    } else {
        return;
    };
    let factor = (extension + adjust.coast_time) * max_acceleration;
    let peak = adjust.acceleration_time * adjust.acceleration + adjust.initial_velocity;
    let discriminant = (factor * factor - (-extension * peak.abs()) * max_acceleration * 4.).max(0.);
    let adjustment = (discriminant.sqrt() - factor) / (max_acceleration + max_acceleration);
    let adjustment = adjustment.min(adjust.acceleration_time.min(adjust.deceleration_time));
    if adjustment <= 0. {
        return;
    }
    let acceleration_time = adjust.acceleration_time - adjustment;
    let peak = acceleration_time * adjust.acceleration + adjust.initial_velocity;
    adjust.acceleration_time = acceleration_time;
    adjust.deceleration_time -= adjustment;
    adjust.coast_time = if peak == 0. {
        0.
    } else {
        ((peak + peak + adjustment * adjust.acceleration) * adjustment) / peak
    };
}

/// `unit_euler_axis_doplan` over `delta` (one tick): the axis's position
/// and velocity after it, and whether it came to rest on the target.
fn do_plan(plan: &Plan, delta: f32, position: f32, velocity: f32) -> (bool, f32, f32) {
    if plan.at_rest {
        return (true, position, velocity);
    }
    let (mut position, mut velocity, mut remaining) = (position, velocity, delta);
    if remaining > 0. && plan.acceleration_time > 0. {
        let time = remaining.min(plan.acceleration_time);
        position += (time * plan.acceleration * 0.5 + velocity) * time;
        velocity += time * plan.acceleration;
        remaining -= time;
    }
    if remaining > 0. && plan.coast_time > 0. {
        let time = remaining.min(plan.coast_time);
        position += velocity * time;
        remaining -= time;
    }
    if remaining > 0. && plan.deceleration_time > 0. {
        let time = remaining.min(plan.deceleration_time);
        position += (0.5 * time * plan.deceleration + velocity) * time;
        velocity += time * plan.deceleration;
        remaining -= time;
    }
    (remaining > 0., position, velocity)
}

fn wrap(angle: f32) -> f32 {
    (angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}
/// The unit direction of a `[yaw, pitch]`, for the angle between two aims.
fn direction([yaw, pitch]: [f32; 2]) -> glam::Vec3 {
    glam::Vec3::new(pitch.cos() * yaw.sin(), pitch.sin(), pitch.cos() * yaw.cos())
}

/// A barrel's `[yaw, pitch]` (radians, relative to its hull, pitch up
/// positive) as it follows an aim: the state of one turret.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AimFollow {
    /// The aim after the previous and the latest 30 Hz step, and the speed
    /// (radians a step) the latest one left each axis at.
    pub before: [f32; 2],
    pub latest: [f32; 2],
    pub velocity: [f32; 2],
    /// Physics ticks into the current 30 Hz step.
    pub phase: u8,
}
impl AimFollow {
    /// A barrel at rest on `aim`.
    pub fn at(aim: [f32; 2]) -> Self {
        Self {
            before: aim,
            latest: aim,
            velocity: [0.; 2],
            phase: 0,
        }
    }
    /// The aim to draw and fire along.
    pub fn aim(&self) -> [f32; 2] {
        let t = f32::from(self.phase + 1) / f32::from(TICKS_PER_LAW_TICK);
        [
            self.before[0] + wrap(self.latest[0] - self.before[0]) * t,
            self.before[1] + (self.latest[1] - self.before[1]) * t,
        ]
    }
    /// Advance one physics tick toward `desired` between the pitches
    /// `[down, up]`, wrapping in yaw (a turntable with no stops).
    pub fn step(&mut self, desired: [f32; 2], limits: &AimLimits, pitch: [f32; 2]) {
        self.phase += 1;
        if self.phase < TICKS_PER_LAW_TICK {
            return;
        }
        self.phase = 0;
        self.before = self.latest;
        self.latest = self.law(desired, limits, pitch);
    }
    /// One Halo tick of `unit_euler_aiming_update`.
    fn law(&mut self, desired: [f32; 2], limits: &AimLimits, [down, up]: [f32; 2]) -> [f32; 2] {
        let target = [wrap(desired[0]), desired[1].clamp(down, up)];
        if limits.max_velocity == 0. && limits.max_acceleration == 0. {
            self.velocity = [0.; 2];
            return target;
        }
        let max_velocity = limits.max_velocity / LAW_RATE;
        let max_acceleration = limits.max_acceleration / (LAW_RATE * LAW_RATE);
        let aim = self.latest;
        let error = [wrap(aim[0] - target[0]), aim[1] - target[1]];
        let mut yaw = build_plan(error[0], self.velocity[0], max_velocity, max_acceleration);
        let mut tilt = build_plan(error[1], self.velocity[1], max_velocity, max_acceleration);
        couple(&mut yaw, &mut tilt, max_acceleration);
        let (yaw_done, yaw_error, yaw_velocity) = do_plan(&yaw, 1., error[0], self.velocity[0]);
        let (tilt_done, tilt_error, tilt_velocity) = do_plan(&tilt, 1., error[1], self.velocity[1]);
        if yaw_done && tilt_done {
            self.velocity = [0.; 2];
            return target;
        }
        let end = [
            wrap(yaw_error + target[0]),
            (tilt_error + target[1]).clamp(down, up),
        ];
        let mut velocity = [yaw_velocity, tilt_velocity];
        // The source scales the 3D speed to the limit: the angle this aim
        // would move through one more step.
        let further = [end[0] + velocity[0], end[1] + velocity[1]];
        let angle = direction(end).dot(direction(further)).clamp(-1., 1.).acos();
        if angle > max_velocity && angle > 0. {
            let scale = max_velocity / angle;
            velocity = velocity.map(|v| v * scale);
        }
        self.velocity = velocity;
        end
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// The Warthog's: 180 degrees a second, 600 degrees a second squared.
    const WARTHOG: AimLimits = AimLimits {
        max_velocity: std::f32::consts::PI,
        max_acceleration: 10.471_976,
    };
    const RANGE: [f32; 2] = [-0.261_799_4, 0.610_865_2];
    /// Run `ticks` physics ticks, returning each aim.
    fn run(follow: &mut AimFollow, desired: [f32; 2], ticks: usize) -> Vec<[f32; 2]> {
        (0..ticks)
            .map(|_| {
                follow.step(desired, &WARTHOG, RANGE);
                follow.aim()
            })
            .collect()
    }
    #[test]
    fn a_barrel_on_its_target_stays_put() {
        let mut follow = AimFollow::at([0.4, 0.2]);
        for aim in run(&mut follow, [0.4, 0.2], 240) {
            assert!((aim[0] - 0.4).abs() < 1e-5 && (aim[1] - 0.2).abs() < 1e-5);
        }
    }
    #[test]
    fn the_barrel_never_exceeds_its_speed_limit() {
        let mut follow = AimFollow::at([0.; 2]);
        let aims = run(&mut follow, [3.0, 0.5], 360);
        let limit = WARTHOG.max_velocity / 120.;
        let mut previous = [0.; 2];
        for aim in aims {
            // Per physics tick, with the interpolation's one-step slack.
            assert!((aim[0] - previous[0]).abs() <= limit * 1.001, "{aim:?}");
            previous = aim;
        }
    }
    #[test]
    fn the_barrel_speeds_up_gradually_and_arrives() {
        let mut follow = AimFollow::at([0.; 2]);
        let aims = run(&mut follow, [2.0, 0.0], 480);
        // Starting from rest, the first half second covers far less than
        // a barrel at the speed limit would.
        let unlimited = WARTHOG.max_velocity * 0.5;
        assert!(aims[59][0] < unlimited * 0.7, "{:?}", aims[59]);
        assert!(aims[59][0] > 0.05);
        let last = aims.last().unwrap();
        assert!((last[0] - 2.0).abs() < 1e-3 && follow.velocity == [0.; 2], "{last:?}");
    }
    #[test]
    fn the_barrel_brakes_without_overshooting() {
        let mut follow = AimFollow::at([0.; 2]);
        let aims = run(&mut follow, [1.0, 0.0], 480);
        let furthest = aims.iter().map(|a| a[0]).fold(f32::MIN, f32::max);
        assert!(furthest < 1.0 + 1e-3, "overshot to {furthest}");
    }
    #[test]
    fn a_turn_takes_the_short_way_round() {
        let mut follow = AimFollow::at([3.0, 0.]);
        // Past straight back: the short way is across the seam.
        let aims = run(&mut follow, [-3.0, 0.], 12);
        assert!(aims.iter().all(|a| a[0].abs() > 2.5), "{aims:?}");
        assert!(aims.last().unwrap()[0] > 3.0 || aims.last().unwrap()[0] < -3.0);
    }
    #[test]
    fn the_barrel_stays_between_its_pitch_stops() {
        let mut follow = AimFollow::at([0.; 2]);
        for aim in run(&mut follow, [0.0, 1.4], 480) {
            assert!(aim[1] <= RANGE[1] + 1e-5);
        }
        for aim in run(&mut follow, [0.0, -1.4], 480) {
            assert!(aim[1] >= RANGE[0] - 1e-5);
        }
    }
    #[test]
    fn axes_arrive_together() {
        // A long yaw with a short pitch: the pitch is held back so both
        // finish at once, not the pitch alone first.
        let mut follow = AimFollow::at([0.; 2]);
        let aims = run(&mut follow, [2.0, 0.4], 600);
        let pitch_done = aims.iter().position(|a| (a[1] - 0.4).abs() < 1e-3).unwrap();
        let yaw_done = aims.iter().position(|a| (a[0] - 2.0).abs() < 1e-3).unwrap();
        assert!(yaw_done.abs_diff(pitch_done) <= 8, "{pitch_done} {yaw_done}");
    }
    #[test]
    fn no_limits_aim_instantly() {
        let mut follow = AimFollow::at([0.; 2]);
        let none = AimLimits {
            max_velocity: 0.,
            max_acceleration: 0.,
        };
        // The law lands on the first Halo tick; the draw catches up over it.
        for _ in 0..2 * TICKS_PER_LAW_TICK {
            follow.step([1.0, 0.3], &none, RANGE);
        }
        let aim = follow.aim();
        assert!((aim[0] - 1.0).abs() < 1e-6 && (aim[1] - 0.3).abs() < 1e-6, "{aim:?}");
    }
    #[test]
    fn a_speed_limit_needs_an_acceleration_limit() {
        assert!(WARTHOG.valid());
        assert!(AimLimits {
            max_velocity: 0.,
            max_acceleration: 5.
        }
        .valid());
        assert!(!AimLimits {
            max_velocity: 3.,
            max_acceleration: 0.
        }
        .valid());
        assert!(!AimLimits {
            max_velocity: f32::NAN,
            max_acceleration: 1.
        }
        .valid());
    }
}
