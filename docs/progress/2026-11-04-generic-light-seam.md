# 2026-11-04 Generic light seam (driver input bits)

## What changed
- `driver_jump_held` is replaced by `driver_inputs: u8` on `VehicleSnapshot`,
  `VehiclePose` and the wire `VehicleView`. Bits live in
  `bri_vehicles::schema::driver_input`: `JUMP` (1), `BRAKE` (2), `REVERSE` (4).
  Protocol change: `crates/net/protocol-changes/vehicle-driver-inputs.md`.
- `JUMP` is the old `driver_jump_held` meaning, unchanged. Brake is not folded
  into it, so JumpHeld lamps behave exactly as on main.
- New `LampActivation::ReverseOrBrake` (tail lights): lit while the driver
  brakes or reverses. Content opts in with `"reverse_or_brake"`.
- The vehicle checkpoint stores `inputs: u8` (serde default) in place of
  `jump_held`.

## Not done (open)
- The generic light seam for non-vehicle objects. Engine point and spot lights
  already exist (`PointLight::cone`, map spot lights); activation is still a
  fixed enum, not a named signal. Next step, before anything else uses it.
- The PR 433 JumpHeld-on-cars change is not included. It alters default
  behaviour and needs a decision (see the review thread).

## Commands and evidence
- `cargo check --workspace --all-targets`: pass.
- `cargo fmt --all -- --check`: pass.
- `cargo test -p bri-vehicles -p bri-sim -p bri-net --lib`: 100 + 516 + 36 pass.
- `cargo test -p bri-net --tests`: all pass.
- `cargo test -p bri-client --lib`: see the run recorded in the thread.
