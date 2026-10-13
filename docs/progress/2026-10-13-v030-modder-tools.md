# 2026-10-13 v0.3.0 modder tools

Max asked what friction the Mario and Halo Add-On work ran into that the
game could take away for modders. Four tools, all generic engine features
with modding docs and no per-Add-On cases:

## Why an Add-On or body is not in a game

Halo Slayer's mode named Master Chief, and a Custom game leaves out every
package a mode claims (`Catalog::for_base_map`), so Chief vanished from a
Custom game with no word why. Now:

- `Package::runs_per_game` separates what a host runs per game (rules,
  bodies, controllers, HUD, creatures, blocks, worlds, modes) from weapons
  and vehicles, which load whatever the game and need no message.
- `Catalog::plays_only(id)` says, in words, which game mode or map plays an
  enabled Add-On; the Add-Ons screen shows it as "Not in every game".
- `Catalog::left_out(mode, map)` lists what a hosted game leaves out and
  why; the host gives the left-out bodies to the session, so `/body` lists
  them ("Not in this game: ...") and `/body <name>` gives the reason.
- A sound the client could not decode says why in the Add-On's health
  ("plays silently: ...").

Tests: `crates/package-runtime/tests/left_out.rs` (the Playground mode's
Stomper; the Stress Lab with and without its mode),
`crates/sim/tests/body_switching.rs`.

## Edits while a game runs

A hosted game keeps the Add-Ons it started with, so editing a file and
switching back changed nothing, silently. `crates/client/src/add_on_edits.rs`
stamps each enabled Add-On folder as the game starts and again when the
window regains focus, on a worker thread (never per frame). An Add-On whose
stamp changed is named once in chat ("... changed on disk. This game keeps
the copy it started with: host again to play your edits.") and marked in
the Add-Ons screen; the screen's status line says changes there apply to
the next game while one runs.

## Live tuning

`/tune` opens the host's Tune Body window (native, in the Environment
window's style): every decimal number of a body by part (Movement, Hands
and eye, Camera, Seats, Animation, Health), each a slider plus an exact
value box.

- Host: `Session::tune_archetype` rebuilds the body from its archetype
  file JSON with the change, by the loader's own code, keeping its bound
  controller. Host-only; `adjusts` bodies and v20 types are refused.
- Wire: `Delta::archetypes` carries tuned bodies (without controller
  modules) when the tune revision moves
  (`crates/net/protocol-changes/live-body-tuning.md`). Replicas apply them
  to a copy of the table and keep it only if it validates; the predictor
  takes the new table so the local player predicts with the new numbers.
- Save to Add-On (`crates/client/src/tune.rs`) writes only the changed
  paths into the body's file with `bri_package::json_edit::set_paths`,
  which keeps the file's text, key order and spacing and refuses a result
  that would not read back as the tuned values. The save restamps the
  edit watcher, so it is not announced as an outside edit.

Not tunable yet: constants inside a WebAssembly movement controller (the
Mario and Halo movement lives there). Doing it needs a controller params
section and an import to read it; left for v0.3.1.

## Packaging

`bri-addon-check <folder> --package [--out FILE.zip]` checks, then writes
the Add-On (and its `-rules` companion) as a native ZIP with sorted entries
and fixed dates, skipping hidden files and `target`, and reads it back
comparing every file's SHA-256 before keeping it. It refuses to pack when
the check fails. Test: `a_packaged_add_on_installs_byte_for_byte_and_packs_the_same_twice`.

## Layout review

Renders from the real UI pack (`crates/client/tests/modder_screens.rs`,
ignored; `BRI_UI_PACK`, `BRI_RENDER_OUT`) were posted to Max for approval:
the Tune window's Movement, Hands and saved states, and the Add-Ons
details with both new paragraphs.

## Commands

- `cargo test -p bri-package --lib json_edit` (3 pass)
- `cargo test -p bri-package-runtime --test left_out` (2 pass)
- `cargo test -p bri-sim --test body_switching` (15 pass), `-p bri-sim --lib tuning` (7 pass)
- `cargo test -p bri-net --test replication` (17 pass)
- `cargo test -p bri-ui --test tune_window` (1 pass)
- `cargo test -p bri-client --lib add_on` (47 pass), `--lib tune::` (1 pass)
- `BRI_UI_PACK=content/ui-pack-001 cargo test -p bri-client --test modder_screens -- --ignored`
- `python tools/check.py quick`, `python tools/check.py focused`
