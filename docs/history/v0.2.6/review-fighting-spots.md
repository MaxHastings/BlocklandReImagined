> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-fighting-spots.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: fighting PR (sight + where to stand), claude/v0.2.6-fighting-ki8k6d @ 764f9e6f vs main bfdf854ec

This was read-only: nothing was built or run. I read all of `spots.rs` and the diffs of `bots.rs`, `combat.rs`, `tactics.rs`, `fire.rs`, `team.rs`, `surprise.rs`, `sight.rs`, `sightlines.rs` and `simulation.rs`. Findings marked (inferred) are reasoned from code and constants, not run.

## Verdict: not yet. Two blockers, both in code this PR deleted. Everything else is sound.

The design landed as agreed:
- one score in one unit;
- seven stable options;
- shots from each place by the real chooser;
- least exposed wins when there is no shot;
- the hold rule;
- the Team mover, `exit`, `back_off` and lean deleted;
- F3 shows each spot's terms.

Visibility is clean, and vanilla aim is unchanged: `aim_point(eye, 1.0)` = `eye − 0.5`, the old point.

## Blockers (a player would hit these; both in code this PR changed)

**B1. Ranged bots no longer give ground to an enemy inside their band.** (inferred)
- `back_off` walked any fighter back while the enemy was closer than its band's near edge. For ranged weapons that edge is `standoff(splash)` or the weapon's near (`bots.rs:825-834`). It was the visible "gunner backs off a rusher, rocket bot backs out of its own blast range".
- The spots meant to take over its job don't reliably do it:
  - **Too small a step.** In and out are one body width, while near edges are several units.
  - **Rocket bots.** A rocket bot with an enemy inside its blast range has no safe shot from here or one width back. So "least exposed" decides, and nothing pulls it out.
  - **Gunners.** A gunner still has a shot from here, so it has no reason to step out at all.
- Behaviour scoring keeps Fight at full strength inside the band (`behaviour.rs` Fight `fade` only past `far`). So the bot stays in Fight and stands.
- **Fix, in this PR, as a case of spots and not a second rule:** size the in and out steps from the weapon's own band.
  - "Out" = `max(body width, near − distance)` when inside the near edge.
  - "In" = `max(body width, distance − far)` when past the far edge.
  - That's the weapon's data, not a new number.
- **Prove it with two absolutes:**
  - a rocket bot with an enemy at half its near distance ends outside that distance, or firing, within a few planning turns;
  - same for a gun.

**B2. A melee bot in a gunner ally's line of fire now gets a penalty with no way out.** (inferred)
- `team::terms` still charges Fight `cfg.harm()` (teamwork 0.6 × `HARM` 0.6 = 0.36) when its place is in an ally's line (`team.rs:115-119`).
- Before, Fight's place was `exit(...)` out of the line, so the charge rarely applied. Now it's the spot, or `feet` for melee, which has no spots.
- So Fight drops from 0.8 to 0.44, below Chase (0.6) and Objective (0.65). A sword bot charging alongside a gunner flips off Fight whenever the gunner's line crosses it.
- The plan moved the line-of-fire cost into where to stand. That makes this term a second home for the same job.
- **Fix:** drop the `harm` term from `team::terms` (and the `HARM` constant, plus its callout key if unused). The shooter's spot already pays for an ally in its line, through `taken`, and its shot through `clear_path` / ally clearance. So the gunner steps aside, and the sword bot keeps fighting.
- If the lane wants to keep the term, show with a mixed-kit test that a melee bot holds Fight while an ally fires past it.

**On your melee question.** With B2 fixed, losing the melee "step out of the line" mover is acceptable for v0.2.6. The player-visible outcome (no friendly fire, and the gunner gets its shot) is kept, because the shooter now moves instead. Spots for melee go on v0.2.7.

## The capabilities-file edits: no behaviour change, apart from the intended aim point

- **`tactics::suitability`** is now `worth()` then `damage/seconds − opportunity_cost`, with the same finite check. That's the same arithmetic.
- **`rate()`** is new and only spots read it.
- **`combat::choose`** takes `origin`. The live caller passes `peer.player.eye()` as before (`fire.rs:127`).
- **`Choice.dealt` / `seconds`** are only set from `worth`, and only spots read them.
- **`Budget::has_turn`** is read-only.
- **The one real change:** the target point is now `seen.aim`, in `choose`, `clear_path` and the fire-clear and harm lines in `fire.rs`. That is plan item 3 (target size), and it's identical at scale 1.

## Determinism: fine

- No wall clock or unseeded random was added.
- The cloned `Mind` and scratch `State` are thrown away. `Mind`'s randomness is hashed from (domain, option, tick), so a clone draws nothing that changes the real mind.
- The shared solver budget is really spent by the spot queries. That's deterministic, but see the cost note below.
- The sight cache key `Subject::Spot(bot, option)` still checks both ends against the slack.

