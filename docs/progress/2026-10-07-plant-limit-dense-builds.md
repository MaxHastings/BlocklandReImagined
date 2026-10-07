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

`Simulation::fits` already promised "not stuck in a player"; it now keeps
that promise by also refusing a brick inside a player's or a bot's body
(`engulfs_character`). Plants are unchanged: they still go where a player
stands, as in v20. `fits` serves rule-placed bricks (`can_place_voxel`,
`can_plant`, package plants) and undo's restore, none of which should bury
anyone.

Evidence: the voxel test passes, refusing the first place because the cube
engulfs the digger and accepting the second; all `bri-sim` tests pass;
`clippy -p bri-sim --tests -D warnings` clean.
