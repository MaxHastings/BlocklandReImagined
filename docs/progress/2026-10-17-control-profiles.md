# 2026-10-17 Control profiles (AA train lane 6)

Branch `codex/control-profiles`, based on origin/main 82639602.

## What changed
- `binds.json` can declare control profiles (`profiles`) and put its binds in
  one (`profile`). A bind is an action: a package command (or a stock
  Blockland action, `stock`) with a suggested key or mouse input (`mouse`).
  Modder doc: `docs/modding/content-kinds.md`, "Actions and control profiles".
- `bri_content::controls` holds the declaration types and validation.
  `bri_ui::profiles::ProfileState` holds each profile's bindings, the active
  profile (follows the local player's body archetype id) and the vehicle layer
  (follows the rider: vehicle, driver, mouse driver, passenger).
- Gameplay lookups (mouse button, wheel, key) go through
  `Core::game_command_for*`: the profile's own bindings first, then
  Blockland's when the profile inherits. Dialog, chat and menu keys stay on
  Blockland's map. Mouse sensitivity and invert come from
  `Core::mouse_settings()` (layer, profile, then the player's global ones).
- Options → Controls: a tab per profile above the list (taken from the list's
  height) plus a row of layer buttons; the existing sensitivity slider and the
  two invert checkboxes are re-pointed at the profile's or layer's preferences
  (`$Pref::Profile::<id>::...`). Conflicts are checked only within the map
  being edited.
- Saved settings: `Settings::profile_binds` (new, defaulted) keeps each
  profile's bindings, including profiles whose Add-On is not running.
  `Settings::binds` and every existing preference are unchanged.

## Evidence
- `crates/ui/src/profiles.rs` unit tests, `crates/ui/tests/control_profiles.rs`
  (profile switches on player-type change through real key input, vehicle layer
  stacks and leaves, per-profile choices persist through a settings round trip,
  settings from before profiles load, a stock action moved off a taken key),
  `options.rs` tab test, `package-runtime` validation tests.
- `python tools/check.py quick`: passed.

## Not done
- Mouse movement (x/y axes) is not an action input; bind validation refuses it.
- The tabs were laid out from the list's own rectangle and checked headlessly;
  Max has not seen them on screen.
- Vehicle layers match by seat role (`SeatLook`), not by vehicle id.
