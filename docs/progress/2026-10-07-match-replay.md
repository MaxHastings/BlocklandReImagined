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
  Passwords are replaced by stand-ins that keep equality, so no admin or
  host password is written. Chat is kept as typed, so a password typed
  into chat is in the file, as are player names, identity public keys and
  the bot-overrides file path. Share recordings with that in mind.
- Every call the host makes on the session, in order: joins, leaves,
  commands, movement, seat and camera reports, steps, Change Map results
  (with the bot dials the new map's session read), and what the host takes
  to send or act on (changed bricks, cues, notices, kicks, map-change
  requests). Each call keeps what the session read from outside while
  running it (the tape) and a digest of what it returned or what was
  taken. Diagnostics the host takes (script time, slow ticks) are not
  play and are not recorded.
- After every tick, one digest of the match state. Every second it also keeps
  each part's digest (bricks, players, weapons, vehicles, minigames,
  Add-On entities and every stored Add-On value, chat) and every bot's
  thought text.

Outside reads go through `Session::outside`. Live, it reads and records;
on replay, it serves the recorded value. A replay never reads live: a read
the recording does not hold stops that call and is reported as where the
runs part (so a divergent replay of `/botsave` cannot write a file the
recording names). If a recording stops because writing failed, its tape
stops keeping reads. The routed reads are the wall clock (admin connects and
requests), the save-load time budget, copy-store polls, Add-On host data,
`/botreload` reads, `/botsave` writes and the admin persist callback. Async
map loads arrive as their own frame with the save they started from.

## Fixed: brick events depended on when the network sent

Brick events and Add-On brick inputs asked "changed since replication last
sent?" (`dirty.contains` in `session/events.rs`, `dirty.iter()` in
`session/packages/brick_events.rs`) instead of "changed since the event
phase last looked?". So when the host drained changes for the network
changed play slightly: a brick edited in the last few ticks reinstalled its
program (which re-checks the event byte budget), and a session nobody
drained (tests, bot tooling) grew that set forever and recompiled programs
on every input. Both sites now read the event phase's own unread set
(`Dirty::unread_by_events`, `events_unread`), which only the event phase's
read clears. Network timing is no longer game input; the recording still
compares the changed bricks the host takes, as a check.

## Using it

- `bri-server --record ...` records into `<state-dir>/recordings`.
- The game records hosted games when `recordmatches 1` is set in the console
  (`$Pref::Server::RecordMatches`). Recordings go into `recordings/` in the
  game's settings folder.
- Recordings are numbered in the order they start (`match-<n>.brimatch`),
  so a clock set back never makes the newest look oldest. Both hosts keep
  the newest ten (`KEPT_RECORDINGS`). A recording that cannot start leaves
  the game unrecorded. Recording is off by default.
- `bri-replay <content-root> <file.brimatch>` exits:
  - 0 when the whole recording plays out the same;
  - 1 when it differs (it prints the tick, the time into the match, the
    parts that differ at the next full check and each bot whose thinking
    differs);
  - 2 when it cannot replay: an empty or damaged file, or other content;
  - 3 when it matches as far as a file cut short (the host stopped while
    writing it) goes.
- A host that crashes leaves a file readable up to the last full second.

Same build and same machine only. Cross-platform exact replay is not
promised, and no video or client view is recorded.

## Evidence

- `cargo test -p bri-sim --test replay` (9 tests):
  - A 1800-tick match with two players, three bots, a minigame, a wrong
    then right admin login and plants replays identically.
  - The file holds no admin password.
  - A changed movement input at tick 900 is caught at tick 901, and
    "players" differs at the next full check.
  - A different Add-On value with the same number of changes is caught,
    in "Add-Ons" only. With the old change-counter digest this test fails.
  - A host take moved to another moment is caught at that take.
  - A clock read missing from the recording is reported, not read live.
  - An empty file and a damaged frame are errors.
  - A cut-off file replays what it holds.
  - An off recorder passes calls straight through.
- `cargo test -p bri-sim --lib dirty`: the event phase's unread set does not
  change when replication takes its set.
- `cargo test -p bri-net --lib a_recorded_match_replays_as_played`:
  - A real server records while a client joins over QUIC with an identity,
    plants a brick, walks and chats. The file is rebuilt from the same
    content folder and replays with no difference.
  - The same file with its last byte cut (a host killed mid-write) matches
    as far as it goes and reports the early end.
  - Rebuilt without the brick it started with, it differs at once, in
    "bricks".
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
- An independent review (`/mnt/project-files/v0.2.6/review-replay.md`)
  found three ways the tool could say "matched" when it did not. All three
  are fixed and tested above, as are the drain bug and these smaller
  findings:
  - redaction now replaces only whole secret values;
  - recording's tape stops when the recording does;
  - the writer refuses frames the reader would;
  - Change Map's bot dials are recorded;
  - recordings are numbered;
  - one panic-message helper;
  - unused code was removed.

## Next

- A re-review of the fixes, then the PC gate.
- Before recording is on by default:
  - write the file on its own thread, so a slow disk never stalls the tick;
  - cap recordings by size, not only by count;
  - Max decides the default.
- Queued from the review:
  - route `AddOnData::save` through the tape (no host calls it yet);
  - digest admin state, time scale, environment and the pending event queue
    directly (today a difference there shows once it reaches a digested
    part);
  - fsync for power loss.
- Not covered:
  - Recording the client's own view.
  - Replay across builds or platforms.
  - Seeking within a recording.
