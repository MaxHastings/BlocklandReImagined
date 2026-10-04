//! Native rain and snow. Host-owned collision, environment, camera and GPU device.
#![allow(
    clippy::disallowed_methods,
    reason = "f32::clamp here is not yet bri_console::Clamp::clamped"
)]
pub mod content;
#[cfg(feature = "gpu")]
pub mod gpu;
pub mod simulation;
pub mod testing;
pub use content::*;
pub use simulation::*;
