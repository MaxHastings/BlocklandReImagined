# 2026-10-04 Spawn brick paint follows its live vehicle

Maxwell: spray painting a vehicle spawn whose "Re-Color Vehicle" is checked
should repaint the vehicle it spawned, wherever it is, without a respawn.
v0.2.3 only reapplied the brick colour on a wrench Send
(`color_vehicle_brick`); any other brick paint (spray can, Fill Can region
fill, `setColor` event, undo, mini-game reset) left the old colour until
the vehicle respawned.

## Change

One mechanism in `crates/sim/src/session/vehicles.rs`. `Vehicles` now keeps
`brick_paint`: the spawn-brick colour (`spawn_color`, i.e. what
`fxDTSBrick::colorVehicle` gives) each spawned vehicle last took. It is
recorded at spawn and by Wrench Send, and dropped with the vehicle.
`reconcile_vehicle_bricks` already reads every changed brick once per change
(the dirty log, no per-tick scan); for a spawn brick whose vehicle kind is
unchanged it now calls `follow_brick_paint`, which recomputes the brick's
paint and, only when it differs from what the vehicle last took, writes the
vehicle's colour. So:

- Any path that changes the brick's colour or its recolor flag (spray, fill,
  events, wrench edit without Send, undo) repaints the live vehicle on the
  next tick, occupied or not, without touching identity, pose or physics.
- Turning Re-Color Vehicle off gives `None`, the model's own material, the
  same as a newly spawned unrecoloured vehicle (and the v0.2.3 Send-off
  behaviour). Turning it on gives the brick colour.
- Unrelated brick edits (name, events, ...) leave the brick paint unchanged,
  so independent Fill Can RGB paint on the vehicle survives them, as v0.2.3
  required. A later brick colour change does repaint over it (it is the new
  brick paint).
- Highlights (admin Highlight Brick Group, duplicator selection flash) are
  read through `Session::unlit`, so the flash never repaints vehicles.
- Undoing a vehicle paint that also repainted its brick
  (`undo_vehicle_paint`) calls `settle_brick_paint`, so the restored vehicle
  colour wins over the reverted brick colour.
- Wrench Send keeps its deliberate reapply (it resets independent paint even
  when the brick paint is unchanged).

Bots: spawn bricks naming a bot kind never took the brick colour at spawn
(`spawn_vehicle_for` returns early for bot kinds; the bot wears its kind's
look), so live paint does not colour them either. Behaviour is consistent.

Wire and client: no change. The colour reaches clients through the existing
reliable-on-change `VehicleInfo::color` in `Delta.vehicles` (one change per
repaint, no new message, no protocol bump). The client's
`ClientVehicles::prepare` derives the body tint from the current
`VehicleInfo` every frame as a per-instance tint (`body_tint`), so no model
rebuild or texture upload happens.

## Tests

Build env for every command: `CARGO_TARGET_DIR=/home/claude/bri-target
CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=3`.

New `crates/sim/tests/vehicles.rs`
`painting_a_spawn_brick_recolors_its_live_vehicle` (synthetic plus the
ignored native variant): a joined player selects the blue spray can
(`Command::UseSprayCan`), aims at the spawn's exposed corner and fires
(`Command::WeaponTrigger`). Once the brick turns blue the same vehicle id
must be blue within one tick and unmoved. Then Re-Color Vehicle off and on
through `edit_brick(Edit::Properties)` (no Send) must give `None` then blue,
and with it off a brick colour change must leave `None`; the infos survive
serialization.

- Before (origin/main 53f05cf, tests only): `cargo test -p bri-sim --test
  vehicles painting_a_spawn_brick` FAILED at "the live vehicle took the
  brick's new colour within a tick": left red, right blue (the spray had
  painted the brick).
- After: `cargo test -p bri-sim --test vehicles` 28 passed, 0 failed
  (including `wrench_send_recolors_the_existing_vehicle_without_respawning`).
  `--test fill_can` 8 passed, `--test tools` 37 passed, `--lib` 189 passed.
- Client: `cargo test -p bri-client --lib vehicles::` 13 passed, including
  the v0.2.3 `a_live_vehicle_updates_its_draw_tint_without_a_new_pose_or_identity`,
  which already covers a colour change on a live vehicle re-tinting its
  prepared instances without a new identity or pose. No client code changed,
  so it passes before and after.
- `cargo clippy -p bri-sim --all-targets -- -D warnings` clean; rustfmt
  `--edition 2024` on the changed files.

Native-content (`--include-ignored`) variants were not run here: no
generated `content/` on this machine. Maxwell's in-game check (spray a parked
and a driven vehicle's spawn, toggle Re-Color Vehicle) remains open.
