# 2026-10-11 Add-On porting workflow (Rallypack and P90 lane)

Branch `claude/v0210-addon-workflow-mvg2uy`, from the v0.2.9 merge. Covers the
items the v0.2.9 ask audit marks not done or partly done.

## What changed

1. **One command** `tools/port_addon.py build` (import with a registered
   recipe, optional `--only` closure, verify with `bri-addon-check`,
   deterministic ZIP, optional install, `BUILD-RECEIPT.json`). Tests:
   `tools/test_port_addon.py`, run on a synthetic Add-On.
2. **Dependency closure** `tools/addon_closure.py`: keeps selected items and
   what their definitions, projectiles, particles, sounds and textures name;
   reports what is missing. Tests: `tools/test_addon_closure.py`.
3. **Timing diagnostics** `crates/package-runtime/src/timing.rs`, called from
   `bri-addon-check`: zero-time cycles over the transition budget, idle
   command loops, same-step callback timing, and state-size limits.
   Test: `timing_and_state_limits_are_named_before_the_game_runs`.
4. **Bot ammo for script weapons** `Image.bot.ammo` (`BotAmmo`): bots read the
   rounds a package keeps in player state, skip an empty rule-fired gun for
   another weapon and never fire it; spare rounds keep it in play. Wire
   change: `crates/net/protocol-changes/script-weapon-bot-ammo.md`. Tests:
   `crates/chaos/tests/bot_tactics.rs` (three rule-fired-gun tests),
   `crates/weapons` validation, `tools/test_rallypack_bot_use.py`. The
   Rallypack and P90 recipes now declare their ammo.
5. **ZIP install in the game**: a ported Add-On ZIP dropped in the Add-Ons
   folder is unpacked (path checks, file and size caps, rules companion),
   not imported; the record ties it to its package so it is not redone each
   launch. Tests in `bri-package` and `bri-client` (`add_ons.rs`).
   Add-On health already existed (`add_on_health`); nothing new there.

## Not verified here

No private recipes, ZIPs or original content were available, so the real
Rallypack/P90 builds were not run; everything ran on synthetic Add-Ons. No
window was opened. Still unsupported: bow and cooked-grenade bots, and a
runtime guard against package churn.

## Evidence

`cargo test -p bri-weapons -p bri-sim -p bri-package-runtime -p bri-package`
pass; `bri-client` `add_ons` tests pass. `bot_soccer_teams::a_bot_scores_in_its_own_teams_goal_after_save_and_reset`
fails identically on the branch base (checked by stashing), so it is not from
this work.
