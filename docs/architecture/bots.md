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
2. **Choose a behaviour.** `behaviour::scores` scores each behaviour
   from a `Situation` (what it holds, the enemy's distance and height, its
   memory, how far it strayed, an item worth arming with), scales each
   score by the kind's `behaviours` weight, and the hold rule picks among
   them (below). The base scores keep this
   order (ties go to the earlier):

   | Behaviour | When | Does |
   |---|---|---|
   | Carry | its tool holds something | carries it to open space, swings and lets go |
   | Interact | a useful, permitted environmental opportunity | reserves a seat or loose hazard, approaches and executes through ordinary controls |
   | Fight | an enemy in sight within its weapon's band (level with it, or, for a ranged weapon, up to its band above) | stands, strafes (melee: steps in, out and aside), backs off when too close |
   | Arm | no attack and a weapon in sight, or a better weapon in sight and a free slot | walks to it and picks it up |
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

   **Holding a choice.** One rule keeps every choice from flip-flopping,
   at every strength (`behaviour::Hold`, fixed in code): a choice is
   kept for 0.5 seconds, and after that a challenger must beat
   it by more than 10%. The scores themselves are
   continuous, with no thresholds to sit on: a fight fades out past its
   band's far edge (over half the band, at least 4.5 units) and with
   height, and a walk home grows with how far past its stroll the bot is.
   An interrupt bypasses the hold at once: urgent damage, an objective
   offered or gone, picked up or dropped, an enemy coming into sight, lost
   or dead, a must-do option winning (a catch in its tool, arming an empty
   hand), or the held option scoring nothing. An objective between two of
   its steps (a step done, the next not yet planned) is held for the hold
   time after it last scored. While a failed objective step cools down
   before it is tried again, the bot does not walk home. The same rule
   holds weapons, aims and chase routes (below), and a weapon in hand is
   kept, its fire held, through a spell shorter than the hold time in
   which it cannot attack (the target inside its blast, an ally across the
   line, a reload) rather than swapped out and back. This replaced the
   band slack, the fight-to-chase dwell, the walk-home-all-the-way rule,
   the objective's one-second step grace and the weapon switch margin.

   An objective outranks a fight while the bot carries what the objective
   delivers (`View::committed`, read from the step: a package carriage it
   holds, or a held body). On the way to a delivery that needs only its
   feet it shoots an enemy ahead or to the side as it goes (run and gun);
   an enemy behind is left, as walking backwards is slow.

   An on-foot bot's leash is its brick and `chase_radius`; a rules bot on
   foot has none and plays the whole map. A driver's leash is measured from
   where it took the controls, 96 units long. So a bot that boards a vehicle 40 m out keeps
   pursuing a target 60 m out rather than turning back at its walking leash.
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
   is on, its weapon can reach, and no body the plan did not price has
   stepped into what the shot sweeps (`harm::Shape`). Mounted
   weapons use their actual projectile, speed, charge state and muzzle. Lost
   targets cancel a held charge without firing it. A driver steers the
   chassis independently of the gunner's world aim; an armed passenger's world
   aim is preserved when its seat converts the look to a relative angle.
   How a chassis reverses is one rule, the drive leg's `route::gear`
   (Routes, below): a goal more than 103 degrees off its hull
   is behind, and while pursuing (Fight, Chase, Search) it is backed onto
   only within 16 units; a farther target behind is
   turned toward, so a chase is not driven as a long retreat.
