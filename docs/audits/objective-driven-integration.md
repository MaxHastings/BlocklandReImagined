# Objective-driven bot integration — acceptance frozen 2026-10-03

This note precedes the new physical-objective implementation. Maxwell's complete
request in the attached “Objective-driven bot integration pass” defines acceptance;
the examples below are evidence for compositional capability, not game handlers.
No item is complete merely because a projected action list exists. All interactive
acceptance belongs to Maxwell. Original installations remain read-only.

## Reuse and ownership

Rules, MiniGames and Add-Ons own desired successful states. Objective providers
read authoritative typed rules/provider state. Available-action providers describe
actual body, inventory, object and control capabilities. Existing planning.rs
composes their bounded preconditions/effect groups. Existing utility arbitration
retains combat/danger preemption. The real world disproves bad predictions.

- Objective discovery: existing bots/objectives.rs and creator-indexed installed
  programs; root owns events.rs source/index/captured-input hooks and package
  objective semantics. Physical object goals retain exact source/object/region
  identity, owner, game, round, team, authored rows and real delay barriers.
- Available actions: new private bots/physical_objectives.rs derives walking
  contact, declared hold/carry/release and controllable seats from authoritative
  descriptors and runtime state. Existing interactions.rs owns scarce claims,
  oriented approaches, contact progress, boarding and vehicle steering. Existing
  tactics/combat/inventory/navigation provide combat actions and dated knowledge.
- Planning: the existing deterministic bool/i64 GOAP planner with unchanged
  action/node/depth/candidate/fact/term/byte limits; no second planner or event
  language. Immutable text kind/spawner comparisons can be grounded into boolean
  observations through canonical rule_query, with identity in their fact keys.
- Execution: normal MoveInput/contact; ordinary equip/trigger/image commands for
  hold/release; ordinary Activate for native supported hand activation; ordinary
  board/seat/drive controls; current combat trigger and projectile paths. No score,
  variable, round, position or event mutation is performed by the planner.
- Observation/repair: captured real input origin/tick/object identity, canonical
  due-time rule queries, actual held object, contact displacement, control seat,
  inventory, permission, claim and live model stamps. Predicted arrival is not
  input admission and predicted WinRound is not an actual winner observation.

Sol High owns physical child/objectives and its headless fixtures/diagnostics.
Root owns shared bots/events/interactions integration and semantic metadata.
The existing seam rows for bounded NPC objective planning, environmental bot
interactions and bot image use own these extensions; this does not introduce a
parallel universal operation bus or freeze a public API during alpha.

## Concrete gaps and minimal seams

Current model/index/ledger only admits activation, player-region entry and bot
touch. ObjectEnter exists canonically for runtime vehicles/balls: actual spawn
brick must belong to the region creator; foreign-game mover credit is excluded;
Player/Instigator come from real contact/hold credit or the actual driver. The
sensor follows vehicle origin positions, including swept crossing, rather than
bounds-centre guesses. Current enemy-directed push/drive and auto-open-space
Carry do not fulfill object delivery. A selected physical goal must survive these
control adapters and be preempted/revalidated through the same brain lifecycle.

Root is adding an optional explicit image BotUse.manipulation descriptor: tagged
`kind=hold`, `Hold { near, reach, force, turn }`. It declares the known physical
hold mechanism controlled by the authored image's ordinary trigger; no inference
from command, item or game names. Descriptor limits must be finite/validated and
match the package's actual script. Release adds no imaginary impulse. Unknown
image scripts stay unknown. Native hold radius/snags/gravity/permissions and
actual heldBy/progress remain authoritative. Root also extends the shared bounded
input ledger key to `(brick, actor, input, optional captured object)`; existing
player actions use None, exact-object entry uses Some(actualObjectId). The ledger
retains its existing1024entry bound.

An active/failed physical step needs exact object/spawn/provider/definition/scale
and region/model stamps, plus its selected method. Expired/previous ObjectEnter
must not satisfy a replacement body; one failed method/object must not suppress
all alternatives at the same source. Existing8object/global32visibility query
budgets,32actions,128cumulative resolved targets and fair global planning turns
remain shared. Discovery cross-products are capped before cloning/queries.

Native hand activation can flip a permitted slow vehicle through ordinary
Activate and credits its real mover. Weapon impulses also exist, but current
weapon-to-vehicle impulse handling discards the source when applying push; it
does not by itself prove the ObjectEnter scorer attribution required by a
physical-hit provider. That gap must be resolved through canonical attribution
or declared unsupported with negative evidence, never fabricated credit in AI.

