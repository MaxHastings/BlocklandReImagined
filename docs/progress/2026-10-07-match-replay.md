# 2026-10-07 Match recording and headless replay

Door-closer (b) in `docs/audits/platform-door-closers.md`: a host can record a
match and `bri-replay` plays it back headlessly, tick for tick, and says where
it first differs. Max approved the plan and its finish line in the thread
"Match replay from recorded inputs".

## What a recording holds

The file holds what the host is given, never what the game made of it:

- How the host set the game up: the starting world (before package worlds
  grow their ground), map, game mode, settings, content (tool catalog,
  weapon, vehicle and bot packs), the Add-On state the session started
  with, Change Map's palette, the durable admin state and bot dial overrides.
  Passwords are replaced by stand-ins that keep equality, so a recording
  holds no password.
- Every call the host makes on the session, in order: joins, leaves,
  commands, movement, seat and camera reports, steps, Change Map results,
  and the host's `take_*` drains. Each call keeps what the session read from
  outside while running it (the tape) and what it returned.
- After every tick, one digest of the match state. Every second it also keeps
  each part's digest (bricks, players, weapons, vehicles, minigames,
  Add-Ons, chat) and every bot's thought text.

Outside reads go through `Session::outside`. Live, it reads and records;
on replay, it serves the recorded value and notes it if the replay asks for
something different. The routed reads are the wall clock (admin connects and
requests), the save-load time budget, copy-store polls, Add-On host data,
`/botreload` reads, `/botsave` writes and the admin persist callback. Async
map loads arrive as their own frame with the save they started from.

## Finding: replication's drain is game input

Brick events and `events.rs` read the session's dirty set until the
replication drain takes it. When the host drains therefore changes the game.
The recording keeps each `take_*` call, so replay drains at the same ticks.
Anything that runs a session without a network host (tests, bots tooling)
should know the drain cadence is part of the input.

## Using it

- `bri-server --record ...` records into `<state-dir>/recordings`.
- The game records hosted games when `recordmatches 1` is set in the console
  (`$Pref::Server::RecordMatches`). Recordings go into `recordings/` in the
  game's settings folder.
- Both keep the newest ten recordings (`KEPT_RECORDINGS`). Recording is off by
  default.
- `bri-replay <content-root> <file.brimatch>` exits 0 when the match plays
  out the same, 1 when it differs (it prints the tick, the time into the
  match, the parts that differ at the next full check and each bot whose
  thinking differs) and 2 when it cannot replay.
- A host that crashes leaves a file readable up to the last full second.

Same build and same machine only. Cross-platform exact replay is not
promised, and no video or client view is recorded.

## Evidence

- `cargo test -p bri-sim --test replay` (5 tests). These cover:
  - A 1800-tick match with two players, three bots, a minigame, a wrong then
    right admin login and plants replays identically, twice.
  - The file holds no password.
  - A changed movement input at tick 900 is caught at tick 901, and
    "players" differs at the next full check.
  - A cut-off file replays what it holds.
  - An off recorder passes calls straight through.
- `cargo test -p bri-net --lib a_recorded_match_replays_as_played`: a real
  server records while a client joins over QUIC with an identity, plants a
  brick, walks and chats. The file is rebuilt from the same content folder
  and replays with no difference.
  - Mutation check: rebuilding with the starting bricks removed reports a
    difference at tick 0 in "bricks".
- Cost, measured with an optimized build on the cloud runner: 50k bricks,
  16 players walking, 3 bots, a plant every 20 ticks, 1200 ticks.
  - Plain step: about 165 µs per tick. Recorded: about 220 µs per tick.
  - The digest is about 32 µs of that (mostly building the player views);
    the rest is writing the frames.
  - File size is 3.0 MB raw for 10 s, about 435 KB compressed (2.6 MB a
    minute).
  - The measurement code was not kept.
- `cargo clippy -p bri-sim -p bri-net -p bri-ui -p bri-client --all-targets -- -D warnings`
  is clean.
- Test suites for bri-sim, bri-net, bri-ui and the bri-client lib pass, apart
  from failures that need a GPU or are already known:
  - `a_bot_jets_over_to_someone_above_it` is a known failure on main
    (`tools/gate-known-failures.toml`, owned by the measured-moves thread).
  - bri-ui and bri-client's offscreen render tests need a GPU, and the
    cloud runner has none ("no wgpu adapter").

## Next

- An independent review, then the PC gate.
- Max decides whether hosted games record by default.
- Not covered:
  - Recording the client's own view.
  - Replay across builds or platforms.
  - Seeking within a recording.
