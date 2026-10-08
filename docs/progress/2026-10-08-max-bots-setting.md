# 2026-10-08 Max bots host setting

Max asked why a mini-game stops at 16 bots and whether it could be a host
option. The 16 was a hard-coded `MAX_BOTS`, shared by spawn-brick bots and
mini-game rules (Slayer) bots. Nothing on the wire or in the UI needed it:
it was the count the bot think-time bar was measured at (1441 us a tick for
16 bots in release, of the 8333 us tick at 120 Hz). The sight-ray budget
was already sized for 32.

## Changed

- `bri_admin::ServerSettings::max_bots`: Max bots, 1 to 32 (`MOST_BOTS`),
  default 16 (`DEFAULT_BOTS`); older saved settings without it read 16.
- `Session::bot_limit()` is the cap for spawn bricks and rules bots and
  their "Server is limited to N bots" lines. Lowering it keeps bots already
  running; it stops new ones.
- `ops::MAX_BOTS` is now the ceiling (32), which the per-tick planning
  share is sized for; `ops::DEFAULT_BOTS` is 16. Scripts' `bot_limit()`
  reads the host's value from the snapshot, so Slayer fills to it.
- Advanced Config and the Admin menu's Server Settings get a Max Bots box
  (`$Pref::Server::MaxBots`), added under Max Player Vehicles in v20's
  serverConfigGui with the rows below moved down.
- Wire: `protocol-changes/server-settings-max-bots.md`.
- `bot_think_time_32` (ignored, release) runs the busiest scenario with 32
  bots against the perf bar scaled from 16.

## Evidence

- `cargo test -p bri-sim --test script_api`: 26 pass, including Max bots 20
  holding a rules fill of 25 at 20 with `bot_limit()` reading 20.
- `cargo test -p bri-ui --test field_flow`: passes with the Max Bots box in
  both tables.
- `cargo test -p bri-admin -p bri-package-runtime`: pass.

## Next

- Run `bot_think_time_32` in release on Max's PC and record the number.
- Max checks the Max Bots box's place in Advanced Config in his playtest.
