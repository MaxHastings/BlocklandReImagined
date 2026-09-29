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

## Every base pack, from scratch (second pass)

The map bundle was not the only pack a fresh bootstrap got wrong. On main,
`tools/regenerate_content.py` could not even finish: the weapon importers'
lock files were stale and the avatar step failed (default parts became names
in `8b0975a`; the importer still stored list positions). Once it ran, 11 of
18 packs differed from the shipped copies, and several differed between two
machines:

- Every importer read the whole v20 folder, so the older C: install's extra
  add-ons (Map_Artic, Map_AvP and others) were in the shipped geometry pass
  and UI pack. Importers now read a private view holding exactly the files
  in `docs/vanilla-reference-files.json` (1,035 files of the designated E:
  reference, SHA-256 each). A missing or changed file stops the import with
  a list of what to restore. v20 adds a `.ml` lighting cache to a map's zip
  when the map is first played; removing it restores the shipped archive
  exactly, so played installs are accepted.
- Packs recorded absolute paths (audio and brick-material evidence, the
  weapon-effects base pack and v20 folder, the foliage source bundle).
- Packs recorded line endings: UI script inputs, the weather atlas
  adaptation and brick-material evidence hashed whatever git checked out.
- The shipped worlds, tutorial, avatar, debris and runtime-effects packs
  were built by older importers or from older upstream packs.

Check: two bootstraps of `claude/bundle-match-clean` with Parity's
`9ea1b6f6` applied, one from a CRLF checkout at one path with the E:
reference, one from an LF checkout at a path with spaces with the older C:
install and its extra add-ons. All 18 packs are byte-identical between them.
The seven unbumped packs are also byte-identical to the shipped copies.

| Package | Version | Directory | Package hash (first 16) |
| --- | --- | --- | --- |
| `v20-map-bundle` | 17.0.0 | `map-bundle-017` | `f1fce74442f68a71` |
| `v20-bricks` | 4.0.0 | `stock-catalog-004` | `83abdc401c8a4c03` |
| `v20-brick-geometry` | 8.0.0 | `maps-pass-008` | `c82e75f451ff52ff` |
| `v20-effects` | 4.0.0 | `effects-pass-004` | `ea81c7cc5fafbe93` |
| `v20-brick-materials` | 2.0.0 | `brick-materials-002` | `3ccc41b425230aa2` |
| `v20-avatar` | 2.0.0 | `avatar-pack-002` | `4b738957c5d598c4` |
| `v20-audio` | 2.0.0 | `audio-pack-002` | `437475015b8ea87b` |
| `v20-weapons` | 9.0.0 | `weapons-pack-009` | `1f99ef485005127b` |
| `v20-item-presentation` | 10.0.0 | `item-presentation-pack-010` | `9a1c3fb87a76d467` |
| `v20-vehicles` | 11.0.0 | `vehicles-pack-011` | `40833e10151a7f71` |
| `v20-events` | 2.0.0 | `events-pack-002` | `e2632d931e1e0f7e` |
| `v20-ui` | 4.0.0 | `ui-pack-004` | `0e04e74383980123` |
| `v20-effects-runtime` | 5.0.0 | `effects-runtime-pack-005` | `a1dfa308954675e0` |
| `v20-weather` | 2.0.0 | `weather-pack-002` | `ff22989fcc6648f9` |
| `v20-foliage` | 2.0.0 | `foliage-pack-003` | `1f1b1baab89895de` |
| `v20-weapon-debris` | 4.0.0 | `weapon-debris-pack-004` | `7578778f63998d9f` |
| `v20-worlds` | 6.0.0 | `worlds-pass-006` | `d8245c745f96494f` |
| `v20-tutorial` | 2.0.0 | `tutorial-pack-002` | `849fa1850b5415b4` |

The `v20-ui` hash includes Parity's help-text importer change (`9ea1b6f6`),
so that change should land before or with this one; landing it later changes
`ui-pack-004` without a new version.

## Next

- Consider recording each base package's expected hash beside
  `base-packages.json`, so the importer and the client can say which side's
  copy is not the release's.
