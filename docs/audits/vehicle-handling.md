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

## The Warthog's tags

Halo PC 1.0's `bloodgulch.map` (`vehicles\warthog\mp_warthog`, `...\warthog`
physics), read with reclaimer; the full table is in the project's
`v0.3.0/assets/halo/FEEL-NOTES.md`. Halo runs 30 ticks a second; 1 Halo
world unit is 4.12 units here. Per-tick values times 30 are inferred, not
tag values.

| Tag | Halo | Here | The buggy |
|---|---|---|---|
| Max forward speed | 0.275 wu/tick | 34 u/s | `max_speed` 34 |
| Max reverse speed | 0.12 wu/tick | 14.8 u/s | `reverse_speed` 14.8, `reverse_limited` (new) |
| Speed acceleration | 0.0028 wu/tick² | 10.4 u/s², 3.3 s to the top | flat `engine_curve`, `engine_force` 5400: 10.2 u/s² |
| Speed deceleration | 0.011 wu/tick² | 41 u/s², 0.8 s from the top | `throttle_brakes` (new), `brake_force` 30000: stops from 34 in 1.08 s, as hard as the tyres hold |
| Max steering | ±30° | 0.5236 rad | `max_steering` 0.5236 |
| Powered wheels | front and back | four-wheel drive | all four `powered` |
| Ground depth (suspension travel) | 0.15 wu | 0.62 | `rest_length` 0.62; hubs 0.08 higher than before so the deck rides as high |
| Grip along / across, front | 0.75 / 0.45 | | `static_friction` 3.0, `lateral_grip` 0.6 (new) |
| Grip along / across, rear | 0.45 / 0.65 | | `static_friction` 1.8, `lateral_grip` 1.44 |
| Centre of mass | 0.107 wu ahead of the origin, 57% on the front axle | | `mass_center` z -0.2 (57% front); springs 10300 front, 7700 rear so the deck sits level |

Grip is Halo's times 4, this tyre model's scale (kinetic friction three
quarters of it). At 5 or 6 times it stops nearer Halo's 0.8 s (0.88 s at 5)
but the rear no longer steps out in a hard turn; the slide was kept.

The weight forward first came with the old even springs and shorter
travel: the nose sat 0.3 lower, and weaving at full throttle the hull
scraped the ground. Each scrape is a contact the driver's prediction
cannot replay exactly (the solver's carried impulses are the host's own),
so the driver was corrected by up to 0.79 units. The springs now share
the weight as the axles do and the deck clears the ground:
`a_nose_heavy_buggy_weaving_on_the_ground_agrees_with_the_host`
(`crates/sim/tests/vehicle_prediction.rs`) predicts 10 s of weaving with
no error at all. Halo's tags have no spring or damper constants, engine
torque, gravity or rear steering; those, the look steering and the 0.85
gravity are the buggy's own, chosen by feel and measured below.

## What a Warthog does, and how the format says it