5. **Move and act.** How the bot moves this tick is decided in one place,
   the act stage (`bots/act.rs`). Every mover *proposes* a walk and buttons
   (`act::Proposal`) and none writes the controls. The movers, lowest
   priority first (`act::Mover`):

   | Mover | Proposes |
   |---|---|
   | Route | along the planned route, or through the opening it leads through; its jump and crawl |
   | Push | into the body an interaction works |
   | Stance | where it fights or works: a weave, melee footwork, a ranged strafe round the place it fights from |
   | Jet | a jet leg flying itself (its jets, jump and no crouch) |
   | Swim | rising and diving in water, or rising afloat |
   | Goof | a goof's walk |
   | Dodge | a dodge's step aside |
   | Stand | standing still on purpose (a hop straight up, a hand-off) |

   `act::resolve` takes the walk of the highest mover that has one, sets
   each button in that order, and adds presses (a goof's or dodge's hop,
   a crouch, a jet) on top. Then safety, which only takes away
   (`Session::bot_safe_walk`): no step into a portal off the route, round
   a vehicle in the way, and never off an edge whose fall would hurt (see
   Routes). A new mover goes in at its place in that order; nothing runs
   "after" another to win. The look is decided the same way: each that
   wants the bot's eyes proposes an `act::Look`, and the highest
   `act::Looker` has them (hold, sweep, route, glance, target, objective,
   carry, down, gesture). The trigger has one owner a tick, in `bot_act`:
   a goof's, else an objective tool's, else the fight's. What the stage did
   (who walked, who looked, who has the trigger, an edge held back, stalls
   and replans) is `act::Acted`, the F3 readout's "Acts" line.

   Then getting unstuck (hop, judged before the stage resolves and pressed
   like any other hop, then plan again, give up the goal) on foot. A bot
   that keeps trying to get somewhere and stays within 2 units of where it
   began is trapped (bricks respawned round it): past 10 s its Respawn
   option rises, and once sure it gives the command a player gives
   (Ctrl+K). Respawn scores 0 with an enemy in sight, when hurt, or while
   carrying, so it is never a way out of a fight; drivers instead brake, replan and
   relinquish an unproductive seat. Shared claims are exclusive advisory
   intentions, released on success, preemption or invalidation. A claim never
   overrides human occupancy, trust, mini-game policy or an Add-On ride hook.
   Claims on a loose body (a ball, a crate) conflict only between allies:
   an opponent may pursue the same body and push it toward its own goal.
   Seats stay exclusive for everyone. That contest is its own piece
   (`bots/contest.rs`): when an opponent is the body's mover or holds a live
   claim on it, the approach leads the body along its velocity (up to
   0.6 s of it, at most 4 units), and being within 4 units of it counts as progress, so the claim's lease does not
   lapse while the two sides fight over it. A step waiting on an admitted
   delivery holds no lease at all.

   The contest piece also covers the other cases of a shared body:
   - **Cover.** A bot whose claim is refused because a teammate holds the
     body keeps its objective. It does not stand down into Wander: it holds
     a cover point 6 units behind the body, against the
     team's delivery heading, and 3 to the side. A lone
     cover takes the side the working teammate is not on. Several covers
     take id-ordered slots on both sides, each further pair further back and
     wider. A cover point stops short of any wall between it and the body.
     When an opponent drives the body back at the cover, the cover stands
     beside the body's path instead, so the drive is not deflected off it.
     When the claim frees, the cover takes the body up at once.
   - **Clear.** A push or hammer on a body an opponent drives straight back
     at it turns by 60 degrees to the bot's side, knocking the
     body out of its line rather than being carried back with it.
   - **Walls.** When solid world geometry stands where a pusher would stand
     (a wall or a corner behind the body), the push turns by the smallest of
     30, 60, 90 or 120 degrees that leaves room. The body is worked along
     and off the wall instead of being pressed into it.
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
| jump | a ledge no higher than the body was measured to jump onto (`reach::Reach::ledge`) | distance + a jump |
| crawl | only a crouched body fits; up into one (a window up a wall), no higher than the body was measured to jump into one, crouching once off the ground (`reach::Reach::crawl_ledge`) | distance + crawling (+ a jump) |
| portal | the body's middle goes in through a linked brick's opening | one cell |
| swim | the floor lies under liquid that would float the body (`swim_coverage`) | distance x walk speed / swim speed + entry |
| jet | the body's jets lift it (measured), the kind's `fly` weight is above 0, the full-width column up from the launch cell, the crossing at the apex and the descent are clear, and its energy holds the leg's measured jetting | the leg's measured flight time (`reach::JetReach`), plus takeoff |
| board, drive, leave | a free, permitted wheeled vehicle in sight whose drive beats the walk (`route::drive_serves`; one that runs over an enemy on foot the rules let the bot hurt is costed at its top speed, since the drive is the blow), or an armed one; never while the bot has a grounded objective of its own (its objective plan decides what it drives) | walk to the seat + boarding + chassis distance / cruise speed |

**Measured reach** (`crate::reach`). What a body can jump onto and how
long a jet leg takes, and how high a crawlspace it can jump into, are
not worked out from a formula: they are measured
once per tuning by running the real player motor on a bare test floor,
under the same controls a bot uses (walking at a ledge and jumping as the
walk leg does; flying a jet leg with `route::jet`), and shared by every
body with that tuning. Jet legs are flown over a grid of climbs and
crossings until one more step adds the same time as the last, and
interpolated between; past the grid they take longer at the last rate.
Gravity, jet strength, jump speed, energy and body size all change the
result with no bot support of their own. The measurement sees no map:
ceilings and crowds stay the planner's geometry checks, and a leg that
goes wrong all the same is given up by what really happens
(`JetLeg::failed`).

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

- walk: step, jump, crouch into crawlspaces (a jump up into one crouches
  once off the ground, as it was measured), walk through openings. The
  grid's eight-way steps are pulled straight (`nav::pull`): each plain
  walking waypoint heads for the farthest later one of the same walk that
  the full-width standing body walks straight to (`Ground::walkable`: its
  box sweeps clear a step up, of the map and of loose bodies such as a
  ball or a parked vehicle (players step aside), and floor it can stand on, not deep water,
  lies within a step all the way along). A jump, crawl, opening or other
  leg's waypoint is never skipped, nor the one where it starts, so a
  diagonal across open floor is one line and a corner the body would
  catch on is still walked round. Walking eases off only at the leg's end.
  A walker does not walk into a vehicle (which the grid leaves out): one
  ahead is walked round by its nearer side, unless it is what the bot
  makes for. Nor is a pulled line drawn past another player or where one
  is heading over the next second (`Ground::crowds`, `MOTION_AHEAD`); a
  player in the way is passed on the left, or on the right when already
  off to the left. A bot standing on a body (a roof, a head) with its
  target close below steps off to the nearest open floor
  (`bot_step_off`). An airborne fighter whose arc (`route::landing`)
  would come down off the floor steers back instead of strafing on, and
  a place to fight from is only one the walk grid has floor for.
- swim: head for the next waypoint across the water, whatever the depth,
  holding jump to rise where the way out is higher.
- jet: walk back onto the launch cell and stand still there (a body that
  lifts off running drifts on under whatever it climbs beside), climb
  straight up (jets lift hardest with no move, and let go once it will
  coast the rest of the way to the crossing height), cross at that height
  (climbing again whenever it sinks below it), cut the jets over the
  landing and brake onto it. A takeoff under a roof or an edge cannot
  happen: the planner only launches where the column up is clear for the
  full-width body, so a bot under a platform walks out from under it
  first. A leg taking twice its measured time is given up.
