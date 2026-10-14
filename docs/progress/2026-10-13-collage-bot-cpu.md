# 2026-10-13 Bot CPU on dense builds (Collage, 32 bots)

Max reported lag on "Crazy Collage Madness" (187,762 bricks) with 32
deathmatch bots carrying rocket launchers, spears and HE grenades. These
are the bot-side fixes from that investigation, built on the CTF bots
branch (#138) under Max's one-bot-PR rule. The parked-vehicle fix from
the same investigation is a separate physics PR.

Profiled headless with `crates/chaos/tests/bot_deathmatch_stress.rs`
(release, 4-core 2.1 GHz cloud Xeon, so absolute numbers run high) and
`samply record -r 2000`. Runs are deterministic: the same build gives the
same deaths and nav counts. Report: project files,
`bots/collage-32-bot-lag-2026-10-13.md`.

```
BRI_CONTENT=<content> BRI_STRESS=1 BRI_STRESS_WORLD=<collage.world.json> BRI_STRESS_BOTS=32 BRI_STRESS_SECONDS=30 \
BRI_STRESS_LOADOUT=v20.weapon.rocketlauncheritem,v20.weapon.spearitem,weapon_hegrenade:weapon/hegrenadeitem,v20.weapon.gunitem,v20.weapon.sworditem \
cargo test --release -p bri-chaos --test bot_deathmatch_stress -- --ignored --nocapture
```

## Changes

1. **Shove landing** (`reach::shove_landing`). It simulates a shoved
   target's flight with the player motor, but stepped without the chunk
   part tags, so the motor polygonised every compound brick chunk it
   crossed on every simulated tick. It was about 27% of the whole tick. It
   now steps with `step_among` and `simulation.chunks()`, like every other
   motor caller.
2. **Stand searches** (`nav::Search::limited`, `STAND_EXPANSIONS`). JET
   and HOP stand goals, often tabletops 3-4.4 units up, ran to 6000
   expansions and failed. 54 exhausted stand searches in 30 s were 44% of
   all expansions, and 90% of the stand places found are found within
   1500. Stand goals now stop at 1500. Other goals keep 6000.
3. **Forgetting only what a change alters** (`Nav::invalidate_all`). A
   brick change forgot every column within a body's width plus a cell.
   A column's sample looks no farther across than the body it places,
   pushed at most half a cell off the centre (`sample_reach`), so only
   those columns are forgotten now. The test
   `a_change_forgets_every_column_it_alters` samples every column round
   bricks of several sizes and alignments, with the brick and without it,
   and checks that each column whose sample changes is forgotten.
4. **Brick-bounded rays** (`WeaponQuery::sweep`,
   `Simulation::target_filtered`). Weapon, aim and sight rays leave bricks
   out of the physics ray and walk bricks on their own grid. The physics
   ray still walked the broad phase the whole length, turning down every
   brick chunk it passed. The nearest brick (and terrain) hit now bounds
   it. A hit level with the brick still wins the tie as before.

## Evidence

Collage, 30 s, step mean and worst step:

| Build | Step mean | p99 | Worst | Floor samples | Forgotten |
|---|---:|---:|---:|---:|---:|
| v0.3.0-experimental | 9.4 ms | | | | |
| + 1 and 2 | 6.4-7.1 ms | 14 ms | 17 ms | 250,275 | 134,279 |
| #138 head + 1 and 2 | 7.7-7.8 ms | 17.5 ms | 28-59 ms | 254,215 | 164,259 |
| #138 head + 1 to 4 | 7.7 ms | 16.4-16.9 ms | 23-28 ms | 228,601 | 98,348 |

Afghan DM with the same bots, #138 head + 1 and 2 against + 1 to 4: step
mean 4.58 to 4.33 ms, bot thinking 3.96 to 3.73 ms per tick.

#138's own changes cost about 1.3 ms a tick on Collage (6.4 to 7.7 ms).
Jetting bots now get a 32-unit drop, so each floor sample's rays run much
deeper through stacked builds, and arc sweeps rose from 49k to 57k.

## What is left

On Collage, bot thinking is about 70% of the tick. Path search is about
36% of bot time, and 60% of that is floor sampling: rays and box tests
against the brick chunk compounds. Weapon choice is 29%: clear-path rays
9%, shove landing 8%. Sight is 9%. No single system runs away. The cost
is the number of physics queries against dense chunk compounds. A
cheaper floor sample, such as walking the brick grid instead of the
chunk BVH, is the next lever. It is a bigger change.

## Tests

`cargo clippy -p bri-sim --all-targets -- -D warnings` and
`cargo test --release -p bri-sim`.
