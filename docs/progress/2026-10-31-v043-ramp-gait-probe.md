# Held hammer on a slope: preliminary gait probe

The report is a quickly flickering hand/hammer in third person while walking
up actual 45-degree ModTer, without swinging, with ragdolls enabled but no
corpse. No reproduction of that full combination exists yet. Gait selection
uses grounded state and vertical velocity; an airborne rising body selects
`jump`, while a grounded moving body selects its ground gait. The held-arm
layer and item socket are separate client presentation paths.

A temporary diagnostic used the existing `player.rs` convex ramp helper with
an 8-by-8-unit 45-degree wedge, walked forward, and inspected the gait for
feet z between -2 and -8. A hypothesized assertion that every sample must be
`run` failed: near the lower section it observed `jump`, `root`, `jump`,
`root`, then stable `run` higher up. The first attempt accidentally omitted
`world.step()`; a corrected run with the normal physics step produced the
same sequence. These are sampled at simulation ticks, not final rendered hand
poses. This does not establish whether brief airborne motion is wrong for the
source motor, or whether it causes Maxwell's flicker. The temporary assertion was
removed because its proposed all-run requirement has not been established;
no production code or existing regression assertion was changed.

Next supported reproduction needs actual converted ModTer geometry, held hammer
and rendered/observer arm state; compare genuine swings and a level control.
Per Maxwell's instruction, do not guess at smoothing or mark this report fixed
without evidence. No interactive game was opened.
