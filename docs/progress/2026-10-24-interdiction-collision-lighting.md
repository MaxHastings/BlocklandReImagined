# 2026-10-24 Interdiction collision and doorway lighting

Maxwell supplied exact native Y-up positions from diagnostic build d7efd1a7a.
An assembled mission session reproduces the outdoor zero-input drag: velocity
[-2.828427, 0, 2.828427], airborne, climbing=true. All three collision reports
are inside an incorrectly enlarged imported ladder trigger. The private brush
reader bounded every stored point, including unused editor points. Referenced
BSP polygon vertices give the actual small brush; all five corrected bounds
match the independently retained source-volume hulls. In the corrected mission,
all three reported positions settle on actual floors and remain stationary.
Ordinary Use still opens the nearby jail door (Mover15, key 0 to key 2).

The source ladder facing can be diagonal independently of its box. Optional
scene property `facing_yaw` preserves that distinction: exact box containment,
independent motor facing/pull and planner extents in the climbing frame. Existing
callers retain their direction. No wire format or game policy enters the engine.

A separate generic door bug is independently reproduced: on obstruction the
host keeps the previous collision but publishes the rejected penetrating pose,
then applies it next tick without checking obstruction because motion stopped.
Cache the accepted frozen state and publish that when blocked. The regression
fails before, passes after, and checks idle stability and host/observer collision
agreement. Existing opening, blocking and reversing cases remain covered.

The retained camera-ambient correction prevents a viewer's room from changing
all visible world lighting. Private source comparison also finds inherited raw
FColor defaults silently becoming black fog; decode BGRA instead. Explicit black,
actor overrides, enabled state and distances stay intact. Fifty Interdiction
region colors change; this is a production conversion fix.

Evidence is outside disposable worktrees in the private
interdiction-jail-report-20261023 archive: human coordinates, original map hash,
source hull/region comparisons, known-bad results, corrected mission results and
captures. The first raw collision probe overwrote sensor tags for labeling and
therefore suppressed ladder recognition: it was inadequate negative evidence,
not a reproduction. This limitation is retained explicitly. A capture setup also
requires an opened door and exact viewpoint assertions; a misplaced frame is
rejected. Full mission appearance and Maxwell's experiential acceptance remain
separate from these bounded checks.

Validation: quick passes (77s); focused passes (1,278s), including strict
all-target clippy, changed crates/dependents and client/render units. The final
private mission regression passes in 84.84s with the production content and
position-only fixture. Nine private conversion checks pass. Three actual
ordinary-client offscreen frames at the reported doorway and return satisfy
0.15-unit viewpoint assertions. A stable right-wall region has 8-bit mean luma
13.07 -> 13.20 -> 13.05. This measures corrected mission continuity, not a
full original-image match. Initial small-teleport setup failed the viewpoint
assertion; a distant setup hop restores exact placement, without relaxing the
assertion or changing production movement. Its unresolved cause is retained.

The matched private source is d0dc3a380. Optimized packaging and the full main
push gate follow this commit; their exact pins/results are retained in the
external package BUILD/MANIFEST and current private mission handoff.
