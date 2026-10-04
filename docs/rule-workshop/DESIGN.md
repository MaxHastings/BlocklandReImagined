# Rule Workshop: chosen semantics and assessment

Rule Workshop ships as an experimental creator tool. Its format and
semantics remain alpha decisions, without a beta compatibility freeze. The
existing `Row`, typed catalog and bounded event scheduler remain the sole
execution path. Optional guards and a small native vocabulary are added to it.
There is no separate RuleProgram interpreter/export contract. The attachment
briefs informed the experiment; they do not define mandatory infrastructure.

## Choices to test deliberately

1. **Familiar rows first.** Classic input/target/output/parameters, delays,
   named bricks, relays and cancellation keep their meanings. IF fields appear
   only when added. Conditions are AND; OR requires another row. Actions stay
   one per row. Basic `onActivate → Self → setColor` has no state requirement.
2. **IF checks when the action is due.** Delay captures the row, context and
   selected targets, then conditions read current world/state when executed.
   A delayed action can skip because another row changed state in the meantime.
   Earlier successful rows in the same phase are visible to later rows.
   Edits/enabling changes do not rewrite already captured jobs. Named target
   membership is selected at activation, not rediscovered after the delay.
3. **No fabricated actor.** Instigator is optional; it never becomes the brick
   owner as a fallback. Ordinary activation/touch supplies the player. Death
   supplies the victim as Player and the killer as Instigator. Environmental
   death has no Instigator. Objects use the existing five-second mover credit
   from pushes/throws/holds, hammer/flip and sufficiently fast walking contact;
   absent credit, a driven vehicle supplies the occupant of its authored control seat. Natural
   motion remains unattributed. A recent pusher can take priority over a driver.
   Reset creates a new object identity and loses old credit.
4. **Absent conditions are false**, even `!=`. `Exists` explicitly returns
   false for absent context. Wrong subject/property pairings yield absent
   values. Booleans/text support equality/inequality; numbers also support
   ordered comparison. Object Speed is rounded world units per second.
5. **Small transient integer state.** Default is zero. Five scopes are enough
   for the delivered recipes and object experiments: Brick, Player, MiniGame,
   Team, Object. Storage is isolated by builder, captured MiniGame and current
   round, then subject identity and case-sensitive name. Brick writes use the
   selected target; IF Self always means the source. Player/Team/Object scopes
   use captured context, not a text-supplied entity ID. Additions reject overflow.
   State is not a save/checkpoint contract; MiniGame Reset clears that game's
   workshop state. Loading authored builds does not restore counter progress.
   Names intentionally share state across a builder's bricks: two courses using
   `checkpoint` in the same match share progress until you rename the key.
6. **Canonical scoring and rounds.** Player points call the real MiniGame
   scoring path. `addTeamScore` awards real points to the targeted team member;
   IF Team Score sums current members' real scores, so changing/leaving a team
   changes the total. No independent team scoreboard is invented. `winRound`
   uses the existing round ending system, naming the player and their current
   team; `endRound` names no winner. Configure automatic MiniGame points
   deliberately to avoid counting twice. Recipe thresholds are editable IF
   rows, not game-mode code. Reset, damage, healing, equipment and other classic
   actions continue through their existing implementations.
7. **Regions are center sensors.** Player sample point is feet + one unit Y;
   ball/vehicle point is its center. Inclusive axis-aligned bounds, centered on
   the brick. Custom dimensions persist/copy with the brick, and width/depth
   rotate on a quarter-turned copy. A swept line test detects complete fast
   passages, delivering Enter then Leave. Teleports, respawns and portal jumps
   are jumps, not travel: only the region the object lands in sees it enter,
   never the sensors on the straight line between. Stay/timers fire every 120 ticks on
   the shared 120 Hz clock, not one second after each individual entry.
   Occupancy is updated before the event phase, so IF sees current occupancy
   for that tick. Opponents excludes the actor and same-team players; without
   teams, other players oppose you. These are headcounts, not weighted capture
   meters, and round ending does not implicitly freeze physical motion.
8. **Authority stays local to the existing systems.** Match facts go to the
   match owner's listening bricks. Region players must share the builder's
   MiniGame, including both being outside games in free build. Objects must have
   that builder's spawn brick; uncredited objects remain observable. In a match,
   credited movers must share the builder's match. In free build an owned object
   remains observable regardless of the mover's match. Score/team/
   round actions require the match owner as builder; delayed player actions
   reject a target who left the captured match. Object actions require an owned
   spawner. These conservative limits make cross-owner collaborative builds an
   important later question, not an invented permission architecture here.
