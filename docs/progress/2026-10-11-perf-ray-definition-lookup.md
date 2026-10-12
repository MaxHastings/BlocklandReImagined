# 2026-10-11 Perf waste fix: one definition lookup per brick ray candidate

Finding 15 of the v0.2.10 performance waste audit (report at
`perf/waste-audit-v0.2.10.md` in the project files).

## What was wrong

`Simulation::walk_bricks` handed each slab-passing candidate brick to the
caller's `accept` closure, which resolved the brick's definition by string
(`definitions.get`, a `BTreeMap<String>` lookup) to test for water, and
then `ray_brick` resolved the same definition again for the collision
test. Every bot sight ray, clear-path check and projectile sweep paid two
string-keyed lookups per candidate. The per-ray duplicate guard also hashed
every bucket entry before the slab test rejected most of them.

## Fix

`walk_bricks` resolves the definition once per candidate and passes
`&Definition` to both `accept` and `ray_brick`; `brick_ray` and
`target_filtered` closures take the definition as an argument, and the six
callers (weapon queries, bot extras, combat spawn rays) were updated. The
duplicate guard now runs after the slab and nearest-hit rejects. Hit results
are identical: the same candidates reach the same collision test in the
same order, and a brick in two buckets is still tested once.

## Evidence

- `cargo test -p bri-sim` (targeting, combat, bot and weapon query tests)
  and `cargo clippy -p bri-sim -p bri-chaos -p bri-net --tests -- -D
  warnings`: pass.
- `bot_combined_perf` (content-free, release, cloud box with nothing else
  running, 3600 ticks, two rounds each, origin/main `01ca563` against the
  whole audit branch, so these numbers are the sum of findings 5, 6, 10, 14,
  15 and 17 on the server tick, not this change alone), step ms p50 / p95:
  mixed-combat-16 1.32, 1.36 / 3.77, 3.86 -> 1.22, 1.21 / 3.71, 3.64;
  mixed-combat-8 1.24, 1.21 / 2.33, 2.70 -> 1.06, 1.15 / 1.88, 2.04;
  independent-16 0.68, 0.73 / 2.99, 3.09 -> 0.55, 0.55 / 2.90, 2.87;
  independent-8 0.70, 0.64 / 1.18, 1.17 -> 0.59, 0.53 / 1.05, 0.99;
  mixed-combat-4 1.10, 1.10 / 2.01, 1.89 -> 1.04, 0.98 / 1.64, 1.58;
  independent-4 0.61, 0.63 / 0.99, 1.00 -> 0.51, 0.52 / 0.86, 0.95.
  About 10% off the p50 of a 16-bot fight tick and 15-25% off an idle
  crowd's; the 8-bot fight's p95 fell about 20%.

## Next

A `u32` definition index on the placed brick would remove the remaining
lookup; that is a schema change for after alpha.
