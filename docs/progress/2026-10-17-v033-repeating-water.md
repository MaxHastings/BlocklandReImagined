# 2026-10-17 v0.3.3 repeating water

Final worker validation is green: quick 671/0 and full normal focused 1,250/0;
root integration, full gate, platform checks and publication remain separate.

## Scope and evidence

Maxwell explicitly requested the repeating-water fix for v0.3.3. Root reviewed
and granted the renderer/client path set after a clean source-free baseline.
This lane starts at public `651efb2ff361579dde657b54d4023b2ee313bab7` on
`codex/v033-repeating-water`, reusing the existing shared-engine-seams worktree.
No other owner's checkout, installed preview or original installation is changed.
The full alpha contract and interactive acceptance remain open.

The original read-only Slopes water has repeat terrain enabled. Native query
`Water::footprint` repeats indefinitely, but the old static renderer drew only
indices -1 through 1 on both horizontal axes. The screenshot does not supply an
exact location, so this confirmed defect does not prove which particular authored
footprint/gap appeared in the image. Slopes' native water footprint is 1024 by
1024 inside its 2048-unit period: its intentional gaps must remain.

Before production edits, a temporary invented native-bundle test loaded both
compatibility and Dynamic visuals at the exact base. `Water::footprint` covered
[336,-272], repeat [5,-4] of a 64-unit fixture; the loaded geometry did not.
`cargo test -p bri-render --test __repeating_water_diagnostic -- --nocapture`
exited 101 with that assertion (0 passed, 1 failed). Both loaders completed;
the assertion tested compatibility output first. The temporary file was copied
to ignored `target/repeating-water-baseline.rs` and removed; the checkout was
clean when root reviewed the checkpoint. Log: `target/repeating-water-baseline.log`.

## Implementation

Repeated water now has a separate camera-following render group; finite water
remains in the map static scene. Visible copies are the exact authored rectangles
intersecting the draw-distance discs for the union of player and secondary eyes.
Surface UVs retain [kx,-kz] offsets; local depth-mask UVs and existing world-space
waves remain independent. The shader and material/instance ABI are unchanged.

New copies sample the existing depth-mask algorithm at their actual world
coordinates. This preserves current TerrainField semantics: primary empty squares
are not propagated into solid repeated terrain, and finite terrain does not
invent distant water coverage. This is an intentional correction beyond draw
extent; it does not establish full original per-hole/underwater fidelity.

Geometry stays cached until it leaves the current view union. Equal depth-mask
pixels share material palettes; a dedicated water-owned TextureCache shares the
source textures and prunes evicted masks. No avatar/world shared cache is pruned.
Stationary and unchanged-copy-set frames build no masks or geometry. Retained CPU source images, active GPU geometry/textures/masks/materials and the maximum serial construction peak enter the checked 256 MiB map-wide bound before GPU allocations. The serial peak includes CPU geometry Vec growth, cloned source images and the upload helper CPU mip chains. Masks reserve conservative padded mip storage.
Unsupported coverage/index/coordinate requests fail explicitly; lists are never
silently truncated. Driver-private allocation overhead is not measured VRAM.

App CPU/GPU ownership follows terrain through asynchronous map adoption, map and
session teardown, graphics adoption, device loss and lighting-source reload.
Scene snapshot, lighting probe and performance probe retain water draws.

## Early verification

All cargo work uses two jobs, RUST_TEST_THREADS=8, debug=0, incremental=0 and no
CARGO_TARGET_DIR. The first all-target client check passed. The six invented
renderer regressions passed, including far/negative coverage, finite controls,
gaps, global/local UVs, actual per-copy terrain masks, actual far-secondary-eye
GPU output, repeated cache eviction and stationary upload counts. Resource
accounting and whole-map rejection checks were subsequently tightened and will
be verified again with final sources.

The first App test compilation caught an immutable test predicate trying to use
the renderer's mutable readiness method. The test now inspects readiness after
the ordinary frame; no runtime change or assertion waiver was used. App lifecycle,
real Slopes bounded rendering, quick and complete normal focused tiers remain
pending. Final commands/results and immutable source hashes will be recorded
before handoff. Root owns integrated gate, platform builds and publication.


## Final targeted evidence

Root review found a cross-group admission risk: later waters could still retain
outgoing resources while an earlier water expanded. Admission now validates all
selections, evicts/prunes all groups, then admits new groups with a per-admission
bound check. A source-free transfer swaps [1,400] and [400,1] visible copies;
both valid endpoints fit, but combining the old large group and new large group
would exceed the cap. Its excessive union refuses before uploads and preserves
the prior visible state. Temporarily restoring sequential eviction produces the
expected red (exit 101, projected 313,690,652 bytes); final source restored
byte-for-byte. Logs: repeating-water-transfer-baseline.log and its receipt.
The corrected seven renderer regressions pass (7/0, no ignores).

The App lifecycle test passes (1/0), including real native async map adoption,
same-device MSAA resource adoption, actual Dynamic/compatibility loader switches,
a second headless device, tracked admin map change and disconnect. An initial
driver sent an untracked map request and was stopped after diagnosis; the retained
log is repeating-water-app-untracked-request.log. Using Core::admin_request, the
same tracked request path as the menu, resolves it without a production change.

Read-only actual Slopes loader/GpuTerrain/GpuWater output was rendered and both
images inspected beyond the old fixed patch. Repeat [5,-4], target
[9874,90.19020080566406,-7546], camera [9874,220.19020080566406,-7366], secondary eye
[-14702,220.19020080566406,13114]. Restored water changes 102,512 pixels. The old
extent control builds the old nine copies; all are beyond this camera's far plane,
so the copy-mask differences near the origin do not affect the comparison. This
is distinct from the exact unmodified-base loader red above.

