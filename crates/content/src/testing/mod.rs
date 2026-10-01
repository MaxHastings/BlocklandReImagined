//! Made-up content for tests that have no generated v20 content: shapes,
//! textures and packs written in code with invented values, so every crate
//! (not only the client) can exercise its loaders on them.
//!
//! Nothing here is read from, measured on, or copied out of the original
//! game's files. Names follow what the engine itself looks up (the avatar
//! slots, `Mount0`, `Eye`, the sequence aliases the client plays); every
//! number is invented.
//!
//! - [`avatar`]: a small Blockhead-like avatar package (rig, outfit parts,
//!   animations, textures).
//! - [`bricks`]: box brick meshes and a brick materials pack.
//! - [`map_bundle`]: lit room maps written as a native map bundle.
//!
//! Each kind of fixture lives in its own file; the generic builders are
//! re-exported here.

mod helpers;
pub use helpers::*;

pub mod avatar;
pub mod bricks;
pub mod map_bundle;
