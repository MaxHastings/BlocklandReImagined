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

/// Proptest settings for a gate test: `cases` cases drawn from a fixed
/// `seed`, so every run tries the same inputs and a failure reproduces
/// exactly, and no failure files. Setting `PROPTEST_RNG_SEED` draws from
/// another seed for a soak.
pub fn proptest_config(cases: u32, seed: u64) -> proptest::test_runner::Config {
    let config = proptest::test_runner::Config::default();
    proptest::test_runner::Config {
        cases,
        failure_persistence: None,
        rng_seed: if std::env::var_os("PROPTEST_RNG_SEED").is_some() {
            config.rng_seed
        } else {
            proptest::test_runner::RngSeed::Fixed(seed)
        },
        ..config
    }
}

/// A `u64` environment variable, or `default`.
pub fn env(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
