# Stair-edge feedback from preview.2

Maxwell reports that bots in `ZombiesHaveTroubleGettingMeStairCase` bias toward
the edge closest to the player and fall, instead of using the middle of broad
stairs. The earlier saved-world target kill does not establish acceptable route
quality; this interactive observation supersedes treating a kill alone as
adequate evidence for this scenario. Prefer a safer supported route through
wide or turning steps over a slightly shorter exposed route, while preserving
necessary narrow paths and legitimate ledge/jump behavior.

Initial code trace: `nav::sample` marks `snug` when an expanded body overlaps
fixed collision, favoring clearance from walls. The overhang penalty covers
nodes whose centre is already over air, but a node with its centre just inside
an exposed stair edge can still have little margin. `Ground::walkable` samples
floor under the centre, and `nav::pull` can shorten routes based on that check;
neither directly measures lateral support margin. These are plausible design
gaps, not yet proof of the observed cause or a reason to reject all edge use.

Next: construct a bounded stair/turn fixture and measure actual support margin,
waypoints and motor falls, including a narrow-only control and full-body
collision. Evaluate both search preference and route pulling together; retain
doors, jumps, multi-floor routes, and dynamic world invalidation. Rerun the
supplied save with route-quality measurements as well as damage. Maxwell is
currently playing preview.2, so no local GPU, soak, benchmark, or heavy build
was started during this trace.
