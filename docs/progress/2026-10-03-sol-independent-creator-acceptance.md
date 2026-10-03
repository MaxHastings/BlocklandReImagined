# 2026-10-03 Independent creator objective acceptance

This entry freezes the independent fixture intent before implementation or a
test run. It follows the linked-pipeline audit and the v0.2.2 delivery contract;
the fourteen-case acceptance contract remains authoritative. No new fixture has
passed at this entry's creation. The existing UI/music/JPEG work is frozen and
its focused validation remains recorded in its separate entries.

## Ownership and execution boundary

This lane owns only a new `crates/chaos/tests/bot_creator_acceptance.rs` and this
entry. Root owns shared integration and the bounded observer of actual
MiniGame `RoundEnded` effects. Performance owns the single objective/action
adapter and provider diagnostics. Existing planner, navigation, activation,
rule scheduler and MiniGame mechanisms execute the fixtures. There is no test
brain injection or a parallel rule interpreter. Setup authors builds and
settings normally; after setup, NPC locomotion/activation and sensors must
cause real rule progress. Author edits use existing authenticated/trusted-host
edit paths, not direct variable/score/position mutation.

## Frozen fixture intent

Each positive below runs two equivalent worlds with different actor-kind IDs,
authored labels/variable keys, brick insertion IDs/order, translated/rotated
layout and unrelated decoration. A nearer tempting source has an unsatisfiable
canonical guard; it must never become successful merely because it is nearby.
Transforms preserve the task's physical feasibility, not its coordinates.

