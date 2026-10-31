# Pursuit continuity through buildings

The tower static audit found a complete no-jet route, while the integrated bot
still failed. Added dated-memory output to the existing private headless probe.
It identifies repeated allied warnings as a second concrete source of route
starvation: perception intentionally varies a heard position slightly, and each
new warning discarded the entire unfinished route. Changing from visible chase
to last-known search (and back) also discarded work even at the same destination.

The brain now retains its existing pursuit route/frontier when the newly requested
destination is within the existing 2.5-unit chase tolerance. Its goal continues
to report the frontier's actual destination; it never silently retargets explored
nodes. Materially displaced evidence still starts a new route. Completed routes
use normal arrival/replanning. This uses only visible or dated evidence, including
its original expiry, and does not query an unseen player's live position.

A second consumer error counted actual traversal toward a last-known location
against the local search-probe timer. It could abandon a tall building after
three seconds of walking. Route computation and traversal now precede that
local timeout. Existing dated-evidence expiration, physical stall recovery and
bounded unchecked probes remain responsible for ending unsuccessful pursuits;
no timer or per-tick planning budget was increased.

The regression first failed on the original chase-to-search transition. It now
checks nearby warning drift, losing/regaining sight, retaining an actual route
through a long traversal and rejecting substantially displaced evidence.
Navigation-body tests (9) and search-memory tests (9) pass with
`cargo test --locked --profile playtest -p bri-sim --lib <filter>`.
Changed sim/net all-target Clippy passes with `-D warnings`.
Direct consumers also pass: navigation invalidation (1), NPC profiles (7) and
replay (11), using `cargo test --locked --profile playtest -p bri-sim --test
nav_invalidation --test npc_profiles --test replay`. Scoped sim/net formatting
and `git diff --check` pass. These are not the full release gate.

Actual saved-world results, using ordinary controls, original saved minigames,
the provided target coordinates and no jets:

| Save | Result | First sampled target death |
| --- | --- | ---: |
| Beta16 City Tower | Zombie climbs to Y=315.61 and kills target | tick 4,683, about 38 simulated seconds after placement |
| Staircase | Target killed; stair centering remains separately open | tick 2,401, about 19 seconds |
| Mattress house | Target killed | tick 2,166, about 17 seconds |
| Farther house-egress control | Still fails; bot respawns after becoming stuck | none |
| Cube | Still fails; physical stacking remains open | none |

Private reports: `diagnostics/v043-{tower,stairs,mattress,house-egress,cube}-pursuit-continuity.json`
under the pinned planning evidence root. The tower's actual target death tick is
4,673; sample intervals are 120 ticks. All reports record zero bot-step failures.
Runs establish actual authoritative traversal and damage, not visual acceptance
or performance measurements. House clearance, stacking, natural stair clearance
and genuinely unreachable-target behavior remain required work.