## Magic numbers

- **Body-width step and seven options:** fine. One comes from the body's data, the other is the design.
- **Unnamed `0.5`s in `spots.rs`:**
  - `there`: `body.step + 0.5`;
  - the hop and jet probe `window = body.step + 0.5`, stepped by `2.0 * window`.
  - The comment says the grid "finds a floor within a step and a half", but the code uses step + 0.5. Read that window from `nav`, where the grid defines it, rather than restating it.
  - Fix in this PR. It's cheap.

## Leftovers from the deleted code

None left dead:
- `Mover::Team`, `team::exit` and its tests, `LEAN`, `LEAN_TICKS`, `LEAN_ROOM` and `salt::LEAN` are all gone;
- `pursue` still uses `near` (chase branch);
- `team::inside` is now shared with spots;
- docs updated.

What's left behind is the `harm` team term. It's alive, but it's now a duplicate (B2).

## v0.2.7 notes

- **Exposure through glass.** Exposure asks `eyes_see`, so a threat that sees a spot through glass counts as dealing harm there, though shots stop on glass. Exposure should ask the shot question (`sight`), not the eye question.
- **Swimmers lost `back_off`** in the weave branch. They have no spots, so a fighting swimmer no longer gives ground. It's minor and rare. Fold it in when spots reach swimmers.
- **Cost.** Up to seven `choose` runs per planning turn spend the shared per-tick solve and ray budget. Bots planning later in the same tick may wait a tick for curved solves. `bot_think_time_16` at 1441 µs (bar 2000) says the cost is fine. Watch the share report for slower first shots in big bot counts.
- **Covering a contested body** stays out of this PR, as agreed. `contest::cover` remains until it's measured on soccer.

---

# Re-review: head 6e5e6438 (fix c600dfc on 764f9e6f, then merge of main c753eada)

## Verdict: passes. Both blockers are closed. Merge exactly 6e5e6438.

The merge commit brings in only main's own changes. All I read was the fix commit (8 files, +158/-130). Nothing run here. The lane reports 165 passed in session::bots + bot_kind, and clean clippy and fmt.

## B1, back-off: closed

- **In and out now size to the band.** `bot_places` takes `Weapon::band()` and steps in by `gap − far` and out by `near − gap`, never less than a body width (`spots.rs` `bot_places`). Each still needs a walkable straight line.
- **A place nearer than the near edge deals nothing** (`spots.rs`, after `seconds`). So for a ranged bot inside its band, every place but out scores on exposure only, and out wins when it can be reached.
- **If out can't be reached** (back to a wall), every place deals nothing. The score falls back to least exposure, and firing itself is untouched (`fire.rs` still calls `choose` from the real eye). That's sensible.
- **Melee is unchanged.** Its band's near is 0 (`bots.rs:815`), so the new rule never fires for it.
- **The test is real.** `a_ranged_bot_with_an_enemy_inside_its_band_steps_back_out_of_it` puts the rocket and gun 2 units from the enemy, asserts near > 2, then requires OUT, landing at the near edge (0.5 slack) and farther than it started. It's absolute, not a tuned threshold.

## The combat.rs body shift: safe

- `shift = origin − eye`, so at the real position (`fire.rs:128` passes `eye`) it's exactly zero, and behaviour there is identical.
- It's applied inside the per-call `LazyCell`, so nothing outlives the call or reaches other callers. The only callers are `fire.rs` (eye) and `spots.rs` (candidate origins, plus the test).
- `Bodies.own` is read only by `clearances` → `safe_blast`, the own-blast check. Allies' bodies and the shot filter (`source: ActorId(bot)`) are untouched.
- One small thing: the moved body keeps the bot's current velocity, so a bot already running projects a little from the new spot. It's harmless.

## B2, team harm term: closed, and nothing else was lost

- Harm only applied when `choice.stand` was set. Only Fight's choice set it (`team_choices`). So removing it changes only Fight scores, for every weapon, not just melee.
- Ranged bots in an ally's line of fire are still handled, now by where they stand: spots charge each place that an ally's line of fire crosses. A ranged Fight no longer drops to Chase because of an ally line. That matches the plan's "spots own line of fire".
- `Intent.harm` stays: spots and `bot_fire_clear` still read it.
- **One visible loss:** bots no longer say "Moving!" in team chat when an ally's line of fire moves them, because spots have no callout. That's a small, acceptable v0.2.7 item (a spot callout keyed to the ally-fire cost). It's not a blocker.

## bots.json `"harm"` key removal: safe for Max

- A stale copy that still has `"harm"` would fail `BotTeam::validate` ("Unknown team callout `harm`", `bot_kind/team.rs:74-80`). So the question is whether one can linger.
  - **Running from a checkout:** `install_from_checkout` runs at game start (`client/src/main.rs:190`). It replaces a changed copy with the checkout's (`defaults.rs` test `a_changed_copy_is_replaced_by_the_checkouts`). So the stale key is gone on the next start.
  - **A release or preview build** ships its own fresh content, which leaves nothing stale.
  - **`--check`** doesn't sync unless asked. But the gate's main checkout content has no blockhead_bot installed (`gate-known-failures.toml:66-76`), so there's nothing stale to trip on.
