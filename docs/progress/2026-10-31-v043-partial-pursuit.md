# 2026-10-31 Execute reachable partial pursuit before rejecting its target

The farther saved-house control remains unresolved after the full-width graph
correction. Its trace prompted a separate policy investigation: `out_of_reach`
is assigned when a graph returns a partial route, and the next behavior decision
can suppress Chase/Search immediately, discarding that route before walking it.

Extended the retained unreachable-roof integration to start the bot 12 units
away. It still requires eventual give-up for the ordinary bot with no way onto
the roof, and now also requires it to execute the reachable approach first.
Against the prior policy it fails: closest horizontal approach is 9.51852 units,
though the planner reported a three-waypoint route ending directly below the
player. Private `diagnostics/partial-route-red.txt` records the actual failure.

The correction keeps Chase/Search eligible while their route/frontier remains
live. Once it is exhausted, the established unreachable policy applies; a route
to an unrelated idle objective cannot reactivate an old pursuit. No search or
memory timers, suicide cooldown, body size, melee reach or movement inputs are
boosted. This is distinct from the still-pending authored persistent/stacking
policy required for zombies. Full bot-route regression and saved-house rerun
are pending; do not mark house egress or cube stacking accepted from this change.


The roof regression passes after retaining the partial pursuit. The complete
bot_routes suite then reports 9 passed/1 failed: an allied-evidence driver leaves
its jeep before seeing the hidden target. Restoring the previous chassis snug
cost alone does not fix it (9/1 again), so that hypothesis is rejected. Controlled
affected-source comparisons pass the driver with both nav/bots from fdd6f49e2,
and also with current full-width nav plus only bots.rs from fdd6f49e2. Other
source remains current (including the independent music-volume increment);
these are narrow comparisons, not historical whole-source release builds.
Both current files were restored byte-for-byte after each comparison. Evidence:
`driver-affected-source-baseline.txt` and `driver-current-nav-old-pursuit.txt`.
This implicates pursuit continuity rather than the corrected pedestrian graph.
A bounded once-per-second driver history is added to the retained assertion to
expose its actual controls/waypoints before choosing the correction. Do not
claim the whole bot suite passes or relabel this regression as pre-existing.


Driver history shows real travel followed by a brake at an orientation-dependent
wall bend, not an initial failure to press throttle. A current-leg headway
experiment delays give-up but still fails (9/1), so it is removed; no timer
increase or relaxed assertion is retained. The earlier continuity increment
incorrectly applied pedestrian route retention to conservative chassis corridors,
which depend on their oriented-hull/turn controller and existing retarget behavior.
Scope route/frontier retention and travel-probe pausing to the pedestrian owner;
keep the established chassis retarget path. This follows the same mechanical
boundary already used by soft-limit pursuit expansion. The retained real-driver
case must pass before calling this corrected; deeper curvature-aware chassis
planning is not silently introduced into the zombie-pathfinding increment.

The actual house audit now observes a real grounded bot on the sill (tick 202),
finds a partial path ending at x=98, and records the bot reaching that sill.
The next missing edge is a crouched drop under a lintel: walkoff previously
required a standing departure and standing crossing. It now keeps a verified
low departure stance through the swept crossing to a standing landing, with the
existing supported-floor, drop, avoidance and work bounds. The first synthetic
window test unexpectedly found a route around its single wall, so the fixture
now encloses its room as the actual house does; its direct-exit and actual-motor
assertions remain. No egress acceptance is claimed until checks and save reruns.


The enclosed, grid-offset window fixture initially reproduces the actual partial
route. Two further defects were retained rather than hidden by the simpler test:
walkoff was considered before provisional jump/step candidates were swept, and
its landing steered a settled lateral offset back to the grid centre before the
body cleared the jamb. Try the bounded supported-drop fallback after all ordinary
links fail validation, and test a real within-cell landing that preserves the
departure lane. Both original and aligned options are checked against actual
geometry/support; cached floor identities stay unchanged. No narrower body or
invented support is introduced. The actual motor passes the offset 1.5-wide,
1.2-high exit; all 46 nav cases pass (one existing ignored measurement).

The first motor assertion wrongly compared against the requested search target
rather than the route's legitimate terminal waypoint. It now requires the
original waypoint arrival tolerance, grounded landing at its actual height,
the entire body clear of the window, and normal standing afterward. Direct exit,
no-jump/no-jet controls and full-width collision requirements remain.

The original-position save still exposed premature route advancement: jam recovery
could skip jump/crouch points while feet were 1.2 below the sill. Those required
actions now remain until physically achieved (jump landing grounded, height
within the nav's existing 0.1-unit quantization). Plain walking recovery keeps
its existing behavior. A four-case session regression covers plain/jump/crouch/
jump-crouch waypoints and preserves stalled physical-progress accounting. The
navigation/body suite passes 10, and nav passes 46 with one historical measurement
ignored. This does not increase jump height, melee reach, pursuit timers or budgets.

The rebuilt normal host/save probe now records real target deaths at tick 1230
for the original house position and 1616 for the farther control, with every
brick loaded and zero set-aside. Private reports are
`diagnostics/v043-house-action-{original,far}.json`. Earlier aligned-drop-only
runs killed the farther target (1736) but not the original, and are retained as
partial evidence rather than represented as complete. Other saved-world reruns,
final bot-route/Clippy checks and cube-stacking acceptance remain pending.

The exact-height/grounded arrival restriction above is **not retained**. Its
first mattress-house rerun fails to reach the balcony over 7,200 ticks; a bounded
every-tick trace and a 14,400-tick control confirm the failure. It overconstrains
the motor's existing sloped-surface arrival behavior. Keep only the supported
correction: jam recovery cannot skip required jump/crouch waypoints at its wider
one-unit radius. Normal waypoint arrival retains its established tolerance and
airborne traversal. Remove the temporary height constant and grounding rule.
Read-only next-action diagnostics remain so future failures expose the required
stance as well as a point. A diagnostic rebuild raced an earlier compile and
reported the newly added field missing; the completed-source rebuild succeeds.

Final normal-save runs (`diagnostics/v043-*-action-no-skip.json`) load every
brick with zero set-aside and record real target deaths:

| Encounter | Death tick |
|---|---:|
| House, original reported position | 1234 |
| House, farther exit control | 1736 |
| Standalone staircase | 3240 |
| City tower | 5123 |
| Mattress-house balcony | 2360 |

These use ordinary host loading, the saved minigame, full-size motors and actual
damage; no teleported bots, stronger jumps, enlarged melee reach or jets. They
establish traversal in these retained worlds, not exhaustive search or stacking.
Cube stacking and authored persistent pursuit remain separate release work.

Final scoped commands: `cargo test --locked --profile playtest -p bri-sim --lib
navigation_body_tests` passes 10; `cargo test --locked --profile playtest
-p bri-chaos --test bot_routes` passes 10 (including the unchanged driver
acceptance and extended roof approach). `cargo clippy --locked --profile playtest
-p bri-sim -p bri-net --all-targets -- -D warnings` passes. The nav suite had
already passed 46 with one historical ignored measurement before the arrival
experiment; removing that experiment leaves nav behavior unchanged. An initial
Clippy failure requested a collapsed conditional, which is corrected. These
are changed-area checks, not the full release gate or a new manual playtest.
