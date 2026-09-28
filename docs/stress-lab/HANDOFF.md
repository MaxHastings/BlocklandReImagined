# Stress Lab handoff

Date: 2026-09-28. Branch `stress-lab`. The Stress Lab is one deliberately
weird vertical slice, used to find out whether the game can be changed at
its core by packages. It has a generated, mineable world, a creeper, a
server-owned economy and a custom HUD, all of it in five ordinary mod packages
(`packages/stresslab`). The engine gained general seams. None of them knows
what mining, ore, creepers or currency are.

Labels: **WORKING NOW** (runs and a test proves it), **PROTOTYPE** (runs, but
the shape is provisional), **MISSING** (not built), **FUTURE IDEA** (not built
on purpose).

## 1. What was fixed or stabilised

Other threads, from the main history since 2026-09-27 21:00Z:

- **WORKING NOW.** Engine foundations:
  - `c359d64`, `aaa3249`: joins and map changes stream as bounded
    4,096-brick chunks built off the host loop.
  - `fa5b9ba`: bricks live in a persistent map, so snapshots no longer
    deep-clone the world.
  - `085b8f4`: every save is written crash-safely through `bri-files`.
- **WORKING NOW.** Platform door-closers:
  - `35e2235`: `packages.json` and one id grammar (`namespace:kind/name`).
  - `b9eeede`: brick owners follow the player's durable principal across
    restarts.
- **WORKING NOW.** Gate and tooling:
  - `a855eaf`, `186e8dd`: the push gate (`tools/gate.py --push`).
  - `e651a63`: `tools/bootstrap.py`.
  - `aa939eb`, `5238e67`: shared sccache.
  - `dd741e5`: crash and session logs (`bri-crash`).
  - `b0205f7`: GPU backend fallback and survival of a lost device.
- **WORKING NOW.** Vanilla fidelity:
  - console `a92a3fe`, `7da3758`;
  - vehicles `31d944a`, `e9e85da`, `f8f1e5f`, `3963e6f`, `cc155c3`;
  - glass map shapes `33f13a2`;
  - held bricks and items `eeebfbe`, `9b872aa`, `7c0527e`;
  - avatars `697e73e`, `3c79620`;
  - presentation `7c622df`, `7c02273`, `1eb0bdd`, `c092117`.
- **WORKING NOW.** Engine fix found by the Stress Lab: a mid-session
  collision pass (a generated chunk, a mined block) right after a player
  joined consumed the new body's pending changes, so it never entered a
  Rapier island and a debug build panicked on the next step. Every
  `Simulation` collision pass now re-marks bodies modified.
- **WORKING NOW.** Stress Lab, this thread:
  - the package runtime and sandbox (`a223280`);
  - the session seams (`b7ede11`);
  - replication, client HUD, models and hosting, tools, soak and packaging
    (queued behind the gate at the time of writing, protocol 32).

## 2. What the Stress Lab can do

