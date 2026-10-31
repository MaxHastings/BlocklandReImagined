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


The safety path is now traced: `bot_safe_walk` accepts an ordinary moving hop
only if `hop_lands` finds world geometry below its projected level-ground landing
point. `admin_players::world_ray` explicitly excludes player and vehicle
colliders, and the probe starts only 0.5 units above the current feet. Therefore
it cannot validate landing on an elevated living body's top; merely requesting a
normal hop cannot establish the requested stacking behavior. The in-place jump
path is separate and bypasses that particular hop probe, but does not provide an
approach, landing plan or cooperation. Preserve the existing world-only test for
ordinary movement: the supported next change needs an explicit living-support
candidate and a trajectory/clearance check using the actual body dimensions,
lifetime and motor, not a blanket inclusion of players in world rays. The v21
collision handler's synchronized jumps are evidence for timing, not evidence of
native clearance or stability. No cube success is claimed.


A second existing behavior matters for cooperation: `bots.rs` treats a grounded
bot with no nearby world-ray floor as perched and may direct it to step off when
its route is missing or starts below its feet. That heuristic also covers narrow
static geometry; it is not proof that a living actor is actually supporting the
bot. The same `on_body` heuristic suppresses idle surprise. A stacking change
must identify the real living support and override step-off only for an active,
valid cooperative attempt; broadly disabling this recovery would regress ordinary
navigation and ledge behavior. The bot's seen-target data already carries the
observed target position, so ordinary stacking can use known pursuit evidence
without reading a hidden target's live transform.


The shared motor now retains the collider tag selected by its existing grounded
contact (`Player::support_tag`), including the follow-floor shape-cast path. It
is cleared on corrections, teleport/place, pass-group changes, death and
archetype changes; a consumer must revalidate the referenced object's current
lifetime/eligibility because other actors can move or die after the motor step.
It is transient local contact evidence, not a new wire or save field. A two-body
case checks the actual supporting actor tag, invalidation on a pass-group change,
and loss of that actor tag after death. Nine focused motor/collider/spawn cases
and warning-free all-target motor Clippy pass; the last body-shape invalidation
change also reran the five still-controller cases and Clippy. Diagnostics:
`stack-contact-tag-final.txt`, `stack-contact-tag-last.txt`, and
`stack-contact-tag-clippy.txt`. No bot currently consumes the seam, and moving
support, prediction agreement, cooperation and the cube remain unverified.
