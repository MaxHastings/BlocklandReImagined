//! `APawn::calcVelocity`: how a walking, swimming or flying pawn's velocity follows its acceleration.
//!
//! Basis: **binary**. Army Ops 2.x `Engine.dll`, `?calcVelocity@APawn@@AAEXVFVector@@MMMHHH@Z` at RVA 0x1ac530
//! (662 instructions, span sha256 starting `a4626...`; the listing stays outside the repository). Constants are the
//! f32 and f64 values read from the DLL's data: 0.03 (0x10671b04), 1e-8 (0x10659ea0), 100.0 (0x106788c8). Property
//! identities by offset: `Velocity` +0x200, `Acceleration` +0x20c, `AccelRate` +0x540, `WalkingPct` +0x54c,
//! `CrouchedPct` +0x550, the pawn flag byte +0x4c0 (bit 0x40 crouched, bit 0x08 walking), `Mass` +0x3c4 and
//! `Buoyancy` +0x3c8, `PhysicsVolume` +0x1e4 whose vector at +0x544 is `Gravity`.
//!
//! The function runs in four parts, in this order:
//! 1. **Braking** (`bBrake` and no acceleration): the velocity is cut by `2 x Velocity x Friction` per step in steps
//!    of 0.03 seconds, then replaced by the time-averaged velocity of the steps that still point the way it started.
//!    It stops dead if it reversed or fell under 10 units per second (squared length 100).
//! 2. **Driving** (otherwise): the acceleration is limited to `AccelRate`, times `CrouchedPct` while crouched or
//!    `WalkingPct` while walking; the velocity turns toward the acceleration direction by
//!    `(Velocity - AccelDir x |Velocity|) x dt x Friction`.
//! 3. The tail both parts share: `Velocity = Velocity x (1 - bFluid x dt x Friction) + Acceleration x dt`, a pawn the
//!    player does not control has its speed limit scaled, buoyancy adds gravity scaled by `1 - Buoyancy / Mass`, and
//!    the pawn's script `ModifyVelocity(dt, OldVelocity)` runs (the caller's job, between [`calc_velocity`] and
//!    [`clamp_velocity`]).
//! 4. The speed limit: `maxSpeed`, times the same crouch or walk percentage, caps the velocity's length.

/// The braking sub-step, seconds.
pub const BRAKE_STEP: f32 = 0.03;
/// Braking stops dead under this squared speed (10 units per second).
pub const BRAKE_STOP_SQ: f32 = 100.0;
/// Below this squared length a vector is not normalised (the limit clamp zeroes it).
pub const MIN_SQ: f64 = 1e-8;

type V = [f32; 3];

fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn scale(a: V, s: f32) -> V {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: V, b: V) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// What `calcVelocity` reads from the pawn and its caller.
#[derive(Clone, Copy, Debug)]
pub struct Input {
    pub velocity: V,
    /// The pawn's current `Acceleration` property.
    pub acceleration: V,
    /// The `AccelDir` argument: the direction the pawn is being driven.
    pub accel_dir: V,
    pub delta_time: f32,
    /// The `maxSpeed` argument (a walking pawn passes its `GroundSpeed`).
    pub max_speed: f32,
    pub friction: f32,
    /// The `bFluid` argument: 1 in water, 0 on the ground.
    pub fluid: bool,
    /// The `bBrake` argument.
    pub brake: bool,
    /// The `bBuoyancy` argument with the volume's `Gravity`, the pawn's `Buoyancy` and `Mass`.
    pub buoyancy: Option<Buoyancy>,
    pub accel_rate: f32,
    pub crouched: bool,
    pub walking: bool,
    pub walking_pct: f32,
    pub crouched_pct: f32,
    /// A pawn nobody controls scales its speed limit by `ai_speed_scale` (+0x4d0).
    pub human_controlled: bool,
    pub ai_speed_scale: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Buoyancy {
    pub gravity: V,
    pub buoyancy: f32,
    pub mass: f32,
}

/// The result before the script's `ModifyVelocity` runs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Output {
    pub velocity: V,
    pub acceleration: V,
    /// The velocity the function started with: `ModifyVelocity`'s second argument.
    pub old_velocity: V,
    /// The speed limit to hand to [`clamp_velocity`] afterwards.
    pub limit: f32,
}

