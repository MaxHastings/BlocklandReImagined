# 2026-10-03 NPC evidence/search and explicit package desired state

Sol High foundation lane on `codex/v0.2.2-hardening`; root owns shared integration,
ordinary actor lifecycle/authority, canonical winner observations and release.
Performance owns the existing objective model/planner adapter and physical
affordances. All fourteen acceptance journeys remain required by
`docs/v022-delivery-contract.md` and `docs/audits/objective-driven-integration.md`.
No interactive gameplay, commits, original-source edits or manifest/lock changes.

## Audit and coordinated direction

The actual linked pipeline audit (`docs/audits/npc-pipeline-current.md`) preceded
these extensions. The existing brain remembered a dated enemy position, followed
it and scanned from there until expiry. Its tool-hold Carry behavior seeks open
space and flings a physical object; package worn-image carriage is different.
Package pickup, zone entry, carrying state and return/scoring already have
ordinary executors. Their opaque policy cannot be inferred from content names.

The existing single planner now separates desired state, causal input projection,
grounded action and selected executor/lifecycle. This lane must append actions
and observation bindings through that shared seam. It must not add a separate
pickup-return brain, duplicate the grouped rule interpreter, or write game
success as a planner side effect. Unknown policy remains unsupported.

Root owns a canonical bounded winner observer and the shared package-participant
admission correction. Actual MiniGame members can participate while anchored
free-build creatures remain excluded from automatic player loadout/spawn hooks.
Death and MiniGame left/ended/team cleanup must retain their canonical policy
delivery. Prior brick-bot native-region tests alone do not prove package returns.

## Current implementation state

`bots/search_memory.rs` supplies a last-known anchor and at most four unchecked
probe points, oriented only from dated observed movement or the original view
direction. It performs no sight query, hidden-position lookup, navigation or
control. Existing bounded navigation walks each point. Reached, failed and
fixed-timeout probes advance without endless retry; the original evidence expiry
is never refreshed by repeated/shared observations. Root links the small Brain
hook and clears evidence when game/round/team, life or hostility changes.
Six pure tests pass in root’s initial 67-test bot-module checkpoint. Four actual
ordinary-control search/elimination fixtures now pass; evidence is below.

`package-runtime/bot_objectives.rs` is a linked bounded schema for explicit
CarryReturn desired-state descriptors: exact pickup source/spawner/incarnation,
item, declared carriage-state/worn-image observations, bounded existing package
zone destinations, and a counter binding into that package's canonical state.
It carries no script commands or arbitrary bot waypoints. Arrays are rejected
before descriptor expansion when they exceed eight objectives/eight destinations;
identity, path, string and image-slot bounds are checked. Unknown kinds/fields
are rejected. Four schema/counter tests pass, including strict initialized
incarnation identity. Epoch and completion have typed own-state bindings;
missing completion map leaves mean zero, whereas missing epoch identity is
unavailable. Distinct bindings are recommended when replacement can happen
without success. The engine permits one binding for both when the owner policy
truthfully defines each replacement as a successful return.

The implemented read-only query extends the existing script call implementation.
It rejects attempted state/entity/operation writes, including a write followed
by restoration or an error swallowed by script policy. Final state equality alone
is insufficient. Discovery uses the existing package/server script-work share
and the common fair objective turn; it never calls the committing run_package
path. Runtime/entity/image/namespace/zone/game/round/team admission remains native.

The linked package adapter grounds pickup and visit as two actions in the existing
planner. Real declared carriage plus the actual worn image observes pickup before
source disappearance is rejected. It retains the journey’s initial counter across
replanning, revalidates source and zone geometry/definition/owner/color and typed
incarnation, and rearms an already occupied zone by ordinary exit and reentry.
Canonical return observation precedes source-epoch rejection because real return
policy may atomically replace its own item. A package’s query is limited to eight
providers/eight total descriptors/eight destinations and 8192 description bytes,
with VM operation limits and native grounding/search budgets also applied. Unknown
policy stays unsupported; bounded de-duplicated query diagnostics expose rejection.

