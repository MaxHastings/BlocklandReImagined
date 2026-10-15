# v0.3.1 release candidate: CI retry policy

## Decision

Keep the renderer and network assertions intact, but make PR CI follow the
release gate's existing policy for named timeout failures: rerun each failed
test by itself once. A retry that passes is reported as flaky; persistent
failures, process crashes, and unnamed hangs continue to fail the check.

## Evidence

- Candidate PR #169, CI run `38690829918`, attempt 1: Windows build, Clippy,
  and shards 1–3 passed. Shard 0 failed only
  `bri-client/avatar_editor_preview::the_editor_portrait_plays_and_loops_the_authored_run::synthetic`
  after WARP readback timed out at 60 seconds.
- The isolated local preview retry passed in 8.49 seconds.
- The failed shard rerun (attempt 2) also reported
  `bri-net/sized_join::stress_sized_join_over_a_throttled_link` after progress
  paused for 3.035 seconds. The local focused run passed that test in 24.99
  seconds. Both failures therefore match the runner-contention cases the local
  gate already retries by test name.
- `python tools/check.py focused --crates bri-content,bri-render,bri-client,bri-addon-import,bri-vehicles,bri-vehicles-import,bri-chaos`
  completed all 42 steps successfully in 2126 seconds before this tooling
  change.

## Next

Run the tool regression tests, then require a new green Windows CI run on the
updated candidate before merging and starting the v0.3.1 release gate.
