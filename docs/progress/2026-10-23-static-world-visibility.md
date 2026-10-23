# 2026-10-23 Measured map-visibility experiments and the next supported correction

Maxwell reports lag in Interdiction. The actual delivered preview uses an
unoptimized dev client; its focused session minute averages 31 FPS / 30.2ms
median. The existing optimized playtest profile is required at the next verified
refresh. Source merges and offscreen diagnostics do not update that preview.

## Rejected prototypes: fewer triangles is not sufficient

The imported main map is unbounded, unlike brick chunks. A conservative
64-triangle range prototype skips 53â€“87% of roughly two million submitted
triangles while preserving original vertices, primitive/transparency order,
hidden props, mutable/shared geometry and offscreen shadows. Pixel/negative
checks and focused tiers pass. But actual quiet optimized offscreen frames get
slower. Ordered indirect runs reduce reported calls without fixing the cost.
A second immutable compact-index cache retains one material draw, but camera
motion rebuilds/uploads several MB per frame and also regresses.

The final matched comparison uses the same optimized profile, content, four
requested poses, 1280x720 target and diagnostic harness; only the main upload
call differs. Each view warms 20 frames then measures 120. Camera sweeps apply
0.005 radians per frame and restore the heading afterward. No compilation or
other tests run during measurement. Median / p95 synchronized-frame ms:

| View | Complete original upload | Compact-index prototype |
| --- | ---: | ---: |
| Room, steady | 4.844 / 5.374 | 5.179 / 5.611 |
| Room, sweep | 4.905 / 5.734 | 8.211 / 8.785 |
| Forest, steady | 6.703 / 7.269 | 6.716 / 7.198 |
| Forest, sweep | 7.094 / 7.696 | 12.473 / 15.189 |

The room sweep uploads 1,116,147,108 index bytes over 120 measured frames;
the forest sweep uploads 484,356,744. Earlier split draws cost 10.325ms room
and 8.989ms forest medians, against 4.704/5.378ms controls from that earlier
run. These synchronized frames include update, recording/submission and GPU
wait, without image copies or PNG encoding. They are not interactive FPS.
GPU pass samples are unavailable on this headless device, not zero cost.
Do not compare two runs' different live NPC states as identical workloads.

Neither static-range prototype is committed, merged or delivered. Their exact
source patch/module, consumer hashes, failures and checks remain ignored under
`.research/pipeline-integration/rejected-static-visibility` and the matching
`static-world-*` logs. The ordinary complete-map upload is restored. Full-map
spatial visibility remains a future dependency requiring a faster measured
approach; no larger compute/occlusion framework is justified by counts alone.

## Active narrower increment: per-view blended-instance rejection

The existing renderer already submits opaque instance ranges once. Keep those
cheap ranges intact. Faded/blended placements currently require independent
transparency sorting and frequently individual draws. Before building those
draws, the candidate tests each complete transformed model extent against the
existing view frustum. Rotation/nonuniform scale are included. Sky/cloud and
shader-displaced water retain their full path; geometry with shader BrickFX
also stays conservative. Extents are qualified against the existing shared
geometry revision, so stale batch views cannot hide later posed geometry.
The original instance buffer is never compacted or rewritten for visibility:
other cameras and shadow passes retain all placements. Global transparency
ordering and the separately checked adjacent-instance coalescer remain intact.

This extends the existing world-pass mechanism, with no content/map names,
guessed offsets, gameplay policy or additional asset schema. The seam owner is
root. Actual steady/sweeping timing below supports focused/gate integration.

The retained original-submission baseline fails the new offscreen-copy check
(four triangles instead of two). Independent CPU-transformed geometry supplies
pixels, including a partially visible rotated/scaled model whose centre is
outside the frustum. A second camera must show the previously culled instance.
Opaque ranges remain one draw; shader FX remain on the conservative full path.
Shared posed geometry is checked against an independent moved oracle.
An initial revision snapshot incorrectly assumed zero; the existing generation
has unique high bits, so snapshots now read the actual fresh revision.
An initially proposed faded-caster shadow check failed because the existing
shadow policy deliberately excludes faded casters. Keep that policy; an opaque
behind-camera caster remains the valid visible-shadow control. The final exact-source render, quick, clippy and actual-map timing below pass;
full integration remains pending.

## Reusable capture quality

A fixed spawn-delay capture accepted a dead/spectating Pipeline player. Reject
that comparison. The tool now waits for living deployment and refuses a dead
or misplaced player (two-unit tolerance). Four qualified Pipeline views pass
and were inspected: office, two control windows and yard. The overbright/stippled
yard and first-frame remote weapon before its deferred body upload remain open.
Five Interdiction views retain foliage/cellar opening and dark underground rooms.
The appliance/wall fixture remains partly occluded, not isolated BSP evidence.

The capture diagnostic measures 30â€“240 synchronized frames, optionally sweeping
the camera, through normal App update/world/HUD recording without readback.
Private tooling restores heading and checks the player before and after.
No native window or user input is automated. Originals remain read-only.
Ignored map evidence lives in private integration scratch `capture-static-world-*`,
`measure-static-world-*` and `pipeline-shared-check/` counterparts. Normal
lighting, remaining blockers, M249 frames, NV, NPC fairness and complete mission
acceptance remain open in the private handoff and single execution plan.

## Measured blended-instance candidate

The narrower candidate improves every retained steady/sweeping comparison.
Same optimized profile, content, pose/heading specification, 20-frame warmup,
120 samples and no competing compilation/tests. Median / p95 synchronized ms:

| View | Original complete upload / sorting | Per-view blended rejection |
| --- | ---: | ---: |
| Room, steady | 4.844 / 5.374 | 4.469 / 4.821 |
| Room, sweep | 4.905 / 5.734 | 4.467 / 5.070 |
| Forest, steady | 6.703 / 7.269 | 6.183 / 6.621 |
| Forest, sweep | 7.094 / 7.696 | 6.459 / 6.883 |

About 8–9% lower medians in these diagnostics, not a promised interactive FPS
gain. Room final-frame submissions fall from 2,969/2,965 to 760/781; forest
from 1,793/1,799 to 772/917. The candidate rejects 1,273–2,422 offscreen
blended batch-instance draws. Roughly 1.94 million main-map triangles remain;
this corrects command/sorting work without the rejected map-index rebuilding.
Source lighting and live NPC state are separate. The two steady frames were
inspected; foreground trees/ferns and room geometry remain, and the room is
still too dark. Passive guard fall deaths remain a disclosed mission failure.

The complete final render suites pass 18 persistent-scene checks (three ignored)
and five shadow checks, including the opaque behind-camera shadow control.
Quick passes in 86s and all-target client/render clippy in 83s. Focused checks
and the final Pipeline consumer run are active. Full gate, Windows checks,
source integration and an actual optimized preview receipt remain required.

Focused passes in 512s across ten dependent crates on the combined candidate
(base bddc1cc0a). Four final Pipeline pose/alive guards pass. Its room/yard
captures retain the prior dark/overbright/stippled appearance problems; this
increment does not relight the map or qualify NPC behavior. Complete original
map submission is retained. The new performance mechanism only avoids
independent offscreen blended draws and preserves their authoritative data.