| Warthog trait | Field(s) | Before W8 | Now |
|---|---|---|---|
| Heavy, planted chassis, nose-heavy | `mass`, `mass_center`, `inertia_box` | yes | buggy 900 mass, 57% on the front axle |
| A steady pull to its top speed | `engine_force`, `engine_curve` | torque fell straight to 0 at `max_speed` (v20) | `engine_curve` (new): torque share by speed share, piecewise linear |
| Pull back and it stops hard | `throttle_brakes` | the reversed engine pushed back, next to nothing near top speed | `throttle_brakes` (new) |
| Backs up slower than it drives | `reverse_speed`, `reverse_limited` | wheeled vehicles reversed as fast as they drove | `reverse_limited` (new) |
| Grips differently along and across each tyre | `tire.lateral_grip` | one friction circle | `lateral_grip` (new): an ellipse |
| Long, soft suspension that soaks landings without pogoing | `rest_length`, `spring`, `damping`, `rebound_damping`, `anti_sway` | one damper both ways | `rebound_damping` (new) damps extension apart from compression |
| Four-wheel drive, and steering that turns tight | `wheels[].powered`, `wheels[].steering` (negative turns the other way) | yes | buggy: all four driven, rears steer -0.35 |
| Slides through a hard turn and comes back | per-wheel `tire` | yes | the rear slides in a hard turn, then straightens |
| Steers toward where the driver looks (Halo's camera steering) | `steering.toward_look` | the mouse turned the wheels (v20 jeep) or the strafe keys did | `toward_look` (new): the wheels turn the hull toward the driver's free look, easing off to arrive without overshoot |
| Calm at speed, tight when slow | `steering.speed_lock` | lock was the same at every speed | `speed_lock` (new): the share of `max_steering` left at `max_speed` |
| Floaty jumps | `gravity_scale` | gravity was engine-wide | `gravity_scale` (new), buggy 0.85 |
| Hard to roll, flips over in a bad crash, players flip it back | `anti_sway`, `mass_center`; the click flip (`Vehicle::onActivate`) | yes | stays on its wheels at full lock and full speed |
| Driver, passenger who shoots their own gun, gunner on a 360° turret | `seats[]` with `controls`, `weapon`; `attachment_*` turret | yes | the three Playground seats; the gunner's turret now aims against the hull's heading after the step |
| Bots drive it | the bot's pure pursuit, the handling measured from the definition | mouse steering only | bots and the handling measurement steer a `toward_look` vehicle by looking |

Every new field is optional and its default reproduces v20 exactly: an
empty `engine_curve` is v20's straight fall, `rebound_damping` 0 is the
single damper, `gravity_scale` absent is v20's gravity, `speed_lock` 1 is
no narrowing, `lateral_grip` 1 is the circle (the force is divided by 1,
which changes no bits), and `throttle_brakes` and `reverse_limited` off are
v20's engine. `an_empty_engine_curve_is_torques_straight_fall_exactly` and
the unchanged v20 handling tests prove the imported vehicles drive bit for
bit as before.

## Measured (the Playground buggy, retuned)

| Test | Result |
|---|---|
| `the_warthog_pulls_steadily_to_its_tag_top_speed_and_tracks_straight` | 10.1, 20.2, 30.4, 33.8, 34.0 u/s after 1 to 5 s (Halo: 10.4 u/s², 34 top); linear torque reaches 20.1 in 3 s; no sideways drift |
| `pulling_back_stops_it_hard` | from 34: stops in 1.08 s pulling back, 4.86 s with v20's reversed engine, 3.96 s coasting |
| `the_buggy_carries_the_warthogs_tag_numbers` | the tag numbers above; backs up at 14.8 u/s |
| `rebound_damping_soaks_a_landing_on_long_springs` | no bounce after a drop, one without rebound damping |
| `four_wheel_steering_turns_tighter` | turning circle 8.35 with the rears steering against 11.53 front-only |
| `it_drifts_through_a_hard_turn_and_comes_out_straight` | the rear slides in a hard turn, then the yaw settles (0.03 rad/s) and the heading holds |
| `it_steers_toward_where_the_driver_looks_and_straightens_on_arrival` | asked to face 1.2 rad round, it turns 1.18 and stops turning |
| `a_lighter_gravity_hangs_longer_off_a_jump` | 0.83 s hang time at 0.85 g, 0.76 s at 1 g |
| `it_stays_on_its_wheels_at_full_lock_and_full_speed` | worst roll 0.005 rad |
| `a_buggy_pinned_against_someone_who_holds_their_ground_never_speeds_up` | never over its 16 u/s; before the fix it left at 390 and 470 u/s |
| `a_buggy_runs_a_stomper_over` | an empty buggy launched at a Stomper hurts it |
| `a_bot_drives_the_look_steered_buggy_at_a_runner` | a Blockhead Bot boards it, turns round and drives from 45 units to within 20 of a Runner, where it may climb to the gun |

Two engine faults the retune turned up are fixed with it:

- **A vehicle pinned against a player gained speed.** A vehicle that hits a
  walking player shares the hit with them by weight (`share_contacts`).
  It shared the solver's whole impulse, including its push out of an
  overlap; against someone who did not give way (a Stomper holding its
  ground, a player with a wall at their back) the share sent the hull back
  in faster each step until it flew off at hundreds of units a second. The
  share now never sends a vehicle on into a body faster than it came.
- **An empty vehicle's run-over was its victim's.** A player's own bump
  into a vehicle credits them as the one who moved it, so an undriven
  vehicle that hit them counted as run by its victim and did no harm. The
  victim's own credit is now skipped and the vehicle's owner runs it, as
  before the bump.

## Three in one Warthog over the Internet

`three_in_the_buggy_over_a_bad_connection_do_not_rubber_band`: driver,
passenger and gunner, 150 ms each way, up to 40 ms more jitter, 5% of
datagrams lost each way, frames of 6 to 25 ms; 14 s of weaving at full
throttle with a hard brake, about 290 units driven (350 in the Halo-tuned buggy). Each player runs this
client's own code: the driver predicts the buggy, the riders draw the host's
poses on their own server clock, the gunner aims locally.

| Measure | Before | After W8 | Halo-tuned buggy |
|---|---|---|---|
| Driver: visible corrections after the first 3 s | 6, worst 0.43 units | 2, worst 0.16 units (eased, never a jolt) | 2, worst 0.29 units: one tick at 34 u/s |
| Driver: frames whose drawn velocity jumped over 8 units/s | 0 | 0 | 0 |
| Riders: frames run past the newest pose | 2 of 725 | 2 of 725 | 2 of 725 |
| Riders: farthest drawn from the host | 0.024 units | 0.024 units | 0.020 units |
| Gun on the host against the gunner's look in the move it ran | 0.020 rad worst | 0.0006 rad | 0.0007 rad |
| Gun on the host behind the gunner's screen (the trip there) | not measured | 0.035 rad worst, 0.014 mean: the one-way trip, not a fault | 0.021 rad worst, 0.010 mean |

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

## The chaingun

The gunner's post fires as the Warthog's `warthog gun` tag: held, 8 rounds
a second rising to 15 over the first second; each round strays up to 1°
rising to 2° over three seconds of fire; 293 wu/s (1207 units/s); no kick
(Halo 1 has no recoil field). Before, the post fired one bolt a pull, at
most 10 a second, at 140 units/s, and each bolt kicked the buggy back
5 units/s, which a stream of 15 a second would have turned into a shove.
The aim deviation (10°, the turret's slack against the camera) is not
modelled: the turret already follows the gunner's look at the post's own
turn rate.

`the_gunners_chaingun_spins_up_spreads_and_does_not_kick` holds the trigger
four seconds: 12 rounds in the first second, 15 a second once spun up, the
first rounds within 1° of the aim and the last within 2°, the buggy moved
under 0.05 units by 60 rounds, and a fresh pull starts slow again. The
spread is the same on every machine for a vehicle and tick.

## Not the vehicle format's job

| Gap | Where it goes |
|---|---|
| Engine sound rising with throttle and speed, tyre squeal | W8 sound buses (next) |
| Chase camera that lags and swings like Halo's | W6 camera ownership |
| Dust and skid marks keyed on wheel slip | effects: trails today run on speed only |
| The bounce Max saw when boarding from above | not reproduced headless: the hull moves under 2 mm once mounted, before and after the retune; Max's playtest decides |
