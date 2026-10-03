# Bots acting together in a physical world

This records the implemented mechanism, its deliberate boundary and acceptance
evidence, integrated for the v0.2.0 alpha release.

## Problem

The existing brain perceives enemies, selects a scored behaviour, paths, aims,
and produces player controls. It already handles portals, crawling, swimming,
body attacks, charged weapons and carrying objects. It previously had no
representation of environmental opportunities or shared commitments.
Before this change every bot independently chose a fight; nobody could agree to
drive while somebody else operated a weapon. The foot motor's path and look also
cannot simply be sent to a vehicle: steering, braking, turret aim and seat
handover have different meanings.

## Small set of rules

**Bots know affordances, not content. Games expose objectives, not bot scripts.**
An authoritative rule or package describes the desired state; capability
providers describe grounded actions that can change it. Planning composes them,
ordinary gameplay executes them, and actual observations determine progress.
The current integration acceptance contract is
[objective-driven-integration.md](../audits/objective-driven-integration.md).
Its open items are requirements, not claims of implemented competence.

1. **Observe before deciding.** Nearby objects provide opportunities through
   their existing capabilities and geometry. Expensive geometry observations are
   shared for a tick; occupancy is read live so one bot boarding cannot leave
   another bot deciding from an obsolete empty seat. Enemy locations come from
   sight or dated memory, never a hidden player's live transform. An opportunity is not permission to act.
2. **Score opportunities against ordinary behaviour.** Keep the existing
   utility selector. An interaction is another candidate, not an override that
   takes over whenever a named vehicle exists. Packages choose the interaction
   weight. Creature kinds do not acquire vehicle skills by accident.
3. **Reserve scarce roles.** A seat or movable object has one claimant. Claims
   are intentions, not occupancy or authority. They expire when progress stops,
   and are released on preemption, rest, death, removal, loss of permission or
   target invalidation. Real players can always occupy a claimed empty seat.
4. **Execute through existing mechanisms.** Reach and permission are checked
   again when boarding; vehicle controls follow the normal seated input path;
   shots use the actual gun; pushing uses physical contact. A thought never
   becomes a teleport, a direct damage call or an invented impulse.
5. **Plan for the body being controlled.** A vehicle uses its footprint and
   ground clearance, cannot take a pedestrian jump or crawl path, and brakes
   for allies. A gunner aims independently of the driver's steering. On foot,
   jets require the body's actual jet capability and available energy.
6. **Failure changes the decision.** An inaccessible vehicle, blocked push or
   stuck chassis is a reason to abandon an action temporarily. It is not a
   reason to retry forever or to force a successful result.
7. **Coordination shares evidence.** Allies share observations with timestamps;
   a crew can use an ally's last known location to travel or search, but may
   shoot only what its own weapon can reach. Team rules and bot sides are
   resolved by one relation helper, including rules bots.

