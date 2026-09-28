# Package runtime: gameplay from packages

Status: prototype, 2026-09-27. Code: `crates/package-runtime`
(`bri-package-runtime`), `crates/sim/src/session/packages.rs`, the client's
`crates/client/src/packages.rs` and the UI's `hud.overlay` slot. Package
identity, `packages.json` and the join comparison are `bri-package`'s
([packages.md](packages.md)); this document covers what a mod package can
*do* once it is enabled.

The engine owns mechanisms; packages own policy. Nothing in these crates
knows about mining, ore, creatures or currency. Those live in the Stress
Lab packages (`packages/stresslab`), which use only the seams below.

## A mod package

A mod package is a directory listed in `packages.json` without a `role`,
holding its own `package.json` (the platform manifest shape: id, version,
api, license, provenance, dependencies, capabilities, provides). Each
`provides` entry names a content id in the package's namespace, a kind and a
file:

| Kind | Side | What it is |
|---|---|---|
| `behaviour` | server | Declared commands, state keys and hooks, and the script file. |
| `script` | server | Sandboxed server script (Rhai). |
| `world` | server | A chunked world provider: chunk size, voxel brick, materials, the generator function. |
| `entity` | server | An entity kind: name, model id, speed, scale, health, think function and interval. |
| `model` | client | A box model: coloured boxes, colours per entity label. |
| `hud` | client | A HUD panel: title, colours, rows bound to public state, keys bound to commands. |

**Clients never receive or run package code.** Server kinds may only appear
in packages listed with `"side": "server"`; models and HUD panels only in
`client` or `shared` packages. A package that breaks this is refused with
`package.side.server_content` or `package.side.client_content`. Clients load
only their `client`/`shared` packages; the server tells them everything
else through replicated data (entities carry their model id).

## The seams

| Seam | Family | Engine side | Package side |
|---|---|---|---|
| Package commands | player/control, security | `Command::Package { package, command, args }`: checked against the declared name, argument types, cooldown and admin flag; optional server-resolved aim. | `fn cmd_<name>(player, args...)` |
| Package state | persistence, game-rule | Namespaced per package; per durable player (principal) and per server; only declared keys; committed only when a call succeeds; keys replicated to the audience their `visible` names; persisted keys saved (`PackageSave`). | reads and writes its own keys |
| Entities | entity/behaviour | Character bodies (motor `spawn_tagged`, collider kind 3), steering, labels, health, fall-out removal, replication as `EntityInfo`. | `fn think(me)` every N ticks: `steer`, `label`, `explode`, ... |
| Chunked world provider | world, persistence | Streams chunks around players, inserts world-owned bricks, records removals (by any means) as edits, saves seed + edits, regenerates on load. | `fn generate(cx, cz)` returns `[[x, y, z, material], ...]` |
| Explosion | game-rule | `Session::explode`: player and entity damage with falloff, brick destruction in a radius, `Explosion` cue. | `explode(x, y, z, r, damage, brick_r)` |
| HUD slot `hud.overlay` | UI | Panels drawn from data, values from replicated public state, key hints; base-game binds win. | `hud` JSON |
| Box models | UI | One cube instanced per box, tinted, following entity pose and label. | `model` JSON |

Hooks: `on_join(player)` and `on_tick()` every `tick_interval` ticks.

## Operations and the capability gate

Scripts never touch the game. A call receives a read-only snapshot (tick,
seed, players, entities, the caller's aim) and a working copy of its own
state; it returns a list of typed operations (`bri_package_runtime::Op`):
`remove_brick`, `place_brick`, `explode`, `damage`, `teleport`, `respawn`,
`set_archetype`, `control`, `spawn_entity`, `remove_entity`, `steer`,
`label`, `tell`, `broadcast`. `control(player, entity)` hands a player's
movement to one of the package's own entities, `release(player)` hands it
back (capability `player`; see `docs/player-simulation.md`).

Every operation passes **`ops::authorize`**, the single capability gate:
bounds first (`op.bounds`), then the capability the manifest declares
(`world.edit`, `damage`, `entity`, `chat`; `op.capability`), then
`op.foreign_entity` (packages spawn only their own kinds). The session then
checks ownership (`op.not_owner`: a package steers only its own entities).
If any operation or state write fails, the whole call is discarded.

## The sandbox (Rhai, prototype)

Chosen for the probe because it is pure Rust, needs no compiler for the
agent, and is easy to bound; not a permanent commitment. Limits
(`script.rs`): operation budget per call (`Budget`: command 200k, think
100k, tick 400k, generate 4M), 32 call levels, expression depth, 4 KiB
strings, 64k arrays, 1k maps, 256 variables and functions, 1024 operations
per call. No `eval`, modules, clock, file or network. Scripts may only
define functions (`script.top_level`). Over budget is `script.budget`,
limits `script.limit`, other failures `script.error`, each with the file and
line. A failing entity think stops that entity for a second instead of
spamming.

## Replication

`Checkpoint` and `Delta` carry `entities: Vec<EntityInfo>`, sent whole when
they change at the 20 Hz delta rate. Package state is per client: a state key
declares `visible` (`server`, `owner` or `everyone`), the welcome's
`Checkpoint.package_state` is `Session::package_state_for(viewer)`, and the
server sends each client `Message::PackageState` with its whole view when
that view changes (protocol 34). One player's owner-visible keys never reach
another client. Whole views are fine for tens of entities and small state;
larger counts need per-entity deltas (see the Stress Lab handoff).

## Workflow

- `stresslab check [<root>] [--list packages.json]`: load, validate and
  compile; JSON diagnostics with stable codes.
- `stresslab test`: headless scenario run with a JSON report.
- `stresslab-soak --clients N --seconds S`: loopback host plus headless
  clients.
- Tests: `cargo test -p bri-package-runtime -p bri-stresslab`, and
  `cargo test -p bri-sim --test packages`.
