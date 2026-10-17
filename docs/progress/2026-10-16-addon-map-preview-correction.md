# 2026-10-16 Add-On map previews through native packages

Root assigned Maxwell's missing Add-On map previews report as B9. The bounded
atom starts from immutable reporting checkpoint `459970cb874998a0b3bc30bb5865eeff325366ae`.
Only v20_map.rs, client/content.rs, two new regression targets and this note
change. Existing report/B1/SP/drop branches remain preserved. No menu, network,
protocol, manifest/dependency or shared schema-definition files change.

## Source and ordinary-menu counterexamples

Read-only community source archives match the installed import reports:

- Creek: archive SHA256
  `335751840111ef75446ad5ccc96ca6d033d3100213ad78de8750a370dbd6d098`,
  creek.jpg 1024x768, image SHA256
  `8b0e166a83dc1fecdf454a87e2e22f7372d2af4df4e6b06220fb2fe27a7e5ae5`;
  native ID `map_creek:map/map_creek`.
- Emerald Isles 4: archive SHA256
  `472bc408a556ebf3360ce06a3044026bd0e3e73eafd5ff95c1bf3ad4467d97dd`,
  VH_EmeraldIsles4.jpg 1152x864, image SHA256
  `f24484a5aab4757e4256ca79925c195c26cfaa09d5db1ad68ffcb9f997fedfcc`;
  native ID `map_emeraldisles4:map/map_emeraldisles4`.

Both originals decode, but their installed native packages lack these image
bytes, matching image dimensions, original filenames and preview bindings.
Native map entries contain only file/id/name. Forest day/night corroborate the
gap. ClientContent also hardcoded IconRef::None for every Add-On map.

The headless baseline uses actual installed native ClientContent and native
menu row mouse events (a reveal_row helper only scrolls). Bedroom decodes its
1024x768 stock preview and both menus select it. Creek/Emerald/Forest day/night
retain the authored default picture in Start Game and Change Map, as does an
invented UI-only missing-image control. AvP is excluded. Admin DTOs provide
menu rows; this is not a live server-authority or gameplay test. The first
driver incorrectly tried Home/Down on a non-keyboard-focused text list; the
corrected mouse-row driver completes all twelve menu observations.

Recovered v20 client getMissionPreviewImage and server serverGetMissionPreviewImage
establish companion mission-stem PNG first, then JPG, empty if neither exists.
Existing ui-import uses the same order. Neither recovered function reads
MissionInfo.previewImage. Root explicitly corrected the earlier field-priority
hypothesis: this change follows the recovered companion rule only.

## Correction

Each map import copies the first existing companion PNG/JPG's ORIGINAL bytes,
using the existing case-insensitive, confined original-resource reader. Its
optional per-map preview metadata reuses the existing native environment Image
record: local filename, source, SHA256 and dimensions. Files are flat hash names
inside that map's bundle and enter its normal recursive package inventory.
Distinct missions retain their own images. Nothing is synthesized or re-encoded.

Encoded size is capped at 4 MiB, each dimension at 8192, pixel count at 16,777,216
and decoder allocation at 64 MiB. Only the PNG/JPG choice's matching encoding is
accepted, with complete decode before binding. Invalid preferred PNG does not
invent a different corrupt-image-to-JPG precedence; it leaves the placeholder.
Missing or bad optional art never fails an otherwise loadable map.

The client reads optional metadata defensively, confines its canonical file to
the native bundle, checks byte budget/hash/dimensions, enforces matching PNG/JPEG
encoding/extension and fully decodes with limits. It registers an existing UI
image under native-map-preview/<full-map-id>, preventing basename collisions.
Malformed, missing, oversized or escaping art produces a cosmetic warning and
IconRef::None; it does not remove the map or affect another map's preview.
Both existing menus receive that same MapInfo.preview. Stock lookup is unchanged.

Format agreement matters because the existing UI decoder opens the stored file
by extension. A mislabeled payload could otherwise pass a byte-sniffing check
yet fail actual UI decoding. Regression cases cover this explicitly.

