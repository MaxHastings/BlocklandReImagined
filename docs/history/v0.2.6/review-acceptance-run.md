> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-acceptance-run.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Acceptance run (test only): 685621e9 on claude/project-thread-uoh3ap-acceptance

Reviewed: crates/chaos/tests/acceptance_unfamiliar.rs and the progress note.
3ff32b33 ignored, as asked (it must not ride along: rebase it out).

**Verdict: PASS for the test design, with one required change (the
grenade own-side check, below). It merges only after ledge pushes, since
"pushed an enemy off the drop" is red in variant 1 until they land, and
after the ally-harm build.**

## Real absolutes, no names? Yes
- The test builds its own package in two name variants, mirrored, and finds
  its content by the ids it generated (canister, shove, ball). That is the
  test knowing its fixture. Production code never sees those names, and
  the second variant proves it.
- Checks are "at least once per variant over 3 seeds" or "never in any run".
  Each run is replayed tick for tick (divergence == None).
- Two small nits:
  - Unnamed numbers: `> 3.0` (velocity change for "moved"), `< 2.0` (push
    reach) and `3 * HZ` (spot freshness). Name them like the others.
  - The load now asserts every brick loaded. Good.

## Wall clock? None
No Instant, SystemTime or sleep. Scoped threads only run the six seeds in
parallel. Play is tick-stepped with a fixed input sequence.

## 50 s: justified, and not flaky by construction
- Seeds are fixed and every run replays exactly, so the outcome only changes
  when code changes. A check that holds at 37 s holds every run.
- The margin (50 against 37) matters only for future code changes. That's
  fine for an acceptance run.
- Two asks:
  1. Print the first-held tick per check per run, so the next change shows
     how close each one is.
  2. Re-measure once ledge pushes merge, since "pushed an enemy off the
     drop" has no first-held time yet. If it first holds after ~40 s,
     lengthen the games rather than loosen the check.

## Plinth removal: honest, but it leaves the reach check untested
- Measured: no hop or jet place is ever *offered* (9 and 17 decisions), so
  "climbs the plinth" can't be an absolute today. Removing it rather than
  forcing it is right.
- But the plinth was the one check that the spot chooser uses the odd body's
  *measured* reach (a ledge between the stock and odd bodies' heights).
  Nothing else in the run proves that now.
- It's also a likely bug, not a taste question: the "hop" and "jet" places
  exist for exactly this, and pushers already weigh every place. So the
  place generator (`bot_places`) isn't finding this ledge for this body.
- Ask: one line of cause in the v0.2.7 entry (which branch of `bot_places`
  rejects it), not just "removed". Not blocking v0.2.6.
- Note: the "no plan for delivering a ball down off a platform" entry is a
  separate v0.2.7 gap. Fine as logged.

## "A grenade never goes off by its own side" under Trade it: MUST CHANGE
- Today it flags any burst within `danger` of the thrower, or of where an
  ally stood at the throw. Under Max's trade (worth = enemy − ally − own,
  with a kill veto), a burst that chips an ally or the thrower is *legal*
  when the enemy harm is larger. So this assert would fail on correct play.
- Change it to match the gauntlet's new rule, judged from what the game
  actually did:
  - per run and team, grenade damage that team dealt its own side (self
    included) < grenade damage it dealt enemies, read from Damage events
    by source and target team; and
  - no grenade kills a teammate (the veto).
- Positions at throw time then only feed the printed detail.

"Zero teammates pushed off" stays as is. Push harm to allies is v0.2.7, so
nothing in the bots models it. This absolute is the alarm that would catch it,
and it holds today.

---
# Re-review: ed9882d7 (622335b2, ed9882d7 on 685621e9)

**Verdict: PASS.** Merge conditions:
- 3ff32b33 (the safe_blast grenade fix) is **still in this branch's history**.
  Rebase it out, as agreed, before merging.
- Merge after ledge pushes (variant 1 "pushed an enemy off" is red until
  then) and after the ally-harm build. Then re-check that the merged head is
  green and record the first-held seconds.

1. **Damage observations (622335b2): good seam.**
   - It's recorded where player damage is applied, `lost` is capped at the
     health left, and the projectile comes from the damage kind.
   - It's read-only and not replicated or saved, mirroring death_results,
     and the shared RETAINED constant is reused. Unit test and seams row
     are present.
   - Nit: the test reads new records each tick by `tick > damage_read`. If
     more than 64 records ever land in one tick, the oldest drop silently.
     Add a guard in the test that the oldest retained record is older than
     the last tick read. Unreachable with 6 bots today.
2. **Grenade checks:** they now follow Max's trade rule, from real damage.
   - Own side (self included) must be less than enemies, per team per run.
   - No grenade kills a teammate (judged by death_results' killer).
   - **Should-fix (fragility, not wrongness):** a single run has few throws,
     so one realized throw that misses a dodging enemy and chips an ally fails
     "own < enemy", although the bot's *expected* trade was right. Sum each
     team's totals over the variant's 3 seeds (still an absolute of the policy,
     less luck). Keep the teammate-kill check per run.
3. **Named numbers: all from data now.**
   - Shove reach: half the body's larger extent plus one tick of the shove's
     flight.
   - "Moved": horizontal change over 2 ticks beyond what its own
     acceleration gives.
   - Spot freshness: the kind's hold_seconds.
4. **First-held seconds printed:** yes.
5. **Plinth:** a v0.2.7 line naming the place generator. Good.

Timing (51-53 s quiet, 170-176 s with four copies at once) is fine for one
named claim.

---
# Delta: ed0680c0 (claude/project-thread-uoh3ap-acceptance-final)

**Confirmed. PASS as the test to merge, once its reds are green on the final
stack.**

- **3ff32b33 is gone.** bots/combat.rs at ed0680c0 is byte-identical to
  d233e7fc. The only bots/ difference from d233e7fc is interactions.rs, from
  the reviewed measured-moves merge (2fd8cede).
- **Grenade own vs enemy:** summed per team over the variant's 3 seeds,
  as asked. The teammate-kill check stays per run.
- **Nothing dropped:** `damage_recorded()` is a running count, incremented
  before the ring pops, with a unit test (== 2). Each run asserts that it
  read exactly recorded − base. That closes the 64-entry gap for real, not
  just in practice.
- **Reds now:** V0 and V1 push-off-the-drop, which wait on ledge pushes;
  V1 "a grenade going off near an enemy", expected to depend on the
  ally-harm build. No own-side or teammate-kill failures.
- **The merge bar is unchanged:** rebased onto ledge pushes plus the
  ally-harm head, every check green with first-held seconds printed. If the
  grenade row stays red there, it's a finding against ally harm, not a reason
  to drop the check.
