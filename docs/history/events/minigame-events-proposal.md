> **Historical note. Written 2026-10-01 (the file's date in the cloud project folder, `events/minigame-events-proposal.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Making mini-games from the wrench

A brainstorm, not a plan of record. Nothing here is built and none of it is
in v0.1.11. It goes into the base game's event system, not an Add-On.

## The contract (after Max's stress-test writeup, 10-01)

These rules win over anything below them in this file.

1. On Enter / On Leave are swept crossings: the engine checks the path an
   object moved since the last check, not just where it is now. When two
   objects cross in the same check, the one that crossed earlier is handled first.
2. Zones keep their own list of who and what is inside. Death, disconnect,
   deletion and despawn take things off the list without firing On Leave.
   Filters can read the list: players inside, objects inside, my team inside,
   enemy team inside.
3. Spawn, respawn, teleport, round reset, editing or moving the zone brick and
   turning its events on refill the list without firing On Enter. This stops
   a ball that respawns inside a goal from scoring, and stops event loops.
4. Object is the context of the event, not a target that pretends to be every
   class. Typed targets come from it (Player, Owner, Last Toucher, Team,
   MiniGame) and are greyed out when they don't apply. Object's own outputs
   are only ones that are safe for any object, starting with Return To Spawn,
   which does nothing for an object with no spawn brick.
5. The wrench has one way to say what counts: On Enter plus filters (Type,
   Kind, Team, Owner, ...). There is no separate "what" picker.
6. Last Toucher is its own record and is not the 5-second vehicle push credit
   (docs/progress.md: "movers are credited for 5 s"). It comes only from direct
   contact: pushing, shooting, driving, throwing or grabbing. It lasts until
   another player touches the object or the object respawns. If that player
   leaves the server, the record stays (team is still known) but you can't
   target them as a player. Credit passed from ball to ball is left out for
   now, so pool and bowling are not promised in the first version.
7. Progress (the per-player number) survives death. It goes back to 0 on
   round reset or on leaving the mini-game.
8. The brick Counter goes back to 0 each round. v20 Print Count does not change.
9. Win Round happens once. After a round is won or ended, nothing can change
   its winner.
10. Delayed events that change the mini-game remember which round they came
    from, and do nothing in a later round. A round reset also cancels them.
11. A player changing team while inside a zone gets no fake Leave or Enter.
    Team filters read the team they are on now. Two mini-games that overlap in
    space each use the object's or player's own mini-game.
12. Zone events go through the existing event queue and its work limits. They
    never get their own fast path.

**The first test build** proves one path from start to finish: Alex pushes a
Steel Ball, it crosses the finish, On Enter [Kind = Steel Ball] fires, Win
Round goes to Last Toucher, the round resets, the ball respawns with Last
Toucher cleared, and the zone list refills. Then the same code, with no special
cases, runs a Jeep checkpoint, a soccer goal, king of the hill, a two-person
pressure plate and a delivery zone, each as an automatic test.

## Revision after Max's second writeup (10-01)

Max's second writeup is adopted as the base, with these changes:

- **One "Object" target instead of more special targets.** The thing that
  caused the event (player, bot, vehicle, ball, item, projectile) is the
  *Object*. *Object's Player*, *Object's Owner*, *Last Toucher* and *Client*
  are offered only where they make sense; the editor greys out the rest. v20's
  Player/Bot/Driver/Ball targets stay as they are.
- **Filters replace my "only if" box.** Each line gets a small fixed set:
  object type, object kind (datablock), team, owner, same mini-game, has a last
  toucher, and later speed/mass/health. Nothing open-ended.
- **Enter and Leave first, Object Touch later.** Enter/Leave use the zone check
  that only runs near watched bricks. Physical touch for every vehicle and ball
  would mean a callback on every collision, so it comes after we measure.
- **One number per player instead of a fixed "Checkpoint".** Call it
  *Progress*: checkpoints, laps, coins or keys all use it, with set, add and
  reset outputs and an equals / at least filter. Hooked to v20's existing
  checkpoint bricks, so dying sends you back to your last checkpoint.
- **Counter and State on bricks**, kept beside v20's print count, which does
  not change.
- **Name stand-ins in messages** (Player, Last Toucher, Team, Score, Counter,
  Progress, Time left). Names only, length-capped like normal chat.
- **Mini-game target gets** add/set score, add/set team score, win round, end
  round, start round, and set/add/pause time, plus onRoundStart and onRoundEnd.
- **The test for any new piece:** it must make at least five unrelated games.

**First release of it:** Object target, Enter/Leave, Last Toucher, filters,
mini-game score/team score/win round, and the player Progress number. Teams
from the wrench come with it if the Slayer/CTF team pieces have landed.

## Where we are today

- Bricks run all 16 v20 inputs (things like *On Activate*, *On Player Touch*,
  *On Projectile Hit*, *On Relay*) and all 65 v20 outputs. Chains run without
  v20's forced 33 ms wait per hop, and every brick has work limits so a loop
  cannot freeze a server.
- A brick only notices **players and bots** touching it, plus the sports ball
  and projectiles. A vehicle, a Steel Ball or a dropped item rolling onto a
  brick does nothing.
- The mini-game can add score to one player (*IncScore*), print messages to
  everyone, reset and respawn everyone. It cannot declare a winner, has no
  teams, no round timer, and no score limit.
- Events cannot ask a question ("only if the player is on Red"). v20 players
  got that from the VCE Add-On; vanilla never had it.
- The Slayer/CTF work (not merged yet) is adding the engine pieces we need:
  teams inside a mini-game, team scores, and "zones" that notice a player
  entering or leaving the space over a brick. Those are only reachable from
  Add-On scripts today. Most of this proposal is putting a wrench face on
  those same pieces, so there is one implementation, not two.

## The seven additions

Ranked by how many games each one unlocks.

### 1. "Something enters this brick" and "Something leaves this brick"

New inputs: **On Enter** and **On Leave**, with a picker for *what* counts:
Player, Bot, Vehicle, Object (Steel Ball, sports ball, physics props), Item, or
Anything, and optionally one specific kind (only Steel Balls). Turn the brick's
collision off and it becomes an invisible finish line, goal mouth, checkpoint
or pressure plate.

Lets players build: finish lines, soccer goals, checkpoints, delivery drop-offs,
"put the ball in the socket" puzzles.

Plus **While Inside**, which fires every N seconds (1 s minimum) while
something stays in the space. That gives you king-of-the-hill and capture points.

### 2. "Last player to touch it"

Every vehicle, ball and physics object remembers the last player who pushed it,
shot it, drove it or threw it. Events get a new target, **Last Toucher**, that
works like *Player* and *Client* (score, messages, kill, etc.), and their team.

Lets players build: anything where you push or shoot something into a goal and
the right person gets the credit. Without this, a Steel Ball's points go to
whoever built the spawn brick, which is usually the host.

Default picked: the memory is kept until someone else touches it, and cleared
when the object respawns.

### 3. Teams from the wrench

Mini-game settings get teams (name and colour, up to 8), using the same engine
teams Slayer uses. New outputs: **Join Team** (puts the player on a team; a
"pick your team" room is just two doors with this event) and **Team score +N**.
New targets: **Team** (the player's whole team, for messages and respawns).

Lets players build: team pick rooms, red-vs-blue soccer, team-only doors, team
bases with their own spawns.

### 4. Win, score limit and rounds

New mini-game outputs: **Win Round** (for a player or a team, announced to
everyone, then the round resets after a few seconds) and **End Round** (no
winner). New mini-game settings: **Score limit** (first player or team to N
wins on their own) and **Round time** (when it runs out the highest score
wins). New inputs on any brick in the game: **On Round Start**, **On Round
End**, **On Time Left** (fires at a chosen number of seconds left, for
"30 seconds!" announcements).

Lets players build: races with a clear winner, timed soccer matches, "first to
10 goals", survival rounds.

### 5. One "only if" check per event line

Each event line gets an optional **Only if** box with a short fixed list:
player is on team X; player holds item X; player is / is not in the mini-game;
this brick's counter is above / below / equal to N; a vehicle or object is of
kind X. That is all: no arithmetic, no text, no nesting. If the check fails
the line is skipped.

Lets players build: team doors, key-and-lock doors, "only the ball, not
players, scores", puzzles that need three switches on.

### 6. A counter on every brick

v20 already hides a number in every print brick (people used it as a counter).
Make it a plain **Counter** on any brick with **Add**, **Set** and **On Counter
reaches N**, and show it on a print if the brick has one. Works with the
*Only if* box above.

Lets players build: laps ("3 laps then win"), "press all four buttons", door
codes, scoreboards made of print bricks.

### 7. Repeating timer and reset-the-object

**Every N seconds** input (1 s minimum, only while a mini-game round is
running) replaces the self-relay loops people built in v20. **Send it back**
output on the Object / Vehicle / Last Toucher's thing returns a ball or vehicle
to its spawn brick (the existing *Respawn Vehicle* only works from the spawn
brick itself).

Lets players build: a ball that resets after each goal, obstacle courses with
moving hazards on a timer, waves that start every 30 seconds.

## Two games made only from the wrench

**Steel Ball race (two players).**
1. Build two lanes. Put a Vehicle Spawn brick with a Steel Ball at the bottom
   of each, and a flat brick with collision off across each finish.
2. Finish brick: *On Enter* (Object: Steel Ball) → *MiniGame: Win Round* for
   *Last Toucher*.
3. Make a mini-game, join it, push. The round resets on its own and both balls
   go back to their spawns.

**Red vs Blue soccer.**
1. Mini-game settings: teams Red and Blue, score limit 5, round time 5 minutes.
2. Two doors in the lobby: *On Activate* → *Join Team* Red / Blue.
3. Each goal mouth: *On Enter* (Object: ball) → *Team score +1* for the other
   team, *Center Print All* "GOAL!", and the ball spawn brick (by name) →
   *Send it back* after 2 seconds.
4. The score limit and timer pick the winner.

Others the same pieces cover: checkpoint kart races (counter = laps), king of
the hill (*While Inside* → score), bowling (*On Leave* on each pin's brick),
capture the flag lite (zones + teams), a delivery game (*On Enter* for an Item).

## What it costs, and the limits

- **Speed.** Only bricks that have one of the new inputs are watched. They are
  checked a few times a second (default every 100 ms, same as the zones seam),
  and only against things near them, using the same nearby search the game
  already uses. Bricks without these events cost nothing.
- **Network.** All of this runs on the server. Nothing new is sent except the
  scoreboard and timer, and only when they change.
- **Limits** so a public server stays safe: 256 watching bricks per builder and
  4,096 per server; *While Inside* and *Every N seconds* no faster than once a
  second; win, score and team outputs only work on bricks that belong to the
  mini-game (v20's existing rule for mini-game events); everything still runs
  inside the existing per-brick work limits.
- **v20 compatibility.** Nothing about existing events or saves changes. The
  new inputs, outputs, targets and the *Only if* box are additions. Old saves
  load exactly as before. A build that uses the new events saves them in our
  own format; v20 save files cannot hold them, so exporting to a v20 .bls drops
  just those lines and says so.

## Where we leave v20 behind

v20 had no teams, no win output, no conditions and no object-entered events
in vanilla. Those all came from Add-Ons (Slayer, VCE, assorted event packs).
Putting them in the base game is a deliberate change from v20, in line with
"QoL plus missing critical features". Every v20 build still works the same.

## What I'd leave out

- **A scripting language in the wrench.** No variables with maths, text
  formulas, loops or nested ifs. That is what Add-On scripts are for, and it
  is what made VCE hard for kids to use.
- **Game-specific features.** No "Race" or "Soccer" mode. Only the pieces.
- **Per-player saved stats** (levels, money that survives a restart). That is
  RPG territory and belongs to Add-Ons.
- **Anything that runs every tick per brick.** Every repeating thing has a
  1-second floor.
- **Client-side cosmetic events** (per-player-only colours, private sounds).
  They cost bandwidth and invite cheating.

## Defaults I picked

- Built into the base game, available in every mini-game (Max's call).
- Last Toucher remembered until someone else touches it, cleared on respawn.
- Up to 8 teams per mini-game.
- Watched bricks checked every 100 ms.
- Slayer/CTF and these events share the same team and zone engine pieces, so
  this waits for that lane to land.

## Suggested order if it goes ahead

1 and 2 together (they are the Steel Ball race), then 4, then 3, then 5 and 6,
then 7. After 1, 2 and 4 the Steel Ball race works; soccer also needs 3 and 7.

## Events or Add-On? (Max asked 10-01)

- Events: a game that belongs to this build (this finish line, these goals).
- Add-On: rules you want on any build (Slayer), or a new thing (CTF's carried
  flag). Both can mix: Slayer for teams and scoring, events for the map's twist.
- To teach it: a "Game mode" list in mini-game settings showing installed
  mode Add-Ons, and a one-page "Events or Add-On?" in the modding guide.
