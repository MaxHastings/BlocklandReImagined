# 2026-10-28 Destruction response regression

Maxwell reports previews4/5 leave shot bricks apparently solid for seconds,
then barely moving/rolling. Buildings also collapse late. Preview3 had some
weirdness, but was substantially better; it is a comparison, not acceptance.
This blocks audio PR381 merge despite the prior technical checks. Latest user
instruction remains merge after the response fix and meaningful scaled movement
checks pass; no additional human approval is needed.

Verified preview3 receipt:089c9400b0808874ecd9557c0a3c4b4755820e91.
Preview4:61afa2ca58a8036ff8e6a1d58c22d96d383c14ed. Preview5 final:
281c9136226e3cd7602fce347b5c8ff9f51fd32f. No audio change explains preview4.
Local recorded sessions93/94 contain TDM Afghan/ACM City load commands with
brick_damage=true. Original recordings and user settings are read-only inputs.
No interactive gameplay is automated.

Post-preview3 hardening added disabled-at-birth rigid bodies, individual
collision windows, a shared128-work budget and half-second angular lookahead.
Repeated queries charge each body for the same surroundings; even cached
terrain was charged as a fresh16-work build. A new motion regression (64
ordinary pieces, empty scene/floor,100ms) failed:15/64 moved more than0.05 units.
Correcting cached-terrain work alone improved it to55/64, still failing the
unchanged56-piece threshold. This demonstrates that body counts/finite poses
were inadequate acceptance checks. Initial new-test compilation corrected a
translation value/reference mismatch; no check was relaxed.

Implement shared, bounded collision tiles and near-term swept preparation so
geometry work is shared among overlapping pieces. Keep existing body/static
limits, map collision and collision-readiness guarantees. Investigate lost
surface direction across delayed cue batches, then measure wall, mast and
building motion/start latency. Final head gate/CI, updated package and merge
remain outstanding. The currently running gate validates281c91362, not this
uncommitted regression correction.

Client correction checkpoint:47 brick-debris tests passed (0.98s). Shared
collision tiles reuse both in-progress and completed world queries. Budget
units distinguish real geometry from cheap cache hits:128 real query operations
per ordinary fixed step and512 per section frame remain the same, as do body,
proxy and static limits. The swept lookahead covers two worst-case frames;
completed tile bounds remain safe for reuse. Cached terrain no longer pays a
build cost repeatedly.

A second negative check proved late blast members were launched into surviving
backing: velocity.z=+11.10 instead of outward. The surface explosion's normal
used to live only in its current cue batch and require an equal tick. A bounded
64-entry recent explosion context now matches delayed members by original
position, force, reach and tick interval. It does not replay the explosion
impulse. Existing authored tiny/direct throw tests continue to pass.

A third negative check found sealed assemblies blocked behind landing-index
preparation:1500/12000 members first moved at116/800ms. Contact witnesses now
prepare during naturally batched member admission. The bounded landing index
continues in flight; complete delivery and GPU readiness still gate coherent
start. No partial section starts and no lost-member/barrier checks were relaxed.

Measured at60 rendered updates/s, real fixed-step Rapier (client-only timing,
not hit-to-delivery or GPU upload latency):
-64 ordinary pieces in empty floor scene:15/64 before,64/64 after at100ms.
-64 pieces beside480 nearby static bricks:64/64 displaced at100ms;90% at50ms.
-512 pieces:512/512 displaced at100ms;90% at33ms.
-2048 pieces:2048/2048 displaced at100ms;90% at83ms (before shared-cache budget
 correction, only171 moved at100ms and1126 at500ms).
-40/1500/12000-member coherent assemblies: actual visible tip motion at33ms
 each; tip displacement at100ms:0.306/0.527/1.877 world units. All members remain
 drawn and the8-body cap remains in these fixtures.

Still required: coverage invalidation on a new solid inside an already prepared
tile, expired/reset explosion context, authoritative proof/complete-delivery
latency, broader focused checks, refreshed preview6 and final-head gate/CI.
The previous281c91362 full gate passed2053s, with creature_think_time_16 failing
then passing alone; Collage/Afghan host budgets4.66/4.11ms passed. That validation
predates this correction and did not establish timely visible destruction.

Authoritative measurement used a struck supporting pillar beneath a shared slab
and64/1536/12032 upper bricks. Before correction, complete65/1537/12033-member
section barriers arrived at1/80/698 host ticks. After bounded coarse-reject
batching and256-record commit scheduling:1/13/100 ticks (120 ticks/s), with the
struck support breaking at tick1. Measured debug-only destruction step means
were0.98/2.98/2.81ms, maxima0.98/5.19/5.38ms; this is not release server/FPS
acceptance. Existing mutation-byte, cue, reservation and real attachment/ground
work bounds remain; coarse metadata rejection is bounded to16 cursor steps per
credit. No new index, permanent topology cache or replicated motion was added.

Review retained recessed/interlocking connections: a cheap candidate reject
must allow attachment cells inside overlapping mesh bounds, not only outer
faces. Added a concrete internal u/d-cell connection case. New client checks
cover solid insertion/removal inside already prepared tile coverage, plus
expired/reset surface context. Actual stock rocket metadata was read from the
accepted weapons pack (force30, radius3, max volume30, floating60); its event
emits unscaled BrickImpact parameters while visual effects carry projectile
scale. Context matching therefore does not equate visual strength to brick
force. Delayed outward ejection passes with effect scales0.5/1/2.

Quick passed730 client units (78 ignored). Simulation first exposed the old
partial-delivery fixture's160-member constant, which no longer spans256 commits.
The fixture now constructs COMMIT_PER_TICK+32 members and still requires an
unsealed prefix, protected restoration deadline, interrupted count and a barrier
with exactly the committed prefix. Follow-up quick passed452 sim units (5
ignored). Focused depth1 is now running the changed crates and direct dependents;
clippy passed. No failing check was waived. A real radius-rocket timing check is
still needed before final packaging; the current server timing isolates support
loss using a direct supporting-pillar hit.

Focused depth1 passed in797s: formatting, warnings-as-errors clippy, all selected
integration tests of five crates (addon-import/chaos/net/sim/stresslab) and730
client units (78 ignored). No failures waived. The final test-only extension
also uses the real stock-radius rocket parameters, not just a targeted support
hit, drains all jobs and records the last Ready barrier across every group:
65/1537/12033 source members complete at3/9/56 ticks (25/75/467ms). The struck
support breaks at tick1/2/2; first section material arrives at tick1/1/4. Largest
case has61 naturally disconnected groups,11897 detached bricks and one slab
split into16 cells, with no capacity hold/rejected job. Debug destruction-phase
means0.30/3.22/5.55ms, maxima0.76/4.46/8.18ms; these exclude the rest of a host
tick and are not rendered-FPS/release performance claims. Both direct-support
and radius modes passed the unchanged response thresholds. This new radius
case is a test-only addition after the broad focused run; final all-target
clippy, integrated gate and CI include it.

Final sustained-motion extension checks translation over500ms and continued
physics readiness, not just first movement.64/512/2048-piece cases all remain
displaced at100/250/500ms. Mean traveled path over500ms:3.368/3.571/2.093 world
units; peak collision-paused bodies after100ms:zero in all three. The targeted
check passed in0.53s. Final client and sim all-target clippy passed without
warnings after these test-only additions. Documentation links/guides passed.
Committing the integrated correction for final CI, full gate and a warm preview6
build. Full human gameplay acceptance remains Maxwell's later playtest.
