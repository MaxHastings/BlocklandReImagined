# bri-render tests build optimized

`bri-render map_lighting` took 410 s alone on the Gate PC (600 s per-binary
limit) and timed out under load in batch176b. Cause: the workspace's
`[profile.dev.package."*"] opt-level = 2` covers dependencies only, so the
light fit and ray tracing in `crates/render/src/map_lighting.rs` ran at
opt-level 0. The stock-map test bakes each map once; nothing was re-baked or
loaded twice.

Fix: `[profile.dev.package.bri-render] opt-level = 2` in the root Cargo.toml.
Same tests, same assertions, same limit; debug assertions and overflow checks
stay on.

Measured here (8 cores, no stock content):
- `cargo test -p bri-render --test map_lighting`: 32.6 s at opt-level 0,
  4.3 s at opt-level 2 (16.5 s at 1).
- bri-render rebuild after touching lib.rs: 6 s at 0, 21 s at 2.
- All bri-render test binaries pass.

The stock-map test should drop by about the same factor on the Gate.
Client test binaries that bake map lighting gain too.
