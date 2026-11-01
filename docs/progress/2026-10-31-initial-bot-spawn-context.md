# Initial bot spawn game context

A saved-build regression with `onBotSpawn -> Bot -> setTeam` reproduced a
rejected first event (`This action needs a mini-game`) even though a later
spawn eventually assigned the desired team. The brick bot was created and its
event fired before periodic reconciliation placed it in its brick's game.

New brick bots now enter their determined game before its spawn is announced.
The existing game placement emits the spawn through the ordinary respawn path,
or holds the spawn if the game requests that; bots with no game still announce
their initial spawn directly. This retains existing authority checks. The new
test encodes and decodes the saved build, loads it into an existing team game,
and checks both the assigned team and the absence of event rejection diagnostics.
The existing replacement-life event test remains in the same suite.

Validation: `cargo test --locked --profile playtest -p bri-chaos --test
bot_builder --test bot_team` passed 13 and 3 tests respectively, including
saved idle seeking and spawn/respawn lifecycle cases. `cargo clippy --locked
--profile playtest -p bri-sim -p bri-chaos --lib --test bot_team --test
bot_builder -- -D warnings` passed. These tests do not establish every
multiplayer or package-specific spawn interaction; combined release checks
remain outstanding.
