> **Historical note. Written 2026-10-11 (the file's date in the cloud project folder, `bots/bot-performance-2026-10-11.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Bot performance on Afghan DM: measurements

Thread: v0.2.9 bots (blocker 4). Branch `claude/v029-bots-hardening-crlqsb`.

## How this was measured

- A new headless harness, `crates/chaos/tests/bot_deathmatch_stress.rs` (ignored test, needs generated content). It loads the real Afghan DM save (14,091 bricks, converted from Max's `.bls`) through the ordinary dedicated-server loader, enables `blockhead_bot`, `gamemode_slayer` and `weapon_hegrenade`, creates a minigame with the v20 gun, rocket launcher and HE grenade, and fills it with Slayer's free-for-all bots. A host player stands at the spawn and watches.
- It steps the real `Session` and times every step. It also runs the server's own pose stream (`bri_net::stream::StateStream`) for the host every pose interval, and replays what the host's client would draw of each bot at 144 fps with the client's interpolation rules (render delay from pose rate, its slew, bounded extrapolation; copied from `crates/client/src/motion.rs`). The replay runs twice: once as if every step took no time (pose rate and interpolation alone) and once with the steps as long as they were measured, through the server loop's 120 Hz wakeups and 8-step catch-up.
- Release build, cloud container: 4 cores of a 2.1 GHz Xeon. Max's PC is faster, so absolute numbers there should be lower. The simulation is deterministic: two runs of the same code give the same deaths and the same slow ticks, so before/after comparisons are like for like.
- Command (from the repo root):
  `BRI_CONTENT=<content> BRI_STRESS_WORLD=<afghan.world> BRI_STRESS_BOTS=32 BRI_STRESS_SECONDS=60 cargo test --release -p bri-chaos --test bot_deathmatch_stress -- --ignored --nocapture`

## Is it sustained?

Yes. 16 bots for 3 game minutes before any change: every 10-second window had a step mean between 4.6 and 7.5 ms, and bot thinking was 87-90% of every window. It drifts up, not down, as the fight goes on (bricks knocked out, more rockets).

## Scaling (server step, ms; 120 Hz budget is 8.3 ms)

| Bots | Code | Mean | p99 | Worst | Ticks over 8.3 ms | Worst 1 s mean | Bot share |
|---|---|---:|---:|---:|---:|---:|---:|
| 16 | before (180 s) | 6.01 | 21.4 | 91.8 | 18.0% | 11.6 | 89% |
| 16 | after (60 s) | 4.09 | 12.4 | 21.5 | 6.1% | 5.9 | 88% |
| 32 | before (60 s) | 7.92 | 23.2 | 86.5 | 29.8% | 11.9 | 86% |
| 32 | after ray fix | 5.98 | 19.0 | 81.8 | 14.7% | 8.7 | 85% |
| 32 | after both fixes | 5.68 | 15.1 | 36.9 | 11.9% | 8.1 | 84% |
| 63 | after both fixes | 8.12 | 16.3 | 27.5 | 36.7% | 10.8 | 81% |

63 is the most the harness can fit: 64 players including the host. The bot caps were raised to 64 locally for that run only and reverted; nothing about the caps is committed.

Cost grows less than linearly with bot count (32 bots cost 1.3x 16) because the weapon chooser shares one solve budget per tick across all bots.

No bot ever skipped an input on the host (0 held ticks out of 170,000 to 350,000 living bot-ticks per run) and no bot moved more than 1.75 units in one tick. The server's own simulation of the bots is smooth.

## Where the time goes (32 bots, before the fixes, perf sampling)

Share of all `Session::step` time:

- `step_bot` 78% in total. Inside it:
  - Weapon choice (`hand_combat::choose`): 24.5%. Of that, clear-path checks 12.8% and validating fire/intents about 10%.
  - Sight lines (`bot_sees` and `eye_or_chest`): 17.5%.
  - Path search (`nav::Search::step` 16.7%, `Nav::floor` 9.5%).
- Clearing path caches after brick changes (`Nav::invalidate`): 5.1%.
- Weapons step (projectiles): 9.5%.
- Add-On scripts: about 4%.

The single hottest leaf was the persistent world map's brick lookup (10.6% self time) plus string compares (5.8%), almost all from brick rays (sight lines, clear paths, projectile sweeps) looking up every brick in each grid bucket the ray crossed before testing whether the ray came near it.

The 50-90 ms spikes are different. Profiling only inside the slowest steps: 77-90% of each was `Nav::invalidate`. A rocket knocks out dozens of bricks in one tick, and the path cache was walked once per knocked-out brick for every body kind.

## What the host's client draws

| Run | Pose rate and interpolation alone | With measured step times |
|---|---|---|
| 16 bots, after | error over 1 unit 0.02%, 0 jumps | 0.02%, 0 jumps |
| 32 bots, after ray fix | 0.06%, 0 jumps | 0.05%, 3 jumps a minute |
| 32 bots, after both | 0.05%, 0 jumps | 0.05%, 0 jumps |
| 63 bots, after both | 0.05%, 0 jumps | 1.68%, 495 jumps a minute, worst error 8.2 units, 13% of moving frames frozen |

"Error" is the distance between where a bot is drawn and where its body really was at the moment drawn. A "jump" is a frame where the drawn bot moves more than 0.75 units and over three times its real step.

So the pose stream and interpolation on their own are not the cause: far and crowded bots are sent at 20, 10 or 5 Hz (by distance and by rank past the nearest 12), the client renders them further back to match, and they stay within half a unit 99% of the time. The teleporting appears when the host's step time stops fitting in the tick: a long spike, or an average near 8.3 ms, delays every pose send, the client runs past its newest pose, holds the bot, then catches it up in one jump. That matches Max's description: bots skip, the world stays smooth.

## Fixes so far (on the branch)

1. `Simulation::walk_bricks`: the cheap slab test against the grid bounds the bucket already holds now runs before the world lookup and the `accept` filter. Results are identical; it only skips bricks the ray cannot reach. 32 bots: mean 7.9 to 6.0 ms, ticks over budget 30% to 15%.
2. `Nav::invalidate_all`: one pass over each path cache per tick for all of that tick's collision changes, with change boxes within a body's reach of each other merged first. It forgets a superset of what it used to (never less), so paths stay correct; some samples are re-taken. Worst 32-bot tick 85 to 37 ms.

## Not yet known

- Numbers on Max's own PC, where the host's client also renders in the same process.
- Rallypack weapons were not in these runs (their `cmd_shot` script currently fails on `ceil`, which the Add-On thread owns).

## Next, in order of measured cost

- Path search and floor sampling (26%).
- Weapon choice clear-path and validation rays (25%).
- Sight lines (17.5%).
