> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-design-ally-harm.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: design note, expected harm to the bot's own side (design-ally-harm.md)

Read against claude/v0.2.6-fighting-pushes-ki8k6d @ 107dd555.

**Verdict: PASS WITH FOUR CHANGES. They are folded into the build brief at the
end, so the lane builds the corrected version. One of them (ally harm is a
veto, not a trade) is a product call: the coordinator should confirm it with
Max, or keep one-for-one and loosen five gauntlet absolutes with his OK.**

## 1. One mechanism? Yes, with two precision fixes
Where the shot goes (the existing flight, chords and burst) → what it does
there (the host's numbers) → one `Harm` in health points. It replaces four
yes/no judges, and the inputs are all capability data. Good.

**Change A: pellets reach every body in the spread, not only the first on
the centre line.**
- "The first body on a chord ends the path" is right for the centre line,
  but pellets diverge: a teammate *beside* the line, inside the spread, takes
  pellets though no chord touches it.
- Compute the share for every body inside the spread at its distance,
  nearer bodies shadowing farther ones.
- Use the per-axis box v20 actually draws: each pellet turns up to
  5π·spread about each cross axis, uniformly (`image_spread`). The share is
  the body's angular rectangle overlapping that box over the box's area, not
  over a circular cone.
- The same box fixes the √2 gap I raised on clear_cone.

**Change B: the bot's own aim error is part of where the shot goes.**
- Today's trigger cone widens by `yaw_error.hypot(pitch_error)`. The note
  drops it.
- Plan the path along the direction the bot will actually fire (aim plus
  `brain.error`).
- Widen the trigger re-test tube by the error's possible change until the
  next planning turn. That comes from the error model's own bound, not a
  new number.

**Rules, named exactly.** The host's gates are
`Session::can_damage_player(source, target, radius)` (minigame policy,
self-damage included) and `spawn_protected`, plus `can_affect_radius` for
blasts. Splash: no line-of-sight test, distance to the body's centre,
quadratic falloff, `p.scale` applied (runtime.rs ~4585-4615). Export that
falloff as one function and call these same gates. Don't paraphrase them
as `minigames.can_damage`.

## 2. Deletes everything claimed? Yes, with one addition
- `clear_cone`/`SPREAD_CLEAR`, the `shot_space` cone and `bot_fire_clear`
  (its only caller is fire.rs:56) go.
- `safe_blast`, `clearances`, the three Context fields and `UnsafeBlast` go.
- The ally loop in `clear_path`, the mount `Unsupported` and pulse, and the
  timed `clear_path` skip go.
- `shot_space` survives only as the tube/sphere builder for `Shot.harm` and
  the chord test (combat.rs:1523). Fine: a shape, not a judge.
- **Add:** the gauntlet scorer's `at_ally`, "a shot line within 1 unit of an
  ally", is a second, cruder ally-harm rule living in the tests.
  - Keep it as an absolute only if ally harm is a veto (change C).
  - Under one-for-one trading, it and `team_kills` would fail by design.

## 3. Cost: bounded
Same chord rays as `clear_path`. The pellet and splash passes are arithmetic
over ≤ 32 bodies per candidate. Bursts already exist (`burst()`, measured
507 µs with a grenade). Mounted gunners add one candidate per gun. Spot
weighing multiplies this by its places, but inside `Budget::share`. The
trigger re-test is a `Space::holds` per body per tick, which is today's
cost. Release is at 595 µs against a 2000 µs bar, so there is room. The PR
re-measures `bot_think_time_16` and quotes it.

## 4. One-for-one with no dial: the unit is right, the trade is not
**Change C: teammates are a veto, the bot itself is a trade.**
- With friendly fire on, any expected harm to a teammate makes the shot no
  option. Harm to itself is traded one-for-one against enemy harm, and a
  shot that would kill itself is refused.
- Still one Harm, one unit, no dial, no number.
- **Why:**
  1. The north star is "a good human player". Good players don't trade
     teammates' health for kills. A bot that rockets you for net +40 feels
     like griefing to the human beside it.
  2. Every gauntlet scenario asserts `team_kills + at_ally == 0`
     (bot_gauntlet.rs:217, 256, 278, 495, 510). Trading would make those
     fail by design, and loosening five absolutes needs Max's word.
  3. Risking yourself for a kill *is* human.
- If Max prefers the trade, it is a one-line change in the same function,
  plus his OK to rewrite those asserts as "ally harm never exceeds enemy
  harm".

**Friendly fire off: sound.**
- `can_damage_player` says no, so ally harm is 0 and the bot shoots past
  teammates, as a player can.
- A projectile still stops on the first body (ally included), so a shot
  blocked by a teammate deals the enemy nothing and isn't an option. That
  is correct, as the note says.
- The gauntlet's `at_ally` counts lines near allies regardless of rules. If
  any scenario runs with friendly fire off, that scenario's absolute must
  read the rules too.

**Never kill an ally:** under C it is implied (any ally harm is a veto).
Keep the "no planned shot expected to kill an ally" test as the absolute.

## 5. Deferring push harm to allies: reasonable
It needs a landing prediction per ally in the impulse radius per candidate
(cost), and nobody has watched blast-near-ledge fights. Keep a `push` slot
in `Harm` so v0.2.7 adds a term, not a rule. Under C the veto then covers it
automatically.

## 6. One change by the fighting lane: yes, as one PR with two commits
- (1) Harm replaces the judges for hand weapons and throws.
- (2) Mounted guns go through `choose`.

(2) is the riskiest part: turret and vehicle aim, charge, crew, and a bot
gunner on a player's vehicle. Keeping it a separate commit makes it
revertable without losing (1). The lane owns combat, fire, interactions and
tactics already.

---

## BUILD BRIEF (plain text for the PC lane)

Ally harm: one mechanism for "what would this shot do to my side". Branch
from claude/v0.2.6-fighting-pushes-ki8k6d (after its review fixes). Two
commits in one PR.

Commit 1, Harm for hand weapons and throws (crates/sim/src/session/bots/):
1. Add `shot_harm(choice, bodies, rules) -> Harm { enemy, ally, own, push }`
   in health points (`push` stays 0 this version; it's the v0.2.7 slot).
   Run it where `clear_path` and `safe_blast` run in `combat::choose`,
   reusing the same chord rays.
2. Path: the direction the bot will actually fire (aim plus `brain.error`).
   - Ray: to reach.
   - Projectile: `flight_position` chord by chord.
   - Timed: `burst()` with bounces and fuse.
   - Contact: the hammer target.
   - The first body on a chord ends it (Timed asks `rebound`).
   - Bodies are self, allies and enemies, grown by their motion over the
     flight.
3. Direct: `direct_damage` on the body the path ends in.
4. Pellets (`projectiles > 1`):
   - Each body inside the spread at its distance takes `direct_damage ×
     count × share`, capped at the count.
   - `share` = the body's angular rectangle overlapping v20's per-axis
     spread box (±5π·spread about each cross axis, `image_spread`), over the
     box area. Nearer bodies shadow farther ones.
   - No `SPREAD_CLEAR`.
5. Splash at the end point, to every body within `radius × scale`:
   - `splash_damage × scale × (1 − (d/r)²)`, d to the body's centre, no
     line-of-sight test.
   - Export the host's falloff from bri_weapons::runtime (~4590) as one
     function, used by both.
   - A body within `danger` takes the full `splash_damage`.
6. Rules: multiply each body's harm by the host's own gates.
   - `Session::can_damage_player(source, target, radius)` (self-damage
     included).
   - `spawn_protected`.
   - `can_affect_radius` for blasts.
   - Call these; don't re-implement them.
7. Worth (tactics::suitability/worth): damage = enemy − own. No option when
   - ally > 0 (a teammate is never traded),
   - own ≥ the bot's health left, or
   - the result is ≤ 0.
   No weights, no numbers.
8. Trigger: keep the planned Harm's path tube and burst sphere with the
   choice. Each tick, re-test only whether a body entered them
   (`Space::holds`), widened by the aim error's possible change until the
   next plan. `Shot.harm` (what teammates step out of) is that tube and
   sphere, not a splash-wide cone along the whole line.
9. Delete:
   - `fire::clear_cone` and `SPREAD_CLEAR`;
   - `bot_fire_clear` and `shot_space`'s cone (keep `shot_space` only as a
     shape builder);
   - `safe_blast`, `clearances`, `Context::{self_clearance, ally_clearance,
     blast_margin}` and `Unsuited::UnsafeBlast`;
   - the ally loop in `clear_path`;
   - the `clear_path` skip for timed deliveries.

Commit 2, mounted guns:
10. Build a `Capability` for a vehicle gun from its projectile data (the
    fields `bot_vehicle_weapon` reads). Mounted bots plan through `choose`.
    Delete the mount `Unsupported` (combat.rs:443) and the mount-only
    trigger pulse (fire.rs ~203). Same-vehicle crew are spared as today.

Tests (sanity absolutes and mechanism checks only):
- A teammate filling half the spread box takes half the pellets' damage;
  one outside takes none.
- Splash harm equals what the host deals at the same distance.
- Friendly fire off: a bot fires past a teammate in its line. On: that shot
  is no option.
- An ally where a timed throw lands makes it no option; one under its arc
  does not.
- A tank gunner never fires a shell whose burst would hurt its crew or an
  ally.
- Over seeds: no planned shot has ally harm > 0 with friendly fire on.
- Keep the gauntlet's `team_kills + at_ally == 0` asserts unchanged.

Evidence to report:
- `bot_think_time_16` (release) against the 2000 µs bar;
- rooftop, deathmatch and all_dials_on pass;
- one vehicle scenario with a bot gunner;
- clippy `-D warnings` and rustfmt clean.

Not in this change: push harm to allies (blast or broom shoving a teammate
off a ledge). That's v0.2.7, as a term in `Harm.push`.

---
## Decision (Max, via coordinator 16:31Z): TRADE
- worth = enemy − ally − own, one-for-one.
- One veto stays: a shot expected to kill a teammate is never an option.
- Gauntlet: team_kills == 0 stays. at_ally == 0 becomes "a team's damage to its
  own side < its damage to enemies over a match".
- Seed test: "no planned shot with ally harm ≥ enemy harm, or expected to kill
  an ally".

When reviewing the PR, judge against this, not against brief item 7's veto.
- worth > 0 already implies ally < enemy, so the seed test checks the code,
  not a separate rule.
- Check the at_ally scorer is replaced by damage totals from real Damage
  events (source/target sides), not the near-line guess.
