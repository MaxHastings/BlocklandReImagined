# 2026-10-30 Bot release blocker triage

Release coordinator requested a bounded correction of three repeatable failures
on final candidate `60fcbe7f4597da5d88c26d93843b0605b06cc742`. This follows the
completed [gameplay correction set](2026-10-29-bot-gameplay-polish.md); it does
not reopen the general bot campaign. No Cargo/builds run in this checkout and
no main push is authorized. Root owns exact-source builds and isolated reruns
against the actual regenerated final release data in its existing warm target.

Original full-run evidence is coordinator artifact
`full-gate-60fcbe7f4597/attempt-1-cache-socket-failure/actual-test-section.log`;
all three fail again alone in `isolated-failures-60fc/`:

- `acceptance_unfamiliar`: Variant(1), team 2 friendly grenade harm 97.84297
  versus enemy harm 20.733315; no teammate kill. Cause not yet established.
- `bot_hardening`: allied file on a deck loses two bots (existing limit one).
  Cause not yet established; retain the existing fall and real-fight assertions.
- `bot_objectives`: the cheaper region 100 units above the floor continually
  wins over the reachable alternative, score zero instead of 43.

The last case has a source-level lifecycle defect: `navigation_failed` gives
up an action for `UNREACHED_TICKS` and schedules planning for exactly that same
tick. Pruning therefore removes the failure before the next choice, so the
cheaper unreachable action can win indefinitely. Schedule replanning on the
existing `REPLAN_TICKS` cadence while the short three-second action exclusion
is still live. Keep the short carrier retry and authored semantic-failure waits;
do not restore the old minute-long navigation exclusion. The retained existing
inaccessible-region integration is the acceptance test; execution is pending.

Opt-in `BRI_BOT_FAILURE_TRACE` adds readonly test evidence for the other two
cases: grenade birth/actual fire-plan and friendly-damage records, plus bounded
recent deck movement/controls when the scorer observes a fall. No assertion,
seed, layout, controls or runtime policy changes for those cases. The initial
formatter command incorrectly requested edition 2021; corrected to repository
edition 2024 with child traversal disabled. No compiler/test pass is claimed.
