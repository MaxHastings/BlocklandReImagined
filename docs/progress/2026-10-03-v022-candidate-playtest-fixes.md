# 2026-10-03 v0.2.2 candidate playtest fixes

Maxwell tested the PR candidate while packaging was underway. Optional polish
remains stopped; the new reports are concrete usability and gameplay defects.
All interactive observations below are Maxwell's. Agents used source inspection,
headless input dispatch and bounded offscreen rendering only.

## Previous candidate validation

The complete local gate passed `8f1ea9de4fcf779f7bb3ba9ddacf95354fb2b1af`
in 384 seconds: build, strict clippy, packaged content check and 340 test
binaries. `BRI_CONTENT` and `BRI_MODERN_BUNDLE` pointed at the main checkout's
generated content. An earlier gate invocation lacked `BRI_CONTENT`, so five
content-dependent tests failed; that invocation was not a pass. The corrected
gate retried one native item-render test which passed alone and was recorded
as flaky under the existing policy. The save corpus was unavailable on this
Mac and skipped explicitly. Logs: `/tmp/bri-v022-final-gate-content.log` and
`../.bri-gate/logs/8f1ea9de4fcf.log`.

That receipt does not cover the following candidate corrections. Windows CI
and artifact build for this SHA are intermediate until the final fixes pass.

## Colorsets

The chooser rebuilt its entire View on every UI update, including ordinary
performance frames. An update between pointer down and pointer up erased the
pressed control, losing the click. It now retains the View unless the actual
colorset catalog changes. Selection remains a local draft until Use; Cancel
does not save it. The new regression dispatches real pointer events across
frame/catalog updates for selection, Use, Cancel and Folder at both 400x300
and 1024x768. All five chooser tests passed in
`/tmp/bri-v022-colorset-pointer-tests.log`.

Pure converted palettes used to appear in the Add-Ons toggle list even though
selection in Colorsets is independent of those switches. Presentation now
omits only healthy, palette-only imported entries with no declared gameplay,
client code, dependencies, roles, companions or additional indexed assets.
Names and package identities are unchanged. Mixed and damaged entries remain
visible. An unfamiliar-name regression verifies this and independent chooser
discovery; it passed in `/tmp/bri-v022-colorset-presentation-test.log`.

## Native event button appearance

The v0.2.2 authoring helper changed from v0.2.1's rounded bitmap buttons and
Block font to rectangular small buttons to avoid bitmap scaling. Maxwell
preferred the previous appearance. The helper now uses the exact v0.2.1
`BlockButtonProfile` and `base/client/ui/button2` artwork again for Explain,
+IF, Copy row, Remove row and condition X. Layout, draft behavior and WHEN/IF/DO
semantics stay as implemented. Twenty-seven Wrench tests passed, with two
content-dependent tests explicitly ignored by that invocation; the actual
generated-content offscreen Wrench test then passed separately across the
existing resolution/scale matrix. Logs: `/tmp/bri-v022-wrench-style-tests.log`
and `/tmp/bri-v022-wrench-native-style-render.log`. Root inspected the rendered
mixed-event dialog and confirmed the restored artwork.

## Gameplay and lighting reports under investigation

Maxwell observed weak hands/hammer object delivery and distraction toward the
player, but also observed a bot using the Gravity Gun to deliver the ball and
win the authored objective. This establishes a useful human success for that
tool path; it does not excuse weak contact execution or objective commitment.
The NPC lane is investigating those shared control/arbitration boundaries,
without introducing content or game-mode-specific solvers.

The Dynamic lane is investigating intermittent black flicker. A shadow-cache
unavailability sentinel can currently suppress recovered lamps while a cohort
refreshes, but this is not yet claimed to reproduce Maxwell's exact flicker.
The fix and bounded multi-light regression must be validated before handoff.