The CTF owner port now declares its real carry/return semantics, sharing explicit
eligibility predicates with actual pickup/return, preserving item incarnation
through carriage/drop and allocating a new incarnation only for a replacement.
No engine logic branches on its name or IDs. Root refreshed generated content,
and the actual imported CTF ordinary-control journey passes. Release/gate and
interactive verification remain root/Maxwell's responsibility.

Elimination reuses exact actual victim/killer death context, full shared
grouped projection and dated sight/search/native combat. Native MiniGame scoring
precedes the death input (`combat.rs::kill` -> `minigames.died`); victim death and
both kill/die score changes need canonical observation aliases. Projected death
does not count as real elimination or a winner. The new Enemy adapter uses
all game-owner death programs via a bounded owner index and the common grouped
projector, including native Alive and saturating kill/die score transitions.
The common compound helper preserves global due-time/source order. Its controls
delegate to existing combat/pursuit/dated search and admission uses actual
DeathResult with exact victim LifeId/credited killer. Root linked the module and
shared control/lifecycle glue. Opaque package death callbacks remain an explicit
unsupported causal prediction; ordinary utility combat is retained. Team score
rule projections and package-authored elimination semantics are not claimed.
The bounded 64-death/64-round histories may evict an observation before delayed
completion; that produces bounded repair/failure, never inferred success.

## Verification and remaining work

`cargo test -p bri-package-runtime --lib --locked` passed 17 tests initially
(`/tmp/bri-v022-sol-package-objective-runtime.log`), then root’s strict-epoch
checkpoint passed 18 (`/tmp/bri-v022-package-query-regression.log`). Four query
negatives cover caught writes, restored writes, normal mutable-call preservation
and operation exhaustion. Root’s native objective baseline passed all 11 and
independent creator baseline eight.

The first linked carryable suite passed read-only attempted-write rejection and
outside-game anchored hook exclusion, but failed the actual pickup positive
before any planner search (`/tmp/bri-v022-linked-objectives-regression.log`).
Investigation found that all bots still skipped packages_joined default-state
initialization; strict native per-player counter/carriage reads could therefore
be unavailable. Root added idempotent declared-default initialization at accepted
MiniGame membership without enabling free-build creature on_join. The fixture now
checks native initialized values and retained state through ordinary RespawnAll.
The corrected real pickup/return and actual elimination/search suites remain
pending. No acceptance 8/9 completion is claimed by prepared modules or fixtures.

The membership rerun passed native defaults/ordinary respawn retention,
read-only write denial and outside-game anchored exclusion (three tests), then
reached real pickup in the positive. Its two-action observation used an obsolete
provider label; it now checks the actual public `declared package pickup/zone`
label and retains the two-action requirement, with a capped distinct-plan trace.
The leave/search negative passed. The elimination positive reacquired and killed
before any unchecked probe, which is valid ordinary behavior but insufficient
for that acceptance case. Its authored spawner is now fifty units away with a
64-unit chase radius, requiring bounded approach/search before reacquisition;
no memory, transforms, immunity or weapon outcomes are injected. Same-observation
hidden evidence must retain its position and expiry. A separate ordinary Gun
fixture covers same-builder/same-kind bots with `fights_bots=false`: canonical
opposed teams must allow credited elimination/win, while identical teams must
not target or kill each other. These revised positives are pending verification.

The earlier delayed-projectile lane is independently complete/frozen with
focused evidence in `2026-10-03-sol-delayed-projectile-events.md`.

