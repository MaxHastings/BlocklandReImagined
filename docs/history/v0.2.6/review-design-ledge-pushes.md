> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-design-ledge-pushes.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: design note, ledge pushes (plan row 7)
Branch claude/v0.2.6-fighting-pushes-ki8k6d @ 9c18547b, `docs/plans/v0.2.6-design-ledge-pushes.md`. Read against the plan (item 7, lane overlap) and the code at 9c18547b.

## Verdict: approve with three changes to the note. Two of them change the hand-off signatures, so fix the note before the coordinator forwards them.

The mechanism is right and stays inside the plan's rule:
- one predicted landing;
- harm read from the engine's own fall rule, gated the same way;
- kill bricks left out because the objective projection has no kill output (the plan's own fallback);
- harm fed into the existing damage slot;
- the shot judge reused, with no outcome record;
- spots get it for free.

## Change 1 (blocker in the note): drop KILL_Y for players

- `KILL_Y` (`packages.rs:61`) removes **package entities** below −64 (`packages.rs:2750, 2775`). Nothing applies it to players. I found no player out-of-world rule, and my capabilities design review found the same.
- So "a landing below the world bound is worth the target's whole health" invents an out-of-world kill for bots. The plan says in so many words: "no out-of-world kill is invented for bots: a landing below the world is worth whatever the fall rule says, nothing more."
- **Fix:**
  - Harm comes from the fall rule only.
  - `shove_landing` returns `None` when the flight doesn't land within its limit, and `None` is worth 0.
  - Delete the "landing below the world bound" test. Replace it with "a shove off into nothing is worth nothing".
- If Max wants falling off the map to kill players, that's an engine rule for players and bots alike, decided separately. Bots would then read it for free.

## Change 2: `shove_landing` must return the impact *vector*, not a speed

- `step_combat`'s fall rule takes `impact: Vec3` (`session/combat.rs`, impacts loop):
  - the speed is its length;
  - Fall vs Impact is its direction against −Y;
  - the guard share depends on the guard facing along it.
- A bare `f32` can't feed `fall_harm(.., impact: Vec3)`. The two hand-offs as written don't fit together.
- **Smallest shapes that work:**
  - **Locomotion:** `shove_landing(tuning, feet, velocity, &ground) -> Option<(Vec3, Vec3)>` (landing feet, impact). Impact must be the same vector the motor reports into `impacts` for that contact. Taking it from the motor's own contact report in the measured flight, not recomputing it, keeps one definition.
    - `tuning` has to be the target's current one, scale included, so the flight's body is the right size.
    - The flight needs a tick limit on the shared combat budget, and `None` when that limit or the budget runs out. A shove off a long drop must not fly seconds of motor ticks per candidate.
  - **Capabilities:** `fall_harm(&Session, owner, impact: Vec3) -> f32`, read-only. `step_combat` calls it, then applies the damage.
- **One catch in that extraction:** `Weapons::guard_fall` is `&mut self`, because it plays the guard's clang burst (`weapons/src/runtime.rs:1137`). Split it:
  - a `&self` share, used by `fall_harm`;
  - the burst, left in the apply path.
  
  Otherwise the predictor can't call it, or would play clangs.
- **The `push` split `(along, up)` and `Context.push_harm: f32`** are the smallest shapes for their jobs. Approve as written.

## Change 3: the open decision. Take (a), with three conditions

**Is (a) a special case?** Not in the plan's sense.
- It keys on a mechanism (`contest::engaged`: the target contests a body this bot's objective wants), not a map, mode or item name. A broom on a ball, a shove off a flag carrier or off a vehicle seat all get it alike.
- It's not a second event interpreter either. It reads no events and no harm, so the ledge-harm rule doesn't apply to it.
- What it *is*: the one flat push number the plan said to delete, kept for one job.

**Is (b) better?** Not now. It's a new quantity, so it needs its own note, and soccer is the most sensitive behaviour (finish-line rule).

**Take (a), under these conditions:**
1. **Move the number, don't add one.**
   - `PUSH_WORTH` leaves `tactics::dealt` entirely.
   - The same value, renamed for its job (say `CONTEST_SHOVE_WORTH`, "what knocking an opponent off a body its objective wants is worth, in health"), lives where `contest::engaged` is known.
   - It enters `push_harm` as `max(harm at landing, contest worth)`, capped at the target's health. Use max, not a sum: a shove that kills also displaces, so don't count both.
   - The net change is one constant, moved and renamed, plus one call site.
2. **The judge must still be the landing.** A contest shove counts as working if the target landed within a body width of the prediction. That's what makes brooms stop losing effectiveness every time they work, which today's "a hit is lost health" rule does wrong.
3. **Add a v0.2.7 row** for (b): replace the contest worth with the landing's distance from the contested body, the same consequence model as the ball, and delete the constant.

**Is the soccer share measurement runnable within lane rules?** Yes, as one named claim.
- Run `two_against_two_play_a_clean_match_across_seeds` with `BRI_SOCCER_KIT=broom` (3 seeds × 150 s by default) on main and on the PR head. Print the broom `combat` total and the `Totals` shares.
- That's one test proving one claim ("brooms still shove opponents off the ball, shares within the existing bounds"). It's allowed if it finishes under 10 minutes.
- If it doesn't, cut `BRI_SOCCER_SECONDS`, or leave the full run to the PC gate (which runs it anyway) and report the gate's line. The full chaos suite stays in the gate.

## Smaller points (fold into the note, not blocking)

- **Cost.** "One landing read per push candidate per planning turn" is fine only with the flight on the shared budget (Change 2). Name the limit in the note and say what the bot does when the budget runs out: no harm, so the shove is worth the contest value or nothing. `bot_think_time_16` as the check is right.
- **Water.** A landing in water does no fall damage. The flight should run the motor's own water handling, so this comes for free. Add one line saying so.
- **Judge timing.** `Shot` carries the predicted landing *and* its tick. The judge reads the target's feet at that tick, not at the next sample, so a target that lands and walks off isn't counted as a miss.
- **"Allies are not targets"** is an absolute test of existing behaviour. Fine as a sanity guard, but say it guards against a regression, not that it proves the new code.
- **Own-goal hand rules** (`physical_objectives.rs`) stay, as the note says. That's correct per the plan ("deleted only once the prediction covers them").

## Hand-off lines to forward (after the note is fixed)

- **Locomotion:** `shove_landing(tuning, feet, velocity, &ground) -> Option<(Vec3 landing, Vec3 impact)>`, flown by the motor on the target's current tuning, impact as the motor reports it, tick-limited on the shared budget, `None` = no landing within the limit.
- **Capabilities:** `fall_harm(&Session, owner, impact: Vec3) -> f32`, read-only, called by `step_combat` before it applies damage. `Weapons::guard_fall` split into a `&self` share and the burst. `Capability::push` becomes `(along, up)`. `Context.push_harm: f32`.