fn slow_pct(i: &Input) -> Option<f32> {
    (i.crouched || i.walking).then_some(if i.crouched {
        i.crouched_pct
    } else {
        i.walking_pct
    })
}

/// Parts 1 to 3.
pub fn calc_velocity(i: &Input) -> Output {
    let old_velocity = i.velocity;
    let dt = i.delta_time;
    let mut velocity = i.velocity;
    let mut acceleration = i.acceleration;
    let braking = i.brake && acceleration == [0.0; 3];
    if braking {
        let start = velocity;
        let mut average = [0.0f32; 3];
        let mut remaining = dt;
        while remaining > BRAKE_STEP {
            velocity = sub(
                velocity,
                scale(scale(velocity, 2.0), BRAKE_STEP * i.friction),
            );
            if dot(start, velocity) > 0.0 {
                average = add(average, scale(velocity, BRAKE_STEP * (1.0 / dt)));
            }
            remaining -= BRAKE_STEP;
        }
        velocity = sub(
            velocity,
            scale(scale(velocity, 2.0), remaining * i.friction),
        );
        if dot(start, velocity) > 0.0 {
            average = add(average, scale(velocity, remaining * (1.0 / dt)));
        }
        velocity = average;
        if dot(start, velocity) < 0.0 || dot(velocity, velocity) < BRAKE_STOP_SQ {
            velocity = [0.0; 3];
        }
    } else {
        let speed = dot(velocity, velocity).sqrt();
        let limit = i.accel_rate * slow_pct(i).unwrap_or(1.0);
        if dot(acceleration, acceleration) > limit * limit {
            acceleration = scale(i.accel_dir, limit);
        }
        let turn = scale(sub(velocity, scale(i.accel_dir, speed)), dt);
        velocity = sub(velocity, scale(turn, i.friction));
    }
    let keep = 1.0 - (i.fluid as i32 as f32) * dt * i.friction;
    velocity = add(scale(velocity, keep), scale(acceleration, dt));
    let mut max_speed = i.max_speed;
    if !i.human_controlled {
        max_speed *= i.ai_speed_scale;
    }
    if let Some(b) = i.buoyancy {
        let lift = 1.0 - b.buoyancy / b.mass;
        velocity = add(velocity, scale(scale(b.gravity, dt), lift));
    }
    Output {
        velocity,
        acceleration,
        old_velocity,
        limit: max_speed * slow_pct(i).unwrap_or(1.0),
    }
}

/// Part 4: a velocity longer than `limit` is scaled to it; a vanishing one becomes zero.
pub fn clamp_velocity(velocity: V, limit: f32) -> V {
    let sq = dot(velocity, velocity);
    if sq > limit * limit {
        if f64::from(sq) < MIN_SQ {
            return [0.0; 3];
        }
        scale(velocity, (1.0 / sq.sqrt()) * limit)
    } else {
        velocity
    }
}

/// `AActor::GetNetBuoyancy(out NetBuoyancy, out NetFluidFriction)` (RVA 0x1a8f80, binary).
///
/// In a water volume (the volume's flag byte at +0x5a4 has its top bit set) a tall actor with buoyancy is weighted
/// by how much of it the volume covers: `immersion` is the game's answer (the volume's brush asked about the
/// actor's box), and the weight is 1 when that is exactly 1, else `1 - immersion`; an actor with no height or no
/// buoyancy weighs 1. The outputs are the weight times the actor's `Buoyancy` (+0x3c8) and times the volume's
/// `FluidFriction` (+0x588). Outside water nothing changes while the volume's gravity (+0x54c) is the level's
/// default; in a volume whose gravity differs, both are halved: `Buoyancy x 0.5` and `FluidFriction x 0.5`.
pub fn net_buoyancy(
    in_water: bool,
    immersion: Option<f32>,
    height: f32,
    buoyancy: f32,
    fluid_friction: f32,
    gravity_differs_from_default: bool,
) -> (f32, f32) {
    if in_water {
        let weight = match immersion {
            Some(m) if height != 0.0 && buoyancy != 0.0 => {
                if m == 1.0 {
                    1.0
                } else {
                    1.0 - m
                }
            }
            _ => 1.0,
        };
        (weight * buoyancy, weight * fluid_friction)
    } else if gravity_differs_from_default {
        (buoyancy * 0.5, fluid_friction * 0.5)
    } else {
        (0.0, 0.0)
    }
}

