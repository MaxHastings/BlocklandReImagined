# A build keeps its mini-game (Slayer's .pathcam)

Slayer saved a mini-game's settings as `.mgame.csv` and `.teams.csv`, and
its fly-through module wrote the camera path beside them as `.pathcam`,
read back when the config loaded (`exportMinigamePreferences`,
`importMinigamePreferences`). We have no config files: the coordinator's
call was that mini-game settings save and load with the build, as a
generic seam any mode's port can use.

## Engine
- `SavedBuild::minigame` (optional, at most 1 MiB as JSON) in the save
  file's header and on the wire (`protocol-changes/build-minigame.md`).
  Builds saved before it load with none: the field defaults, in both the
  binary and the JSON forms.
- Saving keeps the mini-game the saver runs (owns, or may edit as an
  admin): its settings and colour, Add-On settings, teams with their
  settings, and each running package's `per_minigame` state entries for
  that game.
- Loading sets it up once the bricks are in. The loader's own game takes
  the settings; a game mode's game keeps its own settings but takes the
  Add-On settings and teams; a loader in no game gets a new one (the saved
  colour if free). Settings of Add-Ons not running here, or values they no
  longer allow, are left out. If it cannot be set up, the loader is told
  and the bricks load anyway. `on_minigame` then hears `loaded`.
- State keys gain `"per_minigame": true`: a global map from game id to
  that game's value.
- Bots' per-player package state now also leaves the state budget when the
  bot goes.

## Slayer
`flycams` is per mini-game. On `loaded` the path is untested, its rounds
counted afresh, and the game resets, as `serverCmdSlayer_loadConfig` did.
ports.json pins the `.pathcam` load, save and delete shapes.

Found on the way: Slayer_CTF's rules failed when any game ended
(`reset_flags` read the gone game's settings). It now drops that game's
kept state.

## Tests
- `crates/world/src/build.rs`
  `a_builds_mini_game_saves_with_it_and_older_builds_load_without_one`.
- `crates/sim/tests/script_api.rs`
  `a_saved_build_brings_back_its_mini_game_and_the_add_on_state_kept_per_game`.
- `crates/addon-import/tests/slayer.rs`
  `a_saved_build_keeps_its_mini_game_and_fly_through_path`.
