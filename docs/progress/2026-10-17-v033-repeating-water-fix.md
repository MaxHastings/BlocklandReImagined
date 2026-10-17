# 2026-10-17 v0.3.3 repeating-water correction started

Maxwell explicitly asked: "Yeah lets fix the repeating water bug for v0.3.3 too".
This resumes a bounded vanilla renderer correction alongside the already-delivered
private AA sprint/aim preview. See the previous repeating-water report for source
and screenshot evidence. No release publication is authorized by this request.

Existing core/maps owner Coordinate shared engine seams is assigned a minimal
path and failing-predicate proposal from runtime651efb2f, preserving earlier
branches and work. Root owns review/integration. No new agents or checkout are
created. First proposal is read-only until exact scope is reviewed; no speculative
water physics or content-identity change is part of this correction.

Acceptance: an actual loader/render or ordinary App regression observes visible
repeating water well beyond the old3x3 patch, including negative coordinates;
origin/finite-water controls, authored gaps, masks and texture/wave continuity
are preserved. All relevant views and map/device/lighting lifecycle transitions
must use the same bounded camera-following geometry. No per-frame full scene or
texture upload, arbitrary giant mesh, or silent truncation of visible water.
Rendered extent is the diagnosed defect; original per-terrain-hole acceptance
and underwater shader fidelity remain separately qualified rather than being
claimed by a successful extent correction.

Required validation is targeted red/green, quick and full normal focused checks
for changed crates and dependents with Cargo2jobs/testthreads8/targetunset, then
root reviews the immutable source and ordinary consumer evidence. One integrated
candidate gate and Windows checks remain required before any future main push.
Original installations and delivered playtest folders stay unchanged. All other
AA/Halo/GMod research remains parked absent new direct user instructions.

## Reproduction and implementation assignment

The core/maps owner reproduced the actual native-loader mismatch on exact651:
synthetic repeating water physically covers point[336,-272], repeat[5,-4], but
the static mesh has no triangle there (cargo exit101). Both compatibility and
Dynamic loaders ran before the failing assertion. Scratch source/log are retained
in that owner's target/repeating-water-baseline.rs and .log; production was
unchanged at this checkpoint.

Root approved implementation on codex/v033-repeating-water: render water_scene,
scene_loader and minimal scene texture-cache retention; necessary client scene,
GPU, load/session/map, rendering and lighting lifecycle plumbing; scene_snapshot,
lighting_probe and perf_probe; focused render/client regressions and the owner's
progress note. No shader/instance ABI, content/physics/network/import schema or
additional mechanism is granted. Further paths require review.

The bounded design follows all active eyes, preserves copy UVs and finite authored
footprints, computes depth masks at each newly visible copy's coordinates, shares
surface textures and equal masks, and evicts resources outside the view union.
Any texture-cache pruning is confined to its own water cache. Checked resource
and coordinate bounds must fail explicitly rather than silently omit coverage;
the proposed active geometry/mask cap is256MiB, to be measured against real maps
and device limits. Stable visible-copy sets must cause no repeated uploads.

Read-only native Slopes measurement: footprint1024x1024, period2048, so intentional
gaps remain. Each copy has4225 vertices/24576indices and a256x256RGBA mask, roughly
0.63MiB before mip/descriptor costs. A typical4000-unit eye needs about20copies.
Actual source/GPU validation, lifecycle tests and full focused checks are pending;
no fix, integrated pass or release is claimed.
