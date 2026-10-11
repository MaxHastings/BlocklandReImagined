# Tyre stretch overflow ("Invalid vehicle pose")

Date: 2026-10-11. Lane: engine hardening (v0.2.10).

## Symptom

`bri-chaos/net_chaos` failed on Windows CI for #81: a client was dropped
with "Invalid vehicle pose" after ResetVehicles, LoadBuild, RespawnVehicle
and Respawn. Thirty replays of that seed passed locally, because the network
test's interleaving depends on timing.

## Cause

`session_chaos` (deterministic, no network) with 400 seeds reproduced it:
seed `0x1daa66d2b0` fails at tick 973 with
`wheel_tire.0.longitudinal = NaN`. The tank's pose showed one wheel's
tyre at `longitudinal: inf, lateral: 4.7e35` while spinning at 290 rad/s.

`tires::update` steps the tyre stretch the way Torque does, explicitly:
`stretch -= relaxation * |spin| * stretch * dt`. Once
`relaxation * |spin| * dt` passes 2, each step overshoots zero by more than
the stretch it started from, so a wheel left spinning fast (in the air after
a tumble or a respawn) grows its stretch geometrically to infinity within a
few ticks. The next pose carries it to every client, whose replica rightly
refuses a non-finite pose and drops the connection.

## Fix

`relax_rate` caps the relaxation rate at `1 / dt`: a step relaxes the
stretch at most to zero. Below the cap the arithmetic is Torque's exactly,
which covers every ordinary drive; the existing Torque-circle and replay
tests pass unchanged.

## Evidence

- `a_fast_spinning_wheel_in_the_air_keeps_its_tyre_stretch_finite` (new, in
  `crates/vehicles/src/world/tires.rs`) fails without the cap and passes with it.
- `BRI_CHAOS_SEED=0x1daa66d2b0 session_chaos synthetic` failed before the fix
  and passes after.
- `cargo test -p bri-vehicles`, `cargo clippy -p bri-vehicles --all-targets
  -D warnings` and `cargo fmt --check` are clean.
- `BRI_CHAOS_SEEDS=400 session_chaos synthetic`: failed (seed 0x1daa66d2b0)
  before the fix; all 400 pass after it (1224 s, debug build).