Enemy-elimination goals need a truthful rule/MiniGame/provider success descriptor;
ordinary attack utility against any nearest visible opponent is insufficient.
Execution must reuse dated sight/memory, navigation/search and native combat.
A package-owned carryable return needs typed package semantics for actual item
acquisition/carriage/drop/return and scoring. Current ObjectEnter enumerates only
vehicles, so a package entity/flag is not automatically understood. These remain
required integration work rather than claims inherited from a ball test.

## Frozen acceptance intent

Every applicable positive needs a metamorphic variant changing authored names,
IDs, layout/order and irrelevant decoration while preserving semantic mechanics.
Each fixture must assert real movement/control, actual relevant object/hold/tool/
vehicle state, real admitted event and canonical variables/score/winner/round.
Tempting nearby decoys must be rejected. The following intent cannot be weakened
to fit implementation. All fourteen were open when this intent was frozen;
[the current pipeline checkpoint](npc-pipeline-current.md) and dated progress
entries record passing evidence and the remaining release checks.

| # | Required journey and evidence |
| --- | --- |
| 1 | Low ground goal delivered by ordinary walking/contact pushing; real attributed object entry and canonical success. |
| 2 | Same desired object goal with declared hold/carry available; select it only when materially cheaper/more effective than eligible contact. |
| 3 | Elevated/small region where walk-push is insufficient; attempt an available grounded alternative and observe actual physical/event outcome. |
| 4 | Two mechanically identical objects, only the exact authored/named spawner acceptable; nearby decoy cannot complete the goal. |
| 5 | Target object exposes its own valid control seat; board/drive into region through normal reach/permission/steering and canonical credit. |
| 6 | Ordered player checkpoints composed from existing rule state; wrong-order/nearer region is not the goal. |
| 7 | Multiple activation/puzzle actions cause real guarded rule progress and completion, including partial observed progress. |
| 8 | Enemy elimination composes navigation/combat, loss of sight and remembered/unchecked-space search; actual intended elimination/success. |
| 9 | Carryable item/return objective uses typed package semantics and ordinary pickup/carriage/release/return, without any engine mode-name test. |
| 10 | Human/bot or multiple bots compete/interfere; shared claims, real occupancy/credit and observation force truthful repair. |
| 11 | Planned object respawns/is replaced halfway; old input/identity/claim cannot count as the replacement's delivery. |
| 12 | Needed tool/weapon disappears halfway; ordinary release/cancel, invalidation and an eligible alternative/replan or honest failure. |
| 13 | Goal/rules/team change during execution; stale expected context and due-time effects cannot announce success. |
| 14 | Unreachable goals, unknown Add-On behavior and exhausted work/depth/model budgets produce bounded explicit diagnostics, no manufactured completion. |

At least one invented physics object/tool/vehicle with previously nonexistent
production identifiers must expose equivalent mechanics and work without a code
change. Vary geometry, mass, tool force/range, object/spawner count and interacting
actors. Relevant authority, held-object/credit expiry, source disappearance and
delayed guard changes are negative fixtures. Diagnostics stay bounded in existing
Explain/thought output: goal, action/provider and rejection/invalidation reason.

Existing checkpoint/puzzle/combat/permission tests are regression baselines,
not automatic evidence that every new intent is met. Sixteen independent actions
remain outside depth12; this is an honest budget negative, not permission to
omit other journeys. No arbitrary game understanding is claimed.

## Verification and adversarial closeout

Focused provider and actual-control checks precede affected crate tests, scoped
clippy and chaos/regressions; root runs the required full repository gate and
platform packaging checks. Pathological rule/object/action candidates are profiled
against existing budgets without timing thresholds or added work caps. Sustained
city activity and the reproduced ranged-flight deadlock retain their separate
acceptance/evidence; render/network/Windows hangs are not inferred from headless
Session timings.

Before completion, search production code for example-specific identity branches;
list every projected assumption and its observation/failure path; verify no
planner outcome mutates canonical success; check duplicated semantics against
native owners; challenge renamed/unfamiliar content; record unsupported effects,
flight/trajectory/permissions and provider limitations explicitly. Deliver a
packaged build through root with deliberately unusual human combinations that
exercise these seams. No agent-driven interactive gameplay is authorized.