Root's corrected `bot_carryable_objectives` checkpoint passed **4/4**
(`/tmp/bri-v022-common-provider-outcomes.log`): two unfamiliar namespaces/layouts
actually picked up the declared source, showed native carriage and the shared
two-action route, returned via the real zone policy, scored seven and emitted
the actual bot winner. Native defaults/respawn retention, swallowed-query-write
rejection and outside-game anchored exclusion also passed. This does not prove
arbitrary opaque packages or yet prove the original Slayer CTF package.

The new ignored `bot_carryable_ctf` test uses refreshed `BRI_CONTENT`, actual
Slayer/CTF packages and companions, imported flag bricks/item/worn image,
ordinary native settings/teams and real physics. It requires actual carriage,
the shared two-action plan, return scoring, the owner policy's real
`onFlagReturned` input and canonical team winner. Its invented palette has
distinct entries to preserve the authored two team colors across ordinary build
loading. Original content remains ignored/private. This actual-content proof
is prepared and pending regenerated owner policy and validation.

The corrected ordinary search/elimination suite passed **3/3**
(`/tmp/bri-v022-search-corrected.log`): genuine loss of sight, bounded unchecked
space, actual reacquisition, exact-life credited Gun death and canonical winner
in two translated layouts; game departure clears dated hostility; explicit
opposed same-builder/same-kind bots fight despite `fights_bots=false`, while
same-team bots do not attack. One translated layout initially crossed the
synthetic map's fixed opaque wall, so the second translation now stays west of
that obstacle. The team fixture now declares a real minimal Add-On team setting
before using native team authoring. These were fixture admission/geometry fixes,
not engine shortcuts. A fourth test now introduces a real visible enemy B during
the hidden intended A approach, then has B leave normally. Resumed A must use
dated A evidence without hidden observation/expiry renewal, followed by actual
credited elimination/winner. This new interruption case remains pending.

The final focused checkpoints now pass:

- `bot_search_objectives`: **4/4**, 0.29 s;
  `/tmp/bri-v022-search-interruption.log`. The added second visible hostile
  actually preempts A; after B leaves, A resumes with its own dated evidence,
  without expiry renewal, and dies through the ordinary credited weapon path
  before the canonical win.
- `bot_carryable_objectives`: **5/5**;
  `/tmp/bri-v022-carryable-replacement.log`. The added ordinary RespawnAll
  interrupts real carriage, replaces the actor life and source epoch, clears
  carriage, and produces no completion or winner for the old journey. A fresh
  two-action journey then actually picks up/returns, scores seven and wins.
- `BRI_CONTENT=content` ignored `bot_carryable_ctf`: **1/1**, 1.74 s;
  `/tmp/bri-v022-native-ctf.log`. Actual imported flag pickup, the authored slot-3
  image, shared two-action route, score twenty, real `onFlagReturned` input and
  canonical CTF team winner all pass. Root refreshed the generated companion
  while retaining all fourteen exact original extraction bindings and adding
  only the new declared desired-state/incarnation support. CTF port notes now
  describe this bounded support rather than the obsolete absence of flag goals.

These are concrete headless journeys, not a claim of arbitrary policy inference,
universal AI, every delivery-contract fixture, full gate or interactive approval.
Root retains integration, broader correctness/performance and three-platform
packaging. Provider budget/source-mutation negatives and independent review
remain the next narrow evidence work; no speculative mechanism is needed.

Next: narrow actual provider-budget/source-mutation negatives, independent
acceptance review and scoped clippy/combined correctness/performance checks.
Any of the fourteen delivery journeys without actual evidence stays open.


## Final bounded provider audit and same-life correction

Read-only review checked the linked package query, typed adapter and native death
projection rather than adding another provider. Query writes are rejected before
state/entity/operation mutation; attempted writes remain rejected when script
code catches its error or restores the old value. Failed queries still charge
the existing package/server work share. Provider enumeration shares the common
fair objective turn and grounding budget. The global eight opted-in-provider
limit also bounds unrelated opted-in packages; it is an explicit limit, not a
claim that discovery perfectly isolates relevant policy. Snapshot/state copying
uses existing package-store bounds and server accounting rather than a tiny
per-descriptor snapshot byte limit.

