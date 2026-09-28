# Stress Lab handoff

The Stress Lab thread writes sections 1 to 10 of this handoff above this
line. The section below is the stress campaign's.

## Beyond the Stress Lab

The Stress Lab slice is a probe. The deliverable is the engine seams it
exposed, made general enough that a very different game (a race, a round
based PvP mode, a board game, a strategy game) needs no engine change. The
campaign built modes nothing like the Stress Lab, attacked the platform
with a red team, and fixed each class of weakness it found with the
smallest general seam. The running record is
[weakness-ledger.md](weakness-ledger.md): 15 classes (W1 to W15) from 31
experiments and two red-team rounds.

Labels: **WORKING NOW** (built and tested headless), **PROTOTYPE** (built
and tested, not yet reached by a real client or host), **MISSING** (named,
not built), **FUTURE IDEA**.

### The seams

Each seam is justified by two real systems. The last column says how a
game with different assumptions uses it.

| Seam | Family | Justified by | A different mode uses it for | Status |
|---|---|---|---|---|
| Package commands with declared arguments, cooldowns, `admin`, `while_dead`, aim reach | Game rules, security | Mining (Stress Lab), placing a board move (E18) | Any player action a mode invents: "end turn", "train unit" | PROTOTYPE |
| Hooks for engine events: `on_join`, `on_tick`, `on_death(victim, killer)` | Game rules | Racing retire on death (E16), team kill scoring (E17) | Round timers, scoring, elimination | PROTOTYPE |
| Policy points: `allow_respawn`, `allow_build` | Game rules | Elimination (E25), a no-building fight phase (E25) | Any rule that says "not now": a lobby, a build phase | PROTOTYPE |
| Player operations under capability `player`: `teleport`, `respawn`, `set_archetype`, `damage(p, n, by)` | Player and control | Race start grid (E16), round respawn (E17) | Spawning into arenas, resetting rounds | PROTOTYPE |
| Player archetypes: movement constants, collision body (`box`, `ball`), steering model (`strafe`, `turn`), health, riding rules, model and camera distance, as data both sides hold | Player and control | v20's Blockhead and horse (E30), a low-gravity mode (E22), a rolling ball and a kart (E30) | Racers, mechs, animals, spectators with their own bodies; a mini-game picks one like a v20 player type | PROTOTYPE (clients predict them; they do not draw package models on players yet) |
| Entities with a package `think`, first variables at spawn, fair rationing | Entities | RTS units that know their owner (E19), a 1000-agent swarm (E23), zombies (E29) | NPCs, units, pieces, hazards | PROTOTYPE |
| World edits: generated chunks, `place_brick`, `remove_brick`, `explode` | World model | A mineable world (Stress Lab), an arena built and sunk each round (E20) | Arenas, puzzles, destructible maps | PROTOTYPE |
| State with an audience: `visible` = `server`, `owner`, `everyone` | UI, security | A hidden hand of cards (E21), per-player currency (Stress Lab) | Secrets, private inventories, fog of war | PROTOTYPE |
| HUD bindings: `global`, `player` (the viewer's), `players` (scoreboard) | UI | Currency panel (Stress Lab), scoreboard (E26) | Lap timers, turn indicators, standings | PROTOTYPE (the client does not draw package HUDs yet) |
| One capability gate (`ops::authorize`) plus the caller's trust (W9) | Security | Package brick removal (H2-F12), explosions | Any package power, declared and shown to the host | WORKING NOW |
| Per-origin shares (W1): script work per package and per player, world edits, chat lines, entity slots | Performance | 12 red-team findings (H2) | Keeps any mode from starving the others | WORKING NOW |
| One storage budget per carrier (W7): bricks, package state, join chunks, reliable outbox | Persistence, distribution | Heavy worlds (E14, E15), package state (H2-F1), slow joins (E24) | Anything admitted can be saved and sent | WORKING NOW |
| Verified package sync: content-addressed cache, seals, one path rule, conflict checks; a refused join names the differing packages and `connect_fetching` downloads them and joins again | Distribution, security | Auto-download (E6 to E13), download then join (E31), hostile package files (H2-F2 to F5) | Any mod's assets reach clients safely | PROTOTYPE (the game client downloads and loads them on join; nothing draws package models or HUDs yet) |
| Crash-safe autosave for the dedicated server (W2) | Persistence | World (E5), package state (E28) | Any long-running server | WORKING NOW for the dedicated server |

### What is not built

- **MISSING, the rest of W14.** A player can be any archetype now (E22,
  E30), but what a player controls is still the closed `ControlObject`
  enum: avatar, camera, spy or corpse. Driving a second body while the
  avatar stays behind (E27) needs a control target that is an entity, with
  the entity's archetype predicted like a player's. The test is
  `#[ignore]`d and in `tools/gate-known-failures.toml`.
- **MISSING.** The client draws only v20's Blockhead and horse on players.
  A package archetype's `model` and `camera_distance` reach the client in
  the archetype table, but nothing draws or uses them yet.
- **MISSING.** Package state and entities do not replicate to clients yet
  (the Stress Lab's protocol work). `Session::package_state_for(viewer)` is
  the per-client view to send when they do.
- **MISSING.** The client does not draw package HUD panels or box models.
- **MISSING.** The windowed host does not autosave its world (W2).
- **MISSING.** Fog of war for entities: W13's audience rule for package
  entities.
- **FUTURE IDEA.** More policy points in W15's shape (spawn point, damage,
  pickup) and more event hooks in W8's (leave, brick planted) when a second
  mode needs each.

### Saturation evidence

Max's criterion: deliberately orthogonal experiments across nine seam
families, stopping when several different experiments stop finding new
classes. Per family (full table in the ledger):

| Family | Experiments | New classes | Most recent result |
|---|---|---|---|
| World model | E15, E20, H2-F13 | W7 | E20: W8 again |
| Entities and behaviour | E19, E23, E29 | W12 | E29: ordinary |
| Player and control | E16, E22, E27, E30 | W14 (fixed for archetypes; control targets open) | E30: W14's fix held |
| UI model | E21, E26, H2-F4 | W11, W13 | E26: W8 again |
| Game rules | E16 to E18, E25, E28 | W8, W15 | E28: none |
| Persistence | E5, E11, E15, E28, H2 | W2, W6, W7 | E28: none |
| Multiplayer/distribution | E4, E6, E9 to E12, E14, E24, E31 | W6, W7 | E31: W8 again |
| Security/authority | H, M, E7, H2 | W3, W4, W5, W9, W10 | H2: W9, W10 |
| Performance/failure | E1 to E3, E8, E13, E23, H2 | W1, W12 | H2: W1 again |

The last round (E26 to E29), one experiment in each of four different
families, found no new class, and E30 (custom bodies, asked for by Max)
found none either while closing most of W14. That is the first sign of
saturation, not saturation: W14 is open for control targets, and packages
cannot be tested over the network until their state replicates.

### Run it on Windows

From the repository root, in PowerShell:

```powershell
cargo test -p bri-sim --test unlike_modes            # E16-E30: unlike game modes and bodies
cargo test -p bri-sim --test unlike_modes -- --ignored   # the open rest of W14 (expected to fail)
cargo test -p bri-motor                              # archetype table
cargo test -p bri-sim --test hardening_packages      # red team: package commands, hooks, budgets
cargo test -p bri-package-runtime --test hardening_sandbox   # red team: sandbox and package files
cargo test -p bri-sim --test hardening_session       # red team: ownership, admin, commands
cargo test -p bri-net --test hardening_net           # red team: frames and transport
cargo test -p bri-net --test stress_campaign         # admission pools (E1-E4)
cargo test -p bri-net --test package_sync            # package download and cache (E6-E12)
cargo test -p bri-net --test state_limits            # a heavy world joins and saves (E14, E24)
```

Each test's body is the replayable experiment; its name is the ledger row.

### What only a human can test

- Whether the modes are fun: every mode above is played by scripts.
- How package HUDs and entity models look, once the client draws them.
- How a ball or kart body feels to drive, and whether turn steering wants
  its own camera.
- Joining a modded server from a clean install and watching the download.
- Latency feel when a package's share rations its thinks (E23).
