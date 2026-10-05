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
   | Fly | an enemy up where its path does not walk, with usable jets | jets straight up, out from under cover, over, down by them |
   | Interact | a useful, permitted environmental opportunity | reserves a seat or loose hazard, approaches and executes through ordinary controls |
   | Fight | an enemy in sight within its weapon's band | stands, strafes, backs off when too close |
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

   An on-foot bot's leash is its brick and `chase_radius`. A driver's leash
   follows the kind's `mounted` policy instead: with `anchor: "mount"` it is
   measured from where the bot took the controls, with `mounted.chase_radius`;
   with `"home"` from its brick. So a bot that boards a vehicle 40 m out keeps
   pursuing a target 60 m out rather than turning back at its walking leash.
3. **Goal and path.** The behaviour sets the goal; the walk grid
   (`crate::nav`, shared by every bot of a body size, updated as bricks
   change, portals included) finds the way a little each tick. Gaps only
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
   A chassis backs onto a goal more than `mounted.reverse_degrees` off its
   hull. While pursuing (Fight, Chase, Search, Fly) it does so only within
   `mounted.reverse_distance`; a farther target behind is turned toward, so a
   chase is not driven as a long retreat. Fixed deliveries and the walk home
   still reverse.
5. **Move and act.** The behaviour's movement, then getting unstuck (hop,
   plan again, give up the goal) on foot; drivers instead brake, replan and
   relinquish an unproductive seat. Shared claims are exclusive advisory
   intentions, released on success, preemption or invalidation. A claim never
   overrides human occupancy, trust, mini-game policy or an Add-On ride hook.
   Claims on a loose body (a ball, a crate) conflict only between allies:
   an opponent may pursue the same body and push it toward its own goal.
   Seats stay exclusive for everyone. That contest is its own piece
   (`bots/contest.rs`): when an opponent is the body's mover or holds a live
   claim on it, the approach leads the body along its velocity
   (`contest.lead_seconds`, at most `contest.max_lead`), and being within
   `contest.engage` of it counts as progress, so the claim's lease does not
   lapse while the two sides fight over it. A step waiting on an admitted
   delivery holds no lease at all.

   The contest piece also covers the other cases of a shared body:
   - **Cover.** A bot whose claim is refused because a teammate holds the
     body keeps its objective. It does not stand down into Wander: it holds
     a cover point `contest.cover_distance` behind the body, against the
     team's delivery heading, and `contest.cover_side` to the side. A lone
     cover takes the side the working teammate is not on. Several covers
     take id-ordered slots on both sides, each further pair further back and
     wider. A cover point stops short of any wall between it and the body.
     When an opponent drives the body back at the cover, the cover stands
     beside the body's path instead, so the drive is not deflected off it.
     When the claim frees, the cover takes the body up at once.
   - **Clear.** A push or hammer on a body an opponent drives straight back
     at it turns by `contest.clear_degrees` to the bot's side, knocking the
     body out of its line rather than being carried back with it.
   - **Walls.** When solid world geometry stands where a pusher would stand
     (a wall or a corner behind the body), the push turns by the smallest of
     30, 60, 90 or 120 degrees that leaves room. The body is worked along
     and off the wall instead of being pressed into it.
   Physical pushing transfers only momentum stopped at actual motor contacts,
   through the same mechanism for humans and bots; mass and geometry determine
   the outcome. See [bot-interactions.md](bot-interactions.md) for the lifecycle,
   budgets, provider boundary and examples.

## Noticing

`bots/perception.rs` adds two small mechanisms on top of sight, tuned by
the kind's `perception` (eight numbers, on by default for every kind).

- **Glances.** A strolling or homeward bot (Wander, Return) with no enemy
  in sight, no objective at hand, nothing held, no seat and no chassis may
  turn its ordinary aim at something salient for about `glance_seconds`,
  then waits `cooldown_seconds`. Sources are engine data only: a
  projectile blast (the weapons runtime's `Blast` event), noticed out to
  10 units per unit of its radius; a weapon sound, out to 12 units at full
  `volume`; a stare (someone in plain view looking within 8 degrees of it
  for 1.5 seconds) or a body faster than twice its own running speed,
  within 0.3 of its `sight`. `salience` scales every reach (0 turns
  glances off). Salience falls from 1 at the source to 0 at its reach and
  is the chance of a glance. The walk goes on; only the look turns.
- **Reaction.** A newly seen target, or an attacker it was not already
  fighting, starts a reaction: `reaction_seconds` times `combat_scale` when
  it was fighting or hunting (Fight, Chase, Fly, Search), `relaxed_scale`
  when strolling or playing about (Wander, Interact), times `away_scale`
  for a target outside its `view_degrees` cone, varied by 30% either way
  from its seeded RNG. It fires only after it, and turns `away_scale`
  times slower toward a target from outside its cone until then. The same
  scale multiplies its starting aim error (`aim_error_degrees`), which
  narrows over the usual two seconds of tracking. A spawn-protected target
  is watched but not reacted to: the clock starts when it can be hurt.
  Damage still interrupts at once (the chooser sees it as before); only
  the return fire waits. `alertness` (0 to 1) blends the delay, error and
  turn cap in together: 0 keeps exactly `reaction_seconds` and the plain
  error. `perception::delay_ticks` and `Brain::switch_delay` give the same
  delay to any other pause before acting on a change (a chooser's tell).

`BotThought::noticed` reads out the last glance or reaction (`glance:
blast`, `glance: watched`, `reacting: relaxed, from behind`, ...) with
when it started and ends.

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
  - `objective_radius`: how far around itself it looks for loose objects
    an objective can use (24 for the Blockhead).
  - `contest` (`lead_seconds`, `max_lead`, `engage`, `cover_distance`,
    `cover_side`, `clear_degrees`): how it plays a body an opponent is also
    working, and how it covers one a teammate works (above). A
    `cover_distance` of 0 stands down as before, and a `clear_degrees` of
    0 meets a drive head on.
  - `perception`: glances and reaction delays (Noticing, above).
  - `mounted` (`anchor`, `chase_radius`, `reverse_degrees`,
    `reverse_distance`): its pursuit policy while it drives (above). It is
    the same for every vehicle; nothing checks a vehicle's name.
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
