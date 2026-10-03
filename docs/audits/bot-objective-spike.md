# Bounded objective planning for bots

## Implemented v0.2.1 experiment — 2026-10-02

Root approved the runtime ownership and physical-actor context pivot. The new
`bots/planning.rs` and `bots/objectives.rs` are linked into the ordinary bot
brain. Standard Blockheads opt into objective utility through their package's
`bots.json`; creature policy remains package-owned. This is an expressive
experimental rule provider, not universal game understanding or a frozen mod API.

The canonical event adapter now gives a physical NPC **Player and MiniGame
when those slots are declared by the input**, preserves Bot/Driver, and never
fabricates a connected Client. `Trigger.client` still captures the real
spawning owner/driver/LAN account for quotas and legacy attribution. This
intentional alpha deviation removes v20's absent Player/MiniGame bot slots.
Touches remain `onBotTouch`; no `onPlayerTouch` is synthesized. Human and NPC
inputs share one scheduler, target resolver and permission path.

The pure scalar bool/i64 planner searches deterministic positive-cost actions
with ordered guarded effect groups. Groups use actual due-time/row order and
see preceding projected effects. State dominance includes depth. Accepted
search entries, candidate evaluations, facts, actions, terms and string bytes
have explicit bounds; arithmetic overflow is unsupported. Sixteen pure tests
cover order, partial progress, depth dominance, invalid/missing facts, overflow
and adversarial work/memory bounds.

The runtime discovers enabled `onActivate`, `onRegionEnter` and `onBotTouch`
programs through creator-indexed event sources. Decorative named bricks do not
consume candidate limits. Each action summarizes its complete input: canonical
scoped numeric variables, player/team scoring, round end/win and typed Color.
ColorFx, ShapeFx and PlaySound have verified no gameplay callbacks. Unknown
collateral invalidates that candidate. Variable-change child inputs on the
source or actual Target brick are conservatively unsupported. Relevant
same-creator timer/score/end reactions intersect observed keys; an unrelated
clock/color decoration or foreign creator does not disable a valid objective.
Dependencies whose reaction closure remains unknown receive explicit reasons.

Grounding is separately bounded before the planner: at most 64 candidate
sources, 256 candidate rows, 32 actions, 128 cumulative resolved targets,
128 facts, 4096 terms and 64 KiB string content. Full retained row snapshots
consume that budget before cloning. Named resolution may allocate one vector
bounded by the event world's existing named-target limit, then rejects excess
fanout before any per-target guard/effect/context allocation. Relevant reaction
inspection shares the same cumulative grounding budget. One globally rotated
ready bot builds/searches a model per tick; colliding owner-ID residues cannot
starve peers. Completion checks only retained bounded property observations.

Execution approaches actual brick bounds and uses normal aim/movement,
ordinary unequip and Activate/ActivateRelease command sequences. Actual reach,
ray target, special activation and package hooks remain authoritative. Regions
and touches require real admitted physical input; arriving nearby is not
completion. Existing occupancy causes a physical exit/reentry step. Delays
wait through the real input's barrier, compare available canonical affected
properties, and replan from actual progress. Active steps revalidate actual
physical source ownership and installed program owner/name/rows; failed-source
cooldowns expire on those edits. Source/game/round/team changes,
death, missing observations and blocked/no-progress approaches invalidate the
step. Failed source/input pairs have an eight-entry, 60-second cooldown so
reachable alternatives receive a chance; edits/scope changes and actual new
progress invalidate failures. Existing Explain emits up to four related NPC
status/reason lines, avoiding a new UI or hidden goal injection.

