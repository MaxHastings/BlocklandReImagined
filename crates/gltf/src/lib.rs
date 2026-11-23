//! GLB containers and deterministic, lossless compiled Blockland models.
//! Authoring options and scene-to-package policy live in bri-addon-import.
mod native;
mod read;
#[doc(hidden)]
pub mod testing;
mod write;

pub use native::{encode_model, is_model_file, read_model};
pub use read::{Document, array};
pub use write::{Channel, Gltf};
