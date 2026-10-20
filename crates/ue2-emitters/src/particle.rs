//! One particle record, as the original keeps it (0xd0 bytes in the original; here
//! the same fields in the same roles).
use crate::props::Color;
use glam::Vec3;

/// `Particle.Flags` bits, as read in `UpdateParticles`.
pub mod flags {
    /// The slot is in use.
    pub const ALIVE: i32 = 1;
    /// Its speed fell below `MinSquaredVelocity`.
    pub const STOPPED: i32 = 2;
    /// Born this frame.
    pub const NEW: i32 = 4;
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Particle {
    pub location: Vec3,
    pub old_location: Vec3,
    pub velocity: Vec3,
    pub start_size: Vec3,
    /// In angle units (65536 per turn): the spawn scales the authored value by 65535.
    pub spins_per_second: Vec3,
    pub start_spin: Vec3,
    pub revolution_center: Vec3,
    pub revolutions_per_second: Vec3,
    pub revolutions_multiplier: Vec3,
    pub size: Vec3,
    pub start_location: Vec3,
    pub color_multiplier: Vec3,
    pub velocity_multiplier: Vec3,
    pub old_mesh_location: Vec3,
    pub color: Color,
    pub time: f32,
    pub max_lifetime: f32,
    pub mass: f32,
    pub hit_count: i32,
    pub flags: i32,
    /// -1: choose the frame from the age.
    pub subdivision: i32,
    pub bone_index: i32,
    pub num_frames_rendered: u8,
    pub fade_time: f32,
}
