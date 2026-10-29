# 2026-10-29 Sky snow atlas clipping

Maxwell reported quarter-flakes in a snowy city screenshot and requested a fix
merged into main. Cause: client sky precipitation inherited the map weather atlas
layout only when encoded image SHA-256 matched. The independent weather and
environment importers encoded identical original snow pixels into different PNGs,
so the sky used the default 4x4 subdivision instead of four whole 2x2 flakes.

Generated evidence (read only): weather-pack-002 snow PNG SHA-256
`9b1eee968e73331e91dddbd30d01cf84886ce7b9c42a2daf8828d19f2869f21c` and
environments-001 snow PNG SHA-256
`fc705c8d5d434dd3f9efd538bff884c64ac145a72c78559fce64bed63adf4958`.
Both decode to 256x256 RGBA SHA-256
`f6c868e0c389a14fe8fd20181020975de8cebc33a6154625f313e99c67ad4924`.
Python Pillow decoding and the existing weather manifest established equality;
the imported atlas was visually inspected. Original installations were untouched.

`client/weather.rs::with_sky_rain` now also matches decoded RGBA hashes and
requires equal dimensions before borrowing an existing drop/splash atlas layout.
It uses existing weather manifest identity fields; no schema, resource, shader,
network or save changes. Unknown pictures retain the existing fallback.
The ordinary sky replacement/restoration regression now runs both original-file
and re-encoded variants, with negative checks for changed pixels and dimensions.

Validation commands for this change:
- `cargo fmt -p bri-client` and `python tools/check.py focused --crates bri-client`.
- `cargo test --locked -p bri-client --lib weather::tests:: -- --nocapture`.
- History/protocol and platform CI checks before merge.

Local checks and CI were started for this correction; final outcomes are recorded
in the pull request and chat handoff. This entry does not claim a full workspace
pass, a new release/package, or interactive playtest acceptance. Maxwell can
verify complete flakes by selecting the snowy sky in his next updated build.
