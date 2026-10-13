# 2026-10-12 Reading the bots out of a match recording

Max asked how to record a game with bots so odd bot behaviour can be
debugged later. Match recording (2026-10-07, `recordmatches 1`) already
keeps everything needed: every call the host gets, the setup and the bot
dials, so a replay re-runs every bot decision, claim, seat and chat call
exactly. What was missing was a way to look. `bri-replay` only said
whether the match played out the same; the bots' state at a moment was
only there as the five-line readout kept once a second.

## What changed

- `bri_sim::replay::replay_watching` is `replay` that shows a callback the
  session after every replayed tick. `replay` calls it with a no-op. The
  recording format and the recorder are unchanged, so recordings made with
  earlier builds that replay today read out the same way.
- `bri-replay <content-root> <file> --bots FROM[-TO] [--every TICKS]`
  prints, between two moments of the match (`m:ss` or seconds; TO defaults
  to the end), every bot's name, position, velocity, yaw and health, its
  five-line readout (the F3 overlay's) and its full `BotThought`
  (behaviour, route leg, target and memory, seat or push claim, goal,
  objective plan, drives and decisions with every candidate, pressed
  controls, planned harm and fire gate), every `--every` ticks (default
  12, ten a second), plus each chat line said in the window. A replay that
  parts from the recording before the window ends says the lines after it
  are not what happened.

## Evidence

- `cargo test -p bri-sim --test replay` (11 tests): new
  `a_replay_shows_what_the_bots_thought_at_every_tick` records the
  three-bot, 1800-tick match, replays it watching, and checks the callback
  runs once per replayed tick in order, sees all three bots, and that the
  readout it sees at each full check equals the one the recording kept.
- `bri-replay` on a recording written by `a_recorded_match_replays_as_played`
  (a client joins over QUIC, builds, walks, chats) with `--bots 0:00`
  printed the chat line at 0:00.558 and "It plays out exactly as recorded"
  (exit 0); a bad moment and missing arguments exit 2 with the usage.
- `cargo clippy -p bri-net --bin bri-replay -p bri-sim --tests -- -D warnings`
  is clean.

## Next

- Replays still need the same build and the host's own content folder, so
  a recording from Max's PC is replayed on his PC.
