# 2026-10-08: bot brain ownership cleanup and play polish

Thread "How bots really play". Goal (Max): exactly one owner per bot
responsibility, root causes of the soccer and grenade regressions fixed, no
new layers. The ownership map and its reasoning are in the project notes
(`v027/bot-ownership.md`).

Owners after this change:
- Movement direction: the proposal resolver (`act::resolve`). Final safety
  (`bot_safe_walk`) only takes walking away: it refuses a step into a live
  blast, over an edge or through a portal off the route, and never steers.
  The old `bot_blast_walk`, which replaced the direction from inside safety,
  is gone.
- Grenade avoidance: escaping a live blast the bot stands in is an explicit
  Dodge decision in `bot_extras` (`bot_blast_escape`); safety's
  `bot_blast_refuses` only vetoes stepping into one.
- Strafing: the stance strafe, unchanged from main. A sideways-merge
  attempt, a stand-room bound and a cover step-out ban (`stepped_out`) were
  each tried and removed: the grenade regression they were aimed at was the
  glimpse memory (below), and the step-out ban made a bot backing out from
  under a low roof blast itself (`bot_tactics`).
- Stuck recovery: `route::Progress::stalled` stays the one judge (hop, then
  replan, then give up); the trapped clock still leads to Respawn.
- Goals: idle-goal writes from the door goof, hand-offs and surprise goofs go
  through `Brain::idle`, consumed by the Wander behaviour, instead of each
  setting the goal directly.
- The vehicle detour is applied to the walk before safety, not inside it.

Removed after the first PC gate (2026-10-08, c8a4f64): the 0.5 s glimpse
memory, which put a target lost from sight back into sight. The bot then
reported an unseen enemy as visible (`bot_knowledge` x4, `bot_interactions`),
a relaxed bot's reaction was cut short (`bot_perception`), and grenade bots
lost their throws. It also duplicated the hold rule (`Ask::paused`), which
already owns keeping a fight through a moment out of sight. A flicker fix
belongs in that rule.

Play polish kept from earlier in the thread:
- Route bridge: the old route's first walking leg is kept while a new route
  is searched, if it still heads toward the new goal.
- Failed steps are avoided across the body's width; string-pulling no longer
  straightens back through an avoided step.
- Spawn bricks place feet as `fxDTSBrick::getSpawnPoint` does (centre - 1.3,
  ray from + 2.8 down, 0.1 above the hit), counting map floors, and never
  inside the spawn brick itself. The soccer regression was feet placed inside
  the spawn plate.
- `bot_watch` counts jitter from the controls a bot actually pressed.

Loop counts (`bot_watch`, three 2-minute matches per save, 4 bots; saves
without a mini-game borrow Soccer's), main before this thread (A0) against
this branch without the glimpse and step-out ban (A6): hand-overs are the
clunk counter's per-bot-minute rates averaged over the matches, flips and
self kills are totals, stuck is the mean.

| Save | walk hand-overs | look hand-overs | fight/search flips | self kills | stuck |
|---|---|---|---|---|---|
| Soccer | 110 → 102 | 7 → 9 | 9 → 10 | 0 → 0 | 1.4% → 0.4% |
| Close Quarters | 207 → 198 | 165 → 133 | 127 → 128 | 9 → 3 | 10.5% → 3.4% |
| ACM City | 155 → 138 | 89 → 75 | 74 → 78 | 3 → 4 | 2.6% → 3.2% |
| Afghanistan | 157 → 90 | 117 → 66 | 86 → 59 | 14 → 11 | 8.6% → 5.9% |

Fight/search flips are back at main's level without the glimpse; that
flicker is open work for the hold rule.

Evidence (cloud, Linux): `bri-sim` lib 365 passed;
`bots_play_an_unfamiliar_package_by_its_own_rules` and
`team_fill_bots_play_slayer_soccer_with_or_without_weapons` pass, as do the
seven tests the first PC gate failed. The full gate runs on the PC before
landing.

Not done for v0.2.7: fight/search flicker (in the hold rule), head snaps, Close Quarters fall deaths, the jeep driver
running over teammates.
