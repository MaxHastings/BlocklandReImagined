# 2026-10-03 Objective integration and creator workflow follow-up

Maxwell supplied an explicit objective-driven integration contract after the
initial v0.2.2 candidate. The frozen acceptance matrix is in
[objective-driven-integration.md](../audits/objective-driven-integration.md).
All fourteen journeys require real-control evidence, with renamed/unfamiliar
content, decoys and live invalidation cases. A successful projected plan does
not satisfy a journey. This expands the remaining work; no publication or
completion is claimed here.

The existing bot utility brain, bounded planner, claims, navigation, physics,
image execution, rule scheduler and MiniGame outcomes remain the mechanisms.
Production bot providers must not branch on game/content names. Unknown
package semantics remain unknown. The architectural invariant is now recorded
in AGENTS.md: bots know affordances, not content; games expose objectives,
not bot scripts.

Root added a private exact-object component to the existing bounded admitted
input ledger and indexed `onObjectEnter` alongside existing objective inputs.
The component captures the original runtime object ID; existing player actions
use None. This does not invent a region input or announce a round winner.
The existing 1024-entry retention bound is unchanged.

An optional typed `BotUse.manipulation` image descriptor documents a normal
trigger-controlled physical hold. It carries near/reach/force/orientation
limits, validated finite/positive, and requires a hold trigger, package image
commands and no projected attack. The native Gravity Gun package authors its
existing 2.5/60/90000/turn limits; no bot name/command recognition is added.
This is an experimental alpha affordance, not a frozen package API. Its focused
validation test passed with the six image-seam regressions.

## Additional workflow evidence

Guest saving was already implemented through the authoritative SaveBuild
command and separate bounded guest limits. The old FEATURES bullet was wrong;
root corrected it instead of adding another save path. A new remote non-admin
UI regression confirms SaveBricks is emitted and LoadBricks is denied. The
existing real loopback save/late-join/resume coverage must be rerun before
handoff.

Sol's music/UI fixes apply the existing MusicTracks notice to both Wrench and
event choices, retain host authority across tool-catalog refresh, reset it on
disconnect and preserve unavailable selected resources without silent
replacement. Root resends the existing notice on reconnect/map adoption and
changed catalogs. Synthetic and installed-content lifecycle tests pass, as do
the actual ToolUi-to-Ui music bridge and unavailable property/event selection
tests. No new wire message is introduced.

Screenshots now default to JPEG quality95 on the existing asynchronous worker;
PNG remains selectable in Options → Advanced → Gui Options. RGB conversion is
explicit for JPEG; PNG keeps the exact pixels. The encoder test passes real
JPEG signatures/dimensions/bounded error and exact PNG signatures/pixels. UI
draft/Done/cancel tests pass, as does the bounded native 400/1024 capture check.
The visible game was not launched or played by agents.

Logs: `/tmp/bri-v022-sol-guest-music-ui.log`,
`/tmp/bri-v022-sol-screenshot-ui.log`; Sol's validation handoff supplies the
remaining focused logs. The first scoped clippy found a root-owned event ledger
type-complexity diagnostic; root factored private key/value aliases and the
cached strict client/UI rerun passed (`bri-v022-sol-music-jpeg-clippy-final.log`).

## Shared pipeline checkpoint

The first separated Brick executor checkpoint compiled and passed 67 bot unit
tests (one existing ignored test), ten objective scenarios and two objective-rest
regressions. The remaining fanout negative retained all no-prefix/no-action
assertions; its expected coarse failure text was updated to the new precise
model/grounding-budget diagnostic. Logs:
`/tmp/bri-v022-pipeline-baseline-bot-units.log`,
`/tmp/bri-v022-pipeline-baseline-objectives-rest.log`,
`/tmp/bri-v022-pipeline-baseline-rest.log`. The fanout fixture still needs its
focused rerun after the expectation update.

Root added bounded read-only canonical RoundResult history (64 outcomes),
recorded only from existing MiniGame RoundEnded effects. It retains actual
player/team/owner identities; a projection cannot publish a result. The
canonical-effect/retention unit test passes (`/tmp/bri-v022-round-results.log`).
The existing BotThought/Explain now expose derived desired state, selected
action/provider, phase and bounded proposed route. No new UI or authoring
grammar was introduced.

