# 2026-10-12 Perf waste fix: a drop or jump edge is swept once per route grid

Finding 16 of the v0.2.10 performance waste audit (report at
`perf/waste-audit-v0.2.10.md` in the project files): the route grid
remembered its floor samples and leap arcs but not the box sweeps that clear
a drop off a node or a jump up onto the next, so every expansion of a node
beside a ledge, by every bot's search, swept the same volume again.

## What was wrong

`Nav::neighbours` (`crates/sim/src/nav.rs`) swept, per drop edge, one box
from the node to the next cell at standing height (and a clearance query to
know whether it stood crouched), and per jump edge two boxes, up and over.
The result depends only on the two nodes, the fixed collision and the body,
which is exactly what the `floors` and `arcs` caches already key on. A
search expands a node once, but every search starts over, and the bots of a
body size share one grid: with a crowd routing through the same rubble the
same edges were swept once per search per bot.

## Fix

- `Nav::edges`, keyed like `arcs` on `(from, to)`, holds whether the way
  over an edge is clear; `Nav::edge` fills it the first time and answers
  from it after. The sweep itself is unchanged (the same clearance query,
  the same boxes), so routes are identical.
- `Nav::invalidate_all` forgets an edge with the same box test as an arc
  (from the node it leaves to the one it lands on, a body wide, up to the
  jump apex, or the body's jump if higher, and a body over), so a brick
  laid or blown out on a ledge is seen by the next search.
- The cache is capped like the others (`MAX_SAMPLES`) and emptied by a map
  change with the grid.
- `Nav::edged` counts the sweeps; `Session::nav_work_counts` reports floor
  samples, arc sweeps, edge sweeps and forgotten floors over every grid, and
  `bot_combined_perf` and `bot_deathmatch_stress` print it as `nav_total`.

## Tests

- `nav::tests::drop_and_jump_edges_are_swept_once_per_grid`: a route down
  off a 1.5 block and one up onto it sweep edges; the same route again takes
  the same path with no new sweep; a brick on the block's edge forgets the
  edges over it (swept again), one 40 units away forgets none.
- `cargo test -p bri-sim --lib nav::` (36) and `cargo clippy -p bri-sim
  -p bri-chaos --all-targets -- -D warnings` pass on the cloud box; the
  chaos suites `bot_work_counts`, `bot_routes`, `bot_navigation_spike`,
  `bot_hardening` and `bot_gauntlet` pass (1, 10, 8, 5 and 24 tests; the
  sim lib 380).

## Measurements (cloud box, 4-core Xeon, release)

`nav::tests::rubble_edge_sweeps_are_shared_between_searches` (ignored,
printed): 220 blocks 0.5 to 2.5 high scattered on a 60 x 60 floor, 32
searches between points round its edge sharing one grid, the cache kept
against a grid that forgets its edges before every search (the old
behaviour, every other cache kept). Three rounds:

| | edge sweeps | floor samples | arc sweeps | wall ms for the 32 searches |
|---|---|---|---|---|
| edges kept | 391 | 13,228 | 374 | 73.7, 94.8, 74.3 |
| edges forgotten per search | 2,224 | 13,228 | 374 | 75.1, 76.1, 74.3 |

The sweeps go down 5.7x with the same paths (28 of 32 found either way,
127 ticks each). The wall time does not move: in this scene the floor
samples (13k rays) dominate and the 1,833 sweeps saved are under a
millisecond in total. The measured share is below the audit's fix bar; the
fix is kept because it is small, covered and removes repeated work whose
share grows with ledges and crowd size (the content maps the audit counted
at up to 400-1500 casts per tick).

`bot_combined_perf` (content-free, 3600 ticks, 4/8/16 bots) reports
`edge_sweeps=0` in every phase: its synthetic scene has no drop or jump
edges, so it cannot show this fix; its step times with the cache on and off
(two rounds each) are within noise, as expected.

Max's PC: `bot_deathmatch_stress` on the Afghan DM save now prints
`nav_total`, so the release thread's run gives the edge sweep count and
step times on real terrain. Pending.

## Next work

- Hoisting `ground.floats` (two water tests per node when waters exist)
  was left alone: it already runs once per `neighbours` call, and the
  second is in `Search::leaps` on a different node set.
