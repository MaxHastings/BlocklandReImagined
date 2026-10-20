> **Historical note. Written 2026-10-06 (the file's date in the cloud project folder, `bots/review-2026-10-06-act-stage.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Bot review: act stage and soccer scoring, 2026-10-06

I reviewed `claude/v0.2.5-s4s5bn` at tip **6687175**, which is 7 commits after 8537730. The review was read-only. I built nothing and ran nothing, so every claim comes from reading the code. Paths are under `crates/sim/src/session/` and lines are at 6687175. "Go explore" has not landed on this branch.

## Verdict

**Fine to merge. Nothing must change first.** Both of today's main changes are real:

- The walk and the movement buttons are now decided in one place, with safety last.
- Enemies are now a real score that goes through the chooser and the hold rule.

There are no new soccer, ball, goal, mode or item names. Three cheap follow-ups are listed at the end, and none of them blocks the merge.

Be clear about one limit: the act stage covers **the walk and the buttons, not the aim or the trigger**. `step_bot` is still one function of about 1,430 lines (`bots.rs:1619`). This is a real step, not the full split.

## 1. Is the act stage one decision-to-controls place?

**For the walk and jump/crouch/jet: yes.**

- `bots/act.rs` collects proposals from Route, Push, Stance, Jet, Swim, Team, Goof, Dodge and Stand. It sorts them by a fixed priority and resolves them once (`act.rs:103-121`). It has unit tests for "whatever order they came in".
- Safety runs after the movers and can only take away (`bot_safe_walk`, `act.rs:150-191`). It applies these in order:
  1. the portal check;
  2. the vehicle sidestep;
  3. **the edge check, last**.
- This fixes the earlier bug where the vehicle sidestep ran after the edge check.
- The edge check now also applies in the air when the bot is off its route.
- A walk that the edge check holds back now counts as getting nowhere (`vetoed`, `bots.rs:2949`). The stuck judge then hops, replans, and finally drops the goal. A bot no longer stands at a ledge forever.

**One writer is left outside it.** The first-stall hop sets `input.jump = true` after safety (`bots.rs:2975`). It is harmless in practice: it is a jump in place, and the walk has already been checked. It should still be a Press in the act stage so the rule "nothing writes controls after `resolve`" holds. **Cheap.**

**The aim and the trigger are not in the act stage.**

- **Aim.** The precedence chain is unchanged (`bots.rs:2535-2651`): carry, then objective, then enemy, then glance, then route. After that a goof gesture overrides, then extras' aim, then look-down.
- **Trigger.** It is still decided across `bot_act` (`bots.rs:1187-1397`), the fire gate in `step_bot` (2603-2625) and `step_held_bot` (1128, 1142).
- **Goof spray.** It still pulls the trigger directly (`bots/surprise.rs:1385, 1604`) without going through `bot_act`'s record of the trigger.

None of this causes a known bug today. It is the next piece of the split. **Medium.**

## 2. Is the enemy a real score or a filter with exceptions?

**A real score.**

- The hard filter is gone. `bot_menace` (`bots/contest.rs:31-73`) gives each visible enemy a threat value of 0 to 1:
  - **1** with no play at stake, or for an enemy that hurt the bot, that the play names, that contests the body, or that stands at it;
  - **0.75** for an armed enemy away from the play;
  - **0.25** for an unarmed one.
- Fight and Chase are multiplied by it (`bots/behaviour.rs:144, 155`), and the chooser and hold rule weigh it like every other option.
- Whether an enemy is "armed" comes from what it visibly holds and whether that item can attack (`item_attacks`). No item names are involved.

Side effects:

- An armed enemy away from the ball scores Fight 0.6 and Chase 0.45. Both are below the objective's 0.65, so the plain pick never leaves the ball for it; only surprise's near-best pick can. That is what the design intends. It is a tuning choice, not a filter.
- `peaceful_objective` survives, but it now only gates arming (`bots.rs:1840, 1865`).
- `objective_without_attack` still hides enemies from a bot that has nothing to attack with (`bots.rs:1841, 2019`). That is fine, because such a bot can't fight anyway.

**Clear and cover are not folded in.**

- They still swap the chosen objective after the choice (`bots.rs:2235-2251`).
- With menace scoring an enemy at the ball at full value, there are now **two ways to go after an opponent at the ball**: Fight through the chooser, and clear as a post-choice swap.
- The "claimed → `selected_objective = None` with behaviour still Objective" path is unchanged (`bots.rs:2252-2258`). The bot stands still until the hold rule lets go.

Folding clear and cover into scored options is the clean next step. **Medium.** It is not a merge blocker.

## 3. Are any fixes keyed to soccer, ball, goal, mode or item names?

**No new ones.** I grepped every string literal in `bots.rs`, `bots/` and `bots/objectives/`. What's there:

- event names (`onActivate`, `onRulePlayerDied`);
- image state scripts (`onFire`, `onCharge`);
- avatar slot names;
- emote names;
- behaviour weight keys.

**Today's moves are general:**

- **Header jump.** It fires for any body that is above step height and below head height, in front of the bot on the way it delivers (`bots/physical_objectives.rs:850-858`).
- **"Fight an enemy at the ball".** It uses the claimed body's centre and the 4-unit engage radius, not a ball type.
- **Heard fight kept for the walk.** It uses distance divided by the bot's own run speed (`bots/hearing.rs:127-136`).

**Name-keyed checks that were already there:**

- **The spray can.** `SPRAY_CAN_IMAGE` still decides a goof's hold-versus-click and what counts as "spraying" (`bots/surprise.rs:1588`, `bots/team.rs:530`).
- **The hammer.** `native_hammer` requires `image.name == "hammerImage"` (`tools.rs:351`). Bots use it to pick the Hammer push method (`bots/physical_objectives.rs:506, 867`).
  - It is already on main.
  - It is defended as "the engine's own hammer callback". That is fair, because the engine implements that hammer's body flip.
  - It is still a second way to hit a body next to the data-driven `item_shove`.

Neither is new, and neither is a merge blocker.

## 4. Close Quarters ledges, stuck time and CTF

**Ledges: better than before.**

- The edge check is the last word.
- It now covers off-route steering in the air.
- A held-back walk replans instead of freezing.

Remaining gaps, both small:

- Hops from a goof, a dodge or the objective are checked by `hop_lands`. It uses the bot's current velocity, not the walk it is about to take (`bots.rs:2922-2924`), so a hop started at the same tick as a turn toward an edge isn't judged on that turn.
- Knockback has no counter-steer.

**Stuck time: should drop.** A held-back walk now counts toward the stall judge, so a bot replans instead of standing. Fights in their band have no route walk, so the judge doesn't fire there. That is correct.

**CTF:**

- `imported_ctf_bot_takes_the_actual_flag_and_returns_it_for_its_team` is still a known failure (`tools/gate-known-failures.toml:64`).
- The soccer commit reports CTF Battle Grounds kills going from 14 to 46 and wander from 80 % to 33 %. That shows more fighting. It does not show flag play.
- Nothing in today's commits touches flag pickup or return.

## Soccer test limits

- 413e75c puts the soccer limits back to their earlier tight values: stuck 5 %, idle 3 %, kickoff 6 s, jitter 3 and so on. That reverses the loosening I flagged earlier.
- 6687175 adds a sword match with damage on.
- I did not run either, so I can't say whether they pass at the tight values.

## Cheap follow-ups (none blocks the merge)

1. Move the stall hop into the act stage as a Press, so nothing writes controls after `resolve` (`bots.rs:2975`).
2. Route the goof spray's trigger through `bot_act` (`bots/surprise.rs:1385, 1604`).
3. Let `hop_lands` take the walk it is about to use, not just the current velocity.

## Next version

- Put the aim and the trigger into the act stage.
- Fold clear and cover into scored options, then drop the post-choice swap.
- Fix the "claimed → stands" path.
- Add the explore option for big maps. The bot thread measured Afghanistan DM wander at 79 % → 74 % with the hearing fix alone, and says the rest needs explore.
- Get CTF flag play working.
