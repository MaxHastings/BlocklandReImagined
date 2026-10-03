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

The Dynamic lane supplied `47782eb1`, integrated as `f24fedb0`. A shadow-cache
unavailability sentinel suppressed all recovered lamps while a cohort refreshed.
Freshness is now separate from availability: identical light projectors retain
previous runtime shadows during bounded geometry refresh; new sources are
temporarily unshadowed until their runtime faces are ready. No baked illumination
is sampled. The unchanged budget can leave up to five frames of old geometry
shadow faces during refresh, or temporary light leakage on first use.

Root reproduced the defect against the pre-fix `shadow.rs` and shader with the
new frozen 24-lamp, 128px regression: the first geometry-refresh frame fell from
brightness 185 to zero and failed. Exact committed sources were restored in a
`finally` block. The fixed pure cache test passed, and the exact offscreen
regression passed both before and after that negative check. The full lighting
suite passed 17 tests with one generated-content test explicitly ignored by that
invocation. Earlier all-map modern validation remains recorded separately.
Logs: `/tmp/bri-v022-cube-cache-regression.log`,
`/tmp/bri-v022-dynamic-refresh-prefixed-reproduction.log`,
`/tmp/bri-v022-dynamic-refresh-restored-test.log`, and
`/tmp/bri-v022-dynamic-all-lighting-regressions.log`.
This proves a blackout mechanism and its correction, not that every instance
of Maxwell's reported screen flicker has the same cause.
Render all-target strict clippy also passed, recorded in
`/tmp/bri-v022-dynamic-refresh-clippy.log`.

Maxwell subsequently reported intermittent charged-spear restart while tracking
a moving player. The weapon lane is investigating shared charge/release intent,
with the requirement to retain a charge while waiting for a valid shot, while
still respecting native fuse/release and real cancellation semantics.

## Pong report resolved by the playtester

Maxwell initially reported multiple balls and every paddle cell activating.
Root ran all nine native/synthetic Pong regressions with ignored tests included;
all passed, including loaded-palette repaint, paddle limits, rally, scoring,
reset and timed restoration (`/tmp/bri-v022-pong-native-regressions.log`).
The source audit identified a coverage limit: these tests inject brick inputs
and do not prove the ordinary human click path. They were not used to dismiss
the report. Maxwell then confirmed he had loaded both Slate and Bedroom Pong
builds into the same world. Their same-named relay bricks share the builder's
named-target group, so both contraptions receive the same relays. He withdrew
the bug report. No Pong-specific production behavior or naming rewrite was added.

## Cosmetic destruction feedback

Maxwell reports that many destroyed local brick bodies barely move, remaining
visually in the original wall while subsequent authoritative rockets pass
through. The local debris lane is examining initial and later blast impulses,
preserving cosmetic-only physics, resource limits and network behavior.

## Cosmetic blast correction and focused evidence

Local debris now biases surface explosions away from the surviving wall, with
a bounded upward lift and stronger edge impulse. Small authored throws preserve
their earlier behavior. The existing frame consumes a bounded, deduplicated queue
of real explosion effects, so a subsequent blast can move existing debris even
when it breaks no new bricks. No server collider or physics synchronization was
added, and debris counts, retirement limits and the velocity cap stay bounded.

All 18 debris tests passed in
`/tmp/bri-v022-debris-blast-regressions-retry.log`, including outward clearance,
explosion-only wakeup, queue limits, duplicate avoidance and unchanged small
throws. The first compilation exposed four test-only dereferences of Rapier's
by-value velocity; root removed the unnecessary dereferences, preserving every
assertion. This improves the cosmetic mismatch; local debris still cannot stop
a server projectile.

## Shared-name load warning

Loading named bricks into an existing builder group now checks the event runtime's
named-target index using the same save ownership mapping as the actual load.
A loader-only notice says: “Shared brick names: events can affect both builds.”
Names are not rewritten, loading continues, and intentional relays between builds
keep working. The normal LoadBuild regression passed for independent builder
groups and a duplicate same-builder name, with all three bricks and authored
names intact: `/tmp/bri-v022-shared-name-warning-test.log`.

