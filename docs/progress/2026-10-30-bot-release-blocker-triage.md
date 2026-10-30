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
  versus enemy harm 20.733315; no teammate kill. The traced plans credit the
  parent blast's full damage throughout the fragment safety envelope.
- `bot_hardening`: allied file on a deck loses two bots (existing limit one).
  Both traced falls follow long static-floor waypoints while riding allies'
  heads above the deck; retain the existing fall and real-fight assertions.
- `bot_objectives`: the cheaper region 100 units above the floor continually
  wins over the reachable alternative, score zero instead of 43.

The last case has a source-level lifecycle defect: `navigation_failed` gives
up an action for `UNREACHED_TICKS` and schedules planning for exactly that same
tick. Pruning therefore removes the failure before the next choice, so the
cheaper unreachable action can win indefinitely. Schedule replanning on the
existing `REPLAN_TICKS` cadence while the short three-second action exclusion
is still live. Keep the short carrier retry and authored semantic-failure waits;
do not restore the old minute-long navigation exclusion. The retained existing
  inaccessible-region integration is the acceptance test. Root's exact combined
  `4e19a2f7e92c22bec6826ed2d150fc0b6e28de75` rerun passes the original score-43
  assertion with final content after this correction.

Opt-in `BRI_BOT_FAILURE_TRACE` adds readonly test evidence for the other two
cases: grenade birth/actual fire-plan and friendly-damage records, plus bounded
recent deck movement/controls when the scorer observes a fall. No assertion,
seed, layout, controls or runtime policy changes for those cases. The initial
formatter command incorrectly requested edition 2021; corrected to repository
edition 2024 with child traversal disabled. No compiler/test pass is claimed.

## Trace-supported remaining corrections

Root serialized the three exact cases with the opt-in traces under
`bot-release-trace-4e19a2f7e/`. Grenade and deck failures repeat unchanged.

The grenade loss is self-damage: bot 7 takes 50.422626 at native tick 4752,
bot 5 takes 47.420345 at 8434. Their throw plans already price own harm at 60
but claim enemy benefit of 180 and 120. `harm::shot_harm` uses the full parent
explosion damage for every body within `radius + danger`; `danger` is the
possible child-fragment reach, not a guaranteed parent explosion. Here those
three random 5-damage fragments extend a four-unit blast by 2.2 units. Charging
their whole reach as guaranteed 60-damage enemy benefit manufactures favorable
trades. Keep that conservative envelope for own/allied risk and the swept
danger shape, but price enemy benefit using the parent's actual blast falloff.
New unit coverage checks an enemy outside the blast earns no parent-blast
credit while own/allied bodies keep the conservative 60 risk. Existing inside-
blast falloff coverage remains. No weapon/damage/delivery or trade tuning.

The deck's two losses are bot 5 (spacing 1, z 54.5) and bot 4 (spacing 2, z 55).
They follow floor waypoints at y 6 while grounded on allies at y about 8.66,
moving roughly seven units/second toward z 55.5. Leaving the moving support near
the deck rim z 56 carries them beyond z 58 before they land. The existing
nearby-floor step-off only overrides a lower route waypoint when it is already
close, which is too late here. Apply that same step-off to a lower walking
waypoint whenever standing on a body, even if distant: get onto nearby clear
world floor before following the long static route. If there is no clear
landing beside that support, wait rather than following the mismatched-height
route. Preserve the existing nearby ledge behavior when not standing on a body.
No teleport, physics tuning, authored-layout exception or fall-limit change.

These last two corrections are formatter/diff checked only. Root must compile
and rerun the original three acceptance cases, the adjacent stalled-deck case,
harm units and relevant carrier/interaction controls before claiming success.
No Cargo, native build or main push ran in the bot checkout.
