# 2026-10-11 Perf waste fix: client collision mirror refreshed only when a body moved

Finding 7 of the v0.2.10 performance waste audit (report at
`perf/waste-audit-v0.2.10.md` in the project files): the client ran a Rapier
step over its whole collision mirror every render frame.

## What was wrong

`Motion::observe` runs every frame and handed the predictor the newest
remote poses through `Predictor::set_others`, which restored every remote
body (a fresh `SharedShape` per body per frame through `Player::restore`)
and then always called `bri_physics::detect_collisions`: a full
`PhysicsWorld::step` at dt 1e-6 over every chunk collider of the build, plus
a snapshot and restore of every dynamic body. Remote poses change at most at
the 40 Hz pose rate and never with nobody else there, so at 165 Hz that was
a step for nothing on roughly three frames in four, and on every frame in
single player.

## Fix

- `Predictor::set_others` compares each incoming state's feet, crouch, yaw
  (a mount's pose uses it) and tuning with the body it already holds and
  leaves an unchanged body alone. It refreshes the collision world only when
  a body was attached, removed or moved; `others_refreshes()` counts those
  refreshes.
- `Player::restore` replaces the collider shape only when the tuning or the
  crouch flag changed. The motor already swaps the shape when the crouch
  flag changes, so the shape always matches those two.
- The client's `Motion::observe` is unchanged: the gate sits in the
  predictor so the prediction tests cover it.

Correctness: a moved, added or removed body still refreshes before the next
local motor tick, so the walker is still stopped by another body exactly as
before (`prediction_bumps_into_other_players_like_the_host` and the new
test's walk both pass). No protocol change. The Torque tick phase and
velocity of a remote are not compared because the mirror never reads them
(the host's idle player changes `tick.phase` every tick, which is what first
made the gate miss).

## Evidence

- Work count: `cargo test -p bri-sim --test prediction
  unchanged_others_do_not_refresh_the_mirror` (ten observes of the same
  state refresh once; an empty set refreshes never).
- Timing probe, `cargo test --release -p bri-sim --test prediction
  others_refresh_timing -- --ignored --nocapture`, 44k-plate mirror,
  `set_others` per frame with unchanged poses, median of 200 (cloud box):
  before 2.5 us alone and 7.2 us with 8 others per frame, after 0.0 us and
  0.8 us (one refresh in 200 frames). Honest reading: Rapier's broad phase
  is incremental, so the per-frame step the audit flagged cost microseconds
  on this box, not the milliseconds the rate suggested; the fix is kept
  because it is small, tested and removes the step the client never
  needed, but it is not a frame-time win on its own.
- `cargo test -p bri-sim --test prediction`, `cargo test -p bri-motor`,
  `cargo clippy -p bri-sim -p bri-motor --tests -- -D warnings`: pass.

## Next

PC numbers on the v0.2.10 baseline (3440x1440 and 2560x1440 Bedroom and
Afghan DM runs) are recorded in the audit report's status column.
