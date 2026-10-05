# Bot brains

A bot (a Blockhead Bot from a spawn brick, or a rules bot a mini-game
adds) is a player without a connection. Its brain, in
`crates/sim/src/session/bots.rs`, presses the same controls a person does:
movement, aim, jump, jets and the trigger. Everything a player can do, a
bot does through the same code.

## One tick

1. **Perceive.** It looks for an enemy (`bot_sight`: in view, straight or
   through a portal) and remembers where one was seen, where a hit came
   from, and an enemy that went through a portal. Memory fades after the
   kind's `memory_seconds`. Evidence stores the enemy identity, observed
   position, observation tick and expiry. Hidden movement never refreshes it;
   death, removal or a changed alliance invalidates hostility. A kind with
   `alerts_allies` (Bot_Hole's `hAlertOtherBots`) shares sightings and hits with
   nearby allies (`hear_alerts`, after every bot has stepped). A receiver keeps
   the observation's original age and accepts newer evidence, without extending
   its lifetime. Friendly-fire permission does not make teammates enemies.
2. **Choose a behaviour.** `behaviour::choose` scores each behaviour
   from a `Situation` (what it holds, the enemy's distance and height, its
   memory, how far it strayed), scales each score by the kind's
   `behaviours` weight and follows the highest. The base scores keep this
   order (ties go to the earlier):

   | Behaviour | When | Does |
   |---|---|---|
   | Carry | its tool holds something | carries it to open space, swings and lets go |
   | Interact | a useful, permitted environmental opportunity | reserves a seat or loose hazard, approaches and executes through ordinary controls |
   | Fight | an enemy in sight within its weapon's band (level with it, or, for a ranged weapon, up to its band above) | stands, strafes, backs off when too close |
   | Chase | an enemy in sight out of its band | paths to them |
   | Search | an enemy remembered | goes where they were, looks around |
   | Return | strayed from its brick | walks home |
   | Objective | a supported, worthwhile rule or declared package goal | plans bounded steps through ordinary brick, contact/hold/drive, combat or pickup/return controls |
   | Wander | otherwise | strolls near its brick (a rules bot near itself) |

   A weight of 0 turns a behaviour off (`"chase": 0` makes a guard that
   stays at its post and fights what comes within reach); more than 1 puts
   it ahead of others. Ordinary behaviours default to 1; `interact` defaults
   to 0 and the standard Blockhead package opts in. Objective utility also
   requires its explicit kind opt-in; it does not grant a creature a new brain. Creature policy remains
   in its package. Nothing scoring: it wanders.

   Leeway stops flip-flopping: a fighting bot gives chase only one unit
   past its band, and a walk home goes all the way.
3. **Goal and path.** The behaviour sets the goal; the route planner (see
   [Routes](#routes-one-planner-for-every-way-of-getting-about)) over the
   walk grid (`crate::nav`, shared by every bot of a body size, updated as
   bricks change, portals included) finds the way a little each tick,
   walking, jumping, swimming or jetting wherever that is the cheapest. Gaps only
   a crouched body fits (crawlspaces) are on the grid: crawling costs
   more than walking, and the bot crouches into them. A wheeled driver's body
   uses the chassis footprint and clearance, with no pedestrian jump, crawl or
   portal edges. Local execution checks the actual oriented hull and brakes for
   allied bodies. A nearby final interaction approach continues beyond the
   grid cell to the authored point; physical reach still decides success.
4. **Aim and trigger.** The aim follows the enemy whatever the behaviour
   (leading shots by the projectile's speed and drop, with the kind's aim
   error shrinking as it tracks), the path when there is none, and Carry
   steers its own. It fires once its reaction time has passed and the aim
   is on, its weapon can reach, and allies do not obstruct the shot. Mounted
   weapons use their actual projectile, speed, charge state and muzzle. Lost
   targets cancel a held charge without firing it. A driver steers the
   chassis independently of the gunner's world aim; an armed passenger's world
   aim is preserved when its seat converts the look to a relative angle.
5. **Move and act.** The behaviour's movement, then getting unstuck (hop,
   plan again, give up the goal) on foot; drivers instead brake, replan and
   relinquish an unproductive seat. Shared claims are exclusive advisory
   intentions, released on success, preemption or invalidation. A claim never
   overrides human occupancy, trust, mini-game policy or an Add-On ride hook.
   Physical pushing transfers only momentum stopped at actual motor contacts,
   through the same mechanism for humans and bots; mass and geometry determine
   the outcome. See [bot-interactions.md](bot-interactions.md) for the lifecycle,
   budgets, provider boundary and examples.

## Routes: one planner for every way of getting about

A behaviour only says where it wants to be (its goal). How the bot gets
there is one question with one answer: the route planner
(`crate::nav::Search` over the walk grid, `crate::route` for the costs and
the per-leg controls). There is no flying behaviour, no swimming shortcut
for walkers and no driving special case in the arbiter; there are legs.

**The graph.** Nodes are the walk grid's cells (a floor and its height).
Edges are the ways the bot's body can actually move between them now:

| Edge | Exists when | Costs (seconds of travel, in walking units) |
|---|---|---|
| walk, step | the motor steps it (`Body::step`) | distance |
| jump | a ledge within the body's jump apex | distance + a jump |
| crawl | only a crouched body fits | distance + crawling |
| portal | the body's middle goes in through a linked brick's opening | one cell |
| swim | the floor lies under liquid that would float the body (`swim_coverage`) | distance x walk speed / swim speed + entry |
| jet | the body can jet (`can_jet`, energy, the kind's `fly` weight above 0), the column up from the launch cell, the crossing at the apex and the descent are clear, and the climb is within the energy | the flight time from the jet's thrust, lift and gravity, plus takeoff |
| board, drive, leave | a free, permitted wheeled vehicle in sight whose drive beats the walk (`route::drive_serves`), or an armed one; never while the bot has a grounded objective of its own (its objective plan decides what it drives) | walk to the seat + boarding + chassis distance / cruise speed |

Edge costs come from the body's and vehicle's own numbers (`PlayerTuning`
speeds, jet acceleration and lift, gravity, energy drain; a vehicle's
`max_speed`, `max_steering`, wheelbase and `brake_force`), never from
content names. Kinds keep one data knob per mode: the `fly` weight in
`behaviours` (0: never takes a jet leg) scales how willing a kind is to
jet, and `interact` (0: never) whether it may take a vehicle.

**The plan.** A search returns one list of waypoints, each tagged with the
leg it belongs to (`nav::Mode`: walk, swim or jet). A vehicle worth taking
is a seat opportunity costed the same way (walk to it, board, drive), and
once seated the same search plans the chassis's path. Behaviours ask for a
goal and get back that plan.
The search stays bounded: the same per-tick sample and expansion budgets,
at most a few jet tests per search, one landing sample per goal.

**Execution.** Each leg turns into ordinary controls, the same keys a
person presses:

- walk: as before (step, jump, crouch into crawlspaces, walk through
  openings).
- swim: head for the next waypoint across the water, whatever the depth,
  holding jump to rise where the way out is higher.
- jet: climb straight up at the launch cell until above the landing's
  height (jets lift hardest with no move), cross at that height (jetting
  again whenever it sinks to the lip), cut the jets over the landing and
  brake onto it. A takeoff under a roof cannot happen: the planner only
  launches where the column up is clear, so a bot under a platform walks
  out from under it first.
- drive (`route::gear`, `route::pace`): pure pursuit along the chassis
  path. The chassis's tightest turn is its wheelbase over the tangents of
  its front and rear lock (`route::Chassis`). A point deeper than half the
  chassis's width inside either turning circle is not chased round in
  circles: the driver backs away from it (nose swinging toward it) or
  pulls ahead of it, until it is out of the circle. A point behind is
  backed onto when that is sooner, at the definition's cruise speeds, than
  turning round. Speed is no more than the tyres hold on the arc pure
  pursuit takes, manoeuvring speed while backing or pulling out, and what
  it can still brake from by the point. A chassis is at a waypoint, or a
  search probe, within half its footprint.
- board: the ordinary seat approach, claim and mount. The seat claim's
  progress is measured afresh once the bot is seated (the drive's own
  distance), so a long drive is not judged by the walk to the seat.
- leave: a chassis that cannot hurt the enemy it chases (no gun, and no
  runover for someone not on foot) stops and gets out once its side is
  about as close as the bot fights from on foot. A wreck, or a wheeled
  hull on its side or roof, ends the drive at once.

**Replanning from outcomes.** Each leg watches what really happened: a walk
leg that stops moving hops, then plans again (as before); a jet leg that
runs out of time or lands below where it took off plans again from where
the bot came down, and the cells that failed are forgotten; a drive leg
that makes no headway backs up, plans again and finally gives up the seat.
A plan is never trusted past what the world shows. When the best route to
an enemy ends where the bot cannot hurt them (farther across than its
band, or higher than a jump brings within it), chasing them, or searching
where they stand, is worth nothing until they move. A bot standing on
something the grid leaves out (a vehicle's roof) plans from the floor
beneath.

Portals stay inside the same mechanism: an opening is a walk edge of the
grid, so a route that crosses one is a walk leg like any other. Shark-like
kinds (`moves: swim`) keep their own water roaming; their staying in water
is package policy.

## Bounded rule and weapon adapters

Objective discovery reads supported creator rules and opt-in typed package
queries. The bounded planner composes grounded brick inputs, exact spawned-object
contact/declared hold/ground-seat delivery, native elimination and declared
pickup/return actions. They share selection, interruption and live validation;
each executor requests normal controls. The existing event scheduler and native
package callbacks alone apply their gameplay effects. Real input admission,
physical state and canonical death/round observations decide what happened;
a package completion counter does not claim a round winner.

Grounding, search and depth remain finite. Unknown script semantics, thrown-object
trajectories, hookshot routes, cooperative stacking and aircraft/watercraft
delivery remain unsupported. Search uses dated enemy evidence, not unseen live
positions. Source, object incarnation, tool, permission and game/round/team
changes invalidate assumptions. Explain exposes desired state, selected
action/provider, phase, proposed route and bounded failure diagnostics.
An interrupted approach excludes time spent in another behavior, while an event
that was already scheduled keeps its absolute due time. Failed search reuse
revalidates the authoritative model, game, round and team before accepting it.
See [the current pipeline and verification limits](../audits/npc-pipeline-current.md),
[the frozen acceptance contract](../audits/objective-driven-integration.md) and
[the v0.2.2 evidence](../progress/2026-10-03-sol-npc-hardening.md).
The [initial objective spike](../audits/bot-objective-spike.md) is historical.

Supported native hand weapons supply mechanical attack descriptors for bounded
ballistic choice and launch-time safety checks. Unsupported script/mounted
mechanics retain their narrower existing executors. At the final attack boundary,
canonical spawn protection withholds ammunition-spending attacks and charge
release, retaining aim and proven non-firing holds. It does not make prediction
of a moving target infallible or change damage permissions.

## Data

- `bots.json` (a kind): sight, wander and chase radii, reaction, turn
  rate, aim error, memory, whether it fights other builders' bots,
  whether it warns its side (`alerts_allies`), its `behaviours` weights,
  and:
  - `body`: the archetype it plays in (an Add-On's player type: its model,
    speeds and health). It keeps it through respawns and mini-games, which
    otherwise give their own player type. A body no enabled Add-On has is
    an error when Add-Ons load, and a bot that cannot take it does not
    spawn (its builder is told).
  - `melee` (`damage`, `reach`, `seconds`, `action`, `name`): with empty
    hands it fights with its body, from the band its reach gives, hitting
    once every `seconds` and playing `action` on the arms' thread. Damage
    goes through the same rules as a shot.
  - `moves`: `walk`, or `swim`. A swimmer in water skips the walk grid and
    heads straight for its goal at any depth (jump rises, crouch dives),
    roams up and down as well as across, and every goal is kept inside its
    water (`water::swim_point`), so an enemy on land brings it to the edge
    nearest them and no farther. Out of water it walks like any bot.
- A hole brick (a brick catalog entry's `bot`, Bot_Hole's `isBotHole` and
  `holeBot`) keeps one bot of that kind from the moment it is planted, as
  a spawn brick keeps the one chosen in its wrench. Import Add-On turns a
  Bot_Hole bot (`PlayerData` with `isHoleBot`) into its body (an archetype)
  and a kind read from its `h` settings: `hName`, `hType` (side),
  `hSearchRadius` (sight, Bot_Hole's `brickToRadius`), `hSpawnDist`
  (wander, `brickToMetric`) and `hMelee`/`hAttackDamage` (a swipe once a
  second playing `activate2`). A port adds what its scripts did (the
  Zombie's paint, arms out and turning bots; the Shark's swimming, bite
  and death on land). A bot's body may be drawn with its Add-On's own
  model: Import Add-On sets an archetype's `model` to the package's
  converted `shapeFile` (`bot_shark:asset/shark.dts`), and the client draws
  it in place of the Blockhead, painting objects named as colour slots
  (`chest` the torso's), showing the selected named avatar accessories,
  and hiding unselected objects
  (`AvatarAssets::load_bodies`, `body_mesh`).
- A weapon image's `bot` (`BotUse`): `fire` `tap` (pressed again and
  again, for semi-automatics) or `hold` (held on target: a tool that
  reaches and holds), `reach` when its projectile does not say, and
  `near`, the closest it is used from (the Gravity Gun grabs from 2.5
  away; without, a bot keeps clear of the splash). A charged image (a
  state whose letting go fires, `Image::charges`: the Spear) needs no
  data: the bot holds its trigger while it charges and lets go once
  letting go fires. The
  band it fights from follows the reach. Without `bot`, reach comes from
  the projectile (close range without one) and the trigger is tapped; a
  tool reaching or holding right now is held anyway.

## Adding a behaviour

Add a variant to `Behaviour` at its place in the order, its name to
`bot_kind::BEHAVIOURS` at the same place and its score to
`Behaviour::score`, with a test there; its goal goes in `step_bot`'s goal match and its
movement in the movement match. Its numbers belong in the kind
(`bots.json`) or the weapon's `bot` when they differ between kinds or
weapons. Keep perception and the walk grid shared.
