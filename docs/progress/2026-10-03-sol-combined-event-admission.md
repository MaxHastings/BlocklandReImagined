# Combined controller fixture: valid delayed terminal policy

The full gate at `9650a6b19178` repeatedly failed the ignored
`bot_combined_perf::sixteen_objective_controllers_and_mixed_inventory_combat`
case. Its mixed eight-latch phase made real objective/combat progress, but the
phase's empty-event-diagnostic assertion failed without exposing the error.

Bounded fixture instrumentation now counts every event diagnostic outside the
timed `Session::step`, retaining the first 16 samples with relative phase ticks
and at most 1,000 characters each. Acceptance still requires zero diagnostics.
The detailed retry (`/tmp/bri-v022-combined-event-diagnostics.log`, 81.56s)
confirmed exactly one error: mixed eight-latch phase tick 1599,
`host rejected winRound: RoundOver`. Concurrent controllers had completed
their own prerequisites within the authored 450ms delay; the later terminal
request tried to end an already-ended round. Native rejection was correct.

The fixture also authored 16 conditions on its 16-latch terminal rows, exceeding
the unchanged creator limit of eight conditions. The corrected ordinary rule
program derives per-player completion flags in groups of at most eight
independent prerequisites. Delayed score and win outputs require all completion
groups and `MiniGame.RoundOver = No` at execution time. Both outputs retain the
450ms delay. All 4/8/16 independent latch prerequisites remain; no engine
idempotence override, work limit increase, synthetic completion or known-failure
exemption was introduced.

Root's full ignored retry passed **1/1** in **144.81s** after a 1.07s fixture
compile (`/tmp/bri-v022-combined-valid-authoring-retry.log`). It retains the
4/8-latch real objective score/round-end witnesses, all three mixed phases'
actual fighting and canonical damage, the 16-latch bounded-model diagnostic
requirement, and zero genuine event errors for every phase. Passing this
fixture does not claim 16-action objective completion, Windows crash recovery,
shipped FPS or a measured performance improvement. Source and fixture are frozen
for root's remaining release verification.

The read-only developer-artifact inventory was paused for this gate blocker.
Nothing was deleted or cleaned; cleanup remains deferred until publication.
