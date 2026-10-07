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
  still moving, with everything jointed to it (`AddOnPhysics::settle_oldest`):
  the oldest ragdoll lies still, new deaths keep tumbling. One group per
  frame over, so the cost falls back under the bound without a new number. A
  settled body moves again when something hits or holds it.
- "X stopped: why" lines for any Add-On stop (crash, CPU, GPU, device reset,
  start failures) go to the console and log (`bri_console::warn`), never the
  players' chat. Add-On log lines and the trust notices are unchanged.

## Evidence

- New `settling_lays_the_oldest_moving_bodies_still_and_leaves_the_rest`:
  two falling jointed chains; settling lays the older one still for the
  following frames while the newer keeps falling, then the next, then
  nothing is left moving.
- `a_graphics_card_reset_stops_all_addon_code` now checks the chat stays empty.
- `cargo test -p bri-client --lib`, `cargo test -p bri-client-sandbox`,
  workspace `cargo clippy --tests -D warnings`, all at below-normal priority.
