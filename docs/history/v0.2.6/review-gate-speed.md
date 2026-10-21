> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-gate-speed.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: gate speed (db87ecd1 on 498d7f58, claude/fix-gate-speed)

Verdict: **passes, with one must-fix and one ask.**
- Must-fix (small, in this code): a test process that crashes still passes the gate.
- Ask: show one measured gate run before claiming 10 minutes. The split helps much less than the note implies.

Merge the sha with the must-fix.

## The main point: what actually shortens the tests

- **Two of the three slowest binaries have one test each:** `app_soak` and `bot_combined_perf` (one `#[test]` each). `plan_units` caps parts at the number of tests, so they never split. The critical path stays at about 480 s.
- **libtest already runs a binary's tests in parallel threads.** There is no `RUST_TEST_THREADS` or `--test-threads` in the gate; only CI sets 1. Splitting a binary into processes therefore adds no parallelism, unless its tests serialize inside the process. `bot_soccer_match` (6 tests) takes about as long as its longest test either way (inferred).
- **What does help:**
  - longest-first, which starts the 480 s binaries at t = 0;
  - more workers;
  - clippy running alongside the tests.
  
  With those, tests take about max(480 s, total ÷ workers). That can fit 10 minutes without any split.
- **Ask:** one PC gate run with the phase summary, plus the list of binaries that split and their time before and after.
  - If no split binary got faster, delete `list_tests`, `split_evenly` and the split branch of `plan_units`. That's about 40 lines that buy nothing, and Max's "no slop" rule applies.
  - If one did, keep the split and name it in the note.

## (a) Same tests, same pass/fail meaning

- **Split coverage:** `--list` runs with the same args (`--include-ignored` plus every `--skip`). `split_evenly` deals out every name, `--exact` matches each name exactly, and each chunk keeps `--include-ignored` and the skips. The unit test checks the union. Nothing is lost.
- **Fallback:** if `--list` fails or returns nothing, or the command would be too long, the binary runs whole.
- **Known failures, skips, retries:** unchanged. Failures still come from parsing the log. Each split process writes the binary's own "Running" header, so the failure keys and the retry owner lookup (`header_target`) are the same.
- **Deadline:** `BINARY_TIMEOUT` now applies per process. That's the right meaning for a hang guard.
- **MUST-FIX (an old hole, but this PR multiplies processes):** `run_binaries` throws away each process's exit code. It did before too, since the old `ok` was never used. A process that aborts (stack overflow, a segfault in a GPU driver, a panic outside a test, an OOM kill) prints no `test x ... FAILED` line, so `parse_failures` finds nothing and **the gate passes**. Fix: a non-zero exit with no FAILED line from that process adds a `<target>::process_exit` failure. A few lines.

## (b) Is splitting safe

- **Temp dirs:** no test uses a fixed-name temp path (I searched for `temp_dir().join("...")` without an id). The tests that do use temp dirs add the pid and a counter.
- **Statics:** the one process-wide lock in the tests is `crates/client/tests/support/gpu.rs:13` (`static GPU: Mutex`). It shares one headless GPU inside a process. Split, each process makes its own device. That's safe, but each GPU process counts against the heavy pool, which still caps them at 3. The GPU-at-once limit is unchanged: 3 processes before, 3 now.
- **Ports:** port-bound targets never split and stay in the one-at-a-time pool. Good.
- `OnceCell` or `OnceLock` setup per process only repeats setup work in each process. That costs time, not correctness.

## (c) RAM and GPU headroom at 16 workers

- **GPU:** unchanged, at most 3 heavy processes.
- **CPU:** this is the real risk. There are 13 light workers, each a libtest process with up to 16 threads, plus clippy using every core, all next to tests that wait on wall-clock deadlines (the 600 s soccer match, offscreen app timeouts). That is more load than the old 8. The retry-alone step catches flakes, but a retry costs time.
- **RAM:** not measured. Each content-backed chaos process loads generated v20 content.
- **Ask:** in the measured run, record peak RAM. If retries go up or RAM gets tight, default `--jobs` lower (for example 12) rather than 16. Measure, don't guess.

## (d) First run with no timings file

- Every unit gets an unknown (infinite) time, so they run in listing order and nothing splits. That is the old behaviour with more workers.
- Every binary already finished under 600 s on the old gate, so the first run does too. There's no need to seed defaults.
- **Small:** the file lives in `target/`, which the size cap deletes whole (gate.py ~534). Keep it in the gate root (`../.bri-gate/gate-timings.json`), so a target wipe doesn't silently drop back to listing order.

## (e) Does the Add-On install refresh copies that differ? Yes

- `defaults::install` copies every default Add-On whose tree differs (`!same_tree`, then `replace_tree`, `crates/package/src/defaults.rs`), not only missing ones. A stale schema-3 `blockhead_bot` gets replaced.
- **Side effect to note:** `worktree/content` is a junction to the **main checkout's** `content/`. So the gate now writes the gated sha's default Add-Ons into Max's main checkout. That's needed, since the tests read that folder through BRI_CONTENT.
- It reverses the rule written in `install_when_asked`'s doc ("validating ... the shared main checkout's content never writes to it"). Update that doc line.
- A gate that fails leaves the branch's copies in place until the next gate or game start re-installs them, which heals itself.
- Two gates can't race, because they hold the lock.

## Smaller

- If clippy fails, the gate returns before reporting test failures. That's fine, but say "tests not reported" in that message.
- An old harmless oddity: a `gate_timeout` key's name is `label::gate_timeout`, not `gate_timeout`, so the "not retried" branch never fires. A retry then runs zero tests and still counts as failed, which is the right outcome by a roundabout route. Fix only if the code is touched again.