- No other `"harm"` key exists in packages, crates or docs.

## Nav window literal: gone, with one small edge

- `ledge` asks heights 2 steps apart. `node_at` accepts a floor from `h − step − 0.5` up to `h + step/2` (`nav.rs:604-611`).
  - At stock step 1.0, consecutive windows just touch, so no height is skipped.
  - With a step above 1.0 (modded tuning), a sliver between windows goes unasked.
  - `floor()` also rounds its query down to `HINT_BAND`, which can shave the touching edge (inferred).
- The worst case is that a hop or jet spot isn't offered for one ledge height. Not blocking. For v0.2.7: step one `body.step` at a time, or say why two is enough.

## v0.2.7 list (added)

- A team callout for moving out of an ally's line of fire (replaces the lost "Moving!").
- Ledge probe spacing for steps above 1.0.
- Earlier notes still stand: exposure via `sight` not `eyes_see`, swimmers' back-off, shared budget cost.

---

# b6280a24 (on 6e5e6438)

The change is clean, and the test is not loosened. It changes one line in `bot_interactions.rs:1493` to match the 3-part team terms. The ignored slot was the deleted harm term, and the test still asserts `interact` with `uses > 0`. Merge exactly b6280a24.

## Gate fix 0fa3aca7 (one commit on b6280a24)

Verdict: **passes for (1) and (2). I recommend dropping (3) before the gate, so the merge sha is the lane's new sha with (3) removed.** If the merger wants to keep this exact sha instead, 0fa3aca7 is acceptable, but (3) changes how every ranged fighter strafes, and no test or design review backs that change.

1. **Low roof.** The band now comes from the fight's own `Hand::weapon`, passed into `bot_stand` (bots.rs ~2401, spots.rs:105, 147). The cycle comes from `bot_capability(bot, true)`: the selected slot, or with an empty hand the first item that can attack (spots.rs:352-372). That matches the cause the lane gave. Threats and allies keep `empty_hand = false`, which is right because an enemy's empty hand is no threat.
   - Small, v0.2.7: the band and the cycle can come from different items. The chooser's pick may not be the selected slot or the first attacker, for example while switching. That only changes how long a place is stood on, not where.
   - Nit: the old two-line "The weapon in hand's cycle" comment is left above the new one (spots.rs ~136). Drop it if a new sha is cut anyway.
2. **Spot dice: deterministic and varied, yes.** `Domain::seeded` adds Spot to Move/Dodge (surprise.rs:131-136). A draw is `cadence::seed(bot, REFLEX) ^ mix(domain<<56 ^ option<<32 ^ step)`.
   - Deterministic: there is no wall clock and no shared stream.
   - Varied per bot (bot id in the seed), per option, and per second (step = tick/120 in both `drive` call sites).
   - The domain bits keep Spot apart from Move/Dodge even though they share the salt. Spot's drive drift now also draws from the seed, which is consistent.
   - The fix matches the evidence ("confirmed by skipping only the pick"). Spot picks no longer shift weapon, aim or goof dice.
3. **Strafe kept within a body width of `stand`: wait, v0.2.7.**
   - The gate does not need it, so it falls outside the finish line rule.
   - It changes a behaviour players see. `stand` is set where the fight begins (`get_or_insert(feet)`, spots.rs:116), so a ranged bot that keeps its spot (HERE) now strafes only about a body width either way, where before it ran full STRAFE_SECONDS legs. That is less dodging for every ranged fighter, and no test measures whether it helps or hurts.
   - The code itself is sound: it always allows moving back toward the spot, it clears `stand` when the fight ends, and it reuses the existing 0.9 probe.
   - If it comes back in v0.2.7, measure hits taken with and without it, and name the 0.9 probe once for both closures.

**all_dials_on and shared state: not what the lane describes. List it, don't fix it now.**
- The dials are thread-local (`gauntlet/tuning.rs:21-26`), and `with_dials` restores the old value. `run_all` sets them inside its own worker threads, so another test's thread never sees them.
- The only process-wide piece is the panic hook (tuning.rs:285). It checks the panicking thread's own `quiet()`, so it changes nothing for other tests either.
- What is really shared is the CPU. `run_all` starts one worker per core (`available_parallelism`, tuning.rs:278-282), so rooftop alone vs. rooftop next to all_dials differs in load, not in dials. That only matters for a test with a wall-clock limit (inferred; I did not check rooftop for one). Rooftop + all_dials passing together twice is consistent with that.
- v0.2.7: make `all_dials_on` honour a worker cap, or run it on its own in the gate, if a timing test ever flakes beside it.
