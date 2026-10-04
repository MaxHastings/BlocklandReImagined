# 2026-10-03 Real-content event-fuzz fixture repair

Root assigned the candidate gate failure in
`bri-chaos/event_fuzz::random_v20_event_programs_keep_the_host_stepping`
after candidate67997ca4 failed both the full gate and isolated retry. The
focused content reproduction failed deterministically at seed0: the test
expected24 loaded field bricks and observed0 before randomized events ran.
Log: `/tmp/bri-v021-event-fuzz-content-repro.log`.

The harness always authored `fixture::PLATE`, which is the synthetic
`test/brick/plate` ID. The real-content fixture loads actual v20 package
definitions and does not install that synthetic ID. LoadBuild correctly
preserves unknown-definition bricks unloaded; this was a pre-existing fixture
mismatch, not an NPC planner, scheduler or permission regression.

The narrow repair in `crates/chaos/tests/event_fuzz.rs` selects an installed
ordinary filled one-stud, one-plate definition using footprint, height,
special/bot/link/reflection exclusion and exact single-box collision geometry.
Both synthetic and real-content fixtures therefore use the same physical
field geometry. Missing geometry still fails explicitly. No fallback content
is injected and no runtime behavior changes.

The exact24 loaded-brick assertion, field layout, randomized catalog/input/
output/parameter selection, delays, relay stress, deterministic repetition,
event-work limits and finite replicated-state checks are unchanged.

Validation:

- Before: `BRI_CONTENT=/Users/maxhastings/Documents/BlockReImagined/content
  cargo test -p bri-chaos --test event_fuzz
  random_v20_event_programs_keep_the_host_stepping --locked -- --ignored
  --nocapture` failed seed0,0/24 in3.59s.
- After: `BRI_CONTENT=/Users/maxhastings/Documents/BlockReImagined/content
  cargo test -p bri-chaos --test event_fuzz --locked -- --include-ignored
  --nocapture` passed all4,0ignored in14.91s:
  `/tmp/bri-v021-event-fuzz-fixed.log`.
- `cargo clippy -p bri-chaos --test event_fuzz --locked -- -D warnings`
  passed in9.01s: `/tmp/bri-v021-event-fuzz-clippy.log`.
- `cargo fmt --all -- --check` and `git diff --check` pass. Root reruns the
  candidate gate. No commit/push or manifest/content changes were made by
  this lane.

This repairs the verification harness. It does not establish a causal repair
for the separate original firefight crash or long Windows/AMC hangs.
