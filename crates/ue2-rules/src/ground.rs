//! `APawn` ground movement animation, from Army Ops 2.x `Engine.dll`.
//!
//! Basis: **binary** for every rule and constant below (functions `APawn::Get4WayDirection` at RVA 0x19ab40,
//! `PlayRunning` 0x19e670, `UpdateOnGround` 0x1a2710; floats read from the DLL's data). Which Pawn property each
//! struct offset is was matched by the script property order and by `UpdateOnGround` comparing the vector at +0x200
//! (`Velocity`) squared with 2500.0.

/// Four movement animation slots of a Pawn (`MovementAnims`, `CrouchAnims`, `WalkAnims`, ...): index = direction.
pub const FORWARD: usize = 0;
pub const BACKWARD: usize = 1;
pub const LEFT: usize = 2;
pub const RIGHT: usize = 3;

/// `Get4WayDirection` squares: velocities below this on both axes pick `FORWARD` (f64 at 0x10671f98).
pub const MIN_AXIS_SPEED: f64 = 1e-4;
/// Below this squared length the horizontal direction is not normalised (f64 at 0x10659ea0).
pub const MIN_NORMALISE_SQ: f64 = 1e-8;
/// The forward and backward cones: a dot product past this of the unit horizontal velocity with the facing axis
/// (f32 0x106a8b20 and its negative at 0x106a8b1c). cos 34.9 degrees: a 45 degree diagonal plays a strafe clip.
pub const CONE: f32 = 0.82;
/// `UpdateOnGround` runs below this squared speed (f32 at 0x10689f78): 50 units per second.
pub const RUN_SPEED_SQ: f32 = 2500.0;
/// `PlayRunning` scales the class `GroundSpeed` by this before dividing (f32 at 0x10676d90).
pub const RUN_RATE_SCALE: f32 = 1.1;
/// Tween time passed to every ground `LoopAnim` (0x3dcccccd).
pub const TWEEN: f32 = 0.1;

/// `Get4WayDirection`: velocity and the pawn's facing axes (`forward` and `right` are the X and Y axes of
/// `Rotation`) pick the animation direction. Velocity is world space `[x, y, z]`.
pub fn four_way_direction(velocity: [f32; 3], forward: [f32; 3], right: [f32; 3]) -> usize {
    let (vx, vy) = (velocity[0], velocity[1]);
    if f64::from(vx.abs()) < MIN_AXIS_SPEED && f64::from(vy.abs()) < MIN_AXIS_SPEED {
        return FORWARD;
    }
    let sq = f64::from(vx) * f64::from(vx) + f64::from(vy) * f64::from(vy);
    let (mut x, mut y) = (vx, vy);
    if sq >= MIN_NORMALISE_SQ {
        let inv = (1.0 / sq.sqrt()) as f32;
        x *= inv;
        y *= inv;
    }
    let dot = |axis: [f32; 3]| axis[1] * y + axis[0] * x;
    if dot(forward) > CONE {
        FORWARD
    } else if dot(forward) < -CONE {
        BACKWARD
    } else if dot(right) > 0.0 {
        RIGHT
    } else {
        LEFT
    }
}

/// The pawn's posture for `PlayRunning`: which animation set and speed percentage it uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gait {
    Run,
    Walk,
    Crouch,
}

/// `PlayRunning`'s loop rate: speed over the class ground speed (times 1.1, times the gait's percentage).
/// `walk_pct` and `crouch_pct` are the pawn's `WalkingPct` and `CrouchedPct`.
pub fn run_rate(
    velocity: [f32; 3],
    class_ground_speed: f32,
    gait: Gait,
    walk_pct: f32,
    crouch_pct: f32,
) -> f32 {
    let mut base = class_ground_speed * RUN_RATE_SCALE;
    match gait {
        Gait::Crouch => base *= crouch_pct,
        Gait::Walk => base *= walk_pct,
        Gait::Run => {}
    }
    let speed =
        (velocity[0] * velocity[0] + velocity[1] * velocity[1] + velocity[2] * velocity[2]).sqrt();
    speed / base
}

/// Whether `UpdateOnGround` plays the running loop this tick (speed past 50 units per second).
pub fn is_running(velocity: [f32; 3]) -> bool {
    velocity[0] * velocity[0] + velocity[1] * velocity[1] + velocity[2] * velocity[2]
        >= RUN_SPEED_SQ
}

#[cfg(test)]
mod tests {
    use super::*;

    fn axes(yaw: f32) -> ([f32; 3], [f32; 3]) {
        ([yaw.cos(), yaw.sin(), 0.0], [-yaw.sin(), yaw.cos(), 0.0])
    }

    fn pick(angle_degrees: f32) -> usize {
        let (f, r) = axes(0.0);
        let a = angle_degrees.to_radians();
        four_way_direction([a.cos() * 440.0, a.sin() * 440.0, 0.0], f, r)
    }

    #[test]
    fn the_forward_cone_is_34_9_degrees_each_side() {
        assert_eq!(pick(0.0), FORWARD);
        assert_eq!(pick(34.0), FORWARD);
        assert_eq!(pick(-34.0), FORWARD);
        assert_eq!(pick(36.0), RIGHT);
        assert_eq!(pick(-36.0), LEFT);
    }

    #[test]
    fn a_45_degree_diagonal_strafes_instead_of_running_forward() {
        assert_eq!(pick(45.0), RIGHT);
        assert_eq!(pick(-45.0), LEFT);
        assert_eq!(pick(135.0), RIGHT);
        assert_eq!(pick(-135.0), LEFT);
    }

    #[test]
    fn backward_has_its_own_cone() {
        assert_eq!(pick(180.0), BACKWARD);
        assert_eq!(pick(146.0), BACKWARD);
        assert_eq!(pick(144.0), RIGHT);
    }

    #[test]
    fn standing_still_picks_forward() {
        let (f, r) = axes(1.0);
        assert_eq!(four_way_direction([0.0, 0.0, -300.0], f, r), FORWARD);
        assert_eq!(four_way_direction([5e-5, -5e-5, 0.0], f, r), FORWARD);
    }

    #[test]
    fn rate_is_speed_over_ground_speed_with_ten_percent_headroom() {
        let v = [440.0, 0.0, 0.0];
        let rate = run_rate(v, 400.0, Gait::Run, 0.5, 0.4);
        assert!((rate - 1.0).abs() < 1e-6);
        assert!((run_rate([880.0, 0.0, 0.0], 400.0, Gait::Run, 0.5, 0.4) - 2.0).abs() < 1e-6);
        assert!((run_rate([220.0, 0.0, 0.0], 400.0, Gait::Walk, 0.5, 0.4) - 1.0).abs() < 1e-6);
        assert!((run_rate([176.0, 0.0, 0.0], 400.0, Gait::Crouch, 0.5, 0.4) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn the_vertical_velocity_counts_toward_the_rate() {
        let rate = run_rate([0.0, 0.0, 440.0], 400.0, Gait::Run, 0.5, 0.4);
        assert!((rate - 1.0).abs() < 1e-6);
    }

    #[test]
    fn running_starts_at_fifty_units_per_second() {
        assert!(!is_running([49.0, 0.0, 0.0]));
        assert!(is_running([50.0, 0.0, 0.0]));
    }
}
