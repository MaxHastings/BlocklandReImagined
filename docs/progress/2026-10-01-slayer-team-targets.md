# 2026-10-01 Slayer team targets: Add-On wrench event targets

Branch `claude/project-thread-t8k5dx` (Slayer/CTF).

## Engine seam (generic, `brick_events` capability)

- `behaviour.json` `brick_targets` (`registerEventTarget`): up to 8 targets,
  each a name, a class of the Add-On's own and the slot it is found from
  (`Self` or one of the input target slots). `Catalog::with_targets` lists
  each on every input that has its base slot, the engine's and Add-Ons'
  inputs alike, as v20 adds them to every registered input.
- Rows aim at one with `Target::Derived(name)`. `Catalog::row_output` is now
  the one place a row's target turns into its class and output (validation,
  install, checkpoint restore, the wrench dialog), so a derived target runs
  the Add-On's outputs of its class on the base slot's entity. The host
  passes `Dispatch::derived` on to `on_brick_output` as `info.target`, with
  `info.class` the target's class and `info.base` the entity's.
- What Add-Ons add to the catalog travels as one `bri_events::Extension`
  (inputs, targets, outputs): `Catalog::extended`, `Session::package_brick_events`,
  `Checkpoint::brick_events` (protocol file `rules-brick-targets.md`), the
  client's replica and wrench.
- `bottom_print(p, text, seconds, hide_bar)`: the rules can hide the bottom
  print's bar, as `GameConnection::bottomPrint`'s third argument does.
- Input target lists may hold 12 entries (was 8) to leave room for Add-On
  targets.

## Port

- Slayer: `Team(Client)` (whoever set the row off's team, `%client.slyrTeam`)
  and `Team(Brick)` (every team of the colour holding the brick,
  `getTeamControlList`), with `ChatMsgAll`, `CenterPrintAll`,
  `BottomPrintAll` (`%1` is the player's name), `RespawnAll` and `IncScore`
  (the team's own points, counted in its score and the points to win, and
  cleared on reset as `onMinigameReset` does by default; our mini-games
  always clear scores on reset).
- Checked against the real copy: every pin matches, the port applies, and the
  outputs carry the original's parameters.

## Not yet

- Slayer's Restrict Output Events (who may add a restricted output row).
- `onPlayerTouch(TeamN)`, `onActivate(TeamN)` and the `onMinigame` inputs.
- Old saves' rows that use Add-On inputs, outputs or targets stay preserved
  rows: the converter binds against the base catalog only.

## Tests

- `crates/sim/tests/package_brick_outputs.rs`
  `an_add_ons_targets_run_its_outputs_on_their_base`.
- `crates/addon-import/tests/slayer.rs`
  `team_events_message_respawn_and_score_a_whole_team`.
- `crates/client/src/tool_ui.rs`
  `the_wrench_lists_the_servers_add_on_inputs_targets_and_outputs`.
- `crates/package-runtime/src/content.rs`
  `a_brick_target_is_the_add_ons_own_and_found_from_a_slot`.
