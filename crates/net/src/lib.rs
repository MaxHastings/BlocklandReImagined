//! QUIC transport and bounded replication for native game sessions.
#![allow(
    clippy::disallowed_methods,
    reason = "f32::clamp here is not yet bri_console::Clamp::clamped"
)]
mod admin_store;
pub mod allocator;
pub mod client;
pub mod codec;
pub mod content_identity;
pub mod dedicated;
pub mod discovery;
pub mod host_setup;
pub mod impair;
pub mod invite;
pub mod lag;
pub mod map_content;
pub mod natpmp;
pub mod packages;
pub mod protocol;
pub mod reach;
pub mod recovery;
pub mod replica;
pub mod server;
pub mod stream;
pub mod testing;
mod tick_clock;
pub mod timer_resolution;
pub mod traffic;
pub mod upnp;
pub mod wire;