Native identity checks cover actual membership, round/team, actor spawn, source
geometry/definition/owner/color, destination zone identity/geometry, declared
own-namespace counters and native worn-image carriage. The desired counter
baseline survives the pickup-to-visit repair. Real completion is observed before
rejecting a successful return's simultaneous source-incarnation replacement.
Opaque death hooks and team-score death-rule projection remain unsupported;
the independent ordinary utility fighter is retained. No original content or
opaque callback is treated as inferred semantics.

The model audit found that a grounded Enemy action had no Alive precondition,
so search could repeat a projected death for one exact living victim and add an
authored counter twice. Runtime exact-life admission already rejected that
execution, but the model was inaccurate. The approved narrow correction adds
canonical victim Alive=true as a precondition. The existing guaranteed native
transition already sets that same fact false before full authored groups; it
works without a creator-authored IF Alive. No second death simulation, damage
operation or dispatch was added. Its focused model-level unit requires one
counter increment to plan once and two increments to fail with NoPlan, while
actual death and round histories remain empty. Explicit dated unit evidence is
model setup; actual sight/control/death/winner proof remains in the four
ordinary-control search fixtures. Initial unit setup attempted feet at the floor
surface and native spawn correctly rejected it; root corrected those feet to
0.05 without changing assertions. Final focused verification is pending.

A separate read-only finding identified repeatable physical ObjectEnter model
actions without modeled exit/reentry. Performance confirmed and owns that
shared correction with root; this lane made no physical-provider edit. Actual
one-entry physical delivery positives remain useful evidence, but repeated-entry
counter semantics must not be claimed until that correction is checked.


The next serialized package checkpoint passed **7/7**, 0.27 s,
`/tmp/bri-v022-physical-package-adversarial.log`. The two added negatives are
actual source repaint during native carriage (saved source stamp invalidates,
then the bot repairs and completes a real journey) and nine otherwise valid
unique package offers (precise bounded count rejection, no partial plan or false
winner). The neighboring physical suite in that same log was 5/8 and remains
owned by performance/root; package success does not mark that combined run green.


The focused same-life unit now passes **1/1**, 0.02 s,
`/tmp/bri-v022-same-life-projection.log`:
`cargo test -p bri-sim --lib session::bots::combat_objectives::tests::grounded_death_cannot_count_the_same_living_victim_twice`.
It uses the actual native source/shared projection, observes the guaranteed
Alive=false effect without authored IF, allows one death-counter increment and
rejects two increments for that exact life with NoPlan. Native death/winner
histories remain empty. Owned source and evidence are frozen for root's combined
heldout/adversarial verification.


## Final linked authority/lifecycle audit

The final read-only production review found no new content/game-name dispatch in
objective discovery, search or the providers. `CORE_TOOLS` remains an existing
legacy equip exclusion, not a new objective semantic handler. Planner Set/Add
changes private hypothetical facts. Existing direct body/melee/conversion calls
in the shared brain are native spawn/attack mechanisms; providers do not write
canonical score, winners, poses or rule state. Rule/compound-rule/package causes
and brick/physical/enemy/package executors share the selected State lifecycle;
there is no second rule interpreter, native hold executor or pickup-return brain.

Source/model/identity validation and fresh failed-model comparison cover actual
membership, round/team, living actor, source rows/owner/name, object incarnation,
tool/image, zone geometry and package epoch/counter. Dated search never refreshes
unseen positions or relayed expiry; real disconnect hooks run before removal.
Read-only queries reject attempted writes and charge failures. Snapshot cost is
not the descriptor byte cap: one whole-world package Snapshot is built on its
fair query turn, then each opted-in provider copies its own namespace/entity
views under the existing native bounds. Existing whole-vehicle bot observation
is shared once per tick rather than repeated per physical goal. Final performance
must include enabled providers and distinguish these costs from pure planning.

