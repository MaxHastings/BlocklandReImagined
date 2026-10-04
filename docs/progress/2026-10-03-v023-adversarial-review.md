# 2026-10-03 v0.2.3 independent adversarial review

Read-only production review on `codex/v0.2.3-proactive-hardening`, including
committed `e262e455`, the working changes and the pending blueprint/culling
patches. Root authorized this report and a temporary test-only mounted
rollback patch. No production or existing tests were edited by this lane;
no Cargo, GPU, worktree, visible game or interactive input was used. All
findings below are source-level counterexamples, not executed reproductions.

## Integration blockers reported to root

1. **P1: a sliced creator job retains revoked authority.**
   `crates/sim/src/session/blueprints.rs:839` clones the initiating actor;
   `blueprints/plant.rs:69` retains it across ticks. Every later check and
   plant uses that clone. `session/trust.rs:535` refreshes only live peers,
   and `session/copy_jobs.rs:241` resumes work without renewed admission.
   Start a large ordinary PlaceBlueprint into a trusted foreign group, then
   revoke the guest's trust while it is still checking/placing: remaining
   mutation remains authorized by the old clone. The same pattern occurs in
   copy edit `Editor` (`copy_edits/jobs.rs:20`), SuperCut/Fill
   (`copy_edits/box_jobs.rs:57`, `:251`) and captured undo actors
   (`undo/jobs.rs:39`, `:433`). Copy edits even document that revoked trust
   leaves bricks unchanged, while their stored actor prevents that behavior.
   This is a v0.2.2 foundation defect, not a new bot or Gravity Gun exemption.
   Root assigned current-authority correction and ordinary mid-job revoke
   regressions to the NPC/fidelity lane. Preserve the original entrypoint's
   build/trust/float policy; do not invent new alive/inventory requirements.

2. **P1: required support can disappear after preflight.**
   `blueprints/plant.rs:136` accumulates support during Check, then `:178`
   invokes `plant_try(..., true)` during later Place slices. Remove the sole
   real grounded anchor after it was checked but before placement: the job
   can publish an unsupported column because free placement trusts the old
   support result. Pending GroupSupport retains the same stale roots while
   doing its graph work. Root assigned support invalidation/freshness and a
   real command-path anchor-removal regression to the NPC/fidelity lane.

3. **P1: connector work is not bounded by the advertised job budget.**
   `grid.rs:126` scans matching cells; the new predicate at
   `simulation.rs:1903` raycasts the map for each until a clear pair is found.
   Valid brick meshes permit two million attachment cells
   (`crates/content/src/brick.rs:731`). Two touching 1000-by-1000 one-plate
   authored bricks separated by a full map slab can require a million blocked
   raycasts in one ordinary placement or one graph-neighbor step. Charging
   one PLANT does not bound that inner work. GroupSupport also eagerly
   materializes `Index::query` candidates after one SEARCH charge; a large
   or coincident imported copy can collect many candidates before yielding.
   Root assigned bounded connector/candidate work to the NPC/fidelity lane.

## Rendering findings and revised static review

The first tree-face patch recalculated a rectangle union by scanning all
rectangles at every x interval for every neighboring face. Valid meshes can
have 100,000 quads, creating quadratic rebuild work. Its neighbor-cell count
also compared against the target's required area without intersecting the
target's actual face: an opaque 2-by-2 neighbor under a corner of a 6-by-6
logical tree box could hide an exposed central 2-by-2 underside.

The revised `/tmp/bri-v023-tree-face-coverage-production.patch` addresses both
findings statically: at most 64 total quads participate in a cached immutable
mesh proof, unfamiliar detailed/alpha/off-plane/nonrectangular faces remain
drawn, the target must prove a complete cell-aligned rectangle, and neighbor
cells intersect that rectangle with a 16,384-cell proof limit. No further new
blocker was found in this revised patch. Root runtime proof remains pending.
The existing Index visitor still walks candidate neighbors after proof is
complete/exhausted; these limits bound proof work, not arbitrary whole-world
candidate enumeration. The conservative fallback retains authored content.

The compact GroupSupport draft now stores identity/quarter-turn/bounds instead
of full Brick/event metadata. This resolves the immediate retained-metadata
issue; it does not itself resolve the work or lifecycle blockers above.

## Mounted rejected-crossing regression prepared, unrun

**P2 candidate:** `Motion::observe_vehicle` (`motion.rs:229`) restores/replays
authoritative poses but leaves a pending `unshown` carry in place regardless
of whether the host accepted that trip. `show_crossing` (`:783`) later consumes
it using an inverse-carried corrected center. A client can predict passage,
then receive a normal portal unlink and a newer host source-side pose that
acknowledges the rejected move. The old carry can still move the drawn frame
and turn controls. Existing positive tests prove no duplicate replay
announcement; they do not prove cancellation of a rejected prediction.

Root requested `/tmp/bri-v023-mounted-rollback-tests.patch`, prepared without
editing shared tests. It admits and boards real Jeep/Horse riders, uses a
second admitted human's actual wrench trigger to inspect the source frame,
and sends ordinary Wrench properties to clear the link. Prediction starts at
the actual host input sequence. The negative variant unlinks before the host
processes the crossing move; the positive variant unlinks after the host
accepted it. World sync precedes correction with the actual newer VehiclePose
and its input acknowledgement. Negative assertions require canonical source
drawing and no immediate/delayed obsolete carry; positive assertions require
exactly one carry and destination drawing despite later link removal.

`rustfmt --edition 2024 --config skip_children=true` on the temporary source
and `git apply --check /tmp/bri-v023-mounted-rollback-tests.patch` passed.
No compilation, fixture execution or behavioral result is claimed. Root must
first execute the wrench fixture and the intended before/after boundaries.

## Checked boundaries and remaining evidence

Source review covered saved per-mini-game replacement/budget rejection,
stable-ID Add-On preference replay, authenticated pre-admission fetching,
admin refresh and reset lifecycle, linked spawned-body canonical ownership,
live hold permissions, native hold/charge commitment and objective repair,
mounted host/predictor/camera/rig splitting, first-person per-view visibility,
authored unlit/shadow changes, avatar preview and icon orientation. Relevant
test changes and lane proof reports were inspected. No additional confirmed
score/win backdoor, silent trust escalation, admin cross-session leak or new
content-name/mode-name hack was found in those inspected changes. This is a
risk review, not proof that every project path is correct or that a packaged
release has passed its platform gates.

The common old-foundation defect is treating a multi-tick operation's captured
admission as permanent authority, while the native hold path already consults
current rules. The common new cost defect is charging a logical neighbor/brick
while allowing its authored geometry to trigger much larger inner work.
These need cohesive mechanism corrections, followed by actual user command
entry regressions, rather than content-specific exceptions or direct-API-only
proofs. Root is coordinating the corrections and must request fresh review
of revised complex patches before publication. Maxwell retains interactive
acceptance; all three platform artifacts and final gates remain root-owned.
