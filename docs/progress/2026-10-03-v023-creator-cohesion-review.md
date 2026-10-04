# 2026-10-03 v0.2.3 creator flow and cohesion review

Independent GPT-6.1 Sol High review of the existing v0.2.2 creator foundations
and the v0.2.3 integration branch, initially at `e262e455`. Root owns source
integration, Cargo, rendering, packaging and publication. This lane read source
and existing native captures, prepared patches in `/tmp/bri-creator-review/`,
and wrote only this progress entry in the shared checkout. No interactive
playtest, visible game, mouse/keyboard automation, worktree or compute lease was
used. Maxwell's acceptance and the complete alpha contract remain open.

## Prioritized findings

### P1: stock MiniGame favorites silently lose missing equipment

At the reviewed baseline, `screens/minigames.rs:192` maps an unknown saved item
ID to popup index zero (NONE), and `read_rules` at line 388 serializes that
selection as `None`. Store a favorite containing an Add-On rifle, host without
that Add-On, load the favorite and save after changing only its title: the rifle
disappears from the submitted rules. Missing player types instead leave an empty
selection while retaining an invisible old ID, so adjacent resource fields are
also inconsistent. The host correctly rejects unknown content in
`crates/minigames/src/model.rs:213`; this UI transformation prevents that
authoritative validation from detecting lost equipment intent.

The proposed correction preserves IDs outside popup indices, displays an
Unavailable choice and requires explicit replacement before Create/Save.
Storing a favorite preserves its authored unknown IDs. Only creator submission
is gated: server joining and cosmetic content fallbacks are unchanged.
Wrench already has this behavior. A small shared `resource_choices` helper in
`screens/mod.rs` lets its existing wrapper and stock MiniGame menus use the same
mapping; no all-screen abstraction or DTO/schema rewrite is needed.

### P1: a combined favorite reports success after a partial apply

`screens/minigame_addons.rs:1497–1537` sends a favorite's vanilla rules with an
untracked `core.request`, then separately tracks Add-On settings/reset. Its
result handler at line 1727 reports Applied for only that second result.
For example, a favorite's missing weapon can reject vanilla configuration while
valid team/round changes succeed; the UI claims the whole favorite applied.

The proposed correction uses the existing correlated MiniGame request/timeout
mechanism to acknowledge vanilla configuration before sending Add-On changes
or reset. It stops after a refused/unconfirmed stage, retains the draft and
favorite, and reports full Applied only after every required command succeeds.
This is an ordered multi-command flow, not a new atomic protocol. A second
stage can still fail after rules applied; the resulting message names that
partial outcome. Short status text fits the existing footer; its existing
message dialog carries the detailed rejection.

The review also found that the automatic listing-refresh dirty check ignores
loaded vanilla favorite rules. A rules-only rejection followed by a newer
MiniGame listing can erase the draft. The refinement treats those rules as
unfinished until acknowledged, with an actual Load → rejection → newer listing
→ retry regression.

### P2: Explain saved discards the NPC feedback its server produces

`crates/sim/src/session/rules.rs:415` emits selected-action/unsupported NPC
summaries as `[NPC name] ...`, while native `screens/workshop.rs:397` accepts
only `[Events brick] ...`. Authors looking at Explain saved therefore miss the
existing execution/grounding limitations and must discover them in chat.

The minimal correction scopes the NPC summary under the requested brick's
existing envelope, `[Events brick] [NPC name] ...`. It does not admit arbitrary
global NPC chat. The native bound increases from 16 to 18 lines because the
server can emit one saved-row count, one region line, four NPC summaries and
twelve traces; keeping 16 would omit the newest two traces. The existing real
unsupported-objective check is strengthened to use the ordinary `ruleexplain`
command ingress and exact source envelope. A native UI lifecycle test requests
Explain, receives that bounded reply and checks both NPC feedback and the
newest trace while excluding unrelated brick/global chat.

### P2: unavailable Add-On favorite vocabulary is silently omitted

`screens/minigame_addons.rs:370` loads only values that still fit declared
settings; its normal Loaded message does not identify omitted keys or values.
Store can then overwrite the source favorite with this compatible subset.
This repeats the saved-intent failure despite different data types.