/// `APawn::NewFallVelocity(OldVelocity, Gravity, DeltaTime)` (RVA 0x1aefe0, binary): the velocity after a falling
/// step, `Old x (1 - NetFluidFriction x dt) + 0.5 x Gravity x (1 - NetBuoyancy / Mass) x dt`. Only half the
/// gravity step is applied: the falling physics takes it twice per tick, around the move.
pub fn fall_velocity(
    old: [f32; 3],
    gravity: [f32; 3],
    dt: f32,
    net_buoyancy: f32,
    net_fluid_friction: f32,
    mass: f32,
) -> [f32; 3] {
    let weight = 1.0 - net_buoyancy / mass;
    let drag = 1.0 - net_fluid_friction * dt;
    let half = |g: f32| g * weight * 0.5 * dt;
    [
        old[0] * drag + half(gravity[0]),
        old[1] * drag + half(gravity[1]),
        old[2] * drag + half(gravity[2]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn walk(velocity: V, accel_dir: V) -> Input {
        Input {
            velocity,
            acceleration: scale(accel_dir, 1000.0),
            accel_dir,
            delta_time: 1.0 / 60.0,
            max_speed: 220.0,
            friction: 8.0,
            fluid: false,
            brake: true,
            buoyancy: None,
            accel_rate: 1000.0,
            crouched: false,
            walking: false,
            walking_pct: 0.5,
            crouched_pct: 0.5,
            human_controlled: true,
            ai_speed_scale: 1.0,
        }
    }

    #[test]
    fn braking_without_input_decays_by_two_friction_per_step_and_stops_under_ten() {
        let mut i = walk([100.0, 0.0, 0.0], [0.0; 3]);
        i.acceleration = [0.0; 3];
        i.delta_time = 0.01;
        // One partial step of 0.01 s: v x (1 - 2 x 8 x 0.01) = 84, averaged over the whole tick (1.0 of it).
        let out = calc_velocity(&i);
        assert!((out.velocity[0] - 84.0).abs() < 1e-3, "{:?}", out.velocity);
        // Under 10 units per second the pawn stops dead.
        i.velocity = [11.0, 0.0, 0.0];
        assert_eq!(calc_velocity(&i).velocity, [0.0; 3]);
    }

    #[test]
    fn a_long_tick_brakes_in_steps_of_three_hundredths_and_averages_them() {
        let mut i = walk([200.0, 0.0, 0.0], [0.0; 3]);
        i.acceleration = [0.0; 3];
        i.delta_time = 0.1;
        let out = calc_velocity(&i);
        // Three 0.03 s steps then 0.01 s: v1..v3 = 200 x 0.52^n, v4 from v3 x 0.84; each weighted by step / 0.1.
        let (v1, v2, v3) = (
            200.0 * 0.52,
            200.0 * 0.52 * 0.52,
            200.0 * 0.52 * 0.52 * 0.52,
        );
        let v4 = v3 * (1.0 - 2.0 * 8.0 * 0.01);
        let expected = (v1 + v2 + v3) * 0.03 / 0.1 + v4 * 0.01 / 0.1;
        assert!(
            (out.velocity[0] - expected).abs() < 0.01,
            "{} vs {expected}",
            out.velocity[0]
        );
    }

    #[test]
    fn driving_limits_acceleration_and_turns_the_velocity_toward_the_direction() {
        let mut i = walk([0.0, 100.0, 0.0], [1.0, 0.0, 0.0]);
        i.brake = false;
        i.acceleration = [5000.0, 0.0, 0.0];
        let out = calc_velocity(&i);
        assert_eq!(
            out.acceleration,
            [1000.0, 0.0, 0.0],
            "limited to AccelRate along AccelDir"
        );
        // turn: v - (v - dir x |v|) x dt x friction = (100 x 8/60, 100 - 100 x 8/60); then + accel x dt.
        let k = 8.0 / 60.0;
        assert!((out.velocity[0] - (100.0 * k + 1000.0 / 60.0)).abs() < 1e-3);
        assert!((out.velocity[1] - (100.0 - 100.0 * k)).abs() < 1e-3);
    }

    #[test]
    fn crouching_and_walking_scale_the_acceleration_and_the_speed_limit() {
        let mut i = walk([0.0; 3], [1.0, 0.0, 0.0]);
        i.brake = false;
        i.acceleration = [5000.0, 0.0, 0.0];
        i.crouched = true;
        let out = calc_velocity(&i);
        assert_eq!(
            out.acceleration,
            [500.0, 0.0, 0.0],
            "AccelRate x CrouchedPct"
        );
        assert_eq!(out.limit, 110.0, "maxSpeed x CrouchedPct");
        i.crouched = false;
        i.walking = true;
        i.walking_pct = 0.25;
        assert_eq!(calc_velocity(&i).limit, 55.0);
        assert_eq!(
            clamp_velocity([300.0, 0.0, 400.0], 100.0),
            [60.0, 0.0, 80.0]
        );
        assert_eq!(clamp_velocity([3.0, 0.0, 4.0], 100.0), [3.0, 0.0, 4.0]);
    }

    #[test]
    fn a_pawn_nobody_controls_scales_its_limit_and_water_drags() {
        let mut i = walk([100.0, 0.0, 0.0], [1.0, 0.0, 0.0]);
        i.brake = false;
        i.human_controlled = false;
        i.ai_speed_scale = 0.5;
        assert_eq!(calc_velocity(&i).limit, 110.0);
        i.human_controlled = true;
        i.fluid = true;
        i.acceleration = [0.0; 3];
        i.brake = true;
        // Braking with no acceleration, then the fluid drag 1 - 1 x dt x friction on top.
        let dry = {
            let mut d = i;
            d.fluid = false;
            calc_velocity(&d).velocity[0]
        };
        let wet = calc_velocity(&i).velocity[0];
        assert!((wet - dry * (1.0 - 8.0 / 60.0)).abs() < 1e-3);
    }

    #[test]
    fn a_falling_step_takes_half_the_gravity_and_the_fluid_drag() {
        // Dry air: no drag, no buoyancy: 0.5 x -980 x dt added to the z velocity.
        let v = fall_velocity(
            [100.0, 0.0, 0.0],
            [0.0, 0.0, -980.0],
            1.0 / 60.0,
            0.0,
            0.0,
            100.0,
        );
        assert_eq!(v[0], 100.0);
        assert!((v[2] - (-980.0 * 0.5 / 60.0)).abs() < 1e-4);
        // In water: drag 1 - 2 x dt and gravity weighted by 1 - 50 / 100.
        let w = fall_velocity([0.0, 0.0, -60.0], [0.0, 0.0, -980.0], 0.1, 50.0, 2.0, 100.0);
        assert!(
            (w[2] - (-60.0 * 0.8 + (-980.0 * 0.5 * 0.5 * 0.1))).abs() < 1e-3,
            "{w:?}"
        );
    }

    #[test]
    fn buoyancy_follows_immersion_in_water_and_halves_in_odd_gravity() {
        assert_eq!(
            net_buoyancy(true, Some(0.25), 80.0, 40.0, 4.0, false),
            (30.0, 3.0)
        );
        assert_eq!(
            net_buoyancy(true, Some(1.0), 80.0, 40.0, 4.0, false),
            (40.0, 4.0)
        );
        assert_eq!(
            net_buoyancy(true, None, 80.0, 40.0, 4.0, false),
            (40.0, 4.0)
        );
        assert_eq!(
            net_buoyancy(false, None, 80.0, 40.0, 4.0, true),
            (20.0, 2.0)
        );
        assert_eq!(
            net_buoyancy(false, None, 80.0, 40.0, 4.0, false),
            (0.0, 0.0)
        );
    }
}
