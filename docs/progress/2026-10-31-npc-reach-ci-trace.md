# NPC reach CI failure and trace

Linux CI run 41269693205, job 124739790137, failed
`npc_profiles::each_body_plans_with_its_own_reach`: the Wall Kicker reached
its guard post but went around the low wall to absolute Z 8.433148. The
existing contract expects this jump-capable body to go over it with absolute Z
under 6. The same assertion failed on CI's isolated retry. This is not a
passing or merely flaky check. The non-jumping body's around-wall control had
already passed; the later body types in that loop were not reached.

Added failure-only route context to that existing test: sampled positions,
next waypoint, remaining path length, search/jump state and brain explanation.
The original assertions and simulation behavior are unchanged. This enables a
remote CI trace while Maxwell is playtesting without starting a local build or
heavy test. The source change was formatted; compilation and the diagnostic
result need CI evidence. No cause or fix is claimed yet.
