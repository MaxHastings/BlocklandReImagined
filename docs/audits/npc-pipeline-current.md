# NPC pipeline — integration checkpoints, 2026-10-03

The initial audit below was written before adding capabilities, as Maxwell
requested. This current checkpoint describes linked source, not a claim that
v0.2.2 is release-ready. The fourteen-journey contract remains in
[objective-driven-integration.md](objective-driven-integration.md).

## Current linked pipeline

**Authoritative discovery.** Creator-indexed Wrench programs and the canonical
MiniGame membership/round context supply native rule causes. Activation, player
regions, bot touch, exact spawned-object entry and supported native death inputs
use the event system's own context, targets, operation decoding and queries.
Read-only opt-in Add-On queries may declare bounded pickup/return desired states;
query attempts cannot commit writes, gameplay operations or output. Unknown
semantics are rejected rather than inferred from content names.

**Desired state and capabilities.** `DesiredState` separates real round-winning
intent or a declared package completion counter from `Cause` and grounded
`Executor`. The body and current inventory supply native locomotion/combat;
nearby permitted objects expose contact, explicitly declared native hold tools,
and their own ground control seats. Package pickup/return actions use exact
source/carriage/zone/counter declarations. Providers capture actor life, game,
round, team, source and method identities for live revalidation.

**Plan and execution.** The existing bool/integer planner retains its action,
node, depth, candidate, grounding and fair-turn budgets. All executors share
selection, interruption, validation, admission, wait and failure lifecycle.
Actions request ordinary navigation, activation, equip/trigger, contact,
boarding or vehicle controls. Combat uses dated observations and the existing
weapon admission path. No provider sets score, wins a round, teleports a body,
injects an event or duplicates the native hold/vehicle physics executor.

**Observation and repair.** Exact admitted input origins and real delayed rule
queries establish native progress. Bounded canonical death and round histories
identify the actual life, killer and winner; a projected win is insufficient.
Package completion is a changed declared counter, not a claimed round winner.
Actual object displacement/mover credit, grip, occupancy, source geometry,
inventory and authority invalidate stale assumptions. Failed methods cool down
independently. Search follows dated evidence and bounded unchecked places; it
never reads an unseen enemy's current position to refresh knowledge.

**Focused evidence.** Current headless tests pass paired renamed/layout-changed
contact, cheaper holding, elevated holding and rotated ground-vehicle delivery;
ordered checkpoints/puzzles; real elimination with loss-of-sight search and a
different hostile interrupting the retained target; unfamiliar package returns
and actual imported CTF pickup/carriage/score/team winner. These do not replace
remaining independent adversarial evidence, sustained measurements,
full repository gates or platform verification.
The independent held-out package-return/guarded-switch composition now passes
in both transformed worlds without a new production handler. Tightened physical
tests also pass real repeated exits/entries and exact native grip retention
through a six-second delayed action. The five independent adversarial tests now
pass exact named identity, real replacement, competing holders, native tool-loss
cleanup and a paired revoked due-guard negative. Retained momentum may legitimately
complete the goal after a tool disappears; only actual entry and canonical rule
execution can establish that win. A wrong native grip is released through normal
controls before retrying. Renamed/reordered search and package variants also
pass; independent review signs off all fourteen supported journeys and the
held-out composition at source/headless outcome level. Sustained measurements,
human acceptance and release gates remain separate.

**Limits.** Arbitrary Add-On effects, general program inference, thrown-object
trajectory planning, hand-flip/weapon-impulse delivery providers, novel tether
routes, cooperative stacking and aircraft/watercraft delivery remain unsupported.
Opaque death callbacks prevent a native elimination projection. Driving uses
conservative speed/turn heuristics; it does not prove arbitrary tyre/slope routes.
Shared legacy combat heuristics still exist, but new providers do not use IDs as
mechanics. Alpha declarations and formats remain disposable.

## Initial audit before capability expansion

The remaining sections preserve the original source-backed findings and smallest
gaps, including defects subsequently addressed above. They are historical
observations, not statements of the final linked implementation.

## Discovery and desired state

`session/events.rs::install_program` indexes creator-owned enabled programs.
`bots/objectives.rs::objective_snapshot` reads sources belonging to the bot's
MiniGame creator. It requires an active round and objective-enabled bot kind.
Actual grounding still admits only `onActivate`, `onRegionEnter`, and
`onBotTouch`. Adding `onObjectEnter` to the index and captured-input ledger has
not added a physical objective provider or executable delivery action.

