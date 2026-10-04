# 2026-10-03 Review creator journeys, not automated menu completion

Maxwell clarified the next usability pass: imagine an unfamiliar human creating
a MiniGame and events; reduce confusion, clutter and discovery time rather than
optimizing a robot's ability to dispatch known commands.

The [hardening plan](../audits/creator-and-npc-hardening-plan.md) now uses blank
setup journeys for a door/team gate, two-team game, exact-ball/copy ambiguity,
and checkpoint/puzzle diagnosis. Review evidence must identify visible cues,
likely interpretations, feedback and recovery at each step, including mistakes
and backtracking. Fix common task blockers and ambiguity before cosmetic polish;
do not assume a new collection of tabs automatically solves overload.

No source edits or tests were needed for this method clarification. Agent
inspection remains a heuristic; tests do not establish subjective ease or human
time-to-learn. Maxwell retains all interactive gameplay. New files and the plan
change are local documentation; implementation acceptance remains pending.
