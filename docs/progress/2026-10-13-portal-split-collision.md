# 2026-10-13 Split collision for bodies part way through a portal

Max asked that a vehicle half through a portal collide on both sides,
instead of all on one side until its middle crosses, and the same for
players and other shapes, with the cost and risks measured.

## Before

- Walking players already split: their Torque soup cuts the geometry behind
  an opening's plane inside its prism and fills it with the partner's side
  (`Soup::open_passages`).
- Vehicles (Rapier bodies) had no portal awareness during the step. A jeep
  stopped dead at a portal set against a wall, and a wall just past the
  partner was met only after the carry, which threw it back and could carry
  it twice.

## What changed

- `bri_physics::portal` (new): contact rules for one step, applied through
  Rapier `PhysicsHooks` (`filter_contact_pair`, `modify_solver_contacts`).
  - A body part way into an opening loses its contacts past the plane inside
    the opening (the wall behind the portal is not met).
  - Fills: fixed copies of the colliders past the partner, carried back
    through the opening for that step. They pair only with that body and
    keep only contacts past the plane within the opening (0.3 margin). One
    coupled solve, so support is never counted twice (a dynamic twin body
    was tried first and hopped +2.5 u/s from double support).
  - A fill whose source is a dynamic body shares the impulse by mass, the
    far part carried to the far body (as `share_contacts` does).
  - Copies keep their source's `collision_groups()`, and the fill query uses
    the vehicle's groups, so pass groups (Modding building blocks) hold
    through a portal. Rapier checks groups before the hooks run: one
    filtering path.
- Vehicles: `VehiclesWorld` finds straddled openings at the end of
  `pre_step_through`, fills them, and removes the fills in `post_step`.
  Smash intents from a fill are carried to the far side. Wheel rays from a
  mount past the plane (or crossing it) are cast through the opening with
  `Passages::cast`, normals carried back, so front wheels ride onto ground
  past the partner.
- Players: a walking player part way through gets a kinematic stand-in past
  the partner for the step, so a vehicle there meets the part of them that
  is out; a kick on the stand-in is carried back to the player.
- `Simulation::step_with(hooks)`; the host step and the client prediction
  mirror both step with the same rules.

## Evidence

- `cargo test --release -p bri-physics -p bri-vehicles` and
  `-p bri-sim --test portals --test vehicles --test vehicle_prediction
  --test prediction`: all pass (synthetic; content tests ignored here).
- New tests: `a_wall_beyond_the_partner_stops_the_part_past_the_plane`,
  `a_wall_behind_the_opening_is_not_met` (physics);
  `a_jeep_drives_through_a_portal_set_against_a_wall`,
  `a_wall_just_past_the_partner_stops_the_nose_before_the_middle_crosses`,
  `a_car_s_front_wheels_ride_onto_a_plank_past_the_partner_before_it_crosses`
  (nose up > 0.06; 0.040 without wheel rays through),
  `stress_vehicles_part_way_through_openings_cost_and_repeat` (portals);
  `a_car_meets_the_part_of_a_player_beyond_a_portal` (session: the car's
  nose stops at 30.52 against the walker's part ending at 30.63; 29.85
  with the stand-in disabled).
- Cost, host order per 120 Hz tick, jeeps kept awake, median / p99 µs
  (Linux container, release):

  | jeeps | none part way in | all part way in |
  |---|---|---|
  | 1 | 8 / 39 | 25 / 82 |
  | 8 | 17 / 40-61 | 171-193 / 388-484 |
  | 32 | 67 / 157 | 885 / 1969 |

  About 20 µs per vehicle while it is part way in (a pass takes a fraction
  of a second), nothing when no vehicle straddles, and no work at all on
  maps without portals. 32 at once is about 11% of the 8.3 ms tick.
- Repeat: the parked run is bit-identical across runs; parked part way in, a
  jeep stays put (no creep or hop) and rests on the opening's sill.

## Risks and limits

- Fills are inserted and removed every tick a body straddles; keeping them
  between ticks would cut most of the cost if it ever matters.
- No warm start for fill contacts (they are new colliders each step).
- Shots and walkers' own motor soup do not see a vehicle's emerging part
  past the partner yet.
- For rings of more than two links, the list's passage is used, as the
  player soup does.

## Next

Windows CI, then Integration adds it to the v0.3.0 definition of done;
Max playtests driving and walking through portals.
