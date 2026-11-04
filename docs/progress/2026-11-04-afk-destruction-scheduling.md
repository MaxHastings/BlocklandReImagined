# AFK destruction scheduling regression

The preview.2 head (539074e1b) passed all seven executed platform CI checks.
It remains held in #437 pending Maxwell's judgment of the motion. The AFK
validation found additional failures outside the original 60-Hz/default-budget
repro: rigid-body limits0,1 and64 saturated fallback catch-up time, and a30-Hz
frame could effectively reach the same cap. A nearly saturated float could
slip through the original strict `<0.1` comparison. The assertion now rejects
values within float epsilon of that cap; reports are flushed before asserting.
No failure was made green by weakening a requirement.

Two scheduling faults were confirmed. A chunk loop moved its start by only
one after exhausting work, repeatedly serving a busy prefix while tail clumps
waited. Collision preparation also always began at the same index. Separate
preparation/motion cursors and the collision group cursor now continue after
the last actual work served. The contact/planning cursor remains independent.
This fixes the60-Hz, zero-body Pirate repro without increasing its budget.

The remaining30-Hz failure had a clock mismatch: every frame got512 queries
but had to move twice as far at30Hz. Query allowance now follows the local
world's existing fixed-step accumulator,256 equivalents per step, bounded by
its existing maximum catch-up steps. The normal60-Hz allowance stays512;
30Hz gets1024 and120Hz gets256. A zero-step frame may prepare one quantum.
The work remains bounded; body, proxy, lifetime, displacement and catch-up-time
limits are unchanged. The current-frame rubble snapshot and completed atomic
collision step remain intact. New cumulative diagnostics expose preparation
work, motion work and completed fallback steps.

All34 focused section tests pass, including a new513-clump fixture with no
rigid bodies. Its lower foot contacts first; every upper clump must receive
collision service and fall, including the tail, without exceeding the query
envelope. This is a functional fairness regression, not a wall-clock benchmark.

Both exact two-stock-rocket saves pass in each of these final configurations:

| Client Hz | Body limit | Tower maximum debt (ms) | Pirate maximum debt (ms) |
| --- | --- | --- | --- |
|30|512|0|33.333|
|120|512|8.333|33.333|
|60|0|66.667|50|
|30|0|66.667|66.667|
|60|1|16.667|33.333|
|60|64|16.667|33.333|

Every run fires exactly two stock projectiles and drops zero authority cues.
Server ticks remain120Hz; only client cadence and the existing quality limit
vary. Original inputs/camera diagnostics are unchanged. Baseline failures and
final JSONL/summary evidence are retained separately under
`E:\Downloads\BlocklandReImagined-Release-Evidence\afk-validation-2026-11-04`.
Fourteen default-budget offscreen real-geometry captures also pass; representative
first-contact/breakup frames were inspected. That is technical/visual evidence,
not Maxwell's interactive acceptance or source-game fidelity.

Final scoped clippy/format/docs pass. Optimized map-load measurements and
manifest/startup-verified preview.3 are recorded in the
[preview handoff](2026-11-04-v044-preview3-handoff.md). New-head CI remains
required before any merge; #437 additionally stays held for motion judgment.
The broader vehicle and loading work
has its [own receipt](2026-11-04-afk-vehicle-and-load-probes.md). The new
standalone map-load CLI is explicitly opt-in rather than an empty test harness.
The external private-save repro is classified alongside existing manual gate
probes; it was actually run in all configurations above. Synthetic regression
assertions remain in the ordinary suite.
