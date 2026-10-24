# Building encounters with bots

Enable **Blockhead Bot** or **Zombie** in the host's Add-Ons. Enabling Zombie
also enables its Hole Bots dependency. A Vehicle Spawn brick can select an
enabled bot kind; Zombie Hole supplies its own zombie. Bots follow their
builder's minigame. Start a minigame with weapon damage enabled for combat;
the ordinary building sandbox remains harmless.

A spawn point is an idle home. A visible enemy, or valid remembered evidence,
can take an ordinary bot beyond that home area. Explicit guard/patrol orders
from game rules retain their own bounds.

## Wrench event controls

These outputs target **Self** or a named group of spawner bricks. Distances
are native world units; a brick stud is half a world unit. They work with any
enabled bot kind and retain its authored body, weapon and attack profile.

| Output | Parameter | Effect |
| --- | --- | --- |
| `setBotPowered` | on/off | Resume or pause the live bot's AI and attack. A paused bot can still be hurt. |
| `setBotSightRadius` | 0–400 | Detection distance. Zero disables visual enemy acquisition and clears the current sighting/search. |
| `setBotWanderRadius` | 0–64 | Idle strolling distance. Zero stops strolling; combat and explicit tasks remain separate. |
| `setBotReturnHome` | on/off | Return to the spawner when idle, or stroll from the current location. It does not impose a combat boundary. |
| `setBotAutoRespawn` | on/off | Allow the normal automatic respawn, or leave a dead bot waiting for an authored respawn. |
| `setBotMeleeDamage` | 0–1000 | Change body attack damage while retaining an existing attack's timing and animation. Zero disables that body attack. |
| `respawnBot` | none | Restore a fresh bot at this spawner, retaining one bot per brick. Event respawns have a 0.2-second cooldown. |

Use **onBotSpawn** to initialize each new life, including minigame resets.
Its **Bot** target supports the normal player outputs, such as setting health
or scale, when those outputs are in the host's catalog. Ordinary spawn
protection and permissions still apply to player outputs.

**onBotDeath** fires on the spawner. **Player/Client** identify a human killer
when present; **Instigator** can also identify a bot. **Bot** refers to the
deceased body. Delayed Bot-target rows cannot operate on its replacement.
Use **Self → respawnBot** to schedule the spawner's next life.

## A start and stop switch

1. Give the encounter's spawner bricks the same name in the wrench.
2. On each spawner, add `onBotSpawn → Self → setBotPowered: off`.
3. On a start button, add `onActivate → [that named group] → setBotPowered: on`.
4. On a stop button, use the same group with `setBotPowered: off`.

The bots exist while paused. A reset runs their spawn initialization again,
so the start switch releases the fresh encounter when wanted.

## Timed replacement enemies

For a spawner that should replace a defeated enemy ten seconds later:

1. Add `onBotSpawn → Self → setBotAutoRespawn: off`.
2. Add `onBotDeath → Self → respawnBot` with a **10000 ms** delay.
3. Add any sight, wandering and damage settings to `onBotSpawn` so they also
   apply to replacement enemies.

Without the first row, the normal respawn timer can bring the bot back before
the authored delay. A freshly respawned bot starts powered on unless a spawn
row pauses it. A spawn row that pauses every life therefore needs a start
signal for every life as well.

Use existing Rule Workshop counters/conditions for encounter completion and
wave gates. For custom rounds, teams, loadouts, objectives or a director, use
[game rules](rules.md) and [bot profiles](content-kinds.md). The engine carries
out movement and combat; the mode decides those rules.

## Original Add-Ons

These native distance controls deliberately have distinct names and world-unit
parameters. Original Bot_Hole's `SetSearchRadius`, `SetWanderDistance`, custom
wrench UI, and other legacy outputs are not all interchangeable with them.
Unsupported original rows remain explicit. The supplied standard Zombie is
included; a separate `Bot_Zombie_L4B` needs its own files and port.

Zombie conversion checks a brick bot's health before bite damage: a bot already
at half health or less converts and regains health; the bite that first crosses
that threshold only damages it. Human players are not converted by this port.
Original infection name prefixes/markings and team-mode adaptations remain
incomplete. Start with ordinary kind sides when testing conversion.
