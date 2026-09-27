//! Blockland ReImagined native audio runtime.
//!
//! * [`schema`] – versioned native audio pack (`content/audio-pack-NNN/manifest.json`).
//! * [`SoundBank`] – loads a pack: verifies hashes, preloads PCM, keeps music streamable.
//! * [`AudioRuntime`] – game-facing typed commands (play/stop/attach/listener/volume),
//!   bounded voices with observable culling, events and counters.
//! * Output adapters: `Null` and `Offline` (inline, hardware-free) and `Device`
//!   (feature `cpal-output`, opened only on explicit request).
//!
//! Coordinates are native right-handed Y-up world units. See the crate README.

pub mod bank;
pub mod command;
pub mod decode;
#[cfg(feature = "cpal-output")]
mod device;
pub mod engine;
pub mod error;
pub mod runtime;
pub mod schema;
pub mod spatial;
pub mod wav;

pub use bank::{BankOptions, ClipData, SoundAsset, SoundBank};
pub use command::{AudioEvent, CullReason, EntityKey, Placement, SoundHandle, VolumeControl};
pub use engine::{EngineConfig, VoicePolicy};
pub use error::AudioError;
pub use runtime::{AudioRuntime, AudioStats, OutputKind, RuntimeConfig};
pub use spatial::{GainCurve, Listener, Vec3};
