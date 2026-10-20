> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-nav-design.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: v0.2.6 bot navigation design note (with change 5)

Read against main fd719c94: crates/sim/src/nav.rs, crates/sim/src/reach.rs, route.rs, bots.rs, session/combat.rs. The note's line numbers are close but not exact.

## Verdict: PASS on direction, with 7 must-fixes to the note (or the first commit) before the code is reviewed.

The direction matches the GPT review and the bot rules. It keeps the lazy 0.5 grid, the bounded resumable A* and local invalidation. It turns edges into measured body moves, deletes the scalars and cell poisoning they replace, and treats the real saves as evidence rather than tests. The measured failures are real and traced to code: adjacency-only jumps (`neighbours`/`edge`), a start node that is never found, and overlapping treads with no clear cell centre. The test list is mechanism properties, which is right.

The note's GPT corrections check out: "stairs fail" is overstated (it's the crawl cost), and the start bug is real.

## Must-fixes

### 1. One cost unit for every edge, from the body's tuning (answers Q1: yes, 1.5 is a magic number)
Edge costs today mix several made-up numbers with distance (nav.rs ~1262-1271):
- `travel = flat + drop × 0.1`
- jump `+1.0`
- snug `+0.6`
- crawl `+1.5`

Change 2 adds "flight time vs walk time", so the note must move every edge to one unit (seconds, or walk-equivalent units, as `route::Swim::of` already does for swimming):
- walk = distance / run speed
- crawl = distance / crouch speed (`crouch_forward` in PlayerTuning)
- jump = measured flight time
- drop = fall time
- swim as now

That deletes 1.0, 1.5 and 0.1. It also changes the stairs-under-a-roof choice from physics, not a threshold: with stock speeds, crawling costs about 2× walking instead of today's 4× per cell. `snug` 0.6 is a preference, not physics: leave it, named, as a v0.2.7 item, or say what measured thing it stands for. Mixing seconds with these constants would be a second costing scheme.

### 2. The jump envelope: takeoff, range and the executor
- **Takeoff.** Pegs, 1-brick corridor ledges and treads give no run-up. Measure the standing-takeoff envelope, or key the table by the run-up the takeoff floor actually has along the jump. A run-up envelope used from a peg plans hops the bot can't make. A standing-only table is a fine v0.2.6 scope; run-up can follow.
- **Range.** Rises must include negative values down to the drop limit, so a gap across the shaft to a lower ledge is covered.
- **Executor.** A jump edge must carry what the executor needs to replay the measured control program (takeoff point, heading, when to jump and when to crouch in the air). The envelope test must run the **bot executor** on the edge, not only the motor: "inside gives an edge the real bot lands". Otherwise the plan and the controls can disagree, as with the rooftop hop.
- **Dependency.** `hop_flight`/`LANDING_SLACK` are on the rooftop branch (1400e814), not on main yet, so the lane stacks on it or waits for the combined gate.

### 3. Bound the jump-edge branching and measure it
"Any support inside the envelope" can be dozens of candidate landings per node, each a sample plus an arc sweep. The note must say:
- which nodes generate jump edges (e.g. only where a straight neighbour is a hole, a drop or a wall);
- the most candidates per node, from the envelope's size;
- how a sweep is charged against the per-tick budget (a sweep costs more than a sample).

Then re-measure release `bot_think_time_16` (bar 2000 µs) and the planner on Close Quarters and on a big build, before and after.

### 4. The start bug: fix it where floors are found, not only in `first_node` (cause inferred, please verify)
`sample()` caps its downward ray at the ceiling over `from` (`top = (from + jump).min(ceiling - height)`), but `crawl()` starts at `from + crawl_jump` with no ceiling cap. Under a roof exactly a body-height above the feet, the standing test fails, then `crawl()`'s ray can start above the roof and accept the roof's top as the floor. `node_at` then drops it as out of reach. If the probe confirms this, the fix is the ceiling cap in `crawl()`. That fixes every node under such a roof, not just the start. "First_node tries the crouched stance" would be a start-only special case of the same thing.

### 5. Change 5 is not "nothing new": a node's place has to be carried
`Node` is (x, z, y) and `Node::feet()` is the cell centre. Edge sweeps (`neighbours`' drop `sweep_box`), costs, `pull`/`walkable`, waypoints and jet legs all use `node.feet()`. A settled spot changes every one of them. The note must say:
- `Floor` stores the settled (x, z), and every `node.feet()` consumer uses the floor's spot. Waypoints carry it, which also helps the executor.
- `settle` stays inside its half-cell slack (CELL × 0.5 per axis), so the spot stays in its cell. `cell_of`, `avoid`, the cache keys and `invalidate` (which already reaches width + CELL) are unaffected.
- Keep today's narrow-box test at the centre, and settle (full width) only when it fails. Most cells then cost exactly what they do now.
- `settle`'s loop ends only when the slack is spent (each push ≥ 0.01, so up to about 50 rounds). Give it a bound on rounds that follows from the slack and the push size, and count a settle against the sample budget.
- Measure samples per second and cache size on a big build, before and after.

### 6. The 3.2 drop stall: root-cause it in this change (answers Q2: players hit it)
A bot standing at a ledge for 12 s is something players see. Failure memory (change 3) would make it pick another way after failing, but it wouldn't explain why a valid drop failed. Changes 1 and 2 send far more jump and drop edges through the same executor. Find the cause on the measured case (lip, sweep height, the step-off control) and fix it, or show that it's the same cause as an item above.

### 7. The drop limit comes from the host's own fall-damage rule
`Session::fall_damage` (combat.rs ~2469) hurts on impact *speed* (vector length, so horizontal speed counts) above the archetype's `min_impact_speed() × scale`, with the motor's drag on the fall. Don't re-derive this with `sqrt(2gh)`. Have `Reach` measure the deepest drop whose landing speed stays under that same threshold, with the real motor and the bot's step-off, as it measures `ledge`. That deletes `drop: 4.0` (nav.rs ~102).

This is a big behaviour change: the stock limit becomes far deeper than 4. It touches the rooftop fix and the gauntlet's `fell` count. It goes in only together with time costs (must-fix 1, so a long drop costs its fall time) and a rooftop-brawl run showing `fell == 0`. Otherwise it waits for v0.2.7. "In if small" is too loose a test.

## Answers
- **Q1, crawl 1.5:** yes, a magic number. See must-fix 1.
- **Q2, drop stall:** players would hit it, so don't cut it. See must-fix 6.
- **Q3, Max's ledge corridor (1-brick ledges on alternating walls):** changes 1, 2 and 5 cover it once must-fix 2 is in: standing takeoff, sideways offset (already in the note), negative rises for crossing down, and head room under the next ledge above, checked by the arc sweep. Add one probe of exactly that shape, going up and coming down, to the peg-ladder test.

## Smaller points (no change needed unless the lane disagrees)
- Per-transition failure memory: keep the existing until-tick lapse (`Nav::avoid`). It's the one existing mechanism, and moving bodies the grid leaves out make failures transient. Also forget an entry when `invalidate` touches either end.
- Delete list once done:
  - `Reach::ledge`/`crawl_ledge` and their `ledge()` measurement, if the table replaces them;
  - `edge()`'s jump branch;
  - `AVOID_REACH` and the 3×3 poisoning;
  - `Body::drop` 4.0;
  - the cost constants in must-fix 1.

  `Body::jump`/`crawl_jump` are also read by `sample`/`crawl` (how high to look) and `invalidate`, so they become envelope lookups, not separate numbers.
- Cut to v0.2.7 as the note says: ladders, moving supports, coarse regions.
