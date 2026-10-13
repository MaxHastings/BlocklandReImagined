# 2026-10-13 W8: Warthog-like handling and three players in one vehicle

Branch `claude/feel-vehicles-decals-ccqlxz`, the first W8 slice (vehicles).
The audit, measurements and decisions are in
`docs/audits/vehicle-handling.md`; the new fields are documented in
`docs/modding/content-kinds.md`.

## What changed

- `vehicles.json` gains `engine_curve`, `wheels[].rebound_damping`,
  `gravity_scale`, `steering.toward_look` and `steering.speed_lock`. Each
  defaults to v20's behaviour; imported vehicles drive bit for bit as before
  (the tyre code keeps its operation order so the floats match).
- A `toward_look` vehicle steers toward the driver's free look
  (`MoveInput::head_yaw`, carried to `Controls::look_yaw`); the client holds
  the driver's look fixed in the world while the hull turns under it.
  One protocol change file (`vehicle-look-steering.md`).
- The Playground buggy is retuned Warthog-like through its JSON alone
  (`tools/make_playground_assets.py`): heavier, four-wheel drive and
  steering, long soft springs, looser rear, look steering, 0.85 gravity.
- Bots steer a look-steered vehicle by looking, and the handling a bot plans
  with (`reach::Handling`) is measured the same way; before, a bot at the
  wheel never turned it.
- Three players over a bad link: movement datagrams repeat 12 moves (was 6;
  `movement-redundancy-twelve.md`), the host's seated queue keeps a spare of
  4 (was 2), and a gunner's look is turned into a turret angle against the
  hull's heading after the step.

## Evidence

- `cargo test -p bri-vehicles --test handling`: 8 pass.
- `cargo test -p bri-sim --test playground`: 10 pass, including the new
  `a_bot_drives_the_look_steered_buggy_at_a_runner`. The merged
  `a_buggy_runs_a_stomper_over` now waits out the Stomper's 2.5 s of spawn
  protection: the heavier buggy reached it inside that window, and later
  launches were credited to the Stomper's own bump.
- `cargo test -p bri-net --test playground`: passes, steering by look.
- `cargo test -p bri-client --lib crew_link`: driver 2 corrections (worst
  0.16 units) where it had 6 (worst 0.43); riders 2 of 725 frames past the
  newest pose, 0.024 units off the host; the host's gun 0.0006 rad off the
  gunner's move (was 0.020). Five more link seeds: 0 to 3 corrections, all
  under 0.24 units.
- `python tools/check.py quick`: green for bri-net, bri-sim, bri-vehicles,
  bri-vehicles-import. bri-client's six window/GPU unit tests fail in this
  container (no wgpu adapter), identically without these changes.
- Clippy `-D warnings` clean on every changed crate.

## Not done here

- The boarding bounce in the interop matrix did not reproduce headless.
- Engine sound, per-body tuning gaps, sound buses and impact decals are the
  next W8 slices.