| Item | State |
|---|---|
| Host "Stress Lab Strata" from Start Game (single player or LAN) | WORKING NOW |
| Generated, chunked world of grass, dirt and stone over bedrock, with coal, copper and gold, streamed around every player as they walk | WORKING NOW |
| Mining with H: the server removes the aimed block and adds the ore to the player's pouch | WORKING NOW |
| Selling with G: ore becomes Bits at package prices | WORKING NOW |
| Miner HUD panel (top right, own colours and layout) showing Bits, ore and blocks mined from server state | WORKING NOW |
| Creeper: spawns naturally every 10 s near a player (at most 4); J spawns one for the host only; it chases, flashes while its fuse burns, explodes, hurts players and digs a crater | WORKING NOW |
| Creeper look: a box model from its own client package, coloured by the entity's label | WORKING NOW |
| Bits, ore and dug holes survive a reconnect and a host restart (saved to `<state>/packages/<world>.save.json` when the host stops) | WORKING NOW |
| A second client on LAN sees the same world, holes, creepers and every player's public numbers | WORKING NOW (loopback QUIC test; not playtested) |
| Forged, mistyped and unprivileged package commands refused with codes | WORKING NOW |
| Package mismatch at join named per package: a changed or missing shared package refuses the join naming it; a changed client package (the HUD) joins with a chat line naming it | WORKING NOW (door-closers' join check, exercised with Stress Lab packages) |
| Clients download missing packages | FUTURE IDEA (mod platform lane) |

## 3. The exact passing tests

Run on Windows in this worktree (content linked from the main checkout):

- `cargo test -p bri-package-runtime` (7 tests):
  - Stress Lab loads as server and client, and the client never sees scripts.
  - World generation is deterministic.
  - The mining economy works through the runtime alone.
  - The creeper chases, fuses and explodes.
  - One capability gate covers every operation.
  - Runaway scripts are stopped with `script.budget` and `script.limit`.
  - A broken package reports every problem with its code.
- `cargo test -p bri-sim --test packages` (4 tests):
  - The generated world mines into server-owned currency.
  - Clients cannot forge package commands (`command.unknown`,
    `command.args`, `command.admin`, `command.package`,
    `command.cooldown`).
  - The creeper chases, explodes and damages players and the ground.
  - World edits and durable state survive a save and reload.
- `cargo test -p bri-stresslab`:
  - `two_clients_mine_meet_a_creeper_and_keep_their_bits_across_restart`
    runs a real QUIC loopback host with two clients. It covers:
    - both clients agreeing on the generated world;
    - mining replicating to both;
    - forged commands refused;
    - the creeper replicating to both, exploding and hurting;
    - Bits after a reconnect;
    - Bits and holes after a host restart.
  - `check_reports_every_planted_mistake_once` (workflow errors).
  - `differing_packages_are_named_at_join`: a loopback host and clients
    with differing package copies. A changed or missing shared
    `stresslab-creeper-model` is refused naming it, and a changed
    `stresslab-hud` joins and is told `stresslab-hud` differs.
- `cargo test -p bri-client --lib packages` (2 tests): the miner panel shows
  the viewer's server values, and the creeper boxes follow the entity and its
  fuse label.
- `cargo test -p bri-client --release --test stresslab_flow -- --ignored`
  (offscreen GPU, no window). The real client:
  - hosts the Stress Lab from Start Game;
  - shows the miner panel with server values;
  - mines with H;
  - spawns a creeper;
  - captures the frame (`artifacts/stresslab-client/stresslab-hud.png`);
  - checks that the save file is written.

  With `BRI_STRESSLAB_CONTENT=<release>/content` it runs against a packaged
  release's own `packages.json`, and passed.
- `stresslab test` (headless scenario, JSON report): mine, sell, a forged
  command refused, the creeper spawning, exploding and hurting. All steps
  are ok.
- Soak `stresslab-soak --clients 8 --seconds 120`
  (`docs/stress-lab/soak-8x120.json`):
  - 399 chunks generated and 98,743 live voxels;
  - 1,140 blocks mined and 2,850 commands, with 0 rejected;
  - 14,782 host ticks with 0 dropped;
  - all 8 replicas agree.
- Workflow errors: `stresslab check` on a copy with five planted mistakes
  (`docs/stress-lab/workflow-errors.json`).
- Existing suites still pass: `bri-sim`, `bri-net` including replication,
  `bri-ui`. Clippy runs with `-D warnings` on the touched crates.

## 4. The exact Windows command or package

```powershell
cargo build --release -p bri-client
$sha = (Get-FileHash target\release\bri-client.exe -Algorithm SHA256).Hash
.\tools\package_playtest.ps1 -StressLab -Version a11 -ExpectedExecutableSha256 $sha
.\tools\package_playtest.ps1 -VerifyPackage dist\BlocklandReImagined-alpha-a11-stress-lab
```

`-StressLab` copies `packages/stresslab` into `content/stresslab`, writes
`content/packages.json` (the base list plus the five packages) and adds
`PLAYTEST-STRESS-LAB.md`. A trial build verified 3,258 files and passed
`bri-client --check`: 15 maps, including Stress Lab Strata. Play
instructions are in [PLAYTEST-STRESS-LAB.md](PLAYTEST-STRESS-LAB.md).

## 5. General engine changes

Each seam lists its family (from Max's nine), the real systems that justify
it, and how a totally different mode would use it. Design and limits are in
[docs/architecture/package-runtime.md](../architecture/package-runtime.md).

| Seam | Family | Justified by | A different mode | State |
|---|---|---|---|---|
| Mod package loading (`bri-package-runtime`: manifest, provides, kinds, sides, dependencies) | multiplayer/distribution | 5 Stress Lab packages; the Add-On importer writes the same `package.json` | any mod | WORKING NOW |
| Package commands (`Command::Package`: declared name, typed args, cooldown, admin flag, server-resolved aim) | player/control, security/authority | Economy mine/sell, creeper spawn; the Add-On archive's 246 `serverCmd*` functions, which map to behaviour commands | racing: vote for a track; PvP: pick a team | WORKING NOW |
| Namespaced package state (per durable principal and per server, declared keys only, public or private, persisted or not, committed only on success) | persistence, game-rule | Economy balances, creeper global counters; per-object Add-On state such as `%obj.lastFireTime` | lap times and best laps; team scores and round number | WORKING NOW |
| Package entities (character bodies, `think` every N ticks, steer and label, health, fall-out removal, replicated `EntityInfo` with model id) | entity/behaviour | The creeper; Bot_Zombie's 8 bot-framework callbacks, for which the importer names `think` as the runtime hook | PvP guard NPCs, a race pace-setter on foot | PROTOTYPE (whole-list replication, walking bodies only) |
| Chunked world provider (package generates chunks; engine streams, inserts world-owned bricks, records removals by any means, saves seed and edits, regenerates) | world, persistence | Only the Stress Lab world. The second justification is the boundary itself: a generated world cannot be an authored map bundle, and the streaming foundations already chunk joins. | a track generator per race; an arena regenerated each round | PROTOTYPE |
| One explosion operation (`Session::explode`: players and entities with falloff, brick destruction, `Explosion` cue) | game-rule | The creeper's blast; vanilla projectile explosions, not yet routed through it, and 15 archive Add-Ons needing `effects.spawn` | grenades in PvP; mines on a race track | WORKING NOW (vanilla weapons still use their own path) |
| Sandboxed server scripts (Rhai) returning typed operations | security/authority, performance/failure | The world, creeper and economy scripts; 4,836 Add-On functions reported `needs_behaviour` | every rule of a new mode | PROTOTYPE (runtime choice not committed) |
| One capability gate (`ops::authorize`) plus session ownership checks | security/authority | Every operation from the 3 behaviour packages; the Add-On reports' `missing_capabilities` map onto it | any mod the server owner enables | WORKING NOW |
| HUD slot `hud.overlay` (declarative panels bound to public state, key hints) | UI | The miner panel; the door-closer "replace the GUI" scenario asks for slots | lap and position panel; team scoreboard | PROTOTYPE (one slot) |
| Package keys (unbound letters send a declared command; base binds win) | player/control | Mine, sell and spawn keys | horn, emote wheel, ready-up | PROTOTYPE |
| Box models (tinted cube instances per model box, label colours) | UI | Creeper look | markers, simple props, NPCs | PROTOTYPE |
| Package worlds hosted from Start Game on an environment map, saved on host stop | persistence, world | Stress Lab Strata | any generated mode | PROTOTYPE |
| Tools: `stresslab check`, `stresslab test`, `stresslab-soak` | performance/failure | This slice | same commands for any package set | PROTOTYPE (named for the Stress Lab; should become the platform's `bri-mod` commands) |

Smaller engine changes:

- `Player::spawn_tagged`: character bodies with a caller's collider tag.
- `DamageKind::Package`.
- `CueKind::Explosion`.
- `Session::install_packages`, `package_save`, `package_state`,
  `package_entities`, `package_diagnostics`, `package_stats`.
- `ServerReport.packages` and `package_stats`.
- `App::enable_packages`.
- `package_playtest.ps1 -StressLab`.
- `.gitattributes` keeps `packages/**` byte-identical so hashes agree.

Protocol 32: `Checkpoint` and `Delta` carry `entities` and `package_state`.

Family coverage: player/control and game-rule are thin. There is a command
path and a damage operation, but no package can change how players move,
what rules decide a round, or how a player wins. This slice did not build
for them.

## 6. What's still hardcoded, and why

- **PROTOTYPE.** The world stands on a base-game map's sky, light and ground
  (`environment`, default Slate). Packages cannot provide environments yet.
- **PROTOTYPE.** Voxels are one brick definition (`v20/brick/brick4xcubedata`,
  old id spelling) tinted by palette colour. The material is provider data.
  The brick is only its body.
- **PROTOTYPE.** Every entity body is the player motor at a package scale and
  speed, with speed capped at a player's. Entities cannot fly, swim on
  purpose or ride.
- **PROTOTYPE.** Explosions look and sound like v20's rocket (client choice
  in `app.rs` and `audio.rs`). A package cannot pick an effect.
- **PROTOTYPE.** Sides are per package, so each mode ships as a server
  package plus a client package. The Stress Lab is five packages for this
  reason.
- **MISSING.** Base-game weapons ignore entities: collider kind 3 is not a
  weapon target.
- **MISSING.** Bricks that players build on a generated world are not in
  the package save.
- **MISSING.** Player/control and game-rule families (see section 5).
- **PROTOTYPE.** Replication sends the whole entity list and all public
  state on any change, at 20 Hz. That is fine for dozens of entities, not
  for thousands.

## 7. Creature lessons

- **WORKING NOW.** A think function that returns steer, label and explode
  operations was enough for chase, fuse, escape and blast behaviour.
  Twenty think calls per second per creeper stayed far under budget.
- A package rarely knows the ground height, so spawns now lift up to
  32 units out of the ground. The first headless run failed because a
  creeper spawned inside a hill.
- The creeper needed its target's distance and its own speed (to jump
  when stuck). `speed` is now in the entity map. A general "am I blocked"
  signal would be better.
- Entity variables (`entity_get` and `entity_set`) carried the fuse. They
  are package-local and not saved, which is right for creatures.
- **MISSING.**
  - Pathfinding: creepers walk straight at you.
  - A way for base-game weapons to hurt entities.
  - Any entity type that is not a walking body.

## 8. World, UI and economy lessons

- **World.**
  - **WORKING NOW.** Generating in the script (value noise and hash
    helpers from the host) is fast enough: 25 chunks of about 440 voxels at
    startup, then one chunk per tick while players explore.
  - Solid columns cost bricks, about 99k voxels for 399 chunks. A
    surface-only generator that fills revealed voxels would cut that a lot.
    That is a FUTURE IDEA.
  - Recording any brick removal as an edit made hammer, explosion and
    mining persistence one mechanism.
- **UI.**
  - **WORKING NOW.** The panel is data bound to public state, drawn in the
    UI's font over the HUD.
  - F was already a base-game bind, which the offscreen test found, so the
    key moved to H. The client drops package keys the base game uses and
    shows only the ones that work.
  - The panel updates every tick. An earlier version updated only while
    rendering, and a headless test caught it.
- **Economy.**
  - **WORKING NOW.** The server is the only writer: clients send commands,
    and the state commits only if the call and all its operations pass.
  - Undeclared state keys are refused (`state.undeclared`).
  - Cooldown refusals show on screen with their code. That is precise for
    agents but noisy for players.

## 9. Add-On import lessons

From [docs/audits/spike-addon-import.md](../audits/spike-addon-import.md)
(Add-On import thread):

- **WORKING NOW.** The importer's `package.json` parses as this runtime's
  manifest and loads through `Package::load`.
- **WORKING NOW.** Its `needs_behaviour` entries name the runtime hook a
  rewrite can use today: a behaviour command for `serverCmd*`, and entity
  `think` for bot callbacks.
- **MISSING.** Systems read one pack per role, so imported weapons,
  vehicles and bricks cannot load in a hosted game. Its seam 1 proposes
  merging each kind from every package.
- **MISSING.** Capabilities the archive needs most: `schedule` (60 Add-Ons),
  `entities.animate` (52), `random.seeded` (42), `players.inventory` (39),
  `projectiles.spawn` (30). Only `world.edit`, `damage`, `entity` and `chat`
  exist. The gate in `ops.rs` is where they would go.
- **MISSING.** Image state script hooks (the shotgun's `onFire`) and global
  overrides as named hooks.
- **FUTURE IDEA.** A controllable-entity kind with data-driven AI settings,
  for Bot_Hole-style frameworks.

## 10. Next steps

1. Clients download missing data packages (mod platform lane), so the
   mismatch refusal becomes a download instead.
2. Promote `stresslab check` and `stresslab test` into the platform's
   workflow commands (mod platform lane), taking any package set.
3. Per-entity replication deltas, and a weapon target for entities.
4. Surface-only chunk generation, then a larger soak (32 clients).
5. Capabilities from the Add-On evidence: `schedule`, `random.seeded` and
   `projectiles.spawn` first.
6. Package-provided environments (sky and light) and effects for
   `explode`.
7. Game-rule and player/control seams: round lifecycle and win conditions,
   and movement tuning per package. Nothing in this slice exercises them.

## Where the create, check, test and host path confused me

- There were two manifests: `packages.json` (the peer's list, door-closers)
  and each package's own `package.json` (mod lane). I wrote an interim reader
  of the mod manifest, `bri_package_runtime::manifest`, in the same shape.
  It should be replaced by the lane's reader.
- `check` first stopped at a package's manifest errors and then reported
  every dependent binding as "not enabled". It now keeps reading and reports
  a broken dependency once (`set.dependency.broken`).
- A misspelt model id passed `check` until `set.model.unknown` existed.
- A package's side is all-or-nothing, so server scripts and client models
  must be split into separate packages. An agent will try to put them
  together and gets `package.side.server_content`.
- There is no `create` or scaffold command. The five packages were written
  by hand from the schemas in `content.rs`.
- Hosting a package world needs its packages in the content root's
  `packages.json`. Tests use `App::enable_packages` to point elsewhere.
- Protocol numbers moved four times during the session (26, then 29 on main,
  then 30), coordinated through the coordinator.