Root linked dated unchecked-space search into the existing navigation brain
and clears that evidence when game, round or team changes. Package hooks now
admit brick-spawned bots that are actual MiniGame participants; outside-game
creatures retain authored equipment. Search unit tests passed at the checkpoint;
the complete participant/combat integration journeys remain pending.

Native weapon impulse credit now follows a successfully applied finite nonzero
push, for humans and bots alike. A real human trigger test fires a native shot,
moves the exact spawned loose object through a goal and observes that owner's
canonical round win. A zero-impulse shot moves nothing and creates no credit or
winner. `/tmp/bri-v022-native-projectile-credit.log` passes. Initial fixture
repairs moved pack installation before live inventories, converted array feet
to Vec3, and used the valid zero-projectile-impulse negative: vehicle schemas
already prohibit zero blast scales. No production schema was weakened.

The older candidate's Windows CI exposed missing Shark helper archetypes in
the authored CC0 import fixture, a malformed port test reference, stale mirror
debris-lifetime expectations and stale popup token-search expectations. Root
repaired the fixture/reference/mirror expectation; importer/mirror reruns remain
pending. Sol verified both popup regressions with all ten view-input tests
passing (`/tmp/bri-v022-sol-popup-word-search.log`). No production popup change
was required.

## Remaining integration

Physical objective discovery/action providers and real outcomes; canonical
weapon impulse attribution review; loss-of-sight search/elimination; typed
package-owned item-return semantics; all fourteen acceptance journeys; sustained
active battle measurements after the ranged-flight fix; full repository gate,
Windows CI, private content receipt, branch retirement and three-platform
v0.2.2 publication. The original Windows firefight crash remains open unless
causal evidence establishes otherwise.

## Shared control and package-query integration checkpoint

The shared objective View now requests ordinary tool selection/trigger and
boarding through the existing executors. Native held-object identity suppresses
legacy Carry only for the actual selected held target. Exact validated objective
control seats retain their advisory reservation; claims do not confer authority.
Rejected acquisition blocks the proposed objective action and records contention.
This still requires the physical/competition acceptance tests after provider link.
Known unarmed/manipulation-only bodies no longer prefer futile Fight/Fly over a
live non-enemy objective. Unknown scripted attacks conservatively retain combat.

A bounded read-only DeathResult observation follows accepted MiniGame life
transitions and records actual life, credited killer, game/round and tick. It
cannot damage a player or admit an event. Elimination acceptance and its observer
unit test are pending. This avoids guessing success from an attempted attack.

Package runtime tests: **18 passed**, including initialized nonnegative epochs,
exact completion counters, strict bounded descriptors and read-only query denial
even when caught/restored by scripts (`/tmp/bri-v022-package-query-regression.log`).
Independent creator acceptance: **8 passed**; existing objective suite: **11
passed** after shared View integration (`/tmp/bri-v022-linked-creator-shark.log`).
Importer CC0 Shark helper/port-reference repairs: **37 passed** across bot_holes
and ports (`/tmp/bri-v022-importer-ci-repairs.log`).

The first real package carryable run passed its write-denial and outside-game
participant negatives but rejected positive pickup before planning. Discovery
now preserves bounded diagnostic reasons for investigation; no success claimed.
Shark restart rerun passed harm/release/full240-tick rest but failed its immediate
reacquisition assertion because step_bots runs before the due package hook.
Its corrected fixture checks that final resting controller phase and the next
actual controller turn; the exact deadline is unchanged. Reruns remain pending.

## Integrated ordinary-control and broader regression checkpoint

Corrected elimination fixtures now pass all four actual-control scenarios:
dated loss-of-sight search/reacquisition and exact-life credited victory, leaving
the game without forged death, explicit opposed teams despite shared builder/kind,
and a genuinely visible second hostile preempting then resuming the retained
first target with its original dated evidence. Log:
`/tmp/bri-v022-search-interruption.log`.

