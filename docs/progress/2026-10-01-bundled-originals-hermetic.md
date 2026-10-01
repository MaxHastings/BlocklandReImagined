# 2026-10-01 Bundled originals: hermetic gate tests, one content reading, every platform ships the same originals

batch161 came back from the Windows gate with four failures and a PC bundle
build that stopped. All came from this branch removing the committed Stunt
Plane and Duplicator remakes.

- **Tests use CC0 stand-ins, never the third-party originals.** The
  stand-in plane (`crates/vehicles/tests/fixtures/stand-in-plane`) is now an
  Add-On package, `test_plane`. Its numbers are made up, with a plausible
  layout: two front wheels and a tail wheel. The vehicle camera eye test,
  the motion smoothness run, the vehicle wreck tests, the portal plane, Fill
  Can's paint tests and the Add-On join download test all use it. The
  fresh-checkout test installs it under each bundled original's id, in the
  place where bootstrap puts the original, and spawns it as the default
  plane. Checking that the real bundle installs stays with the Gate's
  separate real-copy runs.
- **Fixtures are byte-exact on Windows.** `.gitattributes` marks
  `crates/*/tests/fixtures/**` `-text`. The wreck test failed on "Item
  resource checksum mismatch" because the checkout had converted the
  model's line endings to CRLF.
- **One content-pack reading.** `tools/content_packs.py` decides which packs
  a content root loads: its own `packages.json`, otherwise
  `crates/package/base-packages.json`, as `PackageSet::load_root` does.
  ci_content, addon_bundle and the Linux and Mac packagers all use it. The
  bundle's `find` and `build` now accept the PC's generated content, which
  has no `packages.json`.
- **Mac and Linux ship the Windows release's originals.** Their workflows
  already took the base game from the release's Windows zip, checked
  against its MANIFEST.json. `python tools/addon_bundle.py from-release`
  now takes the originals and CREDITS.md from that zip too, so the three
  zips cannot differ even if the draft bundle changes in between.

Evidence (cloud):
- `cargo check --workspace --tests` is clean.
- The bundle tests pass 4/4, and the new from-release path is covered.
- The motion smoothness run passes on the stand-in plane: 1 and 0
  corrections across the two seeds.
- sim portals and fill_can, addon-import ports, client vehicles and
  Test-PlaytestPackaging.ps1 all pass.
