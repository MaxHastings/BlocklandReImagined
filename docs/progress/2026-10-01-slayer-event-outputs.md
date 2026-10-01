# 2026-10-01 Slayer event outputs: Add-On wrench outputs

Branch `claude/project-thread-t8k5dx` (Slayer/CTF), after main fe162e0e.

## Engine seam (generic, `brick_events` capability)

- `behaviour.json` `brick_outputs` (`registerOutputEvent`): up to 32
  outputs on `fxDTSBrick`, `Player`, `GameConnection` or `MiniGame`, with
  the wrench's parameter kinds. They join the host's event catalog
  (`Catalog::with_outputs`, each `OutputDef` marked with its `package`)
  and players' wrench (`Checkpoint::brick_outputs`, protocol file
  `rules-brick-outputs.md`).
- A row that runs one compiles to `Intent::Package`; the host calls the
  rules' `on_brick_output(output, target, params, info)`.
- The rules may answer with one of their own inputs, optionally with a row
  range: the runtime chains it on the source brick (`Apply::Chain`,
  `Trigger::rows`) in the same event phase, under the relay budgets.
  Slayer's `checkTeam` uses this for `onTeamCheckTrue/False` and its
  `"min max"` row range.
- Event targets `OwnerPlayer` and `OwnerClient` (the brick owner, while on
  the server), for Add-On inputs that list them.
- Inputs the rules fire from inside an output (`fire_brick_input`) run
  after the event phase instead of being lost (`fire_package_input`); the
  engine's own inputs are unchanged.
- Ports read the parameter text with `{{name|event_params}}`, so the
  wrench shows the original's ranges and list names.

## Ports

- Slayer: `setTeamControl` (team spawns spawn the new colour's teams,
  capture points are captured), `setTeamControlLocked` (the capturing bar
  stays empty and shows the original's locked message), `addLives`,
  `addKills`, `addDeaths`, the `set` forms, `joinTeam` with its reason and
  no-respawn flag, `incTimeRemaining` (rounded to the half minute first, as
  the original) and `setTimeRemaining`, `Win` in all six modes,
  `checkTeam`, `checkTeamCount`, `StartFlyThrough` (the mini-game's
  creator or an admin, as its restricted level 3). Slayer counts kills and
  deaths and clears them on reset with Clear Stats on Reset.
- Slayer CTF: `DropFlag` on the player a row aims at.
- Checked against both real copies: every pin matches and the outputs
  come out with the original's parameters.

## Tests

- `crates/sim/tests/package_brick_outputs.rs`: outputs listed and run,
  chained input with a row range and `OwnerClient`, the capability check,
  an input fired from an output.
- `crates/addon-import/tests/slayer.rs`: team checks, a team spawn handed
  to Blue, a locked capture point, extended time and a custom win; the
  `DropFlag` event and `onFlagDropped`.

## Next

Team(Client) and Team(Brick) targets with the team outputs, the (TeamN)
and onMinigame inputs, uniforms, team loadouts and player types, bots,
.pathcam saving, and CTF's Drop Tool key, dropped-flag countdown, flag
light, locked flags and score columns.
