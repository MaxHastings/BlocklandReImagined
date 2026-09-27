//! Host-driven native vehicle simulation. Host owns and steps shared physics at 120 Hz.
pub mod muzzle;
pub mod schema;
pub mod world;
pub use schema::{Definition, Family, Pack, Transform};
pub use world::*;
pub const FIXED_DT: f32 = 1.0 / 120.0;
