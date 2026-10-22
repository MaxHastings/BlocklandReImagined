# 2026-10-22 Allied bots yielding in narrow passages

The ordinary private Pipeline trace showed two oncoming allies requesting the
same sideways passing maneuver into opposite walls of a passage too narrow for
both bodies. `bot_walk_direction` already paced a file going the same way, but
oncoming pairs fell through to lateral passing indefinitely. Its blocker test
also ignored height, so an actor on another floor could obstruct a clear walk.

The correction uses authoritative vertical body bounds and measures the existing
side-ray distances against both bodies' widths. An oncoming allied pair that
cannot pass gets a stable priority; one advances and the other backs along the
passage through ordinary movement and existing safety checks. A human or idle
ally is not assumed to reverse, so the navigating bot yields. Same-direction
pacing and lateral passing where both bodies fit remain available. No content
names, forced positions, teleports or new navigation mechanism enter the engine.

## Evidence

- The initial three component regressions fail for the oncoming pair and the
  actor on another floor, reproducing the observed passing vectors. Open-space
  passing already succeeds.
- The correction passes those cases plus a wider corridor with nearby walls
  and moving/idle ally cases. `python tools/check.py quick --base be2bf43fc`:
  format and 417 sim unit tests pass (four existing ignored long cases).
- A new ordinary 780-game-second Pipeline run uses the actual mode, normal
  roster, controls and objectives without position fixtures. Assault wins at
  186.03 and 310.03 seconds, with resets at 198.03 and 322.03. Objective work
  occurs and there are zero rule/bot failures. All 15 bots move beyond spawn;
  attackers visit 97-207 sixteen-unit cells over their lives.
- The original pair's long meeting stalls do not recur in this run: Anderson's
  worst trying/no-progress interval is three seconds, Grant's zero. This does
  not prove every possible meeting or complete map coverage.

Local logs: `passage-baseline.log`, `passage-fixed.log`, `passage-quick.log`
under `.research/pipeline-integration/`; private `passage-yield-rounds.*` and
`passage-yield-summary.*` under the current round-flow candidate root.
Runtime content is private revision `8f0f295f2`; engine source is this correction
on `be2bf43fc`. Focused checks pass (Clippy with warnings denied and the changed
crates' integration tests and dependents, 817 seconds). The integrated gate and
Windows CI remain required to land.

## Still open

This fixes the passing defect, not every navigation delay. The new trace exposes
a different attacker with a 54-second exploration pause and defenders searching
for 28/16 seconds after changing floors. Other movement stalls reach 10/7 seconds.
These remain active, separately classified issues. Exploration's sparse random
far-point scan and route search are the next investigation; their causes are
not established by the passing regression. Difficulty, presentation and human
playtest acceptance remain open.

A repeated 620-game-second private run adds the existing `sight_starved` thought
field to its trace without changing policy or controls. The same Baker pause at
491-544 seconds has no goal, no search and **no refused sight queries** in all
54 samples. The ordinary sight budget is not the explanation for that pause;
exploration's candidate discovery is the next bounded investigation. The run
passes with objective progress, two assault wins, reset and zero bot failures.
Log/trace: private `explore-budget-rounds.*`. This is diagnostic evidence, not
another navigation correction.
