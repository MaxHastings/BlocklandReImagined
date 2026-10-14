# 2026-10-13 Parked vehicles come to rest

Found while profiling Max's "Crazy Collage Madness" save (187,762 bricks,
10 vehicle spawns) with 32 deathmatch bots. The bot fixes from the same
investigation go into the CTF bots PR (#138), under Max's
one-bot-PR rule. This entry covers only the physics fix.

## Cause

Three of the parked vehicles never slept, so every tick their hull
compounds generated contacts against the brick chunk compounds under them:

- A jeep rocked between two contact poses on the studs, at about 1e-4 u/s.
  Rapier measures that rocking as motion.
- A tank crept slowly upward.
- The horse is a kinematic actor body. Rapier only lets a kinematic body
  sleep at exactly zero velocity, and `interpolate_velocity` returns
  rounding noise (about 0.002 u/s 150 units from the origin) for the same
  pose twice. Leaning on a tank, the horse kept that tank's island awake
  too.

## Change

`settle` in `crates/vehicles/src/world.rs`: an undriven vehicle that stays
below 0.05 u/s and 0.05 rad/s for 120 ticks (one second) is put to sleep.
For an actor body (the horse), still means grounded, zero motor velocity
and the same pose as last tick. A hit, a push, a driver, a brick removed
from under it, or an awake neighbour wakes it as before. The tick counter
is saved in the vehicle checkpoint (`VehicleSave::still`).

## Evidence

Collage with no bots: every vehicle now sleeps, where
before three stayed awake for the whole run. Emptying the vehicle spawns
had cut the non-bot part of the tick from about 2.5 to 1.2 ms. In a 32-bot
fight, vehicles near the combat stay legitimately awake (explosions and
bodies keep waking them), so the gain there is small.

The full investigation report is in the project files
(`bots/collage-32-bot-lag-2026-10-13.md`).
