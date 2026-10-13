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

## Retuned to the Warthog's tags

The project's Halo notes (`v0.3.0/assets/halo/FEEL-NOTES.md`, read from
Halo PC's `bloodgulch.map`) replaced the placeholders. Only numbers came
into the repo; the table is in the audit.

- New `vehicles.json` fields, each off by default: `wheels[].tire.lateral_grip`
  (grip across over grip along, an ellipse), `throttle_brakes` (pulling
  against the roll brakes, as Halo's deceleration does) and
  `reverse_limited` (backs up at `reverse_speed`).
- The buggy: 34 forward and 14.8 back, a flat engine curve pulling 10.2
  u/s², 30 degree lock, 0.62 travel, the weight 57% forward, Halo's grips
  along and across each axle times 4. Decision: grip times 4, not 5 or 6;
  higher stops nearer Halo's 0.8 s but loses the rear slide in a hard turn.
- With the weight forward on even springs the hull scraped the ground
  while weaving and the driver's prediction missed by up to 0.79 units
  (each scrape is a contact the replay cannot reproduce exactly). The
  springs now share the weight (10300 front, 7700 rear) and the hubs sit
  0.08 higher: no error at all over 10 s of weaving
  (`a_nose_heavy_buggy_weaving_on_the_ground_agrees_with_the_host`).
- `share_contacts` no longer sends a vehicle back into a player faster than
  it came: against a Stomper holding its ground the buggy left at over 300
  u/s. `a_buggy_pinned_against_someone_who_holds_their_ground_never_speeds_up`
  reproduces 390 and 470 u/s without the fix.
- An empty vehicle's run-over is no longer credited to its victim's own
  bump. `a_buggy_runs_a_stomper_over` now launches from the Stomper's far
  side: the Runner stood between it and the buggy.
- `a_bot_drives_the_look_steered_buggy_at_a_runner` asks for within 20 units
  (was 15): at 17 the bot now climbs to the gun.

Evidence after the retune: `bri-vehicles` all pass (handling 11);
`bri-sim` all pass; `crew_link` driver 2 corrections, worst 0.29 units (one
tick at 34 u/s), riders 0.020 units off, gun 0.0007 rad off the move;
`bri-client --lib` only the six no-GPU failures.

## Not done here

- The boarding bounce in the interop matrix did not reproduce headless.
- Engine sound, per-body tuning gaps, sound buses and impact decals are the
  next W8 slices.
