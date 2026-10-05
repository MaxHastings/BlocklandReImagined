# 2026-10-05 Runover kills go to the driver; respawns no longer throw parked jeeps

Branch `fix/runover-and-jeep-physics`. The bot route-planner lane found both
bugs, and both show on camera in the v0.2.4 showcase video.

## Runover kill credit

A player run over by a driven vehicle died by their own hand: Slayer scored
a suicide. `Intent::RunOver` carried the driver as `owner`, but the host
asked `mover_credit` first. Mover credit goes to whoever last pushed the
vehicle (`push_contacts`, `vehicle_contacts`). The victim walking into the
jeep pushes it, so the victim was the "mover", and the kill was theirs.

v20's `WheeledVehicleData::onCollision` damages with the vehicle as source,
and its controlling client takes the kill. `Intent::RunOver` now names the
`driver` at contact. The host credits the driver whenever there is one who
is still connected. Mover credit (who threw, holds or last pushed it), then
the spawn owner, apply only to an undriven vehicle, so walking into a parked
jeep is still your own death. `can_damage_player` and the
minigame-only-harm rules run on the credited owner as before. No wire change:
intents never leave the host.

## Parked jeeps thrown off the map

Reproduced headlessly: the gauntlet's `a_jeep_on_each_side` was run for 4
rounds (`BRI_GAUNTLET_ROUNDS=4`) with a temporary per-tick probe of vehicle
velocities. The probe was not committed. Unoccupied jeeps jumped to 16, 46,
67 and about 100 u/s in a single tick, and one crossed some 60 units of map.
Each time, a bot that had died on that jeep's roof (its corpse a sensor,
lying on the roof) respawned on that tick.

Root cause: `Player::teleport` (spawn, respawn, dismount) only set the
kinematic body's *next* pose. Rapier gives a position-based kinematic body
the velocity that reaches that pose in one step, here thousands of units a
second. Respawn makes the collider solid on the same tick. The step then
solved the old contact with the jeep at that speed and threw it, along with
anything touching it.

Fix: a teleport places the body at once (`RigidBody::set_position`, which
sets the current and next pose together). A relocation is a jump, not
travel. Seats (`place`), motor steps and corrections still move by the next
pose. No velocity clamp was added.

Passing through a linked opening (portal) is the same kind of jump, and it
flung things the same way. The motor drove the body to the partner by its
next pose, sweeping it across the gap in one step. A crate standing against
the walker's side, toward the partner 20 units away, was thrown at 403 u/s.
The body is now placed at once when it passes an opening, as a teleport
does.

Seen while building that test, not changed: the motor moves a body once
per 32 ms Torque tick, and the kinematic body covers that move in a single
120 Hz step. A walker stepping 0.25 up onto a sill throws a crate on its
head upward at about 22 u/s. That matches v20's own tick, but anything
resting on a player feels a four-times-faster step.

## Tests

- `crates/sim/tests/vehicles.rs`
  `a_runover_is_the_drivers_kill_even_when_the_victim_walks_into_it`: a
  mini-game, the driver's own jeep, and the victim walking into it. Before:
  `killer: Some(victim)`. After: the driver's kill, and the victim's score
  stays 0.
- `crates/sim/tests/vehicles.rs`
  `respawning_from_a_parked_jeeps_roof_leaves_it_parked`: a seatless car
  (so landing on it does not board it), a player on its roof, suicide, then
  respawn. Before: the jeep reached 67 u/s and 62 rad/s and ended 2.8 units
  up. After: it stays under 1 u/s and under 0.2 units of travel.

- `crates/sim/tests/portals.rs`
  `walking_through_does_not_fling_what_the_walker_touches`: a walker steps
  through a big portal from its sill with a crate against its side. Before:
  the crate is thrown at 403 u/s. After: it stays under 15 u/s. All 11
  portal tests pass, including the one that checks the walk through never
  jumps or stalls.

## Evidence

- `cargo test -p bri-sim --test vehicles --test combat --test player --lib`:
  all pass.
- `cargo test -p bri-motor -p bri-vehicles`: all pass.
- `cargo clippy -p bri-motor -p bri-vehicles -p bri-sim --all-targets -- -D
  warnings`: clean.
- 4-round soak probe after the fix: no vehicle changed speed by more than
  15 u/s in a tick (before: 4 such events, up to 105 u/s).
- Gauntlet `a_jeep_on_each_side`, 1 round. Before: stuck 5.0%, kills 7,
  self_kills 9 of 16 deaths, switches 20.5/min. After: stuck 15.2%, kills 5,
  team_kills 3, self_kills 0, switches 26.6/min. Its stuck limit (8%) now
  fails. With only the runover fix, stuck was 5.0% but switches reached
  34.8/min, over the 32 limit.

## Open: gauntlet `a_jeep_on_each_side` fails its stuck share

Traces show bots jammed around the parked jeeps: one stands on a jeep's roof
or on an enemy's head directly over its quarry, which is under 1.5 units away
horizontally and well below it. The roof walk-off rule only fires beyond 1.5
units. Others have no route around the pile. The flung jeeps and suicide
respawns used to break these jams up. Two narrow walk-off changes were tried
and backed out; neither cut stuck below 10%. This is bot navigation around
vehicle bodies and crowds, and it belongs to the route-planner lane. The
threshold was not loosened.

`stairs_to_a_deck` (circling 4.5% > 4%) fails identically on the base
commit f63480bc and is unrelated.
