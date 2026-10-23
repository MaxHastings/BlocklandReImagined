# 2026-10-22 Short mover motions must replicate

The fresh private map capture disconnects with `Invalid map mover state` before
writing a frame (`capture-terrain-identity`, 69.67s). Its mission uses the existing
supported zero-second mover operation for placed-object visibility. The Session
consumer alone passed because it does not reconstruct client replicas.

`MoverState::go_to` floors duration at 0.005 seconds and advances at
1 / (duration * 120), so supported motions below a simulation tick have a rate
above 1 (maximum approximately 1.667). Replica validation independently capped
rate at 1, rejecting even settled snapshots that retain the original rate.

The network bound now uses the mover's shared maximum derived from its existing
duration floor. Host timing and movement are unchanged. Finite/negative/key/count
checks remain; rates above the supported maximum still fail. One protocol-change
entry records the expanded accepted bound without changing field layout.

The real codec/replica regression initially fails on a host-produced state.
It exercises zero, below-floor, minimum, sub-tick and ordinary durations through
checkpoint and delta replication, compares fractional-tick progress, and checks
negative/nonfinite and one-ULP-over-maximum rates are refused without partial
replica mutation. Baseline log: `short-mover-replication-baseline.log`; corrected
targeted validation passes (47.03s compile, 0.01s test). Combined quick checks
pass all ten steps in 71s (`owned-and-mover-quick.log`). Focused checks, the
integrated gate and Windows CI remain required before landing. The replacement
ordinary private offscreen capture passes with zero script-health problems
(temporary state 43020), reaching deployment and a rendered frame without the
mover-state disconnect. Combined focused checks pass eight steps in 1126s:
Clippy with warnings denied on five changed crates, integration tests of 28
crates, and client/render unit tests. Log:
`.research/pipeline-integration/owned-and-mover-focused.log`.

The map's terrain identity correction remains private conversion work. No AA
assets, rules or source-game-specific branches enter this engine correction.
