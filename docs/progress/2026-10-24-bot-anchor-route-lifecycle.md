# 2026-10-24 Bot anchor route lifecycle

Follow and patrol anchors now distinguish floors. Patrol arrival measures full
3D distance; idle return measures distance to the anchor rather than the
horizontal combat leash. Combat pursuit boundaries retain their existing
semantics.

Home routes previously stored only a `Home` tag. Moving the leader changed the
reported destination while the active search and waypoints still led to the
previous destination. Home goals now retain the requested point and use the
existing positional goal slack to replan after material anchor movement. Failed
home routes remember both their stopping point and requested anchor, so a moved
leader/post permits a retry without repeating the same failed search every tick.

The retained native regression establishes the two-floor fixture after mini-game
creation/respawning and disables optional interaction/surprise choices. Those are
fixture corrections, not production fixes. With only the initial 3D correction,
moving the leader onto the reachable upper floor makes the follower continue its
old route downstairs (`moving-anchor-before-53.log`). With the route lifecycle
correction the same follower reaches the leader; the cross-floor patrol also
passes (`moving-anchor-after-54.log`, two tests, 0.11 seconds). The private logs
and exact source pins remain in the owning thread's external evidence archive.

The assembled private consumer independently shows that long-distance navigation
still fails: a physical route exists but exceeds the local search's expansion
limit. A full-map battle phase capture attributes most recorded bot phase wall
time to navigation. These corrections do not establish global reachability,
accepted battle flow, network latency, or a delivered playtest. Global route
guidance and assembled validation remain the next dependency; do not increase
every bot's search limit to disguise the repeated local dead end.

Validation: `cargo test --profile playtest -p bri-sim --lib anchor_tests --
--nocapture` and `cargo clippy --profile playtest -p bri-sim --lib -- -D warnings`
pass. Formatting uses `rustfmt --edition 2024
crates/sim/src/session/bots.rs`; `cargo fmt --all` exceeds this Windows command
line limit. No main integration or completed full gate is claimed here.

## Stair destination floor

The externally guided private descent then exposed a separate completion defect.
A naturally spawned actor stops issuing movement on a higher stair near its
requested anchor. The synthetic `nav::tests::a_stair_goal_is_reached_on_its_floor`
reproduces this independently of map conversion, scripts and crowds: the clear
goal `(7.75, 1.2, 0)` produces a completed path ending at `(8.5, 1.8, 0)`.
A central-grid target passes the old code; clear sub-cell target positions reveal
the mismatch (`native-stair-arrival-before-58/59`).

The search formerly accepted any node within two vertical units. It now samples
the destination's own walkable floor through the existing budgeted grid and
accepts nearby nodes on that resolved floor. Floating goals without a sampled
floor, swimming arrival and jet landing behavior keep their existing handling.
No search expansion limit or intermediate-driver tolerance is raised.

The new regression and 41 other navigation checks pass; one existing test remains
ignored (`native-stair-arrival-after-60.log`, 0.30 seconds). The same assembled
guided-descent comparison is being repeated without another tolerance change.
Externally supplied milestones cannot establish autonomous global route selection.

The unchanged driver in `guided-deep-route-after-floor-61` progresses through
15 of 29 milestones, including the main descent to the reported lower corridor.
It then stops one stair above a final waypoint. This exposes the second half of
completion: the pedestrian executor permits a step plus slack at every walking
waypoint, including the last. That is useful for intermediate risers but can
discard the destination while still standing higher. The candidate now requires
the final pedestrian walking point to match its floor within half the grid's
height quantum plus placement slack. Intermediate risers and conservative vehicle
footprints retain their existing handling. Comparison 62 repeats the same driver
and thresholds and passes: the actor reaches the deep floor at
`(-21.044, -69.27, 102.877)` after 235.5 simulated seconds across the house,
basement and deep legs. Comparison 61 stops at milestone 15, comparison 57 at
milestone 1. Strict library clippy also passes (`arrival-clippy-63.log`).
This proves the corrected local executor can complete the externally guided
descent, not autonomous global route selection. Global guidance remains unimplemented.