The audit found a concrete native-default accounting defect, reported to root
before editing: accepted player defaults bypassed `host.state_bytes` and state
admission. Cleanup then subtracted values never charged. Global installation
recounted its ledger accurately but admitted new defaults without per-global or
aggregate growth checks. Root approved one narrow private helper and retains
parent/module integration. New `session/packages/state_defaults.rs` prepares
only the affected global/player map, preserves saved values, admits growth with
existing per-map/aggregate limits and charges the exact delta including new
namespace framing. Failure changes neither store nor count. Unchanged maps skip
cloning and keep growth-only legacy semantics; player empty declarations create
nothing, while global installation retains its existing empty-namespace behavior.
Seven focused tests cover idempotence, saved values, native cleanup, framing,
atomic player/global/aggregate rejection, exact aggregate edge and unchanged
legacy over-limit maps. Root linked the helper and verified **7/7**, 0.01 s,
`/tmp/bri-v022-native-default-accounting.log`.

Current outcome map remains evidence-scoped. Cases 1/2/3/5 (contact, cheaper hold,
elevated hold, native ground seat) pass physical positives; case 11 physical
replacement, delayed grip and repeated physical entry now also pass in the
**9/9** physical checkpoint, `/tmp/bri-v022-physical9-bounded-approach.log`.
Case 4 exact named decoy and case 10 actual contention pass the latest creator
adversarial checkpoint. Case 12 remains **open**: both real tool-loss variants
in `/tmp/bri-v022-native-loss-guard-final.log` fail before the authored loss,
with actual grip but no required body motion. That run is **3/5** creator
adversarial, despite actual imported CTF **1/1** and carryable **7/7** passing.
It does not establish a due-time guard defect or close tool-loss acceptance.
Cases 6/7/13 have prior creator-checkpoint, puzzle and active source/team evidence,
to be repeated on final shared source. Cases 8/9 pass actual dated elimination
and declared return, including imported CTF. Case 14 has actual unsupported,
unreachable, fanout/depth/provider-budget and same-life negatives, with combined
verification pending. No row is marked complete by action-list assertions.

The independently frozen held-out exchange passes **1/1**, 0.51 s,
`/tmp/bri-v022-heldout-panel.log`, in two transformed worlds. Real package return
occurs without premature score/winner, then guarded native switch inputs score
97 and identify the actual bot winner. This uses both existing provider kinds
and rejects a nearer locked decoy without a production handler.


## Author declaration review

Read-only review compared `docs/modding/bot-objectives.md` with the current typed
descriptor, read-only query admission, counter readers and live source/carriage/
zone stamps. No schema mismatch was found. Root authorized documentation-only
polish: label the snippets as extensions to existing gameplay policy, collect
the example's at-most-eight same-game desks once before iterating depots, and
state the existing `on_pickup` return contract. This avoids repeated destination
scans and unintended native inventory pickup; it adds no API, helper or brain.
Initialized epoch, exact integer carriage identity and optional worn slots 2/3
remain honest current alpha constraints, not automatic arbitrary-script support.

The focused linked evidence additionally reports sim library **184/184**
(`/tmp/bri-v022-sim-library-boxed-final.log`), mounted interactions **15/15**
(`/tmp/bri-v022-mounted-release-corrected.log`) and sim-library strict clippy
PASS (`/tmp/bri-v022-sim-library-clippy-corrected.log`). These focused checks do
not constitute a workspace, platform or full release review. No Cargo call or
production source edit was made for this author-facing review.


## Final affected-target lint follow-up

Root's affected all-target clippy found `items_after_test_module` in the owned
combat-objective file. The existing same-life test module was moved intact after
all production items; no tests, assertions or production semantics changed, and
no lint allowance was added. Only that file was formatted. Root retains the
serialized Cargo recheck; this lane ran no Cargo command.
