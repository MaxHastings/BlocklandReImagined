# 2026-10-03 Final gate and new effects crash

Root integration; publication remains on hold. The Windows artifact-only build
from `9650a6b19178f877ab3239201a43ee2441229b8d` completed successfully, but its
full source gate did not. This is a temporary playtest package, not v0.2.2
publication or evidence that the latest source passed.

## Gate evidence

The full content-backed gate built, passed strict Clippy and startup checking,
then ran 340 test binaries. Two tests failed again when retried alone:
`bot_combined_perf::sixteen_objective_controllers_and_mixed_inventory_combat`
and `bot_objectives::an_objective_resumes_after_an_ordinary_combat_interruption_and_disconnect`.
Full log: `../.bri-gate/logs/9650a6b19178.log`. No exemption was added.

The combined test authored more conditions than the existing limit and did not
guard concurrent delayed wins against a round already ending. Its ordinary
authoring is corrected without changing runtime admission or relaxing outcome,
combat, damage or zero-error assertions. The complete ignored retry passed in
144.81s (`/tmp/bri-v022-combined-valid-authoring-retry.log`). Details are in
[the lane evidence](2026-10-03-sol-combined-event-admission.md).

The interruption fixture now supplies an actual low-damage armed attacker
through ordinary controls. This exposed a real target-selection defect:
retaliation could select the passive creator instead of the current attacker.
A narrow correction prefers valid dated injury through the existing visibility
query; it supplies no hidden live positions or extended evidence lifetime.
Independent source review is clean. Physical objective tests (14) and weapon
tactics tests (11) pass together after the correction
(`/tmp/bri-v022-injury-priority-physical-tactics.log`). The first objective retry still
failed its sustained interruption duration (3245 versus more than 3600 ticks).
Its ordinary opponent controller deliberately crowded to 1.6 units, inducing
continuous native Gun retreat until the unchanged 48-unit leash. A complete
late trace confirmed fresh injuries and boundary oscillation. The fixture now
uses ordinary movement to maintain a 5.5–6.5 unit ranged standoff rather than
forcing retreat beyond the authored arena. The 34-second duration and every
actual-injury, return-fire, attacker-selection, passive-author-health, longer
than 30-second interruption and canonical resume assertion remain intact.
All 11 objective tests pass in 0.85s
(`/tmp/bri-v022-objectives-valid-standoff-retry.log`).

## New duplicator crash report

Max supplied `client-20261003-225659-874.stderr.log`, identifying the Windows
`9650a6b1` temporary package and NVIDIA RTX 4070 SUPER / DX12. Startup succeeds;
steady frames are approximately 159–165 FPS with several isolated long frames.
The terminal error is twice `Invalid effects instance`, rather than the earlier
duplicator triangle-budget failure. The renderer rejects a particle with an
invalid texture layer, nonfinite position/color/size/axis/spin or negative size.
The stderr does not identify which field, particle or emitter caused it.

Source investigation found a concrete lifecycle defect: successful Add-On
installation replaced WeaponEffects and ActorEffects CPU packs but retained
the old GPU texture atlas. Both actual Duplicator packs add UI-icon textures
absent from the native atlas. Their particle sizes are valid and emitter-size
overrides are disabled. A valid newly appended texture index therefore produces
the reported validation error against that stale atlas. The original stderr
lacks particle fields and reload history, so exact attribution of this user
session remains unproven; the defect and matching failure are reproducible.

Successful installation now invalidates only the effects renderer; the next
render preparation recreates it from the current pack before main or secondary
views. Failed and cancelled preparation preserve the current renderer. Device,
scene, map and other pipelines are not restarted. Genuine invalid instance data
remains rejected, with texture/layer and finite-field diagnostics for future
reports. Independent source review found no material defect.

Initial focused GPU contract tests passed 2/2 in 3.32s
(`/tmp/bri-v022-effects-atlas-regression.log`); the actual client installation
regression passed 1/1 in 3.13s
(`/tmp/bri-v022-app-effects-reload-regression.log`). The strengthened tests now
include secondary-view preparation and equal-count replacement with changed
pixels. Final GPU tests pass 2/2 in 3.42s
(`/tmp/bri-v022-effects-atlas-final.log`); the client installation test passes
1/1 in 3.53s (`/tmp/bri-v022-app-effects-reload-final.log`). Workspace formatting
and strict all-target Clippy pass, the latter in 47.24s
(`/tmp/bri-v022-effects-final-clippy.log`). The next immutable source still needs
the full gate, Windows CI and three-platform packaging before publication.
Original content and interactive gameplay are untouched.
