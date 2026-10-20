> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-fighting-final.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Fighting final review: claude/v0.2.6-fighting-ki8k6d at 78a400e6

Reviewed 0fa3aca7..78a400e6 (6de797e3, 1e55a907, 2bbb9340, 78a400e6).

**Verdict: not yet. Two must-fixes, both small. No sha to merge yet.**

## 1. 6de797e3 strafe limit removed: PASS
Brain::stand goes (-21 lines), as recommended in review-fighting-spots.md.

## 2. 1e55a907 merge of main: PASS
tactics.rs `dealt` now reads `weapon.pushes()` (main's along/up push) and keeps
PUSH_WORTH; `worth` keeps `context.push_harm`. Resolution matches main.

## 3. 2bbb9340 hit mid-strafe turns the other way: MUST FIX (or drop)
(c) It is outside the hand-charge block, and it reuses the existing `parted`
input of `Brain::strafe_leg`, so it is not a second mechanism. Good.

The problem: `hurt_by` is Some on every tick the bot takes a hit
(`bot_evidence`, `self.bots.hurt.remove`). `strafe_leg` (bots.rs:613-637) flips
side and starts a new leg on every `parted` tick. Under automatic fire the bot
reverses on every hit: it jitters in place instead of strafing, which is the
easiest target there is. Each flip also draws `self.random()`, so every hit
shifts the bot's later dice.

The ally case does not repeat (after one flip the ally is behind it); the hit
case does.

Fix: a hit parts a leg once. For example, only when the leg began before the
bot's last hit-turn, so the next hit in the same leg keeps the side.
Add a sanity test: two hits inside one leg give one turn.
Or drop 2bbb9340. It came from the cut human-feel row, so dropping it is
within the finish line.

## 4. 78a400e6 spot work bounded per planning turn
(b) Budget share: PASS.
- `Budget::share` (combat.rs:67-80) gives min(left, SOLVES_PER_TICK/fighters)
  solves and min(left, reserve + (RAYS_PER_TICK - reserve)/fighters) rays.
  `charge` takes back what was spent.
- Deterministic: owner count and named constants only, no clock, no new numbers.
- Fair: one planning fighter per tick, capped at 1/n of the tick's allowance.
- The `ray()` reserve rule still guards launches inside the share.
- Nit: the doc says "fair share of what is left". The code is the fair share
  of the whole allowance, capped by what is left. Say that.

(a) Bound: admissible against the raw score, not against what the chooser
picks on. MUST FIX.
- Raw score: `bot_best_attack` (spots.rs:359-390) is a true upper bound.
  - The chooser reads each slot with the same `capability()` over the same
    inventory (combat.rs:544).
  - `tactics::worth` damage is at most max(damage(1.0), PUSH_WORTH) capped at
    the target's health (the bound uses the same `health.max(1.0)`).
  - Seconds are at least cadence (plus flight and switch).
  - Splash variants zero direct damage, and the near band only zeroes.
  - `taken` only lowers a score. Ties go to "here".
  - So no skipped place has a raw score above here's.
- The mind: the Spot choice goes through `surprise::pick`, which scales each
  score by effectiveness, drift and boredom (each within BAND of 0.15·strength)
  and by a teammate's copy (1 + copied).
  - The gate compares the bound with here's raw score. Once here's shots stop
    working (effectiveness drops) or here grows boring, a place the mind would
    now pick is never offered.
  - "An option that is not working loses score, so another takes over" is the
    mind's whole job for spots. With this gate, a bot whose shots from a good
    spot keep missing stays there. A player would see it: a bot standing in
    one place firing at a target it can't hit.
- Fix, same mechanism, no new numbers: compare against here's score as the
  mind would adjust it, with the most the mind could lift another place.
  - Have `Mind` answer two things: here's current factor
    (effectiveness × lean × copy), and the ceiling for any other option
    ((1 + BAND·strength) × (1 + copied)).
  - Skip only when bound × ceiling ≤ here's adjusted score. While the mind is
    off (strength 0 or gate closed), both factors are 1 and today's gate is
    unchanged.
  - Sanity test: a bot with a good "here" whose effectiveness on here has
    dropped weighs the other places.

(d) Timing evidence: believable as parity, not as a saving.
- Profile: 34.7 s branch, 36.5 s spots off, 36.7 s main.
- Firefight: 13.9 s branch, against 12.4 s and 15.8 s on main.
- Main's own spread on the firefight (3.4 s) is larger than any difference
  shown, and each figure is a single run. So the numbers show no visible
  regression, which is what this row needed.
- They don't show the bound saving anything. Fine; don't claim it in notes.

## Merge
Push the two fixes on top of 78a400e6 (or drop 2bbb9340 and fix the gate).
I review the new sha against this file only.

---
# Re-review: 608cf674 (claude/v0.2.6-fighting-ki8k6d-2) = 78a400e6 + 43cccdcd + 608cf674

**Verdict: PASS. Merge 608cf67421f4612d55c549aaca94d5f2c6e87b07.**

## 43cccdcd: one turn at once per leg. PASS
- `strafe_parted` ignores a second parted request inside the same leg, and a
  new leg clears it.
- The test `hits_through_a_leg_turn_the_strafe_once` covers two hits in one
  leg, then a turn again on the next leg.
- Small side effect: an ally stepping in on the new side mid-leg no longer
  flips it again. That also stops ally-on-both-sides jitter, so it's fine.

## 608cf674: bound against the mind's terms. PASS
- **The bound is right.** `Mind::reach` takes here's lowest score as
  (1 − s(1 − eff)) × (1 − BAND·s) and another place's highest as
  (1 + BAND·s) × (1 + copy).
  - That matches `pick`: lean is clamped to ±band, which covers drift and
    boredom.
  - It is a little more cautious than needed, which is safe: it ignores the
    closed gate, and Spot is never in team_copy's `worked` list, so the copy
    term is always spare.
  - No new numbers.
- **Shots judging their place (new) is not a second mechanism.** It feeds
  the existing `outcome()` effectiveness, the same path aim and weapon
  already use.
  - Without it, Spot effectiveness never moved at all, so it's needed for
    the fix to mean anything.
- **Deterministic.** A new Spot drive draws from `random_in` (seeded per bot,
  domain and option), not the shared stream. `outcome` draws nothing.
  Effectiveness recovers over EFFECTIVENESS_SECONDS as before.
- **Behaviour elsewhere:** it touches only Spot drives.
  - In fights, a bot that keeps missing now repositions sooner. That is the
    intended change.
  - Spot options are relative labels, as aim options are. Misses at "here"
    follow the bot to its next place until they fade, so a cold streak keeps
    it moving for a while. That reads human. Note it in the progress entry;
    it's not a fix.
- **Test:** `a_bot_that_keeps_missing_from_here_is_offered_other_places`
  asserts 1 place weighed first and more than 1 after misses. A sanity
  absolute. Good.
