# 2026-10-07 Gate speed: what moved the test pass, and what did not

Max asked for builds and tests of ten minutes or less each. This entry
records the gate changes that landed for v0.2.6 and the measurements behind
them; the rest is v0.2.7 work.

## What landed

- 6b24b73b, 6ca04c78: test binaries start slowest first, from
  `../.bri-gate/gate-timings.json` (beside the gate's target dir, so wiping
  `target/` keeps it); every logical CPU runs tests (16 here, `--jobs N`
  overrides); clippy runs alongside the test pass into its own log; a phase
  summary ends each run and warns past ten minutes of tests. A test process
  that exits non-zero without naming a failed test (a stack overflow, a
  driver crash, an out-of-memory kill) now fails the gate as
  `<binary>::process_exit`, as a hang fails it as `::gate_timeout`. The
  content check installs the default Add-Ons into the main checkout's
  `content/addons`, so a stale copy cannot fail a content test.
- 5c90e2b7 took back the split of long binaries into several processes by
  test name: only bot_soccer_match ever crossed its share, its longest test
  still set its time, and the test pass did not move (972 s before, 982 s
  with it).
- 0a3bcc8f, 3b39d264, 8b2e3398, 43ce22e9 (the load-flake lane): deadline
  tests count work, not wall time. The fd719c94 gate ran at 16 jobs with no
  retry at all.

## Measurements (PC gate, 16 jobs)

| gate | build | clippy (alongside) | tests | retries |
| --- | --- | --- | --- | --- |
| 6ca04c78 (first 16-job run) | 81 s | 148 s | 972 s | 37 s |
| 82c198bd | 267 s | 270 s | 982 s | 20 s |
| ab8f7f02 (static C runtime, cold) | 1024 s | 562 s | 1077 s | 70 s |
| d068f06e | 185 s | 247 s | 1062 s | 119 s |
| fd719c94 | 212 s | 178 s | 988 s | 0 s |

Peak RAM at 16 jobs: 64.2 GB of 127 GB (6ca04c78), 42.6 GB (423f4d22).

The test pass is held by the heavy pool: 104 bri-client and bri-render
binaries, about 3200 s of work, run three at a time (`HEAVY_JOBS`), about
1060 s a slot, while the 13 light slots carry about 350 s each.

One gate at `HEAVY_JOBS = 6` (423f4d22: fd719c94, plant limit 638e2ae, the
bot stack 1400e814) ran tests in 820 s, but GPU binaries slowed under the
shared GPU (map_lighting 565 s against 92 s at three; bot_combined_perf
562 s, bot_battle_perf 560 s) and bot_soccer_match passed the 600 s
per-binary limit (601 s; 449-472 s in the gates before). Its
`two_against_two_play_a_clean_match_across_seeds` alone, on the same build,
takes 280 s; in the gate it shares one process with three other long
matches. Other builds were running on the PC during that gate, so it is not
a clean measurement. `HEAVY_JOBS` stays 3 for v0.2.6.

## v0.2.7

- Cheaper heavy tests: the soccer matches (four long matches in one
  process, 449-601 s), app_soak (423-809 s), bot_combined_perf and
  bot_battle_perf (about 450-560 s each) set the floor of the test pass
  whatever the pool sizes.
- A faster gate: measure `HEAVY_JOBS` 4-6 on an otherwise idle PC once the
  heavy tests are cheaper.
