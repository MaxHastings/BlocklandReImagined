# v20 client scripts and engine input against ours

Final-touches sweep, 2026-09-28. The datablock audit cannot see behaviour
that lives in v20's client scripts
(`.research/bl-decompiled/v20/client/...`) or in engine input code.
Recent misses came from there: the vehicle mouse flip, the BSD emote, the
name tag colour and window dragging. This file lists what v20 does there,
and whether we match, differ, or leave it out.

## Fixed in this sweep

| v20 | Before | Now |
|---|---|---|
| `BrickSelectorDlg::onWake` sends `serverCmdBSD`, which emotes `BSDProjectile` ("Bricks" rising over the head, seen by everyone) | only `/bsd` | the selector sends it on opening (862b2da) |
| `openBSD` with `$BuildingDisabled` center-prints "Building is currently disabled." and does not open | opened | says so and stays closed |
| Emote commands check `isObject(%client.player)` and do nothing without a body | errored while dead | no error (00a658e3) |
| `GuiShapeNameHud::drawName` (exe 0x527630) draws names white (or the mini-game colour from `createPlayer`) over an 8-way black outline (white under dark names) and ignores `textColor` | pale yellow, no outline | 24b32b1 |
| `GuiShapeNameHud::onRender` (0x5278f0) casts with mask 0x200001d (the map plus `FxBrickObjectType`) and fades from the fog distance | names showed through bricks | 24b32b1 |
| `handleYourSpawn` with `$pref::Input::AutoLight`: each spawn under a sun with red, green and blue all below 0.4 sends `/light` | never | 2cf3769f |
| `serverCmdLight`'s fxLight is on the player object, which the corpse keeps | light survived respawn | a new body starts dark (2cf3769f) |
| `handleYourSpawn` sets `$BrickAutoBuyDone` in a local Tutorial | auto-bought favorites | skipped (00a658e3) |
| `$pref::Input::UseStrafeSteering` / `UseAutoReturnSteering` (exe only; see `v20-unread-fields.md`) | fixed, guessed steering | both prefs, v20's formulas (ebffe9ce) |

## Prefs (`tools/audit_v20_prefs.py`)

`client/defaults.cs` sets 237 prefs. The script compares each with the
default our code passes where it reads that pref.

- **Real differences.** `$pref::Video::disableVerticalSync` is 1 in v20
  (vsync off), but we default to vsync on. This is kept deliberately:
  tearing on modern displays. The OpenGL texture filtering prefs map onto
  our own graphics settings.
- **False positives.** Test fixtures read `MenuSounds`, `masterVolume` and
  `HideBrickBox` with throwaway defaults. The runtime defaults match v20.
- **`$pref::Player::zoomSpeed`.** Its last assignment is 5, not 0.
  `setZoomSpeed` clamps both to 200 ms, so the zoom ramp is unchanged.
- **Named nowhere in our code, but player-facing.** Some are handled under
  other names: the avatar prefs live in our settings file, and ChatSize
  drives the chat font. Others are still left for parity's Options rows:
  the `tempBrickFlash*` ghost-brick flash, the
  `tempBrickInside/Outside*` colours, `renderMyPlayer`, `renderMyItems`,
  `renderMyJets`, `Chat::CurseFilter` and `ChatRepeat`. The rest are
  Torque renderer and network tuning with no counterpart.

## Binds

The default keymap is converted from `defaultControlsGui::apply` for each
mouse and keyboard choice, so it follows v20 by construction. Held
controls match v20's `%val` handling:

- Held: move, crouch, jet, fire, walk (`$RunMultiplier` 0.4), free look,
  and zoom (`toggleZoom` sets the zoom FOV while held).
- Toggle on press: first/third person (fast with
  `FastFirstThirdPerson`) and super shift (`UseSuperShiftToggle` and
  `UseSuperShiftSmartToggle` as v20).
- Jump also jets with `noobjet`.
- `ToggleCursor` does nothing in single player.

## `clientCmd*`

Every `clientCmd` the stock server sends has a native counterpart. Some
have none, and nothing player-visible depends on them:

- `SetFocalPoint`, `SyncClock` and `GameStart` are empty or unused.
- `DoUpdates` and `Use/StopBrickControls` return at their first line in
  v20.
- `SetLoadingIndicator` is the ghosting HUD, covered by our loading
  screen.
- `CancelAutoBrickBuy` is covered by the first-spawn auto-buy.

## Handed to other lanes

- **Parity.** Wire the Options checkboxes for Auto Light
  (`$pref::Input::AutoLight`), Strafe Steering and Steering Auto-Return
  (the two prefs above), Censor Chat, Up to repeat chat, the temp brick
  colours and flash, and Render My Player, Items and Jets. The behaviour
  for Auto Light and steering is in place and reads those names.