## Evidence and limits

The new importer target covers original byte equality and package inventory,
PNG-before-JPG, mixed-case lookup, two missions with distinct originals,
ignored field-first decoy, absent/corrupt/unsupported/mislabeled/oversized art.
The new client target verifies full decoded pixels, distinct IDs with identical
basenames, stock behavior, missing-image fallback and eight actual menu image
quads after row mouse selection. Eleven invalid metadata/file cases cover
traversal, absolute paths, absence, corruption, format mismatch, encoded size,
dimensions, checksum and malformed metadata, with another map staying healthy.
These fixtures contain invented art only.

Initial client test setup omitted `?` on the PNG fixture helper, then lacked
its synthetic default-picture resource. Both were corrected before final-source
baseline/tiers. The latter failed only the missing-image quad; valid previews
already drew. No production/menu change was made for either setup issue.

Final-source scratch plain imports of the two actual community archives use
the designated read-only E: vanilla installation for borrowed resources. Both
complete. An ignored probe links the owned outputs into a synthetic native root,
loads ordinary PackageSet/ClientContent and verifies source/archive identities,
original image file hashes, full decoded dimensions and four real menu quads.
Admin DTOs remain a fixture; no live network or GPU screenshot acceptance is
claimed. No visible game/OS input, original mutation or installed/public bundle
regeneration. AvP remains excluded from public validation.

Receipts retained under ignored target/:
map-preview-source-receipt.json, map-preview-diagnostic-final.log,
map-preview-final-real-{creek,emerald}.log, map-preview-real-ui.rs/.log,
map-preview-final-build.log, map-preview-final-cli-build.log, and the owned
preview-final-real-* native directories. Original images/scripts are not committed.
Existing installed packages need reimport after root combines the final fix.

## Final-source checks

All Cargo calls use two jobs, RUST_TEST_THREADS=8, debug=0, incremental=0 and
CARGO_TARGET_DIR unset. Final unchanged regressions run against parent 459's
production files, then restore fixed bytes in a finally block before tiers.
Expected baseline: importer 1 pass / 1 missing-preview failure; client 0 passes /
2 missing-preview failures. Corrected targeted results: importer 2/0 and client
2/0. Receipts: map-preview-final-baseline-bri-{addon-import,client}.log,
map-preview-{import,client}-format-green.log. Source hashes before the first
focused attempt are in map-preview-frozen-source.json; the final frozen receipt
is map-preview-final-frozen-source.json.

- `python tools/check.py quick --crates bri-addon-import,bri-client`: 4 steps,
  76 seconds, 642 passed / 0 failed / 76 existing ignored (importer 58, client
  584). Receipt: target/map-preview-quick.log.
- First focused attempt stopped after 4 steps / 64 seconds on Clippy's
  manual_range_patterns in the new client test. Replaced `3 | 4 | 5` with the
  equivalent `3..=5`; assertions and production code are unchanged. Failure:
  target/map-preview-focused-first.log. The final quick rerun passes 4 steps /
  70 seconds, 642/0/76, in target/map-preview-quick-final.log. The full focused
  tier restarts on that exact final source; no failed step is waived.
- `python tools/check.py focused --crates bri-addon-import,bri-client --depth 1
  --tests bri-addon-import:map_preview --tests bri-client:addon_map_preview`:
  FINAL all 9 steps pass, actual exit zero, 596 seconds. Both changed crates
  pass all-targets warnings-denied Clippy. Across 76 result summaries: 1,094
  passed / 0 failed / 105 existing ignored, including explicit importer 2/0
  and client 2/0. Receipt: target/map-preview-focused.log. All four production/
  test source hashes remained unchanged against the final frozen receipt.
- `git diff --check` passes. No full gate, main merge, public/installed content
  regeneration or publication was performed. Root owns the combined candidate,
  package regeneration, release validation and playtest handoff. Maps without
  source PNG/JPG previews correctly retain the placeholder.
