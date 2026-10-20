> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-load-flakes.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Load-flaky test fixes: 0a3bcc8f (claude/fix-mirror-recursion-dv7x1g)

Main is at 82c198b, so 0a3bcc8f is the only new commit.

**Verdict: PASS after one must-fix (Stress Lab press cap). Merge the sha that
adds it.**

## (a) Same claims, same strength?
- **bot_think_time_16:** same 15000 us bar. What it measures is now CPU time
  instead of wall time, which is the right measure for "what thinking costs".
  CPU time is never more than wall time, so on an idle machine this is no
  tighter than before.
  - The lane's own numbers show a 1.1 ms gap even alone (4331 us CPU against
    5483 us wall). Unexplained, but outside step_bots' own work, which is
    what the bar is about. Not blocking.
- **night_qa brick hand:** 120 s and 10 s used to be wall-clock hang guards;
  they are now game time, plus a progress-judged load. These were never
  speed claims. The click-places-ghost and trail/puff asserts are unchanged,
  and merging the two copied setups into one is good.
- **app_item_render:** the 45 s budget was a wait, not an assert. The frame
  comparison is unchanged. OK.
- **82c198b** (on main) delists three known failures. Strictly tighter. Good.
- **stresslab_flow: LOOSENED. MUST FIX.**
  - The old test passed if one of 6 presses mined.
  - The new one presses every time the last press has been answered, for up
    to 30 s of game time. That is up to about 300 presses (12-tick cooldown,
    120 Hz).
  - A regression where mining needs 20 tries would pass.
  - Fix: keep the waits, add `ensure!(presses <= 6, ...)` (the old count), or
    tighter if the run shows 1-2.

## (b) CPU-time measure
- **Linux:** CLOCK_THREAD_CPUTIME_ID is correct per thread. Mac supports it
  too (10.12+).
- **Windows:** GetThreadTimes charges whole clock ticks (about 15.6 ms).
  Unbiased over a run, though: 30 s × 120 ticks × ~4.3 ms is about 1000
  clock ticks, so sampling error is around 3%. The code is moved unchanged
  from hardening_packages, which already ran it on Max's PC.
- **Counts only the thinking:** step_bots is single-threaded (no rayon,
  scoped threads or workers in session/bots; the only sim thread is
  drop_later). So the ticking thread's CPU time inside step_bots is all of
  the bots' thinking.
- **Nit:** cpu_time::thread() now runs every server tick in the shipped game.
  Its `assert!` on a failed syscall would panic the server for a diagnostic.
  Return Duration::ZERO instead. Effectively unreachable, so not blocking.

## (c) Waits can't hang or hide slowness
- **Bounded:**
  - `until` fails after STALL (300 s) of compiling, of no progress, or of
    nothing changed.
  - `until_done` fails after STALL of wall time from start.
  - `scene_pipelines_ready` is true without a GPU (probes.rs:8), so headless
    tests aren't paused.
  - A GPU restart pauses the clock, but STALL still bounds it.
- **Masking:** a compile under 300 s no longer shows in these tests. It stays
  visible to players: the loading screen now says COMPILING SHADERS (Slate
  fix). The pipeline-cache/first-compile work is already on the v0.2.7 list.
- **Cheap improvement:** have the wait print how long the compile took, so a
  2-minute compile shows in gate logs. Not blocking.

## (d) Stress Lab first-press miss
Not shown to be a game bug.
- The old test pressed before checking the player was standing.
- The note's "tick 720: two presses, no block" fits a falling or not-yet-
  standing player whose aim met no ground in reach. The new test waits to
  stand first.
- Check: the run prints "mined after N presses".
  - N = 1 means there's no bug. Cap at 1-2.
  - N ≥ 2 while standing: the server read the press before the look arrived.
    A player who flicks down and presses in the same tick would hit that
    (Stress Lab is an Add-On, though). Note it for v0.2.7, cap presses at 2,
    and name it in the assert message.

---
# Re-review: 3b39d264 (covers 7a8af5ec..3b39d264)

**Verdict: PASS. Merge 3b39d264c692d7ae7cd2741a58b5f318e1ebf524.**

## Stress Lab
The test now waits until the server shows the player standing (not just the
local prediction), presses once, and requires that one press to mine. That is
stricter than the old "one of six presses", and it answers (d): the first
press mines, so this was not a game bug.

## CPU time read failing as zero
Can it silently pass a perf test? In theory, yes.
- If the read *after* the bots' step fails, `saturating_sub` gives 0, so that
  tick's thinking counts as nothing. If every such read failed, think time
  would be 0 and the bar would pass.
- If the read *before* fails, the whole thread's CPU so far is counted, an
  over-count that fails loudly.
- `hardening_packages`' `timed()` has the same property.

In practice it can't happen. Both calls only fail on bad arguments, and these
are fixed: the current thread's pseudo-handle on Windows, and
CLOCK_THREAD_CPUTIME_ID on Linux and macOS 10.12+. Zero is the right choice
for the shipped server. Not blocking.

Cheap guard, fold in whenever bot_gauntlet is next touched:
`assert!(think > 0.0, "no bot think time measured")` in `bot_think_time_16`.
Sixteen bots fighting for 30 s cannot cost zero CPU, so this is a sanity
absolute.
