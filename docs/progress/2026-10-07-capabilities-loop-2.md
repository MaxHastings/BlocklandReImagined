# 2026-10-07 Gameplay capabilities loop 2: building tools read once

Branch `claude/v0.2.6-capabilities-8gived`. Plan: `docs/audits/gameplay-capabilities.md`.

## What changed

- New `bri_weapons::HostTool` (Break, Destroy, AdminDestroy, Inspect, Print):
  the building mechanism a hammer, wand, admin wand, wrench or printer image
  runs when it fires. Marked not stable until the modding API freeze.
- Which stock image runs which mechanism is decided in one place, the
  sanctioned name table in `weapons/src/runtime/stock.rs` (`HOST_TOOL_IMAGES`
  moved there from `runtime.rs`, now paired with the mechanism).
  `bri_weapons::host_tool(image)` reads it.
- `Event::ToolFire` carries the mechanism (`tool`). The session's
  `tool_fire` dispatches on it; the second name table in sim
  (`NativeTool` / `native_tool_callback`, keyed by image id) is deleted.
- `native_hammer` asks for `HostTool::Break`; its `"hammerImage"` name check
  is gone (its structural checks stay).
- The wrench and printer dialogs require a held image that runs `Inspect` /
  `Print` instead of the two stock item ids.
- Bots value building tools at 0 through `WeaponsWorld::building_tool`
  instead of comparing against `CORE_TOOLS`. `CORE_TOOLS` stays as the default
  loadout and the engine's fallback tools when a pack has none.

## Plan change

Loop 2 does not change the content packs. Declaring host tools in image data
needs the importer and pack regeneration, the same step loop 3 takes for every
stock behaviour in `stock.rs`, so both move to data together in loop 3. The
client's `Equipment` mapping (item id to Hammer, Wrench, Printer, Wand) moves
in loop 3 too: the client building state has no weapons pack to read today.

## Evidence

- `cargo clippy --workspace --all-targets --exclude bri-client --exclude bri-audio --exclude bri-client-sandbox -- -D warnings`:
  clean (the excluded crates need ALSA, which the cloud container lacks; none
  of them use the changed APIs).
- `cargo test -p bri-weapons`: all pass.
- `cargo test -p bri-sim --no-fail-fast`: all pass except
  `a_bot_jets_over_to_someone_above_it`, already listed in
  `tools/gate-known-failures.toml` and owned by the locomotion thread.
- `cargo test -p bri-chaos` for bot_physical_objectives, bot_objectives,
  bot_watch, bot_extras, bot_interactions, bot_tactics: all pass.
