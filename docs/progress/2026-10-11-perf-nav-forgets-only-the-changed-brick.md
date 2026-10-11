# 2026-10-11 Perf waste fix: a changed brick forgets only its own columns of the walk grid

Finding 10 of the v0.2.10 performance waste audit (report at
`perf/waste-audit-v0.2.10.md` in the project files): one brick knocked out
of a chunk made every bot's walk grid forget the chunk's whole box, old and
new, and each forget walked every cached column.

## What was wrong

- `Chunks::flush` reported a rebuilt chunk as two boxes: the collider's
  AABB before and after the rebuild. For a 1 x 0.5 plate in a full 8 x 8 x 8
  chunk that is about 16x the brick's footprint, times every height band of
  the chunk: roughly 400 columns forgotten where 25 would do. Each column a
  bot needs again costs 3-12 physics queries to resample under the
  96-samples-per-tick cap, so a rocket into a crowded wall left bots
  standing while the grid refilled.
- `Nav::invalidate_all` walked all of `floors` (up to 262k entries per bot)
  with a `retain` for every change tick, however small the change.

## Fix

- `Chunks::flush` computes each part's box before and after the rebuild,
  keyed by brick, and reports only the parts that differ: the removed
  brick's old box, a planted brick's new box, nothing for the bricks that
  stayed. An emptied chunk reports its old parts the same way (the merge in
  `invalidate_all` joins touching boxes, so a cleared chunk is still one
  box to the caches).
- `Nav::invalidate_all` counts the floor keys a change names (cells x
  height bands); when fewer than the cache holds it removes those keys one
  by one, otherwise it walks the cache as before. `Nav::forgotten` counts
  the entries removed.

Correctness: the forgotten set is still a superset of what a change can
alter (the same reach, band and arc margins); the big-change path is the
old code. Leap and arc caches keep their walk: they are keyed by node and
much smaller.

## Evidence

- Work count: `cargo test -p bri-sim --test nav_invalidation` (64 plates in
  one chunk; removing one, then planting it back, each report at most one
  plate's footprint of change instead of the chunk's 64).
- `cargo test -p bri-sim --lib a_small_change_forgets_only_its_own_columns`
  (a 90k-column cache forgets exactly the columns within reach of a 1-unit
  box, and a box over everything still empties it).
- `cargo test -p bri-sim --lib nav` (32 tests), `cargo clippy -p bri-sim
  --tests -- -D warnings`: pass.

## Next

PC numbers on the v0.2.10 baseline are recorded in the audit report's
status column; the gameplay check is bots re-routing after a rocket into a
wall in Max's playtest.
