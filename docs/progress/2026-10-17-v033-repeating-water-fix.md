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

## Final worker acceptance and local integration

Worker headbfcba5dc993688f15d64d3203aaf9e4ffff405ee, direct parent651,
is clean, pushed and independently remote-verified. Root reviewed the final17-path
diff, including the additionally granted tiny app/scenery.rs accessor. Source-free
renderer7 and actual App lifecycle/scenery2 cases pass; quick671/0/76existingignored
and full normal focused15steps/951s/1250/0/120existingignored pass, with both changed
all-target Clippy checks green. The first forbidden scalar-clamp lint was corrected
with the existing repository helper and full tiers rerun; no waiver/new ignore.

Root independently checked all50 durable acceptance archive hashes and inspected
the actual Slopes old/corrected render pair. Final Slopes34copies have a36,444,712
byte logical bound; Slate Sea/Storm84copies across two eyes retain both layers
within179,468,496 bytes summed conservative individual bounds. No stationary
mask/image/geometry reuploads. The bound includes retained logical resources and
one serial construction peak; it excludes driver overhead/deferred reclamation
and is not measured VRAM/RSS. Validate-all/evict-all/admit prevents cross-group
temporary ownership excess. Accepted custom/NO_WATER hides native water; missing
choice restores it. CLI controls are native42/custom0/none0/unknown42, with unknown
output byte-identical to native. Source installations and delivered previews are
unchanged. Full original hole/underwater fidelity and the reporter's exact location
remain unqualified; authored repeat gaps remain intentional.

Root merged the immutable worker head conflict-free into its integration branch
at1f0d0ce81074df7306fbe077a33c57a48843f810. Source/runtime is the exact tested
worker content (which already includes651); this is local integration only.
The existing PR187 previously passed all six required checks on9bad4cad before
water integration. That earlier CI does not cover this new candidate. Updated
candidate CI and one full integrated gate remain required before main. No public
v0.3.3 publication is authorized or claimed, and no installed playtest is replaced.