The conservative refinement warns precisely about omitted vocabulary, preserves
the saved source slot and requires another slot to Store the compatible draft.
Applying the compatible portion is explicit in the warning. This adds no alpha
migration promise, hidden content inference or new authoring mode. Enabling the
missing Add-Ons before hosting and reopening the editor remains the recovery
path for the original slot.

## Reviewed surfaces and cohesion assessment

Read the project instructions, STATUS, v0.2.3 work plan, alpha contract,
platform principles, progress guidance and relevant recent Wrench, MiniGame,
Colorsets and admin-refresh evidence. Traced hosting/Colorsets choices,
stock MiniGame rules/favorites, Add-On Setup/Teams/Players, Wrench properties,
ordinary rows, optional guards, delays/state, exact named-spawner object guards,
copy/preserved rows, Explain, save/load selection/preview, admin revision
handling, request correlation, session reset and map-change dialog removal.
Read the existing bot objective diagnostics/provider boundaries and the
creator playtest guide; the NPC execution/performance lanes own broader physics
verification.

The current UI separates normal event rows from optional IF controls, keeps
Detection region behind Region, retains unsupported rows explicitly, and uses
fixed navigation/action areas for MiniGame settings. Stable identity and draft
preservation are already strong patterns in Wrench and Colorsets. The concrete
MiniGame failures justify sharing the resource mapping and completion policy;
line count alone does not justify rewriting Wrench. Its roughly 2,700 lines of
screen code compose typed `EventsModel`, parameter schema and rule vocabulary,
and much of the remaining file is acceptance/regression coverage. Session's
large integration modules similarly need changes at proven command/state
boundaries, not wholesale rearrangement during release hardening.

Explain should expose existing grounded capability failures and observed state.
It must not promise arbitrary Add-On inference or convert example names into
mechanics. The reviewed source/provider design already keeps unsupported
semantics explicit. Naming exact spawners, owner context and real object entry
are meaningful authored contracts; demonstrations are not a general solver.

## Visual evidence and limits

Inspected existing native images, with filesystem modification time recorded
as capture-age evidence rather than proof of exact source provenance:

- `artifacts/v022-minigame-ui/Slayer-Teams-1024x768-1x.png`: 2026-10-03 09:47:57 CDT.
- `artifacts/ui-native-host-colorsets/StartGame-choices-1024x768-1x.png`: 2026-10-03 10:08:36 CDT.
- `artifacts/ui-native-wrench/Events-BallGoal-1024x768-1x.png` and
  `Events-Input-Search-1920x1080-2x.png`: 2026-10-03 16:41:46 CDT.
- `artifacts/ui-native-wrench/Events-Conditions-400x300-1x.png`: inspected for
  compact navigation/action retention, not readability or human task timing.

These show coherent native profiles, bounded common/advanced controls and
fixed Save/Cancel actions. The tiny physical window fits through scaling but
does not establish comfortable reading. Slayer captures use actual setting
metadata with fixture choices; their item labels are not evidence that the
live content bridge renders those exact fixture names. No current-runtime,
cross-platform or subjective acceptance claim follows from these images.

## Patch and verification handoff

Temp files were formatted with edition 2024. `git apply --check` passed for
the initial combined favorite patch, Explain combined patch and later shared
resource helper. Formatting copied `screens/mod.rs` initially tried to resolve
uncopied sibling modules; `rustfmt --config skip_children=true` corrected that
temp-only invocation. No source or expectations were weakened for that failure.

Root reported all three initial actual favorite-flow regressions fail on the
baseline for the intended missing-ID/sequencing reasons
(`/tmp/bri-v023-creator-favorites-before.log`). Root has applied the production
and refinement and owns their test results. This lane has not run Cargo and
does not count patch application or formatting as behavioral verification.

Initial handoff: `baseline-tests.patch`, `production.patch`, `refinement.patch`.
Further patches: `rules-only-test.patch`, `shared-resource-helper.patch`,
`explain-baseline-tests.patch`, `explain-production.patch` and
`explain-command-path-test.patch`, all in `/tmp/bri-creator-review/`.
The combined patches are alternatives, not additional patches to apply over
their separate parts. Root should rerun the existing MiniGame/Wrench tests,
the native Explain test and real bot Explain/acceptance checks, then the full
gate, Windows CI and platform archive checks before publication.

