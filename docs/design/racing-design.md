# Player-made races: design (2026-11-04)

Status: design only. No engine or content change in this PR. Written with no machine run, so every claim about driving and runtime behaviour is marked as unverified until a run on Max's PC.

## Summary

Map makers build a race from wrench event rows. The engine gets no race mode. Bots and human players drive the same course, and what the engine adds is generic: a seated driver counts as the Player for region inputs, and the bot planner can send a driver to a region goal.

## What exists today (checked this session)

- **Workshop race recipe.** `crates/sim/src/session/rules.rs`, `mode == "race"` (about line 1358). It builds three checkpoint bricks, indexed 0 to 2. Each brick has rows on `onRegionEnter` and `onObjectEnter`, gated by: the round is running, the trigger has a Player, and the Player's `checkpoint` variable equals the brick's index.
  - Checkpoints 0 and 1 set the Player's `checkpoint` variable to index + 1.
  - The finish (index 2) adds 1 to the Player's score, then calls `winRound` when `checkpoint` is 2 and the score is at least 3. It then resets `checkpoint` to 0.
  - So one lap is the three bricks in order. A lap count is the Player's score, and the round is won at three laps.
- **On-foot gauntlet test.** `checkpoint_race` in `crates/chaos/tests/bot_gauntlet.rs` runs four on-foot bots through three laps of this recipe (from the 2026-11-04 notes, not re-read this session).
- **Planner.** The objective planner reads rule goals as facts. `objectives/project.rs`, `discover_rule_causes`, lists `onRegionEnter` alongside `onActivate` and `onBotTouch` as causal inputs, with the actor set to the bot. Checked this session.
- **Driving.** `crates/sim/src/session/bots/interactions.rs` (about 2090 to 2240) steers, brakes to a safety corner speed (`route::pace`) and leaves a wrecked hull (`Leave::Wrecked`). It is tuned for pursuit and seats, not lap time.

## Gaps (each one is a test, not an assumption)

1. **Does a seated driver trigger a checkpoint?** The notes say region and object inputs observe only bots (`crates/sim/src/session/events.rs`, about line 590, `observed_actor` filters `is_bot`). I did not re-read `events.rs` this session because it is large. Unknown until run on a machine.
2. **Can the planner send a driver to a checkpoint?** Planner movement is on foot. No path routes a seated driver to a region goal. Expected to fail today.
3. **Racing line, recovery, lanes.** No racing line, no flip or reset back onto the track, no pass or yield rule.
4. **Planner budget.** The V0.2.1 playtest notes say the planner handled four independent latches and failed at eight. A race with more checkpoints or laps is unmeasured.
5. **Performance.** Crowd cost and physics per car are unmeasured at 8, 16 and 32 cars. The October lag note is stale.

## Design

**Race is data.** A map maker places checkpoint bricks and a finish with the recipe's rows. The course, lap count and win rule live in the map, not in code.

**Engine adds only generic behaviour:**
- Gap 1 fix (if it fails): region and object inputs treat a seated human or bot driver as the Player, the same as on foot.
- Gap 2 fix (if it fails): a driver can take any region goal. The route goes to the region, not a race-specific goal.

**Then driving skills, in order:** racing line with a braking point ahead of each apex, then recovery back onto the track, then lane offset and a pass or yield rule. Each one is judged against the baseline from step 1 below.

**Transfer.** Steering, braking, recovery and passing are vehicle skills, so TDM and other vehicle modes benefit too.

## Steps that need Max's PC

These need a machine run. Nothing here has been run.

1. **Baseline (test only, no brain change).** Extend `checkpoint_race` to jeeps: three laps, three checkpoints on a loop. Record lap times, completions, wrecks and bot-to-bot contacts. This answers gaps 1 and 3 with data.
2. **Driver trigger check.** A human or bot in a jeep crosses a region brick with the recipe. Expected: the Player `checkpoint` variable updates. If it does not, gap 1 is confirmed.
3. **Driver routing check.** A bot in a jeep with an ordered checkpoint objective. Expected: it fails today. This confirms gap 2.
4. **Planner limit.** Race with 4, 6 and 8 checkpoints. Record where planning fails.
5. **Performance.** 8, 16 and 32 cars on the race course. Record physics and tick time.
6. **Regression.** Gauntlet bands for jeep and tank behaviour must pass with and without the race rules.

## Decision for Max

Race content is built by map makers from wrench rows, and the engine adds only generic driving and routing. The alternative is an engine race mode. We recommend the first.

## Risks

- Racing-line tuning may regress jeep and tank pursuit behaviour. Gauntlet bands guard this.
- Without lanes, a pack of cars will wreck at corners. A yield rule needs testing with 6 to 8 cars before any higher count.
- Fairness: bots must not be unbeatable. The human-error band (surprise, mood) must apply to driving too.

---
_Generated by [Claude Code](https://claude.ai/code)_
