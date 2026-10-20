> **Historical note. Written 2026-10-08 (the file's date in the cloud project folder, `v0.2.6/review-ally-harm-2.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Re-review: teammate-harm check, round 2

Branch claude/v0.2.6-fighting-allyharm-ki8k6d = **95a868ba** (on stack-plain), plus test-only **5f28fe11** on top. New since 54095358: 9be47958, e7ef5e59, 887ff6cd, 24095dad, f2cdcea3, 57213f45, 0b99064e, 38b5653b, dfc654b5, 9710656e.

## Verdict: not yet. Three must-fixes, one of them a player bug. The rest passes.

Ran in the cloud on 95a868ba:
- `bri-sim --lib` harm, act, shove and contest tests: 28/28 pass.
- `bri-chaos --test bot_gauntlet push_brooms_on_a_high_deck`: **FAILS, 3 of 3 runs, the same every time** (`push_fired=2 knocked_off=0`, line 310). See must-fix 2.

Capabilities' acceptance evidence on the PC (2 variants × 3 seeds, no reds) is folded in below.

## 🔴 Must-fixes

### 1. Walking out of a blast skips the edge and portal checks (player bug)
In `bot_safe_walk` (act.rs ~293-330), the portal check and `bot_fall_ahead` run first, then `bot_blast_walk` can **replace** the walk with `away` (interactions.rs ~945-965). That new direction is never checked. It even replaces a walk the edge check had just zeroed (`held_back`).
- What players would see: a grenade lands by a bot standing at a lethal ledge (a rooftop fight with grenades), and the bot walks straight out of the blast and off the edge. It can also walk into a portal the same way.
- The function's doc still says safety "only takes away", which is no longer true.
- Fix: work out the blast direction first, then put the result through the same portal and fall checks. If every way out falls, it stands, the same rule as at an edge. Add a test with a live blast beside a lethal drop: the bot doesn't walk off. Update the doc.

### 2. `push_brooms_on_a_high_deck` fails off the PC, so it isn't a mechanism proof yet
Here (Linux, 4 cores) it fails every time: 6 broom bots, one side with its back to the edge, and in 60 s only 2 pushes and none knocked off. The lane says it passes on the PC. The result is repeatable here, so it's not timing noise. Most likely the match takes a different course on another platform (inferred). Either way, the test proves the mechanism only when the course is lucky, which is exactly why the acceptance check was reworded.
- Make it assert the mechanism rather than the luck:
  - A push planned with worth (`BotPlannedHarm::push > 0`, in the readout since 38b5653b) is fired.
  - Every push it fires was planned with worth (brooms do no damage, so there is no other way through the gate).
  - Knocked off > 0, summed over 3 seeds.
- Also explain why only 2 pushes fire in 60 s with six brooms face to face at the edge. If that's normal on the PC as well, the pushes are rarely worth anything even there.
- The deterministic proof already exists: `shove::a_push_only_weapon_at_an_enemy_by_a_drop_is_worth_firing_and_fires` passes here. The gauntlet scenario is the in-match check and has to hold on every platform.

### 3. One trade rule for worth, the fire gate and the gauntlet check
- `worth` caps the push at the target's health left (`pushed.min(target_health - enemy)`). The fire gate compares `ally + own < enemy + push` with the push **uncapped** (combat.rs ~1926). With two allies chipped by a lethal-fall push, the gate can pass a trade that `worth` would have refused. That's rare: one ally at its health left is already vetoed.
- `sane()`'s planned-shot check (gauntlet/mod.rs ~678) still compares against `p.enemy` without `p.push`, so a push-only plan with any side harm counts as "bad" there and as good at the gate.
- Fix: one function, `trade(harm, target_health)`, used by `worth`, `validate_fire` and `sane`.

## Answers

### 9710656e, acceptance off-the-drop reported, not asserted: honest
- The removed check never proved the mechanism. The lane measured 0 planned pushes with worth across all six runs, so the earlier hits were ball-contest pushes by chance. A check that flips with the run's random course is a flake, and removing it removes a false claim.
- Max decided to ship without lining up ledge pushes, the comment says so, and the count is still printed. It's honest **on two conditions**:
  - The in-match proof (must-fix 2) has to hold on every platform.
  - **Release notes:** "ledge pushes" must not read as bots lining enemies up off ledges. Say what's true, for example "bots shove enemies who stand at a ledge" (and "bots don't hop off ledges" stays).

### dfc654b5, contest shove only against a claimant at the body: not a special case
It narrows what "contesting" means (the target must be within the contest's existing `ENGAGE` of the body as well as claiming or moving it), for any body in any mode, with no names. That fits the one-mechanism rule. The fixed `CONTEST_SHOVE_WORTH = 10.0` predates it; worth from the push's real effect on the body goes into v0.2.7's "push weighed 4 ways".

### The other commits
- **9be47958:** my round-1 must-fixes, all done. Per-side check (`each_side_hurts_its_own_less`), planned own harm, dead code, stale comment. `first_wound` has goofs back on and passes.
- **e7ef5e59:** `Harm::push` replaces `Context::push_harm`, one source. Correct, apart from must-fix 3.
- **0b99064e:** every body the way meets is priced, so a push no longer holds fire on its own target. Correct. It means a zero-damage push meeting an ally is priced at 0, which is the known v0.2.7 gap (push harm to allies).
- **38b5653b:** a fall still going when the solver runs out counts at least what it has reached. A lower bound, so it errs toward not pushing. Correct.
- **57213f45:** a timed throw that meets a body short of its burst is priced worst case for its side, back at the thrower. That's conservative and needs no new number. Correct. The walk-out half is must-fix 1.
- **f2cdcea3 / 57213f45 (v0.2.7, not blocking):** `bot_live_blasts` is a second reader of "a live projectile threatens me", beside the dodge's `bot_incoming`. They use different rules: the dodge skips its own and allies' shots with `bot_allies`, while this one uses `can_damage_player`. Fold them into one reader with the dodge's flight model (already a v0.2.7 item).
- **24095dad:** the damage counter. Fine.

### 5f28fe11 as the head
It's fine as the head: it contains 95a868ba and changes only the acceptance test's printing. One nit: `lines.join("<literal newline>")` is split over lines by rustfmt; write `"\n"`. **But it still carries this branch's must-fixes**, so the merger takes the head that has them, not 5f28fe11 as it stands.

### Capabilities' acceptance evidence (PC, 2 × 3 seeds, ok)
- Own-side grenade harm 0 and teammate grenade kills 0 in all six runs.
- Pushes fired: V0 4/10/18, V1 14/28/10.
- No walk-offs, no teammates pushed off, no stuck lives.

That's good evidence for the grenade fixes. It doesn't cover must-fix 1, because the acceptance deck has no blast beside a ledge.

---

## Round 3: 652d9209 (on 5f28fe11)

**Verdict: two of the three must-fixes are done. One small item is left before merge: account for every push the gauntlet sees.**

### Linux runs (cloud, at 652d9209)
- `cargo test --release -p bri-sim --lib -- shove:: act:: tactics:: harm:: combat:: why::`: 53 passed, 0 failed. This includes `act::the_way_out_of_a_blast_meets_the_edge_check` and `shove::a_push_only_weapon_at_an_enemy_by_a_drop_is_worth_firing_and_fires`.
- `bot_gauntlet push_brooms_on_a_high_deck`: passes, with **7 planned with worth, 1 fired, 0 knocked off**.
  - Per seed (planned / fired / planned-and-fired): seed 0 is 2/2/1, seed 1 is 3/1/0, seed 2 is 2/0/0.
  - The planned and fired totals match the PC exactly. Only the knock-off differs (PC 1, Linux 0).

### Must-fix 1: blast escape bypassed the edge checks. ✅
`bot_safe_walk` (act.rs:293-304) now settles the blast way-out first. That walk then meets the portal, vehicle and `bot_fall_ahead` checks like any other walk. The new test covers both cases: the bot stands its ground at a lethal edge, and it walks out where there is floor. The fix is in the right place and takes nothing away.

### Must-fix 3: one trade rule. ✅
- `tactics::trade` (tactics.rs:780) is now the only rule. `worth`, the `choose` veto, `validate_fire`, the harm test helper, the readout's `net` and the gauntlet's `bad_plans` all use it.
- The push is capped the same way everywhere. `harm.push` is non-zero only for pushers (combat.rs:827), so `worth` behaves exactly as before.
- 0d70f6a3 is correct: a shot that does its shooter no harm no longer counts as killing a shooter with 0 health.

### Must-fix 2: push proof. 🔄 Mechanism proven; the gauntlet still needs an accounting.
- The readout fix is real and important. `planned` was always `None` after a step, so every check that read it was passing without testing anything. It is now correct, and it shows the plan from the previous tick, which is the plan the fire gate admits (fire.rs:343-347).
- The new shove unit test proves the whole chain deterministically: plan, gate, fire, knock-off, and `pushed_by`. That is the mechanism proof I asked for, and it passes on Linux.

**Rulings the lane asked for:**

1. **Don't drop the gauntlet's assertion, and don't keep `fired > 0` as is.** At 1 of 7, `fired > 0` is a coin flip on the margin. Dropping it would loosen the test. Replace it with an accounting that is absolute:
   - **Every push fired had a plan with worth on the tick before.** Seed 1 breaks this today: it fired 1 push with 0 planned-and-fired.
   - **Every plan with worth ends one of three ways, counted:** fired; refused at the gate (`validate_intent`'s Abort, with which check refused it); or the plan lapsed (the next planning turn no longer had worth, or the bot never got on target).
   - Print the counts and assert there are no unexplained pushes in either direction.
   - This is in the changed code, so it belongs in v0.2.6.

2. **"7 planned, 1 fired": is something dropping pushes between plan and trigger?** Not proven either way, and the accounting above answers it.
   - The fire gate admits only on the tick right after a planning tick (fire.rs:343), and the trigger also needs to be on target. A plan made while the bot is still turning, followed by a target that steps in off the edge (which the test's own doc says happens within a second), will lapse legitimately. My inference is that this explains most of the 6.
   - Lapses because the bot wasn't lined up are covered by "lining a push up is v0.2.7". Gate refusals of plans that had worth would be a real drop, and those need fixing now.
   - **The seed-1 push fired with no plan with worth just before it is the one to explain first.** Either the scorer's one-tick window missed it, or a held trigger fired while `native_combat_tick` was not the previous tick. In that second case `bot_hand_fire_gate` returns `None` (fire.rs:344) and the trigger goes ahead ungated. That is a pre-existing path, but on this branch it would be exactly the ally-harm hole. The lane should look at the readout or trace for that shot and say which it was.

3. **Libm as the Windows/Linux cause:** plausible, but inferred. Here the planned and fired totals match the PC, and only the one knock-off differs. That fits small floating-point drift in the push flight. Routing those calls through the `libm` crate is fine for v0.2.7. It is not needed for this merge.

### Still holds from round 2
- The acceptance rewording (9710656e) is honest, provided the release note doesn't claim bots line up ledge pushes.
- v0.2.7: `bot_live_blasts` duplicates the dodge's `bot_incoming`.

---

## Final check: b85d0c66 (on 652d9209). ✅ Nothing a player would hit.

Cloud runs: `cargo test --release -p bri-sim --lib -- combat:: shove:: act:: objectives` gives 36 passed. `push_brooms_on_a_high_deck` passes on Linux with **24 planned with worth: 21 fired, 3 refused ("shot refused"), 0 lapsed, 0 unplanned pushes**. Every plan is accounted for, in both directions. That is the accounting asked for in round 3, and it is an absolute rather than a count.

- **The ungated path is closed.** It's the one I suspected: a press with no plan behind it (objective tool controls, a goof's click) returned `None` from `bot_hand_fire_gate` and fired unjudged. Now `validate_unplanned` (combat.rs ~1966) judges any press of an attack weapon along its real flight. It allows the press only when nothing on its own side is hurt (`!kills_ally && ally <= 0 && own <= 0`). A refused press releases the trigger, exactly as a refused planned shot does.
- **Do objective tools get blocked?** Not the ones that aren't attacks. The check returns `None` (it doesn't judge) for any image with no direct damage, no splash and no push, so wrenches, paint, builder and interaction tools pass as before. An *attack* tool used on an objective is refused only when it would hurt the bot's own side.
  - **One narrow case, which I infer and which is not a blocker.** The flight is judged "unstopped by the world", so a brick between the bot and a teammate doesn't shield them. A bot firing a splash or projectile weapon at an objective brick, with a teammate standing behind that brick on the line, gets refused. The objective's own deadline then moves it on, so it doesn't freeze; it just doesn't shoot that brick. Also refused, correctly: a bot rocketing a brick right next to itself.
  - v0.2.7: stop the unplanned flight at the first world hit.
- The gate's verdict is now in the readout (`BotThought::gate`).
