> **Historical note. Written 2026-10-06 (the file's date in the cloud project folder, `bots/review-2026-10-06-round3.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Bot review, round 3: edge cases, seeing why, tests, fair and fast (2026-10-06)

This is a read-only review of branch `claude/v0.2.5-s4s5bn` at tip **41287b0**. Nothing was built or run. Paths are under `crates/sim/src/session/` unless they name another crate or a test. Appendices A to E at the end hold the full row-by-row tables. Four readers wrote them, and I re-checked every claim the summary relies on.

Two words are used throughout:
- **Core** means a flaw in how the bot decides or acts. These fall under "foundation first" and belong in v0.2.5.
- **Later** means behaviour the bots simply don't have yet. These go in the v0.2.6 queue.

## Must do before v0.2.5 ships

1. **One root fix: an option is only worth something if the bot can actually hurt that target.** This covers I2, I8/A7 and part of I4 (see section 1).
2. **Rays outside the shared budget**, starting with explore's endless rescan (sections 1 and 4).
3. **Unknown weapons must stay unsupported.** The old fallback still guesses "melee" (C1).
4. **F3 shows what the act stage did.** It should name the winning mover, any edge veto, the stuck count, and the clear/cover swap (section 2).
5. **Tests follow Max's bar (20:38Z).** Turn about 33 tests into absolutes, delete 6, and stop gating on wall-clock time (section 3).
6. **The watch tool test probably fails the gate.** `bot_watch::watch_bots_play_a_real_save` is `#[ignore]`d, but the gate runs ignored tests. It returns an error when `BRI_WATCH_SAVE` is unset (`crates/chaos/tests/bot_watch.rs:172`), and it isn't on the skip or known-failure lists. I didn't run it to confirm. Fix: print `skipped: needs BRI_WATCH_SAVE` and return Ok. The gate already treats a `skipped:` line as a skip (`tools/gate.py:536-539`).

Small core items that fit if there's room: F9 (the same first-come rule for the line-of-fire cost) and P5 (aim scaled by the target's size).

## 1. The 118 edge cases

There are 119 rows: 49 + 44 + 26, because one id was split.

| Status | Count |
|---|---|
| Fixed | 99 |
| Partly | 13 |
| Still failing | 6 |
| Can't tell from code | 1 (C4, real CTF: its test is still a known failure) |

The audit's top 10 are all fixed or nearly fixed:
- goofing in objective games;
- own goals avoided, with the gate at about 1 in 4;
- one hold rule;
- the view cone;
- pile-on spread;
- wreck and flip exit;
- harmless chasing on roofs;
- score pressure;
- one reversing rule;
- the spawn-protection instant shot.

### What's left, grouped by root cause

| Root cause | Rows | Core or later | Evidence |
|---|---|---|---|
| **Fight/Chase are worth something whether or not the bot can hurt the target** | I2: no weapon and no objective, it still fights at 0.8 and chases at 0.6. I8/A7: a spawn-protected enemy keeps full worth, so the bot camps the spawn. I4: a protected passenger. | **Core, v0.2.5** | `bots/behaviour.rs:150-162` scores Fight and Chase from the enemy's presence alone. The only gate is `objective_without_attack` (`bots.rs:1852`), which applies only when there is an objective. Fix: put "can hurt it" (armed, the target damageable and not protected) into the same factor as `spared`. Then remove `objective_without_attack`. |
| **Rays cast outside the shared budget** | Pf3: surprise's pause, surface, flanks and goof aim, plus extras' mark and hand sight. Pf4: `open_spot` costs up to about 830 rays and reruns every tick while an objective holds a body with a holding tool. Explore's endless rescan (section 4). | **Core, v0.2.5** | `bots/surprise.rs:1015-1035, 1062, 1222-1231, 1589`; `bots/extras.rs:251, 746`; `bots.rs:1735-1736` with `carry` reset at `:1955-1957`; `bots/sight.rs:167-188`; `bots/explore.rs:112-156`. Fix: everything charges to the sight budget in `bots/sightlines.rs`, and a failed scan is remembered. Pf4 is rare in play (Gravity-Gun-type holds). It goes away if the Carry removal (removal list item 9) is taken. |
| **The unknown-weapon rule is broken by the fallback** | C1: when native combat returns Unsupported, `bot_weapon` treats any image with no projectile and no hitscan as melee with reach 3. | **Core, v0.2.5** | `bots/fire.rs:157-164, 248-261`. Fix: unknown stays unsupported. This fits with merging `movement_weapon` and `bot_weapon` (removal list item 5). |
| **An overlap cost with no first-come tie-break** | F9: two allies standing in each other's line of fire both step aside. | Core, tiny | `bots/team.rs:115-119`. The crowding cost already has the tie-break (`:92`). |
| **Fixed body sizes in the aim** | P5: a shrunk player is aimed at around their feet. | Core, tiny | `bots.rs:2623`, `bots/fire.rs:54,65`, `bots/combat.rs:482`. |
| **The drive's ally check runs straight ahead, not along the turn** | T8: a jeep can still clip a teammate while turning. | Core, but in vehicles. Later unless tanks are in the showcase. | `bots/interactions.rs:1464-1470`. `a_jeep_on_each_side` has no team-kill bound. |
| **The planner's jet estimate doesn't match real jets** | N3: `a_bot_jets_over_to_someone_above_it` is a known failure. | Core, **already parked by Max** for after v0.2.5 | `crates/sim/src/route.rs:81`; `bots/next-version.md:32`. |
| **Lifecycle bookkeeping** | L4: a reset wipes surprise state and reseeds. L7: an admin "kick" on a bot is announced but the bot stays. | L7 is a plain bug (worth fixing; `crates/net/src/server.rs:1284-1295` only acts on network peers). L4 is later. | Appendix A |
| **New behaviour** | T3: head-count pressure. V1: an empty vehicle for travel. P3: fog limits sight. P4: invisible players (no engine state yet). | Later | Appendices A and B |

**I disagree with one reader's label.** T9 is "intents read without seeing the ally" (`bots/team.rs:300-339`). Published intents are the team's radio by design, like players talking in chat, so I count it as a design choice, not a flaw.

**A correction to my removal list.** Item 2 said a bot that can't attack already scores Fight at 0. That's wrong. It does so only with an objective. `objective_without_attack` can go only after the root fix in the first row of the table above. I've fixed `removal-candidates.md` to match.

## 2. Seeing why: the F3 readout

**Short answer: what it shows is true, but it only shows the chooser. Everything after the choice is invisible.** The readout is `BotThought::why` (`bots/why.rs:12-81`). The host's F3 overlay draws it for the bot the player looks at (`crates/net/src/server.rs:1560`, `crates/client/src/app/perf.rs:142`). It has four lines:

1. what it's doing, whether surprise picked something other than the plain best, and the hold reason (`why.rs:18-28`);
2. the top three options and their scores;
3. the biggest drift, boredom and effectiveness terms, plus the hold gate;
4. who it sees or remembers, and any glance under way.

The choice line is honest after today's changes. The decision it prints is the one `surprise::behaviour` records (`surprise.rs:569`), and that call already includes the hold rule (`bots.rs:2150-2172`). So "Doing X" really is the behaviour that drives the bot.

### Gaps that would hide the cause of a dumb move

| # | What happens | Where in the code | What F3 shows | Why it matters |
|---|---|---|---|---|
| G1 | **The edge check holds the walk back** | `bot_safe_walk` returns `held_back` (`act.rs:166-183`). `step_bot` uses it as `vetoed` (`bots.rs:2971`) and nowhere else. | Nothing. The bot reads "Doing chase" while it stands still. | This is the Close Quarters "stands at a ledge" case. You can't tell an edge veto from a broken route. |
| G2 | **Which mover won the walk** (route, stance, goof, dodge, team, stand) | `act::resolve` (`act.rs:101-119`) returns `Controls` and keeps no record of the winner. | Nothing. | "Why did it walk sideways off its route?" could be a dodge, a team step-aside or a goof walk. Only a debugger can say. |
| G3 | **The portal veto and the vehicle sidestep** | `act.rs:150-163` | Nothing. | Same problem as G1: the walk changes, but no reason is given. |
| G4 | **A hop refused by `hop_lands`**, and the stall hop pressed after safety | `act.rs:191`; `bots.rs:3018` (`input.jump = jump \|\| stalled && …`) | Nothing. | The stall hop is still the one control written after `resolve`. It isn't judged by `hop_lands` and isn't shown. |
| G5 | **The stuck judge** (stalls, replans, giving up the goal) | `bots.rs:2992-3060`; `brain.replans`, `progress.stalls()` | Nothing. An objective gets a diagnostic, but `why()` doesn't print it. | "Stuck, replanning 3 of N" is the most useful line for the stuck-time bugs. |
| G6 | **The clear and cover swap after the choice** | `bots.rs:2212-2285` swaps `selected_objective`; `BotThought.clearing` exists (`bots.rs:110`) | `why()` never prints `clearing`. Cover isn't recorded anywhere. | The bot reads "Doing objective" while it is really shoving an opponent or standing behind a teammate. **This is the one place F3 actively misleads.** |
| G7 | **Objective diagnostics** ("objective resource claimed", "navigation blocked") | `BotThought.objective_diagnostic`, `objective_detail` | Not printed in `why()`. | The claimed-then-stands case shows "Doing objective" and nothing else. |
| G8 | **Why it aims where it aims, and why it holds fire** | The aim chain and fire gate in `step_bot` (`bots.rs:2535-2651`); trigger choices in `bot_act` | Nothing. | "Why isn't it shooting?" can't be answered from F3. |
| G9 | **The size of the threat (menace) and team terms** | `bot_menace` (`contest.rs:37`); `BotThought.team` exists | The scores already include them, but the Terms line lists only drift, boredom and effectiveness. | "Why did it leave the ball for that guy?" shows a score with no reason. |
| G10 | **The explore target** | `brain.explore.to`; `BotThought.goal` has the point | Not printed in `why()`. | Big-map wandering is hard to judge. |
| G11 | **Extras' private timers** (dodge, door click, hand-off) | `bots/extras.rs` | Only goofs show (`surprise.interrupt`). | These go away once the extras move into the chooser, which is on the bot thread's list. |

The gauntlet's trace (`crates/chaos/tests/gauntlet/mod.rs:620-646`) has the same blind spot: it prints `behaviour`, goal and next, but not vetoes or the winning mover.

### Fix (one mechanism, no special cases)

- `act::resolve` and `bot_safe_walk` already produce everything needed. Keep a small `Acted` record on the brain each tick:
  - the winning mover;
  - which safety step changed the walk (portal, vehicle or edge);
  - whether a hop was refused;
  - the stall count and the replan count.
- Add it to `BotThought`.
- Give `why()` one line for it, for example "Moves: stance; edge held it; stalled 2, replan 3/4". Fold the clearing/cover swap into the first line until clear and cover are removed.
- Make the stall hop a `Press` so the record is complete.
- This is about 40 lines.

**Before v0.2.5: yes, G1, G2, G5 and G6.** It's diagnostics only, so there's no gameplay risk, and Max asked for "help diagnose issues better" as a top priority (19:28Z). The aim and trigger lines (G8) come when aim and trigger move into the act stage.

## 3. Tests, judged by Max's bar (20:38Z)

Max's bar: tests are evidence for basic sanity only. They check absolutes and bizarre outcomes, such as no goals ever, no kills ever, never any goofing, or stuck forever. The design is trusted.

| Verdict | Count |
|---|---|
| Keep (mechanism unit tests, scene capability tests, absolutes) | about 330 |
| Convert to an absolute | 33 |
| Delete | 6 |

### Convert
- **Soccer** (`bot_soccer_match.rs`: `two_against_two…` :1225, `other_line_ups…` :1295, `rockets_spears_and_jeeps…` :1499, the sword match :1473, the shipped save :1328). They should gate only on these:
  - at least one goal per kit;
  - the ball never lost;
  - no damage when damage is off;
  - no bot stuck for a whole match;
  - some goofing;
  - own goals fewer than goals;
  - the ball never untouched for 60 s.

  Stuck 5 %, circling 1 %, idle 3 %, kickoff 6 s, jitter 3 and the other shares become a report that doesn't gate.
- **The 13 gauntlet scenarios** (`bot_gauntlet.rs`). Each has `within` limits set "just past today's measured play", plus `behaviour_bands.json`, which `shares::check` enforces on every single-seed run. That's a hidden tight gate on every scenario. They should gate on:
  - kills above 0;
  - a capture or a race won;
  - a mount or pickup happening at all;
  - no team kills;
  - self-kills fewer than deaths.

  The rest becomes a report.
- **`fair_hit_rate`** (`bot_gauntlet.rs:1424`, a 15-60 % band on one seed): hits above 0, and not near 100 %.
- **Slayer soccer** (`slayer.rs:4540`): at least one goal per arm, and never on the tank for 30 s.
- **Tests with one tight number:** brain 449 and 996, routes 143 and 892, tactics 580 and 1043, interactions 1369, soccer_teams 672. Drop the number and keep the reach, hit or score.
- **Idle ball push** (`bot_extras.rs:213`): it never moves with idle play off, and it moves at least once with idle play on.

### Delete (one-line reasons)
1. `fixed_navigation_does_not_discover_the_physically_valid_actor_support` (`bot_navigation_spike.rs:347`): it pins a known limitation and would fail when navigation improves.
2. `bot_think_time_16` as a gate (`bot_gauntlet.rs:1065`): wall-clock timing on a shared machine. The gate runs it in debug against 15 ms, and nothing automatic checks the 2 ms release bar. Skip it and keep it as an opt-in report.
3. `profile_sixteen_bots_in_the_real_bedroom` as a gate (`bot_battle_perf.rs:670`): its own header says it has no thresholds, yet it's on the known-failure list. Skip it, or keep only "16 bots spawn and stay finite".
4. Crouch-under-fire share (`bot_extras.rs:294`): it measures how often a flavour fires, which is tuning.
5. Hop on a predicted hit (`bot_extras.rs:312`): exact unit tests in `bots/extras.rs` already cover it.
6. Splash aims low more often than not (`bot_tactics.rs:1080`): an aim share is tuning.

### Keep and fix the cause
`bot_firefight` and `bot_carryable_ctf` are known failures, but they already check absolutes. Their failures are real, so keep the tests and fix what they catch.

### Running more than one seed
Only the gauntlet (`BRI_TUNING_SEEDS`) and soccer (`.seeded(n)`) have seeds. Every other test runs one implicit seed. To make an absolute hold across seeds, use the gauntlet's trick of joining N dummy players first, which shifts bot ids and their random streams.

## 4. Fair and fast

**Fair: partly guarded.**
- `fair_hit_rate` gates a hit-rate band, but:
  - the target never fights back;
  - it runs on one seed;
  - Combat and Ordinary alertness scale reaction identically (`bots/perception.rs:284-292`), so alertness doesn't move the gated number.
- `fair_by_dial` (`bot_gauntlet.rs:1453`) ties hit rate to the perception dial, but it's `#[ignore]`d and on the skip list.
- No test checks that a human-like player can win or survive.
- **Threat scoring and explore have no fairness coverage.** In a fight with no objective, `bot_menace` returns 1.0 (`bots/contest.rs:44`), so the fairness test never exercises it.

By Max's bar, the right absolute is "a bot can be killed by a scripted player that fights back, and isn't at 100 % hits". Feel is judged in playtests.

**Fast: budgets exist, but today's code leaks around them.**
- Shared budgets:
  - sight: 128 rays per tick, plus 4 per bot (`bots/sightlines.rs:21-36`);
  - planning: 96 samples and 384 expansions (`nav.rs:47-51`);
  - combat: 2048 solves and 544 rays (`bots/combat.rs:9-10`);
  - one objective turn per tick.
