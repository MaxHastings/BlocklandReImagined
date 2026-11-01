# Idle seeking across saved spawn events and replacement lives

Added a headless regression using the packed build encode/decode and live
`LoadBuild` path. An `onBotSpawn` row enables idle seeking with sight and
random wandering disabled. The first bot moves toward an eligible player;
an authored `respawnBot` event replaces its life, and the replacement also
moves toward that player without gaining sight or dealing damage. Event
diagnostics remain empty. This demonstrates the saved spawn policy is reapplied,
not that a one-time runtime toggle persists automatically across lives.

`cargo test --locked --profile playtest -p bri-chaos --test bot_builder` passed
all 13 tests. The first test draft measured movement after the fixture had
already approached the player during setup; its assertion was corrected to
measure against the spawn position. `cargo fmt --all` could not run on this
Windows workspace because its generated command exceeded the OS length limit;
`rustfmt --edition 2024 crates/chaos/tests/bot_builder.rs` succeeded.
Multiplayer transmission and broader permissions/lifecycle combinations remain
separate release evidence; this is not a human playtest.
