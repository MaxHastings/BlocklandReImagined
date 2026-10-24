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
