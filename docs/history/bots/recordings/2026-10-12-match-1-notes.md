> **Historical note. Written 2026-10-12 (the file's date in the cloud project folder, `bots/recordings/2026-10-12-match-1-notes.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# match-1 (2026-10-12, v0.2.10 f4ce0ae): what the recording shows

Source: `2026-10-12-match-1-f4ce0ae.brimatch` (copied from Max's PC). It was read straight from the file, without replaying it: the cloud has no content folder. The raw dump is in `2026-10-12-match-1-readout.txt`: B lines are each bot's F3 readout once a second, C lines are Max's commands, and the tick number divided by 120 gives seconds since hosting started.

## The game
- Skylands, Slayer free-for-all, `ffa_bots` set to 32, created at 0:28. Ten minutes and 22 seconds in total (74,605 ticks), with about 7 min 45 s of bots (465 readouts per bot).
- Max respawned 29 times. The only refused calls are 6 tool or trigger presses while dead ("Dead players cannot do that"), which is normal.
- No failed or crashed calls, and the file ends cleanly.
- The host kept up with real time: 621.7 s of game time against 622 s of wall clock, so there was no sustained slowdown. The recording keeps no frame timings, so short hitches can't be seen in it.

## Worth going after: bots standing still while chasing or searching
Seconds spent in each behaviour, and how many of them had `walk none` (not moving):

| behaviour | seconds | not moving |
|---|---|---|
| fight | 6152 | 0 (it uses stance or dodge) |
| search | 4293 | 2587 (60%) |
| chase | 2903 | 1620 (56%), mostly while it **sees** its target |
| explore | 942 | 478 (51%) |
| wander | 210 | 107 |

There were 28 stretches where a bot stood still for 15 s or more outside a fight. The longest:
- bot 31, 3:57 to 4:30 (34 s), searching with `look sweep`.
- bot 33, 4:11 to 4:37 (27 s), searching, then exploring.
- bot 10, 7:39 to 8:04 (26 s), searching.
- bot 15, 6:01 to 6:24 (24 s) and 4:14 to 4:36 (23 s).
- bot 7, 5:54 to 6:17 (24 s).
- bot 29, 1:48 to 2:10 (23 s), exploring and seeing nothing.

Guess, not yet checked: on Skylands the target or remembered spot is often on another island with no walkable route, so the bot picks chase or search but has nowhere to walk. The once-a-second readout has no position, route or path length, so this cannot be confirmed from the file alone.

Next step: replay it on Max's PC with `bri-replay <content> match-1.brimatch --bots 3:55-4:32` (PR #89). That prints the bot's position, route steps, goal and next waypoint, and would show whether it is stuck with no route or standing still on purpose.

Other numbers: each bot changed behaviour 90 to 162 times in 465 s, about once every 3 to 4 s.
