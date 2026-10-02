# Rule Workshop: creator test card

Judge creation, not the number of available features. The time labels below
describe ambition, not a promised completion time. Keep [PLAYTEST.md](PLAYTEST.md)
for controls/recipes and [DESIGN.md](DESIGN.md) for the chosen semantics.

## 30-second ambition: start from a blank brick

Before placing a Rule Workshop example, author `onActivate → Self → setColor`, then a switch
that opens a door or changes a named panel's collision/rendering. Try a touch
response and projectile-hit sound using classic events too.

Record how long it takes to get the first working result. Can you ignore IF,
variables and context entirely? Does the editor still feel like Events? Extra
capability does not excuse making this baseline harder.
Repeat these simple tasks in your familiar Blockland version where practical;
compare concrete friction, not just nostalgia or which interface looks nicer.

## 30-minute ambition: discover one new concept at a time

Build a team-only door from scratch. Then choose two recipes to mutate:

| Challenge | Change to make | What it reveals |
|---|---|---|
| Three-button puzzle | Reset after a mistake; add a timeout or a ball-operated switch. | Shared state, readable chains and duplicated bookkeeping. |
| Delayed trap | Change team or state during the countdown; cancel it with another switch. | Due-time IF, captured targets and cancellation expectations. |
| Sequential checkpoints | Change the course/lap count; use two racers or a Jeep. | Per-player state and object/player context. |
| KOTH | Require five consecutive uncontested seconds before scoring; reset progress when contested. | Occupancy, time, teams and whether progress is easy to express. |
| Slayer | Switch the victim's team on death; change the win threshold and penalties. | Victim versus Instigator, real scores and round behavior. |
| Steel-ball soccer | Change allowed teams, own-goal policy and reset delays. | Touch credit, team meaning and object replacement. |

These are challenges, not claims that every mutation is convenient or possible.
If one cannot be expressed, record the precise missing gameplay operation or
the rows you would need. Do not silently substitute an easier game.

## 3-hour ambition: compose something unplanned

Combine a gravity-gun sport with a portal race, a ball-operated door, a timer
and a contested finish. Or invent a different game without choosing its rules
in advance. Use the Toys Add-On's route facts/action in a core scoring or puzzle
chain. Bots and other existing systems are worth trying; the experiment does
not claim that every entity participates in the new Object context.

Build one interaction without copying its recipe. Read a contraption someone
else authored and explain it before testing it. If testing alone, revisit your
own rules tomorrow without your notes; that tests retention but cannot replace
another person's first reading.

## Record friction

Use one short record per challenge:

```text
Challenge / starting save:
Time to first working result:
Times lost or unsure what to select:
Wanted behavior blocked / awkward workaround:
Implementation concept I had to learn:
Explain helped? What remained unexplained?
Readable tomorrow / by another creator?
Was creating it enjoyable? Best moment / most annoying moment:
Brick IDs, expected result, actual result:
```

Gameplay concepts such as player, team, ball, score and round should do most of
the explanatory work. Mark moments when an engine detail becomes necessary.
Compare friction before comparing visual preferences or overall capability.

## Deliberately nasty semantic cases

- Delay an action, then delete its target, change the player's team, disconnect
  the player, reset/end the MiniGame, or reset the ball before it fires.
- Change a row after triggering it; rename/reassign its named targets.
- Let two players trigger the same puzzle or goal close together; cross two
  checkpoints rapidly; teleport through intermediate sensors.
- Copy rules/bricks and check which names/state now interact. Make a variable
  change cause another rule, including a named-target change.
- Try an unattributed rolling ball, an expired touch, a recent pusher and a
  driver. Distinguish what Explain actually shows from what you infer.

## Decide what earned its place

Can basic things remain easy? Does complexity appear gradually? Can ordinary
game policy be authored with readable rules? Do Add-On capabilities compose
with core ones? Which added concept unlocks many useful creations, and which
adds friction for little benefit?

Keep useful primitives and discard awkward ones. A missing expression or action
group is a candidate to investigate only after a concrete creation exposes the
need. This playtest is evidence for the next design, not a vote to freeze this
branch's formats or finish the rewrite.
