# 2026-10-08: bot play polish from headless matches

Thread "How bots really play". Evidence: `bot_watch` 2-minute matches on
Soccer Field Goo, Close Quarters, ACM City and Afghanistan DM (3 warm-ups
each; saves without a mini-game borrow Soccer's with 4 bots), counted by the
new clunk counter (`crates/chaos/tests/watch/clunk.rs`).

Changes:
- Glimpse memory: a target lost from sight stays the target for 0.5 s
  (no firing on it), so a flickering sight line no longer flips the bot
  between fight and search, with goal, look and stance flipping too.
- Route bridge: while a new route is searched, the bot keeps walking the old
  route's first plain walking leg when it still heads toward the new goal.
- A failed step is avoided across the body's width, and string-pulling no
  longer straightens a path back through an avoided step.
- One sideways mover: a ranged fighter's left/right fight spot sets the
  strafe's side instead of becoming a route, so route and strafe no longer
  pull it opposite ways.
- Spawn bricks place feet as `fxDTSBrick::getSpawnPoint` does (centre - 1.3,
  or 0.1 above the first other brick or terrain below centre + 1.5). Before,
  feet were at centre + 0.1: on Afghanistan players spawned 1.4 too high with
  their heads in the ceiling of the 3-unit spawn rooms, and bots wedged there
  until they respawned themselves.

Before (A0) / after (A4), totals over 3 matches per save:

| Save | walk hand-overs | look hand-overs | fight/search flips | self kills | stuck |
|---|---|---|---|---|---|
| Soccer | 110 → 67 | 7 → 4 | 9 → 5 | 0 → 0 | 1.4% → 0.4% |
| Close Quarters | 207 → 120 | 165 → 73 | 127 → 42 | 9 → 6 | 10.5% → 6.0% |
| ACM City | 155 → 97 | 89 → 41 | 74 → 28 | 3 → 4 | 2.6% → 2.7% |
| Afghanistan | 157 → 72 | 117 → 43 | 86 → 29 | 14 → 5 | 8.6% → 5.2% |

Not done (dropped for v0.2.7 to save usage): head snaps, Close Quarters fall
deaths, the jeep driver running over teammates, the stuck-detector merge.
