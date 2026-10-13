# 2026-10-13 Collage 32-bot deathmatch lag

Max reported lag on "Crazy Collage Madness" (187,762 bricks, 10 vehicle
spawns) with 32 deathmatch bots carrying rocket launchers, spears and HE
grenades. Profiled headless with `crates/chaos/tests/bot_deathmatch_stress.rs`
(release build, 4-core 2.1 GHz cloud Xeon, so absolute numbers run higher
than on Max's PC) and `samply record -r 2000`. The full report is in the
project files (`bots/collage-32-bot-lag-2026-10-13.md`).

## Findings

v0.3.0-experimental (b551eff) and v0.2.12 (205b570) cost the same, so this
is not a regression. The baseline step mean was 10.4-11.4 ms, with 60-66% of
ticks over the 8.3 ms budget. Afghan DM with the same bots averaged 5.1 ms.
Bots were about 70% of the tick. Three costs were specific to Collage:

1. **Shove landing (about 27% of the tick).** `reach::shove_landing`
   simulates a shoved target's flight with the player motor. It called
   `step` without the chunk part tags, so the motor's `Soup::gather`
   polygonised every compound brick chunk the flight crossed, on every
   simulated tick. Stacks under it were truncated in the first profile,
   which is why the first report undercounted it.
2. **Stand searches.** JET and HOP stand goals (often tabletops 3-4.4 units
   up) ran `nav::Search` to `MAX_EXPANSIONS` (6000) and failed: 54 exhausted
   searches in 30 s were 44% of all expansions. 90% of the stand places that
   are found are found within 1500 expansions.
3. **Vehicles that never slept.** A jeep rocked between two contact poses
   on the studs (about 1e-4 u/s) and a tank crept, so Rapier never put them
   to sleep. The horse is a kinematic actor body, which Rapier only lets
   sleep at exactly zero velocity, and `interpolate_velocity` returns
   rounding noise at large coordinates. Leaning on a tank, it kept that
   tank's island awake too.

## Changes

- `shove_landing` takes the chunk `PartTags` and steps with `step_among`, like
  every other motor caller (`bots/shove.rs` passes `simulation.chunks()`).
- `nav::Search::limited` caps a search's expansions. Stand goals use
  `STAND_EXPANSIONS = 1500`. Other goals keep 6000.
- An undriven vehicle that has stayed below 0.05 u/s and 0.05 rad/s for
  120 ticks is put to sleep (`settle` in `crates/vehicles/src/world.rs`).
  For an actor body, the condition is grounded, zero motor velocity and
  an unchanged pose. Any hit, push, driver or awake neighbour wakes it as
  before. The counter is saved in `VehicleSave::still`.

## Evidence (Collage, 32 bots, 30 s)

| Build | Step mean | p99 | Worst |
|---|---:|---:|---:|
| Base | 9.4 ms | | |
| + stand cap | 8.6-8.7 ms | 28 ms | 55 ms |
| + shove fix | 6.4-7.1 ms | 14 ms | 17 ms |

These numbers are from the same machine at different times. The base here
read lower than the first runs (10.4-11.4 ms). Deaths rose from 64 to 77
with the stand cap, because bots spend less time stuck planning. Afghan DM
was unchanged. In a quiet match every vehicle now sleeps. In a 32-bot fight,
vehicles near the combat stay legitimately awake, so the physics gain there
is small.

## Next

After the fixes, bots are still about 69% of the tick: path search 24%,
weapon choice 21% (clear-path rays 7%, shove 6%), sight 7%. Candidates are
caching clear-path results per shooter and target cell, and making path
search forget less after explosions. Both must stay deterministic (no
wall-clock budgets).
