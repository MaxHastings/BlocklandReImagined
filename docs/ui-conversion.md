# Native UI conversion audit — 2026-09-26

The transferred `content/ui-pack-001` is preserved. A fresh full-install pass is
`content/ui-pack-002`; it is an installed-content inventory, **not a verified
vanilla content allowlist**. Generated original assets remain ignored. No visible
game, desktop input, or interactive playtest was used.

## Reproduction

Run from the workspace root. Choose a new output directory; its parent must
already exist. Existing outputs are deliberately never overwritten.

```powershell
cargo run -p bri-ui-import --locked -- --v20 'C:/Users/Maxwell/Desktop/Games/B4v21-Launcher-Release/versions/Blockland v20' --decompiled .research/v20-dso --stock-defaults .research/bl-decompiled/v20/client/defaults.cs --out content/ui-pack-002
python tools/verify_ui_pack.py 'C:/Users/Maxwell/Desktop/Games/B4v21-Launcher-Release/versions/Blockland v20' content/ui-pack-002 --compare content/ui-pack-001 --report artifacts/native-ui-pack-verification.json
cargo test -p bri-ui-import --locked
cargo clippy -p bri-ui-import --all-targets --locked -- -D warnings
```

Final targeted tests: 12 passed. Clippy passed with warnings denied. The
independent Python verifier uses Pillow and the standard library, not Rust
converter code. It passed source/output hashes, original byte equality, image
decoding/dimensions, PNG chunk CRCs, original GFT metrics/remap/glyph values,
font sheet byte equality, glyph rectangles and skin bounds.

## Content and comparison

| Entry | Full install pass |
| --- | ---: |
| Images | 528 |
| Fonts / embedded PNG sheets | 39 / 41 |
| Bitmap-array skins | 7 |
| Styles / layouts | 114 / 68 |
| Installed missions | 26 |
| Default binds / remap entries | 122 / 81 |
| Warnings | 23 |
| Verified consumed installation inputs / recovered scripts | 630 / 4 |
| Verified outputs, excluding manifest itself | 570 |

The 571st output is `conversion-manifest.json`. The independent report records
its checksum separately and verifies the output directory has exactly the
manifested files plus that manifest. Every consumed VFS input records its
virtual path, actual loose-file/archive-member origin, SHA-256 and byte count.
All output files have checksums and sizes. The manifest augments UI schema v1
without introducing Torque readers into the runtime graph.

Compared with 001, the full install adds ForestNight and GSF Paradise Night
mission entries and preview images. All shared image bytes match. Smiley's
source path changed only `faces` to the original `Faces` casing. Fonts, skins,
styles, layouts, UI data and all four recovered-script hashes match exactly.
Font sheets are 256×64, 256×128 or 256×256. Image dimensions reach 1152×864.

## Source protection and bounds

The previous lexical `starts_with` output check did not resolve aliases or
`..`. The converter now canonicalizes the existing output parent and original
root, compares Windows paths case-insensitively, requires a fresh final output
directory, and uses no-overwrite file creation. Relative output names reject
traversal, absolute paths, Windows streams/device names and ambiguous suffixes.
Synthetic path tests cover `..` and existing destinations. A synthetic Windows
junction at `artifacts/native-ui-guard-fixture/alias` pointing to its sibling
`original` was rejected before writing; the fixture source remained empty.
No guard test targets the real original installation.

VFS reads are capped at 64 MiB, unsafe archive paths are reported and skipped,
and linked sources are skipped. Images are bounded before decoding skins;
font sheets and metrics are bounded and decoded, and mapped glyph rectangles
must fit. Embedded PNG ends now come from a chunk-length walk, not searching
compressed bytes for the text `IEND`; regression coverage includes embedded
`IEND` text and truncation.

## Warnings and unresolved fidelity

The 23 warnings are 1 unreadable ZIP (`Map_BiomeRacing.zip`, a RAR), 10 rejected
backslash-named Skybox members in community `Map_Slate_Death_valley.zip`, 11
transferred missing style/layout bitmap bindings, and 1 explicit scope warning.
The skybox members are outside this UI conversion's selected assets. Full warning
strings are in the pack and independent report; no warning has been hidden.

Missing style bitmaps are BlockRadioProfile, ColorRadioProfile and
GuiToolWindowProfile. Missing layout bitmap references include Jirue's Knight,
FMJ face thumbnails, and avatar `none` icons. These require evidence-based
dynamic/runtime bindings or explicit adaptation. `Map_MyChallenge` has no
preview. Font selection retains the transferred nearest-size/fallback behavior;
it is not proof of exact original font selection in every layout.

The VFS deliberately includes all installed Map, Print, Face and Decal packages
and 10 screenshot images. Community examples include Forest/Paradise night,
ArcticChallenge3, AvP, EmeraldIsles4, MaxwellChallenge and MyChallenge. Installed
custom content must not silently become the vanilla map or avatar catalog.
Stock package classification and a production menu allowlist remain work.

Tutorial is present in both 001 and 002 as `Map_Tutorial.zip:tutorial.mis`,
23,090 bytes, SHA-256
`b4684d00050610d0417c878667cf4aafd3036b7df2d6e137a60c0138c8f96886`.
Its preview is preserved; no companion description was found. This corrects
the earlier assumption that the current installation lacked a Tutorial mission;
it does not establish its provenance, geometry completeness or playable behavior.

Byte preservation and bounded parsing do not prove UI appearance, navigation,
layout behavior or action dispatch. Runtime adapters, vanilla-only selection,
offscreen visual evidence and Maxwell's interactive acceptance remain separate.

## Native stock icons follow-up

Canonical `content/ui-pack-003` adds30 original add-on brick icons selected by
`--brick-catalog content/stock-catalog-004/stock-catalog.json`. No unrelated
add-on images are pulled in by this extra input. The catalog itself is hashed in
provenance. Counts are now558 images,39fonts/41sheets,7skins,114styles,68layouts,
26 installed maps and the same23 warnings. All166 selectable stock bricks have
an original icon. Independent verification reports660 asset inputs, five metadata
inputs (four recovered scripts plus native catalog),600 outputs plus manifest
checksum in `artifacts/native-ui-pack-003-verification.json`. Its comparison
shows only30 images added; existing images/fonts/styles/layouts/data are unchanged.
