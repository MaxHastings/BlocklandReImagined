# 2026-10-15 Contact material profiles and sprite initial angle

Development checkpoint 914d78a4 preserves the prior image/vehicle/seat/haze work.
It is not a release: focused checks remain incomplete after a sized-join bandwidth
failure. The standalone retry also failed, at 1,755,053 B/s through the unchanged
12,500,000 B/s link. That network issue remains open for release verification.

## Current candidate after that checkpoint

ParticleVariation has optional initial_spin_degrees: sample once separately from
angular velocity. A zero/unset range leaves legacy random sampling unchanged.
Private original center flash, smoke and casing groups bind CE's original
random_initial_angle flag. Content/fx-runtime quick checks pass, and the existing
seeded group regression now also verifies nonzero, deterministic birth rotation.
Bounded rifle captures use actual render adapters and shared depth, in local
artifacts/rifle-initial-angle-inspection. Flash is hidden by the occluding wall;
source dimensions/composition, blend semantics and original comparison remain open.

Decal now has complete per-surface profiles: original texture/blend, seeded
size/lifetime/rotation, authored fade duration and explicit null/no-profile policy.
Client resolves the style from the existing host cue id. Legacy texture overrides
and the stock one-second fade remain default. Profile textures enter ordinary
content-reference validation. Five weapons impact tests and five client decal unit
tests pass. No extra network fields or game-specific engine branches were added.
Focused checks for this new batch remain pending.

## Private source binding and missing collision identity

Private effect conversion previously chose the metal bullet hole globally.
It now follows each impact effect's negative-incident decal part, preserving
different source marks/ranges and omitting marks where none is authored. Other
locations, such as a blood mark along the incident ray, are not substituted as
surface holes. Lifetime/decay interpretation and actual surface blending still
need rendered original comparison; there is no fidelity acceptance here.

The private Blood Gulch conversion had no surface mapping and discarded original
collision material IDs. Merely binding different effect textures could therefore
never make map hits use their correct materials. A private read-only postprocessor
now restores the source collision identity. It refuses unless every collision
vertex matches the converted geometry within .0001 native units and the oriented
triangle multisets match exactly. It does not guess from nearby render faces.

Original level collision: 5940 triangles, 2433 sand, 1382 stone, 1556 metal,
222 plastic and 347 without an authored material. Original scenery collision:
1794 triangles, 1258 stone and 536 wood. No unknown material was guessed. Vertices
and oriented triangles are unchanged. Collision-only material identities use
existing image bindings and are never drawn; render/physics identity stays separate.
Native runtime loading/count comparison and all-private manifest checks pass
(local artifacts/v031-map-surface-runtime.log and v031-private-manifests.log).
Dynamic map loading still reports missing prepared source lights and uses live
sun/ambient only; this does not close the map-lighting audit.

Original map data, raw reader logs and source snapshots remain in ignored research.
Private metadata/art and conversion stay private. A secondary research renderer was
inspected for context only (magellanicus at 5b96247c); it contains explicit FIXME
approximations and is not an original-fidelity authority. No code was copied into
the engine and no dependency added.

No latest playtest binary/content installation or public release occurred. All eight
requested phases, source effect/contact/material gaps, Warthog mechanisms, HUD,
expansion and final package/gates remain open. Do not treat this checkpoint as ready.


## Swept cosmetic structure contacts (subsequent candidate)

Particle definitions now optionally carry restitution, per-contact tangential
friction, a rest threshold and contact/rest death policy. EffectsWorld accepts a
world-owned sweep callback. WeaponEffects connects it to Building's ordinary
map/terrain/brick solid-segment query. No damage, rigid-body mutation or game-specific
classification enters this mechanism. Source-attached flash particles are excluded.
A shared 32768 query budget and 128 substeps per point bound work; unresolved motion
waits instead of tunneling. Contact emitters cannot sleep/refill without collision
history. Existing non-contact stock integration is unchanged.

