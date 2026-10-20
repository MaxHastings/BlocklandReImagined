> **Historical note. Written 2026-10-06 (the file's date in the cloud project folder, `bots/removal-candidates.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# What to remove from the bots, 2026-10-06

I read branch `claude/v0.2.5-s4s5bn` at tip **41287b0** ("hops judged on the walk taken, the goof's trigger through bot_act"). Nothing was built or run. Paths are under `crates/sim/src/session/` unless they name a test, and line counts are rough.

Three rules for this list:
- I left out anything Max asked for: ride-alongs, weapon hand-offs, the jet dodge, ball jumps, explore and emotes. Those get folded into the chooser, not removed.
- Each item says what a player would lose, what replaces it, and the risk.
- Items are ranked by how much simpler the code gets for the least loss.

> **Superseded 2026-10-06 21:10Z.** Max's rule is to keep behaviours a player can see and fix their wiring. See the addendum at the end of `review-2026-10-06-round3.md`. Under that rule, items 6, 8 and 9 are now **keep**, rebuilt on the shared mechanisms. Also, `Drop` only drops a spare weapon, so it can't strand a bot unarmed.

## Remove in v0.2.5 (foundation first)

### 1. Clear and cover, the after-the-choice swap (about 280 lines)

- **What it is:** `contest::clear` and `contest::cover` (`bots/contest.rs:185-420`), plus the swap in `step_bot` that replaces the chosen objective with "knock the opponent off" or "stand behind" (`bots.rs`, the `shove` / `cover` branches after `claims.acquire`). Also the `clearing` field and the "clear" callout.
- **Why:** since the menace scoring (`contest.rs:37`), an opponent at the ball already scores a full Fight through the chooser. Clear is a second, hidden way to do the same thing. Cover is a hand-made position that the team overlap term mostly gives already, since allies spread off the same target and spot.
- **Player loses:** the scripted "stand behind the teammate" spot, and the "I got him, go!" line.
- **Replaced by:** Fight on the opponent at the ball, plus Objective with team overlap.
- **Risk: medium.** The broom only shoves; if `item_attacks` doesn't count a shove as an attack, a broom bot won't pick Fight. Fix that by counting a shove as an attack (it already has a shove impulse), not by keeping clear. Check soccer on a quick match.
- **Status:** the bot thread is already asked to "fold clear and cover into the scored choice". Removing them is the clean way to do that.

### 2. Objective stopgaps the threat score made redundant (about 40 lines)

- **What it is:**
  - `pausing_delivery` (`bots.rs:1758-1776, 1809`), which drops the objective before scoring when hurt;
  - `peaceful_objective` (`bots.rs:1851`, now only gates arming);
  - the `objective_without_attack` filter that hides enemies (`bots.rs:1852, 2049`).
- **Why:** an enemy's worth is now one score (`bot_menace`), and a hurt bot already gets full menace. These are older patches around the filter that no longer exists.
- **Correction (round 3):** a bot that can't attack does *not* score Fight at 0 yet. Fight and Chase score on the enemy's presence alone (`bots/behaviour.rs:150-162`). So `objective_without_attack` can go only after the root fix in round 3: "worth only if it can hurt the target", put into the same factor as `spared`.
- **Player loses:** nothing.
- **Replaced by:** the scores they patched around.
- **Risk: low to medium.** The arming gate needs one line of thought: arm when an enemy is about and it has nothing to fight with, whatever the objective.

### 3. The strafe's own air guard (about 30 lines)

- **What it is:** `overshoots` in the ranged stance (`bots.rs:2819-2846`). Mid-hop, it predicts the landing and steers back.
- **Why:** the act stage's safety step now checks steering in the air off the route (`act.rs`, `bot_safe_walk`), and a strafe is off the route.
- **Player loses:** nothing.
- **Risk: low.** Run Close Quarters once after.

### 4. Settings that only tests can change, and one dead field (about 100 lines)

- **What it is:**
  - 13 `BotKind` fields that are public and range-checked, but that `bots.json` refuses (`bot_kind/tuning.rs:83`): reaction_seconds, turn_degrees, aim_error_degrees, memory_seconds, five perception sub-fields and three team sub-fields;
  - `BotHold.margin`, which is always the same constant;
  - `ObjectStamp.spawner_name`, which is only counted for a size budget (`bots/physical_objectives.rs:76, 172, 476`).
- **Replaced by:** constants, with tests setting the constant.
- **Player loses:** nothing.
- **Risk: low.**

### 5. Duplicate machinery doing one job (about 150 lines)

- **Two weapon builders.** `movement_weapon` (`bots/combat.rs:424`) duplicates `bot_weapon` (`fire.rs:214`) and drops the weapon's spread. Keep one.
- **Two give-up memories in objectives.** The hand-rolled failed list (`objectives.rs`, `State.failed`) sits beside the shared `Cooldowns` type. Use `Cooldowns`.
- **Two progress judges on one objective step.** The claim lease (`claims.rs` progress) and the approach headway (`objectives/turn.rs`) both judge the same step every tick. Keep one.
- **Two constants named `RETRY`,** at 120 and 240. Name them for what they are.
- **A second random stream.** `reflex_rng` (`surprise.rs:412`) exists only to keep dodge draws separate. `cadence` salts already do that.
- **Repeated budget numbers.** `GroundingBudget` repeats 128/4096/65536 (`objectives.rs`) that `limits()` already holds.
- **Player loses:** nothing.
- **Risk: low.** Each is a swap to the kept twin.

### 6. Low-value goofs (about 120 lines)

- **What it is:** the flashlight flick (`Light`), dropping its weapon (`Drop`), the aimless detour walk (`Detour`), and walking to a door to click it for fun (`Door`). See `surprise.rs:292-321, 1293-1496` and `extras.rs:609-620` (the click-for-fun part only).
- **Why:**
  - Each is rarely seen and adds little.
  - `Detour` overlaps with strolling and exploring.
  - `Drop` can strand a bot unarmed.
  - `Door` brings its own timer.
  - The goof chooser is being rebuilt to score each goof from what the bot sees, so four fewer goofs means four fewer to convert.
- **Keep:** stare, emote, hop, look round, crouch, spray, take out a tool, and circle.
- **Keep:** clicking a door that's in the way. That's useful, not a goof.
- **Player loses:** four rare idle bits.
- **Risk: low.** `bots.json` flavour weights and the `bot_extras` door-for-fun test change.

### 7. Tests that pin old behaviour or a known limitation

- **`fixed_navigation_does_not_discover_the_physically_valid_actor_support`** (`crates/chaos/tests/bot_navigation_spike.rs:347`). It asserts the grid *can't* find a valid path. It will fail the day navigation improves, so it pins a gap rather than a behaviour. Delete it, with a one-line reason.
- **`profile_sixteen_bots_in_the_real_bedroom`** (`bot_battle_perf.rs:670`, a known failure). The hand-off note already calls it "likely bogus timing". Either replace it with a frame-cost budget or delete it with a reason.
- **`bedroom_mixed_firefight_survives_spawn_loadout_changes`** (a known failure, "expected old bot behaviour"). Rewrite it to today's design or delete it with a reason.
- **Player loses:** nothing.
- **Gain:** fewer known failures, which Max wants at zero.

## Worth considering, Max's call

### 8. Idle ball-pushing via Interact (about 250 lines)

- **What it is:** with no game running, a bot pushes a loose body toward a player (`bots/interactions.rs:503-837`, `push_contact` / `push_anchor` on the brain).
- **Why:** it's a second body-push beside the objective's Push (`physical_objectives.rs`), with its own progress tracking and none of the own-goal checks. Ad-hoc rules keep the two apart (`bots.rs`, "the game's own ball").
- **Player loses:** bots kicking a loose ball about outside a game.
- **Replaced by:** nothing. A cheaper path is to keep it but route it through the objective's Push.
- **Keep:** Interact for ride-along seats.
- **Risk: medium.** It affects `an_idle_bot_near_a_loose_ball_pushes_it` and the loose-ball known failure.

### 9. Grab, carry and throw (about 180 lines)

- **What it is:** the Carry behaviour. A bot holding something with a tool that holds (the Gravity Gun) carries it to open sky and flings it (`bots.rs` carry constants 186-205 and the carry branch; `open_spot`, `sight.rs:167`).
- **Why:** it's a second way to hold a body beside the objective's Hold method, reconciled by the `objective_hold_control` patch (`bots.rs:1820, 1956, 2039`). The Gravity Gun scored 0 kills in the last check.
- **Player loses:** bots flinging things with the Gravity Gun.
- **Replaced by:** nothing, unless it comes back as a scored attack with that weapon.
- **Risk: medium.** Next version, unless Max doesn't care about Gravity Gun play.

## Not recommended for removal

- **The engine-hammer method** (`native_hammer`). It's tied to the engine's own built-in hammer. It's on the bot thread's list to make general or justify.
- **The kind's emote list** (`BOT_EMOTES`, `bots.rs:210`). It validates Add-On data, which is a different job from the goof emote list.
- **The explore option.** It's new and works. Its stuck spots need fixing, not removal.

## Totals

Items 1 to 7 would remove about 720 lines from the bot code, with no behaviour a player would miss beyond four rare idle goofs and a scripted cover spot. Items 8 and 9 would remove about 430 more if Max is happy to lose idle ball-kicking and Gravity Gun throws.
