# Low-wall navigation during v0.4.3 release checks

Linux CI and the local `npc_profiles::each_body_plans_with_its_own_reach`
regression both showed the Wall Kicker going around a 1.1-unit wall instead
of jumping it. The 1.25-unit-wide body's next half-unit grid cell has no valid
floor placement beside the wall. The leap scanner found a raised surface too
close for a valid landing and stopped without considering the far side.

When that raised surface cannot be landed on, continue scanning within the
body's measured maximum leap distance. Every candidate still needs a measured
flight and a clear full-body swept arc; the first viable landing stops the
scan. This preserves the existing limit on per-tick arc work and does not
change walking, shrink collision, or assume a wall is passable.

Validation on Windows: `cargo test --locked --profile playtest -p bri-sim
--lib nav::` passed 47 (one existing measurement ignored), including stairs,
low ceilings, narrow gaps and walls too high to jump. `cargo test --locked
--profile playtest -p bri-sim --test npc_profiles` passed all seven, including
all four bodies crossing or going around the wall as appropriate. `cargo fmt
-p bri-sim -- --check` and `cargo clippy --locked -p bri-sim --all-targets --
-D warnings` passed. The final release gate, performance workloads and platform
CI are still required on the final commit.

The imported Add-On recipe remains `f41644dde57e1d375b2be9984cf5a5d28187473684de07cea4d3601348c97e42`:
`bri-sim` is a dev-only dependency of the importer and is intentionally
excluded from its generation dependency walk. No imported content changed.
