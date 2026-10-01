# 2026-10-01 Slayer: game settings, late join, and what moves to v0.1.12

Max picked "Ship what's ready" for v0.1.11, so the Slayer and CTF port
lands as it stands. Earlier entries on this branch cover chat, kill lines,
rights, team rules and the client screens.

## What changed

- Slayer's game settings now reach the engine through the game-rule seams:
  Color, Default Minigame, Name Distance, Clear Scores (keep scores),
  cleanup on leave, and brick claims. Save Settings on Update and Auto
  Start With Server keep the host's last game with `set_host_data` and
  bring it back with `restore_minigame`.
- Late Join Time: a player joining a round older than its minutes, with
  more than one other player alive, watches until the next round. Their
  body is taken back with the new `remove_body` op, which nobody scores.
  `on_spawn` leaves such a player alone.
- Reset When Empty pauses a game when its last player leaves and resets it
  when the next one joins.
- Use Spawn Bricks off and Spawn Brick Color: `on_pick_spawn` may answer
  `"map"` for the map's own drop points (`map_spawn`, split out of
  `pick_spawn`).
- Changed settings are announced as Slayer does ("X updated the Y
  minigame." and one line per change that is not quiet), using the new
  `setting_info` and the existing `setting_text`.
- Team swaps, /teams living, /addLives, balancing, shuffling, spectator
  teams and unique team names.
- Bot names come from Slayer's own `first-names.txt` through the new
  rules data files (`rules.data` in port.json, `data_lines(id)`), copied
  from the player's copy at import. The repo has a CC0 stand-in fixture.
- Round-start rules lines go to each group of members with the same lives
  as one `tell(list, line)`, as Slayer tells each member at their team's
  lives.
- Engine: mini-game event chat is charged to the player whose action
  caused it (the editor, joiner or leaver), and the package chat share
  allows a 32-line burst at 8 lines a second. Before this, every join and
  edit shared one server-wide share of 8, and a settings edit with a few
  changes lost lines.

## Merge with main

Main gained its own server-scope settings (host-only, Admin menu, `pref`,
`restart`). This branch had built a second version (server settings in
every game's window, kept in host data). Main's wins: the branch's copy is
removed, Slayer's server settings (Create Minigame Rights, Max Teams,
Auto Start, Save Settings on Update) are edited in the host's Admin menu,
and the rights test sets them with `set_server_settings`. The branch's
editor levels, help, avatar and `quiet` flags stay.

## Fixtures

The stand-in prefs now match the original's defaults where tests depend
on them: Clear Scores on, Disable Slayer Messages off, Default Minigame
off.

## Evidence

- `cargo test -p bri-addon-import --test slayer`: 26 passed, including the
  new `slayers_game_settings_reach_the_engine_and_late_joiners_wait`
  (fails on the old rules: no default game, colour, name distance or late
  join).
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: clean.
- Tests of bri-ui, bri-package, bri-package-runtime, bri-sim,
  bri-addon-import and bri-client after the merge.

## Moved to v0.1.12

Slayer:
- The "<brick> set for <teams>." bottom print as a Slayer brick is
  planted or painted (the `on_brick` seam is on main; the rules do not use
  it yet).
- Region Boundary bricks (`set_minigame_region` exists; nothing computes
  the box yet).
- `Team_<n>` and `TeamVehicle_<n>` brick names, and Team Vehicle Spawns
  keeping other teams off (`on_ride` exists).
- Bricks coming back on reset and `onMiniGameReset` on each brick
  (`revive_bricks` exists).
- CP repaint by the paint can resetting the point's control.
- Team Bot Holes and path or rally nodes.
- Uniforms on player types with their own model, and the advanced-only
  `Slayer_setPref` output.
- Slayer's own "The X minigame was reset." line on automatic resets (the
  engine's "X reset the mini-game" covers resets by a player).

CTF: bots, flag types (Flag Type), Max Flag Colors, and the DropFlag
output's restriction.

Not a gap: team captains. Slayer 4.1.5 never makes anyone captain, so its
captain settings do nothing in the original either.

The `handles` maps in both port.json files were not filled in, and the
real copies were not re-measured after this batch. The Gate's re-import
gives the current numbers.
