# 2026-10-11 Bots jump into a crawlspace at the step limit

A 3x soak of the bot gauntlet (`BRI_GAUNTLET_ROUNDS=3 cargo test -p bri-chaos
--test bot_gauntlet`) had `a_weapon_in_a_cubby_a_step_up_is_fetched` at 7.8%
stuck, the only scenario over 5%.

Cause: the gun sits on a block 1.0 high under a roof 2.0 over it. The route
grid (`nav::edge`) let a crawlspace floor be walked up crouched as high as
`Reach::crawl_step`, measured in open air as the full step (1.0). Under the
real roof the motor does not climb a crouched step at exactly that limit, so
the bot pressed against the lip at zero speed, crouched and walking, until
the stall judge hopped it after a second (traced per tick: forward 1.0,
crouch on, velocity 0 for about 130 ticks, then the stall hop).

Fix: a crawlspace floor is walked up only when it is at least `CRAWL_MARGIN`
(0.1) under the measured crouched step. Higher ones take the existing jump
(crouching in the air, `crawl_jump`).

Evidence:
- New unit test `nav::tests::a_crawlspace_floor_at_the_step_limit_is_jumped_not_walked`
  fails with the margin at 0 and passes with it.
- The cubby scenario: stuck 3.4% to 0.0% at 1x, 7.8% to 0.1% at 3x, and both
  bots arm (one before). Its test now asserts stuck under 2%.
- `cargo test -p bri-sim --lib` and the chaos suites `bot_gauntlet`,
  `bot_routes`, `bot_navigation_spike`, `bot_brain`, `bot_interactions`
  and `bot_physics_interactions` pass. `stairs_to_a_deck` is unchanged
  at 0.4%.
