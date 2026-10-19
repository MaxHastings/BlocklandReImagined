# 2026-10-18 Controls: a game dropdown, status line and per-game list

Branch `codex/controls-ui`, from the train tip with `codex/aa-hud-claim`
(8837acc59, queued to land) merged in. Not landed: waits for build 7.

## Why
The per-game control profiles were two pill buttons inside the Controls
list's scroll area. Nothing said which profile was live in the game, Blockland
rows (Jet, brick binds, Drop Camera, brick options) showed in every game, an
unbound or replaced key was invisible, and Defaults / Remap All / Clear All did
not say what they touched.

## What changed
- `Options > Controls` has a "Controls for" dropdown (any number of games;
  Blockland first; the game in use is tagged "(in use)"). Options opened in a
  game lands on the active game. Max asked for no status line or footer text:
  an action with no key, or whose key another took, draws red in the list
  (`NodeState::flagged`).
- A game's list is its own actions, then Blockland's still-applicable ones
  under "Blockland: <division>" headings (`ProfileState::rebuild`). Binding one
  makes an override in that game only; an input a game binds shows "Taken: <name>"
  on the Blockland row it replaces.
- Hidden Blockland actions come from declarations the game already makes: a
  body-scoped `building` HUD claim (extended to Drop Camera at Player / Drop
  Player at Camera; `profiles::building_command`) and the body's `can_jet`
  (`packages::profile_hides`). No second hide list in `binds.json`.
- Clear All / Remap All / Defaults only touch the game shown (their
  confirmations name it); Defaults
  restores declared keys (`ProfileState::reset`); Remap All walks the game's
  own actions (`Core::remap_all_limit`).
- The Options panel hides the brick options and the Jump/Jet Combo where the
  game has no use for them.
- `LayerDef.name` (optional) labels a layer in a new "Mouse while" dropdown.
- `crates/ui/src/bin/controls_probe.rs` renders the screen offscreen.

## Evidence
- `bri-ui` and `bri-content` tests pass, including the profile list/hide/reset
  unit tests and the Options profile test.
- Offscreen render of the real UI pack (`controls_probe`) checked by eye: the
  in-game Arena Assault view and the open dropdown.

## Not done / next
- Arena Assault's `binds.json` (private add-ons repo) should name its aircraft
  layer ("In aircraft") and declare its body-scoped `building` claim, or the
  game keeps showing Blockland's building rows.
- A `building` claim that applies to every body (no `body` list) is not
  attributed to any profile.
- Max's playtest acceptance is separate.
