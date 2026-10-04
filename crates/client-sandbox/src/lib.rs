//! The client sandbox: Add-On code that runs on a player's machine.
//!
//! Servers send joining players data, plus sandboxed WebAssembly and WGSL
//! shaders from their Add-Ons. This crate is the sandbox those run in. It
//! is presentation only: code here draws, plays sounds and shows panels,
//! and talks to its own server script through messages; gameplay truth
//! stays on the server. Design: `docs/architecture/client-sandbox.md`.
//!
//! - [`addon`]: an Add-On's client code, checked before anything runs.
//! - [`capability`]: what code may ask for, and the trust tier each needs.
//! - [`shader`]: WGSL validation and loop bounding.
//! - [`host`]: the WebAssembly host, its functions and budgets.
//! - [`trust`]: the per-server trust prompt and what the player chose.
//! - [`gpu`]: draws an Add-On's render layer with wgpu.
//! - [`world`]: what the game shows, for code that reads it.
//! - [`bodies`]: local physics bodies and body poses Add-Ons ask for.
pub mod addon;
pub mod bodies;
pub mod capability;
pub mod gpu;
pub mod host;
pub mod shader;
pub mod trust;
mod wasm_imports;
pub mod world;

pub use addon::AddOnCode;
pub use capability::{Capability, Tier};
pub use host::{AddOn, Blend, Budgets, FrameInput, Sandbox, Space, Stopped, View};
pub use trust::{TrustDecision, TrustLevel, TrustPrompt, TrustStore};
pub use world::World;
