# 2026-10-08 Gate: the bot crate compiles optimized in test builds

v0.2.7, faster release check. The gate builds tests in the dev profile, where
workspace crates were unoptimized (bri-render excepted). Bot tests spend
nearly all their time in bri-sim's simulation step, so they came close to or
past the gate's 600 s per-binary limit on Max's PC: bot_soccer_match 526-602 s
(602 s timed out a gate run at 6465c04), bot_combined_perf 520-597 s,
bot_battle_perf 461-560 s, across the last six gate logs in
`../.bri-gate/logs`.

Change: `[profile.dev.package.bri-sim] opt-level = 2` in Cargo.toml, as
bri-render already has for the lighting bake. Debug assertions and overflow
checks stay on, so the tests check exactly what they did.

Measured on a 4-core Linux container (`--include-ignored --test-threads 4`):

| bri-sim opt-level | bot_soccer_match | bot_combined_perf | bri-sim rebuild (no incremental) |
|---|---|---|---|
| 0 (before) | 517 s | 379 s | 26 s |
| 1 | 232 s | 163 s | 62 s |
| 2 (chosen) | 155 s | 92 s | 88 s |
| 2, plus world, physics, weapons, vehicles, events at 2 | 116 s | 83 s | not measured |

Every run gave the same results: the soccer test's
`two_against_two_play_a_clean_match_across_seeds` failed identically at
every level on Linux ("Broom 2v2 seed 2: ball unattended 110.8 s"). It passes
on the PC, so it's a Linux/Windows difference for the cross-platform
determinism thread, not an effect of the optimization.

Cost: about 60 s more each time bri-sim itself recompiles. Crates.io
dependencies were already at opt-level 2.

Next: the wider set (last row) if the bot tests need more room; bot_battle_perf
needs the generated content, so measure it on the PC's next gate log.
