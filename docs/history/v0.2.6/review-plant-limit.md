> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-plant-limit.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Plant limit in dense builds: 92501ea (claude/fix-plant-limit-dense-builds)

Requested by the CTF thread (cross-session). Parent 1ba65f1 is on origin/main.

**Verdict: PASS. Merge 92501ea.** A player hits it (Max, ACM City, ~20k
bricks), so it's inside the finish line.

## The cause is right
`QueryCursor::step` (grid.rs:247) returns `Some(None)` for every bucket entry
it passes that doesn't intersect the query, and once per bucket boundary.
The old loop charged `work_left` for each of those. So one bucket holding
more than 256 entries (BUCKET is 16×40×16 cells) failed an ordinary plant
as `PlantFailure::Limit`, which reads "Brick limit reached".

## The fix
- `work_left` is now charged only for admitted candidates (bricks whose box
  meets the expanded query). The per-candidate cell walk is still charged
  below, unchanged.
- The bucket count is bounded up front (≤ SUPPORT_QUERY_LIMIT buckets),
  the same rule `GroupSupport::add` uses (simulation.rs:96-101). No new
  number.
- The test packs 304 plates into one bucket, beside the plant, then stacks
  a second brick on the first. It fails before the fix and passes after.

## Residual, noted not blocking
Passing over entries that don't touch the plant is now uncharged, so the
worst case per plant is buckets (≤ 256) × entries per bucket.
- Without overlaps a bucket holds at most 16×40×16 = 10,240 one-cell
  entries. That is about 2.6M box tests in a fully packed region, roughly
  milliseconds, and only for a giant brick planted inside solid fill.
- Loaded saves may overlap bricks, so the true cap is MAX_BRICKS.
- A real city runs hundreds per bucket. Fine for v0.2.6.
- v0.2.7 line if wanted: charge passed-over entries against their own
  allowance, sized from the bucket's capacity (16·40·16 × buckets).
  That restores a hard bound with no new number.

## Round 2: 6233c4b (on 38e58ba = 92501ea + main d068f06). FAIL, one must-fix (scope); cause accepted.

### Cause: accepted
The voxel test's "in the way" refusal only ever came from the support-search limit, and fits never checked bodies. So with the limit fixed, piling dirt onto its digger went through. Adding the body check fits already promised in its docs is the right idea. engulfs_character uses the same -0.002 contact depth as the vehicle check.

### What it changes for players (answering the coordinator)
`Simulation::fits` has four callers (git grep `.fits(`). Normal brick-tool plants, admin plants and ghost placement do NOT go through it, so ordinary building is unchanged from v0.2.5 and v20: you can still plant a brick where a player stands.
1. Voxels (`voxel_fits` → rules' can_place_voxel): can't put a voxel inside a body. That's what the test asks for. Fine.
2. Trench Digging `can_plant` (trench.rhai:370, :431): placing or dumping dirt where a player or bot stands now says "You can't dump dirt there!". Likely the original's intent; inferred, I can't see the v20 script.
3. **Add-On `plant_brick`** (packages.rs:1763): now silently skips a brick that overlaps a body. Trench's own edit flush (trench.rhai:159, re-plants split/merged dirt pieces) goes through it. So a dirt piece that would overlap someone is dropped without a word, and a hole appears in the dirt. v0.2.5 planted it.
4. **Undo of a cut** (undo/jobs.rs:325): the whole undo is now refused if anyone stands anywhere inside the cut region, and the reason it gives is `PlantFailure::Overlap` ("bricks in the way"), which is wrong. The player undoing a cut is often standing in the gap they cut. v0.2.5 restored it. **This is a player-visible change to building.**

### Must-fix (default; Max can override 4)
Keep the body check to the two rule QUERIES that need it (`voxel_fits` and script_world `can_plant`), e.g. a separate `fits_clear_of_bodies`, and leave `plant_brick` and undo's pre-check exactly as v0.2.5. That still fixes the failing test, keeps 3 and 4 unchanged, and doesn't widen a gate fix into a behaviour change at the finish line.
If Max wants undo to refuse while someone stands in the cut (its docs say "nobody standing in it"), it needs its own failure reason ("someone is standing there") instead of Overlap. That's for v0.2.7 unless he asks.

### Verified
Callers by git grep at 6233c4b. Tests not re-run here (the lane reports all bri-sim tests pass and clippy is clean).

## Round 3: 638e2ae (delta on 6233c4b). PASS. Merge 638e2ae901607374d439849b53031231c81ac52e.
- `fits` is back to v0.2.5 (plus the limit fix); docs say it may go where a player stands.
- New `fits_clear_of_bodies` is used only by voxel_fits and script_world can_plant (git grep). plant_brick (packages.rs:1763) and undo-cut (undo/jobs.rs:325) use plain fits, so Add-On plants and undo behave as in v0.2.5.
- Ran here: advanced_duplicator undoing_a_cut_puts_it_back_round_a_player_standing_in_the_gap and mode_and_voxels voxels_dig_out_pile_back_up_and_are_saved both pass.