Objective paths continue bounded partial navigation segments only while actual
movement advances. Existing combat utility can interrupt; no normal controls,
score/variable writes or event-dispatch shortcuts are bypassed. Independent
ordinary-control proofs pass for a rotated off-grid doorway,
>155-unit path across bounded segments and a temporarily blocked roofed corridor.
Three idle actors form a stable 10-second stack, and a human ordinary jump can
climb an actor head to a 5-unit platform. Fixed navigation does not observe that
dynamic support; cooperative support jobs remain a measured capability gap.
See `bot-navigation-feasibility.md` for the explicit delivery pivot and limits.
Typed inventory/ballistics integration is linked from the separately owned
`tactics.rs`/`combat.rs` lane. Its actual launch-frame admission runs after
movement in the existing weapon phase, retaining the shared plan-tick budget.
Current actor/hostility, inventory/image/ammo and rederived capability checks
precede the exact actual-direction geometry check. Actual rays must intersect
the intended actor; release-only charge graphs exclude initial or held/timeout
Fire routes before safely skipping geometry while holding a charge.
Unsupported package/mounted/portal mechanics preserve their existing executor.

Current focused evidence: eight ordinary-control rule compositions passed,
including four renamed activation sources with guards/delayed win, reversed
ordered regions, repeated counter reentry, 96 irrelevant named decorations plus
an unrelated timer/cosmetic feedback, unknown collateral, relevant reaction
rejection and an inaccessible cheap region followed by a reachable winning
alternative. The 4096-name fanout negative and creator Explain assertion pass;
independent target-variable child-reaction rejection also passes. Canonical NPC
context/authority and round-robin unit
tests passed; read-only event inspection proves missing/named/derived targets
without scheduler mutation. Final sim152/events1 libraries and scoped
sim/events/chaos clippy pass. Typed combat3 and independent navigation/race8
pass; liveframe wall/actual-ray-miss and charge graph negatives are included in
the libraries. Full combined timing/workspace/platform acceptance remains open.

Limits are deliberate and visible: no opaque Add-On action inference, general
relay/variable-child closure, temporal search across arbitrary pending jobs,
team-score guard projection, objective object transport, hookshot routing,
cooperative stack/role planning or mounted inventory provider. Projected WinRound
means the actual NPC-target operation is expected; runtime never announces a
win from that prediction. Actual round end cancels the objective, and fixtures
assert real canonical scores/one-time completion. A new canonical winner
observation would be needed for a general diagnostic distinction between the
NPC winning and an unrelated actor ending the round. Exact named-ball physical
delivery and broader cooperation remain separately tracked, not silently done.

Existing editable Workshop race/region examples provide the creator path:
create a MiniGame, plant a Blockhead spawner, wrench the region Event rows,
then use Explain to inspect a bot's current objective or unsupported reason.
The human performs interactive validation; agents only use headless fixtures.

## Historical reviewed proposal (before runtime approval)

The following records the earlier review, alternatives and proposed stages.
The implemented section above supersedes proposal/pending statements below.

## 2026-10-02 reviewed implementation boundary

Root relayed Maxwell's latest full autonomy to pivot scope when evidence
warrants it, with a combined v0.2.1 release after root is satisfied. The NPC
deliverable remains an expressive integrated experiment, not standalone search
tests. The useful first delivery is authored activation/region compositions,
typed progress and real round outcomes, conservative delayed observation,
ordinary movement/activation, and explicit unsupported mechanics. Additional
providers follow only when their physical mechanism is grounded. This pivot
must be recorded with outcome and limits in the work plan; no missing example
is silently declared solved. Root coordinates integration and release.

### Context decision proposed to root

The current classic bot branch in `events.rs::fire_input_with` sets Bot and
Driver when advertised, captures a real spawning-owner/driver/LAN client for
event attribution, and omits Player/Client/MiniGame slots. Native Instigator
is available on all catalog inputs. `rule_subject` can derive a MiniGame from
Instigator even with no MiniGame slot; it cannot create a missing row target.
Thus a fallback in `rule_query` alone cannot fix a Player-target win row:
`EventWorld::targets` never schedules that row. Native `onRegion*` declarations
advertise Player, yet this bot branch still suppresses it.