9. **Generated match facts follow the existing phase boundary.** A score or
   round action generated inside the event phase queues its native match fact
   for the next observation tick. Variable changes are immediate; their changed
   input follows the existing deferred package-input path, retaining match/object
   context. Brick-scoped writes notify the changed target brick; other scopes
   notify the source brick. Deleted entity state is reclaimed once per second,
   including worlds with no regions. Timers and reactions
   share the existing scheduler limits; there is no second operation bus.
   Deferred match facts cap at 256 queued facts; overflow skips the fact and
   reports the cap through the existing bounded event diagnostics.
10. **Cancellation retains the classic special case.** Unguarded zero-delay
    `cancelEvents` participates in the old activation prepass. A guarded cancel
    checks IF and cancels in normal due-row order. The classic switch cancels Self,
    clearing its authored delayed jobs (including ones aimed at the named panel). A MiniGame's
    existing LAN/non-LAN cleanup policy still controls legacy schedule cleanup;
    workshop state resetting does not itself cancel all jobs. Test delayed
    actions across Reset, especially ones without a round guard.
11. **Formats are disposable.** Rows/regions now travel in saves, copies and
    replication so you can test them. Matching branch clients/hosts are required;
    checkpoint schema changes are experimental. No migration, compatibility
    freeze, collaborative revision protocol, provenance graph or general VM is
    part of this branch. v20 import remains its separate conversion path.
    The integrated release uses the normal per-user settings/save profile on
    every platform. The earlier branch-only Windows builds used an isolated
    RuleWorkshop profile; those files are not moved into the release profile.
12. **One real Add-On proof.** `rule-workshop-toys` registers `cycleRoute` and
    Red/Green/Blue facts through the existing package catalog and script hook.
    They appear alongside core vocabulary, and the same rows/IF fields run their
    responses. Its private per-brick switch state resets on server/package reload,
    not MiniGame Reset. This difference is explicit evidence for deciding how
    package state and creator-visible state should relate later.
13. **Authored spawner identity is distinct from kind.** Object Spawned by
    reads the creator-owned vehicle/ball spawn brick's name, comparing text
    without case. Missing, deleted, foreign or unnamed spawners are unavailable,
    including for inequality. Respawn creates a new live object while keeping
    its authored spawner relationship; an old captured Object never silently
    becomes that replacement. A shared name intentionally matches multiple
    spawners. This small gameplay relationship is not a general provenance API.
14. **Physical NPCs are players, not connected clients.** Bot-caused inputs
    supply Player and MiniGame when the input declares them, through the same
    canonical context path as human actions. Bot/Driver and real account quota
    attribution remain intact. Client is absent for an NPC; bot touch remains
    onBotTouch. This deliberately changes the earlier v20-style absence of
    Player/MiniGame for bots. Test existing bot events rather than assuming
    legacy quirks define the experimental model.
15. **Bot prediction is conservative and disposable.** Creator rules supply
    supported activation/region/bot-touch, exact spawned-object entry and native
    elimination causes. Opt-in typed package queries may declare pickup/return
    goals; querying them cannot commit gameplay writes. Grounded actions use
    ordinary movement, activation, contact, declared holding, ground-seat
    driving, combat or package pickup/zone controls through one bounded lifecycle.
    The scheduler and native package callbacks alone execute their effects.
    Unknown relevant collateral or reaction semantics reject a candidate.
    Real input, physical state and canonical result observations establish
    progress; package completion alone does not imply a round winner. Failed
    methods cool down so alternatives can be tried. Explain includes desired
    state, selected action/provider, phase, proposed route and failure details.
    This is neither a public goal format nor inference of opaque Add-Ons.
    Inventory tactics similarly use native mechanical descriptors and actual
    launch-time checks, leaving unsupported script-driven mechanics on their
    existing path. A paused approach excludes time spent fighting/resting/riding,
    while scheduled event waits remain absolute. Resting/riding controllers do
    not take the shared planning turn. One bounded failed search may be reused
    only against an equivalent live model; changed facts/authority invalidate it.
    Supported native attacks retain aim/valid non-firing charge holds but wait
    for canonical spawn protection to end before spending ammunition or releasing.
    See the [v0.2.2 checklist](V0.2.2-PLAYTEST.md) for supported creator cases
    and limits. Independent headless acceptance covers the fourteen journeys
    and a held-out composition; it does not replace human playtests.

## Awkward seams and deliberate limits

- Conditions are still attached to classic rows. The GUI now filters checks
  by subject/target class, offers Yes/No and named teams, labels variable fields,
  and splits region/velocity vectors into axes. Supported subjects can still be
  absent from an input's context; menus are not a full capability type system.
  Input selection defaults the target to Self when supported, otherwise the first
  available target. Adding IF creates an unfinished UI check; the creator must
  choose a real property before Send. No neutral Exists/Alive placeholder is
  silently saved. Only guarded rows expand, with IF before the action. Raw
  unfinished values survive local rebuild/copy/return. Changing a completed
  check's subject chooses a supported check if needed.
  Input/target changes reset incompatible Target checks to Exists.
  These are reversible authoring defaults, not runtime or format commitments.
  The native team editor is available without an Add-On declaring a team setting;
  team edits still use its existing request and host authorization. Package mode
  settings can still hide the team section when that mode has no teams.
  Long guarded lists still require scrolling; AND/OR and one-action-per-row
  policy may become awkward as creations grow.
