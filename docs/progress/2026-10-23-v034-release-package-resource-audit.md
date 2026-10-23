# 2026-10-23 Actual v0.3.4 release resource audit

Maxwell supplied Lizzy Rascal's black-world/blank-menu report, DxDiag, settings,
Add-On health and two rounds of logs. The first run identifies release
`v0.3.4 (c7df4052b)` on Radeon RX550/550 Series with DX12. Its old compiler logger inferred DXC from a nearby DLL; that does not prove wgpu Auto's internal compiler choice. Later startup-only
logs confirm Vulkan and DX12/FXC selection. Vulkan compilation completes; FXC's
log ends before scene-pipeline completion. Neither new run enters a map.
These observations do not establish the cause of the black gameplay world.

## Exact released input and verification

Investigated `E:/Downloads/BlocklandReImagined-windows (1).zip`, whose SHA256 is
`3d6b647e07f8c7adc80b6b9113ecd078cff233c0ed098215296014e45a233f55`.
This matches the actual GitHub v0.3.4 Windows asset's published SHA256SUMS;
release source is `c7df4052bfb2ccfede568f44d4d2f5f854dadfac`.
The 13,162 immutable archive files comprise all 13,161 manifest entries plus
the manifest itself. Every size and hash matches, with no missing or extra files.
All 537 UI image entry hashes match their actual payloads.

Extracted only into ignored `.research/v034-release-audit/release/`, then ran
the exact packaged executable with `--check <extracted-content> <scratch-state>`.
It exits zero and writes an Add-On health report with zero problems. No visible
game, gameplay inputs or original installation was run or changed. This check
loads startup content; it does not exercise host animation admission or validate
final output pixels. Its success does not refute either defect below.

## Confirmed missing-resource cause: package-root paths used under assets

The original host log contains 1,362 missing animation-resource messages, 454
distinct warning lines, 431 distinct relative filenames, spanning 19 Add-Ons.
For all 454 distinct warnings, the corresponding file exists in the release at
`content/addons/<package>/assets/models/<hash>.shape.json`; the doubly nested
`content/addons/<package>/assets/assets/models/<hash>.shape.json` does not.

`crates/net/src/content_identity.rs::body_pose_assets` opens the content index
from `dir/assets/content.json`, then incorrectly passes `dir/assets` to
`read_shape` for index entries whose file is already package-relative
`assets/models/...`. The supported correction is to pass `dir` for that loop,
retaining `contained` path validation. These files were not omitted from the
release and this evidence is not a user installation-corruption diagnosis.
A prior local commit `51807529c` contains that exact correction, but is not an
ancestor of v0.3.4 or checked main `b3afae290`. Do not treat an unintegrated fix
as delivered. A regression must exercise package-relative indexed animated
shapes through actual admission, including containment rejection.

This path is host animation metadata; it does not load map geometry or UI
pictures. Fixing it cannot yet be claimed to fix the black gameplay world.

## Confirmed main-menu image omission

The released UI pack has zero image IDs under `screenshots/` and the archive
has no screenshot images. `App::load` selects main-menu backgrounds from exactly
those IDs; the MM_BG layout has no authored bitmap. There is consequently no
background photo to display, independent of Vulkan versus DX12.

The importer already requests `screenshots/` in `crates/ui-import/src/lib.rs`.
The omission is earlier: `tools/regenerate_content.py::reference_view` copies
only its curated input listing and searches only base/Add-Ons/saves. The listing
`docs/vanilla-reference-files.json` has zero screenshot entries. The designated
read-only v20 reference has ten named JPG screenshots, including IcePalace.
They never reach the sanitized view supplied to the UI importer. The next
supported correction is to qualify those named reference images and include
them in curated input identity/view, preserving exclusion of arbitrary personal
screenshots, then regenerate and verify UI content and the packaged output.
Presence in Maxwell's reference is the product reference, not historical proof
of every official installation. Do not substitute invented backgrounds.

## Map thumbnails are a separate result

All 14 declared map thumbnail entries resolve to actual decodable images with
opaque alpha and match their designated original archive members byte-for-byte. Twelve contain nonuniform image pixels; Construct and Destruct
are solid white/black source thumbnails. Bedroom is an intact 1024x768 picture
and was visually inspected after extracting its exact bytes. This establishes
its shipped asset, not successful GPU display on Lizzy's machine. The absent
main-menu screenshot set does not explain a black Bedroom world or prove the
map-thumbnail display works there.

## Retained evidence and next work

Ignored `archive-audit.json` records exact archive provenance, manifest results,
UI entry checks, all warning-to-file resolutions, reference screenshot identities
and map image statistics. Published SHA256SUMS, the extracted exact release,
packaged check state/log and untouched Bedroom PNG are in the same scratch
folder. Original assets and reporter diagnostics must remain uncommitted.

This increment is an investigation and changes no production source or release
assets. Preserve the two confirmed corrections separately from the unresolved
black-world/thumbnail-display report. Next use a Vulkan Bedroom run with its
actual pixels/log, then instrument the earliest failing stage if necessary;
shader creation and nonzero GPU pass timings alone are not pixel correctness.
