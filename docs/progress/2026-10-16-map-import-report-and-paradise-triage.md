# 2026-10-16 Truthful map-import reports and Paradise limitations

Root granted reporting only from released `5956e3a`: `addon-import/src/lib.rs`
import_map, the existing report.rs wording branch, a new map_import_report test
and this note. No map-item mechanism, renderer, v20_map API, schema or wire edits.
The separate drop correction `d1b0d49255dbdc43ed20923284d013bc43c13461` remains
immutable on its own pushed branch; this reporting atom does not include it.

## Reporting correction

Successful geometry conversion previously forced a `converted` verdict while
retained unsupported objects, mission source outside the literal export and
missing-texture stand-ins were absent from the report. With no per-function
entries, Markdown also asserted that every script function was covered.

import_map now reads its already-generated native scenes/bundles. Each retained
Unadapted object becomes an existing unsupported finding. Particle-emitter nodes
actually exported by exact node identity into map_effects.json are excluded.
Each retained nonempty source section becomes an ambiguous finding, carrying
its existing line range/hash and expressly not counting statements or functions;
the range can include comments and declarations. Stand-ins and unresolved
textures become findings from their existing bundle lists. Any such gaps yield
`converted_with_gaps`. No fabricated function or datablock census is added.
Clean imports keep their verdict and existing Markdown success wording. Reports
with unsupported/ambiguous findings and no function entries instead state that
complete script coverage is not established.

This changes reports on import; existing generated reports are not rewritten.
It does not restore omitted items, original textures or other runtime behavior.

## Read-only Paradise evidence and known limitations

The inspected secondary community source archive has SHA256
`964c38ac8dbf3a1c5ebc1a5c379fedc54b3991ec9e82133b222b21c6a7a1f82c`, exactly
matching the installed native import report. Native daylight bundle SHA256 is
`08064d22c7b79ed83f65073dda475280ef955cd03db3818df55b8aa57c776e2d`.
Reporter package pin, platform and render mode remain unknown. All originals
and installed packs stayed read-only; no original assets are committed here.

- The terrain names sand7, absent from the inspected archive, loose map folder
  and other inspected C: Add-On ZIPs. Its packaged stand-in is uniform 4x4
  RGBA(128,128,128,255), explicitly listed in stand_in_textures. No detail or bump
  image is bound; neither is authored in this mission. The map authors blue
  distance fog. These are material/fog facts, not proof of every reported pixel
  or a recovered original tan texture/missing-texture fallback law. No tan
  replacement or shader/fog fix is qualified.
- Three source rotating, static, pickable gas-sign Items are retained as
  Unadapted nodes 92/97/98, referencing one ItemData shape. Their native positions
  are [2.17395,144.4,249.737], [-531.735,230.386,-229.203] and
  [-840,259.577,-249.1]. All have static/rotate/canPickup set; the first declares
  collideable=1 and the other two 0. gassign.dts is absent from native assets.
  Do not manufacture a static prop or blocking collider from these flags.
- Item rotation/presentation and no-image pickup definition helpers exist, but
  native StaticItem ownership, contact/respawn and identity are brick-bound.
  NativeMap has no map-item admission collection; a normal drop falls and
  expires after ten seconds. Neither is a faithful substitute for these static
  authored Items. Root classified admission as a new mechanism, excluded from
  the current bug-fix release; sign restoration remains an explicit limitation.
- The reporter attributed some/all glow to bloom. The bounded static renderer
  capture does not exercise the App bloom pipeline or establish a bloom defect;
  glow/material fidelity remains unclosed.

Three offscreen native captures completed and were inspected. The roof capture
shows the yellow post without the sign; the far-beach capture shows gray shores
and blue distance fog. A near-beach camera landed inside a building and is NOT
a terrain-color control. No original-game comparison or interactive playtest.
`scene_snapshot` was built on d1b0d492 (unrelated drop-loop correction; renderer
unchanged) with two Cargo jobs. Arguments and hashes remain in ignored
`target/gsf-triage-receipt.json`; images/logs are target/gsf-paradise-*.png and
target/gsf-{snapshot,beach-near,beach-far}.log. Feedback inputs remain under the
main checkout's artifacts/feedback/20261016-gsf-*.png. No assets were copied into
tracked documentation.

## Verification

Cargo uses two jobs, debug=0, incremental=0, CARGO_TARGET_DIR unset. Initial
synthetic terrain fixtures lacked a Sun and failed lighting setup; corrected
before the actual reporting red. Final unchanged tests against released
lib.rs/report.rs: 2 clean controls pass, 1 partial-map test fails because verdict
is incorrectly converted. Fixed files restored byte-for-byte afterwards.

- Final baseline: `cargo test -p bri-addon-import --test map_import_report
  --locked`: 2 passed / 1 failed, expected red;
  `target/map-import-report-baseline-final-source.log`.
- Corrected target: 3 passed / 0 failed, including exact object/stand-in/section
  counts, zero invented function/datablock counts, persisted JSON/Markdown,
  clean-header success and actually exported emitter exclusion.
  `target/map-import-report-green-final.log`.
- Existing `an_old_blockland_map_add_on_becomes_a_map_add_on` waterfall/foliage
  control passes unchanged: `target/map-import-report-waterfall.log`.
- Quick: 2 steps / 4 seconds, 58 passed / 0 failed / 0 ignored;
  `target/map-import-report-quick.log`.
- `python tools/check.py focused --crates bri-addon-import --depth 1
  --tests bri-addon-import:map_import_report`: all 6 steps pass, actual exit zero,
  990 seconds. All-targets warnings-denied Clippy passes. Across 74 result
  summaries: 1,091 passed / 0 failed / 105 existing ignored, including the
  final explicit target's 3 passes and unchanged client 584/76 ignored.
  `target/map-import-report-focused.log`. Production/test SHA256s remained
  unchanged (`target/map-import-report-frozen-source.json`).
- `git diff --check` passes. No full gate, main merge, package regeneration,
  publication or fidelity closure is claimed.
