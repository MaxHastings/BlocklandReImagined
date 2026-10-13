# Vehicle handling: can an Add-On drive like a Warthog?

W8, 2026-10-13. Max's bar: "this Warthog drives like the one in the real
game, but I am in Blockland", for three friends in one Warthog over the
Internet. This audit lists what a Halo: Combat Evolved Warthog does, what
`vehicles.json` could already say, what was missing and is now added, and
what is still not the vehicle format's job. Numbers come from headless
tests; Max's playtest judges the feel.

Evidence: `crates/vehicles/tests/handling.rs` (the buggy alone),
`crates/sim/tests/playground.rs` (bodies, seats and a bot at the wheel),
`crates/client/src/crew_link_tests.rs` (three players over a bad link),
`crates/net/tests/playground.rs` (real QUIC).

## What a Warthog does, and how the format says it

| Warthog trait | Field(s) | Before W8 | Now |
|---|---|---|---|
| Heavy, planted chassis | `mass`, `mass_center`, `inertia_box` | yes | buggy 900 mass, centre of mass 0.05 above the hull origin |
| Strong pull off the line that fades near top speed | `engine_force`, `engine_curve` | torque fell straight to 0 at `max_speed` (v20) | `engine_curve` (new): torque share by speed share, piecewise linear |
| Long, soft suspension that soaks landings without pogoing | `rest_length`, `spring`, `damping`, `rebound_damping`, `anti_sway` | one damper both ways | `rebound_damping` (new) damps extension apart from compression |
| Four-wheel drive and four-wheel steering | `wheels[].powered`, `wheels[].steering` (negative turns the other way) | yes | buggy: all four driven, rears steer -0.35 |
| Rear lets go first: it drifts and comes back | per-wheel `tire` friction and stiffness | yes | buggy: rear static/kinetic friction 2.6/1.8 against 3.2/2.4 front |
| Steers toward where the driver looks (Halo's camera steering) | `steering.toward_look` | the mouse turned the wheels (v20 jeep) or the strafe keys did | `toward_look` (new): the wheels turn the hull toward the driver's free look, easing off to arrive without overshoot |
| Calm at speed, tight when slow | `steering.speed_lock` | lock was the same at every speed | `speed_lock` (new): the share of `max_steering` left at `max_speed` |
| Floaty jumps | `gravity_scale` | gravity was engine-wide | `gravity_scale` (new), buggy 0.85 |
| Hard to roll, flips over in a bad crash, players flip it back | `anti_sway`, `mass_center`; the click flip (`Vehicle::onActivate`) | yes | stays on its wheels at full lock and full speed (worst roll 0.075 rad) |
| Driver, passenger who shoots their own gun, gunner on a 360° turret | `seats[]` with `controls`, `weapon`; `attachment_*` turret | yes | the three Playground seats; the gunner's turret now aims against the hull's heading after the step |
| Bots drive it | the bot's pure pursuit, the handling measured from the definition | mouse steering only | bots and the handling measurement steer a `toward_look` vehicle by looking |

Every new field is optional and its default reproduces v20 exactly: an
empty `engine_curve` is v20's straight fall, `rebound_damping` 0 is the
single damper, `gravity_scale` absent is v20's gravity, `speed_lock` 1 is
no narrowing. `an_empty_engine_curve_is_torques_straight_fall_exactly` and
the unchanged v20 handling tests prove the imported vehicles drive bit for
bit as before.

## Measured (the Playground buggy, retuned)

| Test | Result |
|---|---|
| `the_warthog_pulls_hard_to_its_top_speed_and_tracks_straight` | faster than linear torque one second in; sideways drift under 0.05 over the run (the old buggy pulled left) |
| `rebound_damping_soaks_a_landing_on_long_springs` | fewer bounces after a drop than the same springs without rebound damping |
| `four_wheel_steering_turns_tighter` | turning circle 7.52 with the rears steering against 10.44 front-only |
| `it_drifts_through_a_hard_turn_and_comes_out_straight` | the rear slides in a hard turn, then the yaw settles and the heading holds |
| `it_steers_toward_where_the_driver_looks_and_straightens_on_arrival` | asked to face 1.2 rad round, it turns 1.216 and stops turning |
| `a_lighter_gravity_hangs_longer_off_a_jump` | more hang time at 0.85 g than at 1 g |
| `it_stays_on_its_wheels_at_full_lock_and_full_speed` | worst roll 0.075 rad |
| `a_bot_drives_the_look_steered_buggy_at_a_runner` | a Blockhead Bot boards it, turns round and drives at a Runner |

## Three in one Warthog over the Internet

`three_in_the_buggy_over_a_bad_connection_do_not_rubber_band`: driver,
passenger and gunner, 150 ms each way, up to 40 ms more jitter, 5% of
datagrams lost each way, frames of 6 to 25 ms; 14 s of weaving at full
throttle with a hard brake, about 290 units driven. Each player runs this
client's own code: the driver predicts the buggy, the riders draw the host's
poses on their own server clock, the gunner aims locally.

| Measure | Before | After |
|---|---|---|
| Driver: visible corrections after the first 3 s | 6, worst 0.43 units | 2, worst 0.16 units (eased, never a jolt) |
| Driver: frames whose drawn velocity jumped over 8 units/s | 0 | 0 |
| Riders: frames run past the newest pose | 2 of 725 | 2 of 725 |
| Riders: farthest drawn from the host | 0.024 units | 0.024 units |
| Gun on the host against the gunner's look in the move it ran | 0.020 rad worst | 0.0006 rad |
| Gun on the host behind the gunner's screen (the trip there) | not measured | 0.035 rad worst, 0.014 mean: the one-way trip, not a fault |

Six link seeds give the same picture (0 to 3 corrections, all under 0.24
units). The fixes:

- **Skipped moves.** Each movement datagram repeated 6 moves (50 ms); two
  lost in a row skipped moves on the host and corrected the driver by a
  tick or two of travel. It now repeats 12 (100 ms), doubling the
  bytes of moves a player uploads (`MOVEMENT_REDUNDANCY`; one protocol
  change file).
- **A draining seated queue.** The host's queue of a seated player's moves
  drained back to a spare of 2 every 2 s, below what this link's jitter
  needs, and starved again: each starved tick is a tick of travel the
  driver is corrected by. The spare is now 4 (`SEATED_SPARE`), 17 ms more
  for others to see the driver's inputs, nothing for the driver.
- **The gun a tick behind the hull.** The gunner's look was turned into a
  turret angle against the hull's heading before the step that turns it.

What remains for the driver is the host meeting a late burst for the
first time: it runs one tick without their move, their prediction is
corrected by that tick's travel (0.1 to 0.2 units at speed), eased in.

## Not the vehicle format's job

| Gap | Where it goes |
|---|---|
| Engine sound rising with throttle and speed, tyre squeal | W8 sound buses (next) |
| Chase camera that lags and swings like Halo's | W6 camera ownership |
| Dust and skid marks keyed on wheel slip | effects: trails today run on speed only |
| The bounce Max saw when boarding from above | not reproduced headless: the hull moves under 2 mm once mounted, before and after the retune; Max's playtest decides |
