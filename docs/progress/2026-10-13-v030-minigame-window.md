# 2026-10-13 One Mini-Game window

Max: the mini-game menus were "a mess", with too many windows. Before, a
game was set up in three places: v20's Create Mini-Game window, a native
Setup window (Add-On settings, by category), and a native Teams window. Max
approved offscreen renders of a single window built from the same v20
pieces ("yeah looks a lot better"). It is in the v0.3.0 definition of done.

## What changed

- **One window.** `screens/minigame_window.rs` (`ScreenId::MiniGameSettings`)
  is v20's `CreateMiniGameGui` with five tab buttons (`MGW_Tab0..4`): Game,
  Teams, Rules, Players and More Settings. It keeps the window's size, its
  scroll box and its footer (Create/Update, Reset, End, Set Favs, the ten
  favourite slots). The v20 controls are found by name and variable and
  moved between pages (`View::reparent`), so they keep what was typed.
  Sections are v20's white swatches with a black title bar.
- **Packages say where a setting goes.** `SettingDef` gains `place`
  (basics, loadout, win, rounds, teams, scoring, respawn, damage, bricks)
  and `unit` (the words after a number box), and a check box may name its
  on and off choices. The wire change is in
  `crates/net/protocol-changes/minigame-setting-place.md`. Slayer and CTF
  place their settings and get clearer titles. A setting with no place
  shows on More Settings, by category, with its `?` help. The engine
  places nothing by name.
- **Team loadouts.** Each team's section on the Teams page shows its
  loadout first: "Loadout: Same as everyone / Own loadout" (Slayer's Sync
  box), then the player type and five tools while it is the team's own.
- **Favourites** hold the rules, colour, Add-On settings and teams.
  Loading one fills the whole window, and Update sends it: the rules
  first, then the rest.
- **The list** gets a Settings button. It opens the picked game's window,
  read-only when it is not the player's to change. This was the old Setup
  button's way to see another game's settings.
- **The old windows are gone.** `minigames.rs` keeps only v20's list and
  invitation. `minigame_addons.rs` is now only the server-wide Add-On
  Settings that the Admin menu opens (2375 to about 700 lines).
- **A bug the ported tests caught:** loading a favourite while a page
  showed a number box kept the old number. The box's text was captured
  over the favourite's value. Loading now clears the captured text.
- **Rights:** the window keeps showing its game when the player loses the
  right to change it. Add-On settings and teams follow the host's
  `addon_editable`, separately from the v20 rules.

## Evidence

- `cargo test -p bri-ui`: all pass. `minigame_screens` (38) was ported from
  the Setup/Teams windows to the one window. That covers drafts before
  Create, favourites staged rules-then-settings, freezing while a change is
  on its way, hidden partial input, mode-dependent teams, players, and
  read-only views. `field_flow` checks the Game and Rules pages separately.
- `cargo test -p bri-addon-import --test slayer`: 48 pass. The new test
  `team_ctf_teams_spawn_with_their_own_loadouts` runs a team CTF game
  headless: with "Own loadout", Red and Blue spawn with their own tools,
  and "Same as everyone" gives the game's tools back.
- `cargo test -p bri-package`: the new setting fields validate.
- `cargo clippy -p bri-ui -p bri-package -p bri-addon-import -p bri-client
  --all-targets -- -D warnings`: clean.
- Offscreen renders of every tab, for create and edit, from the original
  pack: posted in the thread for Max's approval. They are not committed
  because they need the original art.

## Next

- The team colour popup still says "Colour N", as v20's did.
- Max playtests the window in a packaged build.
