> **Historical note. Written 2026-10-06 (the file's date in the cloud project folder, `bots/foundation-check-2026-10-06.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Bot foundation check, 2026-10-06

This is a read-only review by a session that did not write the bot code. It reads branch `claude/v0.2.5-s4s5bn` at tip **e07dc13** ("Bots: header jumps, and moves chosen as each threat comes"). Paths are under `crates/sim/src/session/` unless they say otherwise, and line numbers are for e07dc13. I built nothing and ran nothing. Claims marked *(inferred)* come from reading the code, not from a run.

## Verdict

**Solid, but fix these first.** The core of the design is real and sound. The layer that turns a choice into controls is not.

The core is the part that should be hard to get right, and it is right:

- One score per option (`bots/behaviour.rs:135-174`).
- One hold rule, shared by behaviour, weapon, aim, route, flavour and move choices (`bots/behaviour.rs:297-347`, used through `bots/surprise.rs:484`).
- One deterministic objective planner with typed executors (`bots/objectives.rs:289`, `bots/objectives/turn.rs`).
- Claims that carry intents (`bots/claims.rs`).
- Dated evidence, so a bot never reads an unseen enemy's position on its targeting path (`bots.rs:1700-1716`, `bots/hearing.rs:31`).
- No map, mode or content names in the decision logic, with one exception (the spray can, item 7 below).

None of this needs a rethink.

The problem is everything after the choice. Several things act on the bot after the chooser has picked and settle conflicts by order, not by score:

- goofs and extras;
- contest clear and cover;
- the door click;
- weapon hand-offs;
- about 11 writes to the walk direction;
- about 12 deciders of the trigger.

All of this sits in one function, `step_bot`, which runs 1,400 lines (`bots.rs:1618-3020`). Every known gameplay bug I traced lands in this layer or in one hard-coded rule. None comes from the core. Tidying this layer is worth doing, and it is not tidying around a bad base.

## The three known issues: where they come from

### 1. Sword soccer bots play the ball but don't fight. Cause: one hard rule. Fix: cheap.

With an objective that names no enemy (the ball) and no recent hurt, `peaceful_objective` is true (`bots.rs:1823`). The visible enemy is then filtered out of what the chooser sees (`bots.rs:2007`, `.filter(|_| !objective_without_attack)`). Arming is also switched off (`bots.rs:1853`).

So Fight and Chase score 0, and Objective (0.65) always wins. The bot fights only after it is hit (`retaliating` → `pausing_delivery`, `bots.rs:1757-1764`). This is a deliberate rule ("seeing someone damageable does not make them more urgent than winning"). It is not a missing mechanism.

It also leaks in ways that look random:

- Search still scores 0.4 off the remembered enemy, and team overlap can push Objective below that.
- `contest::clear` swaps in an attack after the choice (`bots.rs:2220-2235`).
- Objective cooldown gaps unhide Fight (`objectives.rs:724`).

**Fix:** stop hiding the enemy and give it a score instead. An enemy near the ball or near the bot is worth fighting, scaled by how much it threatens the play. Then the chooser and the hold rule balance "fight" against "play ball" the way they already balance everything else. Fold clear and cover into that same scored choice.

### 2. Close Quarters ledge falls. Cause: the edge check is not the last word. Fix: cheap.

`bot_fall_ahead` (`bots/interactions.rs:916`) is sound: its depth comes from the player type's fall-damage data. But:

- **The vehicle detour runs after it** (`bots.rs:2912-2914`, after the veto at 2895-2911) and can set a new direction that nobody checks for floor.
- **The check only runs on the ground** (`state.grounded`). In the air, footwork, goof walks and the stall hop (`bots.rs:2947`) steer freely. Only the ranged strafe has an air guard (`overshoots`, 2753).
- **There are four floor probes with four depths:**
  - `floor_below` (`bots.rs:859`)
  - `bot_fall_ahead`
  - `hop_lands` (`bots.rs:875`, one fixed 1.5 ray at the landing spot)
  - `overshoots`

  The jet dodge checks its landing with a hop's flight time (`extras.rs:450-460`), but a jet carries further.
- **A vetoed walk never replans.** Zero direction makes `trying` false, which resets the stuck judge (`bots.rs:2921-2935`). That contradicts the comment at 2891. A bot can stand at an edge until its goal changes.

**Fix:**
1. One floor probe.
2. Run the veto as the very last write, in the air too.
3. Let a vetoed walk count as getting nowhere.

### 3. Wandering on big maps. Cause: no "go find them" option, and heard fights are forgotten too soon. Fix: cheap, then medium.

A rules bot with nothing seen walks to a random point within `wander_radius` (12) of where it already stands, every 2 to 6 seconds (`bots.rs:662-685`). That is a random walk, and it covers ground slowly.

Its only pull toward the action is weapon noise within 128 units (`bots/hearing.rs:109`). The heard spot lasts `memory_seconds` = 8 s (`hearing.rs:133`, `bot_kind.rs:246`). At run speed 7 (`motor/src/player.rs:642`), 8 s covers about 56 units. A fight heard 128 away is forgotten halfway there, and the bot is back to the random walk *(inferred)*.

**Fixes:**
- **Cheap:** keep a heard fight until the bot reaches the spot or sees something newer.
- **Medium:** add an "explore" option the chooser scores, aimed at places the bot has not looked lately and at what any player can know (where the other team spawns, the objective). This is general, not per-map.

## Fix first, in order

| # | What | Cost |
|---|------|------|
| 1 | Edge safety: one floor probe; the fall veto as the last write and in the air; a vetoed walk replans (issue 2) | cheap |
| 2 | Soccer: score enemies instead of hiding them; clear and cover become scored options (issue 1) | cheap |
| 3 | Heard fighting lasts until reached (issue 3, part 1) | cheap |
| 4 | Bugs found on the way (below) | cheap |
| 5 | One "act" stage after the choice: direction, aim and trigger built once, with safety vetoes last, and no goof or extra overriding a chosen fight | big (a refactor of `step_bot`, no redesign) |
| 6 | Merge duplicates: give-up memories, progress judges, two `Weapon` builders, RNGs, tick-rate math | medium |
| 7 | Explore option for big maps (issue 3, part 2) | medium |

Items 1 to 4 fit this version. Item 5 is what "split step_bot" in the cleanup plan should have produced. Today `step_bot` "hands its phases to named functions" (commit 27a1871) but still holds 1,400 lines of mixed choice and control. I would do item 5 before adding any more extras, because every new extra adds another writer to the same pile.

## Bugs found on the way (cheap)

- **The vehicle detour after the fall veto.** It can steer a bot off a deck edge (`bots.rs:2912`).
- **The door click can drop a carried body.** The route door click has no "carrying" gate (`extras.rs:516`), and its empty-hand click calls `equip_tool(None)` (`extras.rs:488`). Its aim also overrides Carry and Objective aim.
- **A goof can cancel a dodge.** The goof's direction wins over a dodge's (`act.direction.or(extra.direction)`, `bots.rs:2871`). A goofing bot can start a dodge, burn its rest timer and stand still. A goof can also only be stopped by a threat, and a shot from an unseen shooter is not one *(plausible, one case confirmed at `surprise.rs:1544`)*.
- **The goof spray bypasses the trigger record.** It pulls the trigger directly (`surprise.rs:1604`) without updating `fire_down`, so `bot_act`'s idea of the trigger can be wrong.
- **Natively chosen weapons lose their spread.** `movement_weapon` hard-codes `spread: 0.0` (`combat.rs:424`), so shotgun band-narrowing and the spread check never apply to those weapons.

## Patches on the core (what makes it feel like slop)

### Choices made outside the chooser

- **The goof roulette.** Which goof is a weighted roulette with no hold rule (`surprise.rs:754-762`), followed by its own coin flips for emote, slot, colour, detour, gait and duration. It also has its own timer.
- **Move and dodge.** These always pass `interrupt: true` (`extras.rs:408`), so the hold rule never holds them. Private timers rebuild what the hold rule would have done (`crouch_until`, `next_dodge`).
- **Weapon hand-off and route door click.** These are unscored behaviours run on a beat with their own timers (`extras.rs:521-541`, `670-747`). They take over the Wander goal. The hand-off is switched on and off by the teamwork dial.
- **The objective is dropped before scoring** in two places (`bots.rs:1747-1770`). Its result is also rewritten after the choice (`bots.rs:2220-2245`). In the "claimed" case behaviour stays Objective with no goal, so the bot stands still.

### Last writer wins

- **Direction** is written about 11 times after the route, in this order (`bots.rs:2685-2914`):
  1. push
  2. weave
  3. melee footwork
  4. strafe and lean
  5. jet leg
  6. team exit
  7. goof
  8. extras
  9. stand
  10. portal veto
  11. fall veto, then vehicle detour
- **The trigger** is decided in about 12 places across `bot_act` (`bots.rs:1186-1395`), `step_bot` (2591-2612), `step_held_bot`, `fire.rs:270-322` and the goof spray.

### Duplicated mechanisms

- **Give-up memories:**
  - the shared `Cooldowns` (`cooldown.rs`, used by claims and arming);
  - a hand-rolled failed list in objectives (`objectives.rs:572, 724-746`);
  - about 10 scalar `next_*` timers.
  - Two constants are both named `RETRY`, at 120 (`objectives.rs:20`) and 240 (`claims.rs:11`).
- **Progress judges:** the route stall (`bots.rs:2937`, whose comment says "one judge"), the claim lease (`claims.rs:271-285`), the objective approach headway (`turn.rs`) and the interaction push anchor (`interactions.rs:770-785`). The first two judge the same objective step every tick.
- **Two ways to push a body:** Interact (`interactions.rs`) and the objective's Push. Both use `push_approach`, each with its own progress tracking. Ad-hoc exclusions keep them apart (`bots.rs:1786-1800`).
- **Two ways to hold:** the Carry behaviour and the objective's Hold, reconciled by `objective_hold_control` (`bots.rs:1840, 1912-1917`).
- **Two builders of `Weapon`:** `bot_weapon` (`fire.rs:216`) and `movement_weapon` (`combat.rs:424`).
- **Four RNGs:** `Brain.rng`, `Mind.rng` and `Mind.reflex_rng` (the first and last share a seed), plus `cadence` hashing.
- **Five spellings of the tick rate.**
- **Two emote lists that differ:** `bots.rs:208` and `surprise.rs:327`.

### Content names

- **The spray can.** `SPRAY_CAN_IMAGE = "v20.image.bluespraycanimage"` decides goof behaviour (`surprise.rs:1588`) and what counts as "spraying" (`team.rs:530`). It gets past the module's own no-item-names test because it is an imported constant.
- **The hammer.** `native_hammer` keys on `"hammerImage"` and hard-codes reach 5 and damage 10 (`combat.rs:169-184`).
- **Smaller ones:**
  - the `CORE_TOOLS` id list;
  - the `"onfireakimbo"` check;
  - `"interact" | "objective"` defaulting to weight 0 (`bot_kind.rs:280`).

### Comments that read like patch notes

These describe a bug that was fixed rather than the design:

- "Legacy carry must not take over" (`bots.rs:1912`)
- "…mistakes its different subject for a failure" (`bots.rs:1772`)
- "expiring that lease must not release…" (`bots.rs:2167`)
- "account_control suspends its approach clock" (`bots.rs:1757`)

### Hidden state

These are minor. The targeting path is clean.

- `bot_path` feeds every living player's body and velocity, unseen enemies included, into route takeoff (`bots.rs:1439-1456`).
- `target_gone` knows that a target out of sight has died (`bots.rs:1906`).
- The goof's pick of who to goof with is a second sight check with no field of view (`surprise.rs:1215-1227`).

### Dials

There are six real main dials, which meets Max's aim. Two problems:

- **Pseudo-knobs.** 13 fields are public and range-checked but rejected by `bots.json`, so only tests can set them. Examples are `reaction_seconds`, `aim_error_degrees` and `memory_seconds` (`bot_kind/tuning.rs:83`).
- **Coupled dials.** Surprise strength 0 also removes flanking and mood. Teamwork 0 also stops weapon hand-offs. `BotHold.margin` is a field that always holds the same constant.

### Magic numbers

There are about 235 constants across the bot files, plus inline literals. About 60 % of the tuned numbers sit in `surprise.rs` and `extras.rs`.

## Tests

- **Soccer bands loosened.** The soccer match bands were loosened roughly twice over when the test moved from per-run limits to totals over seeds (`crates/chaos/tests/bot_soccer_match.rs`, diff against main):

  | Measure | Before | After |
  |---|---|---|
  | stuck | 5 % | 8 % (and 25 % as a "never") |
  | circling | 1 % | 3 % |
  | idle | 3 % | 6 % |
  | ignoring the ball | 10 % | 15 % |
  | clumped | 5-15 % | 20 % |
  | facing away | 5 % | 12 % |
  | wrong way | 10 % | 20 % |
  | jitter | 3 | 6 |
  | kickoff | 6 s | 15 s |
  | ball untouched | 6 s | 15 s |
  | goof share | 10 % | 15 % |

  Totals over seeds are a fair reason to change the shape. The size of the loosening is not explained anywhere, and it should be.
- **Soccer kits.** The soccer test's kits are Hands, Hammer and Broom only, so nothing tests sword soccer. Its "never" list asserts zero combat.
- **Bot tests still known failures** in `tools/gate-known-failures.toml`:
  - `imported_ctf_bot_takes_the_actual_flag_and_returns_it_for_its_team`
  - `bedroom_mixed_firefight…`
  - `a_bot_jets_over_to_someone_above_it`
  - `profile_sixteen_bots…`

  The CTF one matches "CTF flags never used" in the hand-off note.

## What is good (keep it)

- **`behaviour::Hold`** is one small, well-tested rule, used by every choice that goes through `Mind::pick`.
- **The objective planner** is bounded and deterministic. It keeps a careful negative-result cache, its steps are tied to game, round and team, and executors are rechecked against the real provider every tick.
- **Own-goal avoidance** comes from the game's own sensors, not from named goals.
- **The core infrastructure** is clean: `claims`, `cadence`, `cooldown`, `tuning` (dials set by path) and `why` (a readout of the bot's reasoning).
- **Native fire** is checked after movement against the live frame, and keeps misses off allies (`fire.rs:270-300`).
- **`bot_fall_ahead`** gets its depth from the player type's fall-damage data, not from a map.

## Addendum: the bot thread's claims, checked against tip 8537730

- **"The soccer issue was a one-liner, fixed in 8537730": partly true.**
  - The commit adds `!contest_engaged` to `peaceful_objective` (`bots.rs:1828-1834` at 8537730). A bot within 4 units of the ball (`contest.rs:16`, `ENGAGE`) now sees enemies when an opponent last touched the ball or claims it. `mover_credit` covers human players too.
  - It is still a filter with an exception, not a score.
  - A bot away from the ball stays blind to an enemy that doesn't hurt it.
  - Out of its band, Chase (0.6) still loses to Objective (0.65), so it fights only who is already on top of it.
  - Good enough for v0.2.5. Replacing the filter with a score is the follow-up.
- **"Cleanup 6 to 12 added a net +330 and did not split step_bot": true.**
  - In `crates/sim/src`, 8f789ca..bdbc7e2 (steps 6-12) is +632/-435, a net +197. To the tip it is net +330.
  - `step_bot` is still `bots.rs:1618-3020`.
- **"About 73 tunables": roughly right, but most aren't dials anyone turns.**
  - `bots.json` can set 14 paths, and 6 of those are the main dials (`bot_kind/tuning.rs:83`).
  - 13 more fields are public and range-checked, but only tests set them.
  - The rest are behaviour weights, flavour weights and melee data.
  - The cheap cut is to turn the 13 test-only fields and `BotHold.margin` into constants.
- **"The step-11 test breaks were brittle single-duel tests decided by luck": not checked.** I did not run them. That fits the soccer suite's move to totals over seeds, but those totals also loosened the bands about 2x (see Tests).

## Should the cut happen before v0.2.5 ships?

**The cheap parts, yes:**
1. The fall veto goes last and also covers the air, there is one floor probe, and a vetoed walk replans.
2. A heard fight is kept until the bot reaches the spot.
3. The door click gets a carry gate, and a goof no longer cancels a dodge.
4. The 13 test-only fields become constants.

Each of these is a small change inside one function.

**The step_bot split into one "act" stage: not before ship.** It touches every control path, and it can't be done safely in 30-minute rounds right before a release. Ship v0.2.5 with the cheap parts and make the split the first job of the next version. Until it lands, add no new extras, because each one is another writer to the same pile.
