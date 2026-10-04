//! v20 `.bls` saves as native worlds. The offline converter imports the stock
//! saves with this crate, and the game converts saves players bring over with
//! it too, so it stays free of the Torque asset readers in `bri-convert`.
#![allow(
    clippy::disallowed_methods,
    reason = "f32::clamp here is not yet bri_console::Clamp::clamped"
)]
pub mod bls;
pub mod effect_bindings;
pub mod events;
