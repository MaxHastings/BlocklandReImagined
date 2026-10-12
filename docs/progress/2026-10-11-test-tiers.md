# 2026-10-11 Test tiers: quick, focused, full

Brief P0-05 asked that quick checks run fast and the full gate stay
complete. Until now there were two choices, nothing or the whole
`tools/gate.py` (about twenty minutes on Maxwell's PC, forty on a busy
day), and the audit (ask 24) recorded the tiers as not built.

## What changed

- `tools/check.py` with four commands: `quick` (`cargo fmt --check` and
  unit tests of each changed crate), `focused` (adds clippy `-D warnings`
  for the changed crates and every integration test that needs no window or
  GPU for them and their dependents; `--depth N`, `--tests crate:target` and
  `--crates` narrow or widen it), `full` (delegates to `tools/gate.py`,
  `--push` included) and `soak` (the `stress_`, `endurance_` and `soak_`
  labelled long cases of named crates, in release). `plan <tier>` prints
  the commands without running them.
- Changed crates come from `git diff` against the merge base with
  `origin/main` plus uncommitted and new files; a workspace-level file
  (`Cargo.toml`, `Cargo.lock`, the toolchain pin, `clippy.toml`) changes every
  crate, documentation alone changes none.
- Every cargo call names one crate, so the Windows command-line limit that
  made `tools/check_format.py` loop per crate does not apply.
- `docs/testing-tiers.md` states which tier to use when, the naming labels
  for long tests, and the rule that a tier skips work you did not touch and
  never a test. `AGENTS.md` points to it.

## What did not change

`tools/gate.py` and `tools/gate-known-failures.toml` are untouched: the gate
still builds everything, runs clippy, `bri-client --check` and every test
with `--include-ignored`, and nothing in the tiers retries, skips or hides a
failure. `bri-client` and `bri-render` integration tests run in tier 2 only
when named, and always in the gate.

## Evidence

`tools/test_check.py` (run by CI's "Tool tests" step) covers the change
mapping, nested crate folders that share a prefix (`client` and
`client-sandbox`), workspace files, depth-limited dependents, the plans of
each tier and the one-crate-per-call rule. Timings of the tiers on the PC
are not measured here; `check.py` prints each run's slowest steps so the
first real numbers come from the next few uses.

## Next

Measure `quick` and `focused` on the PC against a warm `target/` and, if
tier 1 is over two minutes for a core crate, move its slowest unit tests
behind the `stress_`/`soak_` labels. The Flaky CI tests thread owns the
runner timeouts; this entry does not touch them.
