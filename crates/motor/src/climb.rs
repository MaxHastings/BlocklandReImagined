//! Climb volumes: ladders. A map places a box (a sensor collider carrying
//! its facing and speed in its tag) where a body may take hold; the motor
//! finds it with the body's middle, as Unreal finds a `LadderVolume` for
//! a pawn by its location. Nothing here is Unreal's: the body's archetype
//! says whether and how it climbs ([`Climb`]), the volume says where.
//!
//! While a body holds a ladder the motor drives it: it faces the way the
//! volume faces, gravity does not pull it, and the move keys run it along
//! the rungs. It lets go when its middle leaves the volume, when it jumps,
//! or when it comes down onto a floor.
use glam::Vec3;
use rapier3d::prelude::*;
use serde::{Deserialize, Serialize};

/// A climb volume's `user_data`: this marker in the high bits, its
/// `strafe` flag and its speed (`f32` bits) in the low.
const MARK: u128 = 0x434c_4d42;
const SHIFT: u32 = 96;
const STRAFE: u128 = 1 << 32;

/// [`crate::player::PlayerTuning::climb`]: a body that climbs ladders.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Climb {
    /// Fastest rising or falling speed that still lets the body take hold
    /// in mid-air, units a second (Unreal's `JumpZ` gate on
    /// `CanGrabLadder`).
    #[serde(default = "Climb::default_grab_speed")]
    pub grab_speed: f32,
    /// How squarely the body must face a ladder to take hold: the dot of
    /// its facing and the volume's, 0 to 1.
    #[serde(default = "Climb::default_facing")]
    pub facing: f32,
    /// How squarely it must be pushing toward the ladder (or not pushing
    /// at all, or in the air): the dot of the move and the volume's facing.
    #[serde(default = "Climb::default_push")]
    pub push: f32,
    /// The speed a jump off a ladder rises at, units a second.
    #[serde(default = "Climb::default_jump")]
    pub jump_speed: f32,
    /// How fast a body holding a ladder is drawn toward the wall it faces,
    /// units a second, until it touches the wall or reaches the far side of
    /// the volume: a climber hangs against the rungs, as the markers a map
    /// places for climbers sit. Zero holds the body where it took hold.
    #[serde(default)]
    pub wall_pull: f32,
}
impl Climb {
    fn default_grab_speed() -> f32 {
        16.0
    }
    fn default_facing() -> f32 {
        0.9
    }
    fn default_push() -> f32 {
        0.7
    }
    fn default_jump() -> f32 {
        16.0
    }
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            [self.grab_speed, self.jump_speed, self.wall_pull]
                .iter()
                .all(|n| n.is_finite() && (0.0..=1000.0).contains(n))
                && [self.facing, self.push]
                    .iter()
                    .all(|n| n.is_finite() && (-1.0..=1.0).contains(n)),
            "Invalid climb: speeds 0 to 1000, facing and push -1 to 1"
        );
        Ok(())
    }
}
impl Default for Climb {
    fn default() -> Self {
        Self {
            grab_speed: Self::default_grab_speed(),
            facing: Self::default_facing(),
            push: Self::default_push(),
            jump_speed: Self::default_jump(),
            wall_pull: 0.0,
        }
    }
}

/// A climb volume found around a point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Volume {
    /// Facing about +Y, as a player's yaw: the way a climber looks at the
    /// rungs.
    pub yaw: f32,
    /// Units a second along the rungs at full push.
    pub speed: f32,
    /// Whether the strafe keys move sideways.
    pub strafe: bool,
    /// The way up the rungs.
    pub up: Vec3,
    /// The middle of the volume.
    pub center: Vec3,
    /// Half its extent along the way a climber faces.
    pub depth: f32,
    /// Half its extent along its own axes: across the rungs, up the rungs, and
    /// toward the wall.
    pub half: Vec3,
}
impl Volume {
    /// The unit vector a climber faces, as [`crate::player::PlayerState::forward`].
    pub fn facing(&self) -> Vec3 {
        Vec3::new(self.yaw.sin(), 0.0, -self.yaw.cos())
    }
}

/// Whether a collider's `user_data` marks a climb volume.
pub fn is_climb(user_data: u128) -> bool {
    user_data >> SHIFT == MARK
}

/// The sensor collider of a climb volume centred at `center`, `half` in
/// extent along its own axes, which `yaw` turns about +Y so a climber
/// faces `yaw`.
pub fn collider(center: Vec3, half: Vec3, yaw: f32, speed: f32, strafe: bool) -> ColliderBuilder {
    collider_with_facing(center, half, yaw, speed, strafe, yaw)
}

