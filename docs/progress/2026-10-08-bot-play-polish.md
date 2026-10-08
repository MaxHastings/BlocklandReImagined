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
- Strafing: the stance strafe. A cover step-out (left/right fight spot)
  records where the bot stepped out from (`stepped_out`), and the strafe does
  not carry the bot back behind that cover. This replaces a sideways-merge
  attempt, which broke grenade play, and a stand-room bound that left bots
  standing still.
- Stuck recovery: `route::Progress::stalled` stays the one judge (hop, then
  replan, then give up); the trapped clock still leads to Respawn.
- Goals: idle-goal writes from the door goof, hand-offs and surprise goofs go
  through `Brain::idle`, consumed by the Wander behaviour, instead of each
  setting the goal directly.
- The vehicle detour is applied to the walk before safety, not inside it.

Play polish kept from earlier in the thread:
- Glimpse memory: a target lost from sight stays the target for 0.5 s (no
  firing), so a flickering sight line no longer flips fight and search.
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
this branch (A5): hand-overs are the clunk counter's per-bot-minute rates
averaged over the matches, flips and self kills are totals, stuck is the mean.

| Save | walk hand-overs | look hand-overs | fight/search flips | self kills | stuck |
|---|---|---|---|---|---|
| Soccer | 110 → 98 | 7 → 7 | 9 → 2 | 0 → 0 | 1.4% → 0.9% |
| Close Quarters | 207 → 158 | 165 → 77 | 127 → 66 | 9 → 9 | 10.5% → 5.6% |
| ACM City | 155 → 143 | 89 → 44 | 74 → 32 | 3 → 0 | 2.6% → 2.5% |
| Afghanistan | 157 → 101 | 117 → 46 | 86 → 21 | 14 → 12 | 8.6% → 6.9% |

The reverted sideways merge had fewer walk hand-overs (route and strafe were
one mover) but broke grenade play; the remaining hand-overs are the route
handing over to the stance strafe on arrival, which is the intended split.

Evidence (cloud, Linux): `bri-sim` lib 365 passed;
`bots_play_an_unfamiliar_package_by_its_own_rules` and
`team_fill_bots_play_slayer_soccer_with_or_without_weapons` pass. The full
gate runs on the PC before landing.

Not done for v0.2.7: head snaps, Close Quarters fall deaths, the jeep driver
running over teammates.
