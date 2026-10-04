//! v20 `.bls` saves as native worlds. The offline converter imports the stock
//! saves with this crate, and the game converts saves players bring over with
//! it too, so it stays free of the Torque asset readers in `bri-convert`.
pub mod bls;
pub mod effect_bindings;
pub mod events;