- **Explore** (`bots/explore.rs:112-156`, called at `bots.rs:1928-1937`). With no target, it casts 12 wall rays plus 12 floor rays. When no direction qualifies, which happens in a small room, on a roof or a bridge, `explore.to` stays `None` and it **rescans every tick with no cooldown**. At 16 bots that's up to 384 extra rays per tick, which is more than the whole sight budget. It also runs before the choice, whatever the kind's explore weight. **Fix before v0.2.5:** remember a failed scan (back off), and charge the rays to the sight budget.
- The act stage adds no new ray work. It moved the existing edge, strafe and vehicle checks and adds one small vector per bot per tick (`bots.rs:2727`).
- `bot_menace` is constant cost.
- Hearing is capped.
- Older per-tick costs, not from today:
  - a full `BotKind` clone per bot (`bots.rs:1952`);
  - a teammates' intents list per bot (`bots/team.rs:300`);
  - every body box rebuilt during a route search (`bots.rs:1453`).

  Next version.

By Max's bar, speed is guarded by budgets in code (counted, deterministic), not by wall-clock tests. A count test is a clean replacement: run 16 bots and check that rays per tick never exceed the budget. It's deterministic and an absolute.

---

# Appendix A: edge cases, Lifecycle, Teams, Objectives and Inventory

Read-only review of `claude/v0.2.5-s4s5bn` at 41287b0. Nothing was built or run. Paths are relative to the repo root.
`bots.rs` = `crates/sim/src/session/bots.rs`; `bots/x.rs` = `crates/sim/src/session/bots/x.rs`; tests are in `crates/chaos/tests/` unless a path says otherwise.
Known-failures = `tools/gate-known-failures.toml`. Of the tests cited below, only `profile_sixteen_bots_in_the_real_bedroom` is listed there.
"(inferred)" means I reasoned from the surrounding code and did not trace the exact runtime path.

Counts: FIXED 41, PARTLY 5, STILL FAILING 3, CAN'T TELL 0 (49 rows).

## Lifecycle

| id | Edge case | Status now | Evidence at 41287b0 | Root cause / class (if not fixed) |
|---|---|---|---|---|
| L1 | Spawn from a brick | FIXED | `bots/lifecycle.rs:52-123` `reconcile_bot_brick` (embody, `Brain::new`, name); team from brick `bots/lifecycle.rs:470-514` `apply_brick_team`; tests `bot_brain.rs:239` `bots_of_one_builder_are_on_one_side`, `bot_soccer_teams.rs:501` `a_spawn_bricks_team_choice_puts_its_bot_on_that_team_across_save_and_load` | |
| L2 | Spawn over the 16-bot cap | FIXED | `bots/lifecycle.rs:81-91` (builder gets a Center notice); `MAX_BOTS` `crates/package-runtime/src/ops/mod.rs:488`, re-exported `bots.rs:143`; rules bots `bots/lifecycle.rs:237-240`; test `bot_brain.rs:372` `a_bot_over_the_server_limit_tells_its_builder` | |
| L3 | Death mid-action (charge, claim, seat, objective) | FIXED | `bots.rs:1069-1112` `step_dead_bot`: claims `release_owner` + `forget` (:1102-1103), objective state reset (:1096), `fire_down=false` (:1106), `surprise.new_life()` (:1108); tests `bot_interactions.rs:524` `a_dead_driver_releases_its_seat_without_removing_the_living_gunner`, `bot_tactics.rs:353` `disconnecting_the_charge_target_cancels_without_a_stale_throw` | |
| L4 | Respawn after a mini-game reset keeps learned prefs and RNG stream | PARTLY | Death keeps the brain (only `new_life`, `bots/surprise.rs:420-424`). A reset calls `respawn_brick_bot` (`crates/sim/src/session/vehicles.rs:1024`), which builds a fresh `Brain::new` (`bots/lifecycle.rs:147`) and copies only `sequence` and `named` (:148-152). `Brain::new` reseeds `rng` from the bot id (`bots.rs:494`) and `surprise::Mind::new(bot)` reseeds both surprise streams and clears drives (`bots/surprise.rs:411-416`, `bots.rs:545`) | Reset path rebuilds the brain instead of carrying `rng`/`surprise` (and team mood) across, so every round restarts from the same seed with boredom/effectiveness wiped. NEW BEHAVIOUR (lifecycle bookkeeping, small) |
| L5 | Team switch mid-life drops hostility, search, plan | FIXED | `bots.rs:1045-1067` `bot_note_context` (memory, target, hurt, evidence cleared on game/round/team change); objective side `bots/objectives/turn.rs:21-38` (`desired_context` change clears desired/failed, replans now); tests `bot_knowledge.rs:412` `becoming_an_ally_invalidates_remembered_hostility_before_expiry`, `bot_creator_acceptance.rs:633` `a_real_team_change_invalidates_approach_and_due_time_success_guards` | |
| L6 | Builder leaves / rules game ends | FIXED | Brick bot's game follows its builder's (`bots/lifecycle.rs:517-563` `sync_bot_minigames`, called every 30 ticks `bots.rs:963-965`; missing builder => `wanted=None` => `host_place(None)`); rules bots leave with their game (`bots/lifecycle.rs:386-399` `rules_bots_gone`, dropped `bots.rs:960-962`); test `bot_knowledge.rs:540` `removing_a_rules_bot_releases_its_reservation_before_the_lease_expires`. "Harmless" outside a game rests on the damage policy (inferred) | |
| L7 | Admin kicks a bot | STILL FAILING | Bots do get an admin connection (`crates/sim/src/session/admin.rs:195-225`, `is_bot`), are listed (`admin.rs:284`) and are kickable (`crates/admin/src/lib.rs:796-804` `protect_target` exempts bots from protection). The kick is announced and queued (`admin.rs:476-492`, `:694`), but the server drains it only for networked peers: `crates/net/src/server.rs:1284-1295` `close_admin_disconnects` does `peers.remove(&target)` on its connection map (bots are never inserted, `server.rs:1516`), so neither `session.disconnect` nor `drop_bot` (`bots/lifecycle.rs:213-227`) runs. The bot stays after "X kicked Bot" is announced. (The chaos harness `crates/chaos/src/local.rs:295-298` also only drops its own driver list.) No test | Kick of a bot owner is never routed to `drop_bot` (and a brick bot would respawn from its brick at the next reconcile anyway). NEW BEHAVIOUR (admin/lifecycle plumbing, outside the decision loop) |
| L8 | Mini-game reset / round end | FIXED | Reset respawns vehicle bricks (`crates/sim/src/session/combat.rs:2105-2115`) => `respawn_brick_bot` (`bots/lifecycle.rs:126-161`, claims and hurt released :144-145); failed plans dropped on round change (`bots/objectives/turn.rs:39-54`, `g.round == f.round && !g.round_over`); tests `bot_soccer_teams.rs:672` `recipe_goals_bots_contest_one_ball_score_and_play_on_after_the_reset`, `bot_soccer_teams.rs:322` `team_sides_and_goal_guards_survive_save_and_reset` | |
| L9 | Map change / build load | FIXED | Kinds rescan sets `vehicles.scanned=false` (`bots/lifecycle.rs:26`) and `reconcile_vehicle_bricks` reconciles every bot brick (`crates/sim/src/session/vehicles.rs:939-969`); team applied by `apply_brick_team`; tests `bot_soccer_match.rs:1328` `the_shipped_soccer_save_loads_a_ready_match`, `bot_soccer_match.rs:1395` `the_shipped_field_survives_load_bricks_whatever_the_pads_size` | |
| L10 | Many bots (16): bounded per-tick cost | FIXED | Shared budgets: `bots.rs:250-262` (interaction, objective-per-tick, combat solves, sight-ray budget), solves capped in `bots/combat.rs:586-605`, one objective plan per tick `bots.rs:967-982`, rotating start `bots.rs:985-990`. Perf tests are `#[ignore]` diagnostics with no thresholds: `bot_battle_perf.rs:670` `profile_sixteen_bots_in_the_real_bedroom` (IN known-failures), `bot_combined_perf.rs:340` `sixteen_objective_controllers_and_mixed_inventory_combat` | (Bounded by construction; no timing threshold verifies it.) |
| L11 | Zero bots costs nothing | FIXED | `bots.rs:941-947` (navs cleared, collision tracking off, early return) | |
| L12 | Only bots, no humans | FIXED (inferred) | Rules bots are independent of humans; brick bots follow the builder's game (`bots/lifecycle.rs:517-563`) | |
| L13 | Bot kind's Add-On disabled: its bots leave | FIXED | `set_bot_kinds` sets `vehicles.scanned=false` (`bots/lifecycle.rs:12-28`), full rescan calls `reconcile_bot_brick` (`vehicles.rs:945-969`), which drops a bot whose kind is gone (`bots/lifecycle.rs:57-69`); test `bot_brain.rs:279` `without_a_bot_add_on_a_spawn_brick_makes_no_bot` (covers re-enabling). Rules bots of a removed kind are not dropped by this path; they keep the old `kind` (inferred, minor) | |
| L14 | Rules hold a bot still (rest) | FIXED | `bots/lifecycle.rs:339-370` `rest_rules_bot` (claims released :365, seat left :366); `bots.rs:1116-1158` `step_held_bot` (fire held, objective suspended); tests `bot_interactions.rs:663` `rules_rest_releases_a_driver_and_resume_can_make_a_new_decision`, `bot_objective_rest.rs:295` `a_resting_failed_model_does_not_starve_other_ready_controllers` | |
| L15 | A dead bot's published intent stops steering teammates | FIXED | `bots.rs:1103` `claims.forget(bot)` in the death branch; `bots/claims.rs:149-151` `forget`; intents also lapse (`bots/claims.rs:154-157`, `:338`) | |

## Teams and allegiance

| id | Edge case | Status now | Evidence at 41287b0 | Root cause / class (if not fixed) |
|---|---|---|---|---|
| T1 | FFA mini-game, one builder's brick bots | FIXED (by decision) | Same-builder brick bots are allies when no explicit teams (`bots/sight.rs:54-83` `bot_allies`, `(None, None) => same brick owner`); decided 2026-10-05 as the faithful v20 rule; test `bot_brain.rs:239` `bots_of_one_builder_are_on_one_side` | |
| T2 | A single team: nothing to fight | FIXED (inferred) | Same-game teammates are allies via `minigames.allied` (`bots/sight.rs:34-45`, `:59-67`), so `bot_enemy` is false for all (`bots/sight.rs:9-31`) | |
| T3 | Uneven teams: short side leans on the objective | STILL FAILING | Pressure now exists but reads score only: `bots/team.rs:370-396` `team_deficit` (points behind best opposing team), applied `bots/team.rs:168-171` `pressed`, called `bots.rs:1945`, `:2135`. Nothing reads team head-counts | No term for being short-handed. NEW BEHAVIOUR |
| T4 | Friendly fire on: teammates still not enemies | FIXED | `bots/sight.rs:10-14` (`bot_allies` checked before `can_damage_player`); test `bot_knowledge.rs:439` `rules_bots_do_not_target_same_team_humans_or_bots_with_friendly_fire_on` | |
| T5 | Ally in line of fire / splash | FIXED | `bots/interactions.rs:1210-1233` `bot_fire_clear`; `bots/combat.rs:985` `safe_blast` (used :621, :645, :900, :1172), allies' bodies `bots/combat.rs:951-968`, `:1076-1077`; gauntlet asserts zero team kills/at-ally fire: `bot_gauntlet.rs:220` (`deathmatch_open_field`), also `swords_four_a_side` :244, `deathmatch_mixed_arsenal` :267, `rooftop_brawl_without_rails` :291 | |
| T6 | Same-builder brick bots on one side | FIXED | `bots/sight.rs:79-82`; test `bot_brain.rs:239` | |
| T7 | Human joins mid-round | FIXED (inferred) | Relations are computed live each tick (`bots/sight.rs:9-83`, candidate scan `:128-146`) | |
| T8 | Driver runs over teammates | PARTLY | Driver brakes for allies in a corridor along the hull's travel direction, sized by stopping distance: `bots/interactions.rs:1464-1470` (`stopping = speed*0.3 + v^2/2a + 1`, `bot_ally_corridor(..., stopping + 2.0, radius)`), corridor `bots/interactions.rs:646-690`; brake `:1507` (`input.forward = ... hazard ...`); test `bot_interactions.rs:982` `a_driver_brakes_for_a_grounded_ally_before_forward_or_reverse_contact`. But `bot_gauntlet.rs:412` `a_jeep_on_each_side` asserts no team-kill bound (unlike the foot scenarios), and its own comment says "Drivers ramming and orbiting one another is open work" (:443-444) | Hazard volume is a straight box along hull-forward, not the swept turning path of the steered chassis, and no test bounds team kills from vehicles. CORE DESIGN (the drive executor can harm allies) |
| T9 | Teammate intents read only when perceivable | STILL FAILING | `bots/team.rs:300-312` `team_intents` filters only `bot_allies`; same for `team_crowd` `bots/team.rs:319-339` (used in target choice `bots/sight.rs:136-141`). No sight/hearing test. (Contrast: mood copying does gate on sight via `seen_doing`, `bots/team.rs:495`) | Intents are read from the global claims table regardless of whether the bot could see or hear the ally. CORE DESIGN (decision uses information the bot could not perceive) |
| T10 | Target becomes an ally mid-charge | FIXED | test `bot_knowledge.rs:655` `becoming_an_ally_cancels_an_armed_hand_spear_without_throwing_it` | |

## Objectives and rules