This is a risk-based whole-project review of the creator journey and recurring
failure patterns, not an assertion that every subsystem or file is complete.
Unavailable imported Wrench-property recovery, broader UI replacement seams,
live populated-provider feel and general unsupported NPC mechanics remain
separate follow-up risks unless further evidence demonstrates a bounded defect.

## Independent review reopened an async draft race

The adversarial reviewer identified another concrete P2 sibling failure:
Add-On setting fields, favorite Load/Store and team Delete remain usable while
a request is pending. Save captures draft A, the user changes/loads B, and the
acknowledgment marks current B as Applied even though the host accepted A.
The next listing can then replace B with A. Both MiniGame and server Add-On
settings share the problem. Ordered requests alone do not close this race.

The temp `busy-production.patch` adds one mutation-control readiness pattern.
It disables author fields, team/player mutation actions, favorites and notify
while a request is active; Setup/Teams/Players, category/team navigation,
help and Close remain available. Queued mutation events and team Delete are
also guarded. It clears inactive text focus and popup captures: the existing
View character path follows focused edits without independently checking their
active flag. This is a screen lifecycle correction, not a general View rewrite.
Existing correlated completion/error/timeout paths restore editing.

`busy-baseline-tests.patch` contains ordinary UI event/key regressions for
both flows. The MiniGame journey attempts text/favorite/team changes during
both acknowledgments, verifies that the captured settings and team are still
sent, then times out the second stage and recovers editing. The server journey
attempts a different favorite and text while ConfigureHost waits, then verifies
the original draft and recovery after its correlated rejection.

Root reported the earlier integrated MiniGame suite passed 29 tests and the
shared-helper/field-flow batch passed (`/tmp/bri-v023-creator-shared-helper.log`).
The native Explain batch passed six tests; creator acceptance passed eight and
bot objectives passed eleven after the scoped diagnostic correction. These
are root's receipts, not this lane's Cargo runs. The busy-race patch's before
and after verification, focused lint, full gate, Windows check and archive
publication remain root's next work. The new deadline limits this lane to
finishing this confirmed race and the review handoff rather than widening scope.

## Player documentation handoff

Root requested source/docs-only release wind-down. The reviewable
`/tmp/bri-v023-docs/player-docs.patch` contains a concise candidate player guide
and release notes under `docs/rule-workshop/`, targeted STATUS/KNOWN-ISSUES
corrections, stale v0.2.2 seam-state corrections and the candidate resource/draft
and controlled portal-frame ownership rows. The public release remains v0.2.2
until root records actual publication. The text distinguishes implemented
candidate behavior from pending full-gate/Windows/archive/human acceptance.

The guide follows host/Add-Ons/colorsets, common MiniGame/team options,
unavailable favorite recovery, ordered/pending saves, simple Wrench rows and
optional IF/delay, named exact-object goals and bots, save/load/copy, vehicle
recolor, portal crossing, map support, administration and reproduced-report
handoff. It explicitly leaves Windows death/disconnect, destruction/respawn
hitches, firefight NaN and tank retreat/cover causes open; arbitrary jet carrying,
high-hoop throws and undeclared Add-On semantics remain unsupported. The full
alpha contract is not closed.

All three existing packagers already invoke `tools/package_guides.py`; two
GUIDES entries include the new documents with no platform-script duplication.
A temporary fixture invoked the proposed Python helper against copies of the
existing guides plus the candidate pages. Both new documents were copied,
all their relative Markdown targets resolved, and KNOWN-ISSUES' candidate-guide
link was flattened successfully. `git apply --check` passed. No Cargo,
interactive input, game window, original-content writes or shared production
edits were performed for this handoff. Root must change candidate wording only
when the corresponding release checks/publication receipts actually exist.

## Typed Shark capture finish blocker handoff