The adapter uses the event system's `input_context`, `row_targets`, `row_intent`,
`rule_key`, and `rule_query`. It summarizes one complete input's rows, ordered
by delay then row index, into guarded expected effect groups. Supported
projection includes bool/integer state, the bot's real score, brick color and
round ending. Cosmetic effects with verified absence of callbacks are omitted;
unknown collateral or relevant reaction dependencies reject the candidate.
Named resolution and authority are not bypassed.

The sole runtime planner goal is currently `wins/<bot> = true`. This is a
**predicted fact**, produced by a projected `winRound` targeting that bot.
`endRound` alone, score alone, a death, item return or object entry do not
independently become goals. There is no general objective-provider hook for
Add-On policy, nor a typed desired physical state separate from the input
action. The mechanism can compose supported authored switches/checkpoints; it
cannot yet discover a solution to the user's exact-object entry objective.

## Capabilities available today

These mechanisms exist, but most are not actions in the objective model.

| Evidence source | Linked behavior and ordinary executor | Objective integration today |
| --- | --- | --- |
| Body tuning, movement kind, geometry, jets/energy/liquid | Shared bounded navigation; player `MoveInput` and native motor | Used to approach activation/region/touch actions |
| Current inventory/image/projectile/charge/ammunition | Native hand combat selects supported attacks; normal equip/trigger/image runtime; post-movement launch admission | Utility combat, not grounded elimination actions |
| Loose-body geometry, mass, contact-hazard capability and permission | Enemy-oriented physical approach; walking collision exchanges real momentum | Opportunity, not transport toward an authored region |
| Vehicle seats, controls/weapons, footprint and occupancy | Existing claims, reachable boarding, ordinary seat switching and wheeled steering/gun controls | Useful combat crew; no delivery objective |
| Actual native physical hold and authored hold trigger | Legacy Carry moves to open space, swings and releases through normal controls | Always wins holding utility; cannot retain a delivery goal |
| Package item pickup, worn images, state and zones | Existing package policy executes contact pickup, carriage/drop, return/scoring | Opaque to objective planning; brick bots excluded from zone callbacks |
| Dated enemy sight/hurt/allied memory | Follow last-known position, look around, expire memory; fire requires fresh sight | No unchecked-space search or intended elimination completion |

The new optional image hold descriptor is validated and has focused tests.
It does not confer transport competence by itself. The unlinked
`bots/physical_objectives.rs` is a draft, not part of the executing brain.

## Planning and utility arbitration

`bots/planning.rs` is the single pure deterministic bool/i64 planner. It
searches positive-cost actions with ordered guarded effects and bounded
admissible guidance. Runtime limits remain 32 actions, 128 facts, 256 nodes,
depth 12 and 4096 candidate evaluations. Grounding separately limits 64 sources,
256 rows, 128 cumulative resolved targets, 4096 terms and 64 KiB string content.
One globally rotated ready bot builds/searches a model per tick.

The runtime keeps the first selected action, executes it, then builds a new
plan after actual progress. It does not retain the complete proposed route for
diagnostics. Failed-model reuse checks fresh facts, action semantics and
game/round/team context. Failed-action cooldown is keyed by source/input;
that is insufficient to distinguish multiple objects or methods at one goal.

Objective is a candidate in the existing utility selector, not another brain.
Default base scores are Carry 1.0, Fly 0.9, Fight 0.8, Objective 0.65,
Chase 0.6 and Search 0.4, scaled by package kind weights. Thus combat does
**not** universally preempt an objective: close fighting/flying usually do,
ordinary chasing need not. Non-Interact selection releases interaction claims;
seated claims are also pruned. These lifecycles must be reconciled before
objective-directed holding/driving can reuse them coherently.

## Execution, observation and repair

Activation uses the actual current eye ray and five-unit reach, ordinary
unequip, then `Command::Activate` / `ActivateRelease`. Region/touch actions
only navigate; native sensors must admit the real input. No input is injected
because a projected plan or locomotion destination exists.

The bounded admitted-input ledger records real origin/tick; its new object
component records the captured original object. A selected step waits through
its real delay barrier and queries canonical affected properties. Any observed
effect change counts as progress, then the runtime replans. This is not proof
that every predicted effect happened, nor proof that this bot won.

