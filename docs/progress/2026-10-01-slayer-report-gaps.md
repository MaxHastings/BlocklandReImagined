# Slayer and Capture the Flag: the gaps in the corrected report

Thread: Capture the Flag (Slayer). From the Gate's corrected report zip
(`slayer-port-reports-8e9f`): the Capture the Flag functions still listed
as unported, and Slayer's three datablocks that did not convert.

## Capture the Flag

Newly ported and pinned in `ports.json` against this copy's text:

- `Slayer_CTF::onGameModeStart` / `onGameModeEnd`: switching a game to
  Capture the Flag puts every flag on its stand at once; switching away
  takes them all, carried and dropped ones included, and clears the
  captures and the report tallies (`resetFlags(true)`,
  `CTF_numFlagReturns = ""`). One `reset_flags(game, gone)` in `ctf.rhai`
  now serves these and the mini-game reset.
- `serverCmdSetWrenchData` (packaged): a Flag Spawn whose item something
  else changes gets its flag back at the next check.
- `Player::mountImage` / `unMountImage` (packaged): a carried flag is no
  other Add-On's to swap or take off. New generic seam: `mount_image(p,
  image, slot, #{ paint, keep: true })` (`Op::WearImage::keep`); the sim
  keeps which package put a kept image on and refuses other packages while
  it is worn. Death or the owner taking it off frees the slot.
- `onAdd`, `onRemove`, `onMiniGameBrickRemoved`, `getFlagImage`,
  `isCarryingFlag`: bookkeeping the rules' flags map and `carrying` value
  already do (a removed Flag Spawn's flag goes at the next check).

Left: bots (`assignBotObjectives` and the bot objective callbacks), with
Slayer's bots.

## Slayer's three datablocks

- `HappyHolidaysAudioDescription`: an `AudioDescription` is a sound's
  settings, which the importer already reads into each sound naming it.
  It now reports as `consumed` (all Add-Ons, not just Slayer).
- `slayerSound`: the loop that makes the ten countdown voices at run time.
  The port's `datablocks.cs` already declares them; a port can now say so
  (`port.json` `replaces`), and the report counts it as the port's.
- `LetItSnowMidi`: the Christmas greeting's music, which Slayer downloads
  from greek2me.us into `config/client/temp` on the player's machine. It is
  not in the copy, and the game downloads nothing from third-party sites,
  so it is not ported. The report now says so: a sound whose file the
  copy's script fetches over HTTP (`connectToUrl`, `HTTPObject`,
  `TCPObject`) is `external`, "needs a resource downloaded from an
  external site, not in the copy", counted apart from recognised-only
  datablocks and not as a gap in the verdict (`import.rs`
  `a_sound_the_add_on_downloads_is_reported_as_external`). The greeting
  itself is a client easter egg (December 20 onward) and is not ported.

## Evidence

- `sim/tests/script_api.rs` `a_kept_worn_image_is_only_its_add_ons_to_change`.
- `addon-import/tests/slayer.rs`
  `flags_follow_the_mode_and_stay_on_their_stands_and_backs`; checked to
  fail with `keep: false` and without the stand check.
- `ports.rs` `slayer_ports_apply_with_their_rules` asserts `slayerSound`
  consumed; `import.rs` asserts the kit's `AudioDescription` consumed.
- The new `ports.json` patterns match both the real copy's
  `game-mode.cs` and the synthetic fixture.
