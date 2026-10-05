# 2026-10-05 Bot route planner: gauntlet root causes after the release merges

Branch `fix/bot-route-planner`. Follows
`2026-10-05-bot-route-planner-integration.md`. Merged, with merge commits:
`fix/runover-and-jeep-physics` 8560ee2c, `claude/project-thread-pt64ji`
fe74899d (extras: one strength dial), 07ce549a (perception) and a00626eb
(think-time refactor, no behaviour change). No limit was loosened and no
test skipped.

## Root causes and fixes

- **rooftop_brawl_without_rails idle 22%.** One fighter fell off the deck
  and lived on below it, so the other side wandered with an unreachable
  enemy. Two ways off were found from traces:
  - a dodge hop near the edge left a ranged fighter strafing in the air;
    its momentum carried it over. `route::landing` predicts where the arc
    comes down (velocity, gravity, the floor below); with no floor there
    the strafe steers against its drift;
  - `team::exit` (stepping out of an ally's line of fire) pointed past the
    edge. A way out now needs floor to stand on, by either side.
- **a_jeep_on_each_side stuck after the jeep-physics fix.**
  - A bot on a jeep roof or a head over a quarry under 1.5 units away
    walked nowhere: it now steps off to the nearest open world floor (no
    wall, no player, not under a vehicle), toward the target first
    (`bot_step_off`).
  - A driver whose chassis can run over its on-foot target, with no route
    left (too close), drives at it; `route::gear` backs out of the circle.
  - A runover seat nearer than the enemy serves the fight like an armed
    one.
  - Tried and backed out: stepping off for any walk waypoint below (stuck
    8.1% over three rounds); cutting chase routes at vehicle hulls (idle
    47-67%).
- **checkpoint_race circling 21%.**
  - Walkers passed every body in the way on their own left, so two already
    offset crossed in front of each other. One already off to the left is
    passed on the right.
  - The team overlap term made a racer give way to an earlier ally making
    for the same checkpoint (flips to Return). A step not planned for a
    mini-game team is the bot's own (`objectives::View::shared`) and no
    longer crowds by place.
  - A pulled walk also keeps clear of where another body is going over the
    next second (`nav::MOTION_AHEAD`).
- **fair_hit_rate 12.3% after the perception merge.** The coin-flip strafe
  carry-on from the earlier merge drifted fighters out of their band (gun
  range 12 to 23 units). Strafe legs alternate again: 27.2% steady.

## The other suites (after the gauntlet work)

- **bot_brain jetting onto a high platform.** A chasing jetter's landing
  was its enemy's own cell, so it braked onto their head and stood there:
  the jet leg only failed on coming down below the landing. The landing
  now moves to the nearest free floor beside a body on the goal, a cell's
  leeway off it for the touchdown drift, and that landing counts as
  arrival (`nav` guard test). A jet leg that comes down off the landing's
  height either way is given up and replanned (`route` guard test).
- **bot_extras door on the route.** The planner marked an enemy behind a
  shut door out of reach (the best route ended at the door), so the bot
  wandered and never walked up to click it. A route that ends with a door
  the bot may activate across the way on no longer gives the chase up
  (`bot_opens_way`).
- **bot_routes jeep beside the pitch.** The objective retry wait carried
  across a change of game, round or team, so after each round reset a bot
  went up to two seconds with no plan and fought: it pushed the match ball
  at an enemy, or boarded the jeep to run the host over. A new context now
  gets its planning turn at once, and until that turn a bot takes up no
  body or seat (`objectives::State::unplanned`).
- **bot_soccer_match 2v2 obstacles, wrong way 0.12.** Every wrong-way tick
  traced was a goal going in: the measure took the way to the defended
  goal from the ball's position, so a ball pushed over the line into the
  attacked goal's pocket (past that goal) read as pushed backwards. It now
  takes the way of the defended goal: the same anywhere on the field, and
  a push into a team's own pocket still counts. Wrong way reads 0.00-0.03
  on every line-up.
- **bot_soccer_teams contested ball, swapped sides: nearest 4.39 > 4.0
  (open).** The overtaking bot goes round the moving ball to the push
  approach's tangent point, 4.3 units off its centre. Unpulled, the grid's
  route keeps the lane closest to the ball (3.5 off), which is what brings
  release (and this branch with pulling off) under 4. String-pulling walks
  the straight line to the tangent point instead, and the other bot
  scores first. Not loosened; fixing it means a closer go-round geometry
  in `push_approach` or a pull that keeps that lane, both outside this
  pass.

## Results at 2fff1747 (gauntlet, one round)

| Scenario | Result |
|---|---|
| checkpoint_race | circling 12.8%, pass |
| a_jeep_on_each_side | stuck 2.3%, circling 0.0%, mounted 9234 < 10000: FAIL (3 rounds: pass, stuck 6.8%) |
| deathmatch_open_field | 39 kills < 40: FAIL (pulling off: 41; 3 rounds: 128 with pulling, 125 without) |
| water_between_the_sides | switches 27.9: FAIL (bots lane changes the switch count) |
| rooftop, stairs, CTF, zombies, weapons, runners, mixed arsenal, approach timeout, fair_hit_rate | pass |

The release tip 07ce549a on its own reads a_jeep stuck 9.5% and
deathmatch 41 kills (bar 40).

## Open

- deathmatch_open_field's one-round bar (40) sits one under the release
  tip's own 41; with pulling the first round reads 39, three rounds read
  higher with pulling than without. Not loosened.
- a_jeep_on_each_side one-round mounted_ticks: the round's dismounts are
  the standoff leave (a jeep cannot hurt a mounted target) and two wander
  dismounts after the target was lost from sight.
- water_between_the_sides: owned by the bots lane.
