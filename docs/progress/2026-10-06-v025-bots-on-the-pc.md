# 2026-10-06 v0.2.5 bots on the PC: soccer, cleanup, failing tests

Branch `claude/v0.2.5-s4s5bn`, taken over from the cloud lane at 2e076a4
(see `2026-10-05-v025-bot-cleanup.md`). Max moved the bot work onto his PC
so every fix is checked on his own saves. Max's finish line for v0.2.5
bots: soccer scores goals on Soccer Field Goo with slot 1; no accidental
ledge deaths and nobody stuck long on Close Quarters Deathmatch; bots hunt
on the big maps; zero failing tests. Max also asked for the bot code to be
cleaned up and consolidated now, all of it.

## How it is checked on his saves

`crates/chaos/tests/bot_watch.rs` (on `claude/project-thread-6kc0fb`, not
in this branch: it is tied to Max's files) hosts a `.world.json` save
headless with his Add-On choices and an Add-Ons-window favourite, fills
bots and writes kills, stuck, idle, goof, falls, items, rounds won and odd
moments, with a trace for top-down frames. It runs against a release-style
content folder in a scratch directory: the base packs, a fresh import of
the bundled originals, and the branch's repository packages. The shared
dev `content/addons` imports were stale against main (Slayer's bot fill
failed in `bot_name`, so no bots appeared).

## Soccer on Soccer Field Goo (240b37c1)

Cause: the save's goal bricks raise Slayer's team score and also post a
chat line, play a sound, step a scoreboard digit on another brick and
respawn everyone. The planner treated each extra as an unknown output and
dropped the whole goal (the cloud lane's owner-indexing suspicion was
wrong: indexed sources by owner were `1: 441`, the game's owner 1).
`changes_nothing_planned` names the outputs that change nothing a plan
reads, used by both the projection and the reaction check; a print-count
step is a display unless it can wrap into its target's overflow rows.
Max's save, slot 1, 3 minutes, 8 bots: objective 0% -> 76% of bot time,
rounds won 0 -> 2 (Slayer's 3-point limit). Test:
`a_goal_that_announces_itself_is_still_delivered_to` (fails without).

Open: with the ball worth chasing, the sword bots no longer fight at all
(13 kills in 3 minutes, against about 80). Max wants both.

## Cleanup steps 1-5

| Step | Commit | What |
|---|---|---|
| 1 | 3b53a05d | 13 kind fields no bots.json could set (contest, mounted, fighting, hold) become constants beside their code; `MountAnchor`, `Geometry`, `switch_delay` gone |
| 2a | 38f9c2c0 | bots.rs split: `lifecycle.rs`, `sight.rs`, `fire.rs`, `hearing.rs`; `bot_thoughts` into `why.rs` |
| 2b | 27a1871d | step_bot hands off: dead and held bots, kind rules, `bot_hand`, `bot_evidence`, `bot_shot`, `bot_act` |
| 2c | 077ab679 | `bot_path`: the route step |
| 3 | 7878209d | objectives.rs split into `discover`, `project`, `turn` |
| 4a | 1a241522 | one `floor_below` probe, one `ticks()` |
| 5 | 195fa59d | the weapon hold uses the hold rule's pause (`paused_hold`) |
| 4b | c3304ae7, ffebcd78 | `set_goal_near`, `BotKind::weight` |

bots.rs 4,413 -> about 3,180 lines; step_bot 2,090 -> about 1,450.
The gauntlet's numbers stayed the same to the kill through each pure step.

Not done from the plan, with reasons: the `Leash` struct (it would wrap
two values used in five places; no rule merges); reaction, turn, aim error
and memory stay kind fields (tests set them to isolate mechanisms).

## Failing tests fixed

- `too_many_real_action_sources_report_the_specific_grounding_bound`
  (0ffad92a): a plan not found in a model cut down to its bounds reports
  the bound (`GroundingBudget::truncated`).
- `a_long_stare_turns_an_idle_bots_head` (ca331e69): a goof's gesture no
  longer takes the look from a glance.
- `rooftop_brawl_without_rails` (7a52bbb1): off its route a bot keeps to
  floor it can walk back up from (the edge guard stopped only falls that
  hurt, and six units did not); every goof or extra hop checks it lands
  (`hop_lands`, the dodge's check, now shared).
- Gauntlet off-target metric (41d27ac7): a shot is off target only when
  25 degrees off every point of the body, feet to head (a sword at a head
  a unit away was counted off a fixed point at the middle).

## Findings not acted on

- Slopes race freeze (watch loop round 2): the bots that never moved had
  spawned into the idle host standing on the spawn brick, an artifact of
  the watch runner's idle host. A probe shows overlapping bodies part as
  soon as they move apart; nothing was changed.
- `swords_four_a_side`: idle 8% and 60+ switches a bot-minute. The
  causes found: a bot hit from behind by someone it cannot see alternates
  chase and return; crowding allies push a chase under a stray's pull.
  Two attempted fixes made it worse and were reverted.

## Next

The full workspace test run for the remaining failure list, then the rest
of the cleanup (one give-up judge, one cooldown memory, chooser terms,
extras into the chooser), fighting as well as soccer, Close Quarters and
the big maps on Max's saves, soccer moves, jet dodge, ride-alongs.
