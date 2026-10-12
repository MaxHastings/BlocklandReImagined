# 2026-10-12 Bots stop standing still mid-chase and mid-search

Max's Skylands recording (`match-1`, v0.2.10, 32 bots, Slayer free-for-all)
replayed exactly on his PC with `bri-replay --bots`. In 59% of the seconds
a bot chased, searched or explored, it was not walking. Every one of those
seconds had no route (`path_steps 0`, no next waypoint).

## Causes

- **A route search started over before it finished.** Route searches share
  a budget of node expansions per tick, and with 32 bots one takes several
  seconds. A chase goal moved, and the search restarted, whenever the
  enemy moved more than 2.5 m. 982 of the 1,221 standing chase samples
  could see the enemy.
- **Search spots timed out while the route was still being found.** The
  enemy search gives the last-seen spot 3 s and each nearby probe 1 s,
  whatever the distance. The remembered spot was a median 85 m away, so
  spots ran out before their routes existed.
- **Bots kept searching after there was nowhere left to look.** Once every
  spot had failed, the bot still chose "search" until the memory expired,
  and stood sweeping its view. This made the long freezes: bot 31 stood
  about a minute through three memories, and there were 188 standing
  streaks of 10 s or more.

## Changes

- While a chase's route is still being found, the enemy may move a quarter
  of the way to the goal (at least 2.5 m) before the search starts over.
- An enemy-search spot's time starts once its route is found (or found
  missing). An earlier version also gave far spots extra time to walk
  there. CI's `loss_of_sight_search_reacquires_and_actually_wins_elimination`
  showed that this kept a bot walking a stalled route to a dead-end anchor
  for 5 s instead of moving on to the probes, so it was dropped.
- "Search" no longer fits a bot whose search of the current evidence is
  exhausted. New evidence starts a fresh search.
- The bot readout's planned-shot trade (`BotThought::planned.net`) now uses
  the healths the plan was made with, not the health after this tick's
  damage. Before, a bot hurt in the tick after planning showed a "bad
  plan" that the fire gate then refuses anyway.
  `vehicles_are_driven_and_crewed_in_a_team_deathmatch` hit this once with
  the route changes, and passes on main without them.

## Evidence

- `cargo test -p bri-sim --lib`: all pass. New search-memory tests: waiting for a route does not use up a
  destination, and exhaustion applies only to the evidence searched.
- `cargo test --release -p bri-chaos --test bot_gauntlet --test bot_search_objectives`:
  24 and 4 passed.
- `cargo test -p bri-sim --test replay`: 10 passed.
- `cargo clippy -p bri-sim --all-targets -- -D warnings` is clean.
- The full workspace suite runs in CI. The cloud runner (4 cores) could
  not build every release test binary in two hours.
- Not yet measured on Skylands: a new recording on this build would show
  whether the share of standing seconds drops.

## Not changed

- The dead end at (-83.1, 6.8, 33.6) on Skylands, where routes stop and
  new searches from it come back empty at once. This may be the island's
  edge; only positions were checked, not the map geometry.
- Replays need the content of the build that recorded them: match-1
  needed `E:\Downloads\BlocklandReImagined-v0.2.10-windows\content`, not
  the main checkout's.
