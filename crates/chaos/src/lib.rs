#![recursion_limit = "512"]
//! Crash hunting (see `docs/crash-hunt.md`): a NaN scanner for anything the
//! host replicates, synthetic and real-content chaos fixtures, bot brains,
//! and runners in one process and over loopback QUIC.
pub mod bots;
pub mod fixture;
pub mod local;
pub mod mutate;
pub mod net;
pub mod scan;

/// A `u64` environment variable, or `default`.
pub fn env(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
