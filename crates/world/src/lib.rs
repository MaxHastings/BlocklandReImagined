//! Portable authoritative state. No window, GPU, physics handle or legacy reader.
pub mod authority;
pub mod build;
pub mod model;
pub mod packed;
pub mod persistence;
pub use model::*;