A real elevated hold exposed duplicate control ownership: the old Carry state
continued its swing timer while the objective owned the exact native grip,
then nulled locomotion. Root clears legacy Carry only while a validated
objective owns that held object. Paired elevated transport then passed. A
longer quarter-turned ground chassis exposed the shared driver's reverse-heading
and steering mismatch. The native controller now uses the reverse body heading,
actual rolling direction for tyre response, yaw-rate damping and ordinary brakes
for direction changes and conservative corner/arrival speeds. Claims still require
real goalward progress. All four paired physical positives pass in
`/tmp/bri-v022-native-drive-heading.log`; this does not prove arbitrary tyre,
slope or aircraft navigation. Shared bot units pass71/ignore1 in
`/tmp/bri-v022-shared-bot-regression.log`, including cross Body/Seat leases and
conservative independent-script attack classification.

Read-only package queries, actual pickup/carriage/zone completion, player state
preservation, actual carried-source replacement and repaint invalidation, and
over-limit offer rejection pass all7 in
`/tmp/bri-v022-physical-package-adversarial.log`. Imported CTF also passes its
real flag pickup/worn slot, two-action route, score20, authored onFlagReturned
marker and canonical team winner in `/tmp/bri-v022-native-ctf.log`. Because
the original CTF archive is absent locally, root matched the committed Rhai
port template exactly against the pinned generated companion, recovered all14
original extraction bindings, then rendered the new template. The JSON
descriptor retains the pinned import plus exactly the new incarnation/default
and opt-in objective fields; generated provenance notes now match current policy.
These generated/private files remain outside Git.

Freshly built importer regenerated the read-only pinned Shark archive, preserving
bundled credit. All15 Shark checks including actual imported mouth capture pass
in `/tmp/bri-v022-shark-refreshed-all.log`. The exact240-tick harm restart and
source thread cues are verified; documented flee/infection/etc. gaps remain.
Actual network guest save/load authority, palette, late join and reconnect
pass in `/tmp/bri-v022-guest-save-loopback.log`. Client library regressions
passed407/ignored55 but failed a new invalid-byte filename fixture at file
creation: macOS returns EILSEQ. That filesystem test is now Linux-only, where
those names are actually accepted; the final rerun remains required.

Independent held-out/adversarial fixtures are written, not yet passed. The
first build found a fixture-only EventValue variant typo, being corrected.
Physical permission revocation after real grip passes, but two delayed Hold
fixtures and a discovery-budget fixture remain failed in the latest8-test run.
No publication, all14 acceptance or full-platform sign-off is claimed.

## Release scope and held-out checkpoint

Maxwell explicitly added pharzedia's duplicator crash and a modern Dynamic
lighting refresh to v0.2.2. A separate Sol High user-owned thread handles the
isolated client/render work; it is a work lane, not a later release. The supplied
Linux log terminates at a 100,024-triangle world against a 100,000-triangle
render budget. Dynamic currently retains lightmap-derived residual irradiance;
the required refresh removes runtime baked-mask dependence while preserving
Classic and Unified. The delivery contract now records both additions.

The independent held-out composition passes in both transformed worlds without
a new production handler: real package carriage unlocks guarded delayed switches,
then canonical score97 and the actual winner. Log:
`/tmp/bri-v022-heldout-panel.log` (1 passed). Its prior failures were genuine
fixture defects (native state target and reserved Rhai identifier); neither goal,
geometry, delay nor intended completion was weakened. Physical repeated-entry,
delayed grip and adversarial identity/competition tests remain under validation.

## Final controller and budget edge cases under repair

Client library regression now passes407 (55 opt-in tests remain for the gate),
including platform-correct colorset filename checks:
`/tmp/bri-v022-client-library-platform-final.log`. UI/events/package-runtime/
weapons all-target strict clippy passes (`/tmp/bri-v022-vocabulary-clippy.log`).
Physical latest run still passes7/fails2; it is not accepted. Trace shows delayed
hold transport orbiting its target and eventually triggering native snag release.
The approach uses a body-to-goal residual direction, singular near arrival; a
stable actor-to-desired-grip radial approach is being repaired without changing
hold physics. Repeated-entry now genuinely exits its region after root releases
the completed action's advisory reservation before legacy combat discovery; its
return approach still pushes from the wrong side. The bounded contact approach
must go around the actual hull, not request a straight path through it.

