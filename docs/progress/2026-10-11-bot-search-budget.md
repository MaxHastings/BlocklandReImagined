# 2026-10-11 Path-search budget grows with the bot crowd

v0.2.10 bot hardening pass, slice 2 (navigation), branch
`claude/v0210-bot-hardening-kgf5ku-nav`.

## Found

A probe of 16 bots a side (32) crossing a map of walls, a deck and stairs
showed bots standing at their spawns in "search" or "chase" with a goal and
no route (`next=None`, `steps=0`) for 40 seconds and more. The gauntlet's
stuck score put a fifth of all bot time there over two minutes, and 46% of
the first half minute.

## Cause

`Nav` gives all searches of one body kind a shared budget per tick:
`SAMPLES_PER_TICK` (96) ground samples, `EXPANSIONS_PER_TICK` (384) node
expansions and `ARCS_PER_TICK` (24) leap arcs. It was sized for the default
16 bots and did not change with the crowd, so at 32 (and the 63 the stress
harness reaches) each search got half or less of what it needs and long
routes waited many seconds for their turn.

## Change

`Nav::begin_tick_for(bots)` multiplies the three budgets by the number of
groups of `BOTS_PER_BUDGET` (16) bots, rounded up. `step_bots` passes the
bot count. 16 bots or fewer are unchanged. The budgets are ceilings, spent on
demand, so a quiet crowd costs nothing extra.

Cost, debug build, `bot_think_time_32` (32 bots fighting, all dials on): bot
think 5159 to 5436 us/tick, whole step 7612 to 8008 us/tick (+5%). Searches
are rare in a fight, so the cost lands where bots are crossing the map.

## Tests

- `bot_hardening::thirty_two_bots_set_off_across_a_walled_map_without_waiting_to_find_a_way`:
  stuck share of the first 30 s is 46.4% before, 4.4% after (bar 12%).
- `nav::tests::the_tick_budget_grows_with_the_crowd_in_groups`.

## Not measured

Release-build cost on Max's PC at 32 and 63 bots. The stress harness
(`bot_deathmatch_stress`) needs the Afghan DM content; re-run it there.
