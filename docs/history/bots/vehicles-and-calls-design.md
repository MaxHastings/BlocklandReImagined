> **Historical note. Written 2026-10-11 (the file's date in the cloud project folder, `bots/vehicles-and-calls-design.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Bots, vehicles and calls: design note (2026-10-11)

Max's ask (thread "Bots, vehicles and calls"): bots get into a jeep, barely
move and get out; two bots crewing a tank is rarer than with people; and
"get out of the way" / "get in" calls feel missing. He asked for a lasting
design, not test-passing special cases, and to challenge what exists rather
than add to it.

## What the code does today

**A vehicle is a taxi to a known enemy.** A seat is an Interact
opportunity (`bots/interactions.rs`, `bot_opportunity`) that exists only
while the bot knows an enemy (or, idle, a player to play near). An unarmed
chassis is boarded only when walking to the seat and driving beats walking
to that enemy (`route::drive_serves`). Then:

- **No exploring from the seat.** `explore_to` requires `driving.is_none()`
  (`bots.rs` ~2219). The moment the enemy is lost, a driver has nothing to
  do but Wander, and a seated Wanderer gets out on its next dismount beat,
  every 3 s (`bot_seated_input`, the `DISMOUNT` cadence). This is the
  "gets in, moves a little, gets out" Max sees in Afghan DM.
- **A short drive leash.** A driver's leash is 96 units from where it took
  the controls (`DRIVE_CHASE_RADIUS`), even for a rules bot whose on-foot
  leash is unbounded. Driving past it wipes its memory of the enemy, which
  again means Wander, then out.
- **A bot that knows no enemy never boards.** In a deathmatch that is most
  of the time between fights, so a bot that spawns beside a jeep or a tank
  walks past it.

**Crew is a 3-second coincidence.** A driver offers its free seats
(`team_seats`) only within `CREW_WAIT` (3 s) of boarding and while at rest,
and only brakes for a coming ally inside that same window. An ally takes
the offer only if it already knows an enemy, is within 24 units, sees the
vehicle, and the vehicle is not moving. A tank's driver cannot fire its
gun, so a lone tank driver is a slow jeep.

**Teammates already talk, silently, and that part is sound.** Every bot
publishes an intent (`claims::Intent`): where it goes, its target, the
seats it offers, and the space its planned shot will hit (`harm`, real
reach and blast). Allies read intents as score terms, never as orders. The
weakness is who listens:

- An ally's planned blast is read only when a fighter picks where to stand
  (`spots`). A bot walking, chasing or exploring through the blast ignores
  it; the shooter just holds fire.
- A moving chassis publishes nothing, so allies on foot walk into a jeep
  that is pulling away (the v0.2.11 run-over residual from the hardening
  pass).
- The only words said are a reply ("Coming!") when the seat term changed
  a choice. The driver never asks and the shooter never warns.

## Verdict: missing pieces and deletions, not a rewrite

The existing shape already treats a vehicle as a tool: a seat is an
Interact opportunity priced as a leg toward where the bot is going, the
same route planner drives the chassis, and leaving is judged from what
the bot observes. "Use the jeep" is not a goal anywhere in the code. What
broke Max's case were gates bolted on top that made the seat a taxi only
to a known enemy: once that enemy was gone, nothing kept the bot playing
from the seat, so it got out. The fix is to delete those gates and fill
three gaps, not to rewrite the vehicle layer.

Deleted or narrowed:
- the `driving.is_none()` gate on exploring (a driver explores);
- the 96-unit drive leash for rules bots (their leash is the map, seated
  or not; brick bots keep theirs);
- the 3-second, at-rest window on offering seats and the driver's matching
  3-second crew wait (the ally's claim lease now bounds the wait);
- the rule that a gunner leaves when nothing is in sight while an ally
  still drives.

Added (each one general, read from the vehicle definition):
- board for exploring and idle play too, not only for a known enemy;
- bail out of a hull the next hit would wreck;
- brake for a drop ahead (a driver that explores must not drive off the
  map's edge); a stalled explore drive forgets its place;
- calls: an ally's planned harm (a shot's way or blast, or a moving
  chassis's sweep) feeds the dodge reflex the receiver already has; an
  `offer` and a `blocked` team-chat line say what the intent already
  says.

**No authored bot profile for vehicles.** The vehicle definition already
says what a bot needs: its family (wheeled), seats and their roles, its
gun, run-over damage and push, its bounds. What it cannot say honestly,
how fast it stops and how tight it turns, is measured from its own
physics (`reach::Handling`), so it is never stale when an author tunes
the vehicle. A hand-written profile beside the model would duplicate
these numbers and drift from them, so the jeep and the tank get none.

The acceptance test for the design: a new wheeled vehicle Add-On works
with no bot code change. Nothing here names a vehicle; seat roles, guns,
run-over damage and measured stopping and turning come from its
definition.

## What a lasting design looks like

One idea: **a seat is a body and a weapon the bot holds, not a leg it
takes.** Once seated, the bot plays the game from the seat with the same
behaviours it uses on foot, and it leaves when the seat stops serving it,
judged by what it observes. Calls are the visible half of intents bots
already publish, and the receiver decides with the reflexes it already has.

### Vehicles

1. **Play from the seat.** Delete the `driving.is_none()` gate on
   exploring: a driver with no fight known explores, with its chassis as
   the body (the route planner already plans chassis routes). A rules bot's
   drive leash matches its on-foot leash (unbounded); a brick bot keeps its
   brick's.
2. **Board for what the bot is doing, not only for a known enemy.** The
   seat opportunity's destination becomes whatever the bot is going toward:
   the enemy it knows, else the place it would explore, else (idle) the
   player it plays near. Same `drive_serves` test, same utilities. An armed
   vehicle, or one that runs people over, is worth boarding for the fight
   itself as today. Nothing names a vehicle; it is all seat roles, speed
   and runover damage from the definition.
3. **One set of reasons to leave, all observed.** Kept: a wreck or a
   flipped hull, a drive that makes no headway, the leave leg (a chassis
   that cannot hurt its quarry carries the bot to it), a passenger with no
   driver, and nothing to do at all. Added: **bail out of a hull the next
   hit would destroy** (the last hit it took is more than the damage it has
   left), which is what people do when a tank starts smoking.

### Calls

4. **"Get in": an offer lasts as long as the seat is free, and the driver
   waits as long as someone is coming.** The offer drops its 3-second
   window and its at-rest condition. An ally within sight of an offered
   vehicle may claim its seat even while it moves; a driver with an ally's
   live claim on its vehicle stops and waits. The claim's own lease bounds
   the wait (renewed only while the ally keeps closing, 15 s at most), so
   the 3-second crew timers go. The driver says the kind's `offer` line in
   team chat ("Get in!") when it starts offering, at most once every 30 s
   as callouts already are; the ally's "Coming!" stays.
5. **"Get out of the way": the receiver dodges, if it wants.** An ally's
   published harm that contains a bot becomes something coming at it, read
   by the same dodge reflex it uses for an enemy's shot in flight (keep or
   dodge, chosen with its surprise, which way chosen likewise). It can
   decline. The shooter still holds fire until clear (unchanged) and says
   its `blocked` line ("Move!") when it holds fire for an ally.
6. **A moving chassis is harm too.** A driver whose vehicle runs people
   over publishes the space it will sweep before it can stop (the same
   stopping distance it brakes by) as its harm. Allies standing to fight
   avoid it, and allies on foot in it dodge out of it. This is the jeep
   run-over residual, fixed by the same mechanism instead of a special case.

## What gets simpler

- The two time windows after boarding (`CREW_WAIT` in `team_seats` and in
  the driver's crew wait) are replaced by the claim lease that already
  exists.
- The driving exception in exploring and the untethered drive leash are
  deleted.
- "Get out of the way" adds no new movement: it feeds the existing dodge.

## Two time bounds that stay, and why they are not behaviour timers

- The seat claim's lease (renewed only while the claimant keeps closing on
  the seat, 15 s at most) bounds how long a driver waits for an ally who
  stopped coming. It is a lease on a reservation, not "drive a bit, then
  get out".
- The 30 s between one bot's team-chat lines bounds chat spam. The intent
  under a line is published every tick regardless.
- Getting out with nothing to do is looked at on a beat so a lull between
  two choices is not taken for it; the reason is still observed
  (`Leave::NothingToDo`), and every seat left is counted by its reason
  (`Session::bot_seat_leaves`), so a timer sneaking back in would show.

## What I will not build

- Orders, a message protocol between bots, or obedience. Calls are intents
  with a line of team chat attached.
- Horses, planes or boats (still wheeled vehicles only), or a change to
  the run-over damage rule.
- Measured vehicle stopping distances (the parked "measured moves" item).
- Anything keyed to the jeep, the tank or Afghan DM by name.

## Evidence I will show

- Content-free gauntlet scenarios: the existing jeep battle, reporting how
  much of the match bots spend driving, and a new team battle with test
  tanks reporting tank crew time, tank kills and own-side kills, before and
  after.
- The real-content deathmatch stress run in Afghan DM with its jeep
  spawns swapped for the tank (a new opt-in setting on that test), run on
  Max's PC, reporting crew time and kills from vehicles.
