> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-replay.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: match replay (PR #27), head cc621a3, base main a78b3a5

This was an independent, read-only review. Nothing was built or run. Two readers went through the code, and I re-checked every blocking item and the drain finding myself (marked **checked**). Paths are under `crates/sim/src/` unless they name another crate.

## Verdict

**The design is right, but three things must change before merge.**

What's already right:
- It records inputs, not events.
- The Session API is unchanged.
- Gameplay doesn't change, and nothing new is allocated, while recording is off.
- No wall-clock or unseeded random read that affects gameplay is missed.
- No password is written as itself.

The three blockers are places where `bri-replay` can say "plays out exactly as recorded" when it doesn't. That is the one promise the tool makes.

## Blocking

**B1. The Add-Ons digest hashes a change counter, not the state.** **(checked)**
- Where: `replay.rs:803` digests `(package_entities(), package_state_revision())`. `package_state_revision` is a counter that goes up by one per change (`session/packages.rs:3512-3514`). The doc at `replay.rs:684` says this part covers "Add-On entities and state".
- What goes wrong: a script stores a different score or flag with the same number of commits, and the replay reports "same".
- Fix: digest the state itself, `package_view(None)` or the store namespaces.

**B2. What the host drains is never compared, and recorded host decisions are applied anyway.** **(checked)**
- Where: `Recorder::take` (`replay.rs:1224-1233`) records every `take_*` as `Outcome::Done` with no digest. The replayer drops what its own session returns.
- What goes wrong:
  - Cues, notices, private notices, admin disconnects and map-change requests are never checked.
  - The recorded `Disconnect` and `MapChanged` frames are applied either way. If the replayed session would not have kicked someone, the replay kicks them anyway and converges to "same".
- Fix: record `Outcome::Replied(digest_of(&taken))` and compare it on replay.

**B3. A damaged or empty recording exits 0.** **(checked)**
- Where: any I/O error while reading, including zstd corruption mid-stream, becomes "cut off" (`replay.rs:879-893`). `bri-replay` succeeds whenever no divergence was found (`net/src/bin/bri-replay.rs:64`). A file that stops right after its header prints "It plays out exactly as recorded" with 0 ticks replayed.
- Fix:
  - Give a cut-off file its own exit code and message ("matches up to tick N, then the file ends").
  - Fail when no frames were read.
  - Treat a decode error that isn't at the end as damage, not a cut.

## The drain finding (question 2)

**The finding is true. It's a real bug, low in severity. Recording the drain is right for replay, but it should not be the fix.** **(checked)**

- `Dirty` (`session/dirty.rs:20-60`) holds two things:
  - replication's `set`, which is emptied only by `take()`;
  - a per-reader cursor log, which is already independent of the network.
- Two gameplay sites read replication's `set` instead of their own cursor:
  - `session/events.rs:556`: "edited since the last event phase" is answered with `dirty.contains`, so a brick reinstalls its program.
  - `session/packages/brick_events.rs:157`: the candidate bricks for an Add-On input are extended with `dirty.iter()`.
- Effect in play, which is small because the server drains on a fixed 6-tick cadence whatever the client count (`net/src/server.rs:1614-1615`):
  - Mostly harmless re-installs and extra candidates that fire nothing.
  - One real difference. A reinstall re-checks the event byte budget (`crates/events/src/runtime.rs:476-479`). Near the budget, a brick edited in the last 6 ticks can be disabled ("Brick N events disabled") where an older one would fire.
  - A session that never drains (bri-sim tests, tooling) grows the set forever and recompiles programs on every input. That is a memory and speed bug for headless use, such as bot tests.
- Why it matters for the design: gameplay must not depend on when the network layer sends. The write-up's advice that "the drain cadence is part of the input" makes that coupling permanent. Please replace it with a filed bug.
- **The real fix is small:**
  1. Add a peek on the Events reader's cursor (`Dirty::is_unread(Reader::Events, id)`, and an `unread` iterator) that doesn't advance it.
  2. Use it at both sites, so `take()` becomes output only.
  3. Optionally, skip reinstalling a brick whose rows, owner and name are unchanged.
- Keep recording `Take::Dirty` either way. It's cheap.
- Not blocking for this PR, provided the write-up is corrected and the bug is queued.

## Not blocking (fix in this PR if cheap, otherwise queue)

**Before recording is on by default:**
- **File writes run on the server's tick loop.** The digest, encode, zstd and `write` all happen synchronously (`replay.rs:1162-1194`, `net/src/replay.rs:179`), with no writer thread. A disk stall (antivirus, slow or network disk) would freeze the game for everyone. Fix: a bounded channel to a writer thread, dropping the recording on overflow.
- **Retention counts files, not bytes** (`KEPT_RECORDINGS`, `net/src/replay.rs:343`), and only prunes when a recording starts. One `bri-server --record` run is a single file of about 2.6 MB/min, so about 3.7 GB per day, on the same disk as the saves. Needs a size cap or rotation.
- **Redaction replaces substrings** (`replay.rs:601-607, 631`), including wrong login guesses. A password like "admin" or "1234" rewrites unrelated reply and error text, so a replay reports a false "differs". Redact only the fields typed as `Secret`.

**Correctness:**
- The tape stays installed after recording stops on an error (`replay.rs:970-981, 1016-1023`; `start` at `947-951`). Reads keep piling up: one copy-store poll per tick. Call `set_tape(None)`.
- After a fault, a replay falls back to live reads (`replay.rs:276`). A divergent replay of `/botsave` then really writes to the path stored in the file, which could be untrusted. Replays should never run side-effecting reads live.
- Change Map's bot-overrides read happens on the loader thread outside the tape. After `/botsave` and then a map change, the replay reports a false "differs".
- `AddOnData::save` (`session/packages/host_data.rs:115`) isn't routed. That's moot today, since no host calls it.
- The writer allows frames up to `u32::MAX`, while the reader refuses anything over 256 MiB (`replay.rs:845` vs `887`). A huge build load gets written but can't be replayed. The reader also allocates from untrusted lengths.
- Admin state, time scale, environment, weapon settings, the pending event queue and RNG seeds aren't in any digest part. A difference there is caught only when it reaches a part that is digested.
- If `prune` sorts by millisecond name and the clock goes backwards, it can delete the newest file. Two recordings started in the same millisecond truncate each other.
- `bri-server --record` fails to start if old recordings can't be pruned (`bin/bri-server.rs:103`). The client degrades gracefully instead.

**Secrets:**
- No real password is written.
  - Stand-ins are sequential labels, not hashes, so there's nothing to crack (`replay.rs:570-580`).
  - Admin, super-admin, login, set-password and host-password paths are all covered.
  - The join password never reaches the session.
  - IPs, resume tokens, private keys and the persist payload aren't recorded.
- Still in the file, which matters if recordings are shared for bug reports:
  - chat as typed, so a password typed into chat is recorded, and "the file holds no password" is overstated;
  - identity public keys, plus the ban and auto-role lists;
  - player names;
  - Add-On setting values;
  - the absolute bot-overrides path, which on Windows includes the user name (`net/src/replay.rs:107`, `client/src/app/session.rs:584`);
  - panic messages unredacted (`replay.rs:1000, 1170`).

**Cost:**
- **Off:** zero. Every recorder method short-circuits, and `outside` with no tape calls straight through.
- **On:** about 55 µs a tick (0.7 % of a core), which is acceptable. Per tick, `Digester::parts` rebuilds about ten view vectors (`replay.rs:781-806`). Per call, `redact_call` clones the whole `Call`, including a command's saved build (`replay.rs:584`).
- About five diagnostic `Take` frames are written on every 120 Hz ticker wake, even when no step happened (`net/src/server.rs:1645-1655`). They have no gameplay effect and could be left out.

**API and shared files:**
- The Session public signatures are unchanged, and the new items are additive.
- `HostSetup` gains a required public field `bot_overrides`, which breaks struct literals; all in-repo literals were updated.
- `HostSetup::session` and the client's map loading were split. Behaviour is the same, but the start world is now cloned, and the UI schema re-read, even with recording off.
- Acceptable, but it's not strictly "additive only".

**Tests that prove less than they say:**
- "Replays identically, twice" (`crates/sim/tests/replay.rs:346`) only shows the replay agrees with itself in one process.
- The off-recorder test doesn't check that no tape or frames were produced.
- The cut-off test cuts uncompressed bytes. The real crash case, a truncated zstd stream, isn't tested.
- No test covers the tape-mismatch branches.
- The write-up's mutation check (starting bricks removed gives a difference at tick 0) isn't in the code.
- The divergence test (`:361`) can genuinely fail, which is good.
- Wanted: tests for B1 to B3 (a changed Add-On value, a changed drain, an empty or corrupt file), each of which must report a difference.

**Slop:**
- `live_kind` matches `type_name` strings (`replay.rs:298-311`), duplicating `Read::kind`. A rename silently degrades it. Use an associated const.
- `panic_message` duplicates `net/src/server.rs:1341`.
- Unused code:
  - `FrameWriter::into_inner`;
  - `Recorder::recording`;
  - `Recorder::redact`;
  - the always-empty `Replayer.secrets`;
  - the first half of `bot_tuning_state()`, which clones every kind (`session/bots/tuning.rs:64`).
- `docs/audits/platform-door-closers.md` marks (b) "Done". That should wait for B1 and B2.

## Checked and fine

- **Ordering:** each call frame is written after its call, Step comes before its Tick, map changes are recorded where the host adopts them, and panics are recorded and flushed before unwinding continues.
- **Determinism:**
  - Outside the tape, only diagnostics read the clock: `think_nanos`, the event watchdog and `script_time`.
  - Bot RNG is seeded per bot, and the hash maps that get iterated use a fixed hasher.
  - rapier runs with `enhanced-determinism`.
  - There are no env reads or threads at runtime.
- **Crash safety:** the stream is flushed after each full check, so at most about 1 s is lost on a kill. Power loss isn't covered, since there's no fsync.
- **Constants:** `COMPRESSION_LEVEL` 1 and `FULL_CHECK_TICKS` 120 are documented. `bri-replay` has clear exit codes, apart from B3.

---

# Re-review: head 912a5e0 (delta cc621a3..912a5e0)

This was read-only: nothing was built or run. I read the whole delta myself.

## Verdict: passes. All three blockers and the drain bug are closed. Ready for the PC gate.

## Blockers

**B1 (Add-Ons digest) is closed.**
- The fix: the Add-Ons part now digests `(package_entities(), package_store())` (`sim/src/replay.rs:820`).
  - `Store` is every namespace's values, hidden ones included, in `BTreeMap`s (`package-runtime/src/state.rs:79-88`). So it hashes the same way on every run.
  - Its size is capped by the existing store size budget, so the digest each tick is bounded.
- The test: `a_changed_add_on_value_is_caught` (`sim/tests/replay.rs`).
  - A probe Add-On keeps one hidden value. The test records `put("left")`, then replays with `put("right")`, and expects the `Add-Ons` part to differ.
  - Under the old digest both runs make one change, so the counter matches and the test fails. It's a real test.

**B2 (host drains) is closed.**
- The fix:
  - Every `take_*` the host acts on now records `Outcome::Replied(digest_of(&taken))`, and the replay compares it (`Take::run` returns the digest).
  - That covers dirty bricks, cues, notices, private notices, map change, admin disconnects and the disconnect message.
  - So if a replayed session wouldn't kick someone or change map, the take before the recorded Disconnect or MapChanged frame differs and is reported.
- The test: `a_changed_take_is_caught` removes one non-empty cues take, and the next take then differs.
  - Under the old code takes were always `Done`, so the test fails there. It's real.
  - It covers cues only, but map change and disconnects go through the same compare.

**B3 (empty or damaged file) is closed.**
- The reader's rules now:
  - **Cut short:** only `UnexpectedEof` from the stream, which is what a zstd stream cut mid-block gives, or a short last frame, counts as a cut.
  - **Damaged:** any other read error, an over-size length, or a frame that doesn't decode is an error that says "damaged".
- A file with no frames errors with "The recording holds no match" (`replay()`, `ticks > 0`).
- `bri-replay` exit codes:
  - 0: matched;
  - 1: differs;
  - 2: can't replay (damaged, empty);
  - 3: matches up to tick N, then the file ends. That's the message I asked for.
- The tests:
  - `an_empty_or_damaged_recording_is_an_error`. The empty half fails on the old code (it returned Ok, with 0 ticks).
  - The server test now cuts the real compressed file by one byte and expects "cut off, no divergence". That's the real crash case I said was missing.
- What the tests don't cover: the "damaged" frame is damaged in the raw stream, not inside zstd. From the code, a corrupt zstd block returns a read error other than EOF, so it's treated as damage, not a cut. This is inferred, not tested. Not blocking.

## The drain bug is fixed at the source

- `Dirty` gains `events_unread` (`session/dirty.rs:23-26`).
  - `insert` fills it.
  - Only the Events reader's `read` or `skip` clears it, and the event phase reads every step (`session.rs:2878`).
  - Replication's `take()` no longer touches anything gameplay reads.
- Both sites now use it:
  - `events.rs:556` uses `unread_by_events`;
  - `brick_events.rs:157` uses `events_unread()`.
- What changes in play: "edited recently" now means "since the last event phase" (1 tick), not "since the last network send" (up to 6 ticks). That's the intended meaning. A session nobody drains no longer grows the set.
- The new unit test `the_event_phase_unread_set_ignores_replication` checks that `take()` and another reader's read leave it alone.
- The write-up now calls this a bug, not "the drain cadence is input". That matches what I asked for.

## Moving the diagnostic takes to server.rs is safe for live play

- `server.rs:1639-1649` now calls the five takes straight on the session: event overload, event diagnostics, slow event ticks, Add-On problems and script time. They run in the same place, in the same order, at the same times as before. The only thing removed is the frame write.
- Gameplay never reads those five queues. I checked every use:
  - `events.over_limit`, `events.diagnostics` and `events.slow` are only added to, and only `take_*` reads them;
  - Add-On `untaken` is filled by `tell_package_problems` and capped at `MAX_UNHEARD`. The chat to admins uses `unheard` and `told`, which the take doesn't touch;
  - `script_time` is only added to.
- None of them is in a digest part. So leaving them out of the recording can't make a replay differ, and the live server's logs and perf overlay behave as before.

## The cheaper findings: fixed

- **Tape:** it stops when the recording does (`Mode::Stopped`).
- **No live reads on replay:** a replay never reads live. A missing read now abandons the call with `ReplayFault`, and `a_missing_outside_read_is_reported_not_read_live` tests that. So `/botsave` can't write files during a replay.
- **Bot dials on Change Map:** they are recorded in `MapChanged` and set on replay.
- **Writer:** it refuses frames the reader would.
- **Redaction:** only a whole secret value is replaced.
- **File names:** they're numbered, which is safe against the clock and uses `create_new`. `bri-server` degrades if it can't set up the recordings folder.
- **Small cleanups:**
  - `KIND` const instead of `type_name`;
  - one `panic_message`;
  - unused code removed;
  - `HostSetup` start world and palette cloned only when recording.

## Still open: v0.2.7, or before recording is on by default (the write-up queues them)

- Writing the file on its own thread, so a slow disk can't stall the tick.
- A size cap on recordings.
- Routing `AddOnData::save` through the tape.
- Digesting admin, time scale, environment and the pending event queue directly.
- fsync.
- Recording stays off by default, so none of these can reach a player in v0.2.6.
- Small:
  - `events_unread` keeps a second copy of what the Events cursor already knows. A peek on the cursor would avoid that, but the set is simpler and cheap. Fine as is.
  - Check that `docs/audits/platform-door-closers.md` (b) says "Done" only once this merges.
