# 2026-10-07 The Ragdoll never switches itself off

Max, in a bot match: "this is dumb stopping ragdol physics lol", with the chat
showing "Ragdoll stopped: its physics were too heavy: 4.7 ms a frame for 30
frames in a row (budget 4.0 ms)".

The stop was in v0.2.5 (2cf0b3a4) and is still on main and in the d068f06e
preview: the earlier fix (93fbffae, merged) made ragdolls about five times
cheaper and stopped build changes waking them, but kept the cutoff. Under
enough deaths at once, or on a slower PC, it still fired, and every death
after it played the old animation for the rest of the session.

## Change

- The physics stop is gone: `Stopped::Physics`, `AddOn::report_physics_time`
  and `Budgets::physics_strikes` are deleted. `physics_ms_per_frame` (4 ms)
  stays as the cost bound.
- A frame whose Add-On physics took longer than that settles the oldest body
  still moving that lies on the world, with everything jointed to it (`AddOnPhysics::settle_oldest`):
  the oldest ragdoll lies still, new deaths keep tumbling. One group per
  frame over, so the cost falls back under the bound without a new number. A
  settled body moves again when something hits or holds it.
- "X stopped: why" lines for any Add-On stop (crash, CPU, GPU, device reset,
  start failures) go to the console and log (`bri_console::warn`), never the
  players' chat. Add-On log lines and the trust notices are unchanged.

## Evidence

- Settling is tested in `settling_lays_the_oldest_ragdoll_on_the_floor_still_never_one_in_the_air`
  (see Review fixes).
- `a_graphics_card_reset_stops_all_addon_code` now checks the chat stays empty.
- `cargo test -p bri-client --lib`, `cargo test -p bri-client-sandbox`,
  workspace `cargo clippy --tests -D warnings`, all at below-normal priority.

## Review fixes

- Settling only lays still a group already lying on the world: a body of it
  touching a fixed collider (map, brick, terrain) within the solver's
  `allowed_linear_error`. A frame over budget with only airborne groups
  moving settles nothing, so a corpse thrown by a blast never hangs in the
  air (`settling_lays_the_oldest_ragdoll_on_the_floor_still_never_one_in_the_air`).
- The 8 ms wall-clock limit per Add-On frame (`Budgets::frame_time`) could
  still stop the Ragdoll in a stall ("it took too long to respond"). Fuel
  already bounds a frame's work; the clock only backs it up for what fuel
  undercounts (bulk memory loops). It is now one `hang_time` for any call,
  the existing 1 s start-up allowance, which no lag spike reaches
  (`a_frame_stalled_far_past_a_display_frame_carries_on`: a frame stalled
  several display frames long carries on; `bulk_memory_loops_are_stopped_by_the_clock`
  still passes).
- Only "you have not trusted this server to run it" stays in the players'
  chat, since they act on it. Add-On log lines, code that cannot run, the
  trust file, sounds that do not play, elevated code left off and item skin
  failures go to the console.
- Only a floor holds a body up: the contact must face up past v20's floor
  rule, `FLOOR_DOT` (re-exported from the motor as
  `bri_sim::player::FLOOR_DOT`, one definition). A body pressed against a
  wall never settles there (`settling_never_sticks_a_body_to_a_wall`, which
  fails when walls count).
