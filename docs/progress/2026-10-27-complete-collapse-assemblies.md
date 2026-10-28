# 2026-10-27 Complete local collapse assemblies

Continuing draft PR #379 after Maxwell's pirate-ship mast playtest. The
screenshot alone cannot identify the cause, but code inspection confirmed two
independent omissions: charging section members to the rigid-body limit, and
rejecting all members after initial landing. Maxwell requested a focused fix
and another private Windows playtest, preserving little bricks while making
physics chunks larger. No interactive play session was automated.

Presentation now separates original visible meshes, shared assembly transforms,
and independent physics bodies. Complete section totals, bounds and a lowest
footprint near the hit accompany existing removal cues; a new protocol-change
file requires matching server/client builds. Late members join the current
pose and breakup waits for complete delivery. Interrupted groups keep their
received art together through the normal fade. Tall shapes lean around a base
before falling free; broad floors drop/tilt and narrow spans rotate. Contact
uses eight cached original-member corners, with 256 shared candidate steps.

Connected region growth starts during the fall and resumes within 4096 cheap
candidate steps/frame. Landing groups keep original geometry and use one box
proxy each. Quality controls granularity; at most 64 assembly bodies share the
existing rigid-body budget with ordinary debris. Sleeping/budget-evicted bodies
keep their art at the last pose. Collision streams within swept oriented bounds
under 512 shared steps/frame (terrain tiles cost 16), pausing motion until ready.
Approximate assembly proxies ignore each other to avoid false collisions through
empty sail/beam bounding space. Existing local pushing, shots and blasts also
reach these bodies. No pose replication, crushing, or gameplay rubble.

The separate render reservation is 65536 members, independent of Physics
Quality. Overflow rejects new whole groups instead of cutting arbitrary members
out of admitted assemblies. The existing per-look mesh/texture instancing is
reused. This remains bounded cosmetic presentation, not unlimited skyscraper
physics. Full 13-second lifetime includes falling and normal final fade.

Up to eight proven server groups drain round-robin under the existing global
128-removal/8192-query tick budgets. Shape metadata accumulates during support
proof, avoiding another unbounded full-component geometry scan. Both authored
edits and direct damage invalidate affected queued proofs. Discovery remains
incremental; the change removes delivery serialization, not proof latency.

Targeted client regressions passed: all 1500 connected mast members retained
through falling/landing at an eight-body limit; late post-landing arrivals keep
the current pose; quality zero preserves admitted assembly art; expiry removes
bodies; mast leans before contact while a broad floor drops. A server regression
checks two 160-member groups both emit in the first 128-removal round with
consistent complete-shape metadata, and subsequent rounds remain capped.

Validation in progress: quick/focused tiers, integrated gate, optimized Windows
build, awaited packaged --check, unique Add-On identities and file hashes.
First focused attempt caught a collapsible-if lint; corrected before commit.
Existing preview.1 and the source installation stay untouched. Maxwell retries
the mast in preview.2 and owns gameplay/visual acceptance; no merge is implied.
