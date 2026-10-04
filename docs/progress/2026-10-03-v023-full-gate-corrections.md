# 2026-10-03 v0.2.3 full-gate corrections

The first exact candidate, `56fa1f82811e28f4753aea035df1f5e20afd66e7`, passed
build, strict Clippy and content startup, then failed six tests across 344 targets.
Each still failed an isolated retry. No failure waiver or acceptance threshold was
added. Candidate Windows CI/package runs were cancelled rather than publishing this
source. GPT-6.1 Sol High reviewers investigated narrow causes; root applied patches
and owns the execution receipts below.

- **Placement:** a shared 256-operation allowance included every floor ray, so an
  ordinary unsupported 16×16 Workshop plate exhausted it. Neighbor/connector and
  raw contact stages retain separate 256 limits; floor/terrain stages account for
  the declared footprint, capped at 4096 cells. Renamed 16×16/64×64 fixture bricks
  can float or find a smaller partial floor, while pathological connector work
  remains refused. This bounds individual stages, not a guaranteed frame time for
  every large copy. A resumable whole-placement seam is not added here.
- **Culling:** the converter's outward 0.0012 grid skin and native trapezoid/triangle
  quilts did not fit the rectangle-only proof. The cached proof now checks bounded
  opaque triangle unions on critical strips. It keeps the 64-quad, 4096-breakpoint
  and 16384-cell bounds. Independent review found that clamping projected vertices
  could fill a skew sliver; only face-normal skin is normalized. In-plane geometry
  stays exact, gaps remain gaps, and collapsed strip midpoints retain faces.
- **Menus:** the synthetic topology fixture's catalog lacks stock default tools.
  It now checks those exact IDs remain Unavailable and explicitly clears them through
  normal dropdown input. This exposed a shared production bug: typing exact NONE
  kept that pinned row unhighlighted, so Return changed nothing. Exact clear choices
  now select normally; ordinary weapon searches still skip the pinned clear row.
- **Startup:** Add-On choice restoration could fail before App loading and omit
  the established "Loading the game" phase context. It now uses that same context.
- **Bot proofs:** quaternion invariance is checked in object-local coordinates at
  the unchanged 1e-5 tolerance; translated-world hull clearance remains checked.
  The guarded coast fixture releases after observed native inbound motion near its
  authored goal, instead of relying on the former accidental remote-grab launch.
  It requires actual grip clearance before real coast entry and no award after
  the guard is revoked. No physics state or bot success is injected.

Focused current-source receipts:

- `/tmp/bri-v023-launch-final.log`: 3 startup tests pass.
- `/tmp/bri-v023-popup-clear.log`: 11 shared View input tests pass.
- `/tmp/bri-v023-local-rotation.log`: unchanged-tolerance rotation proof passes.
- `/tmp/bri-v023-placement-stages.log`: 20 duplicator and 15 building tests pass.
- `/tmp/bri-v023-workshop-race.log`: all 8 navigation/Workshop checks pass.
- `/tmp/bri-v023-coast-earlier.log`: all 5 creator adversarial checks pass, including
  both transformed held/released native object fixtures.
- `/tmp/bri-v023-gate-corrections-final.log`: unchanged native draw-budget test,
  all 10 coverage checks (including skew sliver), and both native/synthetic
  host/guest menu topology journeys pass.
- `/tmp/bri-v023-motion-final-candidate.log`: all 28 Motion checks pass, including
  ordinary native/synthetic same-tick jet ejection and same-vehicle driver promotion.

The first coast forecast refinement failed the second transformed case because
entry preceded release; its earlier authored trigger retains the stronger assertion
and both cases now pass. The first topology recovery failed and led to the real
pinned-clear selection fix. These are evidence-based corrections, not silent skips.

Next: commit this correction, rerun the entire exact-commit local gate and Windows
CI, then merge, tag and verify all three archives. Package compilation can overlap
verification as an unpublished artifact pinned to the same immutable source; any
superseded artifact is discarded. Public publication still waits for all gates.
Reported unreproduced crashes/performance/pursuit and the open alpha contract remain
as listed in the candidate integration note and release notes.
