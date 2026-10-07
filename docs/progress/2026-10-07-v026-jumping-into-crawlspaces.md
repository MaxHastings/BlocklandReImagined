# 2026-10-07 v0.2.6: jumping up into crawlspaces

Branch `claude/v0.2.6-measured-moves-x42j1c`, after the measured jets and
jump height entry. Max asked whether bots understand that crouching lets
them fit smaller spaces, and that climbing into a window up a wall can need
jump, crouch and forward together; no special cases. It is the same
measured mechanism: nothing is named a window.

## What changed

- `reach::Reach::crawl_ledge`: the highest crawlspace floor a walk-and-jump
  lands it in, crouching once off the ground, measured like `ledge` under
  a roof one crouched body (plus half a cell) above it. Stock body: 3.50,
  the same as a plain ledge.
- `nav::Body::crawl_jump`, from it. A step up onto a crawl floor uses it
  instead of `jump`; the rule that never let a jump land on a crawl floor
  is gone. Crawl floors are looked for from that height down.
- Dropping out of a crawlspace sweeps the crouched body when the standing
  one does not fit where it starts (it could jump in but never drop out).
- `session/bots.rs` (shared, small): the walk leg's crouch waits until the
  bot is off the ground when the same waypoint jumps, as the measurement
  does it. `route::JUMP_TAKEOFF` is renamed `route::PRESS_NEAR`, since
  crouching uses the same distance.
- The vehicle body sets `crawl_jump` like its `jump`: none.

## Tests

- `nav`: `a_window_up_a_wall_is_jumped_into_crouching`.
- `reach`: the standard body lands in a crawlspace at its measured
  `crawl_ledge`, not higher, and never higher than its plain `ledge`.
- `bri-chaos` `bot_routes`:
  `a_bot_jumps_crouching_through_a_window_up_a_wall_to_reach_its_enemy`
  (a roofed room whose only way in is a 1.8-high window up its wall, sill
  1.8). The bot jumps, crouches in the air, crawls along the sill and drops
  in. The first setup put the sill above eye height, so the bot never saw
  its enemy and only wandered: a setup mistake, not a bot one.
- Cost: crawl measurement adds about as much as the ledge (about 10 ms
  release, once per tuning).

## Not done / next

- Vehicles: measured stopping distance and turning arc.
