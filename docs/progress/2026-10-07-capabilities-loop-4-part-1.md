# 2026-10-07 Gameplay capabilities loop 4, part 1: copies and a stock-item bug

Branch `claude/v0.2.6-capabilities-8gived`. Plan: `docs/audits/gameplay-capabilities.md`.
This covers the loop 4 items that need no content change.

- **Bug fix:** an Add-On's `set_brick_item` refused every base-game item. The
  check looked for `v20/` while base-game items are spelled `v20.weapon.*`.
  `rules_may_hand_out_base_game_items_but_not_a_strangers` (in
  `sim/tests/package_camera_path.rs`) passes now and fails with the old check
  put back.
- **One print rule:** `bri_content::brick_materials::print_fits` and
  `UNIVERSAL_PRINT_ASPECT` replace the three hand copies of "the brick's class
  or Letters" (session tools, client tool UI, `Print::compatible`).
- **One admin-tool rule:** `Session::holds_admin_tool` (the held image runs
  `HostTool::AdminDestroy`) replaces the two admin-wand image id checks for
  impact damage and brick touches.
- **Rule Workshop soccer** finds a spawnable `Family::Ball` vehicle instead of
  a vehicle whose name contains "steel" and "ball". Its own "ball spawner"
  program labels are the lab's names, not content, and stay.
- **Dead cues:** `CueKind::HammerHit` / `WrenchHit` were never sent and are
  removed. The wire change is recorded in
  `crates/net/protocol-changes/unsent-tool-hit-cues.md`.
- **Deferred:** the two default-loadout copies (`minigames` and `ui`) need a
  new crate dependency, and root owns manifests. Raised with the coordinator.

## Evidence

- `cargo clippy --workspace --all-targets -- -D warnings`: clean, client
  included (the cloud container got ALSA and udev headers for this).
- `cargo test -p bri-sim -p bri-content -p bri-net -p bri-weapons --no-fail-fast`:
  everything passes except the known `a_bot_jets_over_to_someone_above_it`.
- `cargo test -p bri-client --lib`: 475 passed. The 6 failures all fail while
  creating the headless GPU (`bri_ui::gpu::Headless::new`), which the cloud
  container lacks. None of them touch the changed code.