/// A sensor box whose climbing direction need not follow its orientation.
/// This preserves authored volume bounds when its rungs face diagonally.
pub fn collider_with_facing(
    center: Vec3,
    half: Vec3,
    yaw: f32,
    speed: f32,
    strafe: bool,
    facing_yaw: f32,
) -> ColliderBuilder {
    let tag = (MARK << SHIFT)
        | u128::from(speed.to_bits())
        | (u128::from(facing_yaw.to_bits()) << 33)
        | if strafe { STRAFE } else { 0 };
    ColliderBuilder::cuboid(half.x, half.y, half.z)
        .sensor(true)
        .position(Pose::from_parts(
            Vector::from_array(center.to_array()),
            // yaw 0 faces -Z; a quarter turn faces +X.
            Rotation::from_rotation_y(-yaw),
        ))
        .user_data(tag)
}

/// The climb volume holding `point`, if any (the first when several do).
pub fn find(physics: &PhysicsWorld, point: Vec3) -> Option<Volume> {
    let filter = QueryFilter::default().predicate(&|_, c: &Collider| is_climb(c.user_data));
    let query = physics.query_pipeline_with_filter(filter);
    let (_, collider) = query
        .intersect_point(Vector::from_array(point.to_array()))
        .next()?;
    Some(read(collider))
}

fn read(collider: &Collider) -> Volume {
    let tag = collider.user_data;
    let yaw = f32::from_bits(((tag >> 33) & 0xffff_ffff) as u32);
    let local_half = collider
        .shape()
        .as_cuboid()
        .map_or(Vec3::ZERO, |c| Vec3::from_array(c.half_extents.to_array()));
    let into_facing = glam::Quat::from_rotation_y(yaw)
        * glam::Quat::from_array(collider.position().rotation.to_array());
    let half = (into_facing * Vec3::X).abs() * local_half.x
        + (into_facing * Vec3::Y).abs() * local_half.y
        + (into_facing * Vec3::Z).abs() * local_half.z;
    Volume {
        yaw,
        speed: f32::from_bits((tag & 0xffff_ffff) as u32),
        strafe: tag & STRAFE != 0,
        up: Vec3::Y,
        center: Vec3::from_array(collider.position().translation.to_array()),
        depth: half.z,
        half,
    }
}

/// Every climb volume of the world, for a planner that routes over ladders.
pub fn all(physics: &PhysicsWorld) -> Vec<Volume> {
    physics
        .colliders
        .iter()
        .filter(|(_, collider)| is_climb(collider.user_data))
        .map(|(_, collider)| read(collider))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_volume_is_found_around_its_centre_and_reads_back_its_facing() {
        let mut physics = bri_physics::new_world();
        physics.insert_collider(
            collider(
                Vec3::new(10.0, 5.0, -3.0),
                Vec3::new(1.0, 4.0, 1.0),
                std::f32::consts::FRAC_PI_2,
                8.0,
                false,
            ),
            None,
        );
        bri_physics::detect_collisions(&mut physics);
        assert!(find(&physics, Vec3::new(0.0, 0.0, 0.0)).is_none());
        let volume = find(&physics, Vec3::new(10.5, 8.0, -3.5)).unwrap();
        assert!((volume.yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        assert_eq!(volume.speed, 8.0);
        assert!(!volume.strafe);
        // A quarter turn faces +X.
        assert!((volume.facing() - Vec3::X).length() < 1e-5);
        // Above its top the point is out of it.
        assert!(find(&physics, Vec3::new(10.0, 9.5, -3.0)).is_none());
    }

    #[test]
    fn diagonal_facing_does_not_enlarge_the_sensor_box() {
        let mut physics = bri_physics::new_world();
        let yaw = -std::f32::consts::FRAC_PI_4;
        physics.insert_collider(
            collider_with_facing(Vec3::ZERO, Vec3::new(1.0, 4.0, 1.0), 0.0, 8.0, true, yaw),
            None,
        );
        bri_physics::detect_collisions(&mut physics);
        let volume = find(&physics, Vec3::new(0.99, 0.0, 0.99)).unwrap();
        assert!((volume.yaw - yaw).abs() < 1e-6);
        assert!(find(&physics, Vec3::new(1.01, 0.0, 0.0)).is_none());
        assert!((volume.depth - std::f32::consts::SQRT_2).abs() < 1e-5);
    }

    #[test]
    fn only_climb_volumes_carry_the_mark() {
        assert!(is_climb(
            collider(Vec3::ZERO, Vec3::ONE, 0.0, 1.0, true).user_data
        ));
        assert!(!is_climb(u128::MAX));
        assert!(!is_climb(1 << 64));
        assert!(!is_climb(0));
    }

    #[test]
    fn the_defaults_are_a_valid_tuning() {
        assert!(Climb::default().validate().is_ok());
        let bad = Climb {
            facing: 2.0,
            ..Climb::default()
        };
        assert!(bad.validate().is_err());
    }
}
