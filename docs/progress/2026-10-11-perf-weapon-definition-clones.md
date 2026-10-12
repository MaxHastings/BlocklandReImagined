# 2026-10-11 Perf waste fix: weapon definitions read in place, not cloned per tick

Finding 14 of the v0.2.10 performance waste audit (report at
`perf/waste-audit-v0.2.10.md` in the project files).

## What was wrong

`WeaponsWorld::advance` cloned the whole `Image` (about ten Strings and a
`Vec<State>` with four Strings each) for every mounted hand of every actor
on every 120 Hz tick, `projectile_step` cloned the `ProjectileDef` of every
live projectile every tick, and `step` cloned each exploding projectile's
definition. The clones existed only to end a borrow of `self.pack` before
`&mut self` calls, although the pack already sits behind an `Arc` and never
changes during a map.

## Fix

Each site clones the `Arc` (a reference count) and borrows the definition
from it: `let pack = Arc::clone(&self.pack); let image = &pack.images[..]`.
The per-shot `ProjectileDef` lookup gets the same treatment; `hitscan`
keeps its clone because it edits the damage of its copy per shot.
No behaviour change: the definitions are immutable.

## Evidence

- `cargo test -p bri-weapons` and `cargo clippy -p bri-weapons --tests --
  -D warnings`: pass.
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

None; the string-keyed `images[&e.image]` lookup per tick remains and is a
small follow-up if a profile ever shows it.