- drive (`route::gear`, `route::pace`): pure pursuit along the chassis
  path. The chassis's tightest turn is its wheelbase over the tangents of
  its front and rear lock (`route::Chassis`). A point deeper than half the
  chassis's width inside either turning circle is not chased round in
  circles: the driver backs away from it (nose swinging toward it) or
  pulls ahead of it, until it is out of the circle. A point behind (past
  103 degrees) is backed onto when that is
  sooner, at the definition's cruise speeds, than turning round, and, for
  a pursued target, only within 16 units. Speed is no more than the tyres hold on the arc pure
  pursuit takes, manoeuvring speed while backing or pulling out, and what
  it can still brake from by the point. A chassis is at a waypoint, or a
  search probe, within half its footprint.
- board: the ordinary seat approach, claim and mount. The seat claim's
  progress is measured afresh once the bot is seated (the drive's own
  distance), so a long drive is not judged by the walk to the seat.
- leave: a chassis that cannot hurt the enemy it chases (no gun, and no
  runover for someone not on foot) pulls up short of them: it brakes so
  as to stop with its nose about as far off as the bot fights from on
  foot (half its length plus that reach), and never rams them. There it
  gets out, if on foot it would still close on them (`route::walk_closes`:
  they draw away slower than it walks); one outrunning a walker is
  followed in the seat. A wreck, or a wheeled hull on its side or roof,
  ends the drive at once.

**Replanning from outcomes.** Each leg watches what really happened: a walk
leg that makes no headway hops, then plans again, then gives up as
before. Headway is net displacement over a window (`route::Progress`):
over three quarters of a second a walker must cover a fifth of what its
walk speed would carry it, so one shuttling between two spots, or hopping
in place, while its input says move, is as stuck as one standing against
a wall. It is the only stuck check on foot or swimming: the first window
gone nowhere hops, each later one plans again, and a waypoint within a
step that a window got nowhere toward counts as reached (a door jamb). A
bot at work beside its objective, a seat or an arming point is
not judged by it; a jet leg that
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

## Noticing

