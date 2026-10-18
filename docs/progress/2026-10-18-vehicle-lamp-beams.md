# 2026-10-18 Vehicle lamp beams and ramps

Before: a vehicle lamp was an all-round point light that switched on and off at
once, so a headlight lit the road behind and beside it as much as ahead.

After: a lamp may carry `cone` (`inner`, `outer` degrees, `axis` in its node's
frame, default -Z) and `ramp` (`up`, `down` seconds). `PointLight` gained a
`cone` (zero: all round, else `spot_words` without the sentinel); both shaders
apply it to point-lit surfaces, vertex-lit objects and physical highlights.
Each client steps a ramped lamp's level toward its state in its own wall-clock
time (cosmetic; nothing is replicated), scaling the lamp's colour.

Evidence: `render/tests/point_light_cone.rs` (beam lit under it and dark off to
the side, all round without a cone, both shaders). Not built: a replicated
brake or input bit for taillights.
