# 2026-10-31 Stacking contact baseline

The standard motor already accepts one living body standing on another in a
headless two-body contact case. After the lower body becomes a corpse, the upper
body falls through it and lands on the floor. The focused `bri-motor` test
`a_living_body_supports_another_until_it_becomes_a_corpse` and warning-free
all-target bri-motor Clippy pass. Diagnostics are `stack-living-support.txt` and
`stack-living-support-clippy.txt` in the planning evidence directory.

This establishes stationary physical support and death revocation only. It does
not establish jumping onto moving bodies, bot cooperation, replicated support,
scaled bodies, low ceilings or the saved cube attack. Next trace bot navigation
and ordinary jump controls against living-body contacts before selecting a bounded
cooperation mechanism. Preserve the corpse collision contract.


A standard body can also jump from the floor onto a stationary living body
using ordinary movement and no jets. A first attempt overshot because releasing
movement retained horizontal momentum; a bounded position/velocity steering
input lands successfully. All five still-controller cases and changed-crate
Clippy pass. The first Clippy run rejected standard `clamp`; the test now uses
the workspace's required checked-bounds helper. Evidence: `stack-jump-motor.txt`,
`stack-jump-motor-controlled.txt`, `stack-jump-motor-final.txt`, and
`stack-jump-motor-clippy.txt` in planning diagnostics.

The read-only v21 Bot_Hole `packages.cs` collision handler checks the stacking
flag, a following target above the bot and another hole bot, then schedules
jumping on both after a short delay when their relative positions qualify.
Bot_Zombie enables that flag. This is evidence for the source behavior, not a
claim that timer-only copying will solve native crowd coordination. The native
bot safe-walk path currently evaluates ordinary hops through a static landing
probe. Next verify how that probe treats living bodies and connect a grounded,
bounded cooperation decision to ordinary controls; the cube is still unsolved.