| id | Edge case | Status now | Evidence at 41287b0 | Root cause / class (if not fixed) |
|---|---|---|---|---|
| O1 | No objective: fight, wander, flavour | FIXED | `bots/objectives/turn.rs:6-13` returns `None` (weight 0), and `None` when nothing desired; goofing allowed with no objective (`bots.rs:2485`, `idle`) | |
| O2 | Unreadable/unsupported rules: explained, no fake goal | FIXED | `Unsupported` rejection `bots/planning.rs:108`, discovery reports `unsupported` (`bots/objectives/discover.rs:125`, `:193-250`); tests `bot_creator_acceptance.rs:753` `an_unknown_collateral_effect_rejects_the_complete_action`, `bot_navigation_spike.rs:602` `an_unmodelled_named_target_reaction_rejects_the_whole_objective_action` | |
| O3 | Rules too big for budgets: explicit bounded failure | FIXED | `bots/objectives.rs:515-530` `GroundingBudget`; tests `bot_creator_acceptance.rs:823` `too_many_real_action_sources_report_the_specific_grounding_bound`, `bot_creator_acceptance.rs:782` `a_sixteen_step_dependency_is_an_explicit_depth_negative_not_partial_success` | |
| O4 | Moving objective (ball): lead it, stay engaged | FIXED | `bots/contest.rs:116-145` `lead`/`lead_offset`, used `bots/physical_objectives.rs:793`; clearing/cover `bots/contest.rs:146-185`, `:281`; test `bot_soccer_teams.rs:672` | |
| O5 | Objective destroyed/reset: replan at once | FIXED | `bots/objectives/turn.rs:110-122` (`object_gone` :116 => `next = tick`, `paused`); test `bot_carryable_objectives.rs:478` `ordinary_respawn_replaces_the_carried_source_without_completing_the_old_journey` | |
| O6 | Both teams score the same goal | FIXED (inferred) | Team-scoped completion/score `bots/objectives/turn.rs:84-87` (`observed_completion(.., team)`), team score as sum of members `bots/objectives/project.rs:58` | |
| O7 | Own goal: approach from the right side, very rare | FIXED | Push heading never crosses another sensor (own goal) and the approach arc widens near one: `bots/physical_objectives.rs:247-282` `hazard_clear_heading`, `:839-845`; unit test `bots/physical_objectives.rs:1008` `a_push_that_would_score_an_own_goal_turns_off_it`; drive-back avoidance `bots/contest.rs:358-385` (`out_of_the_way`). Soak gate tightened from 1/2 to about 1/4: `bot_soccer_match.rs:1169-1172` (`even_own_goals * 4 <= even_goals + 1`) in `two_against_two_play_a_clean_match_across_seeds` (:1225) | (Rate itself can't be confirmed without running; the gate is still looser than the audit's suggested 1/5.) |
| O8 | Gap between two steps: no blink to Wander | FIXED | Hold rule holds a paused option: `bots/behaviour.rs:245-263` (`Ask::paused`), `:307-309`, `:347-350`; fed from `bots.rs:2156-2163` (`between_steps() || objective.is_none()`); `bots/objectives.rs:631-635`, `:710`; unit test `bots/behaviour.rs:374` `a_paused_choice_is_held_for_the_hold_time` | |
| O9 | Failed step cooling down: no walk home | FIXED | `Situation.pursuing` `bots/behaviour.rs:84-86`, Return gated `:167`; set from `bots/objectives.rs:713` via `bots.rs:2080`; unit test around `bots/behaviour.rs:605-610`. (Still a flag, not a hold-rule case as the audit suggested.) | |
| O10 | Carrier meets an enemy: keep delivering, shoot on the way | FIXED | `committed` 0.92 / `gunning` 0.65 `bots/behaviour.rs:56-60`, `:168-176`; objective kept for committed/run-and-gun `bots.rs:1760-1769`; `can_gun` `bots.rs:2030-2032` | |
| O11 | Long route: no false timeout | FIXED | Deadline extends with approach progress `bots/objectives/turn.rs:57-77` (`APPROACH_PROGRESS`), and physical progress :59-62 | |
| O12 | Team behind: play gains worth | FIXED | `bots/team.rs:370-396` `team_deficit`, `:168-171` `pressed` (`Objective *= 1 + pressure*deficit`); default `pressure: 0.3` `crates/sim/src/bot_kind/team.rs:36-37`, `:51`; applied `bots.rs:1945`, `:2135` | |
| O13 | Carrier attacked: real damage interrupts, passive bystander does not | FIXED | `bots.rs:1756-1778` (`retaliating`, `pausing_delivery`); tests `bot_physical_objectives.rs:704` `an_actual_attacker_can_interrupt_a_retained_delivery`, `:688` `an_armed_bot_retains_delivery_instead_of_attacking_a_passive_visible_creator` | |
| O14 | Every bot replans after a goal: fair turns | FIXED | `bots/objectives.rs:652` `next_turn`, used `bots.rs:967-982`; unit test `bots/objectives.rs:1169` `round_robin_does_not_starve_same_modulo_owner_ids` | |
| O15 | Goofing during an objective game | FIXED | Goof is an option weighed against `play_worth` (`bots/surprise.rs:13-20`, `:1139-1197`); `natural` no longer requires "no objective" (`bots.rs:2497-2504`); only carrying gates it (`bots.rs:2505-2508`); Blockhead ships `surprise.strength 0.6` (`packages/blockhead_bot/assets/bots.json:18`); soak asserts goof share in (0, 0.15] and lull goofs far > near: `bot_soccer_match.rs:1267-1288` in `two_against_two_play_a_clean_match_across_seeds` | |

## Inventory

