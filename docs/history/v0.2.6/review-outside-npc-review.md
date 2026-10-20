> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-outside-npc-review.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# The outside NPC review: release risk, and a v0.2.7 consolidation plan

Checked against the repo at origin/claude/v026-stack-plain 24896f1 (fd719c94 plus the reviewed v0.2.6 stack) and our docs. "Inferred" marks anything I didn't measure.

## Part 1: does either claim hold v0.2.6? No.

### Soccer Field Goo 82 → 36 kills; more stuck time in Close Quarters sword fights
- **Source.** The table is in docs/progress/2026-10-07-v025-bots-finish.md, the **v0.2.5** finish branch measured against the main before it. That work shipped in v0.2.5. It isn't a v0.2.6 change, and nothing in the v0.2.6 stack has been measured against it yet.
- **Soccer Field Goo.** Kills aren't the soccer objective. The same row shows idle time falling from 13.1 % to 0.2 % and stuck time at 1.5 %. Fewer kills on a soccer map most likely means the v0.2.5 rule "an enemy is worth fighting only if the bot can hurt it" moved bots from sword fights onto the play (inferred; nobody measured goals in that run). It isn't evidence of a regression.
- **Close Quarters.** Stuck time was 11.5 % (sword) and 8.0 % (gun), against 7.4 % before, and the note says it moved between 5 % and 18 % across runs. The v0.2.6 navigation lane re-measured Close Quarters at 8.4 % and traced its stuck episodes to real causes:
  - a start node that's never found under a roof;
  - jumps that only reach adjacent cells;
  - a drop the bot stalls at.

  That's being fixed now (review-nav-design.md). It's open, but owned.
- **Hold?** No. One thing to add to the release check (it's already "acceptance run passes several seeds"): before publishing, run one quick match each of Soccer Field Goo (count goals as well as kills) and Close Quarters sword on the final build against v0.2.5. Hold only if the final build is worse than v0.2.5.

### Soccer test target over the 600 s timeout on Windows CI
- **Cause.** CI runs `RUST_TEST_THREADS: 1` on hosted runners (ci.yml, test job), so the four long soccer matches in `bot_soccer_match` run one after another. `two_against_two_play_a_clean_match_across_seeds` alone takes about 280 s on Max's PC, so the binary passes 600 s on a hosted runner. On the PC gate (the merge bar) it runs 449-472 s. It hit 601 s once, in a trial at HEAVY_JOBS = 6 with other builds running, and HEAVY_JOBS stays 3 for v0.2.6 (docs/progress/2026-10-07-gate-speed.md).
- **Status.** Open, as a test-cost problem, not bot behaviour. It's very likely one of the known CI-only failures. CI is advisory and the PC gate is the bar (inferred: I didn't open that CI run). "Cheaper heavy tests: the soccer matches" is already the first v0.2.7 gate item.
- **Hold?** No.

## Part 2: v0.2.7 consolidation plan

Every overlap the outside review names is real. Each one below says what exists, the evidence, and the one mechanism that should own the job. No new framework: each is a fold into a mechanism we already have, deleting the other. These join the whole-picture review's v0.2.7 item 1 list (push weighed 4 ways, the dodge's own flight model, timers outside the chooser, two weapon-worth scores, variation sources, 35 bare `120.0`, old name checks, jet numbers).

| # | Overlap | Real? Evidence | One owner |
|---|---|---|---|
| 1 | Idle ball push vs objective push | **Real.** Idle play runs through `interactions::bot_opportunity` (interactions.rs ~330-500, `idle` flag, `IDLE`/`IDLE_SHORT`, its own `bot_push_progress` at ~800). Objective pushes run through `physical_objectives.rs`. They share only the geometry helper `push_approach`. The Oct 7 note queued exactly this. | The objective push. Idle play becomes a low-worth objective ("move this body toward that player"), chosen by the same chooser, so the claim, progress and push code run once. |
| 2 | Gravity Gun carry vs objective carry | **Real.** bots.rs ~2121-2140 calls it "Legacy carry": `Brain::carry`, its own open-spot search (`bot_open_spot`), its own swing (`SWING_TICKS`, `REGRAB_TICKS`, ~2820-2835). It runs only when the objective doesn't own the hold (`objective_hold_control`). | The objective hold (physical_objectives.rs ~890-930). The throw becomes a step of it (carry to open sky, swing, release), as the Oct 7 note says. `Brain::carry` and the two constants are deleted. |
| 3 | Several progress and deadline judges | **Real, five or more:** `route::Progress` (walk stall windows from the body's speed); `claims` `LEASE` 360 / `GIVE_UP_TICKS` 240 / `PROGRESS` 0.25; `objectives` `REPLAN_TICKS` / `UNREACHED_TICKS` / `APPROACH_PROGRESS`; `bot_push_progress`; the Respawn stuck evidence (2 units over 10 s); `AVOID_TICKS` (30 s). They judge "is this getting anywhere" on different clocks. | `route::Progress` (net progress over a window sized from the body's own speed), generalised to "progress on the current step's own measure" (distance to the goal, or the body's distance to its target). The claim lease, give-up, unreached, Respawn and nav failure memory become readers of it. This also removes most of the "timers outside the chooser". |
| 4 | Commitment through a score bump | **Real.** behaviour.rs:174 scores Objective at 0.92 when committed and 0.65 otherwise; contest.rs ~55-58 has `committed ? 1.0 : HURT`. The hold rule also has its own `Committed` reason (behaviour.rs:292). That's two commitment mechanisms. | The hold rule. A carried or held play is a commitment the hold rule keeps until the play ends or something it reads (hurt, threat) outbids it. The 0.92/0.65 pair and contest's special case are deleted. Risk named in the Oct 7 note: it changes CTF delivery, so the gauntlet CTF runs before and after. |
| 5 | bots.rs size / `Brain` / `step_bot` | **Real.** bots.rs is 3,647 lines; `Brain` has 144 lines of fields; `step_bot` is one function of about 1,575 lines (bots.rs 1794-3368). That's where interference hides. | No framework. `step_bot` becomes the stages it already does in order: perceive → score/choose → act, with `bot_act` already the act stage. Each stage is its own function that takes and returns plain values. `Brain` fields are grouped by the stage that owns them (memory, combat, objective, act). Do this as mechanical moves **after** 1-4, so each fold deletes code rather than moving it twice. Tests unchanged; think time measured before and after. |

### Order and rules for v0.2.7
1. Folds 3, 4, 1 and 2 first, in that order. 3 gives 4 and 1 the progress reader they need, and 2 reuses 1's objective path.
2. Then the whole-picture item 1 list.
3. Then 5, the `step_bot` split.
4. Each fold: design note → review → code. It deletes what it replaces, and its absolute tests stay unchanged. It's re-measured on the gauntlet and the quick-match saves (Soccer Field Goo with goals, Close Quarters, A TDM) against v0.2.6.
5. "Resist adding new intelligence systems" agrees with Max's foundation-first rule. Bot mood waits until these folds are in.

### What the outside review overstates
- "Complexity isn't producing better gameplay" leans on kills as the measure on a soccer map; goals weren't measured.
- The CI timeout is test cost, not bot behaviour.
- Its 3,458-line count is a little old; it's 3,647 now.