1. **Ordered checkpoints (#6):** four spatial player-entry sensors advance a
   player-scoped stage only in order. Delayed effects are observed before the
   next checkpoint. Guarded per-source ColorFX witnesses prove actual canonical
   progress, the final real entry awards the bot and ends its round, and the
   actual winner observer identifies that bot. A nearer out-of-order source and
   the remote author receive neither progress nor awards. Completion cannot be
   inferred from approaching a point or from score alone.
2. **Activation puzzle (#7):** four distinct switches set independent scoped
   flags through ordinary activation; a guarded final action requires every
   flag. Actual per-source witnesses, bot movement, score, real round-end and
   exact winner identify completion. Neither author nor decoy receives credit.
3. **Active edits (#13):** acquire a remote action, change its authored rule and
   destination/source before admission, then require a different real action to
   complete. The stale source cannot award or end the round. A separate team
   case changes the bot's team through the authenticated MiniGame request after
   acquiring a team-qualified goal; only the newly applicable team's goal may
   complete. These are execution cases, not pure cache-key unit tests.
4. **Honest negatives (#14):** an unreachable elevated sensor alone must not
   award, signal success or move the actor synthetically; an unknown collateral
   effect must reject its entire promising action; a sixteen-step dependency
   chain outside depth12 and an oversized grounded action model must stop with
   precise bounded failure information and no successful prefix. Existing
   Explain/thought output, rather than a new debug UI, supplies the reasons.

Canonical results are independent assertions: motion/actual sensor or
activation progress, player-scoped rule witnesses, score and actual
player/team winner effect. No assertion will silently replace the winner with
score or the physical journey with a projected action list. A missing observer
or unsupported mechanic is a production integration gap, not a skipped test.

## Full-contract review and held-out case

The other journeys remain independently reviewable: physical contact, cheaper
declared hold, elevated alternative, exact object with identical decoys, its
own control seat, elimination/search, typed item return, competition, replacement
and tool disappearance. Existing combat damage/ammo or corridor-clearance
fixtures are narrower evidence and do not complete these items. Provider owners
supply their actual-control fixtures; this lane challenges their source
assumptions, metamorphic strength, ordinary execution and canonical outcomes.

After generic provider implementation freezes, a separate held-out composition
will be specified here before running it. It will combine existing mechanics in
a task absent from Maxwell's examples, state desired outcome only, and require
no prescribed plan or production identifier branch. A failure diagnoses the
abstraction; its intent must not be weakened to fit the code.

No Cargo command or visible gameplay was run for this entry. Builds stay
serialized through root. Initial implementation and concrete failures/results
will be appended here after the frozen fixtures are reviewable.

## Initial fixture source, awaiting serialized verification

The new file now contains eight tests (eighteen scene/timing runs). Positive
worlds use different made-up bot-kind IDs, labels and scoped variable keys;
their entire layout rotates/translates, authored insertion order reverses,
runtime source IDs change, and irrelevant decoration counts change. The final
checkpoint is closer than the first and must be re-entered after actual ordered
progress. Both team-change timings run in both worlds.

The unknown-effect case installs a tiny invented Add-On through the normal
catalog loader. Its output name changes with the world; its opaque callback
writes a visible package flag. The fixture checks neither that callback nor any
supported win/score prefix executes. The same fixture package declares one
inert team setting to enable authenticated team creation/assignment; it has no
team gameplay callbacks. It does not supply a solution or mutate bot intent.

Root's `Session::round_results()` observer now supplies canonical game/round/
tick, actual player IDs, resolved owner IDs and team IDs from existing
`RoundEnded` effects. Positive assertions require exactly the real bot owner
and its applicable team, not a marker interpreted as winner identity.

The negative diagnostics remain strict: sixteen dependent inputs require a
depth explanation, thirty-three grounded actions require an action-bound
explanation, and opaque behavior requires unsupported semantics. An unreachable
sensor may be honestly rejected before approach or time out after real controls;
the fixture does not prescribe an attempted plan. Coarse older diagnostics
would fail the new precise bound assertions. Provider integration is underway
in the separately owned production lane.

Touched-file `rustfmt --edition 2024` and `git diff --check` pass. No Cargo
verification, fixture success or full fourteen-case completion is claimed yet.

## Linked creator verification

Root ran the frozen creator fixture after linking the shared typed controls,
package provider and canonical result observers. The recorded output in
`/tmp/bri-v022-linked-creator-shark.log` reports all **eight creator tests
passing**, with no failures or ignored tests (0.84 seconds). This includes the
paired ordered checkpoints, puzzle, live rule/team changes and strict bounded
negative cases described above. The same log reports the existing eleven
`bot_objectives` tests passing (0.56 seconds). This is scoped creator evidence,
not acceptance of every provider or all fourteen journeys.

Exact command: `cargo test -p bri-chaos --test bot_creator_acceptance --test
bot_objectives --test shark_policy --locked -- --nocapture`, redirected to the
log above. The command's complete batch failed on the separate Shark test;
the individual creator and legacy-objective results are the passing evidence.

The batch also contains a separately owned Shark resume failure and one ignored
original-content test. Those results do not support a whole-batch pass. Physical
and intended-enemy provider integration is still being completed; the held-out
composition remains unimplemented and unrun until those generic providers freeze.

The independent read-only control audit flagged moving-point push claim renewal,
the distinction between a selected elimination participant and another visible
combat target, and hand-trigger cleanup during player-ridden suspension. These
are source-grounded integration concerns handed to the relevant owners, not
claimed reproduced failures or fixes in this lane.

Two further source checks sharpened that review. The selected Enemy branch calls
the ordinary pursuit helper with `fight=true`; that helper clears the movement
goal whenever its participant is visible, even outside the weapon band. Since
default Objective utility exceeds distant Chase utility, that path can stop
before reaching attack range. Also, exact-resource claim equality allows a
Body hold and control Seat claim on the same wheeled object simultaneously;
the former seatless push assumption does not cover the newly declared hold
method. Performance received both findings before provider freeze. Their
disposition and actual-control regression evidence are still pending.

Root subsequently wrote shared-control corrections: a newly visible different
hostile yields the selected Enemy view to ordinary combat; intended-enemy
pursuit checks the normal weapon band/height and retains its hold/backoff
movement; rider suspension clears native fire gating, objective-tool ownership
and trigger/charge state. Performance is adding actual credited physical
progress for shared lease renewal. These are reported source corrections,
**not yet independently compiled or verified regressions** at this checkpoint.

## Frozen held-out and adversarial follow-up intent

Root approved two additional independent files after the generic package/brick
mechanisms passed their focused checks. `bot_creator_heldout.rs` requires an
invented package's real pickup and zone return to unlock a guarded multi-switch
brick policy, followed by a delayed canonical bot round win. Package return
alone never scores or ends the round. Two equivalent renamed/rotated/reordered
worlds retain a nearer locked decoy. Assertions require actual carriage/return,
every brick witness, the exact score and actual winner, with no prescribed plan
or production code tailored to the composition.

`bot_creator_adversarial.rs` challenges exact-object identity using two identical
bodies, real selected-object progress before ordinary authored replacement,
authored removal of a hold tool only after genuine grip/displacement, and
same-object contention between two bots. Successful behavior must use actual
movement, new incarnation identity and canonical effects; negatives cannot
inherit a stale winner. Setup may author content/rules, while later mutations
use ordinary authorized edits or package callbacks. Fixture intent is frozen
before implementing or running these files. None has passed at this checkpoint.

The first coordinated build stopped before running either new fixture because
the held-out authoring helper used the nonexistent `EventValue::String`
variant. `/tmp/bri-v022-independent-heldout-adversarial.log` records that sole
compile error at line 216. It now uses the actual `EventValue::Text` variant;
the fixture's world, controls and outcome assertions are unchanged. Touched-file
formatting and `git diff --check` pass. Behavioral results await the rerun.

A pre-rerun API review also corrected the switch's `setVariable` output target
to Self. The canonical catalog exposes that action on a brick; its first
parameter still selects Player scope. The two flags, guards, delays and actual
witness requirements are unchanged. Root approved the narrow fixture correction
before compiling. No production capability was added to fit this composition.

## Final entry-document review

The current creator guide and pipeline audit distinguish supported primitives,
remaining unknown semantics and focused evidence from release acceptance. The
native captures reviewed earlier still show compact plain rows, grouped choices,
direct Teams access, draft-safe colorset selection and the same-row Region/Respawn
actions; this review did not launch gameplay or rerender them.

Source comparison identified stale universal projectile-IF rejection and
builder-only bot alliance claims; root corrected those and clarified the frozen
acceptance note's historical status. The approved current passages in
`architecture/bots.md` and `rule-workshop/DESIGN.md` now describe linked
brick/physical/enemy/package executors, ordinary controls, actual result
observation and precise limits. The current pipeline audit is the primary
handoff; the initial spike stays linked as history. These documentation changes
do not establish a held-out pass or full fourteen-case completion.

## First independent runtime attempt

`/tmp/bri-v022-creator-final-adversarial.log` records all four new adversarial
tests failing. Exact-identity contact and post-progress replacement did move the
intended body, but did not admit the goal or produce a canonical winner. The
first contact body settled about 3.3 units laterally from the three-unit-wide
goal. Its identity, geometry, motion and required outcome remain unchanged;
bounded actual actor/body/goal/control samples were added to diagnose the miss.
This is not yet evidence that identity filtering or replacement repair failed.

The two grip cases reached their final strict empty-diagnostic assertion with
an `Inventory full` warning. The copied grip package enabled its optional
outside-MiniGame auto-grant hook while the author still had full stock slots.
The fixture already grants its instrument through the normal MiniGame loadout;
its authored metadata now disables only that unrelated auto-grant hook. Native
grip commands/script, real progression, removal and canonical assertions remain
unchanged. The warning was not filtered or ignored.

The separate held-out run in `/tmp/bri-v022-independent-heldout.log` stopped at
package compilation: Rhai reserves `switch`, used as a loop variable in the
invented policy. It now uses `panel`; callback effects and required outcomes are
unchanged. Neither held-out world has executed yet. Root coordinates the next
run; no fixture pass or production repair is claimed here.

## Held-out outcome and adversarial split

Root ran `cargo test -p bri-chaos --test bot_creator_heldout --locked --
--nocapture`; `/tmp/bri-v022-heldout-panel.log` reports **one test passing**
(0.51 seconds). That test executes both frozen renamed/rotated/reordered worlds.
Real package pickup/carriage/return unlocks the independent guarded switches;
ordinary delayed brick inputs produce score 97 and the canonical bot winner.
The package itself never scores or ends the round. Only authoring/API corrections
were made to this fixture; no production provider was added to solve it.

`cargo test -p bri-chaos --test bot_creator_adversarial --locked -- --nocapture`
in `/tmp/bri-v022-creator-adversarial-grip-fix.log` reports **two passing and two
failing tests** (2.04 seconds). Genuine tool loss after grip/movement and two-bot
same-object contention pass both transformed worlds, including strict package
diagnostics and actual outcomes. Exact named-body contact and post-progress
replacement remain failing in their first world; neither paired test is closed.

The new trace confirms the contact executor selects the exact intended ID and
moves it, but drifts laterally beyond the goal. Replacement invalidates the old
body and selects the new ID 5, so the observed failure is not reuse of the old
incarnation. At ticks 1440 and 1560, its actor stays at `(-25.842632,33.10557)`
in the horizontal plane with no next waypoint, while its requested approach is
`(-24.817383,32.832397)`: about 1.06 units away. The shared controller's exact
point fallback then required less than one unit. Root corrected that common
boundary using grid resolution, waypoint consumption and current-point drift;
the physical owner is reviewing directional contact alignment/rearm. These are
source corrections pending the unchanged fixtures' rerun, not passing evidence.

The final independent current-entry/ordinary-flow review found no additional
blocking defect beyond these open contact/controller cases and the remaining
adversarial, performance, full gate and platform verification requirements.
Earlier native captures were inspected, not rerendered or interactively tested.

## Contact repair and tool-loss chronology

The subsequent unchanged fixture run in
`/tmp/bri-v022-creator-bounded-approach.log` reports three passing tests: exact
named contact, actual post-progress replacement and same-object contention.
The tool-loss case fails its original unconditional no-winner assertion. That
is not evidence of fabricated planner completion.

The dedicated causal run `/tmp/bri-v022-tool-loss-chronology.log` establishes why:
in the transformed world the first real grip is tick 191; actual tool inventory
is empty at 200 while the intended body is outside the sensor. Native grip
subsequently clears and the bot reports no grounded plan. Retained physical
momentum carries that exact body into the region at tick 364; canonical score 31
and the real bot winner appear at 374. The saved trace shows the exact spawner
and Instigator-exists conditions passing and the three native 80ms outputs
running. The accepted event policy does not require continuing to hold a tool.

Root approved a fixture-only semantic refinement. The unchanged geometry,
real-displacement loss threshold and forty-second observation now require the
method and its mounted command image to invalidate by the next brain tick;
native grip must clear within the copied policy's six-tick hook interval plus
one end-of-step observation, without reacquisition. Any real winner must follow
independently witnessed exact-body entry and verified saved-rule effects with
the exact score/winner. A paired negative adds an initially true Self Color
guard through ordinary authoring; the same loss callback changes only that
color with declared world-edit permission. Body motion and credit are untouched.
It requires no score/winner/FX, an actual coast entry in at least one paired
world, and the saved due-time guard skip. These stronger causal checks replace
the incorrect inference that removing a tool cancels ordinary credited physics.

The refined five-test source is formatted and diff-clean, frozen for root's
serialized rerun. No refined pass is claimed yet; the chronology run itself
failed the earlier assertion.

The first refined rerun compiled but passed three of five cases. The two loss
cases exposed a harness identity error rather than genuine one-metre progress.
`/tmp/bri-v022-loss-identity-diagnostic.log` records a baseline from decoy
`vehicle:4` and a current sample from intended `vehicle:3`; their separation
produced distance squared 18 while the intended body moved only about 0.03m.
The callback now keys its baseline by both actor and native object reference,
still observing every held body and retaining the same displacement threshold,
geometry, duration and normal loss callback. The observer requires the loss
sample's baseline/current references to identify the intended body. Bounded
grip-transition evidence records decoy acquisition/release and intended grip;
public behaviour also rejects legacy carry taking over a declared Hold method.
There is no invented decoy release deadline. Root/provider owners separately
review the actual wrong-grip recovery and ordinary control ownership.

The corrected source is formatted and diff-clean, frozen for root's serialized
rerun. No corrected five-case pass is claimed yet, and no Cargo was run in this
fixture-edit lease.

The identity-corrected rerun passed four of five cases in
`/tmp/bri-v022-native-loss-guard-final.log`. Both positive loss worlds met the
strict motion and cleanup checks; the transformed world also verified the
real coast entry and canonical winner. The remaining guarded negative failed
because its callback chose palette index 4 from `(3 + 1) % 8`, while the actual
synthetic host palette has four entries. Loading an eight-white-color save
remaps paint into that host palette; it does not replace it. Source inspection
confirms package color operations apply immediately after the script returns,
and `package_set_brick_color` rejects an out-of-range index before mutation.
This is a fixture authoring error, not a deferred-operation boundary.

The callback now chooses index 1 for color 0 and index 0 otherwise, with setup
requiring two actual palette entries. The initially true guard, same-tick real
revocation, genuine motion/loss, coast-entry witness and no-score/no-win/no-FX
checks remain strict. Corrected source is frozen for rerun; no Cargo was run
and no five-case pass is claimed yet.

## Final causal outcomes and independent scope review

The final root-serialized `/tmp/bri-v022-native-loss-guard-final.log` now reports
**all five adversarial tests passing**, no failures/ignored tests (2.60 seconds).
Its other targets also pass: actual imported CTF one test (1.59 seconds),
carryable seven tests (0.25 seconds). The adversarial target can be reproduced
with `cargo test -p bri-chaos --test bot_creator_adversarial --locked --
--nocapture`; this lane did not acquire a Cargo lease or rerun the batch.

The loss chronology is causal rather than an inference from a plan. In the
first world a real wrong-body grip starts at tick 150, clears at 158, and the
intended body is gripped at 164. Genuine same-body motion triggers loss at 173;
the selected method clears at 174 and native grip at 175. There is no native
goal entry or winner. In the transformed world intended grip starts at 191,
loss occurs at 200, the method clears at 201 and native grip at 205. The exact
body coasts into the actual region at 364. With the original authored guards,
the native delayed rows award the actual winner. With the initially true color
guard revoked by the same normal loss callback, that real coast entry occurs
but the due-time guard skips: no score, round winner or FX. Tool removal does
not erase native momentum or mover credit. No planner completion is inferred
from either coasting or a projected winning effect.

Root's final ordinary-control batch in
`/tmp/bri-v022-final-ordinary-controls.log` separately reports **66 passing
tests**: creator eight, held-out one, interactions fifteen, rest two, objectives
eleven, physical nine, physics eleven, search four and tactics five. Source
review checked the actual control/outcome assertions and live shared hold
ownership, not just the test names. The frozen held-out composition still
requires real carriage/return without premature score, both provider families,
three canonical switch witnesses, exact bot score 97 and one actual round
winner; both transformed worlds pass without a production handler for it.

| Intent | Outcome-level evidence reviewed |
| --- | --- |
| 1 Ground contact | Physical paired contact and independent exact-body delivery; actual displacement, native credited input and real winner. |
| 2 Cheaper declared hold | Eligible native hold selected, exact native grip and real delivery; cost remains a ranking estimate, not guaranteed physics. |
| 3 Elevated/small region | Paired elevated native hold reaches the actual elevated region and canonical outcome. |
| 4 Exact named spawner/decoys | Identical-body independent worlds require the intended identity and real win; wrong native ray grip is released and repaired. |
| 5 Own control seat | Renamed/rotated ground vehicles require actual control-seat occupancy, displacement and canonical winner. |
| 6 Ordered checkpoints | Independent paired ordered delayed scoped progress and wrong-order decoy checks pass. |
| 7 Multi-action puzzle | Independent paired switch witnesses and guarded final canonical score/winner pass. |
| 8 Elimination/search | Real sight loss, bounded unchecked-space probe, reacquisition, exact-life credited death and winner; hostile interruption and team exclusions also pass. |
| 9 Typed carryable return | Unfamiliar package real pickup/carriage/return and winner; imported CTF real flag/team outcome; read-only query mutation rejection passes. |
| 10 Contention/interference | Two allies share one body controller and exact winner; ordinary human seat occupancy, permission changes and hostile interruption regressions pass. |
| 11 Replacement | Independent post-motion source removal/recreation requires a new real body ID; ordinary package respawn separately invalidates old carriage. |
| 12 Tool disappearance | Strict same-body progress, actual inventory loss, next-brain method/image invalidation and native release bounds pass; real coasting and guarded no-win pair distinguish canonical physics from predictions. |
| 13 Active context edits | Actual rule/team changes and delayed-guard/permission changes reject stale success and select or fail honestly. |
| 14 Honest negatives | Unreachable sensor, unknown complete-action collateral, depth/action/discovery limits and oversized package offers report bounded reasons with no manufactured success. |

This is an independent signoff for those bounded **outcome-level mechanisms**,
not an unconditional signoff of the entire frozen acceptance contract. A final
read of the applicable positive variants found two remaining coverage gaps:
`bot_search_objectives.rs` varies only horizontal translation for its principal
search positive, keeping content IDs, names, insertion IDs/order and decoration
fixed; `bot_carryable_objectives.rs` changes package namespace and translation
but keeps brick IDs/order and decoration fixed. The frozen contract asks for
name/ID/layout/order/irrelevant-decoration transformations. These are fixture
variation gaps, not observed production defects; handed to root/provider owners
for bounded fixture-only strengthening, with no production adaptation requested.

Unknown script inference, novel tether/throw trajectories, cooperative stacks
and general aircraft/watercraft routing remain outside the demonstrated slice.
No new production blocker was found in this final narrow read-only review.
Strict affected-target clippy, sustained performance, full repository gate,
Windows CI and three-platform archives remain root-owned verification. This
entry makes no human-playtest, FPS, crash-cause-repair or publication claim.

## Remaining variation gaps closed

The provider owner strengthened only the two fixtures, without adapting any
production provider. Independent read-only inspection confirms the second
search world now renames the actual bot kind, participant, world, spawn and
rule; its sparse saved spawn/rule IDs change to 801/7, the screen's insertion
IDs reverse, and an off-route named decoration is added. The second package
world changes actual bot/package/item/brick identities and participant/world/
source/destination names, reorders saved spawn/depot/destination IDs to
301/203/17, and adds unrelated native decoration. Translation remains in both.
Package carriage evidence resolves the exact live depot after ordinary build
loading; it does not assume serialized IDs survive native load remapping.
Original real control, duration, exact-life, carriage, score and canonical
winner assertions remain intact.

`/tmp/bri-v022-final-search-variants.log` reports four passing tests, no
failures/ignored tests (0.29 seconds).
`/tmp/bri-v022-final-package-variants.log` reports seven passing tests, no
failures/ignored tests (0.26 seconds). These results close the specific
metamorphic gaps raised above. Together with the final five adversaries,
eight independent creator tests, the frozen held-out composition and the
reviewed ordinary-control suites, this lane signs off the fourteen-case
**bounded NPC increment at source/headless outcome level**. No remaining
acceptance defect was identified in this lane. This is not universal arbitrary
mechanic understanding, subjective human acceptance, a performance result or
the final repository/platform release gate. No Cargo or fixture/source edit
was performed for this final verification.
