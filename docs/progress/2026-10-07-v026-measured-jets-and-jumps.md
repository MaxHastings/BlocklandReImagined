# 2026-10-07 v0.2.6: measured jet legs and jump height

Branch `claude/v0.2.6-measured-moves-x42j1c`, off main 2cf0b3a4. First step
of "measured movement capabilities", agreed with Max in the v0.2.6 bots
thread: measure the actor, not the world; jets first, jump height second,
vehicles only once these prove the seam.

## What changed

- New `crates/sim/src/reach.rs`. `Reach::of(tuning)` runs the real player
  motor once per tuning on a bare test floor and shares the result:
  - `ledge`: the highest ledge a walk-and-jump (as the walk leg does it,
    `route::JUMP_TAKEOFF`) lands it on, bisected to 0.01.
  - `jets`: jet legs flown by the real controller (`route::jet`) over a grid
    of climbs and crossings 2 units apart, until one more step adds the
    same time as the last; seconds and jetting seconds, interpolated.
- `nav::Body::of`: `jump` is the measured ledge. Deleted `rise * 0.8`.
  Stock body: 3.50 (was 2.88).
- `route::Jets` keeps its role and `flight` interface (minus the apex
  argument, which was always the landing plus `JET_CLEARANCE`). Deleted
  `JET_RANGE`, `JET_CLIMB`, the idealised climb/cross/sink formula, the
  extra 0.5 s and the `seconds >= 0.5` rule. Energy stays a live limit:
  a leg flies when its energy holds the leg's measured jetting seconds.
- `route::jet_patience`: twice the measured time (was 2x plus 2 s on a
  formula's guess).
- Jet leg controller (`route::jet`), found by measuring it:
  - walks back onto the launch cell and stands still before lifting off
    (the parked showcase test failed because a running takeoff drifted
    under the deck it climbed beside);
  - lets go once it will coast to the crossing height (it overshot by up
    to 4 units);
  - climbs again whenever it sinks below the crossing height, and jets
    across only up to it (it fell to the floor from long crossings, and a
    low-gravity body climbed on as it crossed).
- `nav::Mode::Jet` carries its launch point; the launch column is checked
  for the full-width body (plus `TAKEOFF_TOLERANCE`), with corner rays as
  the cheap pre-check.
- `a_bot_jets_over_to_someone_above_it` passes; removed from
  `tools/gate-known-failures.toml`.

## Tests

- New mechanism tests in `reach.rs`: lower gravity jumps higher and climbs
  a tall leg on less energy; a stronger jump lands higher; stronger jets
  fly quicker on less energy; jets that cannot lift it give none; one
  measurement per tuning, shared.
- `route.rs` jet tests rewritten for measured legs (energy limits, stop
  before takeoff, climb when sinking).
- `cargo test -p bri-sim --tests`: all pass. Every `bri-chaos` test binary
  (36, run one at a time): all pass (the content-dependent ones stay
  ignored in the cloud).
- Cost (release, cloud): one stock tuning measures in about 90 ms (ledge
  about 10 ms, 70 jet legs about 80 ms), once per tuning per process.

## Not done / next

- Vehicles: measured stopping distance and turning arc (step 4 of the plan).
- The crossing speed cap and braking (`leg_push_speed`) are still fixed
  numbers inside the controller; measured times include them truthfully.
- Measuring runs the first time a tuning is asked for, inside a tick: a
  one-off hitch of about 90 ms when the first bot with a new tuning plans.
  Warming it when tunings load would hide it, but that is session code the
  replay thread owns; a background thread would make plans depend on
  timing, which replay cannot allow.
