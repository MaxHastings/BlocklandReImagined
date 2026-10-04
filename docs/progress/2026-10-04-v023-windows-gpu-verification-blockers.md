# 2026-10-04 v0.2.3 Windows offscreen verification blockers

## Frozen candidate and failed check

The gameplay candidate `55b1f058aaa58f7eae512ef2638adf5a95d5aa21` passed the
full local content-backed gate (344 test binaries, 685 seconds) and all three
platform packaging workflows. A private draft staged those archives while
Windows CI ran; nothing was published or merged on the strength of packaging.

[Windows CI 37178912923](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/37178912923)
completed with two offscreen verification failures. Format, tool tests, build
and strict Clippy passed. The synthetic brick-family audit returned
“The requested Wait timed out before the submission was completed”; its other
two synthetic layouts passed. The combined 20-test lighting binary reached its
unchanged 600-second binary deadline after eight completed tests, while running
`map_lamps_cast_live_shadows_in_unified_modes_by_shadow_quality`.
No gameplay assertion failure was reported in those targets. This does not
establish that either problem is harmless or that Windows coverage passed.

## Test-only corrections

The source wind-down remains in effect. Only these demonstrated release
verification blockers are being corrected; production gameplay is unchanged.

- Split the lighting integration suite into eight groups by behavior, sharing
  test-only fixture builders. All 20 original test names occur exactly once;
  their bodies, comments and test/ignore attributes are byte-identical. Shared
  helper bodies only gain `pub(crate)` visibility. The real-map ignored test
  and CI's dynamic target discovery retain their existing coverage.
- Render synthetic brick audit images at 640×400, preserving the original
  aspect, camera, FOV, geometry, materials and FX. The native v20 reference
  comparison remains 1280×800. Dimensions consistently drive target/depth,
  readback, PNG and layout metadata; both row sizes retain GPU alignment.
  This bounds synthetic raster/readback work, but cold shader compilation
  remains a possible contributor that the Mac cannot diagnose.

Neither the 30-second GPU wait nor the 600-second binary deadline changed.
No assertion was relaxed, test skipped, known-failure waiver added, or runtime
logic modified. GPT-6.1 Sol High agents implemented the two test changes; a
separate readonly GPT-6.1 Sol High review verified the complete inventory and
dimension propagation against `55b1f058`.

## Focused evidence and remaining gates

All 20 lighting tests passed serially on this Mac, including the original
ignored real-map check, with `CARGO_BUILD_JOBS=2`, `RUST_TEST_THREADS=1` and the
actual content folder. Each group took 4.40–7.91 seconds. Selected-target
Clippy with `-D warnings` passed. These Mac timings do not establish software
GPU timings on Windows.

Receipts: `/tmp/bri-v023-windows-ci-failed.log`,
`/tmp/bri-v023-lighting-partition-tests.log`,
`/tmp/bri-v023-lighting-partition-clippy.log` and
`/tmp/bri-v023-lighting-partition-inventory.txt`. Root preserves them in the
ignored release evidence folder. Brick audit focused results, the new full
gate, Windows CI and rebuilt exact-source platform archives remain pending
at this entry. Publication requires those gates; the earlier private draft's
archives will be replaced only by verified final artifacts.

The six brick audits also passed, including all original native comparisons;
focused strict Clippy and formatting passed. Actual PNG/layout dimensions agree:
all three synthetic images are 640×400 and native images remain 1280×800.
All images contain non-background geometry, and bounded visual inspection
confirmed the synthetic fixtures remain visible. Receipts:
`/tmp/bri-v023-ci-brick-audit-local.log`,
`/tmp/bri-v023-ci-brick-audit-clippy.log` and
`/tmp/bri-v023-ci-brick-audit-images.log`. This is local fixture evidence;
the final full gate and Windows check still decide publication.