Prefer the explicitly authorized compatibility pivot: **populate truthful
physical Player and MiniGame slots for bot-caused inputs whenever the input
catalog declares them**. Player refers to the actual NPC peer/actor. MiniGame
uses the same authoritative `player_targets` MiniGame selection policy as a
human physical actor. Never populate a fake Client slot for an NPC. Preserve
Bot/Driver slots and the existing real captured `Trigger.client` for quotas
and legacy attribution. Keep owner/game checks in `rule_game`, event target
permission, stale checks and the scheduler. No new authoring mode, output
name handler, public context framework or second executor is needed.

This changes classic Player/MiniGame slot absence deliberately; it is a small
coherent physical-actor model for the new engine. Alternative native-only or
per-row overlays would preserve that absence, but introduce two contexts for
the same action and must propagate them through all deferred/relay/child
paths. Guard presence must not secretly decide who Player means. Context
normalization belongs to the canonical event adapter, so objectives and
ordinary gameplay see exactly the same semantics. This is a proposal pending
root's integration decision, not a claim that source has changed.

Required context tests: ordinary bot activation and native region input
target the real Player/Instigator NPC; Slot::Client stays absent; existing
Bot/Driver rows and captured real client still work; human contexts remain
unchanged; absent actor/client/game remains absent; remote unrelated MiniGame
and foreign rule owner cannot obtain authority; delayed death/deletion/team
change and object respawn remain stale/revalidated. Mixed Player and Bot rows
must execute against their actual actors. `Command::Activate` uses the peer's
command sequence, separately from movement input sequence.

### Small internal provider API

`planning.rs` stays std-only and unaware of Session. Its reviewed shape is
`Action { id, cost, preconditions, effect_groups }`, with
`EffectGroup { guards, effects }` evaluated against evolving projected facts.
Facts are only bool/i64; identity and authored text references are resolved by
the provider into scoped keys or booleans. Missing/type-mismatched facts are
unknown and fail predicates, including inequality. Limits bound accepted
search entries (queue and visited labels), depth, actions, candidate
evaluations, facts and aggregate model terms/string bytes. The total accepted
entry count includes replacements, so memory cannot grow without consuming a
deterministic budget. Same-depth state dominance retains shallow alternatives.
Overflow invalidates the projection. These are predictions, never execution.

The proposed **new** `bots/objectives.rs` contains the grounded rule provider
and transient lifecycle only after root agrees integration ownership:

```text
ObjectiveSnapshot {
    stamp: { world/program revision, actor, game, round, team },
    facts: planning::Facts,
    actions: Vec<planning::Action>,
    goals: Vec<{ stable id, utility, planning::Goal }>,
    executors: ordered map action id -> GroundedStep,
    diagnostics: bounded typed reasons,
}
GroundedStep {
    source/target identities + program revision,
    kind: ActivateBrick | EnterRegion,
    expected input and real target context,
    approach geometry + completion predicate(s),
    latest known row delay and deterministic observation deadline,
}
Lifecycle = Approaching | Aiming | Acting | AwaitingObserved | Complete
            | Invalidated(reason) | Failed(reason)
```

Provider functions take an authoritative read-only Session snapshot and a
physical actor ID; no user-injected goal overrides or mutation callbacks.
Session/root integrates their `snapshot`, next-step validation and lifecycle
through ordinary brain utility/nav/control. The executor turns an actual
brick box into a reachable approach point, aims by normal `MoveInput`, unequips
through ordinary inventory controls when necessary, and issues
`Session::command(bot, peer.last_sequence + 1, Command::Activate)` only within
real reach/line-of-sight and bounded cadence. Activation's normal
`target_through`, `special_activate`, package hooks and empty-hand weapon
behavior remain in force. Touch/region entry is physical movement. No
`fire_input` calls, direct transform writes, direct variables, score awards or
round ends are execution shortcuts. Do not count arrival alone as completion.