Utility scoring and reservable object slots are established game-AI techniques:
[Game AI Pro, An Introduction to Utility Theory](https://www.gameaipro.com/GameAIPro/GameAIPro_Chapter09_An_Introduction_to_Utility_Theory.pdf)
and [Epic's Smart Objects overview](https://dev.epicgames.com/documentation/en-us/unreal-engine/smart-objects-in-unreal-engine---overview).
Our implementation uses the existing fixed-tick brain and small typed
opportunities; it does not require another behaviour-tree framework, recursive
planner, script VM, learning system or simulation of hypothetical physics.

## First implementation boundary

Ground vehicle seats and physical shoving exercise the same interaction
lifecycle. Capabilities come from seat `controls`/`weapon`, vehicle geometry,
projectile data, and existing contact-hazard capabilities (`shove`, runover
harm/push or smash); names such as Tank, Jeep and Steel Ball are not
decision inputs. The standard Blockhead kind opts in through behaviour weights.
Zombie and Shark policy remains in their kinds.

Seat selection prefers completing a useful allied crew over filling arbitrary
passenger seats. A driver holds a stationary vehicle briefly while nearby allies
board, then leaves when claims complete or expire. A gunner uses the shared
aim/reaction/fire decision with its mounted weapon. An unarmed combat vehicle
driver can pursue a target for contact damage; damage settings still decide
whether impact hurts. Allies in front take precedence over pursuit.

A seatless, unheld contact hazard is a physical opportunity when a bot can get
behind it relative to a seen enemy. The bot walks to that approach and pushes along
the line through contact. Actual mass, friction, obstacles and momentum decide
the result. A blocked or too-heavy object is abandoned, not given extra force.
A Steel Ball's authored 900-unit mass, friction and impact threshold are kept:
walking can roll it, but ordinary walking on flat ground does not promise a
lethal impact. A slope, fall or existing momentum can supply the remaining
energy. Real successful contact credits the mover through the existing damage
attribution path.

Per-polygon portal mappings put contact point, normal and velocity into the
actual collider's world frame, even while the walker remains on the entrance
side. This avoids applying a far-side impulse at an entrance-side location.
Vehicle navigation itself deliberately excludes portal edges.

Aircraft, boats, scripted objectives, construction, tactical cover selection,
arbitrary tool programs and multi-step plans are future providers, not claims
made by this first implementation.

## Examples the rules should permit

These combinations describe the direction of the mechanism. Ground crews,
seat replacement, human occupancy, bounded physical pushing and dated allied
observations have headless acceptance evidence below. Strategic objectives,
general tool programs and complete ballistic or portal planning remain future
work; this table does not claim every combination is implemented.

| Shared fact or capability | Possible combinations |
|---|---|
| Free control and weapon seats | Driver plus gunner; one human and one bot; a replacement driver after a casualty; multiple competing crews |
| Ordinary passenger seats | Armed passengers covering a driver; leaving an occupied human seat alone; choosing a different vehicle when full |
| Movable hazards | Pushing a ball toward an enemy; giving up on excessive mass; choosing a new approach after the object rolls sideways |
| Existing reaching/holding tools | Carrying a prop, moving it out of a doorway, using momentum in a throw, releasing when the target or tool disappears |
| Weapon flight and state | Holding a spear until ready; instant shots without ballistic lead; backing away from splash; ceasing fire when a teammate obstructs the shot |
| Real body capabilities | A jet bot crossing a height gap; a No Jet bot seeking a walk; a large vehicle avoiding a small door; a swimmer keeping to water |
| Changing geometry | Replanning after a door closes; reconsidering a route after bricks are removed; abandoning a portal that disappears |
| Dated allied observations | A driver travelling toward a gunner's sighting; searching after contact is lost; abandoning a known dead enemy |
| Authority and lifecycle | A player taking a reserved seat; rules resting a crew; permissions changing while boarding; a destroyed vehicle returning its bots to walking |

The extensibility test is an unfamiliar vehicle with its seat indices reordered,
different dimensions and a different weapon. Correct roles must follow those
properties without adding an id check or new behaviour for that vehicle.

## Acceptance and evidence

Checks exercise the authoritative session using invented content, including
unfamiliar geometry and reordered seat flags. They observe autonomous decisions
and actual movement/projectiles/damage, rather than assigning bots a task or
forcing a successful outcome. Commands and results are recorded in
[the progress entry](../progress/2026-10-02-bot-world-coordination.md).
Interactive handling and tactical feel remain Maxwell's playtest boundary.

- Exclusive claims, deterministic arbitration, progress renewal and expiry.
- Claims invalidated by a human occupant, death, rest, changed side, lost
  permission, deleted object, lost target and behaviour preemption.
- Boarding checks actual position, body capability, minigame/trust rules and
  the existing Add-On ride hook. No remote mounting or boarding through walls.
- Different seat layouts and vehicle ids produce driver/gunner roles from
  capabilities; drivers and gunners do not exchange their control frames.
- Steering, weapon fire, impact damage and pushing execute through the same
  mechanisms as human actions and retain correct attribution.
- A crew waits a bounded time; a lone driver and an unreachable teammate do
  not deadlock. A blocked vehicle stops or relinquishes its role.
- Driver navigation respects width, height and ground movement; it never
  treats a crawlspace or jumping ledge as a vehicle path.
- Sight, fire and remembered positions remain distinct; crews do not obtain
  omniscient targets or continue firing at a teammate after a team change.
- Multiple bots/vehicles retain bounded work and deterministic results.
- Existing bot, vehicle, weapon, permissions, rest/respawn and portal tests
  continue to pass. Interactive feel remains Maxwell's playtest boundary.

## Work and lifetime bounds

- At most 16 active claims and 64 temporary failed-resource entries. Each bot
  has one claim. Arbitration rotates deterministic bot order each tick.
- A claim lasts 360 ticks (3 s), renewed by at least .25 m of approach progress
  or recent physical contact plus .25 m of object displacement toward the
  remembered target. Attempts and impulses against a blocked body do not count.
  Total age is capped at 1800 ticks (15 s); failure waits 240 ticks (2 s) before
  retrying that resource. Alternatives remain eligible.
- Discovery visits at most 8 objects per bot with a rotating cursor, and all
  bots share at most 32 object visibility queries per tick. Existing navigation
  retains its counted sampling and expansion budgets. Steering considers at
  most 24 nearby path points, plus an actual hull stopping sweep.
- Crew assembly waits at most 360 ticks. A driver without useful displacement
  replans and leaves after 720 stalled ticks. A passenger needing travel with
  no driver waits at most 360 ticks before trying an ordinary dismount; a gunner
  that can still fight in range remains useful. Dismounts retain the existing
  authored exit search and v20 fallback, without a bot-specific exit shortcut.
- Brain thoughts are read-only diagnostics (`Session::bot_thoughts`), not saved
  state, network input or a way to command a bot's decision. Claims, observed
  objects and navigation caches belong to the session and reset with it.

## Adding another provider

Keep the scarce resource and action small: derive eligibility and utility from
existing authored capabilities, identify the resource stably, acquire a bounded
claim, and execute through the authoritative mechanism. Revalidate permission,
physical reach and occupancy at action time. Give progress and failure physical
meaning, with diagnostics and edge-case tests. Only add a new typed resource or
adapter when a real mechanism requires it; avoid a registry or multi-step
planner without a second concrete use.

This branch does not implement aircraft/boat piloting, strategic objectives,
cover tactics, construction or predictive multi-body physics planning. Straight
shot/collision safety checks are local; they are not a full ballistic or
portal-aware friendly-fire planner. Further providers should extend this
lifecycle and evidence model instead of naming special objects in the brain.