Root aligned ordinary final motor approach with the actual grid-search arrival
radius, consumed-waypoint tolerance and existing point-drift tolerance. Named
identity now passes; replacement still fails on actual transport, not old-ID
reuse. Latest creator log: `/tmp/bri-v022-creator-common-approach.log`,3 passed,
1 failed. Tool loss and two-bot contention retain real native-state evidence.

Final independent abstraction audit found native per-player default insertion
was not charged to the package-state byte ledger. A narrow atomic default
admission helper is being added with idempotence and per-player/aggregate budget
regressions; this is a release blocker, not a new package state framework.
Snapshot/namespace cloning costs remain bounded by existing world/entity/store
limits rather than the descriptor/VM-operation limit and must be included in
simulation benchmark costs. No claim of universal object or script reasoning.

## Bounded approach and default-accounting checkpoint

Physical9 now passes all9 in `/tmp/bri-v022-physical9-bounded-approach.log`:
paired ordinary contact/cheaper hold/elevated hold/control-seat delivery, real
candidate-budget failure, permission/due-guard invalidation, exact native grip
through6s delay, and actual exit/reentry for all three physical methods (also
starting inside). Contact approaches use a constant-math expanded-hull arc before
pushing from behind. Hold standoff uses actor-to-desired-grip radial correction;
no body force, native snag limit or winner condition was bypassed.

Root is consolidating that approach geometry into existing interactions rather
than maintaining a separate objective/combat approach. The native motor and
actual collision still decide movement. Completed objective reservations release
before legacy opportunity discovery; an admitted stationary native hold no
longer needs an advisory approach lease. Native ownership, inventory/permission/
source checks and authored delay remain authoritative. Lease limits are unchanged.

Creator adversarial now passes named identity, replacement and contention, but
the tool-loss assertion fails (`/tmp/bri-v022-creator-bounded-approach.log`,3/4).
The faster real hold may produce genuine entry/momentum; an independent exact
chronology is pending. Existing physical motion/credit must not be erased merely
to force a negative test to pass. No final disposition claimed.

Native player/global defaults now use the atomic state admission helper linked
by root. It charges stored-map and namespace-framing growth to the same ledger
used by script commits and honors per-player/global/aggregate limits. Authored
values remain, unchanged legacy state can be read, and no partial defaults/count
are committed on rejection. All7 helper regressions pass in
`/tmp/bri-v022-native-default-accounting.log`. Full participant/package regression
follows after the shared approach extraction.

## Shared regression and native release checkpoint

After shared approach/default integration, sim library passes182/ignores2 in
`/tmp/bri-v022-sim-library-linked-final.log`. Actual imported CTF1, package returns7,
creator acceptance8 and held-out1 pass again in
`/tmp/bri-v022-linked-controller-regression.log`; that batch then exposed an
ordinary mounted-charge regression, so later targets were run separately.
`/tmp/bri-v022-linked-controller-rest.log` passes objective rest2, objective11,
physical9, legacy physics interaction11, dated search4 and weapon tactics5.

The mounted gun reported native charge but never fired, reproduced alone. Root
had overwritten firing intent with requested trigger state, then cancelled charge
on the intentional release before the normal executor could fire. Capturing the
authorized ready release preserves that native shot; unsafe/lost/suspended intent
still cancels charge. The unchanged complete vehicle-interaction suite now passes
all15 (`/tmp/bri-v022-mounted-release-corrected.log`). No charged-weapon timings or
fixture outcome assertions were weakened.

Tool-loss chronology proves actual simulation completion: first grip191, actual
inventory empty200 outside goal, native grip gone and planner reports NoPlan,
exact body coasts into goal364 and real credited score31/winner374 after80ms rows.
Explain verifies canonical exact-spawner/Instigator guards and executed operations.
The old unconditional no-winner assertion conflicts with that authored policy,
not the frozen requirement to avoid manufactured success. Independent review is
refining coverage to require ordinary cancellation and witnessed real completion,
plus a paired live guard revocation negative preserving momentum and goal geometry.
Log: `/tmp/bri-v022-tool-loss-chronology.log`; revised tests remain pending.

Strict sim clippy initially flagged large Completion variant, two needless
borrows and package adapter bool/argument count. Owners boxed the package Stamp,
removed redundant references and derive the existing stamp from its executor.
No new schema or lint suppressions were introduced. Final clippy/regression and
sustained performance are still pending.
