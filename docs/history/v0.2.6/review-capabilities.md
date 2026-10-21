> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-capabilities.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: gameplay capabilities (PR #29), head 8d4365a, base main

This was an independent, read-only review. Nothing was built or run. The branch is current with main. One reader went through the code, and I re-checked the main claims myself: the `HostTool` table, the soccer ball choice and the remaining name constants.

## Verdict: passes. No blockers.

The claimed changes match the code. For vanilla items, behaviour is unchanged apart from two things:
- the stated `set_brick_item` fix;
- one documented change to what bots notice (a human firing an FX can now counts as a spray, not a tool).

## Checked

**Name checks are gone from sim and bots where the plan said.**
- `HostTool` is decided once, by `bri_weapons::host_tool` reading the `HOST_TOOL_IMAGES` table in `crates/weapons/src/runtime/stock.rs:16`. That is checked.
- These are removed:
  - `NativeTool` and `native_tool_callback`;
  - the `"hammerImage"` test in `native_hammer`;
  - `CORE_TOOLS` in bots;
  - `SPRAY_CAN_IMAGE` in bots;
  - the admin-wand id checks in combat and events;
  - the soccer "steel"+"ball" name match.

**What remains is deferred by the plan, each with a reason.**
- The client's `Equipment` map: loop 3.
- The v20 slash-command can and wand constants in `tools.rs:262-278`: the v20 command contract.
- `paint_edit` by projectile name, which `image_paints` is built on: loop 4 part 2.
- Tool sound and explosion names, `brickdeployprojectile`, the pumpkin, and the special-brick ids.

**One rule each.**
- Print: `print_fits` in `bri_content::brick_materials`.
- Admin tool: `Session::holds_admin_tool`.
- None of the removed helpers is still used.

**Vanilla items map the same.**
- Hammer, wand, admin wand, wrench and printer go to Break, Destroy, AdminDestroy, Inspect and Print, the same as the old table.
- The wrench and printer dialogs now ask whether the image runs Inspect or Print. That gives the same result for v20.
- The bots' building-tool set equals the old `CORE_TOOLS`.

**The `set_brick_item` fix is correct.**
- Item ids are spelled `v20.weapon.*`, while the old check wanted `v20/`.
- The test `rules_may_hand_out_base_game_items_but_not_a_strangers` (`crates/sim/tests/package_camera_path.rs:242`) fails under the old check, so it doesn't pass by construction.

**CueKind.**
- `HammerHit` and `WrenchHit` have no remaining construction or match anywhere.
- `crates/net/protocol-changes/unsent-tool-hit-cues.md` is well formed, and VERSION is untouched.
- `Event::ToolFire`'s new field isn't on the wire.

## The skipped default-loadout fold: the reason only partly holds

- **The plan's reason is wrong.** It says `bri-package` is the only crate both depend on, but `bri-ui` uses it only as a dev-dependency, so the two crates share none.
- **The loop-4 note's reason is true but narrow.** It says a shared constant needs a new crate dependency. That holds only for sharing a constant.
- **A fold needs no manifest change.**
  - `crates/client` depends on both crates, and already converts between them (`minigame_ui::rules(&Settings)`, `crates/client/src/minigame_ui.rs:68`).
  - The UI default is only reached through `rules_draft().unwrap_or_default()` (`crates/ui/src/models/minigames.rs:65-70`).
  - The client could seed the UI with `rules(&bri_minigames::Settings::default())`, which removes the copy at `crates/ui/src/api.rs:1236-1243`.
- **Not urgent.** This goes to v0.2.7, and the plan's stated reason should be corrected.

## v0.2.7 notes

- **Behaviour change in Rule Workshop soccer** (`crates/sim/src/session/rules.rs:1050-1059`, checked). The ball is now the first spawnable `Family::Ball` definition.
  - With only the Steel Ball installed, nothing changes.
  - With several ball Add-Ons, it takes whichever comes first in definition order.
  - `&& d.family.spawnable()` is redundant.
  - The test helpers that rename the test ball to "Steel Ball" (`rules.rs:1730, 2193, 2304`) are now pointless.
- **The client's `Equipment` map** is still a second place that decides which item is which host tool (`crates/client/src/building.rs:926-929`). The server accepts any image that runs Inspect or Print, while the client only knows the four v20 items. That doesn't matter for vanilla. It's loop 3.
- **One fact is stored twice.** `StockFire::HostTool` and `Stock::host_tool` (`stock.rs:113-117`) should be one field.
- **The stock table test is thin.** It checks only wrench and rocket. A five-row table test is cheap.
- **The `set_brick_item` test stops short.** It shows the error is gone, not that the item lands on a brick, because the test world has no bricks or weapons pack.
- **Leftovers from the CueKind removal:**
  - the comment in `crates/client/tests/v20_fidelity.rs:56-57`;
  - two unused trigger keys in `crates/audio-import/src/convert.rs:2060-2061`.
- **A bare default.** `item_worth` uses `.unwrap_or(3.0)` (`bots/combat.rs:~361`), the same value as `MELEE_DEFAULT_REACH`. Use the constant.
- **Stale plan text** in `docs/audits/gameplay-capabilities.md`:
  - "Nothing here is built yet";
  - the §1 table still lists finished work as open;
  - §5 says packs change schema in loops 2-4, but loop 2 changed none;
  - "CORE_TOOLS stays only as default-loadout data", though `building_tool` and `contains_item` still use it.