## Native body delivery and charged combat checkpoint

The physical-objective suite passed all 13 tests in
`/tmp/bri-v022-native-body-delivery-final-tests.log`: heavy native hand contact,
ordinary Hammer delivery, passive armed creator retention, actual attacker
retaliation with a canonical credited death, long horizontal held-object delivery,
real exit/reentry, delayed outcomes and revoked guards. Hands now request ordinary
Activate/release controls from the delivery side; Hammer uses the existing native
tool callback and permission/ray path. Tiny jitter no longer renews progress.
The capability/claim lifecycle renews only on observed credited physical progress.
Grounded horizontal holding is covered; jet-assisted object transport is not.

All nine charged-combat tests passed in
`/tmp/bri-v022-charged-combat-final-tests.log`. An unfamiliar renamed 84-tick
charge graph held Armed for 278 ticks while tracking a moving player, then fired
a real damaging projectile. Replacement and target disconnect cancel; a queued
release during spawn protection cannot fire. This checkpoint is followed by two
independent-review corrections: real out-of-range hurt must pause delivery so
existing pursuit can run, and indirect native trigger-up chains must receive the
same post-frame release safety check. Their final evidence is recorded below
after verification.

## Final independent-review corrections

Independent review found two genuine boundaries before publication. A dated
actual attack had unblocked combat but noncombat Objective utility still beat
Chase/Search outside weapon range. The existing retained action is now temporarily
withheld from selection during an unexpired, authorized threat within the
existing leash and with a usable attack. Its completion baseline remains, and
ordinary interruption pauses only approach time. A passive opponent does not
trigger this behavior. The actual Hammer-only bot fixture receives one normal
ranged hit outside its reach, pursues, swings and earns a canonical credited kill.

All 14 physical tests passed in
`/tmp/bri-v022-final-physical-14-retry.log` (1.48 seconds), including a 170-unit
grounded Hold whose measured duration exceeds 1800 simulation ticks (15 seconds).
The first final invocation passed 13 and failed one before injury because the
authored Gun supply brick was off-grid and not admitted. Its coordinates were
corrected to a valid plate center; pickup, damage, pursuit, Hammer-only inventory
and exact killer assertions were preserved. No production change was used to
manufacture that outcome.

Native image runtime can follow more than one trigger-up edge in a single step.
The former immediate-edge release check therefore missed an indirect release.
Admission and real cancellation now share bounded graph reachability, including
pending releases whose button is already up. Actual cooldown remains native.
All 11 ordinary-control tactics tests passed in
`/tmp/bri-v022-final-tactics-11.log` (0.23 seconds), and both finite charge-graph
unit tests passed in `/tmp/bri-v022-final-charge-units.log`. The independent
reviewer closed both source findings; final repository/platform checks follow.

The palette-only presentation guard also confines hiding to folders the chooser
actually discovers. Its initial follow-up test exposed macOS temporary-path
canonicalization (`/var` versus `/private/var`); both folder paths now use the
existing canonical package-directory resolution. A listed palette under an
unrecognized library folder stays visible instead of becoming inaccessible.
Sixteen Python tool tests passed in `/tmp/bri-v022-final-tool-tests.log`; four
packaged guides have eight local links and no broken targets.

The final palette presentation regression passed in
`/tmp/bri-v022-final-palette-guard-final.log`. Its second setup attempt used an
invalid package-side spelling and therefore produced an empty library; changing
the fixture to the existing `shared` enum preserved all visibility assertions.
Final format and diff-whitespace checks passed.

Workspace all-target strict clippy passed in 44.71 seconds after replacing an
unnecessary test-only clone with `std::slice::from_ref`; final rustfmt and diff
checks passed. Log: `/tmp/bri-v022-final-workspace-clippy-retry.log`.

Maxwell then supplied third-party bulb/Kitchen and dresser-shadow refinement
feedback, with an otherwise positive room screenshot, and asked whether to leave
it for now. Root recommends a separate later fidelity pass on source-light
recovery and material behavior. This does not reopen optional lighting redesign
or promise that six additional lights are the correct solution. The verified
lamp-refresh blackout correction stays in this release.