Final Slopes selection: 34 copies/masks, four cached textures, logical resource
bound 36,444,712 bytes including serial peak. A repeated update makes no mask,
texture or geometry uploads. First accounting had charged serial construction
for every retained tile (69,821,624 bytes); that receipt is preserved separately.
Actual stock Slate Sea and Slate Storm each retain both layers across origin and
a far secondary eye at radius 4000: 84 copies, six cached textures, sum of their
individual conservative bounds 179,468,496 bytes (summing individual construction
peaks is stricter than the map's single-peak admission). Both actually draw and
stationary updates reupload nothing. No stock cap exception is needed for these
ordinary cases. This is an active logical resource estimate, excluding driver
overhead and deferred GPU reclamation; it is not measured VRAM or RSS.

The ignored probe temporarily appends a private test to the owned new test target,
then restores that target byte-for-byte in finally. Its sources, JSON receipts and
PNGs remain under target/; original E Map_Slopes.zip and native bundle.json hashes
are unchanged. No private asset/test or generated image is committed. Final quick
and full normal focused closure (no depth limit), all-target warnings-denied
Clippy and the explicit public renderer target are the remaining steps.


## Existing scenery policy and tool coverage

Final consumer review identified the Environment window's existing flat-map
water hide/restore policy. Root additionally granted only app/scenery.rs for a
small derived visibility accessor. The repeated group now follows exactly that
policy: valid custom water and NO_WATER evict native copies/masks; native or a
missing choice restores them; non-flat maps keep native water. Two actual App
regressions pass, covering the accepted settings through ordinary scenery updates
as well as the earlier lifecycle. A controlled pre-correction GPU binding fails
immediately after accepted fixture-water (native ownership true, expected false),
exit 101; final source restored byte-for-byte. Source-free predicate is unchanged
in the final enabled case. See repeating-water-scenery-baseline.log and receipt.

The snapshot tool now explicitly applies the same known-choice replacement to
both native static and repeated water (its old append did not itself strip native
water). Actual rebuilt CLI controls on read-only Slate Sea: native 42 copies,
custom 0, NO_WATER 0, unknown 42 with byte-identical output to native. Custom water
visibly draws and differs from NO_WATER. Another actual CLI image shows the far
Slopes restored water at the probe pose. Images inspected; native bundle and
environment-pack index hashes unchanged. Logs/PNGs and hashes are in
repeating-water-cli-*. This remains offscreen evidence, not interactive acceptance.

First quick before scenery: actual exit 0, four steps/93 seconds, 670 passed,
zero failed, 76 existing ignored. Preserved as quick-before-scenery.log. The final
exact-source tiers below supersede that partial checkpoint.


The first full focused invocation stopped after three steps/17 seconds because
this repository forbids scalar f32::clamp: use bri_console::Clamp::clamped. Three
finite-bound scalar calls now use that existing helper. No lint suppression was
added; an unnecessary too-many-arguments annotation on the seven-argument private
helper was removed as well. The repaired client all-target Clippy check passes.
The first focused log is preserved as focused-first-clamp-lint.log. Final quick
and the full normal closure are rerun after this repair.


## Final frozen-source validation and handoff

- `python tools/check.py quick --crates bri-render,bri-client`: actual exit 0,
  four steps/93 seconds, 671 passed, zero failed, 76 existing ignored.
- `python tools/check.py focused --crates bri-render,bri-client --tests bri-render:repeating_water`:
  actual exit 0, all 15 normal steps/951 seconds, 93 result summaries,
  1,250 passed, zero failed, 120 existing ignored. No depth limit, test exclusion,
  new ignore or waiver. Both changed all-target warnings-denied Clippy checks pass.
  The complete normal dependent closure includes Add-On import, chaos, foliage,
  foliage import, effect import/runtime, weather/import, client/render units and
  the seven explicit renderer cases. Both enabled App lifecycle/scenery cases pass.
- One final read-only original-map probe after the lint repair: 8/0, including
  all seven public cases plus the local actual Slopes/Slate Sea/Slate Storm check.
  The same 34/84-copy resource counts, images and zero stationary upload results
  hold. Temporary private test restored byte-for-byte; all 16 final Rust hashes
  match `repeating-water-frozen-source.json` before and after final verification.
- Exactly 17 granted changed paths, including the one additional scenery path,
  one public source-free renderer target and this note. `git diff --check` passes.
  No manifests/lockfile, shader/ABI, authored content/save schemas, authority,
  physics, network, original installation or installed private preview changes.

Compact final tier metrics, exact source hashes, baseline/transfer/scenery reds,
read-only source hashes, local probe sources, CLI and real-map receipts and PNGs
are preserved outside disposable build output at
`C:/Users/Maxwell/.codex/visualizations/2026/10/16/01a1424d-cf6a-78d3-8d02-df8017d87507/repeating-water-v033-evidence/`.
Its MANIFEST.json records each copied file's SHA256 and final source head. These
private QA files and source pictures are excluded from public commits. The
compiled cache and original target receipts remain for concrete integration
regressions; no cleanup, main push, gate or publication is performed by this lane.

Remaining limits: exact reporter position/revision was not supplied; intentional
water gaps can still be valid. Current TerrainField hole/depth semantics are
qualified, not full original per-hole/underwater fidelity. The 256 MiB cap is an
active logical resource estimate, excluding driver overhead/deferred reclamation.
Explicit unsupported coverage/precision/device-limit errors remain; no silent
truncation. Maxwell owns interactive acceptance and the wider alpha is incomplete.