Root's native/synthetic physical capture checks exposed a package-policy
recapture after the new source-backed typed bite completion: releasing the grip
removes `grabs`, then the ordinary typed weapon damage hook treats the same
holder/victim as a fresh bite. Root retained the receipt
`/tmp/bri-v023-shark-final-3.log` (13 passed, two failed). This lane inspected the
ordinary typed damage adapter and the package lifecycle; generic engine damage
classification is intentional and remains unchanged.

`/tmp/bri-v023-shark-finish/finish.patch` introduces one transient bounded
`finishing` identity map in the companion declaration. The existing observed
capture enters finishing before release; the typed damage hook observes the
matching holder/victim, consumes the identity and returns Unit, preserving
normal damage/death/credit/icon policy. An unrelated bite cannot begin another
capture during the transition. Existing actor and MiniGame lifecycle hooks
cancel the identity; the next tick cancels an uncompleted queued operation.
Normal unmount/orbit/rest release remains immediate, with no stale grip or
new timer-based rest behavior. No damage-amount threshold or content-name
exception is introduced.

Two policy regressions inspect the actual returned damage-hook value and
queued operations, future legitimate capture after respawn, and eight
lifecycle/next-tick cancellation controls. The existing synthetic/native
ordinary physical-capture tests remain the end-to-end finish/death/icon proof.
`finish-tests.patch` and `finish-production.patch` are split alternatives to the
combined patch. Temp Rust formatting and `git apply --check` passed; Cargo and
behavioral before/after evidence remain root-owned. The generated companion
`behaviour.json` must be refreshed alongside `shark.rhai` for its state
declaration; application to source alone does not update a packaged Add-On.

## Exact-commit gate: synthetic topology favorite/resource recovery

Root's exact-commit gate receipt `../.bri-gate/logs/56fa1f82811e.log`
failed `screens_reach_the_server_in_every_topology::synthetic`, including its
isolated retry. The generated-content variant passed. The full synthetic
failure identifies Create MiniGame in all three topologies: settings stays
open, pending requests are zero, and no game is created. The synthetic content
catalog lacks some stock v20 default equipment IDs. The new screen correctly
preserves those authored defaults as Unavailable and refuses submission until
explicit recovery; the old topology helper relied on their silent NONE fallback.

`/tmp/bri-v023-topology-recovery/screen-topology-recovery.patch` changes only
that test's ordinary create journey. It verifies each absent default remains
visibly bound to its exact Unavailable ID, chooses the existing NONE entry
through the normal popup click/type/Return helper, and then submits Create.
An added assertion checks the complete chosen loadout in every client's
authoritative game summary. Available defaults in the native-content case
remain untouched. No production resource validation/acknowledgment guard,
button visibility, existing acceptance assertion or timeout is weakened.
Temp Rust formatting required skip_children because the copied file lacks its
support module; the corrected formatting and patch applicability passed.
Cargo before/after and renewed exact-commit gate remain root-owned.

## Topology rerun exposed exact clear-choice keyboard defect

Root's `/tmp/bri-v023-topology-recovery.log` rerun passed native content but
synthetic recovery still failed: CMG_StartEquip4 retained its unavailable
rocket launcher after typing the dropdown's NONE label and pressing Return.
This was not a viewport/hit overlap. Shared `View::refilter_popup` deliberately
skipped every pinned row when choosing the highlighted search match. An exact
NONE (or event-editor '-') query therefore had no highlighted row; Enter closed
without changing the choice. The helper also copied NONE's leading space,
while runtime popup queries did not reuse the existing trimmed search key.

`/tmp/bri-v023-popup-clear/popup-clear.patch` corrects these two shared popup
lines: normalize through `search_key`, then admit an exact pinned-key match as
the highlight. Ordinary searches still choose their non-pinned match, and
unknown searches retain the existing no-selection behavior. The actual View
mouse-open/type/Return regression covers NONE, whitespace/case, '-', then a
normal Gun search to prove clearing cannot steal unrelated searches. Existing
unknown-query and broader popup assertions are unchanged. The original topology
recovery uses normal input and needs no special bypass. Formatting and patch
applicability passed; root owns baseline/after runs and the renewed full gate.