Synthetic regressions verify reflection at the hit time, remaining frame motion,
floor non-penetration, supported rest, distinct contact death and invalid definitions.
The first runtime run passed 23 tests (3 existing ignored cases). Compilation of
client/test consumers passed. First clippy found an accidentally used f32::clamp;
corrected to the repository's bri_console::Clamp::clamped. Follow-up quick/clippy
and actual-map rendered inspection remain pending at this entry.

Private conversion binds source structure-contact flags, elasticity, surface friction
and death flags. The .05 native rest threshold is explicitly an adaptation, not
recovered CE behavior. Exact original drag/contact timing, contact sounds and
animation-at-rest remain open. Do not call the particle trajectory reconstruction
complete merely because these mechanisms execute.

Earlier actual-map mark inspection: collision/render surface gaps were within
5e-6 native units at chosen sand/stone/metal faces. Render comparisons with marks
removed changed 1451/51/182 pixels respectively at the original diagnostic camera.
Marks are present and depth aligned; their small visible footprint depends on the
source texture padding. No arbitrary enlargement was applied. Closer actual-map
captures and particle snapshots are being generated. Lighting remains live-only;
prepared source lighting is absent. Warm source fog was inspected, but has not had
a matched original camera/falloff acceptance.


## Subsequent validation and source inspection

Quick checks passed all 12 selected steps in 279 s. The new runtime contact
regressions now pass 24 tests (3 existing ignored), including the shared query
budget and animation frame freezing at the exact rest time. The latter exposed
and fixed a duplicate live-age texture lookup that had bypassed the paused clock.
Full fx-runtime tests, including GPU blend/depth checks, passed before that final
clock correction; the runtime target passed again afterward. Selected all-targets
clippy passed in 30.96 s. Native animation-at-rest and physical sprite spin now
stop at rest; independent color/size/lifetime fades continue.

Actual Blood Gulch contact captures use original groups at .017/.1/.3/.7 s. Metal
had 3 contact hits by .3 s and 14 by .7 s. Sand/stone/metal effects differ, with
shared map depth. The .1 s metal image has a conspicuous hard billboard/floor
intersection and coarse bright art at close range: no matched original acceptance.
Do not hide this remaining presentation problem behind contact regressions.

Private sound research found the particle impact_effect path names a material
response table, not a direct sound. Its impact row chooses single-shell dirt/
metal audio or silence by material. Original references now lead to 60 recovered
sound permutations, with source gain/permutation metadata retained; impact profile
volumes bind original gain rather than the earlier hardcoded 1. Surface-dependent
particle contact playback, variation, source class distance defaults/promotion and
listening comparison remain open.

## Network impairment harness root cause

The resumed focused check reached 27 steps/1043 s and stopped at a new diagnostic
regression, latency_without_jitter_preserves_datagram_order. The existing relay
schedules every datagram as an independent sleeping task. With zero jitter and
20 ms latency on a 12,500,000 B/s link, 32 datagrams arrived in this order:
31,5,0,15,16,8,17,1,18,4,9,19,10,20,7,2,11,21,6,3,12,22,13,26,27,14,24,25,28,29,23,30.
This proves unintended reordering; it does not prove it is the entire sized-join
bandwidth failure. No throughput assertion or rate was reduced.

The candidate now uses one deadline-ordered scheduler per direction. Absolute
serialization deadlines retain configured rate/queue/latency/jitter/duplicates;
a bounded inbox/heap prevents uncontrolled task creation, and 16-datagram yields
let socket readers drain coalesced timer bursts. Workers are aborted with the
link. The no-jitter order regression and existing pacing/buffer-drop test pass.
Sized-join verification against the unchanged floor is in progress; focused
remaining targets, final gate and exact packaged release are still required.


Sized-join with the ordered relay passed: 23.0 s join, 21.7 s download at 6.9 MB/s
(58% of the unchanged modeled link), longest progress stall .225 s, 152059
datagrams forwarded and zero router-buffer drops. This is a correction to the
verification harness, not a claim of a production transport optimization. The
14 remaining focused crate targets are being resumed directly, starting at net;
prior successful targets are not restarted without a relevant source change.