| id | Edge case | Status now | Evidence at 41287b0 | Root cause / class (if not fixed) |
|---|---|---|---|---|
| I1 | No weapon, one lying nearby: pick it up | FIXED | `bots/arming.rs:178` `arm_point`, wanted at `bots.rs:1877-1890`; Arm is a must-take option for an empty hand (`bots/behaviour.rs:109-113`); tests `bots/behaviour.rs:479` `an_empty_handed_bot_arms_before_its_objective`, gauntlet `bot_gauntlet.rs:455` `weapons_lying_on_the_ground` | |
| I2 | Empty inventory, nothing to pick up, no melee: don't chase | PARTLY | `can_retaliate` computed `bots.rs:1749-1755`, but the enemy is withheld from the chooser only when there is an objective with no enemy (`objective_without_attack` `bots.rs:1852-1853`, filter `bots.rs:2049`). With no objective, Fight scores 0.8 and Chase 0.6 on enemy presence alone (`bots/behaviour.rs:150-164`) with the default band (2,3) (`bots.rs:2033-2037`) and nothing to hit with | Fight/Chase worth ignores whether the bot can hurt anyone; the capability gate is only wired into the objective case. CORE DESIGN (option worth not grounded in capability) |
| I3 | Ammo out: switch to a usable slot | FIXED | test `bot_tactics.rs:699` `a_depleted_stored_magazine_switches_to_the_usable_undrawn_slot` | |
| I4 | Weapon can't hurt target (sheltered in vehicle, armour) | PARTLY | Non-damageable players are not enemies (`bots/sight.rs:30` `can_damage_player`, i.e. damage policy). Effectiveness signal now exists and is on (shots judged after flight `bots/surprise.rs:1103-1138`, outcome `:591-610`, applied `:521`; Blockhead strength 0.6). But a seated passenger whose seat is protected (`crates/sim/src/session/vehicles.rs:1599` `passenger_protected`) is never consulted by bot targeting (no reference in `bots/`), so the bot keeps choosing that target and only weapon/aim effectiveness decays (inferred) | Target worth does not read passenger/shelter protection; adaptation is only by failed shots and does not move off the target. NEW BEHAVIOUR |
| I5 | Melee only: close to band and swing | FIXED | footwork `bots.rs:840-862`, melee in `bots/fire.rs:345`; test `bot_brain.rs:568` `an_empty_handed_bot_that_bites_closes_in_and_bites_once_a_second` | |
| I6 | Explosive self-damage: never in own blast | FIXED | `bots/combat.rs:985` `safe_blast` (+ uses :621, :645, :900, :1172); tests `bot_tactics.rs:523` `nearby_blast_risk_selects_the_safe_second_inventory_slot`, `bot_tactics.rs:580` `a_close_ranged_bot_backs_out_from_under_a_low_roof_and_delivers_a_safe_blast` (renamed from the audit's `a_close_ranged_flyer_...`) | |
| I7 | Charged weapon, target lost: cancel without stale throw | FIXED | tests `bot_tactics.rs:258` `a_moving_target_keeps_native_windup_until_a_valid_throw`, `bot_tactics.rs:437` `an_already_released_charge_cancels_when_its_target_disconnects` | |
| I8 | Target spawn-protected: don't camp and insta-fire | PARTLY | Insta-fire fixed: reaction clock is withheld until the target is damageable (`bots.rs:1893`, `:1983` -> `bots/perception.rs:603-619` `perceive` -> `withhold` :462-466), and charge release waits (`bots/combat.rs:800-804`, `:1268`). Camping not addressed: a protected target still scores full Fight/Chase (no `spawn_protected` term in `bots/behaviour.rs` or `bot_menace` `bots/contest.rs:37-75`). No test asserts the reaction delay after protection ends (`bot_tactics.rs:728-778` only checks no shot lands during protection) | Protected targets keep full worth, so the bot stands aiming at them. NEW BEHAVIOUR |
| I9 | Flavour "drop" never throws its only weapon | FIXED | `bots/surprise.rs:1233-1241` (`spare_weapon` needs >=2 attacks and one in hand), gate `:705`; hand-off uses `bot_spare_slot` `bots/extras.rs:637-651` | |

# Appendix B: edge cases, Vehicles, Navigation, Perception and Feedback

Code read at 41287b0 (`claude/v0.2.5-s4s5bn`, detached). Read-only, nothing built or run.
Audit rows: `/mnt/project-files/bots/npc-edge-cases.md` lines 243-306.
Paths: `bots.rs` = `crates/sim/src/session/bots.rs`; `bots/x.rs` = `crates/sim/src/session/bots/x.rs`.
None of the tests cited below is `#[ignore]`d or listed in `tools/gate-known-failures.toml`, except where a row says so.

Classes: **CORE** = core design issue (structural flaw in how the bot decides or acts; must-fix). **NEW** = new behaviour (a feature bots don't have yet; can wait).

## Vehicles

| id | Edge case | Status now | Evidence at 41287b0 | Root cause if not fixed / class |
|---|---|---|---|---|
| V1 | Empty vehicle, no enemy: use it to travel, or for fun | PARTLY | With no enemy, `bot_interaction` uses an idle "mark", a player in sight (`bots/interactions.rs:524`). In idle play a bot only rides as a passenger in a vehicle a teammate already drives (`:372-382`). The board-and-drive check `route::drive_serves` (`crates/sim/src/route.rs:543`) is called only against an enemy's position (`bots/interactions.rs:409-418`). An objective can drive its own vehicle object (`bots/physical_objectives.rs:107`, `:611`). | Nothing offers an empty vehicle as a leg toward a non-enemy goal (objective point, search, explore). Driving is still an enemy-only opportunity, not a planner leg. **NEW** |
| V2 | Vehicle destroyed while seated | FIXED | "Affordance lost" check in `bot_seated_input`: on `v.destroyed` it clears plan and search and dismounts at once (`bots/interactions.rs:1291-1306`). Claims are released for a wreck (`:198-216`). No test names this case. | |
| V3 | Seat contention between bots and humans | FIXED | Seats are exclusive whoever claims them (`bots/claims.rs:20-33`). An occupied seat's claim is released (`bots/interactions.rs:213-218`). A seat is never offered if it is occupied or shares a vehicle with a non-ally (`:343-361`). Tests: `a_human_occupant_wins_over_intentions_and_hostile_bots_do_not_join_their_crew` (`crates/chaos/tests/bot_interactions.rs:546`), `two_allied_bots_contend_for_the_same_body_without_duplicate_controllers_or_awards` (`bot_creator_adversarial.rs:1005`). | |
| V4 | Driver with no route | FIXED | Headway watchdog: backs up after `VEHICLE_STALLED`, replans every second, dismounts at `VEHICLE_GIVE_UP` (720 ticks) (`bots/interactions.rs:1537-1556`, constants `:1616-1624`). Test `a_chassis_replans_or_stops_at_a_gap_only_a_pedestrian_can_fit` (`bot_interactions.rs:1119`). | |
| V5 | Flipped vehicle | FIXED | A wheeled hull that is not `route::upright` (up vector within 60°, `route.rs:531-533`) ends the drive at once (`bots/interactions.rs:1293-1296`). There is no flip-back with Activate, and no test. | (The flip-back is optional new behaviour.) |
| V6 | Water: chassis never routes through it; watercraft unsupported | FIXED | The chassis nav body has `swims: false` (`bots/interactions.rs:1265`). `nav::link` refuses wet floor to a non-swimming body (`crates/sim/src/nav.rs:617-618`). Only wheeled, non-flying vehicles are seat opportunities (`bots/interactions.rs:347-348`). `docs/architecture/bots.md:394` lists watercraft as unsupported. | |
| V7 | Gunner with no driver | FIXED | A non-driver needing travel with no driver gets out after `CREW_WAIT`, as does one with an unusable gun (`bots/interactions.rs:1567-1585`). Test `a_lone_driver_does_not_wait_forever_for_an_absent_gunner` (`bot_interactions.rs:437`). | |
| V8 | Tank's charged gun: hold until ready, then release | FIXED | Test `a_charged_mounted_weapon_is_held_until_ready_then_released_and_rearmed` (`bot_interactions.rs:796`). | |
| V9 | One reversing policy | FIXED | `route::gear` is the only rule ("This is the one reversing rule", `route.rs:466-506`), used at `bots/interactions.rs:1408-1434`. `reverses()` is gone (no match in the repo). | |
| V10 | Mounted leash when chasing past the walking leash | FIXED | `Brain::leash` uses `DRIVE_CHASE_RADIUS` from the mount anchor while driving (`bots.rs:470-483`, anchor `:1674-1682`). A passenger's leash follows it (`:1683-1687`). Test `a_mounted_driver_keeps_closing_on_a_retreating_target_beyond_its_walking_leash` (`bot_interactions.rs:1369`). | |
| V11 | Boarding a moving vehicle | FIXED | Boarding is refused above 2 m/s (`bots/interactions.rs:704-711`). A seat is not offered above 2 m/s unless the bot already holds the claim (`:350-351`). | |

## Navigation

| id | Edge case | Status now | Evidence at 41287b0 | Root cause if not fixed / class |
|---|---|---|---|---|
| N1 | Unreachable objective | FIXED | Partial routes are kept (`bots.rs:1499-1516`). After `MAX_REPLANS` the objective fails "objective navigation blocked" and the claim is failed (`bots.rs:3042-3057`). Tests: `an_unreachable_sensor_produces_a_bounded_failure_without_success` (`bot_creator_acceptance.rs:726`), `inaccessible_cheaper_region_cools_down_and_the_reachable_alternative_wins` (`bot_objectives.rs:834`). | |
| N2 | Enemy on a roof it cannot reach | FIXED | When a chase's partial route ends out of reach, `out_of_reach` is recorded (`bots.rs:1499-1511`). Fight and Search then ignore that enemy or memory (`bots.rs:2049-2056`, `:2072-2074`). A ranged weapon still reaches up through `reach_up` (`:2068-2070`). Test `a_melee_bot_gives_up_an_enemy_it_cannot_reach_on_a_roof` (`crates/chaos/tests/bot_routes.rs:892`). Caveat: the give-up is skipped for a chassis (`!body.conservative`, `bots.rs:1506`). | |
| N3 | Enemy high up and the kind can jet | PARTLY | The Fly behaviour is gone. The planner makes jet legs (`nav.rs:963`, executor `bots.rs:2877`, `route.rs:244-355`). Passing tests: `a_jetting_bot_closes_on_an_enemy_on_a_high_brick_platform` (`bot_brain.rs:996`), `a_bot_jets_from_open_sky_onto_a_high_platform_instead_of_hovering_under_it` (`bot_routes.rs:143`). Failing: `a_bot_jets_over_to_someone_above_it` (`crates/sim/tests/showcase.rs:1543`) **is in `tools/gate-known-failures.toml`**. | The climb to a 7-unit deck stalls near 4 units, although the planner's flight estimate (`route::Jets::flight`, `route.rs:81`) says it reaches. The plan model and the jet executor disagree, so the planner commits to legs the controls cannot fly. **CORE** (parked by Max for after v0.2.5, `bots/next-version.md:32`). |
| N4 | Moving platforms, or standing on a body | FIXED (inferred for moving platforms) | `on_body` detects no world floor under its feet (`bots.rs:2398-2421`). It then walks off toward its goal, or `bot_step_off` finds the nearest free floor not under a body (`bots.rs:2428-2438`, `bots/interactions.rs:962-1025`). A stall hops first (`bots.rs:3018`). The `a_jeep_on_each_side` gauntlet notes "walking straight off a vehicle's roof" (`bot_gauntlet.rs:439-441`). I found no moving-brick platform mechanism for a plan to re-anchor on. | |
| N5 | Portals | FIXED | Tests `a_bot_sees_its_enemy_through_a_portal_and_walks_through_after_them` (`bot_brain.rs:449`), `an_armed_bot_shoots_through_a_portal_from_where_it_stands` (`bot_brain.rs:496`). | |
| N6 | Deep water for a walker | FIXED | Swim edges in nav (`nav.rs:514-530`, `:616-630`, `route::Swim` `route.rs:107`). Test `a_walker_swims_across_deep_water_to_reach_its_enemy` (`bot_routes.rs:201`). Gauntlet `water_between_the_sides` (`bot_gauntlet.rs:375`) passes with a loose bound: stuck up to 35%, measured 5-8%, target under 5% (`:398-407`). | |
| N7 | Falling, drops, ledges | FIXED | `bot_fall_ahead` (`bots/interactions.rs:928`) is applied in `bot_act` (`bots/act.rs:174`). Gauntlet `rooftop_brawl_without_rails` asserts `fell == 0` (`bot_gauntlet.rs:271-292`). | |
| N8 | Stuck detection | FIXED | One progress monitor, `route::Progress::stalled` (`route.rs:157-200`). It hops on the first stall, replans and invalidates the local grid on the next, and gives up after `MAX_REPLANS` (`bots.rs:3008-3062`). Gauntlet `stairs_to_a_deck` (`bot_gauntlet.rs:341`). The vehicle headway watchdog is separate (`bots/interactions.rs:1521-1556`). | |
| N9 | Narrow door, many bots | FIXED | In a gap, a bot follows an ally ahead going the same way at that ally's pace instead of sidestepping (`bots/interactions.rs:857-881`). Test `allies_file_through_one_narrow_doorway_without_deadlock` (4 bots, `bot_brain.rs:952`). | |
| N10 | Bricks change mid-route | FIXED | Collision changes invalidate every nav each tick (`bots.rs:947-954`). Test `an_objective_recovers_when_a_real_actor_walks_out_of_its_blocked_corridor` (`bot_navigation_spike.rs:452`). | |
| N11 | Crawlspace | FIXED | Test `a_bot_crawls_through_a_low_gap` (`bot_brain.rs:867`). | |
| N12 | Search probes inside walls | FIXED | A probe is skipped when its route fails or times out (`bots/search_memory.rs:96-112`). | |
| N13 | Leash through portals | FIXED | `bot_crossed` carries yaw, leash, mount anchor and plan through the opening (`bots/lifecycle.rs:566-590`). | |

## Perception

| id | Edge case | Status now | Evidence at 41287b0 | Root cause if not fixed / class |
|---|---|---|---|---|
| P1 | Occlusion | FIXED | A player is seen at the eye, or else at the chest (`bots/sightlines.rs:143-151`). Vehicles with seats occlude (`:187`, `vehicle_between` `:214`). Unit test `a_head_behind_a_pole_with_the_chest_in_view_is_seen` (`bots/sightlines.rs:328`). | |
| P2 | Enemy behind the bot | FIXED | View cone `view_degrees` (default 180) in `perceive` (`bots/perception.rs:608-640`). Outside it, the reaction is scaled by `away_scale` (default 1.5) and the turn is capped until the bot reacts (`turn_scale` `:560`). Defaults at `crates/sim/src/bot_kind.rs:149-155`; shipped with `"perception": { "strength": 1 }` (`packages/blockhead_bot/assets/bots.json:25`). Unit test `a_goofing_bot_reacts_slower_and_errs_wider_than_one_in_combat` (`perception.rs:960`, the behind-the-bot case is at `:969-976`). | |
| P3 | Darkness or fog | STILL FAILING | Sight reach is `kind.sight` everywhere (`bots/sight.rs:93`, `:135`). Nothing in the bot code reads the environment. The engine does carry `visible_distance` / `fog_distance` (`crates/content/src/atmosphere.rs:121-126`, `Session::environment` `crates/sim/src/session/environment.rs:12`), but only as overrides. The host does not resolve the map's own values. | Perception has no environment input. **NEW** |
| P4 | Invisible or hidden players | STILL FAILING (inferred, currently moot) | `bot_sight` checks only `bot_enemy` (alive, damageable) plus the eye/chest ray (`bots/sight.rs:87-100`). I found no engine state for an invisible or hidden player body (searched `player.rs`, `session/*.rs`, net), so there is nothing to read yet. | No visibility check, because no such state exists. **NEW** |
| P5 | Very small or scaled targets | PARTLY (inferred) | The aim point is the sighted point minus a fixed 0.5 units (`bots.rs:2623`; also `bots/fire.rs:54`, `:65`, `bots/combat.rs:482`). The splash aim uses a fixed `FEET_AIM` (`bots.rs:2621`). Neither uses the target's `scale`. | For a player scaled well below 1, eye minus 0.5 falls near or below their feet, so shots go low. The offset should scale with the target's body. **CORE** (minor executor bug) |
| P6 | Stale evidence | FIXED | Tests `an_unseen_enemy_keeps_its_observed_position_and_timestamp` (`bot_knowledge.rs:244`), `expired_evidence_stops_searching_without_refresh_from_the_hidden_actor` (`bot_knowledge.rs:290`). | |
| P7 | Evidence of a dead player | FIXED | Test `a_known_dead_subject_invalidates_a_live_search_before_its_expiry` (`bot_knowledge.rs:308`). `valid_threat` requires `bot_enemy`, which is false for a dead player (`bots/sight.rs:12`, `:108`). | |
| P8 | Ally evidence feeds search | FIXED | Alerts go out (`bots.rs:2010-2016`) and are heard by allies within reach, without extending the evidence's expiry, after a seeded delay (`bots/hearing.rs:68-103`). Test `a_bot_that_sees_an_enemy_warns_its_side` (`bot_brain.rs:780`). | |
| P9 | Blasts, sounds, gaze: glance when idle, capped | FIXED | `MAX_STIMULI = 32` (`bots/perception.rs:53`). Glances only when idle (`may_glance` `:278`). Tests `a_long_stare_turns_an_idle_bots_head`, `no_glance_at_salience_zero` (`crates/chaos/tests/bot_perception.rs:239`, `:250`), unit test `a_salient_blast_in_range_draws_a_glance_only_from_an_idle_bot` (`perception.rs:817`). | |
| P10 | Hurt from behind | FIXED | A threat takes sight priority (`bots/sight.rs:109-121`). A hit from out of sight gives a rough `guess` of where it came from (`perception.rs:137`, `hurt_by` `:646`). Tests `a_hit_from_out_of_sight_gives_a_rough_place_and_the_ally_no_exact_one`, `a_hit_from_someone_in_sight_is_placed_exactly` (`bot_perception.rs:380`, `:395`). | |

## Feedback loops

| id | Edge case | Status now | Evidence at 41287b0 | Root cause if not fixed / class |
|---|---|---|---|---|
| F1 | Everyone locked goofing | FIXED | A goof ends as soon as the pause is no longer natural (threat, Interact, Carry, holding, seated: `bots.rs:2497-2504`), the gate closes, or its 2-3 s run out (`bots/surprise.rs:681-686`, `:767`). Goofs grow stale (`GOOF_BOREDOM`, `:88`). Mood pull is capped (`mood_cap` 10, `crates/sim/src/bot_kind/team.rs:49`). Unit tests `mood_pull_rises_with_the_share_seen_doing_a_flavour_and_is_capped`, `mood_makes_irregular_waves_that_flatten_without_it` (`bots/team.rs:845`, `:942`). | |
| F2 | Everyone locked serious: some goofing even in matches | FIXED | Goofing is a chooser option (`Domain::Flavour`), and boredom with play grows during objective play (`bots/surprise.rs:23-29`, `goof` `:674-787`). It is blocked only while carrying or urgent. Shipped with `surprise.strength` 0.6 (`bots.json:18`). Test `an_idle_bot_goofs_up_close_with_someone_and_leaves_bricks_alone` (`bot_extras.rs:764`). No test asserts goofs in an objective match. | |
| F3 | Oscillation too fast | FIXED | One hold rule (`behaviour::Hold`, `bots/behaviour.rs:236-364`): commit time plus margin, applied at every strength (`Mind::pick` always calls `holds[di].choose`, `bots/surprise.rs:565`). Default hold 0.5 s with a 10% margin (`bot_kind.rs:171-180`). Unit tests `a_challenger_below_the_margin_does_not_switch`, `a_held_choice_yields_at_once_to_each_interrupt`, `the_band_edge_does_not_flip_flop` (`behaviour.rs:663`, `:685`, `:461`). | |
| F4 | All bots copying one human | FIXED | A human's visible play (moving, attacking, holding) counts against goofing, not only emotes (`bots/team.rs:495-541`). The pull is a share of the players in sight, so one source is bounded by its weight over the total, and the sum is capped at `mood_cap` (`:241-264`). | |
| F5 | Pile-on | FIXED | Every visible enemy is a candidate. Each ally already after one makes it rank farther (`team_crowd`, first-come tie-break by (since, id)) (`bots/sight.rs:128-148`, `bots/team.rs:319-338`). Teamwork 0.6 is shipped (`bots.json:22`). No integration test targets spread across enemies. | |
| F6 | Deadlock between two holders | FIXED | Claims are first come (`bots/claims.rs:213-249`). The other bot covers (`contest::cover`, `bots/contest.rs:185`). Overlap breaks ties by (since, id) (`bots/team.rs:92`). | |
| F7 | Hold rule blocking a needed reaction | PARTLY | Damage interrupts the hold only when urgent (`hurt_now = hurt_by.is_some() && gate.urgent`, `bots.rs:1898`), and only from a new attacker (`hurt_anew`, `bots.rs:2101-2103`). Urgent means health under 0.3, or hit from within 8 m in the last 1.5 s (`bots/surprise.rs:943-966`). | A hit from farther than 8 m at good health waits out the commit (at most 0.5 s by default). The interrupt list is narrower than "any real damage". **CORE** (low impact: the wait is bounded by the hold time) |
| F8 | A tell freezing a bot in a fight | FIXED | The tell mechanism is gone (no `tell` in `bots/surprise.rs` or `bot_kind/surprise.rs`). The switch delay is now the perception reaction, and damage still interrupts the choice (`perception.rs:17-20`). | |
| F9 | Harm-volume ping-pong between two allies | PARTLY (inferred) | The `harm` term is added for any ally whose harm volume contains this bot's place, with no (since, id) ordering (`bots/team.rs:115-119`). Overlap does have that ordering (`:92`). `exit` moves the bot out of any ally's line (`:128-158`). There is no test. | Symmetric cost: two allies each in the other's line both pay it and both step aside, instead of only the later one. **CORE** (inferred) |
| F10 | Callout spam | FIXED | Callouts are rate-limited to one per 30 s (`CALLOUT_SECONDS`, `bots/team.rs:18`, `:607`; checked at `bots.rs:2177`, `:2270`). | |

## Counts

44 rows (V1-V11, N1-N13, P1-P10, F1-F10).

| Status | Count | ids |
|---|---|---|
| FIXED | 37 | all others (N4's moving-platform part is inferred) |
| PARTLY | 5 | V1, N3, P5, F7, F9 |
| STILL FAILING | 2 | P3, P4 |
| CAN'T TELL | 0 | |

# Appendix C: edge cases, Performance, Determinism, Abuse and Odd content

Read-only review of `claude/v0.2.5-s4s5bn` at 41287b0. Nothing built or run. Paths are relative to
`crates/sim/src/session/` unless they start with `crates/` or `tools/`. "(inferred)" marks a
conclusion reasoned from surrounding code, not traced end to end. "KF" means the test is listed
in `tools/gate-known-failures.toml`.

## Performance

| id | Edge case | Status now | Evidence at 41287b0 | Root cause if open / class |
|---|---|---|---|---|
| Pf1 | Many bots x many options share budgets | FIXED | Objective planning is one bot per tick: `bots/objectives/turn.rs:165-174`, with the round-robin candidate at `bots.rs:967-984`. Vehicle sights are capped at 32 per tick: `bots/interactions.rs:194`, `:567-576`. Combat uses a shared budget: `bots/combat.rs:9-10` (`SOLVES_PER_TICK` 2048, `RAYS_PER_TICK` 544) and `:32-60`. The sight-ray budget is new: `bots/sightlines.rs:21-32`. Unit test: `shared_budget_rotates_colliding_owner_residues_without_starvation` (`bots/combat.rs:1325`). The 16-bot profile `profile_sixteen_bots_in_the_real_bedroom` (`crates/chaos/tests/bot_battle_perf.rs:670`) is `#[ignore]` and is **KF**. | |
| Pf2 | Sight rays budgeted | FIXED | `bot_sight` (`bots/sight.rs:85-149`) now asks `bot_sees_player`, which uses the shared ledger (`bots/sightlines.rs:158-192`). Ordinary queries share 128 rays per tick and each bot keeps 4 for its current target (`:21-32`, `:109-125`). Ordinary answers are cached for 6 ticks (`:33-36`), and a stale answer stands in once the budget is spent (`:181-183`). Portal candidates (`bots/sight.rs:127-135`) still go through the same budget. | |
| Pf3 | Lane rays (mood, surprise pause, team sightline, watcher polls, surprise surface) share a budget | PARTLY | These now use the budget: team mood and sightline (`bots/team.rs:476`, `:564`), perception watcher (`bots/perception.rs:732`, `:780`) and arming (`bots/arming.rs:156`). These still cast raw rays with no budget: `surprise_pause` casts one `world_ray` per peer within 24 m on every natural tick (`bots/surprise.rs:1222-1231`, called from `bots.rs:2530`); `surprise_surface` makes 8 sweeps per splash slot per combat choice (`bots/surprise.rs:1015-1035`, called from `bots/combat.rs:688-689`); `surprise_flanks` (`bots/surprise.rs:1062`); goof aim (`:1589-1591`); extras mark and hand sight (`bots/extras.rs:251-254`, `:746-748`, capped at 3 by `take(3)` and a cadence). Surprise is on for the Blockhead (`packages/blockhead_bot/assets/bots.json:18`, strength 0.6), so these paths run. | Surprise and extras call `world_ray`/`sight` directly instead of `bot_sees`. CORE DESIGN (perception is meant to be the single, budgeted way rays are cast; small fix) |
| Pf4 | Carry `open_spot` spread over ticks | STILL FAILING | `open_spot` (`bots/sight.rs:167-188`) scans 4 rings x 12 points, each with one floor ray plus `open_at` (`:152-164`, 17 rays). That is about 880 rays in one tick, with no budget. It runs when `holding && carry.is_none()` (`bots.rs:1735-1736`). While an objective owns the hold (`objective_hold_control`, `bots.rs:1820-1822`), `carry` is reset to `None` every tick (`bots.rs:1956-1957`), so the scan repeats every tick for the whole hold (inferred). | Unbudgeted synchronous search, recomputed every tick under objective holds. CORE DESIGN (perf structure, not a missing feature) |
| Pf5 | Arming item checks budgeted | FIXED | Each item's sight check uses `session.bot_sees(..., SightUrgency::Ordinary)` (`bots/arming.rs:150-157`) under the shared ordinary budget and cache (`bots/sightlines.rs`). | |
| Pf6 | Claims growth bounded | FIXED | `MAX_CLAIMS` 16, `MAX_FAILURES` 64, `MAX_AGE` 1800 (`bots/claims.rs:6-9`). Admission cap at `:229`. Intents are pruned (`:330-338`). Test: `storage_is_bounded_and_pruning_and_eviction_are_deterministic` (`bots/claims.rs:522`). | |
| Pf7 | Surprise memory growth bounded | FIXED | Drives are keyed by (domain, option), and options are small enum or slot indices (`bots/surprise.rs:438-461`, `:874-907`, `:1131`), so they cannot grow without bound. Shots are capped at 8 (`:660-663`) and drained when due (`:666-668`). `new_life` clears holds and shots (`:420-424`). | |

## Determinism

| id | Edge case | Status now | Evidence at 41287b0 | Root cause if open / class |
|---|---|---|---|---|
| D1 | Seeded RNG: same seed, same play | FIXED (inferred) | Brain RNG is seeded from the owner (`bots.rs:494`), as are the surprise `Mind` streams (`bots/surprise.rs:411-416`). All draws go through `perception::draw` (`bots.rs:552-553`, `bots/surprise.rs:425-434`). Bot state uses `BTreeMap` throughout (no `HashMap`/`HashSet` in `bots*`). The only wall clock is `think_nanos` (`session.rs:2627-2629`), which feeds just the perf bar. Test `delays_vary_by_seed` (`bots/perception.rs:990`). The audit's `the_chooser_is_deterministic_for_a_seed` no longer exists anywhere in the repo, so no same-seed replay test remains. Three or four RNG streams are still separate (part 27 merge not done; not a failure). | |
| D2 | Seeds and ids follow join order, so the same save and order replays | FIXED | Owner-id seeds: `bots.rs:494`, `bots/surprise.rs:413-414`, `bots/cadence.rs:22` (`seed(bot, salt)`). Test `spread_is_deterministic_and_in_range` (`bots/cadence.rs:220`). | |
| D3 | Tick-order fairness | FIXED | The bot list is rotated by `tick % len` each tick (`bots.rs:987-991`). The objective turn is round-robin (`bots.rs:967-984`); test `round_robin_does_not_starve_same_modulo_owner_ids` (`bots/objectives.rs:1169`). | |
| D4 | Server-only brains, no traffic | FIXED | Module doc, `bots.rs:8-10`. Brains step inside `Session::step_bots` (`bots.rs:940`). | |

## Abuse

| id | Edge case | Status now | Evidence at 41287b0 | Root cause if open / class |
|---|---|---|---|---|
| A1 | Griefer hurts bots outside a mini-game: bots stay harmless | FIXED | `bot_enemy` ends in `can_damage_player` (`bots/sight.rs:9-30`). Hurt and objective threats are filtered through `bot_enemy` (`bots/sight.rs:108-115`). Module doc `bots.rs:15-16`. | |
| A2 | Griefer kites a bot away: leash, then return | FIXED | Past the leash, memory, search and target are dropped (`bots.rs:2019-2024`) and the enemy is filtered (`bots.rs:2027`). Return scores on `strayed` (`bots/behaviour.rs:167`, `bots.rs:2075-2079`). Leash: `bots.rs:450-482`. Untethered rules bots have no leash by design (`bots.rs:475-481`). Related test: `a_mounted_driver_keeps_closing_on_a_retreating_target_beyond_its_walking_leash` (`crates/chaos/tests/bot_interactions.rs:1369`). | |
| A3 | Griefer boxes a bot in: give up the goal, wander | FIXED | The first stalled window hops, later ones replan (`bots.rs:3008-3026`). After `MAX_REPLANS` (3, `bots.rs:167`), the goal is cleared, Arm passes the item over, Objective fails, the claim fails and the wander is rescheduled (`bots.rs:3042-3056`). (inferred) A Chase or Fight is demoted only through surprise effectiveness (`bots.rs:3023-3035`), so a bot can re-enter the same chase. | |
| A4 | Griefer holds or stands on the ball | FIXED | Bodies held by others are skipped or rejected (`bots/physical_objectives.rs:426`, `:785-787`; `bots/interactions.rs:436`). An opponent standing in the body's way is shoved (`bots/contest.rs:274-276`). Test `the_brain_rejects_a_hazard_currently_in_a_players_grip` (`crates/chaos/tests/bot_physics_interactions.rs:649`). | |
| A5 | Griefer takes a bot's seat: human wins | FIXED | An occupied seat releases its claim (`bots/interactions.rs:203-218`). Native seat admission is authoritative (`:692-714`). Opportunities skip occupied seats (`:346`). Tests: `a_human_occupant_wins_over_intentions_and_hostile_bots_do_not_join_their_crew` (`crates/chaos/tests/bot_interactions.rs:546`); `two_allied_bots_contend_for_the_same_body_without_duplicate_controllers_or_awards` (`crates/chaos/tests/bot_creator_adversarial.rs:1005`). | |
| A6 | Bot stuck in a goof loop: cooldown, then ends | FIXED | A goof lasts 2-3 s (`bots/surprise.rs:89`, `:767-770`). Walk-up extensions are added once at Begin (`:1345-1371`). It ends when the pause is no longer natural (threat, interact, carry, seat: `bots.rs:2497-2504`), when the gate closes, or at time-out (`bots/surprise.rs:681-685`). The goof's own boredom is a cooldown (`:88`, `:583`). By design it no longer ends merely because an enemy or objective is in view: play worth decides (`:1148-1198`). Test `an_idle_bot_goofs_up_close_with_someone_and_leaves_bricks_alone` (`crates/chaos/tests/bot_extras.rs:764`). | |
| A7 | Spawn camping: no insta-kill at spawn | PARTLY | The insta-kill is fixed. A protected target withholds the reaction (`bots.rs:1891-1893`, `:1983`; `bots/perception.rs:460-466`, `:608-618`), so the reaction clock starts only once the target can be hurt. Release is blocked while protected (`bots/combat.rs:800-804`, `:1268`). Test `a_protected_target_restarts_the_reaction_when_it_can_be_hurt` (`bots/perception.rs:1015`). Still open: `spawn_protected` (`bots/combat.rs:1002-1008`) never reaches the `Situation`/scores (`bots.rs:2040-2080`), so a protected enemy keeps full Fight/Chase worth. The bot stays planted aiming at the spawn and fires one reaction delay after protection ends. | The chooser's worth ignores "cannot hurt yet". CORE DESIGN (chooser input; same family as N2/I2) |
| A8 | Emote spam steering mood: capped | FIXED | `mood` pull `.min(cfg.mood_cap)` (`bots/team.rs:263`). Only seen others within radius count (`:250-251`). Test `mood_pull_rises_with_the_share_seen_doing_a_flavour_and_is_capped` (`bots/team.rs:845`). | |

## Odd content

| id | Edge case | Status now | Evidence at 41287b0 | Root cause if open / class |
|---|---|---|---|---|
| C1 | Add-On weapons with unusual datablocks: never invent melee, fall back safely | PARTLY (inferred) | The tactics capability refuses to guess (`MissingAttack`, `ScriptedTool`; test `absent_projectile_and_unknown_scripts_are_never_invented_melee`, `bots/tactics.rs:1235`). Test `unfamiliar_direct_splash_charge_and_melee_families_fire_and_hit_through_real_controls` (`crates/chaos/tests/bot_tactics.rs:500`). But when native combat is `Unsupported` (no supported attack, through a portal, or mounted: `bots/combat.rs:465-476`, `:829`), `bot_hand` falls back to legacy `bot_weapon` (`bots/fire.rs:157-164`). For an image with no projectile and no hitscan, that sets `melee: reach < 6.0`, with reach defaulting to 3.0 (`bots/fire.rs:248-261`). The bot then closes to melee band and pulls the trigger ungated (`bots.rs:2646-2659`, `:1240-1295`). | The legacy fallback guesses melee from reach when data is missing. CORE DESIGN ("unknown semantics remain unsupported" is violated in the fallback) |
| C2 | Hitscan weapons use the ray's range | FIXED | Legacy path: `reach: using.reach.unwrap_or(ray.range)` (`bots/fire.rs:224-235`). Test `a_hitscan_guard_understands_ray_range_and_hits_without_chasing_into_melee` (`crates/chaos/tests/bot_knowledge.rs:518`, not ignored, not KF). | |
| C3 | Hold tools (gravity gun): hold and fling | FIXED | `BotFire::Hold` gives `hold` (`bots/fire.rs:218-219`). The Carry behaviour scores `holding` (`bots/behaviour.rs:149`). Goal at `bots.rs:2354-2357`; swing and fire at `bots.rs:2580-2595`; open spot at `bots/sight.rs:167` (see Pf4). No dedicated chaos test found for a fling (inferred). | |
| C4 | Script-driven mini-games: typed package objectives | CAN'T TELL | The package objective machinery exists (`bots/package_objectives.rs`, `bots/objectives.rs`). Test `unfamiliar_package_composes_real_pickup_and_return_with_canonical_winner` (`crates/chaos/tests/bot_carryable_objectives.rs:356`, not ignored). The imported-CTF test `imported_ctf_bot_takes_the_actual_flag_and_returns_it_for_its_team` (`crates/chaos/tests/bot_carryable_ctf.rs:19`) is `#[ignore]` and **KF** ("expected old bot behaviour after the v0.2.4 bot merge"). Code alone can't show whether real Slayer CTF still works. | If it fails, the cause is unknown from code; the KF note suggests stale test expectations |
| C5 | Unusual seat layouts: roles from data | FIXED | Roles come from `d.seats[seat]` (`role.controls`, `role.weapon`, `d.control_seat()`) (`bots/interactions.rs:344-386`). Test `unfamiliar_reordered_seats_form_crews_drive_and_fire_their_actual_weapon` (`crates/chaos/tests/bot_interactions.rs:375`). | |
| C6 | Swimmer on land: package policy | FIXED | `out_of_water_seconds` kills a dry swimmer, in a mini-game only (`bots.rs:1166-1176`; `crates/sim/src/bot_kind.rs:88`). Related tests: `a_swimming_bot_roams_its_water_at_every_depth_and_never_leaves_it` (`crates/chaos/tests/bot_brain.rs:663`). | |
| C7 | Flying wheeled vehicles not used | FIXED | Seat opportunities exclude `d.wheeled_flight.is_some()` (`bots/interactions.rs:347-348`; also `:1295`). Objective drive also checks it (`bots/physical_objectives.rs:584`). | |

# Appendix D: every bot test

This was a read-only review: nothing was built or run. Tiers follow the owner's shape from 2026-10-05. Verdicts follow Max's update: bot tests should check only absolutes and bizarre outcomes.

**Tiers**

| Tier | Meaning |
|---|---|
| **1** | Mechanism: exact or deterministic. Rule outcomes count here even when the session steps. |
| **1s** | Single-scenario capability: one scripted scene, one implicit seed, a generous time budget and a yes/no outcome ("reaches", "hits", "scores"). It is still one run of a chaotic sim, but no tight number is involved. In the counts it is listed apart from tier 1. |
| **2** | Never-invariant (a hard gate). Seeds are given in the table. |
| **3** | Band over several seeds. |
| **4** | Fragile: one run, or several runs with tight numbers, or an A/B share comparison on one seed, or a test that pins a limitation. |
| **T** | Opt-in tool or diagnostic. Note that the gate runs `--include-ignored`, so an `#[ignore]` test still runs in the gate unless `[[skip]]` lists it. |

**Verdicts**

| Verdict | Meaning |
|---|---|
| **KEEP** | Already an absolute or bizarre-outcome check, or an exact mechanism test. |
| **CONVERT** | Convert to an absolute. The column says which absolute. |
| **DELETE** | Delete, with a one-line reason. |

"Seeds" means bot random streams. Outside the gauntlet's `BRI_TUNING_SEEDS` and soccer's `.seeded(n)` (which shifts the pads), a bot's generators follow its id and the tick. So every other test runs **one implicit seed**. The "variants" in the creator and objective tests are layouts, not seeds.

Known failures come from `tools/gate-known-failures.toml`, written as KF.

---

## crates/chaos/tests/bot_soccer_match.rs

The `never()` function is checked per match: ball lost = 0, combat = 0 (except with swords), ball untouched ≤ **6 s**, slowest kickoff ≤ **6 s**, stuck ≤ 25 % of bot-time.

`Totals::problems()` is checked across the matches: goals ≥ number of matches, own goals ≤ (goals+1)/4, stuck ≤ **5 %**, circling ≤ **1 %**, idle ≤ **3 %**, ignoring the ball ≤ **10 %**, clumped ≤ **15 %**, ball on a wall ≤ 15 %, facing away ≤ **0.05**, wrong way ≤ **0.10**, jitter ≤ **3** per bot-minute.

Commit 413e75c restored these from 15 s / 15 s / 8 % / 3 % / 6 % / 15 % / 20 % / 0.12 / 0.2 / 6.

| Line | Test | Tier | Seeds | Thresholds | KF/ign | Verdict |
|---|---|---|---|---|---|---|
| 1225 | two_against_two_play_a_clean_match_across_seeds | **4** (several seeds, tight) | 3 seeds × 3 kits = 9 matches × 150 s | never() + Totals as above; broom clearing > 0; goof share in (0, 0.15]; goof near ball ≤ 0.25 × goof far | – | **CONVERT**: over the 9 matches, at least 1 goal per kit; the ball is never lost; no combat with damage off; no bot stuck for a whole match (longest stuck spell < 60 s); goof share > 0; own goals < total goals; ball never untouched > 60 s. Print the shares, the lull ratio and broom clearing as a report that does not gate. |
| 1295 | other_line_ups_and_an_obstacle_field_play_on | **4** | 5 line-ups × 2 seeds = 10 × 90 s | never() + Totals | – | **CONVERT**: the same absolutes, with at least 1 goal over all 10 matches and none per line-up required. |
| 1328 | the_shipped_soccer_save_loads_a_ready_match | **1** (load) + **4** (match) | 1 run, 90 s | stock bricks only, teams Blue/Red; never() + Totals on one match | – | **CONVERT**: keep the load and team assertions. Cut the match to absolutes (bots join their pads' teams, someone touches the ball, the ball is not lost), or drop the match half: test 1225 covers play. |
| 1395 | the_shipped_field_survives_load_bricks_whatever_the_pads_size | 1 | – (5 pad sizes) | bricks within 0.25/0.1 of where saved | – | KEEP |
| 1473 | sword_soccer_plays_the_ball_and_fights_for_it | 3 (2 seeds) | 2 × 90 s | goals ≥ 2; combat > 0; combat < 50 % of bot-time; near the ball > 25 %; ball lost = 0 | – | **CONVERT**: at least 1 goal over the seeds, combat > 0, ball never lost. Report the 50 % and 25 % shares. |
| 1499 | rockets_spears_and_jeeps_keep_the_match_going | **4** | 3 set-ups × seed 1 only, 60 s | goals ≥ 1 each; ball lost 0; untouched ≤ 10 s; on a wall ≤ 8 s; stuck ≤ 10 %; circling ≤ 2 %; idle ≤ 5 %; kickoff ≤ 8 s | – | **CONVERT**: 3 seeds per set-up; at least 1 goal per set-up across its seeds; ball never lost; no bot stuck the whole run; ball never untouched > 60 s. |

## crates/chaos/tests/bot_gauntlet.rs and gauntlet/

Every scenario runs once at tuning seed 0. The file header says the thresholds "sit just past today's measured play". Each scenario also calls `finish()`, which in turn calls `shares::check` and enforces `behaviour_bands.json` (DORMANT and DOMINANT shares) on that single run.

`within(stuck, idle, circling, switches/min)` adds two more checks: clumped ≤ 1 % and off-target ≤ 5 % of shots.

| Line | Test | Tier | Seeds | Thresholds | KF/ign | Verdict |
|---|---|---|---|---|---|---|
| 208 | deathmatch_open_field | **4** | 1, 60 s | kills ≥ 40; team kills + fire at allies = 0; within(.01, .01, .12, 26); bands | – | **CONVERT**: 3 seeds; kills > 0; team kills = 0 (KEEP as a never-invariant); no bot stuck the whole run. Bands and `within` become a report. |
| 233 | swords_four_a_side | **4** | 1, 60 s | kills ≥ 10; team kills 0; within(.03, .03, .15, 30) | – | **CONVERT**: same as above. |
| 249 | deathmatch_mixed_arsenal | **4** | 1, 60 s | kills ≥ 40; within(.01, .01, .10, 28); self kills 0; team 0 | – | **CONVERT**: kills > 0; team 0; self kills < total deaths, not = 0. |
| 271 | rooftop_brawl_without_rails | **4** | 1, 60 s | kills ≥ 40; within(.01, .01, .08, 58); falls 0; team 0 | – | **CONVERT**: kills > 0; team 0; not every bot falls off. |
| 341 | stairs_to_a_deck | **4** | 1, 60 s | within(.08, .01, .04, 30); kills ≥ 15 | – | **CONVERT**: kills > 0, so the deck was reached. |
| 375 | water_between_the_sides | **4** | 1, 60 s | within(.35, .01, .02, 20); kills > 0 | – | **CONVERT**: keep kills > 0 and drop `within`. |
| 412 | a_jeep_on_each_side | **4** | 1, 60 s | within(.08, .12, .03, 32); mounted ticks ≥ 10 000 | – | **CONVERT**: mounted ticks > 0. |
| 455 | weapons_lying_on_the_ground | **4** | 1, 60 s | within(.01, .01, .10, 50); pickups; kills > 0 | – | **CONVERT**: a weapon was picked up, and kills > 0. |
| 506 | zombie_survival | **4** | 1, 60 s | kills ≥ 35; within(.01, .01, .08, 12); team 0 | – | **CONVERT**: kills > 0; team 0. |
| 553 | capture_the_flag | **4** | 1, 90 s | within(.05, .01, .07, 30); captures ≥ 3; team 0 | – | **CONVERT**: at least 1 capture across 3 seeds; team 0. |
| 583 | runners_cross_head_on | **4** | 1 | within(.02, .01, .12, 18); captures ≥ 2 | – | **CONVERT**: at least 1 capture. A head-on deadlock is the bizarre outcome, and no captures already catches it. |
| 631 | a_run_longer_than_the_approach_timeout | **4** | 1 | within(.02, .01, .05, 6); captures ≥ 2 | – | **CONVERT**: at least 1 capture. |
| 789 | checkpoint_race | **4** | 1, 90 s | within(.01, .01, .17, 4); won within 0–20 s; laps ≥ 9 | – | **CONVERT**: the race is won within 90 s. |
| 869 | surprise_by_strength | 1 (strength 0) + absolute | 1 (3 strengths × 2 scenes × 60 s) | strength 0 gives goof = 0 and surprised = 0; strength > 0 gives idle goof > 0; kills > 0; bands via finish | – | KEEP. It is already absolutes ("never goofs when off", "goofs at all when on"). Make its bands a report. |
| 973 | tuning_dials_and_seeds_reach_the_scenario | 1 | 2 settings × 2 seeds | goof(1,0) > 0; goof differs between seeds | – | KEEP |
| 1014 | all_dials_on | **4** | 1 (12 scenarios) | no scenario broken; enforced bands | ignored, but **the gate runs it** | **CONVERT**: gate only on `broken`. The bands become a report. |
| 1066 | bot_think_time_16 | **4** (timing) | 1 | think time ≤ 15 000 µs in debug, 2 000 µs in release | ignored, **not in [[skip]], so the gate runs it** | **DELETE as a gate**: it is wall-clock and depends on the machine. Add `[[skip]]` and keep it as an opt-in report. |
| 1425 | fair_hit_rate | 3/4 (a band on one seed) | 1 (5 weapons × 2 ranges × 25 s) | steady hit rate in 15–60 % for all weapons together, and separately for the gun and the bow | – | **CONVERT**: hits > 0 ("never hopeless") and hit rate < ~95 % ("never perfect"). Report the band. |
| 955 / 965 / 1453 | off_switches / dial_sweep / fair_by_dial | T | BRI_TUNING_SEEDS | – | ignored and in [[skip]] | KEEP as tools. |
| shares.rs 359, 387 | dormant_needs_an_offer…, the_shipped_bands_parse | 1 | – | – | – | KEEP |

## crates/chaos/tests/bot_navigation_spike.rs

| Line | Test | Tier | Seeds | Thresholds | KF/ign | Verdict |
|---|---|---|---|---|---|---|
| 32 | ordinary_actors_keep_a_three_body_stack_supported | 1 (physics, no bot) | – | lowest point ≥ layer height − 0.05 | – | KEEP. Could move to physics tests. |
| 129 | an_actor_head_is_a_real_jump_support_for_an_unreachable_platform | 1 (scripted inputs) | – | reached is false alone, true when supported | – | KEEP |
| 282 | a_rotated_off_grid_narrow_door_is_passable_by_human_and_pursuing_bot | 1 (human) + 1s (bot) | 1 | human within 0.5; bot within 2.5 in 35 s | – | KEEP. "Bot never gets through" is the absolute. |
| 347 | fixed_navigation_does_not_discover_the_physically_valid_actor_support | **4** (pins a limitation) | – | expects `Found::Partial` | – | **DELETE**: it pins a known limitation, so it fails when navigation improves. |
| 412 | a_distant_authored_objective_continues_multiple_bounded_navigation_segments | 1s | 1 | score = 17 within 60 s; max x > 78 | – | KEEP |
| 452 | an_objective_recovers_when_a_real_actor_walks_out_of_its_blocked_corridor | 1s | 1 | blocked x < 0.5 at 4 s; score 17 within 20 s | – | KEEP |
| 602 | an_unmodelled_named_target_reaction_rejects_the_whole_objective_action | 1 | 1 | plain case scores 17; reactive case rejected | – | KEEP |
| 625 | workshop_race_recipe_finishes_three_laps_through_actual_npc_region_crossings | 1s | 1 | 3 laps within 90 s; no 4th lap | – | KEEP |

## crates/chaos/tests/bot_battle_perf.rs

| Line | Test | Tier | Seeds | Thresholds | KF/ign | Verdict |
|---|---|---|---|---|---|---|
| 670 | profile_sixteen_bots_in_the_real_bedroom | T (profile on real content) | 1 | no timing thresholds; requires 16 bots spawned, packages loaded and finite snapshots. The combat-in-every-10-s-window check runs only with `BRI_BATTLE_REQUIRE_ACTIVE=1`. | ignored (content); **KF** | **DELETE as a gate**: it is a profile by its own header. Add `[[skip]]` and drop the KF entry. If a gate is wanted, add a small content test: "16 bots spawn in the Bedroom, snapshots stay finite". |

## Other content- and perf-backed bot tests in crates/chaos

| Line | Test | Tier | Seeds | Thresholds | KF/ign | Verdict |
|---|---|---|---|---|---|---|
| bot_firefight.rs:11 | bedroom_mixed_firefight_survives_spawn_loadout_changes | 2 (absolutes) | 1 (2 × 45 s) | 16 bots spawn; something fired; snapshots finite | ignored (content); **KF** | KEEP. These are absolutes, so fix the cause rather than loosen. |
| bot_carryable_ctf.rs:19 | imported_ctf_bot_takes_the_actual_flag_and_returns_it_for_its_team | 1s | 1, 35 s | carried; composed; score = 20; flag-returned event; the team wins | ignored (content); **KF** | KEEP the absolutes. If it is only slow, widen the 35 s budget. |
| bot_combined_perf.rs:340 | sixteen_objective_controllers_and_mixed_inventory_combat | T, with absolutes | 1 (3 flag counts × 7200 ticks × 2 phases) | for ≤ 8 flags, score ≥ 17 and the observer is reached; fight > 0 and damage seen | ignored; the gate runs it | KEEP (absolutes). Add `[[skip]]` if its runtime matters. |
| bot_watch.rs:167 | watch_bots_play_a_real_save | T | – | none. It errors when `BRI_WATCH_SAVE` is unset. | ignored; **not in [[skip]] or KF**. It looks like it would fail under the gate's `--include-ignored`; verify. | **DELETE as a gate**: it is a watching tool. Add `[[skip]]`. |

## crates/chaos/tests/bot_brain.rs (18 tests, one seed each)

| Line | Test | Tier | Thresholds | Verdict |
|---|---|---|---|---|
| 131 | a_bot_walks_round_a_wall_it_sees_through… | 1s | goes round the wall (not through it); within 4 in 25 s | KEEP |
| 174 | an_armed_bot_takes_a_moment_to_react_then_hits… | 1 | no hit before the reaction, a hit after | KEEP |
| 239 | bots_of_one_builder_are_on_one_side | 1 | – | KEEP |
| 279 | without_a_bot_add_on_a_spawn_brick_makes_no_bot | 1 | – | KEEP |
| 308 | clear_bots_also_clears_the_mounts_nobody_rides | 1 | – | KEEP |
| 372 | a_bot_over_the_server_limit_tells_its_builder | 1 | 16 bots | KEEP |
| 449 | a_bot_sees_its_enemy_through_a_portal_and_walks_through… | 1s | **out of the room in < 3 s**; within 3 in 20 s | **CONVERT**: "goes through within the 20 s"; drop the 3 s limit. |
| 496 | an_armed_bot_shoots_through_a_portal… | 1s | a hit within 15 s | KEEP |
| 568 | an_empty_handed_bot_that_bites… once a second | 1 | 2 bites of 15 damage, ≥ 119 ticks apart | KEEP (cadence mechanism) |
| 609 | a_bot_keeps_its_kinds_body… | 1 | – | KEEP |
| 663 | a_swimming_bot_roams_its_water… never leaves it | 2 (one seed) + 1s | never leaves the water; depth range > 1.5; nearest point < 10.6 | KEEP |
| 706 | a_swimming_bot_goes_up_after_a_swimmer… | 2 + 1s | never leaves the water; bites within 15 s | KEEP |
| 744 | a_bot_with_a_spear_holds_it_back_then_throws_it | 1s | a hit within 20 s | KEEP |
| 780 | a_bot_that_sees_an_enemy_warns_its_side | 1 (alerts on vs off) | – | KEEP |
| 833 | a_bot_whose_kind_never_chases_holds_its_post | 1 (A/B) | moves > 6 when chasing, < 1 when not | KEEP |
| 867 | a_bot_crawls_through_a_low_gap | 1s | crosses in the gap; within 3.5 in 10 s | KEEP |
| 912 | a_bot_enters_through_a_brick_building_doorway | 1s | within 3.5 in 15 s | KEEP |
| 952 | allies_file_through_one_narrow_doorway_without_deadlock | 1s | all 4 through within 30 s | KEEP (an absolute: no deadlock) |
| 996 | a_jetting_bot_closes_on_an_enemy_on_a_high_brick_platform | 1s / 4 | closest < 4.5; **farthest in the last 5 s < 5.0**; a hit | **CONVERT**: reached and hit only. Drop the "stays within 5" check: a dodge or goof step can break it. |
| 1047 | a_bot_that_fails_to_think_leaves_the_others_thinking | 1 | told ≤ 1 | KEEP |

## crates/chaos/tests/bot_extras.rs (11 tests, one seed)

| Line | Test | Tier | Thresholds | Verdict |
|---|---|---|---|---|
| 213 | an_idle_bot_near_a_loose_ball_pushes_it | **4** (A/B on one seed) | off < 0.3; played > off + 1.0 (20 s) | **CONVERT**: with idle play off the ball never moves (< 0.3: KEEP); with it on, the ball moves (> 0.3) in at least one of 3 seeds. |
| 294 | a_bot_under_ranged_fire_crouches_more_than_one_not_under_fire | **4** (A/B with +120-tick margins) | under fire > quiet + 120; under fire > plain + 120 (15 s each) | **DELETE**: it measures how often a flavour fires, which is tuning. If it must stay, make it "crouches at least once under fire". |
| 312 | a_predicted_hit_triggers_a_hop_more_often_than_a_shot_that_misses | **4** (A/B, +60 ticks) | aimed-at > wide + 60 | **DELETE**: the predicted-hit mechanism has exact unit tests in `bots/extras.rs` (762, 777). |
| 389 | a_dodge_jets_up_only_for_a_kind_whose_jets_fly | 2 + absolute | flies > 0; grounded kind = 0 | KEEP |
| 501 | an_activatable_door_on_the_route_gets_activated | 1s | opened, then out | KEEP |
| 591 | a_bot_opens_a_door_for_fun_but_never_presses_an_event_button | 2 (one seed) | fun clicks > 0; event presses = 0 | KEEP. Better over 3 seeds. |
| 667 | a_spare_weapon_ends_up_with_an_unarmed_teammate | 1s (A/B) | handed with teamwork on, not with it off | KEEP |
| 718 | bots_of_one_kind_get_varied_seeded_looks | 1 | distinct looks ≥ bots/2; deterministic | KEEP |
| 744 | brick_bot_names_come_from_first_names_with_no_duplicates | 1 | – | KEEP |
| 764 | an_idle_bot_goofs_up_close_with_someone_and_leaves_bricks_alone | 2 (one seed, 120 s) | no brick changed; at least 1 close goof | KEEP. Better over 3 seeds. |

## crates/chaos/tests/bot_perception.rs (5 tests)

| Line | Test | Tier | Thresholds | Verdict |
|---|---|---|---|---|
| 239 | a_long_stare_turns_an_idle_bots_head | 1 | off < 12° | KEEP |
| 250 | no_glance_at_salience_zero | 1 | – | KEEP |
| 298 | a_relaxed_bot_returns_fire_after_its_longer_reaction | 1 (seeded delay) | delay 126–234 ticks; relaxed ≥ plain + 60 | KEEP |
| 380 | a_hit_from_out_of_sight_gives_a_rough_place… | 1 | 1 ≤ miss < 25 | KEEP |
| 395 | a_hit_from_someone_in_sight_is_placed_exactly | 1 | < 0.05 | KEEP |

## crates/chaos/tests/bot_routes.rs (9 tests)

| Line | Test | Tier | Thresholds | Verdict |
|---|---|---|---|---|
| 143 | a_bot_jets_from_open_sky_onto_a_high_platform… | 1s / 4 | jetted; **hovering < 120 ticks**; reached; hit (20 s) | **CONVERT**: reached and hit. Turn hovering into "not under the platform for most of the run". |
| 201 | a_walker_swims_across_deep_water… | 1s | reached within 30 s; swam | KEEP |
| 471 | unarmed_bots_with_only_a_ball_to_play_never_take_off | 2 (one seed, 30 s) | never jets; never a jet leg; played | KEEP |
| 499 | a_vehicle_that_does_not_serve_the_ball_game_attracts_no_bot | 2 | never chooses the vehicle; played | KEEP |
| 628 | a_jeep_drives_straight_to_a_point_ahead_and_stops_there | 1 | – | KEEP |
| 649 | a_driver_runs_down_an_enemy_who_sidesteps… | 1s | dodged, then hit within 40 s | KEEP |
| 755 | two_drivers_who_cannot_hurt_each_other_get_out_and_fight | 1s | turns < 3 full turns (no endless circling); drove; fought | KEEP |
| 820 | a_driver_searches_round_a_wall_from_an_allys_sighting | 1s | – | KEEP |
| 892 | a_melee_bot_gives_up_an_enemy_it_cannot_reach_on_a_roof | 1s / 4 | chased > 0; **gave up within 10 s** of 20 | **CONVERT**: "not still chasing at the end of 20 s". Note that a goof also counts as "gave up" here. |

## crates/chaos/tests/bot_tactics.rs (16 tests)

| Line | Test | Tier | Thresholds | Verdict |
|---|---|---|---|---|
| 258 | a_moving_target_keeps_native_windup_until_a_valid_throw | 1 | armed ≥ 12 ticks; threw; hit | KEEP |
| 316, 353, 373, 399, 437 | the charge-cancel and release tests | 1 | exact state checks | KEEP (5) |
| 500 | unfamiliar_direct_splash_charge_and_melee_families_fire_and_hit… | 1s | – | KEEP |
| 523 | nearby_blast_risk_selects_the_safe_second_inventory_slot | 1s + never | clearance > 7 for every blast | KEEP |
| 580 | a_close_ranged_bot_backs_out_from_under_a_low_roof… | 1s / 4 | escaped; **jetting under the roof < 30 ticks**; blasts never closer than 7; hit | **CONVERT**: drop the 30-tick limit, keep the never-closer-than-7 rule and the hit. |
| 656 | a_long_flight_weapon_closes_distance… | 1s | – | KEEP |
| 699 | a_depleted_stored_magazine_switches… | 1 | – | KEEP |
| 791, 898 | the native-hold tests | 1 | – | KEEP (2) |
| 1043 | the_strongest_weapon_is_used_not_the_first | **4** (ratio on one seed) | strong ticks > 4 × weak ticks | **CONVERT**: the strong weapon is drawn at some point and there is a hit. The choice itself is covered exactly by `bots/arming.rs` unit tests. |
| 1080 | a_splash_weapon_aims_low_more_often_than_not | **4** (≥ 6 rockets, one seed) | low > chest | **DELETE**: it is an aim-point share, which is tuning and belongs in a report. |

## crates/chaos/tests/bot_interactions.rs (16 tests)

| Line | Tier | Verdict |
|---|---|---|
| 347, 375, 437, 461, 483, 495, 524, 546, 663, 741, 796, 841, 1283, 1481 | 1 or 1s: seat and crew rules, a generous "moved > 5" and similar | KEEP (14) |
| 982 a_driver_brakes_for_a_grounded_ally… | 1 + never (an ally is never hit, health stays 100) | KEEP |
| 1079 an_unarmed_chassis_pursues_and_hurts_by_physical_runover | 1s | KEEP |
| 1119 a_chassis_replans_or_stops_at_a_gap… | 1s | KEEP |
| 1203 an_armed_crew_advances… | 1s (advance > 3) | KEEP |
| 1369 a_mounted_driver_keeps_closing_on_a_retreating_target… | 1s / 4: gap closes by > 5; **reversing ≤ 10 % of far ticks** | **CONVERT**: drop the 10 % reversing share or report it; keep "closes the gap". |

## crates/chaos/tests/bot_soccer_teams.rs (5 tests)

| Line | Test | Tier | Thresholds | Verdict |
|---|---|---|---|---|
| 322, 365, 501 | team and save mechanics | 1 | – | KEEP (3) |
| 403 | a_bot_scores_in_its_own_teams_goal_after_save_and_reset | 1s | a goal within 90 s, in the right goal | KEEP ("no goal in 90 s" is the absolute) |
| 672 | recipe_goals_bots_contest_one_ball_score_and_play_on_after_the_reset | 1s / 4 (2 swaps, one seed, 60 s) | each targets > 120 ticks; contested > 120; nearest < 4; **combat ≤ 10 % of play**; score exactly 1; retargets | **CONVERT**: keep score = 1 (rule), retarget after reset, both bots reach the ball. Change combat from ≤ 10 % to "not all fighting" (< 50 %), or report it. |

## Remaining crates/chaos files: all tier 1 or 1s, KEEP

| File | Tests | Notes |
|---|---|---|
| bot_knowledge.rs | 8 | search memory and expiry; 439 is a never-invariant (no same-team targeting with friendly fire on) |
| bot_objectives.rs | 12 | planner and rule outcomes; exact scores |
| bot_physical_objectives.rs | 16 | delivery and hold rules; exact outcomes in 2 renamed layouts |
| bot_search_objectives.rs | 4 | – |
| bot_team.rs | 1 | – |
| bot_objective_rest.rs | 2 | – |
| bot_physics_interactions.rs | 11 | 103–196 are physics mechanism; 611–693 are brain scenes with never-checks such as "pushed < 0.1" |
| bot_carryable_objectives.rs | 7 | – |
| bot_creator_acceptance.rs | 8 | 2 metamorphic variants each |
| bot_creator_adversarial.rs | 5 | – |
| bot_creator_heldout.rs | 1 | – |
| shark_policy.rs | 15 | 14 tier 1. 988 is ignored, needs content, and is 1s. |

## Bot tests in other crates

| Location | Test | Tier | Thresholds | KF | Verdict |
|---|---|---|---|---|---|
| addon-import/tests/slayer.rs:4540 | team_fill_bots_play_slayer_soccer_with_or_without_weapons | **4** (2 runs, one seed, 90 s) | goals ≥ 2; no kill or death scores; share in the air < **2 %**; on the tank < 5 s at a stretch; near the tank < **10 %** | – | **CONVERT**: at least 1 goal per arm over 3 seeds; kills never score (KEEP, rule); never on the tank for > 30 s. Report the shares. |
| slayer.rs: 2403, 2511, 2628, 2674, 2695, 2812, 3082, 3088, 4210 | team-fill mechanics | 1 | – | – | KEEP (9) |
| sim/tests/showcase.rs: 1325, 1426 | grab and beam | 1 | – | – | KEEP |
| showcase.rs: 1411, 1470, 1514 | grab and throw, catch from across the room, carry out to throw | 1s | held > 60 ticks; flew > 1.5 in 0.1 s; caught from > 8 | – | KEEP |
| showcase.rs:1543 | a_bot_jets_over_to_someone_above_it | 1s (1800 ticks) | reaches someone 7 units up | **KF** | KEEP (an absolute capability). It is parked until the climb is fixed. |
| sim/tests/script_api.rs: 1467, 1600, 1644 | rules add, rest and remove bots | 1 | – | – | KEEP |
| carry_rules.rs 436, 514; v20_events.rs 456; bot_holes.rs 15, 81; client add_on_join.rs 882; vehicles.rs 1232 | spawn, permission and import mechanics | 1 | – | – | KEEP |

## Unit tests (`#[cfg(test)]`)

| Module | Tests |
|---|---|
| sim/src/session/bots.rs | 3 |
| bots/act.rs | 2 |
| arming.rs | 4 |
| behaviour.rs | 13 |
| cadence.rs | 6 |
| charged_control.rs | 2 |
| claims.rs | 10 |
| combat.rs | 6 |
| combat_objectives.rs | 1 |
| contest.rs | 4 |
| cooldown.rs | 1 |
| explore.rs | 1 |
| extras.rs | 2 |
| looks.rs | 1 |
| objectives.rs | 4 |
| perception.rs | 18 |
| physical_objectives.rs | 8 (includes `a_push_that_would_score_an_own_goal_turns_off_it`, the exact own-goal check) |
| planning.rs | 21 |
| search_memory.rs | 7 |
| sightlines.rs | 3 |
| tactics.rs | 21 |
| team.rs | 11 |
| tuning.rs | 1 |
| why.rs | 1 |
| bot_kind.rs | 5 |
| bot_kind/tuning.rs | 3 |
| **Total** | **159** |

All 159 are tier 1, KEEP.

The statistical-looking ones (cadence intervals vary, mood waves, delays vary by seed, seeded looks) run over fixed seeded generators and are deterministic.

The one exception is `planning.rs:670 profile_guidance_against_the_same_unguided_search`. It is `#[ignore]` and has no timing thresholds; T, KEEP.

---

## Counts

These cover integration tests that spawn bots (plus the soccer and gauntlet tooling). Unit tests are counted separately.

| Tier | Count |
|---|---|
| 1 (mechanism) | ~113 integration + 159 unit |
| 1s (single-scenario capability, generous) | ~60 |
| 2 (never-invariant) | 8. All run one seed: brain 663 and 706, extras 389, 591 and 764, routes 471 and 499, firefight. |
| 3 (band over several seeds) | 1 (soccer sword 1473) |
| 4 (fragile) | 31: soccer 1225, 1295, 1328 (match half) and 1499; gauntlet 208, 233, 249, 271, 341, 375, 412, 455, 506, 553, 583, 631, 789, all_dials_on, bot_think_time_16 and fair_hit_rate; nav 347; extras 213, 294 and 312; tactics 1043 and 1080; slayer 4540; plus the partly fragile 1s tests brain 996, routes 143 and 892, tactics 580, interactions 1369 and soccer_teams 672 |
| T (tools and diagnostics) | 9: off_switches, dial_sweep, fair_by_dial, battle_perf, combined_perf, watch, planning profile, and others |

| Verdict | Count |
|---|---|
| KEEP | about 330, which includes every unit test |
| CONVERT TO ABSOLUTE | 33 |
| DELETE | 6: nav 347, extras 294, extras 312, tactics 1080, and as gates (move to `[[skip]]`) bot_think_time_16, battle_perf and bot_watch |

## Findings worth acting on

- **The gate runs `#[ignore]` tests through `--include-ignored`.** So `bot_think_time_16` (a wall-clock limit, 15 ms in debug) and `all_dials_on` (single-seed bands) both gate.
  - `bot_watch::watch_bots_play_a_real_save` is in neither `[[skip]]` nor the known failures. It appears to fail without `BRI_WATCH_SAVE` set; please verify.
  - `profile_sixteen_bots_in_the_real_bedroom` is a profile "never thresholded" by its own header, yet it sits in the known failures. It should be `[[skip]]`.
- **The gauntlet's `finish()` enforces `behaviour_bands.json` on every single-seed scenario run.** The fix is to make `shares::check` report-only, or enforce it only in a multi-seed run.
- **The soccer thresholds restored in 413e75c are tight.** They are judged over only 3 seeds × 3 kits, and the per-match kickoff and untouched limits are 6 s. These are exactly the "flips on unrelated changes" shape.
- **Seeding exists only in the gauntlet and soccer harnesses.** Converting other scene tests to several seeds needs the same trick the gauntlet uses: join N dummy players first, so bot ids and streams shift.

# Appendix E: fair and fast, detail

Read-only review at 41287b0 (tip of claude/v0.2.5-s4s5bn). Nothing was built or run.

## 1. FAIRNESS

### What exists

**`fair_hit_rate`** (crates/chaos/tests/bot_gauntlet.rs:1424-1446). This is the only real hit-rate test.
- One bot fights a scripted target that strafes and hops. Strafe legs last 0.4-1.2 s and a hop comes every 1.5-3 s (`fair_run`, bot_gauntlet.rs:1151-1338). The run covers 5 weapon classes (gun, rocket, shotgun, bow, bouncer; :1341-1352) at ranges 10 and 25, for 25 s each (crates/chaos/tests/data/bot_tuning.json `fair`).
- Each run is split into phases: first (<2 s after first sight), middle, and steady (>4 s).
- It asserts the **steady** hit rate is inside the band **15%-60%** for all classes together and for gun and bow on their own (:1433-1445). Rocket, shotgun and bouncer are only reported (`TOO GOOD`/`HOPELESS` flags in the table, :1360-1365). The first and middle phases are never asserted.
- Invented content (`Arena::new`), so it runs in GitHub Actions and in the gate. It is not `#[ignore]` and not in tools/gate-known-failures.toml, so **it gates**.

**`fair_by_dial`** (bot_gauntlet.rs:1448-1491).
- Sets `perception.strength` to 0.5x, 1x and 2x and checks that the steady rate moves monotonically the dial's way.
- It is `#[ignore]` and also listed under `[[skip]]` (tools/gate-known-failures.toml:31-33), so **it never runs in the gate**. It is a report/tuning tool only.

**Reaction and alertness**
- Unit test `a_goofing_bot_reacts_slower_and_errs_wider_than_one_in_combat` (crates/sim/src/session/bots/perception.rs:960-987). It loops over 49 seeds and checks that Relaxed is slower than Combat, that Relaxed starts with a wider aim, that a stimulus from behind is slower again, and that the aim narrows after `SETTLE_SECONDS`.
- Integration test `a_relaxed_bot_returns_fire_after_its_longer_reaction` (crates/chaos/tests/bot_perception.rs:298-312). It checks a relaxed delay of 126-234 ticks, and that time to first wound is at least the plain time plus 60 ticks.
- These tests measure **reaction delay and first-aim width**. They do not measure hit rate by alertness.

### Gaps

- **Not a "beatable" test.** The target never fights back. A package makes `on_damage` return 0 for non-bot victims (bot_gauntlet.rs:1162-1171), so the target cannot die and nothing checks whether a human-like player can win, survive or kill the bot. No test in crates/chaos/tests/bot_*.rs has a scripted player defeating bots. A grep for survive, outlast, win and beat found only unrelated hits.
- **Single seed.**
  - The strafe RNG is `0x9E37.. ^ seed()` (bot_gauntlet.rs:1217). `seed()` is a thread-local that defaults to 0 (gauntlet/tuning.rs:23, :55-57), and bots draw from deterministic seeded generators.
  - So the gate checks exactly one trajectory per (weapon, range): 10 runs. The multi-seed path is `BRI_TUNING_SEEDS` (tuning.rs:342), and only the skipped tuning tools use it.
  - Two runs exist at 2 ranges, but neither the hop/strafe pattern nor the bot's seed varies.
- **Alertness is only partly differentiated.** `scale()` gives `Combat => 1.0` and `Ordinary => 1.0` (perception.rs:284-292). Combat and ordinary are therefore identical, and only Relaxed (and "from behind") differ.
- **Steady-state hit rate does not depend on alertness at all.** It comes from the seeded aim drift `aim_error` (bots.rs:801-809) and `perception::steady_error` (perception.rs:570). The alertness scale only widens the first aim until `SETTLE_SECONDS`. That is why `fair_hit_rate` asserts only the steady phase, and why nothing gates the phase that depends on alertness and reaction.
- **No test relates hit rate to `reaction_seconds` or `aim_error_degrees`.** bot_knowledge.rs:42-43 and bot_perception.rs:31 set both to 0 to make other tests deterministic. `fair_by_dial` moves `perception.strength`, not those two fields, and does not gate.

### Coverage of today's commits

- **Act stage (a53f888/ba35f39, 41287b0): yes, indirectly.** The bot's strafe, stance, safety and hop now go through `act::resolve` and `bot_safe_walk` (bots.rs:2727-2985, act.rs:100-191). Those decide whether its own movement spoils its aim. One seed, one flat arena.
- **Threat scoring (6687175 `bot_menace`): not covered.**
  - `fair_run` is a free-for-all with no objective, and `bot_menace` returns 1.0 immediately when there is no play (contest.rs:44-46). With only one enemy, it does not choose between enemies either.
  - The only new coverage is `sword_soccer_plays_the_ball_and_fights_for_it` in bot_soccer_match.rs. It runs 2 seeds and asserts goals ≥ 2, combat > 0, combat below 50% of bot time, and ball time above 25%. It tests behaviour, not fairness.
- **Explore (2d45224): not covered.** The bot spawns facing the target at 10 or 25 units and keeps it in sight or memory, so `explore_to` is never computed. No test anywhere references explore (a grep of crates/chaos/tests for explore finds nothing). Its only test is the unit test `a_place_not_seen_lately_or_near_a_lead_is_worth_more` (explore.rs:163-179).

## 2. PERFORMANCE

### Budgets in the code

| Budget | Where | Scope |
|---|---|---|
| Sight rays: `ORDINARY_RAYS = 128` per tick, plus `TARGET_RAYS = 4` per bot (sized for `MOST_BOTS = 32`); `SIGHT_RAYS = 256`; 6-tick answer cache | sightlines.rs:21-36, `spend` :112-124 | **Shared by all bots.** Covers only queries routed through `bot_sees`/`bot_sees_player` |
| Nav sampling: `SAMPLES_PER_TICK = 96`, `EXPANSIONS_PER_TICK = 384`, `MAX_EXPANSIONS = 6000` per search; `SEARCH_BOUND = 72` | nav.rs:47-51, :449-452; bots.rs:161 | **Per body shape** (`bots.navs` keyed by `Body`, bots.rs:1470-1478). All Blockhead-bodied bots share one; the bot order is rotated each tick (bots.rs:985-990) |
| Combat solver: `SOLVES_PER_TICK = 2048`, `RAYS_PER_TICK = 544`, round-robin turn | combat.rs:9-10, :41-56 | **Shared** |
| Objective grounding: `GroundingBudget` (≤128 targets, 4096 terms, 64 KiB per discovery) | objectives.rs:515-538 | Per search, but only **one bot per tick** gets an objective turn (`objective_budget_used`, objectives/turn.rs:165-177) |
| Stimuli and noises capped at `MAX_STIMULI = 32` per tick | perception.rs:53, :229-239 | Shared |

**Not budgeted.** Every direct `world_ray` / `floor_below` / `hop_lands` call in bot code bypasses the sight budget. In the per-bot step:
- `bot_fall_ahead`: up to 6 rays (interactions.rs:949-952).
- Ranged strafe `ground()`: 2 rays per call, called several times; plus 2 overshoot rays and 1 lean ray (bots.rs:2797-2862).
- Team exit: 1 ray (bots.rs:2938-2943).
- `team::exit` floor checks (bots.rs:2128-2131).
- Hop: 1 ray (act.rs:189).
- Explore: 24 rays (below).
- Also the surprise, interactions and contest rays.

No counter or cap covers these.

### Tests

**`bot_think_time_16`** (bot_gauntlet.rs:1058-1101). This is the only test with a per-tick bar.
- 16 bots (`MAX_BOTS`, asserted) in an 8v8 mixed-arsenal deathmatch on the flat invented arena, with every dial on.
- 5 s of warm-up, then it times 30 s of `Session::bot_think_nanos`. That counter is wall time inside `step_bots` only (session.rs:2627-2629). The whole step time is printed, not asserted.
- Bar: 15000 µs/tick in debug and 2000 µs in release (bot_tuning.json `perf`). The note says debug measured 7528 µs, and the release bar "not measured here".
- It is `#[ignore = "timing; run in release"]`, but the gate runs `--include-ignored` (tools/gate.py:19, :625) in the **debug** profile, without `--release` (gate.py:593, :770). It is not in `[[skip]]`, so **the gate enforces the 15 ms debug bar**.
- It is wall-clock timing on a machine running 8 test binaries in parallel (gate.py:59). That is flake-prone, though the gate retries a failure alone and treats a pass on retry as flaky.
- Coverage of today's work is thin. In an open-field deathmatch, enemies are always seen, so explore never fires and `bot_menace` takes its no-objective early return.

**`profile_sixteen_bots_in_the_real_bedroom`** (crates/chaos/tests/bot_battle_perf.rs:668-871).
- **Diagnostic only, no timing threshold.** The file header says "timings are reported, never thresholded" (:3), and `profile()` only records them (:196-419).
- It measures the full `Session::step` per tick, including step_nanos, slow ticks above 5 ms and 50 ms, RSS, snapshot and encode time, and activity per 10 s window. It runs on the real Bedroom (plus Beta City 16 and ACM City if they are in the content), with 16 bots (Blockhead against converting Zombie) through gun, rocket, gravity-gun and spear phases of 960 ticks each.
- It needs generated v20 content.
- Its only failure conditions are `ensure!` setup checks: the bot count equals 16 (:552-553), the kinds "Blockhead Bot"/"Zombie" exist (:745-754), the CTF provider is loaded, and the `BRI_BATTLE_REQUIRE_ACTIVE=1` activity check, which is off by default (:378-404).
- It **is a known failure** (gate-known-failures.toml:68-71, "Expected old bot behaviour after the v0.2.4 bot merge"). docs/progress/2026-10-05-v025-bot-cleanup.md:136 says "likely delete as bogus timing".
- Because no timing assert exists, its failure must be one of those setup/`ensure!` checks or an error/panic, not a timing flake. The exact cause can't be established without running it.

**`bot_combined_perf.rs`** reports timing without thresholds ("No timing assertions", :2-3) and is `#[ignore]` (:339). It still runs under `--include-ignored`, but only its `ensure!` checks can fail.

**Budget unit tests**
- `rays_stay_under_budget_with_32_bots_and_targets_are_always_checked` (sightlines.rs:282-307) tests the ledger alone.
- `searches_spend_at_most_the_tick_budget` (nav.rs:1485-1503).
- `shared_budget_rotates_colliding_owner_residues_without_starvation` (combat.rs:1325).
- Nothing counts the real total rays per tick across a 16-bot session.

### Verdict

- **Yes:** there is a gating 16-bot test with a per-tick bar (`bot_think_time_16`). In the gate it measures debug, with a 2x margin over one measurement on a 4-core container. It is timing-based and can flake under parallel load.
- **Not checked:** the release bar of 2000 µs, which the shipped game cares about, is checked by nothing that runs automatically.
- The sight budget is shared across all bots. It covers sight queries only; raw geometry rays (movement safety, strafe, explore) are not budgeted.
- The Bedroom profile does not gate, and it fails on something other than timing.

## 3. SCALING of the new code

### `bot_explore_target` (crates/sim/src/session/bots/explore.rs:112-156), called per bot per tick from bots.rs:1928-1937

```rust
if let Some(to) = brain.explore.to { return Some(to); }          // :122 cached: O(1)
for i in 0..DIRECTIONS {                                          // :128 12 directions
    let open = world_ray(&self.simulation, eye, way, reach)       // :131 ray up to 0.6*sight = 48 u
    ...
    let Some(down) = floor_below(&self.simulation, at, body)      // :137 a second ray
    ...
    let crowd = intents.iter()....any(..)                          // :143 O(allies)
```

- **Per recompute:** up to 24 unbudgeted rays (12 of them 48 units long) and 12 × |allies| ≤ 180 distance checks. This is O(bots) per bot, so O(bots²) ≈ 2,900 cheap checks per tick if all 16 bots recompute at once. The checks don't matter; the rays do.
- **When it recomputes:** only while `explore.to` is `None`. The cached path is cheap. It is reset on arrival (`note`, :76-78), when an Explore route settles (bots.rs:2378-2383), and after `MAX_REPLANS` (bots.rs:3043-3044).
- **Hot case: no direction qualifies.** If every one of the 12 directions fails, `best` is `None` and `to` stays `None` (:153-154). The bot then recasts all 24 rays **every tick** for as long as the preconditions hold. Directions fail when:
  - the wall is closer than 8 units (`open < CELL*0.5`, :133), or
  - the probe point has no floor within `0.5+step+drop` of the bot's own height (:137). That is plausible on a roof, a table top in the Bedroom, a bridge, or rolling terrain where ground 48 units away is higher or lower than step/drop.
- There is no back-off, no "tried at tick" cooldown and no failure memo.
- **Worst case at 16 bots on a big terrain or rooftop map:** 16 × 24 = **384 extra rays/tick**, more than the entire shared sight budget (256). At an assumed ~2-5 µs per long BVH-plus-terrain ray in release, that is roughly 0.8-2 ms/tick, about the whole 2000 µs release bar. Debug is several times more. **No existing budget bounds it.**
- **The rays are spent even when Explore won't be chosen.**
  - `explore_to` is computed before the chooser (bots.rs:1928-1937, then passed to `team_choices` at :2133). The gate does not check the kind's `explore` weight.
  - `bot_enemies_about` (explore.rs:102-106) is O(peers) per bot per tick in the worst case. `.any` stops at the first live enemy, so the worst case is when it finds none. That is ≤ 16×(players) `bot_enemy` calls, cheap, unbudgeted, and only reached after the cheaper preconditions pass.
- **Path plans:** each new explore point starts a new `Search`. It goes through the per-body shared Nav budget (96 samples / 384 expansions per tick, 6000 per search), so planning is bounded. Two things are not bounded by Nav:
  - The `bodies`/`motions` `Vec`s are rebuilt over all peers on every tick a search is stepping (bots.rs:1453-1468). That is O(bots × players) with allocations. It existed before today, but explore adds more searches.
  - A point that settles at once (route ends short) clears `to` and recomputes the next tick (bots.rs:2380-2383). The worst case is 24 rays plus a new Search every couple of ticks per bot.
- **Memory and other work is bounded:**
  - `visited` ≤ 256 cells per bot (:17). Eviction is an O(256) `min_by_key` (:59-63), done only when entering a new 16-unit cell with the map full, so it is rare.
  - `note` does a `BTreeMap` insert every tick per bot (:67). That is O(log 256) with no allocation for an existing key.
  - `leads` ≤ 4 entries (:22, :71-74), and `worth` is O(4).

### `bot_menace` (crates/sim/src/session/bots/contest.rs:37-73), called once per bot per tick (bots.rs ~1834)

- O(1). It returns early without play or a seen enemy (:44-46). Otherwise it does one `object_centre` lookup, one weapons actor lookup, `inventory.get(s).cloned()` (a `String` allocation per bot per tick at play, :62), and `item_attacks` (pack hash lookups, combat.rs:271-282).
- It considers only the one enemy already chosen by sight (`sight.target`), so it is not per-enemy and not O(bots²).
- Its effect on scaling is behavioural. It turns more enemies into Fight/Chase options (the commit reports CTF kills going from 14 to 46). More fighting means more combat-budget and strafe-ray use, and the combat solver is budgeted while the strafe rays are not.

### Act stage (crates/sim/src/session/bots/act.rs; caller bots.rs:2727-2985)

- `proposals: Vec<act::Proposal> = Vec::new()` (bots.rs:2727) allocates per bot per tick on the first push: 16 small allocations per tick. A `SmallVec` or a `[Option<Proposal>; 9]` would remove them. `resolve` sorts ≤ 9 entries (act.rs:101).
- `bot_safe_walk` (act.rs:146-191):
  - a portal query when off-route (:158-164);
  - `bot_vehicle_detour`, which loops over all observed vehicles (interactions.rs:1047), so O(bots × vehicles);
  - `bot_fall_ahead`, up to 6 rays (interactions.rs:949-952);
  - `hop_lands`, 1 ray, only when a hop is pressed (act.rs:189).
- All of this moved from the old inline code; the conditions are identical (`git show ba35f39`, old lines 229-263). **There is no new asymptotic cost.** The 41287b0 change only uses the walk taken for the hop's drift.
- The stance's ranged strafe (bots.rs:2797-2862) can reach about 10 rays per fighting bot per tick (2 per `ground` call, called for `side`, `-side` and inside `strafe_leg`, plus overshoot and lean). That existed before, is unbudgeted, and runs for 16 fighting bots, so ~100-160 rays/tick in an all-ranged fight.

### Hearing (crates/sim/src/session/bots/hearing.rs)

- `hear_fighting` (:110-140) is O(noises × bots) with a `Vec` collected per noise. Noises are capped at 32 (perception.rs:229-233), so ≤ 512 `listener`/`bot_enemy` checks per tick. Bounded.
- `hear_alerts` (:68-103) is O(alerts × bots). Alerts are at most about one per bot per tick, rate-limited by `cadence::beat(.., ALERT, .., 30)` or new sight (bots.rs:1992). That gives ≤ 16 × 16 = 256 checks: O(bots²) but small.
- No rays.

### Also seen, not new

- `let kind = brain.kind.clone();` clones the full `BotKind` per bot per tick (bots.rs:1952, from cab1f87).
- `team_intents` collects a `Vec` per bot per tick (team.rs:300-313): O(bots²) intent filtering and 16 allocations per tick.

### Bottom line on scaling

- The only new unbounded per-tick cost is **explore's 24-ray rescan with no back-off when no direction qualifies**. It sits outside every budget and is not exercised by any perf test, since `bot_think_time_16` is an always-in-sight open field.
- Suggested bounds:
  - cache a failed scan for N ticks (`explore.tried_at`), or
  - charge the probes to `Sightlines` as ordinary rays, or
  - stagger the directions across ticks (for example 3 per tick);
  - and skip the scan when the kind's `explore` weight is 0.
- `bot_menace` and the act stage add only constant, small allocations.

---

# Addendum (21:10Z): the removal list, rechecked against "keep the behaviour, fix the wiring"

Max's rule (21:04Z): keep anything a player can see, unless keeping it is a lot of work, and rebuild it on the shared mechanisms. These are the chooser's scored options, the one shared push and hold, capabilities, and no timers of its own. Everything below was checked at 41287b0.

| # | Item | Would a player notice? | Kept by folding into | Size | Verdict |
|---|---|---|---|---|---|
| 1 | Clear and cover (the swap after the choice) | Yes for clear: the bot shoves the opponent off the ball. Barely for cover: the bot stands behind a teammate. | Clear becomes Fight on the opponent at the play, which menace already scores at 1.0, once the broom's shove counts as an attack. Cover becomes the Objective option with the team overlap term. The "I got him, go!" line becomes the callout on that Fight choice. | Medium | **Keep the behaviour, remove the swap** |
| 2 | Objective stopgaps (`pausing_delivery`, `peaceful_objective`, `objective_without_attack`) | No | The scores, after the "can hurt it" fix | Small | **Remove**, after the "can hurt it" fix |
| 3 | The strafe's own air guard (`overshoots`) | No | The act stage's edge check, which already covers steering in the air off the route | Small | **Remove** |
| 4 | Knobs only tests can set, `BotHold.margin`, `spawner_name` | No | Constants | Small | **Remove** |
| 5 | Duplicate machinery (two weapon readers, two give-up lists, two progress judges, two `RETRY`s, `reflex_rng`, repeated budget numbers) | No | The twin that stays. Merging the weapon readers also fixes C1 (unknown weapons guessed as melee). | Small to medium | **Remove** (merge) |
| 6a | Goof: flashlight flick | Yes, now and then | It's already an ordinary command (`surprise.rs:1343`). It becomes one more scored goof when goofs move into the chooser. | Small | **Keep** |
| 6b | Goof: drop a weapon | Yes | Same. **Correction:** it only drops a *spare* weapon (`surprise.rs:705`, `spare_weapon` at `:1233`), so my earlier "can strand a bot unarmed" was wrong. | Small | **Keep** |
| 6c | Goof: aimless detour | Hardly. A few steps off look like a stroll or an explore. | The Explore/Wander goal it already sets (`surprise.rs:1351-1358`); the goof just picks that goal. | Small | **Keep as a scored goof that sets an explore goal**, with no separate walk code. Remove the roll-a-point code. |
| 6d | Goof: walk to a door and click it | Yes, and it's charming | It already uses the shared door-click machinery (`extras.rs:620-632`). Only its fixed `FLAVOUR_CLICK_TICKS` hold is its own; that becomes "until the click lands", like a normal door click. | Small | **Keep** |
| 7 | Tests that pin old behaviour (the navigation spike limitation, the 16-bot profile as a gate, `bedroom_mixed_firefight`) | No | Not applicable. This is Max's "tests as sanity" bar. | Small | **Remove** (or convert to an absolute) |
| 8 | Idle ball-kicking outside a game | Yes, and it's fun | It's already a scored option (Interact, `behaviour.rs:149`). Only its executor is a second push (`interactions.rs:503-837`, `push_contact`/`push_anchor`). Run it through the objective's shared Push in `physical_objectives.rs`, minus the goal-side checks, which don't apply outside a game. | Medium | **Keep, on the shared push** |
| 9 | Gravity Gun grab and throw | Yes | Already decided: the shared hold, a scored option with no 6 s timer, and `open_spot` on the ray budget | Medium | **Keep** (decided 21:0xZ) |

**Net result:** nothing a player can see goes away. What goes is the second copy of each mechanism: the after-choice swap, the second push, the second weapon reader, the goof's own walk and timer, and the stopgaps. My earlier list put items 6 and 8 under "remove". That was wrong under this rule.

## Other decisions from today that run against Max's rules

| Decision | Rule it breaks | What fits |
|---|---|---|
| Soccer limits restored to tight values (413e75c): stuck 5 %, idle 3 %, kickoff 6 s, jitter 3 | Tests as sanity only | Absolutes: at least one goal, the ball never lost, never stuck all match. Shares go to a report. |
| Gauntlet `within` limits "just past today's play", plus `behaviour_bands.json` gating every single-seed scenario | Tests as sanity only | The same: absolutes gate, bands report |
| Water gauntlet stuck bound loosened to 35 % | Tests as sanity only. It's a share that was loosened to pass, so it's neither sanity nor a real bar. | "Never stuck the whole run" |
| Two ways to go after an opponent at the ball: Fight through menace (6687175) and the clear swap (8537730, 402602d) | One mechanism per job | Fight only (item 1) |
| The objective is held two ways: the hold rule, plus `pursuing` with the fixed 0.92/0.65 committed scores (O9/O10) | One mechanism per job | The hold rule alone. Commitment becomes the hold, not a higher score. Medium. Not urgent, but it's the same kind of duplication. |
| Dodge, door clicks and hand-offs run on private timers in `extras.rs`, outside the chooser | One mechanism per job | Already on the bot thread's list. Keep it there. |
| The stall hop is written after `resolve` (`bots.rs:3018`) | One writer for the controls (the act stage's own rule) | Make it a `Press` (tiny) |
| Explore's scan runs before the choice, outside the ray budget, with no back-off | Foundation first; one budget for rays | Remember a failed scan, and charge the rays to the sight budget |
| Fight and Chase score on presence, not on "can hurt it" | Foundation first | One factor alongside `spared` (section 1) |
| My own removal list (items 6 and 8) | Keep behaviours | Corrected above |

Decisions from today that fit the rules:
- the act stage;
- menace scoring;
- the header jump, which is general;
- the heard fight kept for the walk;
- the fall veto last;
- explore as a scored option;
- the `native_hammer` check left for the bot thread to make general or justify.
