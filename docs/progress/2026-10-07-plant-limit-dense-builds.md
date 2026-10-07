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
