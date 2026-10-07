# 2026-10-06 Plant errors follow v20's own plant check

Max reported false "buried", "stuck" and "floating" icons while building.

## Cause

Our plant check refused far more than v20's. It refused a brick for any
contact deeper than 0.002 with any map mesh (interiors and static shapes
alike, apart from a 0.1 floor dip) and for any overlap with any moving body,
players and bots included. So a brick that clipped a corner of a wall, a
tree or prop, or the edge of someone's box (your own included, as you walk
or jump while building) showed buried or stuck.

## v20's rule (decoded from blocklandv20.exe)

`fxDTSBrick::plant` (console method 0x6df3ce -> 0x541c50 -> 0x540930) runs
the server check at 0x53ec40, on the brick's box:

1. Two rays through the brick's centre, along x and along y (Torque's
   horizontal axes) at mid-height, against interiors only (mask 8): a hit
   returns 3 (stuck).
2. Bricks in the box grown by half a stud: a grid overlap returns 1.
3. floor(width) samples per horizontal axis, clamped to 1..4. At each, a
   ray from the top to 0.1 below the bottom against terrain and interiors
   (mask 0xC): an interior hit more than 0.1 above the bottom returns 3;
   terrain is compared with the bottom for grounding. Terrain at or above
   the top at every sample returns 5 (buried).
4. Not stud-attached and not grounded: 4 (unstable) when some sample hit
   ground, else 2 (float).

It never searches for players or vehicles; the client-side plant at
0x53e9a0 only refreshes their collision working sets afterwards. Static
shapes (TSStatic) are not in either mask.

## Change

`check_placement_support` now refuses for the map with v20's centre-line
and sample rays (`buried_in_interiors`), against interiors only: static
shape colliders are recorded in `Simulation::map_statics` and ignored, as
are terrain colliders (still judged by `buried_bounded`). The stud
connector test uses the same interiors-only filter. Players and package
bots never refuse a brick. Vehicles and other moving bodies still refuse it
as Stuck: a brick inside a physics body would fling it. We keep our icons
(map: Buried, bodies: Stuck) rather than v20's code 3 for interiors.
Touching is not crossing: the lines stop 0.002 short of the faces and run
0.002 above mid-height, so a plate sunk the full 0.1 still plants.

Floating was not changed: our support rule is already looser than v20's
(any footprint cell over a floor grounds a brick; v20 wants every sample).

## Evidence

- `cargo test -p bri-sim --test building`: 16 pass, including the new
  `only_what_v20_refuses_blocks_a_plant` (fails on the old check with
  "a clipped corner: Err(Buried)").
- `cargo test -p bri-sim --no-fail-fast`: all pass except
  `a_bot_jets_over_to_someone_above_it`, already in
  `tools/gate-known-failures.toml`.
- `cargo clippy -p bri-sim --tests -- -D warnings`: clean.

## Next

Not run here: the client and chaos suites and the full gate (on the PC with
the release).
