# 2026-11-04 Zombie traversal and preview feedback

Maxwell accepted the Beta16 tower pursuit improvement in v0.4.4-preview.1,
but reported slow/repeated failed jumps at the Bedroom balcony and stair saves,
falling off stairs while staring at the player. These are required follow-up
regressions, not completed acceptance. He also accepted removal of the collapse
pivot/lag but found the new vertical free fall too stiff. #437 is held for
that additional motion work; passing CI alone does not settle that feedback.

The original Bedroom save/screenshot bytes and SHA256 inventory are copied
outside Git under E:/Downloads/BlocklandReImagined-Release-Evidence/
zombie-bedroom-2026-11-04. They include the tower, mattress house, staircase,
small house and cube cases. The saves use the engine's encoded build format
despite .world.json names; use bri_world::build::decode, not a JSON reader.

A 20-second ordinary-control staircase replay on the preview's sim head starts
all three walking within 61 ticks, but none reacquire the live host's floor;
one falls to its death. Facing the route alone does not fix this: the first
prototype still has one fall and no arrival. Inspection found leap waypoints
can be consumed while standing two units away and on a different stair tread;
LeapLeg failure checks only height, not the measured landing footprint. A
second prototype confirms the actual landing before accepting completion and
records misses outside the measured footprint. This is not yet a fix verdict.
No GPU suites, gates, soaks or benchmarks run while Maxwell may be playtesting.

Maxwell also requested a Speedkart return archive. Delivered
E:/Downloads/Speedkart-addon-v0.4.4-preview.1.zip (54 files, 2,196,868 bytes),
SHA256 12c6d0fbd53837ea2734968e51d9011c2afa558601c26aa45d12b99e400157c1.
All three native 1.0.1 packages pass the preview's bri-addon-check; all ZIP
entries round-trip byte-for-byte and CRC validation passes. The compatibility
update replaces lens_objects with independent visuals and uses named
brake/reverse conditions, with current API notes. Model, texture, sound and
script payloads match the supplied original. Credits/provenance are retained.
No original or returned Add-On content enters Git. The earlier exact authored
asset offscreen review remains the visual evidence; no new GPU run was made.

Outstanding: reproduce the balcony, retain tower success, fix actual traversal/
failed-route handling and prove it across these saves; improve coherent collapse
motion using grounded forces/contact rather than restoring a fake hinge; scoped
checks and final-head CI, then a new playable preview and Maxwell's acceptance.

Further trace evidence: restored traversal cost (including footing/clearance)
in the floor-guided queue, then made cost primary and floor distance a tie
breaker. The original greedy tower ordering discarded these costs. The new
prefer_goal_floor name states the narrower contract. The ordinary stair-lane
regression is extended to this pursuit mode without relaxing its assertions.

A shared body-attack contact query now supplies both pursuit readiness and
post-movement damage admission. A preferred distance band alone no longer
makes a body attacker stop pathfinding; aiming uses the actual eligible body
contact. Range, permission, conversion and seated-hull rules are preserved.
No player immunity, damage multiplier or save-specific bot policy was added.

The balcony's first impossible shortcut launched off a ramp: the live motor
scales jump impulse by its support normal, but the planner used flat-floor
height. Support contacts now retain their normal; arc admission rejects rises
above the actual launch apex plus the motor step, and checks the resulting
vertical flight. The existing measured flat-floor lateral reach remains a
filter, not proof of every contact combination. The new balcony replay no
longer sends Zombie #2 through that opening jump: it follows the house route.

Current 60-second functional replays (not benchmark acceptance): all three
stair zombies reacquire the live host's floor and the host is killed, with
zero zombie fall deaths; all three tower zombies reacquire the live host's
floor and the host is killed, zero zombie fall deaths; balcony host is also
killed, with zero zombie fall deaths. First damage is 12.0/34.3/19.4 seconds
for stairs/tower/balcony respectively. First walking remains under the first
observation expiry in every case. These are technical outcomes, not Maxwell's
acceptance or a claim that traversal is optimal. Scoped unit checks and the
unchanged explicit tower acceptance test remain pending.

Scoped verification: 523 sim unit cases pass, five existing ignored measurements;
22 bot_brain cases pass (doorways, crawlspaces, swimming, alerts and ordinary
bite cadence included); the seated-hull bite integration passes; two infection
boundary cases pass. Warning-free sim/chaos all-target clippy passes. The
private acceptance wrapper was extended to require a real bot kill and zero
uncredited bot deaths; the tower still requires all three to reacquire the
living host's floor and retains its original assertions. An initial new
wrapper failed because saved spawn events admitted bots after the initial
snapshot; it now captures actual pursuers during the host's first ordinary
life, instead of falsely accepting an empty initial set. No immunity or
transform injection was introduced and no regression assertion was relaxed.
Final saved-case acceptance runs, maintained format/docs/history and platform
CI are still pending. Do not merge #437 or declare stiff collapse accepted.

Final private acceptance runs pass: the unchanged all-three tower-floor
assertions plus real host kill; the staircase and balcony pursuit assertions
require real bot kills, every admitted pursuer beginning before its first
observation expires, zero uncredited bot deaths and zero failed bot steps.
Source choices and ordinary host mortality remain intact. The first new
wrapper's initial snapshot missed delayed saved spawn events; capturing
pursuers during the host's first life corrected that harness error. This was
not a retry-only pass. The original assertions were retained and strengthened.
Maintained format/docs pass. Final sim/chaos all-target clippy and navigation
invalidation checks are recorded in their logs; platform CI is still required.
