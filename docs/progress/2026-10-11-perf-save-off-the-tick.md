# 2026-10-11 Perf waste fix: a build save is finished off the tick

Finding 12 of the v0.2.10 performance waste audit (report at
`perf/waste-audit-v0.2.10.md` in the project files): saving a build
captured, stripped, validated and encoded the whole world on the host's
tick thread, freezing every player for the duration.

## Measured first

`cargo test --release -p bri-net --test save_probe -- --ignored
--nocapture`, a 200k-brick world on the cloud box (slower than Max's PC,
same shape):

| Step | Where it ran | Time |
| --- | --- | --- |
| `World::clone` (imbl maps, structural sharing) | tick | 1.2 us |
| `SavedBuild::capture` keeping events and owners | tick | 25 ms |
| `SavedBuild::capture` stripping owners (the client's default) | tick | 177 ms |
| `SavedBuild::validate` | tick | 14 ms |
| `codec::encode` of the `Saved` reply (937 KB) | host loop | 306 ms |

About 0.5 s in a loop that has 8.3 ms per tick: 60x the budget, every
save, for everyone connected.

## Fix

- `Session` gains `set_deferred_saves` (the host loop turns it on) and
  `DeferredSave`: with it on, `Command::SaveBuild` does only the clone
  (`saved_world`, microseconds) and the mini-game lookup on the tick,
  answers `Accepted` and leaves the rest for `take_deferred_save`.
  `DeferredSave::finish` is the old body: strip, validate, set the
  mini-game. Direct callers (tests, probes, headless tools) keep the old
  synchronous `Reply::Saved`.
- The host loop (`crates/net/src/server.rs`) takes the deferred save after
  the command and answers with a `Frame::Pending`, the same mechanism a
  world transfer uses: `finish` and the reply's encoding run on a blocking
  thread, and the peer's writer sends the reply at its place in the
  stream, so replies stay ordered and no protocol changes. The flag is set
  again on a map change's new session.

Correctness: the saved content is the world at the command's tick (the
clone is taken inside the command, before any later tick); the reply's
sequence number is unchanged; a failing strip or validation still answers
a `Rejected` reply with its sequence.

## Evidence

- `cargo test -p bri-sim --test packages
  a_deferred_save_is_the_same_build_finished_off_the_tick` (deferred on:
  `Accepted` on the tick, the finished build has the same bricks, owners
  and mini-game as a direct save, ownership stripped).
- `cargo test -p bri-net --test loopback` save tests over real QUIC
  (`host_capability_bulk_load_save_palette_late_join_and_resume`,
  `full_event_list_crosses_real_quic_replication_and_native_save_atomically`),
  `cargo test -p bri-sim` save callers, `cargo clippy -p bri-net -p bri-sim
  --tests -- -D warnings`: pass.

## Next

On Max's PC: save the Bedroom 4-save stack from a client while a second
client watches the frame-time graph; the host's longest tick during the
save is the number to record in the audit report.
