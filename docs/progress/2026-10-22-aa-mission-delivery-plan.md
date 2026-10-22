# 2026-10-22 AA mission delivery scope

Maxwell clarified that gameplay delivery includes visuals/effects, sound,
animation, controls, menus/HUD, map interactions and lighting, mission rules and
competent but beatable NPCs. Bot traversal is the current correction, not the
entire mission milestone. Continue Pipeline, then use Interdiction to exercise
the shared foundation in co-op. Build/test and completed main integration remain
authorized; Maxwell performs interactive playtests.

The current private dependency/acceptance plan is `aa25/PLAN.md`. Older private
backlog, engine-needs and completion-plan documents now point to it and retain
their historical evidence. Source-ledger counts and old audit statuses are not
current completion percentages or instructions to recreate existing mechanisms.

Delivery order: bounded Pipeline traversal/defense correction; complete Pipeline
mission candidate across input, presentation, combat, objectives and lifecycle;
ordinary Interdiction mission with interruption/drop/recovery and co-op paths;
remaining high-value tactical, combat and presentation depth. Promote any later
dependency that blocks the current mission. Shared engine changes remain generic
and require ordinary Add-On consumers; map-specific placement and policy are
reverified on each map. The engine's broader alpha contract remains open.

Evidence at this checkpoint: Pipeline completed 660 game seconds, three ordinary
assault objective wins and resets. Spawn-recovery/guard changes are unmerged;
their 150-second trace shows movement from previously immobile defender starts,
but route quality and distribution remain under review. The prior full gate's
client-process access violation remains unresolved even though an isolated
single-threaded rerun passed all 739 client tests, including ignored cases.
Do not treat that rerun, raw movement distance or debug objective advancement as
complete mission acceptance.
