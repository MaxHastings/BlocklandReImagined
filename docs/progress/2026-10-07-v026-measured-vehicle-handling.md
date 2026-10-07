# 2026-10-07 Bots drive by measured vehicle handling

Locomotion lane, branch `claude/v0.2.6-measured-moves-x42j1c`. Design:
`docs/plans/v0.2.6-design-locomotion.md` item 1, reviewed in
`review-design-locomotion.md`.

## What changed

- `reach::Handling` measures a wheeled chassis on the real
  `VehiclesWorld` at the driver's throttle (`route::DRIVE_THROTTLE`, 0.8):
  a straight run to top speed and a brake to a stop (stopping distance by
  speed), and turns held at steady speeds at full, half and quarter lock
  (the fastest it holds each curvature). Kept per pack fingerprint,
  definition id and scale; a reloaded pack is measured again.
- It replaces the guessed stopping formula, the arrival-speed formula,
  `CORNERING` and `route::Chassis` with its wheelbase radius and fallback;
  they are deleted.
- The driver's hazard check sweeps the hull and the ally corridor along
  the arc the chassis is turning on, out to its measured stopping
  distance, in chords no more than `ARC_STEP` apart.
- One measurement drives at most `BUDGET_SECONDS` (120 s simulated) across
  all its runs; past it, it keeps the turns it read.

## Evidence

- Test car: tightest radius 5.26 measured (the wheelbase formula said
  3.1), top speed 29.6, 6,978 ticks to measure (about half the cap).
- `cargo test --release -p bri-sim --lib reach::` 10/10, `route::` 12/12.
- `cargo test --release -p bri-chaos --test bot_interactions` 19/19;
  `bot_routes` driver tests 3/3; `bri-vehicles` lib passes.
- clippy `-D warnings` clean on bri-sim, bri-chaos, bri-vehicles.

## Decisions

- The mounted-driver test's 10% reverse-share band became an absolute:
  each stretch driven backwards swings the nose toward the target. With
  its true turning circle the rover backs round a target abeam of it once
  (nose from 0 to 43 degrees toward it), which the band read as chasing in
  reverse.

## Not done

- Chaos test "a driver mid-turn brakes for an ally on its arc": needs a
  controlled turn, not a pursuit. One try passed with the arc sweep
  switched off; the other placed the ally beside the target the driver was
  closing on. Goes with the warm-up item.
- Warm-up at bot join and the body `Reach` cost cap: the warm-up item.
