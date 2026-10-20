> **Historical note. Written 2026-10-14 (the file's date in the cloud project folder, `bots/collage-32-bot-lag-2026-10-13.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Crazy Collage Madness, 32-bot deathmatch: where the CPU goes

Thread: "Collage 32-bot lag", 2026-10-13. Fixes: parked vehicles in PR #142 (against v0.3.0-experimental); bot fixes on branch `claude/collage-bot-speed-u23psp`, to be folded into the CTF bots PR #138.

## Setup

- Save: Max's `Crazy Collage Madness.world.json` (187,762 bricks, Bedroom map, 10 vehicle spawns: 5 jeeps, 3 tanks, 1 flying jeep, 1 horse), copied from his PC.
- Content: the 19 packs `base-packages.json` names plus `addons/`, `Add-Ons/`, `shipped-addons`, `minigames-pack-002`, copied from his main checkout.
- Harness: `crates/chaos/tests/bot_deathmatch_stress.rs` on `v0.3.0-experimental` (b551eff), release build, 4-core 2.1 GHz cloud Xeon (slower than Max's PC, so absolute numbers run high).
- 32 Slayer free-for-all bots, loadout rocket launcher, spear, HE grenade, gun, sword. 30 game seconds after a 4 s warm-up. Deterministic: repeated runs give the same deaths and nav counts.
- Profiles: `samply record -r 2000`, symbolised with `addr2line -i`; spike ticks profiled by matching the harness's "slow step" timestamps to sample times.

```
BRI_CONTENT=<content> BRI_STRESS=1 BRI_STRESS_WORLD=<collage.world.json> BRI_STRESS_BOTS=32 BRI_STRESS_SECONDS=30 \
BRI_STRESS_LOADOUT=v20.weapon.rocketlauncheritem,v20.weapon.spearitem,weapon_hegrenade:weapon/hegrenadeitem,v20.weapon.gunitem,v20.weapon.sworditem \
cargo test --release -p bri-chaos --test bot_deathmatch_stress -- --ignored --nocapture
```

## Result

| Run | Step mean | p99 | Worst | Ticks over 8.3 ms | Bot thinking per tick |
|---|---:|---:|---:|---:|---:|
| Collage, v0.3.0-experimental | 10.4-11.4 ms | 29-32 ms | 43-60 ms | 59-66% | 8.0-8.9 ms |
| Collage, v0.2.12 (205b570, what Max plays) | 10.9 ms | 30 ms | 46 ms | 62% | 8.6 ms |
| Collage, vehicle spawns emptied | 10.3 ms | 32 ms | 56 ms | 53% | 9.1 ms |
| Afghan DM, same bots and loadout | 5.1 ms | 11 ms | 60 ms | 4.5% | 4.5 ms |

v0.3.0 is not a regression against v0.2.12. Collage costs about twice what Afghan DM does with the same bots.

## Where an average Collage tick goes (11.4 ms run)

| Part | Share | ms per tick | vs Afghan DM |
|---|---:|---:|---|
| Bot path search (`nav::Search::step`: floor sampling, leaps, jet lift-off checks) | 31% | 3.6 | 1.4x |
| Bot weapon choice (`hand_combat::choose`: clear-path rays, shove landing, aim) | 17% | 1.9 | 2.2x |
| Bot sight lines | 7% | 0.8 | 1.3x |
| Rest of the bot brain (stand spots, extras, surprise, arming) | 16% | 1.9 | |
| Physics (Rapier contacts, almost all parked vehicles vs brick chunks) | 13% | 1.5 | about 20x |
| Player movement | 5% | 0.6 | |
| Weapons and projectiles | 3% | 0.35 | |
| Add-On scripts, vehicles' own step, other | 7% | 0.8 | |

Bots are about 71% of the tick. The 30-60 ms spikes have the same mix (bots 77%, path search 33%, weapon choice 23%): no single runaway system, just bursts of the same work.

## Why Collage is heavier

- Path search: the per-tick budgets (192 samples, 768 expansions for 32 bots) count work, not time. On Collage each unit costs more: stacked floors mean several layers per column, and rockets and grenades knocking out bricks make the grid forget far more (180,000 samples forgotten in 30 s against 22,000 on Afghan), so bots keep re-sampling.
- Weapon choice: its clear-path and surprise rays cross a much denser brick field.
- Physics: two tanks and a jeep on Collage never settle. They jitter at a few mm/s (and explosions keep waking them), so their hull compounds keep generating contacts against the brick chunk compounds every tick. Emptying the vehicle spawns cut the non-bot part of the tick from about 2.5 ms to 1.2 ms. The horse never sleeps by design (it is an actor).

## Correction: shove landing was the biggest single cost

The share table above undercounted one thing. Samples under `reach::shove_landing` had truncated stacks that never reached the bot root, so they were missed. Counted across all samples, `shove_landing` was about 27% of the tick and weapon choice as a whole about 38%. It simulates a shoved target's flight with the player motor, but called `step` without the chunk part tags, so every simulated tick polygonised whole brick chunks.

## Fixes (targeting v0.3.0-experimental)

1. `shove_landing` steps with the chunk part tags (`step_among`), like every other motor caller.
2. Stand goals (JET/HOP spots) cap their search at 1500 expansions instead of 6000. 54 exhausted stand searches were 44% of all expansions; 90% of found stand places are found within 1500.
3. Undriven vehicles that stay slower than 0.05 u/s for one second are put to sleep, and the horse counts as still when grounded and unmoving. Before, a jeep rocking on studs, a creeping tank and the horse (which kept a tank awake) never slept.

| Build (same machine, later runs) | Step mean | p99 | Worst |
|---|---:|---:|---:|
| Base | 9.4 ms | | |
| + stand cap | 8.6-8.7 ms | 28 ms | 55 ms |
| + shove fix | 6.4-7.1 ms | 14 ms | 17 ms |

Afghan DM was unchanged. Deaths rose from 64 to 77, because bots spend less time stuck planning. In a quiet match all vehicles now sleep. In a 32-bot fight, vehicles near the combat stay legitimately awake, so the physics gain there is small.

After the fixes, bots are still about 69% of the tick: path search 24%, weapon choice 21% (clear-path rays 7%, shove 6%), sight 7%, physics 9.5%.

## More bot fixes, on top of #138

4. A brick change forgets only the path-grid columns it can actually change (a sample looks no farther across than the body it places plus half a cell). Forgotten samples on Collage fell from 164k to 98k.
5. Weapon, aim and sight rays bound their physics ray by the nearest brick hit, so they stop walking the whole broad phase past it.

On #138's branch: Collage bot thinking -3% (step mean 7.7 ms either way, worst 28-59 -> 23-28 ms), Afghan DM 4.58 -> 4.33 ms. #138 itself adds about 1.3 ms a tick on Collage (6.4 -> 7.7 ms): jetting bots' 32-unit drop makes each floor sample's rays run deeper.

## #138 at 4539126 (bot fixes folded in, jet-drop sampling fixed)

Same machine, back to back, two runs each: Collage step mean 5.9-6.1 ms against 6.4-6.6 ms for v0.3.0 with the shove and stand fixes alone. Ticks over 8.3 ms: 16-19% against 23-24%. The earlier +1.3 ms from #138 is gone. Afghan DM is unchanged at 4.5 ms.

## Tried and dropped

Checking `MAX_JET_TESTS` before `jet_edge`'s five roof rays (identical behaviour) saved 1-2%, inside run-to-run noise. Not worth a change.

## Next work

1. Make path search cheaper per unit on dense builds: forget less after explosions, or scale the expansion budget down when samples are expensive, keeping it deterministic (no wall-clock budgets: recordings replay bot decisions exactly).
2. Weapon choice rays: cache clear-path results across ticks for the same shooter and target cell.

Not measured: the host's own client rendering 188k bricks. It runs beside the server, so on Max's PC both compete for CPU.
