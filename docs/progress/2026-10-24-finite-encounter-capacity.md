# 2026-10-24 Finite encounter capacity

A private mission consumer replaced capacity-driven defender replacement with a
source-qualified finite roster. The existing host limit of 32 full bots could
not represent that roster. Max bots now permits 64 while retaining default 16.
Admission and replication also shared a separate hard limit of 64 participants:
the unchanged 64-bot combat benchmark spawned only 62 alongside its two builders.

A shared 128-participant budget covers humans, bots and reserved loading slots.
Session admission/resume, Add-On bot admission, public package player state,
minigame membership, held images, inventory, appearance and vitals use it.
The network connection limit remains 64 humans. Existing per-message batching,
minigame count and unrelated resource budgets remain independent.

Two actual QUIC clients pass the 65-participant checkpoint / 66-participant join
update and agree on roster/inventory. Admission rejects participant 129. Replica
negative controls reject overlapping loading/active identities and a combined
roster above 128 without advancing its cursor. Target sight reserve is checked
for every supported bot at capacity.

The existing all-capabilities combat workload now spawns all 64 bots and passes:
3,815 microseconds bot thinking and 5,220 microseconds whole step per tick.
The retained 32-bot comparison also passes: 1,910 and 4,029 microseconds. These
local optimized headless measurements exclude rendering and do not establish a
whole private mission's frame rate. That consumer's actual optimized 1200-tick
sample, including input harness, averages 10.879 ms/tick; mission performance
remains an open limitation. Default remains 16; larger workloads require
an explicit host choice. The first network-capacity fixture failed on obstructed
one-unit spawn spacing; the corrected four-unit grid preserves admission checks.

Logs and private assembled before/after consumers are archived outside disposable
worktrees in `interdiction-ui-lighting-20261024`. Quick checks pass (156 seconds).
The nine-crate focused run passes every target except the Slayer consumer
corrected below; the complete changed importer focused tier then passes (240s).
Full gate and Windows CI must pass before integration. No interactive
playtest is claimed and this change does not complete the private mission.

The first focused run found one additional real consumer mismatch: Slayer's
BRI-added free-for-all count rejected the new host maximum. Its port template
now consumes the shared engine bot limit through `engine_max_bots`; the existing
unchanged fill test exercises 64 rather than weakening its assertion to 32.
