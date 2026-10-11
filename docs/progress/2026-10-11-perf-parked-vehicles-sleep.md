# 2026-10-11 Perf waste fix: parked vehicles sleep

Finding 11 of the v0.2.10 performance waste audit (report at
`perf/waste-audit-v0.2.10.md` in the project files): a vehicle nobody drove
never slept, so every parked spawn cost its wheel rays, suspension, drag and
a turret collider update on every 120 Hz tick.

## What was wrong

Every force the vehicle step applied (`add_force`, `add_force_at_point`,
`add_torque`, `set_linvel`, `set_angvel` in `crates/vehicles/src/world.rs`
and `world/tires.rs`) passed `wake_up = true`. In Rapier that is a strong
wake, which resets the body's sleep timer, so a parked car's own spring
forces kept it in the active set forever. The step also re-posed the turret
collider every tick whether or not the aim changed. (The audit's note on the
zero hull-friction force was wrong in detail: Rapier ignores a zero force;
it is the nonzero spring and support forces that kept the body awake.)

## Fix

- All forces in the vehicle step are applied without waking. Rapier then
  puts a settled vehicle to sleep by its usual threshold and timer (0.05
  units/s, 0.5 s).
- A sleeping vehicle skips the hover ray, hull friction, forces, wheel rays
  and tyre pass. The turret collider removal and re-pose, the weapon step
  and the energy bookkeeping still run.
- A driver aboard or a jet press wakes the body explicitly at the start of
  its step. Hits (`apply_impulse`, wake true), teleports and explosion
  pushes (`set_position`/`set_linvel`, wake true), a kinematic player
  walking into it (Rapier's island propagation) and a brick removed from
  under it (`parking::wake_resting`) wake it as any resting body.
- The turret collider is re-posed only when the gunner's aim changed.
- Work counters `wheel_rays()` and `resting_skips()` on `VehiclesWorld`;
  the checkpoint carries them across a restore.

Correctness: the prediction copy runs the same code, so it sleeps on the
same ticks as the host for the same inputs; checkpoints already stored and
restored the sleeping flag. A sleeping vehicle does not drift (the test
checks its position is unchanged to 1e-3 over 2 s).

## Evidence

- Work count: `cargo test -p bri-vehicles --test parked` (car, tank, flying
  car and carpet: at least 200 of 240 ticks skipped after settling, at most
  40 ticks of wheel rays, no drift, and a driver with throttle moves it more
  than a unit in 2 s).
- Timing probe, `cargo test --release -p bri-vehicles --test parked
  parked_timing -- --ignored --nocapture`, 10 parked cars, per tick, median
  of 600 (cloud box):
  10 parked cars per tick, before: vehicle pre_step 9.0 us + physics step
  and post 6.9 us (no skips, every wheel ray cast); after: 1.0 us + 2.0 us
  (10,390 resting skips in 1,039 ticks; wheel rays only while settling).
  Honest reading: about 1.3 us per parked vehicle per tick, so a dozen
  parked spawns cost ~2 ms a second of tick time before; the win is the
  Rapier island staying asleep (and the turret collider left alone), not
  a frame-time number Max will see.
- `cargo test -p bri-vehicles`, `cargo clippy -p bri-vehicles --tests -- -D
  warnings`: pass.

## Next

PC numbers on the v0.2.10 baseline are recorded in the audit report's
status column; `bot_combined_perf` with vehicle spawns is the host-side
check.