`bots/perception.rs` adds a small "what a bot notices" layer on top of
sight, and `bots/sightlines.rs` the one sight-ray budget every bot sight
query goes through. A kind sets only `perception.strength`; the other six numbers
(`salience`, `glance_seconds`, `cooldown_seconds`, `relaxed_scale`,
`away_scale`, `view_degrees`) are fixed in code. Everything
else is a documented constant in `perception.rs` or comes from engine
data (blast radius, sound volume, the kind's `sight`, `reaction_seconds`,
`aim_error_degrees` and `turn_degrees`, the body's running speed).
`strength` (0 to 4, 1 shipped) is the one dial (the tuning tools' sweep
reads it): it scales the reaction delay, the starting aim error, the
view-cone delay and turn cap, the warning delay, the turn's overshoot, the
idle drift and a steady aim error that never settles, together. At 0 the
bot reacts in exactly `reaction_seconds` with the plain narrowing error,
the plain linear turn and the old fire gate.

- **Glances.** A strolling or homeward bot (Wander, Return; only a
  strolling one looks round at someone merely near) with no enemy in sight, no objective at hand, nothing held, no seat and no chassis may
  turn its ordinary aim at something salient for about `glance_seconds`,
  then waits `cooldown_seconds`. Sources are engine data only: a
  projectile blast (the weapons runtime's `Blast` event), noticed out to
  10 units per unit of its radius; a weapon sound, out to 12 units at full
  `volume`; a stare (someone in plain view looking within 8 degrees of it
  for 1.5 seconds) or a body faster than twice its own running speed,
  within 0.3 of its `sight`; and, less likely, anyone of any team close by
  in plain view. `salience` scales every reach (0 turns glances off).
  Salience falls from 1 at the source to 0 at its reach and is the chance
  of a glance. The look-round for watchers, nearby players and fast bodies
  runs on the bot's own `cadence` beat. The walk goes on; only the look
  turns. Strolling, the look also drifts a few degrees off the way.
- **Reaction.** A newly seen target, or an attacker it was not already
  fighting, starts a reaction: `reaction_seconds` as is when it was
  fighting or hunting (Fight, Chase, Search), times `relaxed_scale`
  when strolling or playing about (Wander, Interact), times `away_scale`
  for a target outside its `view_degrees` cone, varied by 30% either way
  from its seeded RNG. It fires only after it, and turns `away_scale`
  times slower toward a target from outside its cone until then. The same
  scale multiplies its starting aim error, which narrows over the usual
  two seconds of tracking. On top, however long it tracks, its aim trails
  a target moving across its line of sight (its own motion included) by up
  to 0.5 seconds of that motion times `strength` (at most 0.3 rad either
  way, so a shot never leaves far off a close strafer), so a strafing
  target is missed by about the same distance at any range and a still one
  is hit as before. The native fire gate judges a shot by where the bot believes it
  aims (its look without its error), so the error misses for real instead
  of holding the shot back. What the actual shot would do to its own side,
  error and all, is judged again there (`harm::shot_harm`, below). With the fair metric the Blockhead's steady hit
  rate falls as `strength` rises and sits inside the 15-60% band at 1.
  A spawn-protected target is watched but not
  reacted to: the clock starts when it can be hurt. Damage still
  interrupts at once (the chooser sees it as before); only the return fire
  waits. `perception::delay_ticks` and `Brain::switch_delay` give the same
  delay to any other pause before acting on a change (a chooser's tell).
- **Hurt from out of sight.** A hit from someone it cannot see gives the
  way the hit came from to within 25 degrees and the distance to within
  40%, never nearer than a unit to the truth; its look holds until its
  reaction, then turns there. Its warning to allies carries that guess.
  The exact spot comes only from seeing them; a hit from someone in sight
  is placed exactly.
- **Warnings.** An ally's warning is acted on a seeded 0.25 to 1 second
  later (per ally and warning, scaled like a reaction), at a spot up to
  1.5 units off, so allies do not all turn on the same tick.
- **Turning.** The head speeds into a big turn and eases out of it (top
  speed 1.3 times `turn_degrees`, the acceleration set so a half turn takes
  about the plain time), a fast flick overshoots a few degrees and settles
  back within a degree. Handling things (Carry, Objective, Interact) keeps
  the plain turn its controllers are built on, and a startle does not stop
  it.
- **Sight budget.** `Session::bot_sees` and `Session::bot_sees_player`
  answer every bot sight query from one budget per tick: 128 rays shared
  by ordinary queries (scans for enemies, watcher polls, arming, team and
  surprise checks) and 4 per bot (sized for 32) for checking its current
  target or attacker, which therefore always run. An ordinary answer is
  cached per (viewer, subject) for 6 ticks while neither end moves half a
  unit; with the ordinary share spent, the last answer stands in. A player
  is seen at the eye or else at the chest. Vehicles with seats hide what is
  behind them, except the viewer's and the subject's own mounts and a body
  the viewer is pushing; seatless bodies (a ball, a crate) are looked
  past (they hid soccer opponents and pushed the 2v2 hammer match's
  wrong-way share over its bar).

`BotThought::noticed` reads out the last glance or reaction (`glance:
blast`, `glance: watched`, `glance: someone near`, `reacting: relaxed,
from behind`, ...) with when it started and ends.

## Bounded rule and weapon adapters

Objective discovery reads supported creator rules and opt-in typed package
queries. The bounded planner composes grounded brick inputs, exact spawned-object
contact/declared hold/ground-seat delivery, native elimination and declared
pickup/return actions. They share selection, interruption and live validation;
each executor requests normal controls. The existing event scheduler and native
package callbacks alone apply their gameplay effects. Real input admission,
physical state and canonical death/round observations decide what happened;
a package completion counter does not claim a round winner.

Rule outputs are projected by what they do, never by content names. A
Team Score condition reads a fact, the team's total: the sum of its members'
scores, which `addScore` and `addTeamScore` raise. A delayed `resetObject`
on the captured Object, from a brick whose owner owns that object's
spawner, is a known effect after the scoring group: the object is replaced
by a new incarnation. A step that expects it does not fail when the object
disappears, and the bot plans again at once for the replacement. Any other
output the planner does not understand makes the plan unsupported rather
than being skipped.

Grounding, search and depth remain finite. Unknown script semantics, thrown-object
trajectories, hookshot routes, cooperative stacking and aircraft/watercraft
delivery remain unsupported. Search uses dated enemy evidence, not unseen live
positions. Source, object incarnation, tool, permission and game/round/team
changes invalidate assumptions. Explain exposes desired state, selected
action/provider, phase, proposed route and bounded failure diagnostics.
An interrupted approach excludes time spent in another behavior, while an event
that was already scheduled keeps its absolute due time. A step's 30 s approach
deadline moves on each time the bot gets half a unit closer to the step's
point, so only a stalled approach times out, however long the route.
Between steps the bot keeps the finished step's view (standing where it
finished, no trigger) for up to a second until the next is planned, rather
than blinking to Wander. A completed objective looks for the next one 12
ticks later (once the completing event's effects have landed), and a
desired state that cannot be planned hands the bot's next planning turn to
another offered one at once instead of after a retry's wait. Failed search reuse
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

## Surprise

Bots that always do the single best thing are predictable. Surprise is
variation among a bot's choices that changes over time, built from plain
mechanisms over the choices the brain already scores: no personality,
emotion or mood, and no item, vehicle or game names
(`crates/sim/src/session/bots/surprise.rs`).

**Score terms, one hold rule.** Each choice point scores its options and
hands them to `Mind::pick`, which adjusts each score by the bot's own
terms and then applies the hold rule (above) to the adjusted scores:

| Choice point | Options and scores |
|---|---|
| Behaviour | Interact, Fight, Chase, Search, Objective, Wander, by their weighted scores (`behaviour::scores`); Carry, Fly, Arm and Return are never varied |
| Weapon | each inventory slot the hand-combat planner found usable now, by `tactics::suitability`, on the bot's planning turn |
| Aim | the body; with a splash weapon (real `splash_radius` and `splash_damage`) also its feet and a brick, vehicle or terrain face beside it within 0.7 of the radius, scored by splash alone and preferred by half again (a blast at the feet lands when a dodging body would make the shot miss), so rockets go low by default |
| Route | for a chase: straight at the enemy, or 5 units to either side where there is floor |
| Flavour | play, or goof off at a natural pause (below) |

The adjusted score is score × (1 − strength × (1 − effectiveness)) ×
e^drift / (1 + boredom), the last two together bounded to ±15% ×
strength (except flavour, which boredom alone drives):

- *Drift*: every option a bot has met carries a slow random walk
  (Ornstein-Uhlenbeck, a step a second, reverting over 90 s), so each
  bot's preferences wander apart over time.
- *Boredom*: an option in use gains boredom; it halves every 20 s once
  out of use.
- *Effectiveness*: a shot is judged after its flight plus 0.6 s (the
  target lost health or died, or not) for the weapon and aim that fired
  it; getting stuck fails the behaviour and chase route. A failure costs a
  quarter of it, a success restores half the gap, and it recovers on its
  own over 30 s. A spear that keeps missing gives way to the sword; torso
  shots that keep being dodged give way to the feet or a wall beside the
  target. A shot does not judge the behaviour: whether to stand or close
  in answers to the band.

**Gates.** Nothing varies while the bot carries an objective (a body its
tool holds, a mount to deliver, a loose body it pushes, or a picked-up item
on its way) or is urgent (under 30% health, or hurt by an enemy within 8
units in the last 1.5 s): the plain scores, through the same hold rule.
Only options scored above zero are options.

**Harm to its side.** One mechanism, `harm::shot_harm`, says what a shot
would do: it follows the shot's way (a ray to its reach, a projectile chord
by chord, a timed throw through its bounces to its burst, a swing to the
body the hammer finds), ends it at the first body it meets (bodies grown by
their motion), applies the direct hit there, shares pellets out by v20's
spread box with nearer bodies shading farther ones, and applies the blast
with the host's own falloff (`runtime::blast_falloff`). Each body counts
only where the rules let the shooter hurt it (friendly fire, self damage,
radius damage, spawn protection), and at most its health left. Its worth is
the enemies' harm less its allies' and its own, one for one; a shot is no
option when that is not positive, when it would take the shooter's own
health left, or when it would kill a teammate. The trigger keeps the swept
shape with the plan and fires only while no unpriced body has stepped in.

**Splash aims** are solved and checked like any shot: an intercept for
the aimed point, what the shot would do to each side priced like any shot
(`harm::shot_harm`), the path clear. A surface aim counts its path clear when it reaches that
surface, and at the firing gate its impact must lie within the splash
radius of the body rather than inside it.

**Goofing off.** Flavour is a choice like any other, between play and
goof, with boredom per kind of moment: play gains boredom while the bot is
at a natural pause (no threat, on its own feet, not carrying, not
playing with something), faster when it is idle, and a goof relieves it.
Playing is worth what the action is: an enemy trading shots with it is
worth all of it, so a goof never wins mid-fight; an enemy in sight who is
goofing is worth less, so a bot facing someone who stopped fighting may
goof back now and then, and holds its fire while it does. When goof wins, the bot does something
idle for about 2 s, each an ordinary player action through the player's
own path: look at a player in sight, strike an emote (`Command::Emote`:
love, hate, confusion, alarm), hop, run a small circle, walk a short
detour, look round, crouch, paint the floor toward a player with the spray
can, take out another tool and put it back, drop the weapon in hand (only
with a second attack in inventory), or flick the light. The kind's
`surprise.flavours` weights pick among them (0 turns one off). A goof ends
at once if an enemy, a threat or an objective turns up. So bots goof in
objective games too, between plays.

**Strength.** `surprise.strength` (0 to 1, 0.6 shipped) is the one
dial: it scales the drift's bound, the band, boredom and effectiveness
failures. At 0 no term is applied and no random number is drawn: the
plain scores through the hold rule. Every other constant is in code
(`surprise.rs`). The kind's tunables are `strength` and `flavours`.

**Why.** `BotThought::surprise` (`BotSurpriseView`) gives the strength,
the gate in force, a goof under way, every drive (drift, boredom,
effectiveness) and the last decision at each choice point: the plain and
chosen options, whether the terms changed the pick (`varied`), what the
hold rule did (`reason`: first, best, committed, margin, paused, beaten,
interrupt, impossible) and each candidate's score, adjusted score, drift,
boredom and effectiveness.

The gauntlet's `surprise_by_strength` reports variety (distinct choices in
effect per bot-minute), goof share, longest goof and switches away from
the plain pick at strengths 0, 0.5 and 1; the share report below holds
them to bands.

## Timing and aim, per bot

Bots never act on the shared game tick. Every cadence (the refire pulse
of script and mounted weapons, sighting alerts, the stuck hop, the
vehicle dismount check, melee footwork, bursts, aim drift, lead) goes
through `bots::cadence`: `beat(bot, salt, tick, period)` fires once per
cycle at a seeded point (gaps of half to one and a half periods, mean the
period), `cycle` gives the phase of a bot's own cycle, `drift` a smooth
seeded signal, and `spread` a seeded value per occasion (a death, a leg).
All are pure functions of the bot, a salt per cadence and the tick: no
state and no random draws.

- *Aim error* drifts smoothly between seeded points every 0.4 s, sized by
  how long the bot has tracked its target from `aim_error_degrees` down to
  a floor of half of it: never a jump, never exact.
- *Lead* is the target's velocity times 1 ± 0.15, drifting per bot over a
  second: a little under or over, never a perfect intercept.
- *Fire* comes in bursts: each cycle of about 1.25 s ends in a seeded
  pause of 0.15 to 0.5 s (a charge being wound up is let go when ready
  instead).
- *Strafe and weave* are one pattern (`Brain::strafe_leg`): each leg lasts
  0.5 to 1.5 times its mean (3.5 s in a fight, 0.75 s
  weaving at an objective or in water) from the bot's seeded generator,
  and turns back seven times in ten; it turns at once from a way with no
  floor or a wall, or toward open ground from an ally.
- *Melee footwork*: in its band a melee bot steps in, back out and aside,
  each for a seeded share of its own 0.9 s cycle.
- *Respawn*: each death adds a seeded 0.3 to 1.5 s (rules bots) or 0.75 to
  1.75 s (brick bots), so bots one blast killed do not return together.

## Weapons and upgrades

What an item is worth to fight with comes from its data alone
(`hand_combat::item_worth`): damage a second (splash at 0.6) times the
square root of its reach's share of 48 units, from its native profile or,
for a weapon a script fires, from its projectile, ray and shot data; an
unknown attack is worth 1. A bot whose weapons the native chooser does not
handle equips the best by this worth, not the first.

Empty-handed, a weapon in sight within 24 units arms it, and arming takes
over at once. Armed, with a free slot and in a game, a better weapon in
sight is an upgrade the Arm behaviour scores: 0.75 × √(share of extra
worth) × (1 − 0.4 × walk share), less the risk of turning from an enemy
within 8 to 32 units; an equal or worse weapon scores nothing. The hold
rule and boredom weigh it like any behaviour. An item it reached without
getting (an item whose pickup the package decides, or one that did not
come) or found no way to is passed over for 30 s. Pickups a script
decides are never candidates.

## Where to stand

A ranged fighter on its feet weighs a few places to stand on its planning
turn (the shot chooser's, `combat::Budget::has_turn`), as stable options
named from where it stands (`spots::SPOTS`): here; a body's width left
or right, and in toward its enemy or out as far as its weapon's band takes
it back inside (a body's width at least), where the walk grid has floor it
walks straight to; nearer than the band's near edge it deals nothing, so
an enemy hugging it is stepped back from; a ledge ahead a jump lands it on; and one higher its jets
lift it onto, as measured (`reach`). Each has one score, in harm per
second with no weight of its own (`spots::Terms`):

    dealt / (travel + fire) x (1 - taken)

*dealt* and *fire* are the best shot's damage and seconds from that place,
the shot chooser itself run with that place as its origin
(`combat::choose`, `tactics::worth`); *travel* is the walk, hop or
measured jet there; *taken* is the share of its own health the enemies it
knows of that see the place (its target and its attackers, one ray each
on the shared budget), and allies whose line of fire crosses it, would
take meanwhile (`tactics::rate`). The same harm is a bigger share of less
health, so a healthy bot takes the quick shot and a hurt one a worse shot
where nobody looks; cover and giving ground fall out, and retreat has no
other home. With no shot from anywhere, it keeps to the place that costs
it least. The surprise chooser picks (`Domain::Spot`) with its hold rule;
the place chosen is its route's goal (`Goal::Stand`), and once there it
is "here" again, held from then. Around it the strafe goes on. A melee
fighter, a swimmer and a rider keep their stance where they are. F3 shows
each place's three quantities.

## Ball games: own goals

A pushed body is never aimed at another sensor: the regions of bricks
that act on `onObjectEnter`, other than the one it is being delivered to
(in a ball game, the goal that scores for the other side). If the line
ahead of the ball along the delivery heading crosses one (grown by the
ball's radius), the push turns off it by the smallest of 20, 40, 60 or 90
degrees either way that misses; and within 5 units of one the approach
circles the ball a unit wider, so walking round it does not knock it in.

## One bot's failure

`step_bots` steps every bot each tick; one bot's failing step is that
bot's alone. It is told as an Add-On problem (`bot.step`, to the log and
the host's admins, at most once a minute per bot), the bot stays, and the
rest step and hear alerts as usual. Nothing a bot's step does is fatal to
the session, so none is passed up.

## Tuning

The rule is *nothing dormant, nothing dominant*. Everything below reads
the brain's own readout (`BotThought`, the chooser's decisions and
candidates); none of it steers a bot. All of it lives in
`crates/chaos/tests/` (`bot_gauntlet.rs`, `gauntlet/shares.rs`,
`gauntlet/tuning.rs`) and its two data files, except the live dials.

**Share report.** Every gauntlet scenario prints a `SHARES` table: the
share of bot time each kind of activity got (a behaviour, `goof` while a
flavour interrupt runs, `vehicle` while mounted) and, per other choice
point, each option's share of that point's time (`aim:feet`,
`route:left`). Each kind has a band in `tests/data/behaviour_bands.json`.
A kind the chooser offered (scored above zero; or one its band's
`offered_by` names) but that stays under `min` is DORMANT; one over `max`
is DOMINANT; one never offered is `-`. Only an `enforced` edge (`floor`,
`ceiling` or `both`) fails a test: today the floors of `fight` and
`objective`. A scenario may change a band (`scenarios`, a trailing `*`
matching a prefix). It runs with the gauntlet:
`cargo test -p bri-chaos --test bot_gauntlet -- --nocapture`.

**Off-switch check** (`off_switches`): each dial at 0, one at a time
(a dial shipped at 0 is turned on to its `on` value instead, and marked
OFF AT BASE), over every scenario, against the shipped values. It reports
how far each moved objective success (captures, laps or kills), variety
(entropy of the kind shares, in bits), goof share and goof waves (the
standard deviation of the goof share over ten-second windows), stuck
time, team kills, bands broken and frame cost per bot. A dial that moves
none of them past `unchanged` in `tests/data/bot_tuning.json` is a CUT
CANDIDATE. Output: `target/bot-tuning/ablation.{csv,txt}`.

**Sweep** (`dial_sweep`): each point of a grid, `BRI_TUNING_SEEDS` seeds
each (default 1), scored by `weights`: each band broken and each scenario
that failed its own checks costs a lot, then the most variety and goof
waves, then the least stuck time and frame cost. One dial at a time from
the shipped values by default; `BRI_TUNING_GRID=full` tries every
combination; `BRI_TUNING_POINTS="surprise.strength=0,0.3,0.6;behaviours.interact=1,2"`
gives the grid. Output: `target/bot-tuning/sweep.csv` and the ranked
`sweep_summary.txt`.

Both tools find their dials by name in the shipped `bots.json`: every
section's `strength`, plus the paths `bot_tuning.json` lists under
`dials` that `bots.json` has (a lane that names its main dial
`<part>.strength` is found by itself; another adds one line there).
`BRI_TUNING_DIALS=a.b,c.d` replaces the list, `BRI_TUNING_SCENARIOS=ctf,race`
picks scenarios by part of their names, `BRI_TUNING_JOBS` sets the
worker threads. A seed other than 0 joins that many idle spectators and
waits a few ticks before the scenario, so bots' ids and timing (their
random streams) differ. Both are slow and opt-in (`#[ignore = "tuning
tool: slow"]`, skipped by the push gate):

    cargo test --release -p bri-chaos --test bot_gauntlet off_switches -- --ignored --nocapture
    BRI_TUNING_SEEDS=2 cargo test --release -p bri-chaos --test bot_gauntlet dial_sweep -- --ignored --nocapture

**Fair metric** (`fair_hit_rate`, in the gauntlet): one bot per weapon
class against a scripted player who strafes in legs of 0.4 to 1.2 s and
hops every 1.5 to 3 s, at each range in `bot_tuning.json` `fair`. It
reports the bot's hit rate (health drops per trigger tick) in the first
seconds of an engagement and in steady state, against a band: never near
perfect, never hopeless. Enforced: the steady rate of all classes
together, and of the gun and the bow each, sits in the band. `fair_by_dial`
(opt-in) runs it with the perception dial (the first of `fair.dials` that
`bots.json` has: `perception.strength`) at half, shipped and double, and
asserts the rate moves the dial's way.

**All-on run** (`all_dials_on`): every scenario with every dial at its ON
value together (dials shipped at 0 take their `on` value), with the share
report. Every scenario's own checks and every enforced band must hold;
this is the configuration that gates merges. Ignored by default for its
length; the push gate runs ignored tests.

**Performance bar** (`bot_think_time_16`): 16 bots (`MAX_BOTS`) in an
eight-a-side mixed-arsenal deathmatch with every dial on; it measures bot
think time per tick (`Session::bot_think_nanos`, wall time in
`step_bots`) and fails over `perf.debug_us` or `perf.release_us`.

**Live dials.** An administrator types, in chat or the console:

- `/botset surprise.strength`: shows the dial on every bot kind that has it.
- `/botset surprise.strength 0.6`: sets it on every kind that has it
  (`/botset sight 40 bot.blockhead` on one kind). The settable paths are
  `bot_kind::tuning::SETTABLE` (the six main dials plus what a kind is:
  radii, `behaviours`, `melee`, goof weights); the kind's own validation applies,
  so an out-of-range value is refused and changes nothing. Bots take it up
  at their next decision.
- `/botsave`: writes the dials set so far to `bot-overrides.json` in the
  user's data directory, beside `settings.json`
  (`%LOCALAPPDATA%\BlocklandReImagined` on Windows), never the install
  folder. Every game this computer hosts starts with them; one that no
  longer fits (a kind or dial gone) is left out with a console warning.
- `/botreload`: reads the Add-Ons' `bots.json` and the override file again
  (unsaved `/botset`s are dropped), so an edited `bots.json` takes effect
  without restarting.

A player who is not an administrator is told so and nothing changes. A
dedicated server reloads `bots.json` but keeps no overrides. Changing map
starts the new session from the files, so unsaved `/botset`s end with it.

**Why readout.** With the performance overlay open (F3), the bot the host
player looks at shows a few lines under it (`BotThought::why`): what it
is doing and the chooser's reason, the top three candidates with their
scores, the biggest terms on them (drift, boredom, effectiveness, the
hold in force) and what it last noticed. The host fills it four times a
second only while the overlay asks (`ServerPerf::bots_wanted`, host-local,
never sent to players), so it shows for games this computer hosts and not
on another computer's server.

## Coordination

What teammates do is information that moves a bot's scores, never an
order: it goes through the same chooser, with surprise and commitment, as
everything else (`bots/team.rs`). Nothing names a game, item or vehicle.

Each bot publishes its current choice as an *intent* beside the claims
(`claims::Intent`, lapsing three ticks after it stops): where it goes or
stands, its target, a vehicle whose free seats it controls while it waits for crew, the space its
weapon will hit (`claims::Space`, from real reach, splash and aim error,
the same test the hold-fire check uses) and, from a seat it does not drive,
the line its mount needs. Allies' intents enter each option's score as
**overlap** (an earlier ally on the same target, or doing the same option
at the same spot, costs, each further ally half the one before: the first keeps it, and a choice among places or
targets prefers an uncrowded one; following an ally through a gap too narrow to pass it, at its
pace rather than walking round it is the same rule) and **interaction**
(a seat an ally offers, or a driving place from which a seated ally's
mount sees its target, pays). An ally's line of fire is not a term here:
a place to fight from that it crosses costs what that ally's weapon
deals, so it fights from somewhere else (see Where to stand). Crew of one vehicle neither crowd nor endanger
each other; a seat stays the claim's to arbitrate. Socially, an objective
is worth more as the team trails; idle flavours grow likelier with the
share of the players a bot sees goofing, less those it sees playing, a
person counting `mood_human`, capped. Bots and people are read the same
way, by what they visibly do: a goof is a bot's published flavour or a
person's emote, spray can or other tool that does not attack; play is
moving with purpose, attacking or holding something; and an option it saw work for a
teammate (a hit) scores a little more for a while (`copy`, fading over
the surprise `effectiveness_seconds`), with boredom as the brake. A choice
the terms changed may be said in team chat, keyed by the term that moved
it (`callouts`). `BotThought::team` shows what it read and each option's
terms.

A kind sets `team.teamwork` (0-1, scales overlap and interaction
together), `team.mood` (the mood's pull) and `team.pressure` (how much
more its objective is worth as its side trails). `mood_cap`, `mood_human`
and `copy` are fixed in code; radii come from its sight. Still
unsupported: a goal to defend, passing.

## Extras

Five small options round out what a bot does, each through ordinary player
controls and each weighed where the brain weighs everything else: how it
moves this moment is a choice of the surprise chooser (`Domain::Move`:
keep on, crouch, hop), clicking a door for fun is a goof (`door`),
handing a weapon over is teamwork (`team.teamwork` 0 turns it off), idle
play is an Interact opportunity (the `interact` weight), and a door in
the way is always clicked. They live in
`crates/sim/src/session/bots/extras.rs`, with a hook in `step_bot` and the
Interact opportunity. Nothing reads a content name.

| Option | When | Does |
|---|---|---|
| `idle_play` | no enemy seen or remembered | Interact offers a push on a loose body toward the nearest player in sight (stopping 3.5 short of them), and a passenger seat in a vehicle a teammate drives; a rider stays while the teammate drives. Scored 0.2, between Wander and Return, so every purpose outranks it. Off while any bot in the same game works an objective and until 10 s after the last one (the start of a round is not calm), and never on a body another bot claims, so play cannot spoil a match. |
| `crouch` | for 1.5 s after a hit from more than 5 units, while fighting or holding its ground: the move choice weighs a crouch as much as keeping on | crouches (damage already scales with crouching) |
| `dodge` | a projectile, not its own or an ally's, that can hurt (damage or splash damage) and whose path over the next 0.75 s (velocity, ballistic fall) comes within the body plus its splash radius, where floor lies under the spot 0.8 s of its current drift reaches; each projectile is judged once by the move choice, a hop worth as much as keeping on, more by however much of the health it has left past half the shot would take | goes one of three ways (`Domain::Dodge`, each open way worth the same and growing stale as fast as a goof, so dodges vary): a hop straight up for a quarter second; a strafe square to the shot's flight, away from its line, for a third of a second, where floor lies under the step and never into where an ally's weapon will hit; or a crouch's charge and then the jets straight up, only while its jets lift it with half a second of fuel (`route::Jets`) and its kind flies. A bot carrying an objective keeps to its way and does not dodge |
| `activate` | a door within 2.5 units on the straight way to its goal, while not fighting; and the `door` goof: at a natural pause, a door within 8 units in sight (looked for about every half second). A door is a brick whose catalog swap the next click swaps back: the click only opens or closes the brick itself. A brick with only event rows (a reset, win, teleport or blast button) is never clicked, in the way or for fun | aims at it and clicks with the empty hand (`Command::Activate`), putting a tool away first and taking it out again after |
| `hand_weapon` | wandering, calm, no enemy seen, with two or more attacks and a teammate in sight with none and a free slot, its teamwork above 0 | walks within 2.6 units, faces them and drops a spare (not the one in hand) their way (`Command::DropTool`); the ordinary contact pickup, or their arming, takes it |

Route clicks are what an activation is worth: a door in the way is clicked
whenever the option is on. Activations an objective needs stay with the
objective planner. Tests: `crates/chaos/tests/bot_extras.rs`.

## Looks and names

Each bot gets a seeded look on top of the avatar pack's defaults: a face
and a decal from the pack's own lists, and clothing colours from the
server's opaque paint colours (arms and legs in matching pairs, skin
kept, see-through parts left see-through). A kind's `look` is applied on
top. A brick bot is called by a first name of its kind no other player
goes by, kept while it lives; a brick with its own name keeps the
"Kind (name)" form, and a kind without a free first name falls back to
its kind and team label. See `bots/looks.rs`.

## Data

- `bots.json` (a kind) sets only what the kind is and the six main dials
  (`bot_kind::tuning::SETTABLE`); any other number there is an error.
  Everything else (reaction, turn rate, aim error, memory, the rest of
  `perception` and `team`) is fixed in code at `BotKind::default`, and
  the contest, hold margin, strafe and driving numbers are constants, so tuning
  means turning a main dial. What a kind sets:
  - sight, wander and chase radii, whether it fights other builders' bots,
    whether it warns its side (`alerts_allies`) and its `behaviours`
    weights.
  - `objective_radius`: how far around itself it looks for loose objects
    an objective can use (24 for the Blockhead).
  - The main dials: `surprise.strength` (0 to 1, variation and goofing),
    `team.teamwork` (0 to 1), `team.mood` (the pull of others goofing),
    `perception.strength` (0 to 4, how human its noticing and aim are),
    `hold_seconds` (0 to 5, how long a choice is held, 0.5 shipped) and
    `team.pressure` (objective worth as its side trails). Each is commented
    in the Blockhead's `bots.json`; `bots.json` takes `//` comments outside
    strings. `/botset` and the tuning tools turn these and the other
    `SETTABLE` paths below.
  - `surprise.flavours`: a goof's weight, to turn one off for a kind (each
    has its usual weight in code). `team.callouts`: its team chat lines.
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
- A spawn brick's `team` (`VehicleSpawn::team`, the wrench's Team menu,
  listing the builder's mini-game teams): the team slot its bot plays for.
  It is applied when the bot joins its builder's game, after every new life
  (a reset or respawn), after a build loads, and whenever the choice or the
  game changes; in between, the game's own commands (SetTeam) may move the
  bot. A slot the game has not got is applied once the game has it. With no
  choice, the team is left to the game. A spawn brick's bot is named after
  its kind and then its brick's name, or else its team ("Blockhead Bot
  (Red)"), so the MiniGame Players list tells the bots apart.
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
