> **Historical note. Written 2026-10-05 (the file's date in the cloud project folder, `bots/second-opinion-2026-10-05.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Bot work: second opinion (2026-10-05)

This is a read-only review of the Stabilization thread's bot (NPC) work. I read the whole Stabilization thread, the bot memory files, `docs/architecture/bots.md` and `npc-edge-cases.md`, and checked the claims against the code.

- **Release branch:** `claude/project-thread-pt64ji` at e1e2c30.
- **Lane branches:** `fix/bot-perception` 7db3181 (WIP), `fix/bot-team-roles` 244f9f2 (WIP) and `fix/bot-route-planner` cab1f87.

Line numbers refer to the release branch unless a lane is named. Nothing was pushed, and no lane was messaged.

The marks against the frozen plan:
- **IN** means in scope: it would look broken on camera, or it is a risk of bots being stuck, crashing or lagging.
- **NEXT** means next version.

## Where it is solid

- **Multiplayer.**
  - Bot brains run only on the host (`session.rs:2627`).
  - On the network, a bot costs the same as a player.
  - No lane changes the network protocol (no `crates/net` or protocol-changes edits in any lane).
  - Glances only change aim, which is already sent. Teammate intents stay on the server. Callouts are ordinary team chat lines with a rate limit.
- **No special cases by name in game logic.** Soccer, CTF, Slayer, rocket, tank and similar names appear only in comments and test fixtures.
  - Objectives come from rule events and package queries.
  - Weapon numbers come from datablocks (`tactics.rs:378-500`).
  - One small exception: the hammer is recognised by the name `hammerImage`, with fixed damage 10 and reach 5 (`combat.rs:160-176`).
- **Honest knowledge.** Sight is a real line-of-sight ray. Memory holds dated evidence that expires. Search never reads a live hidden position (`bots.rs:1318-1321`).
- **Determinism.** Each bot has a seeded RNG, state is kept in ordered maps, and no decision uses the wall clock.
- **Stuck recovery.** The bot hops at 21 ticks without moving. At 45 ticks it replans. After 3 replans it drops the goal (`bots.rs:117-119`, `2918-2973`). Wedged-on-a-door and partial-route cases are covered too.
  - Gap: a bot that jitters back and forth on a chase or wander goal is never counted as stuck. Its movement per tick stays above 0.01.
- **Route planner.** It uses the shared navigation budget, and jet tests are capped at 24 per search. No navmesh is needed, so there is none to go missing.
- **Outside mini-games, bots are harmless by design** (`bots.rs:15-16`, `1250`). They wander within 12 units of their brick. I found no crash path.

## Blind spots, ranked

### 1. "All on by default" isn't true on any branch yet (IN)

Max was told at 02:07Z that surprise, teamwork and eyes are on by default. On the code:

- **Surprise:** `bots.json:34` still has `"strength": 0` with the comment "Off for now", on the release branch and on team-roles.
- **Teamwork:** every weight is 0 in code (`bot_kind/team.rs:12-34` on the team-roles lane), and `bots.json` has no `team` block.
- **Eyes:** on, but only in the unmerged WIP perception lane.

No chaos or gauntlet test runs with all three on. Only `surprise_by_strength` (`bot_gauntlet.rs:842`) raises strength.

**Fix:** turn all three on in `bots.json` and move the team settings into `bots.json` as part of the same merge. Rerun the gauntlet before any further lane merges, so later merges are measured with everything on.

### 2. Nothing measures "fair", and bots aim close to perfectly (IN for the measurement and the dial; NEXT for a player-facing difficulty setting)

**Aim error** is up to ±5°, redrawn every 0.4 s, and shrinks to about ±1.7° after 2 s of tracking (`bots.rs:2670-2679`). At mid range that stays inside a Blockhead's hitbox.

**Leading:**
- Supported weapons get an exact intercept on the target's current velocity, including projectile gravity (`tactics.rs:100-224`).
- The fallback leads using the target's exact live velocity (`bots.rs:2664-2668`).

**Reaction:**
- 0.35 s, counted from first sight (`bots.rs:2682-2686`).
- Turning is limited to 300°/s.

**Vision:**
- Bots see all the way round: there is no view cone (`bots.rs:1300-1355`). The perception lane only slows the reaction for targets behind the bot.
- Being hit gives a bot the attacker's exact position, even when it can't see the attacker (`note_hurt`, `bots.rs:581-596`). Allies then share that position.

Every gauntlet bar is bot vs bot. No number says whether a person can win a fight.

**Fix:**
- Fold aim error and reaction into the planned "alertness" dial. The perception lane already has the model.
- Add one headless number with a band: a bot's hit rate against a strafing dummy at a few ranges, per weapon type.
- Max's preview sets the final level. A difficulty setting in the mini-game window is next version.

### 3. The settings count is going up, not down (IN; it's step 3 of the plan)

There are 58 settings on the release branch today. The perception lane's `bots.json` block adds 21. The team lane adds about 10 more, in code only. That comes to about 89 after the merge, against a goal of about 6. No lane has cut any settings yet, and the auto sweep can't search 89 values in any useful way.

**Fix:** make "cut to your one dial" a condition for merging each lane, instead of a step after everything is merged. Perception is the first test of that: its 21 settings should come down to about 8.

### 4. Merge risk and work that exists only in one container (IN, process)

Every lane conflicts with the release branch on its own:

| Lane | Conflicts with the release branch |
|---|---|
| team-roles | 1 hunk in `bots.rs` |
| perception | 5 files, 5 hunks in `bots.rs`; clashes with the surprise block in `bots.json` and `bot_kind.rs` |
| route planner | 6 files, 9 hunks in `bots.rs`; 26 commits behind |

These lanes have **no branch on GitHub**: the hold rule, own goals, spacing, weapon upgrades, small choices, tuning tools and copy-what-works. They live only in the Stabilization cloud container, which already lost its worker once (20:56Z on 10-04). At the time of the edge-case audit, three lanes also existed only as uncommitted worktree changes.

**Fix:**
- Every lane pushes WIP to its own branch now.
- Merge in this order: team-roles, then perception, then the route planner after rebasing it onto the release branch.

### 5. Lag, and the "30+ bots" target (IN for the budget and perf bar; NEXT for raising the cap)

The server cap is 16 bots (`MAX_BOTS`, `package-runtime/src/ops/mod.rs:488`), so 30+ can't happen as things stand.

- Sight rays have no budget: each bot can cast one ray per enemy every tick (`bots.rs:1300-1353`). With portals, the distance filter is also skipped.
- The team lane's `team_mood` casts a ray to every player nearby, every tick, once its radius is set above 0.
- Both perf tests are ignored. They have no timing thresholds (`bot_battle_perf.rs:3`, `bot_combined_perf.rs:2-3`) and cover none of the new lanes.

**Fix:**
- Add the promised shared sight budget, shared like combat's `Budget`.
- Run `team_mood` every few ticks instead of every tick.
- Add one perf test at 16 bots with everything on and a frame-time bar.

### 6. Many Add-On weapons get the basic treatment (IN as a PC check; NEXT to broaden it)

A weapon gets the full data profile only if it has none of the following (`tactics.rs:384-458`, `combat.rs:157-207`):
- image scripts;
- spread or several projectiles;
- recoil;
- lob;
- child projectiles;
- cook or akimbo;
- melee without a projectile.

Shotguns, scripted guns, thrown grenades and scripted melee all miss the profile. They fall back to a path that:
- equips the **first** item in the inventory, not the best one (`bots.rs:3276-3295`);
- has no blast-safety check and never switches weapon.

Max's enabled set includes the HE grenade, sniper rifle, butterfly knife and hookshot.

**Fix:** step 4 of the plan (the PC check) should list which of Max's enabled weapons get a profile. Any that is in the video script and looks dumb is IN.

### 7. Bots never ride horses, planes or boats (NEXT, unless a video beat needs it)

Bots only take seats in wheeled ground vehicles that don't fly (`interactions.rs:310-313`). Horses are their own vehicle family, so bots never ride them. The route planner keeps the same limit.

Max asked earlier for variation across "all vehicles ... horses". Tell him plainly that this isn't in v0.2.4.

### 8. Scope crept around the freeze (process)

Between 02:46Z and 02:52Z these were added: copy-what-works, five small choices, five gap fixes, four tuning tools and the F3 "why" readout. After the freeze, three movement items went in at 02:56Z, and a "bot tells" audit started at 02:57Z that may add more. Each one may be justified, but together they make a lot of new work in the same files.

**Fix:** keep one written done-list next to the freeze. Anything added must name the camera moment it fixes.

### 9. Claims made ahead of the evidence (process)

Some examples:
- "It's done" for on-by-default (02:07Z), which is still not true on any branch.
- "I checked your saved settings on your PC" (22:23Z), sent before the PC data arrived. The coordinator caught that one.
- "30+ bots" for the sight budget, when the cap is 16.

**Fix:** every status claim to Max cites the branch and tip, or says "planned".

### 10. One failing bot stops the rest for that tick (IN, small)

`step_bots` calls `self.step_bot(bot, tick)?` in a loop (`bots.rs:1567`). One bot's error skips every bot after it in that tick. A panic that repeats every tick trips the server's panic fuse, which gives up after 8 panics in 60 s (`net/server.rs:1311-1321`).

**Fix:** contain each bot's error and log it.

## Stale memory

`bot-team-coordination` still said to treat Max's examples as acceptance tests, which he contradicted at 01:41Z. It is fixed now to match `max-examples-not-spec`.

---

## Round 2: release branch at 7b34b7b9 (tuning tools + teamwork merged), 2026-10-05 05:40Z

**Verified good:**
- **Teamwork is really on.** `bots.json` `team`: teamwork 0.5, mood 16, pressure 0.3, copy 0.15. The dials are listed for the sweep.
- **The tuning tools landed:** share report, off-switch check, sweep, fair metric, all-on run, 16-bot perf test, `/botset` live dials and the F3 why readout.
- **T1 does not affect free-for-all bots.** "Bots (free-for-all)" bots are rules bots. `is_brick_bot` is false for them (`bots.rs:588`), so the same-builder rule (`bots.rs:1253-1265`) never applies. They are enemies whenever `can_damage_player` allows it. `deathmatch_bots_fill_the_game_with_everyones_enemies` (`addon-import/tests/slayer.rs:2624`) shows that bot A can hurt bot B and that a kill scores.
  - T1 only covers bots from spawn bricks of the same builder. Those stay on one side, as in v20.
  - At 02:49Z, though, Max was told "In a game with no teams, everyone will now fight everyone". He should hear that this changed, and that "Bots (free-for-all)" is the way to get it.

**Still open, all in scope. Each one blocks a clean final round:**
1. **Surprise still ships at 0** (`bots.json:34`). `all_dials_on` is a gate known failure (zombie_survival switches 12.2/min against a bound of 12).
   - Even at 0.6, goofing is DORMANT in every scenario (`docs/progress/2026-10-05-bot-tuning-tools.md`). Flavours only fire at a "natural pause", which needs no objective, no enemy and no memory (`bots.rs:2715-2723`).
   - So goofing never shows in soccer, CTF or TDM, which is audit gap 1. The mood pull and copy-what-works feed only that same gate, so on camera they do nothing either.
2. **Fair is measured but not fixed.**
   - `fair_hit_rate` against a strafing, hopping target: gun 100% at 12 units from the first second, bow 88-90%, overall 57%. The band is 15-60%, and it is "reported, not enforced".
   - `fair_by_dial`: aim error 2.5°, 5° and 10° all give about 57%, flagged NOT MONOTONE. The error shrinks to a third after 2 s, so no current dial makes bots easier to beat.
   - Fix: the perception lane's alertness or wobble has to act after the first seconds too. Then enforce the band.
3. **New lag source from the teamwork merge.** `team_mood` runs for every bot on every tick (`bots.rs:2725`).
   - It is called whether or not the bot is at a pause, with radius `kind.sight` = 80, and casts one `clear_between` ray per live peer in range (`team.rs:390-395`). The ray isn't budgeted.
   - With mood now on, that is about bots × peers rays every tick.
   - Fix: compute it only at a natural pause or every few ticks, under the shared sight budget. That budget is still not in: `bot_sight` has none.
4. **Release perf is unmeasured.** The debug run is 7.5 ms a tick for 16 bots (470 µs a bot). The 2 ms release bar has to be run on Max's PC before publishing.
5. **Earlier small items, still in the code:**
   - `step_bot(bot, tick)?` in `step_bots` (`bots.rs:1588`): one bot's error skips the rest that tick.
   - Being hit still gives the exact attacker position (`note_hurt`).
6. **Dials:** 58 base + 6 team = 64 numeric settings so far. Perception (+21) has not landed yet. None have been cut so far.

**Not judged yet** (not merged): route planner, bots lane (hold rule, own goals, upgrades), eyes, small choices.

---

## Round 3: release branch at f63480bc (low-fruit + carry fix), 2026-10-05 08:10Z

**Verified good:**
- **The five extras are tested.** Each goes through ordinary controls and has its own test against the same scene with the option off (`crates/chaos/tests/bot_extras.rs`).
- **Two regressions were caught and fixed before the merge.** Dodge used to carry bots off the rooftop. Idle play caused own goals in 3v3 soccer.
- **Looks are now seeded and brick bots get first names.** This fixes "every bot looks identical".

**New, in scope:**
1. **"Click a brick for fun" presses any event brick.**
   - `bot_activatable` (`extras.rs:166-177`) accepts any brick with an enabled `onActivate` row, whatever its outputs do.
   - At a natural pause, a bot clicks one within 8 units now and then (`extras.rs:430-450`). Route clicks press any such brick in its way.
   - A natural pause needs only no enemy, no objective and no memory. That covers every free-build server and the gaps between rounds.
   - So bots will press other players' buttons: a round-reset, Win, teleport or MiniNuke button, lights or music.
   - This would look broken on camera, and it is griefing on a shared server.
   - Simplest fix: for-fun clicks only on catalog swaps (doors), and route clicks only when the brick stands in the way. Leave other event bricks alone.
2. **The dial count went up again, from 64 to 69.** The five `extras.*` weights are separate dials (`bots.json`, `bot_tuning.json`), while the plan is one dial per part.
   - Fix: replace the five with a single `extras.strength`, or fold them into surprise. Keep the per-option chances as fixed code values.

**Still open from Round 2:**
- **Surprise is still 0** (`bots.json:34`), and `all_dials_on` is still a known gate failure.
- **Goofing still needs a natural pause.** Idle play now also waits for 10 s with no objective in play, so goofing in matches stays dormant.
- **`team_mood` still runs every tick for every bot with no budget** (`bots.rs:2778`). There is still no shared sight budget.
- **Fair band** not enforced.
- **Release perf bar** not measured.
- **`step_bot(...)?`** still skips the remaining bots on one error (`bots.rs:1620`).

**Not judged** (not merged): route planner, bots lane (hold rule, own goals, upgrades), eyes.

---

## Round 4: release branch at fe74899d (extras review fixes), 2026-10-05 09:35Z

**Both Round 3 findings are fixed:**
- **For-fun clicks:** `bot_activatable` now accepts only catalog swap pairs, meaning doors (`extras.rs`). Event rows are never a reason to click.
- **Extras dials:** the five are now one, `extras.strength`. `bot_tuning.json` is back to the four team dials. The settings count is back to about 65.

**Nothing new found.** These are still open with their lanes, as Stabilization says:
- surprise is still 0, and the `all_dials_on` known failure remains;
- goofing in matches;
- perception's fair band;
- the `team_mood` budget and a shared sight budget;
- stairs circling;
- the route planner;
- the runover fix;
- release perf;
- `step_bot(...)?`.

---

## Round 5: release branch at 07ce549a (perception merged), 2026-10-05 11:10Z

**Verified fixed:**
- **Fair is enforced.** `fair_hit_rate` asserts the 15-60% band for all classes together and for the gun and the bow each.
  - At `perception.strength` 1 the steady hit rate is 28.7%. The gun was at 100% before.
  - `fair_by_dial` is monotone: 38.2% / 28.7% / 20.9% at strength 0.5 / 1 / 2 (`docs/progress/2026-10-05-bot-perception.md:115-129`).
- **The shared sight budget is in.** `bots/sightlines.rs` gives 128 rays a tick for ordinary queries plus 4 per bot reserved for its target, with a 6-tick cache. It is wired at `bots.rs:205`.
- **Being hit gives only a guess now:** the direction within 25° and the distance within 40%. Sight gives the exact spot.
- **The perception settings came down from 21 to 7**, with `strength` as the main dial.

**New, in scope (solid / lag):**
1. **Bot think time nearly doubled** with perception on: 13,302 µs a tick for 16 bots (831 µs a bot) in debug, run alone. Before, it was 7,528 µs (470 µs a bot).
   - The debug bar is 15,000 µs, which leaves 11% headroom. Under load it reaches 42,485 µs and fails.
   - The 2 ms release bar has still never been measured. A 120 Hz tick is 8.3 ms in total.
   - Fix: run `bot_think_time_16` in release on Max's PC before publishing (`cargo test --release -p bri-chaos --test bot_gauntlet bot_think_time_16 -- --ignored --nocapture`). If it is over, profile perception first, since it is the new cost.

**Note, not a finding:** the `deathmatch_open_field` kill bar went from 60 to 40, because fair aim and dodges mean fewer kills (41). That is the intended effect of fairness, not a loosened test.

**Still open, with owners:**
- surprise on and goofing mid-match (bots lane);
- `team_mood` still outside the sight budget (`team.rs` untouched; `bots.rs:2829`, teamwork);
- route planner plus fling fixes;
- `a_jeep_on_each_side` and `water_between_the_sides` gauntlet failures;
- `step_bot(...)?` (`bots.rs:1633`).

**Dial count:** about 72 (65 + 7 perception). There are now four main dials (surprise, team, extras and perception strength), but the leaf settings still have not been cut toward 6.

---

## Round 6: release branch at a00626eb (fix/bot-perf), 2026-10-05 12:25Z

**Verified: the think-time finding from Round 5 is fixed.**
- The `bot_allies` refactor (`bots.rs:1327-1360`) is equivalent to the old code.
  - The explicit team relation is unchanged.
  - The dropped `minigames.allied` branch could only be true when both players have teams in one game (`minigames/src/teams.rs:13-22`), and the relation has already answered that case.
  - `a.game` is the same as `game_of` (`combat.rs:27-30`).
- Callers now test cheap geometry first and look up sides last. These are pure `&&` chains, so the results are the same.
- `Bodies` collects the shooter's allies once per decision. A decision reads `&Session`, so nothing moves during it.
- Evidence (`docs/progress/2026-10-05-bot-think-time.md`):
  - The 30 s state hash is identical before and after.
  - The bot suite outputs match line for line.
  - The fair numbers are unchanged.
  - Bot think time is 4.1 ms a tick against 12.2 ms on 07ce549a, both run alone under the same load.
- The release bar on Max's PC is still worth running once before publishing, but it is no longer a risk.

**Next version (not blocking):** shotgun (5.6% / 10.5%) and bouncer (0%) bots are "hopeless" in the fair table. They fight from too far away for their spread or bounce. The overall band still holds.

**Still open, with owners:**
- surprise on and goofing mid-match (bots lane);
- `team_mood` cadence and budget (teamwork);
- route planner and fling fixes;
- the `a_jeep_on_each_side` and `water_between_the_sides` gauntlet failures;
- `step_bot(...)?`.