Root supplies narrow read-only adapter access where private sibling state
currently prevents reuse: installed program IDs/revisions, the canonical
trigger-context builder, resolved row targets, operation permission checking,
canonical `rule_query` and scoped `rule_key`, current pending/trace observations
and real MiniGame winner/round data. Reuse existing helpers rather than copy
their meaning into string-key heuristics. Candidate discovery uses
`Events.installed` / `EventWorld::program`, maintained by edits/map adoption,
and a deterministic scan cursor/cap. Do not scan every world brick per bot
per tick; cap discovered rows, candidates, facts and diagnostics. Cache
program-only summaries by revision; actor/game/round facts are refreshed.

Goal discovery follows reachable, enabled typed win/score operations and their
actual guards. A goal must distinguish the actor/team's real winning outcome
from merely `RoundOver` (an opponent ending the round is not success). Preserve
variable namespace, source owner, game, round, subject class and identity;
renaming/removal/respawn invalidates grounded steps. No custom rule mode names
or arbitrary four-brick counter. Full input effects must be summarized;
isolating a favorable row and ignoring a reset or score side effect is wrong.

### Viable implementation stages and limits

1. **Canonical contexts and observed keys.** Root chooses/implements the
   physical actor context pivot above, with authority/stale tests. Expose
   bounded read-only observations and installed program cursor. A scalar
   projection cannot substitute for these.
2. **Immediate known rule compositions.** Ground actual activation and region
   candidates whose complete relevant typed rule effects/targets/guards are
   known. Predict scoped Variable set/add and permitted canonical player/team
   scoring and WinRound/EndRound effects. Account for canonical score overflow,
   score-triggered end/respawn and output permission, or reject that edge as
   unsupported. Known irrelevant cosmetic-only outputs may be ignored only
   with an explicit truthful descriptor. Unknown package output, relay,
   derived target, non-scalar dependency or unmodeled triggered reaction
   blocks the affected summary. Guards see preceding applicable row effects.
3. **Delayed observation.** Group known row effects in actual scheduler order,
   including due time then row order; authored order alone is insufficient
   across different delays. A plan is an expected successful sequence, not a
   guarantee of future guards. After causing an input, wait through its
   relevant delay barrier, observe actual scoped transitions/trace, and replan
   from the current state before the next physical step. No speculative
   immediate effect at activation. Pending admission failure, cancellation,
   changed guards/identities/permissions, timer/variable-child interleaving and
   observation timeout are typed invalidation. Do not claim a delayed win
   until the real MiniGame reports it. If the complete reaction closure is
   unknown, diagnose it rather than flatten its favorable rows. Temporal
   planning beyond this observed barrier is a later refinement.
4. **Utility/nav integration and held-out evidence.** The standard Blockhead
   policy can opt into objective utility alongside Interact; creature policy
   remains package-owned and extensible. Combat urgency preempts, releases
   claims and resumes by replanning from real partial progress. Run unfamiliar
   renamed rule layouts, shuffled source order, different count/sequence,
   wrong-team/owner/targets, unreachable brick and delayed reset cases through
   ordinary controls. Assert actual event traces, variable keys, canonical
   scores and winner identity, not only projected plans.
5. **Grounded physical extensions.** Exact-spawner object movement, tool
   capability selection, vehicle jobs and stack/support actions use the same
   lifecycle only when their real mechanics have evidence. Physics can
   disprove a stack strategy; document the result and scope pivot. An opaque
   Add-On effect remains unsupported. Broader GOAP/schema work is justified
   only if it improves the delivered experiment.

Fundamental unresolved holes are context/target equivalence; discovery and
bounded incremental search integration; reaction closure/actual delay order;
canonical winner observation; creator-owner authority separate from event
quota client; actor/team/round namespace invalidation; reachable activation
geometry; claim/preemption and completion evidence; unknown output semantics;
and physical support/transport feasibility. The pure planner solves none of
these alone. Root assigns existing-file ownership before implementation.

