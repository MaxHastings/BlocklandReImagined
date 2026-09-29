# bri-audio-import — vanilla audio converter (offline only)

Builds a native audio pack from the designated Blockland v20 reference
installation and the recovered (decompiled) base scripts. It never writes into
the installation and never executes TorqueScript. Scripts are tokenised and
scanned as text evidence. The Torque readers (script scanner, zip-mounted
virtual file system) live only here, outside the runtime dependency graph.

```powershell
cd crates/audio-import
cargo run --release --locked --bin bri-audio-import -- `
  --v20 "E:\Downloads\B4v21Launcher\versions\Blockland v20" `
  --decompiled ..\..\.research\v20-dso --decompiled-label .research/v20-dso `
  --out-root ..\..\content `
  --evidence ..\..\artifacts\native-audio `
  --coverage ..\..\docs\research\audio\coverage.md
cargo run --release --locked --bin bri-audio-evidence -- --pack ..\..\content\audio-pack-002 --out ..\..\artifacts\native-audio
```

`--out-root` picks the first free `audio-pack-NNN`. An explicit `--out` must not
exist. The pack is staged in a `.audio-pack-NNN.partial` sibling, checked by
loading it with `bri_audio::SoundBank` (SHA-256 verification), and only then
renamed into place.

## What it reads

| Input | Use |
| --- | --- |
| `base/**/*.wav\|ogg`, `Add-Ons/*.zip` members, `Add-Ons/Music/*.ogg` | every audio file; hashed, decoded, bytes preserved |
| `.research/v20-dso/**/*.cs\|gui` | decompiled base scripts; each one's `.dso` is hash-compared with the installation's |
| loose `base/**/*.cs\|gui` | stock defaults and the B4v21 launcher patch layer (files containing `B4v21`) |
| `Add-Ons/**/*.cs\|gui` | add-on profiles, bindings and call sites |
| `base/server/defaultAddOnList.cs`, `.research/v20-dso/server/defaultMusicList.cs` | default-enabled packages and music flags |
| `base/client/defaults.cs` | stock `$pref::Audio::*` (master 0.9, channels 1.0, ...) |
| `blocklandv20.exe` | string search only: names of engine-bound client profiles |

`config/` (user state), screenshots, saves and launcher modules are ignored.

## Resolution rules

- `datablock AudioProfile/AudioDescription(...)` and `new AudioProfile/...`
  with literal fields. Inheritance (`Name : Parent`) merges parent fields.
  `$global` values (e.g. `type = $SimAudioType`) come from literal global
  assignments. `fileName = $RTB::Path @ "x.wav"` concatenations are evaluated
  for literal operands only.
- Paths: `~/x` resolves under the script's top directory, `./x` under the
  script's directory, and anything else from the root. A zip `Add-Ons/N.zip` is
  mounted at `Add-Ons/N/`. Lookup ignores case, as on Windows.
- Duplicate datablocks: the last one in an *assumed* load order wins (base,
  launcher patch, then add-ons with RTB first and the rest alphabetically). An
  `if(!isObject(Name))`-guarded redefinition is skipped when an earlier
  definition exists. Every definition is kept in `inventory.json`.
- Music is modelled from `createMusicDatablocks`, not executed: each
  `Add-Ons/Music/*.ogg` must pass `isValidMusicFilename`, be at most 1 MiB, have
  `$Music__<name> = 1`, and be mono. It then becomes `musicData_<name>` with
  `AudioMusicLooping3d` and `uiName` = name with `_` replaced by spaces.
- Bindings: any datablock field whose value names a profile (`stateSound[n]`,
  `soundProfile`, `JumpSound`, `impactWater*`, `softImpactSound`, ...). Call
  sites: profile names inside `alxPlay`, `ServerPlay3D`, `playAudio`, `play2D`,
  ... with their enclosing function.
- Menus: `event-param:Sound` (no uiName, non-looping, 3D), `event-param:Music`
  and `wrench:Sound` (uiName and looping), matching the stock client lists.

## Outputs

- `content/audio-pack-NNN/manifest.json` — runtime schema `bri.audio-pack` v1:
  clips, descriptions, sounds, triggers, defaults, diagnostics.
- `inventory.json` — schema `bri.audio-inventory` v1, the complete audit trail:
  archives, scripts with hashes, every audio file, every definition, bindings,
  call sites, engine-bound names, the music rule, and prefs.
- `coverage.md` — generated coverage and trigger tables, also written to
  `docs/research/audio/coverage.md`.
- `clips/<sha256>.<wav|ogg>` — original bytes, unmodified. Identical files are
  stored once.
- Evidence: `<pack>-clip-validation.json` and `<pack>-import-summary.json`;
  `bri-audio-evidence` adds `<pack>-offline-renders.json` and `<pack>-renders/*.wav`.

Generated packs contain original game audio and are ignored by git (`/content/`).

## Tests

`cargo test --locked` runs the lexer/scanner, path and end-to-end tests. The
end-to-end fixture builds a synthetic mini installation (zip add-on, patch layer,
music folder, fake exe) and checks every rule and diagnostic, plus determinism.
