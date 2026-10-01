# 2026-10-01 Slayer's team and mini-game inputs, Restrict Output Events

Branch `claude/project-thread-t8k5dx` (Slayer/CTF).

## Engine seams (generic, `brick_events` capability)

- Inputs that follow one of the engine's: a `brick_inputs` entry with
  `"follows": "onActivate"` (or any engine input). When a non-bot player
  sets that input off on a brick with rows on a follower, the rules'
  `on_brick_input(input, brick, player)` may answer with one follower's
  name, which runs too, set off by the same player. Only packages with a
  follower wired on the brick are asked. A host whose catalog lacks the
  followed input simply never fires it. Answers that are not a follower of
  that input are diagnostics (`brick_events.follows`).
- Inputs fired on a whole mini-game (`processMultiSourceInputEvent`):
  `fire_game_input(game, input[, p[, killer]])` runs a package's own input
  on every brick of the game with rows on it, found from a new index of
  rows by input in `EventWorld` (`listeners`) plus bricks edited this tick.
  `MiniGame` is the game, `p` fills `Player`/`Client`, `killer` the new
  `Player(Killer)`/`Client(Killer)` slots (protocol file
  `rules-killer-targets.md`).
- Reviewing wrench rows (`serverCmdAddEvent` packages): with
  `"on_event_row": true`, `on_event_row(player, brick, row)` sees each row
  a player sends; `false` or a reason leaves it out and the player hears
  why. `Session::review_event_rows` is the step the wrench send runs.
- Add-Ons may add 32 inputs each (was 16); a catalog holds 128 inputs.

## Port

- Slayer: `onPlayerTouch(Team1..6)` and `onActivate(Team1..6)` follow the
  engine's inputs (the player's team's place in the game's list, as
  `%team.getGroup().indexOf(%team) + 1`); `onMinigameDeath`,
  `onMinigameJoin`, `onMinigameLeave`, `onMinigameRoundStart` (when the
  round clock starts, after any countdown) and `onMinigameRoundEnd`.
- Restrict Output Events (setting, Events category): the
  `RestrictedEvent__` levels are read from the copy; -1 and 3 need
  `canEdit` (the game's owner or an admin), 0 to 2 an admin (our players
  have no separate host or super admin flag). The original's default is
  on.
- Checked against the real copy: every pin matches and Slayer and Slayer
  CTF still apply.

## Not yet

- Uniforms, team loadouts and player types, bots, `.pathcam` saving, CTF's
  remaining parts.
- Old saves' rows that use Add-On inputs, outputs or targets stay preserved
  rows.

## Tests

- `crates/sim/tests/package_brick_inputs.rs`
  `an_input_that_follows_the_engines_runs_when_the_rules_answer_it`,
  `a_game_input_runs_on_the_games_bricks_with_the_killer`.
- `crates/sim/tests/tools.rs`
  `an_add_on_may_refuse_rows_a_builder_sends_and_says_why`.
- `crates/addon-import/tests/slayer.rs`
  `team_and_mini_game_inputs_run_and_restricted_outputs_need_rights`.
- `crates/addon-import/tests/ports.rs` `slayer_ports_apply_with_their_rules`.
- `crates/package-runtime/src/content.rs`
  `a_brick_input_follows_an_input_by_name_and_may_aim_at_the_killer`.