## Proposal

Build a small, bounded GOAP-style planner that composes actions the simulation
already knows how to execute. Keep the current utility selector, nav system,
shared claims, permission checks, and ordinary control path. Give a selected
objective a costed plan over grounded actions with explicit preconditions and
effects; execute its first action through the existing behavior/provider, then
revalidate and repair the remaining plan from current observations. The planner
must not bypass physics, MiniGame scoring, event scheduling, or Add-On policy.

This is a practical extension of current architecture, not a promise that NPCs
can solve every authored game. It can plan across unseen combinations of known
actions and known rule semantics. An Add-On with new mechanics needs to provide
truthful action metadata or a registered provider that uses existing authority
and control mechanisms; names and opaque scripts alone are insufficient.

Keep v0.2.1 stability work first. Treat the planner and action metadata as
experimental internal work: no persistent save or protocol contract should be
declared from this spike. The [FEAR planning paper by Jeff Orkin](https://www.gamedevs.org/uploads/three-states-plan-ai-of-fear.pdf)
describes the useful practice here: separate goals from authored actions,
search action sequences using preconditions/effects and costs, and execute
actions with replanning. GOAP is a good fit for composition over a finite known
action model; it does not infer hidden Add-On behavior or invent grounded
actions by itself.

## Existing foundations to reuse

- `crates/sim/src/session/bots/behaviour.rs` scores Carry, Fly, Interact, Fight,
  Chase, Search, Return, and Wander every tick. Objective desirability should
  enter as another utility candidate, with existing bot-kind weights and
  current combat priorities deciding when to suspend it. Do not replace this
  selector with an always-running plan.
- `crates/sim/src/session/bots.rs` owns `Goal`, pathing and movement; path
  planning is incremental and reuses shared navigation. A plan action should
  resolve to an existing point goal / grounded provider, then yield controls
  through the usual player step. Add bounded plan state beside the brain's
  transient state, not saved game state.
- `crates/sim/src/session/bots/interactions.rs` already supplies bounded
  environment observations, claim acquisition, invalidation, failure cooldowns
  and ordinary-control execution for seats and physical hazards. Extend that
  lifecycle for an action only when it has a scarce resource/role; do not create
  a second reservation system.
- `crates/events/src/rules.rs` defines typed subjects, properties, conditions,
  and `RuleOp`; `crates/sim/src/session/rules.rs` evaluates them and calls real
  MiniGame/world mechanisms. Reuse this semantic path to reason about state and
  let the authoritative scheduler remain the only rule executor.
- `crates/events/src/catalog.rs` carries the Add-On event catalog. Its current
  input/output/target declarations describe names, parameter types and package
  provenance, not effects. A new Add-On mechanic therefore needs an explicit
  semantic provider/descriptor before a planner can compose it. Keep that
  descriptor experimental until multiple packages validate the shape.

## A concrete bounded model

Represent planner state as a compact set of observed, typed facts keyed by
stable in-session identities and the scope that owns them: alive/existence,
position or region occupancy, MiniGame/team, score/round state, rule variables,
object kind/spawner/velocity, current interactions and relevant permissions.
Do not expose arbitrary reflection or script evaluation. Facts unavailable to
the bot or unresolved by typed rule semantics remain unknown, never guessed.

Each grounded action instance has:

- a stable action/provider identity and concrete target(s);
- typed preconditions evaluated against the current snapshot;
- typed effects used for planning only, with their authority/source identified;
- a cost combining expected action time, navigation cost, risk/failure history
  and scarce-role contention;
- an existing executor that issues ordinary controls or a typed operation and
  reports success, failure, or still-running progress.

Search only a small deterministic candidate set with fixed limits on action
count, depth and expanded nodes. Prefer low-cost plans,
but preserve current utility priorities: fighting or reacting to danger can
preempt a plan. On each execution step, refresh observations and rule state,
validate the next action's preconditions, and repair from the new state if the
world, rules, game/team, target identity, permission, or claim changed. Drop a
plan when it is impossible or repeatedly unproductive; report a typed reason
such as unknown semantics, inaccessible target, lost claim, invalidated fact,
or expansion budget exhausted.

No planner effect should directly award score or end a round. Those outcomes
come only from the existing canonical Add-On/MiniGame rule path. A prediction
that a typed `RuleOp` would change a variable is not permission to write that
variable; the bot must cause the input through a grounded action and observe the
authoritative transition.

## What the current rules can ground

`crates/events/src/rules.rs` has a typed condition vocabulary: Exists, Alive,
IsInstigator, Score, Team, RoundOver, Color, Kind, SpawnedBy, Speed, Variable,
Occupants, and Opponents. Conditions are capped at eight per row
(`MAX_CONDITIONS`). `RuleOp` includes scoped Variable set/add, player/team score,
WinRound/EndRound, SetTeam, ObjectVelocity, ResetObject, RegionSize, and Explain.
`rules.rs::rule_query` provides the observed values; `apply_rule` maps actions
to the MiniGame/world mechanisms. The scoped variables include player/team/game
identity and round, so a plan's fact key must retain that scope.

The event catalog and scheduler also matter: a rule is a guarded delayed row on
an input, addressed through a target slot. Rows may branch, cycle, or run after
a delay. `events.rs` sends a bot touch through `onBotTouch`; it is incorrect to
assume that a human's `onPlayerTouch` row fires for a bot. A plan is executable
only if the chosen action can produce the actual input and target context.
Unknown package output/target semantics are an explicit unsupported edge until
the package supplies a typed description/provider.

## Example plans and limits

### Four distinct activation bricks

Given four named bricks, inspect their enabled rule rows, guards, targets and
actions. If each valid activation increments a per-player `Variable` and only
the required count/flags enables `winRound`, the planner can represent each
reachable activation as a grounded action, path to it via nav, cause its real
input, then observe the authoritative variable transition before selecting the
next action. Distinct source IDs and scoped progress prevent repeatable bricks
from masquerading as four separate steps. Include decoy rows, repeated
activation, reordered rows, reset branches, and an unreachable brick in tests.

This only works when an existing player action can trigger the configured input
and the rule scheduler exposes the resulting typed state. The code currently
has touch and activate event paths, but their exact reach/activation conditions
must be grounded from the live mechanism; a planner cannot synthesize the
input directly or mark a step complete because the bot reached a location.

### Ordered checkpoints

The practice rule programs in `crates/sim/src/session/rules.rs` already show
checkpoint sequencing with a player-scoped `checkpoint` variable, guarded
region/object-enter rows, final scoring and `winRound`. This is a close
semantics test: infer the expected next checkpoint from observed player state,
path to its region, enter it physically, then verify the variable changed. A
decoy/out-of-order entry, delayed reset, team-scoped variant and race between
two bots test repair and variable scope. The plan does not assume the next row
in source order is the next objective.

### Named Steel Ball to a goal

The rule system can identify an object by Kind and SpawnedBy and observe
`onObjectEnter` in a region. A grounded physical action must still move that
exact live object: approach it, use an already-supported push/contact or a
compatible tool, and let physics determine displacement. Existing hazard
interaction can claim the body; Gravity Gun use is governed by its current
`bot` reach/hold definitions. The engine must observe the ball enter the actual
goal and then let the configured Add-On rule award canonical score. Ball respawn
or identity change, obstruction, momentum away from goal, owner/team mismatch,
and natural motion without scorer attribution all invalidate or revise the
plan. Do not add an unconditional “carry SteelBall to goal” handler.

If an Add-On introduces a new mover, tether, teleport, or goal action, it needs
a grounded provider with real preconditions/effects and an executor through
authoritative mechanics. Without that metadata, the route is unknown; GOAP
cannot infer semantics from the action's name.

## Cooperation and three-bot stack

Existing claims are advisory and bounded: one claim per bot, exclusive seats or
hazard bodies, expiry, progress, invalidation and retry cooldowns in
`crates/sim/src/session/bots/claims.rs`. Reuse that policy. Team cooperation for
a multi-step goal needs named roles/resources, readiness/hold-position states,
progress and failure observations, bounded waiting, and invalidation on support
movement/death/team change. Planning must account for another bot's claim and
release the role when combat preempts it. Current claims do not establish
multi-body support behavior.

For the user-requested three-bot stack, first verify that ordinary movement,
jump and crouch controls can produce stable actor-on-actor support. The foot
motor's step-height scan in `crates/motor/src/torque.rs` filters out
`Kind::Actor`, so ordinary nav currently does not step onto another player.
The current code does not prove that stacking is impossible under dynamic
contacts, but it also does not provide a grounded climb/support action. Treat
the stack as unsupported until a headless physics test proves it; then add a
bounded support action with measured preconditions/effects and replan on loss
of support. Do not use teleport, hidden ladders, direct transform writes, or
special placement to make the acceptance test pass.

## Weapon and tool competence audit

The same grounded-action approach can cover weapon families without per-weapon
behavior scripts, but the current bot does not yet select or aim with that
generality:

- `crates/sim/src/session/bots.rs::bot_weapon` reads only the image currently
  mounted in slot 0. It builds one private `Weapon` record from `Image::bot`,
  `Image::melee`, charge-state data and its referenced projectile's reach,
  speed, ballistic fall and explosion radius. `bot_arm` equips the first real
  inventory item; it does not compare weapon capabilities or objective fit.
- `crates/weapons/src/lib.rs::BotUse` describes tap/hold, reach and preferred
  near distance. `Image::charges` / `fires_on_release` expose charge release;
  `ProjectileDef` already describes speed, gravity/ballistic flight, lifetime,
  collision and explosion; `Image::rope` describes rope rendering. These are
  useful mechanical inputs, but the bot record collapses them into a few combat
  numbers and loses the action's purpose/effect.
- Target selection is sight of an enemy. For moving projectiles, the aim adds
  constant-velocity lead using `distance / speed` and a gravity compensation
  term. It aims at `seen.eye - 0.5m`, not an authored body/ground/anchor point,
  and does not solve a high/low ballistic intercept or validate the full curved
  flight against geometry. `bot_fire_clear` is a straight-segment ally/splash
  safety check. This helps ordinary shots; it is not a general aiming solver.
- `Weapon::band` uses melee/reach/splash to choose a fighting distance. Trigger
  handling respects tap vs hold, charge release, and mounted gun charge. Melee
  body bites use a separate reach/cooldown path. Gravity Gun demonstrates a
  held tool (`BotUse::Hold`): when an object is held, the Carry behavior takes
  it to open space and throws it. That behavior is not an objective-directed
  “move this ball to goal” operation.
- The Grapple Rope port shows a real mobility mechanic: projectile collision
  invokes package logic that checks line of sight and uses typed `tether(...)`;
  the player motor simulates the rope. The bot brain has no grounded action
  that chooses an anchor, evaluates a route through a tether, or releases/reels
  for navigation. A hookshot is currently just a weapon image/projectile to
  generic combat aim.

The narrow extension seam should describe an image/weapon as a set of typed
action capabilities, not add one bot branch per named weapon. Reuse projectile,
image and package mechanics for these dimensions where the data is authoritative:

- activation: tap, hold, charged press/release, sustained hold, or provider
  action with completion/cancel signals;
- effect/capability: direct harm, splash harm, melee contact, ballistic hit,
  physical impulse, hold/carry, or player tether/mobility;
- grounding: valid target classes and target point (tracked body, feet/ground,
  loose physics body, or a surface/anchor), maximum range, charge/fuse timing,
  line/trajectory rules, and team/splash safety;
- executor: existing trigger/control or package operation, retaining current
  attribution, authority checks and physics. Unknown package effects stay
  unsupported until their owner supplies a descriptor/provider.

The planner can then ask “which available capability makes progress on this
goal?” The combat utility behavior still arbitrates urgency and may preempt;
the selected executor still uses the normal weapon runtime. Aim solutions
should derive from the declared projectile flight model and target point, then
check an engine-consistent trajectory, friend safety and actual range before
firing. A high-arc solution is valid only for a ballistic projectile whose
definition and collision semantics support it. Tether movement must replan
from the actual tether state and anchor, not pretend the bot is already at the
endpoint. Charge, fuse, release, ammunition, lost target, and failed contact
are action lifecycle facts, not weapon-name exceptions.

Concrete cross-family acceptance fixtures should keep Add-On ids/names
unfamiliar to the planner and assert actual runtime outcomes:

1. A hookshot profile fires at a reachable static anchor, attaches through its
   package collision/typed-tether path, traverses by normal movement controls,
   and releases/replans when the route ends or anchor validity changes.
2. A splash rocket chooses an appropriate impact point for a target protected
   by geometry, checks the shot and ally blast risk, and does not fire inside
   unsafe self/ally radius. Test static-ground aim and a moving target.
3. A ballistic spear, bow and vehicle cannon use their own speed/gravity/muzzle
   data to lead a moving target; compare predicted impact with the actual
   projectile path, including a blocked low arc and available high arc. Mounted
   aim must remain independent of driver steering.
4. A short-range sword/knife closes to valid melee reach and attacks through
   the ordinary trigger/cooldown path; a long-range tool with no damage
   semantics must not be misclassified as melee just because projectile data
   is absent.
5. A Gravity Gun holds a specifically identified loose object, moves it toward
   a real authored region using physical forces, releases only when that
   provider's goal condition can be observed, and repairs after a blocked
   carry or object replacement.

Run these in headless deterministic fixtures and inspect actual damage,
projectile/tether/hold state, collision outcomes, attribution and MiniGame
results. Compare action selection and combat utility against the current
baseline. Rename each profile and vary projectile/tool parameters to catch
name-based special cases. The existing implementation already handles basic
projectile leading, splash spacing/safety, charge state, hold trigger, melee
reach, vehicle weapons and physical carrying in isolation; those are regression
baselines, not full competence. New or script-owned mechanics join the shared
planner only when they expose grounded semantics and an ordinary authoritative
executor.

## Acceptance for the first planner increment

- Unit tests cover deterministic bounded search, precondition/effect matching,
  action costs, plan invalidation, and repair when an observed fact changes.
- Headless integration tests use held-out authored rule layouts for the four
  bricks, ordered checkpoints and named-ball goal; no per-fixture handler or
  direct objective injection. Assert actual rule traces, variables, score and
  round state, and that movement/action came through ordinary controls.
- Unavailable Add-On semantics, disabled inputs, mismatched touch targets,
  wrong team/player scope, changed source identities, moved/reset targets,
  unreachable paths, and planning budget exhaustion produce clear failure
  diagnostics instead of speculative completion.
- A three-bot stack remains a separate acceptance case and is reported as
  unsupported until ordinary-control physics and cooperative role execution
  both pass headless proof. No success claim from a metadata-only planner test.
- Benchmark worst-case rules, bots and candidate actions against the existing
  per-tick navigation/interaction budgets. Plan search must be deterministic,
  bounded and preemptible; no blocking work in the fixed-tick update.

Keep these tests behind the existing rule/action infrastructure and add
experimental semantic metadata only for a concrete mechanic needed by the
examples. Do not freeze a public package API in this spike. The practical
target is compositional capability over known, grounded mechanics, with an
honest “unknown” result when the world contains behavior the engine cannot yet
describe.
