# Map bundle mismatch between two PCs (2026-09-28)

Maxwell built a16 from `64b1487` on two PCs. Joining one from the other was
refused with Can't Join: "This server's add-ons don't match yours", listing
one row, `v20-map-bundle`, with Server 16.0.0 and You 16.0.0.

## What a join compares

A joining client sends each loaded package's id, version, side and content
hash (`crates/package/src/environment.rs`). The host refuses the join when a
`shared` package's hash differs, even with equal versions. The hash covers
every file in the package folder: relative path, length and SHA-256, in
sorted path order. It does not cover the install path, timestamps or the
order the files were written. Only the map bundle differed, so every other
shared package, including the brick geometry the bundle is made from, was
byte-identical on both PCs.

## Evidence

- Rebuilding the map bundle on this PC from the E: v20 reference, with a
  fresh geometry pass and the 14 converted stock missions, reproduced the
  shipped `content/map-bundle-016` byte for byte (853 files). With identical
  inputs, on one machine, the importer is deterministic.
- Swapping the platform `sinf`/`cosf` for portable `libm` changed one file,
  `bundle.json`: three floats of one sun direction, in the last bit
  (`0.5000000596046448` became `0.5000001192092896`). Windows' UCRT picks
  different code paths for these functions by CPU (FMA3 or not), so two PCs
  running the same importer can produce different bundles. The map bundle is
  the only pack that bakes lighting from sun angles.
- Map order in `bundle.json` followed the order missions were passed.
  `tools/regenerate_content.py` sorts them, but the bundler did not.
- The package hash also covered files Windows or macOS leave in folders
  someone opens: `Thumbs.db`, `desktop.ini`, `.DS_Store`. The map bundle is
  814 images, which is the folder most likely to get a thumbnail cache.
- The shipped `content/maps-pass-007` is not what today's converter produces
  from either v20 install (23 scenes differ; it lacks the Tutorial and Slate
  Storm scenes that `map-bundle-016` contains). A PC that runs
  `tools/bootstrap.py` from scratch would therefore also differ on
  `v20-brick-geometry`. Maxwell's refusal did not list geometry, so the
  second PC did not fully regenerate. Which of the causes above hit it
  depends on how that PC got its content, which was asked in the thread.

## Fixes

1. `3658d43`: sun directions, water flow and mission rotations use `libm`,
   and the bundler sorts and dedupes missions.
   `crates/convert/tests/import_determinism.rs` imports a fixture install
   (one loose map, one zipped) from two paths, with files written and
   missions listed in opposite orders, requires equal package hashes, and
   pins the bits of one sun direction. The importer's output changed, so
   `v20-map-bundle` is 17.0.0 in `map-bundle-017`. The new bundle differs
   from 016 only in that one sun, and was placed in the main checkout's
   `content/`.
2. `702f869`: package hashes and download listings skip OS litter files.
3. `42bed4c`: Can't Join explains a same-version row in words. For base
   content it says each PC imports the base game from its own v20 folder, so
   one copy came from an older build or was changed, and to install both PCs
   from the same game package. The intro says "game content" when a base
   package is listed.

## Should a host offer base content for download?

No. Hosts serve Add-Ons only, as Night QA decided.

- v20 never sent base-game files. Every player installed the same game, and
  servers transferred only Add-On content a client lacked. The fidelity
  reference keeps that split.
- The base packages are converted from each player's own copy of the
  original game. A host sending them would redistribute original Blockland
  assets to anyone who connects, which the project avoids: original content
  stays out of Git and out of anything we publish.
- Serving them would hide the real fault. A base package that differs means
  one copy is wrong: an older build, a different importer, or a changed
  folder. Downloading the host's copy would just copy whichever is wrong.
- A join already refuses clearly and now says why. The fix is the same build
  and the same content on both PCs, which the determinism test protects.

## Next

- Rebuild `maps-pass-007` from the current converter (as a new
  `maps-pass-008` with a version bump) so `tools/bootstrap.py` reproduces
  the shipped content; until then, install second PCs from the game package
  rather than bootstrapping.
- Consider recording each base package's expected hash beside
  `base-packages.json`, so the importer and the client can say which side's
  copy is not the release's.
