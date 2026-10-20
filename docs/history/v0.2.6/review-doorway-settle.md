> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-doorway-settle.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: doorway settle, 79c9c48..8810f4f (nav.rs + bot_brain.rs)

This was read-only: I read the diff and `Body::clearance`, and ran nothing.

## Verdict: passes. Two optional tidy-ups.

1. **It's one mechanism, not a second clearance system.**
   - The grid still decides every cell and every edge with its narrower box. For bodies a cell or more wide that box is `(width − CELL)/2`, a quarter-cell slack each side (`nav.rs:112-122`).
   - `settle` runs only on the finished path (`Search::finish`). It only adjusts where a walk waypoint sits inside that same slack, so the full-width body fits. It never adds, removes or reroutes a node.
   - When it can't fit, the waypoint stays exactly as before (`unwrap_or(feet)`). So the worst case is today's behaviour.
   - Jump, opening, swim and jet waypoints are untouched.
2. **It can't move a node anywhere unreachable or out of line.**
   - It moves at most half a cell on each axis (`max_element ≤ CELL*0.5`), sideways only (y never changes). It only accepts a spot with standable floor within a step (`stands`, the same test `walkable` now shares).
   - Each push goes away from what it overlaps. If two jambs push opposite ways in a gap narrower than the body, it runs out of passes and returns `None`.
   - Overlapping a ceiling or a ramp face (no sideways way out) returns `None`, or overshoots the slack and is refused.
   - Neighbouring settled waypoints stay within the grid's own slack of their nodes, so a leg between them crosses no geometry the grid's edge check didn't already pass.
   - It's deterministic: fixed queries, and ties broken by `total_cmp`.
   - Inferred, not run: a leg between two settled points isn't re-checked. Since both ends are clear for the full body and lie within the slack of a checked edge, I see no realistic way through a wall.
3. **No cost spike.**
   - It runs once per route build, per walk waypoint of the raw path, before pulling.
   - A clear waypoint costs one shape query plus one ray. An overlapping one costs up to `SETTLE_PASSES` shape queries plus a contact test each.
   - The search itself does many fit checks per expanded node, and expansions far outnumber path nodes. So this adds a small share of a route build, not a spike. `bot_think_time_16` in the PC gate is the check.
4. **The constants.**
   - **`SETTLE_GAP` 0.01** is a numerical tolerance ("no longer touching"). It's named and documented. Fine.
   - **`SETTLE_PASSES` 4** is justified in its comment (wall, jamb, a corner's other face), but it is a count picked by hand. It can go without a new number:
     - stop when a push would leave the slack (already refused at the end), or when a push reverses the previous one (ping-pong between jambs);
     - and as a backstop, cap the passes at `slack / SETTLE_GAP`, which is derived.
     - Optional. 4 is safe, because running out of passes just keeps the old waypoint.

## Optional tidy-ups (not blocking)

- **Derive the slack from the body.** The slack is hard-coded as `CELL * 0.5`. Use `body.width*0.5 − half_width` from `clearance` instead. They're equal for normal bodies, but for a body under two cells wide the `width*0.25` floor in `clearance` leaves more slack than half a cell, and settle would refuse spots the grid allowed.
- **The `bot_brain` glass wall** now uses `fixture::barrier_courses()`, the same fix as the door room. The old doorway test passed only because the bot hopped the wall. That's correct, and nothing was loosened.

---

## Follow-up 8810f4f..93cd6ed (`SETTLE_PASSES` removed): passes

**It terminates.**
- Every push is a unit sideways direction × (depth ≥ 0 + `SETTLE_GAP`). So each pass adds at least 0.01 to the summed x+z budget.
- The loop returns `None` as soon as either axis's sum passes `CELL*0.5`. That bounds it at about 50 passes, derived from the slack and the gap, not picked by hand.
- The degenerate cases still return:
  - a nearly vertical normal gives a huge push, which runs over the budget;
  - an exactly vertical one returns `None` at once;
  - a NaN depth leaves no overlap at the next query, then `stands` fails.

**Nothing else changed.**
- The final spot is within the slack by construction, since |at − feet| ≤ spent. So the old end check was redundant and was rightly dropped.
- The only behaviour difference: a waypoint that needed more than 4 pushes inside the slack can now settle, where before it kept the raw spot. That's the intent.
- The one-file diff touches nothing else.