Live source owner/name/rows, game/round/team, observations and deadlines
invalidate or repair steps. Death clears objective state. Rest/player-riding/
non-objective control pauses approach time; authored delay waits remain
absolute. Timeouts/no progress cool a source/input so an alternative can be
tried. Combat inventory/aim/launch revalidation is separate and already uses
real native state. Object/method identity, replacement, tool disappearance and
package objective state need corresponding shared action-lifecycle checks.

Canonical `MiniGame::Effect::RoundEnded` carries real player/team winners.
The current public view/rule round-end bridge does not retain those identities.
Fixtures checking score and a round-end marker therefore prove those outcomes,
but cannot alone prove who won. A bounded observation of the existing effect
is needed; planning must never manufacture a winner.

## Production-code audit

Read-only searches covered `bots.rs` and the linked behaviour, planning,
objectives, interactions, tactics and combat modules, excluding test sections.
No game/content-name decision branches for Soccer, Basketball, CTF, Steel Ball,
Gravity Gun, Jeep or Tank were found. Those terms occur in explanatory comments
or test content. Native input/operation names, image-state transitions and
mechanical vehicle families are executor semantics, not mode identification.
Package-owned game policy may name its own content; the engine must not infer
that policy from those names.

`objective_predicate`, `objective_effect`, and planning `Effect::Set/Add`
mutate only a private hypothetical fact map. The linked objective executor
does not directly write score, variables, round results, positions or physics,
and does not call `fire_input`. Its gameplay calls are normal equip/activation
commands. Ordinary combat/boarding/contact retain existing authoritative paths.

There are abstraction weaknesses despite that clean mutation boundary:

- Discovery, causal rule projection and action construction are coupled to
  three input strings; no independent desired-state/affordance boundary exists.
- Legacy fallback `bot_weapon` classifies a projectile-less short-reach tool
  as melee movement. It is not truthful knowledge of arbitrary tool effects.
- Legacy `bot_arm` excludes the fixed `CORE_TOOLS` identities. That is an
  existing equip heuristic, not a game-mode branch, but it means the complete
  linked brain is not yet descriptor-only. Supported new affordances must not
  inherit that exclusion as a substitute for inspecting their mechanics.
- Vehicle weapon impulses discard their source in the session bridge, so
  normal physical hits do not establish mover credit through that path.
- Existing object observation snapshots the whole vehicle collection; a new
  per-source candidate scan must not multiply that into unbounded work.
- Legacy Carry and claim cleanup assume enemy-oriented interactions; transplanting
  a delivery destination without lifecycle repair would be a compatibility hack.
- Existing tests largely use flat synthetic worlds, empty objective loadouts,
  remote human observers and a fixed creator. Permutations of insertion order
  are not paired name/ID/placement/decoration metamorphic evidence.
- Current diagnostics omit desired state, selected input/method/provider,
  phase and route; several distinct limit failures collapse to one message.

## Smallest architectural gaps, in order

1. Separate **authoritative desired-state discovery / causal projection** from
   **grounded action discovery**. Reuse the one rule interpreter and planner;
   an action carries a provider-owned executor and exact live identity stamp.
   Activation/region/touch must use that same seam before adding transport.
2. Establish one selected-action lifecycle in the existing brain: claims,
   interruption, validation, progress, completion and failure. It must preserve
   a supported physical objective across holding/boarding adapters without
   suppressing real danger or bypassing human permission/control mechanisms.
3. Observe existing canonical results, including actual object/input identity
   and winner effect. Extend bounded Explain diagnostics with selected action,
   phase/provider and precise failure; do not create a second debug interface.
4. Add only grounded missing affordances: physical contact, declared hold/
   release, control seats, bounded evidence-based search, and a typed read-only
   package desired-state provider for existing pickup/carriage/return mechanics.
   Resolve actor lifecycle/zone exclusions through the canonical package path.
5. Prove the fourteen frozen journeys, variants, decoys, failures and at least
   one held-out combination requiring no production change. Then measure
   pathological discovery and sustained real activity before release gates.

No arbitrary script understanding, universal physics oracle, autonomous
cooperative stacking, aircraft/boat delivery, hookshot route invention or
unbounded planning is implemented or promised by this increment.
