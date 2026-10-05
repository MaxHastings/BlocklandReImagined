# 2026-10-05 Bot think time: a third of what it was, behaviour bit-identical

Lane `fix/bot-perf` from `claude/project-thread-pt64ji` 07ce549a (the
perception merge). `bot_think_time_16` (16 bots, all dials on, 30 s of
fighting) read about 13 ms a tick in a debug build against the 15 ms bar,
with goofing and the mood cadence still to land on top.

## Where the time went

No profiler is installed here, so temporary counters timed each part of
`step_bot` and counted calls (not committed). Per tick, debug, at 07ce549a:

| part | time | calls |
|---|---|---|
| combat decision (`combat::choose`) | 6.7 ms | 12 |
| of which `clear_path` | 7.2 ms\* | 22 |
| of which the per-segment ally check (`bot_fire_clear`) | 3.4 ms | 89 |
| `bot_allies` (all callers) | 3.5 ms | 1382 |
| sight (`bot_sight`, the shared budget, rays) | 0.45 ms | 12 queries, 24 rays |
| glances (`bot_glance`) | 0.02 ms | 12 |

\* timers include their own overhead; the shares are what matter.

So the per-peer sight queries, salience scans and sightline cache were
not where the time went: perception's sight costs under half a
millisecond. The cost was the side lookup, `bot_allies` (seven or more
map lookups in a debug build), asked about every peer on every path
segment of every candidate shot. It predates perception; perception only
added to the time because fair aim keeps more bots alive and fighting
(12 decisions a tick, 10.2 before).

## What changed (no behaviour change)

- `combat.rs`: `Bodies` gathers the shooter's body and its living allies'
  once a decision (lazily, when a candidate first needs them; once in
  `validate_fire`). `safe_blast`, `clearances`, `clear_path` and `variant`
  read it instead of asking `bot_allies` about every peer on every
  segment. Nothing moves while a bot decides (`choose` reads `&Session`).
- `interactions.rs`: `shot_space` builds the swept space;
  `bot_fire_clear` tests the cheap geometry before the side lookup.
- Cheap conditions first, side lookup last (pure `&&`/`||`, so the
  result is the same): incoming projectiles (`extras.rs`, distance and
  heading before `bot_allies`), allies near a strafe (`bots.rs`), the
  ally-safe miss check (`perception.rs`).
- `bot_allies` looks each player's minigame record up once (the relation
  and `game_of` read the same one) and drops a branch that could never
  hold: `minigames.allied` is true only for two players of one game who
  both have teams, which the explicit relation had already answered.

## Evidence

Same state: a temporary hash of every player's health and spawn tick plus
every bot's `BotThought` after the 30 s run was `1ac6b077f89e88e5` before
and after each step.

Same behaviour: every bot suite run with full output on 07ce549a and on
this branch; the outputs, sorted and with timings removed, are identical
line for line (every gauntlet metric line, the fair table and the dial
sweep, soccer, tactics, brain, perception, extras, physical objectives,
interactions, objectives; `bri-sim` bots unit tests 140 pass). The only
differences are wall-clock times and the order parallel tests print in.
Fair metric, unchanged: gun 57.4% / 38.3%, rocket 35.3% / 15.0%, shotgun
5.6% / 10.5% (hopeless), bow 44.4% / 51.6%, bouncer 0% (hopeless), all
28.7%; `fair_by_dial` 38.2% / 28.7% / 20.9%. Gauntlet fails as on
07ce549a: `a_jeep_on_each_side` (route lane), `water_between_the_sides`
(bots lane), `all_dials_on` (known).

Think time, `bot_think_time_16` run alone, alternating trees (machine
load about 7 from other lanes; no quieter window came):

| tree | bot think | whole step |
|---|---|---|
| 07ce549a | 12.2 ms | 14.3 ms |
| this branch | 4.1 ms | 5.4 ms |
| fe74899d (before perception, same load) | 8.8 ms | 9.8 ms |

Commands: `cargo test -p bri-chaos --test bot_gauntlet bot_think_time_16
-- --include-ignored --exact --nocapture` per tree;
`cargo clippy -p bri-sim -p bri-weapons -p bri-chaos --tests -- -D
warnings`; `cargo fmt --all`.

## Next

- `team_intents` and `Bodies::of` are now the largest `bot_allies`
  callers (about 145 and 290 calls a tick); a per-tick side table would
  cut them further if needed, once it is clear nothing changes sides
  mid-tick.