- Core conditions are a fixed small vocabulary; the delivered Add-On extends
  inputs/actions, not a generalized condition-provider API. Do not mistake this
  proof for a finished mod platform.
- Object context currently means vehicles/balls with spawn bricks. Other native
  entities, projectiles, bots as objects, and arbitrary assembly selection are
  outside the demonstrated path. Immediate Projectile reflection outputs reject
  IF because their specialized contact cache does not execute normal guarded
  rows. Delayed projectile operations use normal due-time guards and affect only
  the original projectile while it is still live; disappearance is an explained
  skip. The editor disables + IF on immediate projectile rows.
- Region dimensions are directly authored in the wrench, with a live preview.
  Building tools reveal detection outlines using the invisible-brick convention.
  The server observer and client renderer share bounds; disabled rows and the
  observer budget have distinct colors. `setRegionSize` remains a runtime action.
  Center tests, ground-height differences and overlapping sensors still deserve
  deliberate playtesting. There is no drag-to-resize gizmo.
- Mover attribution is reused, not comprehensive causality: gun hits or complex
  chains do not provide a guaranteed last-touch/assist history. It expires after
  five seconds. Soccer recipes require credited opposing-team entries: the first
  goal accepts the second team, the second accepts the first. Own goals reset
  without points. These are editable IF rows, not a built-in soccer policy.
- Team points are member score totals, not durable independent team counters.
  The current-team interpretation is simple and canonical, but team switches,
  departures and simultaneous wins may expose a need for a real team score
  primitive in MiniGames later.
- Named brick targets and transient variable names can make duplicate courses
  interact. Recipe names include a batch ID; player/game state names intentionally
  do not. This tests whether shared names feel liberating or fragile.
- Explain names teams using the observed player's MiniGame; unavailable teams
  fall back to their IDs. This is diagnostic presentation, not changed evaluation.
- Explain is a bounded 128-line trace, owner/admin authorized, started on demand;
  latest twelve trace lines are shown in a small read-only window alongside the
  saved row/region summary. It currently reads a brick-tagged chat response rather
  than introducing a new network request/trace schema. It is not an execution debugger, editable-draft
  simulator or causal audit. Region observation has a 256-brick cap; excess IDs
  are skipped with diagnostics rather than stopping the simulation. State has an
  8192-entry cap; guard count is eight. The existing scheduler retains its own
  admission, loop, timing and permission limits.
- Recipes are authored rows planted by an Examples picker using the existing
  convenience command. They are not
  complete maps or polished game modes. Existing saves/settings and unrelated
  automatic scoring can affect them; creator playtesting should include that.

## Reuse assessment

The **strong reusable candidates** are the existing scheduler and canonical
game operations, optional due-time guards, explicit absent actor context, a
shared catalog for editor/runtime, package-provided inputs/actions, and simple
bounded region observations. They add expressive power without requiring a
parallel authoring universe. Automated tests exercise order-sensitive puzzles,
per-player racing, contested occupancy, kill attribution, real object goals,
delayed checks, canonical score/round effects, Add-On chains, save/copy fields
and editor guard copying; human feel remains unverified until your playtests.

The **experimental pieces** are state namespace/lifetime, member-sum team score,
the exact subject/property list, center/sweep/stay semantics, recent-touch
attribution, context permissions, trace presentation and the recipe generator.
They should be redesigned or removed freely if playtesting finds them awkward.
The data shape and protocol increments exist to deliver this build, not to
become an obligation for the real architecture.

The NPC split has a useful reusable shape: a bounded planner predicts known
effects, normal controls attempt them, and canonical observations decide what
actually happened. Native trajectory/ammo queries and temporary seat claims
also reuse mechanisms already owned by the game. The current rule-effect
whitelist, failure cooldowns, objective utility, search/flight horizons and
capability ranking remain experimental tuning and adapters. Unknown reaction
closures expose a real awkward seam; they should be grounded before adding
more vocabulary, rather than hidden behind successful example names.

The most valuable next work is your mutation evidence. If basic rows become
harder, optional complexity needs a UI rethink. If games mostly need arithmetic,
expressions or many bookkeeping rows, test a few concrete missing primitives
before inventing a general language. If unpredictable combinations work with
short, readable event lists, that is stronger evidence than a polished demo win.
