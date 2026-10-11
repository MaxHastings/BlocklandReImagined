# Faster pull-request CI

Date: 2026-10-11. Max: "cut down on what tests we run so we dont move so
slow ... be efficient". Nothing is deleted, disabled or ignored.

## Where the time went (Windows CI, PRs #55 and #72)

A pull request took 65-82 minutes from push to green, on five runners:

- Compiling: 31 min in the check job, and 18-31 min again in each of the four
  test shards, because every runner compiled the whole workspace.
- 18 offscreen render targets on the runner's software GPU (WARP): about 95
  of the 128-155 test-binary minutes. mirrors alone took 15-22 min,
  lighting_environment 13, view_jitter 7-11, persistent_scene and
  dynamic_unlit 8-9.
- Everything else, the rest of the ~390 targets, took 18-21 min in total across
  the four shards. The endurance, large-build stress (10k and 100k bricks) and
  world-safety targets take 1.4-1.7 s each; the bot suites and app_soak 3-5 min.

## What changed

- `[[ci_nightly_target]]` in `tools/gate-known-failures.toml` lists those 18
  render targets (each 1.5 min or more on WARP). `gate.py --ci-test --tier pr`
  leaves them out and shards the rest evenly; `--tier full` runs everything.
- `ci.yml`: pull requests and pushes to main run the pr tier; a nightly
  schedule (07:00 UTC) and manual runs run the full tier. The local push gate
  is unchanged: it still runs every target, with `--include-ignored`, on a
  real GPU before anything reaches main.
- `ci.yml`: sccache (`mozilla-actions/sccache-action`, Actions cache backend)
  wraps rustc in every job, so a workspace library crate compiled by one
  runner or an earlier run is reused instead of compiled again by each of
  the five.
- `gate.py --ci-test --shard K/N` lists the test targets from
  `cargo metadata` (no compile), picks its share, and builds only those
  with `cargo test --workspace --no-run --lib/--bin/--test ...`. Before, all
  four shards compiled and linked all ~390 test binaries. A shard fails if a
  target assigned to it was not built, so nothing drops out silently. The
  check job still builds every target, nightly ones included.

## Evidence

- `python -m unittest discover -s tools -p "test_*.py"` passes, including new
  tests for the tier filter and the nightly list.
- First run of #83 (cold compile cache), all green. Per job: check 37 min
  (build 30, clippy 5); shards 28-47 min, of which compiling 17-27 and
  tests 11-18 (before: 14-50). The four shards together ran about 57 test
  minutes, down from 128-155. Push to green took two hours, almost all of
  it waiting for free Windows runners while other PRs ran.
- sccache's end-of-job stats read zero on the shards: its server stops after
  ten idle minutes, and the shards test for longer than that.
  `SCCACHE_IDLE_TIMEOUT=0` keeps it up so the second run reports hits.
- Second run of #83 (warm caches), all green: check 25 min (build 21,
  clippy 2); shards 27-46 min, of which compiling 17-27 and tests 9-18.
  sccache hit all 34 library crates, but 440 of the 474 compiler calls per
  shard were test or binary executables, which sccache cannot cache
  ("crate-type"), so the shards' compile barely moved. That led to the
  per-shard build above. The labels `cargo metadata` gives match the 386
  targets the full build ran, one for one (checked against the four shards'
  logs plus the skipped and nightly lists).

## Duplicate candidates (not removed)

None found that are clearly redundant: the slow targets render different
scenes. Revisit if a nightly run shows two targets failing together on every
change.
