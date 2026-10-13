# 2026-10-13 Mini-game menu fixes

Max set up a team Capture the Flag game and every team got the mini-game's
default tools instead of the tools he picked for each team. An audit of the
mini-game menus turned up more bugs, fixed here together.

## Fixes

- **Team loadouts were ignored.** Slayer's `team_sync_loadout` (the v20
  "Sync" box) defaults on, and while it is on a team plays with the
  mini-game's own player type and start tools. The menu still showed the
  team's player type and tool popups, so picking tools looked like it worked.
  Those settings are now `shown_when` sync is off and `quiet`, and the sync
  box explains what it does (`gamemode_slayer/rules/behaviour.json`).
- **Team lives were dropped when the window saved.** `mg_teams` re-noted every
  team's lives on the "teams" event, which arrives after the new settings are
  stored, so a team's lives edit never reached its members. It now only
  forgets teams that are gone (`slayer.rhai`, `forget_gone_team_lives`).
- **Changing Game Mode did not reset the game.** A setting marked `resets`
  now resets the game on Update, as Update & Reset would. `edit_settings`
  returns whether a changed setting resets (`sim/.../settings.rs`,
  `combat.rs`).
- **A refused Save looked like success.** An "edit" refusal is now an error
  carrying the package's text, without colour codes (`combat.rs`).
- **Favourites crossed scopes.** Loading a favourite applied server settings
  in the game window and replaced teams from a server-only favourite
  (`ui/.../minigame_addons.rs`).
- **Add Team named a duplicate.** It now picks the lowest free "Team N".
- **Reset in Team Deathmatch put CTF flags on their stands** (`ctf.rhai`).

Known gap, not changed: `captain_*` settings are faithful to Slayer 4.1.5 but
unused, `announcements` is unused in Slayer, and `host_name` is not plumbed.

## Evidence

- `cargo test -p bri-addon-import --test slayer`: 47 passed. New tests
  `a_teams_lives_reach_its_members_when_the_window_saves` (fails without the
  fix) and `saving_a_new_game_mode_resets_the_game`.
- `cargo test -p bri-ui minigame`: 36 passed, 1 ignored.
- `cargo clippy -p bri-sim -p bri-ui -p bri-addon-import --all-targets -- -D warnings`: clean.

## Next

A single Edit Mini-Game window (Game, Teams, Rules, Players, More Settings
tabs) built from the existing v20 controls, waiting on Max's approval of the
offscreen renders.
