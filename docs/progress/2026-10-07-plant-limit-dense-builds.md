# Plants in dense builds no longer fail as "brick limit"

Max, playing ACM City (about 20k bricks): Flag Spawns and team spawns
were refused with LIMIT; clearing nearby bricks let one plant.

Cause: `check_placement_support` (crates/sim/src/simulation.rs) spends a
256-unit allowance per plant. It charged one unit for every brick listed in
each eight-unit bucket its query box touched, including bricks nowhere near
the new one. A dense city packs more than 256 bricks into one bucket, so an
ordinary plant ran out and was reported as `PlantFailure::Limit` ("Brick
limit reached"), the same error as the server brick limit. v20 has no such
refusal.

Fix: only bricks that touch the new brick's box (and their stud cells) are
charged. Passing over the rest is one box test each. The bucket count of the
query is bounded up front, as `GroupSupport::check` already did for groups.

Evidence:
- New test `a_plant_beside_a_dense_bucket_of_untouched_bricks_is_not_a_limit`
  (304 untouched plates in one bucket): failed before the fix with
  "Brick limit reached", passes after.
- `cargo test -p bri-sim --test building --test duplicator --test hardening_session --lib`:
  all pass, including the pathological-limit test.
- `cargo clippy -p bri-sim --tests -D warnings` clean.

Not changed: a brick that really touches more than 256 others (a big plate
dropped on a field of 1x1s) is still refused, and the error still reads as the
brick limit. Splitting the message from the server brick limit needs a new
plant error, which is a wire change.

## Gate failure and second fix

The PC gate (66f3cbb6) failed `mode_and_voxels::voxels_dig_out_pile_back_up_and_are_saved`
at "Piling it back on what they stand on is refused: they are in the way".
That refusal had only ever come from this bug: the test's 16x16x4 voxel
ground puts more than 256 bricks in the buckets round the hole, and
`Simulation::fits` (behind the rules' `can_place_voxel`) failed with
"Brick limit reached" (checked by printing the error on origin/main). Since
plants ignore players (v20), nothing else refused a voxel inside its
placer.

Second review (6233c4b) accepted the cause but not the scope: putting the
body check in `Simulation::fits` also changed package plants (a Trench dirt
piece overlapping someone would silently vanish) and undo of a cut (refused
whole, as "Overlap", when anyone stood in the gap). Both stay as in v0.2.5.

Fix: `Simulation::fits` is as before (it may go where a player stands, like
a plant). The new `Simulation::fits_clear_of_bodies` adds the body check, and
only the two rule queries use it: `can_place_voxel` (`voxel_fits`) and
`can_plant` (script_world). The choice is by operation, not by Add-On.

Evidence: the voxel test passes. The new test
`advanced_duplicator::undoing_a_cut_puts_it_back_round_a_player_standing_in_the_gap`
passes, and fails with 6233c4b's `fits`. All `bri-sim` tests pass, and
`clippy -p bri-sim --tests -D warnings` is clean.
